//! Formatting and execution logic for CLI conversation inspection, search, and export commands.

use bridge_core::{ActorRole, ConversationStore, ParticipantSource, export_json, export_markdown};
use protocol::{BridgeErrorCode, ConversationId};
use std::sync::Arc;
use tokio::sync::broadcast;

use crate::config::{ConversationAction, ExportFormat};
use crate::server::ServerState;

/// Formats the current runtime status without exposing secret tokens.
pub async fn format_status(state: &ServerState, host: &str, port: u16) -> anyhow::Result<String> {
    let sess_state = *state.active_session_state.lock().await;
    let attached_session = state.attached_session_id.lock().await.clone();
    let conv_opt = state.active_conversation.lock().await.clone();

    let mut out = String::new();
    out.push_str("\n--- Bridge Runtime Status ---\n");
    out.push_str(&format!(
        "Bridge\n  listening:    ws://{}:{}\n  state:        running\n",
        host, port
    ));

    let connected = attached_session.is_some();
    let page_ready = matches!(
        sess_state,
        bridge_core::SessionState::PageReady | bridge_core::SessionState::ConversationReady
    );
    let attached_display = attached_session
        .as_ref()
        .map(|s| s.as_str())
        .unwrap_or("none");
    let ref_display = conv_opt
        .as_ref()
        .and_then(|c| c.external_conversation_ref.as_deref())
        .unwrap_or("none");

    out.push_str(&format!(
        "Browser\n  connected:    {}\n  page ready:   {}\n  attached tab: {}\n  ChatGPT ref:  {}\n",
        connected, page_ready, attached_display, ref_display
    ));

    if let Some(c) = &conv_opt {
        let msgs = state.store.load_messages(&c.id, None, 10_000).await?;
        let last_seq = msgs.last().map(|m| m.sequence).unwrap_or(0);
        out.push_str(&format!(
            "Conversation\n  ID:           {}\n  status:       {:?}\n  messages:     {}\n  last seq:     {}\n",
            c.id, c.status, msgs.len(), last_seq
        ));

        let participants = state.store.list_participants(&c.id).await?;
        out.push_str("Participants\n");
        for p in participants {
            out.push_str(&format!(
                "  {} (role: {}, source: {})\n",
                p.display_name,
                p.role.as_str(),
                p.source.as_str()
            ));
        }
    } else {
        out.push_str("Conversation\n  active:       none (awaiting ChatGPT page attachment)\n");
    }

    out.push_str("-----------------------------\n");
    Ok(out)
}

