//! Application core logic for `orbit-ba-bridge`.
//!
//! Provides driver abstractions, session management, and message routing.

pub mod driver;
pub mod router;
pub mod session;

pub use driver::{BrowserDriver, MockBrowserDriver};
pub use router::BridgeRouter;
pub use session::{DEFAULT_MAX_SEEN_MESSAGES, SessionManager, SessionState};
