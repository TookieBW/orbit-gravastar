use std::collections::HashSet;

use egui::{pos2, vec2, Align, Align2, Color32, CursorIcon, Layout, Rect, Sense, Stroke, StrokeKind, Ui};

use crate::app::{Confirm, DeviceUi};
use crate::i18n::{Lang, Strings};
use crate::protocol::keyboard::{KeyPerf, LightingBase, MODE_FIXED, MODE_RAPID, REPORT_RATES};
use crate::protocol::models::{build_layout, lighting_zone, max_travel, Preview, DEADZONE_MAX, LIGHT_SLEEP_MINUTES};
use crate::protocol::Rgb;
use crate::theme::{self, alpha, gradient_rect, icon, semibold};
use crate::widgets::keyboard_view::{KeyValue, KeyboardView};
use crate::widgets::travel_gauge::{LiveKey, TravelGauge};
use crate::widgets::{button, columns_grid, masonry, row, segmented, spinner_row, split, stat, toggle, Kind, Panel, Slider};
use crate::worker::{KbState, Worker};

/// Maps an actuation depth to a tint so the layout doubles as a heat map.
fn heat(mm: f32, max: f32) -> Color32 {
    let f = (mm / max).clamp(0.0, 1.0);
    let (shallow, deep) = if theme::is_light() {
        (Color32::from_rgb(206, 238, 246), Color32::from_rgb(226, 214, 252))
    } else {
        (Color32::from_rgb(14, 52, 64), Color32::from_rgb(52, 30, 92))
    };
    theme::lerp(theme::key_base(), theme::lerp(shallow, deep, f), 0.85)
}

fn update_kb(worker: &Worker, id: &str, f: impl FnOnce(&mut KbState)) {
    if let Some(crate::worker::DeviceData::Keyboard(k)) = worker.lock().devices.iter_mut().find(|d| d.id == id).map(|d| &mut d.data) {
        f(k);
    }
}

// ── Actuation ──────────────────────────────────────────────────────────

