//! Wire and domain types for `orbit-ba-bridge`.
//!
//! Defines versioned message envelopes, browser events, commands, identifiers, and validation rules.

use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::fmt;

/// The current supported protocol version.
pub const CURRENT_PROTOCOL_VERSION: ProtocolVersion = ProtocolVersion(1);

/// Maximum allowed payload size in bytes (256 KiB).
pub const MAX_PAYLOAD_BYTES: usize = 256 * 1024;

/// Maximum allowed message text length in characters (128 KiB).
pub const MAX_MESSAGE_TEXT_LENGTH: usize = 128 * 1024;

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum ProtocolError {
    #[error("unsupported protocol version: {0}")]
    UnsupportedVersion(u16),

    #[error("payload exceeds maximum size of {max} bytes (was {actual} bytes)")]
    PayloadTooLarge { actual: usize, max: usize },

    #[error("message text exceeds maximum length of {max} chars (was {actual} chars)")]
    TextTooLarge { actual: usize, max: usize },

    #[error("session ID cannot be empty")]
    EmptySessionId,

    #[error("correlation ID cannot be empty")]
    EmptyCorrelationId,

    #[error("task ID cannot be empty")]
    EmptyTaskId,

    #[error("conversation ID cannot be empty")]
    EmptyConversationId,

    #[error("participant ID cannot be empty")]
    EmptyParticipantId,

    #[error("message ID cannot be empty")]
    EmptyMessageId,

    #[error("artifact ID cannot be empty")]
    EmptyArtifactId,

    #[error("injection ID cannot be empty")]
    EmptyInjectionId,

    #[error("invalid auth token")]
    InvalidAuthToken,

    #[error("serialization error: {0}")]
    Serialization(String),
}

/// Standard operator and client error codes for bridge interactions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum BridgeErrorCode {
    BrowserNotConnected,
    ChatgptPageNotReady,
    MultipleChatgptTabs,
    ConversationNotFound,
    ExternalConversationChanged,
    ComposerUnavailable,
    DatabaseError,
    AuthFailed,
    ProtocolVersionMismatch,
    UnsupportedBranchMutation,
}

impl BridgeErrorCode {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::BrowserNotConnected => "BROWSER_NOT_CONNECTED",
            Self::ChatgptPageNotReady => "CHATGPT_PAGE_NOT_READY",
            Self::MultipleChatgptTabs => "MULTIPLE_CHATGPT_TABS",
            Self::ConversationNotFound => "CONVERSATION_NOT_FOUND",
            Self::ExternalConversationChanged => "EXTERNAL_CONVERSATION_CHANGED",
            Self::ComposerUnavailable => "COMPOSER_UNAVAILABLE",
            Self::DatabaseError => "DATABASE_ERROR",
            Self::AuthFailed => "AUTH_FAILED",
            Self::ProtocolVersionMismatch => "PROTOCOL_VERSION_MISMATCH",
            Self::UnsupportedBranchMutation => "UNSUPPORTED_BRANCH_MUTATION",
        }
    }
}

impl fmt::Display for BridgeErrorCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

/// Version of the bridge protocol.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ProtocolVersion(pub u16);

impl Serialize for ProtocolVersion {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_u16(self.0)
    }
}

impl<'de> Deserialize<'de> for ProtocolVersion {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let version = u16::deserialize(deserializer)?;
        Ok(ProtocolVersion(version))
    }
}

impl ProtocolVersion {
    pub fn is_supported(&self) -> bool {
        self.0 == CURRENT_PROTOCOL_VERSION.0
    }
}

impl fmt::Display for ProtocolVersion {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "v{}", self.0)
    }
}

/// Identifier for a browser connection/session.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct SessionId(pub String);

impl SessionId {
    pub fn new(id: impl Into<String>) -> Result<Self, ProtocolError> {
        let s = id.into();
        if s.trim().is_empty() {
            return Err(ProtocolError::EmptySessionId);
        }
        Ok(Self(s))
    }

