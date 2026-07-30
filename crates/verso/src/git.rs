use semver::Version;
use std::path::Path;
use std::process::Command;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GitOutput {
    pub stdout: String,
    pub stderr: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReleaseUpstream {
    pub remote: String,
    pub branch: String,
}

pub fn git(root: &Path, args: &[&str]) -> Result<GitOutput, String> {
    let output = Command::new("git")
        .args(args)
        .current_dir(root)
        .output()
        .map_err(|error| format!("failed to run git {}: {error}", args.join(" ")))?;

    let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
    let stderr = String::from_utf8_lossy(&output.stderr).into_owned();

    if output.status.success() {
        Ok(GitOutput { stdout, stderr })
    } else {
        let status = output.status.code().map_or_else(
            || "terminated by signal".to_string(),
            |code| code.to_string(),
        );
        Err(format_git_failure(args, &status, &stdout, &stderr))
    }
}

fn format_git_failure(args: &[&str], status: &str, stdout: &str, stderr: &str) -> String {
    let stdout = stdout.trim();
    let stderr = stderr.trim();
    let details = match (stdout.is_empty(), stderr.is_empty()) {
        (true, true) => "no output".to_string(),
        (true, false) => stderr.to_string(),
        (false, true) => stdout.to_string(),
        (false, false) => format!("stdout:\n{stdout}\nstderr:\n{stderr}"),
    };

    format!(
        "git {} failed with status {status}: {details}",
        args.join(" ")
    )
}

pub fn is_worktree_clean(root: &Path) -> Result<bool, String> {
    let output = git(root, &["status", "--porcelain"])?;
    Ok(output.stdout.trim().is_empty())
}

pub fn is_index_clean(root: &Path) -> Result<bool, String> {
    let output = git(root, &["diff", "--cached", "--name-only"])?;
    Ok(output.stdout.trim().is_empty())
}

pub fn are_paths_clean(root: &Path, paths: &[String]) -> Result<bool, String> {
    if paths.is_empty() {
        return Ok(true);
    }

    let mut args = vec![
        "status".to_string(),
        "--porcelain".to_string(),
        "--".to_string(),
    ];
    args.extend(paths.iter().cloned());
    let arg_refs = args.iter().map(String::as_str).collect::<Vec<_>>();
    let output = git(root, &arg_refs)?;
    Ok(output.stdout.trim().is_empty())
}

pub fn tag_exists(root: &Path, tag: &str) -> Result<bool, String> {
    let output = git(root, &["tag", "--list", tag])?;
    Ok(output.stdout.lines().any(|line| line == tag))
}

pub fn current_head(root: &Path) -> Result<String, String> {
    let output = git(root, &["rev-parse", "HEAD"])?;
    Ok(output.stdout.trim().to_string())
}

pub fn release_upstream(root: &Path) -> Result<ReleaseUpstream, String> {
    let branch = git(root, &["symbolic-ref", "--quiet", "--short", "HEAD"])
        .map_err(|_error| {
            "release requires a named branch; detached HEAD is not supported".to_string()
        })?
        .stdout
        .trim()
        .to_string();
    let upstream = git(
        root,
        &[
            "rev-parse",
            "--abbrev-ref",
            "--symbolic-full-name",
            "@{upstream}",
        ],
    )
    .map_err(|_error| format!("branch {branch} has no upstream"))?
    .stdout
    .trim()
    .to_string();
    let divergence = git(
        root,
        &["rev-list", "--left-right", "--count", "HEAD...@{upstream}"],
    )?
    .stdout;
    let mut counts = divergence.split_whitespace();
    let _ahead = counts
        .next()
        .and_then(|count| count.parse::<usize>().ok())
        .ok_or_else(|| "failed to read upstream divergence".to_string())?;
    let behind = counts
        .next()
        .and_then(|count| count.parse::<usize>().ok())
        .ok_or_else(|| "failed to read upstream divergence".to_string())?;
    if behind > 0 {
        return Err(format!(
            "branch {branch} is behind {upstream} by {behind} commit(s); update it before releasing"
        ));
    }

    let remote_key = format!("branch.{branch}.remote");
    let merge_key = format!("branch.{branch}.merge");
    let remote = git(root, &["config", "--get", &remote_key])?
        .stdout
        .trim()
        .to_string();
    let branch = git(root, &["config", "--get", &merge_key])?
        .stdout
        .trim()
        .strip_prefix("refs/heads/")
        .ok_or_else(|| format!("{merge_key} must name a remote branch"))?
        .to_string();

    Ok(ReleaseUpstream { remote, branch })
}

pub fn push_release(root: &Path, upstream: &ReleaseUpstream, tag: &str) -> Result<(), String> {
    let branch_ref = format!("HEAD:refs/heads/{}", upstream.branch);
    let tag_ref = format!("refs/tags/{tag}:refs/tags/{tag}");
    git(
        root,
        &["push", "--atomic", &upstream.remote, &branch_ref, &tag_ref],
    )?;
    Ok(())
}

pub fn reset_soft(root: &Path, revision: &str) -> Result<(), String> {
    git(root, &["reset", "--soft", revision])?;
    Ok(())
}

pub fn unstage_paths(root: &Path, paths: &[String]) -> Result<(), String> {
    if paths.is_empty() {
        return Ok(());
    }

    let mut args = vec!["reset".to_string(), "--".to_string()];
    args.extend(paths.iter().cloned());
    let arg_refs = args.iter().map(String::as_str).collect::<Vec<_>>();

    git(root, &arg_refs)?;
    Ok(())
}

pub fn delete_tag(root: &Path, tag: &str) -> Result<(), String> {
    git(root, &["tag", "-d", tag])?;
    Ok(())
}

pub fn latest_matching_tag(root: &Path, prefix: &str) -> Result<Option<String>, String> {
    let pattern = format!("{prefix}*");
    let output = git(root, &["tag", "--merged", "HEAD", "--list", &pattern])?;
    Ok(output
        .stdout
        .lines()
        .map(str::trim)
        .filter(|tag| !tag.is_empty())
        .filter_map(|tag| {
            let version = tag.strip_prefix(prefix).and_then(|version| {
                Version::parse(version)
                    .map_err(|_error| ())
                    .ok()
                    .map(|version| (version, tag.to_string()))
            })?;
            Some(version)
        })
        .max_by(|(left_version, _), (right_version, _)| left_version.cmp(right_version))
        .map(|(_version, tag)| tag))
}

pub fn remote_origin_url(root: &Path) -> Option<String> {
    git(root, &["remote", "get-url", "origin"])
        .ok()
        .and_then(|output| {
            let remote = output.stdout.trim();
            (!remote.is_empty()).then(|| remote.to_string())
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn init_repo() -> Result<TempDir, String> {
        let repo = TempDir::new().map_err(|error| error.to_string())?;
        git(repo.path(), &["init"])?;
        git(repo.path(), &["config", "commit.gpgSign", "false"])?;
        git(repo.path(), &["config", "tag.gpgSign", "false"])?;
        Ok(repo)
    }

    #[test]
    fn detects_clean_and_dirty_worktree() -> Result<(), String> {
        let repo = init_repo()?;

        assert!(is_worktree_clean(repo.path())?);

        std::fs::write(repo.path().join("README.md"), "hello")
            .map_err(|error| error.to_string())?;

        assert!(!is_worktree_clean(repo.path())?);
        Ok(())
    }

    #[test]
    fn finds_tags_by_exact_name_and_prefix() -> Result<(), String> {
        let repo = init_repo()?;
        git(repo.path(), &["config", "user.email", "test@example.com"])?;
        git(repo.path(), &["config", "user.name", "Test User"])?;
        std::fs::write(repo.path().join("README.md"), "hello")
            .map_err(|error| error.to_string())?;
        git(repo.path(), &["add", "README.md"])?;
        git(repo.path(), &["commit", "-m", "feat: initial"])?;
        git(repo.path(), &["tag", "-a", "pkg-1.0.0", "-m", "pkg-1.0.0"])?;

        assert!(tag_exists(repo.path(), "pkg-1.0.0")?);
        assert!(!tag_exists(repo.path(), "pkg-2.0.0")?);
        assert_eq!(
            latest_matching_tag(repo.path(), "pkg-")?,
            Some("pkg-1.0.0".to_string())
        );
        assert_eq!(latest_matching_tag(repo.path(), "other-")?, None);
        Ok(())
    }

    #[test]
    fn latest_matching_tag_uses_highest_reachable_semver_not_creation_order() -> Result<(), String>
    {
        let repo = init_repo()?;
        git(repo.path(), &["config", "user.email", "test@example.com"])?;
        git(repo.path(), &["config", "user.name", "Test User"])?;
        std::fs::write(repo.path().join("README.md"), "hello")
            .map_err(|error| error.to_string())?;
        git(repo.path(), &["add", "README.md"])?;
        git(repo.path(), &["commit", "-m", "feat: initial"])?;
        git(repo.path(), &["tag", "v0.25.0"])?;
        git(repo.path(), &["tag", "v0.21.2-fix1"])?;
        git(repo.path(), &["tag", "vnot-semver"])?;

        git(repo.path(), &["checkout", "-b", "future-release"])?;
        std::fs::write(repo.path().join("future.md"), "future")
            .map_err(|error| error.to_string())?;
        git(repo.path(), &["add", "future.md"])?;
        git(repo.path(), &["commit", "-m", "feat: future"])?;
        git(repo.path(), &["tag", "v9.0.0"])?;
        git(repo.path(), &["checkout", "-"])?;

        assert_eq!(
            latest_matching_tag(repo.path(), "v")?,
            Some("v0.25.0".to_string())
        );
        Ok(())
    }

    #[test]
    fn reads_origin_remote_when_present() -> Result<(), String> {
        let repo = init_repo()?;

        assert_eq!(remote_origin_url(repo.path()), None);

        git(
            repo.path(),
            &["remote", "add", "origin", "git@github.com:owner/repo.git"],
        )?;

        assert_eq!(
            remote_origin_url(repo.path()),
            Some("git@github.com:owner/repo.git".to_string())
        );
        Ok(())
    }

    #[test]
    fn failed_git_output_includes_stdout_and_stderr() {
        let message = format_git_failure(
            &["push", "--atomic"],
            "128",
            "stdout reason",
            "stderr reason",
        );

        assert!(message.contains("git push --atomic failed with status 128"));
        assert!(message.contains("stdout reason"));
        assert!(message.contains("stderr reason"));
    }

    #[test]
    fn failed_git_output_uses_stdout_when_stderr_is_empty() {
        let message = format_git_failure(&["push"], "1", "wsl stdout reason", "");

        assert!(message.contains("wsl stdout reason"));
    }

    #[test]
    fn push_release_pushes_only_the_requested_tag() -> Result<(), String> {
        let repo = init_repo()?;
        let remote = TempDir::new().map_err(|error| error.to_string())?;
        git(remote.path(), &["init", "--bare"])?;
        git(repo.path(), &["config", "user.email", "test@example.com"])?;
        git(repo.path(), &["config", "user.name", "Test User"])?;
        std::fs::write(repo.path().join("README.md"), "hello")
            .map_err(|error| error.to_string())?;
        git(repo.path(), &["add", "README.md"])?;
        git(repo.path(), &["commit", "-m", "feat: initial"])?;
        let remote_path = remote.path().to_string_lossy();
        git(repo.path(), &["remote", "add", "origin", &remote_path])?;
        git(repo.path(), &["push", "-u", "origin", "HEAD"])?;
        git(repo.path(), &["tag", "-a", "v1.0.0", "-m", "v1.0.0"])?;
        git(repo.path(), &["tag", "-a", "unrelated", "-m", "unrelated"])?;

        let upstream = release_upstream(repo.path())?;
        push_release(repo.path(), &upstream, "v1.0.0")?;

        let remote_tags = git(repo.path(), &["ls-remote", "--tags", "origin"])?.stdout;
        assert!(remote_tags.contains("refs/tags/v1.0.0"));
        assert!(!remote_tags.contains("refs/tags/unrelated"));
        Ok(())
    }
}
