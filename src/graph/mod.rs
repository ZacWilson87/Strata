/// Private skill graph backed by local SQLite.
///
/// All persistence goes through this module. Only derived data (`SkillTag`,
/// `SkillNode`, `DerivedSummary`) is stored — never raw content.
pub mod insights;
pub mod queries;
pub mod schema;

use std::collections::HashSet;
use std::sync::{Arc, Mutex, MutexGuard};

use chrono::Utc;
use rusqlite::Connection;

use crate::private_mode::{DerivedSummary, SkillTag};

pub use insights::Insight;
pub use queries::{
    topic_summary_key, AuditEntry, CoOccurrenceSummary, GraphError, Preferences, SessionSignalRow,
    SkillEdge, SkillNode, SkillNodeWithVelocity, SkillVelocity, TopicSummaryEntry,
    VelocityDirection, WeeklySnapshot, TOPIC_SUMMARY_PREFIX, USER_PREF_PREFIX,
};

/// Preference-key namespace for dismissed insight ids.
const INSIGHT_DISMISSED_PREFIX: &str = "insight_dismissed:";

/// Thread-safe handle to the skill graph database.
#[derive(Clone)]
pub struct GraphHandle {
    conn: Arc<Mutex<Connection>>,
}

impl GraphHandle {
    /// Open (or create) the SQLite database at the given path and apply migrations.
    ///
    /// On Unix the database files are restricted to owner-only (0600): the graph
    /// holds derived-but-personal data and must not be readable by other users.
    pub fn open(path: &str) -> Result<Self, GraphError> {
        let conn = Connection::open(path)?;
        schema::migrate(&conn)?;
        restrict_db_permissions(path);
        Ok(Self {
            conn: Arc::new(Mutex::new(conn)),
        })
    }

    /// Open an in-memory database — used in tests.
    pub fn open_in_memory() -> Result<Self, GraphError> {
        let conn = Connection::open_in_memory()?;
        schema::migrate(&conn)?;
        Ok(Self {
            conn: Arc::new(Mutex::new(conn)),
        })
    }

