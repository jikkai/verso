use clap::{Args, Parser, Subcommand};
use std::path::PathBuf;

pub const DEFAULT_CONFIG_PATH: &str = "verso.toml";

#[derive(Debug, Parser, PartialEq, Eq)]
#[command(
    name = "verso",
    version,
    disable_version_flag = true,
    about = "Release configured workspace packages with changelog, git tag, and push",
    long_about = "Verso is a focused release CLI. It reads verso.toml, updates package versions, generates an angular-style changelog, commits, tags, and atomically pushes the current upstream branch and release tag. Use --dry-run to preview without changing files."
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Option<Commands>,

    #[arg(
        long,
        help = "Preview the release without writing files or running mutating git commands"
    )]
    pub dry_run: bool,

    #[arg(long, help = "Print dry-run output as JSON")]
    pub json: bool,

    #[arg(
        long = "version",
        value_name = "SEMVER",
        help = "Use a target version without interactive selection"
    )]
    pub target_version: Option<String>,

    #[arg(
        long,
        value_name = "PATH",
        help = "Path to the Verso config file [default: verso.toml]"
    )]
    pub config: Option<String>,

    #[arg(long, help = "Skip release confirmation prompts")]
    pub yes: bool,

    #[arg(
        short = 'V',
        long = "tool-version",
        help = "Print the Verso CLI version and exit"
    )]
    pub tool_version: bool,
}

#[derive(Debug, Subcommand, PartialEq, Eq)]
pub enum Commands {
    #[command(about = "Create a starter verso.toml")]
    Init(InitArgs),
    #[command(about = "Validate Verso config and project release readiness")]
    Doctor(DoctorArgs),
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
    #[arg(long, help = "Print doctor output as JSON")]
    pub json: bool,
}

impl Cli {
    pub fn parse_args() -> Self {
        Self::parse()
    }

    pub fn config_path(&self) -> &str {
        self.config.as_deref().unwrap_or(DEFAULT_CONFIG_PATH)
    }

    pub fn config_path_buf(&self) -> PathBuf {
        PathBuf::from(self.config_path())
    }

    pub fn config_was_explicit(&self) -> bool {
        self.config.is_some()
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
}
