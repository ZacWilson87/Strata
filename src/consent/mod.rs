/// Consent management and audit logging.
///
/// All graph reads and writes must call `ConsentGate::check()` before proceeding.
/// Revocation triggers immediate data deletion.
use chrono::Utc;
use rusqlite::{params, Connection};
use std::sync::{Arc, Mutex};

use crate::graph::GraphHandle;

/// Error type for consent operations.
#[derive(Debug, thiserror::Error)]
pub enum ConsentError {
    #[error("data collection is paused — resume in settings to continue")]
    Paused,

    #[error("consent has been revoked — no data will be collected or read")]
    Revoked,

    #[error("database error: {0}")]
    Db(#[from] rusqlite::Error),

    #[error("graph error: {0}")]
    Graph(#[from] crate::graph::queries::GraphError),

    #[error("lock poisoned")]
    LockPoisoned,
}

/// The consent lifecycle.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum ConsentStatus {
    Granted,
    Paused,
    Revoked,
}

impl std::fmt::Display for ConsentStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ConsentStatus::Granted => write!(f, "granted"),
            ConsentStatus::Paused => write!(f, "paused"),
            ConsentStatus::Revoked => write!(f, "revoked"),
        }
    }
}

/// An auditable event recorded in the audit log.
#[derive(Debug, Clone)]
pub enum AuditEvent {
    ConsentGranted,
    ConsentPaused,
    ConsentRevoked,
    DataDeleted,
    SkillIngested { count: usize, tool: String },
    SkillQueried,
    ContextQueried,
    PreferencesQueried,
}

impl AuditEvent {
    fn as_str(&self) -> &str {
        match self {
            AuditEvent::ConsentGranted => "consent_granted",
            AuditEvent::ConsentPaused => "consent_paused",
            AuditEvent::ConsentRevoked => "consent_revoked",
            AuditEvent::DataDeleted => "data_deleted",
            AuditEvent::SkillIngested { .. } => "skill_ingested",
            AuditEvent::SkillQueried => "skill_queried",
            AuditEvent::ContextQueried => "context_queried",
            AuditEvent::PreferencesQueried => "preferences_queried",
        }
    }

    fn detail(&self) -> Option<String> {
        match self {
            AuditEvent::SkillIngested { count, tool } => Some(format!("count={count} tool={tool}")),
            _ => None,
        }
    }
}

/// Thread-safe consent gate. Must be checked before any graph operation.
#[derive(Clone)]
pub struct ConsentGate {
    status: Arc<Mutex<ConsentStatus>>,
    conn: Arc<Mutex<Connection>>,
}

impl ConsentGate {
    /// Open (or create) the consent gate from the given SQLite connection.
    ///
    /// Creates the `audit_log` and `consent_state` tables if they don't exist.
    /// Reads any previously persisted status from `consent_state` so that
    /// a paused or revoked state survives server restarts. Defaults to `Granted`
    /// only on first run (no prior row in `consent_state`).
    pub fn new(conn: Connection) -> Result<Self, ConsentError> {
        // This connection may be opened outside `graph::schema::migrate`, so it
        // needs the contention/scrub pragmas applied here.
        crate::graph::schema::apply_connection_pragmas(&conn)?;
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS audit_log (
                id          INTEGER PRIMARY KEY AUTOINCREMENT,
                event       TEXT NOT NULL,
                detail      TEXT,
                occurred_at TEXT NOT NULL
            );
            CREATE TABLE IF NOT EXISTS consent_state (
                id         INTEGER PRIMARY KEY CHECK (id = 1),
                status     TEXT NOT NULL DEFAULT 'granted',
                updated_at TEXT NOT NULL
            );",
        )?;

        // Restore persisted status; default to Granted only if no prior state exists.
        let persisted: Option<String> = conn
            .query_row("SELECT status FROM consent_state WHERE id = 1", [], |r| {
                r.get(0)
            })
            .ok();

        let is_first_run = persisted.is_none();
        let initial_status = parse_status(persisted.as_deref());

        let gate = Self {
            status: Arc::new(Mutex::new(initial_status)),
            conn: Arc::new(Mutex::new(conn)),
        };

        if is_first_run {
            gate.set_status(ConsentStatus::Granted)?;
            gate.record(AuditEvent::ConsentGranted)?;
        }

