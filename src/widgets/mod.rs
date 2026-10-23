//! Custom-drawn controls in the Orbit look.

pub mod keyboard_view;
pub mod travel_gauge;

use egui::{pos2, vec2, Align, Color32, CornerRadius, CursorIcon, Frame, Layout, Margin, Rect, Response, Sense, Stroke, StrokeKind, TextEdit, Ui, UiBuilder};

use crate::theme::{self, accent_at, alpha, body, display, gradient_rect, line, line_strong, mono, panel_fill, semibold, white};

// ── Panel ──────────────────────────────────────────────────────────────

pub struct Panel<'a> {
    icon: &'a str,
    title: &'a str,
    desc: Option<&'a str>,
    actions: Option<Box<dyn FnOnce(&mut Ui) + 'a>>,
}

impl<'a> Panel<'a> {
    pub fn new(icon: &'a str, title: &'a str) -> Self {
        Self { icon, title, desc: None, actions: None }
    }

    pub fn desc(mut self, desc: &'a str) -> Self {
        self.desc = Some(desc);
        self
    }

    pub fn actions(mut self, f: impl FnOnce(&mut Ui) + 'a) -> Self {
        self.actions = Some(Box::new(f));
        self
    }

    pub fn show<R>(self, ui: &mut Ui, contents: impl FnOnce(&mut Ui) -> R) -> R {
        Frame::new()
            .fill(panel_fill())
            .stroke(Stroke::new(1.0, line()))
            .corner_radius(CornerRadius::same(14))
            .inner_margin(Margin::same(20))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                if !self.title.is_empty() {
                    ui.horizontal(|ui| {
                        ui.set_min_height(28.0);
                        if !self.icon.is_empty() {
                            ui.label(egui::RichText::new(self.icon).font(body(17.0)).color(theme::ACCENT));
                        }
                        ui.label(egui::RichText::new(self.title).font(display(15.0)).color(theme::text()));
                        if let Some(actions) = self.actions {
                            ui.with_layout(Layout::right_to_left(Align::Center), actions);
                        }
                    });
                    if let Some(d) = self.desc {
                        ui.add_space(-4.0);
                        ui.label(egui::RichText::new(d).font(body(12.5)).color(theme::text3()));
                    }
                    ui.add_space(6.0);
                }
                contents(ui)
            })
            .inner
    }
}

// ── Buttons ────────────────────────────────────────────────────────────

#[derive(Clone, Copy, PartialEq)]
pub enum Kind {
    Normal,
    Primary,
    Danger,
    Ghost,
}

pub fn button(ui: &mut Ui, text: &str, kind: Kind, small: bool, enabled: bool) -> Response {
    let font = semibold(if small { 12.5 } else { 13.5 });
    let galley = ui.painter().layout_no_wrap(text.to_string(), font, theme::text());
    let pad = if small { 10.0 } else { 14.0 };
    let size = vec2(galley.size().x + pad * 2.0, if small { 30.0 } else { 36.0 });
    let (rect, resp) = ui.allocate_exact_size(size, if enabled { Sense::click() } else { Sense::hover() });
    let p = ui.painter();
    let hovered = enabled && resp.hovered();
    let r = 9.0;
    match kind {
        Kind::Primary => {
            theme::glow(p, rect.translate(vec2(0.0, 5.0)).shrink(6.0), r, alpha(theme::ACCENT_MID, if enabled { 0.55 } else { 0.0 }), 14.0);
            gradient_rect(p, rect, r, 30.0, |t| alpha(accent_at(t), if enabled { 1.0 } else { 0.4 }));
            if hovered {
                p.rect_filled(rect, r, white(0.12));
            }
        }
        Kind::Ghost => {
            if hovered {
                p.rect_filled(rect, r, theme::ink(0.05));
            }
        }
        _ => {
            p.rect_filled(rect, r, if hovered { theme::ink(0.08) } else { theme::ink(0.06) });
            let border = if kind == Kind::Danger { alpha(theme::BAD, 0.4) } else { line_strong() };
            p.rect_stroke(rect, r, Stroke::new(1.0, border), StrokeKind::Inside);
        }
    }
    let color = match kind {
        Kind::Danger => theme::BAD,
        Kind::Primary => Color32::WHITE,
        Kind::Ghost => theme::text2(),
        Kind::Normal => theme::text(),
    };
    let color = if enabled { color } else { alpha(color, 0.45) };
    p.galley(pos2(rect.center().x - galley.size().x / 2.0, rect.center().y - galley.size().y / 2.0), galley, color);
    if enabled {
        resp.on_hover_cursor(CursorIcon::PointingHand)
    } else {
        resp
    }
}

