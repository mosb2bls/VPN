//! Tray / menu-bar state icons (owner's icon set, delivered with
//! `Icon/DEVELOPER_PROMPT.md`; assets in `src-tauri/icons/tray/`).
//!
//! Four states derived from the connection state machine — off, connecting,
//! connected, blocked ("connection unavailable"; it does not claim traffic is
//! blocked). The transport (HY2 / REALITY) and routing mode never change the
//! icon. While connecting, frames 0→1→2→3 play at 200 ms each unless the user
//! reduced motion; any state change stops the animation at once.
//!
//! macOS: 44 px black template PNGs, shown at 22 × 22 pt as template images
//! (the system picks the menu-bar colour). Windows: ICOs with 16/20/24/32 px
//! frames; the frame matching the tray's small-icon size is used, black on a
//! light taskbar, white on a dark one, refreshed when the theme or DPI changes.

use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use tauri::image::Image;
use tauri::tray::TrayIcon;
use tauri::{AppHandle, Wry};

use crate::api::error::ErrorCode;
use crate::api::types::{ConnectionState, Status};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrayState {
    Off,
    Connecting,
    Connected,
    Blocked,
}

/// Icon state for a status. Pure.
pub fn tray_state(s: &Status) -> TrayState {
    use ConnectionState::*;
    match s.state {
        // No tunnel: not started, disconnected, or the user's Direct mode.
        Disconnected | Disconnecting | Direct => TrayState::Off,
        Connecting | Reconnecting | FailingOver | Recovering => TrayState::Connecting,
        Connected => TrayState::Connected,
        Degraded => {
            let unavailable = s.failover.as_ref().is_some_and(|f| f.direct)
                || s.last_error.as_ref().is_some_and(|e| {
                    matches!(e.code, ErrorCode::AllNodesFailed | ErrorCode::DirectFallbackActive | ErrorCode::HelperUnreachable)
                });
            if unavailable { TrayState::Blocked } else { TrayState::Connected }
        }
        Error => TrayState::Blocked,
    }
}

pub fn tooltip(t: TrayState) -> String {
    let word = match t {
        TrayState::Off => "Off",
        TrayState::Connecting => "Connecting",
        TrayState::Connected => "Connected",
        TrayState::Blocked => "Connection unavailable",
    };
    format!("{} — {word}", edge_core::APP_NAME)
}

/// Which artwork: a state, or a connecting animation frame (0–3).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Art {
    State(TrayState),
    Frame(u8),
}

const FRAME_MS: u64 = 200;

struct Shown {
    state: Option<TrayState>,
    /// (light taskbar, icon px) the Windows icon was rendered for.
    #[allow(dead_code)]
    theme: (bool, u32),
}

static SHOWN: Mutex<Shown> = Mutex::new(Shown { state: None, theme: (false, 0) });
/// Bumped on every state change; an animation thread exits when it differs.
static ANIM_GEN: AtomicU64 = AtomicU64::new(0);

/// Apply the icon + tooltip for `status` (no-op if the state is unchanged).
pub fn apply(app: &AppHandle, tray: &TrayIcon<Wry>, status: &Status) {
    let t = tray_state(status);
    {
        let mut shown = SHOWN.lock().unwrap_or_else(|p| p.into_inner());
        if shown.state == Some(t) {
            return;
        }
        shown.state = Some(t);
    }
    let generation = ANIM_GEN.fetch_add(1, Ordering::SeqCst) + 1;
    let _ = tray.set_tooltip(Some(tooltip(t)));
    set_art(tray, Art::State(t));
    if t == TrayState::Connecting && !reduced_motion() {
        let app = app.clone();
        std::thread::spawn(move || {
            let mut frame = 0u8;
            loop {
                std::thread::sleep(Duration::from_millis(FRAME_MS));
                if ANIM_GEN.load(Ordering::SeqCst) != generation {
                    return; // state changed: stop
                }
                let Some(tray) = app.tray_by_id(super::TRAY_ID) else { return };
                set_art(&tray, Art::Frame(frame));
                frame = (frame + 1) % 4;
            }
        });
    }
}

/// Re-render the current artwork (Windows: after a theme / DPI change).
#[cfg(windows)]
fn rerender(tray: &TrayIcon<Wry>) {
    let state = SHOWN.lock().unwrap_or_else(|p| p.into_inner()).state;
    if let Some(t) = state {
        // An animation (if any) overwrites this with the next frame itself.
        set_art(tray, Art::State(t));
    }
}

