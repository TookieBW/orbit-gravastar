//! Small info window that opens from the tray icon: battery, DPI and polling
//! rate of the mouse, polling rate and profile of the keyboard.

use egui::{pos2, vec2, Align, Align2, Color32, CornerRadius, Frame, Layout, Margin, Rect, Sense, Stroke, StrokeKind, Ui, Vec2};

use crate::i18n::Strings;
use crate::protocol::keyboard::REPORT_RATES;
use crate::theme::{self, body, display, gradient_rect, icon, mono, semibold};
use crate::widgets::{button, Kind};
use crate::worker::{DeviceData, DeviceEntry};

pub const WIDTH: f32 = 320.0;
const PAD: f32 = 16.0;
const KEYBOARD_CARD: f32 = 112.0;
const MOUSE_CARD: f32 = 150.0;

pub enum PopupAction {
    None,
    OpenWindow,
}

/// Window height needed for the given devices.
pub fn height(devices: &[DeviceEntry]) -> f32 {
    let cards: f32 = devices.iter().map(|d| if matches!(d.data, DeviceData::Mouse(_)) { MOUSE_CARD } else { KEYBOARD_CARD } + 10.0).sum();
    PAD * 2.0 + 44.0 + if devices.is_empty() { 40.0 } else { cards }
}

pub fn show(ui: &mut Ui, s: &Strings, devices: &[DeviceEntry]) -> PopupAction {
    let full = ui.max_rect();
    // Always opaque: the popup gets no system blur (see `chrome::native_setup`).
    theme::paint_backdrop(ui.painter(), full, theme::WINDOW_RADIUS, false);
    ui.painter().rect_stroke(full, theme::WINDOW_RADIUS, Stroke::new(1.0, theme::ink(0.14)), StrokeKind::Inside);
    let mut action = PopupAction::None;

    let mut content = ui.new_child(egui::UiBuilder::new().max_rect(full.shrink(PAD)).layout(Layout::top_down(Align::Min)));
    let ui = &mut content;
    // All vertical gaps are explicit so `height` can predict the window size.
    ui.spacing_mut().item_spacing.y = 0.0;
    ui.horizontal(|ui| {
        let (mark, _) = ui.allocate_exact_size(vec2(28.0, 28.0), Sense::hover());
        crate::app::brand_mark(ui.painter(), mark.expand(3.0));
        ui.add_space(6.0);
        ui.label(egui::RichText::new("Orbit").font(display(17.0)));
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            if button(ui, s.tray_open, Kind::Normal, true, true).clicked() {
                action = PopupAction::OpenWindow;
            }
        });
    });
    ui.add_space(10.0);

    if devices.is_empty() {
        ui.label(egui::RichText::new(s.no_devices).color(theme::text3()));
    }
    for d in devices {
        let height = if matches!(d.data, DeviceData::Mouse(_)) { MOUSE_CARD } else { KEYBOARD_CARD };
        Frame::new()
            .fill(theme::panel_fill())
            .stroke(Stroke::new(1.0, theme::line()))
            .corner_radius(CornerRadius::same(12))
            .inner_margin(Margin::same(12))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.set_height(height - 24.0);
                device_card(ui, s, d);
            });
        ui.add_space(10.0);
    }
    action
}

