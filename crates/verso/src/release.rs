use crate::{
    cargo_manifest,
    changelog::{self, render_changelog_entry},
    cli::Cli,
    config::{self, render_template},
    doctor,
    dry_run::{
        render_dry_run, render_dry_run_json, render_dry_run_styled, PlannedHook, ReleasePlan,
    },
    git, package_json,
    rollback::ChangeSet,
    versioning::{bump_prerelease, bump_stable, parse_custom_version, BaseBump, PrereleaseChannel},
    workspace::PackageFile,
};
use inquire::{Confirm, Select, Text};
use semver::Version;
use std::{
    env, fmt, fs,
    io::{self, IsTerminal, Write},
    path::{Path, PathBuf},
    process::Command,
};

pub fn run(cli: Cli) -> Result<(), String> {
    if cli.json && !cli.dry_run {
        return Err("--json can only be used with --dry-run".to_string());
    }

    let config_path = cli.config_path_buf();
    let allow_missing_default_config = !cli.config_was_explicit();
    let inspection = doctor::inspect_project(&config_path, allow_missing_default_config)?;
    let root = inspection.root;
    let config = inspection.config;
    let packages = inspection.packages;
    let cargo_manifest_files = inspection.cargo_manifest_files;
    let current_version = inspection.current_version;

    let target_version =
        resolve_target_version(cli.target_version.as_deref(), &current_version, cli.yes)?;
    let tag_name = render_template(&config.git.tag_name, &target_version.to_string());
    let commit_message = render_template(&config.git.commit_message, &target_version.to_string());
    let changelog_file = config
        .changelog
        .enabled
        .then(|| root.join(&config.changelog.infile));
    let package_files = packages
        .iter()
        .map(|package| package.package_json.clone())
        .collect::<Vec<_>>();
    let cargo_lock_files = cargo_lock_files_for_manifests(&root, &cargo_manifest_files);
    let extra_version_files = cargo_manifest_files
        .iter()
        .chain(cargo_lock_files.iter())
        .cloned()
        .collect::<Vec<_>>();

    if cli.dry_run {
        let warnings = dry_run_warnings(&root, &tag_name)?;
        let plan = ReleasePlan {
            current_version,
            target_version,
            package_files,
            extra_version_files,
            changelog_file,
            commit_message,
            tag_name,
            hooks: planned_hooks(&config.hooks),
            warnings,
        };
        if cli.json {
            println!("{}", render_dry_run_json(&root, &plan));
        } else if should_style_human_output() {
            print!("{}", render_dry_run_styled(&root, &plan));
        } else {
            print!("{}", render_dry_run(&root, &plan));
        }
        return Ok(());
    }

    let upstream = git::release_upstream(&root)?;
    if config.git.require_clean_worktree {
        if !git::is_worktree_clean(&root)? {
            return Err(dirty_worktree_error());
        }
    } else {
        if !git::is_index_clean(&root)? {
            return Err(dirty_index_error());
        }
        let release_paths = package_files
            .iter()
            .chain(extra_version_files.iter())
            .chain(changelog_file.iter())
            .cloned()
            .collect::<Vec<_>>();
        let release_paths = relative_path_strings(&root, &release_paths);
        if !git::are_paths_clean(&root, &release_paths)? {
            return Err(dirty_release_files_error());
        }
    }
    if git::tag_exists(&root, &tag_name)? {
        return Err(existing_tag_error(&tag_name));
    }

    let changelog_entry = if changelog_file.is_some() {
        let previous_tag = previous_tag(&root, &config.git.tag_name)?;
        let commits = changelog::commits_since(&root, previous_tag.as_deref())?;
        let repo_slug =
            git::remote_origin_url(&root).and_then(|remote| changelog::infer_github_slug(&remote));
        Some(render_changelog_entry(
            &target_version.to_string(),
            previous_tag.as_deref(),
            &tag_name,
            &commits,
            repo_slug.as_deref(),
        ))
    } else {
        None
    };

    let before_head = git::current_head(&root)?;
    confirm_release_step(
        &format!("Modify release files for {target_version}?"),
        cli.yes,
    )?;
    run_hook(&root, "before_version", &config.hooks.before_version)?;
    let release_files = write_release_files(
        &root,
        &packages,
        &cargo_manifest_files,
        &cargo_lock_files,
        changelog_file.as_deref(),
        &target_version,
        changelog_entry.as_deref(),
    )?;
    if let Err(error) = run_hook(&root, "after_version", &config.hooks.after_version) {
        return Err(rollback_add_failure(&root, &release_files, error));
    }
    confirm_release_step(
        &format!("Commit release files with \"{commit_message}\"?"),
        cli.yes,
    )?;
    if let Err(error) = run_hook(&root, "before_commit", &config.hooks.before_commit) {
        return Err(rollback_commit_failure(&root, &release_files, error));
    }
    if let Err(error) = git_add_release_files(&root, &release_files.changed_paths) {
        return Err(rollback_add_failure(&root, &release_files, error));
    }
    if let Err(error) = git::git(&root, &["commit", "-m", &commit_message]) {
        return Err(rollback_commit_failure(&root, &release_files, error));
    }
    let release_head = git::current_head(&root)?;
    if let Err(error) = run_hook(&root, "after_commit", &config.hooks.after_commit) {
        return Err(rollback_tag_failure(
            &root,
            &release_files,
            &before_head,
            &release_head,
            error,
        ));
    }
    confirm_release_step(&format!("Create tag {tag_name}?"), cli.yes)?;
    if let Err(error) = run_hook(&root, "before_tag", &config.hooks.before_tag) {
        return Err(rollback_tag_failure(
            &root,
            &release_files,
            &before_head,
            &release_head,
            error,
        ));
    }
    if let Err(error) = git::git(&root, &["tag", "-a", &tag_name, "-m", &tag_name]) {
        return Err(rollback_tag_failure(
            &root,
            &release_files,
            &before_head,
            &release_head,
            error,
        ));
    }
    if let Err(error) = run_hook(&root, "after_tag", &config.hooks.after_tag) {
        return Err(rollback_after_tag_failure(
            &root,
            &release_files,
            &before_head,
            &release_head,
            &tag_name,
            error,
        ));
    }
    confirm_release_step("Push release commit and tag?", cli.yes)?;
    if let Err(error) = run_hook(&root, "before_push", &config.hooks.before_push) {
        return Err(rollback_after_tag_failure(
            &root,
            &release_files,
            &before_head,
            &release_head,
            &tag_name,
            error,
        ));
    }
    git::push_release(&root, &upstream, &tag_name).map_err(|error| {
        format!(
            "{error}\nLocal release commit and tag were created. Fix the remote problem, then push the current branch and tag {tag_name}."
        )
    })?;
    run_hook(&root, "after_push", &config.hooks.after_push)?;

    Ok(())
}