// ── Segmented control ──────────────────────────────────────────────────

/// Returns the newly picked value, if any.
pub fn segmented<T: PartialEq + Copy>(ui: &mut Ui, value: Option<T>, options: &[(T, String)], enabled: bool) -> Option<T> {
    let font = semibold(12.5);
    let galleys: Vec<_> = options.iter().map(|(_, l)| ui.painter().layout_no_wrap(l.clone(), font.clone(), theme::text())).collect();
    let widths: Vec<f32> = galleys.iter().map(|g| g.size().x + 24.0).collect();
    let total = widths.iter().sum::<f32>() + 6.0 + 2.0 * (options.len().saturating_sub(1)) as f32;
    let (rect, _) = ui.allocate_exact_size(vec2(total, 34.0), Sense::hover());
    let p = ui.painter().clone();
    p.rect_filled(rect, 11.0, theme::well(0.35));
    p.rect_stroke(rect, 11.0, Stroke::new(1.0, line()), StrokeKind::Inside);
    let mut x = rect.min.x + 3.0;
    let mut picked = None;
    for (i, ((v, _), g)) in options.iter().zip(galleys).enumerate() {
        let r = Rect::from_min_size(pos2(x, rect.min.y + 3.0), vec2(widths[i], 28.0));
        let resp = ui.interact(r, ui.scope_id().with(("seg", rect.min.x as i32, rect.min.y as i32, i)), if enabled { Sense::click() } else { Sense::hover() });
        let active = value == Some(*v);
        if active {
            p.rect_filled(r, 8.0, theme::seg_active());
            p.rect_stroke(r, 8.0, Stroke::new(1.0, line_strong()), StrokeKind::Inside);
        } else if enabled && resp.hovered() {
            p.rect_filled(r, 8.0, theme::ink(0.04));
        }
        let color = if active { theme::text() } else { theme::text2() };
        let color = if enabled { color } else { alpha(color, 0.5) };
        p.galley(pos2(r.center().x - g.size().x / 2.0, r.center().y - g.size().y / 2.0), g, color);
        if resp.clicked() && !active {
            picked = Some(*v);
        }
        if enabled {
            resp.on_hover_cursor(CursorIcon::PointingHand);
        }
        x += widths[i] + 2.0;
    }
    picked
}

// ── Toggle ─────────────────────────────────────────────────────────────

/// Returns the new state when clicked.
pub fn toggle(ui: &mut Ui, on: bool, enabled: bool) -> Option<bool> {
    let (rect, resp) = ui.allocate_exact_size(vec2(42.0, 24.0), if enabled { Sense::click() } else { Sense::hover() });
    let t = ui.ctx().animate_bool_with_time(resp.id, on, 0.15);
    let p = ui.painter();
    let fade = if enabled { 1.0 } else { 0.4 };
    p.rect_filled(rect, 12.0, theme::toggle_off());
    if t > 0.0 {
        gradient_rect(p, rect, 12.0, 0.0, |x| alpha(accent_at(x), t * fade));
    }
    p.rect_stroke(rect, 12.0, Stroke::new(1.0, alpha(Color32::WHITE, 0.16 * (1.0 - t))), StrokeKind::Inside);
    let cx = egui::lerp(rect.min.x + 12.0..=rect.max.x - 12.0, t);
    let knob = pos2(cx, rect.center().y);
    p.circle_filled(knob, 9.0, if theme::is_light() { Color32::WHITE } else { theme::lerp(Color32::from_rgb(207, 211, 224), Color32::WHITE, t) });
    if theme::is_light() {
        p.circle_stroke(knob, 9.0, Stroke::new(1.0, theme::ink(0.18 * (1.0 - t))));
    }
    if enabled {
        resp.clone().on_hover_cursor(CursorIcon::PointingHand);
    }
    resp.clicked().then_some(!on)
}

// ── Slider ─────────────────────────────────────────────────────────────

pub struct SliderOut {
    /// Live value while dragging.
    pub changed: Option<f32>,
    /// Final value on release or text entry.
    pub committed: Option<f32>,
}

