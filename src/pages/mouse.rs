use egui::{pos2, vec2, Align2, Color32, CursorIcon, Rect, Sense, Stroke, StrokeKind, Ui};

use crate::app::DeviceUi;
use crate::i18n::Strings;
use crate::pages::keyboard::effect_card;
use crate::protocol::models::Preview;
use crate::protocol::mouse::{addr, light_controls, Button, Light, BUTTON_DISABLED, BUTTON_DPI, BUTTON_MOUSE, BUTTON_SLOTS, REPORT_RATES, SLEEP_TIMES};
use crate::protocol::Rgb;
use crate::theme::{self, alpha, body, display, gradient_rect, icon, mono, semibold};
use crate::widgets::{button, columns_grid, masonry, row, segmented, spinner_row, split, stat, toggle, Kind, Panel, Slider};
use crate::worker::{MouseState, Worker};

fn update(worker: &Worker, id: &str, f: impl FnOnce(&mut crate::protocol::mouse::MouseConfig)) {
    if let Some(crate::worker::DeviceData::Mouse(m)) = worker.lock().devices.iter_mut().find(|d| d.id == id).map(|d| &mut d.data) {
        if let Some(c) = m.config.as_mut() {
            f(c);
        }
    }
}

/// Applies `local` to the cached config right away and writes to the mouse in the background.
fn change(
    worker: &Worker,
    id: &str,
    local: impl FnOnce(&mut crate::protocol::mouse::MouseConfig),
    write: impl FnOnce(&mut crate::protocol::mouse::CompxMouse) -> anyhow::Result<()> + Send + 'static,
) {
    update(worker, id, local);
    let id2 = id.to_string();
    worker.run(id, None, move |b| write(b.mouse(&id2)?));
}

fn rgb(c: Rgb) -> Color32 {
    Color32::from_rgb(c.r, c.g, c.b)
}

pub fn page(ui: &mut Ui, s: &Strings, worker: &Worker, id: &str, m: &MouseState, st: &mut DeviceUi, tab: usize) {
    if !m.online {
        ui.add_space(60.0);
        ui.vertical_centered(|ui| {
            ui.label(egui::RichText::new(icon::BATTERY_MEDIUM).font(body(42.0)).color(theme::text3()));
            ui.add_space(6.0);
            ui.label(egui::RichText::new(s.mouse_asleep_title).font(display(20.0)));
            ui.label(egui::RichText::new(s.mouse_asleep).color(theme::text3()));
        });
        return;
    }
    let (Some(cfg), Some(identity)) = (m.config.clone(), m.identity) else {
        Panel::new("", "").show(ui, |ui| spinner_row(ui, s.loading));
        return;
    };
    match tab {
        0 => dpi(ui, s, worker, id, &cfg, identity.sensor.max_dpi, st),
        1 => sensor(ui, s, worker, id, &cfg, &identity),
        2 => buttons(ui, s, worker, id, &cfg),
        3 => lighting(ui, s, worker, id, &cfg, st),
        _ => power(ui, s, worker, id, &cfg, m),
    }
}

// ── DPI ────────────────────────────────────────────────────────────────

