use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use egui::{pos2, vec2, Align, Align2, Color32, CursorIcon, Frame, Layout, Margin, Rect, Sense, Shape, Stroke, StrokeKind, Ui, UiBuilder, ViewportBuilder, ViewportCommand, ViewportId};

use crate::i18n::{Lang, Strings};
use crate::pages;
use crate::protocol::keyboard::KeyPerf;
use crate::protocol::Rgb;
use crate::theme::{self, alpha, body, display, gradient_rect, icon, line, semibold, white, ThemePref};
use crate::widgets::keyboard_view::{DragSelect, KeyId};
use crate::widgets::{self, badge, battery_badge, Kind, Tone};
use crate::tray::{Tray, TrayAction};
use crate::worker::{DeviceData, DeviceEntry, Worker};

/// Per-device UI state that is not stored on the device.
#[derive(Default)]
pub struct DeviceUi {
    pub tab: usize,
    pub selected: HashSet<KeyId>,
    pub selection_ready: bool,
    pub drag: Option<DragSelect>,
    pub draft: Option<KeyPerf>,
    pub draft_from: Option<KeyId>,
    pub split_release: bool,
    pub area: u8,
    pub palette_edit: Option<(u8, usize, Rgb)>,
    pub edit_stage: usize,
    pub stage_color_edit: Option<(usize, Rgb)>,
    pub light_color_edit: Option<Rgb>,
}

/// An action waiting for the user's confirmation.
pub enum Confirm {
    ReportRate { device: String, rate: u8 },
    FactoryReset { device: String },
}

/// The tray popup and where it was opened from.
struct Popup {
    /// Tray icon rect in physical pixels (x, y, w, h).
    anchor: [f32; 4],
    was_focused: bool,
    /// Written by the popup window, read by the main window.
    feedback: Arc<Mutex<PopupFeedback>>,
}

impl Popup {
    fn new(anchor: [f32; 4]) -> Self {
        Self { anchor, was_focused: false, feedback: Arc::default() }
    }
}

#[derive(Default)]
struct PopupFeedback {
    focused: Option<bool>,
    /// The popup window's top frame line has been removed.
    frame_fixed: bool,
    escape: bool,
    open_window: bool,
}

pub struct OrbitApp {
    pub worker: Worker,
    pub lang: Lang,
    demo: bool,
    active: Option<String>,
    ui: HashMap<String, DeviceUi>,
    pub confirm: Option<Confirm>,
    automation: Option<crate::shot::Automation>,
    tray: Option<Tray>,
    tray_lang: Lang,
    /// Main window hidden to the tray.
    hidden: bool,
    /// "Quit" chosen in the tray menu: let the close request through.
    quitting: bool,
    popup: Option<Popup>,
    /// When the popup last closed because it lost focus (a tray click does that too).
    popup_closed_at: Option<Instant>,
    tooltip_at: Instant,
    theme_pref: ThemePref,
    /// Glass effect (system blur behind a slightly see-through backdrop); Windows only.
    glass: bool,
    /// The settings page is shown instead of a device.
    show_settings: bool,
    autostart: bool,
    /// Launched with `--tray`: hide the window as soon as it exists.
    start_hidden: bool,
}

impl OrbitApp {
    pub fn new(cc: &eframe::CreationContext<'_>, demo: bool, start_hidden: bool) -> Self {
        theme::install_fonts(&cc.egui_ctx);
        theme::install_style(&cc.egui_ctx);
        let lang = cc.storage.and_then(|s| s.get_string("lang")).and_then(|c| Lang::from_code(&c)).unwrap_or_else(Lang::system);
        let theme_pref = cc.storage.and_then(|s| s.get_string("theme")).and_then(|c| ThemePref::from_code(&c)).unwrap_or(ThemePref::Dark);
        theme::set_light(theme_pref.is_light(&cc.egui_ctx));
        theme::install_style(&cc.egui_ctx);
        let glass = cfg!(windows) && cc.storage.and_then(|s| s.get_string("glass")).is_none_or(|v| v == "1");
        theme::set_glass(glass);
        crate::chrome::native_setup(glass);
        let worker = Worker::spawn(cc.egui_ctx.clone(), demo);
        let automation = crate::shot::Automation::from_env();
        // No tray icon for scripted screenshot runs.
        let tray = if automation.is_none() { Tray::new(&cc.egui_ctx, lang.s()) } else { None };
        Self {
            worker,
            lang,
            demo,
            active: None,
            ui: HashMap::new(),
            confirm: None,
            automation,
            tray,
            tray_lang: lang,
            hidden: false,
            quitting: false,
            popup: None,
            popup_closed_at: None,
            tooltip_at: Instant::now(),
            theme_pref,
            glass,
            show_settings: false,
            autostart: crate::autostart::is_enabled(),
            start_hidden,
        }
    }
}

