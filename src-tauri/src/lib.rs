mod commands;
pub mod models;
pub mod services;

pub fn run() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![commands::app::get_app_info])
        .run(tauri::generate_context!())
        .expect("error while running TraceDoc");
}
