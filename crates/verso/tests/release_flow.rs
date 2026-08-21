use assert_cmd::Command;
use predicates::prelude::*;
use std::{fs, path::Path, process::Command as ProcessCommand};
use tempfile::TempDir;

#[test]
fn dry_run_does_not_modify_worktree() -> Result<(), Box<dyn std::error::Error>> {
    let repo = TempDir::new()?;
    init_repo(repo.path())?;

    let root_package = "{\n  \"name\": \"root\",\n  \"version\": \"0.1.0\"\n}\n";
    write_file(&repo.path().join("package.json"), root_package)?;
    write_file(
        &repo.path().join("packages/a/package.json"),
        "{\n  \"name\": \"a\",\n  \"version\": \"0.1.0\"\n}\n",
    )?;
    write_file(
        &repo.path().join("verso.toml"),
        r#"
[workspaces]
patterns = ["packages/*"]
include_root = true
"#,
    )?;
    write_file(&repo.path().join("CHANGELOG.md"), "# Changelog\n")?;

    git(repo.path(), &["add", "."])?;
    git(repo.path(), &["commit", "-m", "chore: initial release"])?;
    git(repo.path(), &["tag", "v0.1.0"])?;
    write_file(&repo.path().join("feature.md"), "feature\n")?;
    git(repo.path(), &["add", "feature.md"])?;
    git(repo.path(), &["commit", "-m", "feat: add feature (#1)"])?;

    Command::cargo_bin("verso")?
        .current_dir(repo.path())
        .args(["--dry-run", "--version", "0.2.0", "--yes"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Verso dry run"))
        .stdout(predicate::str::contains("Version updates"))
        .stdout(predicate::str::contains("git mktag"))
        .stdout(predicate::str::contains(
            "git update-ref 'refs/tags/v0.2.0'",
        ))
        .stdout(predicate::str::contains("git push --atomic"));

    assert_eq!(
        fs::read_to_string(repo.path().join("package.json"))?,
        root_package
    );

    Ok(())
}

#[test]
fn json_dry_run_contains_exact_file_changes_without_writing(
) -> Result<(), Box<dyn std::error::Error>> {
    let repo = TempDir::new()?;
    write_release_fixture(repo.path())?;
    let package_path = repo.path().join("package.json");
    let before = fs::read_to_string(&package_path)?;
    let before_head = git_stdout(repo.path(), &["rev-parse", "HEAD"])?;

    let output = Command::cargo_bin("verso")?
        .current_dir(repo.path())
        .args(["--dry-run", "--json", "--version", "0.2.0", "--yes"])
        .output()?;
    let plan: serde_json::Value = serde_json::from_slice(&output.stdout)?;
    let package_change = plan["fileChanges"]
        .as_array()
        .and_then(|changes| {
            changes
                .iter()
                .find(|change| change["path"] == "package.json")
        })
        .ok_or("package.json change missing from plan")?;

    assert!(output.status.success());
    assert_eq!(plan["operation"], "release");
    assert_eq!(package_change["before"], before);
    assert_eq!(
        package_change["after"],
        "{\n  \"name\": \"root\",\n  \"version\": \"0.2.0\"\n}\n"
    );
    assert_eq!(fs::read_to_string(package_path)?, before);
    assert_eq!(
        git_stdout(repo.path(), &["rev-parse", "HEAD"])?,
        before_head
    );
    assert_eq!(git_stdout(repo.path(), &["status", "--porcelain"])?, "");

    Ok(())
}

#[test]
fn duplicate_release_targets_are_rejected_before_writing() -> Result<(), Box<dyn std::error::Error>>
{
    let repo = TempDir::new()?;
    write_release_fixture(repo.path())?;
    write_file(
        &repo.path().join("verso.toml"),
        r#"
[changelog]
enabled = true
infile = "package.json"
"#,
    )?;
    git(repo.path(), &["add", "verso.toml"])?;
    git(
        repo.path(),
        &[
            "commit",
            "-m",
            "test: configure conflicting release targets",
        ],
    )?;
    let package_path = repo.path().join("package.json");
    let before = fs::read_to_string(&package_path)?;

    Command::cargo_bin("verso")?
        .current_dir(repo.path())
        .args(["--dry-run", "--version", "0.2.0", "--yes"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("release plan targets"));

    assert_eq!(fs::read_to_string(package_path)?, before);
    assert!(!repo.path().join(".git/verso/active.json").exists());
    Ok(())
}

#[test]
fn release_preserves_a_normalized_multiline_commit_message(
) -> Result<(), Box<dyn std::error::Error>> {
    let repo = TempDir::new()?;
    write_release_fixture(repo.path())?;
    write_file(
        &repo.path().join("verso.toml"),
        r#"
[workspaces]
patterns = ["packages/*"]
include_root = true

[git]
commit_message = """
release ${version}
# retained
 """

[changelog]
enabled = true
"#,
    )?;
    git(repo.path(), &["add", "verso.toml"])?;
    git(
        repo.path(),
        &["commit", "-m", "test: configure multiline release message"],
    )?;
    let remote = repo.path().join(".git/release-test-remote.git");
    let remote_string = remote.to_str().ok_or("non-UTF-8 path")?;
    git(repo.path(), &["init", "--bare", remote_string])?;
    let branch = git_stdout(repo.path(), &["symbolic-ref", "--short", "HEAD"])?;
    let branch_ref = format!("HEAD:refs/heads/{}", branch.trim());
    git(repo.path(), &["push", "origin", &branch_ref])?;

    Command::cargo_bin("verso")?
        .current_dir(repo.path())
        .args(["--version", "0.2.0", "--yes"])
        .assert()
        .success();

    assert_eq!(
        git_stdout(repo.path(), &["log", "-1", "--format=%B"])?.trim_end(),
        "release 0.2.0\n# retained"
    );
    assert!(!repo.path().join(".git/verso/active.json").exists());
    Ok(())
}

#[test]
fn bump_minor_after_subcommand_only_updates_selected_group(
) -> Result<(), Box<dyn std::error::Error>> {
    let repo = TempDir::new()?;
    init_repo(repo.path())?;
    write_file(
        &repo.path().join("packages/core/package.json"),
        "{\n  \"name\": \"core\",\n  \"version\": \"1.2.3\"\n}\n",
    )?;
    write_file(
        &repo.path().join("packages/ui/package.json"),
        "{\n  \"name\": \"ui\",\n  \"version\": \"4.5.6\"\n}\n",
    )?;
    write_file(
        &repo.path().join("verso.core.toml"),
        r#"
[version]
root_package = "packages/core/package.json"

[changelog]
enabled = true
"#,
    )?;
    write_file(&repo.path().join("CHANGELOG.md"), "# Changelog\n")?;
    git(repo.path(), &["add", "."])?;
    git(repo.path(), &["commit", "-m", "chore: add release groups"])?;
    git(repo.path(), &["tag", "v1.2.3"])?;
    let before_head = git_stdout(repo.path(), &["rev-parse", "HEAD"])?;

    Command::cargo_bin("verso")?
        .current_dir(repo.path())
        .args(["bump", "minor", "--group", "core", "--yes"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Updated release files to 1.3.0."));

    assert!(
        fs::read_to_string(repo.path().join("packages/core/package.json"))?
            .contains("\"version\": \"1.3.0\"")
    );
    assert!(
        fs::read_to_string(repo.path().join("packages/ui/package.json"))?
            .contains("\"version\": \"4.5.6\"")
    );
    assert_eq!(
        fs::read_to_string(repo.path().join("CHANGELOG.md"))?,
        "# Changelog\n"
    );
    assert_eq!(
        git_stdout(repo.path(), &["rev-parse", "HEAD"])?,
        before_head
    );
    assert_eq!(git_stdout(repo.path(), &["tag", "--list"])?, "v1.2.3\n");
    assert_eq!(
        git_stdout(repo.path(), &["status", "--porcelain"])?,
        " M packages/core/package.json\n"
    );

    Ok(())
}

#[test]
fn named_release_groups_get_distinct_default_tags() -> Result<(), Box<dyn std::error::Error>> {
    let repo = TempDir::new()?;
    let remote = TempDir::new()?;
    init_repo(repo.path())?;
    git(remote.path(), &["init", "--bare"])?;
    for group in ["core", "ui", "default"] {
        write_file(
            &repo.path().join(format!("packages/{group}/package.json")),
            &format!("{{\n  \"name\": \"{group}\",\n  \"version\": \"1.0.0\"\n}}\n"),
        )?;
        write_file(
            &repo.path().join(format!("verso.{group}.toml")),
            &format!("[version]\nroot_package = \"packages/{group}/package.json\"\n"),
        )?;
    }
    git(repo.path(), &["add", "."])?;
    git(repo.path(), &["commit", "-m", "chore: add release groups"])?;
    let remote_path = remote.path().to_str().ok_or("non-UTF-8 path")?;
    git(repo.path(), &["remote", "add", "origin", remote_path])?;
    git(repo.path(), &["push", "-u", "origin", "HEAD"])?;

    for group in ["core", "ui", "default"] {
        Command::cargo_bin("verso")?
            .current_dir(repo.path())
            .args(["--group", group, "--version", "1.1.0", "--yes"])
            .assert()
            .success();
    }

    assert_eq!(
        git_stdout(repo.path(), &["tag", "--list", "*-v1.1.0"])?
            .lines()
            .collect::<Vec<_>>(),
        ["core-v1.1.0", "default-v1.1.0", "ui-v1.1.0"]
    );
    Ok(())
}

#[test]
fn recovery_rejects_a_different_release_group() -> Result<(), Box<dyn std::error::Error>> {
    let repo = TempDir::new()?;
    write_release_fixture(repo.path())?;
    write_file(
        &repo.path().join("verso.ui.toml"),
        r#"
[workspaces]
patterns = ["packages/*"]
include_root = true
"#,
    )?;
    git(repo.path(), &["add", "verso.ui.toml"])?;
    git(repo.path(), &["commit", "-m", "test: add ui release group"])?;

    Command::cargo_bin("verso")?
        .current_dir(repo.path())
        .args(["--version", "0.2.0"])
        .write_stdin("y\nn\n")
        .assert()
        .failure();

    Command::cargo_bin("verso")?
        .current_dir(repo.path())
        .args(["--group", "ui", "abort"])
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "active transaction belongs to group default",
        ));

    Command::cargo_bin("verso")?
        .current_dir(repo.path())
        .arg("abort")
        .assert()
        .success();
    Ok(())
}

#[test]
fn single_package_release_uses_defaults_when_config_is_missing(
) -> Result<(), Box<dyn std::error::Error>> {
    let repo = TempDir::new()?;
    init_repo(repo.path())?;
    write_file(
        &repo.path().join("package.json"),
        "{\n  \"name\": \"root\",\n  \"version\": \"0.1.0\"\n}\n",
    )?;
    git(repo.path(), &["add", "."])?;
    git(repo.path(), &["commit", "-m", "feat: initial"])?;

    Command::cargo_bin("verso")?
        .current_dir(repo.path())
        .args(["--dry-run", "--version", "0.2.0", "--yes"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Package count: 1"))
        .stdout(predicate::str::contains("package.json"));

    Ok(())
}

#[test]
fn single_yaml_package_release_uses_defaults_when_config_is_missing(
) -> Result<(), Box<dyn std::error::Error>> {
    let repo = TempDir::new()?;
    init_repo(repo.path())?;
    write_file(
        &repo.path().join("package.yaml"),
        "name: root\nversion: 0.1.0\n",
    )?;
    git(repo.path(), &["add", "."])?;
    git(repo.path(), &["commit", "-m", "feat: initial"])?;

    Command::cargo_bin("verso")?
        .current_dir(repo.path())
        .args(["--dry-run", "--version", "0.2.0", "--yes"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Package count: 1"))
        .stdout(predicate::str::contains("package.yaml"));

    Ok(())
}

#[test]
fn dry_run_infers_pnpm_workspace_and_lists_manifest_paths() -> Result<(), Box<dyn std::error::Error>>
{
    let repo = TempDir::new()?;
    init_repo(repo.path())?;
    write_file(
        &repo.path().join("package.json"),
        "{\n  \"name\": \"root\",\n  \"version\": \"0.1.0\"\n}\n",
    )?;
    write_file(
        &repo.path().join("packages/a/package.yaml"),
        "name: a\nversion: 0.1.0\n",
    )?;
    write_file(
        &repo.path().join("pnpm-workspace.yaml"),
        "packages:\n  - packages/*\n",
    )?;
    git(repo.path(), &["add", "."])?;
    git(repo.path(), &["commit", "-m", "feat: initial"])?;

    Command::cargo_bin("verso")?
        .current_dir(repo.path())
        .args(["--dry-run", "--version", "0.2.0", "--yes"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Package count: 2"))
        .stdout(predicate::str::contains("packages/a/package.yaml"))
        .stdout(predicate::str::contains(
            "git add -- 'package.json' 'packages/a/package.yaml'",
        ));

    Ok(())
}

#[test]
fn missing_package_version_renders_error_and_help() -> Result<(), Box<dyn std::error::Error>> {
    let repo = TempDir::new()?;
    write_pnpm_workspace_with_missing_version_fixture(repo.path())?;

    Command::cargo_bin("verso")?
        .current_dir(repo.path())
        .args(["--dry-run", "--version", "0.2.0", "--yes"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("error: package docs"))
        .stderr(predicate::str::contains("help:"))
        .stderr(predicate::str::contains("\u{1b}[").not());

    Ok(())
}

#[test]
fn failing_doctor_json_keeps_stderr_empty() -> Result<(), Box<dyn std::error::Error>> {
    let repo = TempDir::new()?;
    write_pnpm_workspace_with_missing_version_fixture(repo.path())?;

    let output = Command::cargo_bin("verso")?
        .current_dir(repo.path())
        .args(["doctor", "--json"])
        .output()?;
    let report: serde_json::Value = serde_json::from_slice(&output.stdout)?;

    assert!(!output.status.success());
    assert_eq!(report["ok"], false);
    assert!(output.stderr.is_empty());
    Ok(())
}

#[test]
fn explicit_missing_config_still_fails() -> Result<(), Box<dyn std::error::Error>> {
    let repo = TempDir::new()?;
    init_repo(repo.path())?;
    write_file(
        &repo.path().join("package.json"),
        "{\n  \"name\": \"root\",\n  \"version\": \"0.1.0\"\n}\n",
    )?;
    git(repo.path(), &["add", "."])?;
    git(repo.path(), &["commit", "-m", "feat: initial"])?;

    Command::cargo_bin("verso")?
        .current_dir(repo.path())
        .args([
            "--dry-run",
            "--version",
            "0.2.0",
            "--yes",
            "--config",
            "missing.toml",
        ])
        .assert()
        .failure()
        .stderr(predicate::str::contains("failed to read"))
        .stderr(predicate::str::contains("help:"));

    Ok(())
}

#[test]
fn json_output_requires_dry_run() -> Result<(), Box<dyn std::error::Error>> {
    let repo = TempDir::new()?;
    write_release_fixture(repo.path())?;

    Command::cargo_bin("verso")?
        .current_dir(repo.path())
        .args(["--json", "--version", "0.2.0", "--yes"])
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "--json can only be used with --dry-run",
        ));

    Ok(())
}

#[test]
fn release_updates_versions_changelog_commit_and_tag_before_push(
) -> Result<(), Box<dyn std::error::Error>> {
    let repo = TempDir::new()?;
    write_release_fixture(repo.path())?;

    Command::cargo_bin("verso")?
        .current_dir(repo.path())
        .args(["--version", "0.2.0", "--yes"])
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "note: Local release commit and tag were created",
        ))
        .stderr(predicate::str::contains("help:"))
        .stderr(predicate::str::contains("git push --atomic"));

    assert!(
        fs::read_to_string(repo.path().join("package.json"))?.contains("\"version\": \"0.2.0\"")
    );
    assert!(
        fs::read_to_string(repo.path().join("packages/a/package.json"))?
            .contains("\"version\": \"0.2.0\"")
    );
    assert!(fs::read_to_string(repo.path().join("CHANGELOG.md"))?.contains("0.2.0"));
    assert_eq!(
        git_stdout(repo.path(), &["tag", "--list", "v0.2.0"])?.trim(),
        "v0.2.0"
    );
    assert_eq!(
        git_stdout(repo.path(), &["cat-file", "-t", "v0.2.0"])?.trim(),
        "tag"
    );
    assert!(git_stdout(repo.path(), &["log", "-1", "--pretty=%s"])?
        .contains("chore(release): release v0.2.0"));
    assert_eq!(
        git_stdout(repo.path(), &["status", "--porcelain"])?,
        String::new()
    );

    Ok(())
}

#[test]
fn release_updates_each_cargo_package_in_its_nearest_lockfile(
) -> Result<(), Box<dyn std::error::Error>> {
    let repo = TempDir::new()?;
    write_release_fixture(repo.path())?;
    write_file(
        &repo.path().join("verso.toml"),
        r#"
[version]
cargo_manifest_paths = ["crates/a/Cargo.toml", "crates/b/Cargo.toml"]

[workspaces]
patterns = ["packages/*"]
include_root = true
"#,
    )?;
    for name in ["a", "b"] {
        write_file(
            &repo.path().join(format!("crates/{name}/Cargo.toml")),
            &format!("[package]\nname = \"{name}\"\nversion = \"0.1.0\"\n"),
        )?;
        write_file(
            &repo.path().join(format!("crates/{name}/Cargo.lock")),
            &format!("version = 4\n\n[[package]]\nname = \"{name}\"\nversion = \"0.1.0\"\n"),
        )?;
    }
    git(repo.path(), &["add", "."])?;
    git(
        repo.path(),
        &["commit", "-m", "test: add independent Cargo lockfiles"],
    )?;

    Command::cargo_bin("verso")?
        .current_dir(repo.path())
        .args(["--version", "0.2.0", "--yes"])
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "Local release commit and tag were created",
        ));

    for name in ["a", "b"] {
        assert!(
            fs::read_to_string(repo.path().join(format!("crates/{name}/Cargo.lock")))?
                .contains("version = \"0.2.0\"")
        );
    }

    Ok(())
}

#[test]
fn dry_run_omits_changelog_by_default() -> Result<(), Box<dyn std::error::Error>> {
    let repo = TempDir::new()?;
    write_release_fixture(repo.path())?;
    write_file(
        &repo.path().join("verso.toml"),
        r#"
[workspaces]
patterns = ["packages/*"]
include_root = true
"#,
    )?;
    git(repo.path(), &["add", "verso.toml"])?;
    git(
        repo.path(),
        &["commit", "-m", "test: use changelog default"],
    )?;

    Command::cargo_bin("verso")?
        .current_dir(repo.path())
        .args(["--dry-run", "--json", "--version", "0.2.0", "--yes"])
        .assert()
        .success()
        .stdout(predicate::str::contains("\"changelogFile\": null"))
        .stdout(predicate::str::contains("CHANGELOG.md").not());

    Ok(())
}

#[test]
fn release_leaves_changelog_unchanged_when_disabled() -> Result<(), Box<dyn std::error::Error>> {
    let repo = TempDir::new()?;
    write_release_fixture(repo.path())?;
    write_file(
        &repo.path().join("verso.toml"),
        r#"
[workspaces]
patterns = ["packages/*"]
include_root = true

[changelog]
enabled = false
"#,
    )?;
    git(repo.path(), &["add", "verso.toml"])?;
    git(repo.path(), &["commit", "-m", "test: disable changelog"])?;

    Command::cargo_bin("verso")?
        .current_dir(repo.path())
        .args(["--version", "0.2.0", "--yes"])
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "Local release commit and tag were created",
        ));

    assert_eq!(
        fs::read_to_string(repo.path().join("CHANGELOG.md"))?,
        "# Changelog\n"
    );
    assert!(
        !git_stdout(repo.path(), &["show", "--format=", "--name-only", "HEAD"])?
            .contains("CHANGELOG.md")
    );

    Ok(())
}

#[test]
fn keep_a_changelog_release_inserts_unreleased_and_change_categories(
) -> Result<(), Box<dyn std::error::Error>> {
    let repo = TempDir::new()?;
    write_release_fixture(repo.path())?;
    write_file(
        &repo.path().join("verso.toml"),
        r#"
[workspaces]
patterns = ["packages/*"]
include_root = true

[changelog]
enabled = true
preset = "keep-a-changelog"
"#,
    )?;
    git(repo.path(), &["add", "verso.toml"])?;
    git(
        repo.path(),
        &["commit", "-m", "chore: use Keep a Changelog"],
    )?;
    write_file(&repo.path().join("fix.md"), "fix\n")?;
    git(repo.path(), &["add", "fix.md"])?;
    git(
        repo.path(),
        &["commit", "-m", "fix: restore interrupted release"],
    )?;
    write_file(&repo.path().join("performance.md"), "faster\n")?;
    git(repo.path(), &["add", "performance.md"])?;
    git(
        repo.path(),
        &["commit", "-m", "perf: reduce planning overhead"],
    )?;

    Command::cargo_bin("verso")?
        .current_dir(repo.path())
        .args(["--version", "0.2.0", "--yes"])
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "Local release commit and tag were created",
        ));

    let changelog = fs::read_to_string(repo.path().join("CHANGELOG.md"))?;
    let unreleased = changelog
        .find("## [Unreleased]")
        .ok_or("Unreleased heading missing")?;
    let release = changelog
        .find("## [0.2.0] - ")
        .ok_or("release heading missing")?;
    assert!(unreleased < release);
    assert!(changelog.contains("### Added\n\n- add feature"));
    assert!(changelog.contains("### Fixed\n\n- restore interrupted release"));
    assert!(changelog.contains("### Changed"));
    assert!(changelog.contains("reduce planning overhead"));

    Ok(())
}

