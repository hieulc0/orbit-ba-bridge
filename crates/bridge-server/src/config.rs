//! Configuration and CLI argument parsing for `bridge-server`.

use clap::{Parser, Subcommand, ValueEnum};

#[derive(Parser, Debug, Clone)]
#[command(
    name = "bridge-server",
    about = "Localhost WebSocket bridge connecting ChatGPT browser sessions to Orbit"
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Option<Commands>,

    #[command(flatten)]
    pub server_config: Config,
}

#[derive(Subcommand, Debug, Clone)]
pub enum Commands {
    /// Run the bridge server daemon (default action)
    Run(Config),
    /// Conversation inspection, export, and search operations
    #[command(subcommand)]
    Conversation(ConversationAction),
}

#[derive(Subcommand, Debug, Clone)]
pub enum ConversationAction {
    /// List recorded conversations
    List {
        #[arg(long, default_value_t = 50)]
        limit: usize,
        #[arg(long, default_value_t = 0)]
        offset: usize,
        #[arg(long)]
        db_path: Option<String>,
    },
    /// Show details and messages for a conversation
    Show {
        id: String,
        #[arg(long, default_value_t = false)]
        full: bool,
        #[arg(long)]
        db_path: Option<String>,
    },
    /// Export a conversation to Markdown or JSON
    Export {
        id: String,
        #[arg(long, value_enum, default_value_t = ExportFormat::Markdown)]
        format: ExportFormat,
        #[arg(long)]
        output: Option<String>,
        #[arg(long)]
        db_path: Option<String>,
    },
    /// Search messages across conversations
    Search {
        query: String,
        #[arg(long, default_value_t = 20)]
        limit: usize,
        #[arg(long)]
        db_path: Option<String>,
    },
}

#[derive(ValueEnum, Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExportFormat {
    Markdown,
    Json,
}

#[derive(Parser, Debug, Clone)]
pub struct Config {
    /// Host interface to bind (defaults to 127.0.0.1; non-localhost is discouraged for security)
    #[arg(long, default_value = "127.0.0.1")]
    pub host: String,

    /// Port to listen on for extension WebSocket connections
    #[arg(short, long, default_value_t = 48117)]
    pub port: u16,

    /// Log filter directive (e.g. info, debug, trace)
    #[arg(long, default_value = "info")]
    pub log_level: String,

    /// Explicit authentication token for extension handshake (if not provided, loaded from token-file or generated)
    #[arg(long)]
    pub auth_token: Option<String>,

    /// Path to file storing the bridge auth token
    #[arg(long)]
    pub token_file: Option<String>,

    /// Path to SQLite database file (defaults to XDG data dir; use ":memory:" for ephemeral)
    #[arg(long)]
    pub db_path: Option<String>,

    /// Run in non-interactive headless mode (disables stdin command prompt)
    #[arg(long, default_value_t = false)]
    pub non_interactive: bool,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            host: "127.0.0.1".to_string(),
            port: 48117,
            log_level: "info".to_string(),
            auth_token: None,
            token_file: None,
            db_path: None,
            non_interactive: false,
        }
    }
}
