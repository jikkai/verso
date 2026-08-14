use crate::{
    cargo_manifest,
    cli::{BumpArgs, BumpLevel, Cli, ResumeArgs},
    config,
    diagnostic::stdout_supports_color,
    doctor,
    dry_run::{render_dry_run, render_dry_run_json, render_dry_run_styled},
    git,
    plan::{self, PlanMode, ReleasePlan},
    transaction::{self, FilesState, ReleaseTransaction, TransactionStage},
    versioning::{bump_prerelease, bump_stable, parse_custom_version, BaseBump, PrereleaseChannel},
    workspace::PackageFile,
};
use inquire::{Confirm, Select, Text};
use semver::Version;
use std::{
    fmt, fs,
    io::{self, IsTerminal, Write},
    path::{Path, PathBuf},
    process::Command,
};

const RELEASE_CANCELLED: &str = "cancelled: release aborted";

pub fn run(cli: Cli) -> Result<(), String> {
    run_mode(cli, PlanMode::Release, None)
}

pub fn run_bump(cli: Cli, args: BumpArgs) -> Result<(), String> {
    run_mode(cli, PlanMode::Bump, args.level)
}

fn run_mode(cli: Cli, mode: PlanMode, bump_level: Option<BumpLevel>) -> Result<(), String> {
    if cli.json && !cli.dry_run {
        return Err("--json can only be used with --dry-run".to_string());
    }

    let config_path = cli.config_path_buf();
    let root = release_root(&config_path)?;
    let absolute_config_path = if config_path.is_absolute() {
        config_path.clone()
    } else {
        std::env::current_dir()
            .map_err(|error| format!("failed to read current dir: {error}"))?
            .join(&config_path)
    };
    let _lock = if cli.dry_run {
        None
    } else {
        Some(transaction::lock(&root)?)
    };
    if _lock.is_some() && transaction::load(&root)?.is_some() {
        return Err(
            "an active Verso transaction already exists\n\nhelp: run `verso status`, then `verso resume` or `verso abort`."
                .to_string(),
        );
    }
    let planned_head = _lock
        .as_ref()
        .map(|_| git::current_head(&root))
        .transpose()?;
    let allow_missing_default_config = !cli.config_was_explicit();
    let inspection = doctor::inspect_project(&config_path, allow_missing_default_config)?;
    let target_version = match (cli.target_version.as_deref(), bump_level) {
        (Some(version), _) => {
            resolve_target_version(Some(version), &inspection.current_version, cli.yes)?
        }
        (None, Some(level)) => bump_stable(
            &inspection.current_version,
            match level {
                BumpLevel::Patch => BaseBump::Patch,
                BumpLevel::Minor => BaseBump::Minor,
                BumpLevel::Major => BaseBump::Major,
            },
        ),
        (None, None) => resolve_target_version(None, &inspection.current_version, cli.yes)?,
    };
    let group = cli.group.clone().unwrap_or_else(|| {
        let stem = config_path
            .file_stem()
            .and_then(|name| name.to_str())
            .unwrap_or("default");
        if stem == "verso" {
            "default".to_owned()
        } else {
            stem.strip_prefix("verso.").unwrap_or(stem).to_owned()
        }
    });
    let mut release_plan = plan::build(
        &inspection,
        &absolute_config_path,
        group,
        target_version,
        mode,
        Vec::new(),
    )?;
    if cli.dry_run {
        release_plan.warnings = dry_run_warnings(&root, release_plan.tag_name.as_deref())?;
    }

    if cli.dry_run {
        if cli.json {
            println!("{}", render_dry_run_json(&root, &release_plan));
        } else if stdout_supports_color() {
            print!("{}", render_dry_run_styled(&root, &release_plan));
        } else {
            print!("{}", render_dry_run(&root, &release_plan));
        }
        return Ok(());
    }

    if mode == PlanMode::Bump && release_plan.file_changes.is_empty() {
        return Err(format!(
            "no version files need updating to {}",
            release_plan.target_version
        ));
    }

    let before_head =
        planned_head.ok_or_else(|| "release execution is missing its lock".to_string())?;
    let upstream = if mode == PlanMode::Release {
        Some(git::release_upstream(&root)?)
    } else {
        None
    };
    verify_clean_for_plan(&release_plan, &inspection.config)?;
    if git::current_head(&root)? != before_head {
        return Err("HEAD changed while preparing the release plan; retry the command".to_string());
    }
    if let Some(tag_name) = &release_plan.tag_name {
        if git::tag_exists(&root, tag_name)? {
            return Err(existing_tag_error(tag_name));
        }
    }

    confirm_release_step(
        &format!("Modify release files for {}?", release_plan.target_version),
        cli.yes,
    )?;
    let mut active = transaction::begin(&root, release_plan, before_head, upstream)?;
    match execute_transaction(&mut active, cli.yes, false) {
        Ok(()) => Ok(()),
        Err(failure) if failure.rollback => {
            let verify_remote = active.plan.mode == PlanMode::Release;
            let rollback = abort_transaction(&mut active, verify_remote);
            Err(match rollback {
                Ok(()) => failure.message,
                Err(rollback_error) => {
                    format!("{}; rollback failed: {rollback_error}", failure.message)
                }
            })
        }
        Err(failure) => Err(failure.message),
    }
}

