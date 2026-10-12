//! App shell (REQ-1): the `popover` and `settings` windows, the tray icon and
//! its menu. Windows are created at launch from `tauri.conf.json`, never
//! destroyed (close = hide), so their event listeners stay attached.

pub mod position;

use std::path::PathBuf;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use tauri::menu::{CheckMenuItem, IsMenuItem, Menu, MenuItem, PredefinedMenuItem, Submenu};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Manager, PhysicalPosition, Rect, WebviewWindow, WindowEvent, Wry};
use tauri_specta::Event;

use crate::api::commands::{self, AppState};
use crate::api::error::{ApiResult, AppError, ErrorCode};
use crate::api::events::{ErrorRaised, PopoverHidden, PopoverShown, SettingsSectionRequested};
use crate::api::types::{CaptureMode, ConnectionState, RoutingMode, SettingsSection};

pub const POPOVER: &str = "popover";
pub const SETTINGS: &str = "settings";
const TRAY_ID: &str = "main";
/// Gap between tray icon / screen edge and the popover, in logical px.
const POPOVER_MARGIN: f64 = 6.0;
/// A tray click this soon after the popover hid on blur is the same click that
/// caused the blur: treat it as "close", not "reopen".
const BLUR_CLICK_DEBOUNCE: Duration = Duration::from_millis(300);

#[derive(Default)]
pub struct ShellState {
    last_blur_hide: Mutex<Option<Instant>>,
    /// Path of the most recent diagnostics export (for `reveal_diagnostics`).
    pub last_diagnostics: Mutex<Option<PathBuf>>,
}

pub fn setup(app: &mut tauri::App) -> tauri::Result<()> {
    // Menu-bar / tray app: no Dock icon on macOS.
    #[cfg(target_os = "macos")]
    app.set_activation_policy(tauri::ActivationPolicy::Accessory);

    let handle = app.handle().clone();
    if let Some(w) = handle.get_webview_window(POPOVER) {
        let _ = w.set_title(edge_core::APP_NAME);
        let h = handle.clone();
        let win = w.clone();
        w.on_window_event(move |e| match e {
            WindowEvent::Focused(false) if win.is_visible().unwrap_or(false) => {
                let _ = hide_popover_window(&h, &win);
                if let Ok(mut t) = h.state::<ShellState>().last_blur_hide.lock() {
                    *t = Some(Instant::now());
                }
            }
            WindowEvent::CloseRequested { api, .. } => {
                api.prevent_close();
                let _ = hide_popover_window(&h, &win);
            }
            _ => {}
        });
    }
    if let Some(w) = handle.get_webview_window(SETTINGS) {
        let _ = w.set_title(&format!("{} Settings", edge_core::APP_NAME));
        let win = w.clone();
        let h = handle.clone();
        w.on_window_event(move |e| {
            if let WindowEvent::CloseRequested { api, .. } = e {
                api.prevent_close();
                let to_tray = h
                    .state::<AppState>()
                    .lock()
                    .map(|st| st.prefs.prefs.app.close_to_tray)
                    .unwrap_or(true);
                if to_tray {
                    let _ = win.hide();
                } else {
                    // Close-to-tray off: closing the window quits (with restore).
                    let h = h.clone();
                    tauri::async_runtime::spawn(async move {
                        let _ = commands::ops::quit(&h).await;
                    });
                }
            }
        });
    }

    let icon = app.default_window_icon().cloned().ok_or_else(|| {
        tauri::Error::AssetNotFound("default window icon (tray)".into())
    })?;
    TrayIconBuilder::with_id(TRAY_ID)
        .icon(icon)
        .tooltip(edge_core::APP_NAME)
        .menu(&build_menu(&handle)?)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| on_menu(app, event.id().as_ref()))
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                rect,
                ..
            } = event
            {
                toggle_popover(tray.app_handle(), rect);
            }
        })
        .build(app)?;
    Ok(())
}

fn window(app: &AppHandle, label: &str) -> ApiResult<WebviewWindow> {
    app.get_webview_window(label)
        .ok_or_else(|| AppError::new(ErrorCode::WindowUnavailable).with_detail(format!("no window '{label}'")))
}