/// Executes a conversation action against a given conversation store.
pub async fn handle_conversation_action(
    action: &ConversationAction,
    store: &dyn ConversationStore,
) -> anyhow::Result<()> {
    match action {
        ConversationAction::List { limit, offset, .. } => {
            let summaries = store.list_conversations(*limit, *offset).await?;
            if summaries.is_empty() {
                println!("No conversations recorded yet.");
                return Ok(());
            }

            println!(
                "\n{:<22} {:<24} {:<10} {:<10} UPDATED (UTC)",
                "ID", "EXTERNAL REF", "STATUS", "MESSAGES"
            );
            println!("{}", "-".repeat(84));
            for s in summaries {
                let ext_ref = s.external_conversation_ref.as_deref().unwrap_or("-");
                let updated_str = s.updated_at.format("%Y-%m-%d %H:%M:%S").to_string();
                println!(
                    "{:<22} {:<24} {:<10} {:<10} {}",
                    s.id.as_str(),
                    ext_ref,
                    s.status.as_str(),
                    s.message_count,
                    updated_str
                );
            }
            println!();
        }
        ConversationAction::Show { id, full, .. } => {
            let conv_id = ConversationId::new(id.trim())?;
            let result = store.get_conversation_with_messages(&conv_id).await?;
            match result {
                None => {
                    eprintln!("Error: {} ({})", BridgeErrorCode::ConversationNotFound, id);
                }
                Some((conv, parts, msgs)) => {
                    println!("\nConversation: {}", conv.id);
                    println!(
                        "External ref: {}",
                        conv.external_conversation_ref.as_deref().unwrap_or("none")
                    );
                    println!("Status:       {}", conv.status.as_str());
                    println!("Participants: {}", parts.len());
                    println!("Messages:     {}", msgs.len());

                    let part_map: std::collections::HashMap<_, _> = parts
                        .iter()
                        .map(|p| (&p.id, p.display_name.as_str()))
                        .collect();

                    println!("\n{:<4} {:<14} {:<14} CONTENT", "SEQ", "ACTOR", "KIND");
                    println!("{}", "-".repeat(70));
                    for m in msgs {
                        let actor_name = part_map.get(&m.actor_id).copied().unwrap_or("Unknown");
                        if *full {
                            println!(
                                "{:<4} {:<14} {:<14}\n{}\n",
                                m.sequence,
                                actor_name,
                                m.kind.as_str(),
                                m.content
                            );
                        } else {
                            let first_line = m.content.lines().next().unwrap_or("");
                            let snippet = if first_line.chars().count() > 60 {
                                let s: String = first_line.chars().take(57).collect();
                                format!("{}...", s)
                            } else {
                                first_line.to_string()
                            };
                            println!(
                                "{:<4} {:<14} {:<14} {}",
                                m.sequence,
                                actor_name,
                                m.kind.as_str(),
                                snippet
                            );
                        }
                    }
                    println!();
                }
            }
        }
        ConversationAction::Export {
            id, format, output, ..
        } => {
            let conv_id = ConversationId::new(id.trim())?;
            let result = store.get_conversation_with_messages(&conv_id).await?;
            match result {
                None => {
                    eprintln!("Error: {} ({})", BridgeErrorCode::ConversationNotFound, id);
                }
                Some((conv, parts, msgs)) => {
                    let exported = match format {
                        ExportFormat::Markdown => export_markdown(&conv, &parts, &msgs),
                        ExportFormat::Json => export_json(&conv, &parts, &msgs)?,
                    };

                    if let Some(out_path) = output {
                        if let Some(parent) = std::path::Path::new(out_path).parent() {
                            let _ = std::fs::create_dir_all(parent);
                        }
                        std::fs::write(out_path, &exported)?;
                        println!("Conversation exported to {}", out_path);
                    } else {
                        println!("{}", exported);
                    }
                }
            }
        }
        ConversationAction::Search { query, limit, .. } => {
            let results = store.search_messages(query, *limit).await?;
            if results.is_empty() {
                println!("No messages matched query: '{}'", query);
                return Ok(());
            }

            println!(
                "\n{:<22} {:<4} {:<14} MATCH SNIPPET",
                "CONVERSATION", "SEQ", "ACTOR"
            );
            println!("{}", "-".repeat(70));
            for r in results {
                println!(
                    "{:<22} {:<4} {:<14} {}",
                    r.conversation_id.as_str(),
                    r.sequence,
                    r.actor_display_name,
                    r.content_snippet
                );
            }
            println!();
        }
    }
    Ok(())
}

