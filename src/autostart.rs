//! "Start with Windows": an entry in the per-user Run key that launches Orbit
//! hidden in the tray.

/// Command-line flag that starts Orbit hidden in the tray.
pub const TRAY_ARG: &str = "--tray";

#[cfg(windows)]
mod imp {
    use anyhow::Result;
    use winreg::enums::{HKEY_CURRENT_USER, KEY_READ};
    use winreg::RegKey;

    pub const RUN_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
    const VALUE: &str = "Orbit";

    pub fn is_enabled_in(key: &str) -> bool {
        RegKey::predef(HKEY_CURRENT_USER)
            .open_subkey_with_flags(key, KEY_READ)
            .and_then(|k| k.get_value::<String, _>(VALUE))
            .is_ok()
    }

    pub fn set_enabled_in(key: &str, on: bool) -> Result<()> {
        let (run, _) = RegKey::predef(HKEY_CURRENT_USER).create_subkey(key)?;
        if on {
            let exe = std::env::current_exe()?;
            run.set_value(VALUE, &format!("\"{}\" {}", exe.display(), super::TRAY_ARG))?;
        } else if let Err(e) = run.delete_value(VALUE) {
            if e.kind() != std::io::ErrorKind::NotFound {
                return Err(e.into());
            }
        }
        Ok(())
    }
}

#[cfg(windows)]
pub fn is_enabled() -> bool {
    imp::is_enabled_in(imp::RUN_KEY)
}

#[cfg(windows)]
pub fn set_enabled(on: bool) -> anyhow::Result<()> {
    imp::set_enabled_in(imp::RUN_KEY, on)
}

#[cfg(not(windows))]
pub fn is_enabled() -> bool {
    false
}

#[cfg(not(windows))]
pub fn set_enabled(_on: bool) -> anyhow::Result<()> {
    anyhow::bail!("autostart is only implemented for Windows")
}

#[cfg(all(test, windows))]
mod tests {
    use super::imp::*;

    #[test]
    fn toggle_in_scratch_key() {
        // A throwaway key, so the test never touches the real Run entry.
        let key = r"Software\OrbitAutostartTest";
        set_enabled_in(key, true).unwrap();
        assert!(is_enabled_in(key));
        set_enabled_in(key, false).unwrap();
        assert!(!is_enabled_in(key));
        set_enabled_in(key, false).unwrap(); // removing twice is fine
        let _ = winreg::RegKey::predef(winreg::enums::HKEY_CURRENT_USER).delete_subkey_all(key);
    }
}
