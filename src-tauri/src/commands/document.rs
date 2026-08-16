use crate::models::workspace::{DocumentIndexUpdate, DocumentReadResult};
use crate::services::document::WorkspaceSession;
use tauri::State;

#[tauri::command]
pub async fn read_document(
    session: State<'_, WorkspaceSession>,
    document_path: String,
    workspace_generation: u64,
) -> Result<DocumentReadResult, String> {
    let lease = session
        .capture(workspace_generation)
        .map_err(|error| error.to_string())?;
    let session = session.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        session
            .read_document(&lease, &document_path)
            .map_err(|error| error.to_string())
    })
    .await
    .map_err(|error| format!("The document read could not be completed: {error}"))?
}

#[tauri::command]
pub async fn write_document(
    session: State<'_, WorkspaceSession>,
    document_path: String,
    content: String,
    expected_content_token: String,
    expected_workspace_revision: u64,
    workspace_generation: u64,
) -> Result<DocumentIndexUpdate, String> {
    let lease = session
        .capture(workspace_generation)
        .map_err(|error| error.to_string())?;
    let session = session.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        session
            .save_document(
                &lease,
                &document_path,
                &content,
                &expected_content_token,
                expected_workspace_revision,
            )
            .map_err(|error| error.to_string())
    })
    .await
    .map_err(|error| format!("The document save could not be completed: {error}"))?
}

#[tauri::command]
pub async fn acknowledge_save_conflict(
    session: State<'_, WorkspaceSession>,
    document_path: String,
    workspace_generation: u64,
) -> Result<(), String> {
    let lease = session
        .capture(workspace_generation)
        .map_err(|error| error.to_string())?;
    let session = session.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        session
            .acknowledge_save_conflict(&lease, &document_path)
            .map_err(|error| error.to_string())
    })
    .await
    .map_err(|error| format!("The save conflict could not be acknowledged: {error}"))?
}

#[cfg(test)]
mod tests {
    use super::{acknowledge_save_conflict, read_document, write_document};
    use crate::services::document::{open_workspace, WorkspaceSession};
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
                "tracedoc-document-command-test-{label}-{}-{fixture_id}",
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

    fn activated_session(workspace: &Path) -> (WorkspaceSession, u64) {
        let (project, root) = open_workspace(workspace).expect("workspace should open");
        let session = WorkspaceSession::default();
        let generation = session
            .activate(root, project)
            .expect("workspace session should activate");
        (session, generation)
    }

    #[test]
    fn write_document_persists_content_and_updates_the_index() {
        let workspace = TestDirectory::new("save-success");
        fs::write(workspace.path().join("page.md"), "# Original")
            .expect("document should be written");
        let (session, generation) = activated_session(workspace.path());
        let app = tauri::test::mock_app();
        app.manage(session);
        let state = app.state::<WorkspaceSession>();

        let read = tauri::async_runtime::block_on(read_document(
            state.clone(),
            "page.md".to_owned(),
            generation,
        ))
        .expect("document should read through the native command");

        let update = tauri::async_runtime::block_on(write_document(
            state,
            "page.md".to_owned(),
            "# Changed".to_owned(),
            read.content_token,
            read.workspace_revision,
            generation,
        ))
        .expect("a writable document should save through the native command");

        assert_eq!(update.document.title.as_deref(), Some("Changed"));
        assert!(update.workspace_revision > read.workspace_revision);
        assert_eq!(
            fs::read_to_string(workspace.path().join("page.md"))
                .expect("saved document should be readable"),
            "# Changed"
        );
    }

    #[test]
    fn write_document_reports_the_external_change_conflict_distinctly() {
        let workspace = TestDirectory::new("save-conflict");
        fs::write(workspace.path().join("page.md"), "# Original")
            .expect("document should be written");
        let (session, generation) = activated_session(workspace.path());
        let app = tauri::test::mock_app();
        app.manage(session);
        let state = app.state::<WorkspaceSession>();

        let error = tauri::async_runtime::block_on(write_document(
            state,
            "page.md".to_owned(),
            "# Changed".to_owned(),
            "stale-token".to_owned(),
            1,
            generation,
        ))
        .expect_err("a stale content token should be rejected as a conflict");

        assert!(error.contains("changed externally"));
    }

    #[test]
    fn write_document_reports_an_actionable_message_after_the_workspace_changes() {
        let first = TestDirectory::new("save-stale-generation-first");
        let second = TestDirectory::new("save-stale-generation-second");
        fs::write(first.path().join("page.md"), "# Original").expect("document should be written");
        let (session, first_generation) = activated_session(first.path());
        let (second_project, second_root) =
            open_workspace(second.path()).expect("second workspace should open");
        session
            .activate(second_root, second_project)
            .expect("second workspace should activate");
        let app = tauri::test::mock_app();
        app.manage(session);
        let state = app.state::<WorkspaceSession>();

        let error = tauri::async_runtime::block_on(write_document(
            state,
            "page.md".to_owned(),
            "# Changed".to_owned(),
            "any-token".to_owned(),
            1,
            first_generation,
        ))
        .expect_err("a stale workspace generation should be rejected");

        assert!(error.contains("workspace changed"));
        assert!(!error.contains("changed externally"));
    }

    #[test]
    fn acknowledge_save_conflict_command_resolves_without_a_pending_conflict() {
        let workspace = TestDirectory::new("acknowledge");
        fs::write(workspace.path().join("page.md"), "# Original")
            .expect("document should be written");
        let (session, generation) = activated_session(workspace.path());
        let app = tauri::test::mock_app();
        app.manage(session);
        let state = app.state::<WorkspaceSession>();

        tauri::async_runtime::block_on(acknowledge_save_conflict(
            state,
            "page.md".to_owned(),
            generation,
        ))
        .expect("acknowledging without a pending conflict should be a no-op");
    }
}