        Ok(gate)
    }

    /// Open an in-memory consent gate (used in tests).
    pub fn open_in_memory() -> Result<Self, ConsentError> {
        let conn = Connection::open_in_memory()?;
        Self::new(conn)
    }

    /// Check whether data operations are currently permitted.
    ///
    /// Re-reads the persisted status on every call so a pause or revoke made by
    /// another process (e.g. the desktop dashboard while the MCP server is
    /// running) takes effect immediately — not at the next server restart.
    pub fn check(&self) -> Result<(), ConsentError> {
        match self.refresh_status()? {
            ConsentStatus::Granted => Ok(()),
            ConsentStatus::Paused => Err(ConsentError::Paused),
            ConsentStatus::Revoked => Err(ConsentError::Revoked),
        }
    }

    /// Return the current consent status, re-read from persistent storage.
    pub fn status(&self) -> Result<ConsentStatus, ConsentError> {
        self.refresh_status()
    }

    /// Read the persisted status from `consent_state` and refresh the cache.
    ///
    /// The in-memory copy alone is not authoritative: the MCP server and the
    /// Tauri app are separate processes sharing one database, and each holds
    /// its own `ConsentGate`.
    fn refresh_status(&self) -> Result<ConsentStatus, ConsentError> {
        let persisted: Option<String> = {
            let conn = self.conn()?;
            match conn.query_row("SELECT status FROM consent_state WHERE id = 1", [], |r| {
                r.get(0)
            }) {
                Ok(s) => Some(s),
                Err(rusqlite::Error::QueryReturnedNoRows) => None,
                Err(e) => return Err(e.into()),
            }
        };

        let status = parse_status(persisted.as_deref());
        let mut cached = self.status.lock().map_err(|_| ConsentError::LockPoisoned)?;
        *cached = status.clone();
        Ok(status)
    }

    /// Pause data collection. Existing data is retained.
    pub fn pause(&self) -> Result<(), ConsentError> {
        self.set_status(ConsentStatus::Paused)?;
        self.record(AuditEvent::ConsentPaused)
    }

    /// Resume data collection after a pause.
    pub fn resume(&self) -> Result<(), ConsentError> {
        self.set_status(ConsentStatus::Granted)?;
        self.record(AuditEvent::ConsentGranted)
    }

    /// Revoke consent and delete all collected data: skills, edges, events,
    /// and preferences (including topic summaries).
    pub fn revoke(&self, graph: &GraphHandle) -> Result<(), ConsentError> {
        self.set_status(ConsentStatus::Revoked)?;
        self.record(AuditEvent::ConsentRevoked)?;
        graph.delete_all_data()?;
        self.record(AuditEvent::DataDeleted)?;
        Ok(())
    }

    /// Record an audit event with a timestamp.
    pub fn record(&self, event: AuditEvent) -> Result<(), ConsentError> {
        let now = Utc::now().to_rfc3339();
        self.conn()?.execute(
            "INSERT INTO audit_log (event, detail, occurred_at) VALUES (?1, ?2, ?3)",
            params![event.as_str(), event.detail(), now],
        )?;
        Ok(())
    }

    /// Lock the shared connection.
    fn conn(&self) -> Result<std::sync::MutexGuard<'_, Connection>, ConsentError> {
        self.conn.lock().map_err(|_| ConsentError::LockPoisoned)
    }

    /// Update the cached status and persist it to the `consent_state` table.
    fn set_status(&self, status: ConsentStatus) -> Result<(), ConsentError> {
        *self.status.lock().map_err(|_| ConsentError::LockPoisoned)? = status.clone();
        let now = Utc::now().to_rfc3339();
        self.conn()?.execute(
            "INSERT OR REPLACE INTO consent_state (id, status, updated_at) VALUES (1, ?1, ?2)",
            params![status.to_string(), now],
        )?;
        Ok(())
    }
}

