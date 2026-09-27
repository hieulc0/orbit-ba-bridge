use bridge_server::BridgeServer;
use futures_util::{SinkExt, StreamExt};
use protocol::{BrowserCommand, BrowserEvent, CorrelationId, MessageEnvelope, SessionId};
use std::time::Duration;
use tokio::sync::broadcast;
use tokio_tungstenite::connect_async;
use tokio_tungstenite::tungstenite::Message;

#[tokio::test]
async fn test_websocket_handshake_and_connected_event() {
    let (shutdown_tx, shutdown_rx) = broadcast::channel(1);
    let server = BridgeServer::bind("127.0.0.1:0", shutdown_rx)
        .await
        .expect("should bind on ephemeral port");

    let addr = server.local_addr().unwrap();
    let server_handle = tokio::spawn(async move {
        server.run().await.unwrap();
    });

    let ws_url = format!("ws://{addr}");
    let (mut ws_stream, _) = connect_async(&ws_url)
        .await
        .expect("WebSocket connection should succeed");

    let session_id = SessionId::new("test-session-ws").unwrap();
    let corr_id = CorrelationId::new("corr-ws-1").unwrap();
    let env = MessageEnvelope::new(session_id, corr_id, BrowserEvent::Connected);

    let json = env.to_json().unwrap();
    ws_stream.send(Message::Text(json.into())).await.unwrap();

    // Expect response ping from server
    if let Some(Ok(Message::Text(resp_text))) = ws_stream.next().await {
        let resp: MessageEnvelope<BrowserCommand> =
            MessageEnvelope::from_json_str(&resp_text).expect("response should be valid envelope");
        assert_eq!(resp.payload, BrowserCommand::Ping);
    } else {
        panic!("expected Text message response from bridge-server");
    }

    // Clean shutdown
    let _ = shutdown_tx.send(());
    let _ = server_handle.await;
}

#[tokio::test]
async fn test_reject_invalid_version_or_malformed_envelope() {
    let (shutdown_tx, shutdown_rx) = broadcast::channel(1);
    let server = BridgeServer::bind("127.0.0.1:0", shutdown_rx)
        .await
        .expect("should bind on ephemeral port");

    let addr = server.local_addr().unwrap();
    let server_handle = tokio::spawn(async move {
        server.run().await.unwrap();
    });

    let ws_url = format!("ws://{addr}");
    let (mut ws_stream, _) = connect_async(&ws_url)
        .await
        .expect("WebSocket connection should succeed");

    // Send malformed JSON
    ws_stream
        .send(Message::Text("not valid json".into()))
        .await
        .unwrap();

    // Expect server to close connection due to policy violation
    let msg = ws_stream.next().await;
    match msg {
        Some(Ok(Message::Close(Some(frame)))) => {
            assert!(
                frame.code
                    == tokio_tungstenite::tungstenite::protocol::frame::coding::CloseCode::Policy
            );
        }
        Some(Ok(Message::Close(None))) | None => {}
        other => panic!("expected close frame, got {:?}", other),
    }

    let _ = shutdown_tx.send(());
    let _ = server_handle.await;
}

#[tokio::test]
async fn test_client_reconnect() {
    let (shutdown_tx, shutdown_rx) = broadcast::channel(1);
    let server = BridgeServer::bind("127.0.0.1:0", shutdown_rx)
        .await
        .expect("should bind on ephemeral port");

    let addr = server.local_addr().unwrap();
    let server_handle = tokio::spawn(async move {
        server.run().await.unwrap();
    });

    let ws_url = format!("ws://{addr}");

    // First connection
    {
        let (mut ws_stream, _) = connect_async(&ws_url).await.unwrap();
        let env = MessageEnvelope::new(
            SessionId::new("sess-1").unwrap(),
            CorrelationId::new("corr-1").unwrap(),
            BrowserEvent::Connected,
        );
        ws_stream
            .send(Message::Text(env.to_json().unwrap().into()))
            .await
            .unwrap();
        let _ = ws_stream.next().await;
        // Drop stream to simulate disconnect
    }

    tokio::time::sleep(Duration::from_millis(50)).await;

    // Second connection (reconnect)
    {
        let (mut ws_stream, _) = connect_async(&ws_url).await.unwrap();
        let env = MessageEnvelope::new(
            SessionId::new("sess-2").unwrap(),
            CorrelationId::new("corr-2").unwrap(),
            BrowserEvent::Connected,
        );
        ws_stream
            .send(Message::Text(env.to_json().unwrap().into()))
            .await
            .unwrap();
        if let Some(Ok(Message::Text(resp_text))) = ws_stream.next().await {
            let resp: MessageEnvelope<BrowserCommand> =
                MessageEnvelope::from_json_str(&resp_text).unwrap();
            assert_eq!(resp.payload, BrowserCommand::Ping);
        } else {
            panic!("expected response on reconnect");
        }
    }

    let _ = shutdown_tx.send(());
    let _ = server_handle.await;
}
