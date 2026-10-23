//! GravaStar Mercury HE keyboards ("v2" web-driver protocol).
//!
//! Every request is a 64-byte output report with report id 0:
//! `[category, command, args…]`. The keyboard answers with a 64-byte input
//! report that echoes category and command. Multi-byte values are little
//! endian; distances are transmitted in micrometres. There is no commit
//! command: the firmware persists every write by itself.
//! See `docs/protocol-keyboard.md`.

use std::thread::sleep;
use std::time::Duration;

use anyhow::Result;

use super::Rgb;
use crate::hid::{transfer, Link, NoAnswer};

/// Vendor ids the official driver routes to this protocol.
pub const VIDS: [u16; 2] = [0x1ca6, 0x31e3];
pub const USAGE_PAGE: u16 = 0xffb0;

/// Product ids the official web driver routes to the v2 (HE) protocol.
pub const PIDS: [u16; 23] = [
    11272, 11273, 11282, 11275, 11276, 11288, 11289, 11296, 11297, 11290, 11291, 11292, 11293, 11274, 11278, 11277, 4896,
    11283, 11284, 11305, 11306, 11300, 11298,
];

const DEVICE: u8 = 1;
const GLOBAL: u8 = 2;
const LAYOUT: u8 = 3;
const PERFORMANCE: u8 = 4;
const LIGHTING: u8 = 5;

const RESET_FACTORY: u8 = 1;
const CONFIG: u8 = 3;
const SYSTEM: u8 = 4;
const REPORT_RATE: u8 = 5;
const CALIBRATION: u8 = 6;
const LIGHTING_AREAS: u8 = 8;
const RT_PRECISION: u8 = 12;
const LIGHTING_SLEEP: u8 = 13;

const READ_LIST: u8 = 0;
const READ: u8 = 1;
const WRITE: u8 = 2;

pub const REPORT_RATES: [&str; 7] = ["8K", "4K", "2K", "1K", "500", "250", "125"];

#[derive(Clone, Debug, Default)]
pub struct KeyboardInfo {
    pub firmware: String,
    pub pcb: String,
    pub build_time: String,
    pub protocol: String,
    pub board_id: u32,
}

/// Actuation settings of one key.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct KeyPerf {
    /// 0 = fixed actuation, 1 = rapid trigger.
    pub mode: u8,
    pub normal_press: f32,
    pub normal_release: f32,
    pub rt_first_touch: f32,
    pub rt_press: f32,
    pub rt_release: f32,
    pub press_deadzone: f32,
    pub release_deadzone: f32,
    // Switch characteristics — echoed back unchanged on write.
    pub axis: u8,
    pub calibrate: u8,
    pub axis_v2_id: u16,
    pub axis_range_max: u16,
    pub axis_coefficient: u16,
}

pub const MODE_FIXED: u8 = 0;
pub const MODE_RAPID: u8 = 1;

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct LightingBase {
    pub on: bool,
    pub mode: u8,
    pub brightness: u8,
    pub speed: u8,
    pub direction: u8,
    /// Index into the 8-entry palette.
    pub color_index: u8,
}

#[derive(Clone, Copy, Debug)]
pub struct LightingArea {
    pub index: u8,
    /// Number of effects the zone supports.
    pub count: u8,
}

/// Physical key geometry as reported by the keyboard.
#[derive(Clone, Copy, Debug)]
pub struct KeySlot {
    pub row: u8,
    pub col: u8,
    /// Position in key units.
    pub x: f32,
    pub y: f32,
    /// Width code, see `models::key_size`.
    pub size: u8,
}

pub struct HeKeyboard {
    link: Box<dyn Link>,
}

fn u16le(v: u16) -> [u8; 2] {
    v.to_le_bytes()
}

fn rd16(d: &[u8], i: usize) -> u16 {
    u16::from_le_bytes([d[i], d[i + 1]])
}

fn um(mm: f32) -> [u8; 2] {
    u16le((mm * 1000.0).round().clamp(0.0, 65535.0) as u16)
}

fn mm(d: &[u8], i: usize) -> f32 {
    rd16(d, i) as f32 / 1000.0
}

impl HeKeyboard {
    pub fn new(link: Box<dyn Link>) -> Self {
        Self { link }
    }

    pub fn matches(vid: u16, pid: u16, usage_page: u16) -> bool {
        VIDS.contains(&vid) && PIDS.contains(&pid) && usage_page == USAGE_PAGE
    }

    /// Sends a request and returns the answer whose first `match_len` bytes echo the request.
    fn request_matching(&mut self, payload: &[u8], timeout_ms: u64, match_len: usize) -> Result<[u8; 64]> {
        let mut pkt = [0u8; 64];
        let n = payload.len().min(64);
        pkt[..n].copy_from_slice(&payload[..n]);
        for _ in 0..3 {
            let answer = transfer(self.link.as_mut(), 0, &pkt, Duration::from_millis(timeout_ms), |r| {
                r.len() >= match_len && r[..match_len] == pkt[..match_len]
            })?;
            if let Some(r) = answer {
                let mut out = [0u8; 64];
                let m = r.len().min(64);
                out[..m].copy_from_slice(&r[..m]);
                return Ok(out);
            }
        }
        Err(NoAnswer(format!("{:02x} {:02x}", pkt[0], pkt[1])).into())
    }

