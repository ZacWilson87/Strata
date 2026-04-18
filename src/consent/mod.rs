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
    SkillIngested { count: usize },
    SkillQueried,
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
            AuditEvent::PreferencesQueried => "preferences_queried",
        }
    }

    fn detail(&self) -> Option<String> {
        match self {
            AuditEvent::SkillIngested { count } => Some(format!("count={count}")),
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
    /// Create a new consent gate backed by the given SQLite connection.
    /// Defaults to `Granted` for MVP — a real product would show an onboarding consent screen.
    pub fn new(conn: Connection) -> Result<Self, ConsentError> {
        let gate = Self {
            status: Arc::new(Mutex::new(ConsentStatus::Granted)),
            conn: Arc::new(Mutex::new(conn)),
        };
        gate.record(AuditEvent::ConsentGranted)?;
        Ok(gate)
    }

    /// Open an in-memory consent gate (used in tests).
    pub fn open_in_memory() -> Result<Self, ConsentError> {
        let conn = Connection::open_in_memory()?;
        // Ensure audit_log table exists.
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS audit_log (
                id          INTEGER PRIMARY KEY AUTOINCREMENT,
                event       TEXT NOT NULL,
                detail      TEXT,
                occurred_at TEXT NOT NULL
            );",
        )?;
        Self::new(conn)
    }

    /// Check whether data operations are currently permitted.
    pub fn check(&self) -> Result<(), ConsentError> {
        let status = self.status.lock().map_err(|_| ConsentError::LockPoisoned)?;
        match *status {
            ConsentStatus::Granted => Ok(()),
            ConsentStatus::Paused => Err(ConsentError::Paused),
            ConsentStatus::Revoked => Err(ConsentError::Revoked),
        }
    }

    /// Return the current consent status.
    pub fn status(&self) -> Result<ConsentStatus, ConsentError> {
        self.status
            .lock()
            .map(|s| s.clone())
            .map_err(|_| ConsentError::LockPoisoned)
    }

    /// Pause data collection. Existing data is retained.
    pub fn pause(&self) -> Result<(), ConsentError> {
        let mut status = self.status.lock().map_err(|_| ConsentError::LockPoisoned)?;
        *status = ConsentStatus::Paused;
        drop(status);
        self.record(AuditEvent::ConsentPaused)
    }

    /// Resume data collection after a pause.
    pub fn resume(&self) -> Result<(), ConsentError> {
        let mut status = self.status.lock().map_err(|_| ConsentError::LockPoisoned)?;
        *status = ConsentStatus::Granted;
        drop(status);
        self.record(AuditEvent::ConsentGranted)
    }

    /// Revoke consent and delete all collected data from the graph.
    pub fn revoke(&self, graph: &GraphHandle) -> Result<(), ConsentError> {
        {
            let mut status = self.status.lock().map_err(|_| ConsentError::LockPoisoned)?;
            *status = ConsentStatus::Revoked;
        }
        self.record(AuditEvent::ConsentRevoked)?;
        graph.delete_all_skills()?;
        self.record(AuditEvent::DataDeleted)?;
        Ok(())
    }

    /// Record an audit event with a timestamp.
    pub fn record(&self, event: AuditEvent) -> Result<(), ConsentError> {
        let conn = self.conn.lock().map_err(|_| ConsentError::LockPoisoned)?;
        let now = Utc::now().to_rfc3339();
        conn.execute(
            "INSERT INTO audit_log (event, detail, occurred_at) VALUES (?1, ?2, ?3)",
            params![event.as_str(), event.detail(), now],
        )?;
        Ok(())
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
        gate.check().unwrap(); // should not error
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
        gate.pause().unwrap();

        let count: i64 = {
            let db = gate.conn.lock().unwrap();
            db.query_row("SELECT COUNT(*) FROM audit_log", [], |r| r.get(0))
                .unwrap()
        };
        // ConsentGranted (from new) + ConsentPaused = 2
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
}
