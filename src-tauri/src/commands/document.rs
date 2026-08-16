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
    workspace_generation: u64,
) -> Result<DocumentIndexUpdate, String> {
    let lease = session
        .capture(workspace_generation)
        .map_err(|error| error.to_string())?;
    let session = session.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        session
            .save_document(&lease, &document_path, &content)
            .map_err(|error| error.to_string())
    })
    .await
    .map_err(|error| format!("The document save could not be completed: {error}"))?
}
