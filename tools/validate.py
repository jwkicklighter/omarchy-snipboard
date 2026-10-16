"""Read-only manifest/lint validation. Temporary import mapping is outside source."""
from pathlib import Path
import subprocess
import tempfile


def main():
    root = Path(__file__).resolve().parent.parent
    subprocess.run(["omarchy", "plugin", "validate", str(root)], check=True)
    with tempfile.TemporaryDirectory(prefix="snipboard-lint-", dir="/tmp/opencode") as directory:
        (Path(directory) / "qs").symlink_to("/usr/share/omarchy/shell", target_is_directory=True)
        result = subprocess.run(["/usr/lib/qt6/bin/qmllint", "-I", directory, str(root / "Snipboard.qml"), str(root / "SbButton.qml")], capture_output=True, text=True)
    output = result.stdout + result.stderr
    log = Path("/tmp/opencode/snipboard-qmllint.log")
    log.write_text(output)
    warnings = [line for line in output.splitlines() if line.startswith("Warning:")]
    known = {"missing-property", "uncreatable-type", "signal-handler-parameters"}
    unexpected = [line for line in warnings if not any(line.endswith("[" + category + "]") for category in known)]
    print(f"Manifest valid. qmllint exit={result.returncode}, {len(warnings)} warnings; full output: {log}")
    print("Known installed metadata warnings: inline shell QtObject tokens, PanelWindow factory, QProcess enum. No warnings suppressed.")
    if result.returncode or unexpected or "Error:" in output:
        print(output)
        raise SystemExit(1)


if __name__ == "__main__":
    main()
