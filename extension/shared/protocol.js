/**
 * Protocol constants and envelope formatting for Orbit BA Bridge.
 */
export const PROTOCOL_VERSION = 1;
export const DEFAULT_WS_URL = "ws://127.0.0.1:48117";

/**
 * Creates a versioned message envelope matching the Rust `MessageEnvelope<T>`.
 *
 * @param {string} sessionId
 * @param {string} correlationId
 * @param {string} type - Event or command type tag
 * @param {object} [payload] - Optional inner payload fields
 * @returns {object} Formatted envelope object
 */
export function createEnvelope(sessionId, correlationId, type, payload = undefined) {
  const envelope = {
    version: PROTOCOL_VERSION,
    session_id: sessionId,
    correlation_id: correlationId,
    type,
  };

  if (payload !== undefined && payload !== null) {
    envelope.payload = payload;
  }

  return envelope;
}
