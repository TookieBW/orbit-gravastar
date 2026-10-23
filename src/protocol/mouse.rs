//! GravaStar mice with a Compx chipset (2.4 GHz receiver or cable).
//!
//! Requests are output report 8 with a 16-byte payload
//! `[cmd, 0, addrHi, addrLo, len, data × 10, checksum]`, where
//! `(8 + sum of all 16 bytes) & 0xFF == 0x55`. Replies come back on input
//! report 8 with the same header; byte 1 is a status (0 ok, 1 failed).
//!
//! All settings live in a flash map read with 0x08 and written with 0x07.
//! Scalars are stored as `[value, 0x55 - value]`; longer records end in a
//! check byte that makes the record sum to 0x55. See `docs/protocol-mouse.md`.

use std::time::{Duration, SystemTime, UNIX_EPOCH};

use anyhow::{anyhow, bail, Result};

use super::Rgb;
use crate::hid::{transfer, Link, NoAnswer};

pub const VID: u16 = 0x3554;
pub const PIDS: [u16; 6] = [0xf54b, 0xf575, 0xf548, 0xf577, 0xf576, 0xf549];
pub const USAGE_PAGE: u16 = 0xff02;
const RID: u8 = 8;

const CMD_INFO: u8 = 0x01;
const CMD_ONLINE: u8 = 0x03;
const CMD_BATTERY: u8 = 0x04;
const CMD_WRITE: u8 = 0x07;
const CMD_READ: u8 = 0x08;
const CMD_MOUSE_FW: u8 = 0x12;
const CMD_DONGLE_FW: u8 = 0x1d;

/// Flash addresses.
pub mod addr {
    pub const REPORT_RATE: u16 = 0;
    pub const STAGE_COUNT: u16 = 2;
    pub const CURRENT_STAGE: u16 = 4;
    pub const LOD: u16 = 10;
    pub const DPI: u16 = 12;
    pub const DPI_COLOR: u16 = 44;
    pub const BUTTONS: u16 = 96;
    pub const LIGHT: u16 = 160;
    pub const LIGHT_STATE: u16 = 167;
    pub const DEBOUNCE: u16 = 169;
    pub const MOTION_SYNC: u16 = 171;
    pub const SLEEP: u16 = 173;
    pub const ANGLE_SNAP: u16 = 175;
    pub const RIPPLE: u16 = 177;
    pub const LIGHT_OFF_MOVING: u16 = 179;
    pub const SENSOR_MODE: u16 = 185;
}

pub const MAX_STAGES: usize = 8;
pub const BUTTON_SLOTS: usize = 6;

/// Only these flash ranges are ever written. Everything else is refused.
const WRITABLE: [(u16, u16); 15] = [
    (addr::REPORT_RATE, 2),
    (addr::STAGE_COUNT, 2),
    (addr::CURRENT_STAGE, 2),
    (addr::LOD, 2),
    (addr::DPI, 4 * MAX_STAGES as u16),
    (addr::DPI_COLOR, 4 * MAX_STAGES as u16),
    (addr::BUTTONS, 4 * BUTTON_SLOTS as u16),
    (addr::LIGHT, 9),
    (addr::DEBOUNCE, 2),
    (addr::MOTION_SYNC, 2),
    (addr::SLEEP, 2),
    (addr::ANGLE_SNAP, 2),
    (addr::RIPPLE, 2),
    (addr::LIGHT_OFF_MOVING, 2),
    (addr::SENSOR_MODE, 2),
];

#[derive(Clone, Copy, Debug)]
pub struct Sensor {
    pub name: &'static str,
    pub max_dpi: u32,
    /// Highest DPI reachable without the ×2 range flag.
    pub base_max: u32,
    pub lod: &'static [(u8, &'static str)],
}

const PAW3395: Sensor = Sensor { name: "PAW3395", max_dpi: 26000, base_max: 26000, lod: &[(1, "1 mm"), (2, "2 mm")] };
const PAW3950: Sensor = Sensor { name: "PAW3950", max_dpi: 32000, base_max: 30000, lod: &[(3, "0.7 mm"), (1, "1 mm"), (2, "2 mm")] };