fn should_style_human_output() -> bool {
    io::stdout().is_terminal()
        && env::var_os("NO_COLOR").is_none()
        && env::var_os("TERM").is_none_or(|term| term != "dumb")
}

fn resolve_target_version(
    input: Option<&str>,
    current: &Version,
    assume_yes: bool,
) -> Result<Version, String> {
    let target = match input {
        Some(version) => parse_custom_version(version)?,
        None => prompt_target_version(current)?,
    };
    confirm_non_forward_version(current, &target, assume_yes)?;
    Ok(target)
}

fn prompt_target_version(current: &Version) -> Result<Version, String> {
    if interactive_terminal() {
        return prompt_target_version_select(current);
    }

    prompt_target_version_text(current)
}

fn prompt_target_version_select(current: &Version) -> Result<Version, String> {
    let choice = Select::new("Select target version", target_version_choices(current))
        .prompt()
        .map_err(inquire_error)?;
    resolve_target_version_choice(choice, current)
}

fn resolve_target_version_choice(
    choice: TargetVersionChoice,
    current: &Version,
) -> Result<Version, String> {
    match choice {
        TargetVersionChoice::Stable(version)
        | TargetVersionChoice::Patch(version)
        | TargetVersionChoice::Minor(version)
        | TargetVersionChoice::Major(version) => Ok(version),
        TargetVersionChoice::Alpha => prompt_prerelease_version(current, PrereleaseChannel::Alpha),
        TargetVersionChoice::Beta => prompt_prerelease_version(current, PrereleaseChannel::Beta),
        TargetVersionChoice::Rc => prompt_prerelease_version(current, PrereleaseChannel::Rc),
        TargetVersionChoice::Custom => parse_custom_version(&prompt_text("Version")?),
    }
}

fn prompt_target_version_text(current: &Version) -> Result<Version, String> {
    loop {
        let choices = target_version_choices(current);

        println!("Select target version:");
        for (index, choice) in choices.iter().enumerate() {
            println!("  {}) {choice}", index + 1);
        }

        let answer = read_prompt("Choice: ")?;
        let choice = answer
            .parse::<usize>()
            .ok()
            .and_then(|index| index.checked_sub(1))
            .and_then(|index| choices.get(index).cloned())
            .or_else(|| {
                choices
                    .iter()
                    .find(|choice| choice.keyword() == answer)
                    .cloned()
            });
        match choice {
            Some(choice) => return resolve_target_version_choice(choice, current),
            None => {
                println!("Please choose stable, patch, minor, major, alpha, beta, rc, or custom.")
            }
        }
    }
}

fn confirm_non_forward_version(
    current: &Version,
    target: &Version,
    assume_yes: bool,
) -> Result<(), String> {
    if target > current || assume_yes {
        return Ok(());
    }

    confirm_default_no("Target version is not greater than current version. Continue?")
}

fn prompt_prerelease_version(
    current: &Version,
    channel: PrereleaseChannel,
) -> Result<Version, String> {
    if interactive_terminal() {
        return prompt_prerelease_version_select(current, channel);
    }

    prompt_prerelease_version_text(current, channel)
}

