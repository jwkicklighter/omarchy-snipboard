//! Thin wrappers over the few libc calls std does not expose.

use std::io;
use std::time::{SystemTime, UNIX_EPOCH};

pub fn uid() -> u32 {
    // SAFETY: getuid has no preconditions and cannot fail.
    unsafe { libc::getuid() }
}

/// Seconds since the epoch, as stored in usage statistics.
pub fn now() -> f64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|elapsed| elapsed.as_secs_f64())
        .unwrap_or(0.0)
}

pub fn random_hex(bytes: usize) -> io::Result<String> {
    let mut buffer = vec![0u8; bytes];
    let mut filled = 0;
    while filled < bytes {
        // SAFETY: the pointer and length describe the unfilled tail of `buffer`.
        let read = unsafe {
            libc::getrandom(buffer[filled..].as_mut_ptr().cast(), bytes - filled, 0)
        };
        if read < 0 {
            let error = io::Error::last_os_error();
            if error.kind() == io::ErrorKind::Interrupted {
                continue;
            }
            return Err(error);
        }
        filled += read as usize;
    }
    Ok(buffer.iter().map(|byte| format!("{byte:02x}")).collect())
}

/// Local wall-clock fields for template variables.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LocalTime {
    pub year: i32,
    pub month: u32,
    pub day: u32,
    pub hour: u32,
    pub minute: u32,
}

impl LocalTime {
    pub fn now() -> Self {
        let seconds = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|elapsed| elapsed.as_secs() as libc::time_t)
            .unwrap_or(0);
        // SAFETY: localtime_r only writes into the zeroed struct we own.
        let mut fields: libc::tm = unsafe { std::mem::zeroed() };
        unsafe { libc::localtime_r(&seconds, &mut fields) };
        LocalTime {
            year: fields.tm_year + 1900,
            month: (fields.tm_mon + 1) as u32,
            day: fields.tm_mday as u32,
            hour: fields.tm_hour as u32,
            minute: fields.tm_min as u32,
        }
    }

    pub fn date(&self) -> String {
        format!("{:04}-{:02}-{:02}", self.year, self.month, self.day)
    }

    pub fn time(&self) -> String {
        format!("{:02}:{:02}", self.hour, self.minute)
    }
}