    /// Request matched on category and command.
    fn request(&mut self, payload: &[u8], timeout_ms: u64) -> Result<[u8; 64]> {
        self.request_matching(payload, timeout_ms, 2)
    }

    fn req(&mut self, payload: &[u8]) -> Result<[u8; 64]> {
        self.request(payload, 600)
    }

    // ── Device ─────────────────────────────────────────────────────────

    pub fn info(&mut self) -> Result<KeyboardInfo> {
        let proto = self.req(&[DEVICE, 1])?;
        let d = self.req(&[DEVICE, 2])?;
        let ts: String = d[29..].iter().take_while(|&&b| b != 0).map(|&b| b as char).collect();
        let build_time = if ts.len() == 16 && ts.as_bytes()[10] == b':' {
            format!("{}-{}-{} {}", &ts[0..4], &ts[4..6], &ts[6..8], &ts[8..])
        } else {
            ts
        };
        let join = |b: &[u8], sep: &str| b.iter().map(|v| v.to_string()).collect::<Vec<_>>().join(sep);
        Ok(KeyboardInfo {
            firmware: join(&d[8..12], "."),
            pcb: join(&d[12..16], "-"),
            build_time,
            protocol: format!("{}.{}", proto[2], proto[3]),
            board_id: u32::from_be_bytes([d[4], d[5], d[6], d[7]]),
        })
    }

    // ── Layout ─────────────────────────────────────────────────────────

    /// Physical key geometry. Each matrix column of a row is a packed u16:
    /// y = bits 15-11, x = bits 10-4 (both in quarter keys), size = bits 3-0.
    pub fn key_slots(&mut self) -> Result<Vec<KeySlot>> {
        let mut slots = Vec::new();
        for row in 0..8u8 {
            let d = self.req(&[LAYOUT, 5, row])?;
            if d[2] == 0xff {
                break;
            }
            for col in 0..21u8 {
                let v = rd16(&d, 3 + 2 * col as usize);
                let size = (v & 15) as u8;
                if v == 0 || size == 0 {
                    continue;
                }
                slots.push(KeySlot {
                    row,
                    col,
                    y: ((v >> 11) & 31) as f32 / 4.0,
                    x: ((v >> 4) & 127) as f32 / 4.0,
                    size,
                });
            }
        }
        Ok(slots)
    }

    /// 16-bit keycodes of one matrix row (21 columns).
    pub fn key_codes(&mut self, layer: u8, row: u8) -> Result<[u16; 21]> {
        let d = self.req(&[LAYOUT, 1, layer, row])?;
        Ok(std::array::from_fn(|i| rd16(&d, 4 + 2 * i)))
    }

    // ── Global ─────────────────────────────────────────────────────────

    pub fn report_rates(&mut self) -> Result<Vec<u8>> {
        let d = self.req(&[GLOBAL, REPORT_RATE, READ_LIST])?;
        let n = (d[3] as usize).min(16);
        Ok(d[4..4 + n].to_vec())
    }

    pub fn report_rate(&mut self) -> Result<u8> {
        Ok(self.req(&[GLOBAL, REPORT_RATE, READ])?[3])
    }

    /// The keyboard restarts afterwards and may drop off the bus before answering.
    pub fn set_report_rate(&mut self, id: u8) -> Result<()> {
        match self.request(&[GLOBAL, REPORT_RATE, WRITE, id], 800) {
            Err(e) if e.is::<NoAnswer>() => Ok(()),
            other => other.map(|_| ()),
        }
    }

    pub fn profile_count(&mut self) -> Result<u8> {
        Ok(self.req(&[GLOBAL, CONFIG, READ_LIST])?[3])
    }

    pub fn profile(&mut self) -> Result<u8> {
        Ok(self.req(&[GLOBAL, CONFIG, READ])?[3])
    }

    pub fn set_profile(&mut self, index: u8) -> Result<()> {
        self.request(&[GLOBAL, CONFIG, WRITE, index], 1500)?;
        // The firmware reloads the profile; give it a moment before reading again.
        sleep(Duration::from_millis(1500));
        Ok(())
    }

    /// 0 = Windows, 1 = macOS.
    pub fn system(&mut self) -> Result<u8> {
        Ok(self.req(&[GLOBAL, SYSTEM, READ])?[3])
    }

    pub fn set_system(&mut self, value: u8) -> Result<()> {
        self.req(&[GLOBAL, SYSTEM, WRITE, value]).map(|_| ())
    }

    /// Finest rapid-trigger step in millimetres.
    pub fn rt_precision(&mut self) -> Result<f32> {
        Ok(self.req(&[GLOBAL, RT_PRECISION, 0])?[3] as f32 / 1000.0)
    }

