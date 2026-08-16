use crate::{
    models::workspace::{Document, Folder, ProjectModel, WorkspaceEventError, WorkspacePatch},
    services::{
        markdown::refresh_document_index,
        workspace::{ignored_relative_ancestor, is_ignored_entry, normalize_relative_path},
    },
};
use notify::{event::ModifyKind, Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use std::{
    collections::{BTreeMap, BTreeSet},
    ffi::OsStr,
    fs,
    path::{Path, PathBuf},
    sync::mpsc,
    thread,
    time::{Duration, Instant},
};
use tauri::{AppHandle, Emitter};

const DEBOUNCE: Duration = Duration::from_millis(120);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ChangeKind {
    Upsert,
    Modify,
    Remove,
    Reconcile,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WorkspaceChange {
    pub path: PathBuf,
    pub kind: ChangeKind,
}

#[derive(Default)]
struct PendingChange {
    structural: bool,
    reindex_existing: bool,
}

/// Registers native recursive filesystem watches. Installation of the
/// resulting watcher into a workspace session is arbitrated entirely by
/// `WorkspaceSession::install_watcher`, under the same lock that guards
/// `generation`/`workspace`, so this type carries no state of its own: two
/// overlapping `arm`/`finish` pairs never contend over which watcher "is
/// active" here - only the session's generation compare-and-set decides
/// that.
#[derive(Clone, Copy, Default)]
pub struct WorkspaceWatcher;

/// A native watch that has been registered with the OS but is not yet
/// delivering events to a session. Returned by `WorkspaceWatcher::arm` and
/// consumed by `WorkspaceWatcher::finish`.
///
/// `root_identity` is the filesystem-object identity observed for the
/// watched root at the moment `arm` registered the watch, kept so `finish`
/// can detect the narrow race where a different directory has since been
/// moved into that same path (see `WorkspaceWatcher::finish`).
pub struct ArmedWatcher {
    watcher: RecommendedWatcher,
    receiver: mpsc::Receiver<notify::Result<Event>>,
    root_identity: Option<RootIdentity>,
}

impl WorkspaceWatcher {
    /// Registers the native recursive filesystem watch for `root` and starts
    /// buffering its events immediately - *before* the workspace is scanned -
    /// so a file created, modified, or removed while the scan is still
    /// running is queued here rather than silently missed.
    ///
    /// Deliberately does *not* touch whatever watcher is currently installed
    /// on any active session: if the scan that follows (or activation of its
    /// result) fails, the caller simply drops the returned `ArmedWatcher`
    /// and the previously active workspace - if any - keeps its own watcher
    /// and live updates uninterrupted. Only a *successful*
    /// `WorkspaceSession::activate` followed by `finish` ever retires a
    /// previous watcher.
    pub fn arm(&self, root: &Path) -> Result<ArmedWatcher, String> {
        let (sender, receiver) = mpsc::channel();
        let mut watcher = notify::recommended_watcher(move |event| {
            let _ = sender.send(event);
        })
        .map_err(|error| format!("Unable to create workspace watcher: {error}"))?;
        watcher
            .watch(root, RecursiveMode::Recursive)
            .map_err(|error| format!("Unable to watch workspace: {error}"))?;
        let root_identity = capture_root_identity(root);
        Ok(ArmedWatcher {
            watcher,
            receiver,
            root_identity,
        })
    }

    /// Drains any changes (and errors) buffered by `arm` while the scan was
    /// running, installs the watcher into `session` gated on `generation`
    /// still being the session's current generation, folds the buffered
    /// changes in through the same incremental reconciliation path used for
    /// live events, and begins normal debounced event delivery for
    /// subsequent changes.
    ///
    /// If a newer `open_workspace` request has already activated a later
    /// generation by the time this runs - the two overlapping requests race
    /// described by the P1 finding - installation is rejected atomically
    /// with the generation check: this watcher is torn down (its buffered
    /// changes and errors discarded) without touching whatever the newer
    /// request installed. The caller's subsequent `session.snapshot(generation)`
    /// call surfaces that loss to the (stale) caller authoritatively.
    ///
    /// Before any of that, revalidates that `root`'s filesystem-object
    /// identity still matches what `arm` observed when it registered the
    /// watch. Path-based watch backends (Linux inotify in particular) stay
    /// bound to the object they resolved `root` to at `watch(2)` time, not
    /// to whatever the path currently names; if something external renamed
    /// that object away and moved a different one into `root`'s path while
    /// the scan was running, `armed` is silently watching the wrong,
    /// vanished directory while the scan (path-based, so it reads whatever
    /// is at `root` right now) already picked up the replacement. When a
    /// mismatch is detected, `armed` is discarded and a fresh watch is
    /// armed on `root` in its place before draining/installing proceeds, so
    /// the watcher that ends up installed tracks the replacement directory
    /// the session just activated. This is a narrow, low-probability race -
    /// it requires an external directory swap at the exact canonical path
    /// during the scan window - not a concern in ordinary single-writer use.
    pub fn finish<R: tauri::Runtime>(
        &self,
        armed: ArmedWatcher,
        root: &Path,
        app: AppHandle<R>,
        session: crate::services::document::WorkspaceSession,
        generation: u64,
    ) -> Result<(), String> {
        let armed = self.revalidate_root_identity(armed, root);
        let ArmedWatcher {
            watcher, receiver, ..
        } = armed;

        let mut pending = Vec::new();
        let mut buffered_errors = Vec::new();
        while let Ok(event) = receiver.try_recv() {
            match event {
                Ok(event) => pending.extend(normalize_event(event)),
                Err(error) => buffered_errors.push(format!("Workspace watcher error: {error}")),
            }
        }

        // Gate installation on the generation check first: a stale watcher
        // (superseded by a newer, already-activated request) is torn down
        // right here - along with everything buffered above - instead of
        // silently displacing the watcher the newer request installed.
        if session.install_watcher(generation, watcher).is_err() {
            return Ok(());
        }

        if !pending.is_empty() {
            // Best-effort: if the workspace already moved on to a newer
            // generation between the install above and here,
            // `session.snapshot` (called by the caller right after this
            // returns) will surface that authoritatively.
            let _ = session.apply_external_changes(generation, &pending);
        }

        // The native backend can report errors (e.g. an overflow or a
        // watched-subtree failure) while the initial scan is still running.
        // Surface them the same way `watch_loop` does for live errors below,
        // instead of letting them vanish along with a successful snapshot.
        // Read the revision only after the reconciliation above so a
        // buffered error alongside a nonempty patch is stamped with the
        // revision the caller's own `session.snapshot` is about to observe.
        for message in buffered_errors {
            let workspace_revision = session.current_revision(generation).unwrap_or(0);
            let _ = app.emit(
                "workspace-watch-error",
                WorkspaceEventError {
                    workspace_generation: generation,
                    workspace_revision,
                    message,
                },
            );
        }

        thread::spawn(move || watch_loop(receiver, app, session, generation));
        Ok(())
    }

    /// Returns `armed` unchanged unless `root`'s filesystem-object identity
    /// has diverged from the identity `arm` captured for it, in which case
    /// `armed` is dropped and a fresh watch is armed on `root` in its place.
    /// Best-effort: if identity cannot be determined on either side (an
    /// unsupported platform, or a transient stat failure), no divergence is
    /// assumed and `armed` is kept as-is, matching pre-revalidation
    /// behavior. If re-arming fails, `armed` is also kept as-is rather than
    /// leaving the workspace with no watcher at all.
    fn revalidate_root_identity(&self, armed: ArmedWatcher, root: &Path) -> ArmedWatcher {
        let diverged = armed
            .root_identity
            .zip(capture_root_identity(root))
            .is_some_and(|(armed_identity, current_identity)| armed_identity != current_identity);
        if !diverged {
            return armed;
        }
        self.arm(root).unwrap_or(armed)
    }
}

/// Opaque filesystem-object identity used to detect whether the directory
/// backing a watched root's *path* has been swapped for a different
/// directory. `(volume, file)` is `(device, inode)` on Unix and
/// `(volume serial number, file index)` on Windows - the same identity
/// pairs `services/document.rs` already relies on for its save-path
/// revalidation, just captured for the workspace root instead of a
/// document's save path.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct RootIdentity {
    volume: u64,
    file: u64,
}

#[cfg(unix)]
fn capture_root_identity(root: &Path) -> Option<RootIdentity> {
    use std::os::unix::fs::MetadataExt;

    let metadata = fs::metadata(root).ok()?;
    Some(RootIdentity {
        volume: metadata.dev(),
        file: metadata.ino(),
    })
}

#[cfg(windows)]
fn capture_root_identity(root: &Path) -> Option<RootIdentity> {
    use std::os::windows::{fs::OpenOptionsExt, io::AsRawHandle};
    use windows_sys::Win32::Storage::FileSystem::{
        GetFileInformationByHandle, BY_HANDLE_FILE_INFORMATION, FILE_FLAG_BACKUP_SEMANTICS,
        FILE_SHARE_DELETE, FILE_SHARE_READ, FILE_SHARE_WRITE,
    };

    let file = fs::OpenOptions::new()
        .read(true)
        .share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE)
        .custom_flags(FILE_FLAG_BACKUP_SEMANTICS)
        .open(root)
        .ok()?;
    let mut information: BY_HANDLE_FILE_INFORMATION = unsafe { std::mem::zeroed() };
    if unsafe { GetFileInformationByHandle(file.as_raw_handle() as _, &mut information) } == 0 {
        return None;
    }
    Some(RootIdentity {
        volume: u64::from(information.dwVolumeSerialNumber),
        file: (u64::from(information.nFileIndexHigh) << 32) | u64::from(information.nFileIndexLow),
    })
}

