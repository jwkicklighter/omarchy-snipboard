//! Offscreen editor tests with inert theme tokens; never loads the shell/plugin.

use std::path::Path;
use std::process::Command;

#[test]
fn editor_offscreen() {
    let runner = ["qmltestrunner", "/usr/lib/qt6/bin/qmltestrunner"]
        .into_iter()
        .find(|runner| Command::new(runner).arg("-h").output().is_ok());
    let Some(runner) = runner else {
        assert!(std::env::var_os("SNIPBOARD_REQUIRE_TEST_RUNNERS").is_none(), "Qt Quick Test runner required in CI");
        eprintln!("skipped: Qt Quick Test runner unavailable");
        return;
    };
    let source = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/qml");
    let cache = tempfile::Builder::new().prefix("snipboard-qml-").tempdir().unwrap();
    let output = Command::new(runner)
        .arg("-input")
        .arg(&source)
        .arg("-import")
        .arg(source.join("imports"))
        .env("QT_QPA_PLATFORM", "offscreen")
        .env("QT_QPA_PLATFORMTHEME", "generic")
        .env("QT_QUICK_CONTROLS_STYLE", "Basic")
        .env("QT_QUICK_BACKEND", "software")
        .env("QML_DISABLE_DISK_CACHE", "1")
        .env("XDG_CACHE_HOME", cache.path())
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}
