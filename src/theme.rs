//! Colours, fonts and paint helpers shared by all widgets.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use egui::epaint::{Mesh, RectShape};
use egui::{pos2, vec2, Color32, CornerRadius, FontData, FontDefinitions, FontFamily, FontId, Painter, Pos2, Rect};

pub const ACCENT: Color32 = Color32::from_rgb(139, 123, 255);
pub const ACCENT_MID: Color32 = Color32::from_rgb(91, 140, 255);
pub const ACCENT2: Color32 = Color32::from_rgb(47, 213, 240);
pub const GOOD: Color32 = Color32::from_rgb(62, 224, 143);
pub const WARN: Color32 = Color32::from_rgb(255, 181, 71);
pub const BAD: Color32 = Color32::from_rgb(255, 93, 115);

// ── Palette ──────────────────────────────────────────────────────────────
//
// Orbit has a dark (default) and a light look. Colours that differ between the
// two are functions reading the active mode; the accent colours above are shared.

static LIGHT: AtomicBool = AtomicBool::new(false);

pub fn set_light(light: bool) {
    LIGHT.store(light, Ordering::Relaxed);
}

pub fn is_light() -> bool {
    LIGHT.load(Ordering::Relaxed)
}

/// Glass effect: the backdrop lets the blurred desktop shine through a little.
static GLASS: AtomicBool = AtomicBool::new(false);

pub fn set_glass(glass: bool) {
    GLASS.store(glass, Ordering::Relaxed);
}

pub fn is_glass() -> bool {
    GLASS.load(Ordering::Relaxed)
}

fn pick(dark: Color32, light: Color32) -> Color32 {
    if is_light() {
        light
    } else {
        dark
    }
}

/// Ink colour of the light theme (a deep navy instead of pure black).
const NAVY: [u8; 3] = [18, 22, 42];

pub fn bg() -> Color32 {
    pick(Color32::from_rgb(6, 7, 11), Color32::from_rgb(242, 244, 250))
}
pub fn key_base() -> Color32 {
    pick(Color32::from_rgb(22, 25, 37), Color32::from_rgb(232, 235, 243))
}
pub fn text() -> Color32 {
    pick(Color32::from_rgb(238, 240, 246), Color32::from_rgb(20, 23, 34))
}
pub fn text2() -> Color32 {
    pick(Color32::from_rgb(163, 168, 184), Color32::from_rgb(74, 80, 99))
}
pub fn text3() -> Color32 {
    pick(Color32::from_rgb(107, 112, 131), Color32::from_rgb(128, 134, 152))
}
/// Opaque surface for dialogs, toasts and solid cards.
pub fn surface() -> Color32 {
    pick(Color32::from_rgb(18, 20, 30), Color32::WHITE)
}
/// Opaque fill behind the keyboard and DPI cards.
pub fn card_solid() -> Color32 {
    pick(Color32::from_rgb(12, 14, 21), Color32::from_rgb(252, 253, 255))
}

/// Pure white with the given opacity: highlights, sheens, stars.
pub fn white(a: f32) -> Color32 {
    Color32::from_white_alpha((a * 255.0) as u8)
}

pub fn black(a: f32) -> Color32 {
    Color32::from_black_alpha((a * 255.0) as u8)
}

/// Overlay "ink" for hover fills, hairlines and tracks: white on dark, navy on light.
pub fn ink(a: f32) -> Color32 {
    if is_light() {
        Color32::from_rgba_unmultiplied(NAVY[0], NAVY[1], NAVY[2], (a * 0.9 * 255.0) as u8)
    } else {
        white(a)
    }
}

/// Recessed background of inputs, segmented controls and tiles.
pub fn well(a: f32) -> Color32 {
    if is_light() {
        Color32::from_rgba_unmultiplied(NAVY[0], NAVY[1], NAVY[2], (a * 0.16 * 255.0) as u8)
    } else {
        black(a)
    }
}

/// Drop shadows: softer on the light theme.
pub fn shadow(a: f32) -> Color32 {
    black(if is_light() { a * 0.22 } else { a })
}

/// Cyan for text and small marks; darker on the light theme so it stays readable.
pub fn accent_text() -> Color32 {
    pick(ACCENT2, Color32::from_rgb(0, 128, 160))
}

/// Text on a selected key.
pub fn selected_text() -> Color32 {
    pick(Color32::WHITE, text())
}

/// Track of a switched-off toggle.
pub fn toggle_off() -> Color32 {
    pick(black(0.4), ink(0.16))
}

