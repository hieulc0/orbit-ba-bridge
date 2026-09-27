# Development Guide

## Prerequisites

- Rust (1.85+ recommended, 2024 edition)
- Cargo
- Chromium-based browser (for extension testing)

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

1. Open Chrome / Chromium and navigate to `chrome://extensions`.
2. Enable **Developer mode** (top right toggle).
3. Click **Load unpacked** and select the `extension/` directory in this repository.
4. Open `https://chatgpt.com` in your browser.
5. Watch the `bridge-server` console logs to observe the connection handshake and events.
