# AShell

English | [简体中文](./README.md)

> A modern, cross-platform terminal / SSH client — bundling a local terminal, remote SSH, SFTP, host monitoring, port forwarding, and an AI assistant into a single desktop app.

AShell focuses on performance, privacy, and customizability. All SSH credentials are encrypted at rest locally; the AI assistant runs on your own machine - your keys never leave your device.

![AShell](./ui.png)

---

## Features

### 🖥 Terminal
- **Cross-platform**: macOS / Windows / Linux, covering x86_64 and ARM64.
- **Multi-tab, multi-window**: draggable tab reordering, rich right-click menu (reconnect / disconnect / clone connection / new window / copy SSH address / rename / export session), seamless cross-window operations.
- **Local terminal**: Windows ConPTY / Unix forkpty, supports PowerShell, cmd, Git Bash, Bash, Zsh, Fish.
- **Remote SSH**: PTY + binary-safe frame passthrough, full preservation of color control codes.
- **Telnet / Serial**: besides SSH, supports Telnet and serial terminal connections.
- **Hardware-accelerated rendering**: WebGL + ligatures + clickable web links + Unicode 11 + search + serialization.
- **Progress bar detection**: automatically recognizes text-style progress from `cargo` / `brew` / `wget` / `curl` / `git` / `rsync` / `docker`, mirrored to the taskbar and terminal top bar.
- **Command suggestions**: Trie prefix matching + history keyword search, with clearable history.
- **sudo auto-fill**: recognizes sudo password prompts and auto-fills.
- **Configurable actions**: Ctrl + scroll to resize font, right/middle mouse button behavior configurable, Ctrl + F search overlay.
- **Reconnect**: offline recovery shows a "reconnect" floating button; local terminal auto-closes its tab on exit.

### 📁 Host Management
- **Infinite-nested tree**: drag-to-move subtrees, cascading delete, cross-directory copy — organize your hosts by folder.
- **Credential encryption**: password / private key encrypted at rest with AES-256-GCM, never exposed through any API.
- **Host metadata**: name, address, port, username, icon, color, description — categorize freely.
- **Resizable side panel**: edge-drag to resize, width persisted across sessions.
- **Split-layout form**: basic info and connection config in separate panes for cleaner editing.

### 📂 SFTP File Management
- **Full CRUD**: list (with owner / permissions / timestamps), mkdir/touch, delete, rename, attribute inspection.
- **Streaming transfer**: upload with progress and cancellation, download via native "save as" dialog, folder upload supported.
- **Breadcrumb path**: clickable + editable for quick navigation.
- **Connection reuse**: shares the same SSH connection with the terminal - no re-auth needed.
- **Inline editor**: built-in CodeMirror editor with find / replace, font-size adjustment, and a row/column status bar.
- **Resizable columns**: table column widths are drag-adjustable.
- **Permission coloring**: permission column colored independently across owner / group / other.

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
- **Multi-agent**: unified sidecar (sidecar-ai) embedding both the Claude Agent SDK and the Pi coding agent engines (factory-selected), one independent process per terminal session.
- **Send to AI**: terminal selection and SFTP right-click menu support sending content to the AI assistant.
- **Collapsible execution trace**: tool calls and returns aggregated into foldable blocks for a clean conversation.
- **Destructive-operation confirmation**: y/n approval bar before execution; sensitive info (SSH session credentials) never revealed directly.
- **Remote command execution**: AI reuses the current SSH session and runs remote commands after you authorize.
- **Configurable model**: custom API key, base URL, and model name, displayed per sidecar type - compatible with proxies and self-hosted endpoints.

### 🎨 Window & Appearance
- **Opacity slider**: real-time window opacity adjustment.
- **Acrylic blur**: native blur on macOS / Windows 11.
- **Background wallpaper**: independent opacity, wallpaper and window transparency decoupled.
- **System font enumeration**: auto-lists installed fonts, deduplicated + sorted.
- **Themes**: dark / light, colors derived uniformly from theme variables.

### 🌐 i18n & Startup
- **Bilingual**: Simplified Chinese / English fully covered, UI language switches on demand.
- **Session restore**: remembers your last tab list, skeleton-restored on startup.
- **Auto-connect**: automatically reconnects remembered tabs on startup.
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

## Build from Source

### Prerequisites

- [Node.js](https://nodejs.org/) 18+
- [Rust](https://www.rust-lang.org/) (stable)
- [Bun](https://bun.sh/) (for compiling sidecar binaries)
- Platform deps: Xcode CLT on macOS; MSVC on Windows; webkit2gtk etc. on Linux

### Steps

```bash
# 1. Install frontend dependencies
npm install

# 2. Install sidecar dependencies
cd sidecar-ai && npm install && cd ..

# 3. Development mode (Vite + Rust hot reload)
npm run tauri dev
#    dev does not auto-compile the sidecar; run this first if you need the AI assistant:
npm run sidecar:build

# 4. Production build (auto-compiles sidecar + frontend + bundles)
npm run tauri build
```

> `npm run tauri build` automatically runs `npm run sidecar:build` (compiles the unified AI sidecar binary) first, then `npm run build` (frontend type-check + Vite build) - no manual sidecar compilation needed.

---

## License

MIT
