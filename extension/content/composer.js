/**
 * Composer controller for ChatGPT prompt input and submission.
 */

(function () {
  class ComposerController {
    findPromptInput() {
      return (
        document.querySelector("#prompt-textarea") ||
        document.querySelector("textarea") ||
        document.querySelector("div[contenteditable='true']")
      );
    }

    findSendButton() {
      return (
        document.querySelector("button[data-testid='send-button']") ||
        document.querySelector("button[aria-label='Send prompt']") ||
        document.querySelector("form button[type='submit']")
      );
    }

    async sendMessage(text) {
      const input = this.findPromptInput();
      if (!input) {
        throw new Error("ChatGPT composer prompt textarea not found");
      }

      input.focus();

      if (input.tagName.toLowerCase() === "textarea") {
        input.value = text;
        input.dispatchEvent(new Event("input", { bubbles: true }));
        input.dispatchEvent(new Event("change", { bubbles: true }));
      } else {
        // Contenteditable div
        input.innerText = text;
        input.dispatchEvent(new InputEvent("input", { bubbles: true }));
      }

      // Allow UI time to process input event and enable send button
      await new Promise((resolve) => setTimeout(resolve, 150));

      const sendButton = this.findSendButton();
      if (!sendButton || sendButton.disabled) {
        // Fallback: dispatch Enter key event on input
        const enterEvent = new KeyboardEvent("keydown", {
          key: "Enter",
          code: "Enter",
          keyCode: 13,
          which: 13,
          bubbles: true,
        });
        input.dispatchEvent(enterEvent);
        return { status: "sent_via_enter_key" };
      }

      sendButton.click();
      return { status: "sent_via_button_click" };
    }
  }

  globalThis.OrbitBridge = globalThis.OrbitBridge || {};
  globalThis.OrbitBridge.ComposerController = ComposerController;
})();
