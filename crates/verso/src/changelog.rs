use crate::git;
use chrono::Local;
use regex::Regex;
use std::collections::BTreeMap;
use std::path::Path;
use std::sync::LazyLock;

static COMMIT_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"^(?P<kind>[A-Za-z][A-Za-z0-9-]*)(?:\((?P<scope>[^)]+)\))?(?P<breaking>!)?: (?P<title>.+)$",
    )
    .expect("conventional commit regex should compile")
});

static PULL_REQUEST_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^(?P<title>.*?)\s+\(#(?P<number>[0-9]+)\)$").expect("PR regex should compile")
});

static BREAKING_FOOTER_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?m)^BREAKING(?: CHANGE|-CHANGE):\s+.+$")
        .expect("breaking footer regex should compile")
});

static GITHUB_SSH_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^git@github\.com:(?P<slug>[^/]+/[^/]+?)(?:\.git)?/?$")
        .expect("GitHub SSH remote regex should compile")
});

static GITHUB_SSH_URL_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^ssh://git@github\.com/(?P<slug>[^/]+/[^/]+?)(?:\.git)?/?$")
        .expect("GitHub SSH URL remote regex should compile")
});

static GITHUB_HTTPS_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^https://github\.com/(?P<slug>[^/]+/[^/]+?)(?:\.git)?/?$")
        .expect("GitHub HTTPS remote regex should compile")
});

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommitEntry {
    pub sha: String,
    pub subject: String,
    pub kind: ChangeKind,
    pub scope: Option<String>,
    pub pull_request: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum ChangeKind {
    Fix,
    Feature,
    Performance,
    Breaking,
    Other(String),
}

pub fn commits_since(root: &Path, previous_tag: Option<&str>) -> Result<Vec<CommitEntry>, String> {
    let range = previous_tag.map_or_else(|| "HEAD".to_string(), |tag| format!("{tag}..HEAD"));
    let args = [
        "log",
        "--format=%H%x1f%s%x1f%b%x1e",
        "--no-merges",
        range.as_str(),
    ];
    let output = git::git(root, &args)?;

    Ok(output
        .stdout
        .split('\x1e')
        .filter_map(|record| {
            let record = record.trim_matches('\n');
            if record.is_empty() {
                return None;
            }

            let mut fields = record.splitn(3, '\x1f');
            let sha = fields.next()?;
            let subject = fields.next()?;
            let body = fields.next().map_or("", |body| body);
            parse_commit_with_body(sha, subject, body)
        })
        .collect())
}

pub fn parse_commit(sha: &str, subject: &str) -> Option<CommitEntry> {
    parse_commit_with_body(sha, subject, "")
}

pub fn parse_commit_with_body(sha: &str, subject: &str, body: &str) -> Option<CommitEntry> {
    let captures = COMMIT_REGEX.captures(subject)?;
    let commit_type = captures.name("kind")?.as_str();
    let breaking = captures.name("breaking").is_some() || BREAKING_FOOTER_REGEX.is_match(body);
    let raw_title = captures.name("title")?.as_str();
    let (title, pull_request) =
        if let Some(pull_request_captures) = PULL_REQUEST_REGEX.captures(raw_title) {
            let title = pull_request_captures.name("title")?.as_str().to_string();
            let pull_request = pull_request_captures
                .name("number")
                .and_then(|number| number.as_str().parse().ok());
            (title, pull_request)
        } else {
            (raw_title.to_string(), None)
        };

    let kind = if breaking {
        ChangeKind::Breaking
    } else {
        match commit_type {
            "feat" => ChangeKind::Feature,
            "fix" => ChangeKind::Fix,
            "perf" => ChangeKind::Performance,
            other => ChangeKind::Other(other.to_string()),
        }
    };

    Some(CommitEntry {
        sha: sha.to_string(),
        subject: title,
        kind,
        scope: captures
            .name("scope")
            .map(|scope| scope.as_str().to_string()),
        pull_request,
    })
}

pub fn render_changelog_entry(
    version: &str,
    previous_tag: Option<&str>,
    tag: &str,
    commits: &[CommitEntry],
    repo_slug: Option<&str>,
) -> String {
    let mut output = String::new();
    let date = Local::now().format("%Y-%m-%d");

    if let (Some(previous_tag), Some(repo_slug)) = (previous_tag, repo_slug) {
        output.push_str(&format!(
            "## [{version}](https://github.com/{repo_slug}/compare/{previous_tag}...{tag}) ({date})\n\n"
        ));
    } else {
        output.push_str(&format!("## {version} ({date})\n\n"));
    }

    let mut rendered_any = false;
    for (kind, heading) in section_order(commits) {
        let section_commits: Vec<&CommitEntry> = commits
            .iter()
            .filter(|commit| commit.kind == kind)
            .collect();
        if section_commits.is_empty() {
            continue;
        }

        rendered_any = true;
        output.push_str(&format!("### {heading}\n\n"));
        for commit in section_commits {
            output.push_str(&render_commit_line(commit, repo_slug));
            output.push('\n');
        }
        output.push('\n');
    }

    if !rendered_any {
        output.push_str("No classifiable changes.\n");
    }

    output
}

pub fn infer_github_slug(remote: &str) -> Option<String> {
    GITHUB_SSH_REGEX
        .captures(remote)
        .or_else(|| GITHUB_SSH_URL_REGEX.captures(remote))
        .or_else(|| GITHUB_HTTPS_REGEX.captures(remote))
        .and_then(|captures| captures.name("slug"))
        .map(|slug| slug.as_str().to_string())
}

fn section_order(commits: &[CommitEntry]) -> Vec<(ChangeKind, String)> {
    let mut other_kinds = BTreeMap::new();
    for commit in commits {
        if let ChangeKind::Other(kind) = &commit.kind {
            other_kinds.insert(kind.clone(), format!("Other Changes ({kind})"));
        }
    }

    let mut sections = vec![
        (ChangeKind::Fix, "Bug Fixes".to_string()),
        (ChangeKind::Feature, "Features".to_string()),
        (
            ChangeKind::Performance,
            "Performance Improvements".to_string(),
        ),
        (ChangeKind::Breaking, "BREAKING CHANGES".to_string()),
    ];
    sections.extend(
        other_kinds
            .into_iter()
            .map(|(kind, heading)| (ChangeKind::Other(kind), heading)),
    );
    sections
}

fn render_commit_line(commit: &CommitEntry, repo_slug: Option<&str>) -> String {
    let mut line = String::from("* ");
    if let Some(scope) = &commit.scope {
        line.push_str(&format!("**{scope}:** "));
    }
    line.push_str(&commit.subject);

    if let Some(pull_request) = commit.pull_request {
        if let Some(repo_slug) = repo_slug {
            line.push_str(&format!(
                " ([#{pull_request}](https://github.com/{repo_slug}/issues/{pull_request}))"
            ));
        } else {
            line.push_str(&format!(" (#{pull_request})"));
        }
    }

    let short_sha: String = commit.sha.chars().take(7).collect();
    if let Some(repo_slug) = repo_slug {
        line.push_str(&format!(
            " ([{short_sha}](https://github.com/{repo_slug}/commit/{})",
            commit.sha
        ));
        line.push(')');
    } else {
        line.push_str(&format!(" ({short_sha})"));
    }

    line
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_angular_commit_with_scope_and_pr() {
        let entry = parse_commit("abc123", "fix(parser): handle nested scopes (#42)")
            .expect("conventional fix should parse");

        assert_eq!(entry.sha, "abc123");
        assert_eq!(entry.subject, "handle nested scopes");
        assert_eq!(entry.kind, ChangeKind::Fix);
        assert_eq!(entry.scope, Some("parser".to_string()));
        assert_eq!(entry.pull_request, Some(42));
    }

    #[test]
    fn ignores_non_conventional_commit() {
        assert_eq!(parse_commit("abc123", "update dependencies"), None);
    }

    #[test]
    fn parses_performance_and_breaking_commits() {
        let performance = parse_commit("def456", "perf(runtime): cache package manifests")
            .expect("perf commit should parse");
        let breaking = parse_commit("fedcba", "feat(api)!: remove deprecated release flag (#7)")
            .expect("breaking commit should parse");

        assert_eq!(performance.kind, ChangeKind::Performance);
        assert_eq!(breaking.kind, ChangeKind::Breaking);
        assert_eq!(breaking.pull_request, Some(7));
    }

    #[test]
    fn parses_breaking_footer_without_subject_bang() {
        let breaking = parse_commit_with_body(
            "abc123",
            "feat(api): remove release flag",
            "The old flag is gone.\n\nBREAKING CHANGE: use --channel instead.",
        )
        .expect("breaking footer should classify commit");

        assert_eq!(breaking.kind, ChangeKind::Breaking);
    }

    #[test]
    fn infers_github_slug_from_ssh_and_https() {
        assert_eq!(
            infer_github_slug("git@github.com:owner/repo.git"),
            Some("owner/repo".to_string())
        );
        assert_eq!(
            infer_github_slug("https://github.com/owner/repo.git"),
            Some("owner/repo".to_string())
        );
        assert_eq!(
            infer_github_slug("ssh://git@github.com/owner/repo.git"),
            Some("owner/repo".to_string())
        );
    }

    #[test]
    fn infers_github_slug_without_git_suffix() {
        assert_eq!(
            infer_github_slug("https://github.com/owner/repo"),
            Some("owner/repo".to_string())
        );
        assert_eq!(
            infer_github_slug("git@github.com:owner/repo"),
            Some("owner/repo".to_string())
        );
    }

    #[test]
    fn renders_compare_heading_date_sections_issue_and_commit_links() {
        let commits = vec![
            CommitEntry {
                sha: "abc1234".to_string(),
                subject: "handle nested scopes".to_string(),
                kind: ChangeKind::Fix,
                scope: Some("parser".to_string()),
                pull_request: Some(42),
            },
            CommitEntry {
                sha: "def4567".to_string(),
                subject: "add release notes".to_string(),
                kind: ChangeKind::Feature,
                scope: None,
                pull_request: None,
            },
        ];

        let rendered = render_changelog_entry(
            "0.25.0",
            Some("v0.24.0"),
            "v0.25.0",
            &commits,
            Some("jikkai/verso"),
        );

        assert!(
            Regex::new(
                r"(?m)^## \[0\.25\.0\]\(https://github\.com/jikkai/verso/compare/v0\.24\.0\.\.\.v0\.25\.0\) \([0-9]{4}-[0-9]{2}-[0-9]{2}\)$"
            )
            .expect("date heading regex should compile")
            .is_match(&rendered)
        );
        let bug_fixes_index = rendered
            .find("### Bug Fixes")
            .expect("Bug Fixes heading should render");
        let features_index = rendered
            .find("### Features")
            .expect("Features heading should render");
        assert!(bug_fixes_index < features_index);
        assert!(rendered.contains("* **parser:** handle nested scopes ([#42](https://github.com/jikkai/verso/issues/42)) ([abc1234](https://github.com/jikkai/verso/commit/abc1234))"));
    }

    #[test]
    fn renders_performance_and_breaking_sections() {
        let commits = vec![
            CommitEntry {
                sha: "def4567".to_string(),
                subject: "cache package manifests".to_string(),
                kind: ChangeKind::Performance,
                scope: Some("runtime".to_string()),
                pull_request: None,
            },
            CommitEntry {
                sha: "fedcba9".to_string(),
                subject: "remove deprecated release flag".to_string(),
                kind: ChangeKind::Breaking,
                scope: None,
                pull_request: None,
            },
        ];

        let rendered =
            render_changelog_entry("2.0.0", None, "v2.0.0", &commits, Some("owner/repo"));

        assert!(
            Regex::new(r"(?m)^## 2\.0\.0 \([0-9]{4}-[0-9]{2}-[0-9]{2}\)$")
                .expect("date heading regex should compile")
                .is_match(&rendered)
        );
        assert!(rendered.contains("### Performance Improvements"));
        assert!(rendered.contains("### BREAKING CHANGES"));
    }

    #[test]
    fn renders_empty_changelog_message() {
        let rendered = render_changelog_entry("1.2.3", None, "v1.2.3", &[], None);

        assert!(
            Regex::new(r"(?m)^## 1\.2\.3 \([0-9]{4}-[0-9]{2}-[0-9]{2}\)$")
                .expect("date heading regex should compile")
                .is_match(&rendered)
        );
        assert!(rendered.contains("No classifiable changes."));
    }
}