fn dpi(ui: &mut Ui, s: &Strings, worker: &Worker, id: &str, cfg: &crate::protocol::mouse::MouseConfig, max_dpi: u32, st: &mut DeviceUi) {
    let count = cfg.stage_count;
    let sel = st.edit_stage.min(count - 1);
    let mut new_count = None;
    Panel::new(icon::CROSSHAIR, s.dpi_stages)
        .desc(s.dpi_stages_hint)
        .actions(|ui| {
            if button(ui, icon::PLUS, Kind::Normal, true, count < cfg.dpi.len()).clicked() {
                new_count = Some(count + 1);
            }
            ui.label(egui::RichText::new(count.to_string()).font(mono(14.0)));
            if button(ui, icon::MINUS, Kind::Normal, true, count > 1).clicked() {
                new_count = Some(count - 1);
            }
            ui.label(egui::RichText::new(s.stage_count).font(body(12.5)).color(theme::text3()));
        })
        .show(ui, |ui| {
            columns_grid(ui, 6, 12.0, count, |ui, i| {
                let r = stage_card(ui, s, i, cfg.dpi[i], rgb(cfg.dpi_colors[i]), sel == i, cfg.current_stage == i);
                if r.clicked() {
                    st.edit_stage = i;
                }
                if r.double_clicked() {
                    change(worker, id, move |c| c.current_stage = i, move |m| m.set_current_stage(i));
                }
            });
        });
    if let Some(n) = new_count {
        let current = cfg.current_stage.min(n - 1);
        let moved = current != cfg.current_stage;
        change(worker, id, move |c| {
            c.stage_count = n;
            c.current_stage = current;
        }, move |m| {
            m.set_stage_count(n)?;
            if moved {
                m.set_current_stage(current)?;
            }
            Ok(())
        });
    }

    ui.add_space(18.0);
    Panel::new(icon::CROSSHAIR, &format!("{} {}", s.stage, sel + 1)).show(ui, |ui| {
        let set_dpi = |v: u32| change(worker, id, move |c| c.dpi[sel] = v, move |m| m.set_dpi(sel, v));
        if let Some(v) = Slider::new(s.sensitivity, 50.0, max_dpi as f32, 50.0).unit("DPI").show(ui, cfg.dpi[sel] as f32).committed {
            set_dpi(v as u32);
        }
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 6.0;
            for p in [400u32, 800, 1600, 3200, 6400, 12800, 26000].into_iter().filter(|&p| p <= max_dpi) {
                if button(ui, &p.to_string(), Kind::Normal, true, true).clicked() {
                    set_dpi(p);
                }
            }
        });
        ui.add_space(8.0);
        row(ui, false, s.stage_color, None, |ui| {
            let mut c = st.stage_color_edit.filter(|e| e.0 == sel).map(|e| rgb(e.1)).unwrap_or(rgb(cfg.dpi_colors[sel]));
            if egui::widgets::color_picker::color_edit_button_srgba(ui, &mut c, egui::widgets::color_picker::Alpha::Opaque).changed() {
                st.stage_color_edit = Some((sel, Rgb { r: c.r(), g: c.g(), b: c.b() }));
            }
        });
        row(ui, false, s.active, None, |ui| {
            if button(ui, s.make_active, Kind::Normal, true, cfg.current_stage != sel).clicked() {
                change(worker, id, move |c| c.current_stage = sel, move |m| m.set_current_stage(sel));
            }
        });
    });
    if let Some((i, c)) = st.stage_color_edit {
        if !ui.input(|inp| inp.pointer.any_down()) {
            st.stage_color_edit = None;
            change(worker, id, move |cfg| cfg.dpi_colors[i] = c, move |m| m.set_dpi_color(i, c));
        }
    }
}

fn stage_card(ui: &mut Ui, s: &Strings, i: usize, dpi: u32, color: Color32, editing: bool, current: bool) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 96.0), Sense::click());
    let resp = resp.on_hover_cursor(CursorIcon::PointingHand);
    let p = ui.painter();
    if editing {
        theme::glow(p, rect.translate(vec2(0.0, 8.0)).shrink(10.0), 14.0, alpha(color, 0.45), 18.0);
    }
    // Opaque fill so the selection glow only shows around the card, tinted by the
    // stage colour from the top-left corner (inside the rounded shape).
    let (center, reach) = (rect.left_top() + vec2(10.0, 8.0), vec2(120.0, 60.0));
    theme::fill_rounded(p, rect, 14.0, |pos| {
        let d = (((pos.x - center.x) / reach.x).powi(2) + ((pos.y - center.y) / reach.y).powi(2)).sqrt();
        theme::lerp(theme::card_solid(), color, 0.22 * (1.0 - d).max(0.0))
    });
    let border = if editing {
        Stroke::new(1.5, color)
    } else if resp.hovered() {
        Stroke::new(1.0, theme::line_strong())
    } else {
        Stroke::new(1.0, theme::line())
    };
    p.rect_stroke(rect, 14.0, border, StrokeKind::Inside);
    let dot = rect.left_top() + vec2(18.0, 20.0);
    p.circle_filled(dot, 7.0, alpha(color, 0.35));
    p.circle_filled(dot, 4.0, color);
    p.text(dot + vec2(10.0, 0.0), Align2::LEFT_CENTER, format!("{} {}", s.stage.to_uppercase(), i + 1), semibold(11.0), theme::text3());
    if current {
        let g = p.layout_no_wrap(s.active.to_uppercase(), semibold(9.5), Color32::BLACK);
        let pill = Rect::from_min_size(pos2(rect.max.x - g.size().x - 22.0, dot.y - 9.0), vec2(g.size().x + 12.0, 18.0));
        p.rect_filled(pill, 9.0, color);
        p.galley(pos2(pill.min.x + 6.0, pill.center().y - g.size().y / 2.0), g, Color32::BLACK);
    }
    p.text(rect.left_top() + vec2(14.0, 52.0), Align2::LEFT_CENTER, dpi.to_string(), display(28.0), theme::text());
    p.text(rect.left_top() + vec2(14.0, 78.0), Align2::LEFT_CENTER, "DPI", semibold(10.5), theme::text3());
    resp
}

