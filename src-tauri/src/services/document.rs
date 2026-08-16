use crate::{
    models::workspace::{
        DocumentIndexUpdate, DocumentReadResult, ProjectModel, WorkspacePatch, WorkspaceSnapshot,
    },
    services::{
        markdown::refresh_document_index,
        watcher::{apply_project_changes, diff_project, WorkspaceChange},
        workspace::{
            normalize_relative_path, resolve_workspace_root, scan_canonical_root, scan_workspace,
            WorkspaceError,
        },
    },
};
use std::{
    collections::{BTreeSet, VecDeque},
    fmt, fs,
    io::{Read, Seek, SeekFrom, Write},
    path::{Path, PathBuf},
    sync::{Arc, RwLock},
    time::{Duration, Instant},
};

#[cfg(unix)]
use std::ffi::CString;

const HISTORY_LIMIT: usize = 512;
const RECOVERY_DIRECTORY: &str = ".tracedoc-recovery";
const RECOVERY_OWNER_SENTINEL: &str = ".owner";
const RECOVERY_OWNER_MAGIC: &[u8] = b"TDOWNER1\n";

#[derive(Clone, Default)]
pub struct WorkspaceSession {
    state: Arc<RwLock<SessionState>>,
}

#[derive(Default)]
struct SessionState {
    generation: u64,
    workspace: Option<ActiveWorkspace>,
}

struct ActiveWorkspace {
    root: PathBuf,
    project: ProjectModel,
    self_writes: std::collections::BTreeMap<String, SelfWrite>,
    revision: u64,
    history: VecDeque<WorkspacePatch>,
}

struct SelfWrite {
    content: String,
    expires_at: Instant,
}

#[derive(Clone, Debug)]
pub struct WorkspaceLease {
    generation: u64,
    root: PathBuf,
}

impl WorkspaceSession {
    pub fn current_revision(&self, generation: u64) -> Result<u64, DocumentError> {
        let state = self
            .state
            .read()
            .map_err(|_| DocumentError::new("The workspace session is unavailable"))?;
        if state.generation != generation {
            return Err(DocumentError::workspace_changed());
        }
        state
            .workspace
            .as_ref()
            .map(|workspace| workspace.revision)
            .ok_or_else(|| DocumentError::new("No workspace is currently open"))
    }

    pub fn activate(&self, root: PathBuf, project: ProjectModel) -> Result<u64, DocumentError> {
        let mut state = self
            .state
            .write()
            .map_err(|_| DocumentError::new("The workspace session is unavailable"))?;
        cleanup_workspace_recovery_directory(&root).map_err(|source| {
            DocumentError::new(format!(
                "Unable to clean stale TraceDoc save state before opening the workspace: {source}"
            ))
        })?;
        state.generation = state
            .generation
            .checked_add(1)
            .ok_or_else(|| DocumentError::new("The workspace generation is unavailable"))?;
        state.workspace = Some(ActiveWorkspace {
            root,
            project,
            self_writes: std::collections::BTreeMap::new(),
            revision: 1,
            history: VecDeque::new(),
        });
        Ok(state.generation)
    }

    pub fn snapshot(&self, expected_generation: u64) -> Result<WorkspaceSnapshot, DocumentError> {
        let state = self
            .state
            .read()
            .map_err(|_| DocumentError::new("The workspace session is unavailable"))?;
        if state.generation != expected_generation {
            return Err(DocumentError::workspace_changed());
        }
        let workspace = state
            .workspace
            .as_ref()
            .ok_or_else(|| DocumentError::new("No workspace is currently open"))?;
        Ok(WorkspaceSnapshot {
            workspace_generation: expected_generation,
            workspace_revision: workspace.revision,
            project: workspace.project.clone(),
        })
    }

    pub fn capture(&self, expected_generation: u64) -> Result<WorkspaceLease, DocumentError> {
        let state = self
            .state
            .read()
            .map_err(|_| DocumentError::new("The workspace session is unavailable"))?;
        let workspace = state
            .workspace
            .as_ref()
            .ok_or_else(|| DocumentError::new("No workspace is currently open"))?;

        if state.generation != expected_generation {
            return Err(DocumentError::workspace_changed());
        }

        Ok(WorkspaceLease {
            generation: state.generation,
            root: workspace.root.clone(),
        })
    }

    pub fn read_document(
        &self,
        lease: &WorkspaceLease,
        document_path: &str,
    ) -> Result<DocumentReadResult, DocumentError> {
        let state = self
            .state
            .read()
            .map_err(|_| DocumentError::new("The workspace session is unavailable"))?;
        let workspace = state
            .workspace
            .as_ref()
            .ok_or_else(|| DocumentError::new("No workspace is currently open"))?;

        validate_workspace(&state, workspace, lease)?;
        let content = read_document(&lease.root, document_path)?;
        Ok(DocumentReadResult {
            workspace_generation: lease.generation,
            workspace_revision: workspace.revision,
            content_token: content_token(&content),
            content,
        })
    }

    pub fn save_document(
        &self,
        lease: &WorkspaceLease,
        document_path: &str,
        content: &str,
        expected_content_token: &str,
        expected_workspace_revision: u64,
    ) -> Result<DocumentIndexUpdate, DocumentError> {
        let mut state = self
            .state
            .write()
            .map_err(|_| DocumentError::new("The workspace session is unavailable"))?;
        let generation = state.generation;
        let workspace = state
            .workspace
            .as_mut()
            .ok_or_else(|| DocumentError::new("No workspace is currently open"))?;

        if generation != lease.generation || workspace.root != lease.root {
            return Err(DocumentError::workspace_changed());
        }
        ensure_history_available(workspace, expected_workspace_revision)?;

        let revision = next_revision(workspace.revision)?;
        let before = workspace.project.clone();
        let mut next_project = before.clone();
        let mut update =
            refresh_document_index(&mut next_project, document_path, content, lease.generation)
                .map_err(DocumentError::new)?;
        update.workspace_revision = revision;
        let mut patch = diff_project(&before, &next_project, lease.generation, BTreeSet::new());
        patch.workspace_revision = revision;
        let mut next_history = workspace.history.clone();
        next_history.push_back(patch);
        while next_history.len() > HISTORY_LIMIT {
            next_history.pop_front();
        }
        update.patches =
            patches_from_history(&next_history, expected_workspace_revision, revision)?;
        let written_content =
            write_document_if_current(&lease.root, document_path, content, expected_content_token)?;
        update.content_token = content_token(&written_content.content);
        update.save_warning = written_content.warning;
        workspace.project = next_project;
        workspace.revision = revision;
        workspace.history = next_history;
        workspace.self_writes.insert(
            document_path.to_owned(),
            SelfWrite {
                content: written_content.content,
                expires_at: Instant::now() + Duration::from_secs(2),
            },
        );
        Ok(update)
    }

    pub fn acknowledge_save_conflict(
        &self,
        lease: &WorkspaceLease,
        document_path: &str,
    ) -> Result<(), DocumentError> {
        let state = self
            .state
            .read()
            .map_err(|_| DocumentError::new("The workspace session is unavailable"))?;
        let workspace = state
            .workspace
            .as_ref()
            .ok_or_else(|| DocumentError::new("No workspace is currently open"))?;
        validate_workspace(&state, workspace, lease)?;
        acknowledge_transaction_quarantine(&lease.root, document_path)
    }

    pub fn apply_external_changes(
        &self,
        generation: u64,
        changes: &[WorkspaceChange],
    ) -> Result<Option<WorkspacePatch>, DocumentError> {
        let mut state = self
            .state
            .write()
            .map_err(|_| DocumentError::new("The workspace session is unavailable"))?;
        if state.generation != generation {
            return Err(DocumentError::workspace_changed());
        }
        let workspace = state
            .workspace
            .as_mut()
            .ok_or_else(|| DocumentError::new("No workspace is currently open"))?;
        let revision = next_revision(workspace.revision)?;
        let mut next_project = workspace.project.clone();
        let mut patch =
            apply_project_changes(&workspace.root, &mut next_project, changes, generation)
                .map_err(DocumentError::new)?;
        let now = Instant::now();
        workspace
            .self_writes
            .retain(|_, write| write.expires_at > now);
        patch.externally_changed_document_ids.retain(|document_id| {
            let Some(document) = next_project.documents.get(document_id) else {
                return true;
            };
            let Some(write) = workspace.self_writes.get(&document.path) else {
                return true;
            };
            fs::read_to_string(workspace.root.join(&document.path))
                .map(|content| content != write.content)
                .unwrap_or(true)
        });
        let empty = patch.upserted_folders.is_empty()
            && patch.removed_folder_ids.is_empty()
            && patch.upserted_documents.is_empty()
            && patch.removed_document_ids.is_empty()
            && patch.upserted_links.is_empty()
            && patch.removed_link_ids.is_empty()
            && patch.externally_changed_document_ids.is_empty();
        if !empty {
            workspace.project = next_project;
            workspace.revision = revision;
            patch.workspace_revision = revision;
            push_history(workspace, patch.clone());
        }
        Ok((!empty).then_some(patch))
    }

    pub fn refresh_workspace(
        &self,
        expected_generation: u64,
    ) -> Result<WorkspaceSnapshot, DocumentError> {
        let (root, revision) = {
            let state = self
                .state
                .read()
                .map_err(|_| DocumentError::new("The workspace session is unavailable"))?;
            if state.generation != expected_generation {
                return Err(DocumentError::workspace_changed());
            }
            let workspace = state
                .workspace
                .as_ref()
                .ok_or_else(|| DocumentError::new("No workspace is currently open"))?;
            (workspace.root.clone(), workspace.revision)
        };
        let project =
            scan_workspace(&root).map_err(|error| DocumentError::new(error.to_string()))?;
        let mut state = self
            .state
            .write()
            .map_err(|_| DocumentError::new("The workspace session is unavailable"))?;
        if state.generation != expected_generation {
            return Err(DocumentError::workspace_changed());
        }
        let workspace = state
            .workspace
            .as_mut()
            .ok_or_else(|| DocumentError::new("No workspace is currently open"))?;
        if workspace.root != root {
            return Err(DocumentError::workspace_changed());
        }
        if workspace.revision != revision {
            return Ok(WorkspaceSnapshot {
                workspace_generation: expected_generation,
                workspace_revision: workspace.revision,
                project: workspace.project.clone(),
            });
        }
        let revision = next_revision(workspace.revision)?;
        let before = workspace.project.clone();
        let mut patch = diff_project(&before, &project, expected_generation, BTreeSet::new());
        patch.workspace_revision = revision;
        workspace.project = project.clone();
        workspace.revision = revision;
        push_history(workspace, patch);
        Ok(WorkspaceSnapshot {
            workspace_generation: expected_generation,
            workspace_revision: workspace.revision,
            project,
        })
    }
}

fn push_history(workspace: &mut ActiveWorkspace, patch: WorkspacePatch) {
    workspace.history.push_back(patch);
    while workspace.history.len() > HISTORY_LIMIT {
        workspace.history.pop_front();
    }
}

fn ensure_history_available(
    workspace: &ActiveWorkspace,
    expected_revision: u64,
) -> Result<(), DocumentError> {
    if expected_revision > workspace.revision {
        return Err(DocumentError::new(
            "The editor workspace revision is newer than the active workspace",
        ));
    }
    if workspace.revision - expected_revision >= HISTORY_LIMIT as u64 {
        return Err(DocumentError::new(
            "The workspace changed too much to save safely",
        ));
    }
    if expected_revision == workspace.revision {
        return Ok(());
    }
    let first = workspace
        .history
        .iter()
        .find(|patch| patch.workspace_revision > expected_revision)
        .ok_or_else(|| DocumentError::new("The workspace changed too much to save safely"))?;
    if first.workspace_revision != expected_revision + 1 {
        return Err(DocumentError::new(
            "The workspace changed too much to save safely",
        ));
    }
    Ok(())
}

fn patches_from_history(
    history: &VecDeque<WorkspacePatch>,
    expected_revision: u64,
    current_revision: u64,
) -> Result<Vec<WorkspacePatch>, DocumentError> {
    let patches: Vec<_> = history
        .iter()
        .filter(|patch| patch.workspace_revision > expected_revision)
        .cloned()
        .collect();
    if patches.first().map(|patch| patch.workspace_revision) != Some(expected_revision + 1)
        || patches
            .windows(2)
            .any(|pair| pair[1].workspace_revision != pair[0].workspace_revision + 1)
        || patches.last().map(|patch| patch.workspace_revision) != Some(current_revision)
    {
        return Err(DocumentError::new(
            "The workspace changed too much to save safely",
        ));
    }
    Ok(patches)
}

fn validate_workspace(
    state: &SessionState,
    workspace: &ActiveWorkspace,
    lease: &WorkspaceLease,
) -> Result<(), DocumentError> {
    if state.generation == lease.generation && workspace.root == lease.root {
        Ok(())
    } else {
        Err(DocumentError::workspace_changed())
    }
}

#[derive(Debug)]
pub struct DocumentError(String);

impl DocumentError {
    fn new(message: impl Into<String>) -> Self {
        Self(message.into())
    }

    fn workspace_changed() -> Self {
        Self::new("The workspace changed before the document request could complete")
    }
}

impl fmt::Display for DocumentError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl std::error::Error for DocumentError {}

pub fn open_workspace(
    root: &Path,
) -> Result<(crate::models::workspace::ProjectModel, PathBuf), WorkspaceError> {
    let canonical_root = resolve_workspace_root(root)?;
    let project = scan_canonical_root(canonical_root.clone())?;
    Ok((project, canonical_root))
}

pub fn read_document(root: &Path, document_path: &str) -> Result<String, DocumentError> {
    let path = resolve_document_path(root, document_path)?;
    fs::read_to_string(&path).map_err(|source| {
        DocumentError::new(format!(
            "Unable to read document '{document_path}': {source}"
        ))
    })
}

pub fn write_document(
    root: &Path,
    document_path: &str,
    content: &str,
) -> Result<String, DocumentError> {
    let path = resolve_document_path(root, document_path)?;
    let existing_content = fs::read_to_string(&path).map_err(|source| {
        DocumentError::new(format!(
            "Unable to read document '{document_path}' before saving: {source}"
        ))
    })?;
    let content = preserve_line_endings(&existing_content, content);
    fs::write(&path, &content).map_err(|source| {
        DocumentError::new(format!(
            "Unable to save document '{document_path}': {source}"
        ))
    })?;
    Ok(content)
}

fn write_document_if_current(
    root: &Path,
    document_path: &str,
    content: &str,
    expected_content_token: &str,
) -> Result<WrittenDocument, DocumentError> {
    write_document_if_current_with_hook(
        root,
        document_path,
        content,
        expected_content_token,
        |_| {},
    )
}

fn write_document_if_current_with_hook(
    root: &Path,
    document_path: &str,
    content: &str,
    expected_content_token: &str,
    before_commit: impl FnOnce(&Path),
) -> Result<WrittenDocument, DocumentError> {
    let path = resolve_document_path(root, document_path)?;
    let current = fs::read_to_string(&path).map_err(|source| {
        DocumentError::new(format!(
            "Unable to read document '{document_path}' before saving: {source}"
        ))
    })?;
    if content_token(&current) != expected_content_token {
        return Err(external_change_error());
    }
    let content = preserve_line_endings(&current, content);
    let warning = transactional_replace(
        root,
        &path,
        content.as_bytes(),
        expected_content_token,
        before_commit,
    )
    .map_err(|source| {
        DocumentError::new(format!(
            "Unable to save document '{document_path}': {source}"
        ))
    })?;
    remove_empty_workspace_recovery_directory(root).map_err(|source| {
        DocumentError::new(format!(
            "Unable to remove completed save state from the workspace: {source}"
        ))
    })?;
    Ok(WrittenDocument { content, warning })
}

fn remove_empty_workspace_recovery_directory(root: &Path) -> std::io::Result<()> {
    let directory = root.join(RECOVERY_DIRECTORY);
    let metadata = match fs::symlink_metadata(&directory) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(error),
    };
    if !is_safe_recovery_directory_metadata(&metadata) {
        return Ok(());
    }
    let entries: Vec<_> = fs::read_dir(&directory)?.collect::<Result<_, _>>()?;
    if entries.len() != 1 || entries[0].file_name() != RECOVERY_OWNER_SENTINEL {
        return Ok(());
    }
    let sentinel = entries[0].path();
    let metadata = fs::symlink_metadata(&sentinel)?;
    if !is_safe_recovery_owner_metadata(&metadata)
        || fs::read(&sentinel).ok().as_deref() != Some(RECOVERY_OWNER_MAGIC)
    {
        return Ok(());
    }
    fs::remove_file(sentinel)?;
    match fs::remove_dir(directory) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::DirectoryNotEmpty => {
            if let Ok(mut sentinel) = fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(root.join(RECOVERY_DIRECTORY).join(RECOVERY_OWNER_SENTINEL))
            {
                #[cfg(unix)]
                {
                    use std::os::unix::fs::PermissionsExt;
                    let _ = sentinel.set_permissions(fs::Permissions::from_mode(0o600));
                }
                let _ = sentinel
                    .write_all(RECOVERY_OWNER_MAGIC)
                    .and_then(|_| sentinel.sync_all());
            }
            Ok(())
        }
        Err(error) => Err(error),
    }
}

fn cleanup_workspace_recovery_directory(root: &Path) -> std::io::Result<()> {
    let directory = root.join(RECOVERY_DIRECTORY);
    let metadata = match fs::symlink_metadata(&directory) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(error),
    };
    if !is_safe_recovery_directory_metadata(&metadata) {
        return Ok(());
    }
    let mut owned_entries = Vec::new();
    let mut sentinel_path = None;
    for entry in fs::read_dir(&directory)? {
        let entry = entry?;
        let name = entry.file_name();
        let metadata = fs::symlink_metadata(entry.path())?;
        if name == std::ffi::OsStr::new(RECOVERY_OWNER_SENTINEL) {
            if !is_safe_recovery_owner_metadata(&metadata)
                || fs::read(entry.path()).ok().as_deref() != Some(RECOVERY_OWNER_MAGIC)
            {
                return Ok(());
            }
            sentinel_path = Some(entry.path());
            continue;
        }
        let Some(name) = name.to_str() else {
            return Ok(());
        };
        if !is_owned_recovery_entry_name(name) || !is_safe_recovery_file_metadata(&metadata) {
            return Ok(());
        }
        if name.ends_with(".recovery")
            && fs::read(entry.path())
                .ok()
                .as_deref()
                .and_then(decode_recovery_artifact)
                .is_none()
        {
            return Ok(());
        }
        owned_entries.push(entry.path());
    }
    let Some(sentinel_path) = sentinel_path else {
        return Ok(());
    };
    for entry in owned_entries {
        fs::remove_file(entry)?;
    }
    fs::remove_file(sentinel_path)?;
    fs::remove_dir(directory)
}

#[cfg(unix)]
fn is_safe_recovery_directory_metadata(metadata: &fs::Metadata) -> bool {
    use std::os::unix::fs::{FileTypeExt, MetadataExt};
    let file_type = metadata.file_type();
    file_type.is_dir()
        && !file_type.is_symlink()
        && !file_type.is_fifo()
        && !file_type.is_socket()
        && !file_type.is_block_device()
        && !file_type.is_char_device()
        && metadata.uid() == unsafe { libc::geteuid() }
        && metadata.mode() & 0o077 == 0
}

#[cfg(windows)]
fn is_safe_recovery_directory_metadata(metadata: &fs::Metadata) -> bool {
    use std::os::windows::fs::MetadataExt;
    use windows_sys::Win32::Storage::FileSystem::FILE_ATTRIBUTE_REPARSE_POINT;
    metadata.is_dir() && metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT == 0
}

#[cfg(unix)]
fn is_safe_recovery_file_metadata(metadata: &fs::Metadata) -> bool {
    use std::os::unix::fs::{FileTypeExt, MetadataExt};
    let file_type = metadata.file_type();
    file_type.is_file()
        && !file_type.is_symlink()
        && !file_type.is_fifo()
        && !file_type.is_socket()
        && !file_type.is_block_device()
        && !file_type.is_char_device()
        && metadata.uid() == unsafe { libc::geteuid() }
}

#[cfg(unix)]
fn is_safe_recovery_owner_metadata(metadata: &fs::Metadata) -> bool {
    use std::os::unix::fs::MetadataExt;
    is_safe_recovery_file_metadata(metadata) && metadata.mode() & 0o077 == 0
}

#[cfg(windows)]
fn is_safe_recovery_file_metadata(metadata: &fs::Metadata) -> bool {
    use std::os::windows::fs::MetadataExt;
    use windows_sys::Win32::Storage::FileSystem::FILE_ATTRIBUTE_REPARSE_POINT;
    metadata.is_file() && metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT == 0
}

#[cfg(windows)]
fn is_safe_recovery_owner_metadata(metadata: &fs::Metadata) -> bool {
    is_safe_recovery_file_metadata(metadata)
}

fn is_owned_recovery_entry_name(name: &str) -> bool {
    let Some((hash, suffix)) = name.split_once('.') else {
        return false;
    };
    hash.len() == 64
        && hash
            .bytes()
            .all(|value| value.is_ascii_hexdigit() && !value.is_ascii_uppercase())
        && matches!(
            suffix,
            "recovery"
                | "lock"
                | "staging"
                | "blocked"
                | "external"
                | "acknowledged"
                | "conflict"
                | "install"
                | "failure-install"
                | "staging-install"
        )
}

#[derive(Debug)]
struct WrittenDocument {
    content: String,
    warning: Option<String>,
}

fn external_change_error() -> DocumentError {
    DocumentError::new("The document changed externally. Your unsaved buffer was not saved.")
}