pub fn performance(ui: &mut Ui, s: &Strings, lang: Lang, worker: &Worker, id: &str, k: &KbState, st: &mut DeviceUi) {
    if k.slots.is_empty() || k.loading.is_some() {
        let text = match k.loading {
            Some((done, total)) => format!("{} {done}/{total}", s.reading_keys),
            None => s.reading_keys.to_string(),
        };
        Panel::new("", "").show(ui, |ui| spinner_row(ui, &text));
        return;
    }
    // German keycaps only exist on ISO boards (they have the extra key next to left shift).
    let german = lang == Lang::De && k.codes.values().any(|&c| c == 0x64);
    let keys = build_layout(&k.slots, &|r, c| k.codes.get(&(r, c)).copied().unwrap_or(0), german);
    if !st.selection_ready {
        st.selected = keys.iter().map(|k| (k.row, k.col)).collect();
        st.selection_ready = true;
    }

    let tint = |kd: &crate::protocol::models::KeyDef| {
        k.perf.get(&(kd.row, kd.col)).map(|p| heat(if p.mode == MODE_RAPID { p.rt_first_touch } else { p.normal_press }, max_travel(p.axis_v2_id)))
    };
    let value = |kd: &crate::protocol::models::KeyDef| match k.perf.get(&(kd.row, kd.col)) {
        Some(p) if p.mode == MODE_RAPID => KeyValue::Rapid(format!("{:.2}", p.rt_press)),
        Some(p) => KeyValue::Text(format!("{:.2}", p.normal_press)),
        None => KeyValue::None,
    };
    // Ask the worker for live key travel while this page is visible.
    worker.lock().live_request = Some((id.to_string(), std::time::Instant::now()));
    ui.ctx().request_repaint_after(std::time::Duration::from_millis(100));
    let live = |kd: &crate::protocol::models::KeyDef| {
        k.live.get(&(kd.row, kd.col)).map(|&(depth, actuated)| (depth / k.perf.get(&(kd.row, kd.col)).map(|p| max_travel(p.axis_v2_id)).unwrap_or(3.5), actuated))
    };
    if let Some(sel) = (KeyboardView { keys: &keys, tint: &tint, value: &value, live: &live }).show(ui, &st.selected, &mut st.drag) {
        st.selected = sel;
    }

    // Selection bar.
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new(format!("{} {}", st.selected.len(), s.selected_keys)).color(theme::text3()));
        ui.label(egui::RichText::new(s.select_hint).font(theme::body(12.0)).color(alpha(theme::text3(), 0.7)));
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            if button(ui, s.select_none, Kind::Ghost, true, true).clicked() {
                st.selected.clear();
            }
            if button(ui, s.select_all, Kind::Ghost, true, true).clicked() {
                st.selected = keys.iter().map(|k| (k.row, k.col)).collect();
            }
        });
    });

    // Load the editor from the first selected key (in layout order).
    let first = keys.iter().map(|k| (k.row, k.col)).find(|id| st.selected.contains(id));
    if first != st.draft_from {
        st.draft_from = first;
        st.draft = first.and_then(|f| k.perf.get(&f).copied());
        if let Some(d) = st.draft {
            st.split_release = (d.rt_press - d.rt_release).abs() > 0.0001;
        }
    }
    let Some(mut draft) = st.draft else { return };
    ui.add_space(8.0);

    let step = k.rt_precision;
    let max = max_travel(draft.axis_v2_id);
    let rapid = draft.mode == MODE_RAPID;
    let writing = k.writing;

    let mut sp = split(ui, 260.0, 18.0);
    {
        let ui = &mut sp.left;
        let mode_now = draft.mode;
        let mut mode_pick = None;
        Panel::new(if rapid { icon::ZAP } else { icon::CROSSHAIR }, s.actuation_mode)
            .actions(|ui| mode_pick = segmented(ui, Some(mode_now), &[(MODE_RAPID, s.mode_rapid.into()), (MODE_FIXED, s.mode_fixed.into())], true))
            .show(ui, |ui| {
                let first = if rapid { s.rt_first_touch } else { s.actuation_point };
                let count = if rapid { 2 } else { 1 };
                columns_grid(ui, 2, 26.0, count, |ui, i| {
                    let (label, v) = match (i, rapid) {
                        (0, true) => (first, &mut draft.rt_first_touch),
                        (0, false) => (first, &mut draft.normal_press),
                        // One value for both directions unless release gets its own.
                        _ => (if st.split_release { s.rt_press } else { s.sensitivity }, &mut draft.rt_press),
                    };
                    if let Some(x) = Slider::new(label, step, max, step).decimals(3).unit("mm").show(ui, *v).changed {
                        *v = x;
                    }
                });
                if rapid {
                    ui.add_space(6.0);
                    row(ui, false, s.separate_release, None, |ui| {
                        if let Some(on) = toggle(ui, st.split_release, true) {
                            st.split_release = on;
                        }
                    });
                    if st.split_release {
                        columns_grid(ui, 2, 26.0, 1, |ui, _| {
                            if let Some(x) = Slider::new(s.rt_release, step, max, step).decimals(3).unit("mm").show(ui, draft.rt_release).changed {
                                draft.rt_release = x;
                            }
                        });
                    }
                }
                ui.add_space(10.0);
                columns_grid(ui, 2, 26.0, 2, |ui, i| {
                    let (label, v) = if i == 0 { (s.press_deadzone, &mut draft.press_deadzone) } else { (s.release_deadzone, &mut draft.release_deadzone) };
                    if let Some(x) = Slider::new(label, 0.0, DEADZONE_MAX, 0.01).decimals(2).unit("mm").show(ui, *v).changed {
                        *v = x;
                    }
                });

                ui.add_space(16.0);
                let y = ui.cursor().min.y;
                ui.painter().hline(ui.max_rect().x_range(), y, Stroke::new(1.0, theme::line()));
                ui.add_space(12.0);
                ui.horizontal(|ui| {
                    if let Some((done, total)) = writing {
                        spinner_row(ui, &format!("{} {done}/{total}…", s.applying));
                    }
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if button(ui, s.apply, Kind::Primary, false, writing.is_none() && !st.selected.is_empty()).clicked() {
                            apply(worker, s, id, k, &keys_in_order(&keys, &st.selected), &draft, st.split_release);
                        }
                    });
                });
            });
        if let Some(mode) = mode_pick {
            draft.mode = mode;
        }
    }
    {
        let ui = &mut sp.right;
        let rapid = draft.mode == MODE_RAPID;
        // The deepest pressed key drives the live readout, like the official driver.
        let deepest = k
            .live
            .iter()
            .max_by(|a, b| a.1 .0.total_cmp(&b.1 .0))
            .map(|(key, &(depth, actuated))| LiveKey { label: keys.iter().find(|kd| (kd.row, kd.col) == *key).map(|kd| kd.label.clone()).unwrap_or_default(), depth, actuated });
        Panel::new("", s.travel).show(ui, |ui| {
            TravelGauge {
                max,
                rapid,
                actuation: if rapid { draft.rt_first_touch } else { draft.normal_press },
                rt_press: draft.rt_press,
                rt_release: if st.split_release { draft.rt_release } else { draft.rt_press },
                top_deadzone: draft.press_deadzone,
                bottom_deadzone: draft.release_deadzone,
                live: deepest,
                idle_hint: s.press_key_hint,
            }
            .show(ui);
        });
    }
    sp.finish(ui);
    st.draft = Some(draft);
}