pub struct Slider<'a> {
    pub label: &'a str,
    pub min: f32,
    pub max: f32,
    pub step: f32,
    pub decimals: usize,
    pub unit: &'a str,
    pub enabled: bool,
}

impl<'a> Slider<'a> {
    pub fn new(label: &'a str, min: f32, max: f32, step: f32) -> Self {
        Self { label, min, max, step, decimals: 0, unit: "", enabled: true }
    }
    pub fn decimals(mut self, d: usize) -> Self {
        self.decimals = d;
        self
    }
    pub fn unit(mut self, u: &'a str) -> Self {
        self.unit = u;
        self
    }
    pub fn enabled(mut self, e: bool) -> Self {
        self.enabled = e;
        self
    }

    fn snap(&self, v: f32) -> f32 {
        let stepped = ((v - self.min) / self.step).round() * self.step + self.min;
        let f = 10f32.powi(self.decimals.max(3) as i32);
        (stepped.clamp(self.min, self.max) * f).round() / f
    }

    pub fn show(self, ui: &mut Ui, value: f32) -> SliderOut {
        let id = ui.make_persistent_id(("slider", self.label));
        let mut out = SliderOut { changed: None, committed: None };
        let drag_id = id.with("drag");
        let mut shown: f32 = ui.data(|d| d.get_temp::<f32>(drag_id)).unwrap_or(value);

        // Editable value on the right, laid out first so it always keeps its width;
        // the label gets what is left and is shortened with "…" if it doesn't fit.
        ui.allocate_ui_with_layout(vec2(ui.available_width(), ui.spacing().interact_size.y), Layout::right_to_left(Align::Center), |ui| {
            if !self.unit.is_empty() {
                ui.label(egui::RichText::new(self.unit).font(mono(12.5)).color(theme::text2()));
            }
            let text_id = id.with("text");
            let mut text = ui.data(|d| d.get_temp::<String>(text_id)).unwrap_or_else(|| format!("{:.*}", self.decimals, shown));
            let resp = ui.add_enabled(
                self.enabled,
                TextEdit::singleline(&mut text)
                    .font(mono(13.0))
                    .desired_width(64.0)
                    .align(egui::Align2::RIGHT_CENTER)
                    .margin(Margin::symmetric(7, 3))
                    .background_color(theme::well(0.35)),
            );
            if resp.has_focus() {
                ui.data_mut(|d| d.insert_temp(text_id, text.clone()));
            }
            if resp.lost_focus() {
                if let Ok(v) = text.replace(',', ".").trim().parse::<f32>() {
                    let v = self.snap(v);
                    shown = v;
                    out.committed = Some(v);
                    out.changed = Some(v);
                }
                ui.data_mut(|d| d.remove::<String>(text_id));
            }
            ui.with_layout(Layout::left_to_right(Align::Center), |ui| {
                let label = egui::RichText::new(self.label).font(semibold(13.5)).color(if self.enabled { theme::text() } else { theme::text3() });
                ui.add(egui::Label::new(label).truncate());
            });
        });

        // Track.
        let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 22.0), if self.enabled { Sense::click_and_drag() } else { Sense::hover() });
        if self.enabled && (resp.dragged() || resp.drag_started() || resp.clicked()) {
            if let Some(pos) = resp.interact_pointer_pos() {
                let t = ((pos.x - rect.min.x - 9.0) / (rect.width() - 18.0)).clamp(0.0, 1.0);
                let v = self.snap(self.min + t * (self.max - self.min));
                if v != shown || resp.drag_started() {
                    out.changed = Some(v);
                }
                shown = v;
                ui.data_mut(|d| d.insert_temp(drag_id, v));
            }
        }
        if resp.drag_stopped() || resp.clicked() {
            out.committed = Some(shown);
            ui.data_mut(|d| d.remove::<f32>(drag_id));
        }

        let p = ui.painter();
        let fade = if self.enabled { 1.0 } else { 0.4 };
        let track = Rect::from_center_size(rect.center(), vec2(rect.width() - 18.0, 6.0)).expand2(vec2(9.0, 0.0));
        p.rect_filled(track, 3.0, theme::ink(0.09));
        let t = ((shown - self.min) / (self.max - self.min)).clamp(0.0, 1.0);
        let fill = Rect::from_min_max(track.min, pos2(track.min.x + 9.0 + t * (track.width() - 18.0), track.max.y));
        gradient_rect(p, fill, 3.0, 0.0, |x| alpha(theme::lerp(theme::ACCENT, theme::ACCENT2, x), fade));
        let knob = pos2(fill.max.x, rect.center().y);
        p.circle_filled(knob, 13.0, alpha(theme::ACCENT, 0.25 * fade));
        p.circle_filled(knob, 9.0, alpha(Color32::WHITE, fade.max(0.6)));
        if theme::is_light() {
            p.circle_stroke(knob, 9.0, Stroke::new(1.0, theme::ink(0.18)));
        }
        if self.enabled {
            resp.on_hover_cursor(CursorIcon::PointingHand);
        }
        out
    }
}

