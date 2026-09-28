/**
 * Background service worker managing WebSocket connectivity to bridge-server.
 */

try {
  importScripts("../shared/browser-compat.js", "../shared/protocol.js");
} catch (e) {
  console.error("[OrbitBridge] Failed to importScripts in background worker:", e);
}

const { browserAPI, PROTOCOL_VERSION, EXTENSION_VERSION, DEFAULT_WS_URL, createEnvelope } =
  globalThis.OrbitBridge;

const SESSION_ID = `ext-${Date.now()}-${Math.random().toString(36).substring(2, 8)}`;
let ws = null;
let isAuthenticated = false;
let reconnectTimer = null;
let reconnectAttempts = 0;
const MAX_RECONNECT_DELAY_MS = 10000;
let cachedAuthToken = null;

async function loadAuthToken() {
  if (cachedAuthToken) return cachedAuthToken;

  // 1. Try chrome.storage.local
  try {
    const stored = await browserAPI.storage.local.get("authToken");
    if (stored && stored.authToken) {
      cachedAuthToken = stored.authToken;
      return cachedAuthToken;
    }
  } catch (_e) {
    // Ignore storage lookup error
  }

  // 2. Try fetching local token.json bundled with extension
  try {
    const url = browserAPI.runtime.getURL("token.json");
    const resp = await fetch(url);
    if (resp.ok) {
      const data = await resp.json();
      if (data && data.token) {
        cachedAuthToken = data.token;
        return cachedAuthToken;
      }
    }
  } catch (_e) {
    // Ignore fetch error if token.json not bundled
  }

  return "default-local-token";
}

async function connectWebSocket() {
  if (ws && (ws.readyState === WebSocket.OPEN || ws.readyState === WebSocket.CONNECTING)) {
    return;
  }

  const token = await loadAuthToken();

  try {
    ws = new WebSocket(DEFAULT_WS_URL);
    isAuthenticated = false;

    ws.onopen = () => {
      reconnectAttempts = 0;
      console.log(`[OrbitBridge] Connected to ${DEFAULT_WS_URL}, sending handshake`);

      // Step 1: Send ClientHello handshake
      const hello = {
        protocol_version: PROTOCOL_VERSION,
        token: token,
        extension_version: EXTENSION_VERSION,
      };
      ws.send(JSON.stringify(hello));
    };

    ws.onmessage = (event) => {
      try {
        const data = JSON.parse(event.data);

        // Check if this is the ServerHelloAck
        if (!isAuthenticated && data.session_id && data.accepted !== undefined) {
          if (data.accepted) {
            isAuthenticated = true;
            console.log(`[OrbitBridge] Handshake accepted by server (session: ${data.session_id})`);
            sendBridgeEvent("connected");
            checkTabsCount();
          } else {
            console.error("[OrbitBridge] Server rejected handshake");
            ws.close();
          }
          return;
        }

        // Regular bridge command envelope
        handleBridgeCommand(data);
      } catch (err) {
        console.error("[OrbitBridge] Failed to parse message:", err);
      }
    };

    ws.onclose = (event) => {
      isAuthenticated = false;
      console.warn(`[OrbitBridge] WebSocket closed (code: ${event.code}), scheduling reconnect`);
      scheduleReconnect();
    };

    ws.onerror = (err) => {
      console.error("[OrbitBridge] WebSocket error:", err);
      ws.close();
    };
  } catch (err) {
    console.error("[OrbitBridge] Connection setup error:", err);
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
  if (!ws || ws.readyState !== WebSocket.OPEN || !isAuthenticated) {
    console.warn("[OrbitBridge] Cannot sendBridgeEvent: WebSocket not ready or not authenticated", {
      type,
      wsReady: ws ? ws.readyState : null,
      isAuthenticated,
    });
    return;
  }

  const correlationId = `corr-${Date.now()}-${Math.random().toString(36).substring(2, 6)}`;
  const envelope = createEnvelope(SESSION_ID, correlationId, type, payload);
  console.log(`[OrbitBridge] Sending event '${type}' over WebSocket`, envelope);
  ws.send(JSON.stringify(envelope));
}

async function checkTabsCount() {
  try {
    const tabs = await browserAPI.tabs.query({
      url: "https://chatgpt.com/*",
    });

    if (tabs && tabs.length > 1) {
      console.warn("[OrbitBridge] Multiple ChatGPT tabs detected:", tabs.length);
      sendBridgeEvent("page_unavailable", {
        reason: "MULTIPLE_CHATGPT_TABS: Keep only one ChatGPT tab open for bridge operation",
      });
      return false;
    }
    return true;
  } catch (_e) {
    return true;
  }
}

async function handleBridgeCommand(envelope) {
  try {
    const tabs = await browserAPI.tabs.query({
      url: "https://chatgpt.com/*",
    });

    if (!tabs || tabs.length === 0) {
      console.warn("[OrbitBridge] No active ChatGPT tab found for command:", envelope);
      sendBridgeEvent("page_unavailable", {
        reason: "No active ChatGPT tab found",
      });
      return;
    }

    if (tabs.length > 1) {
      console.warn("[OrbitBridge] Multiple ChatGPT tabs detected:", tabs.length);
      sendBridgeEvent("page_unavailable", {
        reason: "MULTIPLE_CHATGPT_TABS: Keep only one ChatGPT tab open for bridge operation",
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
          console.warn("[OrbitBridge] Tab communication error:", lastErr.message);
        } else {
          console.log("[OrbitBridge] Tab response:", response);
        }
      }
    );
  } catch (err) {
    console.error("[OrbitBridge] Error handling bridge command:", err);
  }
}

// Forward messages from content scripts to WebSocket
browserAPI.runtime.onMessage.addListener(async (message) => {
  if (message && message.source === "content_script" && message.event) {
    const { type, payload } = message.event;
    console.log(`[OrbitBridge] Forwarding content script event '${type}' to server`, payload);

    const singleTabOk = await checkTabsCount();
    if (!singleTabOk) {
      console.warn(`[OrbitBridge] Blocked event '${type}' due to multiple tabs`);
      return;
    }

    sendBridgeEvent(type, payload);
  }
});

// Tab listeners to react to multiple tabs created or removed
if (browserAPI.tabs && browserAPI.tabs.onCreated) {
  browserAPI.tabs.onCreated.addListener(checkTabsCount);
  browserAPI.tabs.onRemoved.addListener(checkTabsCount);
}

// Start connection on load
connectWebSocket();
