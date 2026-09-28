use bridge_core::{ConversationStore, SqliteConversationStore};
use bridge_server::BridgeServer;
use bridge_server::commands::handle_conversation_action;
use bridge_server::config::{ConversationAction, ExportFormat};
use futures_util::{SinkExt, StreamExt};
use protocol::{
    BrowserEvent, CURRENT_PROTOCOL_VERSION, ClientHello, CorrelationId, MessageEnvelope, SessionId,
};
use std::sync::Arc;
use std::time::Duration;
use tempfile::NamedTempFile;
use tokio::sync::broadcast;
use tokio_tungstenite::connect_async;
use tokio_tungstenite::tungstenite::Message;

#[tokio::test]
async fn test_server_restart_preserves_persistent_sqlite_state() -> anyhow::Result<()> {
    let tmp_db = NamedTempFile::new()?;
    let db_path = tmp_db.path().to_path_buf();
    let token = "test-persistent-token".to_string();

    // -------------------------------------------------------------
    // Session 1: Server run 1
    // -------------------------------------------------------------
    {
        let store = Arc::new(SqliteConversationStore::open(&db_path)?);
        let (shutdown_tx, shutdown_rx) = broadcast::channel(1);
        let server =
            BridgeServer::bind("127.0.0.1:0", token.clone(), store.clone(), shutdown_rx).await?;
        let addr = server.local_addr()?;

        let server_handle = tokio::spawn(async move {
            server.run().await.unwrap();
        });

        let (mut ws, _) = connect_async(format!("ws://{addr}")).await?;
        let hello = ClientHello {
            protocol_version: CURRENT_PROTOCOL_VERSION.0,
            token: token.clone(),
            extension_version: "0.1.0".into(),
        };
        ws.send(Message::Text(serde_json::to_string(&hello)?.into()))
            .await?;
        let ack_msg = ws.next().await.unwrap()?;
        assert!(matches!(ack_msg, Message::Text(_)));

        let sess = SessionId::new("sess-run-1")?;
        let env_conn = MessageEnvelope::new(
            sess.clone(),
            CorrelationId::new("c-1")?,
            BrowserEvent::Connected,
        );
        ws.send(Message::Text(env_conn.to_json()?.into())).await?;
        let _ping = ws.next().await.unwrap()?;

        let env_conv = MessageEnvelope::new(
            sess.clone(),
            CorrelationId::new("c-conv")?,
            BrowserEvent::ConversationDetected {
                conversation_id: Some("conv-persisted".into()),
                external_conversation_ref: Some("/c/persisted-chat".into()),
            },
        );
        ws.send(Message::Text(env_conv.to_json()?.into())).await?;

        let env_msg = MessageEnvelope::new(
            sess.clone(),
            CorrelationId::new("c-msg")?,
            BrowserEvent::UserMessageObserved {
                external_message_id: "m-before-restart".into(),
                text: "Persisted turn before restart".into(),
            },
        );
        ws.send(Message::Text(env_msg.to_json()?.into())).await?;
        tokio::time::sleep(Duration::from_millis(50)).await;

        let _ = shutdown_tx.send(());
        let _ = server_handle.await;
    }

    // -------------------------------------------------------------
    // Session 2: Server run 2 (after bridge restart)
    // -------------------------------------------------------------
    {
        let store = Arc::new(SqliteConversationStore::open(&db_path)?);
        let (shutdown_tx, shutdown_rx) = broadcast::channel(1);
        let server =
            BridgeServer::bind("127.0.0.1:0", token.clone(), store.clone(), shutdown_rx).await?;
        let addr = server.local_addr()?;

        let server_handle = tokio::spawn(async move {
            server.run().await.unwrap();
        });

        let (mut ws, _) = connect_async(format!("ws://{addr}")).await?;
        let hello = ClientHello {
            protocol_version: CURRENT_PROTOCOL_VERSION.0,
            token: token.clone(),
            extension_version: "0.1.0".into(),
        };
        ws.send(Message::Text(serde_json::to_string(&hello)?.into()))
            .await?;
        let _ack = ws.next().await.unwrap()?;

        let sess = SessionId::new("sess-run-2")?;
        let env_conn = MessageEnvelope::new(
            sess.clone(),
            CorrelationId::new("c-2")?,
            BrowserEvent::Connected,
        );
        ws.send(Message::Text(env_conn.to_json()?.into())).await?;
        let _ping = ws.next().await.unwrap()?;

        // Connect back to the same conversation
        let env_conv = MessageEnvelope::new(
            sess.clone(),
            CorrelationId::new("c-conv2")?,
            BrowserEvent::ConversationDetected {
                conversation_id: Some("conv-persisted".into()),
                external_conversation_ref: Some("/c/persisted-chat".into()),
            },
        );
        ws.send(Message::Text(env_conv.to_json()?.into())).await?;

        // Replayed DOM message should be suppressed
        let env_dup = MessageEnvelope::new(
            sess.clone(),
            CorrelationId::new("c-dup")?,
            BrowserEvent::UserMessageObserved {
                external_message_id: "m-before-restart".into(),
                text: "Persisted turn before restart".into(),
            },
        );
        ws.send(Message::Text(env_dup.to_json()?.into())).await?;

        // New message after restart
        let env_msg2 = MessageEnvelope::new(
            sess.clone(),
            CorrelationId::new("c-msg2")?,
            BrowserEvent::UserMessageObserved {
                external_message_id: "m-after-restart".into(),
                text: "Turn after server restart".into(),
            },
        );
        ws.send(Message::Text(env_msg2.to_json()?.into())).await?;
        tokio::time::sleep(Duration::from_millis(50)).await;

        let conv = store
            .find_conversation_by_external_ref("/c/persisted-chat")
            .await?
            .expect("conversation must persist across restart");

        let messages = store.load_messages(&conv.id, None, 10).await?;
        assert_eq!(messages.len(), 2);
        assert_eq!(messages[0].sequence, 1);
        assert_eq!(messages[0].content, "Persisted turn before restart");
        assert_eq!(messages[1].sequence, 2);
        assert_eq!(messages[1].content, "Turn after server restart");

        // Test CLI action commands on the persistent store
        let export_tmp = NamedTempFile::new()?;
        let export_path = export_tmp.path().to_string_lossy().to_string();

        let action_export = ConversationAction::Export {
            id: conv.id.to_string(),
            format: ExportFormat::Markdown,
            output: Some(export_path.clone()),
            db_path: None,
        };
        handle_conversation_action(&action_export, store.as_ref()).await?;

        let exported_md = std::fs::read_to_string(&export_path)?;
        assert!(exported_md.contains("ChatGPT conversation: /c/persisted-chat"));
        assert!(exported_md.contains("Persisted turn before restart"));
        assert!(exported_md.contains("Turn after server restart"));

        let action_list = ConversationAction::List {
            limit: 10,
            offset: 0,
            db_path: None,
        };
        handle_conversation_action(&action_list, store.as_ref()).await?;

        let action_search = ConversationAction::Search {
            query: "server restart".into(),
            limit: 10,
            db_path: None,
        };
        handle_conversation_action(&action_search, store.as_ref()).await?;

        let _ = shutdown_tx.send(());
        let _ = server_handle.await;
    }

    Ok(())
}
