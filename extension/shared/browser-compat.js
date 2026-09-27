/**
 * Cross-browser compatibility wrapper for WebExtension APIs.
 *
 * Normalizes `chrome.*` and `browser.*` APIs for Chromium and Firefox.
 */

(function () {
  const browserAPI = globalThis.browser || globalThis.chrome;

  if (!browserAPI) {
    console.error("[OrbitBridge] Neither `chrome` nor `browser` WebExtension API is available.");
  }

  globalThis.OrbitBridge = globalThis.OrbitBridge || {};
  globalThis.OrbitBridge.browserAPI = browserAPI;
})();