#[cfg(unix)]
fn acknowledge_transaction_quarantine(
    root: &Path,
    document_path: &str,
) -> Result<(), DocumentError> {
    use std::os::fd::AsRawFd;
    let path = resolve_document_path(root, document_path)?;
    let relative = path
        .strip_prefix(root)
        .map_err(|_| DocumentError::new("The document is outside the workspace"))?;
    let relative_path = normalize_relative_path(relative)
        .map_err(|_| DocumentError::new("The document path is malformed"))?;
    let root_directory = open_directory_no_follow(root)?;
    let directory = open_recovery_directory(&root_directory)?;
    let _lock = acquire_unix_transaction_lock(&directory, &relative_path)?;
    let external = recovery_auxiliary_name(&relative_path, "external");
    let staging = recovery_auxiliary_name(&relative_path, "staging");
    let blocked = recovery_auxiliary_name(&relative_path, "blocked");
    let acknowledged = recovery_auxiliary_name(&relative_path, "acknowledged");
    let blocked_exists =
        unix_entry_identity(directory.as_raw_fd(), std::ffi::OsStr::new(&blocked)).is_some();
    if unix_entry_identity(directory.as_raw_fd(), std::ffi::OsStr::new(&external)).is_some() {
        remove_acknowledged_slot(&directory, &acknowledged)?;
        rename_noreplace_between(
            directory.as_raw_fd(),
            std::ffi::OsStr::new(&external),
            directory.as_raw_fd(),
            std::ffi::OsStr::new(&acknowledged),
        )?;
    }
    if blocked_exists
        && unix_entry_identity(directory.as_raw_fd(), std::ffi::OsStr::new(&staging)).is_some()
    {
        remove_acknowledged_slot(&directory, &acknowledged)?;
        rename_noreplace_between(
            directory.as_raw_fd(),
            std::ffi::OsStr::new(&staging),
            directory.as_raw_fd(),
            std::ffi::OsStr::new(&acknowledged),
        )?;
    }
    if blocked_exists {
        remove_acknowledged_slot(&directory, &blocked)?;
    }
    remove_acknowledged_slot(&directory, &acknowledged)?;
    directory.sync_all().map_err(|source| {
        DocumentError::new(format!("Unable to persist conflict resolution: {source}"))
    })
}

#[cfg(unix)]
fn remove_acknowledged_slot(directory: &fs::File, name: &str) -> Result<(), DocumentError> {
    use std::{os::fd::AsRawFd, os::unix::ffi::OsStrExt};
    let entry = std::ffi::OsStr::new(name);
    if unix_entry_identity(directory.as_raw_fd(), entry).is_none() {
        return Ok(());
    }
    let file = openat_file(
        directory.as_raw_fd(),
        entry,
        libc::O_RDONLY | libc::O_NONBLOCK,
        0,
    )?;
    if !file
        .metadata()
        .map_err(|source| DocumentError::new(format!("Unable to verify conflict state: {source}")))?
        .is_file()
    {
        return Err(DocumentError::new(
            "The conflict state is not a regular app artifact",
        ));
    }
    let identity = unix_file_identity(&file)?;
    if !unix_entry_matches_identity(directory.as_raw_fd(), entry, identity) {
        return Err(DocumentError::new("The conflict state changed externally"));
    }
    let name = CString::new(entry.as_bytes())
        .map_err(|_| DocumentError::new("The conflict state name is malformed"))?;
    if unsafe { libc::unlinkat(directory.as_raw_fd(), name.as_ptr(), 0) } != 0 {
        return Err(DocumentError::new(format!(
            "Unable to acknowledge conflict state: {}",
            std::io::Error::last_os_error()
        )));
    }
    Ok(())
}

#[cfg(windows)]
fn acknowledge_transaction_quarantine(
    root: &Path,
    document_path: &str,
) -> Result<(), DocumentError> {
    let path = resolve_document_path(root, document_path)?;
    let relative = path
        .strip_prefix(root)
        .map_err(|_| DocumentError::new("The document is outside the workspace"))?;
    let relative_path = normalize_relative_path(relative)
        .map_err(|_| DocumentError::new("The document path is malformed"))?;
    let directory = root.join(RECOVERY_DIRECTORY);
    let created = match fs::create_dir(&directory) {
        Ok(()) => true,
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => false,
        Err(source) => {
            return Err(DocumentError::new(format!(
                "Unable to open recovery state: {source}"
            )))
        }
    };
    ensure_windows_recovery_owner(&directory, created)?;
    let _lock = acquire_windows_transaction_lock(&directory, &relative_path)?;
    let external = directory.join(recovery_auxiliary_name(&relative_path, "external"));
    let staging = directory.join(recovery_auxiliary_name(&relative_path, "staging"));
    let blocked = directory.join(recovery_auxiliary_name(&relative_path, "blocked"));
    let acknowledged = directory.join(recovery_auxiliary_name(&relative_path, "acknowledged"));
    let blocked_exists = blocked.exists();
    if external.exists() {
        remove_windows_acknowledged_slot(&acknowledged)?;
        windows_move_file(&external, &acknowledged)?;
    }
    if blocked_exists && staging.exists() {
        remove_windows_acknowledged_slot(&acknowledged)?;
        windows_move_file(&staging, &acknowledged)?;
    }
    if blocked_exists {
        remove_windows_acknowledged_slot(&blocked)?;
    }
    remove_windows_acknowledged_slot(&acknowledged)?;
    Ok(())
}

#[cfg(windows)]
fn remove_windows_acknowledged_slot(path: &Path) -> Result<(), DocumentError> {
    use std::os::windows::fs::{MetadataExt, OpenOptionsExt};
    use windows_sys::Win32::Storage::FileSystem::{
        FILE_ATTRIBUTE_REPARSE_POINT, FILE_FLAG_OPEN_REPARSE_POINT, FILE_SHARE_READ,
    };
    if !path.exists() {
        return Ok(());
    }
    let file = fs::OpenOptions::new()
        .read(true)
        .share_mode(FILE_SHARE_READ)
        .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT)
        .open(path)
        .map_err(|source| {
            DocumentError::new(format!("Unable to verify conflict state: {source}"))
        })?;
    let metadata = file.metadata().map_err(|source| {
        DocumentError::new(format!("Unable to verify conflict state: {source}"))
    })?;
    if !metadata.is_file() || metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
        return Err(DocumentError::new(
            "The conflict state is not a regular app artifact",
        ));
    }
    drop(file);
    fs::remove_file(path).map_err(|source| {
        DocumentError::new(format!("Unable to acknowledge conflict state: {source}"))
    })
}

#[cfg(target_os = "linux")]
fn transactional_replace(
    root: &Path,
    path: &Path,
    content: &[u8],
    expected_content_token: &str,
    before_commit: impl FnOnce(&Path),
) -> Result<Option<String>, DocumentError> {
    let mut before_commit = Some(before_commit);
    transactional_replace_with_phase_hook(
        root,
        path,
        content,
        expected_content_token,
        |phase, temp_path| {
            if phase == TransactionPhase::Prepared {
                if let Some(callback) = before_commit.take() {
                    callback(temp_path);
                }
            }
        },
    )
}

#[cfg(target_os = "linux")]
#[derive(Clone, Copy, Eq, PartialEq)]
enum TransactionPhase {
    Prepared,
    Displaced,
    PreExchange,
    Installed,
    PreRestore,
    Cleanup,
}

#[cfg(target_os = "linux")]
fn transactional_replace_with_phase_hook(
    root: &Path,
    path: &Path,
    content: &[u8],
    expected_content_token: &str,
    mut hook: impl FnMut(TransactionPhase, &Path),
) -> Result<Option<String>, DocumentError> {
    use std::os::fd::AsRawFd;

    let relative = path
        .strip_prefix(root)
        .map_err(|_| DocumentError::new("The document is outside the workspace"))?;
    let relative_parent = relative.parent().unwrap_or_else(|| Path::new(""));
    let name = path
        .file_name()
        .ok_or_else(|| DocumentError::new("The document name is unavailable"))?;
    let root_directory = open_directory_no_follow(root)?;
    let root_identity = unix_file_identity(&root_directory)?;
    let directory = open_relative_directory_no_follow(root_directory.as_raw_fd(), relative_parent)?;
    let parent_identity = unix_file_identity(&directory)?;
    let mut source = openat_file(directory.as_raw_fd(), name, libc::O_RDONLY, 0)?;
    let source_identity = unix_file_identity(&source)?;
    let source_content = read_file_bytes(&mut source)?;
    if content_token_bytes(&source_content) != expected_content_token {
        return Err(external_change_error());
    }
    let recovery_directory = open_recovery_directory(&root_directory)?;
    let relative_path = normalize_relative_path(relative)
        .map_err(|_| DocumentError::new("The document path is malformed"))?;
    let transaction_lock = acquire_unix_transaction_lock(&recovery_directory, &relative_path)?;
    let transaction_lock_identity = unix_file_identity(&transaction_lock)?;
    ensure_no_pending_quarantine(&recovery_directory, &relative_path)?;
    let mut recovery =
        persist_recovery_artifact(&recovery_directory, &relative_path, &source_content)?;
    let mut temp = open_unnamed_temp(directory.as_raw_fd())?;
    copy_unix_metadata(&source, &temp)?;
    let temp_identity = unix_file_identity(&temp)?;
    temp.write_all(content)
        .and_then(|_| temp.sync_all())
        .map_err(|source| DocumentError::new(format!("Unable to prepare save: {source}")))?;
    temp.seek(SeekFrom::Start(0)).map_err(|source| {
        DocumentError::new(format!("Unable to verify prepared save: {source}"))
    })?;
    if read_file_bytes(&mut temp)? != content || unix_file_identity(&temp)? != temp_identity {
        return Err(DocumentError::new("Unable to verify the prepared save"));
    }
    let recovery_path = root.join(RECOVERY_DIRECTORY).join(&recovery.slot_name);
    hook(TransactionPhase::Prepared, &recovery_path);
    if !workspace_path_matches(&root_directory, root, root_identity)
        || !relative_directory_matches(root_directory.as_raw_fd(), relative_parent, parent_identity)
        || !recovery_directory_attached(&root_directory, &recovery_directory)
        || !recovery_artifact_matches(&recovery_directory, &recovery)
        || !held_recovery_matches(&mut recovery)
        || !unix_entry_matches_identity(directory.as_raw_fd(), name, source_identity)
        || read_named_token(directory.as_raw_fd(), name).as_deref() != Some(expected_content_token)
    {
        return Err(conflict_with_recovery(
            root,
            &root_directory,
            &recovery_directory,
            &mut recovery,
        ));
    }
    if !recovery_directory_attached(&root_directory, &recovery_directory) {
        return Err(conflict_with_recovery(
            root,
            &root_directory,
            &recovery_directory,
            &mut recovery,
        ));
    }
    let staging_name = recovery_auxiliary_name(&relative_path, "staging");
    let (_staging_handle, staging_identity) = prepare_exchange_staging(
        &recovery_directory,
        &staging_name,
        &source,
        &mut temp,
        content,
        temp_identity,
    )?;
    let staging_path = root.join(RECOVERY_DIRECTORY).join(&staging_name);
    hook(TransactionPhase::Displaced, &staging_path);
    if !workspace_path_matches(&root_directory, root, root_identity)
        || !relative_directory_matches(root_directory.as_raw_fd(), relative_parent, parent_identity)
        || !recovery_directory_attached(&root_directory, &recovery_directory)
        || !recovery_artifact_matches(&recovery_directory, &recovery)
        || !held_recovery_matches(&mut recovery)
        || !unix_entry_matches_identity(directory.as_raw_fd(), name, source_identity)
        || read_named_token(directory.as_raw_fd(), name).as_deref() != Some(expected_content_token)
        || !unix_entry_matches_identity(
            recovery_directory.as_raw_fd(),
            std::ffi::OsStr::new(&staging_name),
            staging_identity,
        )
        || read_named_bytes(
            recovery_directory.as_raw_fd(),
            std::ffi::OsStr::new(&staging_name),
        )
        .as_deref()
            != Some(content)
    {
        return Err(conflict_with_recovery(
            root,
            &root_directory,
            &recovery_directory,
            &mut recovery,
        ));
    }
    hook(TransactionPhase::PreExchange, path);
    rename_exchange_between(
        directory.as_raw_fd(),
        name,
        recovery_directory.as_raw_fd(),
        std::ffi::OsStr::new(&staging_name),
    )?;
    hook(TransactionPhase::Installed, path);
    let installed_valid =
        unix_entry_matches_identity(directory.as_raw_fd(), name, staging_identity)
            && read_named_bytes(directory.as_raw_fd(), name).as_deref() == Some(content)
            && workspace_path_matches(&root_directory, root, root_identity)
            && relative_directory_matches(
                root_directory.as_raw_fd(),
                relative_parent,
                parent_identity,
            )
            && recovery_directory_attached(&root_directory, &recovery_directory)
            && recovery_artifact_matches(&recovery_directory, &recovery)
            && held_recovery_matches(&mut recovery);
    if !installed_valid {
        let quarantine =
            quarantine_after_exchange(&recovery_directory, &relative_path, &staging_name);
        return Err(DocumentError::new(format!(
            "The document changed externally during save. {quarantine}. {}",
            recovery_message(root, &root_directory, &recovery_directory, &mut recovery)
        )));
    }
    let staging_entry = std::ffi::OsStr::new(&staging_name);
    let captured_identity = unix_entry_identity(recovery_directory.as_raw_fd(), staging_entry);
    let captured_content = read_named_bytes(recovery_directory.as_raw_fd(), staging_entry);
    if !captured_baseline_matches(
        source_identity,
        expected_content_token,
        captured_identity,
        captured_content.as_deref(),
    ) {
        hook(TransactionPhase::PreRestore, path);
        let restored = captured_identity
            .zip(captured_content.as_ref())
            .is_some_and(|(external_identity, external_content)| {
                unix_entry_matches_identity(directory.as_raw_fd(), name, staging_identity)
                    && read_named_bytes(directory.as_raw_fd(), name).as_deref() == Some(content)
                    && unix_entry_matches_identity(
                        recovery_directory.as_raw_fd(),
                        staging_entry,
                        external_identity,
                    )
                    && rename_exchange_between(
                        directory.as_raw_fd(),
                        name,
                        recovery_directory.as_raw_fd(),
                        staging_entry,
                    )
                    .is_ok()
                    && unix_entry_matches_identity(directory.as_raw_fd(), name, external_identity)
                    && read_named_bytes(directory.as_raw_fd(), name).as_deref()
                        == Some(external_content.as_slice())
                    && unix_entry_matches_identity(
                        recovery_directory.as_raw_fd(),
                        staging_entry,
                        staging_identity,
                    )
                    && read_named_bytes(recovery_directory.as_raw_fd(), staging_entry).as_deref()
                        == Some(content)
            });
        let quarantine =
            quarantine_after_exchange(&recovery_directory, &relative_path, &staging_name);
        let recovery_reference =
            recovery_message(root, &root_directory, &recovery_directory, &mut recovery);
        return Err(DocumentError::new(if restored {
            format!(
                "The document changed externally and the external version was restored at the live path. {quarantine}. {recovery_reference}"
            )
        } else {
            format!(
                "The document changed externally during save. {quarantine}. {recovery_reference}"
            )
        }));
    }
    hook(TransactionPhase::Cleanup, &recovery_path);
    let durability_warning = directory
        .sync_all()
        .err()
        .map(|error| format!("Directory durability could not be confirmed: {error}"));
    if !workspace_path_matches(&root_directory, root, root_identity)
        || !relative_directory_matches(root_directory.as_raw_fd(), relative_parent, parent_identity)
        || !recovery_directory_attached(&root_directory, &recovery_directory)
        || !recovery_artifact_matches(&recovery_directory, &recovery)
        || !held_recovery_matches(&mut recovery)
    {
        let quarantine =
            quarantine_after_exchange(&recovery_directory, &relative_path, &staging_name);
        return Err(DocumentError::new(format!(
            "The document changed externally after exchange. {quarantine}. {}",
            recovery_message(root, &root_directory, &recovery_directory, &mut recovery)
        )));
    }
    cleanup_successful_unix_transaction(
        &recovery_directory,
        &relative_path,
        &recovery,
        &staging_name,
        source_identity,
        transaction_lock_identity,
    )?;
    Ok(durability_warning)
}

#[cfg(any(test, target_os = "macos", windows))]
enum RecoveredMutationError {
    RecoveryUnavailable,
    Mutation(std::io::Error),
    Verification,
}

#[cfg(any(test, target_os = "macos", windows))]
impl fmt::Display for RecoveredMutationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::RecoveryUnavailable => formatter.write_str("Persisted recovery is unavailable"),
            Self::Mutation(source) => write!(formatter, "Document write failed: {source}"),
            Self::Verification => formatter.write_str("Document write verification failed"),
        }
    }
}

#[cfg(any(test, target_os = "macos", windows))]
fn run_recovered_mutation<T>(
    target: &mut T,
    recovery_is_valid: impl FnOnce() -> bool,
    mutation: impl FnOnce(&mut T) -> std::io::Result<()>,
    verification: impl FnOnce(&mut T) -> Result<bool, DocumentError>,
) -> Result<(), RecoveredMutationError> {
    if !recovery_is_valid() {
        return Err(RecoveredMutationError::RecoveryUnavailable);
    }
    mutation(target).map_err(RecoveredMutationError::Mutation)?;
    if !verification(target).map_err(|_| RecoveredMutationError::Verification)? {
        return Err(RecoveredMutationError::Verification);
    }
    Ok(())
}

#[cfg(target_os = "macos")]
fn transactional_replace(
    root: &Path,
    path: &Path,
    content: &[u8],
    expected_content_token: &str,
    before_commit: impl FnOnce(&Path),
) -> Result<Option<String>, DocumentError> {
    use std::os::fd::AsRawFd;

    let relative = path
        .strip_prefix(root)
        .map_err(|_| DocumentError::new("The document is outside the workspace"))?;
    let relative_parent = relative.parent().unwrap_or_else(|| Path::new(""));
    let name = path
        .file_name()
        .ok_or_else(|| DocumentError::new("The document name is unavailable"))?;
    let root_directory = open_directory_no_follow(root)?;
    let root_identity = unix_file_identity(&root_directory)?;
    let directory = open_relative_directory_no_follow(root_directory.as_raw_fd(), relative_parent)?;
    let parent_identity = unix_file_identity(&directory)?;
    let mut target = openat_file(directory.as_raw_fd(), name, libc::O_RDWR, 0)?;
    if unsafe { libc::flock(target.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) } != 0 {
        return Err(DocumentError::new(format!(
            "Unable to lock the document for saving: {}",
            std::io::Error::last_os_error()
        )));
    }
    let identity = unix_file_identity(&target)?;
    let original = read_file_bytes(&mut target)?;
    if content_token_bytes(&original) != expected_content_token {
        return Err(external_change_error());
    }
    let recovery_directory = open_recovery_directory(&root_directory)?;
    let relative_path = normalize_relative_path(relative)
        .map_err(|_| DocumentError::new("The document path is malformed"))?;
    let transaction_lock = acquire_unix_transaction_lock(&recovery_directory, &relative_path)?;
    let transaction_lock_identity = unix_file_identity(&transaction_lock)?;
    ensure_no_pending_quarantine(&recovery_directory, &relative_path)?;
    let mut recovery = persist_recovery_artifact(&recovery_directory, &relative_path, &original)?;
    if unsafe { libc::flock(recovery.file.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) } != 0
        || !held_recovery_matches(&mut recovery)
    {
        return Err(DocumentError::new(
            "Unable to retain the persisted recovery handle",
        ));
    }
    let staging_name = recovery_auxiliary_name(&relative_path, "staging");
    let (_staging_handle, staging_identity) =
        prepare_macos_exchange_staging(&recovery_directory, &staging_name, &target, content)?;
    before_commit(path);
    if !workspace_path_matches(&root_directory, root, root_identity)
        || !relative_directory_matches(root_directory.as_raw_fd(), relative_parent, parent_identity)
        || !recovery_directory_attached(&root_directory, &recovery_directory)
        || !recovery_artifact_matches(&recovery_directory, &recovery)
        || !held_recovery_matches(&mut recovery)
        || !unix_entry_matches_identity(directory.as_raw_fd(), name, identity)
        || read_named_token(directory.as_raw_fd(), name).as_deref() != Some(expected_content_token)
        || !unix_entry_matches_identity(
            recovery_directory.as_raw_fd(),
            std::ffi::OsStr::new(&staging_name),
            staging_identity,
        )
        || read_named_bytes(
            recovery_directory.as_raw_fd(),
            std::ffi::OsStr::new(&staging_name),
        )
        .as_deref()
            != Some(content)
    {
        return Err(conflict_with_recovery(
            root,
            &root_directory,
            &recovery_directory,
            &mut recovery,
        ));
    }
    rename_exchange_between(
        directory.as_raw_fd(),
        name,
        recovery_directory.as_raw_fd(),
        std::ffi::OsStr::new(&staging_name),
    )?;
    if !unix_entry_matches_identity(directory.as_raw_fd(), name, staging_identity)
        || read_named_bytes(directory.as_raw_fd(), name).as_deref() != Some(content)
    {
        let quarantine =
            quarantine_after_exchange(&recovery_directory, &relative_path, &staging_name);
        return Err(DocumentError::new(format!(
            "The document changed externally during save. {quarantine}. {}",
            recovery_message(root, &root_directory, &recovery_directory, &mut recovery)
        )));
    }
    let staging_entry = std::ffi::OsStr::new(&staging_name);
    let captured_identity = unix_entry_identity(recovery_directory.as_raw_fd(), staging_entry);
    let captured_content = read_named_bytes(recovery_directory.as_raw_fd(), staging_entry);
    if !captured_baseline_matches(
        identity,
        expected_content_token,
        captured_identity,
        captured_content.as_deref(),
    ) {
        let restored = captured_identity
            .zip(captured_content.as_ref())
            .is_some_and(|(external_identity, external_content)| {
                unix_entry_matches_identity(directory.as_raw_fd(), name, staging_identity)
                    && read_named_bytes(directory.as_raw_fd(), name).as_deref() == Some(content)
                    && unix_entry_matches_identity(
                        recovery_directory.as_raw_fd(),
                        staging_entry,
                        external_identity,
                    )
                    && rename_exchange_between(
                        directory.as_raw_fd(),
                        name,
                        recovery_directory.as_raw_fd(),
                        staging_entry,
                    )
                    .is_ok()
                    && unix_entry_matches_identity(directory.as_raw_fd(), name, external_identity)
                    && read_named_bytes(directory.as_raw_fd(), name).as_deref()
                        == Some(external_content.as_slice())
                    && read_named_bytes(recovery_directory.as_raw_fd(), staging_entry).as_deref()
                        == Some(content)
            });
        let quarantine =
            quarantine_after_exchange(&recovery_directory, &relative_path, &staging_name);
        let reference = recovery_message(root, &root_directory, &recovery_directory, &mut recovery);
        return Err(DocumentError::new(if restored {
            format!(
                "The document changed externally and the external version was restored at the live path. {quarantine}. {reference}"
            )
        } else {
            format!("The document changed externally during save. {quarantine}. {reference}")
        }));
    }
    if !workspace_path_matches(&root_directory, root, root_identity)
        || !relative_directory_matches(root_directory.as_raw_fd(), relative_parent, parent_identity)
        || !recovery_directory_attached(&root_directory, &recovery_directory)
        || !recovery_artifact_matches(&recovery_directory, &recovery)
        || !held_recovery_matches(&mut recovery)
    {
        let quarantine =
            quarantine_after_exchange(&recovery_directory, &relative_path, &staging_name);
        return Err(DocumentError::new(format!(
            "The document changed externally after exchange. {quarantine}. {}",
            recovery_message(root, &root_directory, &recovery_directory, &mut recovery)
        )));
    }
    let durability_warning = directory
        .sync_all()
        .err()
        .map(|source| format!("Directory durability could not be confirmed: {source}"));
    cleanup_successful_unix_transaction(
        &recovery_directory,
        &relative_path,
        &recovery,
        &staging_name,
        identity,
        transaction_lock_identity,
    )?;
    Ok(durability_warning)
}

