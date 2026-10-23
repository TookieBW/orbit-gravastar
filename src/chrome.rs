//! Custom window chrome for the frameless window: caption buttons, dragging
//! the window by its top strip, double-click to maximise and edge resizing.

use egui::{pos2, vec2, Color32, CursorIcon, Pos2, Rect, ResizeDirection, Sense, Stroke, StrokeKind, Ui, ViewportCommand};

use crate::theme::{self, alpha};

/// Height of the strip along the top edge that moves the window.
pub const CAPTION_HEIGHT: f32 = 40.0;

fn is_maximized(ui: &Ui) -> bool {
    ui.input(|i| i.viewport().maximized.unwrap_or(false))
}

/// Corner radius of the main window. Square while maximised, and with the glass
/// effect: Windows 10 blurs the whole window rectangle, so rounded corners would
/// show blurred squares behind them (see `native_setup`).
pub fn window_radius(ui: &Ui) -> f32 {
    if is_maximized(ui) || theme::is_glass() {
        0.0
    } else {
        theme::WINDOW_RADIUS
    }
}

/// Register before the page content so real widgets on top keep their clicks.
pub fn caption_drag_area(ui: &mut Ui, full: Rect) {
    // Starts below the top resize edge so dragging and resizing don't compete.
    let strip = Rect::from_min_max(full.min + vec2(0.0, 6.0), pos2(full.max.x, full.min.y + CAPTION_HEIGHT));
    let resp = ui.interact(strip, ui.scope_id().with("caption"), Sense::click_and_drag());
    if resp.double_clicked() {
        let max = is_maximized(ui);
        ui.ctx().send_viewport_cmd(ViewportCommand::Maximized(!max));
    } else if resp.drag_started_by(egui::PointerButton::Primary) {
        ui.ctx().send_viewport_cmd(ViewportCommand::StartDrag);
    }
}

#[derive(Clone, Copy, PartialEq)]
enum Caption {
    Minimize,
    Maximize,
    Close,
}

/// Minimise / maximise / close in the top-right corner. Register after the content.
pub fn caption_buttons(ui: &mut Ui, full: Rect) {
    let maximized = is_maximized(ui);
    let size = vec2(36.0, 30.0);
    let gap = 6.0;
    let top_right = pos2(full.max.x - 12.0, full.min.y + 10.0);
    for (i, kind) in [Caption::Close, Caption::Maximize, Caption::Minimize].into_iter().enumerate() {
        let min = pos2(top_right.x - size.x * (i as f32 + 1.0) - gap * i as f32, top_right.y);
        let rect = Rect::from_min_size(min, size);
        let resp = ui.interact(rect, ui.scope_id().with(("caption-btn", i)), Sense::click()).on_hover_cursor(CursorIcon::PointingHand);
        let pressed = resp.is_pointer_button_down_on();
        let hovered = resp.hovered();
        let p = ui.painter();

        let (bg, fg) = match (kind, hovered, pressed) {
            (Caption::Close, true, false) => (alpha(theme::BAD, 0.9), Color32::WHITE),
            (Caption::Close, true, true) => (alpha(theme::BAD, 0.7), Color32::WHITE),
            (_, true, false) => (theme::ink(0.09), theme::text()),
            (_, true, true) => (theme::ink(0.05), theme::text()),
            _ => (theme::ink(0.03), theme::text2()),
        };
        if kind == Caption::Close && hovered {
            theme::glow(p, rect.translate(vec2(0.0, 3.0)).shrink(3.0), 9.0, alpha(theme::BAD, 0.45), 10.0);
        }
        p.rect_filled(rect, 9.0, bg);
        p.rect_stroke(rect, 9.0, Stroke::new(1.0, if hovered { theme::ink(0.14) } else { theme::ink(0.06) }), StrokeKind::Inside);

        let c = rect.center();
        let stroke = Stroke::new(1.4, fg);
        match kind {
            Caption::Minimize => {
                p.line_segment([c + vec2(-5.0, 0.0), c + vec2(5.0, 0.0)], stroke);
            }
            Caption::Maximize if maximized => {
                // Restore: a square with a second one peeking out behind it.
                let front = Rect::from_center_size(c + vec2(-1.5, 1.5), vec2(8.0, 8.0));
                p.rect_stroke(front, 1.5, stroke, StrokeKind::Middle);
                let (l, t, r, b) = (front.min.x + 3.0, front.min.y - 3.0, front.max.x + 3.0, front.max.y - 3.0);
                p.line_segment([pos2(l, t), pos2(r, t)], stroke);
                p.line_segment([pos2(r, t), pos2(r, b)], stroke);
            }
            Caption::Maximize => {
                p.rect_stroke(Rect::from_center_size(c, vec2(9.0, 9.0)), 2.0, stroke, StrokeKind::Middle);
            }
            Caption::Close => {
                let d = 4.5;
                p.line_segment([c + vec2(-d, -d), c + vec2(d, d)], stroke);
                p.line_segment([c + vec2(-d, d), c + vec2(d, -d)], stroke);
            }
        }

        if resp.clicked() {
            let cmd = match kind {
                Caption::Minimize => ViewportCommand::Minimized(true),
                Caption::Maximize => ViewportCommand::Maximized(!maximized),
                Caption::Close => ViewportCommand::Close,
            };
            ui.ctx().send_viewport_cmd(cmd);
        }
    }
}

