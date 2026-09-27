/**
 * DOM observer and message extraction for ChatGPT web interface.
 */

export class DomObserver {
  constructor(onEvent) {
    this.onEvent = onEvent;
    this.seenMessageIds = new Set();
    this.observer = null;
    this.currentConversationId = null;
  }

  start() {
    this.detectConversation();
    this.scanExistingMessages();

    this.observer = new MutationObserver(() => {
      this.detectConversation();
      this.scanExistingMessages();
    });

    this.observer.observe(document.body, {
      childList: true,
      subtree: true,
    });

    this.onEvent({ type: "page_ready" });
  }

  stop() {
    if (this.observer) {
      this.observer.disconnect();
      this.observer = null;
    }
  }

  detectConversation() {
    const match = window.location.pathname.match(/\/c\/([a-zA-Z0-9-]+)/);
    const convId = match ? match[1] : null;
    if (convId !== this.currentConversationId) {
      this.currentConversationId = convId;
      this.onEvent({
        type: "conversation_detected",
        payload: { conversation_id: convId },
      });
    }
  }

  scanExistingMessages() {
    const messageElements = document.querySelectorAll(
      "article, [data-message-author-role], [data-testid^='conversation-turn-']"
    );

    messageElements.forEach((el, index) => {
      const role =
        el.getAttribute("data-message-author-role") ||
        (el.querySelector("[data-message-author-role='assistant']")
          ? "assistant"
          : el.querySelector("[data-message-author-role='user']")
          ? "user"
          : null);

      if (!role) return;

      const messageId =
        el.getAttribute("data-message-id") ||
        el.getAttribute("data-testid") ||
        `msg-${index}-${el.textContent.slice(0, 24).trim()}`;

      if (this.seenMessageIds.has(messageId)) {
        return;
      }

      const textEl =
        el.querySelector(".markdown") ||
        el.querySelector(".whitespace-pre-wrap") ||
        el;
      const text = textEl.innerText.trim();

      if (!text) return;

      this.seenMessageIds.add(messageId);

      const eventType =
        role === "assistant" ? "assistant_message" : "user_message";

      this.onEvent({
        type: eventType,
        payload: {
          message_id: messageId,
          text,
        },
      });
    });
  }
}