fn keys_in_order(keys: &[crate::protocol::models::KeyDef], selected: &HashSet<(u8, u8)>) -> Vec<(u8, u8)> {
    keys.iter().map(|k| (k.row, k.col)).filter(|id| selected.contains(id)).collect()
}

fn apply(worker: &Worker, s: &Strings, id: &str, k: &KbState, targets: &[(u8, u8)], draft: &KeyPerf, split_release: bool) {
    let writes: Vec<((u8, u8), KeyPerf)> = targets
        .iter()
        .filter_map(|key| k.perf.get(key).map(|old| (*key, old)))
        .map(|(key, old)| {
            let limit = max_travel(old.axis_v2_id);
            let clamp = |v: f32| v.min(limit);
            // Echo the per-switch fields of this key; normalRelease and calibrate
            // are always sent as 0, exactly like the official driver does.
            let p = KeyPerf {
                normal_press: clamp(draft.normal_press),
                rt_first_touch: clamp(draft.rt_first_touch),
                rt_press: clamp(draft.rt_press),
                rt_release: clamp(if split_release { draft.rt_release } else { draft.rt_press }),
                normal_release: 0.0,
                calibrate: 0,
                axis: old.axis,
                axis_v2_id: old.axis_v2_id,
                axis_range_max: old.axis_range_max,
                axis_coefficient: old.axis_coefficient,
                ..*draft
            };
            (key, p)
        })
        .collect();
    let id = id.to_string();
    worker.run(&id.clone(), Some(s.saved), move |b| b.apply_perf(&id, writes));
}

// ── Lighting ───────────────────────────────────────────────────────────

