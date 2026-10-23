//! Scripted screenshots for development and documentation.
//!
//! `ORBIT_STEPS="device=1;tab=2;wait=1.5;shot=out.png"` drives the UI through
//! the steps, saves PNG screenshots and closes the window afterwards.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Instant;

use egui::{ColorImage, ViewportCommand};

use crate::app::DeviceUi;
use crate::worker::DeviceEntry;

enum Step {
    Device(usize),
    Tab(usize),
    Area(u8),
    Wait(f64),
    Shot(PathBuf),
    /// Open the tray popup as if the tray icon had been clicked.
    Popup,
    /// Hide the main window as if it had been closed to the tray.
    Hide,
    /// Minimize the main window to the taskbar.
    Minimize,
    /// Show the settings page.
    Settings,
    /// Switch the theme (dark, light, system).
    Theme(String),
    Glass(bool),
    Lang(String),
    /// Keys to select on the keyboard page, as `row:col` pairs.
    Select(Vec<(u8, u8)>),
    /// Screenshot of the tray popup instead of the main window.
    PopupShot(PathBuf),
}

pub struct Automation {
    steps: Vec<Step>,
    next: usize,
    started: Instant,
    wait_until: Option<f64>,
    shot_done: Option<Arc<AtomicBool>>,
    pub open_popup: bool,
    pub hide_window: bool,
    pub minimize_window: bool,
    pub open_settings: bool,
    /// A device was picked: leave the settings page.
    pub close_settings: bool,
    pub theme: Option<String>,
    pub glass: Option<bool>,
    pub lang: Option<String>,
}

impl Automation {
    pub fn from_env() -> Option<Self> {
        let spec = std::env::var("ORBIT_STEPS").ok()?;
        let steps = spec
            .split(';')
            .filter_map(|part| {
                let (k, v) = part.split_once('=')?;
                Some(match k.trim() {
                    "device" => Step::Device(v.parse().ok()?),
                    "tab" => Step::Tab(v.parse().ok()?),
                    "area" => Step::Area(v.parse().ok()?),
                    "wait" => Step::Wait(v.parse().ok()?),
                    "shot" => Step::Shot(PathBuf::from(v.trim())),
                    "popup" => Step::Popup,
                    "hide" => Step::Hide,
                    "minimize" => Step::Minimize,
                    "settings" => Step::Settings,
                    "theme" => Step::Theme(v.trim().to_string()),
                    "glass" => Step::Glass(v.trim() == "1"),
                    "lang" => Step::Lang(v.trim().to_string()),
                    "select" => Step::Select(
                        v.split(',')
                            .filter_map(|k| k.split_once(':'))
                            .filter_map(|(r, c)| Some((r.trim().parse().ok()?, c.trim().parse().ok()?)))
                            .collect(),
                    ),
                    "popupshot" => Step::PopupShot(PathBuf::from(v.trim())),
                    _ => return None,
                })
            })
            .collect();
        Some(Self { steps, next: 0, started: Instant::now(), wait_until: None, shot_done: None, open_popup: false, hide_window: false, minimize_window: false, open_settings: false, close_settings: false, theme: None, glass: None, lang: None })
    }

