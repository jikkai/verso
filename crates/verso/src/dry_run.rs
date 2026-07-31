use crate::diagnostic::render_warning;
use anstyle::{AnsiColor, Style};
use semver::Version;
use serde_json::json;
use std::{
    collections::BTreeMap,
    path::{Component, Path, PathBuf},
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReleasePlan {
    pub current_version: Version,
    pub target_version: Version,
    pub package_files: Vec<PathBuf>,
    pub extra_version_files: Vec<PathBuf>,
    pub changelog_file: Option<PathBuf>,
    pub commit_message: String,
    pub tag_name: String,
    pub hooks: Vec<PlannedHook>,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlannedHook {
    pub name: String,
    pub command: String,
}

pub fn render_dry_run_json(root: &Path, plan: &ReleasePlan) -> String {
    let package_files = normalized_files(&plan.package_files);
    let extra_version_files = normalized_files(&plan.extra_version_files);
    let version_files = normalized_files(
        &package_files
            .iter()
            .chain(extra_version_files.iter())
            .cloned()
            .collect::<Vec<_>>(),
    );
    let mut git_add_files = version_files.clone();
    if let Some(changelog_file) = &plan.changelog_file {
        git_add_files.push(changelog_file.clone());
    }
    git_add_files.sort();
    git_add_files.dedup();

    let git_add_args = git_add_files
        .iter()
        .map(|file| relative_string(root, file))
        .collect::<Vec<_>>();
    let mut git_add_command = vec!["git".to_owned(), "add".to_owned()];
    git_add_command.extend(git_add_args);
    let git_commands = vec![
        git_add_command,
        vec![
            "git".to_owned(),
            "commit".to_owned(),
            "-m".to_owned(),
            plan.commit_message.clone(),
        ],
        vec![
            "git".to_owned(),
            "tag".to_owned(),
            "-a".to_owned(),
            plan.tag_name.clone(),
            "-m".to_owned(),
            plan.tag_name.clone(),
        ],
        vec![
            "git".to_owned(),
            "push".to_owned(),
            "--atomic".to_owned(),
            "<upstream-remote>".to_owned(),
            "HEAD:<upstream-branch>".to_owned(),
            format!("refs/tags/{0}:refs/tags/{0}", plan.tag_name),
        ],
    ];

    let hooks = plan
        .hooks
        .iter()
        .map(|hook| {
            json!({
                "name": hook.name,
                "command": hook.command,
            })
        })
        .collect::<Vec<_>>();

    serde_json::to_string_pretty(&json!({
        "currentVersion": plan.current_version.to_string(),
        "targetVersion": plan.target_version.to_string(),
        "packageFiles": package_files
            .iter()
            .map(|file| relative_string(root, file))
            .collect::<Vec<_>>(),
        "extraVersionFiles": extra_version_files
            .iter()
            .map(|file| relative_string(root, file))
            .collect::<Vec<_>>(),
        "versionFiles": version_files
            .iter()
            .map(|file| relative_string(root, file))
            .collect::<Vec<_>>(),
        "changelogFile": plan.changelog_file.as_ref().map(|file| relative_string(root, file)),
        "commitMessage": plan.commit_message,
        "tagName": plan.tag_name,
        "hooks": hooks,
        "warnings": plan.warnings,
        "gitCommands": git_commands,
    }))
    .expect("dry run plan should serialize")
}

pub fn render_dry_run(root: &Path, plan: &ReleasePlan) -> String {
    let mut output = String::new();
    let package_files = normalized_files(&plan.package_files);
    let extra_version_files = normalized_files(&plan.extra_version_files);
    let version_files = normalized_files(
        &package_files
            .iter()
            .chain(extra_version_files.iter())
            .cloned()
            .collect::<Vec<_>>(),
    );

    output.push_str("Verso dry run\n\n");
    output.push_str(&format!("Current version: {}\n", plan.current_version));
    output.push_str(&format!("Target version: {}\n", plan.target_version));
    output.push_str(&format!("Package count: {}\n", package_files.len()));
    if !extra_version_files.is_empty() {
        output.push_str(&format!(
            "Extra version file count: {}\n",
            extra_version_files.len()
        ));
    }

    if !plan.warnings.is_empty() {
        output.push_str("\nWarnings:\n");
        for warning in &plan.warnings {
            output.push_str(&render_warning(warning, false));
        }
    }

    output.push_str("\nVersion updates:\n");
    output.push_str(&render_tree(root, &version_files));

    if let Some(changelog_file) = &plan.changelog_file {
        output.push_str(&format!(
            "\nChangelog: {}\n",
            relative_string(root, changelog_file)
        ));
    }

    if !plan.hooks.is_empty() {
        output.push_str("\nPlanned hooks:\n");
        for hook in &plan.hooks {
            output.push_str(&format!("{}: {}\n", hook.name, hook.command));
        }
    }

    let mut git_add_files = version_files;
    if let Some(changelog_file) = &plan.changelog_file {
        git_add_files.push(changelog_file.clone());
    }
    git_add_files.sort();
    git_add_files.dedup();
    let git_add_args = git_add_files
        .iter()
        .map(|file| shell_quote(&relative_string(root, file)))
        .collect::<Vec<_>>()
        .join(" ");

    output.push_str("\nPlanned git commands:\n");
    output.push_str(&format!("git add {git_add_args}\n"));
    output.push_str(&format!(
        "git commit -m {}\n",
        shell_quote(&plan.commit_message)
    ));
    output.push_str(&format!(
        "git tag -a {} -m {}\n",
        shell_quote(&plan.tag_name),
        shell_quote(&plan.tag_name)
    ));
    output.push_str(&format!(
        "git push --atomic <upstream-remote> HEAD:<upstream-branch> {}\n",
        shell_quote(&format!("refs/tags/{0}:refs/tags/{0}", plan.tag_name))
    ));

    output
}

pub fn render_dry_run_styled(root: &Path, plan: &ReleasePlan) -> String {
    let mut output = String::new();
    let package_files = normalized_files(&plan.package_files);
    let extra_version_files = normalized_files(&plan.extra_version_files);
    let version_files = normalized_files(
        &package_files
            .iter()
            .chain(extra_version_files.iter())
            .cloned()
            .collect::<Vec<_>>(),
    );

    output.push_str(&format!(
        "{}DRY RUN{} {}\n",
        style(Style::new().bold().fg_color(Some(AnsiColor::Cyan.into()))),
        reset(),
        style_text("Verso release preview", Style::new().bold())
    ));
    output.push_str(&format!(
        "{} {}\n",
        style_text("Version", Style::new().bold()),
        style_text(
            &format!("{} -> {}", plan.current_version, plan.target_version),
            Style::new().fg_color(Some(AnsiColor::Green.into()))
        )
    ));
    output.push_str(&format!(
        "{} {}\n",
        style_text("Packages", Style::new().bold()),
        package_files.len()
    ));
    if !extra_version_files.is_empty() {
        output.push_str(&format!(
            "{} {}\n",
            style_text("Extra version files", Style::new().bold()),
            extra_version_files.len()
        ));
    }

    if !plan.warnings.is_empty() {
        output.push('\n');
        output.push_str(&section_title("Warnings"));
        for warning in &plan.warnings {
            output.push_str(&render_warning(warning, true));
        }
    }

    output.push('\n');
    output.push_str(&section_title("Version updates"));
    output.push_str(&render_tree(root, &version_files));

    if let Some(changelog_file) = &plan.changelog_file {
        output.push('\n');
        output.push_str(&section_title("Changelog"));
        output.push_str(&format!("{}\n", relative_string(root, changelog_file)));
    }

    if !plan.hooks.is_empty() {
        output.push('\n');
        output.push_str(&section_title("Planned hooks"));
        for hook in &plan.hooks {
            output.push_str(&format!(
                "{} {}\n",
                style_text(&hook.name, Style::new().bold()),
                hook.command
            ));
        }
    }

    let mut git_add_files = version_files;
    if let Some(changelog_file) = &plan.changelog_file {
        git_add_files.push(changelog_file.clone());
    }
    git_add_files.sort();
    git_add_files.dedup();
    let git_add_args = git_add_files
        .iter()
        .map(|file| shell_quote(&relative_string(root, file)))
        .collect::<Vec<_>>()
        .join(" ");

    output.push('\n');
    output.push_str(&section_title("Planned git commands"));
    output.push_str(&command_line(&format!("git add {git_add_args}")));
    output.push_str(&command_line(&format!(
        "git commit -m {}",
        shell_quote(&plan.commit_message)
    )));
    output.push_str(&command_line(&format!(
        "git tag -a {} -m {}",
        shell_quote(&plan.tag_name),
        shell_quote(&plan.tag_name)
    )));
    output.push_str(&command_line(&format!(
        "git push --atomic <upstream-remote> HEAD:<upstream-branch> {}",
        shell_quote(&format!("refs/tags/{0}:refs/tags/{0}", plan.tag_name))
    )));

    output
}

pub fn render_tree(root: &Path, files: &[PathBuf]) -> String {
    let mut tree = TreeNode::default();

    for file in normalized_files(files) {
        let relative = relative_path(root, &file);
        let labels = path_labels(&relative);
        if !labels.is_empty() {
            tree.insert(&labels);
        }
    }

    let mut output = ".\n".to_owned();
    render_children(&tree, "", &mut output);
    output
}

fn normalized_files(files: &[PathBuf]) -> Vec<PathBuf> {
    let mut normalized = files.to_vec();
    normalized.sort();
    normalized.dedup();
    normalized
}

#[derive(Default)]
struct TreeNode {
    children: BTreeMap<String, TreeNode>,
}

impl TreeNode {
    fn insert(&mut self, labels: &[String]) {
        let Some((label, rest)) = labels.split_first() else {
            return;
        };

        self.children.entry(label.clone()).or_default().insert(rest);
    }
}

fn render_children(node: &TreeNode, prefix: &str, output: &mut String) {
    let child_count = node.children.len();
    for (index, (label, child)) in node.children.iter().enumerate() {
        let is_last = index + 1 == child_count;
        let connector = if is_last { "└── " } else { "├── " };
        output.push_str(prefix);
        output.push_str(connector);
        output.push_str(label);
        output.push('\n');

        let child_prefix = if is_last {
            format!("{prefix}    ")
        } else {
            format!("{prefix}│   ")
        };
        render_children(child, &child_prefix, output);
    }
}

fn relative_path(root: &Path, path: &Path) -> PathBuf {
    path.strip_prefix(root)
        .map(Path::to_path_buf)
        .unwrap_or_else(|_| path.to_path_buf())
}

fn relative_string(root: &Path, path: &Path) -> String {
    git_path_string(&relative_path(root, path))
}

fn git_path_string(path: &Path) -> String {
    let mut output = String::new();
    for component in path.components() {
        match component {
            Component::Prefix(prefix) => {
                output.push_str(&prefix.as_os_str().to_string_lossy());
            }
            Component::RootDir => {
                if output.is_empty() || !output.ends_with('/') {
                    output.push('/');
                }
            }
            Component::CurDir => {}
            Component::ParentDir => push_path_segment(&mut output, ".."),
            Component::Normal(label) => {
                push_path_segment(&mut output, &label.to_string_lossy());
            }
        }
    }
    output
}

fn push_path_segment(path: &mut String, segment: &str) {
    if !path.is_empty() && !path.ends_with('/') {
        path.push('/');
    }
    path.push_str(segment);
}

fn path_labels(path: &Path) -> Vec<String> {
    path.components()
        .filter_map(|component| match component {
            Component::Normal(label) => Some(label.to_string_lossy().into_owned()),
            Component::ParentDir => Some("..".to_owned()),
            Component::Prefix(prefix) => Some(prefix.as_os_str().to_string_lossy().into_owned()),
            Component::RootDir => Some(std::path::MAIN_SEPARATOR.to_string()),
            Component::CurDir => None,
        })
        .collect()
}

fn shell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}

fn section_title(label: &str) -> String {
    format!(
        "{}{}{}\n",
        style(Style::new().bold().fg_color(Some(AnsiColor::Blue.into()))),
        label,
        reset()
    )
}

fn command_line(command: &str) -> String {
    format!(
        "{}$ {}{}\n",
        style(Style::new().fg_color(Some(AnsiColor::Magenta.into()))),
        command,
        reset()
    )
}

fn style_text(value: &str, style: Style) -> String {
    format!("{}{}{}", self::style(style), value, reset())
}

fn style(style: Style) -> String {
    style.render().to_string()
}

fn reset() -> String {
    "\u{1b}[0m".to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;
    use semver::Version;
    use std::path::{Path, PathBuf};
    use tempfile::TempDir;

    #[test]
    fn renders_package_tree_with_root_package_and_nested_packages() -> Result<(), String> {
        let root = TempDir::new().map_err(|error| error.to_string())?;
        let files = vec![
            root.path().join("package.json"),
            root.path().join("packages/a/package.json"),
            root.path().join("presets/packages/preset/package.json"),
        ];

        let tree = render_tree(root.path(), &files);

        assert_eq!(
            tree,
            concat!(
                ".\n",
                "├── package.json\n",
                "├── packages\n",
                "│   └── a\n",
                "│       └── package.json\n",
                "└── presets\n",
                "    └── packages\n",
                "        └── preset\n",
                "            └── package.json\n",
            )
        );
        Ok(())
    }

    #[test]
    fn dry_run_mentions_git_push_and_warning() -> Result<(), String> {
        let root = TempDir::new().map_err(|error| error.to_string())?;
        let plan = test_plan(
            root.path(),
            vec![root.path().join("package.json")],
            vec!["working tree has generated files".to_owned()],
        )?;

        let output = render_dry_run(root.path(), &plan);

        assert!(output.contains("Verso dry run"));
        assert!(output.contains("Current version: 1.2.3"));
        assert!(output.contains("Target version: 1.3.0"));
        assert!(output.contains("Package count: 1"));
        assert!(output.contains("Warnings"));
        assert!(output.contains("warning: working tree has generated files"));
        assert!(output.contains("git push --atomic"));
        Ok(())
    }

    #[test]
    fn styled_dry_run_distinguishes_sections_with_ansi() -> Result<(), String> {
        let root = TempDir::new().map_err(|error| error.to_string())?;
        let plan = test_plan(
            root.path(),
            vec![root.path().join("package.json")],
            vec!["working tree has generated files".to_owned()],
        )?;

        let output = render_dry_run_styled(root.path(), &plan);

        assert!(output.contains("\u{1b}["));
        assert!(output.contains("DRY RUN"));
        assert!(output.contains("1.2.3 -> 1.3.0"));
        assert!(output.contains("Warnings"));
        assert!(output.contains("warning"));
        assert!(output.contains("working tree has generated files"));
        assert!(output.contains("$ git push --atomic"));
        Ok(())
    }

    #[test]
    fn plain_dry_run_does_not_emit_ansi() -> Result<(), String> {
        let root = TempDir::new().map_err(|error| error.to_string())?;
        let plan = test_plan(
            root.path(),
            vec![root.path().join("package.json")],
            Vec::new(),
        )?;

        let output = render_dry_run(root.path(), &plan);

        assert!(!output.contains("\u{1b}["));
        Ok(())
    }

    #[test]
    fn dry_run_renders_changelog_path_relative_to_root() -> Result<(), String> {
        let root = TempDir::new().map_err(|error| error.to_string())?;
        let plan = test_plan(
            root.path(),
            vec![root.path().join("packages/a/package.json")],
            Vec::new(),
        )?;

        let output = render_dry_run(root.path(), &plan);

        assert!(output.contains("Changelog: docs/CHANGELOG.md"));
        assert!(!output.contains(&root.path().join("docs/CHANGELOG.md").display().to_string()));
        Ok(())
    }

    #[test]
    fn dry_run_quotes_git_add_paths_with_spaces_and_single_quotes() -> Result<(), String> {
        let root = TempDir::new().map_err(|error| error.to_string())?;
        let plan = test_plan(
            root.path(),
            vec![root.path().join("packages/space dir/bob's/package.json")],
            Vec::new(),
        )?;

        let output = render_dry_run(root.path(), &plan);

        assert!(output
            .contains("git add 'docs/CHANGELOG.md' 'packages/space dir/bob'\\''s/package.json'\n"));
        Ok(())
    }

    #[test]
    fn dry_run_uses_deduped_package_files_for_count_tree_and_git_add() -> Result<(), String> {
        let root = TempDir::new().map_err(|error| error.to_string())?;
        let package = root.path().join("packages/a/package.json");
        let plan = test_plan(root.path(), vec![package.clone(), package], Vec::new())?;

        let output = render_dry_run(root.path(), &plan);

        assert!(output.contains("Package count: 1\n"));
        assert_eq!(output.matches("packages/a/package.json").count(), 1);
        assert!(output.contains("git add 'docs/CHANGELOG.md' 'packages/a/package.json'\n"));
        Ok(())
    }

    #[test]
    fn dry_run_git_paths_use_forward_slashes() {
        let path = PathBuf::from_iter(["packages", "a", "package.yaml"]);

        assert_eq!(git_path_string(&path), "packages/a/package.yaml");
    }

    #[test]
    fn dry_run_includes_extra_version_files_in_tree_and_git_add() -> Result<(), String> {
        let root = TempDir::new().map_err(|error| error.to_string())?;
        let mut plan = test_plan(
            root.path(),
            vec![root.path().join("packages/verso/package.json")],
            Vec::new(),
        )?;
        plan.extra_version_files = vec![root.path().join("crates/verso/Cargo.toml")];

        let output = render_dry_run(root.path(), &plan);

        assert!(output.contains("Package count: 1\nExtra version file count: 1\n"));
        assert!(output.contains("crates\n│   └── verso\n│       └── Cargo.toml"));
        assert!(output.contains(
            "git add 'crates/verso/Cargo.toml' 'docs/CHANGELOG.md' 'packages/verso/package.json'\n"
        ));
        Ok(())
    }

    #[test]
    fn dry_run_lists_planned_hooks() -> Result<(), String> {
        let root = TempDir::new().map_err(|error| error.to_string())?;
        let mut plan = test_plan(
            root.path(),
            vec![root.path().join("packages/verso/package.json")],
            Vec::new(),
        )?;
        plan.hooks = vec![
            PlannedHook {
                name: "before_version".to_owned(),
                command: "pnpm test".to_owned(),
            },
            PlannedHook {
                name: "after_version".to_owned(),
                command: "pnpm build".to_owned(),
            },
        ];

        let output = render_dry_run(root.path(), &plan);

        assert!(output.contains("Planned hooks:\n"));
        assert!(output.contains("before_version: pnpm test\n"));
        assert!(output.contains("after_version: pnpm build\n"));
        Ok(())
    }

    #[test]
    fn dry_run_json_renders_structured_release_plan() -> Result<(), String> {
        let root = TempDir::new().map_err(|error| error.to_string())?;
        let mut plan = test_plan(
            root.path(),
            vec![root.path().join("packages/verso/package.json")],
            vec!["working tree is dirty".to_owned()],
        )?;
        plan.extra_version_files = vec![root.path().join("Cargo.lock")];
        plan.hooks = vec![PlannedHook {
            name: "before_version".to_owned(),
            command: "pnpm test".to_owned(),
        }];

        let json: serde_json::Value =
            serde_json::from_str(&render_dry_run_json(root.path(), &plan))
                .map_err(|error| error.to_string())?;

        assert_eq!(json["currentVersion"], "1.2.3");
        assert_eq!(json["targetVersion"], "1.3.0");
        assert_eq!(json["packageFiles"][0], "packages/verso/package.json");
        assert_eq!(json["extraVersionFiles"][0], "Cargo.lock");
        assert_eq!(json["versionFiles"][0], "Cargo.lock");
        assert_eq!(json["changelogFile"], "docs/CHANGELOG.md");
        assert_eq!(json["hooks"][0]["name"], "before_version");
        assert_eq!(json["gitCommands"][0][0], "git");
        assert_eq!(json["gitCommands"][0][1], "add");
        assert_eq!(json["warnings"][0], "working tree is dirty");
        Ok(())
    }

    fn test_plan(
        root: &Path,
        package_files: Vec<PathBuf>,
        warnings: Vec<String>,
    ) -> Result<ReleasePlan, String> {
        Ok(ReleasePlan {
            current_version: Version::parse("1.2.3").map_err(|error| error.to_string())?,
            target_version: Version::parse("1.3.0").map_err(|error| error.to_string())?,
            package_files,
            extra_version_files: Vec::new(),
            changelog_file: Some(root.join("docs/CHANGELOG.md")),
            commit_message: "chore(release): release v1.3.0".to_owned(),
            tag_name: "v1.3.0".to_owned(),
            hooks: Vec::new(),
            warnings,
        })
    }
}
