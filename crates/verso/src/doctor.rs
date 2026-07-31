use crate::{
    config::{self, Config},
    diagnostic::{render_check, stdout_supports_color},
    git,
    release::{current_version, release_root, verify_cargo_manifest_versions},
    workspace::{
        discover_packages, resolve_package_manifest, verify_consistent_versions, PackageFile,
    },
};
use semver::Version;
use serde::Serialize;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub struct ProjectInspection {
    pub root: PathBuf,
    pub config: Config,
    pub config_source: ConfigSource,
    pub packages: Vec<PackageFile>,
    pub cargo_manifest_files: Vec<PathBuf>,
    pub current_version: Version,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConfigSource {
    File(PathBuf),
    BuiltInSinglePackageDefaults,
}

#[derive(Debug, Clone, Serialize)]
pub struct DoctorReport {
    pub ok: bool,
    pub checks: Vec<DoctorCheck>,
    #[serde(rename = "packageCount")]
    pub package_count: usize,
    #[serde(rename = "currentVersion")]
    pub current_version: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct DoctorCheck {
    pub name: String,
    pub status: DoctorStatus,
    pub message: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum DoctorStatus {
    Pass,
    Fail,
}

pub fn inspect_project(
    config_path: &Path,
    allow_missing_default_config: bool,
) -> Result<ProjectInspection, String> {
    let root = release_root(config_path)?;
    let (config, config_source) =
        load_project_config(&root, config_path, allow_missing_default_config)?;
    let packages = discover_packages(&root, &config)?;
    let cargo_manifest_files = config
        .version
        .cargo_manifest_paths
        .iter()
        .map(|path| root.join(path))
        .collect::<Vec<_>>();

    if config.version.require_consistent_versions {
        verify_consistent_versions(&packages)?;
    }

    let current_version =
        current_version(&root, Path::new(&config.version.root_package), &packages)?;
    if config.version.require_consistent_versions {
        verify_cargo_manifest_versions(&root, &cargo_manifest_files, &current_version)?;
    }

    Ok(ProjectInspection {
        root,
        config,
        config_source,
        packages,
        cargo_manifest_files,
        current_version,
    })
}

pub fn check(config_path: &Path, allow_missing_default_config: bool) -> DoctorReport {
    let mut checks = Vec::new();
    let mut package_count = 0;
    let mut current_version_value = None;

    let root = match release_root(config_path) {
        Ok(root) => {
            checks.push(pass("release root", format!("using {}", root.display())));
            root
        }
        Err(error) => {
            checks.push(fail("release root", error));
            return report(checks, package_count, current_version_value);
        }
    };

    let config = match load_project_config(&root, config_path, allow_missing_default_config) {
        Ok((config, source)) => {
            let message = match &source {
                ConfigSource::File(path) => format!("loaded {}", path.display()),
                ConfigSource::BuiltInSinglePackageDefaults => {
                    "using built-in single-package defaults".to_owned()
                }
            };
            checks.push(pass("config", message));
            config
        }
        Err(error) => {
            checks.push(fail("config", error));
            return report(checks, package_count, current_version_value);
        }
    };

    let packages = match discover_packages(&root, &config) {
        Ok(packages) => {
            package_count = packages.len();
            checks.push(pass(
                "packages",
                format!("discovered {} package(s)", packages.len()),
            ));
            packages
        }
        Err(error) => {
            checks.push(fail("packages", error));
            return report(checks, package_count, current_version_value);
        }
    };

    match current_version(&root, Path::new(&config.version.root_package), &packages) {
        Ok(version) => {
            current_version_value = Some(version.to_string());
            checks.push(pass("current version", version.to_string()));
            if config.version.require_consistent_versions {
                match verify_consistent_versions(&packages) {
                    Ok(()) => checks.push(pass("package versions", "versions are consistent")),
                    Err(error) => checks.push(fail("package versions", error)),
                }
                let cargo_manifest_files = config
                    .version
                    .cargo_manifest_paths
                    .iter()
                    .map(|path| root.join(path))
                    .collect::<Vec<_>>();
                match verify_cargo_manifest_versions(&root, &cargo_manifest_files, &version) {
                    Ok(()) => checks.push(pass("cargo manifests", "versions are consistent")),
                    Err(error) => checks.push(fail("cargo manifests", error)),
                }
            }
        }
        Err(error) => checks.push(fail("current version", error)),
    }

    if config.changelog.enabled {
        let changelog = root.join(&config.changelog.infile);
        let changelog_parent_ok = changelog
            .parent()
            .is_some_and(|parent| parent.exists() && parent.is_dir());
        if changelog_parent_ok {
            checks.push(pass(
                "changelog",
                format!("{} can be written", changelog.display()),
            ));
        } else {
            checks.push(fail(
                "changelog",
                format!(
                    "parent directory for {} does not exist",
                    changelog.display()
                ),
            ));
        }
    } else {
        checks.push(pass("changelog", "disabled"));
    }

    match git::release_upstream(&root) {
        Ok(upstream) => checks.push(pass(
            "git upstream",
            format!("ready to push to {}/{}", upstream.remote, upstream.branch),
        )),
        Err(error) => checks.push(fail("git upstream", error)),
    }

    report(checks, package_count, current_version_value)
}

pub fn run(
    config_path: &Path,
    allow_missing_default_config: bool,
    json: bool,
) -> Result<(), String> {
    if run_with_status(config_path, allow_missing_default_config, json)? {
        Ok(())
    } else {
        Err("doctor found release readiness problems".to_string())
    }
}

#[doc(hidden)]
pub fn run_with_status(
    config_path: &Path,
    allow_missing_default_config: bool,
    json: bool,
) -> Result<bool, String> {
    let report = check(config_path, allow_missing_default_config);
    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(&report)
                .map_err(|error| format!("failed to serialize doctor report: {error}"))?
        );
    } else {
        println!("{}", render_text_report(&report, stdout_supports_color()));
    }

    Ok(report.ok)
}

fn load_project_config(
    root: &Path,
    config_path: &Path,
    allow_missing_default_config: bool,
) -> Result<(Config, ConfigSource), String> {
    if config_path.exists() {
        return config::load_config(config_path)
            .map(|config| (config, ConfigSource::File(config_path.to_path_buf())));
    }

    if allow_missing_default_config && resolve_package_manifest(root).is_some() {
        return Ok((
            config::default_config(),
            ConfigSource::BuiltInSinglePackageDefaults,
        ));
    }

    config::load_config(config_path)
        .map(|config| (config, ConfigSource::File(config_path.to_path_buf())))
}

fn render_text_report(report: &DoctorReport, styled: bool) -> String {
    let mut output = String::from("Verso doctor\n\n");
    for check in &report.checks {
        output.push_str(&render_check(
            check.status == DoctorStatus::Pass,
            &check.name,
            &check.message,
            styled,
        ));
    }
    output
}

fn pass(name: impl Into<String>, message: impl Into<String>) -> DoctorCheck {
    DoctorCheck {
        name: name.into(),
        status: DoctorStatus::Pass,
        message: message.into(),
    }
}

fn fail(name: impl Into<String>, message: impl Into<String>) -> DoctorCheck {
    DoctorCheck {
        name: name.into(),
        status: DoctorStatus::Fail,
        message: message.into(),
    }
}

fn report(
    checks: Vec<DoctorCheck>,
    package_count: usize,
    current_version: Option<String>,
) -> DoctorReport {
    DoctorReport {
        ok: checks
            .iter()
            .all(|check| check.status == DoctorStatus::Pass),
        checks,
        package_count,
        current_version,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn doctor_reports_valid_single_package_project() -> Result<(), String> {
        let temp = TempDir::new().map_err(|error| error.to_string())?;
        std::fs::write(
            temp.path().join("package.json"),
            r#"{"name":"root","version":"1.2.3"}"#,
        )
        .map_err(|error| error.to_string())?;
        std::fs::write(temp.path().join("verso.toml"), "").map_err(|error| error.to_string())?;
        make_git_ready(temp.path())?;

        let report = check(&temp.path().join("verso.toml"), true);

        assert!(report.ok);
        assert_eq!(report.package_count, 1);
        assert_eq!(report.current_version.as_deref(), Some("1.2.3"));
        Ok(())
    }

    #[test]
    fn doctor_reports_invalid_config() -> Result<(), String> {
        let temp = TempDir::new().map_err(|error| error.to_string())?;
        std::fs::write(temp.path().join("verso.toml"), "[workspaces]\npatterns = [")
            .map_err(|error| error.to_string())?;

        let report = check(&temp.path().join("verso.toml"), true);

        assert!(!report.ok);
        assert_eq!(report.checks[1].name, "config");
        assert_eq!(report.checks[1].status, DoctorStatus::Fail);
        Ok(())
    }

    #[test]
    fn doctor_uses_default_single_package_config_when_config_is_missing() -> Result<(), String> {
        let temp = TempDir::new().map_err(|error| error.to_string())?;
        std::fs::write(
            temp.path().join("package.json"),
            r#"{"name":"root","version":"1.2.3"}"#,
        )
        .map_err(|error| error.to_string())?;
        make_git_ready(temp.path())?;

        let report = check(&temp.path().join("verso.toml"), true);

        assert!(report.ok);
        assert_eq!(report.package_count, 1);
        assert!(report.checks[1]
            .message
            .contains("built-in single-package defaults"));
        Ok(())
    }

    #[test]
    fn doctor_rejects_explicit_missing_config() -> Result<(), String> {
        let temp = TempDir::new().map_err(|error| error.to_string())?;
        std::fs::write(
            temp.path().join("package.json"),
            r#"{"name":"root","version":"1.2.3"}"#,
        )
        .map_err(|error| error.to_string())?;

        let report = check(&temp.path().join("missing.toml"), false);

        assert!(!report.ok);
        assert_eq!(report.checks[1].name, "config");
        assert_eq!(report.checks[1].status, DoctorStatus::Fail);
        Ok(())
    }

    #[test]
    fn doctor_reports_a_missing_git_upstream() -> Result<(), String> {
        let temp = TempDir::new().map_err(|error| error.to_string())?;
        std::fs::write(
            temp.path().join("package.json"),
            r#"{"name":"root","version":"1.2.3"}"#,
        )
        .map_err(|error| error.to_string())?;
        std::fs::write(temp.path().join("verso.toml"), "").map_err(|error| error.to_string())?;
        crate::git::git(temp.path(), &["init"])?;
        crate::git::git(temp.path(), &["config", "user.email", "test@example.com"])?;
        crate::git::git(temp.path(), &["config", "user.name", "Test User"])?;
        crate::git::git(temp.path(), &["add", "package.json", "verso.toml"])?;
        crate::git::git(temp.path(), &["commit", "-m", "feat: initial"])?;

        let report = check(&temp.path().join("verso.toml"), true);

        assert!(!report.ok);
        assert!(report
            .checks
            .iter()
            .any(|check| check.name == "git upstream" && check.status == DoctorStatus::Fail));
        Ok(())
    }

    #[test]
    fn doctor_text_distinguishes_pass_and_fail_checks() {
        let report = DoctorReport {
            ok: false,
            checks: vec![
                pass("config", "loaded verso.toml"),
                fail("packages", "missing version"),
            ],
            package_count: 0,
            current_version: None,
        };

        let plain = render_text_report(&report, false);

        assert!(plain.contains("PASS  config"));
        assert!(plain.contains("FAIL  packages"));
        assert!(!plain.contains("\u{1b}["));
    }

    #[test]
    fn doctor_run_preserves_failure_result_for_library_callers() -> Result<(), String> {
        let temp = TempDir::new().map_err(|error| error.to_string())?;
        std::fs::write(temp.path().join("verso.toml"), "[workspaces]\npatterns = [")
            .map_err(|error| error.to_string())?;

        let error = run(&temp.path().join("verso.toml"), true, true)
            .expect_err("failed doctor reports should remain errors for library callers");

        assert_eq!(error, "doctor found release readiness problems");
        Ok(())
    }

    fn make_git_ready(root: &Path) -> Result<(), String> {
        crate::git::git(root, &["init"])?;
        crate::git::git(root, &["config", "user.email", "test@example.com"])?;
        crate::git::git(root, &["config", "user.name", "Test User"])?;
        crate::git::git(root, &["add", "."])?;
        crate::git::git(root, &["commit", "-m", "feat: initial"])?;
        let remote = root.join(".git/release-test-remote.git");
        let remote = remote.to_string_lossy();
        crate::git::git(root, &["init", "--bare", &remote])?;
        crate::git::git(root, &["remote", "add", "origin", &remote])?;
        crate::git::git(root, &["push", "-u", "origin", "HEAD"])?;
        Ok(())
    }
}
