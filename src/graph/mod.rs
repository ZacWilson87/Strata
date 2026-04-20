/// Private skill graph backed by local SQLite.
///
/// All persistence goes through this module. Only derived data (`SkillTag`,
/// `SkillNode`, `DerivedSummary`) is stored — never raw content.
pub mod queries;
pub mod schema;

use std::sync::{Arc, Mutex};

use chrono::Utc;
use rusqlite::Connection;

use crate::private_mode::{DerivedSummary, SkillTag};

pub use queries::{
    CoOccurrenceSummary, GraphError, Preferences, SkillEdge, SkillNode, SkillNodeWithVelocity,
    SkillVelocity, TopicSummaryEntry, VelocityDirection,
};

/// Thread-safe handle to the skill graph database.
#[derive(Clone)]
pub struct GraphHandle {
    conn: Arc<Mutex<Connection>>,
}

impl GraphHandle {
    /// Open (or create) the SQLite database at the given path and apply migrations.
    pub fn open(path: &str) -> Result<Self, GraphError> {
        let conn = Connection::open(path)?;
        schema::migrate(&conn)?;
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

    /// Upsert a skill node. Increments strength and session count if it already exists.
    pub fn upsert_skill(&self, tag: &SkillTag) -> Result<SkillNode, GraphError> {
        let conn = self.conn.lock().map_err(|_| GraphError::LockPoisoned)?;
        queries::upsert_skill(&conn, tag)
    }

    /// Record a co-occurrence edge between two skills.
    pub fn record_co_occurrence(&self, a: &SkillTag, b: &SkillTag) -> Result<(), GraphError> {
        if a == b {
            return Ok(());
        }
        let conn = self.conn.lock().map_err(|_| GraphError::LockPoisoned)?;
        queries::record_co_occurrence(&conn, a, b)
    }

    /// Return the top `limit` skills ranked by strength.
    ///
    /// Runs a passive WAL checkpoint first so that writes from other processes
    /// (e.g. the MCP server) are visible to this long-lived connection.
    pub fn get_top_skills(&self, limit: usize) -> Result<Vec<SkillNode>, GraphError> {
        let conn = self.conn.lock().map_err(|_| GraphError::LockPoisoned)?;
        let _ = conn.execute_batch("PRAGMA wal_checkpoint(PASSIVE);");
        queries::get_top_skills(&conn, limit)
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
        let conn = self.conn.lock().map_err(|_| GraphError::LockPoisoned)?;
        queries::get_preferences(&conn)
    }

    /// Set a user preference key-value pair.
    pub fn set_preference(&self, key: &str, value: &str) -> Result<(), GraphError> {
        let conn = self.conn.lock().map_err(|_| GraphError::LockPoisoned)?;
        queries::set_preference(&conn, key, value)
    }

    /// Delete a single preference by key.
    pub fn delete_preference(&self, key: &str) -> Result<(), GraphError> {
        let conn = self.conn.lock().map_err(|_| GraphError::LockPoisoned)?;
        queries::delete_preference(&conn, key)
    }

    /// Delete all skill data (called on consent revocation).
    pub fn delete_all_skills(&self) -> Result<(), GraphError> {
        let conn = self.conn.lock().map_err(|_| GraphError::LockPoisoned)?;
        queries::delete_all_skills(&conn)
    }

    /// Write (or replace) today's snapshot for a skill.
    pub fn upsert_daily_snapshot(
        &self,
        tag: &SkillTag,
        session_count: i64,
        strength: f64,
    ) -> Result<(), GraphError> {
        let conn = self.conn.lock().map_err(|_| GraphError::LockPoisoned)?;
        queries::upsert_daily_snapshot(&conn, tag, session_count, strength)
    }

    /// Return velocity data for the top `limit` skills.
    pub fn get_skill_velocities(&self, limit: usize) -> Result<Vec<SkillVelocity>, GraphError> {
        let conn = self.conn.lock().map_err(|_| GraphError::LockPoisoned)?;
        let _ = conn.execute_batch("PRAGMA wal_checkpoint(PASSIVE);");
        queries::get_skill_velocities(&conn, limit)
    }

    /// Return the top `limit` skills enriched with velocity and co-occurrence data.
    pub fn get_skills_with_velocity(
        &self,
        limit: usize,
    ) -> Result<Vec<SkillNodeWithVelocity>, GraphError> {
        let conn = self.conn.lock().map_err(|_| GraphError::LockPoisoned)?;
        let _ = conn.execute_batch("PRAGMA wal_checkpoint(PASSIVE);");
        queries::get_skills_with_velocity(&conn, limit)
    }

    /// Return all stored topic summaries, newest first.
    pub fn get_topic_summaries(&self) -> Result<Vec<TopicSummaryEntry>, GraphError> {
        let conn = self.conn.lock().map_err(|_| GraphError::LockPoisoned)?;
        queries::get_topic_summaries(&conn)
    }
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
    fn delete_all_skills_clears_graph() {
        let graph = GraphHandle::open_in_memory().unwrap();
        graph.upsert_skill(&SkillTag::new("rust")).unwrap();
        graph.delete_all_skills().unwrap();
        let top = graph.get_top_skills(10).unwrap();
        assert!(top.is_empty());
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