/// Thin grab zones along the window edges for resizing (only when not maximised).
pub fn resize_edges(ui: &mut Ui, full: Rect) {
    if is_maximized(ui) {
        return;
    }
    let e = 5.0;
    let c = 12.0;
    let zones = [
        (Rect::from_min_max(full.min, full.min + vec2(c, c)), ResizeDirection::NorthWest, CursorIcon::ResizeNwSe),
        (Rect::from_min_max(pos2(full.max.x - c, full.min.y), pos2(full.max.x, full.min.y + c)), ResizeDirection::NorthEast, CursorIcon::ResizeNeSw),
        (Rect::from_min_max(pos2(full.min.x, full.max.y - c), pos2(full.min.x + c, full.max.y)), ResizeDirection::SouthWest, CursorIcon::ResizeNeSw),
        (Rect::from_min_max(full.max - vec2(c, c), full.max), ResizeDirection::SouthEast, CursorIcon::ResizeNwSe),
        (Rect::from_min_max(full.min, pos2(full.max.x, full.min.y + e)), ResizeDirection::North, CursorIcon::ResizeVertical),
        (Rect::from_min_max(pos2(full.min.x, full.max.y - e), full.max), ResizeDirection::South, CursorIcon::ResizeVertical),
        (Rect::from_min_max(full.min, pos2(full.min.x + e, full.max.y)), ResizeDirection::West, CursorIcon::ResizeHorizontal),
        (Rect::from_min_max(pos2(full.max.x - e, full.min.y), full.max), ResizeDirection::East, CursorIcon::ResizeHorizontal),
    ];
    let pointer: Option<Pos2> = ui.input(|i| i.pointer.hover_pos());
    let pressed = ui.input(|i| i.pointer.primary_pressed());
    // Corners first: they overlap the edges and win.
    if let Some((_, dir, cursor)) = pointer.and_then(|p| zones.iter().find(|(r, ..)| r.contains(p))) {
        ui.ctx().set_cursor_icon(*cursor);
        if pressed {
            ui.ctx().send_viewport_cmd(ViewportCommand::BeginResize(*dir));
        }
    }
}

/// Hairline around the window so it stands out from whatever is behind it.
pub fn border(ui: &Ui, full: Rect) {
    if is_maximized(ui) {
        return;
    }
    let layer = egui::LayerId::new(egui::Order::Foreground, ui.scope_id().with("window-border"));
    ui.ctx().layer_painter(layer).rect_stroke(full, window_radius(ui), Stroke::new(1.0, theme::ink(0.12)), StrokeKind::Inside);
}

/// Native touches for our frameless windows (Windows only):
/// * removes the 1 px strip winit reserves above the window for the drop shadow,
///   which Windows paints as a grey line across the rounded corners while active;
/// * `glass` puts the system blur behind the main window. Windows 10 always blurs the
///   whole window rectangle (neither a window region nor the DWM blur region clips it),
///   so the main window is drawn with square corners while glass is on, and the
///   rounded tray popup never gets the blur.
///
/// Safe to call repeatedly: once windows exist, and again when `glass` changes.
#[cfg(windows)]
pub fn native_setup(glass: bool) {
    native::GLASS.store(glass, std::sync::atomic::Ordering::Relaxed);
    // All our windows live on the UI thread.
    unsafe { native::EnumThreadWindows(native::GetCurrentThreadId(), native::visit, 0) };
}

#[cfg(not(windows))]
pub fn native_setup(_glass: bool) {}

#[cfg(windows)]
mod native {
    use std::sync::atomic::{AtomicBool, AtomicIsize, Ordering};

    pub static GLASS: AtomicBool = AtomicBool::new(false);
    /// The main window: the only winit window when `native_setup` first runs.
    static MAIN: AtomicIsize = AtomicIsize::new(0);

    type Hwnd = isize;
    type SubclassProc = unsafe extern "system" fn(Hwnd, u32, usize, isize, usize, usize) -> isize;

    #[repr(C)]
    struct AccentPolicy {
        state: u32,
        flags: u32,
        gradient_color: u32,
        animation_id: u32,
    }

