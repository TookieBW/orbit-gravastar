//! Byte-level device emulation for `ORBIT_DEMO=1`: lets the UI run without
//! hardware while exercising the real protocol code. Seed data was captured
//! from a Mercury V60 Pro ISO (firmware 0.4.1.0) and a PAW3950 mouse (v2.17).

use std::collections::{HashMap, VecDeque};
use std::time::{Duration, Instant};

use anyhow::Result;

use crate::hid::Link;

/// Answers a request payload with a reply payload (or nothing).
trait Emulator {
    fn handle(&mut self, report_id: u8, req: &[u8]) -> Option<Vec<u8>>;
}

pub struct DemoLink {
    emu: Box<dyn Emulator>,
    pending: VecDeque<Vec<u8>>,
}

impl Link for DemoLink {
    fn write(&mut self, report_id: u8, data: &[u8]) -> Result<()> {
        if let Some(r) = self.emu.handle(report_id, data) {
            self.pending.push_back(r);
        }
        Ok(())
    }

    fn read(&mut self, timeout: Duration) -> Result<Option<Vec<u8>>> {
        if !timeout.is_zero() {
            std::thread::sleep(Duration::from_millis(1));
        }
        Ok(self.pending.pop_front())
    }
}

/// A link that never answers (unit tests).
#[cfg(test)]
pub struct NullLink;

#[cfg(test)]
impl Link for NullLink {
    fn write(&mut self, _: u8, _: &[u8]) -> Result<()> {
        Ok(())
    }
    fn read(&mut self, _: Duration) -> Result<Option<Vec<u8>>> {
        Ok(None)
    }
}

pub fn keyboard() -> DemoLink {
    DemoLink { emu: Box::new(DemoKeyboard::new()), pending: VecDeque::new() }
}

pub fn mouse() -> DemoLink {
    DemoLink { emu: Box::new(DemoMouse::new()), pending: VecDeque::new() }
}

fn hex(s: &str) -> Vec<u8> {
    s.split_whitespace().map(|h| u8::from_str_radix(h, 16).unwrap()).collect()
}

fn pad(mut v: Vec<u8>, len: usize) -> Vec<u8> {
    v.resize(len, 0);
    v
}

// ── Keyboard ───────────────────────────────────────────────────────────

struct DemoKeyboard {
    started: Instant,
    rate: u8,
    profile: u8,
    system: u8,
    sleep: u16,
    perf: HashMap<(u8, u8), Vec<u8>>,
    light: HashMap<u8, Vec<u8>>,
    palette: Vec<u8>,
}

const STYLE: [&str; 5] = [
    "01 20 41 20 81 20 c1 20 01 21 41 21 81 21 c1 21 01 22 41 22 81 22 c1 22 01 23 00 00 45 23",
    "03 40 61 40 a1 40 e1 40 21 41 61 41 a1 41 e1 41 21 42 61 42 a1 42 e1 42 21 43 00 00 6b 43",
    "04 60 71 60 b1 60 f1 60 31 61 71 61 b1 61 f1 61 31 62 71 62 b1 62 f1 62 00 00 31 63",
    "01 80 41 80 81 80 c1 80 01 81 41 81 81 81 c1 81 01 82 41 82 81 82 c1 82 01 83 41 83 81 83",
    "02 a0 52 a0 a2 a0 00 00 00 00 00 00 f9 a0 00 00 00 00 81 a2 c1 a2 00 00 01 a3 41 a3 81 a3",
];

const KEYMAP: [&[u16]; 5] = [
    &[0x29, 0x1e, 0x1f, 0x20, 0x21, 0x22, 0x23, 0x24, 0x25, 0x26, 0x27, 0x2d, 0x2e, 0, 0x2a],
    &[0x2b, 0x14, 0x1a, 0x08, 0x15, 0x17, 0x1c, 0x18, 0x0c, 0x12, 0x13, 0x2f, 0x30, 0, 0x28],
    &[0x39, 0x04, 0x16, 0x07, 0x09, 0x0a, 0x0b, 0x0d, 0x0e, 0x0f, 0x33, 0x34, 0, 0x31],
    &[0xe1, 0x64, 0x1d, 0x1b, 0x06, 0x19, 0x05, 0x11, 0x10, 0x36, 0x37, 0x38, 0xe5, 0x52, 0x4c],
    &[0xe0, 0xe3, 0xe2, 0, 0, 0, 0x2c, 0, 0, 0xe6, 0xf101, 0, 0x50, 0x51, 0x4f],
];

const DEFAULT_PERF: &str = "01 e8 03 e8 03 e8 03 2c 01 2c 01 00 00 00 00 00 00 b0 33 38 0e 9a 49";

