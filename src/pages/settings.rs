//! App settings: start with Windows, theme and language.

use egui::{vec2, Sense, Ui};

use crate::i18n::{Lang, Strings};
use crate::theme::{self, body, icon, mono, ThemePref};
use crate::widgets::{masonry, row, segmented, toggle, Panel};
use crate::worker::Worker;

pub struct Settings<'a> {
    pub lang: &'a mut Lang,
    pub theme: &'a mut ThemePref,
    pub glass: &'a mut bool,
    /// Mirrors the registry entry; updated after a successful change.
    pub autostart: &'a mut bool,
}

pub fn page(ui: &mut Ui, s: &Strings, worker: &Worker, st: Settings<'_>) {
    masonry(ui, 2, 18.0, 3, |ui, i| match i {
        0 => Panel::new(icon::POWER, s.general).show(ui, |ui| {
            row(ui, true, s.autostart, Some(s.autostart_hint), |ui| {
                if let Some(on) = toggle(ui, *st.autostart, true) {
                    match crate::autostart::set_enabled(on) {
                        Ok(()) => *st.autostart = crate::autostart::is_enabled(),
                        Err(e) => worker.lock().toast(format!("{}: {e}", s.autostart), true),
                    }
                }
            });
        }),
        1 => Panel::new(icon::SUN, s.appearance).show(ui, |ui| {
            row(ui, true, s.theme, None, |ui| {
                let options = [
                    (ThemePref::Dark, format!("{}  {}", icon::MOON, s.theme_dark)),
                    (ThemePref::Light, format!("{}  {}", icon::SUN, s.theme_light)),
                    (ThemePref::System, format!("{}  {}", icon::MONITOR, s.theme_system)),
                ];
                if let Some(t) = segmented(ui, Some(*st.theme), &options, true) {
                    *st.theme = t;
                }
            });
            if cfg!(windows) {
                row(ui, false, s.glass, Some(s.glass_hint), |ui| {
                    if let Some(on) = toggle(ui, *st.glass, true) {
                        *st.glass = on;
                    }
                });
            }
            row(ui, false, s.language, None, |ui| {
                if let Some(l) = segmented(ui, Some(*st.lang), &[(Lang::De, "Deutsch".into()), (Lang::En, "English".into())], true) {
                    *st.lang = l;
                }
            });
        }),
        _ => Panel::new(icon::INFO, s.about).show(ui, |ui| {
            ui.horizontal(|ui| {
                let (mark, _) = ui.allocate_exact_size(vec2(40.0, 40.0), Sense::hover());
                crate::app::brand_mark(ui.painter(), mark);
                ui.add_space(6.0);
                ui.add(egui::Label::new(egui::RichText::new(s.about_text).font(body(12.5)).color(theme::text2())).wrap());
            });
            ui.add_space(6.0);
            let value = |ui: &mut Ui, v: &str| {
                ui.label(egui::RichText::new(v).font(mono(13.5)).color(theme::text2()));
            };
            row(ui, false, s.version, None, |ui| value(ui, env!("CARGO_PKG_VERSION")));
            row(ui, false, s.license, None, |ui| value(ui, "MIT"));
        }),
    });
}
