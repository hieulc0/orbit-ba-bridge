use bridge_core::{
    ActorRole, ContextProjection, ConversationStore, MessageKind, NewConversation, NewParticipant,
    ParticipantSource, ProjectedMessage, SqliteConversationStore,
};
use protocol::ParticipantId;

#[tokio::test]
async fn test_multiple_actors_with_same_role() {
    let store = SqliteConversationStore::open_in_memory().unwrap();

    let conv = store
        .create_conversation(NewConversation {
            title: Some("Requirements Clarification".into()),
            ..Default::default()
        })
        .await
        .unwrap();

    // Register Human participant
    let human = store
        .add_participant(NewParticipant {
            id: Some(ParticipantId::new("human-hieu").unwrap()),
            conversation_id: conv.id.clone(),
            role: ActorRole::Human,
            display_name: "Hiếu".into(),
            source: ParticipantSource::HumanBrowser,
        })
        .await
        .unwrap();

    // Register first Business Analyst: BA Product
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

    // Register second Business Analyst: BA Research
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

    // Register System Architect: Orbit SA
    let orbit_sa = store
        .add_participant(NewParticipant {
            id: Some(ParticipantId::new("sa-orbit").unwrap()),
            conversation_id: conv.id.clone(),
            role: ActorRole::SystemArchitect,
            display_name: "Orbit SA".into(),
            source: ParticipantSource::Orbit,
        })
        .await
        .unwrap();

    // Verify all 4 participants are distinct
    let participants = store.list_participants(&conv.id).await.unwrap();
    assert_eq!(participants.len(), 4);

    assert_eq!(human.role, ActorRole::Human);
    assert_eq!(human.source, ParticipantSource::HumanBrowser);

    // Both BAs have the same ActorRole::BusinessAnalyst but distinct ParticipantIds and names
    assert_eq!(ba_product.role, ActorRole::BusinessAnalyst);
    assert_eq!(ba_research.role, ActorRole::BusinessAnalyst);
    assert_ne!(ba_product.id, ba_research.id);
    assert_eq!(ba_product.display_name, "BA Product");
    assert_eq!(ba_research.display_name, "BA Research");

    assert_eq!(orbit_sa.role, ActorRole::SystemArchitect);
    assert_eq!(orbit_sa.source, ParticipantSource::Orbit);
}

#[test]
fn test_context_projection_for_actor() {
    let target = ParticipantId::new("ba-product").unwrap();
    let projection = ContextProjection::new(target.clone()).with_messages(vec![
        ProjectedMessage {
            sequence: 1,
            sender_name: "Hiếu".into(),
            sender_role: ActorRole::Human,
            kind: MessageKind::Conversation,
            content: "We need dual-browser support".into(),
        },
        ProjectedMessage {
            sequence: 2,
            sender_name: "Orbit SA".into(),
            sender_role: ActorRole::SystemArchitect,
            kind: MessageKind::Request,
            content: "Can you analyze requirements for Firefox?".into(),
        },
    ]);

    assert_eq!(projection.target_participant, target);
    assert_eq!(projection.messages.len(), 2);
    assert_eq!(projection.messages[0].sender_role, ActorRole::Human);
    assert_eq!(
        projection.messages[1].sender_role,
        ActorRole::SystemArchitect
    );
}
