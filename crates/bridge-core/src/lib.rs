//! Application core logic for `orbit-ba-bridge`.
//!
//! Provides driver abstractions, session management, multi-actor conversations,
//! injection tracking, and SQLite persistence.

pub mod artifact;
pub mod context;
pub mod conversation;
pub mod driver;
pub mod export;
pub mod injection;
pub mod message;
pub mod participant;
pub mod router;
pub mod session;
pub mod sqlite_store;
pub mod store;

pub use artifact::{ArtifactRef, ArtifactType};
pub use context::{ContextProjection, ProjectedMessage};
pub use conversation::{Conversation, ConversationStatus, ConversationSummary, NewConversation};
pub use driver::{BrowserDriver, MockBrowserDriver};
pub use export::{ConversationExportData, OwnedConversationExport, export_json, export_markdown};
pub use injection::{
    InjectionLedger, MaterializedInjection, PendingInjection, format_external_injection,
};
pub use message::{ConversationMessage, MessageKind, MessageSearchResult, NewMessage};
pub use participant::{ActorRole, NewParticipant, Participant, ParticipantSource};
pub use router::BridgeRouter;
pub use session::{DEFAULT_MAX_SEEN_MESSAGES, SessionManager, SessionState};
pub use sqlite_store::SqliteConversationStore;
pub use store::ConversationStore;
