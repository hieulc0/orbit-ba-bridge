use bridge_core::{
    ActorRole, ConversationStore, MessageKind, NewConversation, NewMessage, NewParticipant,
    ParticipantSource, SqliteConversationStore, export_json, export_markdown,
};
use protocol::{ConversationId, ParticipantId};
use rusqlite::Connection;
use tempfile::NamedTempFile;

#[tokio::test]
async fn test_sqlite_schema_migration_v1_to_v2() -> anyhow::Result<()> {
    let tmp_file = NamedTempFile::new()?;
    let path = tmp_file.path().to_path_buf();

    // Step 1: Simulate a pre-existing schema version 1 database
    {
        let conn = Connection::open(&path)?;
        conn.execute_batch(
            "PRAGMA foreign_keys = ON;
             PRAGMA journal_mode = WAL;

             CREATE TABLE IF NOT EXISTS conversations (
                 id TEXT PRIMARY KEY,
                 title TEXT,
                 status TEXT NOT NULL,
                 orbit_workflow_id TEXT,
                 external_conversation_ref TEXT,
                 created_at INTEGER NOT NULL,
                 updated_at INTEGER NOT NULL
             );

             CREATE TABLE IF NOT EXISTS participants (
                 id TEXT PRIMARY KEY,
                 conversation_id TEXT NOT NULL,
                 role TEXT NOT NULL,
                 display_name TEXT NOT NULL,
                 source TEXT NOT NULL,
                 created_at INTEGER NOT NULL,
                 FOREIGN KEY (conversation_id) REFERENCES conversations(id)
             );

             CREATE TABLE IF NOT EXISTS messages (
                 id TEXT PRIMARY KEY,
                 conversation_id TEXT NOT NULL,
                 sequence INTEGER NOT NULL,
                 actor_id TEXT NOT NULL,
                 kind TEXT NOT NULL,
                 content TEXT NOT NULL,
                 correlation_id TEXT,
                 reply_to TEXT,
                 external_message_id TEXT,
                 orbit_workflow_id TEXT,
                 orbit_role_execution_id TEXT,
                 created_at INTEGER NOT NULL,
                 UNIQUE (conversation_id, sequence),
                 FOREIGN KEY (conversation_id) REFERENCES conversations(id),
                 FOREIGN KEY (actor_id) REFERENCES participants(id)
             );

             CREATE TABLE IF NOT EXISTS external_message_links (
                 message_id TEXT NOT NULL,
                 external_id TEXT NOT NULL,
                 created_at INTEGER NOT NULL,
                 PRIMARY KEY (message_id, external_id)
             );

             CREATE TABLE IF NOT EXISTS artifact_refs (
                 id TEXT PRIMARY KEY,
                 conversation_id TEXT NOT NULL,
                 artifact_type TEXT NOT NULL,
                 digest TEXT,
                 orbit_artifact_id TEXT,
                 created_at INTEGER NOT NULL
             );

             INSERT INTO conversations VALUES ('conv-v1', 'Pre-existing chat', 'active', NULL, 'ext-v1', 1000, 1000);
             PRAGMA user_version = 1;",
        )?;
    }

    // Step 2: Open with SqliteConversationStore - should run forward migration to v2
    let store = SqliteConversationStore::open(&path)?;
    assert_eq!(store.schema_version()?, 2);

    // Verify existing data preserved
    let conv = store.find_conversation_by_external_ref("ext-v1").await?;
    assert!(conv.is_some());
    let conv = conv.unwrap();
    assert_eq!(conv.id.as_str(), "conv-v1");
    assert_eq!(conv.title.as_deref(), Some("Pre-existing chat"));

    Ok(())
}

#[tokio::test]
async fn test_conversation_binding_and_lookup() -> anyhow::Result<()> {
    let store = SqliteConversationStore::open_in_memory()?;

    // 1. Create temporary unbound conversation (simulating chatgpt.com/ home before /c/<id> appears)
    let conv = store
        .create_conversation(NewConversation {
            id: None,
            title: Some("Initial Draft".into()),
            orbit_workflow_id: None,
            external_conversation_ref: None,
        })
        .await?;
    assert!(conv.external_conversation_ref.is_none());

    // Verify lookup by external ref returns None
    let not_found = store
        .find_conversation_by_external_ref("chat-uuid-123")
        .await?;
    assert!(not_found.is_none());

    // 2. Bind to external conversation ref
    store.bind_external_ref(&conv.id, "chat-uuid-123").await?;

    // 3. Verify lookup finds the bound conversation
    let found = store
        .find_conversation_by_external_ref("chat-uuid-123")
        .await?;
    assert!(found.is_some());
    let found = found.unwrap();
    assert_eq!(found.id, conv.id);
    assert_eq!(
        found.external_conversation_ref.as_deref(),
        Some("chat-uuid-123")
    );

    Ok(())
}