#[test]
fn release_runs_hooks_in_order_before_push_failure() -> Result<(), Box<dyn std::error::Error>> {
    let repo = TempDir::new()?;
    write_release_fixture(repo.path())?;
    write_file(
        &repo.path().join("verso.toml"),
        r#"
[workspaces]
patterns = ["packages/*"]
include_root = true

[hooks]
before_version = "git config --file hook.log --add hooks.step before_version"
after_version = "git config --file hook.log --add hooks.step after_version"
before_commit = "git config --file hook.log --add hooks.step before_commit"
after_commit = "git config --file hook.log --add hooks.step after_commit"
before_tag = "git config --file hook.log --add hooks.step before_tag"
after_tag = "git config --file hook.log --add hooks.step after_tag"
before_push = "git config --file hook.log --add hooks.step before_push"
"#,
    )?;
    git(repo.path(), &["add", "verso.toml"])?;
    git(repo.path(), &["commit", "-m", "test: add hooks"])?;

    Command::cargo_bin("verso")?
        .current_dir(repo.path())
        .args(["--version", "0.2.0", "--yes"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("git push --atomic"));

    assert_eq!(
        git_stdout(
            repo.path(),
            &["config", "--file", "hook.log", "--get-all", "hooks.step"]
        )?,
        concat!(
            "before_version\n",
            "after_version\n",
            "before_commit\n",
            "after_commit\n",
            "before_tag\n",
            "after_tag\n",
            "before_push\n",
        )
    );

    Ok(())
}

#[test]
fn a_tagged_hook_cannot_publish_and_then_trigger_local_rollback(
) -> Result<(), Box<dyn std::error::Error>> {
    let repo = TempDir::new()?;
    write_release_fixture(repo.path())?;
    let remote = repo.path().join(".git/release-test-remote.git");
    let remote_string = remote.to_str().ok_or("non-UTF-8 path")?;
    git(repo.path(), &["init", "--bare", remote_string])?;
    let branch = git_stdout(repo.path(), &["symbolic-ref", "--short", "HEAD"])?;
    let branch_ref = format!("HEAD:refs/heads/{}", branch.trim());
    git(repo.path(), &["push", "origin", &branch_ref])?;
    let hook = format!(
        "git push --atomic origin HEAD:refs/heads/{} refs/tags/v0.2.0:refs/tags/v0.2.0 && exit 1",
        branch.trim()
    );
    write_file(
        &repo.path().join("verso.toml"),
        &format!(
            r#"
[workspaces]
patterns = ["packages/*"]
include_root = true

[changelog]
enabled = true

[hooks]
after_tag = {hook:?}
"#
        ),
    )?;
    git(repo.path(), &["add", "verso.toml"])?;
    git(
        repo.path(),
        &["commit", "-m", "test: publish inside tagged hook"],
    )?;
    git(repo.path(), &["push"])?;

    Command::cargo_bin("verso")?
        .current_dir(repo.path())
        .args(["--version", "0.2.0", "--yes"])
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "cannot abort because the exact release refs are already present",
        ));

    assert_eq!(
        git_stdout(repo.path(), &["tag", "--list", "v0.2.0"])?.trim(),
        "v0.2.0"
    );
    let status_output = Command::cargo_bin("verso")?
        .current_dir(repo.path())
        .args(["status", "--json"])
        .output()?;
    let status: serde_json::Value = serde_json::from_slice(&status_output.stdout)?;
    assert_eq!(status["stage"], "pushed");
    assert_eq!(status["canAbort"], false);

    Command::cargo_bin("verso")?
        .current_dir(repo.path())
        .args(["resume", "--skip-hook"])
        .assert()
        .success();
    Ok(())
}

