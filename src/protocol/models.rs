//! Model data for GravaStar HE keyboards: key sizes, legends, lighting zones, limits.

use super::keyboard::KeySlot;
use crate::i18n::Lang;

/// Switch id of GravaStar's UFO magnetic switch, which allows 3.5 mm instead of 3.4 mm.
const UFO_SWITCH: u16 = 13232;

pub fn max_travel(axis_v2_id: u16) -> f32 {
    if axis_v2_id == UFO_SWITCH {
        3.5
    } else {
        3.4
    }
}

pub const DEADZONE_MAX: f32 = 1.0;

/// Lighting sleep options in minutes (0 = never), as offered by the official driver.
pub const LIGHT_SLEEP_MINUTES: [u16; 13] = [0, 1, 2, 3, 5, 10, 15, 20, 25, 30, 45, 60, 120];

// ── Layout ─────────────────────────────────────────────────────────────

#[derive(Clone, Debug)]
pub struct KeyDef {
    pub row: u8,
    pub col: u8,
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
    /// ISO enter: 1.5u wide on top, 1.25u below, two rows high.
    pub iso_enter: bool,
    pub label: String,
}

/// Width codes reported by GetKeyLayoutStyle (from the official driver).
fn key_size(code: u8) -> (f32, f32, bool) {
    match code {
        2 => (1.25, 1.0, false),
        3 => (1.5, 1.0, false),
        4 => (1.75, 1.0, false),
        5 => (2.0, 1.0, false),
        6 => (2.25, 1.0, false),
        7 => (2.5, 1.0, false),
        8 => (2.75, 1.0, false),
        9 => (6.25, 1.0, false),
        10 => (1.0, 2.0, false),
        11 => (1.5, 2.0, true),
        _ => (1.0, 1.0, false),
    }
}

/// Key legend for a HID-based keycode. German legends only apply to ISO boards.
pub fn key_label(code: u16, german: bool) -> String {
    let fixed = match code {
        0x28 => "Enter",
        0x29 => "Esc",
        0x2a if german => "Rück",
        0x2a => "Backspace",
        0x2b => "Tab",
        0x2c => "",
        0x39 => "Caps",
        0x46 => "PrtSc",
        0x47 => "ScrLk",
        0x48 => "Pause",
        0x49 => "Ins",
        0x4a => "Home",
        0x4b => "PgUp",
        0x4c if german => "Entf",
        0x4c => "Del",
        0x4d => "End",
        0x4e => "PgDn",
        0x4f => "→",
        0x50 => "←",
        0x51 => "↓",
        0x52 => "↑",
        0x65 => "Menu",
        0xe0 | 0xe4 if german => "Strg",
        0xe0 | 0xe4 => "Ctrl",
        0xe1 | 0xe5 => "Shift",
        0xe2 => "Alt",
        0xe6 if german => "AltGr",
        0xe6 => "Alt",
        0xe3 | 0xe7 => "Win",
        0xf101 => "Fn",
        _ => "",
    };
    if !fixed.is_empty() || code == 0x2c {
        return fixed.to_string();
    }
    let punct_us = [(0x2d, "-"), (0x2e, "="), (0x2f, "["), (0x30, "]"), (0x31, "\\"), (0x32, "#"), (0x33, ";"), (0x34, "'"), (0x35, "`"), (0x36, ","), (0x37, "."), (0x38, "/"), (0x64, "\\")];
    let punct_de = [(0x2d, "ß"), (0x2e, "´"), (0x2f, "Ü"), (0x30, "+"), (0x31, "#"), (0x32, "#"), (0x33, "Ö"), (0x34, "Ä"), (0x35, "^"), (0x36, ","), (0x37, "."), (0x38, "-"), (0x64, "<")];
    let table: &[(u16, &str)] = if german { &punct_de } else { &punct_us };
    if let Some((_, s)) = table.iter().find(|(c, _)| *c == code) {
        return s.to_string();
    }
    match code {
        0x1c if german => "Z".into(),
        0x1d if german => "Y".into(),
        0x04..=0x1d => ((b'A' + (code - 4) as u8) as char).to_string(),
        0x1e..=0x26 => (code - 0x1d).to_string(),
        0x27 => "0".into(),
        0x3a..=0x45 => format!("F{}", code - 0x39),
        0 => String::new(),
        _ => format!("{code:X}"),
    }
}

/// Combines the keyboard's geometry with its live keymap into drawable keys.
pub fn build_layout(slots: &[KeySlot], codes: &dyn Fn(u8, u8) -> u16, german: bool) -> Vec<KeyDef> {
    let top = slots.iter().map(|s| s.y).fold(f32::MAX, f32::min);
    slots
        .iter()
        .map(|s| {
            let (w, h, iso_enter) = key_size(s.size);
            KeyDef { row: s.row, col: s.col, x: s.x, y: s.y - top, w, h, iso_enter, label: key_label(codes(s.row, s.col), german) }
        })
        .collect()
}

