use crate::cli::InitArgs;
use std::path::Path;

pub fn run(config_path: &Path, args: &InitArgs) -> Result<(), String> {
    if config_path.exists() && !args.force {
        return Err(format!(
            "{} already exists\n\nhelp: rerun with --force to overwrite it",
            config_path.display()
        ));
    }

    let config_dir = config_path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    let mode = if args.workspace {
        InitMode::Workspace
    } else if args.single {
        InitMode::Single
    } else {
        detect_mode(config_dir)
    };

    if let Some(parent) = config_path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    {
        std::fs::create_dir_all(parent)
            .map_err(|error| format!("failed to create {}: {error}", parent.display()))?;
    }
    std::fs::write(config_path, render_config(mode))
        .map_err(|error| format!("failed to write {}: {error}", config_path.display()))?;

    println!("Created {}", config_path.display());
    Ok(())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum InitMode {
    Single,
    Workspace,
}

fn detect_mode(root: &Path) -> InitMode {
    let packages_dir = root.join("packages");
    let has_workspace_package = packages_dir
        .read_dir()
        .ok()
        .into_iter()
        .flatten()
        .filter_map(Result::ok)
        .any(|entry| entry.path().join("package.json").exists());

    if has_workspace_package {
        InitMode::Workspace
    } else {
        InitMode::Single
    }
}

fn render_config(mode: InitMode) -> &'static str {
    match mode {
        InitMode::Single => {
            r#"[version]
root_package = "package.json"

[changelog]
enabled = false
infile = "CHANGELOG.md"
"#
        }
        InitMode::Workspace => {
            r#"[version]
root_package = "package.json"

[workspaces]
patterns = ["packages/*"]
include_root = true

[changelog]
enabled = false
infile = "CHANGELOG.md"
"#
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cli::InitArgs;
    use tempfile::TempDir;

    #[test]
    fn init_writes_single_package_config_by_default() -> Result<(), String> {
        let temp = TempDir::new().map_err(|error| error.to_string())?;
        let config = temp.path().join("verso.toml");

        run(
            &config,
            &InitArgs {
                force: false,
                single: false,
                workspace: false,
            },
        )?;

        let contents = std::fs::read_to_string(config).map_err(|error| error.to_string())?;
        assert!(contents.contains("root_package = \"package.json\""));
        assert!(contents.contains("enabled = false"));
        assert!(!contents.contains("[workspaces]"));
        Ok(())
    }

    #[test]
    fn init_auto_detects_packages_workspace() -> Result<(), String> {
        let temp = TempDir::new().map_err(|error| error.to_string())?;
        std::fs::create_dir_all(temp.path().join("packages/a"))
            .map_err(|error| error.to_string())?;
        std::fs::write(
            temp.path().join("packages/a/package.json"),
            r#"{"name":"a","version":"1.0.0"}"#,
        )
        .map_err(|error| error.to_string())?;
        let config = temp.path().join("verso.toml");

        run(
            &config,
            &InitArgs {
                force: false,
                single: false,
                workspace: false,
            },
        )?;

        let contents = std::fs::read_to_string(config).map_err(|error| error.to_string())?;
        assert!(contents.contains("[workspaces]"));
        assert!(contents.contains("patterns = [\"packages/*\"]"));
        Ok(())
    }

    #[test]
    fn init_refuses_to_overwrite_without_force() -> Result<(), String> {
        let temp = TempDir::new().map_err(|error| error.to_string())?;
        let config = temp.path().join("verso.toml");
        std::fs::write(&config, "existing").map_err(|error| error.to_string())?;

        let error = run(
            &config,
            &InitArgs {
                force: false,
                single: true,
                workspace: false,
            },
        )
        .expect_err("existing config should not be overwritten");

        assert!(error.contains("--force"));
        assert_eq!(
            std::fs::read_to_string(config).map_err(|error| error.to_string())?,
            "existing"
        );
        Ok(())
    }
}