#[test]
fn a_committed_hook_cannot_publish_and_then_trigger_local_rollback(
) -> Result<(), Box<dyn std::error::Error>> {
    let repo = TempDir::new()?;
    write_release_fixture(repo.path())?;
    let remote = repo.path().join(".git/release-test-remote.git");
    let remote_string = remote.to_str().ok_or("non-UTF-8 path")?;
    git(repo.path(), &["init", "--bare", remote_string])?;
    let branch = git_stdout(repo.path(), &["symbolic-ref", "--short", "HEAD"])?;
    let branch_ref = format!("HEAD:refs/heads/{}", branch.trim());
    git(repo.path(), &["push", "origin", &branch_ref])?;
    let hook = format!(
        "git tag -a v0.2.0 -m v0.2.0 && git push --atomic origin HEAD:refs/heads/{} refs/tags/v0.2.0:refs/tags/v0.2.0 && exit 1",
        branch.trim()
    );
    write_file(
        &repo.path().join("verso.toml"),
        &format!(
            r#"
[workspaces]
patterns = ["packages/*"]
include_root = true

[changelog]
enabled = true

[hooks]
after_commit = {hook:?}
"#
        ),
    )?;
    git(repo.path(), &["add", "verso.toml"])?;
    git(
        repo.path(),
        &["commit", "-m", "test: publish inside committed hook"],
    )?;
    git(repo.path(), &["push"])?;

    Command::cargo_bin("verso")?
        .current_dir(repo.path())
        .args(["--version", "0.2.0", "--yes"])
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "planned release tag already exists on the remote",
        ));

    assert!(repo.path().join(".git/verso/active.json").exists());
    assert!(fs::read_to_string(repo.path().join("package.json"))?.contains("0.2.0"));
    assert_eq!(
        git_stdout(repo.path(), &["tag", "--list", "v0.2.0"])?.trim(),
        "v0.2.0"
    );
    Command::cargo_bin("verso")?
        .current_dir(repo.path())
        .arg("abort")
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "planned release tag already exists on the remote",
        ));
    Ok(())
}