/// Fill of the selected segment in segmented controls.
pub fn seg_active() -> Color32 {
    pick(white(0.09), Color32::WHITE)
}

pub fn panel_fill() -> Color32 {
    pick(white(0.035), Color32::from_rgba_unmultiplied(255, 255, 255, 190))
}
pub fn line() -> Color32 {
    ink(0.08)
}
pub fn line_strong() -> Color32 {
    ink(0.16)
}

pub fn alpha(c: Color32, a: f32) -> Color32 {
    let [r, g, b, _] = c.to_srgba_unmultiplied();
    Color32::from_rgba_unmultiplied(r, g, b, (a * 255.0) as u8)
}

pub fn lerp(a: Color32, b: Color32, t: f32) -> Color32 {
    let t = t.clamp(0.0, 1.0);
    let [ar, ag, ab, aa] = a.to_srgba_unmultiplied();
    let [br, bg, bb, ba] = b.to_srgba_unmultiplied();
    let f = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * t).round() as u8;
    Color32::from_rgba_unmultiplied(f(ar, br), f(ag, bg), f(ab, bb), f(aa, ba))
}

/// The violet → blue → cyan brand gradient.
pub fn accent_at(t: f32) -> Color32 {
    if t < 0.5 {
        lerp(ACCENT, ACCENT_MID, t * 2.0)
    } else {
        lerp(ACCENT_MID, ACCENT2, (t - 0.5) * 2.0)
    }
}

pub fn rainbow_at(t: f32) -> Color32 {
    let stops = [BAD, WARN, GOOD, ACCENT2, ACCENT, BAD];
    let t = t.rem_euclid(1.0) * (stops.len() - 1) as f32;
    let i = (t as usize).min(stops.len() - 2);
    lerp(stops[i], stops[i + 1], t - i as f32)
}

// ── Fonts ──────────────────────────────────────────────────────────────

const INTER: &[u8] = include_bytes!("../assets/fonts/Inter.ttf");
const SPACE_GROTESK: &[u8] = include_bytes!("../assets/fonts/SpaceGrotesk.ttf");
const JETBRAINS_MONO: &[u8] = include_bytes!("../assets/fonts/JetBrainsMono.ttf");
const LUCIDE: &[u8] = include_bytes!("../assets/fonts/lucide.ttf");

fn weighted(bytes: &'static [u8], weight: f32) -> Arc<FontData> {
    let tweak = egui::FontTweak { coords: egui::epaint::text::VariationCoords::new([("wght", weight)]), ..Default::default() };
    Arc::new(FontData::from_static(bytes).tweak(tweak))
}

pub fn install_fonts(ctx: &egui::Context) {
    let mut fonts = FontDefinitions::default();
    let icon_tweak = egui::FontTweak { y_offset_factor: 0.08, ..Default::default() };
    fonts.font_data.insert("inter".into(), weighted(INTER, 450.0));
    fonts.font_data.insert("inter-semibold".into(), weighted(INTER, 620.0));
    fonts.font_data.insert("grotesk".into(), weighted(SPACE_GROTESK, 600.0));
    fonts.font_data.insert("mono".into(), weighted(JETBRAINS_MONO, 450.0));
    fonts.font_data.insert("lucide".into(), Arc::new(FontData::from_static(LUCIDE).tweak(icon_tweak)));

    let fallback: Vec<String> = fonts.families.get(&FontFamily::Proportional).cloned().unwrap_or_default();
    let family = |primary: &str| {
        let mut v = vec![primary.to_string(), "lucide".to_string()];
        v.extend(fallback.iter().cloned());
        v
    };
    fonts.families.insert(FontFamily::Proportional, family("inter"));
    fonts.families.insert(FontFamily::Monospace, family("mono"));
    fonts.families.insert(FontFamily::Name("semibold".into()), family("inter-semibold"));
    fonts.families.insert(FontFamily::Name("display".into()), family("grotesk"));
    ctx.set_fonts(fonts);
}

pub fn body(size: f32) -> FontId {
    FontId::new(size, FontFamily::Proportional)
}
pub fn semibold(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name("semibold".into()))
}
pub fn display(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name("display".into()))
}
pub fn mono(size: f32) -> FontId {
    FontId::new(size, FontFamily::Monospace)
}