impl DemoKeyboard {
    fn new() -> Self {
        let mut perf = HashMap::new();
        // Like the real board: Win at 3.5 mm fixed, space and R at 1.0 mm fixed.
        let fixed = |mm: u16| {
            let [lo, hi] = mm.to_le_bytes();
            let mut v = hex(DEFAULT_PERF);
            v[0] = 0;
            v[1] = lo;
            v[2] = hi;
            v
        };
        // A typical gaming setup on WASD: Rapid Trigger, 0.5 mm first actuation, 0.1 mm sensitivity.
        let gaming = {
            let mut v = hex(DEFAULT_PERF);
            v[5..7].copy_from_slice(&500u16.to_le_bytes());
            v[7..9].copy_from_slice(&100u16.to_le_bytes());
            v[9..11].copy_from_slice(&100u16.to_le_bytes());
            v
        };
        for key in [(2, 2), (3, 1), (3, 2), (3, 3)] {
            perf.insert(key, gaming.clone());
        }
        perf.insert((5, 1), fixed(3500));
        perf.insert((5, 6), fixed(1000));
        perf.insert((2, 4), fixed(1000));
        Self {
            started: Instant::now(),
            rate: 2,
            profile: 0,
            system: 0,
            sleep: 0,
            perf,
            light: HashMap::from([(0, vec![1, 0, 99, 20, 0, 6]), (1, vec![1, 2, 80, 10, 0, 2]), (2, vec![1, 0, 80, 10, 0, 4])]),
            palette: hex("00 00 00 00 00 00 ff 00 00 7f ff 00 00 ff 00 00 ff ff 00 00 ff 00 00 00 ce 64 ed 00 ff ff ff 00"),
        }
    }
}

impl Emulator for DemoKeyboard {
    fn handle(&mut self, _report_id: u8, r: &[u8]) -> Option<Vec<u8>> {
        let (cat, cmd) = (r[0], r[1]);
        let out: Vec<u8> = match (cat, cmd) {
            (1, 1) => vec![1, 1, 1, 2, 1, 0],
            (1, 2) => {
                let mut v = hex("01 02 01 00 00 2c 00 0e 00 04 01 00 01 00 01 00 00 36 08 12 50 43 37 54 38 34 19 38 4b");
                v.extend_from_slice(b"2026010415:35:44");
                v
            }
            (1, 3) => vec![1, 3, 2, 1, 1, 0],
            (2, 5) => match r[2] {
                0 => vec![2, 5, 0, 6, 0, 1, 2, 3, 4, 5],
                2 => {
                    self.rate = r[3];
                    vec![2, 5, 2, self.rate]
                }
                _ => vec![2, 5, r[2], self.rate],
            },
            (2, 3) => match r[2] {
                0 => vec![2, 3, 0, 4],
                2 => {
                    self.profile = r[3];
                    vec![2, 3, 2, self.profile]
                }
                _ => vec![2, 3, r[2], self.profile],
            },
            (2, 4) => {
                if r[2] == 2 {
                    self.system = r[3];
                }
                vec![2, 4, r[2], self.system]
            }
            (2, 12) => vec![2, 12, 0, 5, 5],
            (2, 8) => vec![2, 8, 0, 3, 0, 17, 6, 15, 1, 9, 1, 42, 2, 3, 1, 3],
            (2, 13) => {
                if r[2] == 0 {
                    self.sleep = u16::from_le_bytes([r[3], r[4]]);
                }
                let [lo, hi] = self.sleep.to_le_bytes();
                vec![2, 13, r[2], lo, hi]
            }
            (3, 5) => {
                if r[2] == 0 {
                    vec![3, 5, 0]
                } else if let Some(s) = STYLE.get(r[2] as usize - 1) {
                    [vec![3, 5, r[2]], hex(s)].concat()
                } else {
                    vec![3, 5, 0xff]
                }
            }
            (3, 1) => {
                let mut v = vec![3, 1, r[2], r[3]];
                if r[2] == 0 && (1..=5).contains(&r[3]) {
                    for c in KEYMAP[r[3] as usize - 1] {
                        v.extend_from_slice(&c.to_le_bytes());
                    }
                }
                v
            }
            (4, 1) => [vec![4, 1, r[2], r[3]], self.perf.get(&(r[2], r[3])).cloned().unwrap_or_else(|| hex(DEFAULT_PERF))].concat(),
            // Live travel: W (row 2, col 2) is pressed down and released every few seconds.
            (4, 3) if r[2] == 1 || r[2] == 3 => {
                let t = self.started.elapsed().as_secs_f32();
                let depth = (t * 1.3).sin().max(0.0).powf(0.7) * 3.64;
                let mut v = vec![4, 3, r[2], r[3]];
                for col in 0..21u8 {
                    let value = match (r[3], col, r[2]) {
                        (2, 2, 1) => (depth * 1000.0) as u16,
                        (2, 2, _) => (depth > 1.0) as u16 * 2,
                        _ => 0,
                    };
                    v.extend_from_slice(&value.to_le_bytes());
                }
                v
            }
            (4, 2) => {
                self.perf.insert((r[2], r[3]), r[4..27].to_vec());
                vec![4, 2, r[2], r[3]]
            }
            (5, 1) if r[3] == 0 => [vec![5, 1, r[2], 0], self.light.get(&r[2]).cloned().unwrap_or_else(|| vec![0; 6])].concat(),
            (5, 1) if r[3] == 1 => [vec![5, 1, r[2], 1], self.palette.clone()].concat(),
            (5, 2) => {
                if r[3] == 0 {
                    self.light.insert(r[2], r[4..10].to_vec());
                } else if r[3] == 1 {
                    self.palette = r[4..36].to_vec();
                }
                vec![5, 2, r[2], r[3]]
            }
            (2, _) => vec![2, cmd, r[2], 0],
            _ => vec![cat, 0xff],
        };
        Some(pad(out, 64))
    }
}

