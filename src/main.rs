//! Orbit — open-source, fully offline configurator for GravaStar keyboards and mice.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod autostart;
mod chrome;
mod cli;
mod demo;
mod hid;
mod i18n;
mod pages;
mod popup;
mod protocol;
mod shot;
mod theme;
mod tray;
mod widgets;
mod worker;

fn main() -> eframe::Result {
    let args: Vec<String> = std::env::args().collect();
    if let Some(code) = cli::run(&args) {
        std::process::exit(code);
    }
    // ORBIT_DEMO=1 runs against emulated devices instead of real hardware.
    let demo = std::env::var("ORBIT_DEMO").is_ok_and(|v| v == "1");
    // Started by "Start with Windows": stay in the tray.
    let start_hidden = args.iter().any(|a| a == autostart::TRAY_ARG);
    let mut viewport = egui::ViewportBuilder::default()
        .with_title("Orbit — GravaStar Configurator")
        // Orbit draws its own caption buttons (see chrome.rs).
        .with_decorations(false)
        // Transparent so the window can have rounded corners (painted by Orbit).
        .with_transparent(true)
        .with_app_id("orbit")
        .with_inner_size([1280.0, 820.0])
        .with_min_inner_size([1040.0, 680.0]);
    if let Ok(icon) = eframe::icon_data::from_png_bytes(include_bytes!("../assets/icon.png")) {
        viewport = viewport.with_icon(icon);
    }
    let mut options = eframe::NativeOptions { viewport, ..Default::default() };
    // Scripted screenshot runs get their own, throwaway storage: default window size and
    // settings, and nothing of the user's saved state is read or overwritten.
    if std::env::var_os("ORBIT_STEPS").is_some() {
        options.persist_window = false;
        options.persistence_path = Some(std::env::temp_dir().join("orbit-scripted"));
        // ORBIT_SIZE=WxH picks another window size, e.g. for README screenshots.
        let size = std::env::var("ORBIT_SIZE").ok().and_then(|s| {
            let (w, h) = s.split_once('x')?;
            Some([w.parse::<f32>().ok()?, h.parse::<f32>().ok()?])
        });
        if let Some(size) = size {
            options.viewport = options.viewport.with_inner_size(size);
        }
    }
    eframe::run_native("Orbit", options, Box::new(move |cc| Ok(Box::new(app::OrbitApp::new(cc, demo, start_hidden)))))
}
