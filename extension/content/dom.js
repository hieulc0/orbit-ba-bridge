/**
 * DOM observer and message extraction for ChatGPT web interface.
 *
 * Implements resilient multi-strategy turn extraction (author-role -> articles -> content markers),
 * local debounced streaming detection, conversation-scoped message IDs, and duplicate suppression.
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
    return (el.innerText || "").trim();
  }

  function extractText(el, role) {
    if (role === "assistant") {
      const md = el.classList?.contains("markdown")
        ? el
        : el.querySelector?.(".markdown, [class*='prose']");
      if (md && md.innerText.trim()) return md.innerText.trim();
    } else {
      const bubble = el.classList?.contains("whitespace-pre-wrap")
        ? el
        : el.querySelector?.(
            ".whitespace-pre-wrap, [class*='whitespace-pre-wrap'], div[dir='auto']"
          );
      if (bubble && bubble.innerText.trim()) return bubble.innerText.trim();
    }
    return cleanNodeText(el);
  }

  function extractId(el, role, index, convId) {
    const directId =
      el.getAttribute?.("data-message-id") ||
      el.querySelector?.("[data-message-id]")?.getAttribute("data-message-id") ||
      el.closest?.("[data-message-id]")?.getAttribute("data-message-id");

    if (directId) return directId;
    return `${convId || "conv"}-${role}-${index}`;
  }

  function isStreaming(el) {
    if (!el) return false;
    return (
      el.classList?.contains("result-streaming") ||
      Boolean(el.querySelector?.(".result-streaming"))
    );
  }

  function findTurns() {
    // Strategy 1: Explicit author-role elements in DOM order
    const authorRoleNodes = Array.from(
      document.querySelectorAll("[data-message-author-role]")
    );
    if (authorRoleNodes.length > 0) {
      const hasUser = authorRoleNodes.some(
        (n) => n.getAttribute("data-message-author-role") === "user"
      );
      const hasAssistant = authorRoleNodes.some(
        (n) => n.getAttribute("data-message-author-role") === "assistant"
      );
      // Only return early if both roles or at least user turns are present
      if (hasUser && hasAssistant) {
        return authorRoleNodes.map((el) => {
          const role = el.getAttribute("data-message-author-role");
          return { el, role: role === "user" ? "user" : "assistant" };
        });
      }
    }

    // Strategy 2: Turn container wrappers (articles or testids)
    const turnContainers = Array.from(
      document.querySelectorAll(
        "article, [data-testid^='conversation-turn-'], main [class*='conversation-turn']"
      )
    );
    if (turnContainers.length > 0) {
      return turnContainers.map((container, index) => {
        const roleEl = container.querySelector("[data-message-author-role]");
        if (roleEl) {
          const role = roleEl.getAttribute("data-message-author-role");
          return { el: roleEl, role: role === "user" ? "user" : "assistant" };
        }
        if (
          container.querySelector(
            ".markdown, [class*='prose'], button[aria-label*='Read aloud'], button[aria-label*='Regenerate'], button[aria-label*='Good response']"
          )
        ) {
          return { el: container, role: "assistant" };
        }
        if (
          container.querySelector("button[aria-label*='Edit'], [data-testid*='edit']")
        ) {
          return { el: container, role: "user" };
        }
        return { el: container, role: index % 2 === 0 ? "user" : "assistant" };
      });
    }

    // Strategy 3: Universal content marker scan
    const main = document.querySelector("main") || document.body;
    const assistantNodes = Array.from(
      main.querySelectorAll(".markdown, [class*='prose']")
    );
    const userNodes = Array.from(
      main.querySelectorAll(
        "[data-message-author-role='user'], .whitespace-pre-wrap, [class*='whitespace-pre-wrap']"
      )
    ).filter(
      (el) =>
        !el.closest("form") &&
        !el.closest("#prompt-textarea") &&
        !el.isContentEditable
    );

    const allTurns = [
      ...assistantNodes.map((el) => ({ el, role: "assistant" })),
      ...userNodes.map((el) => ({ el, role: "user" })),
    ];

    allTurns.sort((a, b) => {
      const pos = a.el.compareDocumentPosition(b.el);
      if (pos & Node.DOCUMENT_POSITION_FOLLOWING) return -1;
      if (pos & Node.DOCUMENT_POSITION_PRECEDING) return 1;
      return 0;
    });

    return allTurns;
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
      const turns = findTurns();

      if (turns.length > 0) {
        console.log(
          `[OrbitBridge DOM] scanMessages found ${turns.length} turns (conv: ${convId})`
        );
      }

      turns.forEach((turn, index) => {
        const { el, role } = turn;
        const text = extractText(el, role);
        if (
          !text ||
          text === "Ready when you are." ||
          text === "What can I help with today?"
        ) {
          return;
        }

        const messageId = extractId(el, role, index, convId);

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
      const latestText = extractText(el, "assistant") || item.text;
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