/// Windows: watch the taskbar theme and DPI; re-render when they change.
pub fn start_theme_watch(app: &AppHandle) {
    #[cfg(windows)]
    {
        let app = app.clone();
        std::thread::spawn(move || {
            loop {
                std::thread::sleep(Duration::from_secs(2));
                let now = (edge_winsys::ui::taskbar_is_light(), edge_winsys::ui::tray_icon_px());
                let changed = {
                    let mut shown = SHOWN.lock().unwrap_or_else(|p| p.into_inner());
                    let c = shown.theme != now;
                    shown.theme = now;
                    c
                };
                if changed && let Some(tray) = app.tray_by_id(super::TRAY_ID) {
                    rerender(&tray);
                }
            }
        });
    }
    #[cfg(not(windows))]
    let _ = app;
}

fn reduced_motion() -> bool {
    #[cfg(windows)]
    {
        edge_winsys::ui::reduced_motion()
    }
    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("/usr/bin/defaults")
            .args(["read", "com.apple.universalaccess", "reduceMotion"])
            .output()
            .is_ok_and(|o| String::from_utf8_lossy(&o.stdout).trim() == "1")
    }
    #[cfg(not(any(windows, target_os = "macos")))]
    {
        true
    }
}

// --- assets -------------------------------------------------------------------

macro_rules! tray_asset {
    ($name:literal, $suffix:literal) => {
        include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/icons/tray/lynnk-tray-", $name, $suffix))
    };
}

#[cfg(target_os = "macos")]
fn asset(art: Art) -> &'static [u8] {
    match art {
        Art::State(TrayState::Off) => tray_asset!("off", "@2x.png"),
        Art::State(TrayState::Connecting) => tray_asset!("connecting", "@2x.png"),
        Art::State(TrayState::Connected) => tray_asset!("connected", "@2x.png"),
        Art::State(TrayState::Blocked) => tray_asset!("blocked", "@2x.png"),
        Art::Frame(0) => tray_asset!("connecting-frame-0", "@2x.png"),
        Art::Frame(1) => tray_asset!("connecting-frame-1", "@2x.png"),
        Art::Frame(2) => tray_asset!("connecting-frame-2", "@2x.png"),
        Art::Frame(_) => tray_asset!("connecting-frame-3", "@2x.png"),
    }
}

#[cfg(windows)]
fn asset(art: Art, light: bool) -> &'static [u8] {
    macro_rules! pick {
        ($name:literal) => {
            if light { tray_asset!($name, "-black.ico") } else { tray_asset!($name, "-white.ico") }
        };
    }
    match art {
        Art::State(TrayState::Off) => pick!("off"),
        Art::State(TrayState::Connecting) => pick!("connecting"),
        Art::State(TrayState::Connected) => pick!("connected"),
        Art::State(TrayState::Blocked) => pick!("blocked"),
        Art::Frame(0) => pick!("connecting-frame-0"),
        Art::Frame(1) => pick!("connecting-frame-1"),
        Art::Frame(2) => pick!("connecting-frame-2"),
        Art::Frame(_) => pick!("connecting-frame-3"),
    }
}

/// Decode a 44 px template PNG to RGBA.
#[cfg(target_os = "macos")]
fn decode_png(bytes: &[u8]) -> Option<(Vec<u8>, u32, u32)> {
    let mut decoder = png::Decoder::new(std::io::Cursor::new(bytes));
    decoder.set_transformations(png::Transformations::EXPAND | png::Transformations::ALPHA);
    let mut reader = decoder.read_info().ok()?;
    let mut buf = vec![0; reader.output_buffer_size()];
    let info = reader.next_frame(&mut buf).ok()?;
    buf.truncate(info.buffer_size());
    (info.color_type == png::ColorType::Rgba && info.bit_depth == png::BitDepth::Eight).then_some((buf, info.width, info.height))
}

/// The ICO frame for `px` (exact size, else the smallest larger, else the largest).
#[cfg(windows)]
fn decode_ico(bytes: &[u8], px: u32) -> Option<(Vec<u8>, u32, u32)> {
    let dir = ico::IconDir::read(std::io::Cursor::new(bytes)).ok()?;
    let entries = dir.entries();
    let best = entries
        .iter()
        .filter(|e| e.width() >= px)
        .min_by_key(|e| e.width())
        .or_else(|| entries.iter().max_by_key(|e| e.width()))?;
    let img = best.decode().ok()?;
    Some((img.rgba_data().to_vec(), img.width(), img.height()))
}

