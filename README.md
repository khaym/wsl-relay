<p align="center">
  <img src="assets/logo.png" alt="WSL Relay" width="120">
</p>

<h1 align="center">WSL Relay</h1>

<p align="center">
  <em>Safely bridge the gap between WSL2/Docker and Windows —<br>notifications, clipboard, and more — without breaking your sandbox.</em>
</p>

<p align="center">
  <a href="https://github.com/khaym/wsl-relay/actions/workflows/ci.yml">
    <img src="https://github.com/khaym/wsl-relay/actions/workflows/ci.yml/badge.svg" alt="CI">
  </a>
  <a href="https://github.com/khaym/wsl-relay/releases">
    <img src="https://img.shields.io/github/v/release/khaym/wsl-relay?include_prereleases&label=release" alt="Release">
  </a>
  <a href="LICENSE-MIT">
    <img src="https://img.shields.io/badge/license-MIT%2FApache--2.0-blue" alt="License">
  </a>
  <img src="https://img.shields.io/badge/platform-Windows%20x86__64-0078D4?logo=windows" alt="Platform">
  <img src="https://img.shields.io/badge/made%20with-Rust-F46623?logo=rust" alt="Rust">
</p>

<p align="center">
  <a href="#features">Features</a> &bull;
  <a href="#quick-start">Quick Start</a> &bull;
  <a href="#api-reference">API Reference</a> &bull;
  <a href="#configuration">Configuration</a> &bull;
  <a href="#security">Security</a> &bull;
  <a href="#license">License</a>
</p>

---

WSL2/Docker environments lack access to Windows-native features like toast notifications and clipboard. Enabling PowerShell interop would fix it, but that breaks your sandbox.

WSL Relay is a tiny Windows-native tray app that exposes a localhost REST API for your WSL/Docker tools — no PowerShell, no security trade-offs.

## Features

- **Toast Notifications** — Get notified when Claude Code completes a task, CI finishes, or tests fail — right on your Windows desktop
- **Clipboard Image** — Grab Windows screenshots directly from your container. No more "save file, copy to mount" dance
- **Secure by Design** — No PowerShell interop needed. Your sandbox stays intact
- **Lightweight** — Rust-built single binary (~1.5 MB). Runs silently in the system tray with minimal resource footprint
- **System Tray + Auto-start** — Stays resident in the tray. Optionally launches at Windows login

## Quick Start

### 1. Download

Grab `wsl-relay.exe` from [GitHub Releases](https://github.com/khaym/wsl-relay/releases).

### 2. Run

Double-click `wsl-relay.exe`. It starts listening on `127.0.0.1:9400` and appears in the system tray.

### 3. Use from WSL/Docker

```bash
# Health check
curl http://host.docker.internal:9400/api/v1/health

# Send a notification
curl -X POST http://host.docker.internal:9400/api/v1/notify \
  -H "Content-Type: application/json" \
  -d '{"title":"Build Complete","body":"All tests passed","icon":"success"}'

# Read clipboard image (returns PNG)
curl http://host.docker.internal:9400/api/v1/clipboard/image -o screenshot.png

# Enable auto-start
curl -X PUT http://host.docker.internal:9400/api/v1/autostart
```

## API Reference

All endpoints are prefixed with `/api/v1`.

### Health

| Method | Path | Description |
|--------|------|-------------|
| `GET` | `/health` | Returns `{"status":"ok","version":"..."}` |

### Notifications

| Method | Path | Description |
|--------|------|-------------|
| `POST` | `/notify` | Send a Windows toast notification |

Request body:
```json
{
  "title": "Build Complete",
  "body": "All 70 tests passed",
  "icon": "success"
}
```

`icon` values: `info` (default), `success`, `warning`, `error`

### Clipboard

| Method | Path | Description |
|--------|------|-------------|
| `GET` | `/clipboard/image` | Read image from clipboard as PNG |

Returns `image/png` binary on success, `500` if no image is in the clipboard.

### Auto-start

| Method | Path | Description |
|--------|------|-------------|
| `GET` | `/autostart` | Check status → `{"enabled": true\|false}` |
| `PUT` | `/autostart` | Register in Windows startup |
| `DELETE` | `/autostart` | Unregister from Windows startup |

### Error Responses

| Status | Meaning |
|--------|---------|
| `403` | Operation disabled in config |
| `500` | Backend operation failed |

## Configuration

WSL Relay works out of the box with no configuration. Create a config file only when you need to change defaults.

### Config File

Create `%APPDATA%\wsl-relay\config.toml`:

```toml
port = 9400
enabled_operations = ["health", "notify", "clipboard", "screenshot", "autostart"]
```

Both fields are optional — omitted fields use defaults shown above.

### Config Priority

| Priority | Source | Scope |
|----------|--------|-------|
| 1 (highest) | `WSL_RELAY_PORT` env var | Port only |
| 2 | `WSL_RELAY_CONFIG` env var | Full config (path to TOML) |
| 3 | `%APPDATA%\wsl-relay\config.toml` | Full config |
| 4 (lowest) | Built-in defaults | Port 9400, all operations enabled |

### Disabling Operations

To restrict which APIs are available:

```toml
enabled_operations = ["health", "notify"]
```

Disabled endpoints return `403 Forbidden`.

## Security

- **Localhost-only binding** — The server binds to `127.0.0.1`, never exposed to the network
- **No shell access** — Unlike PowerShell passthrough, WSL Relay exposes only specific, scoped operations
- **Operation-level control** — Each API can be individually disabled via config

## Building from Source

Requirements: Rust toolchain with `x86_64-pc-windows-msvc` target.

```bash
cargo build --release
```

The binary is at `target/release/wsl-relay.exe` (~1.5 MB with LTO + strip).

### Running Tests

```bash
cargo test
```

Tests run on any platform using stub backends.

## License

Licensed under either of

- [Apache License, Version 2.0](LICENSE-APACHE)
- [MIT License](LICENSE-MIT)

at your option.
