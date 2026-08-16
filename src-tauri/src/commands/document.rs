use crate::services::document::{
    read_document as read_workspace_document, write_document as write_workspace_document,
    WorkspaceSession,
};
use tauri::State;

#[tauri::command]
pub async fn read_document(
    session: State<'_, WorkspaceSession>,
    document_path: String,
) -> Result<String, String> {
    let root = session.root().map_err(|error| error.to_string())?;
    tauri::async_runtime::spawn_blocking(move || {
        read_workspace_document(&root, &document_path).map_err(|error| error.to_string())
    })
    .await
    .map_err(|error| format!("The document read could not be completed: {error}"))?
}

#[tauri::command]
pub async fn write_document(
    session: State<'_, WorkspaceSession>,
    document_path: String,
    content: String,
) -> Result<(), String> {
    let root = session.root().map_err(|error| error.to_string())?;
    tauri::async_runtime::spawn_blocking(move || {
        write_workspace_document(&root, &document_path, &content).map_err(|error| error.to_string())
    })
    .await
    .map_err(|error| format!("The document save could not be completed: {error}"))?
}
