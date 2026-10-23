//! Interactive keyboard: renders the physical layout and handles key selection
//! by click, ctrl-click (toggle) and dragging across keys.

use std::collections::HashSet;

use egui::epaint::Mesh;
use egui::{pos2, vec2, Align2, Color32, CursorIcon, Painter, Pos2, Rect, Sense, Stroke, StrokeKind, Ui, Vec2};

use crate::protocol::models::KeyDef;
use crate::theme::{self, alpha, gradient_rect, mono, semibold, white};

pub type KeyId = (u8, u8);

/// Selection drag in progress: whether keys get added or removed, starting from `base`.
pub struct DragSelect {
    adding: bool,
    base: HashSet<KeyId>,
}

/// What a key shows in its lower-right corner.
pub enum KeyValue {
    None,
    Text(String),
    Rapid(String),
}

pub struct KeyboardView<'a> {
    pub keys: &'a [KeyDef],
    pub tint: &'a dyn Fn(&KeyDef) -> Option<Color32>,
    pub value: &'a dyn Fn(&KeyDef) -> KeyValue,
    /// Live press depth as a fraction of full travel, and whether the key actuated.
    pub live: &'a dyn Fn(&KeyDef) -> Option<(f32, bool)>,
}

impl KeyboardView<'_> {
    /// Draws the board; returns the new selection if it changed.
    pub fn show(&self, ui: &mut Ui, selected: &HashSet<KeyId>, drag: &mut Option<DragSelect>) -> Option<HashSet<KeyId>> {
        let cols = self.keys.iter().map(|k| k.x + k.w).fold(0.0, f32::max);
        let rows = self.keys.iter().map(|k| k.y + k.h).fold(0.0, f32::max);
        let pad = 18.0;
        let avail = ui.available_width();
        let unit = ((avail - pad * 2.0) / cols).floor().min(64.0);
        let frame_size = vec2(avail, rows * unit + pad * 2.0);
        let (frame, _) = ui.allocate_exact_size(frame_size, Sense::hover());

        // Board frame.
        let p = ui.painter();
        theme::glow(p, frame.translate(vec2(0.0, 18.0)).shrink(30.0), 18.0, theme::shadow(0.7), 40.0);
        p.rect_filled(frame, 18.0, theme::card_solid());
        gradient_rect(p, frame, 18.0, 90.0, |t| white(0.045 * (1.0 - t) + 0.01 * t));
        p.rect_stroke(frame, 18.0, Stroke::new(1.0, theme::line()), StrokeKind::Inside);

        let origin = pos2(frame.center().x - cols * unit / 2.0, frame.min.y + pad);
        let gap = (unit * 0.07).max(3.0);
        let key_rects: Vec<(KeyId, Rect, Option<Rect>)> = self
            .keys
            .iter()
            .map(|k| {
                if k.iso_enter {
                    // Top part 1.5u wide, lower part 1.25u wide, shifted right by 0.25u.
                    let top = Rect::from_min_size(origin + vec2(k.x * unit, k.y * unit), vec2(k.w * unit, unit)).shrink(gap / 2.0);
                    let bottom = Rect::from_min_max(pos2(top.min.x + 0.25 * unit, top.center().y), pos2(top.max.x, origin.y + (k.y + 2.0) * unit - gap / 2.0));
                    ((k.row, k.col), top, Some(bottom))
                } else {
                    let r = Rect::from_min_size(origin + vec2(k.x * unit, k.y * unit), vec2(k.w * unit, k.h * unit)).shrink(gap / 2.0);
                    ((k.row, k.col), r, None)
                }
            })
            .collect();

        // Interaction over the whole board.
        let board = Rect::from_min_size(origin, vec2(cols * unit, rows * unit));
        let resp = ui.interact(board, ui.scope_id().with("kbd-board"), Sense::click_and_drag());
        let pointer = ui.input(|i| i.pointer.interact_pos());
        let hit = |pos: Pos2| key_rects.iter().find(|(_, r, b)| r.contains(pos) || b.is_some_and(|b| b.contains(pos))).map(|(id, ..)| *id);
        let hovered = pointer.filter(|_| resp.hovered() || resp.dragged()).and_then(hit);
        if hovered.is_some() {
            ui.ctx().set_cursor_icon(CursorIcon::PointingHand);
        }

        let mut changed = None;
        let pressed = ui.input(|i| i.pointer.primary_pressed());
        let down = ui.input(|i| i.pointer.primary_down());
        let ctrl = ui.input(|i| i.modifiers.command || i.modifiers.shift);
        if pressed && resp.hovered() {
            if let Some(id) = hovered {
                let base = if ctrl { selected.clone() } else { HashSet::new() };
                *drag = Some(DragSelect { adding: !(ctrl && selected.contains(&id)), base });
            }
        }
        if let Some(d) = drag.as_mut() {
            if let Some(id) = pointer.and_then(hit) {
                if d.adding {
                    d.base.insert(id);
                } else {
                    d.base.remove(&id);
                }
                if &d.base != selected {
                    changed = Some(d.base.clone());
                }
            }
            if !down {
                *drag = None;
            }
        }
        let shown = changed.as_ref().unwrap_or(selected);

        // Keys.
        let p = ui.painter();
        for (k, (id, top, bottom)) in self.keys.iter().zip(&key_rects) {
            let sel = shown.contains(id);
            let hov = hovered == Some(*id);
            let base = (self.tint)(k).unwrap_or(theme::key_base());
            let parts: Vec<Rect> = std::iter::once(*top).chain(*bottom).collect();
            let r = 8.0;
            if sel {
                for part in &parts {
                    theme::glow(p, part.expand(2.0), r, alpha(theme::ACCENT2, 0.28), 12.0);
                }
            }
            // ISO enter is one L-shaped outline with rounded outer and inner corners.
            let iso = bottom.map(|b| rounded_polygon(&[top.left_top(), top.right_top(), b.right_bottom(), b.left_bottom(), pos2(b.min.x, top.max.y), top.left_bottom()], r, 6));
            match (&iso, bottom) {
                (Some(outline), Some(b)) => {
                    let center = pos2((b.min.x + top.max.x) / 2.0, top.center().y);
                    let (y0, y1) = (top.min.y, b.max.y);
                    fill_star(p, outline, center, |_| base);
                    fill_star(p, outline, center, |q| {
                        let t = (q.y - y0) / (y1 - y0);
                        white(0.07 * (1.0 - t) + 0.015 * t)
                    });
                }
                _ => {
                    p.rect_filled(*top, r, base);
                    gradient_rect(p, *top, r, 90.0, |t| white(0.07 * (1.0 - t) + 0.015 * t));
                }
            }
            // Pressed keys fill up from the bottom with their live travel.
            if let Some((frac, actuated)) = (self.live)(k) {
                let part = parts.last().copied().unwrap_or(*top);
                let level = Rect::from_min_max(pos2(part.min.x, part.max.y - part.height() * frac.clamp(0.0, 1.0)), part.max);
                let color = if actuated { theme::GOOD } else { theme::ACCENT2 };
                p.with_clip_rect(part).rect_filled(level, r, alpha(color, 0.32));
            }
            // Bottom edge shadow for a bit of depth.
            let lowest = parts.last().copied().unwrap_or(*top);
            p.rect_filled(Rect::from_min_max(pos2(lowest.min.x + 3.0, lowest.max.y - 3.0), pos2(lowest.max.x - 3.0, lowest.max.y)), 2.0, theme::shadow(0.3));
            let border = if sel {
                Stroke::new(1.5, theme::ACCENT2)
            } else if hov {
                Stroke::new(1.0, theme::ink(0.22))
            } else {
                Stroke::new(1.0, theme::ink(0.09))
            };
            if let Some(outline) = &iso {
                // Keep the stroke inside the outline, like `StrokeKind::Inside` on the other keys.
                let inset: Vec<Pos2> = outline.iter().zip(normals(outline)).map(|(q, n)| *q - n * (border.width / 2.0)).collect();
                p.add(egui::Shape::closed_line(inset, border));
            } else {
                p.rect_stroke(*top, r, border, StrokeKind::Inside);
            }

            let text_color = if sel { theme::selected_text() } else if hov { theme::text() } else { theme::text2() };
            let font_size = (unit * 0.2).clamp(9.0, 11.5);
            p.text(top.left_top() + vec2(7.0, 6.0), Align2::LEFT_TOP, &k.label, semibold(font_size), text_color);
            let value_anchor = parts.last().map(|r| r.right_bottom()).unwrap_or(top.right_bottom()) + vec2(-7.0, -7.0);
            let value_color = if sel { theme::accent_text() } else { theme::text3() };
            match (self.value)(k) {
                KeyValue::Text(s) => {
                    p.text(value_anchor, Align2::RIGHT_BOTTOM, s, mono(font_size - 1.0), value_color);
                }
                KeyValue::Rapid(s) => {
                    p.text(value_anchor, Align2::RIGHT_BOTTOM, format!("{} {s}", theme::icon::ZAP), mono(font_size - 1.0), value_color);
                }
                KeyValue::None => {}
            }
        }
        changed
    }
}