#[cfg(windows)]
fn transactional_replace(
    root: &Path,
    path: &Path,
    content: &[u8],
    expected_content_token: &str,
    before_commit: impl FnOnce(&Path),
) -> Result<Option<String>, DocumentError> {
    use std::os::windows::fs::{MetadataExt, OpenOptionsExt};
    use windows_sys::Win32::Storage::FileSystem::{
        FILE_ATTRIBUTE_REPARSE_POINT, FILE_FLAG_OPEN_REPARSE_POINT, FILE_SHARE_READ,
    };

    let parent = path
        .parent()
        .ok_or_else(|| DocumentError::new("The document parent is unavailable"))?;
    let root_guard = WindowsAncestorGuard::open(root)?;
    let ancestors = WindowsAncestorGuard::open(parent)?;
    let mut target = fs::OpenOptions::new()
        .read(true)
        .write(true)
        .share_mode(FILE_SHARE_READ)
        .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT)
        .open(path)
        .map_err(|source| {
            DocumentError::new(format!("Unable to lock document for saving: {source}"))
        })?;
    let metadata = target
        .metadata()
        .map_err(|source| DocumentError::new(format!("Unable to verify document: {source}")))?;
    if metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0 || !metadata.is_file() {
        return Err(DocumentError::new(
            "The document path is not a verified regular file",
        ));
    }
    let identity = windows_file_identity(&target)?;
    let original = read_file_bytes_portable(&mut target)?;
    if content_token_bytes(&original) != expected_content_token {
        return Err(external_change_error());
    }
    let relative = path
        .strip_prefix(root)
        .map_err(|_| DocumentError::new("The document is outside the workspace"))?;
    let relative_path = normalize_relative_path(relative)
        .map_err(|_| DocumentError::new("The document path is malformed"))?;
    let mut recovery = persist_windows_recovery(root, &relative_path, &original)?;
    before_commit(path);
    if !root_guard.matches()? || !ancestors.matches()? || windows_path_identity(path)? != identity {
        return Err(DocumentError::new(format!(
            "The document changed externally. {}",
            windows_recovery_message(root, &root_guard, &mut recovery)
        )));
    }
    if let Err(source) = run_recovered_mutation(
        &mut target,
        || windows_recovery_matches(&recovery),
        |target| {
            target
                .set_len(0)
                .and_then(|_| target.seek(SeekFrom::Start(0)))
                .and_then(|_| target.write_all(content))
                .and_then(|_| target.sync_all())
        },
        |target| {
            Ok(read_file_bytes_portable(target)? == content
                && windows_path_identity(path)? == identity
                && root_guard.matches()?
                && ancestors.matches()?
                && windows_recovery_matches(&recovery))
        },
    ) {
        return Err(DocumentError::new(format!(
            "Unable to save document: {source}. {}",
            windows_recovery_message(root, &root_guard, &mut recovery)
        )));
    }
    if !windows_recovery_matches(&recovery) {
        return Err(DocumentError::new(format!(
            "The recovery changed during save. {}",
            windows_recovery_message(root, &root_guard, &mut recovery)
        )));
    }
    cleanup_successful_windows_transaction(recovery)?;
    Ok(None)
}

#[cfg(windows)]
struct WindowsRecoveryArtifact {
    _transaction_lock: fs::File,
    transaction_lock_path: PathBuf,
    transaction_lock_identity: (u32, u64),
    directory_path: PathBuf,
    directory: fs::File,
    directory_identity: (u32, u64),
    slot_path: PathBuf,
    file: fs::File,
    identity: (u32, u64),
    bytes: Vec<u8>,
}

#[cfg(windows)]
fn persist_windows_recovery(
    root: &Path,
    relative_path: &str,
    content: &[u8],
) -> Result<WindowsRecoveryArtifact, DocumentError> {
    use std::os::windows::fs::{MetadataExt, OpenOptionsExt};
    use windows_sys::Win32::Storage::FileSystem::{
        FILE_ATTRIBUTE_REPARSE_POINT, FILE_FLAG_BACKUP_SEMANTICS, FILE_FLAG_OPEN_REPARSE_POINT,
        FILE_FLAG_WRITE_THROUGH, FILE_SHARE_DELETE, FILE_SHARE_READ,
    };

    let directory_path = root.join(RECOVERY_DIRECTORY);
    let created = match fs::create_dir(&directory_path) {
        Ok(()) => true,
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => false,
        Err(error) => {
            return Err(DocumentError::new(format!(
                "Unable to create recovery directory: {error}"
            )))
        }
    };
    let directory = fs::OpenOptions::new()
        .read(true)
        .share_mode(FILE_SHARE_READ)
        .custom_flags(FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT)
        .open(&directory_path)
        .map_err(|source| {
            DocumentError::new(format!("Unable to open recovery directory: {source}"))
        })?;
    let directory_metadata = directory.metadata().map_err(|source| {
        DocumentError::new(format!("Unable to verify recovery directory: {source}"))
    })?;
    if !directory_metadata.is_dir()
        || directory_metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0
    {
        return Err(DocumentError::new(
            "The recovery directory is not a verified local directory",
        ));
    }
    let directory_identity = windows_file_identity(&directory)?;
    ensure_windows_recovery_owner(&directory_path, created)?;
    let transaction_lock_path = directory_path.join(recovery_auxiliary_name(relative_path, "lock"));
    let transaction_lock = acquire_windows_transaction_lock(&directory_path, relative_path)?;
    let transaction_lock_identity = windows_file_identity(&transaction_lock)?;
    let bytes = encode_recovery_artifact(relative_path, recovery_timestamp()?, content);
    let slot_path = directory_path.join(recovery_slot_name(relative_path));
    match fs::OpenOptions::new()
        .read(true)
        .write(true)
        .share_mode(FILE_SHARE_READ)
        .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT | FILE_FLAG_WRITE_THROUGH)
        .open(&slot_path)
    {
        Ok(mut existing_file) => {
            let metadata = existing_file.metadata().map_err(|source| {
                DocumentError::new(format!("Unable to classify recovery: {source}"))
            })?;
            if !metadata.is_file() || metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0
            {
                drop(existing_file);
                windows_move_file(
                    &slot_path,
                    &directory_path.join(recovery_auxiliary_name(relative_path, "conflict")),
                )?;
            } else {
                let existing = read_file_bytes_portable(&mut existing_file)?;
                if decode_recovery_artifact(&existing)
                    .is_some_and(|(path, _, _)| path == relative_path)
                {
                    let identity = windows_file_identity(&existing_file)?;
                    if windows_path_identity(&slot_path)? != identity {
                        return Err(DocumentError::new(
                            "The recovery slot changed during classification",
                        ));
                    }
                    existing_file
                        .set_len(0)
                        .and_then(|_| existing_file.seek(SeekFrom::Start(0)))
                        .and_then(|_| existing_file.write_all(&bytes))
                        .and_then(|_| existing_file.sync_all())
                        .map_err(|source| {
                            DocumentError::new(format!("Unable to refresh recovery: {source}"))
                        })?;
                    if read_file_bytes_portable(&mut existing_file)? != bytes
                        || windows_path_identity(&slot_path)? != identity
                    {
                        return Err(DocumentError::new("Unable to verify refreshed recovery"));
                    }
                    if !lock_windows_handle(&existing_file) {
                        return Err(DocumentError::new(
                            "Unable to lock the persisted recovery artifact",
                        ));
                    }
                    return Ok(WindowsRecoveryArtifact {
                        _transaction_lock: transaction_lock,
                        transaction_lock_path,
                        transaction_lock_identity,
                        directory_path,
                        directory,
                        directory_identity,
                        slot_path,
                        file: existing_file,
                        identity,
                        bytes,
                    });
                }
                drop(existing_file);
                windows_move_file(
                    &slot_path,
                    &directory_path.join(recovery_auxiliary_name(relative_path, "conflict")),
                )?;
            }
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(_) => windows_move_file(
            &slot_path,
            &directory_path.join(recovery_auxiliary_name(relative_path, "conflict")),
        )?,
    }
    let install_path = directory_path.join(recovery_auxiliary_name(relative_path, "install"));
    let mut file = fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create_new(true)
        .share_mode(FILE_SHARE_READ | FILE_SHARE_DELETE)
        .custom_flags(FILE_FLAG_WRITE_THROUGH)
        .open(&install_path)
        .map_err(|source| DocumentError::new(format!("Unable to create recovery: {source}")))?;
    let mut install_guard = WindowsInstallGuard::new(install_path.clone(), &file)?;
    file.write_all(&bytes)
        .and_then(|_| file.sync_all())
        .map_err(|source| DocumentError::new(format!("Unable to persist recovery: {source}")))?;
    let identity = windows_file_identity(&file)?;
    windows_move_file(&install_path, &slot_path)?;
    install_guard.active = false;
    if !lock_windows_handle(&file) {
        return Err(DocumentError::new(
            "Unable to lock the persisted recovery artifact",
        ));
    }
    let artifact = WindowsRecoveryArtifact {
        _transaction_lock: transaction_lock,
        transaction_lock_path,
        transaction_lock_identity,
        directory_path,
        directory,
        directory_identity,
        slot_path,
        file,
        identity,
        bytes,
    };
    if !windows_recovery_matches(&artifact) {
        return Err(DocumentError::new("Unable to verify persisted recovery"));
    }
    Ok(artifact)
}

#[cfg(windows)]
fn ensure_windows_recovery_owner(directory: &Path, created: bool) -> Result<(), DocumentError> {
    use std::os::windows::fs::{MetadataExt, OpenOptionsExt};
    use windows_sys::Win32::Storage::FileSystem::{
        FILE_ATTRIBUTE_REPARSE_POINT, FILE_FLAG_OPEN_REPARSE_POINT, FILE_FLAG_WRITE_THROUGH,
        FILE_SHARE_READ,
    };

    let path = directory.join(RECOVERY_OWNER_SENTINEL);
    let mut options = fs::OpenOptions::new();
    options
        .read(true)
        .write(created)
        .share_mode(FILE_SHARE_READ)
        .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT | FILE_FLAG_WRITE_THROUGH);
    if created {
        options.create_new(true);
    }
    let mut file = options.open(&path).map_err(|source| {
        DocumentError::new(format!("Unable to verify recovery ownership: {source}"))
    })?;
    if created {
        file.write_all(RECOVERY_OWNER_MAGIC)
            .and_then(|_| file.sync_all())
            .map_err(|source| {
                DocumentError::new(format!("Unable to persist recovery ownership: {source}"))
            })?;
    }
    let metadata = file.metadata().map_err(|source| {
        DocumentError::new(format!("Unable to verify recovery ownership: {source}"))
    })?;
    if !metadata.is_file()
        || metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0
        || read_file_bytes_portable(&mut file)? != RECOVERY_OWNER_MAGIC
    {
        return Err(DocumentError::new(
            "The recovery directory is not owned by TraceDoc",
        ));
    }
    Ok(())
}

#[cfg(windows)]
fn cleanup_successful_windows_transaction(
    artifact: WindowsRecoveryArtifact,
) -> Result<(), DocumentError> {
    if windows_path_identity(&artifact.slot_path)? != artifact.identity
        || windows_path_identity(&artifact.transaction_lock_path)?
            != artifact.transaction_lock_identity
    {
        return Err(DocumentError::new(
            "The completed save artifact changed before cleanup",
        ));
    }
    let directory_path = artifact.directory_path.clone();
    let slot_path = artifact.slot_path.clone();
    let lock_path = artifact.transaction_lock_path.clone();
    drop(artifact);
    fs::remove_file(slot_path).map_err(|source| {
        DocumentError::new(format!("Unable to remove completed recovery: {source}"))
    })?;
    fs::remove_file(lock_path).map_err(|source| {
        DocumentError::new(format!("Unable to remove completed save lock: {source}"))
    })?;
    match fs::remove_dir(directory_path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::DirectoryNotEmpty => Ok(()),
        Err(error) => Err(DocumentError::new(format!(
            "Unable to remove completed recovery directory: {error}"
        ))),
    }
}

#[cfg(windows)]
fn windows_recovery_matches(artifact: &WindowsRecoveryArtifact) -> bool {
    let directory_matches = artifact
        .directory
        .metadata()
        .ok()
        .and_then(|_| windows_file_identity(&artifact.directory).ok())
        == Some(artifact.directory_identity)
        && windows_path_identity(&artifact.directory_path).ok()
            == Some(artifact.directory_identity);
    let data = fs::read(&artifact.slot_path).ok();
    let identity = fs::metadata(&artifact.slot_path)
        .ok()
        .and_then(|_| windows_file_identity(&artifact.file).ok());
    directory_matches
        && data.as_deref() == Some(artifact.bytes.as_slice())
        && identity == Some(artifact.identity)
}

#[cfg(windows)]
fn windows_recovery_message(
    root: &Path,
    root_guard: &WindowsAncestorGuard,
    artifact: &mut WindowsRecoveryArtifact,
) -> String {
    if !windows_recovery_matches(artifact) {
        if let Ok((path, file, identity)) = recreate_windows_recovery(artifact) {
            artifact.slot_path = path;
            artifact.file = file;
            artifact.identity = identity;
        }
    }
    if root_guard.matches().unwrap_or(false) && windows_recovery_matches(artifact) {
        format!(
            "The original remains recoverable at '{}'",
            artifact.slot_path.display()
        )
    } else {
        "Recovery remains available through its held app handle".to_owned()
    }
}

#[cfg(windows)]
fn windows_move_file(source: &Path, destination: &Path) -> Result<(), DocumentError> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Storage::FileSystem::{MoveFileExW, MOVEFILE_WRITE_THROUGH};
    let source: Vec<u16> = source.as_os_str().encode_wide().chain(Some(0)).collect();
    let destination: Vec<u16> = destination
        .as_os_str()
        .encode_wide()
        .chain(Some(0))
        .collect();
    if unsafe {
        MoveFileExW(
            source.as_ptr(),
            destination.as_ptr(),
            MOVEFILE_WRITE_THROUGH,
        )
    } == 0
    {
        return Err(DocumentError::new(format!(
            "Unable to install recovery: {}",
            std::io::Error::last_os_error()
        )));
    }
    Ok(())
}

#[cfg(windows)]
fn acquire_windows_transaction_lock(
    directory: &Path,
    relative_path: &str,
) -> Result<fs::File, DocumentError> {
    use std::os::windows::fs::{MetadataExt, OpenOptionsExt};
    use windows_sys::Win32::Storage::FileSystem::{
        FILE_ATTRIBUTE_REPARSE_POINT, FILE_FLAG_OPEN_REPARSE_POINT, FILE_FLAG_WRITE_THROUGH,
        FILE_SHARE_READ,
    };
    let path = directory.join(recovery_auxiliary_name(relative_path, "lock"));
    let file = fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .share_mode(FILE_SHARE_READ)
        .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT | FILE_FLAG_WRITE_THROUGH)
        .open(path)
        .map_err(|source| {
            DocumentError::new(format!(
                "The document is busy in another TraceDoc save: {source}"
            ))
        })?;
    let metadata = file
        .metadata()
        .map_err(|source| DocumentError::new(format!("Unable to verify save lock: {source}")))?;
    if !metadata.is_file() || metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
        return Err(DocumentError::new(
            "The document save lock is not a regular file",
        ));
    }
    if !lock_windows_handle(&file) {
        return Err(DocumentError::new(
            "The document is busy in another TraceDoc save",
        ));
    }
    Ok(file)
}

#[cfg(windows)]
fn lock_windows_handle(file: &fs::File) -> bool {
    use std::os::windows::io::AsRawHandle;
    use windows_sys::Win32::{
        Storage::FileSystem::{LockFileEx, LOCKFILE_EXCLUSIVE_LOCK, LOCKFILE_FAIL_IMMEDIATELY},
        System::IO::OVERLAPPED,
    };
    let mut overlapped: OVERLAPPED = unsafe { std::mem::zeroed() };
    unsafe {
        LockFileEx(
            file.as_raw_handle() as _,
            LOCKFILE_EXCLUSIVE_LOCK | LOCKFILE_FAIL_IMMEDIATELY,
            0,
            u32::MAX,
            u32::MAX,
            &mut overlapped,
        ) != 0
    }
}

#[cfg(windows)]
struct WindowsInstallGuard {
    path: PathBuf,
    identity: (u32, u64),
    active: bool,
}

#[cfg(windows)]
impl WindowsInstallGuard {
    fn new(path: PathBuf, file: &fs::File) -> Result<Self, DocumentError> {
        Ok(Self {
            path,
            identity: windows_file_identity(file)?,
            active: true,
        })
    }
}

#[cfg(windows)]
impl Drop for WindowsInstallGuard {
    fn drop(&mut self) {
        if self.active && windows_path_identity(&self.path).ok() == Some(self.identity) {
            let _ = fs::remove_file(&self.path);
        }
    }
}

#[cfg(windows)]
fn recreate_windows_recovery(
    artifact: &WindowsRecoveryArtifact,
) -> Result<(PathBuf, fs::File, (u32, u64)), DocumentError> {
    use std::os::windows::fs::OpenOptionsExt;
    use windows_sys::Win32::Storage::FileSystem::{
        FILE_FLAG_WRITE_THROUGH, FILE_SHARE_DELETE, FILE_SHARE_READ,
    };
    let path = match recovery_reference_target(false, !artifact.slot_path.exists()) {
        RecoveryReferenceTarget::Deterministic => artifact.slot_path.clone(),
        RecoveryReferenceTarget::Conflict => artifact.slot_path.with_extension("conflict"),
        RecoveryReferenceTarget::Current => artifact.slot_path.clone(),
    };
    let install = artifact.slot_path.with_extension("failure-install");
    let mut file = fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create_new(true)
        .share_mode(FILE_SHARE_READ | FILE_SHARE_DELETE)
        .custom_flags(FILE_FLAG_WRITE_THROUGH)
        .open(&install)
        .map_err(|source| DocumentError::new(format!("Unable to recreate recovery: {source}")))?;
    let mut install_guard = WindowsInstallGuard::new(install.clone(), &file)?;
    file.write_all(&artifact.bytes)
        .and_then(|_| file.sync_all())
        .map_err(|source| DocumentError::new(format!("Unable to recreate recovery: {source}")))?;
    let identity = windows_file_identity(&file)?;
    windows_move_file(&install, &path)?;
    install_guard.active = false;
    Ok((path, file, identity))
}

#[cfg(windows)]
struct WindowsAncestorGuard {
    entries: Vec<(PathBuf, fs::File, (u32, u64))>,
}

#[cfg(windows)]
impl WindowsAncestorGuard {
    fn open(path: &Path) -> Result<Self, DocumentError> {
        use std::os::windows::fs::OpenOptionsExt;
        use windows_sys::Win32::Storage::FileSystem::{
            FILE_ATTRIBUTE_REPARSE_POINT, FILE_FLAG_BACKUP_SEMANTICS, FILE_FLAG_OPEN_REPARSE_POINT,
            FILE_SHARE_READ, FILE_SHARE_WRITE,
        };

        let mut current = PathBuf::new();
        let mut entries = Vec::new();
        for component in path.components() {
            current.push(component.as_os_str());
            if current.parent().is_none() {
                continue;
            }
            let file = fs::OpenOptions::new()
                .read(true)
                .share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE)
                .custom_flags(FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT)
                .open(&current)
                .map_err(|source| {
                    DocumentError::new(format!("Unable to verify document ancestor: {source}"))
                })?;
            let metadata = file.metadata().map_err(|source| {
                DocumentError::new(format!("Unable to verify document ancestor: {source}"))
            })?;
            use std::os::windows::fs::MetadataExt;
            if metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0 || !metadata.is_dir()
            {
                return Err(DocumentError::new(
                    "A document ancestor is not a verified directory",
                ));
            }
            let identity = windows_file_identity(&file)?;
            entries.push((current.clone(), file, identity));
        }
        Ok(Self { entries })
    }

    fn matches(&self) -> Result<bool, DocumentError> {
        for (path, file, identity) in &self.entries {
            if windows_file_identity(file)?
                != *identity
                || windows_path_identity(path)? != *identity
            {
                return Ok(false);
            }
        }
        Ok(true)
    }
}

#[cfg(windows)]
fn windows_path_identity(path: &Path) -> Result<(u32, u64), DocumentError> {
    use std::os::windows::fs::OpenOptionsExt;
    use windows_sys::Win32::Storage::FileSystem::{
        FILE_FLAG_BACKUP_SEMANTICS, FILE_FLAG_OPEN_REPARSE_POINT, FILE_SHARE_READ,
        FILE_SHARE_WRITE,
    };

    let file = fs::OpenOptions::new()
        .read(true)
        .share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE)
        .custom_flags(FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT)
        .open(path)
        .map_err(|source| DocumentError::new(format!("Unable to verify document path: {source}")))?;
    windows_file_identity(&file)
}