    /// Lock the shared connection.
    fn conn(&self) -> Result<MutexGuard<'_, Connection>, GraphError> {
        self.conn.lock().map_err(|_| GraphError::LockPoisoned)
    }

    /// Lock the shared connection after a passive WAL checkpoint, so writes
    /// from other processes (e.g. the MCP server while the dashboard reads)
    /// are visible to this long-lived connection.
    fn conn_synced(&self) -> Result<MutexGuard<'_, Connection>, GraphError> {
        let conn = self.conn()?;
        let _ = conn.execute_batch("PRAGMA wal_checkpoint(PASSIVE);");
        Ok(conn)
    }

    /// Upsert a skill node. Increments strength and session count if it already exists.
    pub fn upsert_skill(&self, tag: &SkillTag) -> Result<SkillNode, GraphError> {
        queries::upsert_skill(&*self.conn()?, tag)
    }

    /// Upsert a skill with an explicit event day and last-seen timestamp.
    /// Used by the transcript backfill to attribute work to its real date.
    pub fn upsert_skill_on_day(
        &self,
        tag: &SkillTag,
        day: &str,
        last_seen: &str,
    ) -> Result<SkillNode, GraphError> {
        queries::upsert_skill_on_day(&*self.conn()?, tag, day, last_seen)
    }

    /// Whether a transcript session id has already been ingested by the
    /// backfill or the session-end hook.
    pub fn is_session_ingested(&self, session_id: &str) -> Result<bool, GraphError> {
        queries::is_session_ingested(&*self.conn_synced()?, session_id)
    }

    /// Mark a transcript session id as ingested (idempotent).
    pub fn mark_session_ingested(&self, session_id: &str, day: &str) -> Result<(), GraphError> {
        queries::mark_session_ingested(&*self.conn()?, session_id, day)
    }

    /// Record a co-occurrence edge between two skills.
    pub fn record_co_occurrence(&self, a: &SkillTag, b: &SkillTag) -> Result<(), GraphError> {
        if a == b {
            return Ok(());
        }
        queries::record_co_occurrence(&*self.conn()?, a, b)
    }

    /// Return the top `limit` skills ranked by strength.
    pub fn get_top_skills(&self, limit: usize) -> Result<Vec<SkillNode>, GraphError> {
        queries::get_top_skills(&*self.conn_synced()?, limit)
    }

    /// Produce a derived summary of the user's skill profile.
    pub fn get_skill_summary(&self) -> Result<DerivedSummary, GraphError> {
        let skills = self.get_top_skills(10)?;
        if skills.is_empty() {
            return Ok(DerivedSummary::new("No skills recorded yet.".into()));
        }
        let tags: Vec<String> = skills.iter().map(|s| s.tag.clone()).collect();
        Ok(DerivedSummary::new(tags.join(", ")))
    }

    /// Return the current context summary (top skills + active domains).
    pub fn get_context_summary(&self) -> Result<DerivedSummary, GraphError> {
        let skills = self.get_top_skills(5)?;
        if skills.is_empty() {
            return Ok(DerivedSummary::new("No context available yet.".into()));
        }
        let recent: Vec<String> = skills
            .iter()
            .filter(|s| {
                let age = Utc::now().signed_duration_since(s.last_seen).num_hours();
                age < 48
            })
            .map(|s| s.tag.clone())
            .collect();
        if recent.is_empty() {
            Ok(DerivedSummary::new("No recent activity.".into()))
        } else {
            Ok(DerivedSummary::new(format!(
                "Active in: {}",
                recent.join(", ")
            )))
        }
    }

    /// Get stored user preferences.
    pub fn get_preferences(&self) -> Result<Preferences, GraphError> {
        queries::get_preferences(&*self.conn()?)
    }

    /// Return preference (key, value) pairs whose key starts with `prefix`.
    pub fn get_preferences_with_prefix(
        &self,
        prefix: &str,
    ) -> Result<Vec<(String, String)>, GraphError> {
        queries::get_preferences_with_prefix(&*self.conn()?, prefix)
    }

    /// Set a user preference key-value pair.
    pub fn set_preference(&self, key: &str, value: &str) -> Result<(), GraphError> {
        queries::set_preference(&*self.conn()?, key, value)
    }

    /// Delete a single preference by key.
    pub fn delete_preference(&self, key: &str) -> Result<(), GraphError> {
        queries::delete_preference(&*self.conn()?, key)
    }

    /// Return the most recent `limit` audit log entries (newest first).
    pub fn get_audit_log(&self, limit: usize) -> Result<Vec<AuditEntry>, GraphError> {
        queries::get_audit_log(&*self.conn_synced()?, limit)
    }

    /// Return per-week skill activity snapshots for the last `weeks` weeks.
    pub fn get_skill_history(&self, weeks: usize) -> Result<Vec<WeeklySnapshot>, GraphError> {
        queries::get_skill_history(&*self.conn()?, weeks)
    }

    /// Delete ALL collected data — skills, edges, events, and preferences
    /// (including topic summaries). Called on consent revocation.
    pub fn delete_all_data(&self) -> Result<(), GraphError> {
        queries::delete_all_data(&*self.conn()?)
    }

    /// Return recency-weighted strength per tag (30-day half-life decay).
    pub fn get_recent_strengths(
        &self,
    ) -> Result<std::collections::HashMap<String, f64>, GraphError> {
        queries::get_recent_strengths(&*self.conn_synced()?)
    }

    /// Return velocity data for the top `limit` skills.
    pub fn get_skill_velocities(&self, limit: usize) -> Result<Vec<SkillVelocity>, GraphError> {
        queries::get_skill_velocities(&*self.conn_synced()?, limit)
    }

    /// Return the top `limit` skills enriched with velocity and co-occurrence data.
    pub fn get_skills_with_velocity(
        &self,
        limit: usize,
    ) -> Result<Vec<SkillNodeWithVelocity>, GraphError> {
        queries::get_skills_with_velocity(&*self.conn_synced()?, limit)
    }

    /// Return all stored topic summaries, newest first.
    pub fn get_topic_summaries(&self) -> Result<Vec<TopicSummaryEntry>, GraphError> {
        queries::get_topic_summaries(&*self.conn()?)
    }

    /// Record a per-session derived signal row (friction flags, features, outcome).
    pub fn record_session_signal(&self, row: &SessionSignalRow) -> Result<(), GraphError> {
        queries::record_session_signal(&*self.conn()?, row)
    }

    /// Return session signals from the last `days` days, newest first.
    pub fn get_session_signals_since(
        &self,
        days: i64,
    ) -> Result<Vec<SessionSignalRow>, GraphError> {
        queries::get_session_signals_since(&*self.conn_synced()?, days)
    }

    /// Compute actionable workflow insights from the last 30 days of session
    /// signals, excluding any the user has dismissed.
    pub fn get_insights(&self) -> Result<Vec<Insight>, GraphError> {
        let rows = self.get_session_signals_since(insights::INSIGHT_WINDOW_DAYS)?;
        let dismissed: HashSet<String> = self
            .get_preferences_with_prefix(INSIGHT_DISMISSED_PREFIX)?
            .into_iter()
            .filter_map(|(k, _)| k.strip_prefix(INSIGHT_DISMISSED_PREFIX).map(str::to_string))
            .collect();
        Ok(insights::compute_insights(&rows, &dismissed))
    }

    /// Mark an insight as dismissed so it no longer appears in `get_insights`.
    ///
    /// The id is validated here, at the single entry point to persistence:
    /// malformed ids are rejected with an error and never stored.
    pub fn dismiss_insight(&self, id: &str) -> Result<(), GraphError> {
        if !insights::is_valid_insight_id(id) {
            tracing::warn!("rejected malformed insight id in dismiss_insight");
            return Err(GraphError::NotFound("invalid insight id".into()));
        }
        self.set_preference(
            &format!("{INSIGHT_DISMISSED_PREFIX}{id}"),
            &Utc::now().to_rfc3339(),
        )
    }
}

