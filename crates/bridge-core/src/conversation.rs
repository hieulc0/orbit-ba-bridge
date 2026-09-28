//! Conversation definitions and lifecycle status.

use chrono::{DateTime, Utc};
use protocol::ConversationId;
use serde::{Deserialize, Serialize};

/// Operational status of a conversation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConversationStatus {
    Active,
    Paused,
    Completed,
    Archived,
}

impl ConversationStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Active => "active",
            Self::Paused => "paused",
            Self::Completed => "completed",
            Self::Archived => "archived",
        }
    }

    pub fn from_str_name(s: &str) -> Option<Self> {
        match s {
            "active" => Some(Self::Active),
            "paused" => Some(Self::Paused),
            "completed" => Some(Self::Completed),
            "archived" => Some(Self::Archived),
            _ => None,
        }
    }
}

/// A logical discussion space containing participants and ordered messages.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Conversation {
    pub id: ConversationId,
    pub title: Option<String>,
    pub status: ConversationStatus,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub orbit_workflow_id: Option<String>,
    pub external_conversation_ref: Option<String>,
}

/// High-level summary of a conversation for listing and display.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConversationSummary {
    pub id: ConversationId,
    pub title: Option<String>,
    pub status: ConversationStatus,
    pub external_conversation_ref: Option<String>,
    pub message_count: u64,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Input payload to create a new conversation.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct NewConversation {
    pub id: Option<ConversationId>,
    pub title: Option<String>,
    pub orbit_workflow_id: Option<String>,
    pub external_conversation_ref: Option<String>,
}
