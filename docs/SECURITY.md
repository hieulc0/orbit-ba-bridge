# Security Policy & Invariants

## Localhost-Only Default Bind

- The bridge listener binds strictly to `127.0.0.1` by default.
- It must **never** bind to `0.0.0.0` or open external network interfaces without authentication.

## Browser Security

- The extension does not store API keys or provider credentials.
- No session cookies or browser storage secrets are exported to the bridge.
- Extension permissions are strictly restricted to `https://chatgpt.com/*` and localhost communication.

## Input Validation & Rate Limiting

- Incoming message envelopes are strictly checked against `CURRENT_PROTOCOL_VERSION`.
- Message text length is bounded (`MAX_MESSAGE_TEXT_LENGTH = 128 KiB`).
- Total payload size is bounded (`MAX_PAYLOAD_BYTES = 256 KiB`).
- Memory used for deduplication history is bounded to avoid denial-of-service via unbounded sets.

## Operational Logging

- Log structural events (connection state, IDs, payload sizes) rather than raw prompt/response contents by default.
- Never log authorization headers, cookies, or secrets.