    pub fn drive(&mut self, ctx: &egui::Context, devices: &[DeviceEntry], active: &mut Option<String>, ui: &mut HashMap<String, DeviceUi>) {
        ctx.request_repaint();
        // Wall-clock time: egui's input time stands still while the window is minimized.
        let now = self.started.elapsed().as_secs_f64();
        if let Some(t) = self.wait_until {
            if now < t {
                return;
            }
            self.wait_until = None;
        }
        if let Some(done) = &self.shot_done {
            if !done.load(Ordering::SeqCst) {
                return;
            }
            self.shot_done = None;
        }
        let Some(step) = self.steps.get(self.next) else {
            ctx.send_viewport_cmd(ViewportCommand::Close);
            return;
        };
        self.next += 1;
        match step {
            Step::Device(i) => {
                self.close_settings = true;
                if let Some(d) = devices.get(*i) {
                    *active = Some(d.id.clone());
                }
            }
            Step::Tab(t) => {
                if let Some(a) = active.as_ref() {
                    ui.entry(a.clone()).or_default().tab = *t;
                }
            }
            Step::Area(x) => {
                if let Some(a) = active.as_ref() {
                    ui.entry(a.clone()).or_default().area = *x;
                }
            }
            Step::Wait(s) => self.wait_until = Some(now + s),
            Step::Popup => self.open_popup = true,
            Step::Hide => self.hide_window = true,
            Step::Minimize => self.minimize_window = true,
            Step::Settings => self.open_settings = true,
            Step::Theme(t) => self.theme = Some(t.clone()),
            Step::Glass(g) => self.glass = Some(*g),
            Step::Lang(l) => self.lang = Some(l.clone()),
            Step::Select(keys) => {
                if let Some(a) = active.as_ref() {
                    let st = ui.entry(a.clone()).or_default();
                    st.selected = keys.iter().copied().collect();
                    st.selection_ready = true;
                }
            }
            Step::Shot(path) | Step::PopupShot(path) => {
                let target = if matches!(step, Step::PopupShot(_)) { crate::app::popup_viewport() } else { egui::ViewportId::ROOT };
                let done = Arc::new(AtomicBool::new(false));
                let (path, flag) = (path.clone(), done.clone());
                ctx.send_viewport_cmd_to(target, ViewportCommand::Screenshot(egui::ScreenshotCallback::new(move |img| {
                    if let Err(e) = std::fs::write(&path, encode_png(&img)) {
                        eprintln!("screenshot {}: {e}", path.display());
                    }
                    flag.store(true, Ordering::SeqCst);
                })));
                self.shot_done = Some(done);
            }
        }
    }
}

/// Minimal PNG encoder (RGBA, uncompressed deflate) — avoids an image dependency.
fn encode_png(img: &ColorImage) -> Vec<u8> {
    let [w, h] = img.size;
    let mut raw = Vec::with_capacity((w * 4 + 1) * h);
    for y in 0..h {
        raw.push(0);
        for x in 0..w {
            raw.extend_from_slice(&img.pixels[y * w + x].to_srgba_unmultiplied());
        }
    }
    let mut z = vec![0x78, 0x01];
    let chunks: Vec<&[u8]> = raw.chunks(65535).collect();
    for (i, c) in chunks.iter().enumerate() {
        z.push((i + 1 == chunks.len()) as u8);
        z.extend_from_slice(&(c.len() as u16).to_le_bytes());
        z.extend_from_slice(&(!(c.len() as u16)).to_le_bytes());
        z.extend_from_slice(c);
    }
    let (mut a, mut b) = (1u32, 0u32);
    for &byte in &raw {
        a = (a + byte as u32) % 65521;
        b = (b + a) % 65521;
    }
    z.extend_from_slice(&((b << 16) | a).to_be_bytes());

    let mut png = vec![0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a];
    let mut ihdr = Vec::new();
    ihdr.extend_from_slice(&(w as u32).to_be_bytes());
    ihdr.extend_from_slice(&(h as u32).to_be_bytes());
    ihdr.extend_from_slice(&[8, 6, 0, 0, 0]);
    for (kind, data) in [(b"IHDR", ihdr), (b"IDAT", z), (b"IEND", Vec::new())] {
        png.extend_from_slice(&(data.len() as u32).to_be_bytes());
        let start = png.len();
        png.extend_from_slice(kind);
        png.extend_from_slice(&data);
        let crc = crc32(&png[start..]);
        png.extend_from_slice(&crc.to_be_bytes());
    }
    png
}

fn crc32(data: &[u8]) -> u32 {
    let mut crc = 0xffff_ffffu32;
    for &byte in data {
        crc ^= byte as u32;
        for _ in 0..8 {
            crc = if crc & 1 != 0 { (crc >> 1) ^ 0xedb8_8320 } else { crc >> 1 };
        }
    }
    !crc
}
