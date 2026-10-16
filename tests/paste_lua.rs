//! Validate the compositor script against an inert Lua API without injecting input.
use std::path::Path;
use std::process::Command;

#[test]
fn compositor_paste_chords_and_key_release() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let output = Command::new("lua")
        .arg(root.join("tests/lua/paste.lua"))
        .arg(root.join("src/paste.lua"))
        .output();
    if let Err(error) = &output {
        if error.kind() == std::io::ErrorKind::NotFound {
            eprintln!("skipped: standalone Lua interpreter unavailable");
            return;
        }
    }
    let output = output.unwrap();
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
}
