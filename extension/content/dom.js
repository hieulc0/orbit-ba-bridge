/**
 * DOM observer and message extraction for ChatGPT web interface.
 *
 * Implements chronological message extraction, local streaming detection via .result-streaming,
 * duplicate suppression, conversation-scoped message IDs, and robust fallback selectors.
 */

(function () {
  function cleanNodeText(el) {
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

  function extractMessageText(node, role) {
    if (role === "assistant") {
      const md = node.querySelector(".markdown, [class*='prose']");
      if (md && md.innerText.trim()) return md.innerText.trim();
    } else {
      const userBubble = node.querySelector(
        ".whitespace-pre-wrap, [class*='whitespace-pre-wrap'], div[dir='auto']"
      );
      if (userBubble && userBubble.innerText.trim()) return userBubble.innerText.trim();
    }
    return cleanNodeText(node);
  }

  function extractMessageId(node, role, index, convId) {
    const directId =
      node.getAttribute("data-message-id") ||
      node.querySelector("[data-message-id]")?.getAttribute("data-message-id") ||
      node.closest("[data-message-id]")?.getAttribute("data-message-id");

    if (directId) return directId;
    return `${convId || "conv"}-${role}-${index}`;
  }

  class DomObserver {
    constructor(onEvent) {
      this.onEvent = onEvent;
      this.seenMessageIds = new Set();
      this.activeStreaming = new Map(); // id -> { text, timer, el }
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
      const convId = this.currentConversationId;

      // 1. Primary: query authoritative author-role elements in DOM order
      let messageNodes = Array.from(
        document.querySelectorAll("[data-message-author-role]")
      );

      // 2. Fallback: query article turn containers if data-message-author-role not present
      if (messageNodes.length === 0) {
        messageNodes = Array.from(
          document.querySelectorAll("article, [data-testid^='conversation-turn-']")
        );
      }

      if (messageNodes.length === 0) return;

      messageNodes.forEach((node, index) => {
        let role = null;
        const authorRole =
          node.getAttribute("data-message-author-role") ||
          node.querySelector("[data-message-author-role]")?.getAttribute("data-message-author-role");

        if (authorRole === "user") {
          role = "user";
        } else if (authorRole === "assistant") {
          role = "assistant";
        } else if (
          node.querySelector(
            ".markdown, [class*='prose'], button[aria-label*='Read aloud'], button[aria-label*='Regenerate'], button[aria-label*='Good response']"
          )
        ) {
          role = "assistant";
        } else if (
          node.querySelector("button[aria-label*='Edit'], [data-testid*='edit']")
        ) {
          role = "user";
        } else {
          role = index % 2 === 0 ? "user" : "assistant";
        }

        const rawId = extractMessageId(node, role, index, convId);
        const text = extractMessageText(node, role);
        if (
          !text ||
          text === "Ready when you are." ||
          text === "What can I help with today?"
        ) {
          return;
        }

        if (role === "user") {
          this.handleUserMessage(rawId, text);
        } else {
          this.handleAssistantMessage(rawId, text, node);
        }
      });
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
            this.finalizeAssistantMessage(messageId, el);
          }, 600);
        } else {
          const timer = setTimeout(() => {
            this.finalizeAssistantMessage(messageId, el);
          }, 600);
          this.activeStreaming.set(messageId, { text, timer, el });
        }
        return;
      }

      // Generation complete: finalize immediately
      this.finalizeAssistantMessageWithText(messageId, text);
    }

    finalizeAssistantMessage(messageId, el) {
      const item = this.activeStreaming.get(messageId);
      if (!item) return;

      const stillStreaming =
        el &&
        (el.classList.contains("result-streaming") ||
          Boolean(el.querySelector(".result-streaming")));

      if (stillStreaming) {
        item.timer = setTimeout(() => this.finalizeAssistantMessage(messageId, el), 500);
        return;
      }

      this.activeStreaming.delete(messageId);
      const latestText = extractMessageText(el, "assistant") || item.text;
      this.finalizeAssistantMessageWithText(messageId, latestText);
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