#[cfg(not(any(unix, windows)))]
fn capture_root_identity(_root: &Path) -> Option<RootIdentity> {
    None
}

fn watch_loop<R: tauri::Runtime>(
    receiver: mpsc::Receiver<notify::Result<Event>>,
    app: AppHandle<R>,
    session: crate::services::document::WorkspaceSession,
    generation: u64,
) {
    while let Ok(first) = receiver.recv() {
        let mut events = vec![first];
        let deadline = Instant::now() + DEBOUNCE;
        loop {
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                break;
            }
            match receiver.recv_timeout(remaining) {
                Ok(event) => events.push(event),
                Err(mpsc::RecvTimeoutError::Timeout) => break,
                Err(mpsc::RecvTimeoutError::Disconnected) => return,
            }
        }

        let mut changes = Vec::new();
        let mut error_message = None;
        for event in events {
            match event {
                Ok(event) => changes.extend(normalize_event(event)),
                Err(error) => error_message = Some(format!("Workspace watcher error: {error}")),
            }
        }

        if !changes.is_empty() {
            match session.apply_external_changes(generation, &changes) {
                Ok(Some(patch)) => {
                    let _ = app.emit("workspace-patch", patch);
                }
                Ok(None) => {}
                Err(error) if error.to_string().contains("workspace changed") => return,
                Err(error) => {
                    let workspace_revision = session.current_revision(generation).unwrap_or(0);
                    let _ = app.emit(
                        "workspace-watch-error",
                        WorkspaceEventError {
                            workspace_generation: generation,
                            workspace_revision,
                            message: error.to_string(),
                        },
                    );
                }
            }
        }

        // Read the revision only after the reconciliation above so a
        // watcher error batched alongside a nonempty patch is stamped with
        // the revision the patch (if any) just advanced to, not the value
        // that was current before it.
        if let Some(message) = error_message {
            let workspace_revision = session.current_revision(generation).unwrap_or(0);
            let _ = app.emit(
                "workspace-watch-error",
                WorkspaceEventError {
                    workspace_generation: generation,
                    workspace_revision,
                    message,
                },
            );
        }
    }
}

fn normalize_event(event: Event) -> Vec<WorkspaceChange> {
    let kind = match event.kind {
        EventKind::Create(_) => ChangeKind::Upsert,
        EventKind::Modify(ModifyKind::Name(_)) => ChangeKind::Reconcile,
        EventKind::Modify(_) => ChangeKind::Modify,
        EventKind::Remove(_) => ChangeKind::Remove,
        EventKind::Any | EventKind::Other => ChangeKind::Reconcile,
        EventKind::Access(_) => return Vec::new(),
    };
    event
        .paths
        .into_iter()
        .map(|path| WorkspaceChange { path, kind })
        .collect()
}

