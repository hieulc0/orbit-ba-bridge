use bridge_core::{
    ActorRole, ConversationStatus, ConversationStore, MessageKind, NewConversation, NewMessage,
    NewParticipant, ParticipantSource, SqliteConversationStore,
};
use protocol::{ConversationId, CorrelationId, MessageId, ParticipantId};

#[tokio::test]
async fn test_monotonic_sequence_and_message_ordering() {
    let store = SqliteConversationStore::open_in_memory().unwrap();

    let conv = store
        .create_conversation(NewConversation {
            title: Some("Order Test".into()),
            ..Default::default()
        })
        .await
        .unwrap();

    let actor = store
        .add_participant(NewParticipant {
            id: Some(ParticipantId::new("actor-1").unwrap()),
            conversation_id: conv.id.clone(),
            role: ActorRole::BusinessAnalyst,
            display_name: "BA".into(),
            source: ParticipantSource::ChatGptWeb,
        })
        .await
        .unwrap();

    // Append 3 sequential messages
    let msg1 = store
        .append_message(NewMessage {
            id: None,
            conversation_id: conv.id.clone(),
            actor_id: actor.id.clone(),
            kind: MessageKind::Conversation,
            content: "First turn".into(),
            correlation_id: None,
            reply_to: None,
            external_message_id: None,
            orbit_workflow_id: None,
            orbit_role_execution_id: None,
        })
        .await
        .unwrap();

    let msg2 = store
        .append_message(NewMessage {
            id: None,
            conversation_id: conv.id.clone(),
            actor_id: actor.id.clone(),
            kind: MessageKind::Request,
            content: "Second turn".into(),
            correlation_id: None,
            reply_to: Some(msg1.id.clone()),
            external_message_id: None,
            orbit_workflow_id: None,
            orbit_role_execution_id: None,
        })
        .await
        .unwrap();

    let msg3 = store
        .append_message(NewMessage {
            id: None,
            conversation_id: conv.id.clone(),
            actor_id: actor.id.clone(),
            kind: MessageKind::Response,
            content: "Third turn".into(),
            correlation_id: None,
            reply_to: Some(msg2.id.clone()),
            external_message_id: None,
            orbit_workflow_id: None,
            orbit_role_execution_id: None,
        })
        .await
        .unwrap();

    assert_eq!(msg1.sequence, 1);
    assert_eq!(msg2.sequence, 2);
    assert_eq!(msg3.sequence, 3);

    // Test pagination with after_sequence
    let paged = store.load_messages(&conv.id, Some(1), 10).await.unwrap();
    assert_eq!(paged.len(), 2);
    assert_eq!(paged[0].id, msg2.id);
    assert_eq!(paged[1].id, msg3.id);
}

#[tokio::test]
async fn test_external_message_linking_and_lookup() {
    let store = SqliteConversationStore::open_in_memory().unwrap();

    let conv = store
        .create_conversation(NewConversation::default())
        .await
        .unwrap();

    let actor = store
        .add_participant(NewParticipant {
            id: Some(ParticipantId::new("actor-ba").unwrap()),
            conversation_id: conv.id.clone(),
            role: ActorRole::BusinessAnalyst,
            display_name: "BA".into(),
            source: ParticipantSource::ChatGptWeb,
        })
        .await
        .unwrap();

    let msg = store
        .append_message(NewMessage {
            id: None,
            conversation_id: conv.id.clone(),
            actor_id: actor.id.clone(),
            kind: MessageKind::Conversation,
            content: "Hello from ChatGPT".into(),
            correlation_id: None,
            reply_to: None,
            external_message_id: None,
            orbit_workflow_id: None,
            orbit_role_execution_id: None,
        })
        .await
        .unwrap();

    // Link external DOM message ID
    store
        .link_external_message(&msg.id, "dom-article-42")
        .await
        .unwrap();

    // Lookup by external ID
    let found = store
        .find_message_by_external_id(&conv.id, "dom-article-42")
        .await
        .unwrap()
        .expect("Message should be found by external DOM ID");

    assert_eq!(found.id, msg.id);
    assert_eq!(found.content, "Hello from ChatGPT");
    assert_eq!(found.external_message_id.as_deref(), Some("dom-article-42"));
}

#[tokio::test]
async fn test_persistence_across_reopen() {
    let temp_dir = tempfile::tempdir().unwrap();
    let db_path = temp_dir.path().join("bridge_test.sqlite");

    let conv_id: ConversationId;
    let msg_id: MessageId;

    // First session: create and persist
    {
        let store = SqliteConversationStore::open(&db_path).unwrap();

        let conv = store
            .create_conversation(NewConversation {
                title: Some("Durable Conversation".into()),
                orbit_workflow_id: Some("orbit-wf-99".into()),
                ..Default::default()
            })
            .await
            .unwrap();
        conv_id = conv.id.clone();

        let part = store
            .add_participant(NewParticipant {
                id: Some(ParticipantId::new("part-sa").unwrap()),
                conversation_id: conv.id.clone(),
                role: ActorRole::SystemArchitect,
                display_name: "Orbit SA".into(),
                source: ParticipantSource::Orbit,
            })
            .await
            .unwrap();

        let msg = store
            .append_message(NewMessage {
                id: None,
                conversation_id: conv.id.clone(),
                actor_id: part.id.clone(),
                kind: MessageKind::Challenge,
                content: "Persistent challenge content".into(),
                correlation_id: Some(CorrelationId::new("corr-123").unwrap()),
                reply_to: None,
                external_message_id: None,
                orbit_workflow_id: Some("orbit-wf-99".into()),
                orbit_role_execution_id: Some("exec-01".into()),
            })
            .await
            .unwrap();
        msg_id = msg.id.clone();
    }

    // Second session: reopen database from disk and verify state
    {
        let store = SqliteConversationStore::open(&db_path).unwrap();

        let conv = store
            .get_conversation(&conv_id)
            .await
            .unwrap()
            .expect("Conversation must exist");
        assert_eq!(conv.title.as_deref(), Some("Durable Conversation"));
        assert_eq!(conv.orbit_workflow_id.as_deref(), Some("orbit-wf-99"));
        assert_eq!(conv.status, ConversationStatus::Active);

        let participants = store.list_participants(&conv_id).await.unwrap();
        assert_eq!(participants.len(), 1);
        assert_eq!(participants[0].display_name, "Orbit SA");
        assert_eq!(participants[0].role, ActorRole::SystemArchitect);

        let msg = store
            .get_message(&msg_id)
            .await
            .unwrap()
            .expect("Message must exist");
        assert_eq!(msg.content, "Persistent challenge content");
        assert_eq!(msg.sequence, 1);
        assert_eq!(msg.kind, MessageKind::Challenge);
        assert_eq!(
            msg.correlation_id.as_ref().map(|c| c.as_str()),
            Some("corr-123")
        );
    }
}