impl eframe::App for OrbitApp {
    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        // Scripted screenshot runs must not change the user's settings.
        if self.automation.is_some() {
            return;
        }
        storage.set_string("lang", self.lang.code().to_string());
        storage.set_string("theme", self.theme_pref.code().to_string());
        storage.set_string("glass", if self.glass { "1" } else { "0" }.to_string());
    }

    fn logic(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Scripted screenshots run here so they keep going while the window is hidden.
        if let Some(a) = self.automation.as_mut() {
            let devices = self.worker.lock().devices.clone();
            a.drive(ctx, &devices, &mut self.active, &mut self.ui);
            if std::mem::take(&mut a.open_settings) {
                self.show_settings = true;
            }
            if std::mem::take(&mut a.close_settings) {
                self.show_settings = false;
            }
            if let Some(t) = a.theme.take().and_then(|t| ThemePref::from_code(&t)) {
                self.theme_pref = t;
            }
            if let Some(g) = a.glass.take() {
                self.glass = g && cfg!(windows);
            }
            if let Some(l) = a.lang.take().and_then(|c| Lang::from_code(&c)) {
                self.lang = l;
            }
            if std::mem::take(&mut a.minimize_window) {
                ctx.send_viewport_cmd(ViewportCommand::Minimized(true));
            }
            if std::mem::take(&mut a.hide_window) {
                ctx.send_viewport_cmd(ViewportCommand::Visible(false));
                self.hidden = true;
            }
            if std::mem::take(&mut a.open_popup) {
                // Pretend the tray icon sits in the bottom-right corner of the screen.
                let ppp = ctx.input(|i| i.viewport().native_pixels_per_point).unwrap_or(1.0);
                let m = ctx.input(|i| i.viewport().monitor_size).unwrap_or(vec2(1920.0, 1080.0)) * ppp;
                self.popup = Some(Popup::new([m.x - 140.0, m.y - 40.0, 24.0, 40.0]));
            }
        }

        // Closing the window hides it to the tray; "Quit" in the tray menu really exits.
        if ctx.input(|i| i.viewport().close_requested()) && self.tray.is_some() && !self.quitting {
            ctx.send_viewport_cmd(ViewportCommand::CancelClose);
            ctx.send_viewport_cmd(ViewportCommand::Visible(false));
            self.hidden = true;
        }
        let actions = self.tray.as_ref().map(|t| t.poll()).unwrap_or_default();
        for action in actions {
            match action {
                TrayAction::TogglePopup(anchor) => {
                    let just_closed = self.popup_closed_at.is_some_and(|t| t.elapsed() < Duration::from_millis(400));
                    if self.popup.take().is_none() && !just_closed {
                        self.popup = Some(Popup::new(anchor));
                    }
                }
                TrayAction::OpenWindow => self.show_window(ctx),
                TrayAction::Quit => {
                    self.quitting = true;
                    ctx.send_viewport_cmd(ViewportCommand::Close);
                }
            }
        }
        let minimized = ctx.input(|i| i.viewport().minimized == Some(true));
        if self.popup.is_some() && (self.hidden || minimized) {
            // The popup is registered from `ui`, which only runs for a hidden or minimized
            // window on request. The request is honoured on the *next* pass, so ask for one too.
            ctx.send_viewport_cmd(ViewportCommand::RequestPaintWhileHidden);
            ctx.request_repaint();
        }
        if let Some(tray) = self.tray.as_mut() {
            if self.tray_lang != self.lang {
                tray.set_language(self.lang.s());
                self.tray_lang = self.lang;
            }
            if self.tooltip_at.elapsed() > Duration::from_secs(2) {
                self.tooltip_at = Instant::now();
                tray.set_tooltip(tray_tooltip(&self.worker, self.lang.s()));
            }
        }
    }

    fn clear_color(&self, _visuals: &egui::Visuals) -> [f32; 4] {
        // Transparent, so the rounded window corners show what is behind the window.
        [0.0; 4]
    }

    fn ui(&mut self, ui: &mut Ui, _frame: &mut eframe::Frame) {
        if std::mem::take(&mut self.start_hidden) && self.tray.is_some() {
            ui.ctx().send_viewport_cmd(ViewportCommand::Visible(false));
            self.hidden = true;
        }
        // Apply the theme choice (and follow Windows when set to "System").
        let light = self.theme_pref.is_light(ui.ctx());
        if light != theme::is_light() {
            theme::set_light(light);
            theme::install_style(ui.ctx());
        }
        if self.glass != theme::is_glass() {
            theme::set_glass(self.glass);
            crate::chrome::native_setup(self.glass);
        }
        let s = self.lang.s();
        let (devices, scanned, toasts) = {
            let mut shared = self.worker.lock();
            shared.toasts.retain(|t| t.born.elapsed() < Duration::from_millis(if t.error { 6000 } else { 2200 }));
            let toasts: Vec<(String, bool)> = shared.toasts.iter().map(|t| (t.text.clone(), t.error)).collect();
            (shared.devices.clone(), shared.scanned, toasts)
        };
        if !devices.iter().any(|d| Some(&d.id) == self.active.as_ref()) {
            self.active = devices.first().map(|d| d.id.clone());
        }
        let full = ui.max_rect();
        let maximized = ui.input(|i| i.viewport().maximized.unwrap_or(false));
        theme::paint_backdrop(ui.painter(), full, if maximized { 0.0 } else { theme::WINDOW_RADIUS });
        let sidebar = Rect::from_min_size(full.min, vec2(280.0, full.height()));
        let main = Rect::from_min_max(pos2(sidebar.max.x, full.min.y), full.max);
        ui.painter().vline(sidebar.max.x, full.y_range(), Stroke::new(1.0, line()));
        crate::chrome::caption_drag_area(ui, full);

        let mut side = ui.new_child(UiBuilder::new().max_rect(sidebar.shrink2(vec2(16.0, 24.0))).layout(Layout::top_down(Align::Min)));
        self.sidebar(&mut side, s, &devices, scanned);

        let mut content = ui.new_child(UiBuilder::new().max_rect(main).layout(Layout::top_down(Align::Min)));
        if self.show_settings {
            self.settings_screen(&mut content);
        } else {
            match devices.iter().find(|d| Some(&d.id) == self.active.as_ref()).cloned() {
                Some(dev) => self.device_screen(&mut content, s, &dev),
                None => empty_state(&mut content, s, scanned),
            }
        }

        toast_stack(ui.ctx(), &toasts);
        if !toasts.is_empty() {
            ui.ctx().request_repaint_after(Duration::from_millis(250));
        }
        self.confirm_dialog(ui.ctx(), s);

        crate::chrome::caption_buttons(ui, full);
        crate::chrome::resize_edges(ui, full);
        crate::chrome::border(ui, full);

        self.popup_window(ui.ctx(), &devices);
    }
}

