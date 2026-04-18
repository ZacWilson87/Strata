/// Typed query interface for the skill graph.
use chrono::{DateTime, Utc};
use rusqlite::{params, Connection};
use std::collections::HashMap;
use uuid::Uuid;

use crate::private_mode::SkillTag;

/// Errors that can occur during graph operations.
#[derive(Debug, thiserror::Error)]
pub enum GraphError {
    #[error("database error: {0}")]
    Sqlite(#[from] rusqlite::Error),

    #[error("database lock poisoned")]
    LockPoisoned,

    #[error("skill not found: {0}")]
    NotFound(String),
}

/// A skill node stored in the graph.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SkillNode {
    pub id: String,
    pub tag: String,
    pub strength: f64,
    pub last_seen: DateTime<Utc>,
    pub session_count: i64,
}

/// A co-occurrence edge between two skills.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SkillEdge {
    pub from_id: String,
    pub to_id: String,
    pub co_occurrence: i64,
}

/// User preferences stored as key-value pairs.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, Default)]
pub struct Preferences(pub HashMap<String, String>);

impl Preferences {
    pub fn get(&self, key: &str) -> Option<&str> {
        self.0.get(key).map(String::as_str)
    }
}

/// Upsert a skill: insert on first occurrence, increment on subsequent ones.
pub fn upsert_skill(conn: &Connection, tag: &SkillTag) -> Result<SkillNode, GraphError> {
    let now = Utc::now().to_rfc3339();
    let id = Uuid::new_v4().to_string();

    conn.execute(
        "INSERT INTO skills (id, tag, strength, last_seen, session_count)
         VALUES (?1, ?2, 1.0, ?3, 1)
         ON CONFLICT(tag) DO UPDATE SET
             strength = strength + 1.0,
             last_seen = excluded.last_seen,
             session_count = session_count + 1",
        params![id, tag.as_str(), now],
    )?;

    let node = conn.query_row(
        "SELECT id, tag, strength, last_seen, session_count FROM skills WHERE tag = ?1",
        params![tag.as_str()],
        row_to_skill_node,
    )?;
    Ok(node)
}

/// Record a co-occurrence between two skills. The pair is normalised (lexicographic order).
pub fn record_co_occurrence(
    conn: &Connection,
    a: &SkillTag,
    b: &SkillTag,
) -> Result<(), GraphError> {
    // Normalise order so (a,b) and (b,a) map to the same edge.
    let (from_tag, to_tag) = if a.as_str() <= b.as_str() {
        (a, b)
    } else {
        (b, a)
    };

    // Fetch IDs (upsert if not present).
    let from = upsert_skill(conn, from_tag)?;
    let to = upsert_skill(conn, to_tag)?;

    conn.execute(
        "INSERT INTO skill_edges (from_id, to_id, co_occurrence)
         VALUES (?1, ?2, 1)
         ON CONFLICT(from_id, to_id) DO UPDATE SET co_occurrence = co_occurrence + 1",
        params![from.id, to.id],
    )?;
    Ok(())
}

/// Return the top `limit` skills ordered by strength descending.
pub fn get_top_skills(conn: &Connection, limit: usize) -> Result<Vec<SkillNode>, GraphError> {
    let mut stmt = conn.prepare(
        "SELECT id, tag, strength, last_seen, session_count
         FROM skills
         ORDER BY strength DESC
         LIMIT ?1",
    )?;

    let nodes = stmt
        .query_map(params![limit as i64], row_to_skill_node)?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(nodes)
}

/// Return all user preferences.
pub fn get_preferences(conn: &Connection) -> Result<Preferences, GraphError> {
    let mut stmt = conn.prepare("SELECT key, value FROM preferences")?;
    let map: HashMap<String, String> = stmt
        .query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })?
        .collect::<Result<_, _>>()?;
    Ok(Preferences(map))
}

/// Set a preference key-value pair.
pub fn set_preference(conn: &Connection, key: &str, value: &str) -> Result<(), GraphError> {
    let now = Utc::now().to_rfc3339();
    conn.execute(
        "INSERT INTO preferences (key, value, updated_at) VALUES (?1, ?2, ?3)
         ON CONFLICT(key) DO UPDATE SET value = excluded.value, updated_at = excluded.updated_at",
        params![key, value, now],
    )?;
    Ok(())
}

