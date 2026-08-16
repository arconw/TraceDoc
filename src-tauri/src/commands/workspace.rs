use crate::{models::workspace::ProjectModel, services::workspace::scan_workspace};
use std::path::PathBuf;

#[tauri::command]
pub async fn open_workspace(root_path: String) -> Result<ProjectModel, String> {
    tauri::async_runtime::spawn_blocking(move || scan_workspace(&PathBuf::from(root_path)))
        .await
        .map_err(|error| format!("The workspace scan could not be completed: {error}"))?
        .map_err(|error| error.to_string())
}