    pub fn lighting_areas(&mut self) -> Result<Vec<LightingArea>> {
        let d = self.req(&[GLOBAL, LIGHTING_AREAS, 0])?;
        Ok((0..(d[3] as usize).min(14)).map(|i| LightingArea { index: d[4 + 4 * i], count: d[5 + 4 * i] }).collect())
    }

    /// Lighting sleep timeout in minutes (0 = never).
    pub fn lighting_sleep(&mut self) -> Result<u16> {
        Ok(rd16(&self.req(&[GLOBAL, LIGHTING_SLEEP, 1])?, 3))
    }

    pub fn set_lighting_sleep(&mut self, minutes: u16) -> Result<()> {
        let [lo, hi] = u16le(minutes);
        self.req(&[GLOBAL, LIGHTING_SLEEP, 0, lo, hi]).map(|_| ())
    }

    /// Wipes all profiles. The official driver waits 3 s before talking to the keyboard again.
    pub fn factory_reset(&mut self) -> Result<()> {
        self.request(&[GLOBAL, RESET_FACTORY, 0], 5000)?;
        sleep(Duration::from_secs(3));
        Ok(())
    }

    /// Start (true) or finish (false) travel calibration.
    pub fn calibration(&mut self, start: bool) -> Result<()> {
        self.request(&[GLOBAL, CALIBRATION, if start { 0 } else { 1 }], 2000).map(|_| ())
    }

    // ── Performance ────────────────────────────────────────────────────

    pub fn key_perf(&mut self, row: u8, col: u8) -> Result<KeyPerf> {
        let d = self.req(&[PERFORMANCE, 1, row, col])?;
        Ok(KeyPerf {
            mode: d[4],
            normal_press: mm(&d, 5),
            normal_release: mm(&d, 7),
            rt_first_touch: mm(&d, 9),
            rt_press: mm(&d, 11),
            rt_release: mm(&d, 13),
            press_deadzone: mm(&d, 15),
            release_deadzone: mm(&d, 17),
            axis: d[19],
            calibrate: d[20],
            axis_v2_id: rd16(&d, 21),
            axis_range_max: rd16(&d, 23),
            axis_coefficient: rd16(&d, 25),
        })
    }

    pub fn set_key_perf(&mut self, row: u8, col: u8, p: &KeyPerf) -> Result<()> {
        let mut pkt = vec![PERFORMANCE, 2, row, col, p.mode];
        for v in [p.normal_press, p.normal_release, p.rt_first_touch, p.rt_press, p.rt_release, p.press_deadzone, p.release_deadzone] {
            pkt.extend_from_slice(&um(v));
        }
        pkt.extend_from_slice(&[p.axis, p.calibrate]);
        for v in [p.axis_v2_id, p.axis_range_max, p.axis_coefficient] {
            pkt.extend_from_slice(&u16le(v));
        }
        self.req(&pkt).map(|_| ())
    }

    /// Live travel of every key in a matrix row (mm) and whether each key is
    /// currently actuated. Used for the travel display while keys are pressed.
    pub fn live_row(&mut self, row: u8) -> Result<[(f32, bool); 21]> {
        let travel = self.request_matching(&[PERFORMANCE, 3, 1, row], 300, 4)?;
        let status = self.request_matching(&[PERFORMANCE, 3, 3, row], 300, 4)?;
        Ok(std::array::from_fn(|i| (mm(&travel, 4 + 2 * i), rd16(&status, 4 + 2 * i) != 0)))
    }

    // ── Lighting ───────────────────────────────────────────────────────

    pub fn lighting(&mut self, area: u8) -> Result<LightingBase> {
        let d = self.req(&[LIGHTING, READ, area, 0])?;
        Ok(LightingBase { on: d[4] != 0, mode: d[5], brightness: d[6], speed: d[7], direction: d[8], color_index: d[9] })
    }

    pub fn set_lighting(&mut self, area: u8, l: &LightingBase) -> Result<()> {
        self.req(&[LIGHTING, WRITE, area, 0, l.on as u8, l.mode, l.brightness, l.speed, l.direction, l.color_index]).map(|_| ())
    }

    /// The 8 preset colours a lighting mode can pick from (wire order B, G, R, H).
    pub fn palette(&mut self, area: u8) -> Result<Vec<Rgb>> {
        let d = self.req(&[LIGHTING, READ, area, 1])?;
        Ok((0..8).map(|i| Rgb { b: d[4 + 4 * i], g: d[5 + 4 * i], r: d[6 + 4 * i] }).collect())
    }

    pub fn set_palette(&mut self, area: u8, colors: &[Rgb]) -> Result<()> {
        let mut pkt = vec![LIGHTING, WRITE, area, 1];
        for c in colors.iter().take(8) {
            pkt.extend_from_slice(&[c.b, c.g, c.r, 0]);
        }
        self.req(&pkt).map(|_| ())
    }
}
