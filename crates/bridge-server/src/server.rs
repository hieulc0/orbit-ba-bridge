//! WebSocket listener, token handshake, and connection lifecycle management.

use bridge_core::{
    ActorRole, Conversation, ConversationMessage, ConversationStore, InjectionLedger, MessageKind,
    NewConversation, NewMessage, NewParticipant, Participant, ParticipantSource, SessionManager,
    SessionState, format_external_injection,
};
use futures_util::{SinkExt, StreamExt};
use protocol::{
    BridgeErrorCode, BrowserCommand, BrowserEvent, CURRENT_PROTOCOL_VERSION, ClientHello,
    CorrelationId, InjectionId, MessageEnvelope, ServerHelloAck, SessionId,
};
use std::net::SocketAddr;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{Mutex, broadcast, mpsc};
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::tungstenite::protocol::CloseFrame;
use tokio_tungstenite::tungstenite::protocol::frame::coding::CloseCode;
use tracing::{debug, error, info, warn};

/// Shared runtime context for the server instance.
pub struct ServerState {
    pub token: String,
    pub store: Arc<dyn ConversationStore>,
    pub ledger: Arc<InjectionLedger>,
    pub active_conversation: Mutex<Option<Conversation>>,
    pub human_participant: Mutex<Option<Participant>>,
    pub ba_participant: Mutex<Option<Participant>>,
    pub cmd_broadcast_tx: broadcast::Sender<MessageEnvelope<BrowserCommand>>,
    pub active_session_state: Mutex<SessionState>,
    pub attached_session_id: Mutex<Option<SessionId>>,
}

pub struct BridgeServer {
    listener: TcpListener,
    shutdown_rx: broadcast::Receiver<()>,
    active_connections: Arc<AtomicUsize>,
    state: Arc<ServerState>,
}

impl BridgeServer {
    pub async fn bind(
        addr: &str,
        token: String,
        store: Arc<dyn ConversationStore>,
        shutdown_rx: broadcast::Receiver<()>,
    ) -> anyhow::Result<Self> {
        let listener = TcpListener::bind(addr).await?;
        info!("Bridge server listening on {}", listener.local_addr()?);

        let (cmd_broadcast_tx, _) = broadcast::channel(64);
        let ledger = Arc::new(InjectionLedger::new());

        let state = Arc::new(ServerState {
            token,
            store,
            ledger,
            active_conversation: Mutex::new(None),
            human_participant: Mutex::new(None),
            ba_participant: Mutex::new(None),
            cmd_broadcast_tx,
            active_session_state: Mutex::new(SessionState::Disconnected),
            attached_session_id: Mutex::new(None),
        });

        Ok(Self {
            listener,
            shutdown_rx,
            active_connections: Arc::new(AtomicUsize::new(0)),
            state,
        })
    }

    pub fn local_addr(&self) -> std::io::Result<SocketAddr> {
        self.listener.local_addr()
    }

    pub fn active_connections(&self) -> usize {
        self.active_connections.load(Ordering::SeqCst)
    }

    pub fn state(&self) -> Arc<ServerState> {
        Arc::clone(&self.state)
    }

    pub fn token(&self) -> &str {
        &self.state.token
    }

