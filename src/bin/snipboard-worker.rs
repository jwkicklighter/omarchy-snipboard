//! Persistent prototype helper. Usage: snipboard-worker [--data PATH]

use std::io::{Read, Write};
use std::path::PathBuf;
use std::process::ExitCode;

use snipboard::worker::{FATAL, serve_with_library_root};

fn default_data() -> PathBuf {
    let state = std::env::var_os("XDG_STATE_HOME")
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(std::env::var_os("HOME").unwrap_or_default()).join(".local/state"));
    state.join("omarchy-snipboard-prototype")
}

fn default_library() -> PathBuf {
    let config = std::env::var_os("XDG_CONFIG_HOME")
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(std::env::var_os("HOME").unwrap_or_default()).join(".config"));
    config.join("omarchy/extensions/io.github.jwkicklighter.snipboard")
}

fn main() -> ExitCode {
    if std::env::args_os().nth(1).as_deref() == Some(std::ffi::OsStr::new("--clipboard-event")) {
        return clipboard_event();
    }
    let mut data = default_data();
    let mut custom_data = false;
    let mut arguments = std::env::args_os().skip(1);
    while let Some(argument) = arguments.next() {
        match argument.to_str() {
            Some("--data") => match arguments.next() {
                Some(path) => {
                    data = PathBuf::from(path);
                    custom_data = true;
                }
                None => return usage(),
            },
            Some(other) if other.starts_with("--data=") => {
                data = PathBuf::from(&other["--data=".len()..]);
                custom_data = true;
            }
            _ => return usage(),
        }
    }
    // Keep explicit --data sandboxes self-contained for tools and tests.
    let library = if custom_data { data.clone() } else { default_library() };
    match serve_with_library_root(&data, &library) {
        Ok(()) => ExitCode::SUCCESS,
        Err(_) => {
            // The UI may already be gone; a failed report must not turn into a panic.
            let _ = writeln!(std::io::stdout(), r#"{{"op":"fatal","error":"{FATAL}"}}"#);
            ExitCode::FAILURE
        }
    }
}

/// wl-paste invokes this once per selection, with exact bytes on stdin. Emit one
/// ASCII JSON frame so multiline text cannot break the shell's line parser.
fn clipboard_event() -> ExitCode {
    if std::env::var("CLIPBOARD_STATE").is_ok_and(|state| state != "data") {
        return ExitCode::SUCCESS;
    }
    let mut bytes = Vec::new();
    if std::io::stdin().take(65537).read_to_end(&mut bytes).is_err() || bytes.len() > 65536 {
        return ExitCode::SUCCESS;
    }
    let Ok(text) = String::from_utf8(bytes) else { return ExitCode::SUCCESS };
    if text.trim().is_empty() || text.contains('\0') {
        return ExitCode::SUCCESS;
    }
    let event = snipboard::json::to_ascii_string(&serde_json::json!({"text": text}));
    let _ = writeln!(std::io::stdout(), "{event}");
    ExitCode::SUCCESS
}

fn usage() -> ExitCode {
    eprintln!("usage: snipboard-worker [--data PATH]");
    ExitCode::from(2)
}