fn set_art(tray: &TrayIcon<Wry>, art: Art) {
    #[cfg(target_os = "macos")]
    {
        let Some((rgba, w, h)) = decode_png(asset(art)) else { return };
        let _ = tray.set_icon_as_template(true);
        let _ = tray.set_icon(Some(Image::new_owned(rgba, w, h)));
        // tray-icon sizes menu-bar images to 18 pt; the artwork is drawn for 22 × 22 pt.
        let _ = tray.with_inner_tray_icon(|inner| {
            use objc2::MainThreadMarker;
            use objc2_foundation::NSSize;
            let Some(mtm) = MainThreadMarker::new() else { return };
            if let Some(item) = inner.ns_status_item()
                && let Some(button) = item.button(mtm)
                && let Some(image) = button.image()
            {
                image.setSize(NSSize::new(22.0, 22.0));
                button.setImage(Some(&image));
            }
        });
    }
    #[cfg(windows)]
    {
        let (light, px) = (edge_winsys::ui::taskbar_is_light(), edge_winsys::ui::tray_icon_px());
        SHOWN.lock().unwrap_or_else(|p| p.into_inner()).theme = (light, px);
        let Some((rgba, w, h)) = decode_ico(asset(art, light), px) else { return };
        let _ = tray.set_icon(Some(Image::new_owned(rgba, w, h)));
    }
    #[cfg(not(any(windows, target_os = "macos")))]
    let _ = (tray, art);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::error::AppError;
    use crate::api::types::FailoverStatus;

    fn status(state: ConnectionState) -> Status {
        use crate::api::types::{CaptureMode, DnsProfile, RoutingMode};
        Status {
            state,
            capture_mode: CaptureMode::Tun,
            routing_mode: RoutingMode::Rule,
            dns_profile: DnsProfile::Mainland,
            selected_node_id: None,
            active_node_id: None,
            connected_since: None,
            kill_switch_active: false,
            system_proxy_set: false,
            failover: None,
            last_error: None,
            last_recovery: None,
            tun_active: false,
            last_restore: None,
        }
    }

    #[test]
    fn states_map_to_four_icons() {
        use ConnectionState::*;
        for (s, t) in [
            (Disconnected, TrayState::Off),
            (Disconnecting, TrayState::Off),
            (Direct, TrayState::Off),
            (Connecting, TrayState::Connecting),
            (Reconnecting, TrayState::Connecting),
            (FailingOver, TrayState::Connecting),
            (Recovering, TrayState::Connecting),
            (Connected, TrayState::Connected),
            (Degraded, TrayState::Connected),
            (Error, TrayState::Blocked),
        ] {
            assert_eq!(tray_state(&status(s)), t, "{s:?}");
        }
        let mut all_failed = status(Degraded);
        all_failed.last_error = Some(AppError::new(ErrorCode::AllNodesFailed));
        assert_eq!(tray_state(&all_failed), TrayState::Blocked);
        let mut direct = status(Degraded);
        direct.failover = Some(FailoverStatus {
            primary_node_id: "a".into(),
            fallback_node_id: String::new(),
            since: String::new(),
            reason: String::new(),
            earliest_return_at: None,
            direct: true,
        });
        assert_eq!(tray_state(&direct), TrayState::Blocked);
    }

    #[test]
    fn tooltips() {
        assert_eq!(tooltip(TrayState::Off), "Lynnk — Off");
        assert_eq!(tooltip(TrayState::Blocked), "Lynnk — Connection unavailable");
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn mac_assets_decode_as_44px_rgba() {
        for art in [
            Art::State(TrayState::Off),
            Art::State(TrayState::Connecting),
            Art::State(TrayState::Connected),
            Art::State(TrayState::Blocked),
            Art::Frame(0),
            Art::Frame(1),
            Art::Frame(2),
            Art::Frame(3),
        ] {
            let (rgba, w, h) = decode_png(asset(art)).expect("decodes");
            assert_eq!((w, h, rgba.len()), (44, 44, 44 * 44 * 4), "{art:?}");
        }
    }

    #[cfg(windows)]
    #[test]
    fn windows_assets_have_every_tray_size() {
        for light in [true, false] {
            for art in [Art::State(TrayState::Off), Art::State(TrayState::Blocked), Art::Frame(3)] {
                for px in [16, 20, 24, 32] {
                    let (_, w, h) = decode_ico(asset(art, light), px).expect("decodes");
                    assert_eq!((w, h), (px, px), "{art:?} {light} {px}");
                }
            }
        }
    }
}
