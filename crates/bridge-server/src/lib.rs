//! Server runtime for `orbit-ba-bridge`.
//!
//! Provides the localhost WebSocket listener connecting browser extensions to the bridge core.

pub mod config;
pub mod server;

pub use config::Config;
pub use server::BridgeServer;
