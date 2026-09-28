/**
 * DOM observer and message extraction for ChatGPT web interface.
 *
 * Implements chronological turn extraction, local streaming detection via .result-streaming,
 * duplicate suppression, and stable message IDs.
 */

(function () {
  function extractMessageText(el) {
    const contentEl =
      el.querySelector(".markdown") ||
      el.querySelector("[class*='prose']") ||
      el.querySelector(".whitespace-pre-wrap") ||
      el.querySelector("[class*='text-message']") ||
      el.querySelector("div[dir='auto']");

    if (contentEl) {
      const text = contentEl.innerText.trim();
      if (text) return text;
    }

    try {
      const clone = el.cloneNode(true);
      clone
        .querySelectorAll("button, svg, h5, h6, [aria-hidden='true']")
        .forEach((n) => n.remove());
      const cleaned = clone.innerText.trim();
      if (cleaned) return cleaned;
    } catch (_e) {}

    return el.innerText.trim();
  }

  function extractMessageId(el, role, index) {
    const directId =
      el.getAttribute("data-message-id") ||
      el.querySelector("[data-message-id]")?.getAttribute("data-message-id") ||
      el.closest("[data-message-id]")?.getAttribute("data-message-id") ||
      el.getAttribute("data-testid") ||
      el.closest("article")?.getAttribute("data-testid");

    if (directId) return directId;
    return `turn-${role}-${index}`;
  }

  class DomObserver {
    constructor(onEvent) {
      this.onEvent = onEvent;
      this.seenMessageIds = new Set();
      this.activeStreaming = new Map(); // id -> { text, timer }
      this.observer = null;
      this.pollInterval = null;
      this.currentConversationId = null;
      this.pendingInjection = null;
    }

    setPendingInjection(injection) {
      this.pendingInjection = injection;
    }

    start() {
      console.log("[OrbitBridge DOM] Starting DOM observer on", window.location.href);
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

      this.pollInterval = setInterval(() => {
        this.detectConversation();
        this.scanMessages();
      }, 1500);

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
      if (this.pollInterval) {
        clearInterval(this.pollInterval);
        this.pollInterval = null;
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
        console.log(`[OrbitBridge DOM] Conversation detected: ${convId}`);
        this.currentConversationId = convId;
        this.seenMessageIds.clear();
        this.onEvent({
          type: "conversation_detected",
          payload: {
            conversation_id: convId,
            external_conversation_ref: convId,
          },
        });

        [300, 800, 1500, 3000].forEach((delay) => {
          setTimeout(() => this.scanMessages(), delay);
        });
      }
    }

    scanMessages() {
      // 1. Primary: query authoritative author-role elements in DOM order
      const nodes = document.querySelectorAll(
        "[data-message-author-role='user'], [data-message-author-role='assistant']"
      );

      if (nodes.length > 0) {
        nodes.forEach((node, index) => {
          const role = node.getAttribute("data-message-author-role");
          const rawId = extractMessageId(node, role, index);
          const text = extractMessageText(node);
          if (!text) return;

          if (role === "user") {
            this.handleUserMessage(rawId, text);
          } else if (role === "assistant") {
            this.handleAssistantMessage(rawId, text, node);
          }
        });
        return;
      }

      // 2. Fallback: query article turn containers
      const articles = document.querySelectorAll(
        "article, [data-testid^='conversation-turn-']"
      );

      if (articles.length > 0) {
        articles.forEach((art, index) => {
          let role = "user";
          if (art.querySelector("[data-message-author-role='assistant']")) {
            role = "assistant";
          } else if (art.querySelector("[data-message-author-role='user']")) {
            role = "user";
          } else if (art.querySelector(".markdown, [class*='prose']")) {
            role = "assistant";
          }

          const rawId = extractMessageId(art, role, index);
          const text = extractMessageText(art);
          if (!text) return;

          if (role === "user") {
            this.handleUserMessage(rawId, text);
          } else {
            this.handleAssistantMessage(rawId, text, art);
          }
        });
      }
    }

    handleUserMessage(messageId, text) {
      if (this.seenMessageIds.has(messageId)) {
        return;
      }

      this.seenMessageIds.add(messageId);
      console.log(
        `[OrbitBridge DOM] >>> Emitting user_message_observed (${messageId}):`,
        text.slice(0, 50)
      );

      // Check if this matches a pending external injection
      if (this.pendingInjection) {
        const inj = this.pendingInjection;
        const snippet = inj.text.slice(0, 48);
        if (text.includes(snippet) || Date.now() - inj.timestamp < 10000) {
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

      const isStreamingEl =
        el.classList.contains("result-streaming") ||
        Boolean(el.querySelector(".result-streaming"));

      if (isStreamingEl) {
        if (this.activeStreaming.has(messageId)) {
          const item = this.activeStreaming.get(messageId);
          clearTimeout(item.timer);
          item.text = text;
          item.timer = setTimeout(() => {
            this.finalizeAssistantMessage(messageId);
          }, 600);
        } else {
          const timer = setTimeout(() => {
            this.finalizeAssistantMessage(messageId);
          }, 600);
          this.activeStreaming.set(messageId, { text, timer });
        }
        return;
      }

      // Generation complete: finalize immediately
      this.finalizeAssistantMessageWithText(messageId, text);
    }

    finalizeAssistantMessage(messageId) {
      const item = this.activeStreaming.get(messageId);
      if (!item) return;

      this.activeStreaming.delete(messageId);
      this.finalizeAssistantMessageWithText(messageId, item.text);
    }

    finalizeAssistantMessageWithText(messageId, text) {
      if (this.seenMessageIds.has(messageId)) {
        return;
      }

      this.seenMessageIds.add(messageId);
      console.log(
        `[OrbitBridge DOM] >>> Emitting assistant_message_observed (${messageId}):`,
        text.slice(0, 50)
      );

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
