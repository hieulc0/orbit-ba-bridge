/**
 * DOM observer and message extraction for ChatGPT web interface.
 *
 * Implements resilient multi-strategy turn extraction:
 * 1. Top-level turn container attributes: [data-turn] (standard in modern chatgpt.com).
 * 2. Conversation turn test IDs: [data-testid^="conversation-turn-"].
 * 3. Semantic article turn wrappers: article.
 * 4. Direct message author role markers: [data-message-author-role].
 * 5. Role determination with multi-factor fallback (attributes -> UI action buttons -> DOM order).
 * 6. Clean text extraction stripping interactive toolbars and screen-reader elements.
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
    return (el.innerText || el.textContent || "").trim();
  }

  function extractTurnText(el, role) {
    if (role === "assistant") {
      const md = el.querySelector(".markdown, [class*='prose']");
      if (md) {
        const t = cleanNodeText(md);
        if (t) return t;
      }
    } else {
      const userBubble = el.querySelector(
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
    // 1. Direct or descendant data-turn
    const turn =
      turnEl.getAttribute("data-turn") ||
      turnEl.querySelector("[data-turn]")?.getAttribute("data-turn");
    if (turn === "user" || turn === "assistant") return turn;

    // 2. Direct or descendant data-message-author-role
    const authorRole =
      turnEl.getAttribute("data-message-author-role") ||
      turnEl.querySelector("[data-message-author-role]")?.getAttribute(
        "data-message-author-role"
      );
    if (authorRole === "user" || authorRole === "assistant") return authorRole;

    // 3. Test IDs
    const testId = turnEl.getAttribute("data-testid") || "";
    if (testId.includes("user")) return "user";
    if (testId.includes("assistant")) return "assistant";

    // 4. Assistant UI action indicators (buttons exclusive to assistant replies)
    const hasAssistantIndicators = Boolean(
      turnEl.querySelector(
        "button[aria-label*='Read aloud'], button[aria-label*='Listen'], button[aria-label*='Regenerate'], button[aria-label*='Try again'], button[aria-label*='Bad response'], button[aria-label*='Dislike'], button[aria-label*='Good response'], .markdown, [class*='prose']"
      )
    );
    if (hasAssistantIndicators) return "assistant";

    // 5. User UI action indicators (edit button)
    const hasUserIndicators = Boolean(
      turnEl.querySelector("button[aria-label*='Edit'], [data-testid*='edit']")
    );
    if (hasUserIndicators) return "user";

    // 6. Chronological conversation alternating order
    return index % 2 === 0 ? "user" : "assistant";
  }

  function extractTurnId(el, role, index, convId) {
    const directId =
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
    // Strategy 1: [data-turn]
    let turns = Array.from(document.querySelectorAll("[data-turn]"));
    if (turns.length > 0) {
      return turns.map((el, i) => ({
        el,
        role: determineTurnRole(el, i),
        index: i,
      }));
    }

    // Strategy 2: [data-testid^="conversation-turn-"]
    turns = Array.from(
      document.querySelectorAll("[data-testid^='conversation-turn-']")
    );
    if (turns.length > 0) {
      return turns.map((el, i) => ({
        el,
        role: determineTurnRole(el, i),
        index: i,
      }));
    }

    // Strategy 3: article wrappers
    turns = Array.from(document.querySelectorAll("article"));
    if (turns.length > 0) {
      return turns.map((el, i) => ({
        el,
        role: determineTurnRole(el, i),
        index: i,
      }));
    }

    // Strategy 4: [data-message-author-role]
    turns = Array.from(document.querySelectorAll("[data-message-author-role]"));
    if (turns.length > 0) {
      return turns.map((el, i) => ({
        el,
        role: determineTurnRole(el, i),
        index: i,
      }));
    }

    // Strategy 5: Universal content markers inside main
    const main = document.querySelector("main") || document.body;
    const userNodes = Array.from(
      main.querySelectorAll(
        ".whitespace-pre-wrap, [class*='whitespace-pre-wrap']"
      )
    ).filter(
      (el) =>
        !el.closest("form") &&
        !el.closest("#prompt-textarea") &&
        !el.isContentEditable
    );
    const assistantNodes = Array.from(
      main.querySelectorAll(".markdown, [class*='prose']")
    );

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

  function logDomDiagnostic() {
    try {
      const main = document.querySelector("main") || document.body;
      const elements = Array.from(
        main.querySelectorAll(
          "[data-turn], [data-message-author-role], [data-testid^='conversation-turn-'], article, .markdown, .whitespace-pre-wrap"
        )
      ).slice(0, 16);

      console.log(
        "[OrbitBridge DOM Diagnostic] Detected nodes:",
        elements.map((el) => ({
          tag: el.tagName.toLowerCase(),
          turn: el.getAttribute("data-turn"),
          role: el.getAttribute("data-message-author-role"),
          testid: el.getAttribute("data-testid"),
          text: (el.innerText || "").trim().slice(0, 32),
        }))
      );
    } catch (_e) {}
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

      logDomDiagnostic();
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
