use crate::git;
use chrono::Local;
use regex::Regex;
use std::collections::BTreeMap;
use std::path::Path;
use std::str::FromStr;
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChangelogPreset {
    Angular,
    KeepAChangelog,
}

impl ChangelogPreset {
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::Angular => "angular",
            Self::KeepAChangelog => "keep-a-changelog",
        }
    }
}

impl FromStr for ChangelogPreset {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "angular" => Ok(Self::Angular),
            "keep-a-changelog" => Ok(Self::KeepAChangelog),
            _ => Err(format!(
                "unsupported changelog preset {value:?}; expected \"angular\" or \"keep-a-changelog\""
            )),
        }
    }
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
    preset: ChangelogPreset,
    version: &str,
    previous_tag: Option<&str>,
    tag: &str,
    commits: &[CommitEntry],
    repo_slug: Option<&str>,
) -> String {
    match preset {
        ChangelogPreset::Angular => {
            render_angular_entry(version, previous_tag, tag, commits, repo_slug)
        }
        ChangelogPreset::KeepAChangelog => {
            render_keep_a_changelog_entry(version, previous_tag, tag, commits, repo_slug)
        }
    }
}

fn render_angular_entry(
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
            output.push_str(&render_commit_line(commit, repo_slug, "*"));
            output.push('\n');
        }
        output.push('\n');
    }

    if !rendered_any {
        output.push_str("No classifiable changes.\n");
    }

    output
}

fn render_keep_a_changelog_entry(
    version: &str,
    previous_tag: Option<&str>,
    tag: &str,
    commits: &[CommitEntry],
    repo_slug: Option<&str>,
) -> String {
    let mut output = format!("## [{version}] - {}\n\n", Local::now().format("%Y-%m-%d"));
    let mut rendered_any = false;

    for heading in ["Added", "Fixed", "Changed"] {
        let section_commits = commits
            .iter()
            .filter(|commit| keep_a_changelog_section(&commit.kind) == heading)
            .collect::<Vec<_>>();
        if section_commits.is_empty() {
            continue;
        }

        rendered_any = true;
        output.push_str(&format!("### {heading}\n\n"));
        for commit in section_commits {
            output.push_str(&render_commit_line(commit, repo_slug, "-"));
            output.push('\n');
        }
        output.push('\n');
    }

    if !rendered_any {
        output.push_str("No classifiable changes.\n");
    }

    if let Some(repo_slug) = repo_slug {
        output.push_str(&format!(
            "\n[Unreleased]: https://github.com/{repo_slug}/compare/{tag}...HEAD\n"
        ));
        if let Some(previous_tag) = previous_tag {
            output.push_str(&format!(
                "[{version}]: https://github.com/{repo_slug}/compare/{previous_tag}...{tag}\n"
            ));
        } else {
            output.push_str(&format!(
                "[{version}]: https://github.com/{repo_slug}/releases/tag/{tag}\n"
            ));
        }
    }

    output
}

fn keep_a_changelog_section(kind: &ChangeKind) -> &'static str {
    match kind {
        ChangeKind::Feature => "Added",
        ChangeKind::Fix => "Fixed",
        ChangeKind::Performance | ChangeKind::Breaking | ChangeKind::Other(_) => "Changed",
    }
}

pub fn insert_changelog_entry(preset: ChangelogPreset, existing: &str, entry: &str) -> String {
    match preset {
        ChangelogPreset::Angular => insert_angular_entry(existing, entry),
        ChangelogPreset::KeepAChangelog => insert_keep_a_changelog_entry(existing, entry),
    }
}

fn insert_angular_entry(existing: &str, entry: &str) -> String {
    let entry = entry.trim_end();
    let (first_line, body) = existing
        .split_once('\n')
        .map_or((existing, ""), |(first_line, body)| (first_line, body));

    if first_line.trim_end() == "# Changelog" {
        let body = body
            .strip_prefix('\r')
            .unwrap_or(body)
            .trim_start_matches('\n');

        if body.is_empty() {
            format!("# Changelog\n\n{entry}\n")
        } else {
            format!("# Changelog\n\n{entry}\n\n{body}")
        }
    } else if existing.trim().is_empty() {
        format!("# Changelog\n\n{entry}\n")
    } else {
        format!("# Changelog\n\n{entry}\n\n{existing}")
    }
}