    #[repr(C)]
    struct CompositionAttribute {
        attribute: u32,
        data: *mut AccentPolicy,
        size: usize,
    }

    #[link(name = "user32")]
    extern "system" {
        pub fn EnumThreadWindows(thread: u32, visit: unsafe extern "system" fn(Hwnd, isize) -> i32, lparam: isize) -> i32;
        fn GetClassNameW(hwnd: Hwnd, name: *mut u16, len: i32) -> i32;
        fn IsZoomed(hwnd: Hwnd) -> i32;
        fn SetWindowPos(hwnd: Hwnd, after: Hwnd, x: i32, y: i32, cx: i32, cy: i32, flags: u32) -> i32;
    }
    #[link(name = "kernel32")]
    extern "system" {
        pub fn GetCurrentThreadId() -> u32;
        fn GetModuleHandleW(name: *const u16) -> isize;
        fn GetProcAddress(module: isize, name: *const u8) -> *const std::ffi::c_void;
    }
    #[link(name = "comctl32")]
    extern "system" {
        fn SetWindowSubclass(hwnd: Hwnd, proc: SubclassProc, id: usize, data: usize) -> i32;
        fn GetWindowSubclass(hwnd: Hwnd, proc: SubclassProc, id: usize, data: *mut usize) -> i32;
        fn DefSubclassProc(hwnd: Hwnd, msg: u32, wparam: usize, lparam: isize) -> isize;
    }

    const WM_NCCALCSIZE: u32 = 0x0083;
    // SWP_NOSIZE | SWP_NOMOVE | SWP_NOZORDER | SWP_NOACTIVATE | SWP_FRAMECHANGED
    const REFRESH_FRAME: u32 = 0x0001 | 0x0002 | 0x0004 | 0x0010 | 0x0020;
    const WCA_ACCENT_POLICY: u32 = 19;
    const ACCENT_DISABLED: u32 = 0;
    const ACCENT_ENABLE_BLURBEHIND: u32 = 3;
    // Class name winit registers its windows under.
    const WINIT_CLASS: &str = "Window Class";

    unsafe extern "system" fn proc(hwnd: Hwnd, msg: u32, wparam: usize, lparam: isize, _: usize, _: usize) -> isize {
        // Client area = the whole window. Maximised windows keep winit's handling,
        // which fits them to the monitor's work area.
        if msg == WM_NCCALCSIZE && wparam != 0 && unsafe { IsZoomed(hwnd) } == 0 {
            return 0;
        }
        unsafe { DefSubclassProc(hwnd, msg, wparam, lparam) }
    }

    fn set_blur(hwnd: Hwnd, on: bool) {
        // Undocumented (not in the import library) but stable since Windows 10;
        // the taskbar uses it for its blur.
        type SetCompositionAttribute = unsafe extern "system" fn(Hwnd, *mut CompositionAttribute) -> i32;
        let user32: Vec<u16> = "user32.dll\0".encode_utf16().collect();
        let f = unsafe { GetProcAddress(GetModuleHandleW(user32.as_ptr()), c"SetWindowCompositionAttribute".as_ptr().cast()) };
        if f.is_null() {
            return;
        }
        let set_attribute: SetCompositionAttribute = unsafe { std::mem::transmute(f) };
        let mut accent = AccentPolicy { state: if on { ACCENT_ENABLE_BLURBEHIND } else { ACCENT_DISABLED }, flags: 0, gradient_color: 0, animation_id: 0 };
        let mut data = CompositionAttribute { attribute: WCA_ACCENT_POLICY, data: &mut accent, size: std::mem::size_of::<AccentPolicy>() };
        unsafe { set_attribute(hwnd, &mut data) };
    }

    pub unsafe extern "system" fn visit(hwnd: Hwnd, _: isize) -> i32 {
        let mut name = [0u16; 32];
        let len = unsafe { GetClassNameW(hwnd, name.as_mut_ptr(), name.len() as i32) }.max(0) as usize;
        if String::from_utf16_lossy(&name[..len]) != WINIT_CLASS {
            return 1;
        }
        if unsafe { GetWindowSubclass(hwnd, proc, 1, std::ptr::null_mut()) } == 0 {
            unsafe {
                SetWindowSubclass(hwnd, proc, 1, 0);
                SetWindowPos(hwnd, 0, 0, 0, 0, 0, REFRESH_FRAME);
            }
        }
        let _ = MAIN.compare_exchange(0, hwnd, Ordering::Relaxed, Ordering::Relaxed);
        set_blur(hwnd, GLASS.load(Ordering::Relaxed) && MAIN.load(Ordering::Relaxed) == hwnd);
        1
    }
}
