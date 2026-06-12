//! AI-client integration setup — detect and write the local config entries
//! that connect AI tools to the Strata MCP server and session-end hook.
//!
//! Everything here is read-modify-write on the user's own local config files,
//! triggered explicitly from the dashboard's Setup page. Rules:
//! - A config file that exists but doesn't parse as JSON is never touched.
//! - Writes go to a temp file in the same directory, then an atomic rename.
//! - Detection is read-only and never creates files.

use std::path::{Path, PathBuf};

/// Stable integration identifiers used by the frontend.
pub const CLAUDE_DESKTOP: &str = "claude_desktop";
pub const CURSOR: &str = "cursor";
pub const CLAUDE_CODE_HOOK: &str = "claude_code_hook";
pub const CLAUDE_CODE_MCP: &str = "claude_code_mcp";

/// Status of one integration target.
#[derive(Debug, Clone, serde::Serialize)]
pub struct IntegrationStatus {
    /// Stable id (see constants above).
    pub id: String,
    /// Human-readable name for the UI.
    pub name: String,
    /// Whether the client appears to be present on this machine.
    pub detected: bool,
    /// Whether Strata is already wired into its config.
    pub installed: bool,
    /// Whether the dashboard can install this automatically. When false, the
    /// UI shows `manual_command` for the user to run instead.
    pub auto_installable: bool,
    /// Copyable shell command for manual setup (Claude Code MCP scope).
    pub manual_command: Option<String>,
}

fn home() -> Result<PathBuf, String> {
    #[cfg(windows)]
    let var = std::env::var_os("USERPROFILE");
    #[cfg(not(windows))]
    let var = std::env::var_os("HOME");
    var.map(PathBuf::from)
        .ok_or_else(|| "could not resolve home directory".into())
}

/// Path to the `strata` MCP server binary: the sibling of the running app
/// executable (both binaries build into the same target directory).
pub fn server_binary_path() -> Result<PathBuf, String> {
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let dir = exe
        .parent()
        .ok_or_else(|| "executable has no parent directory".to_string())?;
    let bin = dir.join(if cfg!(windows) {
        "strata.exe"
    } else {
        "strata"
    });
    if bin.is_file() {
        Ok(bin)
    } else {
        Err(format!(
            "strata server binary not found at {} — build it with `cargo build --release`",
            bin.display()
        ))
    }
}

fn claude_desktop_config_path() -> Result<PathBuf, String> {
    let home = home()?;
    #[cfg(target_os = "macos")]
    let p = home.join("Library/Application Support/Claude/claude_desktop_config.json");
    #[cfg(target_os = "linux")]
    let p = home.join(".config/Claude/claude_desktop_config.json");
    #[cfg(target_os = "windows")]
    let p = std::env::var_os("APPDATA")
        .map(|a| PathBuf::from(a).join("Claude/claude_desktop_config.json"))
        .unwrap_or_else(|| home.join("AppData/Roaming/Claude/claude_desktop_config.json"));
    Ok(p)
}

fn cursor_config_path() -> Result<PathBuf, String> {
    Ok(home()?.join(".cursor/mcp.json"))
}

fn claude_code_settings_path() -> Result<PathBuf, String> {
    Ok(home()?.join(".claude/settings.json"))
}

fn claude_code_state_path() -> Result<PathBuf, String> {
    Ok(home()?.join(".claude.json"))
}

/// Read and parse a JSON config. `Ok(None)` = file doesn't exist.
/// An unparseable file is an error — never overwrite something we can't read.
fn read_json(path: &Path) -> Result<Option<serde_json::Value>, String> {
    if !path.is_file() {
        return Ok(None);
    }
    let text = std::fs::read_to_string(path)
        .map_err(|e| format!("could not read {}: {e}", path.display()))?;
    let value = serde_json::from_str(&text).map_err(|e| {
        format!(
            "{} is not valid JSON — not touching it: {e}",
            path.display()
        )
    })?;
    Ok(Some(value))
}

/// Write JSON atomically: temp file in the same directory, then rename.
fn write_json(path: &Path, value: &serde_json::Value) -> Result<(), String> {
    let parent = path
        .parent()
        .ok_or_else(|| format!("{} has no parent directory", path.display()))?;
    std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    let tmp = parent.join(format!(
        ".{}.strata-tmp",
        path.file_name().and_then(|n| n.to_str()).unwrap_or("cfg")
    ));
    let pretty = serde_json::to_string_pretty(value).map_err(|e| e.to_string())?;
    std::fs::write(&tmp, pretty).map_err(|e| e.to_string())?;
    std::fs::rename(&tmp, path).map_err(|e| e.to_string())?;
    Ok(())
}

