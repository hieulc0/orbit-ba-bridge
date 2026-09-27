/**
 * Protocol constants and envelope formatting for Orbit BA Bridge.
 */

(function () {
  const PROTOCOL_VERSION = 1;
  const EXTENSION_VERSION = "0.1.0";
  const DEFAULT_WS_URL = "ws://127.0.0.1:48117";

  /**
   * Creates a versioned message envelope matching the Rust `MessageEnvelope<T>`.
   *
   * @param {string} sessionId
   * @param {string} correlationId
   * @param {string} type - Event or command type tag
   * @param {object} [payload] - Optional inner payload fields
   * @returns {object} Formatted envelope object
   */
  function createEnvelope(sessionId, correlationId, type, payload) {
    const envelope = {
      version: PROTOCOL_VERSION,
      session_id: sessionId,
      correlation_id: correlationId,
      type: type,
    };

    if (payload !== undefined && payload !== null) {
      envelope.payload = payload;
    }

    return envelope;
  }

  globalThis.OrbitBridge = globalThis.OrbitBridge || {};
  globalThis.OrbitBridge.PROTOCOL_VERSION = PROTOCOL_VERSION;
  globalThis.OrbitBridge.EXTENSION_VERSION = EXTENSION_VERSION;
  globalThis.OrbitBridge.DEFAULT_WS_URL = DEFAULT_WS_URL;
  globalThis.OrbitBridge.createEnvelope = createEnvelope;
})();
