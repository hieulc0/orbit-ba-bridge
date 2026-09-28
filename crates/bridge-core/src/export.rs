//! Export utilities for conversations into Markdown and JSON representations.

use crate::conversation::Conversation;
use crate::message::ConversationMessage;
use crate::participant::Participant;
use protocol::ParticipantId;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Exports a conversation, its participants, and ordered messages into clean Markdown.
pub fn export_markdown(
    conversation: &Conversation,
    participants: &[Participant],
    messages: &[ConversationMessage],
) -> String {
    let mut out = String::new();
    out.push_str("# Conversation\n\n");
    out.push_str(&format!("Conversation ID: {}\n", conversation.id));
    if let Some(ext) = &conversation.external_conversation_ref {
        out.push_str(&format!("ChatGPT conversation: {}\n", ext));
    }
    out.push('\n');

    let part_map: HashMap<&ParticipantId, &str> = participants
        .iter()
        .map(|p| (&p.id, p.display_name.as_str()))
        .collect();

    for msg in messages {
        let actor_name = part_map
            .get(&msg.actor_id)
            .copied()
            .unwrap_or_else(|| msg.actor_id.as_str());
        out.push_str(&format!("## {}\n\n", actor_name));
        out.push_str(msg.content.trim());
        out.push_str("\n\n");
    }

    out
}

/// Normalized JSON export envelope for a conversation referencing borrowed domain objects.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ConversationExportData<'a> {
    pub conversation: &'a Conversation,
    pub participants: &'a [Participant],
    pub messages: &'a [ConversationMessage],
}

/// Owned normalized JSON export envelope for round-trip deserialization.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OwnedConversationExport {
    pub conversation: Conversation,
    pub participants: Vec<Participant>,
    pub messages: Vec<ConversationMessage>,
}

/// Exports a conversation into normalized, pretty-printed JSON.
pub fn export_json(
    conversation: &Conversation,
    participants: &[Participant],
    messages: &[ConversationMessage],
) -> Result<String, serde_json::Error> {
    let payload = ConversationExportData {
        conversation,
        participants,
        messages,
    };
    serde_json::to_string_pretty(&payload)
}
