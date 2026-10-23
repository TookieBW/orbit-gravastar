//! Raw HID links. Device protocols talk to a [`Link`], which is either a real
//! hidapi handle or the in-memory demo emulation.

use std::time::{Duration, Instant};

use anyhow::Result;

/// One open HID interface.
pub trait Link {
    /// Send an output report.
    fn write(&mut self, report_id: u8, data: &[u8]) -> Result<()>;
    /// Next input report payload (without report id), or `None` after `timeout`.
    fn read(&mut self, timeout: Duration) -> Result<Option<Vec<u8>>>;
}

pub struct HidLink {
    dev: hidapi::HidDevice,
    /// The device uses numbered reports, so hidapi puts the report id in byte 0 of reads.
    numbered: bool,
}

impl HidLink {
    pub fn new(dev: hidapi::HidDevice, numbered: bool) -> Self {
        Self { dev, numbered }
    }
}

impl Link for HidLink {
    fn write(&mut self, report_id: u8, data: &[u8]) -> Result<()> {
        let mut buf = Vec::with_capacity(data.len() + 1);
        buf.push(report_id);
        buf.extend_from_slice(data);
        self.dev.write(&buf)?;
        Ok(())
    }

    fn read(&mut self, timeout: Duration) -> Result<Option<Vec<u8>>> {
        let mut buf = [0u8; 1025];
        let ms = timeout.as_millis().min(i32::MAX as u128) as i32;
        let n = self.dev.read_timeout(&mut buf, ms)?;
        Ok(match n {
            0 => None,
            _ if self.numbered => Some(buf[1..n].to_vec()),
            _ => Some(buf[..n].to_vec()),
        })
    }
}

/// Sends one report and waits for an input report that `accept` recognises as
/// the answer. Stale reports that arrived earlier are discarded first.
pub fn transfer(
    link: &mut dyn Link,
    report_id: u8,
    payload: &[u8],
    timeout: Duration,
    accept: impl Fn(&[u8]) -> bool,
) -> Result<Option<Vec<u8>>> {
    while link.read(Duration::ZERO)?.is_some() {}
    link.write(report_id, payload)?;
    let deadline = Instant::now() + timeout;
    loop {
        let now = Instant::now();
        if now >= deadline {
            return Ok(None);
        }
        match link.read(deadline - now)? {
            Some(report) if accept(&report) => return Ok(Some(report)),
            Some(_) => continue,
            None => return Ok(None),
        }
    }
}

/// The device did not answer in time. Distinguishes timeouts from OS errors,
/// which mean the handle is stale and the device must be reopened.
#[derive(Debug)]
pub struct NoAnswer(pub String);

impl std::fmt::Display for NoAnswer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "no answer to {}", self.0)
    }
}

impl std::error::Error for NoAnswer {}