#[test]
fn a_before_commit_hook_cannot_publish_and_then_trigger_local_rollback(
) -> Result<(), Box<dyn std::error::Error>> {
    let repo = TempDir::new()?;
    write_release_fixture(repo.path())?;
    let remote = repo.path().join(".git/release-test-remote.git");
    let remote_string = remote.to_str().ok_or("non-UTF-8 path")?;
    git(repo.path(), &["init", "--bare", remote_string])?;
    let branch = git_stdout(repo.path(), &["symbolic-ref", "--short", "HEAD"])?;
    let branch_ref = format!("HEAD:refs/heads/{}", branch.trim());
    git(repo.path(), &["push", "origin", &branch_ref])?;
    let hook = format!(
        "git add -- package.json packages/a/package.json CHANGELOG.md && git commit --cleanup=verbatim -m 'chore(release): release v0.2.0' && git tag -a v0.2.0 -m v0.2.0 && git push --atomic origin HEAD:refs/heads/{} refs/tags/v0.2.0:refs/tags/v0.2.0 && exit 1",
        branch.trim()
    );
    write_file(
        &repo.path().join("verso.toml"),
        &format!(
            r#"
[workspaces]
patterns = ["packages/*"]
include_root = true

[changelog]
enabled = true

[hooks]
before_commit = {hook:?}
"#
        ),
    )?;
    git(repo.path(), &["add", "verso.toml"])?;
    git(
        repo.path(),
        &["commit", "-m", "test: publish inside before-commit hook"],
    )?;
    git(repo.path(), &["push"])?;

    Command::cargo_bin("verso")?
        .current_dir(repo.path())
        .args(["--version", "0.2.0", "--yes"])
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "planned release tag already exists on the remote",
        ));

    assert!(repo.path().join(".git/verso/active.json").exists());
    assert!(fs::read_to_string(repo.path().join("package.json"))?.contains("0.2.0"));
    assert_eq!(
        git_stdout(repo.path(), &["tag", "--list", "v0.2.0"])?.trim(),
        "v0.2.0"
    );
    Ok(())
}

#[test]
fn after_version_hook_failure_rolls_back_release_files() -> Result<(), Box<dyn std::error::Error>> {
    let repo = TempDir::new()?;
    write_release_fixture(repo.path())?;
    write_file(
        &repo.path().join("verso.toml"),
        r#"
[workspaces]
patterns = ["packages/*"]
include_root = true

[hooks]
after_version = "git config --file hook.log --get missing.key"
"#,
    )?;
    git(repo.path(), &["add", "verso.toml"])?;
    git(repo.path(), &["commit", "-m", "test: add failing hook"])?;
    let remote = repo.path().join(".git/release-test-remote.git");
    let remote_string = remote.to_str().ok_or("non-UTF-8 path")?;
    git(repo.path(), &["init", "--bare", remote_string])?;
    git(repo.path(), &["push", "origin", "HEAD"])?;

    Command::cargo_bin("verso")?
        .current_dir(repo.path())
        .args(["--version", "0.2.0", "--yes"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("hook after_version failed"));

    assert!(
        fs::read_to_string(repo.path().join("package.json"))?.contains("\"version\": \"0.1.0\"")
    );
    assert!(!fs::read_to_string(repo.path().join("CHANGELOG.md"))?.contains("0.2.0"));
    assert_eq!(
        git_stdout(repo.path(), &["status", "--porcelain"])?,
        String::new()
    );

    Ok(())
}

#[test]
fn a_failed_release_hook_preserves_the_transaction_when_remote_is_unreachable(
) -> Result<(), Box<dyn std::error::Error>> {
    let repo = TempDir::new()?;
    write_release_fixture(repo.path())?;
    write_file(
        &repo.path().join("verso.toml"),
        r#"
[workspaces]
patterns = ["packages/*"]
include_root = true

[hooks]
after_version = "exit 1"
"#,
    )?;
    git(repo.path(), &["add", "verso.toml"])?;
    git(repo.path(), &["commit", "-m", "test: add failing hook"])?;

    Command::cargo_bin("verso")?
        .current_dir(repo.path())
        .args(["--version", "0.2.0", "--yes"])
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "cannot inspect the remote release refs",
        ));

    assert!(repo.path().join(".git/verso/active.json").exists());
    assert!(fs::read_to_string(repo.path().join("package.json"))?.contains("0.2.0"));
    Ok(())
}

#[test]
fn release_prompts_before_each_mutating_step() -> Result<(), Box<dyn std::error::Error>> {
    let repo = TempDir::new()?;
    write_release_fixture(repo.path())?;

    Command::cargo_bin("verso")?
        .current_dir(repo.path())
        .args(["--version", "0.2.0"])
        .write_stdin("y\ny\ny\ny\n")
        .assert()
        .failure()
        .stdout(predicate::str::contains(
            "Modify release files for 0.2.0? [Y/n]",
        ))
        .stdout(predicate::str::contains(
            "Commit release files with \"chore(release): release v0.2.0\"? [Y/n]",
        ))
        .stdout(predicate::str::contains("Create tag v0.2.0? [Y/n]"))
        .stdout(predicate::str::contains(
            "Push release commit and tag? [Y/n]",
        ))
        .stderr(predicate::str::contains("git push --atomic"));

    Ok(())
}

#[test]
fn release_confirmation_defaults_to_yes() -> Result<(), Box<dyn std::error::Error>> {
    let repo = TempDir::new()?;
    write_release_fixture(repo.path())?;

    Command::cargo_bin("verso")?
        .current_dir(repo.path())
        .args(["--version", "0.2.0"])
        .write_stdin("\n\n\n\n")
        .assert()
        .failure()
        .stdout(predicate::str::contains(
            "Modify release files for 0.2.0? [Y/n]",
        ))
        .stdout(predicate::str::contains(
            "Commit release files with \"chore(release): release v0.2.0\"? [Y/n]",
        ))
        .stdout(predicate::str::contains("Create tag v0.2.0? [Y/n]"))
        .stdout(predicate::str::contains(
            "Push release commit and tag? [Y/n]",
        ))
        .stderr(predicate::str::contains("git push --atomic"));

    assert!(
        fs::read_to_string(repo.path().join("package.json"))?.contains("\"version\": \"0.2.0\"")
    );
    assert_eq!(
        git_stdout(repo.path(), &["tag", "--list", "v0.2.0"])?.trim(),
        "v0.2.0"
    );

    Ok(())
}

#[test]
fn abort_before_modifying_release_files_leaves_worktree_clean(
) -> Result<(), Box<dyn std::error::Error>> {
    let repo = TempDir::new()?;
    write_release_fixture(repo.path())?;

    Command::cargo_bin("verso")?
        .current_dir(repo.path())
        .args(["--version", "0.2.0"])
        .write_stdin("n\n")
        .assert()
        .failure()
        .stdout(predicate::str::contains(
            "Modify release files for 0.2.0? [Y/n]",
        ))
        .stderr(predicate::str::contains("cancelled: release aborted"));

    assert!(
        fs::read_to_string(repo.path().join("package.json"))?.contains("\"version\": \"0.1.0\"")
    );
    assert!(!fs::read_to_string(repo.path().join("CHANGELOG.md"))?.contains("0.2.0"));
    assert_eq!(
        git_stdout(repo.path(), &["status", "--porcelain"])?,
        String::new()
    );
    assert_eq!(
        git_stdout(repo.path(), &["tag", "--list", "v0.2.0"])?,
        String::new()
    );

    Ok(())
}