impl OrbitApp {
    fn show_window(&mut self, ctx: &egui::Context) {
        self.hidden = false;
        self.popup = None;
        ctx.send_viewport_cmd(ViewportCommand::Visible(true));
        ctx.send_viewport_cmd(ViewportCommand::Minimized(false));
        ctx.send_viewport_cmd(ViewportCommand::Focus);
    }

    /// The tray popup as its own small borderless window above the tray icon.
    fn popup_window(&mut self, ctx: &egui::Context, devices: &[DeviceEntry]) {
        let Some(popup) = self.popup.as_mut() else { return };
        let height = crate::popup::height(devices);
        let ppp = ctx.input(|i| i.viewport().native_pixels_per_point).unwrap_or(1.0);
        let monitor = ctx.input(|i| i.viewport().monitor_size).unwrap_or(vec2(1920.0, 1080.0));
        let [x, y, w, h] = popup.anchor.map(|v| v / ppp);
        let width = crate::popup::WIDTH;
        let clamp_x = |v: f32| v.clamp(8.0, (monitor.x - width - 8.0).max(8.0));
        let clamp_y = |v: f32| v.clamp(8.0, (monitor.y - height - 8.0).max(8.0));
        // Open away from the taskbar, wherever it is docked.
        use crate::tray::Edge;
        let edge = crate::tray::taskbar_edge().unwrap_or(if y > monitor.y / 2.0 { Edge::Bottom } else { Edge::Top });
        let (px, py) = match edge {
            Edge::Left => (clamp_x(x + w + 12.0), clamp_y(y + h - height)),
            Edge::Right => (clamp_x(x - width - 12.0), clamp_y(y + h - height)),
            Edge::Top => (clamp_x(x + w / 2.0 - width / 2.0), clamp_y(y + h + 10.0)),
            Edge::Bottom => (clamp_x(x + w / 2.0 - width / 2.0), clamp_y(y - height - 10.0)),
        };
        let builder = ViewportBuilder::default()
            .with_title("Orbit")
            .with_decorations(false)
            .with_resizable(false)
            .with_taskbar(false)
            .with_transparent(true)
            .with_always_on_top()
            .with_active(true)
            .with_inner_size([crate::popup::WIDTH, height])
            .with_position([px, py]);
        // A deferred viewport: eframe creates and paints it on its own, which also
        // works while the main window is hidden (an immediate viewport would not).
        let shared = self.worker.shared.clone();
        let feedback = popup.feedback.clone();
        let lang = self.lang;
        ctx.show_viewport_deferred(popup_viewport(), builder, move |ui, _| {
            let devices = shared.lock().map(|s| s.devices.clone()).unwrap_or_default();
            let action = crate::popup::show(ui, lang.s(), &devices);
            let mut f = feedback.lock().unwrap_or_else(|e| e.into_inner());
            if !std::mem::replace(&mut f.frame_fixed, true) {
                crate::chrome::native_setup(theme::is_glass());
            }
            let focused = ui.input(|i| i.viewport().focused);
            let escape = ui.input(|i| i.key_pressed(egui::Key::Escape));
            let open = matches!(action, crate::popup::PopupAction::OpenWindow);
            if focused != f.focused || escape || open {
                // Let the main viewport react (close the popup / show the window).
                ui.ctx().request_repaint_of(ViewportId::ROOT);
            }
            f.focused = focused;
            f.escape |= escape;
            f.open_window |= open;
            ui.ctx().request_repaint_after(Duration::from_millis(500));
        });

        let (focused, escape, open) = {
            let mut f = popup.feedback.lock().unwrap_or_else(|e| e.into_inner());
            (f.focused, std::mem::take(&mut f.escape), std::mem::take(&mut f.open_window))
        };
        if focused == Some(true) {
            popup.was_focused = true;
        }
        let lost_focus = popup.was_focused && focused == Some(false);
        if lost_focus || escape {
            self.popup = None;
            self.popup_closed_at = Some(Instant::now());
            ctx.request_repaint();
        }
        if open {
            self.show_window(ctx);
        }
    }