#[cfg(windows)]
fn windows_file_identity(file: &fs::File) -> Result<(u32, u64), DocumentError> {
    use std::os::windows::io::AsRawHandle;
    use windows_sys::Win32::Storage::FileSystem::{
        GetFileInformationByHandle, BY_HANDLE_FILE_INFORMATION,
    };

    let mut information: BY_HANDLE_FILE_INFORMATION = unsafe { std::mem::zeroed() };
    if unsafe { GetFileInformationByHandle(file.as_raw_handle() as _, &mut information) } == 0 {
        return Err(DocumentError::new(format!(
            "The document file identity is unavailable: {}",
            std::io::Error::last_os_error()
        )));
    }
    Ok((
        information.dwVolumeSerialNumber,
        (u64::from(information.nFileIndexHigh) << 32) | u64::from(information.nFileIndexLow),
    ))
}

#[cfg(windows)]
fn read_file_bytes_portable(file: &mut fs::File) -> Result<Vec<u8>, DocumentError> {
    file.seek(SeekFrom::Start(0))
        .and_then(|_| {
            let mut value = Vec::new();
            file.read_to_end(&mut value).map(|_| value)
        })
        .map_err(|source| DocumentError::new(format!("Unable to verify save content: {source}")))
}

#[cfg(not(any(target_os = "linux", target_os = "macos", windows)))]
fn transactional_replace(
    _root: &Path,
    _path: &Path,
    _content: &[u8],
    _expected_content_token: &str,
    _before_commit: impl FnOnce(&Path),
) -> Result<Option<String>, DocumentError> {
    Err(DocumentError::new(
        "Safe transactional saving is unavailable on this platform",
    ))
}

#[cfg(any(unix, windows, test))]
const RECOVERY_MAGIC: &[u8; 8] = b"TDREC001";

#[cfg(any(unix, windows, test))]
fn encode_recovery_artifact(path: &str, timestamp: u64, content: &[u8]) -> Vec<u8> {
    let path = path.as_bytes();
    let mut artifact = Vec::with_capacity(60 + path.len() + content.len());
    artifact.extend_from_slice(RECOVERY_MAGIC);
    artifact.extend_from_slice(&timestamp.to_le_bytes());
    artifact.extend_from_slice(&(path.len() as u32).to_le_bytes());
    artifact.extend_from_slice(&(content.len() as u64).to_le_bytes());
    artifact.extend_from_slice(blake3::hash(content).as_bytes());
    artifact.extend_from_slice(path);
    artifact.extend_from_slice(content);
    artifact
}

#[cfg(any(unix, windows, test))]
fn decode_recovery_artifact(artifact: &[u8]) -> Option<(&str, u64, &[u8])> {
    if artifact.len() < 60 || &artifact[..8] != RECOVERY_MAGIC {
        return None;
    }
    let timestamp = u64::from_le_bytes(artifact[8..16].try_into().ok()?);
    let path_length = u32::from_le_bytes(artifact[16..20].try_into().ok()?) as usize;
    let content_length = u64::from_le_bytes(artifact[20..28].try_into().ok()?) as usize;
    let header_end = 60usize.checked_add(path_length)?;
    let content_end = header_end.checked_add(content_length)?;
    if content_end != artifact.len() {
        return None;
    }
    let path = std::str::from_utf8(&artifact[60..header_end]).ok()?;
    let content = &artifact[header_end..content_end];
    (blake3::hash(content).as_bytes() == &artifact[28..60]).then_some((path, timestamp, content))
}

#[cfg(any(unix, windows, test))]
fn recovery_slot_name(relative_path: &str) -> String {
    format!(
        "{}.recovery",
        blake3::hash(relative_path.as_bytes()).to_hex()
    )
}

#[cfg(any(unix, windows, test))]
fn recovery_auxiliary_name(relative_path: &str, suffix: &str) -> String {
    format!(
        "{}.{}",
        blake3::hash(relative_path.as_bytes()).to_hex(),
        suffix
    )
}

#[cfg(any(unix, windows, test))]
fn recovery_sibling_name(slot_name: &str, suffix: &str) -> Option<String> {
    let hash = slot_name.split('.').next()?;
    (!hash.is_empty()
        && hash
            .chars()
            .all(|value| value.is_ascii_hexdigit() && !value.is_ascii_uppercase()))
    .then(|| format!("{hash}.{suffix}"))
}

#[cfg(any(unix, windows, test))]
#[derive(Debug, PartialEq, Eq)]
enum RecoveryReferenceTarget {
    Current,
    Deterministic,
    Conflict,
}

#[cfg(any(unix, windows, test))]
fn recovery_reference_target(path_matches: bool, path_absent: bool) -> RecoveryReferenceTarget {
    if path_matches {
        RecoveryReferenceTarget::Current
    } else if path_absent {
        RecoveryReferenceTarget::Deterministic
    } else {
        RecoveryReferenceTarget::Conflict
    }
}

#[cfg(unix)]
struct RecoveryArtifact {
    file: fs::File,
    identity: (u64, u64),
    bytes: Vec<u8>,
    slot_name: String,
}

#[cfg(unix)]
struct OpenedRecoverySlot {
    file: fs::File,
    identity: (u64, u64),
    bytes: Vec<u8>,
}

#[cfg(unix)]
fn held_recovery_matches(artifact: &mut RecoveryArtifact) -> bool {
    read_file_bytes(&mut artifact.file).is_ok_and(|bytes| bytes == artifact.bytes)
}

#[cfg(any(unix, windows))]
fn recovery_timestamp() -> Result<u64, DocumentError> {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .map_err(|source| DocumentError::new(format!("Unable to timestamp recovery: {source}")))
}

#[cfg(unix)]
fn open_recovery_directory(root: &fs::File) -> Result<fs::File, DocumentError> {
    use std::os::fd::AsRawFd;

    let name = CString::new(RECOVERY_DIRECTORY.as_bytes())
        .map_err(|_| DocumentError::new("The recovery directory name is malformed"))?;
    let created = if unsafe { libc::mkdirat(root.as_raw_fd(), name.as_ptr(), 0o700) } != 0 {
        let error = std::io::Error::last_os_error();
        if error.kind() != std::io::ErrorKind::AlreadyExists {
            return Err(DocumentError::new(format!(
                "Unable to create recovery directory: {error}"
            )));
        }
        false
    } else {
        true
    };
    let directory = openat_file(
        root.as_raw_fd(),
        std::ffi::OsStr::new(RECOVERY_DIRECTORY),
        libc::O_RDONLY | libc::O_DIRECTORY,
        0,
    )?;
    if created && unsafe { libc::fchmod(directory.as_raw_fd(), 0o700) } != 0 {
        return Err(DocumentError::new(format!(
            "Unable to secure recovery directory: {}",
            std::io::Error::last_os_error()
        )));
    }
    ensure_unix_recovery_owner(&directory, created)?;
    Ok(directory)
}

#[cfg(unix)]
fn ensure_unix_recovery_owner(directory: &fs::File, created: bool) -> Result<(), DocumentError> {
    use std::os::fd::AsRawFd;

    let metadata = directory.metadata().map_err(|source| {
        DocumentError::new(format!("Unable to verify recovery directory: {source}"))
    })?;
    if !is_safe_recovery_directory_metadata(&metadata) {
        return Err(DocumentError::new(
            "The recovery directory is not owned by TraceDoc",
        ));
    }
    let flags = if created {
        libc::O_RDWR | libc::O_CREAT | libc::O_EXCL
    } else {
        libc::O_RDONLY | libc::O_NONBLOCK
    };
    let mut sentinel = openat_file(
        directory.as_raw_fd(),
        std::ffi::OsStr::new(RECOVERY_OWNER_SENTINEL),
        flags,
        0o600,
    )?;
    if created {
        sentinel
            .write_all(RECOVERY_OWNER_MAGIC)
            .and_then(|_| sentinel.sync_all())
            .and_then(|_| directory.sync_all())
            .map_err(|source| {
                DocumentError::new(format!("Unable to persist recovery ownership: {source}"))
            })?;
    }
    let metadata = sentinel.metadata().map_err(|source| {
        DocumentError::new(format!("Unable to verify recovery ownership: {source}"))
    })?;
    let bytes = read_file_bytes(&mut sentinel)?;
    if !is_safe_recovery_owner_metadata(&metadata) || bytes != RECOVERY_OWNER_MAGIC {
        return Err(DocumentError::new(
            "The recovery directory is not owned by TraceDoc",
        ));
    }
    Ok(())
}

#[cfg(unix)]
fn acquire_unix_transaction_lock(
    directory: &fs::File,
    relative_path: &str,
) -> Result<fs::File, DocumentError> {
    use std::os::fd::AsRawFd;
    let name = recovery_auxiliary_name(relative_path, "lock");
    let file = openat_file(
        directory.as_raw_fd(),
        std::ffi::OsStr::new(&name),
        libc::O_RDWR | libc::O_CREAT | libc::O_NONBLOCK,
        0o600,
    )?;
    if !file
        .metadata()
        .map_err(|source| DocumentError::new(format!("Unable to verify save lock: {source}")))?
        .is_file()
    {
        return Err(DocumentError::new(
            "The document save lock is not a regular file",
        ));
    }
    if unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) } != 0 {
        return Err(DocumentError::new(
            "The document is busy in another TraceDoc save",
        ));
    }
    Ok(file)
}

#[cfg(unix)]
fn cleanup_successful_unix_transaction(
    directory: &fs::File,
    relative_path: &str,
    recovery: &RecoveryArtifact,
    staging_name: &str,
    staging_identity: (u64, u64),
    lock_identity: (u64, u64),
) -> Result<(), DocumentError> {
    remove_unix_transaction_entry(directory, staging_name, staging_identity)?;
    remove_unix_transaction_entry(directory, &recovery.slot_name, recovery.identity)?;
    remove_unix_transaction_entry(
        directory,
        &recovery_auxiliary_name(relative_path, "lock"),
        lock_identity,
    )?;
    directory.sync_all().map_err(|source| {
        DocumentError::new(format!(
            "Unable to persist completed save cleanup: {source}"
        ))
    })
}

#[cfg(unix)]
fn remove_unix_transaction_entry(
    directory: &fs::File,
    name: &str,
    expected_identity: (u64, u64),
) -> Result<(), DocumentError> {
    use std::{os::fd::AsRawFd, os::unix::ffi::OsStrExt};

    let entry = std::ffi::OsStr::new(name);
    if !unix_entry_matches_identity(directory.as_raw_fd(), entry, expected_identity) {
        return Err(DocumentError::new(
            "The completed save artifact changed before cleanup",
        ));
    }
    let name = CString::new(entry.as_bytes())
        .map_err(|_| DocumentError::new("The completed save artifact name is malformed"))?;
    if unsafe { libc::unlinkat(directory.as_raw_fd(), name.as_ptr(), 0) } != 0 {
        return Err(DocumentError::new(format!(
            "Unable to remove completed save artifact: {}",
            std::io::Error::last_os_error()
        )));
    }
    Ok(())
}

#[cfg(unix)]
fn recovery_artifact_matches(directory: &fs::File, artifact: &RecoveryArtifact) -> bool {
    use std::os::fd::AsRawFd;
    unix_entry_matches_identity(
        directory.as_raw_fd(),
        std::ffi::OsStr::new(&artifact.slot_name),
        artifact.identity,
    ) && read_named_bytes(
        directory.as_raw_fd(),
        std::ffi::OsStr::new(&artifact.slot_name),
    )
    .as_deref()
        == Some(artifact.bytes.as_slice())
}

#[cfg(unix)]
fn open_regular_recovery_slot(
    directory: &fs::File,
    slot_name: &str,
    relative_path: &str,
) -> Result<Option<OpenedRecoverySlot>, DocumentError> {
    use std::os::fd::{AsRawFd, FromRawFd};
    use std::os::unix::fs::FileTypeExt;
    let slot = CString::new(slot_name.as_bytes())
        .map_err(|_| DocumentError::new("The recovery slot is malformed"))?;
    let fd = unsafe {
        libc::openat(
            directory.as_raw_fd(),
            slot.as_ptr(),
            libc::O_RDWR | libc::O_CLOEXEC | libc::O_NOFOLLOW | libc::O_NONBLOCK,
        )
    };
    if fd < 0 {
        let error = std::io::Error::last_os_error();
        if error.kind() == std::io::ErrorKind::NotFound {
            return Ok(None);
        }
        preserve_unknown_recovery_slot(directory, slot_name, relative_path)?;
        return Ok(None);
    }
    let mut file = unsafe { fs::File::from_raw_fd(fd) };
    let metadata = file
        .metadata()
        .map_err(|source| DocumentError::new(format!("Unable to classify recovery: {source}")))?;
    if !metadata.file_type().is_file()
        || metadata.file_type().is_fifo()
        || metadata.file_type().is_symlink()
    {
        drop(file);
        preserve_unknown_recovery_slot(directory, slot_name, relative_path)?;
        return Ok(None);
    }
    let identity = unix_file_identity(&file)?;
    let bytes = read_file_bytes(&mut file)?;
    if decode_recovery_artifact(&bytes).is_none_or(|(path, _, _)| path != relative_path) {
        drop(file);
        preserve_unknown_recovery_slot(directory, slot_name, relative_path)?;
        return Ok(None);
    }
    Ok(Some(OpenedRecoverySlot {
        file,
        identity,
        bytes,
    }))
}

#[cfg(unix)]
fn preserve_unknown_recovery_slot(
    directory: &fs::File,
    slot_name: &str,
    relative_path: &str,
) -> Result<(), DocumentError> {
    use std::os::fd::AsRawFd;
    let conflict_name = recovery_auxiliary_name(relative_path, "conflict");
    rename_noreplace_between(
        directory.as_raw_fd(),
        std::ffi::OsStr::new(slot_name),
        directory.as_raw_fd(),
        std::ffi::OsStr::new(&conflict_name),
    )
}

#[cfg(unix)]
fn refresh_recovery_artifact(
    directory: &fs::File,
    slot_name: String,
    bytes: Vec<u8>,
    mut file: fs::File,
    identity: (u64, u64),
    previous: &[u8],
) -> Result<RecoveryArtifact, DocumentError> {
    use std::os::fd::AsRawFd;
    if unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) } != 0
        || !unix_entry_matches_identity(
            directory.as_raw_fd(),
            std::ffi::OsStr::new(&slot_name),
            identity,
        )
        || read_file_bytes(&mut file)? != previous
    {
        return Err(DocumentError::new(
            "The recovery slot changed during classification",
        ));
    }
    file.set_len(0)
        .and_then(|_| file.seek(SeekFrom::Start(0)))
        .and_then(|_| file.write_all(&bytes))
        .and_then(|_| file.sync_all())
        .map_err(|source| DocumentError::new(format!("Unable to refresh recovery: {source}")))?;
    if read_file_bytes(&mut file)? != bytes
        || !unix_entry_matches_identity(
            directory.as_raw_fd(),
            std::ffi::OsStr::new(&slot_name),
            identity,
        )
    {
        return Err(DocumentError::new("Unable to verify refreshed recovery"));
    }
    Ok(RecoveryArtifact {
        file,
        identity,
        bytes,
        slot_name,
    })
}

#[cfg(target_os = "linux")]
fn persist_recovery_artifact(
    directory: &fs::File,
    relative_path: &str,
    content: &[u8],
) -> Result<RecoveryArtifact, DocumentError> {
    persist_recovery_artifact_with_hook(directory, relative_path, content, || {})
}

#[cfg(target_os = "linux")]
fn persist_recovery_artifact_with_hook(
    directory: &fs::File,
    relative_path: &str,
    content: &[u8],
    before_install: impl FnOnce(),
) -> Result<RecoveryArtifact, DocumentError> {
    use std::os::fd::AsRawFd;
    let bytes = encode_recovery_artifact(relative_path, recovery_timestamp()?, content);
    let slot_name = recovery_slot_name(relative_path);
    if let Some(existing) = open_regular_recovery_slot(directory, &slot_name, relative_path)? {
        return refresh_recovery_artifact(
            directory,
            slot_name,
            bytes,
            existing.file,
            existing.identity,
            &existing.bytes,
        );
    }
    let mut file = open_unnamed_temp(directory.as_raw_fd())?;
    file.write_all(&bytes)
        .and_then(|_| file.sync_all())
        .map_err(|source| DocumentError::new(format!("Unable to persist recovery: {source}")))?;
    let identity = unix_file_identity(&file)?;
    file.seek(SeekFrom::Start(0))
        .map_err(|source| DocumentError::new(format!("Unable to verify recovery: {source}")))?;
    if read_file_bytes(&mut file)? != bytes {
        return Err(DocumentError::new("Unable to verify recovery artifact"));
    }
    before_install();
    link_unnamed_temp(
        file.as_raw_fd(),
        directory.as_raw_fd(),
        std::ffi::OsStr::new(&slot_name),
    )?;
    if unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) } != 0 {
        return Err(DocumentError::new(
            "Unable to lock the persisted recovery artifact",
        ));
    }
    directory.sync_all().map_err(|source| {
        DocumentError::new(format!("Unable to persist recovery directory: {source}"))
    })?;
    let artifact = RecoveryArtifact {
        file,
        identity,
        bytes,
        slot_name,
    };
    if !recovery_artifact_matches(directory, &artifact) {
        return Err(DocumentError::new(
            "Unable to verify installed recovery artifact",
        ));
    }
    Ok(artifact)
}

#[cfg(target_os = "macos")]
fn persist_recovery_artifact(
    directory: &fs::File,
    relative_path: &str,
    content: &[u8],
) -> Result<RecoveryArtifact, DocumentError> {
    use std::os::fd::AsRawFd;
    let bytes = encode_recovery_artifact(relative_path, recovery_timestamp()?, content);
    let slot_name = recovery_slot_name(relative_path);
    if let Some(existing) = open_regular_recovery_slot(directory, &slot_name, relative_path)? {
        return refresh_recovery_artifact(
            directory,
            slot_name,
            bytes,
            existing.file,
            existing.identity,
            &existing.bytes,
        );
    }
    let install_name = recovery_auxiliary_name(relative_path, "install");
    let mut file = openat_file(
        directory.as_raw_fd(),
        std::ffi::OsStr::new(&install_name),
        libc::O_RDWR | libc::O_CREAT | libc::O_EXCL,
        0o600,
    )?;
    file.write_all(&bytes)
        .and_then(|_| file.sync_all())
        .map_err(|source| DocumentError::new(format!("Unable to persist recovery: {source}")))?;
    let identity = unix_file_identity(&file)?;
    let mut guard = MacInstallGuard {
        directory_fd: directory.as_raw_fd(),
        name: install_name.clone(),
        identity,
        active: true,
    };
    rename_noreplace_between(
        directory.as_raw_fd(),
        std::ffi::OsStr::new(&install_name),
        directory.as_raw_fd(),
        std::ffi::OsStr::new(&slot_name),
    )?;
    guard.active = false;
    directory.sync_all().map_err(|source| {
        DocumentError::new(format!("Unable to persist recovery directory: {source}"))
    })?;
    let artifact = RecoveryArtifact {
        file,
        identity,
        bytes,
        slot_name,
    };
    if !recovery_artifact_matches(directory, &artifact) {
        return Err(DocumentError::new(
            "Unable to verify installed recovery artifact",
        ));
    }
    Ok(artifact)
}

#[cfg(target_os = "macos")]
struct MacInstallGuard {
    directory_fd: std::os::fd::RawFd,
    name: String,
    identity: (u64, u64),
    active: bool,
}

#[cfg(target_os = "macos")]
impl Drop for MacInstallGuard {
    fn drop(&mut self) {
        use std::os::unix::ffi::OsStrExt;
        if !self.active
            || !unix_entry_matches_identity(
                self.directory_fd,
                std::ffi::OsStr::new(&self.name),
                self.identity,
            )
        {
            return;
        }
        if let Ok(name) = CString::new(std::ffi::OsStr::new(&self.name).as_bytes()) {
            unsafe {
                libc::unlinkat(self.directory_fd, name.as_ptr(), 0);
            }
        }
    }
}

#[cfg(unix)]
fn ensure_recovery_reference(
    root: &Path,
    root_directory: &fs::File,
    directory: &fs::File,
    artifact: &mut RecoveryArtifact,
) -> String {
    use std::os::fd::AsRawFd;
    if !recovery_artifact_matches(directory, artifact) {
        let path_absent = unix_entry_identity(
            directory.as_raw_fd(),
            std::ffi::OsStr::new(&artifact.slot_name),
        )
        .is_none();
        let name = match recovery_reference_target(false, path_absent) {
            RecoveryReferenceTarget::Deterministic => artifact.slot_name.clone(),
            RecoveryReferenceTarget::Conflict => {
                recovery_sibling_name(&artifact.slot_name, "conflict")
                    .unwrap_or_else(|| artifact.slot_name.clone())
            }
            RecoveryReferenceTarget::Current => artifact.slot_name.clone(),
        };
        let Ok((file, identity)) = recreate_unix_recovery(directory, &name, &artifact.bytes) else {
            return "Recovery remains available through its held app handle".to_owned();
        };
        if directory.sync_all().is_err() {
            return "Recovery remains available through its held app handle".to_owned();
        }
        artifact.file = file;
        artifact.identity = identity;
        artifact.slot_name = name;
    }
    let root_stable = unix_file_identity(root_directory)
        .ok()
        .is_some_and(|identity| workspace_path_matches(root_directory, root, identity));
    if root_stable
        && recovery_directory_attached(root_directory, directory)
        && recovery_artifact_matches(directory, artifact)
    {
        format!(
            "The original remains recoverable at '{}'",
            root.join(RECOVERY_DIRECTORY)
                .join(&artifact.slot_name)
                .display()
        )
    } else {
        "The workspace location changed; recovery remains available through its held app handle"
            .to_owned()
    }
}

#[cfg(target_os = "linux")]
fn recreate_unix_recovery(
    directory: &fs::File,
    name: &str,
    bytes: &[u8],
) -> Result<(fs::File, (u64, u64)), DocumentError> {
    use std::os::fd::AsRawFd;
    let mut file = open_unnamed_temp(directory.as_raw_fd())?;
    file.write_all(bytes)
        .and_then(|_| file.sync_all())
        .map_err(|source| DocumentError::new(format!("Unable to recreate recovery: {source}")))?;
    let identity = unix_file_identity(&file)?;
    link_unnamed_temp(
        file.as_raw_fd(),
        directory.as_raw_fd(),
        std::ffi::OsStr::new(name),
    )?;
    Ok((file, identity))
}

