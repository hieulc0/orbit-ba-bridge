# Orbit BA Bridge Protocol

## Connection Handshake

Before any application events or commands are processed, the client and server complete an authentication handshake over WebSocket:

```text
Extension (Client)                           bridge-server
        │                                           │
        │─── ClientHello (token, versions) ────────>│
        │                                           │ [validates token & version]
        │<─── ServerHelloAck (session_id, ok) ──────│
        │                                           │
```

### Handshake Frames

**ClientHello**:
```json
{\n  \"protocol_version\": 1,\n  \"token\": \"3a7c...token...9f\",\n  \"extension_version\": \"0.1.0\"\n}\n```

**ServerHelloAck**:
```json
{\n  \"protocol_version\": 1,\n  \"session_id\": \"browser-54123\",\n  \"accepted\": true\n}\n```

If the handshake token does not match or protocol version differs from `CURRENT_PROTOCOL_VERSION` (1), `bridge-server` closes the connection with code `1008` (`PolicyViolation`) and an explanatory `BridgeErrorCode` reason frame.

---

## Standardized Error Codes (`BridgeErrorCode`)

The bridge defines standard error codes across transport, session, and storage layers:

| Error Code | Layer | Description |
|---|---|---|
| `BROWSER_NOT_CONNECTED` | Transport | Extension has not connected or connection dropped |
| `CHATGPT_PAGE_NOT_READY` | Browser | Content script not yet attached or DOM prompt area not found |
| `MULTIPLE_CHATGPT_TABS` | Browser / Tab | More than one ChatGPT tab open; routing ambiguous |
| `CONVERSATION_NOT_FOUND` | Store | Requested conversation ID does not exist |
| `EXTERNAL_CONVERSATION_CHANGED` | Browser / Conv | URL changed to another conversation |
| `COMPOSER_UNAVAILABLE` | Browser | Input textarea disabled, blocked, or missing |
| `DATABASE_ERROR` | Store | SQLite error, schema corruption, or query failure |
| `AUTH_FAILED` | Transport / Security | Invalid or missing bridge token |
| `PROTOCOL_VERSION_MISMATCH` | Transport | Unsupported client/server protocol version |
| `UNSUPPORTED_BRANCH_MUTATION` | Core / Store | Historical turn was edited or regenerated externally |

---

## Multi-Tab Isolation & Policy

The bridge strictly enforces a single controlled ChatGPT tab per browser session:

1. **Extension Detection**: The background service worker tracks tabs matching `https://chatgpt.com/*`. If `tabs.length > 1`, it immediately fires `page_unavailable` with reason `MULTIPLE_CHATGPT_TABS: Keep only one ChatGPT tab open for bridge operation` and suppresses further DOM observations.
2. **Server Enforcement**: If a second WebSocket connection is attempted while an existing browser session is attached, `bridge-server` rejects the incoming connection with close code `1008` and reason `MULTIPLE_CHATGPT_TABS`.
3. **Recovery**: When extra tabs are closed, the extension detects single-tab state and normal operations resume automatically.

---

## Message Wire Envelope

All subsequent communication between the browser extension and `bridge-server` uses the versioned JSON envelope:

```json
{\n  \"version\": 1,\n  \"session_id\": \"browser-54123\",\n  \"correlation_id\": \"corr-1710000000-xyz\",\n  \"type\": \"assistant_message_observed\",\n  \"payload\": {\n    \"external_message_id\": \"turn-0-analysis\",\n    \"text\": \"Analysis of requirement brief.\",\n    \"is_final\": true\n  }\n}\n```

### Envelope Fields

| Field | Type | Description |
|---|---|---|
| `version` | `u16` | Protocol version (`1`). Unsupported versions are rejected. |
| `session_id` | `string` | Unique identifier for the client session. |
| `correlation_id` | `string` | Unique identifier used to correlate request/response pairs. |
| `type` | `string` | Discriminator tag for event or command (`snake_case`). |
| `payload` | `object` | Variant-specific arguments. |