pub const REPORT_RATES: [u32; 7] = [125, 250, 500, 1000, 2000, 4000, 8000];
/// Sleep timeouts in units of 10 s.
pub const SLEEP_TIMES: [u8; 8] = [1, 3, 6, 12, 18, 30, 60, 90];

/// Which light controls apply per mode: (colour, brightness, speed).
pub fn light_controls(mode: u8) -> (bool, bool, bool) {
    match mode {
        1 | 4 | 5 => (false, true, true),
        2 => (true, true, true),
        3 => (true, true, false),
        6 => (false, true, false),
        _ => (false, false, false),
    }
}

pub const BUTTON_DISABLED: u8 = 0;
pub const BUTTON_MOUSE: u8 = 1;
pub const BUTTON_DPI: u8 = 2;

#[derive(Clone, Copy, Debug)]
pub struct Identity {
    pub cid: u8,
    pub mid: u8,
    pub wired: bool,
    pub max_rate: u32,
    pub sensor: Sensor,
    /// Product name from the model id, if known.
    pub model: Option<&'static str>,
}

/// GravaStar model ids (customer id 18), as named by the official driver ("GS X PRO", …).
pub fn model_name(mid: u8) -> Option<&'static str> {
    match mid {
        1 => Some("Mercury M1"),
        2 => Some("Mercury M2"),
        3 => Some("Mercury X Pro"),
        4 => Some("Mercury X"),
        5 => Some("Mercury X Air"),
        _ => None,
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct Battery {
    pub level: u8,
    pub charging: bool,
    pub millivolts: u16,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Light {
    pub on: bool,
    pub mode: u8,
    pub color: Rgb,
    pub speed: u8,
    pub brightness: u8,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Button {
    pub kind: u8,
    pub param: u16,
}

#[derive(Clone, Debug, Default)]
pub struct MouseConfig {
    pub report_rate: u32,
    pub stage_count: usize,
    pub current_stage: usize,
    pub dpi: [u32; MAX_STAGES],
    pub dpi_colors: [Rgb; MAX_STAGES],
    pub lod: u8,
    pub light: Light,
    pub debounce: u8,
    pub motion_sync: bool,
    pub sleep: u8,
    pub angle_snap: bool,
    pub ripple: bool,
    pub light_off_moving: bool,
    pub sensor_mode: u8,
    pub buttons: [Button; BUTTON_SLOTS],
}

const VOLTAGE_TABLE: [u16; 21] =
    [3050, 3420, 3480, 3540, 3600, 3660, 3720, 3760, 3800, 3840, 3880, 3920, 3940, 3960, 3980, 4000, 4020, 4040, 4060, 4080, 4110];

/// Battery percentage from cell voltage, as the official driver computes it.
fn voltage_to_percent(mv: u16, charging: bool) -> u8 {
    if mv >= VOLTAGE_TABLE[20] {
        return if charging { 99 } else { 100 };
    }
    if mv <= VOLTAGE_TABLE[0] {
        return 0;
    }
    let i = VOLTAGE_TABLE.iter().position(|&v| v > mv).unwrap_or(20);
    let (lo, hi) = (VOLTAGE_TABLE[i - 1] as f32, VOLTAGE_TABLE[i] as f32);
    ((mv as f32 - lo) / ((hi - lo) / 5.0) + (i as f32 - 1.0) * 5.0).round() as u8
}

pub fn packet(cmd: u8, address: u16, len: u8, data: &[u8]) -> [u8; 16] {
    let mut p = [0u8; 16];
    p[0] = cmd;
    p[2..4].copy_from_slice(&address.to_be_bytes());
    p[4] = len;
    let n = data.len().min(10);
    p[5..5 + n].copy_from_slice(&data[..n]);
    let sum = p[..15].iter().fold(0u8, |a, &b| a.wrapping_add(b));
    p[15] = 0x55u8.wrapping_sub(RID).wrapping_sub(sum);
    p
}

fn pair(v: u8) -> [u8; 2] {
    [v, 0x55u8.wrapping_sub(v)]
}

fn record(bytes: &[u8]) -> Vec<u8> {
    let sum = bytes.iter().fold(0u8, |a, &b| a.wrapping_add(b));
    let mut out = bytes.to_vec();
    out.push(0x55u8.wrapping_sub(sum));
    out
}

pub struct CompxMouse {
    link: Box<dyn Link>,
    pub identity: Option<Identity>,
}

impl CompxMouse {
    pub fn new(link: Box<dyn Link>) -> Self {
        Self { link, identity: None }
    }

    pub fn matches(vid: u16, pid: u16, usage_page: u16) -> bool {
        vid == VID && PIDS.contains(&pid) && usage_page == USAGE_PAGE
    }

    fn request(&mut self, pkt: [u8; 16]) -> Result<[u8; 16]> {
        for _ in 0..5 {
            let answer = transfer(self.link.as_mut(), RID, &pkt, Duration::from_millis(250), |r| {
                r.len() >= 16 && r[0] == pkt[0] && (pkt[0] != CMD_READ && pkt[0] != CMD_WRITE || (r[2] == pkt[2] && r[3] == pkt[3]))
            })?;
            if let Some(r) = answer {
                if r[1] == 1 {
                    bail!("command 0x{:02x} rejected by the mouse", pkt[0]);
                }
                let mut out = [0u8; 16];
                out.copy_from_slice(&r[..16]);
                return Ok(out);
            }
        }
        Err(NoAnswer(format!("mouse command 0x{:02x}", pkt[0])).into())
    }

    // ── Status ─────────────────────────────────────────────────────────

    pub fn identify(&mut self) -> Result<Identity> {
        let seed = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.subsec_nanos()).unwrap_or(0x5eed);
        let challenge = seed.to_le_bytes();
        let d = self.request(packet(CMD_INFO, 0, 8, &[challenge[0], challenge[1], challenge[2], challenge[3], 0, 0, 0, 0]))?;
        if d[9] == 0 || d[10] == 0 {
            return Err(anyhow!("mouse offline"));
        }
        let (wired, max_rate) = match d[11] {
            1 => (false, 4000),
            4 => (false, 2000),
            5 => (false, 8000),
            2 => (true, 1000),
            3 => (true, 8000),
            _ => (false, 1000),
        };
        let sensor = if (3..=5).contains(&d[10]) { PAW3950 } else { PAW3395 };
        let model = if d[9] == 18 { model_name(d[10]) } else { None };
        let id = Identity { cid: d[9], mid: d[10], wired, max_rate, sensor, model };
        self.identity = Some(id);
        Ok(id)
    }

    pub fn online(&mut self) -> Result<bool> {
        Ok(self.request(packet(CMD_ONLINE, 0, 0, &[]))?[5] == 1)
    }

    pub fn battery(&mut self) -> Result<Battery> {
        let d = self.request(packet(CMD_BATTERY, 0, 0, &[]))?;
        let charging = d[6] == 1;
        let millivolts = u16::from_be_bytes([d[7], d[8]]);
        let level = if d[9] == 1 {
            d[10]
        } else if millivolts > 0 {
            voltage_to_percent(millivolts, charging)
        } else {
            d[5]
        };
        Ok(Battery { level: level.min(100), charging, millivolts })
    }

    pub fn firmware(&mut self) -> Result<(String, Option<String>)> {
        let fmt = |d: [u8; 16]| format!("v{}.{:02x}", d[5], d[6]);
        let mouse = fmt(self.request(packet(CMD_MOUSE_FW, 0, 0, &[]))?);
        let wired = self.identity.map(|i| i.wired).unwrap_or(false);
        // Older receivers don't answer the version query.
        let dongle = if wired { None } else { self.request(packet(CMD_DONGLE_FW, 0, 0, &[])).ok().map(fmt) };
        Ok((mouse, dongle))
    }

    // ── Flash access ───────────────────────────────────────────────────

    pub fn read_flash(&mut self, address: u16, len: usize) -> Result<Vec<u8>> {
        let mut out = Vec::with_capacity(len);
        for off in (0..len).step_by(10) {
            let n = (len - off).min(10);
            let d = self.request(packet(CMD_READ, address + off as u16, n as u8, &[]))?;
            out.extend_from_slice(&d[5..5 + n]);
        }
        Ok(out)
    }

    fn write_flash(&mut self, address: u16, bytes: &[u8]) -> Result<()> {
        let end = address + bytes.len() as u16;
        if !WRITABLE.iter().any(|&(start, size)| address >= start && end <= start + size) {
            bail!("refusing to write flash {address}+{}", bytes.len());
        }
        if !self.online()? {
            bail!("mouse offline");
        }
        for (i, chunk) in bytes.chunks(10).enumerate() {
            self.request(packet(CMD_WRITE, address + (i * 10) as u16, chunk.len() as u8, chunk))?;
        }
        Ok(())
    }

    fn write_value(&mut self, address: u16, v: u8) -> Result<()> {
        self.write_flash(address, &pair(v))
    }

    // ── Config ─────────────────────────────────────────────────────────

    pub fn read_config(&mut self) -> Result<MouseConfig> {
        let f = self.read_flash(0, 190)?;
        let rgb = |a: usize| Rgb { r: f[a], g: f[a + 1], b: f[a + 2] };
        let light = addr::LIGHT as usize;
        Ok(MouseConfig {
            report_rate: if f[0] >= 16 { f[0] as u32 / 16 * 2000 } else { 1000 / (f[0].max(1) as u32) },
            stage_count: (f[addr::STAGE_COUNT as usize] as usize).clamp(1, MAX_STAGES),
            current_stage: f[addr::CURRENT_STAGE as usize] as usize,
            dpi: std::array::from_fn(|i| decode_dpi(&f[addr::DPI as usize + 4 * i..])),
            dpi_colors: std::array::from_fn(|i| rgb(addr::DPI_COLOR as usize + 4 * i)),
            lod: f[addr::LOD as usize],
            light: Light {
                on: f[addr::LIGHT_STATE as usize] == 1,
                mode: f[light],
                color: rgb(light + 1),
                speed: f[light + 4].min(9),
                brightness: f[light + 5].min(9),
            },
            debounce: f[addr::DEBOUNCE as usize],
            motion_sync: f[addr::MOTION_SYNC as usize] == 1,
            sleep: f[addr::SLEEP as usize],
            angle_snap: f[addr::ANGLE_SNAP as usize] == 1,
            ripple: f[addr::RIPPLE as usize] == 1,
            light_off_moving: f[addr::LIGHT_OFF_MOVING as usize] == 1,
            sensor_mode: f[addr::SENSOR_MODE as usize],
            buttons: std::array::from_fn(|i| {
                let a = addr::BUTTONS as usize + 4 * i;
                Button { kind: f[a], param: u16::from_be_bytes([f[a + 1], f[a + 2]]) }
            }),
        })
    }

    /// The DPI button on the mouse changes this behind our back.
    pub fn current_stage(&mut self) -> Result<usize> {
        Ok(self.read_flash(addr::CURRENT_STAGE, 1)?[0] as usize)
    }

    fn sensor(&self) -> Sensor {
        self.identity.map(|i| i.sensor).unwrap_or(PAW3395)
    }

    /// Round a requested DPI to what the sensor can store.
    pub fn clamp_dpi(&self, dpi: u32) -> u32 {
        let s = self.sensor();
        let step = if dpi > s.base_max { 100 } else { 50 };
        ((dpi as f32 / step as f32).round() as u32 * step).clamp(50, s.max_dpi)
    }

    fn encode_dpi(&self, dpi: u32) -> Vec<u8> {
        let high = dpi > self.sensor().base_max;
        let raw = ((dpi / if high { 100 } else { 50 }).saturating_sub(1)).min(1023) as u16;
        let lo = (raw & 0xff) as u8;
        let hi = (raw >> 8) as u8 & 3;
        let flags = high as u8;
        record(&[lo, lo, (hi << 2) | (hi << 6) | flags | (flags << 4)])
    }

    pub fn set_report_rate(&mut self, hz: u32) -> Result<()> {
        let v = if hz <= 1000 { 1000 / hz } else { hz / 2000 * 16 };
        self.write_value(addr::REPORT_RATE, v as u8)
    }

    pub fn set_dpi(&mut self, stage: usize, dpi: u32) -> Result<()> {
        let rec = self.encode_dpi(self.clamp_dpi(dpi));
        self.write_flash(addr::DPI + 4 * stage as u16, &rec)
    }

    pub fn set_dpi_color(&mut self, stage: usize, c: Rgb) -> Result<()> {
        self.write_flash(addr::DPI_COLOR + 4 * stage as u16, &record(&[c.r, c.g, c.b]))
    }

    pub fn set_stage_count(&mut self, n: usize) -> Result<()> {
        self.write_value(addr::STAGE_COUNT, n.clamp(1, MAX_STAGES) as u8)
    }

    pub fn set_current_stage(&mut self, i: usize) -> Result<()> {
        self.write_value(addr::CURRENT_STAGE, i as u8)
    }

    pub fn set_lod(&mut self, v: u8) -> Result<()> {
        self.write_value(addr::LOD, v)
    }

    pub fn set_light(&mut self, l: &Light) -> Result<()> {
        if !l.on || l.mode == 0 {
            return self.write_value(addr::LIGHT_STATE, 0);
        }
        let mut bytes = record(&[l.mode, l.color.r, l.color.g, l.color.b, l.speed, l.brightness]);
        bytes.extend_from_slice(&pair(1));
        self.write_flash(addr::LIGHT, &bytes)
    }

    pub fn set_debounce(&mut self, ms: u8) -> Result<()> {
        self.write_value(addr::DEBOUNCE, ms.min(15))
    }

    pub fn set_flag(&mut self, address: u16, on: bool) -> Result<()> {
        self.write_value(address, on as u8)
    }

    pub fn set_sleep(&mut self, v: u8) -> Result<()> {
        self.write_value(addr::SLEEP, v)
    }

    pub fn set_sensor_mode(&mut self, v: u8) -> Result<()> {
        self.write_value(addr::SENSOR_MODE, v)
    }

    pub fn set_button(&mut self, slot: usize, b: Button) -> Result<()> {
        if slot >= BUTTON_SLOTS {
            bail!("invalid button slot {slot}");
        }
        let [hi, lo] = b.param.to_be_bytes();
        self.write_flash(addr::BUTTONS + 4 * slot as u16, &record(&[b.kind, hi, lo]))
    }
}

fn decode_dpi(rec: &[u8]) -> u32 {
    let raw = rec[0] as u32 | (((rec[2] >> 2) & 3) as u32) << 8;
    let mut dpi = (raw + 1) * 50;
    if rec[2] & 1 != 0 {
        dpi *= 2;
    }
    if rec[2] & 2 != 0 {
        dpi *= 2;
    }
    dpi
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn packets_match_captured_bytes() {
        // Battery request and "1000 Hz" write as seen on the wire.
        assert_eq!(packet(CMD_BATTERY, 0, 0, &[])[15], 0x49);
        let p = packet(CMD_WRITE, 0, 2, &pair(1));
        assert_eq!(p[..7], [0x07, 0, 0, 0, 2, 0x01, 0x54]);
        assert_eq!(p[15], 0xef);
    }

    #[test]
    fn dpi_roundtrip() {
        assert_eq!(decode_dpi(&[0x1f, 0x1f, 0x00, 0x17]), 1600);
        let m = CompxMouse { link: Box::new(crate::demo::NullLink), identity: None };
        assert_eq!(m.encode_dpi(1600), vec![0x1f, 0x1f, 0x00, 0x17]);
    }

    #[test]
    fn battery_curve() {
        assert_eq!(voltage_to_percent(3900, false), 53);
        assert_eq!(voltage_to_percent(4200, true), 99);
    }
}