#[cfg(target_os = "macos")]
fn recreate_unix_recovery(
    directory: &fs::File,
    name: &str,
    bytes: &[u8],
) -> Result<(fs::File, (u64, u64)), DocumentError> {
    use std::os::fd::AsRawFd;
    let install = recovery_sibling_name(name, "failure-install")
        .ok_or_else(|| DocumentError::new("The recovery slot is malformed"))?;
    let mut file = openat_file(
        directory.as_raw_fd(),
        std::ffi::OsStr::new(&install),
        libc::O_RDWR | libc::O_CREAT | libc::O_EXCL,
        0o600,
    )?;
    file.write_all(bytes)
        .and_then(|_| file.sync_all())
        .map_err(|source| DocumentError::new(format!("Unable to recreate recovery: {source}")))?;
    let identity = unix_file_identity(&file)?;
    let mut guard = MacInstallGuard {
        directory_fd: directory.as_raw_fd(),
        name: install.clone(),
        identity,
        active: true,
    };
    rename_noreplace_between(
        directory.as_raw_fd(),
        std::ffi::OsStr::new(&install),
        directory.as_raw_fd(),
        std::ffi::OsStr::new(name),
    )?;
    guard.active = false;
    Ok((file, identity))
}

#[cfg(unix)]
fn recovery_directory_attached(root: &fs::File, directory: &fs::File) -> bool {
    use std::os::fd::AsRawFd;

    let expected = unix_file_identity(directory).ok();
    openat_file(
        root.as_raw_fd(),
        std::ffi::OsStr::new(RECOVERY_DIRECTORY),
        libc::O_RDONLY | libc::O_DIRECTORY,
        0,
    )
    .and_then(|current| unix_file_identity(&current))
    .ok()
        == expected
}

#[cfg(unix)]
fn workspace_path_matches(root_directory: &fs::File, root: &Path, identity: (u64, u64)) -> bool {
    directory_path_matches(root_directory, root)
        && unix_file_identity(root_directory).ok() == Some(identity)
}

#[cfg(unix)]
fn relative_directory_matches(
    root_fd: std::os::fd::RawFd,
    relative: &Path,
    identity: (u64, u64),
) -> bool {
    open_relative_directory_no_follow(root_fd, relative)
        .and_then(|directory| unix_file_identity(&directory))
        .is_ok_and(|current| current == identity)
}

#[cfg(unix)]
fn open_relative_directory_no_follow(
    root_fd: std::os::fd::RawFd,
    relative: &Path,
) -> Result<fs::File, DocumentError> {
    use std::os::fd::{AsRawFd, FromRawFd};

    let duplicate = unsafe { libc::fcntl(root_fd, libc::F_DUPFD_CLOEXEC, 0) };
    if duplicate < 0 {
        return Err(DocumentError::new(format!(
            "Unable to retain workspace root: {}",
            std::io::Error::last_os_error()
        )));
    }
    let mut directory = unsafe { fs::File::from_raw_fd(duplicate) };
    for component in relative.components() {
        let std::path::Component::Normal(name) = component else {
            return Err(DocumentError::new("The document parent path is malformed"));
        };
        directory = openat_file(
            directory.as_raw_fd(),
            name,
            libc::O_RDONLY | libc::O_DIRECTORY,
            0,
        )?;
    }
    Ok(directory)
}

#[cfg(unix)]
fn recovery_message(
    root: &Path,
    root_directory: &fs::File,
    recovery_directory: &fs::File,
    artifact: &mut RecoveryArtifact,
) -> String {
    ensure_recovery_reference(root, root_directory, recovery_directory, artifact)
}

#[cfg(unix)]
fn conflict_with_recovery(
    root: &Path,
    root_directory: &fs::File,
    recovery_directory: &fs::File,
    artifact: &mut RecoveryArtifact,
) -> DocumentError {
    DocumentError::new(format!(
        "The document changed externally. {}",
        recovery_message(root, root_directory, recovery_directory, artifact)
    ))
}

#[cfg(unix)]
fn directory_path_matches(directory: &fs::File, path: &Path) -> bool {
    use std::os::unix::fs::MetadataExt;

    let Ok(handle_metadata) = directory.metadata() else {
        return false;
    };
    let Ok(path_metadata) = fs::symlink_metadata(path) else {
        return false;
    };
    path_metadata.is_dir()
        && !path_metadata.file_type().is_symlink()
        && handle_metadata.dev() == path_metadata.dev()
        && handle_metadata.ino() == path_metadata.ino()
}

#[cfg(unix)]
fn unix_file_identity(file: &fs::File) -> Result<(u64, u64), DocumentError> {
    use std::os::unix::fs::MetadataExt;

    file.metadata()
        .map(|metadata| (metadata.dev(), metadata.ino()))
        .map_err(|source| DocumentError::new(format!("Unable to verify save entry: {source}")))
}

#[cfg(target_os = "linux")]
fn open_unnamed_temp(directory_fd: std::os::fd::RawFd) -> Result<fs::File, DocumentError> {
    use std::os::fd::FromRawFd;

    let current =
        CString::new(".").map_err(|_| DocumentError::new("The document parent is malformed"))?;
    let fd = unsafe {
        libc::openat(
            directory_fd,
            current.as_ptr(),
            libc::O_TMPFILE | libc::O_RDWR | libc::O_CLOEXEC,
            0o600,
        )
    };
    if fd < 0 {
        Err(DocumentError::new(format!(
            "Unable to create a handle-bound save entry: {}",
            std::io::Error::last_os_error()
        )))
    } else {
        Ok(unsafe { fs::File::from_raw_fd(fd) })
    }
}

#[cfg(unix)]
fn read_file_bytes(file: &mut fs::File) -> Result<Vec<u8>, DocumentError> {
    file.seek(SeekFrom::Start(0))
        .and_then(|_| {
            let mut value = Vec::new();
            file.read_to_end(&mut value).map(|_| value)
        })
        .map_err(|source| DocumentError::new(format!("Unable to verify save content: {source}")))
}

#[cfg(unix)]
fn read_named_bytes(directory_fd: std::os::fd::RawFd, name: &std::ffi::OsStr) -> Option<Vec<u8>> {
    let mut file = openat_file(directory_fd, name, libc::O_RDONLY, 0).ok()?;
    read_file_bytes(&mut file).ok()
}

#[cfg(unix)]
fn read_named_token(directory_fd: std::os::fd::RawFd, name: &std::ffi::OsStr) -> Option<String> {
    read_named_bytes(directory_fd, name).map(|content| content_token_bytes(&content))
}

fn content_token_bytes(content: &[u8]) -> String {
    blake3::hash(content).to_hex().to_string()
}

#[cfg(any(unix, test))]
fn captured_baseline_matches(
    expected_identity: (u64, u64),
    expected_token: &str,
    captured_identity: Option<(u64, u64)>,
    captured_content: Option<&[u8]>,
) -> bool {
    captured_identity == Some(expected_identity)
        && captured_content.is_some_and(|content| content_token_bytes(content) == expected_token)
}

#[cfg(unix)]
fn ensure_no_pending_quarantine(
    directory: &fs::File,
    relative_path: &str,
) -> Result<(), DocumentError> {
    use std::os::fd::AsRawFd;
    for suffix in ["external", "blocked"] {
        let name = recovery_auxiliary_name(relative_path, suffix);
        if unix_entry_identity(directory.as_raw_fd(), std::ffi::OsStr::new(&name)).is_some() {
            return Err(DocumentError::new(
                "The document has a quarantined external save state. Resolve the conflict before saving again.",
            ));
        }
    }
    Ok(())
}

#[cfg(unix)]
fn quarantine_after_exchange(
    directory: &fs::File,
    relative_path: &str,
    staging_name: &str,
) -> String {
    use std::os::fd::AsRawFd;
    let staging = std::ffi::OsStr::new(staging_name);
    let external_name = recovery_auxiliary_name(relative_path, "external");
    let external = std::ffi::OsStr::new(&external_name);
    let staged = openat_file(
        directory.as_raw_fd(),
        staging,
        libc::O_RDONLY | libc::O_NONBLOCK,
        0,
    )
    .and_then(|mut file| {
        if !file
            .metadata()
            .map_err(|source| {
                DocumentError::new(format!("Unable to verify bounded staging: {source}"))
            })?
            .is_file()
        {
            return Err(DocumentError::new(
                "The bounded staging is not a regular file",
            ));
        }
        let identity = unix_file_identity(&file)?;
        let content = read_file_bytes(&mut file)?;
        Ok((file, identity, content))
    });
    let staged_token = staged
        .as_ref()
        .map(|(_, _, content)| content_token_bytes(content))
        .unwrap_or_else(|_| "unavailable".to_owned());
    let blocked_name = recovery_auxiliary_name(relative_path, "blocked");
    let blocked = std::ffi::OsStr::new(&blocked_name);
    if unix_entry_identity(directory.as_raw_fd(), blocked).is_none() {
        if let Ok(mut file) = openat_file(
            directory.as_raw_fd(),
            blocked,
            libc::O_RDWR | libc::O_CREAT | libc::O_EXCL,
            0o600,
        ) {
            let state = format!("TDBLOCK1\n{staged_token}\n");
            let _ = file
                .write_all(state.as_bytes())
                .and_then(|_| file.sync_all())
                .and_then(|_| directory.sync_all());
        }
    }
    if let Ok((mut staged_file, staged_identity, staged_content)) = staged {
        if unix_entry_matches_identity(directory.as_raw_fd(), staging, staged_identity)
            && rename_noreplace_between(
                directory.as_raw_fd(),
                staging,
                directory.as_raw_fd(),
                external,
            )
            .is_ok()
            && directory.sync_all().is_ok()
            && unix_entry_matches_identity(directory.as_raw_fd(), external, staged_identity)
            && read_named_bytes(directory.as_raw_fd(), external).as_deref()
                == Some(staged_content.as_slice())
            && unix_file_identity(&staged_file).ok() == Some(staged_identity)
            && read_file_bytes(&mut staged_file).ok().as_deref() == Some(staged_content.as_slice())
        {
            return format!(
                "The transaction state is quarantined in bounded slot '{external_name}' with token {}; save state '{blocked_name}' blocks reuse",
                content_token_bytes(&staged_content)
            );
        }
    }
    format!(
        "The captured transaction with token {staged_token} remains in bounded external slot '{external_name}' or staging slot '{staging_name}'; save state '{blocked_name}' blocks reuse"
    )
}

#[cfg(target_os = "linux")]
fn rename_noreplace_between(
    source_directory_fd: std::os::fd::RawFd,
    source: &std::ffi::OsStr,
    destination_directory_fd: std::os::fd::RawFd,
    destination: &std::ffi::OsStr,
) -> Result<(), DocumentError> {
    use std::os::unix::ffi::OsStrExt;

    let source = CString::new(source.as_bytes())
        .map_err(|_| DocumentError::new("The document path is malformed"))?;
    let destination = CString::new(destination.as_bytes())
        .map_err(|_| DocumentError::new("The recovery path is malformed"))?;
    let result = unsafe {
        libc::syscall(
            libc::SYS_renameat2,
            source_directory_fd,
            source.as_ptr(),
            destination_directory_fd,
            destination.as_ptr(),
            libc::RENAME_NOREPLACE,
        )
    };
    if result == 0 {
        Ok(())
    } else {
        Err(DocumentError::new(format!(
            "Unable to reserve the document recovery entry: {}",
            std::io::Error::last_os_error()
        )))
    }
}

#[cfg(target_os = "linux")]
fn rename_exchange_between(
    source_directory_fd: std::os::fd::RawFd,
    source: &std::ffi::OsStr,
    destination_directory_fd: std::os::fd::RawFd,
    destination: &std::ffi::OsStr,
) -> Result<(), DocumentError> {
    use std::os::unix::ffi::OsStrExt;
    let source = CString::new(source.as_bytes())
        .map_err(|_| DocumentError::new("The document path is malformed"))?;
    let destination = CString::new(destination.as_bytes())
        .map_err(|_| DocumentError::new("The staging path is malformed"))?;
    if unsafe {
        libc::syscall(
            libc::SYS_renameat2,
            source_directory_fd,
            source.as_ptr(),
            destination_directory_fd,
            destination.as_ptr(),
            libc::RENAME_EXCHANGE,
        )
    } == 0
    {
        Ok(())
    } else {
        Err(DocumentError::new(format!(
            "Unable to exchange the verified save entry: {}",
            std::io::Error::last_os_error()
        )))
    }
}

#[cfg(target_os = "macos")]
fn rename_exchange_between(
    source_directory_fd: std::os::fd::RawFd,
    source: &std::ffi::OsStr,
    destination_directory_fd: std::os::fd::RawFd,
    destination: &std::ffi::OsStr,
) -> Result<(), DocumentError> {
    use std::os::unix::ffi::OsStrExt;
    let source = CString::new(source.as_bytes())
        .map_err(|_| DocumentError::new("The document path is malformed"))?;
    let destination = CString::new(destination.as_bytes())
        .map_err(|_| DocumentError::new("The staging path is malformed"))?;
    if unsafe {
        libc::renameatx_np(
            source_directory_fd,
            source.as_ptr(),
            destination_directory_fd,
            destination.as_ptr(),
            libc::RENAME_SWAP,
        )
    } == 0
    {
        Ok(())
    } else {
        Err(DocumentError::new(format!(
            "Unable to exchange the verified save entry: {}",
            std::io::Error::last_os_error()
        )))
    }
}

#[cfg(target_os = "linux")]
fn prepare_exchange_staging(
    directory: &fs::File,
    name: &str,
    source: &fs::File,
    temp: &mut fs::File,
    content: &[u8],
    temp_identity: (u64, u64),
) -> Result<(fs::File, (u64, u64)), DocumentError> {
    use std::os::fd::AsRawFd;
    let entry = std::ffi::OsStr::new(name);
    match openat_file(
        directory.as_raw_fd(),
        entry,
        libc::O_RDWR | libc::O_NONBLOCK,
        0,
    ) {
        Ok(mut staging) => {
            let metadata = staging.metadata().map_err(|source| {
                DocumentError::new(format!("Unable to classify save staging: {source}"))
            })?;
            if !metadata.is_file()
                || unsafe { libc::flock(staging.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) } != 0
            {
                return Err(DocumentError::new(
                    "The bounded save staging entry is unavailable",
                ));
            }
            let identity = unix_file_identity(&staging)?;
            if !unix_entry_matches_identity(directory.as_raw_fd(), entry, identity) {
                return Err(DocumentError::new(
                    "The bounded save staging entry changed externally",
                ));
            }
            copy_unix_metadata(source, &staging)?;
            staging
                .set_len(0)
                .and_then(|_| staging.seek(SeekFrom::Start(0)))
                .and_then(|_| staging.write_all(content))
                .and_then(|_| staging.sync_all())
                .map_err(|source| {
                    DocumentError::new(format!("Unable to prepare bounded save staging: {source}"))
                })?;
            if read_file_bytes(&mut staging)? != content
                || !unix_entry_matches_identity(directory.as_raw_fd(), entry, identity)
            {
                return Err(DocumentError::new("Unable to verify bounded save staging"));
            }
            Ok((staging, identity))
        }
        Err(_) if unix_entry_identity(directory.as_raw_fd(), entry).is_none() => {
            link_unnamed_temp(temp.as_raw_fd(), directory.as_raw_fd(), entry)?;
            if unsafe { libc::flock(temp.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) } != 0 {
                return Err(DocumentError::new(
                    "Unable to lock the bounded save staging entry",
                ));
            }
            let handle = temp.try_clone().map_err(|source| {
                DocumentError::new(format!("Unable to retain save staging: {source}"))
            })?;
            Ok((handle, temp_identity))
        }
        Err(error) => Err(DocumentError::new(format!(
            "The bounded save staging entry is unavailable: {error}"
        ))),
    }
}

#[cfg(target_os = "macos")]
fn prepare_macos_exchange_staging(
    directory: &fs::File,
    name: &str,
    source: &fs::File,
    content: &[u8],
) -> Result<(fs::File, (u64, u64)), DocumentError> {
    use std::os::fd::AsRawFd;
    let entry = std::ffi::OsStr::new(name);
    match openat_file(
        directory.as_raw_fd(),
        entry,
        libc::O_RDWR | libc::O_NONBLOCK,
        0,
    ) {
        Ok(mut staging) => {
            let metadata = staging.metadata().map_err(|source| {
                DocumentError::new(format!("Unable to classify save staging: {source}"))
            })?;
            if !metadata.is_file()
                || unsafe { libc::flock(staging.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) } != 0
            {
                return Err(DocumentError::new(
                    "The bounded save staging entry is unavailable",
                ));
            }
            let identity = unix_file_identity(&staging)?;
            copy_unix_metadata(source, &staging)?;
            staging
                .set_len(0)
                .and_then(|_| staging.seek(SeekFrom::Start(0)))
                .and_then(|_| staging.write_all(content))
                .and_then(|_| staging.sync_all())
                .map_err(|source| {
                    DocumentError::new(format!("Unable to prepare bounded save staging: {source}"))
                })?;
            if read_file_bytes(&mut staging)? != content
                || !unix_entry_matches_identity(directory.as_raw_fd(), entry, identity)
            {
                return Err(DocumentError::new("Unable to verify bounded save staging"));
            }
            Ok((staging, identity))
        }
        Err(_) if unix_entry_identity(directory.as_raw_fd(), entry).is_none() => {
            let install_name = recovery_sibling_name(name, "staging-install")
                .ok_or_else(|| DocumentError::new("The staging name is malformed"))?;
            let mut staging = openat_file(
                directory.as_raw_fd(),
                std::ffi::OsStr::new(&install_name),
                libc::O_RDWR | libc::O_CREAT | libc::O_EXCL,
                0o600,
            )?;
            copy_unix_metadata(source, &staging)?;
            staging
                .write_all(content)
                .and_then(|_| staging.sync_all())
                .map_err(|source| {
                    DocumentError::new(format!("Unable to prepare bounded save staging: {source}"))
                })?;
            let identity = unix_file_identity(&staging)?;
            let mut guard = MacInstallGuard {
                directory_fd: directory.as_raw_fd(),
                name: install_name.clone(),
                identity,
                active: true,
            };
            rename_noreplace_between(
                directory.as_raw_fd(),
                std::ffi::OsStr::new(&install_name),
                directory.as_raw_fd(),
                entry,
            )?;
            guard.active = false;
            if unsafe { libc::flock(staging.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) } != 0 {
                return Err(DocumentError::new("Unable to lock bounded save staging"));
            }
            Ok((staging, identity))
        }
        Err(error) => Err(DocumentError::new(format!(
            "The bounded save staging entry is unavailable: {error}"
        ))),
    }
}

#[cfg(target_os = "macos")]
fn rename_noreplace_between(
    source_directory_fd: std::os::fd::RawFd,
    source: &std::ffi::OsStr,
    destination_directory_fd: std::os::fd::RawFd,
    destination: &std::ffi::OsStr,
) -> Result<(), DocumentError> {
    use std::os::unix::ffi::OsStrExt;
    let source = CString::new(source.as_bytes())
        .map_err(|_| DocumentError::new("The recovery source name is malformed"))?;
    let destination = CString::new(destination.as_bytes())
        .map_err(|_| DocumentError::new("The recovery slot name is malformed"))?;
    if unsafe {
        libc::renameatx_np(
            source_directory_fd,
            source.as_ptr(),
            destination_directory_fd,
            destination.as_ptr(),
            libc::RENAME_EXCL,
        )
    } == 0
    {
        Ok(())
    } else {
        Err(DocumentError::new(format!(
            "Unable to install recovery artifact: {}",
            std::io::Error::last_os_error()
        )))
    }
}

#[cfg(target_os = "linux")]
fn link_unnamed_temp(
    temp_fd: std::os::fd::RawFd,
    directory_fd: std::os::fd::RawFd,
    target: &std::ffi::OsStr,
) -> Result<(), DocumentError> {
    use std::os::unix::ffi::OsStrExt;

    let empty = CString::new("").map_err(|_| DocumentError::new("The save handle is malformed"))?;
    let target = CString::new(target.as_bytes())
        .map_err(|_| DocumentError::new("The document path is malformed"))?;
    if unsafe {
        libc::linkat(
            temp_fd,
            empty.as_ptr(),
            directory_fd,
            target.as_ptr(),
            libc::AT_EMPTY_PATH,
        )
    } == 0
    {
        Ok(())
    } else {
        Err(DocumentError::new(format!(
            "Unable to install the verified save entry: {}",
            std::io::Error::last_os_error()
        )))
    }
}

#[cfg(target_os = "linux")]
fn copy_unix_metadata(source: &fs::File, target: &fs::File) -> Result<(), DocumentError> {
    use std::{os::fd::AsRawFd, os::unix::fs::MetadataExt};
    use xattr::FileExt;

    let metadata = source.metadata().map_err(|error| {
        DocumentError::new(format!("Unable to read document metadata: {error}"))
    })?;
    if unsafe { libc::fchown(target.as_raw_fd(), metadata.uid(), metadata.gid()) } != 0 {
        let error = std::io::Error::last_os_error();
        return Err(DocumentError::new(format!(
            "Unable to preserve document ownership: {error}"
        )));
    }
    if unsafe { libc::fchmod(target.as_raw_fd(), metadata.mode()) } != 0 {
        return Err(DocumentError::new(format!(
            "Unable to preserve document mode: {}",
            std::io::Error::last_os_error()
        )));
    }
    let attributes: Vec<_> = source
        .list_xattr()
        .map_err(|error| {
            DocumentError::new(format!("Unable to read document attributes: {error}"))
        })?
        .collect();
    let target_attributes: Vec<_> = target
        .list_xattr()
        .map_err(|error| {
            DocumentError::new(format!("Unable to read prepared attributes: {error}"))
        })?
        .collect();
    for name in target_attributes {
        if !attributes.contains(&name) {
            target.remove_xattr(&name).map_err(|error| {
                DocumentError::new(format!(
                    "Unable to remove stale document attribute: {error}"
                ))
            })?;
        }
    }
    for name in attributes {
        if let Some(value) = source.get_xattr(&name).map_err(|error| {
            DocumentError::new(format!("Unable to read document attribute: {error}"))
        })? {
            target.set_xattr(&name, &value).map_err(|error| {
                DocumentError::new(format!("Unable to preserve document attribute: {error}"))
            })?;
        }
    }
    Ok(())
}