/// Whether `config.mcpServers.strata` is present.
fn has_strata_mcp_entry(config: &serde_json::Value) -> bool {
    config
        .pointer("/mcpServers/strata")
        .is_some_and(|v| !v.is_null())
}

/// Add `mcpServers.strata = { command }` to an MCP-style JSON config.
fn install_mcp_entry(path: &Path, command: &Path) -> Result<(), String> {
    let mut config = read_json(path)?.unwrap_or_else(|| serde_json::json!({}));
    if !config.is_object() {
        return Err(format!("{} is not a JSON object", path.display()));
    }
    let servers = config
        .as_object_mut()
        .expect("checked is_object above")
        .entry("mcpServers")
        .or_insert_with(|| serde_json::json!({}));
    let Some(servers) = servers.as_object_mut() else {
        return Err(format!("mcpServers in {} is not an object", path.display()));
    };
    servers.insert(
        "strata".into(),
        serde_json::json!({ "command": command.to_string_lossy() }),
    );
    write_json(path, &config)
}

/// Whether any SessionEnd hook command in Claude Code settings invokes strata.
fn has_session_end_hook(settings: &serde_json::Value) -> bool {
    settings
        .pointer("/hooks/SessionEnd")
        .and_then(|v| v.as_array())
        .is_some_and(|entries| {
            entries.iter().any(|entry| {
                entry
                    .get("hooks")
                    .and_then(|h| h.as_array())
                    .is_some_and(|hooks| {
                        hooks.iter().any(|h| {
                            h.get("command")
                                .and_then(|c| c.as_str())
                                .is_some_and(|c| c.contains("strata") && c.contains("session-end"))
                        })
                    })
            })
        })
}

/// Append the Strata SessionEnd hook to `~/.claude/settings.json`.
fn install_session_end_hook(path: &Path, binary: &Path) -> Result<(), String> {
    let mut settings = read_json(path)?.unwrap_or_else(|| serde_json::json!({}));
    if !settings.is_object() {
        return Err(format!("{} is not a JSON object", path.display()));
    }
    if has_session_end_hook(&settings) {
        return Ok(());
    }
    let command = format!("\"{}\" hook session-end", binary.to_string_lossy());
    let hook_entry = serde_json::json!({
        "hooks": [ { "type": "command", "command": command } ]
    });

    let hooks = settings
        .as_object_mut()
        .expect("checked is_object above")
        .entry("hooks")
        .or_insert_with(|| serde_json::json!({}));
    let Some(hooks) = hooks.as_object_mut() else {
        return Err(format!("hooks in {} is not an object", path.display()));
    };
    let session_end = hooks
        .entry("SessionEnd")
        .or_insert_with(|| serde_json::json!([]));
    let Some(session_end) = session_end.as_array_mut() else {
        return Err(format!(
            "hooks.SessionEnd in {} is not an array",
            path.display()
        ));
    };
    session_end.push(hook_entry);
    write_json(path, &settings)
}

