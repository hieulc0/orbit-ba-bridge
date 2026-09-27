//! Conversation store trait abstraction.
//!
//! Provides a narrow asynchronous interface for conversation persistence, decoupling
//! storage implementation details from domain logic.

use crate::conversation::{Conversation, ConversationStatus, NewConversation};
use crate::message::{ConversationMessage, NewMessage};
use crate::participant::{NewParticipant, Participant};
use async_trait::async_trait;
use protocol::{ConversationId, MessageId, ParticipantId};

#[async_trait]
pub trait ConversationStore: Send + Sync {
    /// Create a new conversation space.
    async fn create_conversation(
        &self,
        conversation: NewConversation,
    ) -> anyhow::Result<Conversation>;

    /// Retrieve a conversation by its identifier.
    async fn get_conversation(&self, id: &ConversationId) -> anyhow::Result<Option<Conversation>>;

    /// Update the operational status of a conversation.
    async fn update_conversation_status(
        &self,
        id: &ConversationId,
        status: ConversationStatus,
    ) -> anyhow::Result<()>;

    /// Add a participant to an existing conversation.
    async fn add_participant(&self, participant: NewParticipant) -> anyhow::Result<Participant>;

    /// Retrieve a participant by their identifier.
    async fn get_participant(&self, id: &ParticipantId) -> anyhow::Result<Option<Participant>>;

    /// List all participants registered in a conversation.
    async fn list_participants(
        &self,
        conversation_id: &ConversationId,
    ) -> anyhow::Result<Vec<Participant>>;

    /// Append a message to a conversation with a guaranteed monotonic sequence number.
    async fn append_message(&self, message: NewMessage) -> anyhow::Result<ConversationMessage>;

    /// Retrieve a single message by identifier.
    async fn get_message(&self, id: &MessageId) -> anyhow::Result<Option<ConversationMessage>>;

    /// Load ordered messages for a conversation, optionally paging after a sequence number.
    async fn load_messages(
        &self,
        conversation_id: &ConversationId,
        after_sequence: Option<u64>,
        limit: usize,
    ) -> anyhow::Result<Vec<ConversationMessage>>;

    /// Link a stored message to an external browser DOM message identifier.
    async fn link_external_message(
        &self,
        message_id: &MessageId,
        external_id: &str,
    ) -> anyhow::Result<()>;

    /// Find a message in a conversation linked to an external browser DOM message identifier.
    async fn find_message_by_external_id(
        &self,
        conversation_id: &ConversationId,
        external_id: &str,
    ) -> anyhow::Result<Option<ConversationMessage>>;
}
