use serde::Serialize;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppInfo {
    name: &'static str,
    version: &'static str,
}

#[tauri::command]
pub fn get_app_info() -> AppInfo {
    AppInfo {
        name: "TraceDoc",
        version: env!("CARGO_PKG_VERSION"),
    }
}

#[cfg(test)]
mod tests {
    use super::get_app_info;

    #[test]
    fn reports_the_packaged_product_identity() {
        let info = get_app_info();
        assert_eq!(info.name, "TraceDoc");
        assert_eq!(info.version, env!("CARGO_PKG_VERSION"));
    }
}