#[cfg(unix)]
fn unix_entry_matches_identity(
    directory_fd: std::os::fd::RawFd,
    name: &std::ffi::OsStr,
    identity: (u64, u64),
) -> bool {
    unix_entry_identity(directory_fd, name) == Some(identity)
}

#[cfg(unix)]
fn unix_entry_identity(
    directory_fd: std::os::fd::RawFd,
    name: &std::ffi::OsStr,
) -> Option<(u64, u64)> {
    use std::{mem::MaybeUninit, os::unix::ffi::OsStrExt};

    let name = CString::new(name.as_bytes()).ok()?;
    let mut metadata = MaybeUninit::<libc::stat>::zeroed();
    if unsafe {
        libc::fstatat(
            directory_fd,
            name.as_ptr(),
            metadata.as_mut_ptr(),
            libc::AT_SYMLINK_NOFOLLOW,
        )
    } != 0
    {
        return None;
    }
    let metadata = unsafe { metadata.assume_init() };
    Some((metadata.st_dev, metadata.st_ino))
}

#[cfg(unix)]
fn open_directory_no_follow(path: &Path) -> Result<fs::File, DocumentError> {
    use std::{os::fd::FromRawFd, path::Component};

    let root =
        CString::new("/").map_err(|_| DocumentError::new("The filesystem root is malformed"))?;
    let root_fd = unsafe {
        libc::open(
            root.as_ptr(),
            libc::O_RDONLY | libc::O_DIRECTORY | libc::O_CLOEXEC,
        )
    };
    if root_fd < 0 {
        return Err(DocumentError::new(format!(
            "Unable to open the filesystem root: {}",
            std::io::Error::last_os_error()
        )));
    }
    let mut directory = unsafe { fs::File::from_raw_fd(root_fd) };
    for component in path.components() {
        let Component::Normal(name) = component else {
            continue;
        };
        directory = openat_file(
            std::os::fd::AsRawFd::as_raw_fd(&directory),
            name,
            libc::O_RDONLY | libc::O_DIRECTORY,
            0,
        )?;
    }
    Ok(directory)
}

#[cfg(unix)]
fn openat_file(
    directory_fd: std::os::fd::RawFd,
    name: &std::ffi::OsStr,
    flags: libc::c_int,
    mode: libc::mode_t,
) -> Result<fs::File, DocumentError> {
    use std::{os::fd::FromRawFd, os::unix::ffi::OsStrExt};

    let name = CString::new(name.as_bytes())
        .map_err(|_| DocumentError::new("The document path is malformed"))?;
    let fd = unsafe {
        libc::openat(
            directory_fd,
            name.as_ptr(),
            flags | libc::O_CLOEXEC | libc::O_NOFOLLOW,
            mode,
        )
    };
    if fd < 0 {
        Err(DocumentError::new(format!(
            "Unable to open a verified workspace entry: {}",
            std::io::Error::last_os_error()
        )))
    } else {
        Ok(unsafe { fs::File::from_raw_fd(fd) })
    }
}

fn content_token(content: &str) -> String {
    blake3::hash(content.as_bytes()).to_hex().to_string()
}

fn next_revision(revision: u64) -> Result<u64, DocumentError> {
    revision
        .checked_add(1)
        .ok_or_else(|| DocumentError::new("The workspace revision is unavailable"))
}

fn resolve_document_path(root: &Path, document_path: &str) -> Result<PathBuf, DocumentError> {
    if document_path.is_empty() || has_forbidden_backslash(document_path, cfg!(windows)) {
        return Err(DocumentError::new(format!(
            "Invalid document path: {document_path}"
        )));
    }

    let relative_path = Path::new(document_path);
    let normalized = normalize_relative_path(relative_path)
        .map_err(|_| DocumentError::new(format!("Invalid document path: {document_path}")))?;

    if normalized != document_path {
        return Err(DocumentError::new(format!(
            "Invalid document path: {document_path}"
        )));
    }

    if !relative_path
        .extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| extension.eq_ignore_ascii_case("md"))
    {
        return Err(DocumentError::new(format!(
            "The document is not Markdown: {document_path}"
        )));
    }

    let path = fs::canonicalize(root.join(relative_path)).map_err(|source| {
        DocumentError::new(format!(
            "Unable to access document '{document_path}': {source}"
        ))
    })?;

    if !path.starts_with(root) {
        return Err(DocumentError::new(format!(
            "The document is outside the workspace: {document_path}"
        )));
    }

    if !path.is_file() {
        return Err(DocumentError::new(format!(
            "The document is not a file: {document_path}"
        )));
    }

    Ok(path)
}

fn has_forbidden_backslash(document_path: &str, is_windows: bool) -> bool {
    is_windows && document_path.contains('\\')
}

fn preserve_line_endings(existing_content: &str, content: &str) -> String {
    let normalized = content.replace("\r\n", "\n").replace('\r', "\n");

    if existing_content.contains("\r\n") {
        normalized.replace('\n', "\r\n")
    } else if existing_content.contains('\r') {
        normalized.replace('\n', "\r")
    } else {
        normalized
    }
}

#[cfg(test)]
mod tests {
    use super::{
        content_token, decode_recovery_artifact, encode_recovery_artifact, has_forbidden_backslash,
        open_workspace, read_document, recovery_reference_target, recovery_slot_name,
        run_recovered_mutation, write_document, write_document_if_current_with_hook,
        RecoveryReferenceTarget, WorkspaceSession, RECOVERY_DIRECTORY, RECOVERY_OWNER_MAGIC,
        RECOVERY_OWNER_SENTINEL,
    };
    #[cfg(target_os = "linux")]
    use super::{transactional_replace_with_phase_hook, TransactionPhase};
    use crate::{
        models::workspace::WorkspacePatch,
        services::watcher::{ChangeKind, WorkspaceChange},
    };
    use std::{
        fs,
        path::{Path, PathBuf},
        process::Command,
        sync::{
            atomic::{AtomicU64, Ordering},
            Arc, Barrier,
        },
        thread,
    };

    static NEXT_FIXTURE_ID: AtomicU64 = AtomicU64::new(0);

    #[test]
    fn recovery_reference_state_preserves_substitutions_and_reuses_absent_slots() {
        assert_eq!(
            recovery_reference_target(true, false),
            RecoveryReferenceTarget::Current
        );
        assert_eq!(
            recovery_reference_target(false, true),
            RecoveryReferenceTarget::Deterministic
        );
        assert_eq!(
            recovery_reference_target(false, false),
            RecoveryReferenceTarget::Conflict
        );
    }

    #[test]
    fn swap_state_requires_the_exact_captured_baseline_identity_and_token() {
        let identity = (7, 11);
        let token = content_token("# Baseline");
        assert!(super::captured_baseline_matches(
            identity,
            &token,
            Some(identity),
            Some(b"# Baseline")
        ));
        assert!(!super::captured_baseline_matches(
            identity,
            &token,
            Some((7, 12)),
            Some(b"# Baseline")
        ));
        assert!(!super::captured_baseline_matches(
            identity,
            &token,
            Some(identity),
            Some(b"# External")
        ));
    }
    #[test]
    fn recovered_mutation_never_writes_without_recovery_and_reports_all_failures() {
        let mut value = 0u8;
        let missing = run_recovered_mutation(
            &mut value,
            || false,
            |value| {
                *value = 1;
                Ok(())
            },
            |_| Ok(true),
        );
        assert!(missing.is_err());
        assert_eq!(value, 0);

        let write_failure = run_recovered_mutation(
            &mut value,
            || true,
            |_| Err(std::io::Error::other("injected write failure")),
            |_| Ok(true),
        );
        assert!(write_failure.is_err());
        assert_eq!(value, 0);

        let verification_failure = run_recovered_mutation(
            &mut value,
            || true,
            |value| {
                *value = 2;
                Ok(())
            },
            |_| Ok(false),
        );
        assert!(verification_failure.is_err());
        assert_eq!(value, 2);
    }

    fn empty_patch(generation: u64, revision: u64) -> WorkspacePatch {
        WorkspacePatch {
            workspace_generation: generation,
            workspace_revision: revision,
            upserted_folders: Vec::new(),
            removed_folder_ids: Vec::new(),
            upserted_documents: Vec::new(),
            removed_document_ids: Vec::new(),
            upserted_links: Vec::new(),
            removed_link_ids: Vec::new(),
            externally_changed_document_ids: Vec::new(),
        }
    }

    struct TestDirectory {
        path: PathBuf,
    }

    impl TestDirectory {
        fn new(label: &str) -> Self {
            let fixture_id = NEXT_FIXTURE_ID.fetch_add(1, Ordering::Relaxed);
            let path = std::env::temp_dir().join(format!(
                "tracedoc-document-test-{label}-{}-{fixture_id}",
                std::process::id()
            ));
            fs::create_dir_all(&path).expect("test directory should be created");
            let path = fs::canonicalize(path).expect("test directory should canonicalize");
            Self { path }
        }

        fn path(&self) -> &Path {
            &self.path
        }
    }

