mod commands;
pub mod models;
pub mod services;

pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .manage(services::document::WorkspaceSession::default())
        .manage(services::watcher::WorkspaceWatcher)
        .invoke_handler(tauri::generate_handler![
            commands::app::get_app_info,
            commands::workspace::open_workspace,
            commands::workspace::refresh_workspace,
            commands::document::read_document,
            commands::document::write_document,
            commands::document::acknowledge_save_conflict
        ])
        .run(tauri::generate_context!())
        .expect("error while running TraceDoc");
}