// ── Sensor ─────────────────────────────────────────────────────────────

fn sensor(ui: &mut Ui, s: &Strings, worker: &Worker, id: &str, cfg: &crate::protocol::mouse::MouseConfig, identity: &crate::protocol::mouse::Identity) {
    Panel::new(icon::GAUGE, s.polling_rate).desc(s.polling_hint).show(ui, |ui| {
        let rates: Vec<(u32, String)> =
            REPORT_RATES.iter().filter(|&&r| r <= identity.max_rate).map(|&r| (r, if r >= 1000 { format!("{}K", r / 1000) } else { r.to_string() })).collect();
        if let Some(r) = segmented(ui, Some(cfg.report_rate), &rates, true) {
            change(worker, id, move |c| c.report_rate = r, move |m| m.set_report_rate(r));
        }
        ui.add_space(8.0);
        row(ui, false, s.sensor_mode, (!identity.wired).then_some(s.sensor_mode_hint), |ui| {
            if let Some(v) = segmented(ui, Some(cfg.sensor_mode), &[(0, s.sensor_lp.into()), (1, s.sensor_hp.into())], !identity.wired) {
                change(worker, id, move |c| c.sensor_mode = v, move |m| m.set_sensor_mode(v));
            }
        });
    });
    ui.add_space(18.0);
    columns_grid(ui, 2, 18.0, 2, |ui, i| {
        if i == 0 {
            Panel::new(icon::ACTIVITY, s.tracking).show(ui, |ui| {
                row(ui, true, s.lod, None, |ui| {
                    let options: Vec<(u8, String)> = identity.sensor.lod.iter().map(|(v, l)| (*v, l.to_string())).collect();
                    if let Some(v) = segmented(ui, Some(cfg.lod), &options, true) {
                        change(worker, id, move |c| c.lod = v, move |m| m.set_lod(v));
                    }
                });
                let flags = [
                    (s.motion_sync, Some(s.motion_sync_hint), cfg.motion_sync, addr::MOTION_SYNC),
                    (s.ripple, None, cfg.ripple, addr::RIPPLE),
                    (s.angle_snap, None, cfg.angle_snap, addr::ANGLE_SNAP),
                ];
                for (label, hint, on, address) in flags {
                    row(ui, false, label, hint, |ui| {
                        if let Some(v) = toggle(ui, on, true) {
                            change(
                                worker,
                                id,
                                move |c| match address {
                                    addr::MOTION_SYNC => c.motion_sync = v,
                                    addr::RIPPLE => c.ripple = v,
                                    _ => c.angle_snap = v,
                                },
                                move |m| m.set_flag(address, v),
                            );
                        }
                    });
                }
            });
        } else {
            Panel::new(icon::TIMER, s.debounce).show(ui, |ui| {
                if let Some(v) = Slider::new(s.debounce, 0.0, 15.0, 1.0).unit("ms").show(ui, cfg.debounce as f32).committed {
                    let v = v as u8;
                    change(worker, id, move |c| c.debounce = v, move |m| m.set_debounce(v));
                }
            });
        }
    });
}

// ── Buttons ────────────────────────────────────────────────────────────

