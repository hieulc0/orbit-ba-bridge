/**
 * Content script entry point connecting ChatGPT DOM to the background service worker.
 */

(function () {
  const { browserAPI, DomObserver, ComposerController } = globalThis.OrbitBridge;

  const composer = new ComposerController();

  const observer = new DomObserver((event) => {
    browserAPI.runtime.sendMessage({
      source: "content_script",
      event,
    });
  });

  observer.start();

  browserAPI.runtime.onMessage.addListener((message, _sender, sendResponse) => {
    if (!message || !message.command) return;

    const { type, payload } = message.command;

    switch (type) {
      case "inject_message": {
        // 1. Immediately acknowledge acceptance of injection
        browserAPI.runtime.sendMessage({
          source: "content_script",
          event: {
            type: "injection_accepted",
            payload: { injection_id: payload.injection_id },
          },
        });

        // 2. Set pending injection tracking for DOM materialization
        observer.setPendingInjection({
          injection_id: payload.injection_id,
          correlation_id: payload.correlation_id,
          text: payload.text,
          timestamp: Date.now(),
        });

        // 3. Inject text through composer
        composer
          .sendMessage(payload.text)
          .then((result) => sendResponse({ success: true, result }))
          .catch((err) => sendResponse({ success: false, error: err.message }));

        return true; // Keep response channel open for async execution
      }

      case "send_message":
        composer
          .sendMessage(payload.text)
          .then((result) => sendResponse({ success: true, result }))
          .catch((err) => sendResponse({ success: false, error: err.message }));
        return true;

      case "request_page_state":
        observer.scanMessages();
        sendResponse({ success: true });
        break;

      case "ping":
        sendResponse({ success: true, pong: true });
        break;

      default:
        sendResponse({ success: false, error: `Unknown command: ${type}` });
        break;
    }
  });
})();