pub fn transaction_status(config_path: &Path, json: bool) -> Result<(), String> {
    let root = release_root(config_path)?;
    let active = transaction::load(&root)?;
    if let Some(active) = &active {
        verify_transaction_config(active, config_path)?;
    }
    print!("{}", transaction::render_status(active.as_ref(), json));
    Ok(())
}

pub fn resume(config_path: &Path, args: &ResumeArgs) -> Result<(), String> {
    let root = release_root(config_path)?;
    let _lock = transaction::lock(&root)?;
    let mut active = transaction::require(&root)?;
    verify_transaction_config(&active, config_path)?;
    if active.aborting {
        abort_transaction(&mut active, false)?;
        println!("Completed the interrupted Verso rollback.");
        return Ok(());
    }
    resolve_interrupted_hook(&mut active, args)?;
    execute_transaction(&mut active, true, true).map_err(|failure| failure.message)
}

fn resolve_interrupted_hook(
    active: &mut ReleaseTransaction,
    args: &ResumeArgs,
) -> Result<(), String> {
    match (active.active_hook.clone(), args.retry_hook, args.skip_hook) {
        (Some(_), true, false) => transaction::retry_active_hook(active),
        (Some(hook), false, true) => transaction::finish_hook(active, &hook),
        (Some(hook), false, false) => Err(format!(
            "hook {hook} was interrupted and its outcome is unknown\n\nhelp: inspect its side effects, then run `verso resume --retry-hook` to run it again or `verso resume --skip-hook` to mark it complete.\nnote: `verso abort` is also available before push."
        )),
        (None, true, false) | (None, false, true) => {
            Err("there is no interrupted hook to recover".to_string())
        }
        (_, true, true) => Err("choose only one interrupted hook action".to_string()),
        (None, false, false) => Ok(()),
    }
}

pub fn abort(config_path: &Path) -> Result<(), String> {
    let root = release_root(config_path)?;
    let _lock = transaction::lock(&root)?;
    let mut active = transaction::require(&root)?;
    verify_transaction_config(&active, config_path)?;
    abort_transaction(&mut active, true)?;
    println!("Aborted Verso transaction and restored release files.");
    Ok(())
}

fn verify_transaction_config(
    active: &ReleaseTransaction,
    requested_config: &Path,
) -> Result<(), String> {
    let requested = if requested_config.is_absolute() {
        requested_config.to_path_buf()
    } else {
        std::env::current_dir()
            .map_err(|error| format!("failed to read current dir: {error}"))?
            .join(requested_config)
    };
    let planned = active.plan.config_path.clone();
    let requested = requested.canonicalize().unwrap_or(requested);
    let planned = planned.canonicalize().unwrap_or(planned);
    if requested == planned {
        Ok(())
    } else {
        Err(format!(
            "active transaction belongs to group {} ({}) rather than requested config {}\n\nhelp: rerun the recovery command with `--config {}`.",
            active.plan.group,
            planned.display(),
            requested.display(),
            planned.display()
        ))
    }
}

struct ExecutionFailure {
    message: String,
    rollback: bool,
}

impl ExecutionFailure {
    fn rollback(message: String) -> Self {
        Self {
            message,
            rollback: true,
        }
    }

    fn preserve(message: String) -> Self {
        Self {
            message,
            rollback: false,
        }
    }
}