pub fn apply_project_changes(
    root: &Path,
    project: &mut ProjectModel,
    changes: &[WorkspaceChange],
    generation: u64,
) -> Result<WorkspacePatch, String> {
    let before = project.clone();
    let mut content_paths = BTreeSet::new();
    let mut topology_changed = false;
    let mut normalized_changes: BTreeMap<String, PendingChange> = BTreeMap::new();

    for change in changes {
        let Ok(relative) = change.path.strip_prefix(root) else {
            continue;
        };
        let Ok(relative) = normalize_relative_path(relative) else {
            continue;
        };
        if relative.is_empty() {
            continue;
        }
        let pending = normalized_changes.entry(relative).or_default();
        match change.kind {
            ChangeKind::Modify => {}
            ChangeKind::Upsert => pending.structural = true,
            ChangeKind::Remove | ChangeKind::Reconcile => {
                pending.structural = true;
                pending.reindex_existing = true;
            }
        }
    }

    let mut covered_directories = Vec::new();
    for (relative, pending) in normalized_changes {
        if covered_directories
            .iter()
            .any(|directory: &String| relative.starts_with(&format!("{directory}/")))
        {
            continue;
        }
        let absolute = root.join(Path::new(&relative));
        let metadata = fs::symlink_metadata(&absolute).ok();
        let is_directory = metadata.as_ref().is_some_and(|value| value.is_dir())
            || project
                .folders
                .values()
                .any(|folder| folder.path == relative);
        if let Some(ignored_ancestor) = ignored_relative_ancestor(root, &relative, is_directory) {
            topology_changed |= remove_path(project, &ignored_ancestor);
            if is_directory {
                covered_directories.push(ignored_ancestor);
            }
            continue;
        }
        if let Some(symlink_path) = first_symlink_component(root, &relative) {
            topology_changed |= remove_path(project, &symlink_path);
            continue;
        }
        if metadata
            .as_ref()
            .is_some_and(|value| value.file_type().is_symlink())
        {
            topology_changed |= remove_path(project, &relative);
            continue;
        }
        if metadata.as_ref().is_some_and(|value| value.is_dir()) {
            if project
                .documents
                .values()
                .any(|document| document.path == relative)
            {
                topology_changed |= remove_path(project, &relative);
            }
            if pending.structural || !project.folders.contains_key(&folder_id(&relative)) {
                topology_changed |= reconcile_directory(
                    root,
                    project,
                    &relative,
                    &mut content_paths,
                    pending.reindex_existing,
                );
                covered_directories.push(relative);
            }
            continue;
        }
        if metadata.as_ref().is_some_and(|value| value.is_file()) && is_markdown(&absolute) {
            if project
                .folders
                .values()
                .any(|folder| folder.path == relative)
            {
                topology_changed |= remove_path(project, &relative);
            }
            let was_new = !project.documents.contains_key(&document_id(&relative));
            ensure_folder_chain(project, parent_path(&relative));
            upsert_document(project, &relative);
            content_paths.insert(relative);
            topology_changed |= was_new;
        } else {
            topology_changed |= remove_path(project, &relative);
            if is_directory {
                covered_directories.push(relative);
            }
        }
    }

    rebuild_hierarchy(project);
    let externally_changed_document_ids: BTreeSet<_> = content_paths
        .iter()
        .map(|path| document_id(path))
        .chain(
            before
                .documents
                .keys()
                .filter(|id| !project.documents.contains_key(*id))
                .cloned(),
        )
        .collect();

    for path in &content_paths {
        reindex_path(root, project, path, generation);
    }

    if topology_changed {
        let linked_sources: BTreeSet<_> = before
            .links
            .iter()
            .chain(project.links.iter())
            .map(|link| link.source_document_id.clone())
            .collect();
        for source_id in linked_sources {
            if let Some(path) = project
                .documents
                .get(&source_id)
                .map(|document| document.path.clone())
            {
                if !content_paths.contains(&path) {
                    reindex_path(root, project, &path, generation);
                }
            }
        }
    }

    Ok(diff_project(
        &before,
        project,
        generation,
        externally_changed_document_ids,
    ))
}

fn reconcile_directory(
    root: &Path,
    project: &mut ProjectModel,
    relative: &str,
    content_paths: &mut BTreeSet<String>,
    reindex_existing: bool,
) -> bool {
    let before_documents: BTreeSet<_> = project.documents.keys().cloned().collect();
    let mut seen_folders = BTreeSet::from([relative.to_owned()]);
    let mut seen_documents = BTreeSet::new();
    ensure_folder_chain(project, relative);
    scan_directory(
        root,
        project,
        relative,
        content_paths,
        &mut seen_folders,
        &mut seen_documents,
        reindex_existing,
    );
    let prefix = format!("{relative}/");
    let removed_document_ids: BTreeSet<_> = project
        .documents
        .values()
        .filter(|document| {
            (document.path == relative || document.path.starts_with(&prefix))
                && !seen_documents.contains(&document.path)
        })
        .map(|document| document.id.clone())
        .collect();
    project.documents.retain(|id, document| {
        !removed_document_ids.contains(id) || seen_documents.contains(&document.path)
    });
    project.folders.retain(|_, folder| {
        folder.path.is_empty()
            || !folder.path.starts_with(&prefix)
            || seen_folders.contains(&folder.path)
            || folder.path == relative
    });
    project
        .links
        .retain(|link| !removed_document_ids.contains(&link.source_document_id));
    before_documents != project.documents.keys().cloned().collect()
}

fn scan_directory(
    root: &Path,
    project: &mut ProjectModel,
    relative: &str,
    content_paths: &mut BTreeSet<String>,
    seen_folders: &mut BTreeSet<String>,
    seen_documents: &mut BTreeSet<String>,
    reindex_existing: bool,
) {
    let Ok(entries) = fs::read_dir(root.join(relative)) else {
        return;
    };
    let mut entries: Vec<_> = entries.filter_map(Result::ok).collect();
    entries.sort_by_key(|entry| entry.file_name());
    for entry in entries {
        let Ok(file_type) = entry.file_type() else {
            continue;
        };
        if file_type.is_symlink() {
            continue;
        }
        let Ok(name) = entry.file_name().into_string() else {
            continue;
        };
        if is_ignored_entry(&entry, &name, file_type.is_dir()) {
            continue;
        }
        let child = if relative.is_empty() {
            name
        } else {
            format!("{relative}/{name}")
        };
        if file_type.is_dir() {
            seen_folders.insert(child.clone());
            ensure_folder_chain(project, &child);
            scan_directory(
                root,
                project,
                &child,
                content_paths,
                seen_folders,
                seen_documents,
                reindex_existing,
            );
        } else if file_type.is_file() && is_markdown(&entry.path()) {
            let is_new = !project.documents.contains_key(&document_id(&child));
            upsert_document(project, &child);
            seen_documents.insert(child.clone());
            if is_new || reindex_existing {
                content_paths.insert(child);
            }
        }
    }
}

fn ensure_folder_chain(project: &mut ProjectModel, relative: &str) {
    if relative.is_empty() {
        return;
    }
    let mut current = String::new();
    for segment in relative.split('/') {
        current = if current.is_empty() {
            segment.to_owned()
        } else {
            format!("{current}/{segment}")
        };
        if project.folders.contains_key(&folder_id(&current)) {
            continue;
        }
        let parent = parent_path(&current);
        let id = folder_id(&current);
        project.folders.insert(
            id.clone(),
            Folder {
                id,
                name: segment.to_owned(),
                path: current.clone(),
                parent_id: Some(folder_id(parent)),
                child_folder_ids: Vec::new(),
                document_ids: Vec::new(),
            },
        );
    }
}

fn upsert_document(project: &mut ProjectModel, relative: &str) {
    let id = document_id(relative);
    let name = relative.rsplit('/').next().unwrap_or(relative).to_owned();
    project.documents.entry(id.clone()).or_insert(Document {
        id,
        title: Path::new(&name)
            .file_stem()
            .and_then(OsStr::to_str)
            .map(str::to_owned),
        name,
        headings: Vec::new(),
        path: relative.to_owned(),
        parent_id: folder_id(parent_path(relative)),
    });
}

fn remove_path(project: &mut ProjectModel, relative: &str) -> bool {
    let prefix = format!("{relative}/");
    let removed_document_ids: BTreeSet<_> = project
        .documents
        .values()
        .filter(|document| document.path == relative || document.path.starts_with(&prefix))
        .map(|document| document.id.clone())
        .collect();
    let removed_folder_ids: BTreeSet<_> = project
        .folders
        .values()
        .filter(|folder| {
            !folder.path.is_empty() && (folder.path == relative || folder.path.starts_with(&prefix))
        })
        .map(|folder| folder.id.clone())
        .collect();
    project
        .documents
        .retain(|id, _| !removed_document_ids.contains(id));
    project
        .folders
        .retain(|id, _| !removed_folder_ids.contains(id));
    project
        .links
        .retain(|link| !removed_document_ids.contains(&link.source_document_id));
    !removed_document_ids.is_empty()
}

