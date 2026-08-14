use crate::{
    config::{Config, WorkspaceConfig},
    package_json::{manifest_names, read_package, read_package_manifest, PackageInfo},
};
use globset::{GlobBuilder, GlobSet, GlobSetBuilder};
use ignore::WalkBuilder;
use noyalib::borrowed::{from_str_borrowed, BorrowedValue};
use std::{
    collections::BTreeSet,
    fs,
    path::{Component, Path, PathBuf},
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PackageFile {
    pub dir: PathBuf,
    pub package_json: PathBuf,
    pub info: PackageInfo,
}

pub fn discover_packages(root: &Path, config: &Config) -> Result<Vec<PackageFile>, String> {
    let mut package_paths = Vec::new();
    let mut matched_workspace_package = false;
    let root_package = resolve_root_package_manifest(root, config);
    let workspace_pattern_values = effective_workspace_patterns(root, config)?;
    let workspace_patterns = WorkspacePatterns::new(&workspace_pattern_values)?;
    let workspace_ignores = WorkspaceIgnores::new(&config.workspaces.ignore)?;
    let workspace_search_roots = workspace_search_roots(root, &workspace_pattern_values);
    let all_workspace_search_roots_ignored = !workspace_search_roots.is_empty()
        && workspace_search_roots
            .iter()
            .all(|search_root| workspace_ignores.is_match(root, search_root));

    if config.workspaces.include_root {
        if let Some(root_package) = &root_package {
            package_paths.push(root_package.clone());
        }
    }

    for search_root in workspace_search_roots {
        if workspace_ignores.is_match(root, &search_root) {
            continue;
        }
        for dir in collect_dirs(root, &search_root, &config.workspaces)? {
            if !workspace_patterns.is_match(root, &dir) {
                continue;
            }
            if let Some(package_json) = resolve_package_manifest(&dir) {
                if Some(&package_json) != root_package.as_ref() {
                    matched_workspace_package = true;
                }
                package_paths.push(package_json);
            }
        }
    }

    package_paths.sort();
    package_paths.dedup();

    let mut packages = Vec::with_capacity(package_paths.len());
    for package_json in package_paths {
        validate_release_path(root, &package_json)?;
        let dir = package_json
            .parent()
            .ok_or_else(|| format!("{} has no parent directory", package_json.display()))?
            .to_path_buf();
        let info = read_package(&package_json)?;
        packages.push(PackageFile {
            dir,
            package_json,
            info,
        });
    }

    if packages.is_empty() {
        return Err(format!(
            "no packages discovered under {} from configured workspaces",
            root.display()
        ));
    }

    if !workspace_pattern_values.is_empty()
        && !matched_workspace_package
        && !all_workspace_search_roots_ignored
    {
        return Err(format!(
            "no workspace package manifests matched configured workspaces under {}",
            root.display()
        ));
    }

    Ok(packages)
}

fn resolve_root_package_manifest(root: &Path, config: &Config) -> Option<PathBuf> {
    let configured = root.join(&config.version.root_package);
    if configured.exists() {
        return Some(configured);
    }
    if config.version.root_package == "package.json" {
        return resolve_package_manifest(root);
    }
    None
}

pub(crate) fn resolve_package_manifest(dir: &Path) -> Option<PathBuf> {
    manifest_names()
        .iter()
        .map(|name| dir.join(name))
        .find(|path| path.exists())
}

fn effective_workspace_patterns(root: &Path, config: &Config) -> Result<Vec<String>, String> {
    if !config.workspaces.patterns.is_empty() {
        return Ok(config.workspaces.patterns.clone());
    }
    if let Some(patterns) = read_pnpm_workspace_patterns(root)? {
        return Ok(patterns);
    }
    let Some(root_manifest) = resolve_root_package_manifest(root, config) else {
        return Ok(Vec::new());
    };
    validate_release_path(root, &root_manifest)?;
    Ok(read_package_manifest(&root_manifest)?
        .workspaces
        .unwrap_or_default())
}

fn read_pnpm_workspace_patterns(root: &Path) -> Result<Option<Vec<String>>, String> {
    let path = root.join("pnpm-workspace.yaml");
    validate_release_path(root, &path)?;
    if !path.exists() {
        return Ok(None);
    }
    let contents = fs::read_to_string(&path)
        .map_err(|error| format!("failed to read {}: {error}", path.display()))?;
    let value = from_str_borrowed(&contents)
        .map_err(|error| format!("failed to parse {} as YAML: {error}", path.display()))?;
    let Some(mapping) = value.as_mapping() else {
        return Ok(None);
    };
    let Some(packages) = mapping.get("packages").and_then(BorrowedValue::as_sequence) else {
        return Ok(None);
    };
    let patterns = packages
        .iter()
        .map(|item| {
            item.as_str()
                .map(ToOwned::to_owned)
                .ok_or_else(|| format!("{} packages must be strings", path.display()))
        })
        .collect::<Result<Vec<_>, _>>()?;
    Ok(Some(patterns))
}

pub(crate) fn validate_release_path(root: &Path, path: &Path) -> Result<(), String> {
    let unsafe_path = || {
        format!(
            "release plan contains an unsafe path outside {} or through a symbolic link: {}",
            root.display(),
            path.display()
        )
    };
    if !root.is_absolute() || !path.is_absolute() {
        return Err(unsafe_path());
    }
    let canonical_root = root
        .canonicalize()
        .map_err(|error| format!("failed to resolve release root {}: {error}", root.display()))?;
    let relative = path.strip_prefix(root).map_err(|_| unsafe_path())?;
    if relative.as_os_str().is_empty()
        || relative.components().any(|component| {
            matches!(
                component,
                Component::ParentDir
                    | Component::CurDir
                    | Component::RootDir
                    | Component::Prefix(_)
            )
        })
    {
        return Err(unsafe_path());
    }

    let mut current = root.to_path_buf();
    for component in relative.components() {
        current.push(component.as_os_str());
        match fs::symlink_metadata(&current) {
            Ok(metadata) => {
                if metadata.file_type().is_symlink()
                    || !current
                        .canonicalize()
                        .is_ok_and(|resolved| resolved.starts_with(&canonical_root))
                {
                    return Err(unsafe_path());
                }
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => break,
            Err(error) => {
                return Err(format!("failed to inspect {}: {error}", current.display()));
            }
        }
    }
    Ok(())
}

pub fn verify_consistent_versions(packages: &[PackageFile]) -> Result<(), String> {
    let Some(first) = packages.first() else {
        return Ok(());
    };

    let expected = &first.info.version;
    let mismatches: Vec<&PackageFile> = packages
        .iter()
        .filter(|package| package.info.version != *expected)
        .collect();

    if mismatches.is_empty() {
        return Ok(());
    }

    let details = packages
        .iter()
        .map(|package| {
            format!(
                "{} has version {}",
                package_label(package),
                package.info.version
            )
        })
        .collect::<Vec<_>>();

    Err(format!("package versions differ: {}", details.join("; ")))
}

struct WorkspacePatterns {
    includes: GlobSet,
    excludes: GlobSet,
}

impl WorkspacePatterns {
    fn new(patterns: &[String]) -> Result<Self, String> {
        let mut includes = GlobSetBuilder::new();
        let mut excludes = GlobSetBuilder::new();

        for pattern in patterns {
            let (excluded, pattern) = match pattern.strip_prefix('!') {
                Some(pattern) => (true, pattern),
                None => (false, pattern.as_str()),
            };
            let glob = GlobBuilder::new(pattern)
                .literal_separator(true)
                .build()
                .map_err(|error| format!("invalid workspace pattern {pattern:?}: {error}"))?;
            if excluded {
                excludes.add(glob);
            } else {
                includes.add(glob);
            }
        }

        Ok(Self {
            includes: includes
                .build()
                .map_err(|error| format!("failed to build workspace include patterns: {error}"))?,
            excludes: excludes
                .build()
                .map_err(|error| format!("failed to build workspace exclude patterns: {error}"))?,
        })
    }

    fn is_match(&self, root: &Path, dir: &Path) -> bool {
        let Ok(relative) = dir.strip_prefix(root) else {
            return false;
        };
        self.includes.is_match(relative) && !self.excludes.is_match(relative)
    }
}

fn workspace_search_roots(root: &Path, patterns: &[String]) -> Vec<PathBuf> {
    let mut roots = BTreeSet::new();
    for pattern in patterns {
        if pattern.starts_with('!') {
            continue;
        }
        let prefix = static_pattern_prefix(pattern);
        let search_root = if prefix.is_empty() {
            root.to_path_buf()
        } else {
            root.join(prefix)
        };
        roots.insert(search_root);
    }
    roots.into_iter().collect()
}

struct WorkspaceIgnores {
    names: BTreeSet<String>,
    patterns: GlobSet,
}

impl WorkspaceIgnores {
    fn new(patterns: &[String]) -> Result<Self, String> {
        let mut names = BTreeSet::new();
        let mut globs = GlobSetBuilder::new();

        for pattern in patterns {
            if is_plain_path_segment(pattern) {
                names.insert(pattern.to_owned());
            } else {
                let glob = GlobBuilder::new(pattern)
                    .literal_separator(true)
                    .build()
                    .map_err(|error| {
                        format!("invalid workspace ignore pattern {pattern:?}: {error}")
                    })?;
                globs.add(glob);
            }
        }

        Ok(Self {
            names,
            patterns: globs
                .build()
                .map_err(|error| format!("failed to build workspace ignore patterns: {error}"))?,
        })
    }

    fn is_match(&self, root: &Path, dir: &Path) -> bool {
        if dir
            .file_name()
            .is_some_and(|name| name == "node_modules" || name == ".git")
        {
            return true;
        }
        if dir
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| self.names.contains(name))
        {
            return true;
        }
        let Ok(relative) = dir.strip_prefix(root) else {
            return false;
        };
        self.patterns.is_match(relative)
    }
}

fn is_plain_path_segment(pattern: &str) -> bool {
    !pattern.contains('/')
        && !pattern
            .chars()
            .any(|character| matches!(character, '*' | '?' | '[' | '{'))
}

fn static_pattern_prefix(pattern: &str) -> String {
    let glob_start = pattern
        .char_indices()
        .find_map(|(index, character)| matches!(character, '*' | '?' | '[' | '{').then_some(index))
        .unwrap_or(pattern.len());
    let prefix = &pattern[..glob_start];
    match prefix.rsplit_once('/') {
        Some((parent, _segment)) => parent.trim_end_matches('/').to_owned(),
        None if glob_start == pattern.len() => pattern.trim_end_matches('/').to_owned(),
        None => String::new(),
    }
}

fn collect_dirs(
    root: &Path,
    search_root: &Path,
    workspaces: &WorkspaceConfig,
) -> Result<Vec<PathBuf>, String> {
    if !search_root.exists() {
        return Ok(Vec::new());
    }
    if !search_root.is_dir() {
        return Ok(Vec::new());
    }

    let ignores = WorkspaceIgnores::new(&workspaces.ignore)?;
    let root = root.to_path_buf();
    let mut builder = WalkBuilder::new(search_root);
    builder
        .hidden(false)
        .parents(workspaces.use_gitignore)
        .git_ignore(workspaces.use_gitignore)
        .require_git(false)
        .git_global(false)
        .git_exclude(false)
        .ignore(false);

    let mut dirs = Vec::new();
    for entry in builder
        .filter_entry(move |entry| !ignores.is_match(&root, entry.path()))
        .build()
    {
        let entry = entry.map_err(|error| {
            format!(
                "failed to read workspace directory {}: {error}",
                search_root.display()
            )
        })?;
        if entry
            .file_type()
            .is_some_and(|file_type| file_type.is_dir())
        {
            dirs.push(entry.path().to_path_buf());
        }
    }

    Ok(dirs)
}

fn package_label(package: &PackageFile) -> String {
    match &package.info.name {
        Some(name) => format!("{name} ({})", package.package_json.display()),
        None => package.package_json.display().to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{
        ChangelogConfig, Config, GitConfig, GithubReleaseConfig, HooksConfig, VersionConfig,
        WorkspaceConfig,
    };
    use std::{fs, path::Path};
    use tempfile::TempDir;

    #[test]
    fn discovers_root_and_workspace_packages() -> Result<(), String> {
        let temp = TempDir::new().map_err(|error| error.to_string())?;
        write_package(temp.path(), "root", "1.2.3")?;
        write_package(&temp.path().join("packages/a"), "a", "1.2.3")?;
        write_package(&temp.path().join("apps/web"), "web", "1.2.3")?;
        fs::create_dir_all(temp.path().join("packages/empty"))
            .map_err(|error| error.to_string())?;
        let config = test_config(vec!["packages/*", "apps/*"], true);

        let packages = discover_packages(temp.path(), &config)?;

        assert_package_dirs(
            &packages,
            &[
                &temp.path().join("apps/web"),
                temp.path(),
                &temp.path().join("packages/a"),
            ],
        );
        verify_consistent_versions(&packages)?;
        Ok(())
    }

    #[test]
    fn discovers_root_package_when_workspace_patterns_are_omitted() -> Result<(), String> {
        let temp = TempDir::new().map_err(|error| error.to_string())?;
        write_package(temp.path(), "root", "1.2.3")?;
        let config = test_config(Vec::new(), true);

        let packages = discover_packages(temp.path(), &config)?;

        assert_package_dirs(&packages, &[temp.path()]);
        verify_consistent_versions(&packages)?;
        Ok(())
    }

    #[test]
    fn infers_workspace_patterns_from_pnpm_workspace_yaml() -> Result<(), String> {
        let temp = TempDir::new().map_err(|error| error.to_string())?;
        write_package(temp.path(), "root", "1.2.3")?;
        write_package(&temp.path().join("packages/a"), "a", "1.2.3")?;
        fs::write(
            temp.path().join("pnpm-workspace.yaml"),
            "packages:\n  - packages/*\n",
        )
        .map_err(|error| error.to_string())?;
        let config = test_config(Vec::new(), true);

        let packages = discover_packages(temp.path(), &config)?;

        assert_package_dirs(&packages, &[temp.path(), &temp.path().join("packages/a")]);
        Ok(())
    }

    #[test]
    fn infers_workspace_patterns_from_root_manifest_workspaces() -> Result<(), String> {
        let temp = TempDir::new().map_err(|error| error.to_string())?;
        fs::write(
            temp.path().join("package.json5"),
            r#"{
  name: "root",
  version: "1.2.3",
  workspaces: {
    packages: ["packages/*"],
  },
}"#,
        )
        .map_err(|error| error.to_string())?;
        write_package(&temp.path().join("packages/a"), "a", "1.2.3")?;
        let config = test_config(Vec::new(), true);

        let packages = discover_packages(temp.path(), &config)?;

        assert_package_dirs(&packages, &[temp.path(), &temp.path().join("packages/a")]);
        Ok(())
    }

    #[test]
    fn explicit_workspace_patterns_override_package_manager_metadata() -> Result<(), String> {
        let temp = TempDir::new().map_err(|error| error.to_string())?;
        write_package(temp.path(), "root", "1.2.3")?;
        write_package(&temp.path().join("packages/a"), "a", "1.2.3")?;
        write_package(&temp.path().join("apps/web"), "web", "1.2.3")?;
        fs::write(
            temp.path().join("pnpm-workspace.yaml"),
            "packages:\n  - packages/*\n",
        )
        .map_err(|error| error.to_string())?;
        let config = test_config(vec!["apps/*"], true);

        let packages = discover_packages(temp.path(), &config)?;

        assert_package_dirs(&packages, &[&temp.path().join("apps/web"), temp.path()]);
        Ok(())
    }

    #[test]
    fn discovers_yaml_package_manifests_in_workspaces() -> Result<(), String> {
        let temp = TempDir::new().map_err(|error| error.to_string())?;
        write_package(temp.path(), "root", "1.2.3")?;
        fs::create_dir_all(temp.path().join("packages/a")).map_err(|error| error.to_string())?;
        fs::write(
            temp.path().join("packages/a/package.yaml"),
            "name: a\nversion: 1.2.3\n",
        )
        .map_err(|error| error.to_string())?;
        let config = test_config(vec!["packages/*"], true);

        let packages = discover_packages(temp.path(), &config)?;

        let manifests = packages
            .iter()
            .map(|package| {
                package
                    .package_json
                    .strip_prefix(temp.path())
                    .map_err(|error| error.to_string())
            })
            .collect::<Result<Vec<_>, _>>()?;
        assert_eq!(
            manifests,
            vec![
                Path::new("package.json"),
                Path::new("packages/a/package.yaml")
            ]
        );
        Ok(())
    }

    #[test]
    fn detects_inconsistent_versions() -> Result<(), String> {
        let temp = TempDir::new().map_err(|error| error.to_string())?;
        write_package(temp.path(), "root", "1.2.3")?;
        write_package(&temp.path().join("packages/a"), "a", "1.2.4")?;
        let config = test_config(vec!["packages/*"], true);
        let packages = discover_packages(temp.path(), &config)?;

        let error = verify_consistent_versions(&packages)
            .expect_err("version mismatch should return an error");

        assert!(error.contains("root"));
        assert!(error.contains("a"));
        assert!(error.contains("1.2.3"));
        assert!(error.contains("1.2.4"));
        Ok(())
    }

    #[test]
    fn mismatch_message_lists_versions_without_arbitrary_expected_baseline() -> Result<(), String> {
        let temp = TempDir::new().map_err(|error| error.to_string())?;
        write_package(temp.path(), "root", "1.2.3")?;
        write_package(&temp.path().join("apps/web"), "web", "1.2.4")?;
        let config = test_config(vec!["apps/*"], true);
        let packages = discover_packages(temp.path(), &config)?;

        let error = verify_consistent_versions(&packages)
            .expect_err("version mismatch should return an error");

        assert!(error.contains("root"));
        assert!(error.contains("web"));
        assert!(error.contains("1.2.3"));
        assert!(error.contains("1.2.4"));
        assert!(!error.contains("expected all packages to use 1.2.4"));
        Ok(())
    }

    #[test]
    fn discovers_nested_prefix_workspace_pattern() -> Result<(), String> {
        let temp = TempDir::new().map_err(|error| error.to_string())?;
        write_package(&temp.path().join("presets/packages/foo"), "foo", "1.2.3")?;
        let config = test_config(vec!["presets/packages/*"], false);

        let packages = discover_packages(temp.path(), &config)?;

        assert_package_dirs(&packages, &[&temp.path().join("presets/packages/foo")]);
        Ok(())
    }

    #[test]
    fn discovers_packages_with_recursive_workspace_globs() -> Result<(), String> {
        let temp = TempDir::new().map_err(|error| error.to_string())?;
        write_package(&temp.path().join("packages/a"), "a", "1.2.3")?;
        write_package(&temp.path().join("packages/nested/b"), "b", "1.2.3")?;
        let config = test_config(vec!["packages/**"], false);

        let packages = discover_packages(temp.path(), &config)?;

        assert_package_dirs(
            &packages,
            &[
                &temp.path().join("packages/a"),
                &temp.path().join("packages/nested/b"),
            ],
        );
        Ok(())
    }

    #[test]
    fn excludes_packages_with_negative_workspace_globs() -> Result<(), String> {
        let temp = TempDir::new().map_err(|error| error.to_string())?;
        write_package(&temp.path().join("packages/a"), "a", "1.2.3")?;
        write_package(&temp.path().join("packages/demo"), "demo", "1.2.3")?;
        write_package(
            &temp.path().join("packages/nested/fixture"),
            "fixture",
            "1.2.3",
        )?;
        let config = test_config(
            vec!["packages/**", "!packages/demo", "!packages/**/fixture"],
            false,
        );

        let packages = discover_packages(temp.path(), &config)?;

        assert_package_dirs(&packages, &[&temp.path().join("packages/a")]);
        Ok(())
    }

    #[test]
    fn recursive_workspace_globs_ignore_node_modules() -> Result<(), String> {
        let temp = TempDir::new().map_err(|error| error.to_string())?;
        write_package(&temp.path().join("packages/a"), "a", "1.2.3")?;
        write_package(
            &temp.path().join("packages/a/node_modules/dependency"),
            "dependency",
            "9.9.9",
        )?;
        let config = test_config(vec!["packages/**"], false);

        let packages = discover_packages(temp.path(), &config)?;

        assert_package_dirs(&packages, &[&temp.path().join("packages/a")]);
        Ok(())
    }

    #[test]
    fn workspace_discovery_respects_root_gitignore() -> Result<(), String> {
        let temp = TempDir::new().map_err(|error| error.to_string())?;
        write_package(&temp.path().join("packages/a"), "a", "1.2.3")?;
        write_package(
            &temp.path().join("packages/generated"),
            "generated",
            "9.9.9",
        )?;
        fs::write(temp.path().join(".gitignore"), "packages/generated/\n")
            .map_err(|error| error.to_string())?;
        let config = test_config(vec!["packages/**"], false);

        let packages = discover_packages(temp.path(), &config)?;

        assert_package_dirs(&packages, &[&temp.path().join("packages/a")]);
        Ok(())
    }

    #[test]
    fn workspace_discovery_respects_nested_gitignore() -> Result<(), String> {
        let temp = TempDir::new().map_err(|error| error.to_string())?;
        write_package(&temp.path().join("packages/a"), "a", "1.2.3")?;
        write_package(
            &temp.path().join("packages/a/generated"),
            "generated",
            "9.9.9",
        )?;
        fs::write(temp.path().join("packages/a/.gitignore"), "generated/\n")
            .map_err(|error| error.to_string())?;
        let config = test_config(vec!["packages/**"], false);

        let packages = discover_packages(temp.path(), &config)?;

        assert_package_dirs(&packages, &[&temp.path().join("packages/a")]);
        Ok(())
    }

    #[test]
    fn workspace_discovery_can_disable_gitignore() -> Result<(), String> {
        let temp = TempDir::new().map_err(|error| error.to_string())?;
        write_package(&temp.path().join("packages/a"), "a", "1.2.3")?;
        write_package(
            &temp.path().join("packages/generated"),
            "generated",
            "1.2.3",
        )?;
        fs::write(temp.path().join(".gitignore"), "packages/generated/\n")
            .map_err(|error| error.to_string())?;
        let mut config = test_config(vec!["packages/**"], false);
        config.workspaces.use_gitignore = false;

        let packages = discover_packages(temp.path(), &config)?;

        assert_package_dirs(
            &packages,
            &[
                &temp.path().join("packages/a"),
                &temp.path().join("packages/generated"),
            ],
        );
        Ok(())
    }

    #[test]
    fn workspace_discovery_respects_configured_ignore() -> Result<(), String> {
        let temp = TempDir::new().map_err(|error| error.to_string())?;
        write_package(&temp.path().join("packages/a"), "a", "1.2.3")?;
        write_package(&temp.path().join("packages/fixtures"), "fixtures", "9.9.9")?;
        let mut config = test_config(vec!["packages/**"], false);
        config.workspaces.ignore = vec!["fixtures".to_owned()];

        let packages = discover_packages(temp.path(), &config)?;

        assert_package_dirs(&packages, &[&temp.path().join("packages/a")]);
        Ok(())
    }

    #[test]
    fn workspace_discovery_can_ignore_exact_search_root() -> Result<(), String> {
        let temp = TempDir::new().map_err(|error| error.to_string())?;
        write_package(temp.path(), "root", "1.2.3")?;
        write_package(&temp.path().join("docs"), "docs", "9.9.9")?;
        let mut config = test_config(vec!["docs"], true);
        config.workspaces.ignore = vec!["docs".to_owned()];

        let packages = discover_packages(temp.path(), &config)?;

        assert_package_dirs(&packages, &[temp.path()]);
        Ok(())
    }

    #[test]
    fn skips_root_when_include_root_is_false() -> Result<(), String> {
        let temp = TempDir::new().map_err(|error| error.to_string())?;
        write_package(temp.path(), "root", "1.2.3")?;
        write_package(&temp.path().join("packages/a"), "a", "1.2.3")?;
        let config = test_config(vec!["packages/*"], false);

        let packages = discover_packages(temp.path(), &config)?;

        assert_package_dirs(&packages, &[&temp.path().join("packages/a")]);
        Ok(())
    }

    #[test]
    fn single_star_workspace_globs_do_not_cross_path_segments() -> Result<(), String> {
        let temp = TempDir::new().map_err(|error| error.to_string())?;
        write_package(&temp.path().join("packages/a"), "a", "1.2.3")?;
        write_package(
            &temp.path().join("packages/a/node_modules/dependency"),
            "dependency",
            "9.9.9",
        )?;
        let config = test_config(vec!["packages/*"], false);

        let packages = discover_packages(temp.path(), &config)?;

        assert_package_dirs(&packages, &[&temp.path().join("packages/a")]);
        Ok(())
    }

    #[test]
    fn errors_when_root_exists_but_no_workspace_packages_match() -> Result<(), String> {
        let temp = TempDir::new().map_err(|error| error.to_string())?;
        write_package(temp.path(), "root", "1.2.3")?;
        fs::create_dir_all(temp.path().join("packages/empty"))
            .map_err(|error| error.to_string())?;
        let config = test_config(vec!["packages/*"], true);

        let error = discover_packages(temp.path(), &config)
            .expect_err("root package alone should not satisfy workspace discovery");

        assert!(error.contains("no workspace package manifests matched configured workspaces"));
        Ok(())
    }

    #[test]
    fn deduplicates_duplicate_workspace_patterns() -> Result<(), String> {
        let temp = TempDir::new().map_err(|error| error.to_string())?;
        write_package(&temp.path().join("packages/a"), "a", "1.2.3")?;
        let config = test_config(vec!["packages/*", "packages/*"], false);

        let packages = discover_packages(temp.path(), &config)?;

        assert_eq!(packages.len(), 1);
        assert_package_dirs(&packages, &[&temp.path().join("packages/a")]);
        Ok(())
    }

    #[test]
    fn errors_when_no_packages_are_discovered() -> Result<(), String> {
        let temp = TempDir::new().map_err(|error| error.to_string())?;
        fs::create_dir_all(temp.path().join("packages/empty"))
            .map_err(|error| error.to_string())?;
        let config = test_config(vec!["packages/*"], false);

        let error =
            discover_packages(temp.path(), &config).expect_err("empty discovery should fail");

        assert!(error.contains("no packages"));
        Ok(())
    }

    #[test]
    fn discovers_packages_across_multiple_workspace_roots() -> Result<(), String> {
        let temp = TempDir::new().map_err(|error| error.to_string())?;
        write_package(&temp.path().join("apps/docs"), "docs", "1.2.3")?;
        write_package(&temp.path().join("bundle/core"), "core-bundle", "1.2.3")?;
        write_package(&temp.path().join("packages/sheets"), "sheets", "1.2.3")?;
        write_package(
            &temp.path().join("packages-experimental/labs"),
            "labs",
            "1.2.3",
        )?;
        write_package(
            &temp.path().join("presets/packages/basic"),
            "basic",
            "1.2.3",
        )?;
        let config = test_config(
            vec![
                "apps/*",
                "bundle/*",
                "packages/*",
                "packages-experimental/*",
                "presets/packages/*",
            ],
            false,
        );

        let packages = discover_packages(temp.path(), &config)?;

        assert_package_dirs(
            &packages,
            &[
                &temp.path().join("apps/docs"),
                &temp.path().join("bundle/core"),
                &temp.path().join("packages/sheets"),
                &temp.path().join("packages-experimental/labs"),
                &temp.path().join("presets/packages/basic"),
            ],
        );
        Ok(())
    }

    fn test_config(patterns: Vec<&str>, include_root: bool) -> Config {
        Config {
            version: VersionConfig {
                root_package: "package.json".to_owned(),
                require_consistent_versions: true,
                cargo_manifest_paths: Vec::new(),
            },
            workspaces: WorkspaceConfig {
                patterns: patterns.into_iter().map(ToOwned::to_owned).collect(),
                include_root,
                ignore: Vec::new(),
                use_gitignore: true,
            },
            changelog: ChangelogConfig {
                enabled: true,
                infile: "CHANGELOG.md".to_owned(),
                preset: crate::changelog::ChangelogPreset::Angular,
            },
            git: GitConfig {
                require_clean_worktree: true,
                commit_message: "chore(release): release v${version}".to_owned(),
                tag_name: "v${version}".to_owned(),
                push: "atomic".to_owned(),
            },
            hooks: HooksConfig::default(),
            github_release: GithubReleaseConfig { enabled: false },
        }
    }

    fn write_package(dir: &Path, name: &str, version: &str) -> Result<(), String> {
        fs::create_dir_all(dir).map_err(|error| error.to_string())?;
        fs::write(
            dir.join("package.json"),
            format!(r#"{{"name":"{name}","version":"{version}"}}"#),
        )
        .map_err(|error| error.to_string())
    }

    fn assert_package_dirs(packages: &[PackageFile], expected: &[&Path]) {
        let actual: Vec<&Path> = packages
            .iter()
            .map(|package| package.dir.as_path())
            .collect();
        assert_eq!(actual, expected);
    }
}
