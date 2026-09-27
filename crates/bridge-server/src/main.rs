use bridge_server::{BridgeServer, Config};
use clap::Parser;
use tokio::sync::broadcast;
use tracing::info;
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let config = Config::parse();

    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(&config.log_level)),
        )
        .init();

    info!(
        host = %config.host,
        port = %config.port,
        "Starting orbit-ba-bridge server"
    );

    let (shutdown_tx, shutdown_rx) = broadcast::channel(1);

    let bind_addr = format!("{}:{}", config.host, config.port);
    let server = BridgeServer::bind(&bind_addr, shutdown_rx).await?;

    tokio::spawn(async move {
        if let Ok(()) = tokio::signal::ctrl_c().await {
            info!("Received shutdown signal (Ctrl+C)");
            let _ = shutdown_tx.send(());
        }
    });

    server.run().await?;
    info!("Bridge server shut down cleanly");
    Ok(())
}
