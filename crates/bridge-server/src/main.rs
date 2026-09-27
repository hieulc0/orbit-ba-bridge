use bridge_core::{ActorRole, ParticipantSource, SqliteConversationStore};
use bridge_server::{BridgeServer, Config};
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
        None => PathBuf::from("target/bridge_token"),
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

    // Also write to extension/token.json for seamless Chrome development loading
    let ext_token_path = Path::new("extension/token.json");
    let ext_json = serde_json::json!({
        "token": &token,
        "protocol_version": 1
    });
    let _ = std::fs::write(ext_token_path, ext_json.to_string());

    Ok(token)
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let config = Config::parse();

    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(&config.log_level)),
        )
        .init();

    let token = resolve_token(&config)?;

    let db_path = config
        .db_path
        .clone()
        .unwrap_or_else(|| "target/bridge_data.sqlite".to_string());

    let store = if db_path == ":memory:" {
        Arc::new(SqliteConversationStore::open_in_memory()?)
    } else {
        if let Some(parent) = Path::new(&db_path).parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        Arc::new(SqliteConversationStore::open(&db_path)?)
    };

    println!("\n==============================================================");
    println!("orbit-ba-bridge server");
    println!("Listening on: ws://{}:{}", config.host, config.port);
    println!("Local bridge token: {}", token);
    println!("SQLite store: {}", db_path);
    println!("Extension token synced: extension/token.json");
    if !config.non_interactive {
        println!("Type 'help' for interactive injection commands");
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

    // Spawn interactive CLI if enabled
    if !config.non_interactive {
        let cli_state = Arc::clone(&server_state);
        let cli_shutdown = shutdown_tx.clone();

        tokio::spawn(async move {
            let mut lines = BufReader::new(tokio::io::stdin()).lines();
            while let Ok(Some(line)) = lines.next_line().await {
                let trimmed = line.trim();
                if trimmed.is_empty() {
                    continue;
                }

                if trimmed == "quit" || trimmed == "exit" {
                    println!("Exiting...");
                    let _ = cli_shutdown.send(());
                    break;
                } else if trimmed == "help" {
                    println!("\nAvailable commands:");
                    println!(
                        "  status                        - Show current connection & session status"
                    );
                    println!(
                        "  inject sa <message>           - Inject message as Orbit SA (SystemArchitect)"
                    );
                    println!(
                        "  inject ba_research <message>  - Inject message as BA Research (BusinessAnalyst)"
                    );
                    println!(
                        "  messages                      - Display persisted conversation messages"
                    );
                    println!("  quit | exit                   - Stop server\n");
                } else if trimmed == "status" {
                    let sess_state = *cli_state.active_session_state.lock().await;
                    let conv_opt = cli_state.active_conversation.lock().await;
                    println!("\n--- Runtime Status ---");
                    println!("Browser connection state: {:?}", sess_state);
                    if let Some(c) = &*conv_opt {
                        println!("Active conversation ID: {}", c.id);
                        println!("External ref: {:?}", c.external_conversation_ref);
                    } else {
                        println!("Active conversation: None (waiting for page)");
                    }
                    println!("----------------------\n");
                } else if trimmed == "messages" {
                    match cli_state.list_messages().await {
                        Ok(msgs) => {
                            println!("\n--- Persisted Messages ({}) ---", msgs.len());
                            for m in msgs {
                                println!(
                                    "[{:>2}] actor={} kind={:?} | {}",
                                    m.sequence,
                                    m.actor_id,
                                    m.kind,
                                    m.content.lines().next().unwrap_or("")
                                );
                            }
                            println!("-------------------------------\n");
                        }
                        Err(e) => println!("Error listing messages: {}", e),
                    }
                } else if trimmed.starts_with("inject ") {
                    let parts: Vec<&str> = trimmed.splitn(3, ' ').collect();
                    if parts.len() < 3 {
                        println!("Usage: inject <sa|ba_research> <message>");
                        continue;
                    }
                    let target_role = parts[1];
                    let text = parts[2];

                    let (role, name, source) = match target_role {
                        "sa" | "orbit_sa" => (
                            ActorRole::SystemArchitect,
                            "Orbit SA",
                            ParticipantSource::Orbit,
                        ),
                        "ba_research" | "research" => (
                            ActorRole::BusinessAnalyst,
                            "BA Research",
                            ParticipantSource::ChatGptWeb,
                        ),
                        other => {
                            println!("Unknown role '{}'. Use 'sa' or 'ba_research'.", other);
                            continue;
                        }
                    };

                    match cli_state.inject(role, name, source, text).await {
                        Ok(inj_id) => {
                            println!("Injected message as '{}' (InjectionId: {})", name, inj_id);
                        }
                        Err(e) => {
                            println!("Failed to inject message: {}", e);
                        }
                    }
                } else {
                    println!("Unknown command '{}'. Type 'help' for commands.", trimmed);
                }
            }
        });
    }

    server.run().await?;
    info!("Bridge server shut down cleanly");
    Ok(())
}
