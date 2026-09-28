//! Session lifecycle and deduplication management.

use protocol::{BrowserEvent, SessionId};
use std::collections::{HashSet, VecDeque};

pub const DEFAULT_MAX_SEEN_MESSAGES: usize = 1000;

/// Runtime lifecycle states of an ephemeral browser connection.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionState {
    Disconnected,
    Connecting,
    Connected,
    PageReady,
    ConversationReady,
    Unavailable,
}

/// Manages session state transitions and duplicate message suppression.
#[derive(Debug)]
pub struct SessionManager {
    session_id: SessionId,
    state: SessionState,
    conversation_id: Option<String>,
    external_conversation_ref: Option<String>,
    seen_messages: HashSet<String>,
    message_order: VecDeque<String>,
    max_seen_history: usize,
}

impl SessionManager {
    pub fn new(session_id: SessionId) -> Self {
        Self {
            session_id,
            state: SessionState::Disconnected,
            conversation_id: None,
            external_conversation_ref: None,
            seen_messages: HashSet::new(),
            message_order: VecDeque::new(),
            max_seen_history: DEFAULT_MAX_SEEN_MESSAGES,
        }
    }

    pub fn session_id(&self) -> &SessionId {
        &self.session_id
    }

    pub fn state(&self) -> SessionState {
        self.state
    }

    pub fn set_state(&mut self, state: SessionState) {
        self.state = state;
    }

    pub fn conversation_id(&self) -> Option<&str> {
        self.conversation_id.as_deref()
    }

    pub fn external_conversation_ref(&self) -> Option<&str> {
        self.external_conversation_ref.as_deref()
    }

    /// Update session state according to incoming event.
    /// Returns `true` if the event is a new unique message (or state event),
    /// and `false` if it is a duplicate message that should be ignored.
    pub fn handle_event(&mut self, event: &BrowserEvent) -> bool {
        match event {
            BrowserEvent::Hello { .. } => {
                self.state = SessionState::Connecting;
                true
            }
            BrowserEvent::Connected => {
                self.state = SessionState::Connected;
                true
            }
            BrowserEvent::PageReady { .. } => {
                self.state = SessionState::PageReady;
                true
            }
            BrowserEvent::ConversationDetected {
                conversation_id,
                external_conversation_ref,
            } => {
                let is_new_conv = match (&self.conversation_id, conversation_id) {
                    (Some(curr), Some(new_id)) => curr != new_id,
                    (None, Some(_)) => true,
                    _ => false,
                };
                if is_new_conv {
                    self.seen_messages.clear();
                    self.message_order.clear();
                }

                if let Some(c_id) = conversation_id {
                    self.conversation_id = Some(c_id.clone());
                }
                if let Some(ext_ref) = external_conversation_ref {
                    self.external_conversation_ref = Some(ext_ref.clone());
                } else if self.external_conversation_ref.is_none() {
                    self.external_conversation_ref = conversation_id.clone();
                }
                self.state = SessionState::ConversationReady;
                true
            }
            BrowserEvent::UserMessageObserved {
                external_message_id,
                ..
            }
            | BrowserEvent::AssistantMessageObserved {
                external_message_id,
                ..
            } => {
                if self.is_duplicate(external_message_id) {
                    false
                } else {
                    self.record_message(external_message_id.clone());
                    true
                }
            }
            BrowserEvent::AssistantMessage { message_id, .. }
            | BrowserEvent::UserMessage { message_id, .. } => {
                if self.is_duplicate(message_id) {
                    false
                } else {
                    self.record_message(message_id.clone());
                    true
                }
            }
            BrowserEvent::InjectionAccepted { .. } | BrowserEvent::InjectionMaterialized { .. } => {
                true
            }
            BrowserEvent::PageUnavailable { .. } | BrowserEvent::SessionUnavailable { .. } => {
                self.state = SessionState::Unavailable;
                true
            }
            BrowserEvent::Disconnected => {
                self.state = SessionState::Disconnected;
                true
            }
        }
    }

    /// Checks if a message ID has already been recorded.
    pub fn is_duplicate(&self, message_id: &str) -> bool {
        self.seen_messages.contains(message_id)
    }

    /// Records a new message ID, maintaining bounded memory.
    fn record_message(&mut self, message_id: String) {
        if self.seen_messages.insert(message_id.clone()) {
            self.message_order.push_back(message_id);
            if self.message_order.len() > self.max_seen_history
                && let Some(oldest) = self.message_order.pop_front()
            {
                self.seen_messages.remove(&oldest);
            }
        }
    }

    /// Reset connection state without clearing conversation references or message deduplication caches.
    pub fn reset_connection(&mut self) {
        self.state = SessionState::Disconnected;
    }
}