#[test]
fn abort_before_commit_keeps_release_files() -> Result<(), Box<dyn std::error::Error>> {
    let repo = TempDir::new()?;
    write_release_fixture(repo.path())?;
    let before_head = git_stdout(repo.path(), &["rev-parse", "HEAD"])?;

    Command::cargo_bin("verso")?
        .current_dir(repo.path())
        .args(["--version", "0.2.0"])
        .write_stdin("y\nn\n")
        .assert()
        .failure()
        .stdout(predicate::str::contains(
            "Commit release files with \"chore(release): release v0.2.0\"? [Y/n]",
        ))
        .stderr(predicate::str::contains("release aborted"));

    assert!(
        fs::read_to_string(repo.path().join("package.json"))?.contains("\"version\": \"0.2.0\"")
    );
    assert!(fs::read_to_string(repo.path().join("CHANGELOG.md"))?.contains("0.2.0"));
    assert_eq!(
        git_stdout(repo.path(), &["rev-parse", "HEAD"])?,
        before_head
    );
    let status = git_stdout(repo.path(), &["status", "--porcelain"])?;
    assert!(status.contains(" M CHANGELOG.md"));
    assert!(status.contains(" M package.json"));

    Ok(())
}

#[test]
fn files_applied_status_and_abort_restore_transaction() -> Result<(), Box<dyn std::error::Error>> {
    let repo = TempDir::new()?;
    write_release_fixture(repo.path())?;
    let before_head = git_stdout(repo.path(), &["rev-parse", "HEAD"])?;

    Command::cargo_bin("verso")?
        .current_dir(repo.path())
        .args(["--version", "0.2.0"])
        .write_stdin("y\nn\n")
        .assert()
        .failure()
        .stderr(predicate::str::contains("release aborted"));

    let status_output = Command::cargo_bin("verso")?
        .current_dir(repo.path())
        .args(["status", "--json"])
        .output()?;
    let status: serde_json::Value = serde_json::from_slice(&status_output.stdout)?;
    assert!(status_output.status.success());
    assert_eq!(status["active"], true);
    assert_eq!(status["operation"], "release");
    assert_eq!(status["stage"], "files-applied");
    assert_eq!(status["targetVersion"], "0.2.0");
    assert_eq!(status["canAbort"], true);

    Command::cargo_bin("verso")?
        .current_dir(repo.path())
        .args(["abort", "--dry-run"])
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "--dry-run can only be used with release or bump",
        ));
    assert!(repo.path().join(".git/verso/active.json").exists());
    assert!(
        fs::read_to_string(repo.path().join("package.json"))?.contains("\"version\": \"0.2.0\"")
    );

    Command::cargo_bin("verso")?
        .current_dir(repo.path())
        .arg("abort")
        .assert()
        .success()
        .stdout(predicate::str::contains("restored release files"));

    assert!(
        fs::read_to_string(repo.path().join("package.json"))?.contains("\"version\": \"0.1.0\"")
    );
    assert!(
        fs::read_to_string(repo.path().join("packages/a/package.json"))?
            .contains("\"version\": \"0.1.0\"")
    );
    assert!(!fs::read_to_string(repo.path().join("CHANGELOG.md"))?.contains("0.2.0"));
    assert_eq!(
        git_stdout(repo.path(), &["rev-parse", "HEAD"])?,
        before_head
    );
    assert_eq!(git_stdout(repo.path(), &["status", "--porcelain"])?, "");
    assert!(!repo.path().join(".git/verso/active.json").exists());

    let cleared_output = Command::cargo_bin("verso")?
        .current_dir(repo.path())
        .args(["status", "--json"])
        .output()?;
    let cleared: serde_json::Value = serde_json::from_slice(&cleared_output.stdout)?;
    assert_eq!(cleared["active"], false);

    Ok(())
}

#[test]
fn push_failure_requires_resume_and_rejects_partial_remote_state(
) -> Result<(), Box<dyn std::error::Error>> {
    let repo = TempDir::new()?;
    write_release_fixture(repo.path())?;
    let before_release = git_stdout(repo.path(), &["rev-parse", "HEAD"])?;

    Command::cargo_bin("verso")?
        .current_dir(repo.path())
        .args(["--version", "0.2.0"])
        .write_stdin("y\nn\n")
        .assert()
        .failure()
        .stderr(predicate::str::contains("release aborted"));

    Command::cargo_bin("verso")?
        .current_dir(repo.path())
        .arg("resume")
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "Local release commit and tag were created",
        ));

    let status_output = Command::cargo_bin("verso")?
        .current_dir(repo.path())
        .args(["status", "--json"])
        .output()?;
    let status: serde_json::Value = serde_json::from_slice(&status_output.stdout)?;
    assert_eq!(status["active"], true);
    assert_eq!(status["stage"], "tagged");
    assert_eq!(status["pushStarted"], true);
    assert_eq!(status["pushFailed"], true);
    assert_eq!(status["canAbort"], false);
    assert_eq!(
        git_stdout(repo.path(), &["log", "-1", "--pretty=%s"])?.trim(),
        "chore(release): release v0.2.0"
    );
    assert_eq!(
        git_stdout(repo.path(), &["tag", "--list", "v0.2.0"])?.trim(),
        "v0.2.0"
    );

    Command::cargo_bin("verso")?
        .current_dir(repo.path())
        .arg("abort")
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "cannot automatically abort after a push was started",
        ));

    let remote = repo.path().join(".git/release-test-remote.git");
    let remote_string = remote.to_str().ok_or("non-UTF-8 path")?;
    git(repo.path(), &["init", "--bare", remote_string])?;
    let branch = git_stdout(repo.path(), &["symbolic-ref", "--short", "HEAD"])?;
    let branch_ref = format!("HEAD:refs/heads/{}", branch.trim());
    git(repo.path(), &["push", "origin", &branch_ref])?;

    Command::cargo_bin("verso")?
        .current_dir(repo.path())
        .arg("resume")
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "remote release refs are partial or moved",
        ));

    let before_ref = format!("{}:refs/heads/{}", before_release.trim(), branch.trim());
    git(repo.path(), &["push", "--force", "origin", &before_ref])?;
    git(
        repo.path(),
        &["push", "origin", "refs/tags/v0.2.0:refs/tags/v0.2.0"],
    )?;
    Command::cargo_bin("verso")?
        .current_dir(repo.path())
        .arg("resume")
        .assert()
        .failure()
        .stderr(predicate::str::contains("does not contain release commit"));
    git(repo.path(), &["push", "origin", ":refs/tags/v0.2.0"])?;
    Command::cargo_bin("verso")?
        .current_dir(repo.path())
        .arg("resume")
        .assert()
        .success();

    let status_output = Command::cargo_bin("verso")?
        .current_dir(repo.path())
        .args(["status", "--json"])
        .output()?;
    let status: serde_json::Value = serde_json::from_slice(&status_output.stdout)?;
    assert_eq!(status["active"], false);

    Ok(())
}

#[test]
fn force_abort_unlocks_a_transaction_after_the_release_tag_was_removed(
) -> Result<(), Box<dyn std::error::Error>> {
    let repo = TempDir::new()?;
    write_release_fixture(repo.path())?;

    Command::cargo_bin("verso")?
        .current_dir(repo.path())
        .args(["--version", "0.2.0", "--yes"])
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "Local release commit and tag were created",
        ));
    git(repo.path(), &["tag", "--delete", "v0.2.0"])?;
    let remote = repo.path().join(".git/release-test-remote.git");
    let remote_string = remote.to_str().ok_or("non-UTF-8 path")?;
    git(repo.path(), &["init", "--bare", remote_string])?;

    Command::cargo_bin("verso")?
        .current_dir(repo.path())
        .arg("resume")
        .assert()
        .failure()
        .stderr(predicate::str::contains("release tag v0.2.0 is missing"));
    let head_before_force = git_stdout(repo.path(), &["rev-parse", "HEAD"])?;
    let package_before_force = fs::read_to_string(repo.path().join("package.json"))?;
    let status_output = Command::cargo_bin("verso")?
        .current_dir(repo.path())
        .args(["status", "--json"])
        .output()?;
    let status: serde_json::Value = serde_json::from_slice(&status_output.stdout)?;
    assert_eq!(status["canAbort"], false);
    assert_eq!(status["canForceAbort"], true);

    Command::cargo_bin("verso")?
        .current_dir(repo.path())
        .args(["abort", "--force"])
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "Discarded the active Verso transaction",
        ))
        .stderr(predicate::str::contains(
            "did not change local files, commits, tags, or remote refs",
        ));

    let status_output = Command::cargo_bin("verso")?
        .current_dir(repo.path())
        .args(["status", "--json"])
        .output()?;
    let status: serde_json::Value = serde_json::from_slice(&status_output.stdout)?;
    assert_eq!(status["active"], false);
    assert_eq!(
        git_stdout(repo.path(), &["rev-parse", "HEAD"])?,
        head_before_force
    );
    assert_eq!(
        fs::read_to_string(repo.path().join("package.json"))?,
        package_before_force
    );
    assert_eq!(git_stdout(repo.path(), &["tag", "--list", "v0.2.0"])?, "");

    Ok(())
}

