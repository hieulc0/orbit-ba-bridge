//! Browser driver abstraction and mock implementations.

use async_trait::async_trait;
use protocol::{BrowserCommand, BrowserEvent};
use tokio::sync::mpsc;

/// Trait abstracting communication with the browser transport layer.
#[async_trait]
pub trait BrowserDriver: Send {
    /// Receive the next normalized event from the browser.
    async fn recv_event(&mut self) -> anyhow::Result<BrowserEvent>;

    /// Send a command to the browser.
    async fn send_command(&mut self, command: BrowserCommand) -> anyhow::Result<()>;
}

/// A mock implementation of `BrowserDriver` for testing without a real browser.
pub struct MockBrowserDriver {
    event_rx: mpsc::Receiver<BrowserEvent>,
    command_tx: mpsc::Sender<BrowserCommand>,
}

impl MockBrowserDriver {
    /// Creates a new `MockBrowserDriver` and returns its external control channels:
    /// - `mpsc::Sender<BrowserEvent>` to feed events into the driver.
    /// - `mpsc::Receiver<BrowserCommand>` to read commands emitted by the driver.
    pub fn pair(
        buffer_size: usize,
    ) -> (
        Self,
        mpsc::Sender<BrowserEvent>,
        mpsc::Receiver<BrowserCommand>,
    ) {
        let (event_tx, event_rx) = mpsc::channel(buffer_size);
        let (command_tx, command_rx) = mpsc::channel(buffer_size);
        (
            Self {
                event_rx,
                command_tx,
            },
            event_tx,
            command_rx,
        )
    }
}

#[async_trait]
impl BrowserDriver for MockBrowserDriver {
    async fn recv_event(&mut self) -> anyhow::Result<BrowserEvent> {
        self.event_rx
            .recv()
            .await
            .ok_or_else(|| anyhow::anyhow!("mock event channel closed"))
    }

    async fn send_command(&mut self, command: BrowserCommand) -> anyhow::Result<()> {
        command.validate()?;
        self.command_tx
            .send(command)
            .await
            .map_err(|e| anyhow::anyhow!("mock command channel closed: {e}"))
    }
}