#[tokio::test]
async fn test_conversation_list_and_search() -> anyhow::Result<()> {
    let store = SqliteConversationStore::open_in_memory()?;

    let conv1 = store
        .create_conversation(NewConversation {
            id: None,
            title: Some("Architecture Spec".into()),
            orbit_workflow_id: None,
            external_conversation_ref: Some("c/arch-01".into()),
        })
        .await?;

    let human = store
        .add_participant(NewParticipant {
            id: None,
            conversation_id: conv1.id.clone(),
            role: ActorRole::Human,
            display_name: "Human".into(),
            source: ParticipantSource::HumanBrowser,
        })
        .await?;

    let ba = store
        .add_participant(NewParticipant {
            id: None,
            conversation_id: conv1.id.clone(),
            role: ActorRole::BusinessAnalyst,
            display_name: "BA Product".into(),
            source: ParticipantSource::ChatGptWeb,
        })
        .await?;

    store
        .append_message(NewMessage {
            id: None,
            conversation_id: conv1.id.clone(),
            actor_id: human.id.clone(),
            kind: MessageKind::Conversation,
            content: "Please review the authentication token mechanism.".into(),
            correlation_id: None,
            reply_to: None,
            external_message_id: Some("ext-msg-1".into()),
            orbit_workflow_id: None,
            orbit_role_execution_id: None,
        })
        .await?;

    store
        .append_message(NewMessage {
            id: None,
            conversation_id: conv1.id.clone(),
            actor_id: ba.id.clone(),
            kind: MessageKind::Response,
            content: "I recommend using local token files under ~/.local/state.".into(),
            correlation_id: None,
            reply_to: None,
            external_message_id: Some("ext-msg-2".into()),
            orbit_workflow_id: None,
            orbit_role_execution_id: None,
        })
        .await?;

    // 1. Verify list_conversations includes message counts
    let summaries = store.list_conversations(10, 0).await?;
    assert_eq!(summaries.len(), 1);
    assert_eq!(summaries[0].id, conv1.id);
    assert_eq!(summaries[0].message_count, 2);

    // 2. Search messages
    let search_results = store.search_messages("authentication token", 10).await?;
    assert_eq!(search_results.len(), 1);
    assert_eq!(search_results[0].conversation_id, conv1.id);
    assert_eq!(search_results[0].actor_display_name, "Human");
    assert!(
        search_results[0]
            .content_snippet
            .contains("authentication token")
    );

    let search_ba = store.search_messages("local/state", 10).await?;
    assert_eq!(search_ba.len(), 1);
    assert_eq!(search_ba[0].actor_display_name, "BA Product");

    Ok(())
}

#[tokio::test]
async fn test_conversation_export_markdown_and_json() -> anyhow::Result<()> {
    let store = SqliteConversationStore::open_in_memory()?;

    let conv = store
        .create_conversation(NewConversation {
            id: Some(ConversationId::new("conv-export-test")?),
            title: Some("UX Discussion".into()),
            orbit_workflow_id: None,
            external_conversation_ref: Some("c/ux-101".into()),
        })
        .await?;

    let human = store
        .add_participant(NewParticipant {
            id: Some(ParticipantId::new("part-human")?),
            conversation_id: conv.id.clone(),
            role: ActorRole::Human,
            display_name: "Human".into(),
            source: ParticipantSource::HumanBrowser,
        })
        .await?;

    let ba = store
        .add_participant(NewParticipant {
            id: Some(ParticipantId::new("part-ba")?),
            conversation_id: conv.id.clone(),
            role: ActorRole::BusinessAnalyst,
            display_name: "BA Product".into(),
            source: ParticipantSource::ChatGptWeb,
        })
        .await?;

    store
        .append_message(NewMessage {
            id: None,
            conversation_id: conv.id.clone(),
            actor_id: human.id.clone(),
            kind: MessageKind::Conversation,
            content: "Design credential status UX.".into(),
            correlation_id: None,
            reply_to: None,
            external_message_id: Some("turn-1".into()),
            orbit_workflow_id: None,
            orbit_role_execution_id: None,
        })
        .await?;

    store
        .append_message(NewMessage {
            id: None,
            conversation_id: conv.id.clone(),
            actor_id: ba.id.clone(),
            kind: MessageKind::Response,
            content: "I suggest separating availability from authentication state.".into(),
            correlation_id: None,
            reply_to: None,
            external_message_id: Some("turn-2".into()),
            orbit_workflow_id: None,
            orbit_role_execution_id: None,
        })
        .await?;

    let full_conv = store.get_conversation_with_messages(&conv.id).await?;
    assert!(full_conv.is_some());
    let (c, parts, msgs) = full_conv.unwrap();

    // Export to Markdown
    let md = export_markdown(&c, &parts, &msgs);
    assert!(md.contains("# Conversation\n\n"));
    assert!(md.contains("Conversation ID: conv-export-test"));
    assert!(md.contains("ChatGPT conversation: c/ux-101"));
    assert!(md.contains("## Human\n\nDesign credential status UX."));
    assert!(
        md.contains(
            "## BA Product\n\nI suggest separating availability from authentication state."
        )
    );

    // Export to JSON
    let json_str = export_json(&c, &parts, &msgs)?;
    let parsed: bridge_core::OwnedConversationExport = serde_json::from_str(&json_str)?;
    assert_eq!(parsed.conversation.id, conv.id);
    assert_eq!(parsed.participants.len(), 2);
    assert_eq!(parsed.messages.len(), 2);
    assert_eq!(parsed.messages[0].content, "Design credential status UX.");
    assert_eq!(
        parsed.messages[1].content,
        "I suggest separating availability from authentication state."
    );

    Ok(())
}