pub fn lighting(ui: &mut Ui, s: &Strings, lang: Lang, worker: &Worker, id: &str, k: &KbState, st: &mut DeviceUi) {
    if k.areas.is_empty() {
        Panel::new("", "").show(ui, |ui| spinner_row(ui, s.loading));
        return;
    }
    if k.areas.len() > 1 {
        let options: Vec<(u8, String)> = k.areas.iter().map(|a| (a.index, lighting_zone(a.index, a.count).name(lang).to_string())).collect();
        if let Some(a) = segmented(ui, Some(st.area), &options, true) {
            st.area = a;
        }
        ui.add_space(6.0);
    }
    let area = st.area;
    let count = k.areas.iter().find(|a| a.index == area).map(|a| a.count).unwrap_or(0);
    let zone = lighting_zone(area, count);
    let (Some(base), Some(palette)) = (k.lighting.get(&area).copied(), k.palettes.get(&area).cloned()) else {
        Panel::new("", "").show(ui, |ui| spinner_row(ui, s.loading));
        return;
    };
    let mode = zone.modes.iter().find(|m| m.id == base.mode).copied();
    let on = base.on;
    let edit = st.palette_edit;
    let color = |i: u8| -> Color32 {
        let c = edit.filter(|e| e.0 == area && e.1 == i as usize).map(|e| e.2).unwrap_or_else(|| palette.get(i as usize).copied().unwrap_or_default());
        Color32::from_rgb(c.r, c.g, c.b)
    };
    let current_color = if zone.multicolor_index0 && base.color_index == 0 { None } else { Some(color(base.color_index)) };

    let write = {
        let worker = worker.clone();
        let id = id.to_string();
        move |next: LightingBase| {
            update_kb(&worker, &id, |s| {
                s.lighting.insert(area, next);
            });
            let id2 = id.clone();
            worker.run(&id, None, move |b| b.kb(&id2)?.set_lighting(area, &next));
        }
    };

    ui.ctx().request_repaint_after(std::time::Duration::from_millis(40));
    let time = ui.input(|i| i.time) as f32;

    let mut sp = split(ui, 330.0, 18.0);
    {
        let ui = &mut sp.left;
        let mut toggled = None;
        Panel::new(icon::SPARKLES, s.effect).actions(|ui| toggled = toggle(ui, on, true)).show(ui, |ui| {
            let n = zone.modes.len();
            columns_grid(ui, 4, 10.0, n, |ui, i| {
                let m = zone.modes[i];
                let active = base.mode == m.id;
                let tint = if m.palette { current_color } else { None };
                if effect_card(ui, m.name(lang), m.preview, tint, active, on, time).clicked() {
                    // Like the official driver: modes without a palette colour get index 0.
                    write(LightingBase { mode: m.id, color_index: if m.palette { base.color_index } else { 0 }, ..base });
                }
            });
        });
        if let Some(v) = toggled {
            write(LightingBase { on: v, ..base });
        }
    }
    {
        let ui = &mut sp.right;
        Panel::new(icon::SLIDERS, s.lighting).show(ui, |ui| {
            if let Some(v) = Slider::new(s.brightness, 0.0, 100.0, 1.0).unit("%").enabled(on).show(ui, base.brightness as f32).committed {
                write(LightingBase { brightness: v as u8, ..base });
            }
            if zone.speed {
                ui.add_space(8.0);
                if let Some(v) = Slider::new(s.speed, 0.0, 100.0, 1.0).unit("%").enabled(on).show(ui, base.speed as f32).committed {
                    write(LightingBase { speed: v as u8, ..base });
                }
            }
            if zone.direction {
                ui.add_space(6.0);
                row(ui, false, s.direction, None, |ui| {
                    if let Some(d) = segmented(ui, Some(base.direction), &[(1, "←".into()), (0, "→".into())], on) {
                        write(LightingBase { direction: d, ..base });
                    }
                });
            }
        });

        if mode.is_some_and(|m| m.palette) {
            ui.add_space(18.0);
            Panel::new(icon::PALETTE, s.palette).show(ui, |ui| {
                let size = ((ui.available_width() - 7.0 * 8.0) / 8.0).floor();
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 8.0;
                    for i in 0..8u8 {
                        let multi = zone.multicolor_index0 && i == 0;
                        let (rect, resp) = ui.allocate_exact_size(vec2(size, size), Sense::click());
                        let p = ui.painter();
                        if multi {
                            gradient_rect(p, rect, 10.0, 45.0, theme::rainbow_at);
                        } else {
                            p.rect_filled(rect, 10.0, color(i));
                        }
                        let active = base.color_index == i;
                        p.rect_stroke(rect, 10.0, Stroke::new(2.0, if active { theme::text() } else { theme::ink(0.1) }), StrokeKind::Inside);
                        if resp.on_hover_cursor(CursorIcon::PointingHand).clicked() && on {
                            write(LightingBase { color_index: i, ..base });
                        }
                    }
                });
                if !(zone.multicolor_index0 && base.color_index == 0) {
                    ui.add_space(8.0);
                    row(ui, false, s.color, None, |ui| {
                        let i = base.color_index as usize;
                        let mut c = color(base.color_index);
                        if egui::widgets::color_picker::color_edit_button_srgba(ui, &mut c, egui::widgets::color_picker::Alpha::Opaque).changed() {
                            st.palette_edit = Some((area, i, Rgb { r: c.r(), g: c.g(), b: c.b() }));
                        }
                    });
                }
            });
        }

        // Commit an edited palette colour once the picker is released.
        if let Some((a, i, c)) = st.palette_edit {
            if !ui.input(|inp| inp.pointer.any_down()) {
                st.palette_edit = None;
                let mut colors = palette.clone();
                if let Some(slot) = colors.get_mut(i) {
                    *slot = c;
                }
                update_kb(worker, id, |s| {
                    s.palettes.insert(a, colors.clone());
                });
                let id2 = id.to_string();
                worker.run(id, None, move |b| b.kb(&id2)?.set_palette(a, &colors));
            }
        }

        if area == 0 {
            if let Some(sleep) = k.light_sleep {
                ui.add_space(18.0);
                Panel::new(icon::MOON, s.light_sleep).show(ui, |ui| {
                    let label = |m: u16| if m == 0 { s.never.to_string() } else { format!("{m} {}", s.minutes) };
                    egui::ComboBox::from_id_salt(("light-sleep", id)).selected_text(label(sleep)).width(160.0).show_ui(ui, |ui| {
                        for m in LIGHT_SLEEP_MINUTES {
                            if ui.selectable_label(sleep == m, label(m)).clicked() && sleep != m {
                                update_kb(worker, id, |s| s.light_sleep = Some(m));
                                let id2 = id.to_string();
                                worker.run(id, None, move |b| b.kb(&id2)?.set_lighting_sleep(m));
                            }
                        }
                    });
                });
            }
        }
    }
    sp.finish(ui);
}