pub fn install_style(ctx: &egui::Context) {
    ctx.all_styles_mut(|s| {
        s.visuals = if is_light() { egui::Visuals::light() } else { egui::Visuals::dark() };
        s.visuals.panel_fill = bg();
        s.visuals.window_fill = surface();
        s.visuals.extreme_bg_color = well(0.35);
        s.visuals.override_text_color = Some(text());
        s.visuals.selection.bg_fill = alpha(ACCENT_MID, 0.45);
        s.visuals.selection.stroke = egui::Stroke::new(1.0, ACCENT2);
        // Combo boxes, popups and egui's own buttons in the glassy Orbit look.
        let w = &mut s.visuals.widgets;
        for (state, fill, border) in [
            (&mut w.inactive, ink(0.06), line_strong()),
            (&mut w.hovered, ink(0.09), ink(0.24)),
            (&mut w.active, ink(0.11), ink(0.3)),
            (&mut w.open, ink(0.11), ink(0.3)),
        ] {
            state.weak_bg_fill = fill;
            state.bg_fill = fill;
            state.bg_stroke = egui::Stroke::new(1.0, border);
            state.fg_stroke = egui::Stroke::new(1.0, text());
            state.corner_radius = CornerRadius::same(9);
            state.expansion = 0.0;
        }
        w.noninteractive.bg_stroke = egui::Stroke::new(1.0, line());
        s.spacing.interact_size = vec2(40.0, 32.0);
        s.spacing.button_padding = vec2(12.0, 6.0);
        s.visuals.menu_corner_radius = CornerRadius::same(10);
        s.visuals.window_corner_radius = CornerRadius::same(14);
        s.visuals.window_stroke = egui::Stroke::new(1.0, line_strong());
        s.spacing.item_spacing = vec2(10.0, 10.0);
        s.spacing.scroll.bar_width = 6.0;
        // Keep the bar off the window's edge hairline.
        s.spacing.scroll.bar_outer_margin = 4.0;
        s.interaction.selectable_labels = false;
    });
}

// ── Paint helpers ──────────────────────────────────────────────────────

/// Fills a rounded rect with a gradient; `color(t)` gets 0..1 along the
/// gradient axis. Directions within 45° of vertical run top→bottom,
/// everything else left→right.
///
/// The body is built from thin strips across the axis, so multi-stop
/// gradients stay smooth, and a one-pixel feathered rim anti-aliases the
/// outline the same way egui's own shapes are smoothed.
pub fn gradient_rect(painter: &Painter, rect: Rect, radius: f32, dir_deg: f32, color: impl Fn(f32) -> Color32) {
    if rect.width() <= 0.0 || rect.height() <= 0.0 {
        return;
    }
    let vertical = dir_deg.to_radians().sin().abs() > dir_deg.to_radians().cos().abs();
    let ppp = painter.ctx().pixels_per_point();
    let half = 0.5 / ppp;
    let r = radius.min(rect.width() / 2.0).min(rect.height() / 2.0).max(0.0);
    // Corner smoothness follows the on-screen radius.
    let segs = ((r * ppp).sqrt() * 2.5).ceil().clamp(2.0, 24.0) as usize;
    let (g0, g1) = if vertical { (rect.min.y, rect.max.y) } else { (rect.min.x, rect.max.x) };
    let t_of = |a: f32| ((a - g0) / (g1 - g0)).clamp(0.0, 1.0);

    // Solid body, inset by half a pixel so the rim straddles the true edge.
    let body = rect.shrink(half);
    let rb = (r - half).max(0.0);
    let (a0, a1, c0, c1) = if vertical {
        (body.min.y, body.max.y, body.min.x, body.max.x)
    } else {
        (body.min.x, body.max.x, body.min.y, body.max.y)
    };
    let inset = |a: f32| {
        let d = if a < a0 + rb { a0 + rb - a } else if a > a1 - rb { a - (a1 - rb) } else { 0.0 };
        rb - (rb * rb - d * d).max(0.0).sqrt()
    };
    let quarter = |k: f32| 1.0 - (k * std::f32::consts::FRAC_PI_2).cos();
    let mut samples: Vec<f32> = (0..=segs).map(|i| a0 + rb * quarter(i as f32 / segs as f32)).collect();
    let steps = ((a1 - a0 - 2.0 * rb) / 4.0).ceil().max(1.0) as usize;
    samples.extend((1..steps).map(|i| a0 + rb + (a1 - a0 - 2.0 * rb) * i as f32 / steps as f32));
    samples.extend((0..=segs).rev().map(|i| a1 - rb * quarter(i as f32 / segs as f32)));

    let mut mesh = Mesh::default();
    for &a in &samples {
        let c = color(t_of(a));
        let d = inset(a);
        let (p, q) = if vertical { (pos2(c0 + d, a), pos2(c1 - d, a)) } else { (pos2(a, c0 + d), pos2(a, c1 - d)) };
        mesh.colored_vertex(p, c);
        mesh.colored_vertex(q, c);
    }
    for i in 0..samples.len() as u32 - 1 {
        let b = i * 2;
        mesh.add_triangle(b, b + 1, b + 2);
        mesh.add_triangle(b + 1, b + 3, b + 2);
    }

    // Feathered rim: opaque on the inner outline, transparent one pixel further out.
    let inner = rounded_outline(body, rb, segs);
    let outer = rounded_outline(rect.expand(half), r + half, segs);
    let base = mesh.vertices.len() as u32;
    for (pi, po) in inner.iter().zip(&outer) {
        mesh.colored_vertex(*pi, color(t_of(if vertical { pi.y } else { pi.x })));
        mesh.colored_vertex(*po, Color32::TRANSPARENT);
    }
    let n = inner.len() as u32;
    for i in 0..n {
        let j = (i + 1) % n;
        let (a, b, c, d) = (base + 2 * i, base + 2 * i + 1, base + 2 * j, base + 2 * j + 1);
        mesh.add_triangle(a, b, c);
        mesh.add_triangle(b, d, c);
    }
    painter.add(mesh);
}