fn prompt_prerelease_version_select(
    current: &Version,
    channel: PrereleaseChannel,
) -> Result<Version, String> {
    match Select::new(
        &format!("Select {} base", prerelease_channel_label(channel)),
        prerelease_base_choices(current, channel),
    )
    .prompt()
    .map_err(inquire_error)?
    {
        PrereleaseBaseChoice::Patch(version)
        | PrereleaseBaseChoice::Minor(version)
        | PrereleaseBaseChoice::Major(version) => Ok(version),
        PrereleaseBaseChoice::Custom => {
            let base = parse_custom_version(&prompt_text("Base version")?)?;
            Ok(prerelease_from_custom_base(base, channel))
        }
    }
}

fn prompt_prerelease_version_text(
    current: &Version,
    channel: PrereleaseChannel,
) -> Result<Version, String> {
    loop {
        let patch = bump_prerelease(current, BaseBump::Patch, channel);
        let minor = bump_prerelease(current, BaseBump::Minor, channel);
        let major = bump_prerelease(current, BaseBump::Major, channel);
        let channel_label = prerelease_channel_label(channel);

        println!("Select {channel_label} base:");
        println!("  1) patch ({patch})");
        println!("  2) minor ({minor})");
        println!("  3) major ({major})");
        println!("  4) custom base version");

        match read_prompt("Choice: ")?.as_str() {
            "1" | "patch" => return Ok(patch),
            "2" | "minor" => return Ok(minor),
            "3" | "major" => return Ok(major),
            "4" | "custom" | "custom base" | "custom base version" => {
                let base = parse_custom_version(&read_prompt("Base version: ")?)?;
                return Ok(prerelease_from_custom_base(base, channel));
            }
            _ => println!("Please choose patch, minor, major, or custom."),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum TargetVersionChoice {
    Stable(Version),
    Patch(Version),
    Minor(Version),
    Major(Version),
    Alpha,
    Beta,
    Rc,
    Custom,
}

impl fmt::Display for TargetVersionChoice {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TargetVersionChoice::Stable(version) => write!(formatter, "stable ({version})"),
            TargetVersionChoice::Patch(version) => write!(formatter, "patch ({version})"),
            TargetVersionChoice::Minor(version) => write!(formatter, "minor ({version})"),
            TargetVersionChoice::Major(version) => write!(formatter, "major ({version})"),
            TargetVersionChoice::Alpha => formatter.write_str("alpha"),
            TargetVersionChoice::Beta => formatter.write_str("beta"),
            TargetVersionChoice::Rc => formatter.write_str("rc"),
            TargetVersionChoice::Custom => formatter.write_str("custom semver"),
        }
    }
}

impl TargetVersionChoice {
    fn keyword(&self) -> &str {
        match self {
            TargetVersionChoice::Stable(_) => "stable",
            TargetVersionChoice::Patch(_) => "patch",
            TargetVersionChoice::Minor(_) => "minor",
            TargetVersionChoice::Major(_) => "major",
            TargetVersionChoice::Alpha => "alpha",
            TargetVersionChoice::Beta => "beta",
            TargetVersionChoice::Rc => "rc",
            TargetVersionChoice::Custom => "custom",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum PrereleaseBaseChoice {
    Patch(Version),
    Minor(Version),
    Major(Version),
    Custom,
}

impl fmt::Display for PrereleaseBaseChoice {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PrereleaseBaseChoice::Patch(version) => write!(formatter, "patch ({version})"),
            PrereleaseBaseChoice::Minor(version) => write!(formatter, "minor ({version})"),
            PrereleaseBaseChoice::Major(version) => write!(formatter, "major ({version})"),
            PrereleaseBaseChoice::Custom => formatter.write_str("custom base version"),
        }
    }
}

fn target_version_choices(current: &Version) -> Vec<TargetVersionChoice> {
    let mut choices = Vec::new();
    if !current.pre.is_empty() {
        choices.push(TargetVersionChoice::Stable(Version::new(
            current.major,
            current.minor,
            current.patch,
        )));
    }
    choices.extend([
        TargetVersionChoice::Patch(bump_stable(current, BaseBump::Patch)),
        TargetVersionChoice::Minor(bump_stable(current, BaseBump::Minor)),
        TargetVersionChoice::Major(bump_stable(current, BaseBump::Major)),
        TargetVersionChoice::Alpha,
        TargetVersionChoice::Beta,
        TargetVersionChoice::Rc,
        TargetVersionChoice::Custom,
    ]);
    choices
}

fn prerelease_base_choices(
    current: &Version,
    channel: PrereleaseChannel,
) -> Vec<PrereleaseBaseChoice> {
    vec![
        PrereleaseBaseChoice::Patch(bump_prerelease(current, BaseBump::Patch, channel)),
        PrereleaseBaseChoice::Minor(bump_prerelease(current, BaseBump::Minor, channel)),
        PrereleaseBaseChoice::Major(bump_prerelease(current, BaseBump::Major, channel)),
        PrereleaseBaseChoice::Custom,
    ]
}

fn prerelease_from_custom_base(mut base: Version, channel: PrereleaseChannel) -> Version {
    base.pre = semver::Prerelease::new(&format!("{}.0", prerelease_channel_label(channel)))
        .expect("generated prerelease identifier should be valid semver");
    base.build = semver::BuildMetadata::EMPTY;
    base
}

fn prerelease_channel_label(channel: PrereleaseChannel) -> &'static str {
    match channel {
        PrereleaseChannel::Alpha => "alpha",
        PrereleaseChannel::Beta => "beta",
        PrereleaseChannel::Rc => "rc",
    }
}

fn confirm_release_step(question: &str, assume_yes: bool) -> Result<(), String> {
    if assume_yes {
        return Ok(());
    }

    confirm_default_yes(question)
}

fn confirm_default_yes(question: &str) -> Result<(), String> {
    if interactive_terminal() {
        return match Confirm::new(question)
            .with_default(true)
            .prompt()
            .map_err(inquire_error)?
        {
            true => Ok(()),
            false => Err("release aborted".to_string()),
        };
    }

    let answer = read_prompt(&format!("{question} [Y/n] "))?;
    match answer.as_str() {
        "" | "y" | "Y" | "yes" | "YES" | "Yes" => Ok(()),
        _ => Err("release aborted".to_string()),
    }
}

fn confirm_default_no(question: &str) -> Result<(), String> {
    if interactive_terminal() {
        return match Confirm::new(question)
            .with_default(false)
            .prompt()
            .map_err(inquire_error)?
        {
            true => Ok(()),
            false => Err("release aborted".to_string()),
        };
    }

    let answer = read_prompt(&format!("{question} [y/N] "))?;
    match answer.as_str() {
        "y" | "Y" | "yes" | "YES" | "Yes" => Ok(()),
        _ => Err("release aborted".to_string()),
    }
}

fn prompt_text(question: &str) -> Result<String, String> {
    if interactive_terminal() {
        return Text::new(question).prompt().map_err(inquire_error);
    }

    read_prompt(&format!("{question}: "))
}

fn interactive_terminal() -> bool {
    io::stdin().is_terminal() && io::stdout().is_terminal()
}

fn inquire_error(error: inquire::InquireError) -> String {
    match error {
        inquire::InquireError::OperationCanceled | inquire::InquireError::OperationInterrupted => {
            "release aborted".to_string()
        }
        error => format!("interactive prompt failed: {error}"),
    }
}

fn run_hook(root: &Path, name: &str, command: &Option<String>) -> Result<(), String> {
    let Some(command) = command else {
        return Ok(());
    };

    let status = shell_command(command)
        .current_dir(root)
        .status()
        .map_err(|error| format!("failed to run hook {name}: {error}"))?;

    if status.success() {
        Ok(())
    } else {
        let status = status.code().map_or_else(
            || "terminated by signal".to_string(),
            |code| code.to_string(),
        );
        Err(format!(
            "hook {name} failed with status {status}: {command}"
        ))
    }
}

fn planned_hooks(hooks: &config::HooksConfig) -> Vec<PlannedHook> {
    [
        ("before_version", &hooks.before_version),
        ("after_version", &hooks.after_version),
        ("before_commit", &hooks.before_commit),
        ("after_commit", &hooks.after_commit),
        ("before_tag", &hooks.before_tag),
        ("after_tag", &hooks.after_tag),
        ("before_push", &hooks.before_push),
        ("after_push", &hooks.after_push),
    ]
    .into_iter()
    .filter_map(|(name, command)| {
        command.as_ref().map(|command| PlannedHook {
            name: name.to_owned(),
            command: command.clone(),
        })
    })
    .collect()
}

fn shell_command(command: &str) -> Command {
    #[cfg(windows)]
    {
        let mut process = Command::new("cmd");
        process.args(["/C", command]);
        process
    }

    #[cfg(not(windows))]
    {
        let mut process = Command::new("sh");
        process.args(["-c", command]);
        process
    }
}

fn read_prompt(prompt: &str) -> Result<String, String> {
    print!("{prompt}");
    io::stdout()
        .flush()
        .map_err(|error| format!("failed to flush prompt: {error}"))?;

    let mut input = String::new();
    let bytes = io::stdin()
        .read_line(&mut input)
        .map_err(|error| format!("failed to read prompt input: {error}"))?;
    if bytes == 0 {
        return Err("interactive prompt requires input".to_string());
    }

    Ok(input.trim().to_string())
}

pub(crate) fn release_root(config_path: &Path) -> Result<PathBuf, String> {
    let current_dir =
        std::env::current_dir().map_err(|error| format!("failed to read current dir: {error}"))?;

    let Some(parent) = config_path.parent() else {
        return Ok(current_dir);
    };
    if parent.as_os_str().is_empty() {
        return Ok(current_dir);
    }
    if parent.is_absolute() {
        Ok(parent.to_path_buf())
    } else {
        Ok(current_dir.join(parent))
    }
}

pub(crate) fn current_version(
    root: &Path,
    root_package: &Path,
    packages: &[PackageFile],
) -> Result<Version, String> {
    let root_package = root.join(root_package);
    if let Some(package) = packages
        .iter()
        .find(|package| package.package_json == root_package)
    {
        return Ok(package.info.version.clone());
    }

    packages
        .first()
        .map(|package| package.info.version.clone())
        .ok_or_else(|| "no package version discovered".to_string())
}

fn dry_run_warnings(root: &Path, tag_name: &str) -> Result<Vec<String>, String> {
    let mut warnings = Vec::new();

    if !git::is_worktree_clean(root)? {
        warnings.push(dirty_worktree_warning());
    }
    if git::tag_exists(root, tag_name)? {
        warnings.push(existing_tag_warning(tag_name));
    }

    Ok(warnings)
}

fn dirty_worktree_warning() -> String {
    "working tree is dirty; Commit, stash, or revert local changes before releasing.".to_string()
}

fn existing_tag_warning(tag_name: &str) -> String {
    format!(
        "tag {tag_name} already exists; choose a different version or inspect it with: git show {tag_name}"
    )
}

fn dirty_worktree_error() -> String {
    [
        "working tree is dirty",
        "Commit, stash, or revert local changes before releasing.",
        "Run with --dry-run to preview without requiring a clean worktree.",
        "If dirty releases are intentional, set git.require_clean_worktree = false in verso.toml.",
    ]
    .join("\n")
}

fn dirty_release_files_error() -> String {
    [
        "release files are dirty",
        "Commit, stash, or revert changes to package manifests, Cargo manifests and lockfiles, or the changelog before releasing.",
        "git.require_clean_worktree = false only permits changes to unrelated files.",
    ]
    .join("\n")
}

fn dirty_index_error() -> String {
    [
        "Git index is not clean",
        "Commit or unstage existing staged changes before releasing.",
        "Verso will not include unrelated staged files in a release commit.",
    ]
    .join("\n")
}

fn existing_tag_error(tag_name: &str) -> String {
    [
        format!("tag {tag_name} already exists"),
        "Choose a different version, or inspect the existing tag before continuing.".to_string(),
        format!("Inspect it with: git show {tag_name}"),
        format!("If it was created by mistake, delete it with: git tag -d {tag_name}"),
    ]
    .join("\n")
}

fn previous_tag(root: &Path, tag_template: &str) -> Result<Option<String>, String> {
    let Some((prefix, suffix)) = tag_template.split_once("${version}") else {
        return Ok(None);
    };

    let output = git::git(root, &["tag", "--merged", "HEAD", "--list"])?;
    Ok(output
        .stdout
        .lines()
        .map(str::trim)
        .filter_map(|tag| {
            let version = tag
                .strip_prefix(prefix)?
                .strip_suffix(suffix)
                .and_then(|version| Version::parse(version).ok())?;
            Some((version, tag.to_string()))
        })
        .max_by(|(left_version, _), (right_version, _)| left_version.cmp(right_version))
        .map(|(_version, tag)| tag))
}

pub(crate) fn verify_cargo_manifest_versions(
    root: &Path,
    manifest_files: &[PathBuf],
    expected: &Version,
) -> Result<(), String> {
    let mut mismatches = Vec::new();

    for manifest_path in manifest_files {
        let contents = fs::read_to_string(manifest_path)
            .map_err(|error| format!("failed to read {}: {error}", manifest_path.display()))?;
        let package = cargo_manifest::read_package_version(manifest_path, &contents)?;
        if package.version != *expected {
            mismatches.push(format!(
                "{} has version {}",
                relative_path(root, manifest_path).display(),
                package.version
            ));
        }
    }

    if mismatches.is_empty() {
        return Ok(());
    }

    Err(format!(
        "inconsistent versions: {}; configured Cargo manifests must match release version {expected}. Set version.require_consistent_versions = false to skip this check.",
        mismatches.join("; ")
    ))
}

fn cargo_lock_files_for_manifests(root: &Path, manifest_files: &[PathBuf]) -> Vec<PathBuf> {
    let mut lock_files = manifest_files
        .iter()
        .filter_map(|manifest_path| cargo_lock_file_for_manifest(root, manifest_path))
        .collect::<Vec<_>>();
    lock_files.sort();
    lock_files.dedup();
    lock_files
}

fn cargo_lock_file_for_manifest(root: &Path, manifest_path: &Path) -> Option<PathBuf> {
    let mut current = manifest_path.parent()?;

    loop {
        let lock_path = current.join("Cargo.lock");
        if lock_path.exists() {
            return Some(lock_path);
        }

        if current == root {
            return None;
        }

        current = current.parent()?;
    }
}

struct ReleaseFileChanges {
    changed_paths: Vec<PathBuf>,
    change_set: ChangeSet,
}

fn write_release_files(
    root: &Path,
    packages: &[PackageFile],
    cargo_manifest_files: &[PathBuf],
    cargo_lock_files: &[PathBuf],
    changelog_file: Option<&Path>,
    target_version: &Version,
    changelog_entry: Option<&str>,
) -> Result<ReleaseFileChanges, String> {
    let mut paths = packages
        .iter()
        .map(|package| package.package_json.clone())
        .collect::<Vec<_>>();
    paths.extend(cargo_manifest_files.iter().cloned());
    paths.extend(cargo_lock_files.iter().cloned());
    paths.extend(changelog_file.map(Path::to_path_buf));

    let mut changes = ChangeSet::snapshot(&paths)?;
    let result = (|| {
        let mut changed_paths = Vec::new();
        for package in packages {
            let contents = fs::read_to_string(&package.package_json).map_err(|error| {
                format!("failed to read {}: {error}", package.package_json.display())
            })?;
            let updated = package_json::replace_manifest_version_preserving_style(
                &package.package_json,
                &contents,
                target_version,
            )?;
            if updated != contents {
                changes.write(&package.package_json, updated.as_bytes())?;
                changed_paths.push(package.package_json.clone());
            }
        }

        let mut cargo_package_updates = Vec::new();
        for manifest_path in cargo_manifest_files {
            let contents = fs::read_to_string(manifest_path)
                .map_err(|error| format!("failed to read {}: {error}", manifest_path.display()))?;
            let package = cargo_manifest::read_package_version(manifest_path, &contents)?;
            let updated = cargo_manifest::replace_package_version_preserving_style(
                manifest_path,
                &contents,
                target_version,
            )?;
            if updated != contents {
                changes.write(manifest_path, updated.as_bytes())?;
                changed_paths.push(manifest_path.clone());
                cargo_package_updates
                    .push((package, cargo_lock_file_for_manifest(root, manifest_path)));
            }
        }

        for lock_path in cargo_lock_files {
            let contents = fs::read_to_string(lock_path)
                .map_err(|error| format!("failed to read {}: {error}", lock_path.display()))?;
            let mut updated = contents.clone();
            for (package, package_lock_path) in &cargo_package_updates {
                if package_lock_path.as_ref() != Some(lock_path) {
                    continue;
                }
                updated = cargo_manifest::replace_lock_package_version_preserving_style(
                    lock_path,
                    &updated,
                    &package.name,
                    &package.version,
                    target_version,
                )?;
            }
            if updated != contents {
                changes.write(lock_path, updated.as_bytes())?;
                changed_paths.push(lock_path.clone());
            }
        }

        if let (Some(changelog_file), Some(changelog_entry)) = (changelog_file, changelog_entry) {
            let existing_changelog = read_changelog(changelog_file)?;
            let changelog = insert_changelog_entry(&existing_changelog, changelog_entry);
            if changelog != existing_changelog {
                changes.write(changelog_file, changelog.as_bytes())?;
                changed_paths.push(changelog_file.to_path_buf());
            }
        }

        Ok(changed_paths)
    })();

    match result {
        Ok(changed_paths) => Ok(ReleaseFileChanges {
            changed_paths,
            change_set: changes,
        }),
        Err(error) => match changes.rollback() {
            Ok(_restored) => Err(error),
            Err(rollback_error) => Err(format!("{error}; rollback failed: {rollback_error}")),
        },
    }
}

fn read_changelog(path: &Path) -> Result<String, String> {
    match fs::read_to_string(path) {
        Ok(contents) => Ok(contents),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            Ok("# Changelog\n".to_string())
        }
        Err(error) => Err(format!("failed to read {}: {error}", path.display())),
    }
}

