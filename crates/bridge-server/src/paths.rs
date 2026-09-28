//! Standard directory path resolution adhering to the XDG Base Directory Specification.

use std::path::PathBuf;

/// Returns the default directory for storing persistent application data.
///
/// Follows `$XDG_DATA_HOME/orbit-ba-bridge`, falling back to `~/.local/share/orbit-ba-bridge`.
pub fn default_data_dir() -> PathBuf {
    if let Ok(xdg_data) = std::env::var("XDG_DATA_HOME")
        && !xdg_data.trim().is_empty()
    {
        return PathBuf::from(xdg_data).join("orbit-ba-bridge");
    }

    if let Ok(home) = std::env::var("HOME")
        && !home.trim().is_empty()
    {
        return PathBuf::from(home)
            .join(".local")
            .join("share")
            .join("orbit-ba-bridge");
    }

    PathBuf::from(".data")
}

/// Returns the default directory for storing runtime state and tokens.
///
/// Follows `$XDG_STATE_HOME/orbit-ba-bridge`, falling back to `~/.local/state/orbit-ba-bridge`.
pub fn default_state_dir() -> PathBuf {
    if let Ok(xdg_state) = std::env::var("XDG_STATE_HOME")
        && !xdg_state.trim().is_empty()
    {
        return PathBuf::from(xdg_state).join("orbit-ba-bridge");
    }

    if let Ok(home) = std::env::var("HOME")
        && !home.trim().is_empty()
    {
        return PathBuf::from(home)
            .join(".local")
            .join("state")
            .join("orbit-ba-bridge");
    }

    PathBuf::from(".state")
}

/// Default filesystem path for the SQLite database.
pub fn default_db_path() -> PathBuf {
    default_data_dir().join("conversations.sqlite")
}

/// Default filesystem path for the local bridge authentication token.
pub fn default_token_path() -> PathBuf {
    default_state_dir().join("bridge-token")
}