/// Outline of a rounded rect, clockwise from the top edge, `segs` steps per corner.
fn rounded_outline(rect: Rect, r: f32, segs: usize) -> Vec<Pos2> {
    let centers = [
        (pos2(rect.max.x - r, rect.min.y + r), -90.0f32),
        (pos2(rect.max.x - r, rect.max.y - r), 0.0),
        (pos2(rect.min.x + r, rect.max.y - r), 90.0),
        (pos2(rect.min.x + r, rect.min.y + r), 180.0),
    ];
    let mut pts = Vec::with_capacity(4 * (segs + 1));
    for (c, start) in centers {
        for i in 0..=segs {
            let a = (start + 90.0 * i as f32 / segs as f32).to_radians();
            pts.push(pos2(c.x + r * a.cos(), c.y + r * a.sin()));
        }
    }
    pts
}

/// Glossy highlight for gradient tiles: a white sheen fading out by mid-height.
pub fn sheen(painter: &Painter, rect: Rect, radius: f32, strength: f32) {
    gradient_rect(painter, rect, radius, 90.0, |t| white((strength * (1.0 - t / 0.55)).max(0.0)));
}

/// Soft radial glow: `color` in the centre fading to transparent at the ellipse edge.
pub fn radial_glow(painter: &Painter, center: Pos2, radius: egui::Vec2, color: Color32) {
    let mut mesh = Mesh::default();
    mesh.colored_vertex(center, color);
    let steps = 48;
    for i in 0..steps {
        let a = i as f32 / steps as f32 * std::f32::consts::TAU;
        mesh.colored_vertex(pos2(center.x + radius.x * a.cos(), center.y + radius.y * a.sin()), Color32::TRANSPARENT);
    }
    for i in 0..steps {
        mesh.add_triangle(0, 1 + i, 1 + (i + 1) % steps);
    }
    painter.add(mesh);
}

/// Fills the part of a rounded rect between `x0` and `x1`, following the corner
/// arcs, e.g. a stripe that must not poke out of the rounded corners.
pub fn rounded_span(painter: &Painter, rect: Rect, radius: f32, x0: f32, x1: f32, color: Color32) {
    let (x0, x1) = (x0.max(rect.min.x), x1.min(rect.max.x));
    if x1 <= x0 {
        return;
    }
    let r = radius.min(rect.width() / 2.0).min(rect.height() / 2.0).max(0.0);
    let inset = |x: f32| {
        let d = if x < rect.min.x + r { rect.min.x + r - x } else if x > rect.max.x - r { x - (rect.max.x - r) } else { 0.0 };
        r - (r * r - d * d).max(0.0).sqrt()
    };
    let steps = 8;
    let mut mesh = Mesh::default();
    for i in 0..=steps {
        let x = x0 + (x1 - x0) * i as f32 / steps as f32;
        let d = inset(x);
        mesh.colored_vertex(pos2(x, rect.min.y + d), color);
        mesh.colored_vertex(pos2(x, rect.max.y - d), color);
    }
    for i in 0..steps as u32 {
        let b = i * 2;
        mesh.add_triangle(b, b + 1, b + 2);
        mesh.add_triangle(b + 1, b + 3, b + 2);
    }
    painter.add(mesh);
}