    fn sidebar(&mut self, ui: &mut Ui, s: &Strings, devices: &[DeviceEntry], scanned: bool) {
        // Brand.
        ui.horizontal(|ui| {
            ui.add_space(8.0);
            let (rect, _) = ui.allocate_exact_size(vec2(34.0, 34.0), Sense::hover());
            brand_mark(ui.painter(), rect);
            ui.add_space(4.0);
            ui.vertical(|ui| {
                ui.add_space(-2.0);
                ui.label(egui::RichText::new("Orbit").font(display(20.0)));
                ui.add_space(-8.0);
                ui.label(egui::RichText::new(s.app_sub).font(body(10.5)).color(theme::text3()));
            });
        });
        ui.add_space(14.0);
        ui.horizontal(|ui| {
            ui.add_space(8.0);
            ui.label(egui::RichText::new(s.devices).font(semibold(11.0)).color(theme::text3()));
        });
        ui.add_space(2.0);

        for d in devices {
            if device_card(ui, s, d, !self.show_settings && self.active.as_ref() == Some(&d.id)) {
                self.active = Some(d.id.clone());
                self.show_settings = false;
            }
        }
        if devices.is_empty() {
            ui.horizontal(|ui| {
                ui.add_space(8.0);
                ui.label(egui::RichText::new(if scanned { s.no_devices } else { s.searching }).color(theme::text3()));
            });
        }

        // Settings gear and language switch pinned to the very bottom, demo badge above.
        let bottom = ui.max_rect().max.y + 8.0;
        let gear = Rect::from_min_max(pos2(ui.max_rect().min.x, bottom - 46.0), pos2(ui.max_rect().min.x + 46.0, bottom));
        let switch = Rect::from_min_max(pos2(gear.max.x + 8.0, bottom - 46.0), pos2(ui.max_rect().max.x, bottom));
        if gear_button(ui, gear, self.show_settings) {
            self.show_settings = !self.show_settings;
        }
        // Scripted screenshots (README) leave the demo badge out.
        if self.demo && self.automation.is_none() {
            let badge_rect = Rect::from_min_size(pos2(gear.min.x, switch.min.y - 34.0), vec2(switch.max.x - gear.min.x, 22.0));
            badge(&mut ui.new_child(UiBuilder::new().max_rect(badge_rect)), &format!("{} {}", icon::FLASK, s.demo_mode), Tone::Warn);
        }
        if let Some(l) = language_switch(ui, switch, self.lang) {
            self.lang = l;
        }
    }

