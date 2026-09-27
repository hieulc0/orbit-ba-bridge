//! WebSocket listener and connection lifecycle management.

use bridge_core::SessionManager;
use futures_util::{SinkExt, StreamExt};
use protocol::{BrowserCommand, BrowserEvent, CorrelationId, MessageEnvelope, SessionId};
use std::net::SocketAddr;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{broadcast, mpsc};
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::tungstenite::protocol::CloseFrame;
use tokio_tungstenite::tungstenite::protocol::frame::coding::CloseCode;
use tracing::{error, info, warn};

pub struct BridgeServer {
    listener: TcpListener,
    shutdown_rx: broadcast::Receiver<()>,
    active_connections: Arc<AtomicUsize>,
}

impl BridgeServer {
    pub async fn bind(addr: &str, shutdown_rx: broadcast::Receiver<()>) -> anyhow::Result<Self> {
        let listener = TcpListener::bind(addr).await?;
        info!("Bridge server listening on {}", listener.local_addr()?);
        Ok(Self {
            listener,
            shutdown_rx,
            active_connections: Arc::new(AtomicUsize::new(0)),
        })
    }

    pub fn local_addr(&self) -> std::io::Result<SocketAddr> {
        self.listener.local_addr()
    }

    pub fn active_connections(&self) -> usize {
        self.active_connections.load(Ordering::SeqCst)
    }

    pub async fn run(mut self) -> anyhow::Result<()> {
        loop {
            tokio::select! {
                res = self.listener.accept() => {
                    match res {
                        Ok((stream, peer_addr)) => {
                            info!(peer = %peer_addr, "Accepted incoming TCP connection");
                            let conn_count = Arc::clone(&self.active_connections);
                            let shutdown_rx = self.shutdown_rx.resubscribe();
                            tokio::spawn(async move {
                                conn_count.fetch_add(1, Ordering::SeqCst);
                                if let Err(e) = handle_connection(stream, peer_addr, shutdown_rx).await {
                                    warn!(peer = %peer_addr, error = %e, "Connection closed with error");
                                }
                                conn_count.fetch_sub(1, Ordering::SeqCst);
                            });
                        }
                        Err(e) => {
                            error!(error = %e, "Failed to accept TCP connection");
                        }
                    }
                }
                _ = self.shutdown_rx.recv() => {
                    info!("Bridge server received shutdown signal; stopping accept loop");
                    break;
                }
            }
        }
        Ok(())
    }
}

async fn handle_connection(
    stream: TcpStream,
    peer_addr: SocketAddr,
    mut shutdown_rx: broadcast::Receiver<()>,
) -> anyhow::Result<()> {
    let ws_stream = tokio_tungstenite::accept_async(stream).await?;
    info!(peer = %peer_addr, "WebSocket handshake completed");

    let (mut ws_sink, mut ws_stream) = ws_stream.split();
    let (cmd_tx, mut cmd_rx) = mpsc::channel::<MessageEnvelope<BrowserCommand>>(32);

    let session_id = SessionId::new(format!("browser-{}", peer_addr.port()))?;
    let mut session = SessionManager::new(session_id.clone());

    loop {
        tokio::select! {
            msg = ws_stream.next() => {
                match msg {
                    Some(Ok(Message::Text(text))) => {
                        match MessageEnvelope::<BrowserEvent>::from_json_str(&text) {
                            Ok(env) => {
                                info!(
                                    session_id = %env.session_id,
                                    correlation_id = %env.correlation_id,
                                    "Received browser event envelope"
                                );
                                let accepted = session.handle_event(&env.payload);
                                if !accepted {
                                    warn!("Suppressed duplicate event in session");
                                    continue;
                                }

                                match &env.payload {
                                    BrowserEvent::Connected => {
                                        info!("Browser extension connected");
                                        let ack = MessageEnvelope::new(
                                            session.session_id().clone(),
                                            CorrelationId::new("ack-connected")?,
                                            BrowserCommand::Ping,
                                        );
                                        let _ = cmd_tx.send(ack).await;
                                    }
                                    BrowserEvent::PageReady => {
                                        info!("ChatGPT page ready");
                                    }
                                    BrowserEvent::ConversationDetected { conversation_id } => {
                                        info!(conversation_id = ?conversation_id, "Conversation detected");
                                    }
                                    BrowserEvent::AssistantMessage { message_id, text } => {
                                        info!(message_id = %message_id, length = text.len(), "Assistant message received");
                                    }
                                    BrowserEvent::UserMessage { message_id, text } => {
                                        info!(message_id = %message_id, length = text.len(), "User message received");
                                    }
                                    BrowserEvent::SessionUnavailable { reason } => {
                                        warn!(reason = %reason, "Session reported unavailable");
                                    }
                                }
                            }
                            Err(e) => {
                                warn!(error = %e, "Rejected invalid message envelope");
                                let close_frame = CloseFrame {
                                    code: CloseCode::Policy,
                                    reason: e.to_string().into(),
                                };
                                let _ = ws_sink.send(Message::Close(Some(close_frame))).await;
                                break;
                            }
                        }
                    }
                    Some(Ok(Message::Ping(data))) => {
                        let _ = ws_sink.send(Message::Pong(data)).await;
                    }
                    Some(Ok(Message::Close(_))) | None => {
                        info!(peer = %peer_addr, "Browser extension disconnected");
                        session.reset_connection();
                        break;
                    }
                    Some(Err(e)) => {
                        warn!(peer = %peer_addr, error = %e, "WebSocket error");
                        break;
                    }
                    _ => {}
                }
            }
            cmd = cmd_rx.recv() => {
                if let Some(cmd_envelope) = cmd {
                    let text = cmd_envelope.to_json()?;
                    ws_sink.send(Message::Text(text.into())).await?;
                }
            }
            _ = shutdown_rx.recv() => {
                info!(peer = %peer_addr, "Closing connection due to server shutdown");
                let _ = ws_sink.send(Message::Close(None)).await;
                break;
            }
        }
    }

    Ok(())
}