fn insert_keep_a_changelog_entry(existing: &str, entry: &str) -> String {
    let (entry_body, entry_references) = split_reference_definitions(entry);
    let existing = if existing.trim().is_empty() {
        "# Changelog\n\n## [Unreleased]"
    } else {
        existing.trim_end()
    };
    let (body, existing_references) = split_reference_definitions(existing);
    let mut body = body.to_string();

    if let Some(unreleased_start) = heading_start(&body, "## [Unreleased]") {
        let after_heading = unreleased_start + "## [Unreleased]".len();
        let insertion = next_level_two_heading(&body, after_heading).unwrap_or(body.len());
        let before = body[..insertion].trim_end();
        let after = body[insertion..].trim_start();
        body = if after.is_empty() {
            format!("{before}\n\n{}", entry_body.trim())
        } else {
            format!("{before}\n\n{}\n\n{after}", entry_body.trim())
        };
    } else if let Some(rest) = body.strip_prefix("# Changelog") {
        let rest = rest.trim_start();
        body = if rest.is_empty() {
            format!("# Changelog\n\n## [Unreleased]\n\n{}", entry_body.trim())
        } else {
            format!(
                "# Changelog\n\n## [Unreleased]\n\n{}\n\n{rest}",
                entry_body.trim()
            )
        };
    } else {
        body = format!(
            "# Changelog\n\n## [Unreleased]\n\n{}\n\n{}",
            entry_body.trim(),
            body.trim()
        );
    }

    let mut references = entry_references;
    for reference in existing_references {
        if !references
            .iter()
            .any(|new_reference| reference_label(new_reference) == reference_label(&reference))
        {
            references.push(reference);
        }
    }

    if references.is_empty() {
        format!("{}\n", body.trim_end())
    } else {
        format!("{}\n\n{}\n", body.trim_end(), references.join("\n"))
    }
}

fn split_reference_definitions(contents: &str) -> (&str, Vec<String>) {
    let mut split = contents.len();
    let mut references = Vec::new();
    let mut lines = Vec::new();
    let mut offset = 0;

    for segment in contents.split_inclusive('\n') {
        let line = segment.trim_end_matches(['\r', '\n']);
        lines.push((offset, line));
        offset += segment.len();
    }

    for (offset, line) in lines.into_iter().rev() {
        if line.trim().is_empty() {
            split = offset;
            continue;
        }
        if reference_label(line).is_some() {
            references.push(line.trim().to_string());
            split = offset;
            continue;
        }
        break;
    }

    references.reverse();
    (contents[..split].trim_end(), references)
}

fn reference_label(reference: &str) -> Option<&str> {
    reference
        .trim()
        .strip_prefix('[')?
        .split_once("]: ")
        .map(|(label, _)| label)
}

fn heading_start(contents: &str, heading: &str) -> Option<usize> {
    let mut offset = 0;
    for line in contents.split_inclusive('\n') {
        if line.trim_end_matches([' ', '\t', '\r', '\n']) == heading {
            return Some(offset);
        }
        offset += line.len();
    }
    None
}

