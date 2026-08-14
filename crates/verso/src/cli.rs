use clap::{Args, Parser, Subcommand, ValueEnum};
use std::{borrow::Cow, path::PathBuf};

pub const DEFAULT_CONFIG_PATH: &str = "verso.toml";

#[derive(Debug, Parser, PartialEq, Eq)]
#[command(
    name = "verso",
    version,
    disable_version_flag = true,
    about = "Release configured workspace packages with changelog, git tag, and push",
    long_about = "Verso is a focused release CLI. It reads one release-group config, updates package versions, generates the configured changelog preset, commits, tags, and atomically pushes the current upstream branch and release tag. Use --dry-run to preview without changing files."
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Option<Commands>,

    #[arg(
        long,
        global = true,
        help = "Preview the release without writing files or running mutating git commands"
    )]
    pub dry_run: bool,

    #[arg(long, global = true, help = "Print plan or status output as JSON")]
    pub json: bool,

    #[arg(
        long = "version",
        global = true,
        value_name = "SEMVER",
        help = "Use a target version without interactive selection"
    )]
    pub target_version: Option<String>,

    #[arg(
        long,
        global = true,
        value_name = "PATH",
        conflicts_with = "group",
        help = "Path to the Verso config file [default: verso.toml]"
    )]
    pub config: Option<String>,

    #[arg(
        long,
        global = true,
        value_name = "NAME",
        value_parser = parse_group_name,
        conflicts_with = "config",
        help = "Use the release group config verso.NAME.toml"
    )]
    pub group: Option<String>,

    #[arg(long, global = true, help = "Skip release confirmation prompts")]
    pub yes: bool,

    #[arg(
        short = 'V',
        long = "tool-version",
        global = true,
        help = "Print the Verso CLI version and exit"
    )]
    pub tool_version: bool,
}

#[derive(Debug, Subcommand, PartialEq, Eq)]
pub enum Commands {
    #[command(about = "Update release files without committing, tagging, or pushing")]
    Bump(BumpArgs),
    #[command(about = "Create a starter verso.toml")]
    Init(InitArgs),
    #[command(about = "Validate Verso config and project release readiness")]
    Doctor(DoctorArgs),
    #[command(about = "Show the active release transaction")]
    Status,
    #[command(about = "Continue the active release transaction")]
    Resume(ResumeArgs),
    #[command(about = "Abort and roll back the active release transaction")]
    Abort,
}

#[derive(Debug, Args, PartialEq, Eq)]
pub struct BumpArgs {
    #[arg(
        value_enum,
        value_name = "LEVEL",
        required_unless_present = "target_version",
        conflicts_with = "target_version"
    )]
    pub level: Option<BumpLevel>,
}

#[derive(Debug, Args, PartialEq, Eq)]
pub struct ResumeArgs {
    #[arg(
        long,
        conflicts_with = "skip_hook",
        help = "Re-run an interrupted hook whose outcome is unknown"
    )]
    pub retry_hook: bool,

    #[arg(
        long,
        conflicts_with = "retry_hook",
        help = "Mark an interrupted hook complete without re-running it"
    )]
    pub skip_hook: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum BumpLevel {
    Patch,
    Minor,
    Major,
}

#[derive(Debug, Args, PartialEq, Eq)]
pub struct InitArgs {
    #[arg(long, help = "Overwrite an existing config file")]
    pub force: bool,

    #[arg(
        long,
        conflicts_with = "workspace",
        help = "Generate single-package config"
    )]
    pub single: bool,

    #[arg(long, conflicts_with = "single", help = "Generate workspace config")]
    pub workspace: bool,
}

#[derive(Debug, Args, PartialEq, Eq)]
pub struct DoctorArgs {
    #[arg(from_global)]
    pub json: bool,
}

impl Cli {
    pub fn parse_args() -> Self {
        Self::parse()
    }

    pub fn config_path(&self) -> Cow<'_, str> {
        if let Some(path) = &self.config {
            Cow::Borrowed(path)
        } else if let Some(group) = &self.group {
            Cow::Owned(format!("verso.{group}.toml"))
        } else {
            Cow::Borrowed(DEFAULT_CONFIG_PATH)
        }
    }

    pub fn config_path_buf(&self) -> PathBuf {
        PathBuf::from(self.config_path().as_ref())
    }

    pub fn config_was_explicit(&self) -> bool {
        self.config.is_some() || self.group.is_some()
    }

    pub fn validate_command_options(&self) -> Result<(), String> {
        let release_or_bump =
            self.command.is_none() || matches!(self.command, Some(Commands::Bump(_)));
        if !release_or_bump && self.dry_run {
            return Err("--dry-run can only be used with release or bump".to_string());
        }
        if !release_or_bump && self.target_version.is_some() {
            return Err("--version can only be used with release or bump".to_string());
        }
        if !release_or_bump && self.yes {
            return Err("--yes can only be used with release or bump".to_string());
        }
        if self.json
            && !release_or_bump
            && !matches!(self.command, Some(Commands::Doctor(_) | Commands::Status))
        {
            return Err(
                "--json can only be used with release, bump, doctor, or status".to_string(),
            );
        }
        Ok(())
    }
}

