# Security Policy & Invariants

## Localhost-Only Default Bind

- The bridge listener binds strictly to `127.0.0.1` by default.
- It must **never** bind to `0.0.0.0` or open external network interfaces without authentication.

## Localhost Session Handshake & Token Authentication

### Why Handshake Authentication Exists
On modern operating systems, arbitrary local processes, non-sandboxed scripts, or malicious web pages running on `localhost` (e.g. via cross-site WebSocket hijacking from a local web server or malicious dev server) could theoretically attempt to connect to `ws://127.0.0.1:48117`.

Without an authentication handshake, an unauthorized local page or script could inject arbitrary prompts into the user's logged-in ChatGPT session or snoop on private discussion history.

### How Handshake & Token Management Work
1. **Token Generation & Storage**:
   - On startup, `bridge-server` resolves its bridge auth token.
   - If `--auth-token <TOKEN>` is provided via CLI, it is used directly.
   - Otherwise, the server looks for `--token-file <PATH>` or `target/bridge_token`. If missing, it generates a high-entropy cryptographically random UUID token and saves it to the token file.
   - For developer convenience, `bridge-server` automatically writes `{ "token": "<TOKEN>", "protocol_version": 1 }` into `extension/token.json`.
2. **Handshake Sequence**:
   - Upon opening the WebSocket connection, the browser extension immediately issues a `ClientHello` frame:
     ```json
     {
       "protocol_version": 1,
       "token": "<TOKEN>",
       "extension_version": "0.1.0"
     }
     ```
   - `bridge-server` validates the token against its active server token.
   - If the token does not match, or if `protocol_version != CURRENT_PROTOCOL_VERSION` (1), the server terminates the connection immediately with WebSocket close code `1008` (`PolicyViolation`).
   - If valid, the server replies with `ServerHelloAck`:
     ```json
     {
       "protocol_version": 1,
       "session_id": "browser-54321",
       "accepted": true
     }
     ```
   - Only after receiving `ServerHelloAck` does the extension begin streaming DOM events or processing bridge commands.

## Browser Security Boundary

- The extension does not store API keys or provider credentials.
- No session cookies or browser storage secrets are exported to the bridge.
- Extension permissions are strictly restricted to `https://chatgpt.com/*`, localhost storage, and localhost WebSocket communication.

## Input Validation & Rate Limiting

- Incoming message envelopes are strictly checked against `CURRENT_PROTOCOL_VERSION`.
- Message text length is bounded (`MAX_MESSAGE_TEXT_LENGTH = 128 KiB`).
- Total payload size is bounded (`MAX_PAYLOAD_BYTES = 256 KiB`).
- Memory used for deduplication history is bounded to avoid memory exhaustion via unbounded sets.

## Operational Logging

- Log structural events (connection state, IDs, payload sizes) rather than raw prompt/response contents by default.
- Never log authorization headers, cookies, or secrets.