fn next_level_two_heading(contents: &str, from: usize) -> Option<usize> {
    let tail = &contents[from..];
    tail.match_indices('\n').find_map(|(offset, _)| {
        let start = from + offset + 1;
        contents[start..].starts_with("## ").then_some(start)
    })
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

fn render_commit_line(commit: &CommitEntry, repo_slug: Option<&str>, bullet: &str) -> String {
    let mut line = format!("{bullet} ");
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
            ChangelogPreset::Angular,
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

        let rendered = render_changelog_entry(
            ChangelogPreset::Angular,
            "2.0.0",
            None,
            "v2.0.0",
            &commits,
            Some("owner/repo"),
        );

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
        let rendered =
            render_changelog_entry(ChangelogPreset::Angular, "1.2.3", None, "v1.2.3", &[], None);

        assert!(
            Regex::new(r"(?m)^## 1\.2\.3 \([0-9]{4}-[0-9]{2}-[0-9]{2}\)$")
                .expect("date heading regex should compile")
                .is_match(&rendered)
        );
        assert!(rendered.contains("No classifiable changes."));
    }

    #[test]
    fn parses_supported_changelog_presets() {
        assert_eq!(
            "angular".parse::<ChangelogPreset>(),
            Ok(ChangelogPreset::Angular)
        );
        assert_eq!(
            "keep-a-changelog".parse::<ChangelogPreset>(),
            Ok(ChangelogPreset::KeepAChangelog)
        );
        assert_eq!(ChangelogPreset::KeepAChangelog.as_str(), "keep-a-changelog");
        assert!("custom".parse::<ChangelogPreset>().is_err());
    }

    #[test]
    fn renders_keep_a_changelog_sections_and_compare_links() {
        let commits = vec![
            CommitEntry {
                sha: "added12".to_string(),
                subject: "add release plans".to_string(),
                kind: ChangeKind::Feature,
                scope: None,
                pull_request: None,
            },
            CommitEntry {
                sha: "fixed34".to_string(),
                subject: "restore interrupted releases".to_string(),
                kind: ChangeKind::Fix,
                scope: None,
                pull_request: None,
            },
            CommitEntry {
                sha: "change5".to_string(),
                subject: "replace the release format".to_string(),
                kind: ChangeKind::Breaking,
                scope: None,
                pull_request: None,
            },
        ];

        let rendered = render_changelog_entry(
            ChangelogPreset::KeepAChangelog,
            "2.0.0",
            Some("v1.0.0"),
            "v2.0.0",
            &commits,
            Some("owner/repo"),
        );

        assert!(
            Regex::new(r"(?m)^## \[2\.0\.0\] - [0-9]{4}-[0-9]{2}-[0-9]{2}$")
                .expect("date heading regex should compile")
                .is_match(&rendered)
        );
        assert!(rendered.contains("### Added\n\n- add release plans"));
        assert!(rendered.contains("### Fixed\n\n- restore interrupted releases"));
        assert!(rendered.contains("### Changed\n\n- replace the release format"));
        assert!(rendered.contains("[2.0.0]: https://github.com/owner/repo/compare/v1.0.0...v2.0.0"));
        assert!(
            rendered.contains("[Unreleased]: https://github.com/owner/repo/compare/v2.0.0...HEAD")
        );
    }

    #[test]
    fn inserts_keep_a_changelog_release_after_unreleased_and_updates_links() {
        let existing = "# Changelog\n\n## [Unreleased]\n\n### Added\n\n- pending\n\n## [1.0.0] - 2025-01-01\n\n- old\n\n[Unreleased]: https://github.com/owner/repo/compare/v1.0.0...HEAD\n[1.0.0]: https://github.com/owner/repo/releases/tag/v1.0.0\n";
        let entry = "## [2.0.0] - 2026-08-12\n\n### Added\n\n- new\n\n[Unreleased]: https://github.com/owner/repo/compare/v2.0.0...HEAD\n[2.0.0]: https://github.com/owner/repo/compare/v1.0.0...v2.0.0\n";

        let updated = insert_changelog_entry(ChangelogPreset::KeepAChangelog, existing, entry);

        let unreleased = updated.find("## [Unreleased]").expect("Unreleased exists");
        let released = updated.find("## [2.0.0]").expect("new release exists");
        let previous = updated.find("## [1.0.0]").expect("old release exists");
        assert!(unreleased < released && released < previous);
        assert!(updated.contains("- pending\n\n## [2.0.0]"));
        assert_eq!(updated.matches("[Unreleased]:").count(), 1);
        assert!(updated.ends_with("[1.0.0]: https://github.com/owner/repo/releases/tag/v1.0.0\n"));
    }

    #[test]
    fn angular_insertion_preserves_existing_behavior() {
        let updated = insert_changelog_entry(
            ChangelogPreset::Angular,
            "# Changelog   \r\nold",
            "## 0.2.0\n\n* feature",
        );

        assert_eq!(updated, "# Changelog\n\n## 0.2.0\n\n* feature\n\nold");
    }
}
