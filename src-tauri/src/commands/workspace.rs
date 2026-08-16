use crate::{
    models::workspace::WorkspaceSnapshot,
    services::document::WorkspaceSession,
    services::watcher::WorkspaceWatcher,
    services::workspace::{resolve_workspace_root, scan_canonical_root},
};
use std::path::PathBuf;
use tauri::{AppHandle, State};

#[tauri::command]
pub async fn open_workspace<R: tauri::Runtime>(
    app: AppHandle<R>,
    session: State<'_, WorkspaceSession>,
    watcher: State<'_, WorkspaceWatcher>,
    root_path: String,
) -> Result<WorkspaceSnapshot, String> {
    let session = session.inner().clone();
    let watcher = watcher.inner().clone();
    let root_path = PathBuf::from(root_path);

    // Resolve the canonical root and arm the native watch on it *before*
    // scanning, so any change made while the scan is running is queued by
    // the watcher instead of being missed (the scan-to-watch race). The
    // buffered changes are folded back in by `finish` below, right after
    // activation, through the same incremental path used for live events -
    // so this closes the race without paying for a second full scan.
    let scan_watcher = watcher.clone();
    let (armed_watcher, canonical_root, project) =
        tauri::async_runtime::spawn_blocking(move || {
            let canonical_root =
                resolve_workspace_root(&root_path).map_err(|error| error.to_string())?;
            let armed_watcher = scan_watcher.arm(&canonical_root)?;
            let project =
                scan_canonical_root(canonical_root.clone()).map_err(|error| error.to_string())?;
            Ok::<_, String>((armed_watcher, canonical_root, project))
        })
        .await
        .map_err(|error| format!("The workspace scan could not be completed: {error}"))??;

    let workspace_generation = session
        .activate(canonical_root, project)
        .map_err(|error| error.to_string())?;

    let finish_session = session.clone();
    tauri::async_runtime::spawn_blocking(move || {
        watcher.finish(armed_watcher, app, finish_session, workspace_generation)
    })
    .await
    .map_err(|error| format!("The workspace watcher could not start: {error}"))??;

    session
        .snapshot(workspace_generation)
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub async fn refresh_workspace(
    session: State<'_, WorkspaceSession>,
    workspace_generation: u64,
) -> Result<WorkspaceSnapshot, String> {
    let session = session.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        session
            .refresh_workspace(workspace_generation)
            .map_err(|error| error.to_string())
    })
    .await
    .map_err(|error| format!("The workspace refresh could not be completed: {error}"))?
}

#[cfg(test)]
mod tests {
    use super::open_workspace;
    use crate::services::document::WorkspaceSession;
    use crate::services::watcher::WorkspaceWatcher;
    use std::{
        fs,
        path::{Path, PathBuf},
        sync::atomic::{AtomicU64, Ordering},
    };
    use tauri::Manager;

    static NEXT_FIXTURE_ID: AtomicU64 = AtomicU64::new(0);

    struct TestDirectory {
        path: PathBuf,
    }

    impl TestDirectory {
        fn new(label: &str) -> Self {
            let fixture_id = NEXT_FIXTURE_ID.fetch_add(1, Ordering::Relaxed);
            let path = std::env::temp_dir().join(format!(
                "tracedoc-workspace-command-test-{label}-{}-{fixture_id}",
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

    #[test]
    fn open_workspace_arms_the_watcher_before_scanning_and_returns_an_activated_snapshot() {
        let workspace = TestDirectory::new("open-happy-path");
        fs::write(workspace.path().join("page.md"), "# Page").expect("page should be written");

        let app = tauri::test::mock_app();
        app.manage(WorkspaceSession::default());
        app.manage(WorkspaceWatcher::default());
        let session_state = app.state::<WorkspaceSession>();
        let watcher_state = app.state::<WorkspaceWatcher>();
        let handle = app.handle().clone();

        let snapshot = tauri::async_runtime::block_on(open_workspace(
            handle,
            session_state,
            watcher_state,
            workspace.path().to_string_lossy().into_owned(),
        ))
        .expect("workspace should open");

        assert_eq!(snapshot.workspace_revision, 1);
        assert!(snapshot.project.documents.contains_key("document:page.md"));
    }

    #[test]
    fn open_workspace_rejects_a_root_that_does_not_exist() {
        let workspace = TestDirectory::new("open-missing-root");
        let missing = workspace.path().join("missing-child");

        let app = tauri::test::mock_app();
        app.manage(WorkspaceSession::default());
        app.manage(WorkspaceWatcher::default());
        let session_state = app.state::<WorkspaceSession>();
        let watcher_state = app.state::<WorkspaceWatcher>();
        let handle = app.handle().clone();

        let error = tauri::async_runtime::block_on(open_workspace(
            handle,
            session_state,
            watcher_state,
            missing.to_string_lossy().into_owned(),
        ))
        .expect_err("a missing root should be rejected");

        assert!(error.contains("Unable to access workspace"));
    }
}
