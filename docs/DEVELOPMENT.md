# Development Guide

## Prerequisites

- Rust (1.85+ recommended, 2024 edition)
- Cargo
- Chromium-based browser (Chrome, Brave, Edge)
- Node.js (for syntax validation of extension JS files)

---

## Building the Workspace

Build all crates in the workspace:

```bash
cargo build --workspace
```

---

## Running Tests & Quality Gates

Run all unit and integration tests across crates:

```bash
cargo test --workspace
```

Run formatting and Clippy checks:

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
```

Verify extension JavaScript files:

```bash
node --check extension/shared/browser-compat.js
node --check extension/shared/protocol.js
node --check extension/content/dom.js
node --check extension/content/composer.js
node --check extension/content/content.js
node --check extension/background/background.js
```

---

## Running Bridge Server

Start the local bridge server on `127.0.0.1:48117`:

```bash
cargo run --bin bridge-server
```

Upon starting, `bridge-server` will:
1. Generate (or load) a bridge auth token at `$XDG_STATE_HOME/orbit-ba-bridge/bridge_token` (fallback `~/.local/state/orbit-ba-bridge/bridge_token`).
2. Automatically synchronize `{ "token": "...", "protocol_version": 1 }` into `extension/token.json`.
3. Open or initialize the SQLite store at `$XDG_DATA_HOME/orbit-ba-bridge/conversations.sqlite` (fallback `~/.local/share/orbit-ba-bridge/conversations.sqlite`).
4. Open an interactive command prompt on stdin.

Available server CLI options:

```bash
cargo run --bin bridge-server -- --help
# Options:
#   --port <PORT>             Port to listen on (default 48117)
#   --auth-token <TOKEN>      Explicit authentication token
#   --db-path <PATH>          Path to SQLite database (use ":memory:" for ephemeral)
#   --non-interactive         Disable stdin command prompt
```

---

## CLI Conversation Commands

Inspect, search, and export recorded conversations without starting the WebSocket server:

```bash
# List recorded conversations
cargo run --bin bridge-server -- conversation list

# Show conversation details and messages (truncated snippet)
cargo run --bin bridge-server -- conversation show <CONVERSATION_ID>

# Show conversation details with full message bodies
cargo run --bin bridge-server -- conversation show <CONVERSATION_ID> --full

# Search messages across all conversations
cargo run --bin bridge-server -- conversation search "architecture"

# Export conversation to Markdown
cargo run --bin bridge-server -- conversation export <CONVERSATION_ID> --format markdown --output notes.md

# Export conversation to JSON
cargo run --bin bridge-server -- conversation export <CONVERSATION_ID> --format json --output history.json
```

---

## Interactive Stdin Prompt Commands

When running `bridge-server` interactively:

- `status`: Show bridge listener, browser connection, attached tab, active conversation, and participants (strictly redacting secrets).
- `conversation list` (or `conv list`): List recorded conversations.
- `conversation show <id> [--full]` (or `conv show <id> [--full]`): Inspect conversation turns.
- `messages`: Shorthand to inspect turns in the currently active conversation.
- `conversation export <id> --format <markdown|json> [--output <path>]`: Export conversation.
- `conversation search <query>` (or `conv search <query>`): Search messages.
- `inject sa <message>`: Inject turn as Orbit SA.
- `inject ba_research <message>`: Inject turn as BA Research.
- `help`: Print available commands.
- `quit` | `exit`: Gracefully stop the server.

---

## Manual Acceptance Testing with Real Chromium

Follow these step-by-step instructions to verify the complete browser round-trip:

### Step 1: Start Bridge Server
In terminal 1:
```bash
cargo run --bin bridge-server
```
You will see:
```text
==============================================================
orbit-ba-bridge server
Listening on:           ws://127.0.0.1:48117
SQLite store:           ~/.local/share/orbit-ba-bridge/conversations.sqlite
Token status:           loaded (a1b2...c3d4)
Extension token sync:   extension/token.json
Interactive prompt:     Type 'help' for commands, 'quit' to stop
==============================================================
```

### Step 2: Load Unpacked Extension in Chromium
1. Open Google Chrome (or Brave / Chromium).
2. Navigate to `chrome://extensions/`.
3. Enable **Developer mode** toggle in the top-right corner.
4. Click **Load unpacked** (top left).
5. Select the `orbit-ba-bridge/extension` directory.
6. Verify that "Orbit BA Bridge Extension" appears and is enabled.

### Step 3: Open ChatGPT
1. In the same browser, open `https://chatgpt.com`.
2. Open a new or existing chat.
3. Switch back to your server terminal. You should see:
   - `browser_connected { session_id: "browser-...", ext_version: "0.1.0" }`
   - `page_ready { url: "https://chatgpt.com/c/..." }`
   - `conversation_selected { conversation_id: "...", external_ref: "..." }`

### Step 4: Manual Human Message Test
1. In the ChatGPT browser tab, type:
   `"We are designing the system architecture."` and hit Send.
2. In your server terminal, observe:
   `message_observed { actor_id: "...", sequence: 1, external_id: "..." }`
3. As ChatGPT streams its response, the extension observer debounces the stream.
4. When ChatGPT completes generating, observe:
   `assistant_response_observed { actor_id: "...", sequence: 2, external_id: "..." }`

### Step 5: External Injection Test (Orbit SA)
1. In your server terminal, type:
   ```text
   inject sa Propose separating credential management from driver transport.
   ```
2. Watch the browser tab:
   - The extension receives `inject_message`.
   - The extension populates the ChatGPT composer with:
     `[External participant: Orbit SA]\n\nPropose separating credential management from driver transport.`
   - The send button is clicked automatically.
3. In your server terminal, observe:
   - `injection_accepted { injection_id: "..." }`
   - `injection_materialized { injection_id: "...", external_id: "..." }`
   - `message_observed` with logical sender mapped to `Orbit SA` (SystemArchitect) and sequence 3!
4. When ChatGPT responds to SA's injection, observe `assistant_response_observed` with sequence 4!

### Step 6: Reload Recovery & Deduplication Test
1. In the browser tab, refresh the page (`F5` or `Cmd+R`).
2. The extension reconnects to `ws://127.0.0.1:48117` and re-scans historical DOM turns.
3. In your server terminal:
   - `browser_connected`
   - `conversation_selected`
   - Historical turns are checked against SQLite and logged as `duplicate_message_suppressed`.
   - **No duplicate rows are created** in SQLite.
4. Send another message in ChatGPT:
   `"Proceed with next steps."`
5. Observe that the new message is assigned monotonic sequence 5!

### Step 7: Inspect Persisted Discussion & Export
In the server terminal, type:
```text
status
messages
conversation export <ID> --format markdown --output discussion.md
```
You will see the fully sequenced, actor-attributed conversation log and exported Markdown file.
Type `quit` to cleanly exit the server.
