//! Command-line helpers.
//!
//! * `orbit --probe` prints what Orbit reads from every connected device
//!   (read-only; handy for bug reports and adding new models).
//! * `orbit --selftest-writes` additionally performs small, reversible writes
//!   and restores the original values (development aid).

use std::time::Duration;

use anyhow::{bail, Context, Result};
use hidapi::HidApi;

use crate::hid::HidLink;
use crate::protocol::keyboard::{HeKeyboard, MODE_RAPID};
use crate::protocol::mouse::CompxMouse;

pub fn run(args: &[String]) -> Option<i32> {
    let probe = args.iter().any(|a| a == "--probe");
    let writes = args.iter().any(|a| a == "--selftest-writes");
    if !probe && !writes {
        return None;
    }
    attach_console();
    Some(match run_inner(writes) {
        Ok(()) => 0,
        Err(e) => {
            println!("error: {e:#}");
            1
        }
    })
}

/// Release builds have no console window; reuse the parent's when started from a terminal.
fn attach_console() {
    #[cfg(windows)]
    {
        #[link(name = "kernel32")]
        extern "system" {
            fn AttachConsole(pid: u32) -> i32;
        }
        unsafe {
            AttachConsole(u32::MAX);
        }
    }
}

fn run_inner(writes: bool) -> Result<()> {
    let api = HidApi::new()?;
    let mut found = 0;
    for d in api.device_list() {
        let (vid, pid, page) = (d.vendor_id(), d.product_id(), d.usage_page());
        if HeKeyboard::matches(vid, pid, page) {
            found += 1;
            println!("== Keyboard {vid:04X}:{pid:04X} {}", d.product_string().unwrap_or(""));
            let mut kb = HeKeyboard::new(Box::new(HidLink::new(d.open_device(&api)?, false)));
            probe_keyboard(&mut kb)?;
            if writes {
                write_test_keyboard(&mut kb)?;
            }
        } else if CompxMouse::matches(vid, pid, page) {
            found += 1;
            println!("== Mouse {vid:04X}:{pid:04X} {}", d.product_string().unwrap_or(""));
            let mut m = CompxMouse::new(Box::new(HidLink::new(d.open_device(&api)?, true)));
            if !m.online()? {
                println!("   mouse asleep or off — move it and try again");
                continue;
            }
            probe_mouse(&mut m)?;
            if writes {
                write_test_mouse(&mut m)?;
            }
        }
    }
    if found == 0 {
        println!("no supported GravaStar device found");
    }
    Ok(())
}

fn probe_keyboard(kb: &mut HeKeyboard) -> Result<()> {
    let info = kb.info()?;
    println!("   firmware {} · protocol {} · build {} · board 0x{:08X}", info.firmware, info.protocol, info.build_time, info.board_id);
    println!("   profile {}/{} · polling id {:?} of {:?}", kb.profile()? + 1, kb.profile_count()?, kb.report_rate().ok(), kb.report_rates()?);
    let slots = kb.key_slots()?;
    println!("   {} keys in rows {:?}", slots.len(), slots.iter().map(|s| s.row).collect::<std::collections::BTreeSet<_>>());
    let rapid = slots.iter().filter_map(|s| kb.key_perf(s.row, s.col).ok()).filter(|p| p.mode == MODE_RAPID).count();
    println!("   rapid trigger on {rapid} keys");
    for a in kb.lighting_areas()? {
        let l = kb.lighting(a.index)?;
        println!("   light zone {}: on={} mode={} brightness={} speed={} color={}", a.index, l.on, l.mode, l.brightness, l.speed, l.color_index);
    }
    Ok(())
}

fn write_test_keyboard(kb: &mut HeKeyboard) -> Result<()> {
    println!("   -- reversible write test");
    // Lighting: change the effect of zone 0, read back, restore.
    let orig = kb.lighting(0)?;
    let probe_mode = if orig.mode == 12 { 13 } else { 12 };
    kb.set_lighting(0, &crate::protocol::keyboard::LightingBase { mode: probe_mode, ..orig })?;
    let got = kb.lighting(0)?.mode;
    kb.set_lighting(0, &orig)?;
    let restored = kb.lighting(0)?;
    check("lighting mode", got == probe_mode && restored == orig)?;

    // Actuation: nudge the first-activation depth of one key, read back, restore.
    let slot = kb.key_slots()?.into_iter().find(|s| s.row == 2 && s.col == 11).context("test key not found")?;
    let orig = kb.key_perf(slot.row, slot.col)?;
    let mut changed = orig;
    changed.rt_first_touch = if orig.rt_first_touch < 3.0 { orig.rt_first_touch + 0.2 } else { orig.rt_first_touch - 0.2 };
    changed.normal_release = 0.0;
    changed.calibrate = 0;
    kb.set_key_perf(slot.row, slot.col, &changed)?;
    let got = kb.key_perf(slot.row, slot.col)?;
    let restore = crate::protocol::keyboard::KeyPerf { normal_release: 0.0, calibrate: 0, ..orig };
    kb.set_key_perf(slot.row, slot.col, &restore)?;
    let back = kb.key_perf(slot.row, slot.col)?;
    check("key actuation", (got.rt_first_touch - changed.rt_first_touch).abs() < 0.001 && (back.rt_first_touch - orig.rt_first_touch).abs() < 0.001)?;
    Ok(())
}

fn probe_mouse(m: &mut CompxMouse) -> Result<()> {
    let id = m.identify()?;
    let (fw, dongle) = m.firmware()?;
    let b = m.battery()?;
    println!(
        "   {} · cid {} mid {} · {} · {} · max {} Hz",
        id.model.unwrap_or("unknown model"),
        id.cid,
        id.mid,
        id.sensor.name,
        if id.wired { "wired" } else { "2.4 GHz" },
        id.max_rate
    );
    println!("   firmware {fw} · receiver {} · battery {}% ({} mV){}", dongle.as_deref().unwrap_or("-"), b.level, b.millivolts, if b.charging { " charging" } else { "" });
    let c = m.read_config()?;
    println!("   {} Hz · stages {:?} (count {}, active {}) · LOD {} · debounce {} ms · motion sync {}", c.report_rate, c.dpi, c.stage_count, c.current_stage + 1, c.lod, c.debounce, c.motion_sync);
    Ok(())
}

fn write_test_mouse(m: &mut CompxMouse) -> Result<()> {
    println!("   -- reversible write test");
    let orig = m.read_config()?.dpi[0];
    let probe = if orig + 50 <= 26000 { orig + 50 } else { orig - 50 };
    m.set_dpi(0, probe)?;
    std::thread::sleep(Duration::from_millis(50));
    let got = m.read_config()?.dpi[0];
    m.set_dpi(0, orig)?;
    let back = m.read_config()?.dpi[0];
    check("mouse DPI stage 1", got == probe && back == orig)
}

fn check(what: &str, ok: bool) -> Result<()> {
    println!("   {} {what}", if ok { "OK  " } else { "FAIL" });
    if !ok {
        bail!("{what} did not read back as written");
    }
    Ok(())
}
