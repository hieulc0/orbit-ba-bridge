# Architecture Overview

`orbit-ba-bridge` connects external Business Analyst-style reasoning sessions, initially ChatGPT running in an interactive browser session, to Orbit-managed agent development workflows.

## System Topology

```text
ChatGPT Web (Browser)
       │
Extension Content Scripts (DOM observer & composer controller)
       │
Extension Background Worker (Service Worker / Event Page)
       │
Localhost WebSocket (`ws://127.0.0.1:48117` with auth handshake)
       │
bridge-server (Rust CLI & runtime, connection loop, interactive injection CLI)
       │
bridge-core (Driver abstractions, session management, routing, multi-actor conversations, SQLite store)
       │
protocol (Wire types, versioned envelopes, domain identifiers, validation)
       │
Orbit ACP (Future milestone)
```

## Core Invariants

1. **Orbit Authority**: Orbit remains the single workflow authority for task status, role execution, verification, and code mutation. The bridge does not become a second workflow engine.
2. **Provider Neutrality**: Core protocol and driver abstractions do not depend on ChatGPT-specific DOM concepts.
3. **Local-Only Transport with Session Handshake**: WebSocket listener binds to `127.0.0.1` by default. Every connection must complete a token authentication handshake to prevent arbitrary local processes from controlling the session.
4. **Driver Boundary**: All browser interactions pass through the `BrowserDriver` trait, allowing `ExtensionDomDriver`, `MockBrowserDriver`, and future `HeadlessChromiumDriver` to share the same domain and routing logic.
5. **No Repository Mutation by BA**: BA reasoning roles can challenge, analyze, and propose requirements, but have no direct repository write or command execution access.
6. **Multi-Actor Independence**: `ActorRole` is strictly separated from `Participant` and `ParticipantSource`. A conversation supports multiple instances of the same role (e.g. `BA Product` and `BA Research`).
7. **Authoritative Sender vs Browser Role**: ChatGPT Web natively exposes only `user` and `assistant`. Injected external turns (e.g. `Orbit SA`) submit through the user composer, but the bridge maintains the authoritative logical sender (`sa:orbit`) via an injection ledger rather than overwriting it as a human message.
8. **Monotonic Message Sequence**: Every conversation enforces a strictly increasing monotonic sequence for message ordering.
9. **Durable Discussion vs Model Context**: Full discussion history is persisted in SQLite, but each actor turn receives only a bounded projection (`ContextProjection`).
10. **Structured Artifact Promotion**: Casual chat stays local to the bridge; only validated engineering artifacts (`RequirementBrief`, `TechnicalProposal`, `Challenge`, `Resolution`, `AcceptanceDecision`) are promoted to Orbit.
11. **Deduplication Invariant**: Every observed DOM turn is keyed by `external_message_id`. If already persisted for the active conversation, it is acknowledged as duplicate and discarded without altering monotonic sequence numbering.
12. **Reload Recovery Invariant**: When a user refreshes ChatGPT (`F5` / `Cmd+R`), the extension reconnects, re-detects the conversation from the URL, and re-scans the DOM. The bridge matches the existing `external_conversation_ref` in SQLite and ignores historical messages already in store, resuming seamless operation for subsequent turns.

---

## DOM Extraction Strategy & Known Selectors

Modern ChatGPT Web (`https://chatgpt.com`) uses dynamic React DOM structures. The extension relies on prioritized selector fallbacks:

### Conversation URL & Identification
- Path matching: `window.location.pathname.match(/\/c\/([a-zA-Z0-9-]+)/)`
- Yields the native ChatGPT conversation UUID, mapped to `external_conversation_ref`.

### Composer Prompt Input
Priority order:
1. `div#prompt-textarea[contenteditable="true"]` (Modern ChatGPT rich-text editor)
2. `textarea#prompt-textarea` (Fallback textarea)
3. `div[contenteditable="true"]`
4. `textarea`

*Text injection technique*: When `contenteditable` is active, the extension executes `document.execCommand("insertText", false, text)` or updates `innerText` with an `InputEvent` so React's internal state machine detects the input.

### Send Button & Trigger
Priority order:
1. `button[data-testid="send-button"]`
2. `button[aria-label="Send prompt"]`
3. `button[aria-label="Send Message"]`
4. `form button[type="submit"]`
5. *Fallback*: KeyboardEvent `Enter` (`keyCode: 13`) dispatched to the focused input.

### Message Turns & Role Detection
Elements:
- `article`
- `[data-message-author-role]`
- `[data-testid^="conversation-turn-"]`

Role extraction:
- `el.getAttribute("data-message-author-role")`
- `el.querySelector("[data-message-author-role='assistant']")` -> `assistant`
- `el.querySelector("[data-message-author-role='user']")` -> `user`

Text extraction:
- `el.querySelector(".markdown")`
- `el.querySelector(".whitespace-pre-wrap")`
- `el.innerText.trim()`

### Streaming & Completion Detection
ChatGPT streams assistant responses token-by-token. Emitting events during streaming would spam the database with incomplete partial turns. Generation is detected as in-progress if:
- `button[data-testid="stop-button"]` is in DOM
- `button[aria-label="Stop generating"]` is in DOM
- `button[aria-label="Stop streaming"]` is in DOM
- `.result-streaming` class is present on any turn element

The extension content script debounces streaming updates (800ms quiet window after stop button disappearance) before emitting `assistant_message_observed` with `is_final: true`.

---

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

---

## Crate Responsibilities

- **`crates/protocol`**: Stable wire format, `MessageEnvelope<T>`, `BrowserEvent`, `BrowserCommand`, `ClientHello`, `ServerHelloAck`, `InjectionId`, domain identifiers (`SessionId`, `CorrelationId`, `TaskId`, `ConversationId`, `ParticipantId`, `MessageId`, `ArtifactId`), and bounds validation.
- **`crates/bridge-core`**:
  - `BrowserDriver` trait & `MockBrowserDriver`
  - `SessionManager` & `SessionState`
  - Multi-actor model: `ActorRole`, `Participant`, `ParticipantSource`
  - Discussion model: `Conversation`, `ConversationMessage`, `MessageKind`
  - `InjectionLedger`: Tracks pending injections, acceptance, materialization, and logical sender reconciliation
  - Artifact reference definitions & context projection
  - `ConversationStore` trait and `SqliteConversationStore` implementation
- **`crates/bridge-server`**: Executable CLI, configuration, token management, `tokio-tungstenite` WebSocket listener on localhost, interactive CLI stdin prompt for manual injection, logging via `tracing`, and graceful shutdown handling.