    fn device_screen(&mut self, ui: &mut Ui, s: &Strings, dev: &DeviceEntry) {
        let st = self.ui.entry(dev.id.clone()).or_default();
        let tabs: Vec<(&str, &str)> = match &dev.data {
            DeviceData::Keyboard(_) => vec![(icon::GAUGE, s.tab_performance), (icon::LIGHTBULB, s.tab_lighting), (icon::SETTINGS, s.tab_device)],
            DeviceData::Mouse(_) => vec![
                (icon::CROSSHAIR, s.tab_dpi),
                (icon::GAUGE, s.tab_sensor),
                (icon::POINTER, s.tab_buttons),
                (icon::LIGHTBULB, s.tab_lighting),
                (icon::MOON, s.tab_power),
            ],
        };
        st.tab = st.tab.min(tabs.len() - 1);

        // Header.
        let (subtitle, battery) = match &dev.data {
            DeviceData::Keyboard(k) => (
                k.info.as_ref().map(|i| format!("{} {} · {} {}", s.firmware, i.firmware, s.profile, k.profile + 1)).unwrap_or_else(|| "…".into()),
                None,
            ),
            DeviceData::Mouse(m) => (
                match (&m.identity, m.online) {
                    (Some(i), true) => format!("{} · {}", i.sensor.name, if i.wired { s.wired } else { s.wireless }),
                    _ => s.asleep.to_string(),
                },
                m.battery.filter(|_| m.online),
            ),
        };
        Frame::new().inner_margin(Margin { left: 32, right: 32, top: 34, bottom: 0 }).show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.vertical(|ui| {
                    ui.label(egui::RichText::new(&dev.name).font(display(26.0)));
                    ui.add_space(-6.0);
                    ui.label(egui::RichText::new(subtitle).font(body(13.0)).color(theme::text3()));
                });
                if let Some(b) = battery {
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| battery_badge(ui, b.level, b.charging));
                }
            });
        });

        // Tabs.
        ui.add_space(12.0);
        let tab_row = ui.allocate_exact_size(vec2(ui.available_width(), 44.0), Sense::hover()).0;
        ui.painter().hline(tab_row.x_range(), tab_row.max.y, Stroke::new(1.0, line()));
        let mut x = tab_row.min.x + 32.0;
        for (i, (glyph, label)) in tabs.iter().enumerate() {
            let text = format!("{glyph}  {label}");
            let g = ui.painter().layout_no_wrap(text, semibold(13.5), theme::text());
            let r = Rect::from_min_size(pos2(x, tab_row.min.y), vec2(g.size().x + 28.0, tab_row.height()));
            let resp = ui.interact(r, ui.scope_id().with(("tab", &dev.id, i)), Sense::click()).on_hover_cursor(CursorIcon::PointingHand);
            let active = st.tab == i;
            let color = if active {
                theme::text()
            } else if resp.hovered() {
                theme::text2()
            } else {
                theme::text3()
            };
            ui.painter().galley(pos2(r.min.x + 14.0, r.center().y - g.size().y / 2.0 - 2.0), g, color);
            if active {
                let bar = Rect::from_min_max(pos2(r.min.x + 10.0, r.max.y - 2.0), pos2(r.max.x - 10.0, r.max.y));
                gradient_rect(ui.painter(), bar, 1.0, 0.0, theme::accent_at);
            }
            if resp.clicked() {
                st.tab = i;
            }
            x = r.max.x + 4.0;
        }

        // Page content.
        let tab = st.tab;
        page_scroll_area(ui, ("page", &dev.id, tab)).show(ui, |ui| {
            Frame::new().inner_margin(Margin { left: 32, right: 32, top: 24, bottom: 40 }).show(ui, |ui| {
                ui.set_width(ui.available_width());
                let st = self.ui.get_mut(&dev.id).expect("created above");
                match &dev.data {
                    DeviceData::Keyboard(k) => match tab {
                        0 => pages::keyboard::performance(ui, s, self.lang, &self.worker, &dev.id, k, st),
                        1 => pages::keyboard::lighting(ui, s, self.lang, &self.worker, &dev.id, k, st),
                        _ => pages::keyboard::device(ui, s, &self.worker, &dev.id, k, &mut self.confirm),
                    },
                    DeviceData::Mouse(m) => pages::mouse::page(ui, s, &self.worker, &dev.id, m, st, tab),
                }
            });
        });
    }

    fn settings_screen(&mut self, ui: &mut Ui) {
        let s = self.lang.s();
        Frame::new().inner_margin(Margin { left: 32, right: 32, top: 34, bottom: 0 }).show(ui, |ui| {
            ui.label(egui::RichText::new(s.settings).font(display(26.0)));
            ui.add_space(-6.0);
            ui.label(egui::RichText::new(s.settings_sub).font(body(13.0)).color(theme::text3()));
        });
        // No tab row here, so the divider follows the header directly.
        ui.add_space(22.0);
        ui.painter().hline(ui.max_rect().x_range(), ui.cursor().min.y, Stroke::new(1.0, line()));
        ui.add_space(1.0);
        page_scroll_area(ui, "settings").show(ui, |ui| {
            Frame::new().inner_margin(Margin { left: 32, right: 32, top: 24, bottom: 40 }).show(ui, |ui| {
                ui.set_width(ui.available_width());
                crate::pages::settings::page(
                    ui,
                    s,
                    &self.worker,
                    crate::pages::settings::Settings { lang: &mut self.lang, theme: &mut self.theme_pref, glass: &mut self.glass, autostart: &mut self.autostart },
                );
            });
        });
    }

    fn confirm_dialog(&mut self, ctx: &egui::Context, s: &Strings) {
        let Some(c) = &self.confirm else { return };
        let (title, text) = match c {
            Confirm::ReportRate { .. } => (s.polling_rate, s.confirm_rate),
            Confirm::FactoryReset { .. } => (s.factory_reset, s.confirm_reset),
        };
        let mut decision = None;
        let modal = egui::Modal::new(egui::Id::unique("confirm"))
            .backdrop_color(theme::black(0.55))
            .frame(Frame::new().fill(theme::surface()).stroke(Stroke::new(1.0, theme::line_strong())).corner_radius(14).inner_margin(Margin::same(24)))
            .show(ctx, |ui| {
                ui.set_max_width(380.0);
                ui.label(egui::RichText::new(title).font(display(17.0)));
                ui.add_space(4.0);
                ui.label(egui::RichText::new(text).color(theme::text2()));
                ui.add_space(14.0);
                ui.horizontal(|ui| {
                    let danger = matches!(c, Confirm::FactoryReset { .. });
                    if widgets::button(ui, s.confirm, if danger { Kind::Danger } else { Kind::Primary }, false, true).clicked() {
                        decision = Some(true);
                    }
                    if widgets::button(ui, s.cancel, Kind::Ghost, false, true).clicked() {
                        decision = Some(false);
                    }
                });
            });
        if modal.should_close() && decision.is_none() {
            decision = Some(false);
        }
        if let Some(go) = decision {
            let c = self.confirm.take().expect("checked above");
            if go {
                pages::keyboard::confirmed(&self.worker, s, c);
            }
        }
    }
}

