/**
 * DOM observer and message extraction for ChatGPT web interface.
 *
 * Implements:
 * 1. Stable, deterministic message ID extraction:
 *    - Native ChatGPT selection UUID: data-chatgpt-selection-message-id
 *    - Search unit UUIDs: data-chatgpt-search-message-ids
 *    - Deterministic content hash with occurrence index (never relies on shifting DOM array indices).
 * 2. Active streaming detection (isChatGPTGenerating):
 *    - Detects generation stop button: button[data-testid="stop-button"], button[aria-label*="Stop"]
 *    - Suppresses partial assistant chunks while generating; only emits once generation stabilizes.
 * 3. Accurate turn identification using ChatGPT's modern container and role tags:
 *    - [data-chatgpt-search-unit-key], [data-content-search-unit-key]
 *    - [data-conversation-role] headings (ChatGPT said vs You said)
 *    - [data-markdown-text-style="assistant-message"]
 * 4. Deduplication of nested DOM wrappers and echo suppression for both Human and Assistant messages.
 */

(function () {
  function hashString(str) {
    let hash = 0;
    for (let i = 0; i < str.length; i++) {
      hash = (hash << 5) - hash + str.charCodeAt(i);
      hash |= 0;
    }
    return (hash >>> 0).toString(36);
  }

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

  function isChatGPTGenerating() {
    return Boolean(
      document.querySelector(
        "button[data-testid='stop-button'], button[aria-label*='Stop generating'], button[aria-label*='Stop'], .result-streaming, [class*='result-streaming']"
      )
    );
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

  function extractTurnId(el, role, text, occurrence, convId) {
    // 1. Direct native UUID on element or children/ancestors
    const directId =
      el.getAttribute?.("data-chatgpt-selection-message-id") ||
      el.querySelector?.("[data-chatgpt-selection-message-id]")?.getAttribute("data-chatgpt-selection-message-id") ||
      el.closest?.("[data-chatgpt-selection-message-id]")?.getAttribute("data-chatgpt-selection-message-id") ||
      el.getAttribute?.("data-message-id") ||
      el.querySelector?.("[data-message-id]")?.getAttribute("data-message-id");

    if (directId) return directId;

    // 2. Search message IDs attribute (e.g. data-chatgpt-search-message-ids="<UUID> <UUID>")
    const searchMsgIds =
      el.getAttribute?.("data-chatgpt-search-message-ids") ||
      el.querySelector?.("[data-chatgpt-search-message-ids]")?.getAttribute("data-chatgpt-search-message-ids");
    if (searchMsgIds) {
      const firstUuid = searchMsgIds.trim().split(/\s+/)[0];
      if (firstUuid && firstUuid.length >= 8) return firstUuid;
    }

    // 3. Completely deterministic content hash + occurrence index
    const hash = hashString(text.trim());
    return `${convId || "conv"}-${role}-${occurrence}-${hash}`;
  }

  function findConversationTurns() {
    // Strategy 1: Search unit key containers (filtering out nested descendants)
    const unitContainers = Array.from(
      document.querySelectorAll(
        "[data-chatgpt-search-unit-key], [data-content-search-unit-key]"
      )
    );
    if (unitContainers.length > 0) {
      const topLevelUnits = unitContainers.filter((el, idx) => {
        return !unitContainers.some(
          (other, oIdx) => oIdx !== idx && other.contains(el)
        );
      });

      return topLevelUnits.map((el, i) => ({
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

    // Strategy 3: Assistant markdown containers + User bubbles
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
      this.seenUserTexts = new Set(); // Suppress duplicate user message emissions
      this.seenFinalTexts = new Set(); // Suppress duplicate assistant emissions
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
      }, 1000);

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
    }

    detectConversation() {
      const match = window.location.pathname.match(/\/c\/([a-zA-Z0-9-]+)/);
      const convId = match ? match[1] : null;
      if (convId !== this.currentConversationId) {
        console.log(`[OrbitBridge DOM] Conversation detected: ${convId}`);
        this.currentConversationId = convId;
        this.seenMessageIds.clear();
        this.seenUserTexts.clear();
        this.seenFinalTexts.clear();
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
      const rawTurns = findConversationTurns();
      if (rawTurns.length === 0) return;

      // Extract valid text turns
      const validTurns = [];
      const occurrenceCount = new Map();

      rawTurns.forEach((turn) => {
        const { el, role } = turn;
        const text = extractTurnText(el, role);
        if (
          !text ||
          text === "Ready when you are." ||
          text === "What can I help with today?"
        ) {
          return;
        }

        const occKey = `${role}:${text}`;
        const occ = occurrenceCount.get(occKey) || 0;
        occurrenceCount.set(occKey, occ + 1);

        const messageId = extractTurnId(el, role, text, occ, convId);
        validTurns.push({ el, role, text, messageId, occ });
      });

      // Collapse adjacent duplicate turn wrappers (e.g. nested outer/inner search unit elements)
      const collapsedTurns = [];
      validTurns.forEach((turn) => {
        const last = collapsedTurns[collapsedTurns.length - 1];
        if (last && last.role === turn.role && last.text === turn.text) {
          // If this duplicate element has the native UUID, prefer it
          if (turn.messageId && !turn.messageId.startsWith(convId || "conv")) {
            collapsedTurns[collapsedTurns.length - 1] = turn;
          }
          return;
        }
        collapsedTurns.push(turn);
      });

      const isGenerating = isChatGPTGenerating();

      collapsedTurns.forEach((turn, idx) => {
        const { role, text, messageId } = turn;
        const isLastTurn = idx === collapsedTurns.length - 1;

        if (role === "user") {
          this.handleUserMessage(messageId, text);
        } else {
          this.handleAssistantMessage(messageId, text, isLastTurn, isGenerating);
        }
      });
    }

    handleUserMessage(messageId, text) {
      if (this.seenMessageIds.has(messageId)) {
        return;
      }

      // If we already captured a user message with this exact text in this thread, skip duplicate DOM wrapper
      if (this.seenUserTexts.has(text)) {
        this.seenMessageIds.add(messageId);
        return;
      }

      this.seenMessageIds.add(messageId);
      this.seenUserTexts.add(text);

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

    handleAssistantMessage(messageId, text, isLastTurn, isGenerating) {
      if (this.seenMessageIds.has(messageId)) {
        return;
      }

      // If this is the active last turn and ChatGPT is still generating, wait until complete
      if (isLastTurn && isGenerating) {
        return;
      }

      // If we already finalized an assistant message with this exact text in this thread, skip
      if (this.seenFinalTexts.has(text)) {
        this.seenMessageIds.add(messageId);
        return;
      }

      this.seenMessageIds.add(messageId);
      this.seenFinalTexts.add(text);

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
