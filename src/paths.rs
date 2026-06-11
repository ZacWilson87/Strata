//! Platform data-directory resolution shared by the two Strata binaries.
//!
//! The MCP server (`src/main.rs`) and the Tauri desktop app (`src-tauri/`)
//! open the same SQLite file. Resolving the path in one place guarantees the
//! two processes can never diverge onto split databases.

/// Return the platform-appropriate data directory path.
pub fn data_dir() -> Option<String> {
    #[cfg(target_os = "macos")]
    {
        std::env::var("HOME")
            .ok()
            .map(|h| format!("{h}/Library/Application Support/Strata"))
    }
    #[cfg(target_os = "linux")]
    {
        std::env::var("XDG_DATA_HOME")
            .ok()
            .or_else(|| {
                std::env::var("HOME")
                    .ok()
                    .map(|h| format!("{h}/.local/share"))
            })
            .map(|base| format!("{base}/strata"))
    }
    #[cfg(target_os = "windows")]
    {
        std::env::var("APPDATA")
            .ok()
            .map(|d| format!("{d}\\Strata"))
    }
    #[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
    {
        None
    }
}

/// Restrict the data directory to owner-only access (Unix). Best-effort:
/// a permissions failure must not prevent the app from starting.
#[cfg(unix)]
pub fn restrict_dir_permissions(dir: &str) {
    use std::os::unix::fs::PermissionsExt;
    if let Err(e) = std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o700)) {
        tracing::warn!("could not restrict permissions on {dir}: {e}");
    }
}

#[cfg(not(unix))]
pub fn restrict_dir_permissions(_dir: &str) {}

/// Resolve the data directory, create it, restrict its permissions, and
/// return the database file path inside it.
pub fn prepare_db_path() -> std::io::Result<String> {
    let dir = data_dir().unwrap_or_else(|| ".".into());
    std::fs::create_dir_all(&dir)?;
    restrict_dir_permissions(&dir);
    Ok(format!("{dir}/strata.db"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn data_dir_resolves_on_supported_platforms() {
        // HOME / APPDATA are set in any normal environment.
        assert!(data_dir().is_some());
    }

    #[test]
    fn data_dir_ends_with_app_directory() {
        let dir = data_dir().unwrap();
        assert!(
            dir.ends_with("Strata") || dir.ends_with("strata"),
            "unexpected data dir: {dir}"
        );
    }
}
