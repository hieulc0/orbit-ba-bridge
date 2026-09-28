/**
 * DOM observer and message extraction for ChatGPT web interface.
 *
 * Implements robust multi-strategy role detection, debounced streaming assistant
 * detection, duplicate suppression, and stable external message ID extraction.
 */

(function () {
  function determineRole(el) {
    // 1. Direct or child attribute data-message-author-role
    const attrRole =
      el.getAttribute("data-message-author-role") ||
      el.querySelector("[data-message-author-role]")?.getAttribute("data-message-author-role");
    if (attrRole === "user" || attrRole === "assistant") return attrRole;

    // 2. data-testid indicators
    const testId = (el.getAttribute("data-testid") || "").toLowerCase();
    if (testId.includes("user")) return "user";
    if (testId.includes("assistant")) return "assistant";
    if (el.querySelector("[data-testid*='user']")) return "user";
    if (el.querySelector("[data-testid*='assistant']")) return "assistant";

    // 3. Aria-labels (e.g. "You said:", "ChatGPT said:")
    const ariaLabel = (
      el.getAttribute("aria-label") ||
      el.querySelector("[aria-label]")?.getAttribute("aria-label") ||
      ""
    ).toLowerCase();
    if (ariaLabel.includes("you said")) return "user";
    if (ariaLabel.includes("chatgpt said")) return "assistant";

    // 4. Action buttons: assistant has copy/read aloud buttons; user has edit button
    if (
      el.querySelector(
        "button[aria-label*='Copy'], button[data-testid*='copy'], button[aria-label*='Read aloud']"
      )
    ) {
      return "assistant";
    }
    if (
      el.querySelector("button[aria-label*='Edit'], button[data-testid*='edit']")
    ) {
      return "user";
    }

    // 5. Headings: ChatGPT displays "You" or "ChatGPT" in turn headers
    const heading = el.querySelector("h5, h6, [class*='font-semibold']");
    if (heading) {
      const headingText = heading.innerText.trim().toLowerCase();
      if (headingText === "you") return "user";
      if (headingText === "chatgpt") return "assistant";
    }

    // 6. Content structure: assistant messages contain .markdown / prose
    if (el.querySelector(".markdown, [class*='prose']")) {
      return "assistant";
    }

    // 7. Avatar indicators
    if (
      el.querySelector("[data-testid*='user-avatar']") ||
      el.querySelector("img[alt*='User']")
    ) {
      return "user";
    }

    return null;
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

    // Fallback: clone element and remove UI buttons, icons, headers
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
      this.currentConversationId = null;
      this.pendingInjection = null;
    }

    setPendingInjection(injection) {
      this.pendingInjection = injection;
    }

    start() {
      console.log("[OrbitBridge DOM] Starting DOM observer");
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
        setTimeout(() => this.scanMessages(), 150);
      }
    }

    isGenerating() {
      const stopBtn =
        document.querySelector("button[data-testid='stop-button']") ||
        document.querySelector("button[aria-label='Stop generating']") ||
        document.querySelector("button[aria-label='Stop streaming']");
      const streamingEl = document.querySelector(".result-streaming");
      return Boolean(stopBtn || streamingEl);
    }

    scanMessages() {
      // Find candidate turn elements
      const candidates = document.querySelectorAll(
        "article, [data-testid^='conversation-turn-'], [data-message-author-role]"
      );

      // De-duplicate nested candidates (keep the top-level turn container or specific role element)
      const turns = [];
      const seenNodes = new Set();

      candidates.forEach((el) => {
        // If el is an article, use it directly
        if (el.tagName.toLowerCase() === "article") {
          if (!seenNodes.has(el)) {
            seenNodes.add(el);
            turns.push(el);
          }
          return;
        }

        // If inside an article already queued, don't re-add
        const parentArticle = el.closest("article");
        if (parentArticle && seenNodes.has(parentArticle)) {
          return;
        }

        if (!seenNodes.has(el)) {
          seenNodes.add(el);
          turns.push(el);
        }
      });

      if (turns.length === 0) {
        return;
      }

      turns.forEach((el, index) => {
        const role = determineRole(el);
        if (!role) {
          return;
        }

        const rawId = extractMessageId(el, role, index);
        const text = extractMessageText(el);

        if (!text) {
          return;
        }

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
      console.log(`[OrbitBridge DOM] Emitting user_message_observed:`, {
        messageId,
        length: text.length,
        snippet: text.slice(0, 40),
      });

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
      const globallyGenerating = this.isGenerating();

      if (isStreamingEl || globallyGenerating) {
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

      this.finalizeAssistantMessageWithText(messageId, text);
    }

    finalizeAssistantMessage(messageId) {
      if (this.isGenerating()) {
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
      console.log(`[OrbitBridge DOM] Emitting assistant_message_observed:`, {
        messageId,
        length: text.length,
        snippet: text.slice(0, 40),
      });

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