fn device_card(ui: &mut Ui, s: &Strings, d: &DeviceEntry) {
    ui.spacing_mut().item_spacing.y = 0.0;
    let (glyph, online, status) = match &d.data {
        DeviceData::Keyboard(_) => (icon::KEYBOARD, true, format!("{} · {}", s.connected, s.wired)),
        DeviceData::Mouse(m) => (
            icon::MOUSE,
            m.online,
            if !m.online {
                s.asleep.to_string()
            } else if m.identity.is_some_and(|i| i.wired) {
                s.wired.to_string()
            } else {
                s.wireless.to_string()
            },
        ),
    };
    ui.horizontal(|ui| {
        let (tile, _) = ui.allocate_exact_size(vec2(32.0, 32.0), Sense::hover());
        let p = ui.painter();
        gradient_rect(p, tile, 9.0, 45.0, theme::accent_at);
        theme::sheen(p, tile, 9.0, 0.22);
        p.text(tile.center(), Align2::CENTER_CENTER, glyph, body(16.0), Color32::WHITE);
        ui.add_space(4.0);
        ui.vertical(|ui| {
            ui.spacing_mut().item_spacing.y = 0.0;
            ui.label(egui::RichText::new(&d.name).font(semibold(13.5)));
            ui.horizontal(|ui| {
                let (dot, _) = ui.allocate_exact_size(vec2(8.0, 14.0), Sense::hover());
                ui.painter().circle_filled(dot.center(), 3.5, if online { theme::GOOD } else { theme::text3() });
                ui.label(egui::RichText::new(status).font(body(11.5)).color(theme::text3()));
            });
        });
    });
    ui.add_space(8.0);

    match &d.data {
        DeviceData::Keyboard(k) => {
            let rate = k.rate.and_then(|r| REPORT_RATES.get(r as usize)).map(|r| format!("{r} Hz")).unwrap_or_else(|| "–".into());
            metrics(ui, &[(s.polling_rate, rate), (s.profile, (k.profile + 1).to_string())]);
        }
        DeviceData::Mouse(m) => {
            let cfg = m.config.as_ref().filter(|_| m.online);
            battery_bar(ui, s, m.battery.filter(|_| m.online));
            ui.add_space(8.0);
            let dpi = cfg.map(|c| c.dpi[c.current_stage.min(c.dpi.len() - 1)].to_string()).unwrap_or_else(|| "–".into());
            let rate = cfg.map(|c| if c.report_rate >= 1000 { format!("{}K Hz", c.report_rate / 1000) } else { format!("{} Hz", c.report_rate) }).unwrap_or_else(|| "–".into());
            metrics(ui, &[("DPI", dpi), (s.polling_rate, rate)]);
        }
    }
}

/// Two value tiles side by side.
fn metrics(ui: &mut Ui, items: &[(&str, String)]) {
    let gap = 8.0;
    let w = (ui.available_width() - gap) / 2.0;
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = gap;
        for (label, value) in items {
            let (r, _) = ui.allocate_exact_size(vec2(w, 42.0), Sense::hover());
            let p = ui.painter();
            p.rect_filled(r, 9.0, theme::well(0.25));
            p.rect_stroke(r, 9.0, Stroke::new(1.0, theme::line()), StrokeKind::Inside);
            p.text(r.left_top() + vec2(10.0, 8.0), Align2::LEFT_TOP, label.to_uppercase(), semibold(9.5), theme::text3());
            p.text(r.left_bottom() + vec2(10.0, -7.0), Align2::LEFT_BOTTOM, value, mono(14.0), theme::text());
        }
    });
}

fn battery_bar(ui: &mut Ui, s: &Strings, battery: Option<crate::protocol::mouse::Battery>) {
    let (r, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 22.0), Sense::hover());
    let p = ui.painter();
    let label_w = 64.0;
    let track = Rect::from_min_max(r.min + vec2(0.0, 6.0), pos2(r.max.x - label_w, r.max.y - 6.0));
    p.rect_filled(track, 5.0, theme::ink(0.08));
    match battery {
        Some(b) => {
            let fill = Rect::from_min_size(track.min, vec2(track.width() * b.level as f32 / 100.0, track.height()));
            let low = b.level <= 15;
            gradient_rect(p, fill, 5.0, 0.0, |t| if low { theme::lerp(theme::BAD, theme::WARN, t) } else { theme::lerp(theme::GOOD, theme::ACCENT2, t) });
            let glyph = if b.charging { icon::BATTERY_CHARGING } else { icon::BATTERY_MEDIUM };
            p.text(pos2(r.max.x, r.center().y), Align2::RIGHT_CENTER, format!("{glyph} {}%", b.level), semibold(13.0), if low { theme::WARN } else { theme::text() });
        }
        None => {
            p.text(pos2(r.max.x, r.center().y), Align2::RIGHT_CENTER, format!("{} –", s.battery), body(12.0), theme::text3());
        }
    }
}
