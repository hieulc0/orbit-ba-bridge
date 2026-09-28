/**
 * DOM observer and message extraction for ChatGPT web interface.
 *
 * Implements debounced streaming assistant detection, duplicate suppression,
 * and stable external message ID extraction.
 */

(function () {
  class DomObserver {
    constructor(onEvent) {
      this.onEvent = onEvent;
      this.seenMessageIds = new Set();
      this.activeStreaming = new Map(); // id -> { text, timer }
      this.observer = null;
      this.currentConversationId = null;
      this.pendingInjection = null;
    }

    setPendingInjection(injection) {
      this.pendingInjection = injection;
    }

    start() {
      this.detectConversation();
      this.scanMessages();

      this.observer = new MutationObserver(() => {
        this.detectConversation();
        this.scanMessages();
      });

      this.observer.observe(document.body, {
        childList: true,
        subtree: true,
        characterData: true,
      });

      this.onEvent({
        type: "page_ready",
        payload: { external_url: window.location.href },
      });
    }

    stop() {
      if (this.observer) {
        this.observer.disconnect();
        this.observer = null;
      }
      for (const item of this.activeStreaming.values()) {
        if (item.timer) clearTimeout(item.timer);
      }
      this.activeStreaming.clear();
    }

    detectConversation() {
      const match = window.location.pathname.match(/\/c\/([a-zA-Z0-9-]+)/);
      const convId = match ? match[1] : null;
      if (convId !== this.currentConversationId) {
        this.currentConversationId = convId;
        this.seenMessageIds.clear();
        this.onEvent({
          type: "conversation_detected",
          payload: {
            conversation_id: convId,
            external_conversation_ref: convId,
          },
        });
        // Immediately scan messages for newly selected conversation
        setTimeout(() => this.scanMessages(), 100);
      }
    }

    isGenerating() {
      return Boolean(
        document.querySelector("button[data-testid='stop-button']") ||
          document.querySelector("button[aria-label='Stop generating']") ||
          document.querySelector("button[aria-label='Stop streaming']") ||
          document.querySelector(".result-streaming")
      );
    }

    scanMessages() {
      // 1. Primary: match elements with explicit data-message-author-role
      const roleElements = document.querySelectorAll("[data-message-author-role]");
      if (roleElements.length > 0) {
        roleElements.forEach((el, index) => {
          const role = el.getAttribute("data-message-author-role");
          if (!role || (role !== "user" && role !== "assistant")) return;

          const article = el.closest("article");
          const rawId =
            el.getAttribute("data-message-id") ||
            (article && article.getAttribute("data-testid")) ||
            `turn-${index}`;

          const textEl =
            el.querySelector(".markdown") ||
            el.querySelector(".whitespace-pre-wrap") ||
            el;
          const text = textEl.innerText.trim();

          if (!text) return;

          if (role === "user") {
            this.handleUserMessage(rawId, text);
          } else if (role === "assistant") {
            this.handleAssistantMessage(rawId, text, el);
          }
        });
        return;
      }

      // 2. Fallback: match <article> elements
      const articles = document.querySelectorAll("article, [data-testid^='conversation-turn-']");
      articles.forEach((el, index) => {
        const role =
          el.getAttribute("data-message-author-role") ||
          (el.querySelector("[data-message-author-role='assistant']")
            ? "assistant"
            : el.querySelector("[data-message-author-role='user']")
            ? "user"
            : null);

        if (!role) return;

        const rawId =
          el.getAttribute("data-message-id") ||
          el.getAttribute("data-testid") ||
          `turn-${index}`;

        const textEl =
          el.querySelector(".markdown") ||
          el.querySelector(".whitespace-pre-wrap") ||
          el;
        const text = textEl.innerText.trim();

        if (!text) return;

        if (role === "user") {
          this.handleUserMessage(rawId, text);
        } else if (role === "assistant") {
          this.handleAssistantMessage(rawId, text, el);
        }
      });
    }

    handleUserMessage(messageId, text) {
      if (this.seenMessageIds.has(messageId)) {
        return;
      }

      this.seenMessageIds.add(messageId);

      // Check if this matches a pending external injection
      if (this.pendingInjection) {
        const inj = this.pendingInjection;
        const snippet = inj.text.slice(0, 48);
        if (text.includes(snippet) || (Date.now() - inj.timestamp < 10000)) {
          this.onEvent({
            type: "injection_materialized",
            payload: {
              injection_id: inj.injection_id,
              external_message_id: messageId,
            },
          });
          this.pendingInjection = null;
        }
      }

      this.onEvent({
        type: "user_message_observed",
        payload: {
          external_message_id: messageId,
          text,
        },
      });
    }

    handleAssistantMessage(messageId, text, el) {
      if (this.seenMessageIds.has(messageId)) {
        return;
      }

      const isStreamingEl = el.classList.contains("result-streaming") ||
        Boolean(el.querySelector(".result-streaming"));
      const globallyGenerating = this.isGenerating();

      if (isStreamingEl || globallyGenerating) {
        // Debounce streaming: wait for generation to settle
        if (this.activeStreaming.has(messageId)) {
          const item = this.activeStreaming.get(messageId);
          clearTimeout(item.timer);
          item.text = text;
          item.timer = setTimeout(() => {
            this.finalizeAssistantMessage(messageId);
          }, 800);
        } else {
          const timer = setTimeout(() => {
            this.finalizeAssistantMessage(messageId);
          }, 800);
          this.activeStreaming.set(messageId, { text, timer });
        }
        return;
      }

      // If not streaming at all, finalize immediately
      this.finalizeAssistantMessageWithText(messageId, text);
    }

    finalizeAssistantMessage(messageId) {
      if (this.isGenerating()) {
        // Still generating globally, reschedule
        const item = this.activeStreaming.get(messageId);
        if (item) {
          clearTimeout(item.timer);
          item.timer = setTimeout(() => this.finalizeAssistantMessage(messageId), 600);
        }
        return;
      }

      const item = this.activeStreaming.get(messageId);
      if (item) {
        this.activeStreaming.delete(messageId);
        this.finalizeAssistantMessageWithText(messageId, item.text);
      }
    }

    finalizeAssistantMessageWithText(messageId, text) {
      if (this.seenMessageIds.has(messageId)) {
        return;
      }

      this.seenMessageIds.add(messageId);

      this.onEvent({
        type: "assistant_message_observed",
        payload: {
          external_message_id: messageId,
          text,
          is_final: true,
        },
      });
    }
  }

  globalThis.OrbitBridge = globalThis.OrbitBridge || {};
  globalThis.OrbitBridge.DomObserver = DomObserver;
})();