/// Map a persisted status string to a `ConsentStatus`. Missing or unknown
/// values fall back to `Granted`, matching the schema default.
fn parse_status(persisted: Option<&str>) -> ConsentStatus {
    match persisted {
        Some("paused") => ConsentStatus::Paused,
        Some("revoked") => ConsentStatus::Revoked,
        _ => ConsentStatus::Granted,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::graph::GraphHandle;

    #[test]
    fn new_gate_is_granted() {
        let gate = ConsentGate::open_in_memory().unwrap();
        assert_eq!(gate.status().unwrap(), ConsentStatus::Granted);
        gate.check().unwrap();
    }

    #[test]
    fn pause_blocks_check() {
        let gate = ConsentGate::open_in_memory().unwrap();
        gate.pause().unwrap();
        assert!(matches!(gate.check(), Err(ConsentError::Paused)));
    }

    #[test]
    fn resume_after_pause_allows_check() {
        let gate = ConsentGate::open_in_memory().unwrap();
        gate.pause().unwrap();
        gate.resume().unwrap();
        gate.check().unwrap();
    }

    #[test]
    fn revoke_blocks_check_and_deletes_data() {
        let gate = ConsentGate::open_in_memory().unwrap();
        let graph = GraphHandle::open_in_memory().unwrap();
        graph
            .upsert_skill(&crate::private_mode::SkillTag::new("rust"))
            .unwrap();

        gate.revoke(&graph).unwrap();

        assert!(matches!(gate.check(), Err(ConsentError::Revoked)));
        let skills = graph.get_top_skills(10).unwrap();
        assert!(skills.is_empty());
    }

    #[test]
    fn audit_events_are_recorded() {
        let gate = ConsentGate::open_in_memory().unwrap();
        gate.pause().unwrap();

        let count: i64 = {
            let db = gate.conn.lock().unwrap();
            db.query_row("SELECT COUNT(*) FROM audit_log", [], |r| r.get(0))
                .unwrap()
        };
        // ConsentGranted (first run) + ConsentPaused = 2
        assert_eq!(count, 2);
    }

    #[test]
    fn status_transitions_correctly() {
        let gate = ConsentGate::open_in_memory().unwrap();
        assert_eq!(gate.status().unwrap(), ConsentStatus::Granted);
        gate.pause().unwrap();
        assert_eq!(gate.status().unwrap(), ConsentStatus::Paused);
        gate.resume().unwrap();
        assert_eq!(gate.status().unwrap(), ConsentStatus::Granted);
    }

    #[test]
    fn paused_status_persists_across_reopen() {
        let path = tempfile::NamedTempFile::new().unwrap().into_temp_path();
        let path_str = path.to_str().unwrap();

        {
            let conn = Connection::open(path_str).unwrap();
            let gate = ConsentGate::new(conn).unwrap();
            gate.pause().unwrap();
        }

        // Re-open from the same file — status must still be Paused.
        let conn = Connection::open(path_str).unwrap();
        let gate = ConsentGate::new(conn).unwrap();
        assert_eq!(gate.status().unwrap(), ConsentStatus::Paused);
        assert!(matches!(gate.check(), Err(ConsentError::Paused)));
    }

    #[test]
    fn revoked_status_persists_across_reopen() {
        let path = tempfile::NamedTempFile::new().unwrap().into_temp_path();
        let path_str = path.to_str().unwrap();

        {
            let conn = Connection::open(path_str).unwrap();
            let gate = ConsentGate::new(conn).unwrap();
            let graph = GraphHandle::open(path_str).unwrap();
            gate.revoke(&graph).unwrap();
        }

        let conn = Connection::open(path_str).unwrap();
        let gate = ConsentGate::new(conn).unwrap();
        assert_eq!(gate.status().unwrap(), ConsentStatus::Revoked);
        assert!(matches!(gate.check(), Err(ConsentError::Revoked)));
    }

    #[test]
    fn revoke_twice_is_idempotent() {
        let gate = ConsentGate::open_in_memory().unwrap();
        let graph = GraphHandle::open_in_memory().unwrap();
        gate.revoke(&graph).unwrap();
        // Second revoke: status is already Revoked, data already deleted — must not error.
        gate.revoke(&graph).unwrap();
        assert!(matches!(gate.check(), Err(ConsentError::Revoked)));
    }

    #[test]
    fn revoke_after_pause_succeeds() {
        let gate = ConsentGate::open_in_memory().unwrap();
        let graph = GraphHandle::open_in_memory().unwrap();
        gate.pause().unwrap();
        gate.revoke(&graph).unwrap();
        assert!(matches!(gate.check(), Err(ConsentError::Revoked)));
    }

    #[test]
    fn skill_ingested_audit_event_records_count_detail() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS audit_log (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                event TEXT NOT NULL,
                detail TEXT,
                occurred_at TEXT NOT NULL
            );",
        )
        .unwrap();
        let gate = ConsentGate::new(conn).unwrap();
        gate.record(AuditEvent::SkillIngested {
            count: 7,
            tool: "claude".into(),
        })
        .unwrap();

        let detail: Option<String> = {
            let db = gate.conn.lock().unwrap();
            db.query_row(
                "SELECT detail FROM audit_log WHERE event = 'skill_ingested' LIMIT 1",
                [],
                |r| r.get(0),
            )
            .unwrap()
        };
        assert_eq!(detail.as_deref(), Some("count=7 tool=claude"));
    }

    #[test]
    fn pause_in_one_gate_blocks_another_gate_on_same_db() {
        // The desktop app and the MCP server each hold their own ConsentGate
        // over the same database file. A pause in one MUST block the other
        // without a restart.
        let path = tempfile::NamedTempFile::new().unwrap().into_temp_path();
        let path_str = path.to_str().unwrap();

        let dashboard_gate = ConsentGate::new(Connection::open(path_str).unwrap()).unwrap();
        let server_gate = ConsentGate::new(Connection::open(path_str).unwrap()).unwrap();

        server_gate.check().unwrap();
        dashboard_gate.pause().unwrap();
        assert!(
            matches!(server_gate.check(), Err(ConsentError::Paused)),
            "pause from another gate must block immediately"
        );

        dashboard_gate.resume().unwrap();
        server_gate.check().unwrap();
    }

    #[test]
    fn revoke_in_one_gate_blocks_another_gate_on_same_db() {
        let path = tempfile::NamedTempFile::new().unwrap().into_temp_path();
        let path_str = path.to_str().unwrap();

        let graph = GraphHandle::open(path_str).unwrap();
        let dashboard_gate = ConsentGate::new(Connection::open(path_str).unwrap()).unwrap();
        let server_gate = ConsentGate::new(Connection::open(path_str).unwrap()).unwrap();

        server_gate.check().unwrap();
        dashboard_gate.revoke(&graph).unwrap();
        assert!(matches!(server_gate.check(), Err(ConsentError::Revoked)));
    }

    #[test]
    fn revoke_deletes_preferences_and_topic_summaries() {
        let path = tempfile::NamedTempFile::new().unwrap().into_temp_path();
        let path_str = path.to_str().unwrap();

        let graph = GraphHandle::open(path_str).unwrap();
        let gate = ConsentGate::new(Connection::open(path_str).unwrap()).unwrap();

        graph
            .upsert_skill(&crate::private_mode::SkillTag::new("rust"))
            .unwrap();
        graph
            .set_preference("topic_summary:1000", "private work description")
            .unwrap();

        gate.revoke(&graph).unwrap();

        assert!(graph.get_top_skills(10).unwrap().is_empty());
        assert!(
            graph.get_preferences().unwrap().0.is_empty(),
            "preferences (incl. topic summaries) must be wiped on revoke"
        );
    }

    #[test]
    fn no_duplicate_granted_audit_on_restart() {
        let path = tempfile::NamedTempFile::new().unwrap().into_temp_path();
        let path_str = path.to_str().unwrap();

        // First open → 1 ConsentGranted event
        {
            let conn = Connection::open(path_str).unwrap();
            ConsentGate::new(conn).unwrap();
        }

        // Second open (not first run) → no new ConsentGranted event
        let conn = Connection::open(path_str).unwrap();
        let gate = ConsentGate::new(conn).unwrap();
        let count: i64 = {
            let db = gate.conn.lock().unwrap();
            db.query_row(
                "SELECT COUNT(*) FROM audit_log WHERE event = 'consent_granted'",
                [],
                |r| r.get(0),
            )
            .unwrap()
        };
        assert_eq!(
            count, 1,
            "should only have one consent_granted event across restarts"
        );
    }

    #[test]
    fn status_lock_poisoned_returns_error() {
        let gate = ConsentGate::open_in_memory().unwrap();
        let status_clone = Arc::clone(&gate.status);
        let _ = std::thread::spawn(move || {
            let _guard = status_clone.lock().unwrap();
            panic!("intentional poison for test");
        })
        .join();
        assert!(matches!(gate.check(), Err(ConsentError::LockPoisoned)));
        assert!(matches!(gate.status(), Err(ConsentError::LockPoisoned)));
    }

    #[test]
    fn conn_lock_poisoned_returns_error_on_record() {
        let gate = ConsentGate::open_in_memory().unwrap();
        let conn_clone = Arc::clone(&gate.conn);
        let _ = std::thread::spawn(move || {
            let _guard = conn_clone.lock().unwrap();
            panic!("intentional poison for test");
        })
        .join();
        assert!(matches!(
            gate.record(AuditEvent::SkillQueried),
            Err(ConsentError::LockPoisoned)
        ));
    }
}