fn execute_transaction(
    active: &mut ReleaseTransaction,
    assume_yes: bool,
    resuming: bool,
) -> Result<(), ExecutionFailure> {
    let root = active.plan.root.clone();
    if active.push_started
        && active.stage == TransactionStage::Tagged
        && remote_contains_release(active).map_err(ExecutionFailure::preserve)?
    {
        transaction::set_stage(active, TransactionStage::Pushed)
            .map_err(ExecutionFailure::preserve)?;
    }
    if active.stage != TransactionStage::Pushed
        && !(active.push_started && active.stage == TransactionStage::Tagged)
    {
        reconcile_transaction(active).map_err(ExecutionFailure::preserve)?;
        verify_expected_head(active).map_err(ExecutionFailure::preserve)?;
    }

    if active.stage == TransactionStage::Planned {
        run_transaction_hook(active, "before_version", !resuming)?;
        transaction::apply_files(active).map_err(ExecutionFailure::rollback)?;
        transaction::set_stage(active, TransactionStage::FilesApplied)
            .map_err(ExecutionFailure::preserve)?;
    }

    if active.stage == TransactionStage::FilesApplied {
        run_transaction_hook(active, "after_version", !resuming)?;
        if active.plan.mode == PlanMode::Bump {
            if transaction::files_state(active).map_err(ExecutionFailure::preserve)?
                != FilesState::Applied
            {
                return Err(ExecutionFailure::preserve(
                    "release files no longer match the exact bump plan; preserving the transaction for inspection"
                        .to_string(),
                ));
            }
            let target = active.plan.target_version.clone();
            transaction::clear(&root).map_err(ExecutionFailure::preserve)?;
            println!("Updated release files to {target}.");
            return Ok(());
        }

        let commit_message = active.plan.commit_message.clone().ok_or_else(|| {
            ExecutionFailure::preserve("release plan has no commit message".to_string())
        })?;
        if !resuming {
            confirm_release_step(
                &format!("Commit release files with \"{commit_message}\"?"),
                assume_yes,
            )
            .map_err(ExecutionFailure::preserve)?;
        }
        run_transaction_hook(active, "before_commit", !resuming)?;
        verify_expected_head(active).map_err(ExecutionFailure::preserve)?;
        let paths = active
            .plan
            .file_changes
            .iter()
            .map(|change| change.path.clone())
            .collect::<Vec<_>>();
        git_add_release_files(&root, &paths).map_err(ExecutionFailure::rollback)?;
        verify_staged_plan(active).map_err(ExecutionFailure::rollback)?;
        transaction::start_commit(active).map_err(ExecutionFailure::preserve)?;
        git::git(
            &root,
            &["commit", "--cleanup=verbatim", "-m", &commit_message],
        )
        .map_err(ExecutionFailure::rollback)?;
        let release_head = git::current_head(&root).map_err(ExecutionFailure::preserve)?;
        if !is_expected_release_commit(active, &release_head).map_err(ExecutionFailure::preserve)? {
            return Err(ExecutionFailure::preserve(
                "the created commit does not match the exact release plan; preserving the transaction for inspection"
                    .to_string(),
            ));
        }
        transaction::set_release_head(active, release_head).map_err(ExecutionFailure::preserve)?;
    }

    if active.stage == TransactionStage::Committed {
        run_transaction_hook(active, "after_commit", !resuming)?;
        let tag_name = active.plan.tag_name.clone().ok_or_else(|| {
            ExecutionFailure::preserve("release plan has no tag name".to_string())
        })?;
        if !resuming {
            confirm_release_step(&format!("Create tag {tag_name}?"), assume_yes)
                .map_err(ExecutionFailure::preserve)?;
        }
        run_transaction_hook(active, "before_tag", !resuming)?;
        let release_head = required_release_head(active).map_err(ExecutionFailure::preserve)?;
        let tag_object = match active.tag_object.clone() {
            Some(tag_object) => tag_object,
            None => {
                if transaction::tag_object(&root, &tag_name)
                    .map_err(ExecutionFailure::preserve)?
                    .is_some()
                {
                    return Err(ExecutionFailure::preserve(format!(
                        "tag {tag_name} appeared before Verso created it; refusing to claim it"
                    )));
                }
                let tag_object = git::create_annotated_tag_object(&root, &tag_name, &release_head)
                    .map_err(ExecutionFailure::rollback)?;
                transaction::set_tag_object(active, tag_object.clone())
                    .map_err(ExecutionFailure::preserve)?;
                tag_object
            }
        };
        match transaction::tag_object(&root, &tag_name).map_err(ExecutionFailure::preserve)? {
            Some(object) if object == tag_object => {}
            Some(object) => {
                return Err(ExecutionFailure::preserve(format!(
                    "tag {tag_name} has object {object}, expected {tag_object}"
                )));
            }
            None => {
                git::create_tag_ref(&root, &tag_name, &tag_object)
                    .map_err(ExecutionFailure::rollback)?;
            }
        }
        verify_release_tag(active).map_err(ExecutionFailure::preserve)?;
        transaction::set_stage(active, TransactionStage::Tagged)
            .map_err(ExecutionFailure::preserve)?;
    }

    if active.stage == TransactionStage::Tagged {
        run_transaction_hook(active, "after_tag", !resuming)?;
        if !resuming {
            confirm_release_step("Push release commit and tag?", assume_yes)
                .map_err(ExecutionFailure::preserve)?;
        }
        run_transaction_hook(active, "before_push", !resuming)?;
        let tag_name = active.plan.tag_name.clone().ok_or_else(|| {
            ExecutionFailure::preserve("release plan has no tag name".to_string())
        })?;
        let upstream = active.upstream.clone().ok_or_else(|| {
            ExecutionFailure::preserve("release transaction has no upstream".to_string())
        })?;
        let release_head = required_release_head(active).map_err(ExecutionFailure::preserve)?;
        verify_release_tag(active).map_err(ExecutionFailure::preserve)?;
        let tag_object = active.tag_object.clone().ok_or_else(|| {
            ExecutionFailure::preserve("release transaction is missing its tag object".to_string())
        })?;
        transaction::start_push(active).map_err(ExecutionFailure::preserve)?;
        if let Err(error) =
            git::push_release(&root, &upstream, &release_head, &tag_object, &tag_name)
        {
            let journal_error = transaction::fail_push(active).err();
            let mut message = format!(
                "{error}\n\nnote: Local release commit and tag were created.\nhelp: fix the remote problem, then run `verso resume`."
            );
            if let Some(journal_error) = journal_error {
                message.push_str(&format!(
                    "\nnote: failed to record push failure: {journal_error}"
                ));
            }
            return Err(ExecutionFailure::preserve(message));
        }
        transaction::set_stage(active, TransactionStage::Pushed)
            .map_err(ExecutionFailure::preserve)?;
    }

    if active.stage == TransactionStage::Pushed {
        run_transaction_hook(active, "after_push", false)?;
        transaction::clear(&root).map_err(ExecutionFailure::preserve)?;
    }

    Ok(())
}

