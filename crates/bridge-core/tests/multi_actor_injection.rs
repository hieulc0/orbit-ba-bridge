use bridge_core::{
    ActorRole, ConversationStore, InjectionLedger, MessageKind, NewConversation, NewMessage,
    NewParticipant, ParticipantSource, SqliteConversationStore, format_external_injection,
};
use protocol::{CorrelationId, ParticipantId};

#[tokio::test]
async fn test_multi_actor_lifecycle_and_injection_ledger() {
    let store = SqliteConversationStore::open_in_memory().unwrap();
    let ledger = InjectionLedger::new();

    // 1. Create shared conversation
    let conv = store
        .create_conversation(NewConversation {
            title: Some("Multi-Actor System Design".into()),
            orbit_workflow_id: Some("orbit-flow-001".into()),
            ..Default::default()
        })
        .await
        .unwrap();

    // 2. Register distinct participants
    let human = store
        .add_participant(NewParticipant {
            id: Some(ParticipantId::new("human-user").unwrap()),
            conversation_id: conv.id.clone(),
            role: ActorRole::Human,
            display_name: "Hiếu".into(),
            source: ParticipantSource::HumanBrowser,
        })
        .await
        .unwrap();

    let ba_product = store
        .add_participant(NewParticipant {
            id: Some(ParticipantId::new("ba-product").unwrap()),
            conversation_id: conv.id.clone(),
            role: ActorRole::BusinessAnalyst,
            display_name: "BA Product".into(),
            source: ParticipantSource::ChatGptWeb,
        })
        .await
        .unwrap();

    let ba_research = store
        .add_participant(NewParticipant {
            id: Some(ParticipantId::new("ba-research").unwrap()),
            conversation_id: conv.id.clone(),
            role: ActorRole::BusinessAnalyst,
            display_name: "BA Research".into(),
            source: ParticipantSource::ChatGptWeb,
        })
        .await
        .unwrap();

    let orbit_sa = store
        .add_participant(NewParticipant {
            id: Some(ParticipantId::new("orbit-sa").unwrap()),
            conversation_id: conv.id.clone(),
            role: ActorRole::SystemArchitect,
            display_name: "Orbit SA".into(),
            source: ParticipantSource::Orbit,
        })
        .await
        .unwrap();

    // -------------------------------------------------------------
    // Scenario Step A: Human manually types in ChatGPT Web
    // -------------------------------------------------------------
    let human_text = "Let's design the dual-browser extension architecture.";
    let resolved_actor = ledger
        .resolve_user_message(None, human_text)
        .unwrap_or_else(|| human.id.clone());

    assert_eq!(resolved_actor, human.id);

    let msg_human = store
        .append_message(NewMessage {
            id: None,
            conversation_id: conv.id.clone(),
            actor_id: resolved_actor,
            kind: MessageKind::Conversation,
            content: human_text.into(),
            correlation_id: None,
            reply_to: None,
            external_message_id: Some("dom-human-msg-1".into()),
            orbit_workflow_id: None,
            orbit_role_execution_id: None,
        })
        .await
        .unwrap();
    assert_eq!(msg_human.sequence, 1);
    assert_eq!(msg_human.actor_id, human.id);

    // -------------------------------------------------------------
    // Scenario Step B: ChatGPT BA responds to Human
    // -------------------------------------------------------------
    let ba_text = "I recommend decoupling provider concepts from driver abstractions.";
    let msg_ba1 = store
        .append_message(NewMessage {
            id: None,
            conversation_id: conv.id.clone(),
            actor_id: ba_product.id.clone(),
            kind: MessageKind::Response,
            content: ba_text.into(),
            correlation_id: None,
            reply_to: Some(msg_human.id.clone()),
            external_message_id: Some("dom-ba-msg-1".into()),
            orbit_workflow_id: None,
            orbit_role_execution_id: None,
        })
        .await
        .unwrap();
    assert_eq!(msg_ba1.sequence, 2);
    assert_eq!(msg_ba1.actor_id, ba_product.id);

    // -------------------------------------------------------------
    // Scenario Step C: Orbit SA injects turn into ChatGPT Web
    // -------------------------------------------------------------
    let sa_content = "I propose separating provider selection from credential availability.";
    let formatted_prompt = format_external_injection("Orbit SA", sa_content);
    let sa_corr = CorrelationId::new("corr-sa-001").unwrap();

    // Record in injection ledger
    ledger.record_injection(sa_corr.clone(), orbit_sa.id.clone(), &formatted_prompt);
    assert_eq!(ledger.pending_count(), 1);

    // ChatGPT DOM detects native user message matching the injected composer input
    let resolved_sa_actor = ledger
        .resolve_user_message(Some(&sa_corr), &formatted_prompt)
        .expect("Injected message must resolve to the registered external actor");

    // Critical Invariant: Despite being a native user message in DOM, logical actor is Orbit SA!
    assert_eq!(resolved_sa_actor, orbit_sa.id);
    assert_eq!(ledger.pending_count(), 0);

    let msg_sa = store
        .append_message(NewMessage {
            id: None,
            conversation_id: conv.id.clone(),
            actor_id: resolved_sa_actor,
            kind: MessageKind::Request,
            content: sa_content.into(),
            correlation_id: Some(sa_corr.clone()),
            reply_to: Some(msg_ba1.id.clone()),
            external_message_id: Some("dom-user-composer-msg-2".into()),
            orbit_workflow_id: Some("orbit-flow-001".into()),
            orbit_role_execution_id: Some("sa-exec-1".into()),
        })
        .await
        .unwrap();
    assert_eq!(msg_sa.sequence, 3);
    assert_eq!(msg_sa.actor_id, orbit_sa.id);

    // -------------------------------------------------------------
    // Scenario Step D: ChatGPT BA responds to SA
    // -------------------------------------------------------------
    let ba_response_to_sa = "Requirements updated: credential rotation is decoupled.";
    let msg_ba2 = store
        .append_message(NewMessage {
            id: None,
            conversation_id: conv.id.clone(),
            actor_id: ba_product.id.clone(),
            kind: MessageKind::Response,
            content: ba_response_to_sa.into(),
            correlation_id: Some(sa_corr.clone()),
            reply_to: Some(msg_sa.id.clone()),
            external_message_id: Some("dom-ba-msg-2".into()),
            orbit_workflow_id: Some("orbit-flow-001".into()),
            orbit_role_execution_id: None,
        })
        .await
        .unwrap();
    assert_eq!(msg_ba2.sequence, 4);

    // -------------------------------------------------------------
    // Scenario Step E: BA Research injects findings
    // -------------------------------------------------------------
    let research_content = "External benchmark indicates Firefox MV3 background script difference.";
    let research_corr = CorrelationId::new("corr-res-002").unwrap();
    let research_prompt = format_external_injection("BA Research", research_content);

    ledger.record_injection(
        research_corr.clone(),
        ba_research.id.clone(),
        &research_prompt,
    );
    let resolved_research_actor = ledger
        .resolve_user_message(Some(&research_corr), &research_prompt)
        .unwrap();

    assert_eq!(resolved_research_actor, ba_research.id);

    let msg_res = store
        .append_message(NewMessage {
            id: None,
            conversation_id: conv.id.clone(),
            actor_id: resolved_research_actor,
            kind: MessageKind::Challenge,
            content: research_content.into(),
            correlation_id: Some(research_corr),
            reply_to: Some(msg_ba2.id.clone()),
            external_message_id: Some("dom-user-composer-msg-3".into()),
            orbit_workflow_id: Some("orbit-flow-001".into()),
            orbit_role_execution_id: None,
        })
        .await
        .unwrap();
    assert_eq!(msg_res.sequence, 5);
    assert_eq!(msg_res.actor_id, ba_research.id);

    // -------------------------------------------------------------
    // Verification: Load full conversation history from SQLite
    // -------------------------------------------------------------
    let all_messages = store.load_messages(&conv.id, None, 100).await.unwrap();
    assert_eq!(all_messages.len(), 5);

    // Sequence is strictly monotonic: 1, 2, 3, 4, 5
    for (idx, msg) in all_messages.iter().enumerate() {
        assert_eq!(msg.sequence, (idx + 1) as u64);
    }

    // Verify participants map to the expected actors
    assert_eq!(all_messages[0].actor_id, human.id);
    assert_eq!(all_messages[1].actor_id, ba_product.id);
    assert_eq!(all_messages[2].actor_id, orbit_sa.id);
    assert_eq!(all_messages[3].actor_id, ba_product.id);
    assert_eq!(all_messages[4].actor_id, ba_research.id);
}
