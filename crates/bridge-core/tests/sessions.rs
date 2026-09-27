use bridge_core::{SessionManager, SessionState};
use protocol::{BrowserEvent, SessionId};

#[test]
fn test_session_lifecycle_transitions() {
    let session_id = SessionId::new("test-session-1").unwrap();
    let mut manager = SessionManager::new(session_id.clone());

    assert_eq!(manager.state(), SessionState::Disconnected);
    assert_eq!(manager.conversation_id(), None);

    // Connected event
    assert!(manager.handle_event(&BrowserEvent::Connected));
    assert_eq!(manager.state(), SessionState::Connected);

    // PageReady event
    assert!(manager.handle_event(&BrowserEvent::PageReady));
    assert_eq!(manager.state(), SessionState::PageReady);

    // ConversationDetected event
    assert!(manager.handle_event(&BrowserEvent::ConversationDetected {
        conversation_id: Some("conv-abc-123".to_string()),
    }));
    assert_eq!(manager.state(), SessionState::ActiveConversation);
    assert_eq!(manager.conversation_id(), Some("conv-abc-123"));

    // SessionUnavailable event
    assert!(manager.handle_event(&BrowserEvent::SessionUnavailable {
        reason: "Page crashed".to_string(),
    }));
    assert_eq!(manager.state(), SessionState::Unavailable);
}

#[test]
fn test_duplicate_message_suppression() {
    let session_id = SessionId::new("test-session-2").unwrap();
    let mut manager = SessionManager::new(session_id);

    let msg1 = BrowserEvent::AssistantMessage {
        message_id: "msg-1".to_string(),
        text: "First response".to_string(),
    };

    let msg2 = BrowserEvent::AssistantMessage {
        message_id: "msg-2".to_string(),
        text: "Second response".to_string(),
    };

    // First time msg1 is handled: accepted
    assert!(manager.handle_event(&msg1));
    assert!(manager.is_duplicate("msg-1"));

    // Second time msg1 is handled: rejected as duplicate
    assert!(!manager.handle_event(&msg1));

    // msg2 is new: accepted
    assert!(manager.handle_event(&msg2));
    assert!(manager.is_duplicate("msg-2"));

    // Both are duplicate now
    assert!(!manager.handle_event(&msg1));
    assert!(!manager.handle_event(&msg2));
}

#[test]
fn test_connection_reset() {
    let session_id = SessionId::new("test-session-3").unwrap();
    let mut manager = SessionManager::new(session_id);

    manager.handle_event(&BrowserEvent::Connected);
    manager.handle_event(&BrowserEvent::ConversationDetected {
        conversation_id: Some("conv-999".to_string()),
    });
    assert_eq!(manager.state(), SessionState::ActiveConversation);

    manager.reset_connection();
    assert_eq!(manager.state(), SessionState::Disconnected);
    assert_eq!(manager.conversation_id(), None);
}