fn run_transaction_hook(
    active: &mut ReleaseTransaction,
    name: &str,
    rollback_on_failure: bool,
) -> Result<(), ExecutionFailure> {
    let Some(command) = active
        .plan
        .hooks
        .iter()
        .find(|hook| hook.name == name)
        .map(|hook| hook.command.clone())
    else {
        return Ok(());
    };
    if !transaction::start_hook(active, name).map_err(ExecutionFailure::preserve)? {
        return Ok(());
    }
    if let Err(message) = run_hook(&active.plan.root, name, &Some(command)) {
        return Err(if rollback_on_failure {
            ExecutionFailure::rollback(message)
        } else {
            ExecutionFailure::preserve(message)
        });
    }
    transaction::finish_hook(active, name).map_err(ExecutionFailure::preserve)?;
    if active.stage == TransactionStage::Pushed {
        Ok(())
    } else {
        verify_expected_head(active).map_err(ExecutionFailure::preserve)
    }
}

fn reconcile_transaction(active: &mut ReleaseTransaction) -> Result<(), String> {
    let root = active.plan.root.clone();
    if active.stage == TransactionStage::Planned {
        match transaction::files_state(active)? {
            FilesState::Applied => {
                transaction::set_stage(active, TransactionStage::FilesApplied)?;
            }
            FilesState::Original => {}
            FilesState::Mixed => {
                transaction::apply_files(active)?;
                transaction::set_stage(active, TransactionStage::FilesApplied)?;
            }
        }
    }
    if active.stage == TransactionStage::FilesApplied {
        match transaction::files_state(active)? {
            FilesState::Applied => {}
            FilesState::Original => {
                if !active.plan.file_changes.is_empty() {
                    return Err(
                        "release files were restored while the transaction remained active"
                            .to_string(),
                    );
                }
            }
            FilesState::Mixed => {
                return Err(
                    "release files are only partially applied; refusing to commit a mixed version set\n\nhelp: run `verso abort` to complete the rollback."
                        .to_string(),
                );
            }
        }
        let current_head = git::current_head(&root)?;
        if current_head != active.before_head {
            if is_expected_release_commit(active, &current_head)? {
                transaction::set_release_head(active, current_head)?;
            } else {
                return Err(format!(
                    "HEAD moved from {} to {} during the transaction",
                    active.before_head, current_head
                ));
            }
        }
    }
    if active.stage == TransactionStage::Committed {
        let release_head = required_release_head(active)?;
        let current_head = git::current_head(&root)?;
        if current_head != release_head {
            return Err(format!(
                "HEAD moved from release commit {release_head} to {current_head}"
            ));
        }
        if let Some(tag_name) = &active.plan.tag_name {
            match (
                active.tag_object.as_deref(),
                transaction::tag_object(&root, tag_name)?,
            ) {
                (Some(expected), Some(actual)) if actual == expected => {
                    verify_release_tag(active)?;
                    transaction::set_stage(active, TransactionStage::Tagged)?;
                }
                (Some(_), None) | (None, None) => {}
                (Some(expected), Some(actual)) => {
                    return Err(format!(
                        "tag {tag_name} has object {actual}, expected {expected}"
                    ));
                }
                (None, Some(_)) => {
                    return Err(format!(
                        "tag {tag_name} appeared before Verso created it; refusing to claim it"
                    ));
                }
            }
        }
    }
    if active.stage == TransactionStage::Tagged {
        let release_head = required_release_head(active)?;
        let current_head = git::current_head(&root)?;
        if current_head != release_head {
            return Err(format!(
                "HEAD moved from release commit {release_head} to {current_head}"
            ));
        }
        verify_release_tag(active)?;
    }
    Ok(())
}