/// Executes an interactive command entered via the stdin CLI prompt.
pub async fn execute_interactive_command(
    line: &str,
    state: &Arc<ServerState>,
    host: &str,
    port: u16,
    shutdown_tx: &broadcast::Sender<()>,
) -> anyhow::Result<bool> {
    let trimmed = line.trim();
    if trimmed.is_empty() {
        return Ok(false);
    }

    if trimmed == "quit" || trimmed == "exit" {
        println!("Shutting down bridge server...");
        let _ = shutdown_tx.send(());
        return Ok(true);
    }

    if trimmed == "help" {
        println!("\nAvailable commands:");
        println!(
            "  status                                          - Show bridge & browser runtime status"
        );
        println!("  conversation list (or conv list)                - List recorded conversations");
        println!(
            "  conversation show <id> [--full]                 - Show messages in conversation"
        );
        println!(
            "  messages                                        - Shorthand: show messages in active conversation"
        );
        println!(
            "  conversation export <id> --format <md|json>     - Export conversation to Markdown or JSON"
        );
        println!(
            "  conversation search <query>                     - Search messages across conversations"
        );
        println!("  inject sa <message>                             - Inject message as Orbit SA");
        println!(
            "  inject ba_research <message>                    - Inject message as BA Research"
        );
        println!("  quit | exit                                     - Stop server\n");
        return Ok(false);
    }

    if trimmed == "status" {
        let status_str = format_status(state, host, port).await?;
        println!("{}", status_str);
        return Ok(false);
    }

    if trimmed == "messages" {
        let active = state.active_conversation.lock().await.clone();
        if let Some(c) = active {
            let action = ConversationAction::Show {
                id: c.id.to_string(),
                full: false,
                db_path: None,
            };
            handle_conversation_action(&action, state.store.as_ref()).await?;
        } else {
            println!(
                "No active conversation. Type 'conversation list' to view existing conversations."
            );
        }
        return Ok(false);
    }

    if trimmed == "conversation list" || trimmed == "conv list" {
        let action = ConversationAction::List {
            limit: 50,
            offset: 0,
            db_path: None,
        };
        handle_conversation_action(&action, state.store.as_ref()).await?;
        return Ok(false);
    }

    if trimmed.starts_with("conversation show ") || trimmed.starts_with("conv show ") {
        let remainder = trimmed
            .strip_prefix("conversation show ")
            .or_else(|| trimmed.strip_prefix("conv show "))
            .unwrap_or("")
            .trim();

        let full = remainder.ends_with("--full");
        let id = remainder.trim_end_matches("--full").trim();

        let action = ConversationAction::Show {
            id: id.to_string(),
            full,
            db_path: None,
        };
        handle_conversation_action(&action, state.store.as_ref()).await?;
        return Ok(false);
    }

    if trimmed.starts_with("conversation export ") || trimmed.starts_with("conv export ") {
        let remainder = trimmed
            .strip_prefix("conversation export ")
            .or_else(|| trimmed.strip_prefix("conv export "))
            .unwrap_or("")
            .trim();

        let parts: Vec<&str> = remainder.split_whitespace().collect();
        if parts.is_empty() {
            println!("Usage: conversation export <id> [--format markdown|json] [--output <file>]");
            return Ok(false);
        }

        let id = parts[0];
        let mut format = ExportFormat::Markdown;
        let mut output = None;

        let mut i = 1;
        while i < parts.len() {
            match parts[i] {
                "--format" if i + 1 < parts.len() => {
                    match parts[i + 1] {
                        "json" => format = ExportFormat::Json,
                        "md" | "markdown" => format = ExportFormat::Markdown,
                        other => println!("Unknown format '{}'; defaulting to markdown", other),
                    }
                    i += 2;
                }
                "--output" if i + 1 < parts.len() => {
                    output = Some(parts[i + 1].to_string());
                    i += 2;
                }
                _ => i += 1,
            }
        }

        let action = ConversationAction::Export {
            id: id.to_string(),
            format,
            output,
            db_path: None,
        };
        handle_conversation_action(&action, state.store.as_ref()).await?;
        return Ok(false);
    }

    if trimmed.starts_with("conversation search ") || trimmed.starts_with("conv search ") {
        let query = trimmed
            .strip_prefix("conversation search ")
            .or_else(|| trimmed.strip_prefix("conv search "))
            .unwrap_or("")
            .trim();

        let action = ConversationAction::Search {
            query: query.to_string(),
            limit: 20,
            db_path: None,
        };
        handle_conversation_action(&action, state.store.as_ref()).await?;
        return Ok(false);
    }

    if trimmed.starts_with("inject ") {
        let parts: Vec<&str> = trimmed.splitn(3, ' ').collect();
        if parts.len() < 3 {
            println!("Usage: inject <sa|ba_research> <message>");
            return Ok(false);
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
                return Ok(false);
            }
        };

        match state.inject(role, name, source, text).await {
            Ok(inj_id) => {
                println!("Injected message as '{}' (InjectionId: {})", name, inj_id);
            }
            Err(e) => {
                println!("Failed to inject message: {}", e);
            }
        }
        return Ok(false);
    }

    println!(
        "Unknown command '{}'. Type 'help' for available commands.",
        trimmed
    );
    Ok(false)
}
