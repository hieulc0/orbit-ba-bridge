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
    const sendToTab = (tabId, retriesLeft = 1) => {
      browserAPI.tabs.sendMessage(
        tabId,
        { command: { type: envelope.type, payload: envelope.payload } },
        (response) => {
          const lastErr = browserAPI.runtime.lastError;
          if (lastErr) {
            if (retriesLeft > 0 && lastErr.message.includes("Receiving end does not exist")) {
              setTimeout(() => sendToTab(tabId, retriesLeft - 1), 500);
            } else {
              console.warn("[OrbitBridge] Tab communication error:", lastErr.message);
            }
          } else {
            console.log("[OrbitBridge] Tab response:", response);
          }
        }
      );
    };

    sendToTab(targetTab.id);
  } catch (err) {
    console.error("[OrbitBridge] Error handling bridge command:", err);
  }
}
