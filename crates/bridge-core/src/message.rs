//! Conversation message models and message classifications.

use chrono::{DateTime, Utc};
use protocol::{ConversationId, CorrelationId, MessageId, ParticipantId};
use serde::{Deserialize, Serialize};

/// High-level classification of a message's conversational intent.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MessageKind {
    Conversation,
    Request,
    Response,
    Challenge,
    Decision,
    SystemEvent,
}

impl MessageKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Conversation => "conversation",
            Self::Request => "request",
            Self::Response => "response",
            Self::Challenge => "challenge",
            Self::Decision => "decision",
            Self::SystemEvent => "system_event",
        }
    }

    pub fn from_str_name(s: &str) -> Option<Self> {
        match s {
            "conversation" => Some(Self::Conversation),
            "request" => Some(Self::Request),
            "response" => Some(Self::Response),
            "challenge" => Some(Self::Challenge),
            "decision" => Some(Self::Decision),
            "system_event" => Some(Self::SystemEvent),
            _ => None,
        }
    }
}

/// An immutable message within a conversation with guaranteed monotonic sequence ordering.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConversationMessage {
    pub id: MessageId,
    pub conversation_id: ConversationId,
    pub sequence: u64,
    pub actor_id: ParticipantId,
    pub kind: MessageKind,
    pub content: String,
    pub correlation_id: Option<CorrelationId>,
    pub reply_to: Option<MessageId>,
    pub external_message_id: Option<String>,
    pub orbit_workflow_id: Option<String>,
    pub orbit_role_execution_id: Option<String>,
    pub created_at: DateTime<Utc>,
}

/// Input payload to append a message to a conversation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewMessage {
    pub id: Option<MessageId>,
    pub conversation_id: ConversationId,
    pub actor_id: ParticipantId,
    pub kind: MessageKind,
    pub content: String,
    pub correlation_id: Option<CorrelationId>,
    pub reply_to: Option<MessageId>,
    pub external_message_id: Option<String>,
    pub orbit_workflow_id: Option<String>,
    pub orbit_role_execution_id: Option<String>,
}