/// Tray tooltip: app name plus battery of every awake mouse.
fn tray_tooltip(worker: &Worker, s: &Strings) -> String {
    let mut text = String::from("Orbit");
    for d in &worker.lock().devices {
        if let DeviceData::Mouse(m) = &d.data {
            match (m.online, m.battery) {
                (true, Some(b)) => text.push_str(&format!("\n{}: {}%", d.name, b.level)),
                _ => text.push_str(&format!("\n{}: {}", d.name, s.asleep)),
            }
        }
    }
    text
}

pub fn popup_viewport() -> ViewportId {
    ViewportId::from_hash_of("orbit-tray-popup")
}

pub fn brand_mark(p: &egui::Painter, rect: Rect) {
    theme::radial_glow(p, rect.center(), vec2(30.0, 30.0), alpha(theme::ACCENT_MID, 0.45));
    let planet = rect.shrink(4.0);
    gradient_rect(p, planet, 13.0, 45.0, theme::accent_at);
    // Sphere shading and a specular highlight make the planet look glossy.
    gradient_rect(p, planet, 13.0, 90.0, |t| theme::black(0.28 * t * t));
    theme::radial_glow(p, planet.center() + vec2(-4.0, -5.0), vec2(8.0, 6.5), white(0.5));
    let c = rect.center();
    let (sin, cos) = (-24f32).to_radians().sin_cos();
    let pts: Vec<_> = (0..48)
        .map(|i| {
            let a = i as f32 / 48.0 * std::f32::consts::TAU;
            let (x, y) = (22.0 * a.cos(), 7.0 * a.sin());
            pos2(c.x + x * cos - y * sin, c.y + x * sin + y * cos)
        })
        .collect();
    p.add(Shape::closed_line(pts, Stroke::new(1.5, theme::ink(0.4))));
}

