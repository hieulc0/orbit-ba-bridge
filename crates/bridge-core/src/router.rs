//! Message routing and dispatching across driver boundaries.

use crate::driver::BrowserDriver;
use crate::session::SessionManager;
use protocol::{BrowserCommand, BrowserEvent, CorrelationId, MessageEnvelope};
use tracing::{info, warn};

/// Orchestrates message dispatching, event processing, and session updates.
pub struct BridgeRouter<D: BrowserDriver> {
    driver: D,
    session: SessionManager,
}

impl<D: BrowserDriver> BridgeRouter<D> {
    pub fn new(driver: D, session: SessionManager) -> Self {
        Self { driver, session }
    }

    pub fn session(&self) -> &SessionManager {
        &self.session
    }

    pub fn session_mut(&mut self) -> &mut SessionManager {
        &mut self.session
    }

    /// Dispatch a command to the browser driver.
    pub async fn dispatch_command(&mut self, command: BrowserCommand) -> anyhow::Result<()> {
        info!(command = ?command, "Dispatching command to browser driver");
        self.driver.send_command(command).await
    }

    /// Receive and process the next event from the browser driver.
    /// Returns `None` if the event is a duplicate message, or `Some(event)` if it was accepted.
    pub async fn process_next_event(&mut self) -> anyhow::Result<Option<BrowserEvent>> {
        let event = self.driver.recv_event().await?;
        event.validate()?;

        let accepted = self.session.handle_event(&event);
        if !accepted {
            warn!(event = ?event, "Suppressed duplicate browser event");
            return Ok(None);
        }

        info!(event = ?event, "Processed browser event");
        Ok(Some(event))
    }

    /// Wrap a command in a versioned envelope for the current session.
    pub fn create_command_envelope(
        &self,
        correlation_id: CorrelationId,
        command: BrowserCommand,
    ) -> MessageEnvelope<BrowserCommand> {
        MessageEnvelope::new(self.session.session_id().clone(), correlation_id, command)
    }
}
