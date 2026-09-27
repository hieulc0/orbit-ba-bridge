use bridge_core::{ActorRole, ConversationStore, ParticipantSource, SqliteConversationStore};
use bridge_server::BridgeServer;
use futures_util::{SinkExt, StreamExt};
use protocol::{
    BrowserCommand, BrowserEvent, CURRENT_PROTOCOL_VERSION, ClientHello, CorrelationId,
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
    let token = "test-token-secret-123".to_string();

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

#[tokio::test]
async fn test_websocket_handshake_and_connected_event() {
    let (ws_url, token, _store, shutdown_tx, server_handle, _state) = spawn_test_server().await;

    let (mut ws_stream, _) = connect_async(&ws_url)
        .await
        .expect("WebSocket connection should succeed");

    // 1. Send valid ClientHello
    let hello = ClientHello {
        protocol_version: CURRENT_PROTOCOL_VERSION.0,
        token,
        extension_version: "0.1.0".into(),
    };
    ws_stream
        .send(Message::Text(serde_json::to_string(&hello).unwrap().into()))
        .await
        .unwrap();

    // 2. Expect ServerHelloAck
    let ack_msg = ws_stream.next().await.unwrap().unwrap();
    if let Message::Text(text) = ack_msg {
        let ack: ServerHelloAck = serde_json::from_str(&text).unwrap();
        assert_eq!(ack.protocol_version, CURRENT_PROTOCOL_VERSION.0);
        assert!(ack.accepted);
    } else {
        panic!("expected ServerHelloAck text message");
    }

    // 3. Send Connected event
    let session_id = SessionId::new("test-session-ws").unwrap();
    let corr_id = CorrelationId::new("corr-ws-1").unwrap();
    let env = MessageEnvelope::new(session_id, corr_id, BrowserEvent::Connected);
    ws_stream
        .send(Message::Text(env.to_json().unwrap().into()))
        .await
        .unwrap();

    // Expect ping command response
    if let Some(Ok(Message::Text(resp_text))) = ws_stream.next().await {
        let resp: MessageEnvelope<BrowserCommand> =
            MessageEnvelope::from_json_str(&resp_text).expect("response should be valid envelope");
        assert_eq!(resp.payload, BrowserCommand::Ping);
    } else {
        panic!("expected Text message response from bridge-server");
    }

    let _ = shutdown_tx.send(());
    let _ = server_handle.await;
}

#[tokio::test]
async fn test_reject_invalid_token() {
    let (ws_url, _token, _store, shutdown_tx, server_handle, _state) = spawn_test_server().await;

    let (mut ws_stream, _) = connect_async(&ws_url).await.unwrap();

    let hello = ClientHello {
        protocol_version: CURRENT_PROTOCOL_VERSION.0,
        token: "wrong-token".into(),
        extension_version: "0.1.0".into(),
    };
    ws_stream
        .send(Message::Text(serde_json::to_string(&hello).unwrap().into()))
        .await
        .unwrap();

    let msg = ws_stream.next().await;
    match msg {
        Some(Ok(Message::Close(Some(frame)))) => {
            assert_eq!(frame.code, CloseCode::Policy);
            assert_eq!(frame.reason, "invalid bridge auth token");
        }
        other => panic!(
            "expected close frame with policy violation, got {:?}",
            other
        ),
    }

    let _ = shutdown_tx.send(());
    let _ = server_handle.await;
}

#[tokio::test]
async fn test_reject_unsupported_protocol_version() {
    let (ws_url, token, _store, shutdown_tx, server_handle, _state) = spawn_test_server().await;

    let (mut ws_stream, _) = connect_async(&ws_url).await.unwrap();

    let hello = ClientHello {
        protocol_version: 999,
        token,
        extension_version: "0.1.0".into(),
    };
    ws_stream
        .send(Message::Text(serde_json::to_string(&hello).unwrap().into()))
        .await
        .unwrap();

    let msg = ws_stream.next().await;
    match msg {
        Some(Ok(Message::Close(Some(frame)))) => {
            assert_eq!(frame.code, CloseCode::Policy);
            assert_eq!(frame.reason, "unsupported protocol version");
        }
        other => panic!("expected close frame, got {:?}", other),
    }

    let _ = shutdown_tx.send(());
    let _ = server_handle.await;
}

#[tokio::test]
async fn test_reject_malformed_envelope() {
    let (ws_url, _token, _store, shutdown_tx, server_handle, _state) = spawn_test_server().await;

    let (mut ws_stream, _) = connect_async(&ws_url).await.unwrap();

    // Send malformed text instead of Hello frame
    ws_stream
        .send(Message::Text("not valid json".into()))
        .await
        .unwrap();

    let msg = ws_stream.next().await;
    match msg {
        Some(Ok(Message::Close(Some(frame)))) => {
            assert_eq!(frame.code, CloseCode::Policy);
        }
        Some(Ok(Message::Close(None))) | None => {}
        other => panic!("expected close frame, got {:?}", other),
    }

    let _ = shutdown_tx.send(());
    let _ = server_handle.await;
}

#[tokio::test]
async fn test_client_reconnect() {
    let (ws_url, token, _store, shutdown_tx, server_handle, _state) = spawn_test_server().await;

    // First connection
    {
        let (mut ws_stream, _) = connect_async(&ws_url).await.unwrap();
        let hello = ClientHello {
            protocol_version: CURRENT_PROTOCOL_VERSION.0,
            token: token.clone(),
            extension_version: "0.1.0".into(),
        };
        ws_stream
            .send(Message::Text(serde_json::to_string(&hello).unwrap().into()))
            .await
            .unwrap();
        let _ack = ws_stream.next().await.unwrap().unwrap();
        // Drop stream to simulate disconnect
    }

    tokio::time::sleep(Duration::from_millis(50)).await;

    // Second connection (reconnect)
    {
        let (mut ws_stream, _) = connect_async(&ws_url).await.unwrap();
        let hello = ClientHello {
            protocol_version: CURRENT_PROTOCOL_VERSION.0,
            token: token.clone(),
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
            panic!("expected ack on reconnect");
        }
    }

    let _ = shutdown_tx.send(());
    let _ = server_handle.await;
}

#[tokio::test]
async fn test_duplicate_dom_event_persisted_once() {
    let (ws_url, token, store, shutdown_tx, server_handle, state) = spawn_test_server().await;

    let (mut ws_stream, _) = connect_async(&ws_url).await.unwrap();
    let hello = ClientHello {
        protocol_version: CURRENT_PROTOCOL_VERSION.0,
        token,
        extension_version: "0.1.0".into(),
    };
    ws_stream
        .send(Message::Text(serde_json::to_string(&hello).unwrap().into()))
        .await
        .unwrap();
    let _ack = ws_stream.next().await.unwrap().unwrap();

    let session_id = SessionId::new("sess-dup").unwrap();
    let corr_id = CorrelationId::new("corr-dup").unwrap();

    let user_event = BrowserEvent::UserMessageObserved {
        external_message_id: "ext-turn-1".into(),
        text: "Hello from human user".into(),
    };
    let env = MessageEnvelope::new(session_id.clone(), corr_id.clone(), user_event.clone());

    // Send the exact same DOM event twice
    ws_stream
        .send(Message::Text(env.to_json().unwrap().into()))
        .await
        .unwrap();
    tokio::time::sleep(Duration::from_millis(50)).await;

    ws_stream
        .send(Message::Text(env.to_json().unwrap().into()))
        .await
        .unwrap();
    tokio::time::sleep(Duration::from_millis(50)).await;

    // Verify messages in store: exactly 1 message
    let conv = state.active_conversation.lock().await.clone().unwrap();
    let messages = store.load_messages(&conv.id, None, 10).await.unwrap();
    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0].content, "Hello from human user");

    let _ = shutdown_tx.send(());
    let _ = server_handle.await;
}