fn verify_expected_head(active: &ReleaseTransaction) -> Result<(), String> {
    let current_head = git::current_head(&active.plan.root)?;
    let expected = active
        .release_head
        .as_deref()
        .unwrap_or(&active.before_head);
    if current_head == expected {
        Ok(())
    } else {
        Err(format!(
            "HEAD moved from expected commit {expected} to {current_head}; refusing to continue the transaction"
        ))
    }
}

fn verify_staged_plan(active: &ReleaseTransaction) -> Result<(), String> {
    let root = &active.plan.root;
    let output = git::git(root, &["diff", "--cached", "--name-only", "-z"])?;
    let mut actual = output
        .stdout
        .split('\0')
        .filter(|path| !path.is_empty())
        .map(str::to_owned)
        .collect::<Vec<_>>();
    let mut expected = relative_path_strings(
        root,
        &active
            .plan
            .file_changes
            .iter()
            .map(|change| change.path.clone())
            .collect::<Vec<_>>(),
    );
    actual.sort();
    expected.sort();
    if actual != expected {
        return Err(format!(
            "Git index does not match the release plan; staged paths: {}; planned paths: {}",
            actual.join(", "),
            expected.join(", ")
        ));
    }
    for change in &active.plan.file_changes {
        let relative = relative_path(root, &change.path).display().to_string();
        let expected = expected_plan_entry(active, change)?;
        let staged = index_entry(root, &relative)?
            .ok_or_else(|| format!("planned path {relative} is missing from the Git index"))?;
        if staged != expected {
            return Err(format!(
                "staged mode or content for {relative} does not match the release plan"
            ));
        }
    }
    Ok(())
}

fn is_expected_release_commit(
    active: &ReleaseTransaction,
    current_head: &str,
) -> Result<bool, String> {
    if active.plan.mode != PlanMode::Release {
        return Ok(false);
    }
    let output = git::git(
        &active.plan.root,
        &["log", "-1", "--format=%P%x00%B%x00", current_head],
    )?;
    let mut fields = output.stdout.splitn(3, '\0');
    let (Some(parents), Some(message), Some(_)) = (fields.next(), fields.next(), fields.next())
    else {
        return Ok(false);
    };
    let expected_message = format!("{}\n", active.plan.commit_message.as_deref().unwrap_or(""));
    if parents.trim() != active.before_head || message != expected_message {
        return Ok(false);
    }
    let output = git::git(
        &active.plan.root,
        &[
            "diff",
            "--name-only",
            "-z",
            &active.before_head,
            current_head,
            "--",
        ],
    )?;
    let mut actual = output
        .stdout
        .split('\0')
        .filter(|path| !path.is_empty())
        .map(str::to_owned)
        .collect::<Vec<_>>();
    let mut expected = relative_path_strings(
        &active.plan.root,
        &active
            .plan
            .file_changes
            .iter()
            .map(|change| change.path.clone())
            .collect::<Vec<_>>(),
    );
    actual.sort();
    expected.sort();
    if actual != expected {
        return Ok(false);
    }
    for change in &active.plan.file_changes {
        let relative = relative_path(&active.plan.root, &change.path)
            .display()
            .to_string();
        if tree_entry(&active.plan.root, current_head, &relative)?
            != Some(expected_plan_entry(active, change)?)
        {
            return Ok(false);
        }
    }
    Ok(true)
}

fn expected_plan_entry(
    active: &ReleaseTransaction,
    change: &crate::plan::PlannedFileChange,
) -> Result<(String, String), String> {
    let relative = relative_path(&active.plan.root, &change.path)
        .display()
        .to_string();
    let mode = tree_entry(&active.plan.root, &active.before_head, &relative)?
        .map(|(mode, _)| mode)
        .unwrap_or_else(|| "100644".to_string());
    let object = git::hash_object(&active.plan.root, &relative, change.after.as_bytes())?;
    Ok((mode, object))
}

fn index_entry(root: &Path, relative: &str) -> Result<Option<(String, String)>, String> {
    let output = git::git(root, &["ls-files", "--stage", "--", relative])?;
    parse_git_entry(&output.stdout, false)
}

fn tree_entry(
    root: &Path,
    revision: &str,
    relative: &str,
) -> Result<Option<(String, String)>, String> {
    let output = git::git(root, &["ls-tree", revision, "--", relative])?;
    parse_git_entry(&output.stdout, true)
}