/// Blurred rounded rect, used for glows behind selected elements.
pub fn glow(painter: &Painter, rect: Rect, radius: f32, color: Color32, blur: f32) {
    painter.add(RectShape::filled(rect, CornerRadius::same(radius as u8), color).with_blur_width(blur));
}

/// Corner radius of the frameless windows (main window and tray popup).
pub const WINDOW_RADIUS: f32 = 12.0;

/// The app backdrop: deep space with coloured nebula glows and a few stars,
/// filling a rounded rect so it works as the window shape of a transparent window.
pub fn paint_backdrop(painter: &Painter, rect: Rect, radius: f32) {
    let light = is_light();
    let (w, h) = (rect.width(), rect.height());
    // Nebula glows: centre, radii, colour and peak opacity; linear falloff to the ellipse edge.
    // Sizes are relative to the surface (tuned on the 1280×820 main window), so small
    // windows like the tray popup get the same dark look instead of sitting inside a glow.
    let glows = [
        (pos2(rect.min.x + w * 0.85, rect.min.y - h * 0.1), vec2(w * 0.94, h * 0.73), ACCENT_MID, if light { 0.10 } else { 0.13 }),
        (pos2(rect.min.x - w * 0.1, rect.max.y + h * 0.1), vec2(w * 0.70, h * 0.61), ACCENT, if light { 0.08 } else { 0.11 }),
        (pos2(rect.center().x, rect.max.y + h * 0.2), vec2(w * 0.47, h * 0.37), ACCENT2, if light { 0.06 } else { 0.06 }),
    ];
    // Only slightly see-through, so the page stays calm and readable.
    let opacity = if !is_glass() { 1.0 } else if light { 0.82 } else { 0.86 };
    let color_at = |p: Pos2| {
        let mut c = bg();
        for (center, r, glow, a) in glows {
            let d = (((p.x - center.x) / r.x).powi(2) + ((p.y - center.y) / r.y).powi(2)).sqrt();
            if d < 1.0 {
                c = lerp(c, glow, a * (1.0 - d));
            }
        }
        c.gamma_multiply(opacity)
    };
    fill_rounded(painter, rect, radius, color_at);
    if light {
        // Stars only make sense in the night sky.
        return;
    }
    for &(fx, fy, r, a) in &[
        (0.12, 0.18, 1.0, 0.5),
        (0.72, 0.12, 1.0, 0.4),
        (0.38, 0.72, 1.0, 0.35),
        (0.88, 0.64, 1.4, 0.45),
        (0.22, 0.88, 1.0, 0.3),
        (0.58, 0.38, 0.9, 0.25),
        (0.92, 0.92, 1.0, 0.35),
        (0.05, 0.5, 1.3, 0.3),
        (0.47, 0.06, 0.9, 0.3),
        (0.66, 0.83, 1.1, 0.25),
    ] {
        painter.circle_filled(pos2(rect.min.x + w * fx, rect.min.y + h * fy), r, white(a));
    }
}