fn buttons(ui: &mut Ui, s: &Strings, worker: &Worker, id: &str, cfg: &crate::protocol::mouse::MouseConfig) {
    let actions: [(&str, Button); 7] = [
        (s.btn_left, Button { kind: BUTTON_MOUSE, param: 0x0100 }),
        (s.btn_right, Button { kind: BUTTON_MOUSE, param: 0x0200 }),
        (s.btn_middle, Button { kind: BUTTON_MOUSE, param: 0x0400 }),
        (s.btn_back, Button { kind: BUTTON_MOUSE, param: 0x0800 }),
        (s.btn_forward, Button { kind: BUTTON_MOUSE, param: 0x1000 }),
        (s.btn_dpi, Button { kind: BUTTON_DPI, param: 0x0100 }),
        (s.btn_disabled, Button { kind: BUTTON_DISABLED, param: 0 }),
    ];
    let left = actions[0].1;
    let left_count = cfg.buttons.iter().filter(|b| **b == left).count();
    Panel::new(icon::POINTER, s.buttons).show(ui, |ui| {
        for slot in 0..BUTTON_SLOTS {
            let current = cfg.buttons[slot];
            let label = actions.iter().find(|(_, b)| *b == current).map(|(l, _)| *l).unwrap_or(s.btn_custom);
            // Never allow removing the last left click — the mouse would become unusable.
            let locked = current == left && left_count == 1;
            row(ui, slot == 0, s.button_slots[slot], None, |ui| {
                egui::ComboBox::from_id_salt(("button", id, slot)).selected_text(label).width(230.0).show_ui(ui, |ui| {
                    for (name, b) in actions {
                        let enabled = !locked || b == left;
                        if ui.add_enabled(enabled, egui::Button::selectable(current == b, name)).clicked() && current != b {
                            change(worker, id, move |c| c.buttons[slot] = b, move |m| m.set_button(slot, b));
                        }
                    }
                });
            });
        }
    });
}

// ── Lighting ───────────────────────────────────────────────────────────

fn lighting(ui: &mut Ui, s: &Strings, worker: &Worker, id: &str, cfg: &crate::protocol::mouse::MouseConfig, st: &mut DeviceUi) {
    let light = cfg.light;
    let on = light.on && light.mode != 0;
    let (has_color, has_brightness, has_speed) = light_controls(light.mode);
    let color = st.light_color_edit.map(rgb).unwrap_or(rgb(light.color));
    let set = |next: Light| change(worker, id, move |c| c.light = next, move |m| m.set_light(&next));
    ui.ctx().request_repaint_after(std::time::Duration::from_millis(40));
    let time = ui.input(|i| i.time) as f32;

    let mut sp = split(ui, 330.0, 18.0);
    {
        let ui = &mut sp.left;
        let mut toggled = None;
        Panel::new(icon::SPARKLES, s.effect).actions(|ui| toggled = toggle(ui, on, true)).show(ui, |ui| {
            columns_grid(ui, 3, 10.0, 6, |ui, i| {
                let mode = i as u8 + 1;
                let preview = match mode {
                    2 => Preview::Pulse,
                    3 => Preview::Solid,
                    _ => Preview::Rainbow,
                };
                let tint = light_controls(mode).0.then_some(color);
                if effect_card(ui, s.light_modes[mode as usize], preview, tint, on && light.mode == mode, on, time).clicked() {
                    set(Light { mode, on: true, ..light });
                }
            });
        });
        if let Some(v) = toggled {
            set(Light { on: v, mode: if v && light.mode == 0 { 3 } else { light.mode }, ..light });
        }
    }
    {
        let ui = &mut sp.right;
        Panel::new(icon::SLIDERS, s.lighting).show(ui, |ui| {
            if let Some(v) = Slider::new(s.brightness, 1.0, 9.0, 1.0).enabled(on && has_brightness).show(ui, light.brightness as f32).committed {
                set(Light { brightness: v as u8, ..light });
            }
            ui.add_space(8.0);
            if let Some(v) = Slider::new(s.speed, 1.0, 9.0, 1.0).enabled(on && has_speed).show(ui, light.speed as f32).committed {
                set(Light { speed: v as u8, ..light });
            }
            if has_color {
                ui.add_space(6.0);
                row(ui, false, s.color, None, |ui| {
                    let mut c = color;
                    if ui.add_enabled_ui(on, |ui| egui::widgets::color_picker::color_edit_button_srgba(ui, &mut c, egui::widgets::color_picker::Alpha::Opaque)).inner.changed() {
                        st.light_color_edit = Some(Rgb { r: c.r(), g: c.g(), b: c.b() });
                    }
                });
            }
            row(ui, false, s.light_off_moving, None, |ui| {
                if let Some(v) = toggle(ui, cfg.light_off_moving, true) {
                    change(worker, id, move |c| c.light_off_moving = v, move |m| m.set_flag(addr::LIGHT_OFF_MOVING, v));
                }
            });
        });
    }
    sp.finish(ui);
    if let Some(c) = st.light_color_edit {
        if !ui.input(|inp| inp.pointer.any_down()) {
            st.light_color_edit = None;
            set(Light { color: c, ..light });
        }
    }
}

