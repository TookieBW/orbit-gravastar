//! System tray icon: left click toggles the info popup, double click opens
//! the main window, the context menu offers "open" and "quit".

use std::sync::mpsc::{self, Receiver};

use tray_icon::menu::{Menu, MenuEvent, MenuId, MenuItem, PredefinedMenuItem};
use tray_icon::{Icon, MouseButton, MouseButtonState, TrayIcon, TrayIconBuilder, TrayIconEvent};

use crate::i18n::Strings;

pub enum TrayAction {
    /// Toggle the popup next to the icon (icon rect in physical pixels: x, y, w, h).
    TogglePopup([f32; 4]),
    OpenWindow,
    Quit,
}

pub struct Tray {
    icon: TrayIcon,
    rx: Receiver<TrayAction>,
    open: MenuItem,
    quit: MenuItem,
    tooltip: String,
}

impl Tray {
    pub fn new(ctx: &egui::Context, s: &Strings) -> Option<Self> {
        let png = eframe::icon_data::from_png_bytes(include_bytes!("../assets/tray.png")).ok()?;
        let image = Icon::from_rgba(png.rgba, png.width, png.height).ok()?;

        let open = MenuItem::new(s.tray_open, true, None);
        let quit = MenuItem::new(s.tray_quit, true, None);
        let menu = Menu::new();
        menu.append_items(&[&open, &PredefinedMenuItem::separator(), &quit]).ok()?;

        let icon = TrayIconBuilder::new()
            .with_icon(image)
            .with_tooltip("Orbit")
            .with_menu(Box::new(menu))
            .with_menu_on_left_click(false)
            .build()
            .ok()?;

        let (tx, rx) = mpsc::channel();
        let (open_id, quit_id): (MenuId, MenuId) = (open.id().clone(), quit.id().clone());
        {
            let (tx, ctx) = (tx.clone(), ctx.clone());
            TrayIconEvent::set_event_handler(Some(move |e: TrayIconEvent| {
                let action = match e {
                    TrayIconEvent::Click { button: MouseButton::Left, button_state: MouseButtonState::Up, rect, .. } => Some(TrayAction::TogglePopup([
                        rect.position.x as f32,
                        rect.position.y as f32,
                        rect.size.width as f32,
                        rect.size.height as f32,
                    ])),
                    TrayIconEvent::DoubleClick { button: MouseButton::Left, .. } => Some(TrayAction::OpenWindow),
                    _ => None,
                };
                if let Some(a) = action {
                    let _ = tx.send(a);
                    // Wakes the app even while the main window is hidden.
                    ctx.request_repaint();
                }
            }));
        }
        {
            let ctx = ctx.clone();
            MenuEvent::set_event_handler(Some(move |e: MenuEvent| {
                let action = if e.id == open_id {
                    TrayAction::OpenWindow
                } else if e.id == quit_id {
                    TrayAction::Quit
                } else {
                    return;
                };
                let _ = tx.send(action);
                ctx.request_repaint();
            }));
        }
        Some(Self { icon, rx, open, quit, tooltip: String::new() })
    }

    pub fn poll(&self) -> Vec<TrayAction> {
        self.rx.try_iter().collect()
    }

    pub fn set_tooltip(&mut self, text: String) {
        if text != self.tooltip {
            let _ = self.icon.set_tooltip(Some(&text));
            self.tooltip = text;
        }
    }

    pub fn set_language(&self, s: &Strings) {
        self.open.set_text(s.tray_open);
        self.quit.set_text(s.tray_quit);
    }
}

/// Screen edge the taskbar is docked to.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Edge {
    Left,
    Top,
    Right,
    Bottom,
}

/// Asks Windows where the taskbar is.
#[cfg(windows)]
pub fn taskbar_edge() -> Option<Edge> {
    #[repr(C)]
    struct AppBarData {
        cb_size: u32,
        hwnd: isize,
        callback_message: u32,
        edge: u32,
        rc: [i32; 4],
        lparam: isize,
    }
    #[link(name = "shell32")]
    extern "system" {
        fn SHAppBarMessage(message: u32, data: *mut AppBarData) -> usize;
    }
    const ABM_GETTASKBARPOS: u32 = 5;
    let mut data = AppBarData { cb_size: std::mem::size_of::<AppBarData>() as u32, hwnd: 0, callback_message: 0, edge: 0, rc: [0; 4], lparam: 0 };
    if unsafe { SHAppBarMessage(ABM_GETTASKBARPOS, &mut data) } == 0 {
        return None;
    }
    Some(match data.edge {
        0 => Edge::Left,
        1 => Edge::Top,
        2 => Edge::Right,
        _ => Edge::Bottom,
    })
}

#[cfg(not(windows))]
pub fn taskbar_edge() -> Option<Edge> {
    None
}
