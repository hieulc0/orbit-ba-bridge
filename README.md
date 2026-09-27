# orbit-ba-bridge

`orbit-ba-bridge` connects external Business Analyst-style reasoning sessions,
initially ChatGPT running in a normal browser, to Orbit-managed development
workflows.

The first integration uses a lightweight browser extension and localhost
WebSocket bridge. Orbit remains the workflow authority; the browser integration
only transports normalized role messages.

Future drivers may support headless Chromium without changing the core BA or
Orbit protocol.

## Architecture

```text
ChatGPT Web (Browser)
       ↓
Browser Extension (DOM observer & composer)
       ↓
localhost WebSocket (ws://127.0.0.1:48117)
       ↓
orbit-ba-bridge (Rust)
       ↓
Orbit ACP (Future milestone)
       ↓
Orbit workflow
```

## Repository Structure

- `crates/protocol`: Typed, versioned message envelopes, browser events, and commands.
- `crates/bridge-core`: Core application logic, `BrowserDriver` abstraction, session lifecycle, and message routing.
- `crates/bridge-server`: Localhost WebSocket server executable and connection lifecycle.
- `extension/`: Chrome/Chromium browser extension for ChatGPT web integration.
- `docs/`: In-depth architecture, protocol specifications, security guidelines, and development guides.
- `scripts/`: Development and helper scripts.

## Quick Start

```bash
# Verify workspace compilation and tests
./scripts/dev.sh

# Run bridge-server
cargo run --bin bridge-server
```

See [docs/DEVELOPMENT.md](file:///home/hieulc/projects/orbit-ba-bridge/docs/DEVELOPMENT.md) for complete setup instructions.