/// Rounds every right-angle corner of a polygon — convex or concave — with
/// radius `r` (limited to half of the adjacent edges), `segs` steps per corner.
fn rounded_polygon(corners: &[Pos2], r: f32, segs: usize) -> Vec<Pos2> {
    let n = corners.len();
    let mut out = Vec::with_capacity(n * (segs + 1));
    for i in 0..n {
        let v = corners[i];
        let (to_prev, to_next) = (corners[(i + n - 1) % n] - v, corners[(i + 1) % n] - v);
        let rr = r.min(to_prev.length() / 2.0).min(to_next.length() / 2.0);
        let (a, b) = (to_prev.normalized(), to_next.normalized());
        let center = v + a * rr + b * rr;
        for k in 0..=segs {
            let th = k as f32 / segs as f32 * std::f32::consts::FRAC_PI_2;
            out.push(center - b * rr * th.cos() - a * rr * th.sin());
        }
    }
    out
}

/// Outward unit normals of a clockwise polygon in screen coordinates.
fn normals(pts: &[Pos2]) -> Vec<Vec2> {
    let n = pts.len();
    (0..n)
        .map(|i| {
            let d = pts[(i + 1) % n] - pts[(i + n - 1) % n];
            vec2(d.y, -d.x).normalized()
        })
        .collect()
}

/// Anti-aliased fill of a polygon that is star-shaped around `center`,
/// coloured per position (linear gradients come out exact).
fn fill_star(p: &Painter, pts: &[Pos2], center: Pos2, color: impl Fn(Pos2) -> Color32) {
    let half = 0.5 / p.ctx().pixels_per_point();
    let mut mesh = Mesh::default();
    mesh.colored_vertex(center, color(center));
    for (q, n) in pts.iter().zip(normals(pts)) {
        let inner = *q - n * half;
        mesh.colored_vertex(inner, color(inner));
        mesh.colored_vertex(*q + n * half, Color32::TRANSPARENT);
    }
    let m = pts.len() as u32;
    for i in 0..m {
        let j = (i + 1) % m;
        let (ii, io, ji, jo) = (1 + 2 * i, 2 + 2 * i, 1 + 2 * j, 2 + 2 * j);
        mesh.add_triangle(0, ii, ji);
        mesh.add_triangle(ii, io, ji);
        mesh.add_triangle(io, jo, ji);
    }
    p.add(mesh);
}
