//! Background thread that owns every HID handle. The UI never blocks on USB:
//! it queues jobs here and renders from the shared [`Shared`] snapshot.

use std::collections::HashMap;
use std::sync::mpsc::{self, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant};

use anyhow::{anyhow, Result};
use hidapi::HidApi;

use crate::demo;
use crate::hid::{HidLink, NoAnswer};
use crate::protocol::keyboard::{HeKeyboard, KeyPerf, KeySlot, KeyboardInfo, LightingArea, LightingBase};
use crate::protocol::mouse::{Battery, CompxMouse, Identity, MouseConfig};
use crate::protocol::Rgb;

#[derive(Clone, Default)]
pub struct KbState {
    pub info: Option<KeyboardInfo>,
    pub profile: u8,
    pub profile_count: u8,
    pub rates: Vec<u8>,
    pub rate: Option<u8>,
    pub system: Option<u8>,
    pub rt_precision: f32,
    pub slots: Vec<KeySlot>,
    pub codes: HashMap<(u8, u8), u16>,
    pub perf: HashMap<(u8, u8), KeyPerf>,
    pub areas: Vec<LightingArea>,
    pub lighting: HashMap<u8, LightingBase>,
    pub palettes: HashMap<u8, Vec<Rgb>>,
    pub light_sleep: Option<u16>,
    /// Keys read so far while loading (done, total).
    pub loading: Option<(usize, usize)>,
    /// Progress of a multi-key write (done, total).
    pub writing: Option<(usize, usize)>,
    pub calibrating: bool,
    /// Keys currently pressed: travel in mm and whether the key has actuated.
    pub live: HashMap<(u8, u8), (f32, bool)>,
}

#[derive(Clone, Default)]
pub struct MouseState {
    pub online: bool,
    pub identity: Option<Identity>,
    pub battery: Option<Battery>,
    pub config: Option<MouseConfig>,
    pub firmware: Option<(String, Option<String>)>,
}

#[derive(Clone)]
pub enum DeviceData {
    Keyboard(KbState),
    Mouse(MouseState),
}

#[derive(Clone)]
pub struct DeviceEntry {
    pub id: String,
    pub name: String,
    pub data: DeviceData,
}

pub struct Toast {
    pub text: String,
    pub error: bool,
    pub born: Instant,
}

#[derive(Default)]
pub struct Shared {
    pub devices: Vec<DeviceEntry>,
    pub scanned: bool,
    pub toasts: Vec<Toast>,
    /// The UI renews this while it shows live key travel for a keyboard.
    pub live_request: Option<(String, Instant)>,
}

impl Shared {
    pub fn toast(&mut self, text: impl Into<String>, error: bool) {
        if self.toasts.len() > 3 {
            self.toasts.remove(0);
        }
        self.toasts.push(Toast { text: text.into(), error, born: Instant::now() });
    }
}

type Job = Box<dyn FnOnce(&mut Backend) -> Result<()> + Send>;

struct Task {
    device: String,
    ok: Option<String>,
    job: Job,
}

/// UI-side handle to the worker thread.
#[derive(Clone)]
pub struct Worker {
    tx: Sender<Task>,
    pub shared: Arc<Mutex<Shared>>,
}

impl Worker {
    pub fn spawn(ctx: egui::Context, demo_mode: bool) -> Self {
        let shared = Arc::new(Mutex::new(Shared::default()));
        let (tx, rx) = mpsc::channel::<Task>();
        let shared2 = shared.clone();
        std::thread::Builder::new()
            .name("orbit-hid".into())
            .spawn(move || {
                let mut b = Backend::new(shared2, ctx, demo_mode);
                let mut last_scan = Instant::now() - Duration::from_secs(10);
                let mut last_poll = Instant::now();
                let mut last_live = Instant::now();
                loop {
                    if last_scan.elapsed() >= Duration::from_secs(2) {
                        b.scan();
                        last_scan = Instant::now();
                    }
                    if last_poll.elapsed() >= Duration::from_secs(2) {
                        b.poll_mice();
                        last_poll = Instant::now();
                    }
                    let live = b.live_target();
                    if live.is_some() && last_live.elapsed() >= Duration::from_millis(20) {
                        b.poll_live(live.as_deref());
                        last_live = Instant::now();
                    } else if live.is_none() {
                        b.poll_live(None);
                    }
                    let wait = if live.is_some() { 4 } else { 100 };
                    match rx.recv_timeout(Duration::from_millis(wait)) {
                        Ok(task) => b.execute(task),
                        Err(RecvTimeoutError::Timeout) => {}
                        Err(RecvTimeoutError::Disconnected) => break,
                    }
                }
            })
            .expect("spawn HID thread");
        Self { tx, shared }
    }

