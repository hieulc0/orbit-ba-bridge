/**
 * Background service worker managing WebSocket connectivity to bridge-server.
 */

import { DEFAULT_WS_URL, createEnvelope } from "../shared/protocol.js";

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
      console.log(`[Bridge Extension] Connected to ${DEFAULT_WS_URL}`);
      sendBridgeEvent("connected");
    };

    ws.onmessage = (event) => {
      try {
        const envelope = JSON.parse(event.data);
        handleBridgeCommand(envelope);
      } catch (err) {
        console.error("[Bridge Extension] Failed to parse message:", err);
      }
    };

    ws.onclose = () => {
      console.warn("[Bridge Extension] WebSocket connection closed, scheduling reconnect");
      scheduleReconnect();
    };

    ws.onerror = (err) => {
      console.error("[Bridge Extension] WebSocket error:", err);
      ws.close();
    };
  } catch (err) {
    console.error("[Bridge Extension] Connection error:", err);
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
  const tabs = await chrome.tabs.query({
    url: "https://chatgpt.com/*",
  });

  if (tabs.length === 0) {
    console.warn("[Bridge Extension] No active ChatGPT tab found for command:", envelope);
    sendBridgeEvent("session_unavailable", {
      reason: "No active ChatGPT tab found",
    });
    return;
  }

  const targetTab = tabs.find((t) => t.active) || tabs[0];
  chrome.tabs.sendMessage(
    targetTab.id,
    { command: { type: envelope.type, payload: envelope.payload } },
    (response) => {
      if (chrome.runtime.lastError) {
        console.warn("[Bridge Extension] Tab error:", chrome.runtime.lastError.message);
      } else {
        console.log("[Bridge Extension] Tab response:", response);
      }
    }
  );
}

// Listen for messages from content scripts
chrome.runtime.onMessage.addListener((message) => {
  if (message && message.source === "content_script" && message.event) {
    const { type, payload } = message.event;
    sendBridgeEvent(type, payload);
  }
});

// Start connection on load
connectWebSocket();
