# Development workflow

Whenever you finish a fix or feature:

1. Run the relevant tests.
2. Build the plugin worker with `cargo build --release`.
3. Restart the Omarchy shell with `omarchy restart shell` so the current code is usable. Rescanning or toggling the plugin can retain cached QML and is not sufficient.
4. Verify that the shell responds and Snipboard's worker is ready.

The owner has granted standing permission to restart the Omarchy shell while working on this app. This briefly interrupts the bar and overlays, but should not close other apps. Do not reset desktop settings or change unrelated plugins.
