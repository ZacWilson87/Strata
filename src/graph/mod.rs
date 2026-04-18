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

pub use queries::{GraphError, Preferences, SkillEdge, SkillNode};

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
    pub fn get_top_skills(&self, limit: usize) -> Result<Vec<SkillNode>, GraphError> {
        let conn = self.conn.lock().map_err(|_| GraphError::LockPoisoned)?;
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

    /// Delete all skill data (called on consent revocation).
    pub fn delete_all_skills(&self) -> Result<(), GraphError> {
        let conn = self.conn.lock().map_err(|_| GraphError::LockPoisoned)?;
        queries::delete_all_skills(&conn)
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
}
