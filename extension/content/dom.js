/**
 * DOM observer and message extraction for ChatGPT web interface.
 *
 * Implements chronological turn extraction, action-bar completion detection,
 * debounced streaming assistant detection, duplicate suppression, and stable message IDs.
 */

(function () {
  function isVisible(el) {
    if (!el) return false;
    return Boolean(el.offsetWidth || el.offsetHeight || el.getClientRects().length);
  }

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
      this.activeStreaming = new Map(); // id -> { text, timer, unchangedCount }
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

    isGenerating() {
      const stopBtn =
        document.querySelector("button[data-testid='stop-button']") ||
        document.querySelector("button[aria-label='Stop generating']") ||
        document.querySelector("button[aria-label='Stop streaming']");

      if (stopBtn && isVisible(stopBtn) && !stopBtn.disabled) {
        return true;
      }

      const streamingEl = document.querySelector(".result-streaming");
      if (streamingEl && isVisible(streamingEl)) {
        return true;
      }

      return false;
    }

    scanMessages() {
      // 1. Primary: scan turn articles in chronological DOM order
      const articles = document.querySelectorAll(
        "article, [data-testid^='conversation-turn-']"
      );

      if (articles.length > 0) {
        articles.forEach((art, index) => {
          const isAssistant = Boolean(
            art.getAttribute("data-message-author-role") === "assistant" ||
            art.querySelector("[data-message-author-role='assistant']") ||
            art.querySelector(
              ".markdown, [class*='prose'], button[aria-label*='Copy'], button[data-testid*='copy']"
            )
          );

          const role = isAssistant ? "assistant" : "user";
          const rawId = extractMessageId(art, role, index);
          const text = extractMessageText(art);

          if (!text) return;

          if (role === "user") {
            this.handleUserMessage(rawId, text);
          } else {
            this.handleAssistantMessage(rawId, text, art);
          }
        });
        return;
      }

      // 2. Fallback: direct query for role containers
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

      // Check if action bar is already rendered (Copy button, thumbs, etc.)
      // When action bar is present, generation is 100% complete
      const hasActionBar = Boolean(
        el.querySelector(
          "button[aria-label*='Copy'], button[data-testid*='copy'], button[aria-label*='Good response'], button[aria-label*='Bad response'], button[aria-label*='Read aloud']"
        ) ||
        el.closest("article")?.querySelector(
          "button[aria-label*='Copy'], button[data-testid*='copy']"
        )
      );

      if (hasActionBar) {
        this.finalizeAssistantMessageWithText(messageId, text);
        return;
      }

      const isStreamingEl =
        el.classList.contains("result-streaming") ||
        Boolean(el.querySelector(".result-streaming"));
      const globallyGenerating = this.isGenerating();

      if (isStreamingEl || globallyGenerating) {
        if (this.activeStreaming.has(messageId)) {
          const item = this.activeStreaming.get(messageId);
          clearTimeout(item.timer);

          if (item.text === text) {
            item.unchangedCount = (item.unchangedCount || 0) + 1;
            if (item.unchangedCount >= 3) {
              this.activeStreaming.delete(messageId);
              this.finalizeAssistantMessageWithText(messageId, text);
              return;
            }
          } else {
            item.text = text;
            item.unchangedCount = 0;
          }

          item.timer = setTimeout(() => {
            this.finalizeAssistantMessage(messageId);
          }, 800);
        } else {
          const timer = setTimeout(() => {
            this.finalizeAssistantMessage(messageId);
          }, 800);
          this.activeStreaming.set(messageId, { text, timer, unchangedCount: 0 });
        }
        return;
      }

      this.finalizeAssistantMessageWithText(messageId, text);
    }

    finalizeAssistantMessage(messageId) {
      const item = this.activeStreaming.get(messageId);
      if (!item) return;

      if (this.isGenerating() && item.unchangedCount < 3) {
        item.timer = setTimeout(() => this.finalizeAssistantMessage(messageId), 600);
        return;
      }

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
