/**
 * Content script entry point connecting ChatGPT DOM to the background service worker.
 */

(function () {
  const { browserAPI, DomObserver, ComposerController } = globalThis.OrbitBridge;

  // Cleanly teardown any prior observer if extension reloaded
  if (globalThis.__orbitBridgeCurrentObserver) {
    try {
      globalThis.__orbitBridgeCurrentObserver.stop();
    } catch (_e) {}
  }

  const composer = new ComposerController();

  const observer = new DomObserver((event) => {
    browserAPI.runtime.sendMessage({
      source: "content_script",
      event,
    });
  });

  globalThis.__orbitBridgeCurrentObserver = observer;
  observer.start();

  browserAPI.runtime.onMessage.addListener((message, _sender, sendResponse) => {
    if (!message || !message.command) return;

    const { type, payload } = message.command;

    switch (type) {
      case "inject_message": {
        browserAPI.runtime.sendMessage({
          source: "content_script",
          event: {
            type: "injection_accepted",
            payload: { injection_id: payload.injection_id },
          },
        });

        observer.setPendingInjection({
          injection_id: payload.injection_id,
          correlation_id: payload.correlation_id,
          text: payload.text,
          timestamp: Date.now(),
        });

        composer
          .sendMessage(payload.text)
          .then((result) => sendResponse({ success: true, result }))
          .catch((err) => sendResponse({ success: false, error: err.message }));

        return true;
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
