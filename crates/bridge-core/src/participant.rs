//! Multi-actor participant definitions.
//!
//! Separates logical actor roles from runtime execution sources, allowing multiple
//! participants to instantiate the same role within a single conversation.

use chrono::{DateTime, Utc};
use protocol::{ConversationId, ParticipantId};
use serde::{Deserialize, Serialize};

/// High-level logical engineering and discussion roles.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ActorRole {
    Human,
    BusinessAnalyst,
    SystemArchitect,
    Implementer,
    Reviewer,
    SecurityReviewer,
    DomainExpert,
    System,
}

impl ActorRole {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Human => "human",
            Self::BusinessAnalyst => "business_analyst",
            Self::SystemArchitect => "system_architect",
            Self::Implementer => "implementer",
            Self::Reviewer => "reviewer",
            Self::SecurityReviewer => "security_reviewer",
            Self::DomainExpert => "domain_expert",
            Self::System => "system",
        }
    }

    pub fn from_str_name(s: &str) -> Option<Self> {
        match s {
            "human" => Some(Self::Human),
            "business_analyst" => Some(Self::BusinessAnalyst),
            "system_architect" => Some(Self::SystemArchitect),
            "implementer" => Some(Self::Implementer),
            "reviewer" => Some(Self::Reviewer),
            "security_reviewer" => Some(Self::SecurityReviewer),
            "domain_expert" => Some(Self::DomainExpert),
            "system" => Some(Self::System),
            _ => None,
        }
    }
}

/// Runtime source or execution environment hosting the participant.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ParticipantSource {
    HumanBrowser,
    ChatGptWeb,
    Orbit,
    ExternalAdapter,
    System,
}

impl ParticipantSource {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::HumanBrowser => "human_browser",
            Self::ChatGptWeb => "chat_gpt_web",
            Self::Orbit => "orbit",
            Self::ExternalAdapter => "external_adapter",
            Self::System => "system",
        }
    }

    pub fn from_str_name(s: &str) -> Option<Self> {
        match s {
            "human_browser" => Some(Self::HumanBrowser),
            "chat_gpt_web" => Some(Self::ChatGptWeb),
            "orbit" => Some(Self::Orbit),
            "external_adapter" => Some(Self::ExternalAdapter),
            "system" => Some(Self::System),
            _ => None,
        }
    }
}

/// A specific participant active in a conversation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Participant {
    pub id: ParticipantId,
    pub conversation_id: ConversationId,
    pub role: ActorRole,
    pub display_name: String,
    pub source: ParticipantSource,
    pub created_at: DateTime<Utc>,
}

/// Input payload to register a new participant in a conversation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewParticipant {
    pub id: Option<ParticipantId>,
    pub conversation_id: ConversationId,
    pub role: ActorRole,
    pub display_name: String,
    pub source: ParticipantSource,
}