    impl Drop for TestDirectory {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.path);
        }
    }

    #[cfg(target_os = "linux")]
    fn create_owned_recovery_directory(workspace: &TestDirectory) -> PathBuf {
        let root = super::open_directory_no_follow(workspace.path()).expect("root should open");
        super::open_recovery_directory(&root).expect("owned recovery should open");
        workspace.path().join(RECOVERY_DIRECTORY)
    }

    #[test]
    fn reads_and_writes_unicode_markdown() {
        let workspace = TestDirectory::new("round-trip");
        fs::create_dir_all(workspace.path().join("guides"))
            .expect("nested folder should be created");
        fs::write(workspace.path().join("guides/start.md"), "# Start")
            .expect("document should be written");

        write_document(workspace.path(), "guides/start.md", "# Привет\n\nZażółć")
            .expect("document should save");

        assert_eq!(
            read_document(workspace.path(), "guides/start.md")
                .expect("document should be readable"),
            "# Привет\n\nZażółć"
        );
    }

    #[test]
    fn rejects_invalid_and_non_markdown_paths() {
        let workspace = TestDirectory::new("invalid-paths");
        fs::write(workspace.path().join("page.md"), "# Page").expect("document should be written");
        fs::write(workspace.path().join("notes.txt"), "Notes")
            .expect("text file should be written");

        for invalid_path in ["../page.md", "/page.md"] {
            let error = read_document(workspace.path(), invalid_path)
                .expect_err("invalid document path should fail");
            assert!(error.to_string().contains("Invalid document path"));
        }

        let error = read_document(workspace.path(), "notes.txt")
            .expect_err("non-Markdown document should fail");
        assert!(error.to_string().contains("not Markdown"));
    }

    #[test]
    fn treats_backslash_as_a_separator_only_on_windows() {
        assert!(has_forbidden_backslash("folder\\page.md", true));
        assert!(!has_forbidden_backslash("folder\\page.md", false));
    }

    #[test]
    fn preserves_crlf_line_endings_when_saving() {
        let workspace = TestDirectory::new("crlf");
        let path = workspace.path().join("windows.md");
        fs::write(&path, "# Heading\r\n\r\nOriginal\r\n").expect("CRLF document should be written");

        write_document(
            workspace.path(),
            "windows.md",
            "# Heading\n\nChanged Zażółć\n",
        )
        .expect("CRLF document should save");

        assert_eq!(
            fs::read_to_string(path).expect("CRLF document should be readable"),
            "# Heading\r\n\r\nChanged Zażółć\r\n"
        );
    }

    #[test]
    fn preserves_lf_line_endings_when_saving() {
        let workspace = TestDirectory::new("lf");
        let path = workspace.path().join("unix.md");
        fs::write(&path, "# Heading\n\nOriginal\n").expect("LF document should be written");

        write_document(workspace.path(), "unix.md", "# Heading\n\nChanged\n")
            .expect("LF document should save");

        assert_eq!(
            fs::read_to_string(path).expect("LF document should be readable"),
            "# Heading\n\nChanged\n"
        );
    }

    #[test]
    fn saves_and_reindexes_only_the_active_document() {
        let workspace = TestDirectory::new("save-and-index");
        fs::write(workspace.path().join("page.md"), "# Original").expect("page should be written");
        fs::write(workspace.path().join("target.md"), "# Target")
            .expect("target should be written");
        let (project, root) = open_workspace(workspace.path()).expect("workspace should open");
        let session = WorkspaceSession::default();
        let generation = session
            .activate(root, project)
            .expect("workspace session should activate");
        let lease = session
            .capture(generation)
            .expect("workspace lease should be captured");
        let read = session
            .read_document(&lease, "page.md")
            .expect("document should read through its workspace lease");

        let update = session
            .save_document(
                &lease,
                "page.md",
                "# Changed\n\n[[target]]",
                &content_token("# Original"),
                1,
            )
            .expect("document should save and reindex");

        assert_eq!(read.workspace_generation, generation);
        assert_eq!(read.content, "# Original");
        assert_eq!(update.workspace_generation, generation);
        assert_eq!(update.document.title.as_deref(), Some("Changed"));
        assert_eq!(update.links.len(), 1);
        assert_eq!(
            update.links[0].target_document_id.as_deref(),
            Some("document:target.md")
        );
        assert_eq!(
            fs::read_to_string(workspace.path().join("page.md"))
                .expect("saved document should be readable"),
            "# Changed\n\n[[target]]"
        );
    }

    #[test]
    fn open_workspace_pairs_a_canonical_root_with_the_scanned_project() {
        let workspace = TestDirectory::new("open-workspace-pairing");
        fs::write(workspace.path().join("page.md"), "# Page").expect("page should be written");
        let missing = workspace.path().join("missing-child");

        let (project, root) = open_workspace(workspace.path()).expect("workspace should open");

        assert_eq!(
            root,
            fs::canonicalize(workspace.path()).expect("workspace root should canonicalize")
        );
        assert!(project.documents.contains_key("document:page.md"));
        assert!(open_workspace(&missing).is_err());
    }

    #[test]
    fn activating_a_workspace_returns_its_snapshot_without_rescanning_disk() {
        let workspace = TestDirectory::new("activate-snapshot");
        fs::write(workspace.path().join("page.md"), "# Page").expect("page should be written");
        let (project, root) = open_workspace(workspace.path()).expect("workspace should open");
        let session = WorkspaceSession::default();
        let generation = session
            .activate(root, project)
            .expect("workspace session should activate");

        fs::write(workspace.path().join("added.md"), "# Added")
            .expect("an out-of-band document should be written after activation");

        let snapshot = session
            .snapshot(generation)
            .expect("the active snapshot should be readable");

        assert_eq!(snapshot.workspace_generation, generation);
        assert_eq!(snapshot.workspace_revision, 1);
        assert!(snapshot.project.documents.contains_key("document:page.md"));
        assert!(!snapshot.project.documents.contains_key("document:added.md"));
        assert!(session.snapshot(generation + 1).is_err());
    }

    #[test]
    fn unicode_link_targets_do_not_panic_or_poison_the_workspace_session() {
        let workspace = TestDirectory::new("unicode-link-session");
        fs::write(workspace.path().join("page.md"), "# Original").expect("page should be written");
        fs::write(workspace.path().join("éé.md"), "# Unicode")
            .expect("unicode target should be written");
        let (project, root) = open_workspace(workspace.path()).expect("workspace should open");
        let session = WorkspaceSession::default();
        let generation = session
            .activate(root, project)
            .expect("session should activate");
        let lease = session.capture(generation).expect("lease should capture");

        session
            .save_document(
                &lease,
                "page.md",
                "[x](éé)\n[[éé]]\n[x](éé.MD)",
                &content_token("# Original"),
                1,
            )
            .expect("multibyte targets should not panic");
        let read = session
            .read_document(&lease, "page.md")
            .expect("session lock should remain usable");
        assert_eq!(read.content, "[x](éé)\n[[éé]]\n[x](éé.MD)");
    }

    #[test]
    fn suppresses_the_watcher_echo_of_a_successful_internal_save() {
        let workspace = TestDirectory::new("self-save-watch");
        fs::write(workspace.path().join("page.md"), "# Original").expect("page should be written");
        let (project, root) = open_workspace(workspace.path()).expect("workspace should open");
        let session = WorkspaceSession::default();
        let generation = session
            .activate(root, project)
            .expect("workspace should activate");
        let lease = session.capture(generation).expect("lease should capture");
        session
            .save_document(
                &lease,
                "page.md",
                "# Saved",
                &content_token("# Original"),
                1,
            )
            .expect("page should save");

        let patch = session
            .apply_external_changes(
                generation,
                &[WorkspaceChange {
                    path: workspace.path().join("page.md"),
                    kind: ChangeKind::Modify,
                }],
            )
            .expect("watcher echo should process");

        assert!(patch.is_none());
    }

    #[test]
    fn rejects_a_dirty_save_when_an_external_write_is_pending_debounce() {
        let workspace = TestDirectory::new("pending-external-write");
        fs::write(workspace.path().join("page.md"), "# Baseline").expect("page should be written");
        let (project, root) = open_workspace(workspace.path()).expect("workspace should open");
        let session = WorkspaceSession::default();
        let generation = session
            .activate(root, project)
            .expect("workspace should activate");
        let lease = session.capture(generation).expect("lease should capture");
        let baseline = session
            .read_document(&lease, "page.md")
            .expect("baseline should read");
        fs::write(workspace.path().join("page.md"), "# External")
            .expect("external edit should be written");

        let error = session
            .save_document(&lease, "page.md", "# Local", &baseline.content_token, 1)
            .expect_err("stale dirty save should fail");

        assert!(error.to_string().contains("changed externally"));
        assert_eq!(
            fs::read_to_string(workspace.path().join("page.md"))
                .expect("external edit should remain"),
            "# External"
        );
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn preserves_an_external_replacement_between_validation_and_commit() {
        let workspace = TestDirectory::new("transaction-race");
        let path = workspace.path().join("page.md");
        fs::write(&path, "# Baseline").expect("baseline should be written");

        let error = write_document_if_current_with_hook(
            workspace.path(),
            "page.md",
            "# Local",
            &content_token("# Baseline"),
            |_| {
                let replacement = workspace.path().join("replacement.md");
                fs::write(&replacement, "# External").expect("replacement should be written");
                fs::rename(replacement, &path).expect("replacement should become current");
            },
        )
        .expect_err("the raced save should conflict");

        assert!(error.to_string().contains("changed externally"));
        assert_eq!(
            fs::read_to_string(path).expect("external replacement should remain"),
            "# External"
        );
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn preserves_mode_and_supported_extended_attributes() {
        use std::os::unix::fs::{MetadataExt, PermissionsExt};

        let workspace = TestDirectory::new("transaction-metadata");
        let path = workspace.path().join("page.md");
        fs::write(&path, "# Baseline").expect("baseline should be written");
        fs::set_permissions(&path, fs::Permissions::from_mode(0o640)).expect("mode should be set");
        let xattr_supported = xattr::set(&path, "user.tracedoc-test", b"preserved").is_ok();

        write_document_if_current_with_hook(
            workspace.path(),
            "page.md",
            "# Saved",
            &content_token("# Baseline"),
            |_| {},
        )
        .expect("document should save");

        assert_eq!(
            fs::metadata(&path).expect("metadata should read").mode() & 0o777,
            0o640
        );
        if xattr_supported {
            assert_eq!(
                xattr::get(&path, "user.tracedoc-test").expect("attribute should read"),
                Some(b"preserved".to_vec())
            );
        }
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn installs_the_held_temp_when_a_named_source_is_substituted() {
        let workspace = TestDirectory::new("transaction-temp-substitution");
        let path = workspace.path().join("page.md");
        fs::write(&path, "# Baseline").expect("baseline should be written");
        let substitute = workspace.path().join("guessed-save-source.tmp");

        transactional_replace_with_phase_hook(
            workspace.path(),
            &path,
            b"# Local",
            &content_token("# Baseline"),
            |phase, _| {
                if phase == TransactionPhase::Prepared {
                    fs::write(&substitute, "# Substitute").expect("substitute should be written");
                }
            },
        )
        .expect("held unnamed temp should save");

        assert_eq!(
            fs::read_to_string(path).expect("target should remain"),
            "# Local"
        );
        assert_eq!(
            fs::read_to_string(substitute).expect("substitute should remain"),
            "# Substitute"
        );
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn restores_external_writes_from_the_final_preexchange_window() {
        for replacement in [false, true] {
            let workspace = TestDirectory::new(if replacement {
                "preexchange-replace"
            } else {
                "preexchange-in-place"
            });
            let path = workspace.path().join("page.md");
            let external = if replacement {
                format!("# External\n{}", "x".repeat(4 * 1024 * 1024))
            } else {
                "# External in place".to_owned()
            };
            fs::write(&path, "# Baseline").expect("baseline should be written");
            let error = transactional_replace_with_phase_hook(
                workspace.path(),
                &path,
                b"# Local",
                &content_token("# Baseline"),
                |phase, _| {
                    if phase == TransactionPhase::PreExchange {
                        if replacement {
                            let incoming = workspace.path().join("incoming.md");
                            fs::write(&incoming, &external).expect("incoming should be written");
                            fs::rename(&incoming, &path).expect("target should be replaced");
                        } else {
                            fs::write(&path, &external).expect("target should change in place");
                        }
                    }
                },
            )
            .expect_err("final-window external write should conflict");
            assert!(error.to_string().contains("external version was restored"));
            assert_eq!(
                fs::read_to_string(&path).expect("external target should remain live"),
                external
            );
            super::acknowledge_transaction_quarantine(workspace.path(), "page.md")
                .expect("the user should acknowledge the quarantined save state");
            write_document_if_current_with_hook(
                workspace.path(),
                "page.md",
                "# Follow-up",
                &content_token(&external),
                |_| {},
            )
            .expect("next save should serialize from the restored external baseline");
            assert_eq!(
                fs::read_to_string(path).expect("follow-up should read"),
                "# Follow-up"
            );
        }
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn quarantines_preexchange_and_installed_external_versions() {
        let workspace = TestDirectory::new("transaction-preexchange-installed");
        let path = workspace.path().join("page.md");
        fs::write(&path, "# Baseline").expect("baseline should be written");
        let error = transactional_replace_with_phase_hook(
            workspace.path(),
            &path,
            b"# Local",
            &content_token("# Baseline"),
            |phase, live| match phase {
                TransactionPhase::PreExchange => {
                    fs::write(live, "# External B").expect("external B should be written");
                }
                TransactionPhase::Installed => {
                    fs::write(live, "# External C").expect("external C should be written");
                }
                _ => {}
            },
        )
        .expect_err("rapid external edits must conflict");
        assert!(error.to_string().contains("quarantined"));
        assert_eq!(
            fs::read_to_string(&path).expect("live C should read"),
            "# External C"
        );
        let recovery = workspace.path().join(RECOVERY_DIRECTORY);
        let external = recovery.join(super::recovery_auxiliary_name("page.md", "external"));
        assert_eq!(
            fs::read_to_string(&external).expect("external B should remain"),
            "# External B"
        );
        let baseline = fs::read(recovery.join(super::recovery_slot_name("page.md")))
            .expect("baseline recovery should read");
        assert_eq!(
            decode_recovery_artifact(&baseline)
                .expect("baseline recovery should decode")
                .2,
            b"# Baseline"
        );
        let blocked = write_document_if_current_with_hook(
            workspace.path(),
            "page.md",
            "# Follow-up",
            &content_token("# External C"),
            |_| {},
        )
        .expect_err("ordinary save must not reuse quarantine");
        assert!(blocked.to_string().contains("quarantined"));
        assert_eq!(
            fs::read_to_string(&external).expect("external B must remain"),
            "# External B"
        );
        super::acknowledge_transaction_quarantine(workspace.path(), "page.md")
            .expect("explicit resolution should archive B");
        write_document_if_current_with_hook(
            workspace.path(),
            "page.md",
            "# Follow-up",
            &content_token("# External C"),
            |_| {},
        )
        .expect("ordered save should work after resolution");
        assert_eq!(
            fs::read_to_string(path).expect("follow-up should read"),
            "# Follow-up"
        );
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn quarantines_external_change_before_restore_exchange() {
        let workspace = TestDirectory::new("transaction-prerestore-external");
        let path = workspace.path().join("page.md");
        fs::write(&path, "# Baseline").expect("baseline should be written");
        let error = transactional_replace_with_phase_hook(
            workspace.path(),
            &path,
            b"# Local",
            &content_token("# Baseline"),
            |phase, live| match phase {
                TransactionPhase::PreExchange => {
                    fs::write(live, "# External B").expect("external B should be written");
                }
                TransactionPhase::PreRestore => {
                    fs::write(live, "# External C").expect("external C should be written");
                }
                _ => {}
            },
        )
        .expect_err("external edit before restore must conflict");
        assert!(error.to_string().contains("quarantined"));
        assert_eq!(
            fs::read_to_string(&path).expect("live C should read"),
            "# External C"
        );
        let recovery = workspace.path().join(RECOVERY_DIRECTORY);
        let external = recovery.join(super::recovery_auxiliary_name("page.md", "external"));
        assert_eq!(
            fs::read_to_string(&external).expect("external B should remain"),
            "# External B"
        );
        let blocked = write_document_if_current_with_hook(
            workspace.path(),
            "page.md",
            "# Follow-up",
            &content_token("# External C"),
            |_| {},
        )
        .expect_err("ordinary save must remain blocked");
        assert!(blocked.to_string().contains("quarantined"));
        super::acknowledge_transaction_quarantine(workspace.path(), "page.md")
            .expect("explicit resolution should archive B");
        write_document_if_current_with_hook(
            workspace.path(),
            "page.md",
            "# Follow-up",
            &content_token("# External C"),
            |_| {},
        )
        .expect("ordered save should work after resolution");
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn occupied_external_slot_blocks_bounded_transaction_state() {
        let workspace = TestDirectory::new("transaction-external-occupied");
        let path = workspace.path().join("page.md");
        fs::write(&path, "# Baseline").expect("baseline should be written");
        let recovery = workspace.path().join(RECOVERY_DIRECTORY);
        let external = recovery.join(super::recovery_auxiliary_name("page.md", "external"));
        let error = transactional_replace_with_phase_hook(
            workspace.path(),
            &path,
            b"# Local",
            &content_token("# Baseline"),
            |phase, live| {
                if phase == TransactionPhase::Installed {
                    fs::write(&external, "# Occupied A").expect("external slot should be occupied");
                    fs::write(live, "# External C").expect("external C should be written");
                }
            },
        )
        .expect_err("occupied external slot must fail closed");
        assert!(error.to_string().contains("blocks reuse"));
        let staging = recovery.join(super::recovery_auxiliary_name("page.md", "staging"));
        let blocked = recovery.join(super::recovery_auxiliary_name("page.md", "blocked"));
        assert_eq!(
            fs::read_to_string(&external).expect("A should remain"),
            "# Occupied A"
        );
        assert_eq!(
            fs::read_to_string(&staging).expect("B should remain"),
            "# Baseline"
        );
        assert!(blocked.is_file());
        assert_eq!(
            fs::read_to_string(&path).expect("C should remain"),
            "# External C"
        );
        let retry = write_document_if_current_with_hook(
            workspace.path(),
            "page.md",
            "# Follow-up",
            &content_token("# External C"),
            |_| {},
        )
        .expect_err("blocked state must prevent reuse");
        assert!(retry.to_string().contains("quarantined"));
        super::acknowledge_transaction_quarantine(workspace.path(), "page.md")
            .expect("explicit resolution should clear bounded state");
        assert!(!external.exists());
        assert!(!staging.exists());
        assert!(!blocked.exists());
        write_document_if_current_with_hook(
            workspace.path(),
            "page.md",
            "# Follow-up",
            &content_token("# External C"),
            |_| {},
        )
        .expect("save should work after explicit resolution");
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn serializes_simultaneous_transactions_with_one_bounded_lock() {
        use std::sync::mpsc;
        let workspace = TestDirectory::new("transaction-lock");
        let root = workspace.path().to_path_buf();
        let path = root.join("page.md");
        fs::write(&path, "# Baseline").expect("baseline should be written");
        let (ready_sender, ready_receiver) = mpsc::channel();
        let (release_sender, release_receiver) = mpsc::channel();
        let first_root = root.clone();
        let first_path = path.clone();
        let first = thread::spawn(move || {
            transactional_replace_with_phase_hook(
                &first_root,
                &first_path,
                b"# First",
                &content_token("# Baseline"),
                |phase, _| {
                    if phase == TransactionPhase::Prepared {
                        ready_sender.send(()).expect("ready should send");
                        release_receiver.recv().expect("release should arrive");
                    }
                },
            )
        });
        ready_receiver.recv().expect("first save should hold lock");
        let second = transactional_replace_with_phase_hook(
            &root,
            &path,
            b"# Second",
            &content_token("# Baseline"),
            |_, _| {},
        )
        .expect_err("second transaction should report busy");
        assert!(second.to_string().contains("busy"));
        release_sender.send(()).expect("first save should release");
        first
            .join()
            .expect("first thread should join")
            .expect("first save should succeed");
        assert_eq!(
            fs::read_to_string(&path).expect("target should read"),
            "# First"
        );
        write_document_if_current_with_hook(
            &root,
            "page.md",
            "# Second",
            &content_token("# First"),
            |_| {},
        )
        .expect("ordered retry should succeed");
        assert_eq!(
            fs::read_to_string(path).expect("target should read"),
            "# Second"
        );
        assert!(!root.join(RECOVERY_DIRECTORY).exists());
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn interrupted_staging_content_never_reaches_the_live_path() {
        let workspace = TestDirectory::new("transaction-staging-interrupt");
        let path = workspace.path().join("page.md");
        fs::write(&path, "# Baseline").expect("baseline should be written");
        write_document_if_current_with_hook(
            workspace.path(),
            "page.md",
            "# Current",
            &content_token("# Baseline"),
            |_| {},
        )
        .expect("first save should succeed");
        let error = transactional_replace_with_phase_hook(
            workspace.path(),
            &path,
            b"# Intended",
            &content_token("# Current"),
            |phase, staging_path| {
                if phase == TransactionPhase::Displaced {
                    fs::write(staging_path, "partial").expect("staging should be interrupted");
                }
            },
        )
        .expect_err("interrupted staging must fail before exchange");
        assert!(error.to_string().contains("changed externally"));
        assert_eq!(
            fs::read_to_string(&path).expect("live path should read"),
            "# Current"
        );
        write_document_if_current_with_hook(
            workspace.path(),
            "page.md",
            "# Retry",
            &content_token("# Current"),
            |_| {},
        )
        .expect("bounded staging should recover on retry");
        assert_eq!(
            fs::read_to_string(path).expect("retry should read"),
            "# Retry"
        );
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn preserves_concurrent_content_when_recovery_cannot_be_restored() {
        let workspace = TestDirectory::new("transaction-restore-race");
        let target = workspace.path().join("page.md");
        let mut recovery = None;
        fs::write(&target, "# Baseline").expect("baseline should be written");

        let error = transactional_replace_with_phase_hook(
            workspace.path(),
            &target,
            b"# Local A",
            &content_token("# Baseline"),
            |phase, recovery_path| match phase {
                TransactionPhase::Displaced => {
                    fs::write(recovery_path, "# External B")
                        .expect("displaced entry should be edited");
                    recovery = Some(recovery_path.to_path_buf());
                    fs::write(&target, "# External C").expect("external C should become current");
                }
                TransactionPhase::Prepared
                | TransactionPhase::PreExchange
                | TransactionPhase::Installed
                | TransactionPhase::PreRestore
                | TransactionPhase::Cleanup => {}
            },
        )
        .expect_err("failed safe restore should report conflict");

        assert!(error.to_string().contains("changed externally"));
        assert_eq!(
            fs::read_to_string(target).expect("external C should remain"),
            "# External C"
        );
        assert_eq!(
            fs::read_to_string(recovery.expect("recovery should be recorded"))
                .expect("external B should remain recoverable"),
            "# External B"
        );
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn remains_panic_free_when_the_displaced_entry_is_moved() {
        let workspace = TestDirectory::new("transaction-moved-recovery");
        let target = workspace.path().join("page.md");
        let moved = workspace.path().join("externally-moved-baseline.md");
        fs::write(&target, "# Baseline").expect("baseline should be written");

        let error = transactional_replace_with_phase_hook(
            workspace.path(),
            &target,
            b"# Local",
            &content_token("# Baseline"),
            |phase, recovery_path| {
                if phase == TransactionPhase::Displaced {
                    fs::rename(recovery_path, &moved).expect("recovery should be moved");
                }
            },
        )
        .expect_err("moved recovery should conflict");

        assert!(error.to_string().contains("changed externally"));
        assert_eq!(
            fs::read_to_string(&target).expect("live target should be restored"),
            "# Baseline"
        );
        assert_eq!(
            fs::read_to_string(moved).expect("intended staging should remain recoverable"),
            "# Local"
        );
    }

    #[test]
    fn recovery_artifact_is_self_contained_and_slot_names_are_path_safe() {
        let artifact = encode_recovery_artifact("docs/../safe.md", 42, b"# Original");
        let decoded = decode_recovery_artifact(&artifact).expect("artifact should decode");
        assert_eq!(decoded, ("docs/../safe.md", 42, b"# Original".as_slice()));
        let slot = recovery_slot_name("../../outside.md");
        assert!(slot.ends_with(".recovery"));
        assert!(super::is_owned_recovery_entry_name(&slot));
        assert!(!super::is_owned_recovery_entry_name("personal-notes.md"));
        assert!(slot
            .strip_suffix(".recovery")
            .is_some_and(|hash| !hash.is_empty()
                && hash
                    .chars()
                    .all(|value| value.is_ascii_hexdigit() && !value.is_ascii_uppercase())));
        assert!(decode_recovery_artifact(&artifact[..artifact.len() - 1]).is_none());
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn repeated_successful_saves_leave_no_workspace_artifacts_or_prior_text() {
        let workspace = TestDirectory::new("transaction-single-slot");
        let target = workspace.path().join("page.md");
        fs::write(&target, "# Version 0").expect("baseline should be written");
        for version in 1..=8 {
            let current = fs::read_to_string(&target).expect("current should read");
            write_document_if_current_with_hook(
                workspace.path(),
                "page.md",
                &format!("# Version {version}"),
                &content_token(&current),
                |_| {},
            )
            .expect("version should save");
        }
        assert!(!workspace.path().join(RECOVERY_DIRECTORY).exists());
        assert_eq!(
            fs::read_to_string(&target).expect("saved document should read"),
            "# Version 8"
        );
        let workspace_bytes: Vec<_> = fs::read_dir(workspace.path())
            .expect("workspace should read")
            .filter_map(Result::ok)
            .filter(|entry| entry.path().is_file())
            .flat_map(|entry| fs::read(entry.path()).unwrap_or_default())
            .collect();
        assert!(!String::from_utf8_lossy(&workspace_bytes).contains("Version 0"));
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn repeated_noop_saves_keep_a_git_workspace_clean() {
        let workspace = TestDirectory::new("transaction-git-clean");
        let target = workspace.path().join("page.md");
        fs::write(&target, "# Baseline").expect("baseline should be written");
        for arguments in [
            ["init", "-q"].as_slice(),
            ["config", "user.email", "tests@tracedoc.local"].as_slice(),
            ["config", "user.name", "TraceDoc tests"].as_slice(),
            ["add", "page.md"].as_slice(),
            ["commit", "-qm", "baseline"].as_slice(),
        ] {
            assert!(Command::new("git")
                .args(arguments)
                .current_dir(workspace.path())
                .status()
                .expect("git should run")
                .success());
        }
        for _ in 0..4 {
            write_document_if_current_with_hook(
                workspace.path(),
                "page.md",
                "# Baseline",
                &content_token("# Baseline"),
                |_| {},
            )
            .expect("noop save should succeed");
        }
        let status = Command::new("git")
            .args(["status", "--porcelain", "--untracked-files=all"])
            .current_dir(workspace.path())
            .output()
            .expect("git status should run");
        assert!(status.status.success());
        assert!(String::from_utf8_lossy(&status.stdout).trim().is_empty());
        assert!(!workspace.path().join(RECOVERY_DIRECTORY).exists());
    }

    #[test]
    fn opening_workspace_removes_crash_artifacts_and_deleted_secrets() {
        let workspace = TestDirectory::new("startup-recovery-cleanup");
        fs::write(workspace.path().join("page.md"), "# Current")
            .expect("document should be written");
        #[cfg(target_os = "linux")]
        let recovery = create_owned_recovery_directory(&workspace);
        #[cfg(not(target_os = "linux"))]
        let recovery = {
            let recovery = workspace.path().join(RECOVERY_DIRECTORY);
            fs::create_dir(&recovery).expect("recovery directory should be created");
            fs::write(recovery.join(RECOVERY_OWNER_SENTINEL), RECOVERY_OWNER_MAGIC)
                .expect("recovery owner should be written");
            recovery
        };
        let secret = "deleted-token-7dcb31";
        fs::write(
            recovery.join(recovery_slot_name("page.md")),
            encode_recovery_artifact("page.md", 42, secret.as_bytes()),
        )
        .expect("crash recovery should be written");
        fs::write(
            recovery.join(super::recovery_auxiliary_name("page.md", "staging")),
            secret,
        )
        .expect("crash staging should be written");

        let (project, root) =
            open_workspace(workspace.path()).expect("workspace should scan safely");
        let session = WorkspaceSession::default();
        session
            .activate(root, project.clone())
            .expect("workspace activation should clean crash state");

        assert!(project.documents.contains_key("document:page.md"));
        assert_eq!(project.documents.len(), 1);
        assert!(!recovery.exists());
        let remaining = fs::read_to_string(workspace.path().join("page.md"))
            .expect("current document should remain readable");
        assert!(!remaining.contains(secret));
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn startup_cleanup_preserves_unowned_top_level_entries() {
        use std::os::unix::fs::MetadataExt;

        for kind in ["regular", "symlink"] {
            let workspace = TestDirectory::new(&format!("startup-top-level-{kind}"));
            let recovery = workspace.path().join(RECOVERY_DIRECTORY);
            if kind == "regular" {
                fs::write(&recovery, "user content").expect("regular entry should be written");
            } else {
                let target = workspace.path().join("user-target");
                fs::create_dir(&target).expect("user target should be created");
                fs::write(target.join("secret.txt"), "untouched")
                    .expect("user target should contain data");
                std::os::unix::fs::symlink(&target, &recovery)
                    .expect("top-level symlink should be created");
            }
            let before = fs::symlink_metadata(&recovery).expect("entry should have identity");

            super::cleanup_workspace_recovery_directory(workspace.path())
                .expect("unowned entry should be ignored safely");

            let after = fs::symlink_metadata(&recovery).expect("entry should remain");
            assert_eq!((before.dev(), before.ino()), (after.dev(), after.ino()));
            if kind == "regular" {
                assert_eq!(fs::read_to_string(&recovery).unwrap(), "user content");
            } else {
                assert_eq!(
                    fs::read_to_string(recovery.join("secret.txt")).unwrap(),
                    "untouched"
                );
            }
        }

        let workspace = TestDirectory::new("startup-no-sentinel");
        let recovery = workspace.path().join(RECOVERY_DIRECTORY);
        fs::create_dir(&recovery).expect("unowned directory should be created");
        let slot = recovery.join(recovery_slot_name("page.md"));
        let bytes = encode_recovery_artifact("page.md", 42, b"user-owned lookalike");
        fs::write(&slot, &bytes).expect("lookalike should be written");
        let before = fs::symlink_metadata(&slot).expect("lookalike should have identity");
        super::cleanup_workspace_recovery_directory(workspace.path())
            .expect("directory without sentinel should be ignored");
        let after = fs::symlink_metadata(&slot).expect("lookalike should remain");
        assert_eq!((before.dev(), before.ino()), (after.dev(), after.ino()));
        assert_eq!(fs::read(slot).unwrap(), bytes);
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn startup_cleanup_preflights_and_preserves_every_entry_on_unsafe_content() {
        use std::{
            ffi::CString,
            os::unix::{
                ffi::OsStrExt,
                fs::{FileTypeExt, MetadataExt},
            },
        };

        let workspace = TestDirectory::new("startup-preflight");
        let recovery = create_owned_recovery_directory(&workspace);
        let owned = recovery.join(recovery_slot_name("owned.md"));
        let owned_bytes = encode_recovery_artifact("owned.md", 42, b"old secret");
        fs::write(&owned, &owned_bytes).expect("owned recovery should be written");
        let symlink = recovery.join(super::recovery_auxiliary_name("link.md", "staging"));
        let symlink_target = workspace.path().join("user.md");
        fs::write(&symlink_target, "user target").expect("symlink target should be written");
        std::os::unix::fs::symlink(&symlink_target, &symlink)
            .expect("recognized-name symlink should be created");
        let fifo = recovery.join(super::recovery_auxiliary_name("fifo.md", "lock"));
        let fifo_name = CString::new(fifo.as_os_str().as_bytes()).expect("fifo path should encode");
        assert_eq!(unsafe { libc::mkfifo(fifo_name.as_ptr(), 0o600) }, 0);
        let nested = recovery.join(super::recovery_auxiliary_name("nested.md", "external"));
        fs::create_dir(&nested).expect("recognized-name nested directory should be created");
        fs::write(nested.join("user.txt"), "nested user data")
            .expect("nested user data should be written");
        let unknown = recovery.join("personal.txt");
        fs::write(&unknown, "unknown user data").expect("unknown entry should be written");
        let sentinel = recovery.join(RECOVERY_OWNER_SENTINEL);
        let protected = [&owned, &symlink, &fifo, &nested, &unknown, &sentinel];
        let identities: Vec<_> = protected
            .iter()
            .map(|path| {
                let metadata = fs::symlink_metadata(path).expect("protected entry should exist");
                (metadata.dev(), metadata.ino())
            })
            .collect();

        super::cleanup_workspace_recovery_directory(workspace.path())
            .expect("unsafe content should preserve the entire directory");

        for (path, identity) in protected.iter().zip(identities) {
            let metadata = fs::symlink_metadata(path).expect("protected entry should remain");
            assert_eq!((metadata.dev(), metadata.ino()), identity);
        }

        assert_eq!(fs::read(&owned).unwrap(), owned_bytes);
        assert!(fs::symlink_metadata(&symlink)
            .unwrap()
            .file_type()
            .is_symlink());
        assert_eq!(fs::read_to_string(&symlink_target).unwrap(), "user target");
        assert!(fs::symlink_metadata(&fifo).unwrap().file_type().is_fifo());
        assert_eq!(
            fs::read_to_string(nested.join("user.txt")).unwrap(),
            "nested user data"
        );
        assert_eq!(fs::read_to_string(&unknown).unwrap(), "unknown user data");
        assert_eq!(fs::read(sentinel).unwrap(), RECOVERY_OWNER_MAGIC);
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn malformed_slot_is_preserved_under_a_safe_conflict_name() {
        let workspace = TestDirectory::new("transaction-malformed-slot");
        let target = workspace.path().join("page.md");
        fs::write(&target, "# Baseline").expect("baseline should be written");
        let recovery_directory = create_owned_recovery_directory(&workspace);
        let slot = recovery_directory.join(recovery_slot_name("page.md"));
        fs::write(&slot, "malformed external value").expect("slot should be substituted");
        write_document_if_current_with_hook(
            workspace.path(),
            "page.md",
            "# Saved again",
            &content_token("# Baseline"),
            |_| {},
        )
        .expect("second save should succeed");
        let conflicts: Vec<_> = fs::read_dir(&recovery_directory)
            .expect("recovery should read")
            .filter_map(Result::ok)
            .filter(|entry| {
                entry.path().extension().and_then(|value| value.to_str()) == Some("conflict")
            })
            .collect();
        assert_eq!(conflicts.len(), 1);
        assert_eq!(
            fs::read_to_string(conflicts[0].path()).expect("conflict should read"),
            "malformed external value"
        );
        assert!(!slot.exists());
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn recovery_slot_classification_never_blocks_or_follows_special_entries() {
        use std::{ffi::CString, os::unix::ffi::OsStrExt};
        for kind in ["symlink", "fifo", "unreadable"] {
            let workspace = TestDirectory::new(&format!("recovery-special-{kind}"));
            let target = workspace.path().join("page.md");
            fs::write(&target, "# Baseline").expect("baseline should be written");
            let directory = create_owned_recovery_directory(&workspace);
            let slot = directory.join(recovery_slot_name("page.md"));
            match kind {
                "symlink" => {
                    std::os::unix::fs::symlink(&target, &slot).expect("symlink should be injected");
                }
                "fifo" => {
                    let slot = CString::new(slot.as_os_str().as_bytes())
                        .expect("fifo path should be valid");
                    assert_eq!(unsafe { libc::mkfifo(slot.as_ptr(), 0o600) }, 0);
                }
                _ => {
                    fs::write(&slot, "unrecognized").expect("unknown slot should be injected");
                    let mut permissions = fs::metadata(&slot)
                        .expect("unknown slot should have metadata")
                        .permissions();
                    std::os::unix::fs::PermissionsExt::set_mode(&mut permissions, 0);
                    fs::set_permissions(&slot, permissions)
                        .expect("unknown slot should become unreadable");
                }
            }
            write_document_if_current_with_hook(
                workspace.path(),
                "page.md",
                "# Saved again",
                &content_token("# Baseline"),
                |_| {},
            )
            .expect("special slot should be bounded and preserved");
            let conflict = directory.join(super::recovery_auxiliary_name("page.md", "conflict"));
            assert!(fs::symlink_metadata(conflict).is_ok());
            assert!(!slot.exists());
        }
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn occupied_bounded_conflict_fails_before_document_mutation() {
        let workspace = TestDirectory::new("recovery-conflict-bound");
        let target = workspace.path().join("page.md");
        fs::write(&target, "# Baseline").expect("baseline should be written");
        let root = super::open_directory_no_follow(workspace.path()).expect("root should open");
        let directory = super::open_recovery_directory(&root).expect("recovery should open");
        let slot = workspace
            .path()
            .join(RECOVERY_DIRECTORY)
            .join(recovery_slot_name("page.md"));
        let conflict = workspace
            .path()
            .join(RECOVERY_DIRECTORY)
            .join(super::recovery_auxiliary_name("page.md", "conflict"));
        fs::write(&slot, "unknown normal").expect("unknown slot should be injected");
        fs::write(&conflict, "unknown conflict").expect("conflict should be occupied");
        let error = super::persist_recovery_artifact(&directory, "page.md", b"# Baseline")
            .err()
            .expect("occupied conflict should fail closed");
        assert!(error.to_string().contains("reserve"));
        assert_eq!(
            fs::read_to_string(target).expect("target should read"),
            "# Baseline"
        );
        assert_eq!(
            fs::read_to_string(slot).expect("slot should remain"),
            "unknown normal"
        );
        assert_eq!(
            fs::read_to_string(conflict).expect("conflict should remain"),
            "unknown conflict"
        );
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn concurrent_slot_appearance_is_preserved_and_install_fails_closed() {
        let workspace = TestDirectory::new("recovery-install-race");
        let root = super::open_directory_no_follow(workspace.path()).expect("root should open");
        let directory = super::open_recovery_directory(&root).expect("recovery should open");
        let slot = workspace
            .path()
            .join(RECOVERY_DIRECTORY)
            .join(recovery_slot_name("page.md"));
        let error = super::persist_recovery_artifact_with_hook(
            &directory,
            "page.md",
            b"# Baseline",
            || fs::write(&slot, "concurrent external slot").expect("slot should race install"),
        )
        .err()
        .expect("noreplace install should fail");
        assert!(error.to_string().contains("install"));
        assert_eq!(
            fs::read_to_string(slot).expect("concurrent slot should remain"),
            "concurrent external slot"
        );
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn detached_recovery_directory_fails_before_target_exchange() {
        let workspace = TestDirectory::new("recovery-detach-prepared");
        let target = workspace.path().join("page.md");
        let recovery = workspace.path().join(RECOVERY_DIRECTORY);
        let detached = workspace.path().join("detached-recovery");
        fs::write(&target, "# Baseline").expect("baseline should be written");
        let error = transactional_replace_with_phase_hook(
            workspace.path(),
            &target,
            b"# Local",
            &content_token("# Baseline"),
            |phase, _| {
                if phase == TransactionPhase::Prepared {
                    fs::rename(&recovery, &detached).expect("recovery should detach");
                    fs::create_dir(&recovery).expect("replacement recovery should appear");
                }
            },
        )
        .expect_err("detached recovery must fail before exchange");
        assert!(error.to_string().contains("changed externally"));
        assert_eq!(
            fs::read_to_string(target).expect("target should read"),
            "# Baseline"
        );
        assert!(fs::read_dir(detached)
            .expect("detached recovery should read")
            .next()
            .is_some());
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn detached_recovery_directory_after_exchange_never_publishes_success() {
        let workspace = TestDirectory::new("recovery-detach-installed");
        let target = workspace.path().join("page.md");
        let recovery = workspace.path().join(RECOVERY_DIRECTORY);
        let detached = workspace.path().join("detached-recovery");
        fs::write(&target, "# Baseline").expect("baseline should be written");
        let error = transactional_replace_with_phase_hook(
            workspace.path(),
            &target,
            b"# Local",
            &content_token("# Baseline"),
            |phase, _| {
                if phase == TransactionPhase::Installed {
                    fs::rename(&recovery, &detached).expect("recovery should detach");
                    fs::create_dir(&recovery).expect("replacement recovery should appear");
                }
            },
        )
        .expect_err("detached recovery must prevent success");
        assert!(error.to_string().contains("changed externally during save"));
        assert_eq!(
            fs::read_to_string(target).expect("target should read"),
            "# Local"
        );
        let preserved: Vec<_> = fs::read_dir(detached)
            .expect("detached recovery should read")
            .filter_map(Result::ok)
            .collect();
        assert!(preserved.len() >= 2);
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn never_follows_a_symlink_swapped_before_commit() {
        use std::os::unix::fs::symlink;

        let workspace = TestDirectory::new("transaction-symlink-race");
        let outside = TestDirectory::new("transaction-symlink-outside");
        let path = workspace.path().join("page.md");
        let outside_path = outside.path().join("outside.md");
        fs::write(&path, "# Baseline").expect("baseline should be written");
        fs::write(&outside_path, "# Outside").expect("outside should be written");

        let error = write_document_if_current_with_hook(
            workspace.path(),
            "page.md",
            "# Local",
            &content_token("# Baseline"),
            |_| {
                fs::remove_file(&path).expect("baseline should be removed");
                symlink(&outside_path, &path).expect("symlink should be installed");
            },
        )
        .expect_err("the symlink race should conflict");

        assert!(error.to_string().contains("changed externally"));
        assert_eq!(
            fs::read_to_string(outside_path).expect("outside file should remain"),
            "# Outside"
        );
        assert!(fs::symlink_metadata(path)
            .expect("symlink should remain")
            .file_type()
            .is_symlink());
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn rejects_a_parent_directory_swap_without_writing_outside() {
        use std::os::unix::fs::symlink;

        let workspace = TestDirectory::new("transaction-parent-race");
        let outside = TestDirectory::new("transaction-parent-outside");
        fs::create_dir_all(workspace.path().join("docs")).expect("docs should be created");
        fs::write(workspace.path().join("docs/page.md"), "# Baseline")
            .expect("baseline should be written");
        fs::write(outside.path().join("page.md"), "# Outside").expect("outside should be written");

        let error = write_document_if_current_with_hook(
            workspace.path(),
            "docs/page.md",
            "# Local",
            &content_token("# Baseline"),
            |_| {
                fs::rename(
                    workspace.path().join("docs"),
                    workspace.path().join("docs-old"),
                )
                .expect("docs should move");
                symlink(outside.path(), workspace.path().join("docs"))
                    .expect("parent symlink should be installed");
            },
        )
        .expect_err("the parent swap should conflict");

        assert!(error.to_string().contains("changed externally"));
        assert_eq!(
            fs::read_to_string(outside.path().join("page.md")).expect("outside file should remain"),
            "# Outside"
        );
        assert_eq!(
            fs::read_to_string(workspace.path().join("docs-old/page.md"))
                .expect("original file should remain"),
            "# Baseline"
        );
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn rejects_success_when_the_parent_moves_after_install() {
        let workspace = TestDirectory::new("transaction-parent-after-install");
        let docs = workspace.path().join("docs");
        let moved_docs = workspace.path().join("docs-old");
        fs::create_dir_all(&docs).expect("docs should be created");
        fs::write(docs.join("page.md"), "# Baseline").expect("baseline should be written");

        let error = transactional_replace_with_phase_hook(
            workspace.path(),
            &docs.join("page.md"),
            b"# Local",
            &content_token("# Baseline"),
            |phase, _| {
                if phase == TransactionPhase::Installed {
                    fs::rename(&docs, &moved_docs).expect("docs should move");
                    fs::create_dir(&docs).expect("replacement docs should be created");
                    fs::write(docs.join("page.md"), "# External C")
                        .expect("external C should become current");
                }
            },
        )
        .expect_err("stale parent install must not publish success");

        assert!(error.to_string().contains("recoverable"));
        assert_eq!(
            fs::read_to_string(docs.join("page.md")).expect("external C should remain"),
            "# External C"
        );
        assert_eq!(
            fs::read_to_string(moved_docs.join("page.md")).expect("stale install should remain"),
            "# Local"
        );
        let recovery_contains_baseline = fs::read_dir(workspace.path().join(RECOVERY_DIRECTORY))
            .expect("recovery should read")
            .filter_map(Result::ok)
            .any(|entry| fs::read_to_string(entry.path()).is_ok_and(|value| value == "# Baseline"));
        assert!(recovery_contains_baseline);
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn avoids_a_false_recovery_path_when_the_workspace_root_moves() {
        let workspace = TestDirectory::new("transaction-root-move");
        let root = workspace.path().to_path_buf();
        let moved = root.with_extension("moved");
        let target = root.join("page.md");
        fs::write(&target, "# Baseline").expect("baseline should be written");

        let error = transactional_replace_with_phase_hook(
            &root,
            &target,
            b"# Local",
            &content_token("# Baseline"),
            |phase, _| {
                if phase == TransactionPhase::Prepared {
                    fs::rename(&root, &moved).expect("workspace root should move");
                    fs::create_dir(&root).expect("replacement root should be created");
                }
            },
        )
        .expect_err("moved root must invalidate save");

        assert!(error.to_string().contains("held app handle"));
        assert!(!error.to_string().contains(".tracedoc-recovery/"));
        assert_eq!(
            fs::read_to_string(moved.join("page.md")).expect("baseline should remain"),
            "# Baseline"
        );
        fs::remove_dir_all(moved).expect("moved fixture should be removed");
    }

    #[test]
    fn preserves_contiguous_history_at_the_511_and_512_revision_boundaries() {
        let workspace = TestDirectory::new("history-boundary");
        fs::write(workspace.path().join("page.md"), "# Baseline").expect("page should be written");
        let project = crate::services::workspace::scan_workspace(workspace.path())
            .expect("workspace should scan");
        let session = WorkspaceSession::default();
        let generation = session
            .activate(workspace.path().to_path_buf(), project)
            .expect("workspace should activate");
        let lease = session.capture(generation).expect("lease should capture");
        {
            let mut state = session.state.write().expect("session should lock");
            let active = state
                .workspace
                .as_mut()
                .expect("workspace should be active");
            active.revision = 512;
            active.history = (2..=512)
                .map(|revision| empty_patch(generation, revision))
                .collect();
        }
        let update = session
            .save_document(
                &lease,
                "page.md",
                "# Saved",
                &content_token("# Baseline"),
                1,
            )
            .expect("511 intervening revisions should remain recoverable");
        assert_eq!(update.workspace_revision, 513);
        assert_eq!(update.patches.len(), 512);
        assert_eq!(update.patches[0].workspace_revision, 2);
        assert_eq!(update.patches[511].workspace_revision, 513);

        fs::write(workspace.path().join("page.md"), "# New baseline")
            .expect("new baseline should be written");
        {
            let mut state = session.state.write().expect("session should lock");
            let active = state
                .workspace
                .as_mut()
                .expect("workspace should be active");
            active.revision = 513;
            active.history = (2..=513)
                .map(|revision| empty_patch(generation, revision))
                .collect();
        }
        let error = session
            .save_document(
                &lease,
                "page.md",
                "# Must not write",
                &content_token("# New baseline"),
                1,
            )
            .expect_err("512 intervening revisions should reject before disk commit");
        assert!(error.to_string().contains("changed too much"));
        assert_eq!(
            fs::read_to_string(workspace.path().join("page.md")).expect("baseline should remain"),
            "# New baseline"
        );
    }

    #[test]
    fn rejects_revision_overflow_before_disk_commit() {
        let workspace = TestDirectory::new("revision-overflow");
        fs::write(workspace.path().join("page.md"), "# Baseline").expect("page should be written");
        let project = crate::services::workspace::scan_workspace(workspace.path())
            .expect("workspace should scan");
        let session = WorkspaceSession::default();
        let generation = session
            .activate(workspace.path().to_path_buf(), project)
            .expect("workspace should activate");
        let lease = session.capture(generation).expect("lease should capture");
        {
            let mut state = session.state.write().expect("session should lock");
            state
                .workspace
                .as_mut()
                .expect("workspace should be active")
                .revision = u64::MAX;
        }

        let error = session
            .save_document(
                &lease,
                "page.md",
                "# Must not write",
                &content_token("# Baseline"),
                u64::MAX,
            )
            .expect_err("overflow should reject");
        assert!(error.to_string().contains("revision is unavailable"));
        assert_eq!(
            fs::read_to_string(workspace.path().join("page.md")).expect("baseline should remain"),
            "# Baseline"
        );
    }

    #[test]
    fn watcher_and_refresh_overflow_leave_model_and_history_unchanged() {
        let workspace = TestDirectory::new("mutation-overflow");
        fs::write(workspace.path().join("page.md"), "# Baseline").expect("page should be written");
        let project = crate::services::workspace::scan_workspace(workspace.path())
            .expect("workspace should scan");
        let session = WorkspaceSession::default();
        let generation = session
            .activate(workspace.path().to_path_buf(), project)
            .expect("workspace should activate");
        let (before_project, before_history) = {
            let mut state = session.state.write().expect("session should lock");
            let active = state
                .workspace
                .as_mut()
                .expect("workspace should be active");
            active.revision = u64::MAX;
            (active.project.clone(), active.history.clone())
        };
        fs::write(workspace.path().join("page.md"), "# External")
            .expect("external content should be written");

        session
            .apply_external_changes(
                generation,
                &[WorkspaceChange {
                    path: workspace.path().join("page.md"),
                    kind: ChangeKind::Modify,
                }],
            )
            .expect_err("watcher overflow should fail");
        session
            .refresh_workspace(generation)
            .expect_err("refresh overflow should fail");

        let state = session.state.read().expect("session should lock");
        let active = state
            .workspace
            .as_ref()
            .expect("workspace should be active");
        assert_eq!(active.project, before_project);
        assert_eq!(active.history, before_history);
        assert_eq!(active.revision, u64::MAX);
    }

    #[test]
    fn orders_save_watcher_and_refresh_mutations_monotonically() {
        let workspace = TestDirectory::new("revision-order");
        fs::write(workspace.path().join("page.md"), "# One").expect("page should be written");
        let (project, root) = open_workspace(workspace.path()).expect("workspace should open");
        let session = WorkspaceSession::default();
        let generation = session
            .activate(root, project)
            .expect("workspace should activate");
        let lease = session.capture(generation).expect("lease should capture");
        let read = session
            .read_document(&lease, "page.md")
            .expect("page should read");
        assert_eq!(read.workspace_revision, 1);
        let saved = session
            .save_document(&lease, "page.md", "# Two", &read.content_token, 1)
            .expect("page should save");
        assert_eq!(saved.workspace_revision, 2);

        fs::write(workspace.path().join("page.md"), "# Three")
            .expect("external change should be written");
        let watched = session
            .apply_external_changes(
                generation,
                &[WorkspaceChange {
                    path: workspace.path().join("page.md"),
                    kind: ChangeKind::Modify,
                }],
            )
            .expect("external change should apply")
            .expect("external change should emit a patch");
        assert_eq!(watched.workspace_revision, 3);

        let refreshed = session
            .refresh_workspace(generation)
            .expect("workspace should refresh");
        assert_eq!(refreshed.workspace_revision, 4);
    }

    #[test]
    fn save_response_catches_up_an_unrelated_pending_watcher_patch() {
        let workspace = TestDirectory::new("save-catch-up");
        fs::write(workspace.path().join("page.md"), "# Page").expect("page should be written");
        fs::write(workspace.path().join("other.md"), "# Other").expect("other should be written");
        let (project, root) = open_workspace(workspace.path()).expect("workspace should open");
        let session = WorkspaceSession::default();
        let generation = session
            .activate(root, project)
            .expect("workspace should activate");
        let lease = session.capture(generation).expect("lease should capture");
        let page = session
            .read_document(&lease, "page.md")
            .expect("page should read");
        fs::write(workspace.path().join("other.md"), "# External other")
            .expect("other should change externally");
        let watched = session
            .apply_external_changes(
                generation,
                &[WorkspaceChange {
                    path: workspace.path().join("other.md"),
                    kind: ChangeKind::Modify,
                }],
            )
            .expect("watcher change should apply")
            .expect("watcher change should emit");
        assert_eq!(watched.workspace_revision, 2);

        let saved = session
            .save_document(
                &lease,
                "page.md",
                "# Saved page",
                &page.content_token,
                page.workspace_revision,
            )
            .expect("save should include catch-up patches");

        assert_eq!(saved.workspace_revision, 3);
        assert_eq!(
            saved
                .patches
                .iter()
                .map(|patch| patch.workspace_revision)
                .collect::<Vec<_>>(),
            [2, 3]
        );
        assert_eq!(
            saved.patches[0].upserted_documents[0].title.as_deref(),
            Some("External other")
        );
    }

    #[test]
    fn rejects_a_pending_save_after_the_workspace_changes() {
        let first = TestDirectory::new("pending-save-first");
        let second = TestDirectory::new("pending-save-second");
        fs::write(first.path().join("page.md"), "# First").expect("first page should be written");
        fs::write(second.path().join("page.md"), "# Second")
            .expect("second page should be written");
        let (first_project, first_root) =
            open_workspace(first.path()).expect("first workspace should open");
        let (second_project, second_root) =
            open_workspace(second.path()).expect("second workspace should open");
        let session = WorkspaceSession::default();
        let first_generation = session
            .activate(first_root, first_project)
            .expect("first workspace should activate");
        let lease = session
            .capture(first_generation)
            .expect("first workspace lease should be captured");
        let barrier = Arc::new(Barrier::new(2));
        let pending_session = session.clone();
        let pending_barrier = barrier.clone();
        let pending_save = thread::spawn(move || {
            pending_barrier.wait();
            pending_session.save_document(
                &lease,
                "page.md",
                "# Stale",
                &content_token("# First"),
                1,
            )
        });

        session
            .activate(second_root, second_project)
            .expect("second workspace should activate");
        barrier.wait();
        let error = pending_save
            .join()
            .expect("pending save thread should finish")
            .expect_err("stale save should fail");

        assert!(error.to_string().contains("workspace changed"));
        assert_eq!(
            fs::read_to_string(first.path().join("page.md"))
                .expect("first page should remain readable"),
            "# First"
        );
        assert_eq!(
            fs::read_to_string(second.path().join("page.md"))
                .expect("second page should remain readable"),
            "# Second"
        );
    }

    #[test]
    fn rejects_pending_reads_and_late_capture_after_the_workspace_changes() {
        let first = TestDirectory::new("pending-read-first");
        let second = TestDirectory::new("pending-read-second");
        fs::write(first.path().join("page.md"), "# First").expect("first page should be written");
        fs::write(second.path().join("page.md"), "# Second")
            .expect("second page should be written");
        let (first_project, first_root) =
            open_workspace(first.path()).expect("first workspace should open");
        let (second_project, second_root) =
            open_workspace(second.path()).expect("second workspace should open");
        let session = WorkspaceSession::default();
        let first_generation = session
            .activate(first_root, first_project)
            .expect("first workspace should activate");
        let lease = session
            .capture(first_generation)
            .expect("first workspace lease should be captured");

        session
            .activate(second_root, second_project)
            .expect("second workspace should activate");

        let read_error = session
            .read_document(&lease, "page.md")
            .expect_err("stale read should fail");
        let capture_error = session
            .capture(first_generation)
            .expect_err("late stale capture should fail");
        assert!(read_error.to_string().contains("workspace changed"));
        assert!(capture_error.to_string().contains("workspace changed"));
    }

    #[cfg(unix)]
    #[test]
    fn opens_scanned_unix_markdown_filename_with_backslash() {
        let workspace = TestDirectory::new("unix-backslash");
        let document_path = r"part\name.md";
        fs::write(workspace.path().join(document_path), "# Original")
            .expect("Unix document should be written");

        let (project, root) = open_workspace(workspace.path()).expect("workspace should scan");
        assert!(project
            .documents
            .contains_key(&format!("document:{document_path}")));
        assert_eq!(
            read_document(&root, document_path).expect("Unix document should open"),
            "# Original"
        );

        write_document(&root, document_path, "# Changed").expect("Unix document should save");
        assert_eq!(
            fs::read_to_string(workspace.path().join(document_path))
                .expect("Unix document should remain readable"),
            "# Changed"
        );
    }

    #[test]
    fn handles_a_moderately_large_markdown_document() {
        let workspace = TestDirectory::new("large-document");
        let content = format!(
            "# Large\n\n{}",
            "A searchable Markdown line.\n".repeat(20_000)
        );
        fs::write(workspace.path().join("large.md"), &content)
            .expect("large document should be written");

        assert_eq!(
            read_document(workspace.path(), "large.md").expect("large document should be readable"),
            content
        );
    }

    #[cfg(unix)]
    #[test]
    fn rejects_symlinks_that_escape_the_workspace() {
        use std::os::unix::fs::symlink;

        let workspace = TestDirectory::new("workspace");
        let outside = TestDirectory::new("outside");
        fs::write(outside.path().join("secret.md"), "Secret")
            .expect("outside document should be written");
        symlink(
            outside.path().join("secret.md"),
            workspace.path().join("escape.md"),
        )
        .expect("symlink should be created");

        let read_error =
            read_document(workspace.path(), "escape.md").expect_err("outside read should fail");
        let write_error = write_document(workspace.path(), "escape.md", "Changed")
            .expect_err("outside write should fail");
        assert!(read_error.to_string().contains("outside the workspace"));
        assert!(write_error.to_string().contains("outside the workspace"));
        assert_eq!(
            fs::read_to_string(outside.path().join("secret.md"))
                .expect("outside document should remain readable"),
            "Secret"
        );
    }
}