#[tokio::test]
async fn test_manual_human_and_injected_sa_attribution() {
    let (ws_url, token, store, shutdown_tx, server_handle, state) = spawn_test_server().await;

    let (mut ws_stream, _) = connect_async(&ws_url).await.unwrap();
    let hello = ClientHello {
        protocol_version: CURRENT_PROTOCOL_VERSION.0,
        token,
        extension_version: "0.1.0".into(),
    };
    ws_stream
        .send(Message::Text(serde_json::to_string(&hello).unwrap().into()))
        .await
        .unwrap();
    let _ack = ws_stream.next().await.unwrap().unwrap();

    let session_id = SessionId::new("sess-actors").unwrap();

    // 1. Manual Human Turn (untracked in injection ledger)
    let human_env = MessageEnvelope::new(
        session_id.clone(),
        CorrelationId::new("c-human").unwrap(),
        BrowserEvent::UserMessageObserved {
            external_message_id: "dom-human-1".into(),
            text: "Human asks a question".into(),
        },
    );
    ws_stream
        .send(Message::Text(human_env.to_json().unwrap().into()))
        .await
        .unwrap();
    tokio::time::sleep(Duration::from_millis(50)).await;

    // 2. External Turn: Inject Orbit SA turn from bridge server
    let inj_id = state
        .inject(
            ActorRole::SystemArchitect,
            "Orbit SA",
            ParticipantSource::Orbit,
            "SA proposes architecture",
        )
        .await
        .unwrap();

    // Extension receives injection command over WebSocket
    let cmd_msg = ws_stream.next().await.unwrap().unwrap();
    if let Message::Text(text) = cmd_msg {
        let cmd_env: MessageEnvelope<BrowserCommand> =
            MessageEnvelope::from_json_str(&text).unwrap();
        match cmd_env.payload {
            BrowserCommand::InjectMessage {
                injection_id, text, ..
            } => {
                assert_eq!(injection_id, inj_id);
                assert!(text.contains("Orbit SA"));
            }
            other => panic!("expected InjectMessage, got {:?}", other),
        }
    } else {
        panic!("expected text command");
    }

    // Extension accepts injection
    let ack_env = MessageEnvelope::new(
        session_id.clone(),
        CorrelationId::new("c-ack").unwrap(),
        BrowserEvent::InjectionAccepted {
            injection_id: inj_id.clone(),
        },
    );
    ws_stream
        .send(Message::Text(ack_env.to_json().unwrap().into()))
        .await
        .unwrap();

    // Extension materializes injection into native DOM user message
    let mat_env = MessageEnvelope::new(
        session_id.clone(),
        CorrelationId::new("c-mat").unwrap(),
        BrowserEvent::InjectionMaterialized {
            injection_id: inj_id.clone(),
            external_message_id: "dom-sa-2".into(),
        },
    );
    ws_stream
        .send(Message::Text(mat_env.to_json().unwrap().into()))
        .await
        .unwrap();

    // DOM observer emits observed user message
    let obs_env = MessageEnvelope::new(
        session_id.clone(),
        CorrelationId::new("c-obs").unwrap(),
        BrowserEvent::UserMessageObserved {
            external_message_id: "dom-sa-2".into(),
            text: "[External participant: Orbit SA]\n\nSA proposes architecture".into(),
        },
    );
    ws_stream
        .send(Message::Text(obs_env.to_json().unwrap().into()))
        .await
        .unwrap();
    tokio::time::sleep(Duration::from_millis(50)).await;

    // 3. ChatGPT BA responds
    let ba_env = MessageEnvelope::new(
        session_id.clone(),
        CorrelationId::new("c-ba").unwrap(),
        BrowserEvent::AssistantMessageObserved {
            external_message_id: "dom-ba-3".into(),
            text: "ChatGPT BA accepts architecture proposal".into(),
            is_final: true,
        },
    );
    ws_stream
        .send(Message::Text(ba_env.to_json().unwrap().into()))
        .await
        .unwrap();
    tokio::time::sleep(Duration::from_millis(50)).await;

    // 4. Verification in SQLite
    let conv = state.active_conversation.lock().await.clone().unwrap();
    let messages = store.load_messages(&conv.id, None, 10).await.unwrap();
    assert_eq!(messages.len(), 3);

    let human_part = store
        .get_participant(&messages[0].actor_id)
        .await
        .unwrap()
        .unwrap();
    let sa_part = store
        .get_participant(&messages[1].actor_id)
        .await
        .unwrap()
        .unwrap();
    let ba_part = store
        .get_participant(&messages[2].actor_id)
        .await
        .unwrap()
        .unwrap();

    assert_eq!(human_part.role, ActorRole::Human);
    assert_eq!(human_part.source, ParticipantSource::HumanBrowser);

    assert_eq!(sa_part.role, ActorRole::SystemArchitect);
    assert_eq!(sa_part.source, ParticipantSource::Orbit);
    assert_eq!(sa_part.display_name, "Orbit SA");

    assert_eq!(ba_part.role, ActorRole::BusinessAnalyst);
    assert_eq!(ba_part.source, ParticipantSource::ChatGptWeb);

    let _ = shutdown_tx.send(());
    let _ = server_handle.await;
}