fn parse_group_name(value: &str) -> Result<String, String> {
    let mut bytes = value.bytes();
    if bytes
        .next()
        .is_some_and(|byte| byte.is_ascii_alphanumeric())
        && bytes.all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
    {
        Ok(value.to_owned())
    } else {
        Err("group names may contain only ASCII letters, digits, '-' and '_'".to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::CommandFactory;

    #[test]
    fn help_mentions_dry_run_and_version() {
        let mut command = Cli::command();
        let mut buffer = Vec::new();
        command
            .write_long_help(&mut buffer)
            .expect("long help should render");
        let help = String::from_utf8(buffer).expect("help should be valid UTF-8");
        assert!(help.contains("--dry-run"));
        assert!(help.contains("--json"));
        assert!(help.contains("--version <SEMVER>"));
        assert!(help.contains("verso.toml"));
        assert!(help.contains("Skip release confirmation prompts"));
        assert!(help.contains("-V, --tool-version"));
    }

    #[test]
    fn parses_release_options() {
        let cli = Cli::try_parse_from([
            "verso",
            "--dry-run",
            "--version",
            "1.2.3",
            "--config",
            "custom.toml",
            "--yes",
        ])
        .expect("release options should parse");

        assert_eq!(
            cli,
            Cli {
                dry_run: true,
                json: false,
                command: None,
                target_version: Some("1.2.3".to_string()),
                config: Some("custom.toml".to_string()),
                group: None,
                yes: true,
                tool_version: false,
            }
        );
    }

    #[test]
    fn parses_tool_version_option() {
        let cli = Cli::try_parse_from(["verso", "--tool-version"])
            .expect("tool version option should parse");

        assert_eq!(
            cli,
            Cli {
                dry_run: false,
                json: false,
                command: None,
                target_version: None,
                config: None,
                group: None,
                yes: false,
                tool_version: true,
            }
        );
    }

    #[test]
    fn parses_json_dry_run_option() {
        let cli = Cli::try_parse_from(["verso", "--dry-run", "--json"])
            .expect("json dry run option should parse");

        assert!(cli.dry_run);
        assert!(cli.json);
        assert_eq!(cli.config_path(), "verso.toml");
        assert!(!cli.config_was_explicit());
    }

    #[test]
    fn parses_init_subcommand() {
        let cli = Cli::try_parse_from(["verso", "init", "--workspace", "--force"])
            .expect("init command should parse");

        assert_eq!(
            cli.command,
            Some(Commands::Init(InitArgs {
                force: true,
                single: false,
                workspace: true,
            }))
        );
    }

    #[test]
    fn parses_doctor_subcommand() {
        let cli = Cli::try_parse_from(["verso", "doctor", "--json"])
            .expect("doctor command should parse");

        assert_eq!(
            cli.command,
            Some(Commands::Doctor(DoctorArgs { json: true }))
        );
    }

    #[test]
    fn parses_bump_level_with_global_options_after_subcommand() {
        let cli = Cli::try_parse_from([
            "verso",
            "bump",
            "minor",
            "--dry-run",
            "--json",
            "--group",
            "core",
            "--yes",
        ])
        .expect("bump level and global options should parse");

        assert_eq!(
            cli.command,
            Some(Commands::Bump(BumpArgs {
                level: Some(BumpLevel::Minor),
            }))
        );
        assert!(cli.dry_run);
        assert!(cli.json);
        assert!(cli.yes);
        assert_eq!(cli.config_path(), "verso.core.toml");
        assert!(cli.config_was_explicit());
    }

    #[test]
    fn parses_bump_with_exact_version() {
        let cli = Cli::try_parse_from(["verso", "bump", "--version", "2.0.0"])
            .expect("bump should accept an exact version instead of a level");

        assert_eq!(cli.command, Some(Commands::Bump(BumpArgs { level: None })));
        assert_eq!(cli.target_version.as_deref(), Some("2.0.0"));
    }

    #[test]
    fn bump_requires_exactly_one_target() {
        assert!(Cli::try_parse_from(["verso", "bump"]).is_err());
        assert!(Cli::try_parse_from(["verso", "bump", "minor", "--version", "2.0.0"]).is_err());
    }

    #[test]
    fn group_conflicts_with_config_and_rejects_unsafe_names() {
        assert!(
            Cli::try_parse_from(["verso", "--group", "core", "--config", "custom.toml"]).is_err()
        );

        for name in ["../core", "core/ui", "core.toml", "core ui", "-core", "_"] {
            assert!(
                Cli::try_parse_from(["verso", "--group", name]).is_err(),
                "unsafe group {name:?} should be rejected"
            );
        }
    }

    #[test]
    fn parses_transaction_commands() {
        assert_eq!(
            Cli::try_parse_from(["verso", "status"])
                .expect("status should parse")
                .command,
            Some(Commands::Status)
        );
        assert_eq!(
            Cli::try_parse_from(["verso", "resume"])
                .expect("resume should parse")
                .command,
            Some(Commands::Resume(ResumeArgs {
                retry_hook: false,
                skip_hook: false,
            }))
        );
        assert_eq!(
            Cli::try_parse_from(["verso", "abort"])
                .expect("abort should parse")
                .command,
            Some(Commands::Abort)
        );
    }

    #[test]
    fn resume_requires_one_explicit_interrupted_hook_action() {
        assert_eq!(
            Cli::try_parse_from(["verso", "resume", "--retry-hook"])
                .expect("retry-hook should parse")
                .command,
            Some(Commands::Resume(ResumeArgs {
                retry_hook: true,
                skip_hook: false,
            }))
        );
        assert!(Cli::try_parse_from(["verso", "resume", "--retry-hook", "--skip-hook"]).is_err());
    }

    #[test]
    fn rejects_ignored_global_options_on_other_commands() {
        for args in [
            ["verso", "abort", "--dry-run"],
            ["verso", "resume", "--yes"],
            ["verso", "init", "--version=1.2.3"],
            ["verso", "abort", "--json"],
        ] {
            let cli = Cli::try_parse_from(args).expect("global option should parse");
            assert!(cli.validate_command_options().is_err());
        }
    }
}
