use semver::Version;
use serde::{Deserialize, Serialize};
use std::{
    io::Write,
    path::Path,
    process::{Command, Stdio},
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GitOutput {
    pub stdout: String,
    pub stderr: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReleaseUpstream {
    pub remote: String,
    pub branch: String,
    pub push_url: String,
    pub branch_target: String,
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
    let branch_target = git(root, &["rev-parse", &upstream])?
        .stdout
        .trim()
        .to_string();

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
    let push_url = git(root, &["remote", "get-url", "--push", &remote])?
        .stdout
        .trim()
        .to_string();
    if push_url.is_empty() {
        return Err(format!("remote {remote} has no push URL"));
    }
    Ok(ReleaseUpstream {
        remote,
        branch,
        push_url,
        branch_target,
    })
}

pub fn push_release(
    root: &Path,
    upstream: &ReleaseUpstream,
    revision: &str,
    tag_revision: &str,
    tag: &str,
) -> Result<(), String> {
    let branch_ref = format!("{revision}:refs/heads/{}", upstream.branch);
    let tag_ref = format!("{tag_revision}:refs/tags/{tag}");
    git(
        root,
        &[
            "push",
            "--atomic",
            "--",
            &upstream.push_url,
            &branch_ref,
            &tag_ref,
        ],
    )?;
    Ok(())
}

pub fn remote_branch_contains(
    root: &Path,
    upstream: &ReleaseUpstream,
    revision: &str,
    branch_target: &str,
) -> Result<bool, String> {
    let branch_ref = format!("refs/heads/{}", upstream.branch);
    git(
        root,
        &[
            "fetch",
            "--quiet",
            "--no-tags",
            "--no-write-fetch-head",
            "--",
            &upstream.push_url,
            &branch_ref,
        ],
    )?;
    is_ancestor(root, revision, branch_target)
}

fn is_ancestor(root: &Path, ancestor: &str, descendant: &str) -> Result<bool, String> {
    let args = ["merge-base", "--is-ancestor", ancestor, descendant];
    let output = Command::new("git")
        .args(args)
        .current_dir(root)
        .output()
        .map_err(|error| format!("failed to run git {}: {error}", args.join(" ")))?;
    match output.status.code() {
        Some(0) => Ok(true),
        Some(1) => Ok(false),
        code => Err(format_git_failure(
            &args,
            &code.map_or_else(
                || "terminated by signal".to_string(),
                |code| code.to_string(),
            ),
            &String::from_utf8_lossy(&output.stdout),
            &String::from_utf8_lossy(&output.stderr),
        )),
    }
}

pub fn hash_object(root: &Path, path: &str, contents: &[u8]) -> Result<String, String> {
    let path_arg = format!("--path={path}");
    Ok(
        git_with_stdin(root, &["hash-object", &path_arg, "--stdin"], contents)?
            .stdout
            .trim()
            .to_owned(),
    )
}

pub fn create_annotated_tag_object(root: &Path, tag: &str, target: &str) -> Result<String, String> {
    let identity = git(root, &["var", "GIT_COMMITTER_IDENT"])?.stdout;
    let input = format!(
        "object {target}\ntype commit\ntag {tag}\ntagger {}\n\n{tag}\n",
        identity.trim()
    );
    Ok(git_with_stdin(root, &["mktag"], input.as_bytes())?
        .stdout
        .trim()
        .to_owned())
}

pub fn create_tag_ref(root: &Path, tag: &str, object: &str) -> Result<(), String> {
    let reference = format!("refs/tags/{tag}");
    git(root, &["update-ref", &reference, object, ""])?;
    Ok(())
}

pub fn delete_tag_ref(root: &Path, tag: &str, expected_object: &str) -> Result<(), String> {
    let reference = format!("refs/tags/{tag}");
    git(root, &["update-ref", "-d", &reference, expected_object])?;
    Ok(())
}

pub fn compare_and_swap_head(root: &Path, revision: &str, expected: &str) -> Result<(), String> {
    git(root, &["update-ref", "HEAD", revision, expected])?;
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

fn git_with_stdin(root: &Path, args: &[&str], input: &[u8]) -> Result<GitOutput, String> {
    let mut child = Command::new("git")
        .args(args)
        .current_dir(root)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|error| format!("failed to run git {}: {error}", args.join(" ")))?;
    if let Err(error) = child
        .stdin
        .take()
        .ok_or_else(|| "failed to open git stdin".to_string())?
        .write_all(input)
    {
        let _ = child.kill();
        let _ = child.wait();
        return Err(format!(
            "failed to write git {} input: {error}",
            args.join(" ")
        ));
    }
    let output = child
        .wait_with_output()
        .map_err(|error| format!("failed to wait for git {}: {error}", args.join(" ")))?;
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
    fn head_update_ref_refuses_to_rewind_an_unexpected_commit() -> Result<(), String> {
        let repo = init_repo()?;
        git(repo.path(), &["config", "user.email", "test@example.com"])?;
        git(repo.path(), &["config", "user.name", "Test User"])?;
        std::fs::write(repo.path().join("README.md"), "first")
            .map_err(|error| error.to_string())?;
        git(repo.path(), &["add", "README.md"])?;
        git(repo.path(), &["commit", "-m", "first"])?;
        let first = current_head(repo.path())?;
        std::fs::write(repo.path().join("README.md"), "second")
            .map_err(|error| error.to_string())?;
        git(repo.path(), &["commit", "-am", "second"])?;
        let second = current_head(repo.path())?;

        assert!(compare_and_swap_head(repo.path(), &first, &first).is_err());
        assert_eq!(current_head(repo.path())?, second);
        Ok(())
    }

    #[test]
    fn push_release_pushes_only_the_requested_tag() -> Result<(), String> {
        let repo = init_repo()?;
        let remote = TempDir::new().map_err(|error| error.to_string())?;
        let other_remote = TempDir::new().map_err(|error| error.to_string())?;
        git(remote.path(), &["init", "--bare"])?;
        git(other_remote.path(), &["init", "--bare"])?;
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
        let tag_object = git(repo.path(), &["rev-parse", "refs/tags/v1.0.0"])?
            .stdout
            .trim()
            .to_owned();
        let other_remote_path = other_remote.path().to_string_lossy();
        git(
            repo.path(),
            &["remote", "set-url", "origin", &other_remote_path],
        )?;
        push_release(repo.path(), &upstream, "HEAD", &tag_object, "v1.0.0")?;

        let remote_tags = git(repo.path(), &["ls-remote", "--tags", &upstream.push_url])?.stdout;
        assert!(remote_tags.contains("refs/tags/v1.0.0"));
        assert!(!remote_tags.contains("refs/tags/unrelated"));
        assert!(git(repo.path(), &["ls-remote", "--tags", "origin"])?
            .stdout
            .is_empty());
        Ok(())
    }

    #[test]
    fn remote_branch_must_contain_the_release_commit() -> Result<(), String> {
        let repo = init_repo()?;
        let remote = TempDir::new().map_err(|error| error.to_string())?;
        git(remote.path(), &["init", "--bare"])?;
        git(repo.path(), &["config", "user.email", "test@example.com"])?;
        git(repo.path(), &["config", "user.name", "Test User"])?;
        std::fs::write(repo.path().join("README.md"), "base").map_err(|error| error.to_string())?;
        git(repo.path(), &["add", "README.md"])?;
        git(repo.path(), &["commit", "-m", "base"])?;
        let base = current_head(repo.path())?;
        let remote_path = remote.path().to_string_lossy();
        git(repo.path(), &["remote", "add", "origin", &remote_path])?;
        git(repo.path(), &["push", "-u", "origin", "HEAD"])?;
        let upstream = release_upstream(repo.path())?;

        std::fs::write(repo.path().join("README.md"), "release")
            .map_err(|error| error.to_string())?;
        git(repo.path(), &["commit", "-am", "release"])?;
        let release = current_head(repo.path())?;
        let branch_ref = format!("{release}:refs/heads/{}", upstream.branch);
        git(repo.path(), &["push", &upstream.push_url, &branch_ref])?;
        assert!(remote_branch_contains(
            repo.path(),
            &upstream,
            &release,
            &release
        )?);

        let base_ref = format!("{base}:refs/heads/{}", upstream.branch);
        git(
            repo.path(),
            &["push", "--force", &upstream.push_url, &base_ref],
        )?;
        assert!(!remote_branch_contains(
            repo.path(),
            &upstream,
            &release,
            &base
        )?);
        Ok(())
    }
}
