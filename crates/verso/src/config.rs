use serde::Deserialize;
use std::{
    fs,
    io::ErrorKind,
    path::{Component, Path},
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Config {
    pub version: VersionConfig,
    pub workspaces: WorkspaceConfig,
    pub changelog: ChangelogConfig,
    pub git: GitConfig,
    pub hooks: HooksConfig,
    pub github_release: GithubReleaseConfig,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VersionConfig {
    pub root_package: String,
    pub require_consistent_versions: bool,
    pub cargo_manifest_paths: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkspaceConfig {
    pub patterns: Vec<String>,
    pub include_root: bool,
    pub ignore: Vec<String>,
    pub use_gitignore: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChangelogConfig {
    pub infile: String,
    pub preset: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GitConfig {
    pub require_clean_worktree: bool,
    pub commit_message: String,
    pub tag_name: String,
    pub push: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct HooksConfig {
    pub before_version: Option<String>,
    pub after_version: Option<String>,
    pub before_commit: Option<String>,
    pub after_commit: Option<String>,
    pub before_tag: Option<String>,
    pub after_tag: Option<String>,
    pub before_push: Option<String>,
    pub after_push: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GithubReleaseConfig {
    pub enabled: bool,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawConfig {
    version: Option<RawVersionConfig>,
    workspaces: Option<RawWorkspaceConfig>,
    changelog: Option<RawChangelogConfig>,
    git: Option<RawGitConfig>,
    hooks: Option<RawHooksConfig>,
    github_release: Option<RawGithubReleaseConfig>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawVersionConfig {
    root_package: Option<String>,
    require_consistent_versions: Option<bool>,
    cargo_manifest_paths: Option<Vec<String>>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawWorkspaceConfig {
    patterns: Option<Vec<String>>,
    include_root: Option<bool>,
    ignore: Option<Vec<String>>,
    use_gitignore: Option<bool>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawChangelogConfig {
    infile: Option<String>,
    preset: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawGitConfig {
    require_clean_worktree: Option<bool>,
    commit_message: Option<String>,
    tag_name: Option<String>,
    push: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawHooksConfig {
    before_version: Option<String>,
    after_version: Option<String>,
    before_commit: Option<String>,
    after_commit: Option<String>,
    before_tag: Option<String>,
    after_tag: Option<String>,
    before_push: Option<String>,
    after_push: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawGithubReleaseConfig {
    enabled: Option<bool>,
}

pub fn load_config(path: &Path) -> Result<Config, String> {
    let contents = fs::read_to_string(path).map_err(|error| {
        if error.kind() == ErrorKind::NotFound {
            format!(
                "failed to read {}: {error}\nCreate a verso.toml with `verso init`, or omit [workspaces] for a single-package release.\nPass a different config path with --config <PATH>.",
                path.display()
            )
        } else {
            format!("failed to read {}: {error}", path.display())
        }
    })?;
    parse_config_with_label(&contents, &path.display().to_string())
}

pub fn default_config() -> Config {
    Config {
        version: VersionConfig {
            root_package: "package.json".to_owned(),
            require_consistent_versions: true,
            cargo_manifest_paths: Vec::new(),
        },
        workspaces: WorkspaceConfig {
            patterns: Vec::new(),
            include_root: true,
            ignore: Vec::new(),
            use_gitignore: true,
        },
        changelog: ChangelogConfig {
            infile: "CHANGELOG.md".to_owned(),
            preset: "angular".to_owned(),
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

pub fn parse_config(contents: &str) -> Result<Config, String> {
    parse_config_with_label(contents, "verso.toml")
}

fn parse_config_with_label(contents: &str, label: &str) -> Result<Config, String> {
    let raw: RawConfig =
        toml::from_str(contents).map_err(|error| format!("failed to parse {label}: {error}"))?;

    let version = raw.version.unwrap_or(RawVersionConfig {
        root_package: None,
        require_consistent_versions: None,
        cargo_manifest_paths: None,
    });
    let changelog = raw.changelog.unwrap_or(RawChangelogConfig {
        infile: None,
        preset: None,
    });
    let git = raw.git.unwrap_or(RawGitConfig {
        require_clean_worktree: None,
        commit_message: None,
        tag_name: None,
        push: None,
    });
    let hooks = raw.hooks.unwrap_or(RawHooksConfig {
        before_version: None,
        after_version: None,
        before_commit: None,
        after_commit: None,
        before_tag: None,
        after_tag: None,
        before_push: None,
        after_push: None,
    });
    let github_release = raw
        .github_release
        .unwrap_or(RawGithubReleaseConfig { enabled: None });

    let config = Config {
        version: VersionConfig {
            root_package: version
                .root_package
                .unwrap_or_else(|| "package.json".to_string()),
            require_consistent_versions: version.require_consistent_versions.unwrap_or(true),
            cargo_manifest_paths: version.cargo_manifest_paths.unwrap_or_default(),
        },
        workspaces: WorkspaceConfig {
            patterns: raw
                .workspaces
                .as_ref()
                .and_then(|workspaces| workspaces.patterns.clone())
                .unwrap_or_default(),
            include_root: raw
                .workspaces
                .as_ref()
                .and_then(|workspaces| workspaces.include_root)
                .unwrap_or(true),
            ignore: raw
                .workspaces
                .as_ref()
                .and_then(|workspaces| workspaces.ignore.clone())
                .unwrap_or_default(),
            use_gitignore: raw
                .workspaces
                .as_ref()
                .and_then(|workspaces| workspaces.use_gitignore)
                .unwrap_or(true),
        },
        changelog: ChangelogConfig {
            infile: changelog
                .infile
                .unwrap_or_else(|| "CHANGELOG.md".to_string()),
            preset: changelog.preset.unwrap_or_else(|| "angular".to_string()),
        },
        git: GitConfig {
            require_clean_worktree: git.require_clean_worktree.unwrap_or(true),
            commit_message: git
                .commit_message
                .unwrap_or_else(|| "chore(release): release v${version}".to_string()),
            tag_name: git.tag_name.unwrap_or_else(|| "v${version}".to_string()),
            push: git.push.map_or_else(
                || "atomic".to_string(),
                |push| {
                    if push == "follow-tags" {
                        "atomic".to_string()
                    } else {
                        push
                    }
                },
            ),
        },
        hooks: HooksConfig {
            before_version: normalize_hook(hooks.before_version),
            after_version: normalize_hook(hooks.after_version),
            before_commit: normalize_hook(hooks.before_commit),
            after_commit: normalize_hook(hooks.after_commit),
            before_tag: normalize_hook(hooks.before_tag),
            after_tag: normalize_hook(hooks.after_tag),
            before_push: normalize_hook(hooks.before_push),
            after_push: normalize_hook(hooks.after_push),
        },
        github_release: GithubReleaseConfig {
            enabled: github_release.enabled.unwrap_or(false),
        },
    };

    validate_config(&config)?;
    Ok(config)
}

fn validate_config(config: &Config) -> Result<(), String> {
    if config.workspaces.patterns.is_empty() && !config.workspaces.include_root {
        return Err(
            "workspaces.patterns must contain at least one pattern when workspaces.include_root is false"
                .to_string(),
        );
    }
    if config.version.root_package.trim().is_empty() {
        return Err("version.root_package must not be empty".to_string());
    }
    if config.changelog.infile.trim().is_empty() {
        return Err("changelog.infile must not be empty".to_string());
    }
    if config.git.commit_message.trim().is_empty() {
        return Err("git.commit_message must not be empty".to_string());
    }
    if config.git.tag_name.trim().is_empty() {
        return Err("git.tag_name must not be empty".to_string());
    }
    if !config.git.tag_name.contains("${version}") {
        return Err("git.tag_name must contain ${version}".to_string());
    }
    let example_tag_name = render_template(&config.git.tag_name, "1.2.3");
    if !is_valid_git_tag_name(&example_tag_name) {
        return Err(format!(
            "git.tag_name must render a valid Git tag; example rendered tag {example_tag_name:?} is invalid"
        ));
    }
    if config
        .workspaces
        .patterns
        .iter()
        .any(|pattern| pattern.trim().is_empty())
    {
        return Err("workspaces.patterns must not contain empty patterns".to_string());
    }
    for pattern in &config.workspaces.patterns {
        validate_workspace_pattern(pattern)?;
    }
    if config
        .workspaces
        .ignore
        .iter()
        .any(|pattern| pattern.trim().is_empty())
    {
        return Err("workspaces.ignore must not contain empty patterns".to_string());
    }
    for pattern in &config.workspaces.ignore {
        validate_workspace_pattern(pattern)?;
    }
    validate_config_relative_path("version.root_package", &config.version.root_package)?;
    if config
        .version
        .cargo_manifest_paths
        .iter()
        .any(|path| path.trim().is_empty())
    {
        return Err("version.cargo_manifest_paths must not contain empty paths".to_string());
    }
    for path in &config.version.cargo_manifest_paths {
        validate_config_relative_path("version.cargo_manifest_paths", path)?;
    }
    validate_config_relative_path("changelog.infile", &config.changelog.infile)?;
    if config.changelog.preset != "angular" {
        return Err("only changelog preset \"angular\" is supported".to_string());
    }
    if config.git.push != "atomic" {
        return Err("only git.push = \"atomic\" is supported".to_string());
    }
    validate_hooks(&config.hooks)?;
    if config.github_release.enabled {
        return Err("github_release.enabled = true is not supported in this version".to_string());
    }
    Ok(())
}

fn normalize_hook(command: Option<String>) -> Option<String> {
    command.and_then(|command| {
        let trimmed = command.trim();
        (!trimmed.is_empty()).then(|| trimmed.to_owned())
    })
}

fn validate_hooks(hooks: &HooksConfig) -> Result<(), String> {
    for (name, command) in [
        ("before_version", &hooks.before_version),
        ("after_version", &hooks.after_version),
        ("before_commit", &hooks.before_commit),
        ("after_commit", &hooks.after_commit),
        ("before_tag", &hooks.before_tag),
        ("after_tag", &hooks.after_tag),
        ("before_push", &hooks.before_push),
        ("after_push", &hooks.after_push),
    ] {
        if matches!(command, Some(command) if command.trim().is_empty()) {
            return Err(format!("hooks.{name} must not be empty"));
        }
    }
    Ok(())
}

fn validate_config_relative_path(key: &str, value: &str) -> Result<(), String> {
    if uses_platform_specific_path_syntax(value) {
        return Err(format!("{key} must use forward slashes in config paths"));
    }

    if escapes_config_directory(value) {
        return Err(format!(
            "{key} must be relative to the config directory and must not contain parent directory segments"
        ));
    }
    Ok(())
}

fn validate_workspace_pattern(pattern: &str) -> Result<(), String> {
    let pattern = pattern.strip_prefix('!').unwrap_or(pattern);
    if pattern.trim().is_empty() {
        return Err("workspaces.patterns must not contain empty patterns".to_string());
    }
    validate_config_relative_path("workspaces.patterns", pattern)
}

fn uses_platform_specific_path_syntax(value: &str) -> bool {
    value.contains('\\') || has_windows_drive_prefix(value)
}

fn has_windows_drive_prefix(value: &str) -> bool {
    let bytes = value.as_bytes();
    matches!(bytes, [letter, b':', ..] if letter.is_ascii_alphabetic())
}

fn escapes_config_directory(value: &str) -> bool {
    value.starts_with('/')
        || value.starts_with('\\')
        || Path::new(value).components().any(|component| {
            matches!(
                component,
                Component::ParentDir | Component::RootDir | Component::Prefix(_)
            )
        })
}

fn is_valid_git_tag_name(tag: &str) -> bool {
    if tag.is_empty()
        || tag == "@"
        || tag.starts_with('-')
        || tag.starts_with('/')
        || tag.ends_with('/')
        || tag.ends_with('.')
        || tag.contains("//")
        || tag.contains("..")
        || tag.contains("@{")
    {
        return false;
    }

    if tag.bytes().any(|byte| {
        byte <= b' '
            || byte == 0x7f
            || matches!(byte, b'~' | b'^' | b':' | b'?' | b'*' | b'[' | b'\\')
    }) {
        return false;
    }

    tag.split('/').all(|component| {
        !component.is_empty() && !component.starts_with('.') && !component.ends_with(".lock")
    })
}

pub fn render_template(template: &str, version: &str) -> String {
    template.replace("${version}", version)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn minimal_config_uses_defaults() -> Result<(), String> {
        let config = parse_config(
            r#"
            [workspaces]
            patterns = ["packages/*"]
            "#,
        )?;

        assert_eq!(config.version.root_package, "package.json");
        assert!(config.version.require_consistent_versions);
        assert!(config.version.cargo_manifest_paths.is_empty());
        assert_eq!(config.workspaces.patterns, vec!["packages/*"]);
        assert!(config.workspaces.include_root);
        assert!(config.workspaces.ignore.is_empty());
        assert!(config.workspaces.use_gitignore);
        assert_eq!(config.changelog.infile, "CHANGELOG.md");
        assert_eq!(config.changelog.preset, "angular");
        assert!(config.git.require_clean_worktree);
        assert_eq!(
            config.git.commit_message,
            "chore(release): release v${version}"
        );
        assert_eq!(config.git.tag_name, "v${version}");
        assert_eq!(config.git.push, "atomic");
        assert_eq!(config.hooks, HooksConfig::default());
        assert!(!config.github_release.enabled);

        Ok(())
    }

    #[test]
    fn config_without_workspaces_uses_single_package_defaults() -> Result<(), String> {
        let config = parse_config("")?;

        assert_eq!(config.workspaces.patterns, Vec::<String>::new());
        assert!(config.workspaces.include_root);
        assert!(config.workspaces.ignore.is_empty());
        assert!(config.workspaces.use_gitignore);
        assert_eq!(config.version.root_package, "package.json");
        Ok(())
    }

    #[test]
    fn legacy_follow_tags_push_mode_uses_atomic_push() -> Result<(), String> {
        let config = parse_config(
            r#"
            [git]
            push = "follow-tags"
            "#,
        )?;

        assert_eq!(config.git.push, "atomic");
        Ok(())
    }

    #[test]
    fn rejects_empty_workspace_discovery_when_root_is_excluded() {
        let error = parse_config(
            r#"
            [workspaces]
            include_root = false
            "#,
        )
        .expect_err("config with no root and no workspace patterns should be rejected");

        assert!(error.contains("workspaces.patterns"));
        assert!(error.contains("include_root"));
    }

    #[test]
    fn parses_hooks_and_trims_commands() -> Result<(), String> {
        let config = parse_config(
            r#"
            [workspaces]
            patterns = ["packages/*"]

            [hooks]
            before_version = " pnpm test "
            after_push = "node scripts/notify-release.mts"
            "#,
        )?;

        assert_eq!(config.hooks.before_version.as_deref(), Some("pnpm test"));
        assert_eq!(
            config.hooks.after_push.as_deref(),
            Some("node scripts/notify-release.mts")
        );
        assert_eq!(config.hooks.before_commit, None);
        Ok(())
    }

    #[test]
    fn rejects_enabled_github_release() {
        let error = parse_config(
            r#"
            [workspaces]
            patterns = ["packages/*"]

            [github_release]
            enabled = true
            "#,
        )
        .expect_err("github releases should be unsupported");

        assert!(error.contains("github_release.enabled = true"));
    }

    #[test]
    fn missing_config_error_includes_setup_hint() {
        let temp = tempfile::tempdir().expect("temp dir should be created");
        let config_path = temp.path().join("missing/verso.toml");
        let error = load_config(&config_path).expect_err("missing config should fail");

        assert!(error.contains("failed to read"));
        assert!(error.contains("verso init"));
        assert!(error.contains("single-package"));
        assert!(error.contains("--config <PATH>"));
    }

    #[test]
    fn load_config_parse_error_mentions_requested_path() {
        let temp = tempfile::tempdir().expect("temp dir should be created");
        let config_path = temp.path().join("custom.toml");
        fs::write(&config_path, "[workspaces]\npatterns = [\n")
            .expect("invalid config should be written");

        let error = load_config(&config_path).expect_err("invalid config should fail");

        assert!(error.contains("failed to parse"));
        assert!(error.contains("custom.toml"));
        assert!(!error.contains("failed to parse verso.toml"));
    }

    #[test]
    fn rejects_empty_workspace_patterns_array() {
        let error = parse_config(
            r#"
            [workspaces]
            patterns = []
            include_root = false
            "#,
        )
        .expect_err("empty workspace patterns should be rejected");

        assert!(error.contains("workspaces.patterns"));
    }

    #[test]
    fn rejects_blank_workspace_pattern_entries() {
        for pattern in ["", "   "] {
            let contents = format!(
                r#"
                [workspaces]
                patterns = [{pattern:?}]
                "#
            );
            let error =
                parse_config(&contents).expect_err("blank workspace pattern should be rejected");

            assert!(error.contains("workspaces.patterns"));
        }
    }

    #[test]
    fn parses_cargo_manifest_version_paths() -> Result<(), String> {
        let config = parse_config(
            r#"
            [version]
            cargo_manifest_paths = ["crates/verso/Cargo.toml"]

            [workspaces]
            patterns = ["packages/*"]
            "#,
        )?;

        assert_eq!(
            config.version.cargo_manifest_paths,
            vec!["crates/verso/Cargo.toml"]
        );
        Ok(())
    }

    #[test]
    fn rejects_blank_cargo_manifest_paths() {
        let error = parse_config(
            r#"
            [version]
            cargo_manifest_paths = [" "]

            [workspaces]
            patterns = ["packages/*"]
            "#,
        )
        .expect_err("blank manifest path should be rejected");

        assert!(error.contains("version.cargo_manifest_paths"));
    }

    #[test]
    fn rejects_paths_that_escape_the_config_directory() {
        let cases = [
            (
                "workspaces.patterns",
                r#"
                [workspaces]
                patterns = ["../packages/*"]
                "#,
            ),
            (
                "workspaces.patterns",
                r#"
                [workspaces]
                patterns = ["/tmp/packages/*"]
                "#,
            ),
            (
                "version.root_package",
                r#"
                [version]
                root_package = "../package.json"

                [workspaces]
                patterns = ["packages/*"]
                "#,
            ),
            (
                "version.cargo_manifest_paths",
                r#"
                [version]
                cargo_manifest_paths = ["../Cargo.toml"]

                [workspaces]
                patterns = ["packages/*"]
                "#,
            ),
            (
                "changelog.infile",
                r#"
                [workspaces]
                patterns = ["packages/*"]

                [changelog]
                infile = "../CHANGELOG.md"
                "#,
            ),
        ];

        for (key, contents) in cases {
            let error = parse_config(contents).expect_err("escaping paths should be rejected");

            assert!(error.contains(key), "{key} error should mention the key");
            assert!(
                error.contains("config directory"),
                "{key} error should explain paths stay under the config directory"
            );
        }
    }

    #[test]
    fn rejects_platform_specific_path_syntax() {
        let cases = [
            (
                "workspaces.patterns",
                r#"
                [workspaces]
                patterns = ["packages\\*"]
                "#,
            ),
            (
                "version.root_package",
                r#"
                [version]
                root_package = "C:\\repo\\package.json"

                [workspaces]
                patterns = ["packages/*"]
                "#,
            ),
            (
                "changelog.infile",
                r#"
                [workspaces]
                patterns = ["packages/*"]

                [changelog]
                infile = "docs\\CHANGELOG.md"
                "#,
            ),
        ];

        for (key, contents) in cases {
            let error =
                parse_config(contents).expect_err("platform-specific paths should be rejected");

            assert!(error.contains(key), "{key} error should mention the key");
            assert!(
                error.contains("forward slashes"),
                "{key} error should explain that config paths use forward slashes"
            );
        }
    }

    #[test]
    fn rejects_blank_string_config_values() {
        let cases = [
            (
                "version.root_package",
                r#"
                [version]
                root_package = " "
                "#,
            ),
            (
                "changelog.infile",
                r#"
                [changelog]
                infile = ""
                "#,
            ),
            (
                "git.commit_message",
                r#"
                [git]
                commit_message = " "
                "#,
            ),
            (
                "git.tag_name",
                r#"
                [git]
                tag_name = ""
                "#,
            ),
        ];

        for (key, snippet) in cases {
            let contents = format!(
                r#"
                [workspaces]
                patterns = ["packages/*"]

                {snippet}
                "#
            );

            let error =
                parse_config(&contents).expect_err(&format!("{key} should reject blank values"));

            assert!(error.contains(key), "{key} error should mention the key");
            assert!(
                error.contains("must not be empty"),
                "{key} error should explain the value cannot be empty"
            );
        }
    }

    #[test]
    fn rejects_tag_name_without_version_placeholder() {
        let error = parse_config(
            r#"
            [workspaces]
            patterns = ["packages/*"]

            [git]
            tag_name = "release"
            "#,
        )
        .expect_err("tag templates without a version should be rejected");

        assert!(error.contains("git.tag_name"));
        assert!(error.contains("${version}"));
    }

    #[test]
    fn rejects_tag_name_templates_that_render_invalid_git_refs() {
        for tag_name in [
            "bad tag ${version}",
            "release..${version}",
            "release/${version}.lock",
            "release@{${version}",
        ] {
            let contents = format!(
                r#"
                [workspaces]
                patterns = ["packages/*"]

                [git]
                tag_name = {tag_name:?}
                "#
            );
            let error =
                parse_config(&contents).expect_err("invalid git tag templates should be rejected");

            assert!(error.contains("git.tag_name"));
            assert!(error.contains("valid Git tag"));
            assert!(
                error.contains("1.2.3"),
                "error should include the rendered sample tag"
            );
        }
    }

    #[test]
    fn rejects_invalid_changelog_preset() {
        let error = parse_config(
            r#"
            [workspaces]
            patterns = ["packages/*"]

            [changelog]
            preset = "conventional"
            "#,
        )
        .expect_err("unsupported changelog preset should be rejected");

        assert!(error.contains("changelog preset"));
    }

    #[test]
    fn rejects_invalid_git_push() {
        let error = parse_config(
            r#"
            [workspaces]
            patterns = ["packages/*"]

            [git]
            push = "never"
            "#,
        )
        .expect_err("unsupported git push mode should be rejected");

        assert!(error.contains("git.push"));
    }

    #[test]
    fn rejects_unknown_root_key() {
        let error = parse_config(
            r#"
            unknown = true

            [workspaces]
            patterns = ["packages/*"]
            "#,
        )
        .expect_err("unknown root keys should be rejected");

        assert!(error.contains("unknown field"));
    }

    #[test]
    fn rejects_unknown_nested_key() {
        let error = parse_config(
            r#"
            [workspaces]
            patterns = ["packages/*"]
            typo = true
            "#,
        )
        .expect_err("unknown nested keys should be rejected");

        assert!(error.contains("unknown field"));
    }

    #[test]
    fn renders_version_template() {
        assert_eq!(
            render_template("chore(release): release v${version}", "0.2.0"),
            "chore(release): release v0.2.0"
        );
    }
}
