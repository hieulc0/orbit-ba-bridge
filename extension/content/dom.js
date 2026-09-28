/**
 * DOM observer and message extraction for ChatGPT web interface.
 *
 * Implements resilient multi-tier turn extraction:
 * 1. Explicit author-role elements if both user & assistant tags exist.
 * 2. Hierarchy turn container detection derived from user-role parent tree (guaranteed turn interleaving).
 * 3. Fallback articles / testid inspection.
 *
 * Supports local debounced streaming detection, conversation-scoped message IDs,
 * and duplicate suppression.
 */

(function () {
  function cleanNodeText(el) {
    try {
      const clone = el.cloneNode(true);
      clone
        .querySelectorAll(
          "button, svg, h5, h6, [aria-hidden='true'], [class*='sr-only']"
        )
        .forEach((n) => n.remove());
      const cleaned = clone.innerText.trim();
      if (cleaned) return cleaned;
    } catch (_e) {}
    return (el.innerText || "").trim();
  }

  function extractTurnText(el, role) {
    if (role === "assistant") {
      const contentEl =
        el.querySelector(".markdown") ||
        el.querySelector("[class*='prose']") ||
        el.querySelector("[data-message-author-role='assistant']") ||
        el.querySelector("[class*='text-message']");
      if (contentEl) {
        const t = cleanNodeText(contentEl);
        if (t) return t;
      }
    } else {
      const userBubble =
        el.querySelector("[data-message-author-role='user']") ||
        el.querySelector(".whitespace-pre-wrap") ||
        el.querySelector("[class*='whitespace-pre-wrap']") ||
        el.querySelector("div[dir='auto']");
      if (userBubble) {
        const t = cleanNodeText(userBubble);
        if (t) return t;
      }
    }
    return cleanNodeText(el);
  }

  function extractId(el, role, roleIndex, convId) {
    const directId =
      el.getAttribute?.("data-message-id") ||
      el.querySelector?.("[data-message-id]")?.getAttribute("data-message-id") ||
      el.closest?.("[data-message-id]")?.getAttribute("data-message-id");

    if (directId) return directId;
    return `${convId || "conv"}-${role}-${roleIndex}`;
  }

  function isStreaming(el) {
    if (!el) return false;
    return (
      el.classList?.contains("result-streaming") ||
      Boolean(el.querySelector?.(".result-streaming"))
    );
  }

  function findConversationTurns() {
    const userRoles = Array.from(
      document.querySelectorAll("[data-message-author-role='user']")
    );
    const assistantRoles = Array.from(
      document.querySelectorAll("[data-message-author-role='assistant']")
    );

    // Tier 1: Explicit user AND assistant author-roles
    if (userRoles.length > 0 && assistantRoles.length > 0) {
      let userCount = 0;
      let assistantCount = 0;
      const all = [
        ...userRoles.map((el) => ({ el, role: "user" })),
        ...assistantRoles.map((el) => ({ el, role: "assistant" })),
      ];
      all.sort((a, b) => {
        const pos = a.el.compareDocumentPosition(b.el);
        if (pos & Node.DOCUMENT_POSITION_FOLLOWING) return -1;
        if (pos & Node.DOCUMENT_POSITION_PRECEDING) return 1;
        return 0;
      });
      return all.map((t) => {
        const roleIndex = t.role === "user" ? userCount++ : assistantCount++;
        return { el: t.el, role: t.role, roleIndex };
      });
    }

    // Tier 2: Turn containers derived from user-role parent hierarchy
    if (userRoles.length > 0) {
      let curr = userRoles[0];
      let turnContainer = null;
      while (
        curr &&
        curr.parentElement &&
        curr.parentElement.tagName.toLowerCase() !== "main" &&
        curr.parentElement !== document.body
      ) {
        const parent = curr.parentElement;
        const children = Array.from(parent.children);
        const userMatches = children.filter(
          (c) =>
            c.getAttribute("data-message-author-role") === "user" ||
            Boolean(c.querySelector("[data-message-author-role='user']"))
        ).length;

        if (userMatches >= 2 || (userRoles.length === 1 && children.length >= 2)) {
          turnContainer = parent;
          break;
        }
        curr = parent;
      }

      if (turnContainer) {
        const turns = [];
        let userCount = 0;
        let assistantCount = 0;
        const children = Array.from(turnContainer.children);

        children.forEach((child) => {
          if (
            child.querySelector("form") ||
            child.querySelector("#prompt-textarea") ||
            child.tagName.toLowerCase() === "form"
          ) {
            return;
          }

          const isUser = Boolean(
            child.getAttribute("data-message-author-role") === "user" ||
              child.querySelector("[data-message-author-role='user']")
          );
          const role = isUser ? "user" : "assistant";
          const text = extractTurnText(child, role);

          if (
            !text ||
            text === "Ready when you are." ||
            text === "What can I help with today?"
          ) {
            return;
          }

          const roleIndex = isUser ? userCount++ : assistantCount++;
          turns.push({ el: child, role, roleIndex, text });
        });

        if (turns.length > 0) {
          return turns;
        }
      }
    }

    // Tier 3: Universal container & article fallback
    const articles = Array.from(
      document.querySelectorAll(
        "article, [data-testid^='conversation-turn-'], main [class*='conversation-turn']"
      )
    );
    if (articles.length > 0) {
      let userCount = 0;
      let assistantCount = 0;
      return articles
        .map((container, index) => {
          const isUser = Boolean(
            container.getAttribute("data-message-author-role") === "user" ||
              container.querySelector("[data-message-author-role='user']") ||
              container.querySelector("button[aria-label*='Edit'], [data-testid*='edit']")
          );
          const role = isUser ? "user" : (index % 2 === 0 ? "user" : "assistant");
          const roleIndex = role === "user" ? userCount++ : assistantCount++;
          return { el: container, role, roleIndex };
        })
        .filter((t) => {
          const text = extractTurnText(t.el, t.role);
          return Boolean(
            text &&
              text !== "Ready when you are." &&
              text !== "What can I help with today?"
          );
        });
    }

    return [];
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
      const turns = findConversationTurns();

      if (turns.length > 0) {
        console.log(
          `[OrbitBridge DOM] scanMessages found ${turns.length} turns (conv: ${convId}):`,
          turns.map((t) => `${t.role}[${t.roleIndex}]`)
        );
      }

      turns.forEach((turn) => {
        const { el, role, roleIndex } = turn;
        const text = turn.text || extractTurnText(el, role);
        if (
          !text ||
          text === "Ready when you are." ||
          text === "What can I help with today?"
        ) {
          return;
        }

        const messageId = extractId(el, role, roleIndex, convId);

        if (role === "user") {
          this.handleUserMessage(messageId, text);
        } else {
          this.handleAssistantMessage(messageId, text, el);
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

      if (isStreaming(el)) {
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

      this.finalizeAssistantMessageWithText(messageId, text);
    }

    finalizeAssistantMessage(messageId, el) {
      const item = this.activeStreaming.get(messageId);
      if (!item) return;

      if (isStreaming(el)) {
        item.timer = setTimeout(
          () => this.finalizeAssistantMessage(messageId, el),
          500
        );
        return;
      }

      this.activeStreaming.delete(messageId);
      const latestText = extractTurnText(el, "assistant") || item.text;
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
