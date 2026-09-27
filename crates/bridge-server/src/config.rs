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
}

impl Default for Config {
    fn default() -> Self {
        Self {
            host: "127.0.0.1".to_string(),
            port: 48117,
            log_level: "info".to_string(),
        }
    }
}