fn toggle_popover(app: &AppHandle, icon: Rect) {
    let Ok(w) = window(app, POPOVER) else { return };
    if w.is_visible().unwrap_or(false) {
        let _ = hide_popover_window(app, &w);
        return;
    }
    let just_hidden = app
        .state::<ShellState>()
        .last_blur_hide
        .lock()
        .ok()
        .and_then(|t| *t)
        .is_some_and(|t| t.elapsed() < BLUR_CLICK_DEBOUNCE);
    if just_hidden {
        return;
    }
    place_popover(app, &w, icon);
    let _ = w.show();
    let _ = w.set_focus();
    let _ = PopoverShown.emit_to(app, POPOVER);
}

fn place_popover(app: &AppHandle, w: &WebviewWindow, icon: Rect) {
    // Tray rects are reported in physical pixels.
    let pos = icon.position.to_physical::<f64>(1.0);
    let size = icon.size.to_physical::<f64>(1.0);
    let Ok(Some(monitor)) = app.monitor_from_point(pos.x + size.width / 2.0, pos.y + size.height / 2.0)
    else {
        return;
    };
    let work = monitor.work_area();
    let Ok(win) = w.outer_size() else { return };
    let (x, y) = position::popover_position(
        position::Rect { x: pos.x, y: pos.y, w: size.width, h: size.height },
        position::Rect {
            x: work.position.x as f64,
            y: work.position.y as f64,
            w: work.size.width as f64,
            h: work.size.height as f64,
        },
        win.width as f64,
        win.height as f64,
        POPOVER_MARGIN * monitor.scale_factor(),
    );
    let _ = w.set_position(PhysicalPosition::new(x as i32, y as i32));
}

pub fn hide_popover(app: &AppHandle) -> ApiResult<()> {
    let w = window(app, POPOVER)?;
    hide_popover_window(app, &w)
}

/// The only place the popover is hidden: hides it if visible and then emits
/// `popoverHidden` to the popover, whatever the cause (blur, tray toggle,
/// `hide_popover`, `open_settings`, close request). No-op if already hidden.
fn hide_popover_window(app: &AppHandle, w: &WebviewWindow) -> ApiResult<()> {
    if !w.is_visible().unwrap_or(false) {
        return Ok(());
    }
    w.hide().map_err(|e| AppError::new(ErrorCode::WindowUnavailable).with_detail(e.to_string()))?;
    let _ = PopoverHidden.emit_to(app, POPOVER);
    Ok(())
}