/// Fills a rounded rect with a colour that varies freely over the surface
/// (`color(pos)`), sampled on a ~16 px grid, with an anti-aliased outline.
pub fn fill_rounded(painter: &Painter, rect: Rect, radius: f32, color: impl Fn(Pos2) -> Color32) {
    if rect.width() <= 0.0 || rect.height() <= 0.0 {
        return;
    }
    let ppp = painter.ctx().pixels_per_point();
    let half = 0.5 / ppp;
    let r = radius.min(rect.width() / 2.0).min(rect.height() / 2.0).max(0.0);
    let segs = ((r * ppp).sqrt() * 2.5).ceil().clamp(2.0, 24.0) as usize;
    let body = rect.shrink(half);
    let rb = (r - half).max(0.0);
    let (x0, x1) = (body.min.x, body.max.x);
    let inset = |x: f32| {
        let d = if x < x0 + rb { x0 + rb - x } else if x > x1 - rb { x - (x1 - rb) } else { 0.0 };
        rb - (rb * rb - d * d).max(0.0).sqrt()
    };
    let quarter = |k: f32| 1.0 - (k * std::f32::consts::FRAC_PI_2).cos();
    let mut xs: Vec<f32> = (0..=segs).map(|i| x0 + rb * quarter(i as f32 / segs as f32)).collect();
    let steps = ((x1 - x0 - 2.0 * rb) / 16.0).ceil().max(1.0) as usize;
    xs.extend((1..steps).map(|i| x0 + rb + (x1 - x0 - 2.0 * rb) * i as f32 / steps as f32));
    xs.extend((0..=segs).rev().map(|i| x1 - rb * quarter(i as f32 / segs as f32)));
    let rows = (body.height() / 16.0).ceil().max(1.0) as usize;

    let mut mesh = Mesh::default();
    for &x in &xs {
        let d = inset(x);
        let (top, bottom) = (body.min.y + d, body.max.y - d);
        for j in 0..=rows {
            let p = pos2(x, top + (bottom - top) * j as f32 / rows as f32);
            mesh.colored_vertex(p, color(p));
        }
    }
    let stride = rows as u32 + 1;
    for i in 0..xs.len() as u32 - 1 {
        for j in 0..rows as u32 {
            let a = i * stride + j;
            let b = a + stride;
            mesh.add_triangle(a, b, a + 1);
            mesh.add_triangle(b, b + 1, a + 1);
        }
    }

    // Feathered rim for smooth (rounded) edges.
    let inner = rounded_outline(body, rb, segs);
    let outer = rounded_outline(rect.expand(half), r + half, segs);
    let base = mesh.vertices.len() as u32;
    for (pi, po) in inner.iter().zip(&outer) {
        mesh.colored_vertex(*pi, color(*pi));
        mesh.colored_vertex(*po, Color32::TRANSPARENT);
    }
    let n = inner.len() as u32;
    for i in 0..n {
        let j = (i + 1) % n;
        let (a, b, c, d) = (base + 2 * i, base + 2 * i + 1, base + 2 * j, base + 2 * j + 1);
        mesh.add_triangle(a, b, c);
        mesh.add_triangle(b, d, c);
    }
    painter.add(mesh);
}

/// Icon glyphs from the embedded Lucide font (rendered through the normal text families).
pub mod icon {
    pub const KEYBOARD: &str = "\u{e283}";
    pub const MOUSE: &str = "\u{e28d}";
    pub const FLASK: &str = "\u{e0da}";
    pub const GAUGE: &str = "\u{e1be}";
    pub const LIGHTBULB: &str = "\u{e1c1}";
    pub const SETTINGS: &str = "\u{e244}";
    pub const CPU: &str = "\u{e0ad}";
    pub const LAYERS: &str = "\u{e52d}";
    pub const ROTATE_CCW: &str = "\u{e14b}";
    pub const RULER: &str = "\u{e14e}";
    pub const TIMER: &str = "\u{e1df}";
    pub const MOON: &str = "\u{e121}";
    pub const PALETTE: &str = "\u{e1dc}";
    pub const SLIDERS: &str = "\u{e299}";
    pub const SPARKLES: &str = "\u{e416}";
    pub const CROSSHAIR: &str = "\u{e0b0}";
    pub const ZAP: &str = "\u{e1b3}";
    pub const MINUS: &str = "\u{e11f}";
    pub const PLUS: &str = "\u{e140}";
    pub const ACTIVITY: &str = "\u{e038}";
    pub const POINTER: &str = "\u{e1c2}";
    pub const BATTERY_CHARGING: &str = "\u{e058}";
    pub const BATTERY_FULL: &str = "\u{e059}";
    pub const BATTERY_LOW: &str = "\u{e05a}";
    pub const BATTERY_MEDIUM: &str = "\u{e05b}";
    pub const ALERT: &str = "\u{e192}";
    pub const CHECK: &str = "\u{e225}";
    pub const GEAR: &str = "\u{e157}";
    pub const SUN: &str = "\u{e17b}";
    pub const MONITOR: &str = "\u{e120}";
    pub const POWER: &str = "\u{e143}";
    pub const INFO: &str = "\u{e0fe}";
}

/// The user's theme choice; `System` follows the Windows app theme.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ThemePref {
    Dark,
    Light,
    System,
}

impl ThemePref {
    pub fn code(self) -> &'static str {
        match self {
            ThemePref::Dark => "dark",
            ThemePref::Light => "light",
            ThemePref::System => "system",
        }
    }

    pub fn from_code(s: &str) -> Option<Self> {
        match s {
            "dark" => Some(ThemePref::Dark),
            "light" => Some(ThemePref::Light),
            "system" => Some(ThemePref::System),
            _ => None,
        }
    }

    /// Whether the light palette should be active.
    pub fn is_light(self, ctx: &egui::Context) -> bool {
        match self {
            ThemePref::Dark => false,
            ThemePref::Light => true,
            ThemePref::System => ctx.system_theme() == Some(egui::Theme::Light),
        }
    }
}