/// Vertical page scroller whose bar ends above the window's rounded bottom corner.
fn page_scroll_area(ui: &Ui, salt: impl egui::AsIdSalt) -> egui::ScrollArea {
    let area = ui.available_rect_before_wrap();
    let bar = Rect::from_min_max(pos2(area.min.x, area.min.y + 6.0), pos2(area.max.x, area.max.y - theme::WINDOW_RADIUS - 6.0));
    egui::ScrollArea::vertical().id_salt(salt).auto_shrink([false, false]).scroll_bar_rect(bar)
}

/// Square settings button next to the language switch; highlighted while settings are open.
fn gear_button(ui: &mut Ui, rect: Rect, active: bool) -> bool {
    let resp = ui.interact(rect, ui.scope_id().with("settings-gear"), Sense::click()).on_hover_cursor(CursorIcon::PointingHand);
    let p = ui.painter();
    if active {
        theme::glow(p, rect.translate(vec2(0.0, 4.0)).shrink(6.0), 12.0, alpha(theme::ACCENT_MID, 0.45), 12.0);
        gradient_rect(p, rect, 13.0, 45.0, theme::accent_at);
        theme::sheen(p, rect, 13.0, 0.22);
    } else {
        p.rect_filled(rect, 13.0, if resp.hovered() { theme::ink(0.08) } else { theme::well(0.35) });
        p.rect_stroke(rect, 13.0, Stroke::new(1.0, line()), StrokeKind::Inside);
    }
    let color = if active { Color32::WHITE } else if resp.hovered() { theme::text() } else { theme::text2() };
    p.text(rect.center(), Align2::CENTER_CENTER, icon::GEAR, body(19.0), color);
    resp.clicked()
}

/// Two-way DE/EN switch next to the settings button.
fn language_switch(ui: &mut Ui, rect: Rect, lang: Lang) -> Option<Lang> {
    let p = ui.painter();
    p.rect_filled(rect, 13.0, theme::well(0.35));
    p.rect_stroke(rect, 13.0, Stroke::new(1.0, line()), StrokeKind::Inside);
    let mut picked = None;
    let half = (rect.width() - 8.0) / 2.0;
    for (i, (l, label)) in [(Lang::De, "DE"), (Lang::En, "EN")].into_iter().enumerate() {
        let r = Rect::from_min_size(pos2(rect.min.x + 4.0 + i as f32 * half, rect.min.y + 4.0), vec2(half, rect.height() - 8.0));
        let resp = ui.interact(r, ui.scope_id().with(("lang", i)), Sense::click()).on_hover_cursor(CursorIcon::PointingHand);
        let active = lang == l;
        let p = ui.painter();
        if active {
            p.rect_filled(r, 10.0, theme::seg_active());
            p.rect_stroke(r, 10.0, Stroke::new(1.0, theme::line_strong()), StrokeKind::Inside);
        } else if resp.hovered() {
            p.rect_filled(r, 10.0, theme::ink(0.04));
        }
        let color = if active { theme::text() } else { theme::text2() };
        p.text(r.center(), Align2::CENTER_CENTER, label, semibold(15.0), color);
        if resp.clicked() && !active {
            picked = Some(l);
        }
    }
    picked
}

