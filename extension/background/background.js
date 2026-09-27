/**
 * Background service worker managing WebSocket connectivity to bridge-server.
 */

try {
  importScripts("../shared/browser-compat.js", "../shared/protocol.js");
} catch (e) {
  console.error("[OrbitBridge] Failed to importScripts in background worker:", e);
}

const { browserAPI, DEFAULT_WS_URL, createEnvelope } = globalThis.OrbitBridge;

const SESSION_ID = `ext-${Date.now()}-${Math.random().toString(36).substring(2, 8)}`;
let ws = null;
let reconnectTimer = null;
let reconnectAttempts = 0;
const MAX_RECONNECT_DELAY_MS = 10000;

function connectWebSocket() {
  if (ws && (ws.readyState === WebSocket.OPEN || ws.readyState === WebSocket.CONNECTING)) {
    return;
  }

  try {
    ws = new WebSocket(DEFAULT_WS_URL);

    ws.onopen = () => {
      reconnectAttempts = 0;
      console.log(`[OrbitBridge] Connected to ${DEFAULT_WS_URL}`);
      sendBridgeEvent("connected");
    };

    ws.onmessage = (event) => {
      try {
        const envelope = JSON.parse(event.data);
        handleBridgeCommand(envelope);
      } catch (err) {
        console.error("[OrbitBridge] Failed to parse message:", err);
      }
    };

    ws.onclose = () => {
      console.warn("[OrbitBridge] WebSocket connection closed, scheduling reconnect");
      scheduleReconnect();
    };

    ws.onerror = (err) => {
      console.error("[OrbitBridge] WebSocket error:", err);
      ws.close();
    };
  } catch (err) {
    console.error("[OrbitBridge] Connection error:", err);
    scheduleReconnect();
  }
}

function scheduleReconnect() {
  if (reconnectTimer) return;

  const delay = Math.min(1000 * Math.pow(1.5, reconnectAttempts), MAX_RECONNECT_DELAY_MS);
  reconnectAttempts++;

  reconnectTimer = setTimeout(() => {
    reconnectTimer = null;
    connectWebSocket();
  }, delay);
}

function sendBridgeEvent(type, payload = undefined) {
  if (!ws || ws.readyState !== WebSocket.OPEN) {
    return;
  }

  const correlationId = `corr-${Date.now()}-${Math.random().toString(36).substring(2, 6)}`;
  const envelope = createEnvelope(SESSION_ID, correlationId, type, payload);
  ws.send(JSON.stringify(envelope));
}

async function handleBridgeCommand(envelope) {
  try {
    const tabs = await browserAPI.tabs.query({
      url: "https://chatgpt.com/*",
    });

    if (!tabs || tabs.length === 0) {
      console.warn("[OrbitBridge] No active ChatGPT tab found for command:", envelope);
      sendBridgeEvent("session_unavailable", {
        reason: "No active ChatGPT tab found",
      });
      return;
    }

    const targetTab = tabs.find((t) => t.active) || tabs[0];
    browserAPI.tabs.sendMessage(
      targetTab.id,
      { command: { type: envelope.type, payload: envelope.payload } },
      (response) => {
        const lastErr = browserAPI.runtime.lastError;
        if (lastErr) {
          console.warn("[OrbitBridge] Tab error:", lastErr.message);
        } else {
          console.log("[OrbitBridge] Tab response:", response);
        }
      }
    );
  } catch (err) {
    console.error("[OrbitBridge] Error handling bridge command:", err);
  }
}

// Listen for messages from content scripts
browserAPI.runtime.onMessage.addListener((message) => {
  if (message && message.source === "content_script" && message.event) {
    const { type, payload } = message.event;
    sendBridgeEvent(type, payload);
  }
});

// Start connection on load
connectWebSocket();