/// Restrict the database file (and its WAL/SHM siblings) to owner-only access.
/// Best-effort: a permissions failure must not prevent the app from starting.
#[cfg(unix)]
fn restrict_db_permissions(path: &str) {
    use std::os::unix::fs::PermissionsExt;
    for candidate in [
        path.to_string(),
        format!("{path}-wal"),
        format!("{path}-shm"),
    ] {
        if let Ok(metadata) = std::fs::metadata(&candidate) {
            let mut perms = metadata.permissions();
            perms.set_mode(0o600);
            if let Err(e) = std::fs::set_permissions(&candidate, perms) {
                tracing::warn!("could not restrict permissions on {candidate}: {e}");
            }
        }
    }
}

#[cfg(not(unix))]
fn restrict_db_permissions(_path: &str) {
    // Windows: %APPDATA% is already per-user; ACL hardening tracked separately.
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn open_in_memory_succeeds() {
        GraphHandle::open_in_memory().unwrap();
    }

    #[test]
    fn upsert_and_retrieve_skill() {
        let graph = GraphHandle::open_in_memory().unwrap();
        let tag = SkillTag::new("rust");
        graph.upsert_skill(&tag).unwrap();
        let top = graph.get_top_skills(10).unwrap();
        assert_eq!(top.len(), 1);
        assert_eq!(top[0].tag, "rust");
    }

    #[test]
    fn upsert_skill_is_idempotent() {
        let graph = GraphHandle::open_in_memory().unwrap();
        let tag = SkillTag::new("async");
        graph.upsert_skill(&tag).unwrap();
        graph.upsert_skill(&tag).unwrap();
        let top = graph.get_top_skills(10).unwrap();
        assert_eq!(top.len(), 1);
        assert_eq!(top[0].session_count, 2);
    }

    #[test]
    fn get_top_skills_ranks_by_strength() {
        let graph = GraphHandle::open_in_memory().unwrap();
        graph.upsert_skill(&SkillTag::new("rust")).unwrap();
        graph.upsert_skill(&SkillTag::new("rust")).unwrap();
        graph.upsert_skill(&SkillTag::new("python")).unwrap();
        let top = graph.get_top_skills(10).unwrap();
        assert_eq!(top[0].tag, "rust");
    }

    #[test]
    fn get_skill_summary_returns_derived_only() {
        let graph = GraphHandle::open_in_memory().unwrap();
        graph.upsert_skill(&SkillTag::new("rust")).unwrap();
        let summary = graph.get_skill_summary().unwrap();
        assert!(summary.as_str().contains("rust"));
        // Must not contain any raw content marker
        assert!(!summary.as_str().contains("prompt"));
    }

    #[test]
    fn delete_all_data_clears_graph() {
        let graph = GraphHandle::open_in_memory().unwrap();
        graph.upsert_skill(&SkillTag::new("rust")).unwrap();
        graph.set_preference("topic_summary:1", "secret").unwrap();
        graph.delete_all_data().unwrap();
        assert!(graph.get_top_skills(10).unwrap().is_empty());
        assert!(graph.get_preferences().unwrap().0.is_empty());
    }

    #[test]
    fn preferences_roundtrip() {
        let graph = GraphHandle::open_in_memory().unwrap();
        graph.set_preference("theme", "dark").unwrap();
        let prefs = graph.get_preferences().unwrap();
        assert_eq!(prefs.get("theme"), Some("dark"));
    }

    #[test]
    fn get_skill_summary_empty_returns_fallback() {
        let graph = GraphHandle::open_in_memory().unwrap();
        let summary = graph.get_skill_summary().unwrap();
        assert_eq!(summary.as_str(), "No skills recorded yet.");
    }

    #[test]
    fn get_context_summary_empty_returns_fallback() {
        let graph = GraphHandle::open_in_memory().unwrap();
        let summary = graph.get_context_summary().unwrap();
        assert_eq!(summary.as_str(), "No context available yet.");
    }

    #[test]
    fn get_context_summary_recent_skills_appear_in_output() {
        let graph = GraphHandle::open_in_memory().unwrap();
        graph.upsert_skill(&SkillTag::new("rust")).unwrap();
        let summary = graph.get_context_summary().unwrap();
        assert!(
            summary.as_str().contains("rust"),
            "recent skill should appear: {}",
            summary.as_str()
        );
        assert!(summary.as_str().starts_with("Active in:"));
    }

    #[test]
    fn get_context_summary_old_skills_return_no_recent_activity() {
        let graph = GraphHandle::open_in_memory().unwrap();
        graph.upsert_skill(&SkillTag::new("rust")).unwrap();
        // Backdating the skill to 49 hours ago directly via the internal connection.
        {
            let conn = graph.conn.lock().unwrap();
            let old_ts = (Utc::now() - chrono::Duration::hours(49)).to_rfc3339();
            conn.execute(
                "UPDATE skills SET last_seen = ?1",
                rusqlite::params![old_ts],
            )
            .unwrap();
        }
        let summary = graph.get_context_summary().unwrap();
        assert_eq!(summary.as_str(), "No recent activity.");
    }

    #[test]
    fn record_co_occurrence_same_tag_returns_ok() {
        let graph = GraphHandle::open_in_memory().unwrap();
        let tag = SkillTag::new("rust");
        // Self-reference guard in GraphHandle: should return Ok without creating an edge.
        graph.record_co_occurrence(&tag, &tag).unwrap();
        let skills = graph.get_top_skills(10).unwrap();
        // No skills upserted because the call short-circuits before upsert.
        assert!(skills.is_empty());
    }

    #[test]
    fn delete_preference_nonexistent_key_is_ok() {
        let graph = GraphHandle::open_in_memory().unwrap();
        graph.delete_preference("no_such_key").unwrap();
    }

    #[test]
    fn get_insights_end_to_end_fires_and_dismisses() {
        let graph = GraphHandle::open_in_memory().unwrap();
        let today = Utc::now().date_naive().to_string();
        for _ in 0..3 {
            graph
                .record_session_signal(&SessionSignalRow {
                    day: today.clone(),
                    tool: "claude-code".into(),
                    work_type: Some("creation".into()),
                    domains: vec!["rust".into()],
                    friction: vec!["repeated_context".into()],
                    features: vec![],
                    outcome: Some("resolved".into()),
                })
                .unwrap();
        }

        let insights = graph.get_insights().unwrap();
        assert_eq!(insights.len(), 1);
        assert_eq!(insights[0].id, "repeated_context:rust");
        assert_eq!(insights[0].rule, "repeated_context");

        graph.dismiss_insight("repeated_context:rust").unwrap();
        assert!(graph.get_insights().unwrap().is_empty());
    }

    #[test]
    fn dismiss_insight_rejects_malformed_ids() {
        let graph = GraphHandle::open_in_memory().unwrap();
        assert!(graph.dismiss_insight("").is_err());
        assert!(graph.dismiss_insight("Bad Id!").is_err());
        assert!(graph.dismiss_insight(&"x".repeat(129)).is_err());
        // Nothing was persisted for the rejected ids.
        assert!(graph.get_preferences().unwrap().0.is_empty());
    }

    #[test]
    fn upsert_skill_returns_lock_poisoned_when_conn_is_poisoned() {
        let graph = GraphHandle::open_in_memory().unwrap();
        let conn_clone = Arc::clone(&graph.conn);
        let _ = std::thread::spawn(move || {
            let _guard = conn_clone.lock().unwrap();
            panic!("intentional poison for test");
        })
        .join();
        let tag = SkillTag::new("rust");
        assert!(matches!(
            graph.upsert_skill(&tag),
            Err(GraphError::LockPoisoned)
        ));
    }
}
