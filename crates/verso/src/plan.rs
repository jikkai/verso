use crate::{
    cargo_manifest,
    changelog::{self, insert_changelog_entry, render_changelog_entry},
    config::{render_template, Config, DEFAULT_TAG_NAME_TEMPLATE},
    doctor::ProjectInspection,
    package_json,
    workspace::validate_release_path,
};
use semver::Version;
use serde::{Deserialize, Serialize};
use std::{
    collections::HashSet,
    fs,
    path::{Path, PathBuf},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum PlanMode {
    Bump,
    Release,
}

impl PlanMode {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Bump => "bump",
            Self::Release => "release",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum FileKind {
    PackageManifest,
    CargoManifest,
    CargoLock,
    Changelog,
}

impl FileKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::PackageManifest => "package-manifest",
            Self::CargoManifest => "cargo-manifest",
            Self::CargoLock => "cargo-lock",
            Self::Changelog => "changelog",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlannedFileChange {
    pub path: PathBuf,
    pub kind: FileKind,
    pub before: Option<String>,
    pub after: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlannedHook {
    pub name: String,
    pub command: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReleasePlan {
    pub mode: PlanMode,
    pub group: String,
    pub root: PathBuf,
    pub config_path: PathBuf,
    pub current_version: Version,
    pub target_version: Version,
    pub package_files: Vec<PathBuf>,
    pub extra_version_files: Vec<PathBuf>,
    pub changelog_file: Option<PathBuf>,
    pub file_changes: Vec<PlannedFileChange>,
    pub commit_message: Option<String>,
    pub tag_name: Option<String>,
    pub hooks: Vec<PlannedHook>,
    pub warnings: Vec<String>,
}

pub fn build(
    inspection: &ProjectInspection,
    config_path: &Path,
    group: String,
    target_version: Version,
    mode: PlanMode,
    warnings: Vec<String>,
) -> Result<ReleasePlan, String> {
    let root = &inspection.root;
    let config = &inspection.config;
    let package_files = inspection
        .packages
        .iter()
        .map(|package| package.package_json.clone())
        .collect::<Vec<_>>();
    let changelog_file = (mode == PlanMode::Release && config.changelog.enabled)
        .then(|| root.join(&config.changelog.infile));
    for path in package_files
        .iter()
        .chain(inspection.cargo_manifest_files.iter())
        .chain(changelog_file.iter())
    {
        validate_release_path(root, path)?;
    }
    let cargo_lock_files = cargo_lock_files_for_manifests(root, &inspection.cargo_manifest_files)?;
    let extra_version_files = inspection
        .cargo_manifest_files
        .iter()
        .chain(cargo_lock_files.iter())
        .cloned()
        .collect::<Vec<_>>();
    let named_group = config_path.file_name().and_then(|name| name.to_str()) != Some("verso.toml");
    let tag_template = if named_group && config.git.tag_name == DEFAULT_TAG_NAME_TEMPLATE {
        format!("{group}-v${{version}}")
    } else {
        config.git.tag_name.clone()
    };
    let tag_name = (mode == PlanMode::Release)
        .then(|| render_template(&tag_template, &target_version.to_string()))
        .map(|tag| {
            crate::config::validate_rendered_tag_name(&tag)?;
            Ok::<_, String>(tag)
        })
        .transpose()?;
    let commit_message = (mode == PlanMode::Release).then(|| {
        render_template(&config.git.commit_message, &target_version.to_string())
            .trim()
            .to_owned()
    });
    let mut file_changes = plan_version_files(
        root,
        &inspection.packages,
        &inspection.cargo_manifest_files,
        &cargo_lock_files,
        &target_version,
    )?;

    if let (Some(changelog_file), Some(tag_name)) = (&changelog_file, &tag_name) {
        let previous_tag = previous_tag(root, &tag_template)?;
        let commits = changelog::commits_since(root, previous_tag.as_deref())?;
        let repo_slug = crate::git::remote_origin_url(root)
            .and_then(|remote| changelog::infer_github_slug(&remote));
        let entry = render_changelog_entry(
            config.changelog.preset,
            &target_version.to_string(),
            previous_tag.as_deref(),
            tag_name,
            &commits,
            repo_slug.as_deref(),
        );
        let before = read_optional_text(root, changelog_file)?;
        let after = insert_changelog_entry(
            config.changelog.preset,
            before.as_deref().unwrap_or(""),
            &entry,
        );
        if before.as_deref() != Some(after.as_str()) {
            file_changes.push(PlannedFileChange {
                path: changelog_file.clone(),
                kind: FileKind::Changelog,
                before,
                after,
            });
        }
    }

    let mut planned_paths = HashSet::new();
    if let Some(duplicate) = file_changes
        .iter()
        .map(|change| &change.path)
        .find(|path| !planned_paths.insert((*path).clone()))
    {
        return Err(format!(
            "release plan targets {} more than once",
            duplicate.display()
        ));
    }

    Ok(ReleasePlan {
        mode,
        group,
        root: root.clone(),
        config_path: config_path.to_path_buf(),
        current_version: inspection.current_version.clone(),
        target_version,
        package_files,
        extra_version_files,
        changelog_file,
        file_changes,
        commit_message,
        tag_name,
        hooks: planned_hooks(config, mode),
        warnings,
    })
}

fn plan_version_files(
    root: &Path,
    packages: &[crate::workspace::PackageFile],
    cargo_manifest_files: &[PathBuf],
    cargo_lock_files: &[PathBuf],
    target_version: &Version,
) -> Result<Vec<PlannedFileChange>, String> {
    let mut changes = Vec::new();

    for package in packages {
        let before = read_text(root, &package.package_json)?;
        let after = package_json::replace_manifest_version_preserving_style(
            &package.package_json,
            &before,
            target_version,
        )?;
        push_change(
            &mut changes,
            package.package_json.clone(),
            FileKind::PackageManifest,
            before,
            after,
        );
    }

    let mut cargo_package_updates = Vec::new();
    for manifest_path in cargo_manifest_files {
        let before = read_text(root, manifest_path)?;
        let package = cargo_manifest::read_package_version(manifest_path, &before)?;
        let after = cargo_manifest::replace_package_version_preserving_style(
            manifest_path,
            &before,
            target_version,
        )?;
        if after != before {
            cargo_package_updates
                .push((package, cargo_lock_file_for_manifest(root, manifest_path)?));
            changes.push(PlannedFileChange {
                path: manifest_path.clone(),
                kind: FileKind::CargoManifest,
                before: Some(before),
                after,
            });
        }
    }

    for lock_path in cargo_lock_files {
        let before = read_text(root, lock_path)?;
        let mut after = before.clone();
        for (package, package_lock_path) in &cargo_package_updates {
            if package_lock_path.as_ref() == Some(lock_path) {
                after = cargo_manifest::replace_lock_package_version_preserving_style(
                    lock_path,
                    &after,
                    &package.name,
                    &package.version,
                    target_version,
                )?;
            }
        }
        push_change(
            &mut changes,
            lock_path.clone(),
            FileKind::CargoLock,
            before,
            after,
        );
    }

    Ok(changes)
}

fn push_change(
    changes: &mut Vec<PlannedFileChange>,
    path: PathBuf,
    kind: FileKind,
    before: String,
    after: String,
) {
    if before != after {
        changes.push(PlannedFileChange {
            path,
            kind,
            before: Some(before),
            after,
        });
    }
}

fn read_text(root: &Path, path: &Path) -> Result<String, String> {
    validate_release_path(root, path)?;
    fs::read_to_string(path).map_err(|error| format!("failed to read {}: {error}", path.display()))
}

fn read_optional_text(root: &Path, path: &Path) -> Result<Option<String>, String> {
    validate_release_path(root, path)?;
    if !path.exists()
        && !path
            .parent()
            .is_some_and(|parent| parent.exists() && parent.is_dir())
    {
        return Err(format!(
            "parent directory for {} does not exist",
            path.display()
        ));
    }
    match fs::read_to_string(path) {
        Ok(contents) => Ok(Some(contents)),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(format!("failed to read {}: {error}", path.display())),
    }
}

fn previous_tag(root: &Path, tag_template: &str) -> Result<Option<String>, String> {
    let Some((prefix, suffix)) = tag_template.split_once("${version}") else {
        return Ok(None);
    };
    let output = crate::git::git(root, &["tag", "--merged", "HEAD", "--list"])?;
    Ok(output
        .stdout
        .lines()
        .map(str::trim)
        .filter_map(|tag| {
            let version = tag
                .strip_prefix(prefix)?
                .strip_suffix(suffix)
                .and_then(|version| Version::parse(version).ok())?;
            Some((version, tag.to_string()))
        })
        .max_by(|(left, _), (right, _)| left.cmp(right))
        .map(|(_, tag)| tag))
}

fn cargo_lock_files_for_manifests(
    root: &Path,
    manifest_files: &[PathBuf],
) -> Result<Vec<PathBuf>, String> {
    let mut lock_files = manifest_files
        .iter()
        .map(|manifest_path| cargo_lock_file_for_manifest(root, manifest_path))
        .collect::<Result<Vec<_>, _>>()?
        .into_iter()
        .flatten()
        .collect::<Vec<_>>();
    lock_files.sort();
    lock_files.dedup();
    Ok(lock_files)
}

fn cargo_lock_file_for_manifest(
    root: &Path,
    manifest_path: &Path,
) -> Result<Option<PathBuf>, String> {
    let Some(mut current) = manifest_path.parent() else {
        return Ok(None);
    };
    loop {
        let lock_path = current.join("Cargo.lock");
        validate_release_path(root, &lock_path)?;
        if lock_path
            .try_exists()
            .map_err(|error| format!("failed to inspect {}: {error}", lock_path.display()))?
        {
            return Ok(Some(lock_path));
        }
        if current == root {
            return Ok(None);
        }
        let Some(parent) = current.parent() else {
            return Ok(None);
        };
        current = parent;
    }
}

fn planned_hooks(config: &Config, mode: PlanMode) -> Vec<PlannedHook> {
    let hooks = &config.hooks;
    let all = [
        ("before_version", &hooks.before_version),
        ("after_version", &hooks.after_version),
        ("before_commit", &hooks.before_commit),
        ("after_commit", &hooks.after_commit),
        ("before_tag", &hooks.before_tag),
        ("after_tag", &hooks.after_tag),
        ("before_push", &hooks.before_push),
        ("after_push", &hooks.after_push),
    ];
    all.into_iter()
        .filter(|(name, _)| {
            mode == PlanMode::Release || matches!(*name, "before_version" | "after_version")
        })
        .filter_map(|(name, command)| {
            command.as_ref().map(|command| PlannedHook {
                name: name.to_owned(),
                command: command.clone(),
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn plan_mode_names_are_stable() {
        assert_eq!(PlanMode::Bump.as_str(), "bump");
        assert_eq!(PlanMode::Release.as_str(), "release");
    }

    #[test]
    fn release_paths_must_stay_inside_the_root() -> Result<(), String> {
        let temp = TempDir::new().map_err(|error| error.to_string())?;
        let root = temp.path().join("root");
        fs::create_dir(&root).map_err(|error| error.to_string())?;

        let error = validate_release_path(&root, &temp.path().join("package.json"))
            .expect_err("outside paths must be rejected");

        assert!(error.contains("unsafe path outside"));
        Ok(())
    }

    #[test]
    fn missing_release_files_are_safe_when_their_parent_is_safe() -> Result<(), String> {
        let temp = TempDir::new().map_err(|error| error.to_string())?;

        assert_eq!(
            read_optional_text(temp.path(), &temp.path().join("CHANGELOG.md"))?,
            None
        );
        Ok(())
    }

    #[cfg(unix)]
    #[test]
    fn release_paths_reject_symlink_components() -> Result<(), String> {
        use std::os::unix::fs::symlink;

        let temp = TempDir::new().map_err(|error| error.to_string())?;
        let root = temp.path().join("root");
        let outside = temp.path().join("outside");
        fs::create_dir(&root).map_err(|error| error.to_string())?;
        fs::create_dir(&outside).map_err(|error| error.to_string())?;
        symlink(&outside, root.join("linked")).map_err(|error| error.to_string())?;

        let error = read_text(&root, &root.join("linked/package.json"))
            .expect_err("symlink components must be rejected");

        assert!(error.contains("symbolic link"));
        Ok(())
    }
}
