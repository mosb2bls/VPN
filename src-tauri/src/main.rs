// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    if std::env::args().any(|a| a == "--restore-networking") {
        std::process::exit(edge_app_lib::restore_networking_cli());
    }
    if std::env::args().any(|a| a == "--helper-status") {
        std::process::exit(edge_app_lib::helper_status_cli());
    }
    edge_app_lib::run()
}
