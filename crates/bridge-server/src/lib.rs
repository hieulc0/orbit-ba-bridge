//! Server runtime for `orbit-ba-bridge`.
//!
//! Provides the localhost WebSocket listener connecting browser extensions to the bridge core.

pub mod commands;
pub mod config;
pub mod paths;
pub mod server;

pub use commands::{execute_interactive_command, format_status, handle_conversation_action};
pub use config::{Cli, Commands, Config, ConversationAction, ExportFormat};
pub use server::BridgeServer;
