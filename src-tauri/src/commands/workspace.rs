use crate::{
    models::workspace::ProjectModel,
    services::document::{open_workspace as open_workspace_service, WorkspaceSession},
};
use std::path::PathBuf;
use tauri::State;

#[tauri::command]
pub async fn open_workspace(
    session: State<'_, WorkspaceSession>,
    root_path: String,
) -> Result<ProjectModel, String> {
    let (project, canonical_root) = tauri::async_runtime::spawn_blocking(move || {
        open_workspace_service(&PathBuf::from(root_path))
    })
    .await
    .map_err(|error| format!("The workspace scan could not be completed: {error}"))?
    .map_err(|error| error.to_string())?;
    session
        .activate(canonical_root)
        .map_err(|error| error.to_string())?;
    Ok(project)
}