/// Delete all skill data (called on consent revocation).
pub fn delete_all_skills(conn: &Connection) -> Result<(), GraphError> {
    conn.execute_batch("DELETE FROM skill_edges; DELETE FROM skills;")?;
    Ok(())
}

fn row_to_skill_node(row: &rusqlite::Row<'_>) -> rusqlite::Result<SkillNode> {
    let last_seen_str: String = row.get(3)?;
    let last_seen = DateTime::parse_from_rfc3339(&last_seen_str)
        .map(|dt| dt.with_timezone(&Utc))
        .unwrap_or_else(|_| Utc::now());
    Ok(SkillNode {
        id: row.get(0)?,
        tag: row.get(1)?,
        strength: row.get(2)?,
        last_seen,
        session_count: row.get(4)?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::graph::schema::migrate;

    fn fresh_conn() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        migrate(&conn).unwrap();
        conn
    }

    #[test]
    fn upsert_creates_skill() {
        let conn = fresh_conn();
        let node = upsert_skill(&conn, &SkillTag::new("rust")).unwrap();
        assert_eq!(node.tag, "rust");
        assert_eq!(node.session_count, 1);
    }

    #[test]
    fn upsert_increments_existing_skill() {
        let conn = fresh_conn();
        upsert_skill(&conn, &SkillTag::new("rust")).unwrap();
        let node = upsert_skill(&conn, &SkillTag::new("rust")).unwrap();
        assert_eq!(node.session_count, 2);
        assert!((node.strength - 2.0).abs() < f64::EPSILON);
    }

    #[test]
    fn get_top_skills_respects_limit() {
        let conn = fresh_conn();
        for tag in &["rust", "python", "typescript", "go"] {
            upsert_skill(&conn, &SkillTag::new(*tag)).unwrap();
        }
        let top = get_top_skills(&conn, 2).unwrap();
        assert_eq!(top.len(), 2);
    }

    #[test]
    fn get_top_skills_orders_by_strength() {
        let conn = fresh_conn();
        upsert_skill(&conn, &SkillTag::new("python")).unwrap();
        upsert_skill(&conn, &SkillTag::new("rust")).unwrap();
        upsert_skill(&conn, &SkillTag::new("rust")).unwrap();
        let top = get_top_skills(&conn, 10).unwrap();
        assert_eq!(top[0].tag, "rust");
    }

    #[test]
    fn co_occurrence_is_idempotent_for_pair_order() {
        let conn = fresh_conn();
        let a = SkillTag::new("async");
        let b = SkillTag::new("rust");
        record_co_occurrence(&conn, &a, &b).unwrap();
        record_co_occurrence(&conn, &b, &a).unwrap(); // reversed pair
        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM skill_edges", [], |r| r.get(0))
            .unwrap();
        assert_eq!(count, 1);
    }

    #[test]
    fn preferences_set_and_get() {
        let conn = fresh_conn();
        set_preference(&conn, "color_scheme", "dark").unwrap();
        let prefs = get_preferences(&conn).unwrap();
        assert_eq!(prefs.get("color_scheme"), Some("dark"));
    }

    #[test]
    fn preferences_upsert_updates_value() {
        let conn = fresh_conn();
        set_preference(&conn, "lang", "en").unwrap();
        set_preference(&conn, "lang", "fr").unwrap();
        let prefs = get_preferences(&conn).unwrap();
        assert_eq!(prefs.get("lang"), Some("fr"));
    }

    #[test]
    fn delete_all_skills_clears_nodes_and_edges() {
        let conn = fresh_conn();
        let a = SkillTag::new("rust");
        let b = SkillTag::new("async");
        upsert_skill(&conn, &a).unwrap();
        upsert_skill(&conn, &b).unwrap();
        record_co_occurrence(&conn, &a, &b).unwrap();
        delete_all_skills(&conn).unwrap();
        let skills = get_top_skills(&conn, 10).unwrap();
        assert!(skills.is_empty());
        let edge_count: i64 = conn
            .query_row("SELECT COUNT(*) FROM skill_edges", [], |r| r.get(0))
            .unwrap();
        assert_eq!(edge_count, 0);
    }
}