pub fn open_settings(app: &AppHandle, section: Option<SettingsSection>) -> ApiResult<()> {
    if let Ok(p) = window(app, POPOVER) {
        let _ = hide_popover_window(app, &p);
    }
    let w = window(app, SETTINGS)?;
    let fail = |e: tauri::Error| AppError::new(ErrorCode::WindowUnavailable).with_detail(e.to_string());
    w.show().map_err(fail)?;
    let _ = w.unminimize();
    w.set_focus().map_err(fail)?;
    if let Some(section) = section {
        let _ = SettingsSectionRequested { section }.emit_to(app, SETTINGS);
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Tray menu
// ---------------------------------------------------------------------------

fn state_label(s: ConnectionState) -> &'static str {
    match s {
        ConnectionState::Disconnected => "Disconnected",
        ConnectionState::Connecting => "Connecting…",
        ConnectionState::Connected => "Connected",
        ConnectionState::Degraded => "Connected (degraded)",
        ConnectionState::FailingOver => "Switching node…",
        ConnectionState::Reconnecting => "Reconnecting…",
        ConnectionState::Direct => "Direct (no proxy)",
        ConnectionState::Disconnecting => "Disconnecting…",
        ConnectionState::Recovering => "Recovering…",
        ConnectionState::Error => "Error",
    }
}

fn is_active(s: ConnectionState) -> bool {
    !matches!(s, ConnectionState::Disconnected)
}

fn build_menu(app: &AppHandle) -> tauri::Result<Menu<Wry>> {
    let (status, nodes) = {
        let st = app.state::<AppState>();
        let g = st.lock().unwrap_or_else(|p| p.into_inner());
        (g.status.clone(), g.nodes.list())
    };
    let active = is_active(status.state);
    let shown_node = status.active_node_id.as_ref().or(status.selected_node_id.as_ref());
    let node_name = shown_node
        .and_then(|id| nodes.iter().find(|n| &n.id == id))
        .map(|n| n.name.as_str())
        .unwrap_or("none");

    let header = MenuItem::with_id(
        app,
        "header",
        format!("{} — {}", edge_core::APP_NAME, state_label(status.state)),
        false,
        None::<&str>,
    )?;
    let toggle = if active {
        MenuItem::with_id(app, "disconnect", "Disconnect", true, None::<&str>)?
    } else {
        MenuItem::with_id(app, "connect", "Connect", true, None::<&str>)?
    };

    let node_items = nodes
        .iter()
        .map(|n| {
            CheckMenuItem::with_id(
                app,
                format!("node:{}", n.id),
                &n.name,
                true,
                status.selected_node_id.as_deref() == Some(n.id.as_str()),
                None::<&str>,
            )
        })
        .collect::<tauri::Result<Vec<_>>>()?;
    let node_refs: Vec<&dyn IsMenuItem<Wry>> = node_items.iter().map(|i| i as &dyn IsMenuItem<Wry>).collect();
    let nodes_menu = Submenu::with_id_and_items(app, "nodes", format!("Node: {node_name}"), !nodes.is_empty(), &node_refs)?;

    let check = |id: &str, label: &str, on: bool| CheckMenuItem::with_id(app, id, label, true, on, None::<&str>);
    let routing = Submenu::with_id_and_items(
        app,
        "routing",
        "Routing mode",
        true,
        &[
            &check("routing:rule", "Rule", status.routing_mode == RoutingMode::Rule)?,
            &check("routing:global", "Global", status.routing_mode == RoutingMode::Global)?,
            &check("routing:direct", "Direct", status.routing_mode == RoutingMode::Direct)?,
        ],
    )?;
    let capture = Submenu::with_id_and_items(
        app,
        "capture",
        "Capture mode",
        true,
        &[
            &check("capture:system_proxy", "System proxy", status.capture_mode == CaptureMode::SystemProxy)?,
            &check("capture:tun", "TUN (whole device)", status.capture_mode == CaptureMode::Tun)?,
        ],
    )?;

    Menu::with_items(
        app,
        &[
            &header,
            &toggle,
            &PredefinedMenuItem::separator(app)?,
            &nodes_menu,
            &routing,
            &capture,
            &PredefinedMenuItem::separator(app)?,
            &MenuItem::with_id(app, "restore", "Restore networking", true, None::<&str>)?,
            &MenuItem::with_id(app, "settings", "Settings…", true, None::<&str>)?,
            &PredefinedMenuItem::separator(app)?,
            &MenuItem::with_id(app, "quit", format!("Quit {}", edge_core::APP_NAME), true, None::<&str>)?,
        ],
    )
}

/// Rebuild the tray menu from current state. Must be called without holding
/// the `AppState` lock.
pub fn refresh_tray(app: &AppHandle) {
    if let Some(tray) = app.tray_by_id(TRAY_ID)
        && let Ok(menu) = build_menu(app)
    {
        let _ = tray.set_menu(Some(menu));
    }
}

fn on_menu(app: &AppHandle, id: &str) {
    let app = app.clone();
    let id = id.to_string();
    tauri::async_runtime::spawn(async move {
        let result: ApiResult<()> = match id.as_str() {
            "connect" => commands::ops::connect(&app).await.map(drop),
            "disconnect" => commands::ops::disconnect(&app).await.map(drop),
            "restore" => commands::ops::restore_networking(&app).await.map(drop),
            "settings" => open_settings(&app, None),
            "quit" => commands::ops::quit(&app).await,
            "routing:rule" => commands::ops::set_routing_mode_live(&app, RoutingMode::Rule).await.map(drop),
            "routing:global" => commands::ops::set_routing_mode_live(&app, RoutingMode::Global).await.map(drop),
            "routing:direct" => commands::ops::set_routing_mode_live(&app, RoutingMode::Direct).await.map(drop),
            "capture:system_proxy" => commands::ops::set_capture_mode_live(&app, CaptureMode::SystemProxy).await.map(drop),
            "capture:tun" => commands::ops::set_capture_mode_live(&app, CaptureMode::Tun).await.map(drop),
            other => match other.strip_prefix("node:") {
                Some(node) => commands::ops::select_node(&app, node.to_string()).await.map(drop),
                None => Ok(()),
            },
        };
        if let Err(error) = result {
            let _ = ErrorRaised { error }.emit(&app);
        }
        // Check items toggle themselves on click; rebuild so they show real state.
        refresh_tray(&app);
    });
}