#[test]
fn force_abort_can_discard_a_corrupt_transaction_journal() -> Result<(), Box<dyn std::error::Error>>
{
    let repo = TempDir::new()?;
    write_release_fixture(repo.path())?;
    let journal = repo.path().join(".git/verso/active.json");
    write_file(&journal, "not valid JSON\n")?;

    Command::cargo_bin("verso")?
        .current_dir(repo.path())
        .arg("status")
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "failed to parse transaction journal",
        ));

    Command::cargo_bin("verso")?
        .current_dir(repo.path())
        .args(["abort", "--force"])
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "Discarded the active Verso transaction",
        ));

    assert!(!journal.exists());

    Ok(())
}

#[test]
fn a_new_release_prompts_to_resume_or_abort_the_active_transaction(
) -> Result<(), Box<dyn std::error::Error>> {
    let repo = TempDir::new()?;
    write_release_fixture(repo.path())?;

    Command::cargo_bin("verso")?
        .current_dir(repo.path())
        .args(["--version", "0.2.0"])
        .write_stdin("y\nn\n")
        .assert()
        .failure()
        .stderr(predicate::str::contains("release aborted"));

    Command::cargo_bin("verso")?
        .current_dir(repo.path())
        .args(["--version", "0.3.0"])
        .write_stdin("abort\n")
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "An active Verso transaction already exists",
        ))
        .stdout(predicate::str::contains("1) Resume"))
        .stdout(predicate::str::contains("2) Abort"))
        .stdout(predicate::str::contains(
            "Aborted Verso transaction and restored release files",
        ));

    assert!(
        fs::read_to_string(repo.path().join("package.json"))?.contains("\"version\": \"0.1.0\"")
    );
    assert!(!repo.path().join(".git/verso/active.json").exists());

    Ok(())
}

#[test]
fn choosing_resume_from_a_new_release_finishes_only_the_active_transaction(
) -> Result<(), Box<dyn std::error::Error>> {
    let repo = TempDir::new()?;
    write_release_fixture(repo.path())?;

    Command::cargo_bin("verso")?
        .current_dir(repo.path())
        .args(["--version", "0.2.0"])
        .write_stdin("y\nn\n")
        .assert()
        .failure()
        .stderr(predicate::str::contains("release aborted"));
    let remote = repo.path().join(".git/release-test-remote.git");
    let remote_string = remote.to_str().ok_or("non-UTF-8 path")?;
    git(repo.path(), &["init", "--bare", remote_string])?;

    Command::cargo_bin("verso")?
        .current_dir(repo.path())
        .args(["--version", "0.3.0"])
        .write_stdin("resume\n")
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "An active Verso transaction already exists",
        ));

    assert!(
        fs::read_to_string(repo.path().join("package.json"))?.contains("\"version\": \"0.2.0\"")
    );
    assert_eq!(
        git_stdout(repo.path(), &["tag", "--list", "v0.2.0"])?.trim(),
        "v0.2.0"
    );
    assert_eq!(git_stdout(repo.path(), &["tag", "--list", "v0.3.0"])?, "");
    assert!(!repo.path().join(".git/verso/active.json").exists());

    Ok(())
}

#[test]
fn resume_uses_the_pinned_release_after_local_head_advances(
) -> Result<(), Box<dyn std::error::Error>> {
    let repo = TempDir::new()?;
    write_release_fixture(repo.path())?;

    Command::cargo_bin("verso")?
        .current_dir(repo.path())
        .args(["--version", "0.2.0", "--yes"])
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "Local release commit and tag were created",
        ));
    let release_head = git_stdout(repo.path(), &["rev-parse", "HEAD"])?;
    git(
        repo.path(),
        &["commit", "--allow-empty", "-m", "chore: later local work"],
    )?;
    let later_head = git_stdout(repo.path(), &["rev-parse", "HEAD"])?;
    let remote = repo.path().join(".git/release-test-remote.git");
    let remote_string = remote.to_str().ok_or("non-UTF-8 path")?;
    git(repo.path(), &["init", "--bare", remote_string])?;

    Command::cargo_bin("verso")?
        .current_dir(repo.path())
        .arg("resume")
        .assert()
        .success();

    let branch = git_stdout(repo.path(), &["symbolic-ref", "--short", "HEAD"])?;
    let branch_ref = format!("refs/heads/{}", branch.trim());
    assert_eq!(
        git_stdout(repo.path(), &["ls-remote", "origin", &branch_ref])?
            .split_whitespace()
            .next(),
        Some(release_head.trim())
    );
    assert_eq!(git_stdout(repo.path(), &["rev-parse", "HEAD"])?, later_head);
    assert!(!repo.path().join(".git/verso/active.json").exists());
    Ok(())
}

#[test]
fn abort_before_tag_keeps_release_commit_and_files() -> Result<(), Box<dyn std::error::Error>> {
    let repo = TempDir::new()?;
    write_release_fixture(repo.path())?;

    Command::cargo_bin("verso")?
        .current_dir(repo.path())
        .args(["--version", "0.2.0"])
        .write_stdin("y\ny\nn\n")
        .assert()
        .failure()
        .stdout(predicate::str::contains("Create tag v0.2.0? [Y/n]"))
        .stderr(predicate::str::contains("release aborted"));

    assert!(
        fs::read_to_string(repo.path().join("package.json"))?.contains("\"version\": \"0.2.0\"")
    );
    assert!(fs::read_to_string(repo.path().join("CHANGELOG.md"))?.contains("0.2.0"));
    assert!(git_stdout(repo.path(), &["log", "-1", "--pretty=%s"])?
        .contains("chore(release): release v0.2.0"));
    assert_eq!(
        git_stdout(repo.path(), &["status", "--porcelain"])?,
        String::new()
    );
    assert_eq!(
        git_stdout(repo.path(), &["tag", "--list", "v0.2.0"])?,
        String::new()
    );

    Ok(())
}

#[test]
fn abort_before_push_keeps_local_release_commit_and_tag() -> Result<(), Box<dyn std::error::Error>>
{
    let repo = TempDir::new()?;
    write_release_fixture(repo.path())?;

    Command::cargo_bin("verso")?
        .current_dir(repo.path())
        .args(["--version", "0.2.0"])
        .write_stdin("y\ny\ny\nn\n")
        .assert()
        .failure()
        .stdout(predicate::str::contains(
            "Push release commit and tag? [Y/n]",
        ))
        .stderr(predicate::str::contains("release aborted"));

    assert!(
        fs::read_to_string(repo.path().join("package.json"))?.contains("\"version\": \"0.2.0\"")
    );
    assert!(fs::read_to_string(repo.path().join("CHANGELOG.md"))?.contains("0.2.0"));
    assert!(git_stdout(repo.path(), &["log", "-1", "--pretty=%s"])?
        .contains("chore(release): release v0.2.0"));
    assert_eq!(
        git_stdout(repo.path(), &["tag", "--list", "v0.2.0"])?.trim(),
        "v0.2.0"
    );
    assert_eq!(
        git_stdout(repo.path(), &["status", "--porcelain"])?,
        String::new()
    );

    Ok(())
}

#[test]
fn abort_refuses_when_the_remote_branch_already_contains_the_release(
) -> Result<(), Box<dyn std::error::Error>> {
    let repo = TempDir::new()?;
    write_release_fixture(repo.path())?;
    let remote = repo.path().join(".git/release-test-remote.git");
    let remote_string = remote.to_str().ok_or("non-UTF-8 path")?;
    git(repo.path(), &["init", "--bare", remote_string])?;
    let branch = git_stdout(repo.path(), &["symbolic-ref", "--short", "HEAD"])?;
    let branch_ref = format!("HEAD:refs/heads/{}", branch.trim());
    git(repo.path(), &["push", "origin", &branch_ref])?;

    Command::cargo_bin("verso")?
        .current_dir(repo.path())
        .args(["--version", "0.2.0"])
        .write_stdin("y\ny\ny\nn\n")
        .assert()
        .failure();

    let release_head = git_stdout(repo.path(), &["rev-parse", "HEAD"])?;
    let release_head = release_head.trim();
    let tree = format!("{release_head}^{{tree}}");
    let descendant = git_stdout(
        repo.path(),
        &[
            "commit-tree",
            &tree,
            "-p",
            release_head,
            "-m",
            "test: later remote commit",
        ],
    )?;
    let descendant_ref = format!("{}:refs/heads/{}", descendant.trim(), branch.trim());
    git(repo.path(), &["push", "origin", &descendant_ref])?;

    Command::cargo_bin("verso")?
        .current_dir(repo.path())
        .arg("abort")
        .assert()
        .failure()
        .stderr(predicate::str::contains("already contains release commit"));

    assert!(repo.path().join(".git/verso/active.json").exists());
    assert_eq!(
        git_stdout(repo.path(), &["rev-parse", "HEAD"])?.trim(),
        release_head
    );
    assert_eq!(
        git_stdout(repo.path(), &["tag", "--list", "v0.2.0"])?.trim(),
        "v0.2.0"
    );
    Ok(())
}

