use bridge_core::{ConversationStore, SqliteConversationStore};
use bridge_server::BridgeServer;
use futures_util::{SinkExt, StreamExt};
use protocol::{
    BridgeErrorCode, BrowserEvent, CURRENT_PROTOCOL_VERSION, ClientHello, CorrelationId,
    MessageEnvelope, ServerHelloAck, SessionId,
};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::broadcast;
use tokio_tungstenite::connect_async;
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::tungstenite::protocol::frame::coding::CloseCode;

async fn spawn_test_server() -> (
    String,
    String,
    Arc<SqliteConversationStore>,
    broadcast::Sender<()>,
    tokio::task::JoinHandle<()>,
    Arc<bridge_server::server::ServerState>,
) {
    let (shutdown_tx, shutdown_rx) = broadcast::channel(1);
    let store = Arc::new(SqliteConversationStore::open_in_memory().unwrap());
    let token = "test-token-flow-123".to_string();

    let server = BridgeServer::bind("127.0.0.1:0", token.clone(), store.clone(), shutdown_rx)
        .await
        .expect("should bind on ephemeral port");

    let addr = server.local_addr().unwrap();
    let state = server.state();

    let server_handle = tokio::spawn(async move {
        server.run().await.unwrap();
    });

    (
        format!("ws://{addr}"),
        token,
        store,
        shutdown_tx,
        server_handle,
        state,
    )
}

async fn connect_and_handshake(
    ws_url: &str,
    token: &str,
    session_id: &str,
) -> tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>> {
    let (mut ws_stream, _) = connect_async(ws_url).await.unwrap();
    let hello = ClientHello {
        protocol_version: CURRENT_PROTOCOL_VERSION.0,
        token: token.to_string(),
        extension_version: "0.1.0".into(),
    };
    ws_stream
        .send(Message::Text(serde_json::to_string(&hello).unwrap().into()))
        .await
        .unwrap();

    let ack_msg = ws_stream.next().await.unwrap().unwrap();
    if let Message::Text(text) = ack_msg {
        let ack: ServerHelloAck = serde_json::from_str(&text).unwrap();
        assert!(ack.accepted);
    } else {
        panic!("expected ServerHelloAck");
    }

    let env = MessageEnvelope::new(
        SessionId::new(session_id).unwrap(),
        CorrelationId::new("corr-conn").unwrap(),
        BrowserEvent::Connected,
    );
    ws_stream
        .send(Message::Text(env.to_json().unwrap().into()))
        .await
        .unwrap();

    // Consume ping ack
    let _ = ws_stream.next().await.unwrap().unwrap();

    ws_stream
}