fn parse_git_entry(output: &str, has_type: bool) -> Result<Option<(String, String)>, String> {
    let Some(metadata) = output.split_once('\t').map(|(metadata, _)| metadata) else {
        return Ok(None);
    };
    let fields = metadata.split_whitespace().collect::<Vec<_>>();
    let object_index = usize::from(has_type) + 1;
    let mode = fields.first().copied();
    let object = fields.get(object_index).copied();
    match (mode, object) {
        (Some(mode), Some(object)) => Ok(Some((mode.to_string(), object.to_string()))),
        _ => Err(format!("failed to parse Git tree entry: {output:?}")),
    }
}

fn required_release_head(active: &ReleaseTransaction) -> Result<String, String> {
    active
        .release_head
        .clone()
        .ok_or_else(|| "release transaction is missing its release commit".to_string())
}

fn verify_release_tag(active: &ReleaseTransaction) -> Result<(), String> {
    let root = &active.plan.root;
    let tag_name = active
        .plan
        .tag_name
        .as_deref()
        .ok_or_else(|| "release plan has no tag name".to_string())?;
    let tag_object = active
        .tag_object
        .as_deref()
        .ok_or_else(|| "release transaction is missing its tag object".to_string())?;
    let actual_object = transaction::tag_object(root, tag_name)?
        .ok_or_else(|| format!("release tag {tag_name} is missing"))?;
    if actual_object != tag_object {
        return Err(format!(
            "tag {tag_name} has object {actual_object}, expected {tag_object}"
        ));
    }
    let release_head = required_release_head(active)?;
    if transaction::tag_target(root, tag_name)?.as_deref() != Some(release_head.as_str()) {
        return Err(format!(
            "tag {tag_name} does not point to release commit {release_head}"
        ));
    }
    Ok(())
}

fn verify_clean_for_plan(plan: &ReleasePlan, config: &config::Config) -> Result<(), String> {
    let root = &plan.root;
    if plan.mode == PlanMode::Release && config.git.require_clean_worktree {
        if !git::is_worktree_clean(root)? {
            return Err(dirty_worktree_error());
        }
        return Ok(());
    }
    if plan.mode == PlanMode::Release && !git::is_index_clean(root)? {
        return Err(dirty_index_error());
    }
    let paths = relative_path_strings(
        root,
        &plan
            .file_changes
            .iter()
            .map(|change| change.path.clone())
            .collect::<Vec<_>>(),
    );
    if !git::are_paths_clean(root, &paths)? {
        return Err(dirty_release_files_error());
    }
    Ok(())
}

fn abort_transaction(active: &mut ReleaseTransaction, verify_push: bool) -> Result<(), String> {
    if active.stage == TransactionStage::Pushed {
        return Err(
            "cannot abort a release that was pushed\n\nhelp: finish it with `verso resume`."
                .to_string(),
        );
    }
    let root = active.plan.root.clone();
    if active.push_started {
        return Err(
            "cannot automatically abort after a push was started because its outcome may be unknown\n\nhelp: run `verso resume` to retry the exact release refs, or inspect the remote and recover manually."
                .to_string(),
        );
    }
    let current_head = git::current_head(&root)?;
    if active.release_head.is_none()
        && active.plan.mode == PlanMode::Release
        && current_head != active.before_head
        && is_expected_release_commit(active, &current_head)?
    {
        active.release_head = Some(current_head.clone());
    }
    let verify_remote = verify_push
        && active.plan.mode == PlanMode::Release
        && (active.commit_started
            || active.release_head.is_some()
            || active.tag_object.is_some()
            || active.active_hook.is_some()
            || !active.completed_hooks.is_empty());
    transaction::start_abort(active, verify_remote)?;
    if active.abort_remote_check {
        if remote_may_contain_release(active)? {
            active.stage = TransactionStage::Pushed;
            active.aborting = false;
            active.abort_remote_check = false;
            transaction::save(active)?;
            return Err(
                "cannot abort because the exact release refs are already present on the remote\n\nhelp: run `verso resume` to finish the pushed release."
                    .to_string(),
            );
        }
        transaction::finish_abort_remote_check(active)?;
    }

    let release_head = active.release_head.clone();
    if let Some(release_head) = &release_head {
        if current_head != *release_head && current_head != active.before_head {
            return Err(format!(
                "refusing to abort because HEAD moved to {current_head}; expected {release_head} or {}",
                active.before_head
            ));
        }
    } else if current_head != active.before_head {
        return Err(format!(
            "refusing to abort because HEAD moved from {} to {current_head}",
            active.before_head
        ));
    }
    transaction::files_state(active)?;

    if let (Some(tag_name), Some(expected_object)) = (&active.plan.tag_name, &active.tag_object) {
        if let Some(actual_object) = transaction::tag_object(&root, tag_name)? {
            if actual_object != *expected_object {
                return Err(format!(
                    "refusing to delete tag {tag_name}; it has object {actual_object}, expected {expected_object}"
                ));
            }
            git::delete_tag_ref(&root, tag_name, expected_object)?;
        }
    }
    if current_head != active.before_head {
        git::compare_and_swap_head(&root, &active.before_head, &current_head)?;
    }
    let paths = relative_path_strings(
        &root,
        &active
            .plan
            .file_changes
            .iter()
            .map(|change| change.path.clone())
            .collect::<Vec<_>>(),
    );
    git::unstage_paths(&root, &paths)?;
    transaction::restore_files(active)?;
    transaction::clear(&root)
}

