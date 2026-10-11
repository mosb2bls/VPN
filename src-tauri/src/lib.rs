pub mod api;
mod shell;
mod stub;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let builder = api::specta_builder();

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .manage(api::commands::AppState::default())
        .manage(shell::ShellState::default())
        .invoke_handler(builder.invoke_handler())
        .setup(move |app| {
            builder.mount_events(app);
            shell::setup(app)?;
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
