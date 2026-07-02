# AShell

English | [简体中文](./README.md)

> A modern, cross-platform terminal / SSH client — bundling a local terminal, remote SSH, SFTP, host monitoring, port forwarding, and an AI assistant into a single desktop app.

AShell focuses on performance, privacy, and customizability. All SSH credentials are encrypted at rest locally; the AI assistant runs on your own machine — your keys never leave your device.

---

## Features

### 🖥 Terminal
- **Cross-platform**: macOS / Windows / Linux, covering x86_64 and ARM64.
- **Multi-tab, multi-window**: draggable tab reordering, rich right-click menu (reconnect / disconnect / clone connection / new window / copy SSH address / rename / export session), seamless cross-window operations.
- **Local terminal**: Windows ConPTY / Unix forkpty, supports PowerShell, cmd, Git Bash, Bash, Zsh, Fish.
- **Remote SSH**: PTY + binary-safe frame passthrough, full preservation of color control codes.
- **Hardware-accelerated rendering**: WebGL + ligatures + clickable web links + Unicode 11 + search + serialization.
- **Progress bar detection**: automatically recognizes text-style progress from `cargo` / `brew` / `wget` / `curl` / `git` / `rsync` / `docker`, mirrored to the taskbar and terminal top bar.
- **Configurable actions**: Ctrl + scroll to resize font, right/middle mouse button behavior configurable, Ctrl + F search overlay.

### 📁 Host Management
- **Infinite-nested tree**: drag-to-move subtrees, cascading delete, cross-directory copy — organize your hosts by folder.
- **Credential encryption**: password / private key encrypted at rest with AES-256-GCM, never exposed through any API.
- **Host metadata**: name, address, port, username, icon, color, description — categorize freely.
- **Resizable side panel**: edge-drag to resize, width persisted across sessions.

### 📂 SFTP File Management
- **Full CRUD**: list (with owner / permissions / timestamps), mkdir/touch, delete, rename, attribute inspection.
- **Streaming transfer**: upload with progress and cancellation, download via native "save as" dialog, folder upload supported.
- **Breadcrumb path**: clickable + editable for quick navigation.
- **Connection reuse**: shares the same SSH connection with the terminal — no re-auth needed.

### 📊 Host Monitoring
- **Real-time resources**: CPU / memory / swap / disk / cumulative network bytes, 1.5s throttled polling.
- **Top 5 processes**: compatible with both procps and BusyBox.
- **Network throughput**: pure SVG dual-line (rx/tx) ring-buffer chart, per-NIC selection.
- **Disk folding**: root mount shown prominently, other mount points collapsed.

### 🔌 Port Forwarding
- **Three modes**: local (`-L`), remote (`-R`), dynamic (`-D`).
- **Visual rule table**: rule list + live traffic / status refresh.
- **GUI creation**: fill in a form to add a forwarding rule — no CLI needed.

### 📡 Broadcast Input
- **Multi-tab sync**: keystroke-level real-time forwarding to multiple terminals, selectable target tabs.
- **Cross-window support**: source tab selectable, auto-append newline, cross-window broadcast just works.

### 🤖 AI Assistant
- **Claude-powered**: one independent AI process per terminal session, fully isolated.
- **Collapsible execution trace**: tool calls and returns aggregated into foldable blocks for a clean conversation.
- **Destructive-operation confirmation**: y/n approval bar before execution; sensitive info (SSH session credentials) never revealed directly.
- **Remote command execution**: AI reuses the current SSH session and runs remote commands after you authorize.
- **Configurable model**: custom API key, base URL, and model name — compatible with proxies and self-hosted endpoints.

### 🎨 Window & Appearance
- **Opacity slider**: real-time window opacity adjustment.
- **Acrylic blur**: native blur on macOS / Windows 11.
- **Background wallpaper**: independent opacity, wallpaper and window transparency decoupled.
- **System font enumeration**: auto-lists installed fonts, deduplicated + sorted.
- **Themes**: dark / light, colors derived uniformly from theme variables.

### 🌐 i18n & Startup
- **Bilingual**: Simplified Chinese / English fully covered, UI language switches on demand.
- **Session restore**: remembers your last tab list, skeleton-restored on startup.
- **Default shell**: Auto / PowerShell / cmd / Git Bash / Bash / Zsh / Fish selectable, auto-opens a local terminal on startup.

---

## Download

Grab the installer for your platform from [Releases](https://github.com/vcqr/ashell/releases):

| Platform | File |
|---|---|
| macOS (Apple Silicon) | `*.dmg` |
| macOS (Intel) | `*.dmg` |
| Windows x64 | `*.msi` / `*.exe` |
| Windows ARM64 | `*.msi` / `*.exe` |
| Linux x64 | `*.deb` / `*.rpm` / `*.AppImage` |
| Linux ARM64 | `*.deb` / `*.rpm` / `*.AppImage` |

---

## License

MIT