fn remote_contains_release(active: &ReleaseTransaction) -> Result<bool, String> {
    let release_head = required_release_head(active)?;
    let tag_object = active
        .tag_object
        .as_deref()
        .ok_or_else(|| "release transaction is missing its tag object".to_string())?;
    let refs = remote_release_refs(active)?;
    remote_release_is_published(active, &refs, &release_head, tag_object)
}

fn remote_may_contain_release(active: &ReleaseTransaction) -> Result<bool, String> {
    let refs = remote_release_refs(active)?;
    if let (Some(release_head), Some(tag_object)) =
        (active.release_head.as_deref(), active.tag_object.as_deref())
    {
        if remote_release_is_published(active, &refs, release_head, tag_object)? {
            return Ok(true);
        }
    }
    if refs.tag_object.is_some() || refs.tag_target.is_some() {
        return Err(
            "the planned release tag already exists on the remote but was not created by this transaction\n\nhelp: inspect the remote tag before deciding how to recover."
                .to_string(),
        );
    }
    let upstream = active
        .upstream
        .as_ref()
        .ok_or_else(|| "release transaction has no upstream".to_string())?;
    if let Some(release_head) = active.release_head.as_deref() {
        if let Some(branch) = refs.branch.as_deref() {
            if branch == release_head
                || git::remote_branch_contains(&active.plan.root, upstream, release_head, branch)?
            {
                return Err(format!(
                    "remote branch {} already contains release commit {release_head}\n\nhelp: inspect the remote refs before deciding how to recover.",
                    upstream.branch
                ));
            }
        }
    } else if refs
        .branch
        .as_deref()
        .is_some_and(|branch| branch != upstream.branch_target)
    {
        return Err(format!(
            "remote branch {} moved during the release transaction\n\nhelp: inspect the remote refs before deciding how to recover.",
            upstream.branch
        ));
    }
    Ok(false)
}

struct RemoteReleaseRefs {
    branch: Option<String>,
    tag_object: Option<String>,
    tag_target: Option<String>,
}

fn remote_release_refs(active: &ReleaseTransaction) -> Result<RemoteReleaseRefs, String> {
    let upstream = active
        .upstream
        .as_ref()
        .ok_or_else(|| "release transaction has no upstream".to_string())?;
    let branch_ref = format!("refs/heads/{}", upstream.branch);
    let tag_name = active
        .plan
        .tag_name
        .as_deref()
        .ok_or_else(|| "release plan has no tag name".to_string())?;
    let tag_ref = format!("refs/tags/{tag_name}");
    let peeled_tag_ref = format!("refs/tags/{tag_name}^{{}}");
    let output = git::git(
        &active.plan.root,
        &[
            "ls-remote",
            "--",
            &upstream.push_url,
            &branch_ref,
            &tag_ref,
            &peeled_tag_ref,
        ],
    )
    .map_err(|error| {
        format!(
            "cannot inspect the remote release refs: {error}\n\nhelp: restore remote access, then retry the current recovery command."
        )
    })?;
    let mut branch_target = None;
    let mut tag_object_target = None;
    let mut tag_target = None;
    for line in output.stdout.lines() {
        let mut fields = line.split_whitespace();
        let object = fields.next();
        match fields.next() {
            Some(reference) if reference == branch_ref => branch_target = object.map(str::to_owned),
            Some(reference) if reference == tag_ref => {
                tag_object_target = object.map(str::to_owned)
            }
            Some(reference) if reference == peeled_tag_ref => {
                tag_target = object.map(str::to_owned)
            }
            _ => {}
        }
    }
    Ok(RemoteReleaseRefs {
        branch: branch_target,
        tag_object: tag_object_target,
        tag_target,
    })
}