    pub fn generate() -> Self {
        Self(format!("session-{}", uuid::Uuid::new_v4()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for SessionId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Identifier for message correlation across bridge boundaries.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct CorrelationId(pub String);

impl CorrelationId {
    pub fn new(id: impl Into<String>) -> Result<Self, ProtocolError> {
        let s = id.into();
        if s.trim().is_empty() {
            return Err(ProtocolError::EmptyCorrelationId);
        }
        Ok(Self(s))
    }

    pub fn generate() -> Self {
        Self(format!("corr-{}", uuid::Uuid::new_v4()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for CorrelationId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Identifier for an Orbit task or workflow unit.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct TaskId(pub String);

impl TaskId {
    pub fn new(id: impl Into<String>) -> Result<Self, ProtocolError> {
        let s = id.into();
        if s.trim().is_empty() {
            return Err(ProtocolError::EmptyTaskId);
        }
        Ok(Self(s))
    }

    pub fn generate() -> Self {
        Self(format!("task-{}", uuid::Uuid::new_v4()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for TaskId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Identifier for a conversation.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ConversationId(pub String);

impl ConversationId {
    pub fn new(id: impl Into<String>) -> Result<Self, ProtocolError> {
        let s = id.into();
        if s.trim().is_empty() {
            return Err(ProtocolError::EmptyConversationId);
        }
        Ok(Self(s))
    }

    pub fn generate() -> Self {
        Self(format!("conv-{}", uuid::Uuid::new_v4()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for ConversationId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Identifier for a conversation participant.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ParticipantId(pub String);

impl ParticipantId {
    pub fn new(id: impl Into<String>) -> Result<Self, ProtocolError> {
        let s = id.into();
        if s.trim().is_empty() {
            return Err(ProtocolError::EmptyParticipantId);
        }
        Ok(Self(s))
    }

    pub fn generate() -> Self {
        Self(format!("part-{}", uuid::Uuid::new_v4()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for ParticipantId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Identifier for a conversation message.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct MessageId(pub String);

impl MessageId {
    pub fn new(id: impl Into<String>) -> Result<Self, ProtocolError> {
        let s = id.into();
        if s.trim().is_empty() {
            return Err(ProtocolError::EmptyMessageId);
        }
        Ok(Self(s))
    }

    pub fn generate() -> Self {
        Self(format!("msg-{}", uuid::Uuid::new_v4()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for MessageId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Identifier for a structured engineering artifact.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ArtifactId(pub String);

impl ArtifactId {
    pub fn new(id: impl Into<String>) -> Result<Self, ProtocolError> {
        let s = id.into();
        if s.trim().is_empty() {
            return Err(ProtocolError::EmptyArtifactId);
        }
        Ok(Self(s))
    }

    pub fn generate() -> Self {
        Self(format!("art-{}", uuid::Uuid::new_v4()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for ArtifactId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Identifier for an outbound message injection turn.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct InjectionId(pub String);

impl InjectionId {
    pub fn new(id: impl Into<String>) -> Result<Self, ProtocolError> {
        let s = id.into();
        if s.trim().is_empty() {
            return Err(ProtocolError::EmptyInjectionId);
        }
        Ok(Self(s))
    }

    pub fn generate() -> Self {
        Self(format!("inj-{}", uuid::Uuid::new_v4()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for InjectionId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Client handshake payload sent upon establishing a WebSocket connection.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClientHello {
    pub protocol_version: u16,
    pub token: String,
    pub extension_version: String,
}

/// Server handshake response validating connection and session establishment.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ServerHelloAck {
    pub protocol_version: u16,
    pub session_id: SessionId,
    pub accepted: bool,
}

/// Supported external roles.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExternalRole {
    BusinessAnalyst,
}

/// Events emitted by the browser extension to the bridge.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", content = "payload", rename_all = "snake_case")]
pub enum BrowserEvent {
    Hello {
        protocol_version: u16,
        token: String,
        extension_version: String,
    },
    Connected,
    PageReady {
        #[serde(default)]
        external_url: Option<String>,
    },
    ConversationDetected {
        #[serde(default)]
        conversation_id: Option<String>,
        #[serde(default)]
        external_conversation_ref: Option<String>,
    },
    UserMessageObserved {
        external_message_id: String,
        text: String,
    },
    AssistantMessageObserved {
        external_message_id: String,
        text: String,
        #[serde(default)]
        is_final: bool,
    },
    InjectionAccepted {
        injection_id: InjectionId,
    },
    InjectionMaterialized {
        injection_id: InjectionId,
        external_message_id: String,
    },
    PageUnavailable {
        reason: String,
    },
    Disconnected,
    // Backward-compatibility aliases
    AssistantMessage {
        message_id: String,
        text: String,
    },
    UserMessage {
        message_id: String,
        text: String,
    },
    SessionUnavailable {
        reason: String,
    },
}

impl BrowserEvent {
    pub fn validate(&self) -> Result<(), ProtocolError> {
        match self {
            BrowserEvent::AssistantMessage { text, .. }
            | BrowserEvent::UserMessage { text, .. }
            | BrowserEvent::AssistantMessageObserved { text, .. }
            | BrowserEvent::UserMessageObserved { text, .. }
                if text.len() > MAX_MESSAGE_TEXT_LENGTH =>
            {
                Err(ProtocolError::TextTooLarge {
                    actual: text.len(),
                    max: MAX_MESSAGE_TEXT_LENGTH,
                })
            }
            _ => Ok(()),
        }
    }
}

/// Commands sent from the bridge to the browser extension.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", content = "payload", rename_all = "snake_case")]
pub enum BrowserCommand {
    HelloAck {
        protocol_version: u16,
        session_id: SessionId,
    },
    InjectMessage {
        injection_id: InjectionId,
        correlation_id: CorrelationId,
        text: String,
    },
    SendMessage {
        text: String,
    },
    Ping,
    RequestPageState,
}

impl BrowserCommand {
    pub fn validate(&self) -> Result<(), ProtocolError> {
        match self {
            BrowserCommand::SendMessage { text } | BrowserCommand::InjectMessage { text, .. }
                if text.len() > MAX_MESSAGE_TEXT_LENGTH =>
            {
                Err(ProtocolError::TextTooLarge {
                    actual: text.len(),
                    max: MAX_MESSAGE_TEXT_LENGTH,
                })
            }
            _ => Ok(()),
        }
    }
}

/// Versioned envelope wrapping all messages transmitted over the wire.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MessageEnvelope<T> {
    pub version: ProtocolVersion,
    pub session_id: SessionId,
    pub correlation_id: CorrelationId,
    #[serde(flatten)]
    pub payload: T,
}

impl<T> MessageEnvelope<T> {
    pub fn new(session_id: SessionId, correlation_id: CorrelationId, payload: T) -> Self {
        Self {
            version: CURRENT_PROTOCOL_VERSION,
            session_id,
            correlation_id,
            payload,
        }
    }
}

impl<T: Serialize> MessageEnvelope<T> {
    pub fn to_json(&self) -> Result<String, ProtocolError> {
        let json =
            serde_json::to_string(self).map_err(|e| ProtocolError::Serialization(e.to_string()))?;
        if json.len() > MAX_PAYLOAD_BYTES {
            return Err(ProtocolError::PayloadTooLarge {
                actual: json.len(),
                max: MAX_PAYLOAD_BYTES,
            });
        }
        Ok(json)
    }
}

impl<'de, T: Deserialize<'de>> MessageEnvelope<T> {
    pub fn from_json_str(raw: &'de str) -> Result<Self, ProtocolError> {
        if raw.len() > MAX_PAYLOAD_BYTES {
            return Err(ProtocolError::PayloadTooLarge {
                actual: raw.len(),
                max: MAX_PAYLOAD_BYTES,
            });
        }
        let env: Self =
            serde_json::from_str(raw).map_err(|e| ProtocolError::Serialization(e.to_string()))?;
        if !env.version.is_supported() {
            return Err(ProtocolError::UnsupportedVersion(env.version.0));
        }
        Ok(env)
    }
}