#[test]
fn existing_tag_blocks_release_without_writing_files() -> Result<(), Box<dyn std::error::Error>> {
    let repo = TempDir::new()?;
    write_release_fixture(repo.path())?;
    git(repo.path(), &["tag", "v0.2.0"])?;

    Command::cargo_bin("verso")?
        .current_dir(repo.path())
        .args(["--version", "0.2.0", "--yes"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("tag v0.2.0 already exists"))
        .stderr(predicate::str::contains("help:"));

    assert!(
        fs::read_to_string(repo.path().join("package.json"))?.contains("\"version\": \"0.1.0\"")
    );
    assert!(!fs::read_to_string(repo.path().join("CHANGELOG.md"))?.contains("0.2.0"));
    assert_eq!(
        git_stdout(repo.path(), &["status", "--porcelain"])?,
        String::new()
    );

    Ok(())
}

#[test]
fn behind_upstream_blocks_release_before_writing_files() -> Result<(), Box<dyn std::error::Error>> {
    let repo = TempDir::new()?;
    write_release_fixture(repo.path())?;
    git(repo.path(), &["remote", "remove", "origin"])?;
    let remote = repo.path().join(".git/release-test-remote.git");
    git(
        repo.path(),
        &["init", "--bare", remote.to_str().ok_or("non-UTF-8 path")?],
    )?;
    git(
        repo.path(),
        &[
            "remote",
            "add",
            "origin",
            remote.to_str().ok_or("non-UTF-8 path")?,
        ],
    )?;
    git(repo.path(), &["push", "-u", "origin", "HEAD"])?;
    write_file(&repo.path().join("remote-only.md"), "remote")?;
    git(repo.path(), &["add", "remote-only.md"])?;
    git(repo.path(), &["commit", "-m", "feat: remote only"])?;
    git(repo.path(), &["push"])?;
    git(repo.path(), &["reset", "--hard", "HEAD~1"])?;

    Command::cargo_bin("verso")?
        .current_dir(repo.path())
        .args(["--version", "0.2.0", "--yes"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("is behind"));

    assert!(
        fs::read_to_string(repo.path().join("package.json"))?.contains("\"version\": \"0.1.0\"")
    );
    assert_eq!(git_stdout(repo.path(), &["tag", "--list", "v0.2.0"])?, "");
    Ok(())
}

#[test]
fn dirty_release_files_are_rejected_when_global_clean_check_is_disabled(
) -> Result<(), Box<dyn std::error::Error>> {
    let repo = TempDir::new()?;
    write_release_fixture(repo.path())?;
    write_file(
        &repo.path().join("verso.toml"),
        r#"
[workspaces]
patterns = ["packages/*"]
include_root = true

[git]
require_clean_worktree = false
"#,
    )?;
    git(repo.path(), &["add", "verso.toml"])?;
    git(
        repo.path(),
        &["commit", "-m", "test: allow unrelated dirty files"],
    )?;
    write_file(
        &repo.path().join("package.json"),
        "{\n  \"name\": \"root\",\n  \"version\": \"0.1.0\",\n  \"private\": true\n}\n",
    )?;

    Command::cargo_bin("verso")?
        .current_dir(repo.path())
        .args(["--version", "0.2.0", "--yes"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("release files are dirty"))
        .stderr(predicate::str::contains("help:"));

    assert!(
        fs::read_to_string(repo.path().join("package.json"))?.contains("\"version\": \"0.1.0\"")
    );
    assert_eq!(git_stdout(repo.path(), &["tag", "--list", "v0.2.0"])?, "");
    Ok(())
}

#[test]
fn staged_unrelated_files_are_rejected_when_global_clean_check_is_disabled(
) -> Result<(), Box<dyn std::error::Error>> {
    let repo = TempDir::new()?;
    write_release_fixture(repo.path())?;
    write_file(
        &repo.path().join("verso.toml"),
        r#"
[workspaces]
patterns = ["packages/*"]
include_root = true

[git]
require_clean_worktree = false
"#,
    )?;
    git(repo.path(), &["add", "verso.toml"])?;
    git(
        repo.path(),
        &["commit", "-m", "test: allow unrelated dirty files"],
    )?;
    write_file(&repo.path().join("notes.md"), "do not release\n")?;
    git(repo.path(), &["add", "notes.md"])?;
    let before_head = git_stdout(repo.path(), &["rev-parse", "HEAD"])?;

    Command::cargo_bin("verso")?
        .current_dir(repo.path())
        .args(["--version", "0.2.0", "--yes"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("Git index is not clean"));

    assert_eq!(
        git_stdout(repo.path(), &["rev-parse", "HEAD"])?,
        before_head
    );
    assert!(
        git_stdout(repo.path(), &["diff", "--cached", "--name-only"])?
            .lines()
            .any(|path| path == "notes.md")
    );
    assert_eq!(git_stdout(repo.path(), &["tag", "--list", "v0.2.0"])?, "");
    Ok(())
}

#[test]
fn unstaged_unrelated_files_are_preserved_when_global_clean_check_is_disabled(
) -> Result<(), Box<dyn std::error::Error>> {
    let repo = TempDir::new()?;
    write_release_fixture(repo.path())?;
    write_file(
        &repo.path().join("verso.toml"),
        r#"
[workspaces]
patterns = ["packages/*"]
include_root = true

[git]
require_clean_worktree = false
"#,
    )?;
    write_file(&repo.path().join("notes.md"), "keep this\n")?;
    git(repo.path(), &["add", "verso.toml", "notes.md"])?;
    git(
        repo.path(),
        &["commit", "-m", "test: allow unrelated dirty files"],
    )?;
    write_file(&repo.path().join("notes.md"), "keep this update\n")?;

    Command::cargo_bin("verso")?
        .current_dir(repo.path())
        .args(["--version", "0.2.0", "--yes"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("git push --atomic"));

    assert_eq!(
        fs::read_to_string(repo.path().join("notes.md"))?,
        "keep this update\n"
    );
    assert!(
        !git_stdout(repo.path(), &["show", "--format=", "--name-only", "HEAD"])?
            .lines()
            .any(|path| path == "notes.md")
    );
    Ok(())
}

#[test]
fn commit_failure_unstages_and_rolls_back_release_files() -> Result<(), Box<dyn std::error::Error>>
{
    let repo = TempDir::new()?;
    write_release_fixture(repo.path())?;
    let remote = repo.path().join(".git/release-test-remote.git");
    let remote_string = remote.to_str().ok_or("non-UTF-8 path")?;
    git(repo.path(), &["init", "--bare", remote_string])?;
    let branch = git_stdout(repo.path(), &["symbolic-ref", "--short", "HEAD"])?;
    let branch_ref = format!("HEAD:refs/heads/{}", branch.trim());
    git(repo.path(), &["push", "origin", &branch_ref])?;
    git(repo.path(), &["config", "--unset", "user.email"])?;
    git(repo.path(), &["config", "--unset", "user.name"])?;
    let isolated_home = TempDir::new()?;
    let before_head = git_stdout(repo.path(), &["rev-parse", "HEAD"])?;

    Command::cargo_bin("verso")?
        .current_dir(repo.path())
        .env("HOME", isolated_home.path())
        .env("XDG_CONFIG_HOME", isolated_home.path())
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_AUTHOR_NAME", "")
        .env("GIT_AUTHOR_EMAIL", "")
        .env("GIT_COMMITTER_NAME", "")
        .env("GIT_COMMITTER_EMAIL", "")
        .args(["--version", "0.2.0", "--yes"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("git commit"));

    assert!(
        fs::read_to_string(repo.path().join("package.json"))?.contains("\"version\": \"0.1.0\"")
    );
    assert!(!fs::read_to_string(repo.path().join("CHANGELOG.md"))?.contains("0.2.0"));
    assert_eq!(
        git_stdout(repo.path(), &["rev-parse", "HEAD"])?,
        before_head
    );
    assert_eq!(
        git_stdout(repo.path(), &["status", "--porcelain"])?,
        String::new()
    );
    assert_eq!(
        git_stdout(repo.path(), &["tag", "--list", "v0.2.0"])?,
        String::new()
    );

    Ok(())
}

#[cfg(unix)]
#[test]
fn native_commit_hook_cannot_change_the_exact_release_commit(
) -> Result<(), Box<dyn std::error::Error>> {
    use std::os::unix::fs::PermissionsExt;

    let repo = TempDir::new()?;
    write_release_fixture(repo.path())?;
    let hook = repo.path().join(".git/hooks/pre-commit");
    write_file(
        &hook,
        "#!/bin/sh\nperl -0pi -e 's/\"version\": \"0.2.0\"/\"version\": \"9.9.9\"/' package.json\ngit add package.json\n",
    )?;
    fs::set_permissions(&hook, fs::Permissions::from_mode(0o755))?;

    Command::cargo_bin("verso")?
        .current_dir(repo.path())
        .args(["--version", "0.2.0", "--yes"])
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "created commit does not match the exact release plan",
        ));

    assert!(repo.path().join(".git/verso/active.json").exists());
    assert_eq!(
        git_stdout(repo.path(), &["tag", "--list", "v0.2.0"])?.trim(),
        ""
    );
    assert!(fs::read_to_string(repo.path().join("package.json"))?.contains("9.9.9"));

    Ok(())
}

#[test]
fn add_failure_unstages_and_rolls_back_release_files() -> Result<(), Box<dyn std::error::Error>> {
    let repo = TempDir::new()?;
    write_release_fixture(repo.path())?;
    write_file(
        &repo.path().join("verso.toml"),
        r#"
[workspaces]
patterns = ["packages/*"]
include_root = true

[changelog]
enabled = true
infile = "ignored/CHANGELOG.md"
"#,
    )?;
    write_file(&repo.path().join(".gitignore"), "ignored/\n")?;
    git(repo.path(), &["add", "verso.toml", ".gitignore"])?;
    git(
        repo.path(),
        &["commit", "-m", "test: ignored changelog path"],
    )?;
    fs::create_dir(repo.path().join("ignored"))?;

    Command::cargo_bin("verso")?
        .current_dir(repo.path())
        .args(["--version", "0.2.0", "--yes"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("git add"))
        .stderr(predicate::str::contains("ignored"));

    assert!(
        fs::read_to_string(repo.path().join("package.json"))?.contains("\"version\": \"0.1.0\"")
    );
    assert!(
        fs::read_to_string(repo.path().join("packages/a/package.json"))?
            .contains("\"version\": \"0.1.0\"")
    );
    assert!(!repo.path().join("ignored/CHANGELOG.md").exists());
    assert_eq!(
        git_stdout(repo.path(), &["status", "--porcelain"])?,
        String::new()
    );

    Ok(())
}

#[test]
fn explicit_non_forward_version_requires_confirmation() -> Result<(), Box<dyn std::error::Error>> {
    let repo = TempDir::new()?;
    write_release_fixture(repo.path())?;

    Command::cargo_bin("verso")?
        .current_dir(repo.path())
        .args(["--dry-run", "--version", "0.1.0"])
        .write_stdin("n\n")
        .assert()
        .failure()
        .stdout(predicate::str::contains(
            "Target version is not greater than current version. Continue? [y/N]",
        ))
        .stderr(predicate::str::contains("release aborted"));

    Command::cargo_bin("verso")?
        .current_dir(repo.path())
        .args(["--dry-run", "--version", "0.0.9"])
        .write_stdin("n\n")
        .assert()
        .failure()
        .stdout(predicate::str::contains(
            "Target version is not greater than current version. Continue? [y/N]",
        ))
        .stderr(predicate::str::contains("release aborted"));

    Command::cargo_bin("verso")?
        .current_dir(repo.path())
        .args(["--dry-run", "--version", "0.1.0"])
        .write_stdin("\n")
        .assert()
        .failure()
        .stdout(predicate::str::contains(
            "Target version is not greater than current version. Continue? [y/N]",
        ))
        .stderr(predicate::str::contains("release aborted"));

    Command::cargo_bin("verso")?
        .current_dir(repo.path())
        .args(["--dry-run", "--version", "0.1.0", "--yes"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Target version: 0.1.0"));

    Ok(())
}

#[test]
fn interactive_custom_equal_version_requires_confirmation() -> Result<(), Box<dyn std::error::Error>>
{
    let repo = TempDir::new()?;
    write_release_fixture(repo.path())?;

    Command::cargo_bin("verso")?
        .current_dir(repo.path())
        .args(["--dry-run"])
        .write_stdin("custom\n0.1.0\nn\n")
        .assert()
        .failure()
        .stdout(predicate::str::contains("Target version is not greater"))
        .stderr(predicate::str::contains("release aborted"));

    Ok(())
}

#[test]
fn interactive_beta_minor_dry_run_uses_computed_prerelease(
) -> Result<(), Box<dyn std::error::Error>> {
    let repo = TempDir::new()?;
    write_release_fixture(repo.path())?;

    Command::cargo_bin("verso")?
        .current_dir(repo.path())
        .args(["--dry-run"])
        .write_stdin("beta\nminor\n")
        .assert()
        .success()
        .stdout(predicate::str::contains("Target version: 0.2.0-beta.0"));

    Ok(())
}

#[test]
fn interactive_prerelease_accepts_custom_base_version() -> Result<(), Box<dyn std::error::Error>> {
    let repo = TempDir::new()?;
    write_release_fixture(repo.path())?;

    Command::cargo_bin("verso")?
        .current_dir(repo.path())
        .args(["--dry-run"])
        .write_stdin("beta\ncustom\n0.3.0\n")
        .assert()
        .success()
        .stdout(predicate::str::contains("Target version: 0.3.0-beta.0"));

    Ok(())
}

#[test]
fn interactive_prerelease_rejects_a_lower_custom_base_by_default(
) -> Result<(), Box<dyn std::error::Error>> {
    let repo = TempDir::new()?;
    write_release_fixture(repo.path())?;

    Command::cargo_bin("verso")?
        .current_dir(repo.path())
        .args(["--dry-run"])
        .write_stdin("beta\ncustom\n0.0.9\n\n")
        .assert()
        .failure()
        .stdout(predicate::str::contains(
            "Target version is not greater than current version. Continue? [y/N]",
        ))
        .stderr(predicate::str::contains("release aborted"));

    Ok(())
}

fn init_repo(path: &Path) -> Result<(), Box<dyn std::error::Error>> {
    git(path, &["init"])?;
    git(path, &["config", "user.email", "test@example.com"])?;
    git(path, &["config", "user.name", "Test User"])?;
    git(path, &["config", "commit.gpgSign", "false"])?;
    git(path, &["config", "tag.gpgSign", "false"])?;
    Ok(())
}

fn write_file(path: &Path, contents: &str) -> Result<(), Box<dyn std::error::Error>> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, contents)?;
    Ok(())
}

fn write_pnpm_workspace_with_missing_version_fixture(
    path: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
    write_file(
        &path.join("package.json"),
        "{\n  \"name\": \"root\",\n  \"version\": \"0.1.0\"\n}\n",
    )?;
    write_file(
        &path.join("docs/package.json"),
        "{\n  \"name\": \"docs\"\n}\n",
    )?;
    write_file(&path.join("pnpm-workspace.yaml"), "packages:\n  - docs\n")?;
    Ok(())
}

fn write_release_fixture(path: &Path) -> Result<(), Box<dyn std::error::Error>> {
    init_repo(path)?;
    write_file(
        &path.join("package.json"),
        "{\n  \"name\": \"root\",\n  \"version\": \"0.1.0\"\n}\n",
    )?;
    write_file(
        &path.join("packages/a/package.json"),
        "{\n  \"name\": \"a\",\n  \"version\": \"0.1.0\"\n}\n",
    )?;
    write_file(
        &path.join("verso.toml"),
        r#"
[workspaces]
patterns = ["packages/*"]
include_root = true

[changelog]
enabled = true
"#,
    )?;
    write_file(&path.join("CHANGELOG.md"), "# Changelog\n")?;

    git(path, &["add", "."])?;
    git(path, &["commit", "-m", "chore: initial release"])?;
    git(path, &["tag", "v0.1.0"])?;
    write_file(&path.join("feature.md"), "feature\n")?;
    git(path, &["add", "feature.md"])?;
    git(path, &["commit", "-m", "feat: add feature (#1)"])?;

    let remote = path.join(".git/release-test-remote.git");
    let remote = remote.to_str().ok_or("non-UTF-8 path")?;
    git(path, &["init", "--bare", remote])?;
    git(path, &["remote", "add", "origin", remote])?;
    git(path, &["push", "-u", "origin", "HEAD"])?;
    fs::remove_dir_all(remote)?;

    Ok(())
}

fn git(path: &Path, args: &[&str]) -> Result<(), Box<dyn std::error::Error>> {
    let output = ProcessCommand::new("git")
        .args(args)
        .current_dir(path)
        .output()?;

    if output.status.success() {
        Ok(())
    } else {
        Err(format!(
            "git {} failed: {}",
            args.join(" "),
            String::from_utf8_lossy(&output.stderr)
        )
        .into())
    }
}

fn git_stdout(path: &Path, args: &[&str]) -> Result<String, Box<dyn std::error::Error>> {
    let output = ProcessCommand::new("git")
        .args(args)
        .current_dir(path)
        .output()?;

    if output.status.success() {
        Ok(String::from_utf8(output.stdout)?)
    } else {
        Err(format!(
            "git {} failed: {}",
            args.join(" "),
            String::from_utf8_lossy(&output.stderr)
        )
        .into())
    }
}
