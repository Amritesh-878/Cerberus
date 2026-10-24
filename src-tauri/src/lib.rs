#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

#[cfg(test)]
mod tests {
    use serde_json::Value;

    #[test]
    fn config_declares_one_main_window() {
        let config: Value = serde_json::from_str(include_str!("../tauri.conf.json")).unwrap();
        let windows = config["app"]["windows"].as_array().unwrap();

        assert_eq!(windows.len(), 1);
        assert_eq!(windows[0]["title"], "Cerberus");
    }
}