---

## Browser Events (`BrowserEvent`)

Events sent from the browser extension to the bridge:

- `hello`: Alternative envelope format for the initial handshake (`protocol_version`, `token`, `extension_version`).
- `connected`: Extension connected and ready for traffic.
- `page_ready`: ChatGPT tab finished initial rendering (`external_url`).
- `conversation_detected`: Active conversation identified (`conversation_id`, `external_conversation_ref`).
- `injection_accepted`: Extension confirmed receipt and scheduled insertion of an external turn (`injection_id`).
- `injection_materialized`: The injected text appeared in the ChatGPT conversation DOM (`injection_id`, `external_message_id`).
- `user_message_observed`: A user prompt appeared in DOM (`external_message_id`, `text`).
- `assistant_message_observed`: An assistant response completed in DOM (`external_message_id`, `text`, `is_final`).
- `page_unavailable`: ChatGPT session lost, tab closed, or navigate away (`reason`).
- `disconnected`: Client closing connection gracefully.

---

## Bridge Commands (`BrowserCommand`)

Commands sent from the bridge to the browser extension:

- `hello_ack`: Handshake acknowledgement (`protocol_version`, `session_id`).
- `inject_message`: Directs extension to insert an external turn into composer (`injection_id`, `correlation_id`, `text`).
- `send_message`: Dispatches plain text to composer and submits (`text`).
- `request_page_state`: Prompts DOM observer to rescan active messages.
- `ping`: Health-check probe.

---

## Exact Lifecycle Sequences

### 1. Injected External Actor Turn (e.g. Orbit SA)
```text
bridge-server              Extension Background           Extension Content Script         ChatGPT DOM
      │                              │                               │                          │
      │─── InjectMessage(I42) ──────>│                               │                          │
      │                              │─── tabs.sendMessage() ───────>│                          │
      │                              │                               │─── InjectionAccepted ────> [composer insert]
      │<─── InjectionAccepted(I42) ───│<─── runtime.sendMessage() ─────│                          │
      │                              │                               │─── click send button ───>│
      │                              │                               │                          │ [turn renders]
      │                              │                               │<─── DOM MutationObserver ─│
      │                              │<─── InjectionMaterialized ─────│                          │
      │<─── InjectionMaterialized ────│                               │                          │
      │    (I42 -> ext_id: M123)     │                               │                          │
      │                              │<─── UserMessageObserved ───────│                          │
      │<─── UserMessageObserved ──────│                               │                          │
      │    (ext_id: M123)            │                               │                          │
      │                              │                               │                          │
 [bridge links M123 to Orbit SA]     │                               │                          │
```

### 2. Manual Human User Turn
```text
Human User                   ChatGPT DOM            Extension Content Script              bridge-server
     │                            │                             │                              │
     │─── types & hits send ─────>│                             │                              │
     │                            │─── MutationObserver ───────>│                              │
     │                            │                             │─── UserMessageObserved ─────>│
     │                            │                             │    (ext_id: M124)            │
     │                            │                             │                              │
     │                            │                             │               [ledger lookup: None -> Human]
     │                            │                             │               [persists with sequence #]
```

### 3. Streaming Assistant Response Completion
```text
ChatGPT Model                ChatGPT DOM            Extension Content Script              bridge-server
     │                            │                             │                              │
     │─── streaming tokens ──────>│ [stop button visible]       │                              │
     │                            │ [class: .result-streaming]  │ (debounces, holds event)     │
     │─── generation finishes ───>│ [stop button gone]          │                              │
     │                            │ [copy button appears]       │─── AssistantMessageObserved ─>│
     │                            │                             │    (is_final: true)          │
     │                            │                             │                              │
     │                            │                             │               [dedup check: new]
     │                            │                             │               [persists as active BA]
```

---

## Size Constraints

- `MAX_PAYLOAD_BYTES`: 256 KiB
- `MAX_MESSAGE_TEXT_LENGTH`: 128 KiB
