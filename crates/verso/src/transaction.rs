use crate::{
    git::{self, ReleaseUpstream},
    plan::ReleasePlan,
    rollback::atomic_write,
};
use fs2::FileExt;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::{
    fs::{self, File, OpenOptions},
    io::Write,
    path::{Component, Path, PathBuf},
};

const SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum TransactionStage {
    Planned,
    FilesApplied,
    Committed,
    Tagged,
    Pushed,
}

impl TransactionStage {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Planned => "planned",
            Self::FilesApplied => "files-applied",
            Self::Committed => "committed",
            Self::Tagged => "tagged",
            Self::Pushed => "pushed",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReleaseTransaction {
    schema_version: u32,
    pub stage: TransactionStage,
    pub plan: ReleasePlan,
    pub before_head: String,
    pub release_head: Option<String>,
    #[serde(default)]
    pub tag_object: Option<String>,
    pub upstream: Option<ReleaseUpstream>,
    pub completed_hooks: Vec<String>,
    pub active_hook: Option<String>,
    #[serde(default)]
    pub commit_started: bool,
    pub push_started: bool,
    pub push_failed: bool,
    #[serde(default)]
    pub aborting: bool,
    #[serde(default)]
    pub abort_remote_check: bool,
}

pub struct TransactionLock {
    _file: File,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FilesState {
    Original,
    Applied,
    Mixed,
}

pub fn lock(root: &Path) -> Result<TransactionLock, String> {
    let directory = transaction_directory(root)?;
    create_transaction_directory(&directory)?;
    let path = directory.join("lock");
    let file = OpenOptions::new()
        .create(true)
        .read(true)
        .write(true)
        .truncate(false)
        .open(&path)
        .map_err(|error| {
            format!(
                "failed to open transaction lock {}: {error}",
                path.display()
            )
        })?;
    FileExt::try_lock_exclusive(&file).map_err(|error| {
        format!(
            "another Verso transaction command is running: {error}\n\nhelp: wait for it to finish, then retry."
        )
    })?;
    Ok(TransactionLock { _file: file })
}

pub fn begin(
    root: &Path,
    plan: ReleasePlan,
    before_head: String,
    upstream: Option<ReleaseUpstream>,
) -> Result<ReleaseTransaction, String> {
    validate_plan(root, &plan)?;
    let transaction = ReleaseTransaction {
        schema_version: SCHEMA_VERSION,
        stage: TransactionStage::Planned,
        plan,
        before_head,
        release_head: None,
        tag_object: None,
        upstream,
        completed_hooks: Vec::new(),
        active_hook: None,
        commit_started: false,
        push_started: false,
        push_failed: false,
        aborting: false,
        abort_remote_check: false,
    };
    create(root, &transaction)?;
    Ok(transaction)
}

pub fn load(root: &Path) -> Result<Option<ReleaseTransaction>, String> {
    let path = active_path(root)?;
    let contents = match fs::read_to_string(&path) {
        Ok(contents) => contents,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => {
            return Err(format!(
                "failed to read transaction journal {}: {error}",
                path.display()
            ));
        }
    };
    let transaction = serde_json::from_str::<ReleaseTransaction>(&contents).map_err(|error| {
        format!(
            "failed to parse transaction journal {}: {error}\n\nhelp: preserve the journal and inspect it before making manual changes.",
            path.display()
        )
    })?;
    if transaction.schema_version != SCHEMA_VERSION {
        return Err(format!(
            "unsupported transaction journal schema {}; expected {SCHEMA_VERSION}",
            transaction.schema_version
        ));
    }
    validate_plan(root, &transaction.plan)?;
    Ok(Some(transaction))
}

pub fn require(root: &Path) -> Result<ReleaseTransaction, String> {
    load(root)?.ok_or_else(|| {
        "no active Verso transaction\n\nhelp: start a release or bump before using resume or abort."
            .to_string()
    })
}

pub fn save(transaction: &ReleaseTransaction) -> Result<(), String> {
    let bytes = serde_json::to_vec_pretty(transaction)
        .map_err(|error| format!("failed to serialize transaction journal: {error}"))?;
    let path = active_path(&transaction.plan.root)?;
    atomic_write(&path, &bytes)
        .map_err(|error| format!("failed to save transaction journal: {error}"))
}

pub fn clear(root: &Path) -> Result<(), String> {
    force_clear(root).map(|_| ())
}

pub fn force_clear(root: &Path) -> Result<bool, String> {
    let path = active_path(root)?;
    match fs::remove_file(&path) {
        Ok(()) => {
            sync_parent(path.parent())?;
            Ok(true)
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(format!(
            "failed to remove transaction journal {}: {error}",
            path.display()
        )),
    }
}

pub fn set_stage(
    transaction: &mut ReleaseTransaction,
    stage: TransactionStage,
) -> Result<(), String> {
    transaction.stage = stage;
    save(transaction)
}

pub fn set_release_head(
    transaction: &mut ReleaseTransaction,
    release_head: String,
) -> Result<(), String> {
    transaction.release_head = Some(release_head);
    set_stage(transaction, TransactionStage::Committed)
}

pub fn set_tag_object(
    transaction: &mut ReleaseTransaction,
    tag_object: String,
) -> Result<(), String> {
    transaction.tag_object = Some(tag_object);
    save(transaction)
}

pub fn start_hook(transaction: &mut ReleaseTransaction, name: &str) -> Result<bool, String> {
    if transaction.completed_hooks.iter().any(|hook| hook == name) {
        return Ok(false);
    }
    transaction.active_hook = Some(name.to_owned());
    save(transaction)?;
    Ok(true)
}

pub fn finish_hook(transaction: &mut ReleaseTransaction, name: &str) -> Result<(), String> {
    if !transaction.completed_hooks.iter().any(|hook| hook == name) {
        transaction.completed_hooks.push(name.to_owned());
    }
    transaction.active_hook = None;
    save(transaction)
}

pub fn retry_active_hook(transaction: &mut ReleaseTransaction) -> Result<(), String> {
    transaction.active_hook = None;
    save(transaction)
}

pub fn start_push(transaction: &mut ReleaseTransaction) -> Result<(), String> {
    transaction.push_started = true;
    transaction.push_failed = false;
    save(transaction)
}

pub fn start_commit(transaction: &mut ReleaseTransaction) -> Result<(), String> {
    transaction.commit_started = true;
    save(transaction)
}

pub fn fail_push(transaction: &mut ReleaseTransaction) -> Result<(), String> {
    transaction.push_failed = true;
    save(transaction)
}

pub fn start_abort(
    transaction: &mut ReleaseTransaction,
    verify_remote: bool,
) -> Result<(), String> {
    if !transaction.aborting {
        transaction.aborting = true;
        transaction.abort_remote_check = verify_remote;
        save(transaction)?;
    }
    Ok(())
}

pub fn finish_abort_remote_check(transaction: &mut ReleaseTransaction) -> Result<(), String> {
    transaction.abort_remote_check = false;
    save(transaction)
}

pub fn apply_files(transaction: &ReleaseTransaction) -> Result<(), String> {
    files_state(transaction)?;
    for change in &transaction.plan.file_changes {
        validate_change_path(&transaction.plan.root, &change.path)?;
        let current = read_optional_text(&change.path)?;
        if current == Some(change.after.clone()) {
            continue;
        }
        if current != change.before {
            return Err(stale_file_error(&change.path));
        }
        atomic_write(&change.path, change.after.as_bytes())
            .map_err(|error| format!("failed to apply {}: {error}", change.path.display()))?;
    }
    Ok(())
}

pub fn files_state(transaction: &ReleaseTransaction) -> Result<FilesState, String> {
    let mut original = 0;
    let mut applied = 0;
    for change in &transaction.plan.file_changes {
        validate_change_path(&transaction.plan.root, &change.path)?;
        let current = read_optional_text(&change.path)?;
        if current == change.before {
            original += 1;
        } else if current.as_deref() == Some(change.after.as_str()) {
            applied += 1;
        } else {
            return Err(stale_file_error(&change.path));
        }
    }
    Ok(match (original, applied) {
        (_, 0) => FilesState::Original,
        (0, _) => FilesState::Applied,
        _ => FilesState::Mixed,
    })
}

pub fn restore_files(transaction: &ReleaseTransaction) -> Result<Vec<PathBuf>, String> {
    let mut restored = Vec::new();
    let mut errors = Vec::new();
    for change in transaction.plan.file_changes.iter().rev() {
        let result = (|| {
            validate_change_path(&transaction.plan.root, &change.path)?;
            let current = read_optional_text(&change.path)?;
            if current == change.before {
                return Ok(());
            }
            if current.as_deref() != Some(change.after.as_str()) {
                return Err(format!(
                    "refusing to overwrite edits made after interruption in {}",
                    change.path.display()
                ));
            }
            match &change.before {
                Some(before) => atomic_write(&change.path, before.as_bytes()),
                None => match fs::remove_file(&change.path) {
                    Ok(()) => sync_parent(change.path.parent()),
                    Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
                    Err(error) => Err(format!("failed to remove file: {error}")),
                },
            }
        })();
        match result {
            Ok(()) => restored.push(change.path.clone()),
            Err(error) => errors.push(format!("{}: {error}", change.path.display())),
        }
    }
    if errors.is_empty() {
        Ok(restored)
    } else {
        Err(errors.join("; "))
    }
}

pub fn render_status(transaction: Option<&ReleaseTransaction>, json_output: bool) -> String {
    let Some(transaction) = transaction else {
        return if json_output {
            serde_json::to_string_pretty(&json!({ "active": false }))
                .expect("static transaction status should serialize")
        } else {
            "No active Verso transaction.\n".to_string()
        };
    };
    if json_output {
        return serde_json::to_string_pretty(&json!({
            "active": true,
            "operation": transaction.plan.mode.as_str(),
            "group": transaction.plan.group,
            "stage": transaction.stage.as_str(),
            "currentVersion": transaction.plan.current_version.to_string(),
            "targetVersion": transaction.plan.target_version.to_string(),
            "activeHook": transaction.active_hook,
            "changedFiles": transaction.plan.file_changes.len(),
            "canAbort": transaction.stage != TransactionStage::Pushed && !transaction.push_started,
            "commitStarted": transaction.commit_started,
            "pushStarted": transaction.push_started,
            "pushFailed": transaction.push_failed,
            "aborting": transaction.aborting,
            "canForceAbort": true,
        }))
        .expect("transaction status should serialize");
    }
    let mut output = format!(
        "Verso transaction\n\nOperation: {}\nGroup: {}\nStage: {}\nVersion: {} -> {}\nChanged files: {}\n",
        transaction.plan.mode.as_str(),
        transaction.plan.group,
        transaction.stage.as_str(),
        transaction.plan.current_version,
        transaction.plan.target_version,
        transaction.plan.file_changes.len(),
    );
    if let Some(hook) = &transaction.active_hook {
        output.push_str(&format!("Interrupted hook: {hook}\n"));
    }
    if transaction.aborting {
        output.push_str("\nRun `verso abort` to continue the interrupted rollback");
    } else if transaction.active_hook.is_some() {
        output.push_str(
            "\nInspect the hook outcome, then run `verso resume --retry-hook` or `verso resume --skip-hook`",
        );
    } else {
        output.push_str("\nRun `verso resume` to continue");
    }
    if transaction.stage != TransactionStage::Pushed && !transaction.push_started {
        output.push_str(" or `verso abort` to roll back");
    }
    output.push_str(
        ".\nRun `verso abort --force` only to discard the journal after manual recovery.\n",
    );
    output
}

pub fn tag_target(root: &Path, tag: &str) -> Result<Option<String>, String> {
    if !git::tag_exists(root, tag)? {
        return Ok(None);
    }
    let reference = format!("refs/tags/{tag}^{{}}");
    Ok(Some(
        git::git(root, &["rev-parse", &reference])?
            .stdout
            .trim()
            .to_owned(),
    ))
}

pub fn tag_object(root: &Path, tag: &str) -> Result<Option<String>, String> {
    if !git::tag_exists(root, tag)? {
        return Ok(None);
    }
    let reference = format!("refs/tags/{tag}");
    Ok(Some(
        git::git(root, &["rev-parse", &reference])?
            .stdout
            .trim()
            .to_owned(),
    ))
}

fn create(root: &Path, transaction: &ReleaseTransaction) -> Result<(), String> {
    let path = active_path(root)?;
    let parent = path
        .parent()
        .ok_or_else(|| format!("{} has no parent directory", path.display()))?;
    create_transaction_directory(parent)?;
    let mut temporary = tempfile::NamedTempFile::new_in(parent)
        .map_err(|error| format!("failed to create transaction journal: {error}"))?;
    let bytes = serde_json::to_vec_pretty(transaction)
        .map_err(|error| format!("failed to serialize transaction journal: {error}"))?;
    temporary
        .write_all(&bytes)
        .and_then(|()| temporary.flush())
        .and_then(|()| temporary.as_file().sync_all())
        .map_err(|error| format!("failed to write transaction journal: {error}"))?;
    temporary.persist_noclobber(&path).map_err(|error| {
        if error.error.kind() == std::io::ErrorKind::AlreadyExists {
            "an active Verso transaction already exists\n\nhelp: run `verso status`, then `verso resume` or `verso abort`."
                .to_string()
        } else {
            format!("failed to create transaction journal: {}", error.error)
        }
    })?;
    sync_parent(Some(parent))
}

fn validate_plan(root: &Path, plan: &ReleasePlan) -> Result<(), String> {
    for change in &plan.file_changes {
        validate_change_path(&plan.root, &change.path)?;
    }
    let requested_metadata = common_metadata_directory(root)?;
    let plan_metadata = common_metadata_directory(&plan.root)?;
    if canonical_or_self(&requested_metadata) != canonical_or_self(&plan_metadata) {
        return Err("transaction belongs to a different Git repository".to_string());
    }
    Ok(())
}

fn validate_change_path(root: &Path, path: &Path) -> Result<(), String> {
    let relative = path.strip_prefix(root).ok();
    if !path.is_absolute()
        || relative.is_none_or(|relative| {
            relative.as_os_str().is_empty()
                || relative
                    .components()
                    .any(|component| matches!(component, Component::ParentDir | Component::CurDir))
        })
        || !path_is_within_root(path, root)
    {
        return Err(format!(
            "transaction contains an unsafe file path: {}",
            path.display()
        ));
    }
    Ok(())
}

fn path_is_within_root(path: &Path, root: &Path) -> bool {
    if path.is_symlink() {
        return false;
    }
    let canonical_root = canonical_or_self(root);
    let Ok(relative_path) = path.strip_prefix(root) else {
        return false;
    };
    let mut current = root.to_path_buf();
    for component in relative_path.components() {
        current.push(component.as_os_str());
        if current.is_symlink() {
            return false;
        }
    }
    let mut existing = path;
    while !existing.exists() {
        let Some(parent) = existing.parent() else {
            return false;
        };
        existing = parent;
    }
    existing
        .canonicalize()
        .is_ok_and(|existing| existing.starts_with(canonical_root))
}

fn canonical_or_self(path: &Path) -> PathBuf {
    path.canonicalize().unwrap_or_else(|_| path.to_path_buf())
}

fn stale_file_error(path: &Path) -> String {
    format!(
        "release plan is stale because {} changed after planning\n\nhelp: preserve your edit, run `verso abort`, then create a new plan.",
        path.display()
    )
}

fn read_optional_text(path: &Path) -> Result<Option<String>, String> {
    match fs::read_to_string(path) {
        Ok(contents) => Ok(Some(contents)),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(format!("failed to read {}: {error}", path.display())),
    }
}

fn common_metadata_directory(root: &Path) -> Result<PathBuf, String> {
    let output = git::git(root, &["rev-parse", "--git-common-dir"])?;
    let path = PathBuf::from(output.stdout.trim());
    Ok(if path.is_absolute() {
        path
    } else {
        root.join(path)
    })
}

fn transaction_directory(root: &Path) -> Result<PathBuf, String> {
    let directory = common_metadata_directory(root)?.join("verso");
    if directory.is_symlink() {
        return Err(format!(
            "transaction directory must not be a symbolic link: {}",
            directory.display()
        ));
    }
    Ok(directory)
}

fn active_path(root: &Path) -> Result<PathBuf, String> {
    let path = transaction_directory(root)?.join("active.json");
    if path.is_symlink() {
        return Err(format!(
            "transaction journal must not be a symbolic link: {}",
            path.display()
        ));
    }
    Ok(path)
}

fn create_transaction_directory(directory: &Path) -> Result<(), String> {
    let parent = directory
        .parent()
        .ok_or_else(|| format!("{} has no parent directory", directory.display()))?;
    if directory.exists() {
        sync_parent(Some(parent))?;
        return sync_parent(Some(directory));
    }
    match fs::create_dir(directory) {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => return Ok(()),
        Err(error) => {
            return Err(format!(
                "failed to create transaction directory {}: {error}",
                directory.display()
            ));
        }
    }
    sync_parent(Some(parent))?;
    sync_parent(Some(directory))
}

#[cfg(unix)]
fn sync_parent(parent: Option<&Path>) -> Result<(), String> {
    let Some(parent) = parent else {
        return Ok(());
    };
    File::open(parent)
        .and_then(|directory| directory.sync_all())
        .map_err(|error| format!("failed to sync transaction directory: {error}"))
}

#[cfg(not(unix))]
fn sync_parent(_parent: Option<&Path>) -> Result<(), String> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn empty_status_is_machine_readable() {
        let status: serde_json::Value =
            serde_json::from_str(&render_status(None, true)).expect("empty status should be JSON");
        assert_eq!(status["active"], false);
    }

    #[test]
    fn linked_worktrees_share_one_transaction_journal() -> Result<(), String> {
        let temp = TempDir::new().map_err(|error| error.to_string())?;
        let repository = temp.path().join("repository");
        let worktree = temp.path().join("worktree");
        fs::create_dir(&repository).map_err(|error| error.to_string())?;
        git::git(&repository, &["init"])?;
        git::git(&repository, &["config", "user.email", "test@example.com"])?;
        git::git(&repository, &["config", "user.name", "Test User"])?;
        fs::write(repository.join("README.md"), "test\n").map_err(|error| error.to_string())?;
        git::git(&repository, &["add", "README.md"])?;
        git::git(&repository, &["commit", "-m", "test: initial"])?;
        let worktree_path = worktree.to_string_lossy();
        git::git(
            &repository,
            &["worktree", "add", "-b", "test-worktree", &worktree_path],
        )?;

        let repository_path = active_path(&repository)?;
        let worktree_path = active_path(&worktree)?;
        fs::create_dir_all(repository_path.parent().expect("journal parent"))
            .map_err(|error| error.to_string())?;
        fs::write(&repository_path, "test").map_err(|error| error.to_string())?;
        assert_eq!(
            repository_path
                .canonicalize()
                .map_err(|error| error.to_string())?,
            worktree_path
                .canonicalize()
                .map_err(|error| error.to_string())?
        );
        Ok(())
    }
}
