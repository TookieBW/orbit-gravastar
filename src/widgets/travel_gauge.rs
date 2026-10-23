//! Vertical switch-travel diagram: 0 mm (top) to `max` mm (bottomed out),
//! with the actuation point, rapid-trigger sensitivities, deadzones and the
//! live position of the deepest pressed key.

use egui::{pos2, vec2, Align2, Rect, Sense, Stroke, StrokeKind, Ui};

use crate::theme::{self, alpha, display, gradient_rect, mono, semibold};

/// The key that is pressed deepest right now.
pub struct LiveKey {
    pub label: String,
    pub depth: f32,
    pub actuated: bool,
}

pub struct TravelGauge<'a> {
    pub max: f32,
    pub rapid: bool,
    pub actuation: f32,
    pub rt_press: f32,
    pub rt_release: f32,
    pub top_deadzone: f32,
    pub bottom_deadzone: f32,
    pub live: Option<LiveKey>,
    /// Shown while no key is pressed.
    pub idle_hint: &'a str,
}

impl TravelGauge<'_> {
    pub fn show(&self, ui: &mut Ui) {
        // Live readout above the diagram.
        let (head, _) = ui.allocate_exact_size(vec2(ui.available_width(), 34.0), Sense::hover());
        let p = ui.painter();
        match &self.live {
            Some(live) => {
                let color = if live.actuated { theme::GOOD } else { theme::text() };
                p.text(head.left_center(), Align2::LEFT_CENTER, format!("{:.3} mm", live.depth.min(self.max)), display(20.0), color);
                p.text(head.right_center(), Align2::RIGHT_CENTER, &live.label, semibold(13.0), theme::text2());
            }
            None => {
                p.text(head.left_center(), Align2::LEFT_CENTER, self.idle_hint, theme::body(12.0), theme::text3());
            }
        }

        let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), 300.0), Sense::hover());
        let p = ui.painter();
        let h = 260.0;
        let top = rect.min.y + 18.0;
        let y = |mm: f32| top + (mm.clamp(0.0, self.max) / self.max) * h;
        let x0 = rect.center().x - 60.0;

        // Scale.
        let mut mm = 0.0;
        while mm <= self.max + 0.001 {
            let major = (mm * 2.0).round() as i32 % 2 == 0;
            p.line_segment([pos2(x0 - 22.0, y(mm)), pos2(x0 - if major { 10.0 } else { 14.0 }, y(mm))], Stroke::new(1.0, theme::ink(0.25)));
            if major {
                p.text(pos2(x0 - 28.0, y(mm)), Align2::RIGHT_CENTER, format!("{mm:.1}"), mono(10.0), theme::text3());
            }
            mm += 0.5;
        }

        // Switch housing.
        let housing = Rect::from_min_max(pos2(x0, top - 6.0), pos2(x0 + 56.0, top + h + 6.0));
        p.rect_filled(housing, 10.0, theme::ink(0.03));
        p.rect_stroke(housing, 10.0, Stroke::new(1.0, theme::ink(0.1)), StrokeKind::Inside);
        let inner = housing.shrink2(vec2(4.0, 0.0));
        let hatch = |from: f32, to: f32| {
            let r = Rect::from_min_max(pos2(inner.min.x, y(from)), pos2(inner.max.x, y(to)));
            if r.height() > 0.5 {
                p.rect_filled(r, 2.0, theme::ink(0.07));
                let clipped = p.with_clip_rect(r);
                let mut x = r.min.x - r.height();
                while x < r.max.x {
                    clipped.line_segment([pos2(x, r.max.y), pos2(x + r.height(), r.min.y)], Stroke::new(1.0, theme::ink(0.1)));
                    x += 6.0;
                }
            }
        };
        hatch(0.0, self.top_deadzone);
        hatch(self.max - self.bottom_deadzone, self.max);

        // Active range below the actuation point.
        let fill = Rect::from_min_max(pos2(inner.min.x, y(self.actuation)), pos2(inner.max.x, y(self.max)));
        gradient_rect(p, fill, 4.0, 90.0, |t| alpha(theme::lerp(theme::ACCENT, theme::ACCENT2, t), 0.3));

        // Live plunger: how far the deepest key is pressed.
        if let Some(live) = &self.live {
            let color = if live.actuated { theme::GOOD } else { theme::ink(0.85) };
            let ly = y(live.depth);
            let plunger = Rect::from_min_max(pos2(inner.min.x + 6.0, y(0.0)), pos2(inner.max.x - 6.0, ly));
            p.rect_filled(plunger, 4.0, alpha(color, 0.22));
            theme::glow(p, Rect::from_center_size(pos2(inner.center().x, ly), vec2(inner.width(), 4.0)), 2.0, alpha(color, 0.6), 8.0);
            p.line_segment([pos2(inner.min.x, ly), pos2(inner.max.x, ly)], Stroke::new(2.5, color));
        }

        // Actuation marker.
        let my = y(self.actuation);
        p.line_segment([pos2(x0 - 4.0, my), pos2(x0 + 60.0, my)], Stroke::new(2.0, theme::ACCENT2));
        p.circle_filled(pos2(x0 + 60.0, my), 3.5, theme::ACCENT2);
        p.text(pos2(x0 + 68.0, my), Align2::LEFT_CENTER, format!("{:.3} mm", self.actuation), mono(11.0), theme::accent_text());
        if self.rapid {
            p.text(pos2(x0 + 68.0, my + 22.0), Align2::LEFT_CENTER, format!("↓ {:.3}", self.rt_press), mono(10.5), theme::text2());
            p.text(pos2(x0 + 68.0, my + 38.0), Align2::LEFT_CENTER, format!("↑ {:.3}", self.rt_release), mono(10.5), theme::text2());
        }
    }
}