fn rebuild_hierarchy(project: &mut ProjectModel) {
    for folder in project.folders.values_mut() {
        folder.child_folder_ids.clear();
        folder.document_ids.clear();
    }
    let folders: Vec<_> = project.folders.values().cloned().collect();
    for folder in folders {
        if let Some(parent_id) = folder.parent_id {
            if let Some(parent) = project.folders.get_mut(&parent_id) {
                parent.child_folder_ids.push(folder.id);
            }
        }
    }
    let documents: Vec<_> = project.documents.values().cloned().collect();
    for document in documents {
        if let Some(parent) = project.folders.get_mut(&document.parent_id) {
            parent.document_ids.push(document.id);
        }
    }
    for folder in project.folders.values_mut() {
        folder.child_folder_ids.sort();
        folder.document_ids.sort();
    }
}

fn reindex_path(root: &Path, project: &mut ProjectModel, path: &str, generation: u64) {
    match fs::read_to_string(root.join(path)) {
        Ok(content) => {
            let _ = refresh_document_index(project, path, &content, generation);
        }
        Err(_) => {
            let id = document_id(path);
            if let Some(document) = project.documents.get_mut(&id) {
                document.headings.clear();
                document.title = Path::new(&document.name)
                    .file_stem()
                    .and_then(OsStr::to_str)
                    .map(str::to_owned);
            }
            project.links.retain(|link| link.source_document_id != id);
        }
    }
}

pub(crate) fn diff_project(
    before: &ProjectModel,
    after: &ProjectModel,
    generation: u64,
    externally_changed_document_ids: BTreeSet<String>,
) -> WorkspacePatch {
    WorkspacePatch {
        workspace_generation: generation,
        workspace_revision: 0,
        upserted_folders: after
            .folders
            .iter()
            .filter(|(id, value)| before.folders.get(*id) != Some(*value))
            .map(|(_, value)| value.clone())
            .collect(),
        removed_folder_ids: before
            .folders
            .keys()
            .filter(|id| !after.folders.contains_key(*id))
            .cloned()
            .collect(),
        upserted_documents: after
            .documents
            .iter()
            .filter(|(id, value)| before.documents.get(*id) != Some(*value))
            .map(|(_, value)| value.clone())
            .collect(),
        removed_document_ids: before
            .documents
            .keys()
            .filter(|id| !after.documents.contains_key(*id))
            .cloned()
            .collect(),
        upserted_links: after
            .links
            .iter()
            .filter(|link| before.links.iter().find(|old| old.id == link.id) != Some(*link))
            .cloned()
            .collect(),
        removed_link_ids: before
            .links
            .iter()
            .filter(|link| !after.links.iter().any(|current| current.id == link.id))
            .map(|link| link.id.clone())
            .collect(),
        externally_changed_document_ids: externally_changed_document_ids.into_iter().collect(),
    }
}

fn is_markdown(path: &Path) -> bool {
    path.extension()
        .and_then(OsStr::to_str)
        .is_some_and(|extension| extension.eq_ignore_ascii_case("md"))
}

fn first_symlink_component(root: &Path, relative: &str) -> Option<String> {
    let mut current = root.to_path_buf();
    let mut normalized = String::new();
    for segment in relative.split('/') {
        current.push(segment);
        normalized = if normalized.is_empty() {
            segment.to_owned()
        } else {
            format!("{normalized}/{segment}")
        };
        if fs::symlink_metadata(&current)
            .map(|metadata| metadata.file_type().is_symlink())
            .unwrap_or(false)
        {
            return Some(normalized);
        }
    }
    None
}

fn parent_path(path: &str) -> &str {
    path.rsplit_once('/').map_or("", |(parent, _)| parent)
}

fn folder_id(path: &str) -> String {
    if path.is_empty() {
        "folder:.".to_owned()
    } else {
        format!("folder:{path}")
    }
}

fn document_id(path: &str) -> String {
    format!("document:{path}")
}

#[cfg(test)]
mod tests {
    use super::{apply_project_changes, ChangeKind, WorkspaceChange};
    use crate::services::workspace::scan_workspace;
    use notify::{RecursiveMode, Watcher};
    use std::{
        fs,
        path::{Path, PathBuf},
        sync::atomic::{AtomicU64, Ordering},
        time::Duration,
    };

    static NEXT_FIXTURE_ID: AtomicU64 = AtomicU64::new(0);

    struct TestWorkspace {
        path: PathBuf,
    }

    impl TestWorkspace {
        fn new() -> Self {
            let id = NEXT_FIXTURE_ID.fetch_add(1, Ordering::Relaxed);
            let path = std::env::temp_dir()
                .join(format!("tracedoc-watcher-test-{}-{id}", std::process::id()));
            fs::create_dir_all(&path).expect("fixture should be created");
            let path = fs::canonicalize(path).expect("fixture should canonicalize");
            Self { path }
        }

        fn path(&self) -> &Path {
            &self.path
        }

        fn change(&self, relative: &str, kind: ChangeKind) -> WorkspaceChange {
            WorkspaceChange {
                path: self.path.join(relative),
                kind,
            }
        }
    }