// ── Power ──────────────────────────────────────────────────────────────

fn power(ui: &mut Ui, s: &Strings, worker: &Worker, id: &str, cfg: &crate::protocol::mouse::MouseConfig, m: &MouseState) {
    masonry(ui, 2, 18.0, 3, |ui, i| match i {
        0 => Panel::new(icon::BATTERY_CHARGING, s.battery).show(ui, |ui| {
            if let Some(b) = m.battery {
                ui.horizontal(|ui| {
                    let (rect, _) = ui.allocate_exact_size(vec2(160.0, 64.0), Sense::hover());
                    let p = ui.painter();
                    let shell = Rect::from_min_size(rect.min, vec2(150.0, 64.0));
                    p.rect_stroke(shell, 14.0, Stroke::new(2.0, theme::line_strong()), StrokeKind::Inside);
                    p.rect_filled(Rect::from_min_size(pos2(shell.max.x + 2.0, shell.center().y - 9.0), vec2(6.0, 18.0)), 3.0, theme::line_strong());
                    let inner = shell.shrink(7.0);
                    let fill = Rect::from_min_size(inner.min, vec2(inner.width() * b.level as f32 / 100.0, inner.height()));
                    let low = b.level <= 15;
                    let t = ui.input(|i| i.time) as f32;
                    gradient_rect(p, fill, 8.0, 0.0, |x| {
                        let x = if b.charging { (x + t * 0.5).rem_euclid(1.0) } else { x };
                        if low {
                            theme::lerp(theme::BAD, theme::WARN, x)
                        } else {
                            theme::lerp(theme::GOOD, theme::ACCENT2, x)
                        }
                    });
                    if b.charging {
                        ui.ctx().request_repaint_after(std::time::Duration::from_millis(40));
                    }
                    ui.add_space(12.0);
                    ui.vertical(|ui| {
                        ui.label(egui::RichText::new(format!("{}%", b.level)).font(display(34.0)));
                        let mut detail = String::new();
                        if b.charging {
                            detail.push_str(s.charging);
                            detail.push(' ');
                        }
                        if b.millivolts > 0 {
                            detail.push_str(&format!("{:.2} V", b.millivolts as f32 / 1000.0));
                        }
                        ui.label(egui::RichText::new(detail).color(theme::text3()));
                    });
                });
            }
        }),
        1 => Panel::new(icon::MOON, s.sleep_after).desc(s.sleep_hint).show(ui, |ui| {
            let label = |v: u8| if v < 6 { format!("{} {}", v as u32 * 10, s.seconds) } else { format!("{} {}", v as u32 / 6, s.minutes) };
            egui::ComboBox::from_id_salt(("sleep", id)).selected_text(label(cfg.sleep)).width(160.0).show_ui(ui, |ui| {
                for v in SLEEP_TIMES {
                    if ui.selectable_label(cfg.sleep == v, label(v)).clicked() && cfg.sleep != v {
                        change(worker, id, move |c| c.sleep = v, move |m| m.set_sleep(v));
                    }
                }
            });
        }),
        _ => Panel::new(icon::CPU, s.device_info).show(ui, |ui| {
            let id_ = m.identity;
            let mut items: Vec<(&str, String)> = Vec::new();
            if let Some((mouse_fw, dongle)) = &m.firmware {
                items.push((s.mouse_fw, mouse_fw.clone()));
                if let Some(d) = dongle {
                    items.push((s.dongle_fw, d.clone()));
                }
            }
            if let Some(i) = id_ {
                items.push((s.sensor, i.sensor.name.to_string()));
                items.push((s.connection, if i.wired { s.wired.to_string() } else { format!("{} · {}K", s.wireless, i.max_rate / 1000) }));
                items.push(("Model ID", format!("{}/{}", i.cid, i.mid)));
            }
            columns_grid(ui, 2, 16.0, items.len(), |ui, i| stat(ui, items[i].0, &items[i].1));
        }),
    });
}
