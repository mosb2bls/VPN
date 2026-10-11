pub mod api;
mod shell;
mod state;
mod store;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let builder = api::specta_builder();

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_clipboard_manager::init())
        .manage(shell::ShellState::default())
        .invoke_handler(builder.invoke_handler())
        .setup(move |app| {
            load_state(app)?;
            builder.mount_events(app);
            shell::setup(app)?;
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

/// Load persisted nodes / preferences and install the keychain-backed secret
/// store. Load problems are reported (not fatal): the app starts with what it
/// could read.
fn load_state(app: &mut tauri::App) -> Result<(), Box<dyn std::error::Error>> {
    use tauri::Manager;
    let dir = app.path().app_config_dir()?;
    let secrets: Box<dyn store::secrets::SecretStore> =
        match store::secrets::KeychainStore::init(app.config().identifier.as_str()) {
            Ok(k) => Box::new(k),
            Err(e) => Box::new(store::secrets::Unavailable(e)),
        };
    let (data, warnings) = state::AppData::load(&dir, secrets);
    app.manage(api::commands::AppState::new(data));
    for w in warnings {
        // TODO(B2 logging): also record in the redacted log.
        eprintln!("startup warning: {:?}: {}", w.code, w.detail.as_deref().unwrap_or(""));
        let _ = tauri_specta::Event::emit(&api::events::ErrorRaised { error: w }, app.handle());
    }
    Ok(())
}