#[tokio::test]
async fn test_conversation_switching_preserves_identities() {
    let (ws_url, token, store, shutdown_tx, server_handle, _state) = spawn_test_server().await;
    let mut ws = connect_and_handshake(&ws_url, &token, "sess-switching").await;
    let sess = SessionId::new("sess-switching").unwrap();

    // 1. Visit Conversation A (/c/conv-a) and add message A1
    let env_ca = MessageEnvelope::new(
        sess.clone(),
        CorrelationId::new("c-ca").unwrap(),
        BrowserEvent::ConversationDetected {
            conversation_id: Some("conv-a".into()),
            external_conversation_ref: Some("/c/conv-a".into()),
        },
    );
    ws.send(Message::Text(env_ca.to_json().unwrap().into()))
        .await
        .unwrap();

    let env_ma1 = MessageEnvelope::new(
        sess.clone(),
        CorrelationId::new("c-ma1").unwrap(),
        BrowserEvent::UserMessageObserved {
            external_message_id: "m-a1".into(),
            text: "Hello from Conversation A".into(),
        },
    );
    ws.send(Message::Text(env_ma1.to_json().unwrap().into()))
        .await
        .unwrap();
    tokio::time::sleep(Duration::from_millis(50)).await;

    // 2. Switch to Conversation B (/c/conv-b) and add message B1
    let env_cb = MessageEnvelope::new(
        sess.clone(),
        CorrelationId::new("c-cb").unwrap(),
        BrowserEvent::ConversationDetected {
            conversation_id: Some("conv-b".into()),
            external_conversation_ref: Some("/c/conv-b".into()),
        },
    );
    ws.send(Message::Text(env_cb.to_json().unwrap().into()))
        .await
        .unwrap();

    let env_mb1 = MessageEnvelope::new(
        sess.clone(),
        CorrelationId::new("c-mb1").unwrap(),
        BrowserEvent::UserMessageObserved {
            external_message_id: "m-b1".into(),
            text: "Hello from Conversation B".into(),
        },
    );
    ws.send(Message::Text(env_mb1.to_json().unwrap().into()))
        .await
        .unwrap();
    tokio::time::sleep(Duration::from_millis(50)).await;

    // 3. Switch back to Conversation A (/c/conv-a) and add message A2
    let env_ca2 = MessageEnvelope::new(
        sess.clone(),
        CorrelationId::new("c-ca2").unwrap(),
        BrowserEvent::ConversationDetected {
            conversation_id: Some("conv-a".into()),
            external_conversation_ref: Some("/c/conv-a".into()),
        },
    );
    ws.send(Message::Text(env_ca2.to_json().unwrap().into()))
        .await
        .unwrap();

    let env_ma2 = MessageEnvelope::new(
        sess.clone(),
        CorrelationId::new("c-ma2").unwrap(),
        BrowserEvent::UserMessageObserved {
            external_message_id: "m-a2".into(),
            text: "Second turn in Conversation A".into(),
        },
    );
    ws.send(Message::Text(env_ma2.to_json().unwrap().into()))
        .await
        .unwrap();
    tokio::time::sleep(Duration::from_millis(50)).await;

    // Verification in SQLite
    let conv_a = store
        .find_conversation_by_external_ref("/c/conv-a")
        .await
        .unwrap()
        .expect("conv A must exist");
    let conv_b = store
        .find_conversation_by_external_ref("/c/conv-b")
        .await
        .unwrap()
        .expect("conv B must exist");

    assert_ne!(conv_a.id, conv_b.id);

    let msgs_a = store.load_messages(&conv_a.id, None, 10).await.unwrap();
    assert_eq!(msgs_a.len(), 2);
    assert_eq!(msgs_a[0].content, "Hello from Conversation A");
    assert_eq!(msgs_a[0].sequence, 1);
    assert_eq!(msgs_a[1].content, "Second turn in Conversation A");
    assert_eq!(msgs_a[1].sequence, 2);

    let msgs_b = store.load_messages(&conv_b.id, None, 10).await.unwrap();
    assert_eq!(msgs_b.len(), 1);
    assert_eq!(msgs_b[0].content, "Hello from Conversation B");
    assert_eq!(msgs_b[0].sequence, 1);

    let _ = shutdown_tx.send(());
    let _ = server_handle.await;
}

#[tokio::test]
async fn test_temporary_conversation_binding() {
    let (ws_url, token, store, shutdown_tx, server_handle, state) = spawn_test_server().await;
    let mut ws = connect_and_handshake(&ws_url, &token, "sess-temp-binding").await;
    let sess = SessionId::new("sess-temp-binding").unwrap();

    // 1. User starts typing on unbound homepage (no /c/<id> yet)
    let env_u1 = MessageEnvelope::new(
        sess.clone(),
        CorrelationId::new("c-u1").unwrap(),
        BrowserEvent::UserMessageObserved {
            external_message_id: "turn-1".into(),
            text: "Design bridge architecture".into(),
        },
    );
    ws.send(Message::Text(env_u1.to_json().unwrap().into()))
        .await
        .unwrap();

    let env_a1 = MessageEnvelope::new(
        sess.clone(),
        CorrelationId::new("c-a1").unwrap(),
        BrowserEvent::AssistantMessageObserved {
            external_message_id: "turn-2".into(),
            text: "Understood. Let's outline the core invariants.".into(),
            is_final: true,
        },
    );
    ws.send(Message::Text(env_a1.to_json().unwrap().into()))
        .await
        .unwrap();
    tokio::time::sleep(Duration::from_millis(50)).await;

    let active_before = state.active_conversation.lock().await.clone().unwrap();
    assert!(active_before.external_conversation_ref.is_none());

    // 2. ChatGPT assigns conversation URL /c/uuid-999
    let env_bind = MessageEnvelope::new(
        sess.clone(),
        CorrelationId::new("c-bind").unwrap(),
        BrowserEvent::ConversationDetected {
            conversation_id: Some("uuid-999".into()),
            external_conversation_ref: Some("/c/uuid-999".into()),
        },
    );
    ws.send(Message::Text(env_bind.to_json().unwrap().into()))
        .await
        .unwrap();
    tokio::time::sleep(Duration::from_millis(50)).await;

    // 3. Third turn occurs after binding
    let env_u2 = MessageEnvelope::new(
        sess.clone(),
        CorrelationId::new("c-u2").unwrap(),
        BrowserEvent::UserMessageObserved {
            external_message_id: "turn-3".into(),
            text: "Next step: persistence and tests.".into(),
        },
    );
    ws.send(Message::Text(env_u2.to_json().unwrap().into()))
        .await
        .unwrap();
    tokio::time::sleep(Duration::from_millis(50)).await;

    // Verify exactly ONE conversation in store, bound to /c/uuid-999 with 3 messages
    let convs = store.list_conversations(10, 0).await.unwrap();
    assert_eq!(convs.len(), 1);
    assert_eq!(convs[0].id, active_before.id);
    assert_eq!(
        convs[0].external_conversation_ref.as_deref(),
        Some("/c/uuid-999")
    );
    assert_eq!(convs[0].message_count, 3);

    let _ = shutdown_tx.send(());
    let _ = server_handle.await;
}

