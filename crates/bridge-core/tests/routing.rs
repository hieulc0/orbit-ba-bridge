use bridge_core::{BridgeRouter, MockBrowserDriver, SessionManager};
use protocol::{BrowserCommand, BrowserEvent, CorrelationId, SessionId};

#[tokio::test]
async fn test_dispatch_command_via_router() {
    let (driver, _event_tx, mut command_rx) = MockBrowserDriver::pair(10);
    let session_id = SessionId::new("session-router-1").unwrap();
    let mut router = BridgeRouter::new(driver, SessionManager::new(session_id));

    let cmd = BrowserCommand::SendMessage {
        text: "Generate test specifications".to_string(),
    };

    router
        .dispatch_command(cmd.clone())
        .await
        .expect("command dispatch should succeed");

    let received = command_rx
        .recv()
        .await
        .expect("command should be received by driver");
    assert_eq!(received, cmd);
}

#[tokio::test]
async fn test_process_events_and_filtering() {
    let (driver, event_tx, _command_rx) = MockBrowserDriver::pair(10);
    let session_id = SessionId::new("session-router-2").unwrap();
    let mut router = BridgeRouter::new(driver, SessionManager::new(session_id));

    // Send Connected event
    event_tx.send(BrowserEvent::Connected).await.unwrap();
    let res = router.process_next_event().await.unwrap();
    assert_eq!(res, Some(BrowserEvent::Connected));

    // Send AssistantMessage
    let msg = BrowserEvent::AssistantMessage {
        message_id: "m-1".to_string(),
        text: "Analyzing requirements...".to_string(),
    };
    event_tx.send(msg.clone()).await.unwrap();
    let res = router.process_next_event().await.unwrap();
    assert_eq!(res, Some(msg.clone()));

    // Send duplicate AssistantMessage
    event_tx.send(msg).await.unwrap();
    let res = router.process_next_event().await.unwrap();
    assert_eq!(res, None, "duplicate message should be suppressed");
}

#[tokio::test]
async fn test_create_command_envelope() {
    let (driver, _event_tx, _command_rx) = MockBrowserDriver::pair(10);
    let session_id = SessionId::new("session-router-3").unwrap();
    let router = BridgeRouter::new(driver, SessionManager::new(session_id.clone()));

    let corr = CorrelationId::new("corr-100").unwrap();
    let cmd = BrowserCommand::Ping;
    let envelope = router.create_command_envelope(corr.clone(), cmd.clone());

    assert_eq!(envelope.session_id, session_id);
    assert_eq!(envelope.correlation_id, corr);
    assert_eq!(envelope.payload, cmd);
}
