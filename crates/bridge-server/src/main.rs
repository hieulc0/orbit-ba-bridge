use bridge_core::SqliteConversationStore;
use bridge_server::paths;
use bridge_server::{
    BridgeServer, Cli, Commands, Config, ConversationAction, execute_interactive_command,
    handle_conversation_action,
};
use clap::Parser;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::sync::broadcast;
use tracing::info;
use tracing_subscriber::EnvFilter;

fn resolve_token(config: &Config) -> anyhow::Result<String> {
    if let Some(t) = &config.auth_token {
        return Ok(t.clone());
    }

    let token_path = match &config.token_file {
        Some(p) => PathBuf::from(p),
        None => paths::default_token_path(),
    };

    if token_path.exists()
        && let Ok(content) = std::fs::read_to_string(&token_path)
    {
        let trimmed = content.trim().to_string();
        if !trimmed.is_empty() {
            return Ok(trimmed);
        }
    }

    let token = uuid::Uuid::new_v4().to_string();
    if let Some(parent) = token_path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let _ = std::fs::write(&token_path, &token);

    // Also write to extension/token.json for seamless local browser extension loading
    let ext_token_path = Path::new("extension/token.json");
    let ext_json = serde_json::json!({
        "token": &token,
        "protocol_version": 1
    });
    let _ = std::fs::write(ext_token_path, ext_json.to_string());

    Ok(token)
}

fn open_store(db_path: &str) -> anyhow::Result<SqliteConversationStore> {
    if db_path == ":memory:" {
        SqliteConversationStore::open_in_memory()
    } else {
        if let Some(parent) = Path::new(db_path).parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        SqliteConversationStore::open(db_path)
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    // -------------------------------------------------------------
    // Branch 1: CLI Inspection / Export / Search Subcommands
    // -------------------------------------------------------------
    if let Some(Commands::Conversation(ref action)) = cli.command {
        let explicit_db = match action {
            ConversationAction::List { db_path, .. } => db_path.as_deref(),
            ConversationAction::Show { db_path, .. } => db_path.as_deref(),
            ConversationAction::Export { db_path, .. } => db_path.as_deref(),
            ConversationAction::Search { db_path, .. } => db_path.as_deref(),
        };

        let default_db_str = paths::default_db_path().to_string_lossy().to_string();
        let db_str = explicit_db
            .or(cli.server_config.db_path.as_deref())
            .unwrap_or(&default_db_str);

        let store = open_store(db_str)?;
        handle_conversation_action(action, &store).await?;
        return Ok(());
    }

    // -------------------------------------------------------------
    // Branch 2: Run Bridge Server Daemon
    // -------------------------------------------------------------
    let config = match cli.command {
        Some(Commands::Run(cfg)) => cfg,
        _ => cli.server_config,
    };

    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(&config.log_level)),
        )
        .init();

    let token = resolve_token(&config)?;

    let default_db_str = paths::default_db_path().to_string_lossy().to_string();
    let db_path = config.db_path.as_deref().unwrap_or(&default_db_str);
    let store = Arc::new(open_store(db_path)?);

    let masked_token = if token.len() > 8 {
        format!("{}...{}", &token[..4], &token[token.len() - 4..])
    } else {
        "***".to_string()
    };

    println!("\n==============================================================");
    println!("orbit-ba-bridge server");
    println!(
        "Listening on:           ws://{}:{}",
        config.host, config.port
    );
    println!("SQLite store:           {}", db_path);
    println!("Token status:           loaded ({})", masked_token);
    println!("Extension token sync:   extension/token.json");
    if !config.non_interactive {
        println!("Interactive prompt:     Type 'help' for commands, 'quit' to stop");
    }
    println!("==============================================================\n");

    let (shutdown_tx, shutdown_rx) = broadcast::channel(1);

    let bind_addr = format!("{}:{}", config.host, config.port);
    let server = BridgeServer::bind(&bind_addr, token, store, shutdown_rx).await?;

    let server_state = server.state();
    let shutdown_signal_tx = shutdown_tx.clone();

    // Spawn Ctrl+C listener
    tokio::spawn(async move {
        if let Ok(()) = tokio::signal::ctrl_c().await {
            info!("Received shutdown signal (Ctrl+C)");
            let _ = shutdown_signal_tx.send(());
        }
    });

    // Spawn interactive CLI prompt if enabled
    if !config.non_interactive {
        let cli_state = Arc::clone(&server_state);
        let cli_shutdown = shutdown_tx.clone();
        let host = config.host.clone();
        let port = config.port;

        tokio::spawn(async move {
            let mut lines = BufReader::new(tokio::io::stdin()).lines();
            while let Ok(Some(line)) = lines.next_line().await {
                match execute_interactive_command(&line, &cli_state, &host, port, &cli_shutdown)
                    .await
                {
                    Ok(should_exit) => {
                        if should_exit {
                            break;
                        }
                    }
                    Err(e) => {
                        eprintln!("Command error: {}", e);
                    }
                }
            }
        });
    }

    server.run().await?;
    info!("Bridge server shut down cleanly");
    Ok(())
}