fn remote_release_is_published(
    active: &ReleaseTransaction,
    refs: &RemoteReleaseRefs,
    release_head: &str,
    tag_object: &str,
) -> Result<bool, String> {
    let upstream = active
        .upstream
        .as_ref()
        .ok_or_else(|| "release transaction has no upstream".to_string())?;
    match (
        refs.branch.as_deref(),
        refs.tag_object.as_deref(),
        refs.tag_target.as_deref(),
    ) {
        (Some(branch), Some(object), Some(tag))
            if object == tag_object && tag == release_head =>
        {
            if branch == release_head
                || git::remote_branch_contains(
                    &active.plan.root,
                    upstream,
                    release_head,
                    branch,
                )?
            {
                Ok(true)
            } else {
                Err(format!(
                    "remote tag is published but branch {} does not contain release commit {release_head}\n\nhelp: inspect the remote refs and recover manually before resuming.",
                    upstream.branch
                ))
            }
        }
        (branch, None, None) if branch != Some(release_head) => Ok(false),
        (branch, object, tag) => Err(format!(
            "remote release refs are partial or moved; expected exact tag object {tag_object}, peeled tag at {release_head}, and a remote branch, found branch {}, tag object {}, and peeled tag {}\n\nhelp: inspect the remote refs and recover manually before resuming.",
            branch.unwrap_or("missing"),
            object.unwrap_or("missing"),
            tag.unwrap_or("missing")
        )),
    }
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
            false => Err(RELEASE_CANCELLED.to_string()),
        };
    }

    let answer = read_prompt(&format!("{question} [Y/n] "))?;
    match answer.as_str() {
        "" | "y" | "Y" | "yes" | "YES" | "Yes" => Ok(()),
        _ => Err(RELEASE_CANCELLED.to_string()),
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
            false => Err(RELEASE_CANCELLED.to_string()),
        };
    }

    let answer = read_prompt(&format!("{question} [y/N] "))?;
    match answer.as_str() {
        "y" | "Y" | "yes" | "YES" | "Yes" => Ok(()),
        _ => Err(RELEASE_CANCELLED.to_string()),
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
            RELEASE_CANCELLED.to_string()
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

fn dry_run_warnings(root: &Path, tag_name: Option<&str>) -> Result<Vec<String>, String> {
    let mut warnings = Vec::new();

    if !git::is_worktree_clean(root)? {
        warnings.push(dirty_worktree_warning());
    }
    if let Some(tag_name) = tag_name {
        if git::tag_exists(root, tag_name)? {
            warnings.push(existing_tag_warning(tag_name));
        }
    }

    Ok(warnings)
}

fn dirty_worktree_warning() -> String {
    [
        "working tree is dirty",
        "",
        "note: dry-run will not modify files.",
        "help: commit, stash, or revert local changes before running a real release.",
    ]
    .join("\n")
}

fn existing_tag_warning(tag_name: &str) -> String {
    format!(
        "tag {tag_name} already exists\n\nhelp: choose a different version or inspect it with: git show {tag_name}"
    )
}

fn dirty_worktree_error() -> String {
    [
        "working tree is dirty",
        "",
        "help: commit, stash, or revert local changes before releasing.",
        "note: run with --dry-run to preview without requiring a clean worktree.",
        "note: if dirty releases are intentional, set git.require_clean_worktree = false in verso.toml.",
    ]
    .join("\n")
}

fn dirty_release_files_error() -> String {
    [
        "release files are dirty",
        "",
        "help: commit, stash, or revert changes to package manifests, Cargo manifests and lockfiles, or the changelog before releasing.",
        "note: git.require_clean_worktree = false only permits changes to unrelated files.",
    ]
    .join("\n")
}

fn dirty_index_error() -> String {
    [
        "Git index is not clean",
        "",
        "help: commit or unstage existing staged changes before releasing.",
        "note: Verso will not include unrelated staged files in a release commit.",
    ]
    .join("\n")
}

fn existing_tag_error(tag_name: &str) -> String {
    [
        format!("tag {tag_name} already exists"),
        String::new(),
        "help: choose a different version, or inspect the existing tag before continuing."
            .to_string(),
        format!("note: inspect it with: git show {tag_name}"),
        format!("note: if it was created by mistake, delete it with: git tag -d {tag_name}"),
    ]
    .join("\n")
}

pub(crate) fn verify_cargo_manifest_versions(
    root: &Path,
    manifest_files: &[PathBuf],
    expected: &Version,
) -> Result<(), String> {
    let mut mismatches = Vec::new();

    for manifest_path in manifest_files {
        crate::workspace::validate_release_path(root, manifest_path)?;
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
        "inconsistent versions: {}; configured Cargo manifests must match release version {expected}. Use a separate Verso config for each independent version group.",
        mismatches.join("; ")
    ))
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
    fn prerelease_target_choices_include_the_current_stable_version() -> Result<(), String> {
        let current = Version::parse("1.0.0-rc.2").map_err(|error| error.to_string())?;

        assert_eq!(
            target_version_choices(&current).first(),
            Some(&TargetVersionChoice::Stable(Version::new(1, 0, 0)))
        );
        Ok(())
    }

    #[test]
    fn dry_run_warnings_include_actionable_help() {
        let dirty = dirty_worktree_warning();
        let tag = existing_tag_warning("v1.2.3");

        assert!(dirty.contains("note:"));
        assert!(dirty.contains("help:"));
        assert!(tag.contains("help:"));
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