fn insert_changelog_entry(existing: &str, entry: &str) -> String {
    let entry = entry.trim_end();

    let (first_line, body) = existing
        .split_once('\n')
        .map_or((existing, ""), |(first_line, body)| (first_line, body));

    if first_line.trim_end() == "# Changelog" {
        let heading = "# Changelog";
        let body = body
            .strip_prefix('\r')
            .unwrap_or(body)
            .trim_start_matches('\n');

        if body.is_empty() {
            format!("{heading}\n\n{entry}\n")
        } else {
            format!("{heading}\n\n{entry}\n\n{body}")
        }
    } else if existing.trim().is_empty() {
        format!("# Changelog\n\n{entry}\n")
    } else {
        format!("# Changelog\n\n{entry}\n\n{existing}")
    }
}

fn git_add_release_files(root: &Path, files: &[PathBuf]) -> Result<(), String> {
    if files.is_empty() {
        return Ok(());
    }

    let mut files = files.to_vec();
    files.sort();
    files.dedup();

    let mut args = vec!["add".to_string(), "--".to_string()];
    args.extend(
        files
            .iter()
            .map(|file| relative_path(root, file).display().to_string()),
    );
    let arg_refs = args.iter().map(String::as_str).collect::<Vec<_>>();

    git::git(root, &arg_refs)?;
    Ok(())
}

