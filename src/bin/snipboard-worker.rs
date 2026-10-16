//! Persistent prototype helper. Usage: snipboard-worker [--data PATH]

use std::io::Write;
use std::path::PathBuf;
use std::process::ExitCode;

use snipboard::worker::{FATAL, serve};

fn default_data() -> PathBuf {
    let state = std::env::var_os("XDG_STATE_HOME")
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(std::env::var_os("HOME").unwrap_or_default()).join(".local/state"));
    state.join("omarchy-snipboard-prototype")
}

fn main() -> ExitCode {
    let mut data = default_data();
    let mut arguments = std::env::args_os().skip(1);
    while let Some(argument) = arguments.next() {
        match argument.to_str() {
            Some("--data") => match arguments.next() {
                Some(path) => data = PathBuf::from(path),
                None => return usage(),
            },
            Some(other) if other.starts_with("--data=") => data = PathBuf::from(&other["--data=".len()..]),
            _ => return usage(),
        }
    }
    match serve(&data) {
        Ok(()) => ExitCode::SUCCESS,
        Err(_) => {
            // The UI may already be gone; a failed report must not turn into a panic.
            let _ = writeln!(std::io::stdout(), r#"{{"op":"fatal","error":"{FATAL}"}}"#);
            ExitCode::FAILURE
        }
    }
}

fn usage() -> ExitCode {
    eprintln!("usage: snipboard-worker [--data PATH]");
    ExitCode::from(2)
}