#[tokio::test]
async fn test_identical_content_actors() {
    let (ws_url, token, store, shutdown_tx, server_handle, state) = spawn_test_server().await;

    let (mut ws_stream, _) = connect_async(&ws_url).await.unwrap();
    let hello = ClientHello {
        protocol_version: CURRENT_PROTOCOL_VERSION.0,
        token,
        extension_version: "0.1.0".into(),
    };
    ws_stream
        .send(Message::Text(serde_json::to_string(&hello).unwrap().into()))
        .await
        .unwrap();
    let _ack = ws_stream.next().await.unwrap().unwrap();

    let session_id = SessionId::new("sess-identical").unwrap();

    // 1. Human says "continue"
    let env1 = MessageEnvelope::new(
        session_id.clone(),
        CorrelationId::new("c-1").unwrap(),
        BrowserEvent::UserMessageObserved {
            external_message_id: "ext-1".into(),
            text: "continue".into(),
        },
    );
    ws_stream
        .send(Message::Text(env1.to_json().unwrap().into()))
        .await
        .unwrap();
    tokio::time::sleep(Duration::from_millis(50)).await;

    // 2. Orbit SA says "continue"
    let sa_inj = state
        .inject(
            ActorRole::SystemArchitect,
            "Orbit SA",
            ParticipantSource::Orbit,
            "continue",
        )
        .await
        .unwrap();
    let _ = ws_stream.next().await.unwrap().unwrap(); // drain command

    let mat_sa = MessageEnvelope::new(
        session_id.clone(),
        CorrelationId::new("c-mat-sa").unwrap(),
        BrowserEvent::InjectionMaterialized {
            injection_id: sa_inj,
            external_message_id: "ext-2".into(),
        },
    );
    ws_stream
        .send(Message::Text(mat_sa.to_json().unwrap().into()))
        .await
        .unwrap();

    let env2 = MessageEnvelope::new(
        session_id.clone(),
        CorrelationId::new("c-2").unwrap(),
        BrowserEvent::UserMessageObserved {
            external_message_id: "ext-2".into(),
            text: "continue".into(),
        },
    );
    ws_stream
        .send(Message::Text(env2.to_json().unwrap().into()))
        .await
        .unwrap();
    tokio::time::sleep(Duration::from_millis(50)).await;

    // 3. BA Research says "continue"
    let res_inj = state
        .inject(
            ActorRole::BusinessAnalyst,
            "BA Research",
            ParticipantSource::ChatGptWeb,
            "continue",
        )
        .await
        .unwrap();
    let _ = ws_stream.next().await.unwrap().unwrap(); // drain command

    let mat_res = MessageEnvelope::new(
        session_id.clone(),
        CorrelationId::new("c-mat-res").unwrap(),
        BrowserEvent::InjectionMaterialized {
            injection_id: res_inj,
            external_message_id: "ext-3".into(),
        },
    );
    ws_stream
        .send(Message::Text(mat_res.to_json().unwrap().into()))
        .await
        .unwrap();

    let env3 = MessageEnvelope::new(
        session_id.clone(),
        CorrelationId::new("c-3").unwrap(),
        BrowserEvent::UserMessageObserved {
            external_message_id: "ext-3".into(),
            text: "continue".into(),
        },
    );
    ws_stream
        .send(Message::Text(env3.to_json().unwrap().into()))
        .await
        .unwrap();
    tokio::time::sleep(Duration::from_millis(50)).await;

    // Verify all 3 messages have distinct actor IDs despite having identical "continue" text!
    let conv = state.active_conversation.lock().await.clone().unwrap();
    let messages = store.load_messages(&conv.id, None, 10).await.unwrap();
    assert_eq!(messages.len(), 3);

    assert_eq!(messages[0].content, "continue");
    assert_eq!(messages[1].content, "continue");
    assert_eq!(messages[2].content, "continue");

    let actor0 = messages[0].actor_id.clone();
    let actor1 = messages[1].actor_id.clone();
    let actor2 = messages[2].actor_id.clone();

    assert_ne!(actor0, actor1);
    assert_ne!(actor1, actor2);
    assert_ne!(actor0, actor2);

    let p0 = store.get_participant(&actor0).await.unwrap().unwrap();
    let p1 = store.get_participant(&actor1).await.unwrap().unwrap();
    let p2 = store.get_participant(&actor2).await.unwrap().unwrap();

    assert_eq!(p0.role, ActorRole::Human);
    assert_eq!(p1.role, ActorRole::SystemArchitect);
    assert_eq!(p2.role, ActorRole::BusinessAnalyst);
    assert_eq!(p2.display_name, "BA Research");

    let _ = shutdown_tx.send(());
    let _ = server_handle.await;
}