fn rollback_commit_failure(
    root: &Path,
    release_files: &ReleaseFileChanges,
    error: String,
) -> String {
    let paths = relative_path_strings(root, &release_files.changed_paths);
    let unstage_result = git::unstage_paths(root, &paths);
    let rollback_result = release_files.change_set.rollback();

    append_best_effort_errors(error, unstage_result, rollback_result)
}

fn rollback_add_failure(root: &Path, release_files: &ReleaseFileChanges, error: String) -> String {
    let paths = relative_path_strings(root, &release_files.changed_paths);
    let unstage_result = git::unstage_paths(root, &paths);
    let rollback_result = release_files.change_set.rollback();

    append_best_effort_errors(error, unstage_result, rollback_result)
}

fn rollback_tag_failure(
    root: &Path,
    release_files: &ReleaseFileChanges,
    before_head: &str,
    release_head: &str,
    error: String,
) -> String {
    match git::current_head(root) {
        Ok(current_head) if current_head == release_head => {
            let reset_result = git::reset_soft(root, before_head);
            let paths = relative_path_strings(root, &release_files.changed_paths);
            let unstage_result = git::unstage_paths(root, &paths);
            let rollback_result = release_files.change_set.rollback();

            append_tag_rollback_errors(error, reset_result, unstage_result, rollback_result)
        }
        Ok(current_head) => {
            let rollback_result = release_files.change_set.rollback();
            let rollback_note = match rollback_result {
                Ok(_restored) => String::new(),
                Err(rollback_error) => format!("; file rollback failed: {rollback_error}"),
            };
            format!(
                "{error}; HEAD moved unexpectedly from release commit {release_head} to {current_head}; skipped git reset{rollback_note}"
            )
        }
        Err(head_error) => {
            let rollback_result = release_files.change_set.rollback();
            let rollback_note = match rollback_result {
                Ok(_restored) => String::new(),
                Err(rollback_error) => format!("; file rollback failed: {rollback_error}"),
            };
            format!(
                "{error}; failed to verify HEAD before rollback: {head_error}; skipped git reset{rollback_note}"
            )
        }
    }
}