    impl Drop for TestWorkspace {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.path);
        }
    }

    #[test]
    fn incrementally_creates_modifies_and_deletes_documents() {
        let workspace = TestWorkspace::new();
        fs::write(workspace.path().join("source.md"), "[New](new.md)")
            .expect("source should be written");
        let mut project = scan_workspace(workspace.path()).expect("workspace should scan");
        assert!(!project.links[0].resolved);

        fs::write(workspace.path().join("new.md"), "# New").expect("new file should be written");
        let created = apply_project_changes(
            workspace.path(),
            &mut project,
            &[workspace.change("new.md", ChangeKind::Upsert)],
            1,
        )
        .expect("create should apply");
        assert!(project.documents.contains_key("document:new.md"));
        assert!(project.links[0].resolved);
        assert_eq!(created.externally_changed_document_ids, ["document:new.md"]);

        fs::write(workspace.path().join("new.md"), "# Changed\n\n## Details")
            .expect("new file should change");
        let changed = apply_project_changes(
            workspace.path(),
            &mut project,
            &[
                workspace.change("new.md", ChangeKind::Modify),
                workspace.change("new.md", ChangeKind::Modify),
            ],
            1,
        )
        .expect("change should apply");
        assert_eq!(
            project.documents["document:new.md"].title.as_deref(),
            Some("Changed")
        );
        assert_eq!(changed.externally_changed_document_ids.len(), 1);

        fs::remove_file(workspace.path().join("new.md")).expect("new file should be removed");
        let removed = apply_project_changes(
            workspace.path(),
            &mut project,
            &[workspace.change("new.md", ChangeKind::Remove)],
            1,
        )
        .expect("remove should apply");
        assert!(!project.documents.contains_key("document:new.md"));
        assert!(!project.links[0].resolved);
        assert_eq!(removed.removed_document_ids, ["document:new.md"]);

        fs::write(workspace.path().join("old.md"), "# Old").expect("old file should be written");
        apply_project_changes(
            workspace.path(),
            &mut project,
            &[workspace.change("old.md", ChangeKind::Upsert)],
            1,
        )
        .expect("old file should be added");
        fs::rename(
            workspace.path().join("old.md"),
            workspace.path().join("renamed.md"),
        )
        .expect("file should be renamed");
        apply_project_changes(
            workspace.path(),
            &mut project,
            &[
                workspace.change("old.md", ChangeKind::Reconcile),
                workspace.change("renamed.md", ChangeKind::Reconcile),
            ],
            1,
        )
        .expect("rename should apply");
        assert!(!project.documents.contains_key("document:old.md"));
        assert!(project.documents.contains_key("document:renamed.md"));
        assert_eq!(
            project,
            scan_workspace(workspace.path()).expect("full recovery scan should match")
        );
    }

    #[test]
    fn incrementally_reconciles_folder_moves_and_ignores_hidden_content() {
        let workspace = TestWorkspace::new();
        let mut project = scan_workspace(workspace.path()).expect("workspace should scan");
        fs::create_dir_all(workspace.path().join("old/nested")).expect("folder should be created");
        fs::write(workspace.path().join("old/nested/page.md"), "# Page")
            .expect("page should be written");
        apply_project_changes(
            workspace.path(),
            &mut project,
            &[workspace.change("old", ChangeKind::Upsert)],
            2,
        )
        .expect("folder create should apply");
        assert!(project.folders.contains_key("folder:old/nested"));
        assert!(project
            .documents
            .contains_key("document:old/nested/page.md"));

        fs::rename(
            workspace.path().join("old"),
            workspace.path().join("renamed"),
        )
        .expect("folder should move");
        apply_project_changes(
            workspace.path(),
            &mut project,
            &[
                workspace.change("old", ChangeKind::Reconcile),
                workspace.change("renamed", ChangeKind::Reconcile),
            ],
            2,
        )
        .expect("folder move should apply");
        assert!(!project.folders.contains_key("folder:old"));
        assert!(project.folders.contains_key("folder:renamed/nested"));
        assert!(project
            .documents
            .contains_key("document:renamed/nested/page.md"));
        assert_eq!(
            project,
            scan_workspace(workspace.path()).expect("full recovery scan should match")
        );

        fs::create_dir_all(workspace.path().join(".hidden")).expect("hidden should be created");
        fs::write(workspace.path().join(".hidden/secret.md"), "# Secret")
            .expect("hidden page should be written");
        apply_project_changes(
            workspace.path(),
            &mut project,
            &[workspace.change(".hidden", ChangeKind::Upsert)],
            2,
        )
        .expect("hidden event should be ignored");
        assert!(!project.documents.contains_key("document:.hidden/secret.md"));
        fs::write(workspace.path().join(".notes.md"), "# Notes")
            .expect("dotfile document should be written");
        apply_project_changes(
            workspace.path(),
            &mut project,
            &[workspace.change(".notes.md", ChangeKind::Upsert)],
            2,
        )
        .expect("dotfile event should apply");
        assert!(project.documents.contains_key("document:.notes.md"));
        assert_eq!(
            project,
            scan_workspace(workspace.path())
                .expect("incremental and full ignore policy should match")
        );
    }

    #[test]
    fn ignores_events_outside_the_workspace() {
        let workspace = TestWorkspace::new();
        let outside = TestWorkspace::new();
        fs::write(outside.path().join("outside.md"), "# Outside")
            .expect("outside page should be written");
        let mut project = scan_workspace(workspace.path()).expect("workspace should scan");
        let before = project.clone();
        let patch = apply_project_changes(
            workspace.path(),
            &mut project,
            &[WorkspaceChange {
                path: outside.path().join("outside.md"),
                kind: ChangeKind::Upsert,
            }],
            3,
        )
        .expect("outside event should be ignored");
        assert_eq!(project, before);
        assert!(patch.upserted_documents.is_empty());
    }

    #[test]
    fn contradictory_events_converge_to_the_final_filesystem_state() {
        let workspace = TestWorkspace::new();
        let mut project = scan_workspace(workspace.path()).expect("workspace should scan");
        fs::write(workspace.path().join("atomic.md"), "# Present")
            .expect("atomic file should exist");
        apply_project_changes(
            workspace.path(),
            &mut project,
            &[
                workspace.change("atomic.md", ChangeKind::Upsert),
                workspace.change("atomic.md", ChangeKind::Remove),
            ],
            4,
        )
        .expect("contradictory create remove should reconcile");
        assert!(project.documents.contains_key("document:atomic.md"));

        fs::remove_file(workspace.path().join("atomic.md")).expect("atomic file should be removed");
        apply_project_changes(
            workspace.path(),
            &mut project,
            &[
                workspace.change("atomic.md", ChangeKind::Remove),
                workspace.change("atomic.md", ChangeKind::Upsert),
            ],
            4,
        )
        .expect("contradictory remove create should reconcile");
        assert!(!project.documents.contains_key("document:atomic.md"));
        assert_eq!(
            project,
            scan_workspace(workspace.path()).expect("final model should match full scan")
        );
    }

    #[test]
    fn file_and_folder_type_flips_converge_to_a_full_scan() {
        let workspace = TestWorkspace::new();
        fs::create_dir_all(workspace.path().join("thing.md"))
            .expect("markdown-named folder should be created");
        fs::write(workspace.path().join("thing.md/child.md"), "# Child")
            .expect("child should be written");
        let mut project = scan_workspace(workspace.path()).expect("workspace should scan");

        fs::remove_dir_all(workspace.path().join("thing.md")).expect("folder should be removed");
        fs::write(workspace.path().join("thing.md"), "# File").expect("file should be written");
        apply_project_changes(
            workspace.path(),
            &mut project,
            &[workspace.change("thing.md", ChangeKind::Reconcile)],
            1,
        )
        .expect("folder to file flip should reconcile");
        assert_eq!(
            project,
            scan_workspace(workspace.path()).expect("workspace should rescan")
        );

        fs::remove_file(workspace.path().join("thing.md")).expect("file should be removed");
        fs::create_dir_all(workspace.path().join("thing.md")).expect("folder should be recreated");
        fs::write(workspace.path().join("thing.md/child.md"), "# Again")
            .expect("child should be recreated");
        apply_project_changes(
            workspace.path(),
            &mut project,
            &[workspace.change("thing.md", ChangeKind::Reconcile)],
            1,
        )
        .expect("file to folder flip should reconcile");
        assert_eq!(
            project,
            scan_workspace(workspace.path()).expect("workspace should rescan")
        );
    }

    #[test]
    fn native_watcher_observes_markdown_creation_without_polling() {
        let workspace = TestWorkspace::new();
        let (sender, receiver) = std::sync::mpsc::channel();
        let mut watcher = notify::recommended_watcher(move |event| {
            let _ = sender.send(event);
        })
        .expect("native watcher should initialize");
        watcher
            .watch(workspace.path(), RecursiveMode::Recursive)
            .expect("fixture should be watched");
        let created_path = workspace.path().join("live.md");
        fs::write(&created_path, "# Live").expect("live page should be written");

        let observed = (0..20).any(|_| {
            receiver
                .recv_timeout(Duration::from_millis(250))
                .ok()
                .and_then(Result::ok)
                .is_some_and(|event| event.paths.iter().any(|path| path == &created_path))
        });
        assert!(observed, "native watcher should report the created file");
    }

    #[test]
    fn reconciles_changes_made_between_arming_the_watcher_and_the_initial_scan() {
        use crate::services::document::WorkspaceSession;

        let workspace = TestWorkspace::new();
        fs::write(workspace.path().join("page.md"), "# Page").expect("page should be written");
        fs::write(workspace.path().join("stale.md"), "# Stale")
            .expect("stale file should be written");

        let watcher = super::WorkspaceWatcher;
        let armed = watcher.arm(workspace.path()).expect("watcher should arm");

        // This scan stands in for the initial `open_workspace` scan, which in
        // production now always runs *after* the watch has been armed above.
        let baseline = scan_workspace(workspace.path()).expect("workspace should scan");
        assert!(baseline.documents.contains_key("document:page.md"));
        assert!(baseline.documents.contains_key("document:stale.md"));
        assert!(!baseline.documents.contains_key("document:added.md"));

        // Mutate the workspace after the scan has already produced its
        // snapshot, i.e. exactly the race the P1 regression described: a
        // create, a modify, and a delete that the scan can no longer see.
        fs::write(workspace.path().join("added.md"), "# Added")
            .expect("a file created during the scan window should be written");
        fs::write(workspace.path().join("page.md"), "# Changed")
            .expect("a file modified during the scan window should be written");
        fs::remove_file(workspace.path().join("stale.md"))
            .expect("a file removed during the scan window should be removed");

        // Give the native watcher, armed before the scan started, a moment
        // to actually deliver these events into its channel.
        std::thread::sleep(Duration::from_millis(750));

        let session = WorkspaceSession::default();
        let generation = session
            .activate(workspace.path().to_path_buf(), baseline)
            .expect("workspace session should activate");

        let app = tauri::test::mock_app();
        let handle = app.handle().clone();
        watcher
            .finish(armed, workspace.path(), handle, session.clone(), generation)
            .expect("watcher should finish arming");

        let snapshot = session
            .snapshot(generation)
            .expect("the activated snapshot should be readable");

        assert!(
            snapshot.project.documents.contains_key("document:added.md"),
            "a file created during the scan window must not be lost until a manual refresh"
        );
        assert!(
            !snapshot.project.documents.contains_key("document:stale.md"),
            "a file removed during the scan window must not linger until a manual refresh"
        );
        assert_eq!(
            snapshot.project.documents["document:page.md"]
                .title
                .as_deref(),
            Some("Changed"),
            "a file modified during the scan window must be reconciled without a manual refresh"
        );
    }

    #[test]
    fn a_slow_stale_finish_does_not_disturb_a_faster_winning_watcher() {
        use crate::services::document::WorkspaceSession;

        // Simulates two overlapping `open_workspace` requests (the P1
        // regression): an earlier request (`a`) whose `finish` is delayed -
        // e.g. by scheduling on a busy blocking thread pool - until after a
        // later request (`b`) has already activated and finished installing
        // its own watcher. The earlier request's belated `finish` must not
        // be allowed to replace `b`'s live watcher; if it did, `b`'s watch
        // loop would lose its native watch and stop observing filesystem
        // changes even though `b` remains the active workspace.
        let workspace_a = TestWorkspace::new();
        let workspace_b = TestWorkspace::new();
        fs::write(workspace_a.path().join("only-a.md"), "# A")
            .expect("a's page should be written");
        fs::write(workspace_b.path().join("only-b.md"), "# B")
            .expect("b's page should be written");

        let watcher = super::WorkspaceWatcher;
        let armed_a = watcher.arm(workspace_a.path()).expect("a should arm");
        let project_a = scan_workspace(workspace_a.path()).expect("a should scan");
        let armed_b = watcher.arm(workspace_b.path()).expect("b should arm");
        let project_b = scan_workspace(workspace_b.path()).expect("b should scan");

        let session = WorkspaceSession::default();
        let generation_a = session
            .activate(workspace_a.path().to_path_buf(), project_a)
            .expect("a should activate");
        let generation_b = session
            .activate(workspace_b.path().to_path_buf(), project_b)
            .expect("b should activate");
        assert!(generation_b > generation_a);

        let app = tauri::test::mock_app();
        let handle = app.handle().clone();

        // `b`, the later/winning request, finishes first.
        watcher
            .finish(
                armed_b,
                workspace_b.path(),
                handle.clone(),
                session.clone(),
                generation_b,
            )
            .expect("b should finish");
        // `a`'s belated finish arrives after `b` has already won.
        watcher
            .finish(
                armed_a,
                workspace_a.path(),
                handle,
                session.clone(),
                generation_a,
            )
            .expect("a's stale finish should be a benign no-op");

        // `a` is stale and must be reported as such...
        assert!(session.snapshot(generation_a).is_err());
        // ...while `b` must remain the live, correctly populated workspace.
        let snapshot = session
            .snapshot(generation_b)
            .expect("b should remain the active workspace");
        assert!(snapshot.project.documents.contains_key("document:only-b.md"));

        // The decisive check: b's native watcher must still be delivering
        // live events. Under the pre-fix behavior, `a`'s stale `finish`
        // replaced the single shared "active" watcher slot with its own
        // (already-superseded) watcher, which tore down `b`'s OS-level
        // watch and disconnected its `watch_loop`.
        fs::write(workspace_b.path().join("live.md"), "# Live")
            .expect("a new file should be written to b's workspace");
        let observed = (0..40).any(|_| {
            std::thread::sleep(Duration::from_millis(75));
            session
                .snapshot(generation_b)
                .map(|snapshot| snapshot.project.documents.contains_key("document:live.md"))
                .unwrap_or(false)
        });
        assert!(
            observed,
            "b's watcher must still be live after a's stale finish"
        );
    }

    #[test]
    fn a_failed_reopen_does_not_disturb_the_existing_active_watcher() {
        use crate::services::document::WorkspaceSession;

        // Simulates opening a second workspace whose scan (or activation)
        // fails after the watcher has already been armed (the P2 "keep the
        // current watcher" regression). Before this fix, `arm` unconditionally
        // tore down whatever watcher was previously installed the moment it
        // was called, so a failed reopen silently killed the existing
        // workspace's live updates even though that existing workspace
        // never stopped being the active one.
        let existing = TestWorkspace::new();
        fs::write(existing.path().join("page.md"), "# Page").expect("page should be written");
        let attempted = TestWorkspace::new();

        let watcher = super::WorkspaceWatcher;
        let session = WorkspaceSession::default();

        let armed_existing = watcher.arm(existing.path()).expect("existing should arm");
        let project_existing = scan_workspace(existing.path()).expect("existing should scan");
        let generation_existing = session
            .activate(existing.path().to_path_buf(), project_existing)
            .expect("existing should activate");
        let app = tauri::test::mock_app();
        let handle = app.handle().clone();
        watcher
            .finish(
                armed_existing,
                existing.path(),
                handle,
                session.clone(),
                generation_existing,
            )
            .expect("existing should finish");

        // Arm a second watch for a would-be reopen, then simulate its scan
        // (or activation) failing by simply dropping the armed watcher
        // without ever calling `activate`/`finish` for it - exactly what
        // the real `open_workspace` command does when `scan_canonical_root`
        // returns an error after `arm` has already succeeded.
        let armed_attempt = watcher.arm(attempted.path()).expect("attempt should arm");
        drop(armed_attempt);

        // The pre-existing workspace's watcher must still be alive.
        fs::write(existing.path().join("live.md"), "# Live")
            .expect("a new file should be written to the existing workspace");
        let observed = (0..40).any(|_| {
            std::thread::sleep(Duration::from_millis(75));
            session
                .snapshot(generation_existing)
                .map(|snapshot| snapshot.project.documents.contains_key("document:live.md"))
                .unwrap_or(false)
        });
        assert!(
            observed,
            "the existing workspace's watcher must survive a failed reopen attempt"
        );
    }

    #[test]
    fn buffered_watcher_errors_from_the_scan_window_reach_workspace_watch_error() {
        use crate::services::document::WorkspaceSession;
        use std::sync::{Arc, Mutex};
        use tauri::Listener;

        // Simulates the native backend (e.g. inotify) reporting an error -
        // such as an overflow - while the initial scan is still running,
        // i.e. before `finish` has a chance to drain the buffered channel
        // (the P2 "preserve buffered errors" regression). These errors must
        // reach the same `workspace-watch-error` notification the normal
        // `watch_loop` emits for live errors, not be silently dropped
        // alongside an apparently-successful snapshot.
        let workspace = TestWorkspace::new();
        fs::write(workspace.path().join("page.md"), "# Page").expect("page should be written");

        let (sender, receiver) = std::sync::mpsc::channel();
        let injector = sender.clone();
        let mut native = notify::recommended_watcher(move |event| {
            let _ = sender.send(event);
        })
        .expect("native watcher should initialize");
        native
            .watch(workspace.path(), RecursiveMode::Recursive)
            .expect("fixture should be watched");
        injector
            .send(Err(notify::Error::generic("synthetic overflow")))
            .expect("the synthetic error should buffer");
        let armed = super::ArmedWatcher {
            watcher: native,
            receiver,
            root_identity: None,
        };

        let project = scan_workspace(workspace.path()).expect("workspace should scan");
        let session = WorkspaceSession::default();
        let generation = session
            .activate(workspace.path().to_path_buf(), project)
            .expect("workspace should activate");

        let app = tauri::test::mock_app();
        let handle = app.handle().clone();
        let received: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
        let collector = received.clone();
        handle.listen("workspace-watch-error", move |event| {
            collector
                .lock()
                .expect("lock should not be poisoned")
                .push(event.payload().to_owned());
        });

        let watcher = super::WorkspaceWatcher;
        watcher
            .finish(armed, workspace.path(), handle, session.clone(), generation)
            .expect("finish should succeed despite the buffered error");

        let payloads = received.lock().expect("lock should not be poisoned");
        assert_eq!(
            payloads.len(),
            1,
            "the buffered error should reach exactly one notification, not be dropped"
        );
        assert!(payloads[0].contains("synthetic overflow"));
        assert!(payloads[0].contains(&generation.to_string()));
    }

    #[test]
    fn buffered_watcher_errors_paired_with_a_nonempty_patch_are_stamped_with_the_post_reconciliation_revision(
    ) {
        use crate::services::document::WorkspaceSession;
        use std::sync::{Arc, Mutex};
        use tauri::Listener;

        // The P2 "stamp buffered errors after reconciliation" regression: a
        // scan-window error buffered alongside a scan-window filesystem
        // change that produces a nonempty patch. `finish` must not emit the
        // error with the pre-reconciliation revision - `apply_external_changes`
        // below bumps the revision past it, and `applyWorkspaceError` in
        // `project-state.ts` discards any error whose revision is older
        // than the workspace's current revision as stale.
        let workspace = TestWorkspace::new();
        fs::write(workspace.path().join("page.md"), "# Page").expect("page should be written");

        let (sender, receiver) = std::sync::mpsc::channel();
        let injector = sender.clone();
        let mut native = notify::recommended_watcher(move |event| {
            let _ = sender.send(event);
        })
        .expect("native watcher should initialize");
        native
            .watch(workspace.path(), RecursiveMode::Recursive)
            .expect("fixture should be watched");

        let project = scan_workspace(workspace.path()).expect("workspace should scan");

        fs::write(workspace.path().join("added.md"), "# Added")
            .expect("a file created during the scan window should be written");
        std::thread::sleep(Duration::from_millis(750));

        injector
            .send(Err(notify::Error::generic("synthetic overflow")))
            .expect("the synthetic error should buffer alongside the real change above");

        let armed = super::ArmedWatcher {
            watcher: native,
            receiver,
            root_identity: None,
        };

        let session = WorkspaceSession::default();
        let generation = session
            .activate(workspace.path().to_path_buf(), project)
            .expect("workspace should activate");

        let app = tauri::test::mock_app();
        let handle = app.handle().clone();
        let received: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
        let collector = received.clone();
        handle.listen("workspace-watch-error", move |event| {
            collector
                .lock()
                .expect("lock should not be poisoned")
                .push(event.payload().to_owned());
        });

        let watcher = super::WorkspaceWatcher;
        watcher
            .finish(armed, workspace.path(), handle, session.clone(), generation)
            .expect("finish should succeed despite the buffered error");

        let snapshot = session
            .snapshot(generation)
            .expect("the activated snapshot should be readable");
        assert!(
            snapshot.project.documents.contains_key("document:added.md"),
            "the scan-window change must still be reconciled"
        );
        assert_eq!(
            snapshot.workspace_revision, 2,
            "a nonempty patch produced during finish must bump the revision past its \
             pre-reconciliation value, or this test cannot distinguish the two revisions"
        );

        let payloads = received.lock().expect("lock should not be poisoned");
        assert_eq!(
            payloads.len(),
            1,
            "the buffered error should reach exactly one notification, not be dropped"
        );
        assert!(payloads[0].contains("synthetic overflow"));
        let post_reconciliation_marker =
            format!("\"workspaceRevision\":{}", snapshot.workspace_revision);
        assert!(
            payloads[0].contains(&post_reconciliation_marker),
            "the emitted error must carry the post-reconciliation revision {} so \
             applyWorkspaceError does not discard it as stale: {}",
            snapshot.workspace_revision,
            payloads[0]
        );
    }

    #[test]
    fn watch_loop_stamps_a_live_batch_error_with_the_post_reconciliation_revision_when_paired_with_a_nonempty_patch(
    ) {
        use crate::services::document::WorkspaceSession;
        use std::sync::{Arc, Mutex};
        use tauri::Listener;

        // The same ordering hazard as `finish`'s P2 regression, but for a
        // live batch: `watch_loop`'s own debounce window can coalesce a
        // watcher error together with a real filesystem change into one
        // batch. Stamping the error before `apply_external_changes` bumps
        // the revision would make `applyWorkspaceError` in
        // `project-state.ts` discard it as stale the moment the
        // accompanying patch lands.
        let workspace = TestWorkspace::new();
        fs::write(workspace.path().join("page.md"), "# Page").expect("page should be written");
        let project = scan_workspace(workspace.path()).expect("workspace should scan");

        let session = WorkspaceSession::default();
        let generation = session
            .activate(workspace.path().to_path_buf(), project)
            .expect("workspace should activate");

        let (sender, receiver) = std::sync::mpsc::channel();
        let injector = sender.clone();
        let mut native = notify::recommended_watcher(move |event| {
            let _ = sender.send(event);
        })
        .expect("native watcher should initialize");
        native
            .watch(workspace.path(), RecursiveMode::Recursive)
            .expect("fixture should be watched");

        fs::write(workspace.path().join("added.md"), "# Added")
            .expect("a live file creation should be written");
        std::thread::sleep(Duration::from_millis(750));
        injector
            .send(Err(notify::Error::generic("synthetic overflow")))
            .expect("the synthetic error should batch alongside the real change above");

        let app = tauri::test::mock_app();
        let handle = app.handle().clone();
        let received: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
        let collector = received.clone();
        handle.listen("workspace-watch-error", move |event| {
            collector
                .lock()
                .expect("lock should not be poisoned")
                .push(event.payload().to_owned());
        });

        let loop_session = session.clone();
        let loop_handle = handle.clone();
        std::thread::spawn(move || super::watch_loop(receiver, loop_handle, loop_session, generation));

        let snapshot = (0..40)
            .find_map(|_| {
                std::thread::sleep(Duration::from_millis(75));
                session.snapshot(generation).ok().filter(|snapshot| {
                    snapshot.project.documents.contains_key("document:added.md")
                })
            })
            .expect("the live batch's change must be reconciled");
        assert_eq!(
            snapshot.workspace_revision, 2,
            "a nonempty patch from a live batch must bump the revision past its \
             pre-reconciliation value, or this test cannot distinguish the two revisions"
        );

        let payloads = (0..40)
            .find_map(|_| {
                std::thread::sleep(Duration::from_millis(75));
                let payloads = received.lock().expect("lock should not be poisoned");
                (!payloads.is_empty()).then(|| payloads.clone())
            })
            .expect("the buffered live error must reach a notification");
        assert_eq!(
            payloads.len(),
            1,
            "the buffered live error should reach exactly one notification, not be dropped"
        );
        assert!(payloads[0].contains("synthetic overflow"));
        let post_reconciliation_marker =
            format!("\"workspaceRevision\":{}", snapshot.workspace_revision);
        assert!(
            payloads[0].contains(&post_reconciliation_marker),
            "the emitted live error must carry the post-reconciliation revision {} so \
             applyWorkspaceError does not discard it as stale: {}",
            snapshot.workspace_revision,
            payloads[0]
        );
    }

    #[cfg(unix)]
    #[test]
    fn incremental_reconcile_does_not_follow_symlink_components() {
        use std::os::unix::fs::symlink;

        let workspace = TestWorkspace::new();
        let outside = TestWorkspace::new();
        fs::create_dir_all(workspace.path().join("docs")).expect("docs should exist");
        fs::write(workspace.path().join("docs/page.md"), "# Inside")
            .expect("inside page should exist");
        fs::write(outside.path().join("page.md"), "# Outside").expect("outside page should exist");
        let mut project = scan_workspace(workspace.path()).expect("workspace should scan");
        fs::remove_dir_all(workspace.path().join("docs")).expect("docs should be removed");
        symlink(outside.path(), workspace.path().join("docs"))
            .expect("outside directory should be linked");

        apply_project_changes(
            workspace.path(),
            &mut project,
            &[workspace.change("docs/page.md", ChangeKind::Reconcile)],
            5,
        )
        .expect("symlink event should reconcile conservatively");

        assert!(!project.documents.contains_key("document:docs/page.md"));
        assert_eq!(
            project,
            scan_workspace(workspace.path()).expect("symlink policy should match full scan")
        );
    }

    #[cfg(unix)]
    #[test]
    fn finish_re_arms_when_the_watched_root_is_replaced_at_the_same_path_during_the_scan_window() {
        use crate::services::document::WorkspaceSession;

        // Narrow edge case (low-probability, not ordinary use): between
        // `arm` registering the native watch and `finish` installing it,
        // something external renames the watched directory away and moves
        // a *different* directory into that exact canonical path. Linux
        // inotify stays bound to the object it originally resolved `root`
        // to, not to whatever the path names now, so without revalidation
        // the installed watcher would silently keep watching the
        // renamed-away, no-longer-reachable original while the scan (and
        // the session it activates) already read the replacement. This
        // requires an external directory swap at the exact canonical path
        // during the scan window and is unlikely outside adversarial or
        // heavily scripted environments.
        let workspace = TestWorkspace::new();
        let root = workspace.path().to_path_buf();
        fs::write(root.join("old.md"), "# Old").expect("original file should be written");

        let watcher = super::WorkspaceWatcher;
        let armed = watcher
            .arm(&root)
            .expect("watcher should arm on the original directory");

        let displaced = root.with_file_name(format!(
            "{}-displaced",
            root.file_name()
                .expect("fixture root should have a name")
                .to_string_lossy()
        ));
        fs::rename(&root, &displaced).expect("the original directory should be renamed away");
        fs::create_dir_all(&root)
            .expect("a replacement directory should be created at the same path");
        fs::write(root.join("new.md"), "# New").expect("the replacement file should be written");

        let project = scan_workspace(&root).expect("scan should read the replacement directory");
        assert!(project.documents.contains_key("document:new.md"));
        assert!(!project.documents.contains_key("document:old.md"));

        let session = WorkspaceSession::default();
        let generation = session
            .activate(root.clone(), project)
            .expect("workspace should activate");

        let app = tauri::test::mock_app();
        let handle = app.handle().clone();
        watcher
            .finish(armed, &root, handle, session.clone(), generation)
            .expect("finish should re-arm on the replacement directory");

        fs::write(root.join("live.md"), "# Live")
            .expect("a live file should be written to the replacement directory");
        let observed = (0..40).any(|_| {
            std::thread::sleep(Duration::from_millis(75));
            session
                .snapshot(generation)
                .map(|snapshot| snapshot.project.documents.contains_key("document:live.md"))
                .unwrap_or(false)
        });
        assert!(
            observed,
            "the watcher must track the replacement directory living at root's path, \
             not the renamed-away original it was armed on"
        );

        let _ = fs::remove_dir_all(&displaced);
    }
}
