# Orbit BA Bridge Protocol

## Connection Handshake

Before any application events or commands are processed, the client and server complete an authentication handshake over WebSocket:

```text
Extension (Client)                           bridge-server
        │                                           │
        │─── ClientHello (token, versions) ────────>│
        │                                           │ [validates token & version]
        │<── ServerHelloAck (session_id, ok) ───────│
        │                                           │
```

### Handshake Frames

**ClientHello**:
```json
{
  "protocol_version": 1,
  "token": "3a7c...token...9f",
  "extension_version": "0.1.0"
}
```

**ServerHelloAck**:
```json
{
  "protocol_version": 1,
  "session_id": "browser-54123",
  "accepted": true
}
```

If the handshake token does not match or protocol version differs from `CURRENT_PROTOCOL_VERSION` (1), `bridge-server` closes the connection with code `1008` (`PolicyViolation`) and an explanatory reason frame.

---

## Message Wire Envelope

All subsequent communication between the browser extension and `bridge-server` uses the versioned JSON envelope:

```json
{
  "version": 1,
  "session_id": "browser-54123",
  "correlation_id": "corr-1710000000-xyz",
  "type": "assistant_message_observed",
  "payload": {
    "external_message_id": "turn-0-analysis",
    "text": "Analysis of requirement brief.",
    "is_final": true
  }
}
```

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
      │<── InjectionAccepted(I42) ───│<── runtime.sendMessage() ─────│                          │
      │                              │                               │─── click send button ───>│
      │                              │                               │                          │ [turn renders]
      │                              │                               │<── DOM MutationObserver ─│
      │                              │<── InjectionMaterialized ─────│                          │
      │<── InjectionMaterialized ────│                               │                          │
      │    (I42 -> ext_id: M123)     │                               │                          │
      │                              │<── UserMessageObserved ───────│                          │
      │<── UserMessageObserved ──────│                               │                          │
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