/// A clickable effect tile with an animated preview strip.
pub fn effect_card(ui: &mut Ui, name: &str, preview: Preview, tint: Option<Color32>, active: bool, enabled: bool, time: f32) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 78.0), if enabled { Sense::click() } else { Sense::hover() });
    let p = ui.painter();
    let fade = if enabled { 1.0 } else { 0.4 };
    p.rect_filled(rect, 12.0, if active { alpha(theme::ACCENT2, 0.06) } else { theme::well(0.25) });
    let border = if active {
        Stroke::new(1.5, theme::ACCENT2)
    } else if enabled && resp.hovered() {
        Stroke::new(1.0, theme::line_strong())
    } else {
        Stroke::new(1.0, theme::line())
    };
    p.rect_stroke(rect, 12.0, border, StrokeKind::Inside);
    let sw = Rect::from_min_size(rect.min + vec2(10.0, 10.0), vec2(rect.width() - 20.0, 34.0));
    let base = tint.unwrap_or(theme::ACCENT_MID);
    let dark = Color32::from_rgb(11, 13, 20);
    let t = time * 0.25;
    match (preview, tint) {
        (Preview::Solid, _) => {
            p.rect_filled(sw, 8.0, alpha(base, fade));
        }
        (Preview::Pulse, _) => {
            let k = 0.35 + 0.65 * (time * 1.6).sin().abs();
            p.rect_filled(sw, 8.0, alpha(theme::lerp(dark, base, k), fade));
        }
        (Preview::Rainbow, _) => gradient_rect(p, sw, 8.0, 0.0, |x| alpha(theme::rainbow_at(x * 0.6 + t), fade)),
        (Preview::Sweep, _) => gradient_rect(p, sw, 8.0, 0.0, |x| {
            let d = ((x - (t * 2.0).rem_euclid(1.4) + 0.2) * 5.0).powi(2);
            alpha(theme::lerp(dark, base, (-d).exp()), fade)
        }),
        (Preview::Stripes, _) => {
            p.rect_filled(sw, 8.0, alpha(dark, fade));
            let mut x = sw.min.x - 24.0 + (time * 18.0).rem_euclid(24.0);
            while x < sw.max.x {
                theme::rounded_span(p, sw, 8.0, x, x + 8.0, alpha(base, fade));
                x += 24.0;
            }
        }
        (Preview::Dots, _) => {
            p.rect_filled(sw, 8.0, alpha(theme::lerp(dark, base, 0.25), fade));
            let clip = p.with_clip_rect(sw);
            for (i, fx) in [0.15f32, 0.4, 0.62, 0.85].iter().enumerate() {
                let y = sw.min.y + ((time * 10.0 + i as f32 * 9.0).rem_euclid(sw.height() + 8.0)) - 4.0;
                clip.circle_filled(pos2(sw.min.x + sw.width() * fx, y), 2.5, alpha(Color32::WHITE, fade));
            }
        }
    }
    let text_color = if active { theme::text() } else { theme::text2() };
    p.text(pos2(rect.min.x + 10.0, rect.max.y - 15.0), Align2::LEFT_CENTER, name, semibold(12.5), alpha(text_color, fade.max(0.6)));
    if enabled {
        resp.on_hover_cursor(CursorIcon::PointingHand)
    } else {
        resp
    }
}

