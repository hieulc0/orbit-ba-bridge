# Development Guide

## Prerequisites

- Rust (1.85+ recommended, 2024 edition)
- Cargo
- Chromium-based browser (primary implementation target) or Firefox

## Building the Workspace

Build all crates in the workspace:

```bash
cargo build --workspace
```

## Running Tests

Run all unit and integration tests across crates:

```bash
cargo test --workspace
```

## Running Bridge Server

Start the local bridge server on `127.0.0.1:48117`:

```bash
cargo run --bin bridge-server
```

Or pass custom options:

```bash
cargo run --bin bridge-server -- --port 48117 --log-level debug
```

## Loading the Browser Extension

### Chromium (Chrome, Edge, Brave) — Primary

1. Open browser and navigate to `chrome://extensions`.
2. Enable **Developer mode** (top right toggle).
3. Click **Load unpacked** and select the `extension/` directory.
4. Open `https://chatgpt.com` in your browser.
5. Check `bridge-server` console logs to observe the connection handshake and events.

### Firefox (Dual-Browser Target)

1. Copy or link `extension/manifest.firefox.json` as `manifest.json`.
2. Open Firefox and navigate to `about:debugging#/runtime/this-firefox`.
3. Click **Load Temporary Add-on...** and select `extension/manifest.json`.
4. Open `https://chatgpt.com` in Firefox.