/// Report the status of all integrations (read-only).
pub fn status_all() -> Result<Vec<IntegrationStatus>, String> {
    let binary = server_binary_path().ok();
    let manual_command = binary.as_ref().map(|b| {
        format!(
            "claude mcp add --scope user strata -- \"{}\"",
            b.to_string_lossy()
        )
    });

    let mut out = Vec::new();

    {
        let path = claude_desktop_config_path()?;
        let detected = path.parent().is_some_and(Path::is_dir);
        let installed = matches!(read_json(&path), Ok(Some(ref c)) if has_strata_mcp_entry(c));
        out.push(IntegrationStatus {
            id: CLAUDE_DESKTOP.into(),
            name: "Claude Desktop".into(),
            detected,
            installed,
            auto_installable: true,
            manual_command: None,
        });
    }
    {
        let path = cursor_config_path()?;
        let detected = path.parent().is_some_and(Path::is_dir);
        let installed = matches!(read_json(&path), Ok(Some(ref c)) if has_strata_mcp_entry(c));
        out.push(IntegrationStatus {
            id: CURSOR.into(),
            name: "Cursor".into(),
            detected,
            installed,
            auto_installable: true,
            manual_command: None,
        });
    }
    {
        let path = claude_code_settings_path()?;
        let detected = path.parent().is_some_and(Path::is_dir);
        let installed = matches!(read_json(&path), Ok(Some(ref s)) if has_session_end_hook(s));
        out.push(IntegrationStatus {
            id: CLAUDE_CODE_HOOK.into(),
            name: "Claude Code — session capture hook".into(),
            detected,
            installed,
            auto_installable: true,
            manual_command: None,
        });
    }
    {
        // ~/.claude.json is the CLI's own state file and is rewritten by the
        // CLI while it runs — Strata only ever reads it. Setup is manual via
        // `claude mcp add`, shown as a copyable command.
        let path = claude_code_state_path()?;
        let detected = path.is_file();
        let installed = matches!(read_json(&path), Ok(Some(ref c)) if has_strata_mcp_entry(c));
        out.push(IntegrationStatus {
            id: CLAUDE_CODE_MCP.into(),
            name: "Claude Code — MCP server".into(),
            detected,
            installed,
            auto_installable: false,
            manual_command,
        });
    }

    Ok(out)
}

/// Install one integration by id. Returns the refreshed status list.
pub fn install(id: &str) -> Result<Vec<IntegrationStatus>, String> {
    let binary = server_binary_path()?;
    match id {
        CLAUDE_DESKTOP => {
            let path = claude_desktop_config_path()?;
            if !path.parent().is_some_and(Path::is_dir) {
                return Err("Claude Desktop does not appear to be installed".into());
            }
            install_mcp_entry(&path, &binary)?;
        }
        CURSOR => {
            install_mcp_entry(&cursor_config_path()?, &binary)?;
        }
        CLAUDE_CODE_HOOK => {
            let path = claude_code_settings_path()?;
            if !path.parent().is_some_and(Path::is_dir) {
                return Err("Claude Code does not appear to be installed".into());
            }
            install_session_end_hook(&path, &binary)?;
        }
        CLAUDE_CODE_MCP => {
            return Err("Claude Code MCP setup is manual — run the shown command".into());
        }
        other => return Err(format!("unknown integration: {other}")),
    }
    status_all()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn install_mcp_entry_creates_and_preserves() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.json");
        std::fs::write(
            &path,
            r#"{"existing": {"keep": true}, "mcpServers": {"other": {"command": "x"}}}"#,
        )
        .unwrap();

        install_mcp_entry(&path, Path::new("/usr/local/bin/strata")).unwrap();

        let config = read_json(&path).unwrap().unwrap();
        assert!(has_strata_mcp_entry(&config));
        assert_eq!(
            config.pointer("/existing/keep"),
            Some(&serde_json::json!(true))
        );
        assert_eq!(
            config.pointer("/mcpServers/other/command"),
            Some(&serde_json::json!("x"))
        );
    }

    #[test]
    fn install_mcp_entry_refuses_invalid_json() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.json");
        std::fs::write(&path, "{ definitely not json").unwrap();
        let err = install_mcp_entry(&path, Path::new("/bin/strata")).unwrap_err();
        assert!(err.contains("not touching it"));
        // Original content untouched.
        assert_eq!(
            std::fs::read_to_string(&path).unwrap(),
            "{ definitely not json"
        );
    }

    #[test]
    fn session_end_hook_appended_once_and_preserves_existing() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.json");
        std::fs::write(
            &path,
            r#"{"hooks": {"SessionEnd": [{"hooks": [{"type": "command", "command": "echo done"}]}]}, "model": "opus"}"#,
        )
        .unwrap();

        install_session_end_hook(&path, Path::new("/bin/strata")).unwrap();
        install_session_end_hook(&path, Path::new("/bin/strata")).unwrap(); // idempotent

        let settings = read_json(&path).unwrap().unwrap();
        assert!(has_session_end_hook(&settings));
        let entries = settings
            .pointer("/hooks/SessionEnd")
            .unwrap()
            .as_array()
            .unwrap();
        assert_eq!(entries.len(), 2, "existing hook kept, strata added once");
        assert_eq!(settings.get("model"), Some(&serde_json::json!("opus")));
    }

    #[test]
    fn missing_files_report_not_installed() {
        let config = read_json(Path::new("/nonexistent/strata-test.json")).unwrap();
        assert!(config.is_none());
    }
}