// ── Mouse ──────────────────────────────────────────────────────────────

struct DemoMouse {
    flash: Vec<u8>,
    battery: f32,
}

impl DemoMouse {
    fn new() -> Self {
        // Flash 0..189 as read from the real mouse (6 stages enabled for the demo).
        let seed = "01 54 06 4F 00 55 00 55 00 55 01 54 1F 1F 00 17 17 17 00 27 0F 0F 00 37 2F 2F 00 F7 3F 3F 00 D7 \
            7F 7F 00 57 7F 7F 00 57 7F 7F 00 57 00 FF 00 56 00 00 FF 56 FF 00 00 56 FF FF 00 57 00 FF FF 57 \
            FF 00 FF 57 FF 00 FF 57 FF 00 FF 57 02 53 80 D5 03 52 00 55 FF 00 FF 57 00 55 80 D5 03 52 00 55 \
            01 01 00 53 01 02 00 52 01 04 00 50 01 08 00 4C 01 10 00 44 02 01 00 52 02 02 00 51 02 03 00 50 \
            07 00 00 4E 08 04 00 49 04 0A 03 44 00 00 00 55 00 00 00 55 00 00 00 55 00 00 00 55 00 00 00 55 \
            04 FF 00 FF 07 04 48 01 54 08 4D 01 54 01 54 00 55 00 55 01 54 00 55 06 4F 00 55 0A 4B";
        let mut flash = vec![0xff; 7000];
        for (i, b) in hex(seed).into_iter().enumerate() {
            flash[i] = b;
        }
        Self { flash, battery: 72.0 }
    }

    fn reply(r: &[u8], data: &[u8], len: u8) -> Vec<u8> {
        let mut out = vec![r[0], 0, r[2], r[3], len];
        out.extend_from_slice(data);
        out.resize(15, 0);
        let sum = out.iter().fold(0u8, |a, &b| a.wrapping_add(b));
        out.push(0x4du8.wrapping_sub(sum));
        out
    }
}

impl Emulator for DemoMouse {
    fn handle(&mut self, _report_id: u8, r: &[u8]) -> Option<Vec<u8>> {
        let address = u16::from_be_bytes([r[2], r[3]]) as usize;
        let n = r[4] as usize;
        Some(match r[0] {
            0x01 => Self::reply(r, &[0x52, 0xf3, 0x93, 0x47, 18, 3, 5], r[4]),
            0x03 => Self::reply(r, &[1, 0xd8, 0x47, 0xea], 1),
            0x04 => {
                self.battery = (self.battery - 0.05).max(5.0);
                Self::reply(r, &[self.battery as u8, 0, 0x0f, 0x3c], 2)
            }
            0x08 => Self::reply(r, &self.flash[address..address + n].to_vec(), r[4]),
            0x07 => {
                self.flash[address..address + n].copy_from_slice(&r[5..5 + n]);
                Self::reply(r, &r[5..5 + n].to_vec(), r[4])
            }
            0x0e => Self::reply(r, &[0], 1),
            0x12 => Self::reply(r, &[2, 0x17], 2),
            0x1d => Self::reply(r, &[1, 0x05], 2),
            _ => return None,
        })
    }
}
