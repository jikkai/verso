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
        .stdout(predicate::str::contains("git tag -a 'v0.2.0' -m 'v0.2.0'"))
        .stdout(predicate::str::contains("git push --atomic"));

    assert_eq!(
        fs::read_to_string(repo.path().join("package.json"))?,
        root_package
    );

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
            "git add 'CHANGELOG.md' 'package.json' 'packages/a/package.yaml'",
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
fn dry_run_omits_changelog_when_disabled() -> Result<(), Box<dyn std::error::Error>> {
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
infile = "ignored/CHANGELOG.md"
"#,
    )?;
    write_file(&repo.path().join(".gitignore"), "ignored/\n")?;
    git(repo.path(), &["add", "verso.toml", ".gitignore"])?;
    git(
        repo.path(),
        &["commit", "-m", "test: ignored changelog path"],
    )?;

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
