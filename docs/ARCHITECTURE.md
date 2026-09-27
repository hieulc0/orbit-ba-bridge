# Architecture Overview

`orbit-ba-bridge` connects external Business Analyst-style reasoning sessions, initially ChatGPT running in an interactive browser session, to Orbit-managed agent development workflows.

## System Topology

```text
ChatGPT Web (Browser)
       │
Extension Content Scripts (DOM observer & composer)
       │
Extension Background Worker (Service Worker / Event Page)
       │
Localhost WebSocket (`ws://127.0.0.1:48117`)
       │
bridge-server (Rust CLI & runtime)
       │
bridge-core (Driver abstractions, session management, routing)
       │
protocol (Wire types, versioned envelopes, validation)
       │
Orbit ACP (Future milestone)
```

## Core Invariants

1. **Orbit Authority**: Orbit remains the single workflow authority for task status, role execution, verification, and code mutation. The bridge does not become a second workflow engine.
2. **Provider Neutrality**: Core protocol and driver abstractions do not depend on ChatGPT-specific DOM concepts.
3. **Local-Only Transport**: WebSocket listener binds to `127.0.0.1` by default. No remote access is exposed without explicit security layers.
4. **Driver Boundary**: All browser interactions pass through the `BrowserDriver` trait, allowing `ExtensionDomDriver`, `MockBrowserDriver`, and future `HeadlessChromiumDriver` to share the same domain and routing logic.
5. **No Repository Mutation by BA**: BA reasoning roles can challenge, analyze, and propose requirements, but have no direct repository write or command execution access.

## Dual-Browser Architecture (Chromium & Firefox)

The browser extension is designed for seamless dual-browser execution across Chromium (Chrome, Edge, Brave) and Gecko (Firefox):

1. **Unified API Abstraction**:
   - WebExtension API differences are normalized through `extension/shared/browser-compat.js` using `globalThis.browser || globalThis.chrome`.
2. **Shared Codebase**:
   - The DOM observer (`content/dom.js`), composer submission controller (`content/composer.js`), and bridge protocol serialization (`shared/protocol.js`) are 100% shared without browser-specific branching.
   - Script loading uses standard sequential content-script injection and Service Worker `importScripts()`, avoiding bundler complexity.
3. **Manifest Strategies**:
   - **Chromium (Active implementation)**: `manifest.json` defines a standard MV3 background `service_worker`.
   - **Firefox**: `manifest.firefox.json` defines MV3 background `scripts` and `browser_specific_settings.gecko` for unsigned development loading.
4. **Rollout Sequence**:
   - Chromium is implemented and verified first.
   - Firefox is enabled simply by referencing the Gecko manifest template.

## Crate Responsibilities

- **`crates/protocol`**: Stable wire format, `MessageEnvelope<T>`, `BrowserEvent`, `BrowserCommand`, identifiers (`SessionId`, `CorrelationId`, `TaskId`), and strict bounds validation.
- **`crates/bridge-core`**: Trait `BrowserDriver`, `MockBrowserDriver`, `SessionManager` (lifecycle state machine and duplicate message filtering), and `BridgeRouter`.
- **`crates/bridge-server`**: Executable CLI, configuration, `tokio-tungstenite` WebSocket listener on localhost, logging via `tracing`, and graceful shutdown handling.
