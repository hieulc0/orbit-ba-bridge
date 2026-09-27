//! Configuration and CLI argument parsing for `bridge-server`.

use clap::Parser;

#[derive(Parser, Debug, Clone)]
#[command(
    name = "bridge-server",
    about = "Localhost WebSocket bridge connecting ChatGPT browser sessions to Orbit"
)]
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

    /// Path to SQLite database file (defaults to target/bridge_data.sqlite; use ":memory:" for ephemeral)
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
