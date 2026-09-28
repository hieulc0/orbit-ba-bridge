/**
 * Background service worker / event page managing WebSocket connectivity to bridge-server.
 */

if (typeof importScripts === "function") {
  try {
    importScripts("../shared/browser-compat.js", "../shared/protocol.js");
  } catch (e) {
    console.error("[OrbitBridge] Failed to importScripts in background worker:", e);
  }
}

const { browserAPI, PROTOCOL_VERSION, EXTENSION_VERSION, DEFAULT_WS_URL, createEnvelope } =
  globalThis.OrbitBridge;