fn rollback_after_tag_failure(
    root: &Path,
    release_files: &ReleaseFileChanges,
    before_head: &str,
    release_head: &str,
    tag_name: &str,
    error: String,
) -> String {
    let delete_tag_result = git::delete_tag(root, tag_name);
    let mut message = rollback_tag_failure(root, release_files, before_head, release_head, error);
    if let Err(delete_tag_error) = delete_tag_result {
        message.push_str(&format!("; tag cleanup failed: {delete_tag_error}"));
    }
    message
}

fn append_best_effort_errors(
    error: String,
    unstage_result: Result<(), String>,
    rollback_result: Result<Vec<PathBuf>, String>,
) -> String {
    let mut message = error;
    if let Err(unstage_error) = unstage_result {
        message.push_str(&format!("; unstage failed: {unstage_error}"));
    }
    if let Err(rollback_error) = rollback_result {
        message.push_str(&format!("; rollback failed: {rollback_error}"));
    }
    message
}

fn append_tag_rollback_errors(
    error: String,
    reset_result: Result<(), String>,
    unstage_result: Result<(), String>,
    rollback_result: Result<Vec<PathBuf>, String>,
) -> String {
    let mut message = error;
    if let Err(reset_error) = reset_result {
        message.push_str(&format!("; soft reset failed: {reset_error}"));
    }
    if let Err(unstage_error) = unstage_result {
        message.push_str(&format!("; unstage failed: {unstage_error}"));
    }
    if let Err(rollback_error) = rollback_result {
        message.push_str(&format!("; rollback failed: {rollback_error}"));
    }
    message
}

