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

The plugin installs a separately released Linux x86_64 worker (glibc 2.39 or newer), so
`omarchy plugin add https://github.com/jwkicklighter/omarchy-snipboard --enable`
requires no Rust toolchain or manual build. On first run it downloads the engine
version pinned in the repository, verifies its committed SHA-256 checksum, and
installs it in the user's XDG data directory. CI tests the published pin from a
disposable extracted installation separately from source-built candidates. The README
documents runtime dependencies, updates, disabling, and removal. Other CPU
architectures are not supported. First-run downloads require GitHub Releases
access; a verified cached worker runs offline. No system packages are installed.

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
