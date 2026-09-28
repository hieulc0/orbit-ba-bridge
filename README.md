# orbit-ba-bridge

`orbit-ba-bridge` connects external Business Analyst-style reasoning sessions,
initially ChatGPT running in an interactive browser session, to Orbit-managed
development workflows.

It functions today as a **standalone daily Human ↔ ChatGPT conversation bridge**
with durable local SQLite history, reliable browser reconnect, conversation switching,
and Markdown/JSON export—while preserving the multi-actor foundations for future Orbit integration.

## Architecture

```text
Human
  ↕
ChatGPT Web (Browser)
  ↕ (Content Scripts / DOM observer & composer)
Extension Service Worker
  ↕ (Localhost WebSocket: ws://127.0.0.1:48117 with auth handshake)
bridge-server (Rust CLI & daemon runtime)
  ↕
bridge-core (Multi-actor conversation store & SQLite WAL engine)
  ↕
[Future: Orbit SA / Orbit ACP]
```

## Highlights & Capabilities

- **Durable Local History**: All turns are strictly ordered and persisted to local SQLite (`$XDG_DATA_HOME/orbit-ba-bridge/conversations.sqlite`) with schema versioning and forward migrations.
- **Reliable Browser Reconnect & Reload Recovery**: Refreshing ChatGPT (`F5`/`Cmd+R`) or restarting the browser maintains session continuity; historical turns are verified against SQLite and never duplicated.
- **Conversation Switching & Dynamic Binding**: Switching chats in ChatGPT (`/c/<id>`) transparently switches active conversation contexts without merging turns. Starting an unbound draft binds seamlessly upon URL assignment.
- **Multi-Tab Isolation**: Strictly enforces a single active ChatGPT tab; reports `MULTIPLE_CHATGPT_TABS` error when ambiguous multiple tabs are opened.
- **Conversation Inspection & Search**: CLI subcommands to list conversations, inspect turns, and search messages across conversations.
- **Markdown & JSON Export**: Complete export of discussions with participants, timestamps, and monotonic sequence numbers.
- **Security & Token Hygiene**: Localhost-only WebSocket binding with token handshake; secrets stored in XDG state and masked in CLI output; zero secrets tracked in git.

## Repository Structure

- `crates/protocol`: Typed, versioned message envelopes, browser events, commands, and standardized error codes.
- `crates/bridge-core`: Core application logic, `BrowserDriver` abstraction, multi-actor domain models, SQLite persistence with migrations, injection ledger, and exporters.
- `crates/bridge-server`: Localhost WebSocket server daemon, CLI commands, interactive stdin prompt, and XDG path resolution.
- `extension/`: Chromium & Firefox browser extension for ChatGPT web integration.
- `docs/`: In-depth architecture, protocol specifications, security guidelines, and development guides.
- `scripts/`: Development and helper scripts.

## Quick Start

### 1. Build and Test
```bash
cargo test --workspace
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
```

### 2. Run the Bridge Server
```bash
cargo run --bin bridge-server
```

Load `extension/` as an unpacked extension in Chrome/Chromium (`chrome://extensions`), open `https://chatgpt.com`, and start conversing.

### 3. CLI Inspection & Export
```bash
# List conversations
cargo run --bin bridge-server -- conversation list

# Inspect conversation turns
cargo run --bin bridge-server -- conversation show <CONVERSATION_ID> --full

# Search message history
cargo run --bin bridge-server -- conversation search "architecture"

# Export to Markdown
cargo run --bin bridge-server -- conversation export <CONVERSATION_ID> --format markdown --output discussion.md

# Export to JSON
cargo run --bin bridge-server -- conversation export <CONVERSATION_ID> --format json --output discussion.json
```

See [docs/DEVELOPMENT.md](file:///home/hieulc/projects/orbit-ba-bridge/docs/DEVELOPMENT.md) for complete setup instructions.
