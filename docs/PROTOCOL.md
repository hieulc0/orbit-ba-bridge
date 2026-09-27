# Orbit BA Bridge Protocol

## Message Wire Envelope

All messages between the browser extension and `bridge-server` use a typed JSON envelope:

```json
{
  "version": 1,
  "session_id": "ext-1710000000-abc",
  "correlation_id": "corr-1710000000-xyz",
  "type": "assistant_message",
  "payload": {
    "message_id": "msg-001",
    "text": "Analysis of requirement brief."
  }
}
```

### Envelope Fields

| Field | Type | Description |
|---|---|---|
| `version` | `u16` | Protocol version (`1`). Unsupported versions are rejected. |
| `session_id` | `string` | Unique identifier for the client session. |
| `correlation_id` | `string` | Unique identifier used to correlate request/response pairs. |
| `type` | `string` | Discriminator tag for event or command. |
| `payload` | `object` | Optional variant-specific arguments. |

## Browser Events (`BrowserEvent`)

Events sent from browser extension to the bridge:

- `connected`: Extension connected to bridge WebSocket.
- `page_ready`: ChatGPT tab finished initial rendering.
- `conversation_detected`: Active conversation identified, contains optional `conversation_id`.
- `assistant_message`: Assistant reply observed in DOM (`message_id`, `text`).
- `user_message`: User prompt observed in DOM (`message_id`, `text`).
- `session_unavailable`: ChatGPT session lost or tab closed (`reason`).

## Bridge Commands (`BrowserCommand`)

Commands sent from the bridge to browser extension:

- `send_message`: Dispatches text to prompt composer and triggers submission (`text`).
- `ping`: Health-check probe.
- `request_page_state`: Prompts DOM observer to rescan active messages.

## Size Constraints

- `MAX_PAYLOAD_BYTES`: 256 KiB
- `MAX_MESSAGE_TEXT_LENGTH`: 128 KiB