fn device_card(ui: &mut Ui, s: &Strings, d: &DeviceEntry, active: bool) -> bool {
    let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 64.0), Sense::click());
    let resp = resp.on_hover_cursor(CursorIcon::PointingHand);
    let p = ui.painter();
    if active {
        p.rect_filled(rect, 14.0, theme::ink(0.06));
        p.rect_stroke(rect, 14.0, Stroke::new(1.0, theme::line_strong()), StrokeKind::Inside);
    } else if resp.hovered() {
        p.rect_filled(rect, 14.0, theme::ink(0.035));
    }
    let icon_rect = Rect::from_min_size(rect.min + vec2(12.0, 12.0), vec2(40.0, 40.0));
    if active {
        theme::glow(p, icon_rect.translate(vec2(0.0, 5.0)).shrink(5.0), 11.0, alpha(theme::ACCENT_MID, 0.5), 12.0);
        gradient_rect(p, icon_rect, 11.0, 45.0, theme::accent_at);
        theme::sheen(p, icon_rect, 11.0, 0.24);
    } else {
        p.rect_filled(icon_rect, 11.0, theme::ink(0.06));
        p.rect_stroke(icon_rect, 11.0, Stroke::new(1.0, line()), StrokeKind::Inside);
    }
    let glyph = if matches!(d.data, DeviceData::Keyboard(_)) { icon::KEYBOARD } else { icon::MOUSE };
    p.text(icon_rect.center(), Align2::CENTER_CENTER, glyph, body(20.0), if active { Color32::WHITE } else { theme::text2() });
    p.text(pos2(icon_rect.max.x + 12.0, rect.min.y + 22.0), Align2::LEFT_CENTER, &d.name, semibold(13.5), theme::text());

    let (online, meta, battery) = match &d.data {
        DeviceData::Keyboard(_) => (true, format!("{} · {}", s.connected, s.wired), None),
        DeviceData::Mouse(m) => (
            m.online,
            if m.online { if m.identity.is_some_and(|i| i.wired) { s.wired } else { s.wireless }.to_string() } else { s.asleep.to_string() },
            m.battery.filter(|_| m.online),
        ),
    };
    let dot = pos2(icon_rect.max.x + 15.0, rect.min.y + 43.0);
    if online {
        p.circle_filled(dot, 6.0, alpha(theme::GOOD, 0.25));
    }
    p.circle_filled(dot, 3.5, if online { theme::GOOD } else { theme::text3() });
    let meta_g = p.layout_no_wrap(meta, body(12.0), theme::text3());
    let meta_w = meta_g.size().x;
    p.galley(pos2(dot.x + 9.0, dot.y - meta_g.size().y / 2.0), meta_g, theme::text3());
    if let Some(b) = battery {
        let mut child = ui.new_child(UiBuilder::new().max_rect(Rect::from_min_size(pos2(dot.x + 18.0 + meta_w, dot.y - 11.0), vec2(80.0, 22.0))));
        battery_badge(&mut child, b.level, b.charging);
    }
    resp.clicked()
}

fn empty_state(ui: &mut Ui, s: &Strings, scanned: bool) {
    let rect = ui.max_rect();
    let p = ui.painter();
    let c = pos2(rect.center().x, rect.center().y - 60.0);
    let t = ui.input(|i| i.time) as f32;
    theme::radial_glow(p, c, vec2(70.0, 70.0), alpha(theme::ACCENT_MID, 0.5));
    gradient_rect(p, Rect::from_center_size(c, vec2(44.0, 44.0)), 22.0, 45.0, theme::accent_at);
    let pts: Vec<_> = (0..64).map(|i| i as f32 / 64.0 * std::f32::consts::TAU).map(|a| pos2(c.x + 60.0 * a.cos(), c.y + 60.0 * a.sin())).collect();
    for (i, w) in pts.windows(2).enumerate() {
        if i % 2 == 0 {
            p.line_segment([w[0], w[1]], Stroke::new(1.0, theme::ink(0.18)));
        }
    }
    let a = t * 0.7;
    let moon = pos2(c.x + 60.0 * a.cos(), c.y + 60.0 * a.sin());
    p.circle_filled(moon, 9.0, theme::ink(0.25));
    p.circle_filled(moon, 4.5, theme::text());
    p.text(pos2(c.x, c.y + 100.0), Align2::CENTER_CENTER, if scanned { s.empty_title } else { s.searching }, display(20.0), theme::text());
    let g = p.layout(s.empty_text.to_string(), body(13.5), theme::text3(), 420.0);
    p.galley(pos2(c.x - g.size().x / 2.0, c.y + 122.0), g, theme::text3());
    ui.ctx().request_repaint_after(Duration::from_millis(16));
}

fn toast_stack(ctx: &egui::Context, toasts: &[(String, bool)]) {
    if toasts.is_empty() {
        return;
    }
    egui::Area::new(egui::Id::unique("toasts")).order(egui::Order::Foreground).anchor(Align2::RIGHT_BOTTOM, vec2(-22.0, -22.0)).interactable(false).show(ctx, |ui| {
        for (text, error) in toasts {
            Frame::new()
                .fill(theme::alpha(theme::surface(), 0.95))
                .stroke(Stroke::new(1.0, if *error { alpha(theme::BAD, 0.45) } else { theme::line_strong() }))
                .corner_radius(12)
                .inner_margin(Margin::symmetric(14, 11))
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        let (glyph, color) = if *error { (icon::ALERT, theme::BAD) } else { (icon::CHECK, theme::GOOD) };
                        ui.label(egui::RichText::new(glyph).font(body(16.0)).color(color));
                        ui.label(egui::RichText::new(text).font(body(13.0)));
                    });
                });
            ui.add_space(4.0);
        }
    });
}
