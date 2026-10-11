//! Popover placement relative to the tray icon (REQ-1). Pure maths in physical
//! pixels so it can be unit-tested for every taskbar edge and monitor layout.

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rect {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}

impl Rect {
    fn cx(&self) -> f64 {
        self.x + self.w / 2.0
    }
    fn cy(&self) -> f64 {
        self.y + self.h / 2.0
    }
    fn right(&self) -> f64 {
        self.x + self.w
    }
    fn bottom(&self) -> f64 {
        self.y + self.h
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Edge {
    Top,
    Bottom,
    Left,
    Right,
}

/// Which screen edge the tray lives on. The work area excludes the menu bar /
/// taskbar, so the icon sits outside it on that edge. If the icon is inside the
/// work area (auto-hide taskbar, Windows overflow flyout) fall back to the
/// nearest horizontal half.
fn tray_edge(icon: Rect, work: Rect) -> Edge {
    if icon.cy() <= work.y {
        Edge::Top
    } else if icon.cy() >= work.bottom() {
        Edge::Bottom
    } else if icon.cx() <= work.x {
        Edge::Left
    } else if icon.cx() >= work.right() {
        Edge::Right
    } else if icon.cy() < work.cy() {
        Edge::Top
    } else {
        Edge::Bottom
    }
}

fn clamp(v: f64, lo: f64, hi: f64) -> f64 {
    if hi < lo { lo } else { v.clamp(lo, hi) }
}

/// Top-left position (physical px) for a `win_w`×`win_h` popover anchored to
/// `icon`. The result is always fully inside `work` with at least `margin` px
/// to its edges, so next to a taskbar it sits `margin` from the work-area edge.
pub fn popover_position(icon: Rect, work: Rect, win_w: f64, win_h: f64, margin: f64) -> (f64, f64) {
    let min_x = work.x + margin;
    let max_x = work.right() - win_w - margin;
    let min_y = work.y + margin;
    let max_y = work.bottom() - win_h - margin;
    let (x, y) = match tray_edge(icon, work) {
        Edge::Top => (icon.cx() - win_w / 2.0, icon.bottom() + margin),
        Edge::Bottom => (icon.cx() - win_w / 2.0, icon.y - win_h - margin),
        Edge::Left => (icon.right() + margin, icon.cy() - win_h / 2.0),
        Edge::Right => (icon.x - win_w - margin, icon.cy() - win_h / 2.0),
    };
    (clamp(x, min_x, max_x).round(), clamp(y, min_y, max_y).round())
}

#[cfg(test)]
mod tests {
    use super::*;

    const W: f64 = 680.0; // 340 logical @2x
    const H: f64 = 960.0;
    const M: f64 = 8.0;

    fn r(x: f64, y: f64, w: f64, h: f64) -> Rect {
        Rect { x, y, w, h }
    }

    #[test]
    fn macos_menu_bar_places_below_icon_centered() {
        // 3024x1964 screen, 74px menu bar, icon near the right.
        let work = r(0.0, 74.0, 3024.0, 1890.0);
        let icon = r(2600.0, 0.0, 44.0, 74.0);
        let (x, y) = popover_position(icon, work, W, H, M);
        assert_eq!(y, 74.0 + M);
        assert_eq!(x, (2622.0 - W / 2.0).round());
    }

    #[test]
    fn icon_at_far_right_is_clamped_inside_work_area() {
        let work = r(0.0, 74.0, 3024.0, 1890.0);
        let icon = r(2990.0, 0.0, 34.0, 74.0);
        let (x, _) = popover_position(icon, work, W, H, M);
        assert_eq!(x, 3024.0 - W - M);
    }

    #[test]
    fn windows_bottom_taskbar_places_above_icon() {
        let work = r(0.0, 0.0, 1920.0, 1032.0);
        let icon = r(1700.0, 1040.0, 24.0, 40.0);
        let (x, y) = popover_position(icon, work, 340.0, 480.0, M);
        // Directly above the taskbar, `margin` inside the work area.
        assert_eq!(y, 1032.0 - 480.0 - M);
        assert!(x + 340.0 <= 1920.0 - M);
    }

    #[test]
    fn windows_top_taskbar_places_below() {
        let work = r(0.0, 48.0, 1920.0, 1032.0);
        let icon = r(1700.0, 8.0, 24.0, 32.0);
        // Below the taskbar, at least `margin` inside the work area.
        assert_eq!(popover_position(icon, work, 340.0, 480.0, M).1, 48.0 + M);
    }

    #[test]
    fn windows_left_taskbar_places_right_of_icon_within_screen() {
        let work = r(62.0, 0.0, 1858.0, 1080.0);
        let icon = r(15.0, 1000.0, 32.0, 32.0);
        let (x, y) = popover_position(icon, work, 340.0, 480.0, M);
        assert_eq!(x, 62.0 + M); // right of the taskbar, inside the work area
        assert_eq!(y, 1080.0 - 480.0 - M); // clamped to bottom of work area
    }

    #[test]
    fn windows_right_taskbar_places_left_of_icon() {
        let work = r(0.0, 0.0, 1858.0, 1080.0);
        let icon = r(1873.0, 1000.0, 32.0, 32.0);
        // Left of the taskbar, kept fully inside the work area.
        assert_eq!(popover_position(icon, work, 340.0, 480.0, M).0, 1858.0 - 340.0 - M);
    }

    #[test]
    fn secondary_monitor_with_negative_origin() {
        // Monitor left of the primary; taskbar at the bottom.
        let work = r(-1920.0, 0.0, 1920.0, 1032.0);
        let icon = r(-200.0, 1040.0, 24.0, 40.0);
        let (x, y) = popover_position(icon, work, 340.0, 480.0, M);
        assert!(x >= -1920.0 + M && x + 340.0 <= -M);
        assert_eq!(y, 1032.0 - 480.0 - M);
    }

    #[test]
    fn icon_inside_work_area_falls_back_by_half() {
        // Auto-hide taskbar: icon reported inside the work area, lower half.
        let work = r(0.0, 0.0, 1920.0, 1080.0);
        let icon = r(1700.0, 1050.0, 24.0, 24.0);
        assert_eq!(popover_position(icon, work, 340.0, 480.0, M).1, 1050.0 - 480.0 - M);
    }
}