    pub fn lock(&self) -> MutexGuard<'_, Shared> {
        self.shared.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// Queue a device operation. `ok` is shown as a toast when it succeeds;
    /// errors are always reported.
    pub fn run(&self, device: &str, ok: Option<&str>, job: impl FnOnce(&mut Backend) -> Result<()> + Send + 'static) {
        let _ = self.tx.send(Task { device: device.to_string(), ok: ok.map(str::to_string), job: Box::new(job) });
    }
}

pub struct Backend {
    api: Option<HidApi>,
    demo: bool,
    kbs: HashMap<String, HeKeyboard>,
    /// Matrix rows that hold keys, per keyboard (for live travel polling).
    kb_rows: HashMap<String, Vec<u8>>,
    /// Keyboard whose live travel is currently being polled.
    live_active: Option<String>,
    mice: HashMap<String, CompxMouse>,
    shared: Arc<Mutex<Shared>>,
    ctx: egui::Context,
}

impl Backend {
    fn new(shared: Arc<Mutex<Shared>>, ctx: egui::Context, demo: bool) -> Self {
        let api = if demo { None } else { HidApi::new().ok() };
        Self { api, demo, kbs: HashMap::new(), kb_rows: HashMap::new(), live_active: None, mice: HashMap::new(), shared, ctx }
    }