// ── Device ─────────────────────────────────────────────────────────────

pub fn device(ui: &mut Ui, s: &Strings, worker: &Worker, id: &str, k: &KbState, confirm: &mut Option<Confirm>) {
    // Order chosen so both columns come out about the same height.
    masonry(ui, 2, 18.0, 5, |ui, i| match i {
        0 => Panel::new(icon::TIMER, s.polling_rate).desc(s.polling_hint).show(ui, |ui| {
            let options: Vec<(u8, String)> = k.rates.iter().map(|&r| (r, REPORT_RATES.get(r as usize).copied().unwrap_or("?").to_string())).collect();
            if let Some(rate) = segmented(ui, k.rate, &options, true) {
                *confirm = Some(Confirm::ReportRate { device: id.to_string(), rate });
            }
        }),
        1 => Panel::new(icon::LAYERS, s.profiles).show(ui, |ui| {
            let options: Vec<(u8, String)> = (0..k.profile_count).map(|p| (p, format!("{} {}", s.profile, p + 1))).collect();
            if let Some(p) = segmented(ui, Some(k.profile), &options, true) {
                let id2 = id.to_string();
                worker.run(id, Some(s.saved), move |b| {
                    b.kb(&id2)?.set_profile(p)?;
                    b.load_keyboard(&id2)
                });
            }
            if let Some(sys) = k.system {
                ui.add_space(10.0);
                row(ui, false, s.os_mode, None, |ui| {
                    if let Some(v) = segmented(ui, Some(sys), &[(0, "Windows".into()), (1, "macOS".into())], true) {
                        update_kb(worker, id, |s| s.system = Some(v));
                        let id2 = id.to_string();
                        worker.run(id, None, move |b| b.kb(&id2)?.set_system(v));
                    }
                });
            }
        }),
        2 => Panel::new(icon::RULER, s.calibration).desc(s.calibration_hint).show(ui, |ui| {
            let (label, kind, start) = if k.calibrating { (s.finish_calibration, Kind::Primary, false) } else { (s.start_calibration, Kind::Normal, true) };
            if button(ui, label, kind, false, true).clicked() {
                let id2 = id.to_string();
                worker.run(id, if start { None } else { Some(s.saved) }, move |b| {
                    b.kb(&id2)?.calibration(start)?;
                    b.update_kb(&id2, |s| s.calibrating = start);
                    Ok(())
                });
            }
        }),
        3 => Panel::new(icon::CPU, s.device_info).show(ui, |ui| {
            if let Some(info) = &k.info {
                let items = [
                    (s.firmware, info.firmware.clone()),
                    (s.protocol, info.protocol.clone()),
                    (s.build, info.build_time.clone()),
                    ("PCB", info.pcb.clone()),
                    ("Board ID", format!("0x{:08X}", info.board_id)),
                ];
                columns_grid(ui, 2, 16.0, items.len(), |ui, i| stat(ui, items[i].0, &items[i].1));
            }
        }),
        _ => Panel::new(icon::ROTATE_CCW, s.factory_reset).desc(s.factory_reset_hint).show(ui, |ui| {
            if button(ui, &format!("{}  {}", icon::ROTATE_CCW, s.factory_reset), Kind::Danger, false, true).clicked() {
                *confirm = Some(Confirm::FactoryReset { device: id.to_string() });
            }
        }),
    });
}

/// Runs an action the user confirmed in the dialog.
pub fn confirmed(worker: &Worker, s: &Strings, c: Confirm) {
    match c {
        Confirm::ReportRate { device, rate } => {
            update_kb(worker, &device, |s| s.rate = Some(rate));
            let id = device.clone();
            worker.run(&device, Some(s.saved), move |b| b.kb(&id)?.set_report_rate(rate));
        }
        Confirm::FactoryReset { device } => {
            let id = device.clone();
            worker.run(&device, Some(s.factory_reset), move |b| {
                b.kb(&id)?.factory_reset()?;
                b.load_keyboard(&id)
            });
        }
    }
}

