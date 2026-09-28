/**
 * DOM observer and message extraction for ChatGPT web interface.
 *
 * Targets ChatGPT's active DOM structure:
 * - Turn containers: [data-chatgpt-search-unit-key], [data-content-search-unit-key]
 * - Turn headings: [data-conversation-role="assistant" | "user"]
 * - Content blocks: [data-markdown-text-style="assistant-message"], [data-message-author-role]
 * - Native message IDs: [data-chatgpt-selection-message-id], [data-message-id]
 */

(function () {
  function cleanNodeText(el) {
    try {
      const clone = el.cloneNode(true);
      clone
        .querySelectorAll(
          "button, svg, h1, h2, h3, h4, h5, h6, [aria-hidden='true'], [class*='sr-only']"
        )
        .forEach((n) => n.remove());
      const cleaned = clone.innerText.trim();
      if (cleaned) return cleaned;
    } catch (_e) {}
    return (el.innerText || el.textContent || "").trim();
  }

  function extractTurnText(el, role) {
    if (role === "assistant") {
      const md =
        el.querySelector("[data-markdown-text-style='assistant-message']") ||
        el.querySelector("[class*='MarkdownRoot']") ||
        el.querySelector(".markdown, [class*='prose']");
      if (md) {
        const t = cleanNodeText(md);
        if (t) return t;
      }
    } else {
      const userBubble =
        el.querySelector("[data-markdown-text-style='user-message']") ||
        el.querySelector("[data-message-author-role='user']") ||
        el.querySelector(
          ".whitespace-pre-wrap, [class*='whitespace-pre-wrap'], div[dir='auto']"
        );
      if (userBubble) {
        const t = cleanNodeText(userBubble);
        if (t) return t;
      }
    }
    return cleanNodeText(el);
  }

  function determineTurnRole(turnEl, index) {
    // 1. Direct or descendant search unit key (ends in :assistant or :user)
    const unitKey =
      turnEl.getAttribute?.("data-chatgpt-search-unit-key") ||
      turnEl.getAttribute?.("data-content-search-unit-key") ||
      turnEl.querySelector?.("[data-chatgpt-search-unit-key]")?.getAttribute("data-chatgpt-search-unit-key") ||
      "";
    if (unitKey.includes(":assistant")) return "assistant";
    if (unitKey.includes(":user")) return "user";

    // 2. Direct or descendant conversation-role heading
    const convRole =
      turnEl.getAttribute?.("data-conversation-role") ||
      turnEl.querySelector?.("[data-conversation-role]")?.getAttribute("data-conversation-role");
    if (convRole === "assistant" || convRole === "user") return convRole;

    // 3. Direct or descendant markdown style
    const mdStyle =
      turnEl.getAttribute?.("data-markdown-text-style") ||
      turnEl.querySelector?.("[data-markdown-text-style]")?.getAttribute("data-markdown-text-style");
    if (mdStyle === "assistant-message") return "assistant";
    if (mdStyle === "user-message") return "user";

    // 4. Traditional author role
    const authorRole =
      turnEl.getAttribute?.("data-message-author-role") ||
      turnEl.querySelector?.("[data-message-author-role]")?.getAttribute("data-message-author-role");
    if (authorRole === "assistant" || authorRole === "user") return authorRole;

    // 5. Test IDs or assistant UI buttons
    const hasAssistantIndicators = Boolean(
      turnEl.querySelector?.(
        "[data-markdown-text-style='assistant-message'], button[aria-label*='Read aloud'], button[aria-label*='Listen'], button[aria-label*='Regenerate'], button[aria-label*='Try again'], button[aria-label*='Bad response'], button[aria-label*='Dislike'], button[aria-label*='Good response']"
      )
    );
    if (hasAssistantIndicators) return "assistant";

    const hasUserIndicators = Boolean(
      turnEl.querySelector?.("button[aria-label*='Edit'], [data-testid*='edit']")
    );
    if (hasUserIndicators) return "user";

    return index % 2 === 0 ? "user" : "assistant";
  }

  function extractTurnId(el, role, index, convId) {
    const directId =
      el.getAttribute?.("data-chatgpt-selection-message-id") ||
      el.querySelector?.("[data-chatgpt-selection-message-id]")?.getAttribute("data-chatgpt-selection-message-id") ||
      el.closest?.("[data-chatgpt-selection-message-id]")?.getAttribute("data-chatgpt-selection-message-id") ||
      el.getAttribute?.("data-message-id") ||
      el.querySelector?.("[data-message-id]")?.getAttribute("data-message-id");

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

  function findConversationTurns() {
    // Strategy 1: Search unit key containers (the primary modern ChatGPT turn wrapper)
    const unitContainers = Array.from(
      document.querySelectorAll(
        "[data-chatgpt-search-unit-key], [data-content-search-unit-key]"
      )
    );
    if (unitContainers.length > 0) {
      return unitContainers.map((el, i) => ({
        el,
        role: determineTurnRole(el, i),
        index: i,
      }));
    }

    // Strategy 2: Role headings (h4[data-conversation-role])
    const roleHeadings = Array.from(
      document.querySelectorAll("[data-conversation-role]")
    );
    if (roleHeadings.length > 0) {
      return roleHeadings.map((h, i) => {
        const container = h.closest("div") || h.parentElement || h;
        return {
          el: container,
          role: determineTurnRole(h, i),
          index: i,
        };
      });
    }

    // Strategy 3: Assistant markdown containers + User author-role / bubbles
    const assistantNodes = Array.from(
      document.querySelectorAll(
        "[data-markdown-text-style='assistant-message'], [class*='MarkdownRoot']"
      )
    );
    const userNodes = Array.from(
      document.querySelectorAll(
        "[data-message-author-role='user'], [data-markdown-text-style='user-message'], .whitespace-pre-wrap"
      )
    ).filter(
      (el) =>
        !el.closest("form") &&
        !el.closest("#prompt-textarea") &&
        !el.isContentEditable
    );

    if (assistantNodes.length > 0 || userNodes.length > 0) {
      const all = [
        ...userNodes.map((el) => ({ el, role: "user" })),
        ...assistantNodes.map((el) => ({ el, role: "assistant" })),
      ];

      all.sort((a, b) => {
        const pos = a.el.compareDocumentPosition(b.el);
        if (pos & Node.DOCUMENT_POSITION_FOLLOWING) return -1;
        if (pos & Node.DOCUMENT_POSITION_PRECEDING) return 1;
        return 0;
      });

      return all.map((item, i) => ({
        el: item.el,
        role: item.role,
        index: i,
      }));
    }

    // Strategy 4: Fallback to author-role or articles
    const fallbackNodes = Array.from(
      document.querySelectorAll(
        "[data-message-author-role], [data-turn], [data-testid^='conversation-turn-'], article"
      )
    );
    return fallbackNodes.map((el, i) => ({
      el,
      role: determineTurnRole(el, i),
      index: i,
    }));
  }

  class DomObserver {
    constructor(onEvent) {
      this.onEvent = onEvent;
      this.seenMessageIds = new Set();
      this.activeStreaming = new Map();
      this.observer = null;
      this.pollInterval = null;
      this.currentConversationId = null;
      this.pendingInjection = null;
    }

    setPendingInjection(injection) {
      this.pendingInjection = injection;
    }

    start() {
      console.log(
        "[OrbitBridge DOM] Starting DOM observer on",
        window.location.href
      );
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
        payload: {
          external_url: window.location.href,
        },
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
          turns.map((t) => `${t.role}[${t.index}]`)
        );
      }

      turns.forEach((turn) => {
        const { el, role, index } = turn;
        const text = extractTurnText(el, role);
        if (
          !text ||
          text === "Ready when you are." ||
          text === "What can I help with today?"
        ) {
          return;
        }

        const messageId = extractTurnId(el, role, index, convId);

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