#[tokio::test]
async fn test_reload_recovery_does_not_duplicate_history() {
    let (ws_url, token, store, shutdown_tx, server_handle, state) = spawn_test_server().await;

    let session_id = SessionId::new("sess-reload").unwrap();

    // Initial page load & conversation detection
    {
        let (mut ws_stream, _) = connect_async(&ws_url).await.unwrap();
        let hello = ClientHello {
            protocol_version: CURRENT_PROTOCOL_VERSION.0,
            token: token.clone(),
            extension_version: "0.1.0".into(),
        };
        ws_stream
            .send(Message::Text(serde_json::to_string(&hello).unwrap().into()))
            .await
            .unwrap();
        let _ack = ws_stream.next().await.unwrap().unwrap();

        let conv_event = BrowserEvent::ConversationDetected {
            conversation_id: Some("conv-abc".into()),
            external_conversation_ref: Some("chatgpt-conv-xyz".into()),
        };
        let env_conv = MessageEnvelope::new(
            session_id.clone(),
            CorrelationId::new("c-c").unwrap(),
            conv_event,
        );
        ws_stream
            .send(Message::Text(env_conv.to_json().unwrap().into()))
            .await
            .unwrap();

        let user_event = BrowserEvent::UserMessageObserved {
            external_message_id: "m-1".into(),
            text: "First turn before reload".into(),
        };
        let env_user = MessageEnvelope::new(
            session_id.clone(),
            CorrelationId::new("c-u").unwrap(),
            user_event,
        );
        ws_stream
            .send(Message::Text(env_user.to_json().unwrap().into()))
            .await
            .unwrap();
        tokio::time::sleep(Duration::from_millis(50)).await;
    }

    // Verify 1 message persisted
    let conv = state.active_conversation.lock().await.clone().unwrap();
    let messages = store.load_messages(&conv.id, None, 10).await.unwrap();
    assert_eq!(messages.len(), 1);

    // Simulate Page Reload: Reconnect and re-emit existing conversation & messages
    {
        let (mut ws_stream, _) = connect_async(&ws_url).await.unwrap();
        let hello = ClientHello {
            protocol_version: CURRENT_PROTOCOL_VERSION.0,
            token: token.clone(),
            extension_version: "0.1.0".into(),
        };
        ws_stream
            .send(Message::Text(serde_json::to_string(&hello).unwrap().into()))
            .await
            .unwrap();
        let _ack = ws_stream.next().await.unwrap().unwrap();

        // Re-detect conversation
        let conv_event = BrowserEvent::ConversationDetected {
            conversation_id: Some("conv-abc".into()),
            external_conversation_ref: Some("chatgpt-conv-xyz".into()),
        };
        let env_conv = MessageEnvelope::new(
            session_id.clone(),
            CorrelationId::new("c-c2").unwrap(),
            conv_event,
        );
        ws_stream
            .send(Message::Text(env_conv.to_json().unwrap().into()))
            .await
            .unwrap();

        // DOM observer re-scans historical message "m-1"
        let user_event = BrowserEvent::UserMessageObserved {
            external_message_id: "m-1".into(),
            text: "First turn before reload".into(),
        };
        let env_user = MessageEnvelope::new(
            session_id.clone(),
            CorrelationId::new("c-u2").unwrap(),
            user_event,
        );
        ws_stream
            .send(Message::Text(env_user.to_json().unwrap().into()))
            .await
            .unwrap();
        tokio::time::sleep(Duration::from_millis(50)).await;

        // Verify still exactly 1 message (NO duplication!)
        let messages_after = store.load_messages(&conv.id, None, 10).await.unwrap();
        assert_eq!(messages_after.len(), 1);

        // Now send new message after reload
        let new_user_event = BrowserEvent::UserMessageObserved {
            external_message_id: "m-2".into(),
            text: "Second turn after reload".into(),
        };
        let env_new_user = MessageEnvelope::new(
            session_id.clone(),
            CorrelationId::new("c-u3").unwrap(),
            new_user_event,
        );
        ws_stream
            .send(Message::Text(env_new_user.to_json().unwrap().into()))
            .await
            .unwrap();
        tokio::time::sleep(Duration::from_millis(50)).await;

        // Verify exactly 2 messages in sequence 1 and 2
        let messages_final = store.load_messages(&conv.id, None, 10).await.unwrap();
        assert_eq!(messages_final.len(), 2);
        assert_eq!(messages_final[0].sequence, 1);
        assert_eq!(messages_final[1].sequence, 2);
        assert_eq!(messages_final[1].content, "Second turn after reload");
    }

    let _ = shutdown_tx.send(());
    let _ = server_handle.await;
}