    /// Injects an external turn from a logical participant (e.g. Orbit SA or BA Research)
    /// into the connected browser session.
    pub async fn inject(
        &self,
        role: ActorRole,
        display_name: &str,
        source: ParticipantSource,
        content: &str,
    ) -> anyhow::Result<InjectionId> {
        self.state.inject(role, display_name, source, content).await
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
                            let state = Arc::clone(&self.state);
                            tokio::spawn(async move {
                                conn_count.fetch_add(1, Ordering::SeqCst);
                                if let Err(e) = handle_connection(stream, peer_addr, state, shutdown_rx).await {
                                    warn!(peer = %peer_addr, error = %e, "Connection closed");
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

impl ServerState {
    /// Injects an external message to the connected browser session.
    pub async fn inject(
        &self,
        role: ActorRole,
        display_name: &str,
        source: ParticipantSource,
        content: &str,
    ) -> anyhow::Result<InjectionId> {
        let conv = self.get_active_or_default_conversation().await?;

        // Ensure participant exists in database
        let mut existing_part = None;
        let participants = self.store.list_participants(&conv.id).await?;
        for p in participants {
            if p.display_name == display_name && p.role == role {
                existing_part = Some(p);
                break;
            }
        }

        let participant = match existing_part {
            Some(p) => p,
            None => {
                self.store
                    .add_participant(NewParticipant {
                        id: None,
                        conversation_id: conv.id.clone(),
                        role,
                        display_name: display_name.to_string(),
                        source,
                    })
                    .await?
            }
        };

        let injection_id = InjectionId::generate();
        let correlation_id = CorrelationId::generate();
        let formatted = format_external_injection(display_name, content);

        self.ledger.record_injection(
            injection_id.clone(),
            correlation_id.clone(),
            participant.id.clone(),
            &formatted,
        );

        let command = BrowserCommand::InjectMessage {
            injection_id: injection_id.clone(),
            correlation_id: correlation_id.clone(),
            text: formatted,
        };

        let envelope = MessageEnvelope::new(
            SessionId::new("server-dispatch")?,
            correlation_id.clone(),
            command,
        );

        info!(
            injection_id = %injection_id,
            correlation_id = %correlation_id,
            actor = %display_name,
            "injection_requested"
        );

        let _ = self.cmd_broadcast_tx.send(envelope);

        Ok(injection_id)
    }

    /// Create a new conversation and initialize its default Human and BA Product participants.
    pub async fn create_conversation_for_ref(
        &self,
        external_ref: Option<String>,
    ) -> anyhow::Result<Conversation> {
        let title = match &external_ref {
            Some(r) => format!("ChatGPT Discussion ({})", r),
            None => "ChatGPT Discussion".to_string(),
        };

        let conv = self
            .store
            .create_conversation(NewConversation {
                id: None,
                title: Some(title),
                orbit_workflow_id: None,
                external_conversation_ref: external_ref,
            })
            .await?;

        self.store
            .add_participant(NewParticipant {
                id: None,
                conversation_id: conv.id.clone(),
                role: ActorRole::Human,
                display_name: "Human".into(),
                source: ParticipantSource::HumanBrowser,
            })
            .await?;

        self.store
            .add_participant(NewParticipant {
                id: None,
                conversation_id: conv.id.clone(),
                role: ActorRole::BusinessAnalyst,
                display_name: "BA Product".into(),
                source: ParticipantSource::ChatGptWeb,
            })
            .await?;

        Ok(conv)
    }

    /// Sets the active conversation and refreshes cached participant references.
    pub async fn set_active_conversation(&self, conv: Conversation) -> anyhow::Result<()> {
        let mut human = None;
        let mut ba = None;

        let participants = self.store.list_participants(&conv.id).await?;
        for p in participants {
            if p.role == ActorRole::Human && human.is_none() {
                human = Some(p);
            } else if p.role == ActorRole::BusinessAnalyst && ba.is_none() {
                ba = Some(p);
            }
        }

        let human = match human {
            Some(h) => h,
            None => {
                self.store
                    .add_participant(NewParticipant {
                        id: None,
                        conversation_id: conv.id.clone(),
                        role: ActorRole::Human,
                        display_name: "Human".into(),
                        source: ParticipantSource::HumanBrowser,
                    })
                    .await?
            }
        };

        let ba = match ba {
            Some(b) => b,
            None => {
                self.store
                    .add_participant(NewParticipant {
                        id: None,
                        conversation_id: conv.id.clone(),
                        role: ActorRole::BusinessAnalyst,
                        display_name: "BA Product".into(),
                        source: ParticipantSource::ChatGptWeb,
                    })
                    .await?
            }
        };

        *self.human_participant.lock().await = Some(human);
        *self.ba_participant.lock().await = Some(ba);
        *self.active_conversation.lock().await = Some(conv);

        Ok(())
    }

    /// Switch to existing conversation by external ref, bind active temporary conversation,
    /// or create a new conversation for this ref.
    pub async fn switch_or_bind_conversation(
        &self,
        external_ref: &str,
    ) -> anyhow::Result<Conversation> {
        let current_opt = self.active_conversation.lock().await.clone();

        if let Some(active) = current_opt {
            if active.external_conversation_ref.as_deref() == Some(external_ref) {
                return Ok(active);
            }

            if active.external_conversation_ref.is_none() {
                if let Some(existing) = self
                    .store
                    .find_conversation_by_external_ref(external_ref)
                    .await?
                {
                    self.set_active_conversation(existing.clone()).await?;
                    return Ok(existing);
                } else {
                    self.store
                        .bind_external_ref(&active.id, external_ref)
                        .await?;
                    let mut updated = active.clone();
                    updated.external_conversation_ref = Some(external_ref.to_string());
                    *self.active_conversation.lock().await = Some(updated.clone());
                    return Ok(updated);
                }
            }

            // Switched to a different external ref
            if let Some(existing) = self
                .store
                .find_conversation_by_external_ref(external_ref)
                .await?
            {
                self.set_active_conversation(existing.clone()).await?;
                return Ok(existing);
            } else {
                let new_conv = self
                    .create_conversation_for_ref(Some(external_ref.to_string()))
                    .await?;
                self.set_active_conversation(new_conv.clone()).await?;
                return Ok(new_conv);
            }
        }

        // No active conversation currently set
        if let Some(existing) = self
            .store
            .find_conversation_by_external_ref(external_ref)
            .await?
        {
            self.set_active_conversation(existing.clone()).await?;
            Ok(existing)
        } else {
            let new_conv = self
                .create_conversation_for_ref(Some(external_ref.to_string()))
                .await?;
            self.set_active_conversation(new_conv.clone()).await?;
            Ok(new_conv)
        }
    }

    /// Ensure an unbound conversation exists as the active conversation.
    pub async fn ensure_unbound_conversation(&self) -> anyhow::Result<Conversation> {
        let lock = self.active_conversation.lock().await;
        if let Some(existing) = &*lock {
            return Ok(existing.clone());
        }

        drop(lock);
        let conv = self.create_conversation_for_ref(None).await?;
        self.set_active_conversation(conv.clone()).await?;
        Ok(conv)
    }

    /// Retrieve the current active conversation or initialize a default unbound one.
    pub async fn get_active_or_default_conversation(&self) -> anyhow::Result<Conversation> {
        let lock = self.active_conversation.lock().await;
        if let Some(existing) = &*lock {
            return Ok(existing.clone());
        }
        drop(lock);
        self.ensure_unbound_conversation().await
    }

    pub async fn list_messages(&self) -> anyhow::Result<Vec<ConversationMessage>> {
        let lock = self.active_conversation.lock().await;
        if let Some(conv) = &*lock {
            self.store.load_messages(&conv.id, None, 100).await
        } else {
            Ok(Vec::new())
        }
    }
}

async fn handle_connection(
    stream: TcpStream,
    peer_addr: SocketAddr,
    state: Arc<ServerState>,
    mut shutdown_rx: broadcast::Receiver<()>,
) -> anyhow::Result<()> {
    let mut ws_stream = tokio_tungstenite::accept_async(stream).await?;
    info!(peer = %peer_addr, "WebSocket connection opened");

    let session_id = SessionId::new(format!("browser-{}", peer_addr.port()))?;
    let mut session = SessionManager::new(session_id.clone());

    // -------------------------------------------------------------
    // Step 1: Session Handshake Validation
    // -------------------------------------------------------------
    let handshake_ok = match ws_stream.next().await {
        Some(Ok(Message::Text(raw))) => {
            let client_hello = if let Ok(h) = serde_json::from_str::<ClientHello>(&raw) {
                Some(h)
            } else if let Ok(env) = MessageEnvelope::<BrowserEvent>::from_json_str(&raw) {
                if let BrowserEvent::Hello {
                    protocol_version,
                    token,
                    extension_version,
                } = env.payload
                {
                    Some(ClientHello {
                        protocol_version,
                        token,
                        extension_version,
                    })
                } else {
                    None
                }
            } else {
                None
            };

            match client_hello {
                Some(h) => {
                    if h.protocol_version != CURRENT_PROTOCOL_VERSION.0 {
                        warn!(
                            code = %BridgeErrorCode::ProtocolVersionMismatch,
                            version = h.protocol_version,
                            "Handshake rejected: unsupported protocol version"
                        );
                        let _ = ws_stream
                            .send(Message::Close(Some(CloseFrame {
                                code: CloseCode::Policy,
                                reason: BridgeErrorCode::ProtocolVersionMismatch.as_str().into(),
                            })))
                            .await;
                        false
                    } else if h.token != state.token {
                        warn!(
                            code = %BridgeErrorCode::AuthFailed,
                            "Handshake rejected: invalid bridge auth token"
                        );
                        let _ = ws_stream
                            .send(Message::Close(Some(CloseFrame {
                                code: CloseCode::Policy,
                                reason: BridgeErrorCode::AuthFailed.as_str().into(),
                            })))
                            .await;
                        false
                    } else {
                        // Enforce single attached browser session policy
                        let mut attached = state.attached_session_id.lock().await;
                        if let Some(existing_session) = &*attached {
                            warn!(
                                existing = %existing_session,
                                incoming = %session_id,
                                code = %BridgeErrorCode::MultipleChatgptTabs,
                                "Multiple browser connections rejected: bridge requires a single attached session"
                            );
                            let _ = ws_stream
                                .send(Message::Close(Some(CloseFrame {
                                    code: CloseCode::Policy,
                                    reason: BridgeErrorCode::MultipleChatgptTabs.as_str().into(),
                                })))
                                .await;
                            false
                        } else {
                            *attached = Some(session_id.clone());
                            drop(attached);

                            info!(
                                session_id = %session_id,
                                ext_version = %h.extension_version,
                                "browser_connected"
                            );
                            session.handle_event(&BrowserEvent::Connected);
                            *state.active_session_state.lock().await = SessionState::Connected;

                            let ack = ServerHelloAck {
                                protocol_version: CURRENT_PROTOCOL_VERSION.0,
                                session_id: session_id.clone(),
                                accepted: true,
                            };
                            let ack_json = serde_json::to_string(&ack)?;
                            ws_stream.send(Message::Text(ack_json.into())).await?;
                            true
                        }
                    }
                }
                None => {
                    warn!("Handshake rejected: missing or malformed Hello frame");
                    let _ = ws_stream
                        .send(Message::Close(Some(CloseFrame {
                            code: CloseCode::Policy,
                            reason: "missing hello handshake".into(),
                        })))
                        .await;
                    false
                }
            }
        }
        _ => false,
    };

    if !handshake_ok {
        return Ok(());
    }

    // -------------------------------------------------------------
    // Step 2: Main Event & Command Loop
    // -------------------------------------------------------------
    let (mut ws_sink, mut ws_stream) = ws_stream.split();
    let mut broadcast_rx = state.cmd_broadcast_tx.subscribe();
    let (cmd_tx, mut cmd_rx) = mpsc::channel::<MessageEnvelope<BrowserCommand>>(32);

    loop {
        tokio::select! {
            msg = ws_stream.next() => {
                match msg {
                    Some(Ok(Message::Text(text))) => {
                        match MessageEnvelope::<BrowserEvent>::from_json_str(&text) {
                            Ok(env) => {
                                let accepted = session.handle_event(&env.payload);
                                if !accepted {
                                    debug!("Suppressed duplicate event in session deduplication cache");
                                    continue;
                                }

                                match env.payload {
                                    BrowserEvent::PageReady { external_url } => {
                                        info!(url = ?external_url, "page_ready");
                                        *state.active_session_state.lock().await = SessionState::PageReady;
                                    }
                                    BrowserEvent::ConversationDetected { conversation_id, external_conversation_ref } => {
                                        let conv = if let Some(ref_str) = &external_conversation_ref {
                                            state.switch_or_bind_conversation(ref_str).await?
                                        } else {
                                            state.ensure_unbound_conversation().await?
                                        };

                                        *state.active_session_state.lock().await = SessionState::ConversationReady;
                                        info!(
                                            conversation_id = %conv.id,
                                            detected_id = ?conversation_id,
                                            external_ref = ?external_conversation_ref,
                                            "conversation_selected"
                                        );
                                    }
                                    BrowserEvent::InjectionAccepted { injection_id } => {
                                        state.ledger.mark_accepted(&injection_id);
                                        info!(injection_id = %injection_id, "injection_accepted");
                                    }
                                    BrowserEvent::InjectionMaterialized { injection_id, external_message_id } => {
                                        state.ledger.mark_materialized(&injection_id, &external_message_id);
                                        info!(
                                            injection_id = %injection_id,
                                            external_message_id = %external_message_id,
                                            "injection_materialized"
                                        );
                                    }
                                    BrowserEvent::UserMessageObserved { external_message_id, text } => {
                                        let conv = state.get_active_or_default_conversation().await?;
                                        let trimmed = text.trim();

                                        // Store deduplication & branch mutation check
                                        if let Some(existing) = state.store.find_message_by_external_id(&conv.id, &external_message_id).await? {
                                            if existing.content == trimmed {
                                                debug!(external_id = %external_message_id, "duplicate_message_suppressed");
                                            } else {
                                                warn!(
                                                    code = %BridgeErrorCode::UnsupportedBranchMutation,
                                                    external_id = %external_message_id,
                                                    "UNSUPPORTED_BRANCH_MUTATION: external user message content changed; preserving immutable original in store"
                                                );
                                            }
                                            continue;
                                        }

                                        // Resolve logical actor
                                        let actor_id = if let Some(injected_actor) = state.ledger.resolve_user_message(Some(&external_message_id), None, trimmed) {
                                            injected_actor
                                        } else {
                                            state.human_participant.lock().await.as_ref().unwrap().id.clone()
                                        };

                                        let msg = state.store.append_message(NewMessage {
                                            id: None,
                                            conversation_id: conv.id.clone(),
                                            actor_id: actor_id.clone(),
                                            kind: MessageKind::Conversation,
                                            content: trimmed.to_string(),
                                            correlation_id: None,
                                            reply_to: None,
                                            external_message_id: Some(external_message_id.clone()),
                                            orbit_workflow_id: None,
                                            orbit_role_execution_id: None,
                                        }).await?;

                                        state.store.link_external_message(&msg.id, &external_message_id).await?;

                                        info!(
                                            actor_id = %actor_id,
                                            sequence = msg.sequence,
                                            length = trimmed.len(),
                                            external_id = %external_message_id,
                                            "message_observed"
                                        );
                                    }
                                    BrowserEvent::AssistantMessageObserved { external_message_id, text, is_final } => {
                                        if !is_final {
                                            debug!(length = text.len(), "assistant_streaming_update");
                                            continue;
                                        }

                                        let conv = state.get_active_or_default_conversation().await?;
                                        let trimmed = text.trim();

                                        // Store deduplication & branch mutation check
                                        if let Some(existing) = state.store.find_message_by_external_id(&conv.id, &external_message_id).await? {
                                            if existing.content == trimmed {
                                                debug!(external_id = %external_message_id, "duplicate_message_suppressed");
                                            } else {
                                                warn!(
                                                    code = %BridgeErrorCode::UnsupportedBranchMutation,
                                                    external_id = %external_message_id,
                                                    "UNSUPPORTED_BRANCH_MUTATION: external assistant message content changed; preserving immutable original in store"
                                                );
                                            }
                                            continue;
                                        }

                                        let ba = state.ba_participant.lock().await.as_ref().unwrap().clone();

                                        let msg = state.store.append_message(NewMessage {
                                            id: None,
                                            conversation_id: conv.id.clone(),
                                            actor_id: ba.id.clone(),
                                            kind: MessageKind::Response,
                                            content: trimmed.to_string(),
                                            correlation_id: None,
                                            reply_to: None,
                                            external_message_id: Some(external_message_id.clone()),
                                            orbit_workflow_id: None,
                                            orbit_role_execution_id: None,
                                        }).await?;

                                        state.store.link_external_message(&msg.id, &external_message_id).await?;

                                        info!(
                                            actor_id = %ba.id,
                                            sequence = msg.sequence,
                                            length = trimmed.len(),
                                            external_id = %external_message_id,
                                            "assistant_response_observed"
                                        );
                                    }
                                    BrowserEvent::PageUnavailable { reason } => {
                                        if reason.contains("MULTIPLE_CHATGPT_TABS") {
                                            warn!(
                                                code = %BridgeErrorCode::MultipleChatgptTabs,
                                                reason = %reason,
                                                "Multiple ChatGPT tabs detected: refusing ambiguous routing"
                                            );
                                        } else {
                                            warn!(reason = %reason, "page_unavailable");
                                        }
                                        *state.active_session_state.lock().await = SessionState::Unavailable;
                                    }
                                    BrowserEvent::Disconnected => {
                                        info!(session_id = %session_id, "browser_disconnected");
                                        session.reset_connection();
                                        *state.attached_session_id.lock().await = None;
                                        *state.active_session_state.lock().await = SessionState::Disconnected;
                                        break;
                                    }
                                    BrowserEvent::Connected => {
                                        let ack = MessageEnvelope::new(
                                            session.session_id().clone(),
                                            CorrelationId::new("ack-ping")?,
                                            BrowserCommand::Ping,
                                        );
                                        let _ = cmd_tx.send(ack).await;
                                    }
                                    _ => {}
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
                        info!(peer = %peer_addr, "browser_disconnected");
                        session.reset_connection();
                        *state.attached_session_id.lock().await = None;
                        *state.active_session_state.lock().await = SessionState::Disconnected;
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
            broadcast_cmd = broadcast_rx.recv() => {
                if let Ok(cmd_envelope) = broadcast_cmd {
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

    *state.attached_session_id.lock().await = None;
    *state.active_session_state.lock().await = SessionState::Disconnected;

    Ok(())
}
