pub mod api;
pub mod core_bin;
pub mod net;
pub mod rules;
mod shell;
mod state;
mod store;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let builder = api::specta_builder();

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_clipboard_manager::init())
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            Some(vec!["--autostart"]),
        ))
        .manage(net::session::OpLock(tokio::sync::Mutex::new(())))
        .manage(shell::ShellState::default())
        .invoke_handler(builder.invoke_handler())
        .setup(move |app| {
            load_state(app)?;
            builder.mount_events(app);
            shell::setup(app)?;
            net::session::recover_at_launch(app.handle());
            net::session::auto_connect_at_launch(app.handle());
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
    let data_dir = app.path().app_data_dir()?;
    net::logbuf::global().set_file_dir(&data_dir.join("logs"));
    let (mut data, mut warnings) = state::AppData::load(&dir, secrets);
    data.data_dir = data_dir;
    let active_rules = app.path().app_data_dir()?.join("rules");
    data.rules = rules::ensure_active(
        &rules::embedded_manifest(),
        &rules::bundled_dir(app.path().resource_dir().ok()),
        &active_rules,
    );
    if let Err(e) = &data.rules {
        warnings.push(e.clone());
    }
    app.manage(api::commands::AppState::new(data));
    for w in warnings {
        net::logbuf::app(
            api::types::LogLevel::Warn,
            format!("startup: {:?}: {}", w.code, w.detail.as_deref().unwrap_or("")),
        );
        let _ = tauri_specta::Event::emit(&api::events::ErrorRaised { error: w }, app.handle());
    }
    Ok(())
}

/// `--restore-networking` entry point (no UI). Returns the process exit code.
pub fn restore_networking_cli() -> i32 {
    net::session::restore_cli()
}
