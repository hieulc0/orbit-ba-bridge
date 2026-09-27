//! Context projection for model turns.
//!
//! Separates durable conversation history from the bounded context window projected to each actor.

use crate::artifact::ArtifactRef;
use crate::message::MessageKind;
use crate::participant::ActorRole;
use protocol::ParticipantId;
use serde::{Deserialize, Serialize};

/// A normalized message projected into an actor's input context.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProjectedMessage {
    pub sequence: u64,
    pub sender_name: String,
    pub sender_role: ActorRole,
    pub kind: MessageKind,
    pub content: String,
}

/// The bounded context provided to an actor for a specific turn.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContextProjection {
    pub target_participant: ParticipantId,
    pub messages: Vec<ProjectedMessage>,
    pub artifact_refs: Vec<ArtifactRef>,
}

impl ContextProjection {
    pub fn new(target_participant: ParticipantId) -> Self {
        Self {
            target_participant,
            messages: Vec::new(),
            artifact_refs: Vec::new(),
        }
    }

    pub fn with_messages(mut self, messages: Vec<ProjectedMessage>) -> Self {
        self.messages = messages;
        self
    }

    pub fn with_artifacts(mut self, artifact_refs: Vec<ArtifactRef>) -> Self {
        self.artifact_refs = artifact_refs;
        self
    }
}
