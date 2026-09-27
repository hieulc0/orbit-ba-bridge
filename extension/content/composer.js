/**
 * Composer controller for ChatGPT prompt input and submission.
 */

(function () {
  class ComposerController {
    findPromptInput() {
      return (
        document.querySelector("#prompt-textarea") ||
        document.querySelector("div[contenteditable='true']#prompt-textarea") ||
        document.querySelector("div[contenteditable='true']") ||
        document.querySelector("textarea#prompt-textarea") ||
        document.querySelector("textarea")
      );
    }

    findSendButton() {
      return (
        document.querySelector("button[data-testid='send-button']") ||
        document.querySelector("button[aria-label='Send prompt']") ||
        document.querySelector("button[aria-label='Send Message']") ||
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
        // Modern ChatGPT contenteditable div
        // Use insertText if possible to respect React's internal text buffer
        const success = document.execCommand("insertText", false, text);
        if (!success || input.innerText.trim() !== text.trim()) {
          input.innerText = text;
          input.dispatchEvent(new InputEvent("input", { bubbles: true }));
        }
      }

      // Allow UI time to process input event and enable the send button
      await new Promise((resolve) => setTimeout(resolve, 250));

      const sendButton = this.findSendButton();
      if (sendButton && !sendButton.disabled) {
        sendButton.click();
        return { status: "sent_via_button_click" };
      }

      // Fallback: dispatch Enter key event
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
  }

  globalThis.OrbitBridge = globalThis.OrbitBridge || {};
  globalThis.OrbitBridge.ComposerController = ComposerController;
})();