#[tokio::test]
async fn test_branch_mutation_detection() {
    let (ws_url, token, store, shutdown_tx, server_handle, state) = spawn_test_server().await;
    let mut ws = connect_and_handshake(&ws_url, &token, "sess-mutation").await;
    let sess = SessionId::new("sess-mutation").unwrap();

    // 1. Initial user message
    let env_msg = MessageEnvelope::new(
        sess.clone(),
        CorrelationId::new("c-m1").unwrap(),
        BrowserEvent::UserMessageObserved {
            external_message_id: "mut-msg-1".into(),
            text: "Original immutable turn".into(),
        },
    );
    ws.send(Message::Text(env_msg.to_json().unwrap().into()))
        .await
        .unwrap();
    tokio::time::sleep(Duration::from_millis(50)).await;

    // 2. Simulated external edit of mut-msg-1 (different text)
    let env_edited = MessageEnvelope::new(
        sess.clone(),
        CorrelationId::new("c-m2").unwrap(),
        BrowserEvent::UserMessageObserved {
            external_message_id: "mut-msg-1".into(),
            text: "Mutated edited turn from ChatGPT web UI".into(),
        },
    );
    ws.send(Message::Text(env_edited.to_json().unwrap().into()))
        .await
        .unwrap();
    tokio::time::sleep(Duration::from_millis(50)).await;

    // 3. Stored row must remain intact and immutable!
    let conv = state.active_conversation.lock().await.clone().unwrap();
    let msgs = store.load_messages(&conv.id, None, 10).await.unwrap();
    assert_eq!(msgs.len(), 1);
    assert_eq!(msgs[0].content, "Original immutable turn");

    let _ = shutdown_tx.send(());
    let _ = server_handle.await;
}

#[tokio::test]
async fn test_multiple_tabs_rejected() {
    let (ws_url, token, _store, shutdown_tx, server_handle, _state) = spawn_test_server().await;

    // Client 1 connects successfully
    let _ws1 = connect_and_handshake(&ws_url, &token, "sess-tab-1").await;

    // Client 2 attempts to connect concurrently
    let (mut ws2, _) = connect_async(&ws_url).await.unwrap();
    let hello = ClientHello {
        protocol_version: CURRENT_PROTOCOL_VERSION.0,
        token: token.clone(),
        extension_version: "0.1.0".into(),
    };
    ws2.send(Message::Text(serde_json::to_string(&hello).unwrap().into()))
        .await
        .unwrap();

    let msg = ws2.next().await;
    match msg {
        Some(Ok(Message::Close(Some(frame)))) => {
            assert_eq!(frame.code, CloseCode::Policy);
            assert_eq!(frame.reason, BridgeErrorCode::MultipleChatgptTabs.as_str());
        }
        other => panic!(
            "expected close frame with MultipleChatgptTabs, got {:?}",
            other
        ),
    }

    let _ = shutdown_tx.send(());
    let _ = server_handle.await;
}