fn relative_path_strings(root: &Path, paths: &[PathBuf]) -> Vec<String> {
    paths
        .iter()
        .map(|path| relative_path(root, path).display().to_string())
        .collect()
}

fn relative_path(root: &Path, path: &Path) -> PathBuf {
    path.strip_prefix(root)
        .map(Path::to_path_buf)
        .unwrap_or_else(|_error| path.to_path_buf())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::package_json::PackageInfo;
    use tempfile::TempDir;

    #[test]
    fn previous_tag_matches_template_prefix_and_suffix() -> Result<(), String> {
        let repo = init_repo()?;
        fs::write(repo.path().join("README.md"), "hello").map_err(|error| error.to_string())?;
        git::git(repo.path(), &["add", "README.md"])?;
        git::git(repo.path(), &["commit", "-m", "feat: initial"])?;
        git::git(repo.path(), &["tag", "pkg-0.1.0-release"])?;
        git::git(repo.path(), &["tag", "pkg-0.2.0-release"])?;
        git::git(repo.path(), &["tag", "pkg-9.9.9-other"])?;

        assert_eq!(
            previous_tag(repo.path(), "pkg-${version}-release")?,
            Some("pkg-0.2.0-release".to_string())
        );

        Ok(())
    }

    #[test]
    fn current_version_prefers_root_package() -> Result<(), String> {
        let temp = TempDir::new().map_err(|error| error.to_string())?;
        let root_package = test_package(temp.path(), None, "root", "1.2.3")?;
        let workspace_package =
            test_package(temp.path(), Some(Path::new("packages/a")), "a", "9.9.9")?;

        assert_eq!(
            current_version(
                temp.path(),
                Path::new("package.json"),
                &[workspace_package, root_package]
            )?,
            Version::parse("1.2.3").expect("test semver should parse")
        );

        Ok(())
    }

    #[test]
    fn write_release_files_returns_only_changed_paths() -> Result<(), String> {
        let temp = TempDir::new().map_err(|error| error.to_string())?;
        let root_package = test_package(temp.path(), None, "root", "0.2.0")?;
        let workspace_package =
            test_package(temp.path(), Some(Path::new("packages/a")), "a", "0.1.0")?;
        let changelog = temp.path().join("CHANGELOG.md");
        fs::write(&changelog, "# Changelog\n").map_err(|error| error.to_string())?;

        let changed = write_release_files(
            temp.path(),
            &[root_package.clone(), workspace_package.clone()],
            &[],
            &[],
            Some(&changelog),
            &Version::parse("0.2.0").expect("test semver should parse"),
            Some("# 0.2.0 (2026-06-24)\n\nNo classifiable changes.\n"),
        )?;

        assert_eq!(
            changed.changed_paths,
            vec![workspace_package.package_json.clone(), changelog.clone()]
        );
        assert_eq!(
            fs::read_to_string(&root_package.package_json).map_err(|error| error.to_string())?,
            "{\n  \"name\": \"root\",\n  \"version\": \"0.2.0\"\n}\n"
        );

        Ok(())
    }

    #[test]
    fn cargo_lock_file_search_prefers_nearest_lock_under_release_root() -> Result<(), String> {
        let temp = TempDir::new().map_err(|error| error.to_string())?;
        let root_lock = temp.path().join("Cargo.lock");
        let nested_lock = temp.path().join("crates/verso/Cargo.lock");
        let manifest = temp.path().join("crates/verso/Cargo.toml");
        fs::create_dir_all(manifest.parent().expect("manifest should have a parent"))
            .map_err(|error| error.to_string())?;
        fs::write(&root_lock, "version = 4\n").map_err(|error| error.to_string())?;
        fs::write(&nested_lock, "version = 4\n").map_err(|error| error.to_string())?;

        assert_eq!(
            cargo_lock_file_for_manifest(temp.path(), &manifest),
            Some(nested_lock)
        );

        Ok(())
    }

    #[test]
    fn changelog_insertion_normalizes_heading_whitespace() -> Result<(), String> {
        let updated = insert_changelog_entry("# Changelog   \nold", "## 0.2.0\n\n* feature");
        let crlf_updated = insert_changelog_entry("# Changelog   \r\nold", "## 0.2.0\n\n* feature");

        assert_eq!(updated, "# Changelog\n\n## 0.2.0\n\n* feature\n\nold");
        assert_eq!(crlf_updated, "# Changelog\n\n## 0.2.0\n\n* feature\n\nold");
        Ok(())
    }

    #[test]
    fn prerelease_target_choices_include_the_current_stable_version() -> Result<(), String> {
        let current = Version::parse("1.0.0-rc.2").map_err(|error| error.to_string())?;

        assert_eq!(
            target_version_choices(&current).first(),
            Some(&TargetVersionChoice::Stable(Version::new(1, 0, 0)))
        );
        Ok(())
    }

    fn init_repo() -> Result<TempDir, String> {
        let repo = TempDir::new().map_err(|error| error.to_string())?;
        git::git(repo.path(), &["init"])?;
        git::git(repo.path(), &["config", "user.email", "test@example.com"])?;
        git::git(repo.path(), &["config", "user.name", "Test User"])?;
        git::git(repo.path(), &["config", "commit.gpgSign", "false"])?;
        git::git(repo.path(), &["config", "tag.gpgSign", "false"])?;
        Ok(repo)
    }

    fn test_package(
        root: &Path,
        dir: Option<&Path>,
        name: &str,
        version: &str,
    ) -> Result<PackageFile, String> {
        let dir = dir.map_or_else(|| root.to_path_buf(), |dir| root.join(dir));
        fs::create_dir_all(&dir).map_err(|error| error.to_string())?;
        let package_json = dir.join("package.json");
        fs::write(
            &package_json,
            format!("{{\n  \"name\": \"{name}\",\n  \"version\": \"{version}\"\n}}\n"),
        )
        .map_err(|error| error.to_string())?;

        Ok(PackageFile {
            dir,
            package_json,
            info: PackageInfo {
                name: Some(name.to_string()),
                version: Version::parse(version).map_err(|error| error.to_string())?,
            },
        })
    }
}
