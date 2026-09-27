/**
 * Content script entry point connecting ChatGPT DOM to the background service worker.
 */

import { DomObserver } from "./dom.js";
import { ComposerController } from "./composer.js";

const composer = new ComposerController();

const observer = new DomObserver((event) => {
  chrome.runtime.sendMessage({
    source: "content_script",
    event,
  });
});

observer.start();

chrome.runtime.onMessage.addListener((message, _sender, sendResponse) => {
  if (!message || !message.command) return;

  const { type, payload } = message.command;

  switch (type) {
    case "send_message":
      composer
        .sendMessage(payload.text)
        .then((result) => sendResponse({ success: true, result }))
        .catch((err) => sendResponse({ success: false, error: err.message }));
      return true; // Keep message channel open for async response

    case "request_page_state":
      observer.scanExistingMessages();
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
