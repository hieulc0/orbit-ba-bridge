use protocol::{
    BrowserCommand, BrowserEvent, CURRENT_PROTOCOL_VERSION, CorrelationId, MAX_MESSAGE_TEXT_LENGTH,
    MessageEnvelope, ProtocolError, SessionId,
};

#[test]
fn test_round_trip_assistant_message_event() {
    let session_id = SessionId::new("session-123").unwrap();
    let correlation_id = CorrelationId::new("corr-456").unwrap();
    let event = BrowserEvent::AssistantMessage {
        message_id: "msg-789".to_string(),
        text: "Here is the architectural analysis.".to_string(),
    };

    let envelope = MessageEnvelope::new(session_id.clone(), correlation_id.clone(), event);
    let serialized = envelope.to_json().expect("serialization should succeed");

    assert!(serialized.contains(r#""version":1"#));
    assert!(serialized.contains(r#""session_id":"session-123""#));
    assert!(serialized.contains(r#""correlation_id":"corr-456""#));
    assert!(serialized.contains(r#""type":"assistant_message""#));
    assert!(serialized.contains(r#""message_id":"msg-789""#));

    let deserialized: MessageEnvelope<BrowserEvent> =
        MessageEnvelope::from_json_str(&serialized).expect("deserialization should succeed");

    assert_eq!(deserialized.version, CURRENT_PROTOCOL_VERSION);
    assert_eq!(deserialized.session_id, session_id);
    assert_eq!(deserialized.correlation_id, correlation_id);
    match deserialized.payload {
        BrowserEvent::AssistantMessage { message_id, text } => {
            assert_eq!(message_id, "msg-789");
            assert_eq!(text, "Here is the architectural analysis.");
        }
        other => panic!("unexpected event variant: {:?}", other),
    }
}

#[test]
fn test_round_trip_send_message_command() {
    let session_id = SessionId::new("session-abc").unwrap();
    let correlation_id = CorrelationId::new("corr-def").unwrap();
    let command = BrowserCommand::SendMessage {
        text: "Please elaborate on user story #3.".to_string(),
    };

    let envelope = MessageEnvelope::new(session_id.clone(), correlation_id.clone(), command);
    let serialized = envelope.to_json().expect("serialization should succeed");

    let deserialized: MessageEnvelope<BrowserCommand> =
        MessageEnvelope::from_json_str(&serialized).expect("deserialization should succeed");

    assert_eq!(deserialized.version, CURRENT_PROTOCOL_VERSION);
    match deserialized.payload {
        BrowserCommand::SendMessage { text } => {
            assert_eq!(text, "Please elaborate on user story #3.");
        }
        other => panic!("unexpected command variant: {:?}", other),
    }
}

#[test]
fn test_reject_unsupported_protocol_version() {
    let raw = r#"{
        "version": 999,
        "session_id": "session-1",
        "correlation_id": "corr-1",
        "type": "connected"
    }"#;

    let result = MessageEnvelope::<BrowserEvent>::from_json_str(raw);
    match result {
        Err(ProtocolError::UnsupportedVersion(999)) => {}
        other => panic!("expected UnsupportedVersion(999), got {:?}", other),
    }
}

#[test]
fn test_empty_identifiers_rejected() {
    assert_eq!(SessionId::new("   "), Err(ProtocolError::EmptySessionId));
    assert_eq!(
        CorrelationId::new(""),
        Err(ProtocolError::EmptyCorrelationId)
    );
}

#[test]
fn test_text_length_limit_validation() {
    let huge_text = "a".repeat(MAX_MESSAGE_TEXT_LENGTH + 1);
    let event = BrowserEvent::AssistantMessage {
        message_id: "m1".to_string(),
        text: huge_text.clone(),
    };
    assert!(matches!(
        event.validate(),
        Err(ProtocolError::TextTooLarge { .. })
    ));

    let cmd = BrowserCommand::SendMessage { text: huge_text };
    assert!(matches!(
        cmd.validate(),
        Err(ProtocolError::TextTooLarge { .. })
    ));
}
