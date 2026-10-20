### Repository URL

https://github.com/jwkicklighter/omarchy-snipboard

### Category

Productivity

### Tags

hyprland, quickshell, launcher

### Suggest a missing tag

_No response_

### Maintainer notes

Snipboard Prototype provides local clipboard text history and snippet search in
an Omarchy Quattro overlay. This is explicitly a prototype; global typed keyword
expansion and image history are not implemented.

Please classify this listing as manual-setup. The standard Omarchy installer
clones the repository but does not compile its Rust helper. Users must build
`build/release/snipboard-worker` with Rust 1.89+ before enabling the plugin, or
install a prebuilt Linux x86_64 GitHub release bundle. The README documents both
paths, external dependencies, updates, disabling, and removal. There is no
automatic binary download or package installation.

The enabled plugin records regular text clipboard contents using wl-paste and
stores history/snippets locally in owner-only files. Clipboard contents can be
sensitive; source-provided sensitive hints are honored, but are not universal
password detection. Explicit paste actions use Hyprland's Lua key-state API.
No desktop shortcuts, unrelated clipboard services, or user configuration are
overwritten. The repository and plugin are MIT-licensed. The root `preview.png`
shows the plugin on an Omarchy desktop using fictional sample data.

### Submission checklist

- [x] The repository is public and contains installation and removal instructions.
- [x] I have documented the plugin license and any external dependencies.
- [x] I confirm that I own or have permission to submit this plugin and its preview assets.
- [x] The plugin does not overwrite user configuration without explicit consent.
- [x] I understand that approval is for listing and is not a security review.
