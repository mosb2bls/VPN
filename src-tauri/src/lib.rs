pub mod api;
mod stub;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let builder = api::specta_builder();

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .manage(api::commands::AppState::default())
        .invoke_handler(builder.invoke_handler())
        .setup(move |app| {
            use tauri::Manager;
            builder.mount_events(app);
            if let Some(w) = app.get_webview_window("main") {
                let _ = w.set_title(edge_core::APP_NAME);
            }
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
