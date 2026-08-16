use crate::{
    models::workspace::WorkspaceSnapshot,
    services::document::{open_workspace as open_workspace_service, WorkspaceSession},
    services::watcher::WorkspaceWatcher,
};
use std::path::PathBuf;
use tauri::{AppHandle, State};

#[tauri::command]
pub async fn open_workspace(
    app: AppHandle,
    session: State<'_, WorkspaceSession>,
    watcher: State<'_, WorkspaceWatcher>,
    root_path: String,
) -> Result<WorkspaceSnapshot, String> {
    let (project, canonical_root) = tauri::async_runtime::spawn_blocking(move || {
        open_workspace_service(&PathBuf::from(root_path))
    })
    .await
    .map_err(|error| format!("The workspace scan could not be completed: {error}"))?
    .map_err(|error| error.to_string())?;
    let workspace_generation = session
        .activate(canonical_root.clone(), project)
        .map_err(|error| error.to_string())?;
    watcher.start(
        app,
        session.inner().clone(),
        canonical_root,
        workspace_generation,
    )?;
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