// ── Small pieces ───────────────────────────────────────────────────────

#[derive(Clone, Copy, PartialEq)]
pub enum Tone {
    Neutral,
    Good,
    Warn,
}

pub fn badge(ui: &mut Ui, text: &str, tone: Tone) -> Response {
    let (fg, bg, border) = match tone {
        Tone::Good => (theme::GOOD, alpha(theme::GOOD, 0.08), alpha(theme::GOOD, 0.3)),
        Tone::Warn => (theme::WARN, alpha(theme::WARN, 0.08), alpha(theme::WARN, 0.3)),
        Tone::Neutral => (theme::text2(), theme::ink(0.06), line()),
    };
    let galley = ui.painter().layout_no_wrap(text.to_string(), semibold(11.5), fg);
    let (rect, resp) = ui.allocate_exact_size(vec2(galley.size().x + 16.0, 22.0), Sense::hover());
    let p = ui.painter();
    p.rect_filled(rect, 11.0, bg);
    p.rect_stroke(rect, 11.0, Stroke::new(1.0, border), StrokeKind::Inside);
    p.galley(pos2(rect.min.x + 8.0, rect.center().y - galley.size().y / 2.0), galley, fg);
    resp
}

pub fn battery_badge(ui: &mut Ui, level: u8, charging: bool) {
    let glyph = if charging {
        theme::icon::BATTERY_CHARGING
    } else if level > 70 {
        theme::icon::BATTERY_FULL
    } else if level > 30 {
        theme::icon::BATTERY_MEDIUM
    } else {
        theme::icon::BATTERY_LOW
    };
    let tone = if charging {
        Tone::Good
    } else if level <= 20 {
        Tone::Warn
    } else {
        Tone::Neutral
    };
    badge(ui, &format!("{glyph} {level}%"), tone);
}

/// A settings row: label (+ hint) on the left, control on the right, hairline above.
pub fn row(ui: &mut Ui, first: bool, label: &str, hint: Option<&str>, control: impl FnOnce(&mut Ui)) {
    if !first {
        let y = ui.cursor().min.y;
        ui.painter().hline(ui.max_rect().x_range(), y, Stroke::new(1.0, line()));
        ui.add_space(10.0);
    }
    // Control first (right-aligned), then the text takes whatever width is left.
    ui.allocate_ui_with_layout(vec2(ui.available_width(), 34.0), Layout::right_to_left(Align::Center), |ui| {
        control(ui);
        ui.add_space(12.0);
        let width = ui.available_width();
        // Centre the text block on the 34 px control line.
        let mut text_h = ui.painter().layout(label.to_string(), semibold(13.5), theme::text(), width).size().y;
        if let Some(h) = hint {
            text_h += 2.0 + ui.painter().layout(h.to_string(), body(12.0), theme::text3(), width).size().y;
        }
        ui.allocate_ui_with_layout(vec2(width, text_h.max(34.0)), Layout::top_down(Align::Min), |ui| {
            ui.add_space(((34.0 - text_h) / 2.0).max(0.0));
            ui.spacing_mut().item_spacing.y = 2.0;
            ui.label(egui::RichText::new(label).font(semibold(13.5)));
            if let Some(h) = hint {
                ui.label(egui::RichText::new(h).font(body(12.0)).color(theme::text3()));
            }
        });
    });
    ui.add_space(4.0);
}

pub fn stat(ui: &mut Ui, label: &str, value: &str) {
    ui.vertical(|ui| {
        ui.label(egui::RichText::new(label.to_uppercase()).font(semibold(10.5)).color(theme::text3()));
        ui.label(egui::RichText::new(value).font(mono(14.5)).color(theme::text()));
    });
}

