mod commands;
pub mod models;
pub mod services;

pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .invoke_handler(tauri::generate_handler![
            commands::app::get_app_info,
            commands::workspace::open_workspace
        ])
        .run(tauri::generate_context!())
        .expect("error while running TraceDoc");
}