    fn lock(&self) -> MutexGuard<'_, Shared> {
        self.shared.lock().unwrap_or_else(|e| e.into_inner())
    }

    fn repaint(&self) {
        self.ctx.request_repaint();
    }

    fn execute(&mut self, task: Task) {
        let result = (task.job)(self);
        match result {
            Ok(()) => {
                if let Some(text) = task.ok {
                    self.lock().toast(text, false);
                }
            }
            Err(e) => {
                self.lock().toast(e.to_string(), true);
                // OS errors mean the handle is stale (device rebooted or unplugged):
                // forget it so the next scan reopens it.
                if !e.is::<NoAnswer>() && e.downcast_ref::<hidapi::HidError>().is_some() {
                    self.forget(&task.device);
                }
            }
        }
        self.repaint();
    }

    fn forget(&mut self, id: &str) {
        self.kbs.remove(id);
        self.mice.remove(id);
        self.lock().devices.retain(|d| d.id != id);
    }

    // ── Discovery ──────────────────────────────────────────────────────

    fn scan(&mut self) {
        let found = self.discover();
        let present: Vec<String> = found.iter().map(|(id, ..)| id.clone()).collect();

        for (id, name, kind) in found {
            if self.kbs.contains_key(&id) || self.mice.contains_key(&id) {
                continue;
            }
            match kind {
                Found::Known => {}
                Found::Keyboard(link) => {
                    self.kbs.insert(id.clone(), HeKeyboard::new(link));
                    self.lock().devices.insert(0, DeviceEntry { id: id.clone(), name, data: DeviceData::Keyboard(KbState::default()) });
                    self.repaint();
                    if let Err(e) = self.load_keyboard(&id) {
                        self.lock().toast(e.to_string(), true);
                    }
                }
                Found::Mouse(link) => {
                    self.mice.insert(id.clone(), CompxMouse::new(link));
                    self.lock().devices.push(DeviceEntry { id: id.clone(), name, data: DeviceData::Mouse(MouseState::default()) });
                    self.poll_mouse(&id);
                }
            }
        }

        let gone: Vec<String> = self.kbs.keys().chain(self.mice.keys()).filter(|id| !present.contains(id)).cloned().collect();
        for id in gone {
            self.forget(&id);
        }
        self.lock().scanned = true;
        self.repaint();
    }

    fn discover(&mut self) -> Vec<(String, String, Found)> {
        let mut out = Vec::new();
        if self.demo {
            for (id, name, kb) in [("demo://keyboard", "Mercury V60 Pro", true), ("demo://mouse", "GravaStar Mouse", false)] {
                if self.kbs.contains_key(id) || self.mice.contains_key(id) {
                    out.push((id.to_string(), name.to_string(), Found::Known));
                } else if kb {
                    out.push((id.to_string(), name.to_string(), Found::Keyboard(Box::new(demo::keyboard()))));
                } else {
                    out.push((id.to_string(), name.to_string(), Found::Mouse(Box::new(demo::mouse()))));
                }
            }
            return out;
        }
        let Some(api) = self.api.as_mut() else { return out };
        if api.refresh_devices().is_err() {
            return out;
        }
        for d in api.device_list() {
            let (vid, pid, page) = (d.vendor_id(), d.product_id(), d.usage_page());
            let is_kb = HeKeyboard::matches(vid, pid, page);
            let is_mouse = CompxMouse::matches(vid, pid, page);
            if !is_kb && !is_mouse {
                continue;
            }
            let id = d.path().to_string_lossy().into_owned();
            if self.kbs.contains_key(&id) || self.mice.contains_key(&id) {
                out.push((id, String::new(), Found::Known));
                continue;
            }
            let Ok(dev) = d.open_device(api) else { continue };
            if is_kb {
                let name = d.product_string().unwrap_or("GravaStar Keyboard").trim_start_matches("GravaStar").trim().to_string();
                out.push((id, name, Found::Keyboard(Box::new(HidLink::new(dev, false)))));
            } else {
                out.push((id, "GravaStar Mouse".to_string(), Found::Mouse(Box::new(HidLink::new(dev, true)))));
            }
        }
        out
    }

    // ── Shared state helpers ───────────────────────────────────────────

    pub fn update_kb(&self, id: &str, f: impl FnOnce(&mut KbState)) {
        if let Some(DeviceData::Keyboard(s)) = self.lock().devices.iter_mut().find(|d| d.id == id).map(|d| &mut d.data) {
            f(s);
        }
        self.repaint();
    }

    pub fn update_mouse(&self, id: &str, f: impl FnOnce(&mut MouseState)) {
        if let Some(DeviceData::Mouse(s)) = self.lock().devices.iter_mut().find(|d| d.id == id).map(|d| &mut d.data) {
            f(s);
        }
        self.repaint();
    }

    pub fn kb(&mut self, id: &str) -> Result<&mut HeKeyboard> {
        self.kbs.get_mut(id).ok_or_else(|| anyhow!("keyboard disconnected"))
    }

    pub fn mouse(&mut self, id: &str) -> Result<&mut CompxMouse> {
        self.mice.get_mut(id).ok_or_else(|| anyhow!("mouse disconnected"))
    }

    // ── Keyboard ───────────────────────────────────────────────────────

    /// Reads everything the UI shows. Called on connect and after profile switches.
    pub fn load_keyboard(&mut self, id: &str) -> Result<()> {
        let kb = self.kb(id)?;
        let info = kb.info()?;
        let profile = kb.profile()?;
        let profile_count = kb.profile_count()?;
        let rates = kb.report_rates()?;
        let rate = kb.report_rate().ok();
        let system = kb.system().ok();
        let rt_precision = kb.rt_precision().unwrap_or(0.005);
        let slots = kb.key_slots()?;
        let mut codes = HashMap::new();
        let mut rows: Vec<u8> = slots.iter().map(|s| s.row).collect();
        rows.dedup();
        self.kb_rows.insert(id.to_string(), rows.clone());
        let kb = self.kb(id)?;
        for row in rows {
            for (col, code) in kb.key_codes(0, row)?.into_iter().enumerate() {
                codes.insert((row, col as u8), code);
            }
        }
        let total = slots.len();
        self.update_kb(id, |s| {
            s.info = Some(info);
            s.profile = profile;
            s.profile_count = profile_count;
            s.rates = rates;
            s.rate = rate;
            s.system = system;
            s.rt_precision = if rt_precision > 0.0 { rt_precision } else { 0.005 };
            s.slots = slots.clone();
            s.codes = codes;
            s.loading = Some((0, total));
        });

        let mut perf = HashMap::new();
        for (i, slot) in slots.iter().enumerate() {
            if let Ok(p) = self.kb(id)?.key_perf(slot.row, slot.col) {
                perf.insert((slot.row, slot.col), p);
            }
            if i % 8 == 0 {
                self.update_kb(id, |s| s.loading = Some((i, total)));
            }
        }

        let kb = self.kb(id)?;
        let areas = kb.lighting_areas().unwrap_or_default();
        let mut lighting = HashMap::new();
        let mut palettes = HashMap::new();
        for a in &areas {
            if let Ok(l) = kb.lighting(a.index) {
                lighting.insert(a.index, l);
            }
            if let Ok(p) = kb.palette(a.index) {
                palettes.insert(a.index, p);
            }
        }
        let light_sleep = kb.lighting_sleep().ok();
        self.update_kb(id, |s| {
            s.perf = perf;
            s.areas = areas;
            s.lighting = lighting;
            s.palettes = palettes;
            s.light_sleep = light_sleep;
            s.loading = None;
        });
        Ok(())
    }

    /// Writes `value` to several keys, echoing each key's switch fields.
    pub fn apply_perf(&mut self, id: &str, keys: Vec<((u8, u8), KeyPerf)>) -> Result<()> {
        let total = keys.len();
        self.update_kb(id, |s| s.writing = Some((0, total)));
        let result = (|| {
            for (i, ((row, col), p)) in keys.into_iter().enumerate() {
                self.kb(id)?.set_key_perf(row, col, &p)?;
                self.update_kb(id, |s| {
                    s.perf.insert((row, col), p);
                    s.writing = Some((i + 1, total));
                });
            }
            Ok(())
        })();
        self.update_kb(id, |s| s.writing = None);
        result
    }

    // ── Live key travel ────────────────────────────────────────────────

    /// The keyboard the UI currently wants live travel for (renewed every frame).
    fn live_target(&self) -> Option<String> {
        let shared = self.lock();
        let (id, at) = shared.live_request.as_ref()?;
        (at.elapsed() < Duration::from_millis(600) && self.kbs.contains_key(id)).then(|| id.clone())
    }

    /// One pass over all key rows; publishes only when something changed.
    fn poll_live(&mut self, id: Option<&str>) {
        let Some(id) = id else {
            // Live view closed: clear stale values once.
            if let Some(old) = self.live_active.take() {
                self.update_kb(&old, |s| s.live.clear());
            }
            return;
        };
        self.live_active = Some(id.to_string());
        let rows = self.kb_rows.get(id).cloned().unwrap_or_default();
        let mut live = HashMap::new();
        for row in rows {
            let Some(kb) = self.kbs.get_mut(id) else { return };
            let Ok(values) = kb.live_row(row) else { return };
            for (col, (depth, actuated)) in values.into_iter().enumerate() {
                if depth > 0.05 {
                    live.insert((row, col as u8), (depth, actuated));
                }
            }
        }
        let changed = {
            let shared = self.lock();
            match shared.devices.iter().find(|d| d.id == id).map(|d| &d.data) {
                Some(DeviceData::Keyboard(s)) => s.live != live,
                _ => false,
            }
        };
        if changed {
            self.update_kb(id, |s| s.live = live);
        }
    }

    // ── Mouse ──────────────────────────────────────────────────────────

    fn poll_mice(&mut self) {
        let ids: Vec<String> = self.mice.keys().cloned().collect();
        for id in ids {
            self.poll_mouse(&id);
        }
    }

    /// Online state, battery and the active DPI stage (the DPI button changes it).
    fn poll_mouse(&mut self, id: &str) {
        let Some(mouse) = self.mice.get_mut(id) else { return };
        let online = mouse.online().unwrap_or(false);
        let was_online = matches!(self.mouse_state(id), Some(s) if s.online);
        if !online {
            self.update_mouse(id, |s| s.online = false);
            return;
        }
        let mouse = self.mice.get_mut(id).expect("checked above");
        let mut fresh = None;
        if !was_online || mouse.identity.is_none() {
            // Woke up or just connected: settings may have changed on the device.
            let identity = mouse.identify();
            let config = mouse.read_config();
            let firmware = mouse.firmware().ok();
            if let (Ok(identity), Ok(config)) = (identity, config) {
                fresh = Some((identity, config, firmware));
            } else {
                return;
            }
        }
        let battery = mouse.battery().ok();
        let stage = if fresh.is_none() { mouse.current_stage().ok() } else { None };
        // Show the real product name once the mouse has identified itself.
        if let Some(model) = fresh.as_ref().and_then(|(identity, ..)| identity.model) {
            if let Some(entry) = self.lock().devices.iter_mut().find(|d| d.id == id) {
                entry.name = model.to_string();
            }
        }
        self.update_mouse(id, |s| {
            s.online = true;
            if let Some((identity, config, firmware)) = fresh {
                s.identity = Some(identity);
                s.config = Some(config);
                s.firmware = firmware;
            }
            if battery.is_some() {
                s.battery = battery;
            }
            if let (Some(stage), Some(c)) = (stage, s.config.as_mut()) {
                c.current_stage = stage;
            }
        });
    }

    fn mouse_state(&self, id: &str) -> Option<MouseState> {
        self.lock().devices.iter().find(|d| d.id == id).and_then(|d| match &d.data {
            DeviceData::Mouse(s) => Some(s.clone()),
            _ => None,
        })
    }
}

enum Found {
    Known,
    Keyboard(Box<dyn crate::hid::Link>),
    Mouse(Box<dyn crate::hid::Link>),
}