/// Lays out children in `cols` equal columns per row, top-aligned.
pub fn columns_grid(ui: &mut Ui, cols: usize, gap: f32, items: usize, mut cell: impl FnMut(&mut Ui, usize)) {
    let width = ui.available_width();
    let cw = (width - gap * (cols as f32 - 1.0)) / cols as f32;
    let mut i = 0;
    while i < items {
        let top = ui.cursor().min.y;
        let mut bottom = top;
        for c in 0..cols {
            if i >= items {
                break;
            }
            let rect = Rect::from_min_size(pos2(ui.max_rect().min.x + c as f32 * (cw + gap), top), vec2(cw, 0.0));
            let mut child = ui.new_child(UiBuilder::new().max_rect(Rect::from_min_size(rect.min, vec2(cw, f32::INFINITY))).layout(Layout::top_down(Align::Min)));
            cell(&mut child, i);
            bottom = bottom.max(child.min_rect().max.y);
            i += 1;
        }
        ui.advance_cursor_after_rect(Rect::from_min_max(pos2(ui.max_rect().min.x, top), pos2(ui.max_rect().min.x + width, bottom)));
        if i < items {
            ui.add_space(gap - ui.spacing().item_spacing.y);
        }
    }
}

/// Lays out cards in `cols` equal columns, each card going into the column that
/// is currently shortest, so cards of different heights pack without gaps.
pub fn masonry(ui: &mut Ui, cols: usize, gap: f32, items: usize, mut cell: impl FnMut(&mut Ui, usize)) {
    let width = ui.available_width();
    let cw = (width - gap * (cols as f32 - 1.0)) / cols as f32;
    let (left, top) = (ui.max_rect().min.x, ui.cursor().min.y);
    let mut bottoms = vec![top; cols];
    for i in 0..items {
        // Near-ties go to the leftmost column, so small height changes don't make cards jump.
        let shortest = bottoms.iter().copied().fold(f32::INFINITY, f32::min);
        let c = bottoms.iter().position(|&b| b <= shortest + 24.0).unwrap_or(0);
        let y = if bottoms[c] > top { bottoms[c] + gap } else { top };
        let rect = Rect::from_min_size(pos2(left + c as f32 * (cw + gap), y), vec2(cw, f32::INFINITY));
        let mut child = ui.new_child(UiBuilder::new().max_rect(rect).layout(Layout::top_down(Align::Min)));
        cell(&mut child, i);
        bottoms[c] = child.min_rect().max.y;
    }
    let bottom = bottoms.into_iter().fold(top, f32::max);
    ui.advance_cursor_after_rect(Rect::from_min_max(pos2(left, top), pos2(left + width, bottom)));
}

pub fn spinner_row(ui: &mut Ui, text: &str) {
    ui.horizontal(|ui| {
        ui.add(egui::Spinner::new().size(14.0).color(theme::ACCENT2));
        ui.label(egui::RichText::new(text).color(theme::text2()));
    });
}

/// Two columns side by side: flexible left, fixed-width right. Fill `left`
/// and `right`, then call [`Split::finish`] to advance the parent layout.
pub struct Split {
    pub left: Ui,
    pub right: Ui,
    top: egui::Pos2,
    width: f32,
}

pub fn split(ui: &mut Ui, right_width: f32, gap: f32) -> Split {
    let top = ui.cursor().min;
    let width = ui.available_width();
    let left_rect = Rect::from_min_size(top, vec2(width - right_width - gap, f32::INFINITY));
    let right_rect = Rect::from_min_size(pos2(top.x + width - right_width, top.y), vec2(right_width, f32::INFINITY));
    Split {
        left: ui.new_child(UiBuilder::new().max_rect(left_rect).layout(Layout::top_down(Align::Min))),
        right: ui.new_child(UiBuilder::new().max_rect(right_rect).layout(Layout::top_down(Align::Min))),
        top,
        width,
    }
}

impl Split {
    pub fn finish(self, ui: &mut Ui) {
        let bottom = self.left.min_rect().max.y.max(self.right.min_rect().max.y);
        ui.advance_cursor_after_rect(Rect::from_min_max(self.top, pos2(self.top.x + self.width, bottom)));
    }
}