// ── Lighting ───────────────────────────────────────────────────────────

#[derive(Clone, Copy)]
pub struct LightingMode {
    pub id: u8,
    pub en: &'static str,
    pub de: &'static str,
    /// Mode picks one of the 8 palette colours.
    pub palette: bool,
    pub preview: Preview,
}

impl LightingMode {
    pub fn name(&self, lang: Lang) -> &'static str {
        match lang {
            Lang::De => self.de,
            Lang::En => self.en,
        }
    }
}

/// How an effect card previews the effect.
#[derive(Clone, Copy, PartialEq)]
pub enum Preview {
    Solid,
    Pulse,
    Rainbow,
    Stripes,
    Dots,
    Sweep,
}

pub struct LightingZone {
    pub en: &'static str,
    pub de: &'static str,
    pub modes: Vec<LightingMode>,
    pub speed: bool,
    pub direction: bool,
    /// Palette index 0 means "multicolour" in this zone.
    pub multicolor_index0: bool,
}

const fn m(id: u8, en: &'static str, de: &'static str, palette: bool, preview: Preview) -> LightingMode {
    LightingMode { id, en, de, palette, preview }
}

const KEY_MODES: [LightingMode; 15] = [
    m(12, "Static", "Dauerlicht", true, Preview::Solid),
    m(0, "Drift", "Drift", true, Preview::Sweep),
    m(1, "Snow", "Schnee", true, Preview::Dots),
    m(2, "Blossom", "Blüte", true, Preview::Pulse),
    m(3, "Sine", "Sinus", true, Preview::Stripes),
    m(4, "Clouds", "Wolken", false, Preview::Rainbow),
    m(5, "Shadow", "Schatten", false, Preview::Rainbow),
    m(6, "Shine", "Glanz", false, Preview::Rainbow),
    m(7, "Pane", "Fenster", false, Preview::Rainbow),
    m(8, "Ripple", "Kräuseln", false, Preview::Rainbow),
    m(9, "Flowers", "Blumen", false, Preview::Rainbow),
    m(10, "Rain", "Regen", true, Preview::Stripes),
    m(13, "Rainbow", "Regenbogen", false, Preview::Rainbow),
    m(14, "Laser", "Laser", true, Preview::Sweep),
    m(15, "Waves", "Wellen", true, Preview::Pulse),
];

const BAR_MODES: [LightingMode; 8] = [
    m(0, "Static", "Dauerlicht", true, Preview::Solid),
    m(1, "Breathing", "Atmen", true, Preview::Pulse),
    m(2, "Marquee", "Lauflicht", false, Preview::Stripes),
    m(3, "Rainbow", "Regenbogen", false, Preview::Rainbow),
    m(4, "Colour cycle", "Farbwechsel", false, Preview::Rainbow),
    m(5, "Laser", "Laser", false, Preview::Sweep),
    m(6, "Reacts to keys", "Reagiert auf Tasten", true, Preview::Pulse),
    m(7, "Follow keys", "Wie Tastenlicht", false, Preview::Rainbow),
];

const INDICATOR_MODES: [LightingMode; 2] =
    [m(0, "Static", "Dauerlicht", true, Preview::Solid), m(1, "Follow keys", "Wie Tastenlicht", false, Preview::Rainbow)];

/// Zones and their effects. `count` from the lighting-area query is the number
/// of effects the zone supports, which hides the optional modes.
pub fn lighting_zone(area: u8, count: u8) -> LightingZone {
    match area {
        0 => LightingZone { en: "Keys", de: "Tasten", modes: KEY_MODES.to_vec(), speed: true, direction: true, multicolor_index0: true },
        1 => LightingZone {
            en: "Side light bar",
            de: "Seitliche Lichtleiste",
            modes: BAR_MODES.iter().copied().filter(|x| x.id < 6 || (x.id == 6 && count > 8) || (x.id == 7 && count > 7)).collect(),
            speed: true,
            direction: false,
            multicolor_index0: false,
        },
        _ => LightingZone {
            en: "Indicator",
            de: "Indikator",
            modes: INDICATOR_MODES.iter().copied().filter(|x| x.id == 0 || count > 2).collect(),
            speed: false,
            direction: false,
            multicolor_index0: false,
        },
    }
}

impl LightingZone {
    pub fn name(&self, lang: Lang) -> &'static str {
        match lang {
            Lang::De => self.de,
            Lang::En => self.en,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn german_legends() {
        assert_eq!(key_label(0x1c, true), "Z");
        assert_eq!(key_label(0x1c, false), "Y");
        assert_eq!(key_label(0x2f, true), "Ü");
        assert_eq!(key_label(0x1e, false), "1");
        assert_eq!(key_label(0x3a, false), "F1");
        assert_eq!(key_label(0xf101, false), "Fn");
    }
}
