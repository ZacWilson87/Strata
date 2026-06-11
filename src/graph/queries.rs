/// Typed query interface for the skill graph.
use chrono::{DateTime, Utc};
use rusqlite::{params, Connection, OptionalExtension};
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

/// Direction of a skill's growth trajectory over the last 7 days vs the prior 7.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum VelocityDirection {
    /// Growing faster than prior period (>20% more sessions).
    Accelerating,
    /// No significant change.
    Stable,
    /// Growing slower than prior period (>20% fewer sessions).
    Declining,
    /// Fewer than 2 days of history — direction cannot be computed yet.
    New,
}

/// Velocity of a single skill over rolling 7-day windows.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SkillVelocity {
    pub tag: String,
    pub direction: VelocityDirection,
    /// `sessions_last_7d - sessions_prior_7d`. Positive means accelerating.
    pub delta: i64,
    /// Absolute session count in the most recent 7-day window.
    pub recent_sessions: i64,
}

/// A co-occurrence entry for a skill — the tag it appears with and how often.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct CoOccurrenceSummary {
    pub tag: String,
    pub co_occurrence: i64,
}

/// A `SkillNode` enriched with velocity and co-occurrence data.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SkillNodeWithVelocity {
    pub id: String,
    pub tag: String,
    pub strength: f64,
    pub last_seen: DateTime<Utc>,
    pub session_count: i64,
    pub velocity: SkillVelocity,
    /// Top co-occurring skills (up to 5), ordered by co_occurrence descending.
    pub co_occurrences: Vec<CoOccurrenceSummary>,
}

/// A topic summary entry parsed from preferences storage.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct TopicSummaryEntry {
    /// Unix timestamp in milliseconds, from the preference key.
    pub timestamp_ms: i64,
    pub summary: String,
    pub conversation_id: Option<String>,
}

impl Preferences {
    pub fn get(&self, key: &str) -> Option<&str> {
        self.0.get(key).map(String::as_str)
    }
}

/// Upsert a skill: insert on first occurrence, increment on subsequent ones.
///
/// Also appends to the `skill_events` time series (one row per tag per UTC day),
/// which is the source of truth for history, velocity, and decayed strength.
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

    let day = Utc::now().date_naive().to_string();
    conn.execute(
        "INSERT INTO skill_events (tag, day, count) VALUES (?1, ?2, 1)
         ON CONFLICT(tag, day) DO UPDATE SET count = count + 1",
        params![tag.as_str(), day],
    )?;

    let node = conn.query_row(
        "SELECT id, tag, strength, last_seen, session_count FROM skills WHERE tag = ?1",
        params![tag.as_str()],
        row_to_skill_node,
    )?;
    Ok(node)
}

/// Return a skill's node id, creating the node (as a first occurrence) only if
/// it doesn't exist yet. Unlike `upsert_skill`, an existing node is NOT
/// incremented — callers that have already counted this occurrence use this to
/// avoid double-counting strength.
fn get_or_create_skill_id(conn: &Connection, tag: &SkillTag) -> Result<String, GraphError> {
    let existing: Option<String> = conn
        .query_row(
            "SELECT id FROM skills WHERE tag = ?1",
            params![tag.as_str()],
            |r| r.get(0),
        )
        .optional()?;
    match existing {
        Some(id) => Ok(id),
        None => upsert_skill(conn, tag).map(|n| n.id),
    }
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

    // Look up node ids without re-incrementing strength: tags in this signal
    // were already upserted once by the ingest path, and an ingest with n tags
    // visits each tag in n-1 pairs.
    let from_id = get_or_create_skill_id(conn, from_tag)?;
    let to_id = get_or_create_skill_id(conn, to_tag)?;

    conn.execute(
        "INSERT INTO skill_edges (from_id, to_id, co_occurrence)
         VALUES (?1, ?2, 1)
         ON CONFLICT(from_id, to_id) DO UPDATE SET co_occurrence = co_occurrence + 1",
        params![from_id, to_id],
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

/// Delete a single preference by key.
pub fn delete_preference(conn: &Connection, key: &str) -> Result<(), GraphError> {
    conn.execute("DELETE FROM preferences WHERE key = ?1", params![key])?;
    Ok(())
}

/// Delete ALL collected data: skills, edges, events, and preferences —
/// including topic summaries, which are the most sensitive data stored.
/// Called on consent revocation.
///
/// After deleting, truncates the WAL and VACUUMs so the removed rows are not
/// recoverable from free pages or the write-ahead log. Plain `DELETE` alone
/// leaves row content in the database file.
pub fn delete_all_data(conn: &Connection) -> Result<(), GraphError> {
    conn.execute_batch(
        "DELETE FROM skill_edges;
         DELETE FROM skills;
         DELETE FROM skill_events;
         DELETE FROM preferences;",
    )?;
    // Best-effort scrub: checkpoint failure (e.g. another connection holds a
    // read txn) must not turn a successful delete into an error.
    let _ = conn.execute_batch("PRAGMA wal_checkpoint(TRUNCATE);");
    let _ = conn.execute_batch("VACUUM;");
    Ok(())
}

/// A single entry from the audit log.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct AuditEntry {
    pub event: String,
    pub detail: Option<String>,
    pub occurred_at: String,
}

/// Return the most recent `limit` audit log entries, newest first.
pub fn get_audit_log(conn: &Connection, limit: usize) -> Result<Vec<AuditEntry>, GraphError> {
    let mut stmt = conn.prepare(
        "SELECT event, detail, occurred_at
         FROM audit_log
         ORDER BY occurred_at DESC
         LIMIT ?1",
    )?;
    let entries = stmt
        .query_map(params![limit as i64], |row| {
            Ok(AuditEntry {
                event: row.get(0)?,
                detail: row.get(1)?,
                occurred_at: row.get(2)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(entries)
}

/// A snapshot of skill activity for a single ISO week.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct WeeklySnapshot {
    /// ISO week label, e.g. "2026-W15".
    pub week: String,
    /// Top skill tags (no prefix) active during this week, by activity count.
    pub top_tags: Vec<String>,
    /// Total skill occurrences recorded during this week.
    pub total_sessions: i64,
}

/// Return per-week skill activity for the last `weeks` weeks.
///
/// Built from the append-only `skill_events` time series, so a skill appears
/// in every week it was actually used — using a skill again never erases it
/// from past weeks. Returns weeks in ascending order so the UI can render a
/// left-to-right timeline.
pub fn get_skill_history(
    conn: &Connection,
    weeks: usize,
) -> Result<Vec<WeeklySnapshot>, GraphError> {
    let cutoff_day = (Utc::now() - chrono::Duration::weeks(weeks as i64))
        .date_naive()
        .to_string();

    let mut stmt = conn.prepare(
        "SELECT tag, day, count
         FROM skill_events
         WHERE day >= ?1
           AND tag NOT LIKE 'wt:%'
           AND tag NOT LIKE 'dt:%'
           AND tag NOT LIKE 'tool:%'",
    )?;

    let rows: Vec<(String, String, i64)> = stmt
        .query_map(params![cutoff_day], |row| {
            Ok((row.get(0)?, row.get(1)?, row.get(2)?))
        })?
        .collect::<Result<Vec<_>, _>>()?;

    use std::collections::BTreeMap;
    let mut by_week: BTreeMap<String, (HashMap<String, i64>, i64)> = BTreeMap::new();

    for (tag, day, count) in rows {
        let week = iso_week_from_timestamp(&day);
        let entry = by_week.entry(week).or_default();
        *entry.0.entry(tag).or_insert(0) += count;
        entry.1 += count;
    }

    let snapshots = by_week
        .into_iter()
        .map(|(week, (tag_counts, total_sessions))| {
            let mut tags: Vec<(String, i64)> = tag_counts.into_iter().collect();
            tags.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
            WeeklySnapshot {
                week,
                top_tags: tags.into_iter().take(5).map(|(t, _)| t).collect(),
                total_sessions,
            }
        })
        .collect();

    Ok(snapshots)
}

/// Derive an ISO week label ("YYYY-WNN") from an RFC3339 timestamp string.
fn iso_week_from_timestamp(ts: &str) -> String {
    use chrono::{Datelike, NaiveDate};
    let date_part = ts.get(..10).unwrap_or("1970-01-01");
    if let Ok(d) = date_part.parse::<NaiveDate>() {
        format!("{}-W{:02}", d.iso_week().year(), d.iso_week().week())
    } else {
        "unknown".into()
    }
}

/// Compute velocity for the top `limit` skills using a 7-day rolling window
/// over the `skill_events` time series.
pub fn get_skill_velocities(
    conn: &Connection,
    limit: usize,
) -> Result<Vec<SkillVelocity>, GraphError> {
    let now = Utc::now();
    let seven_days_ago = (now - chrono::Duration::days(7)).date_naive().to_string();
    let fourteen_days_ago = (now - chrono::Duration::days(14)).date_naive().to_string();

    let mut stmt = conn.prepare(
        "SELECT s.tag,
            COALESCE(SUM(CASE WHEN e.day >= ?1 THEN e.count ELSE 0 END), 0) AS recent,
            COALESCE(SUM(CASE WHEN e.day < ?1 AND e.day >= ?2
                              THEN e.count ELSE 0 END), 0) AS prior
         FROM skills s
         LEFT JOIN skill_events e ON e.tag = s.tag
         GROUP BY s.tag
         ORDER BY s.strength DESC
         LIMIT ?3",
    )?;

    let velocities = stmt
        .query_map(
            params![seven_days_ago, fourteen_days_ago, limit as i64],
            |row| {
                let tag: String = row.get(0)?;
                let recent: i64 = row.get(1)?;
                let prior: i64 = row.get(2)?;
                Ok((tag, recent, prior))
            },
        )?
        .collect::<Result<Vec<_>, _>>()?
        .into_iter()
        .map(|(tag, recent, prior)| {
            let direction = compute_direction(recent, prior);
            SkillVelocity {
                tag,
                direction,
                delta: recent - prior,
                recent_sessions: recent,
            }
        })
        .collect();

    Ok(velocities)
}

/// Half-life (days) for recency-weighted strength.
const DECAY_HALF_LIFE_DAYS: f64 = 30.0;
/// Events older than this contribute negligibly and are excluded.
const DECAY_WINDOW_DAYS: i64 = 90;

/// Recency-weighted strength per tag: each event contributes
/// `count * 0.5^(age_days / 30)` over a 90-day window.
///
/// Unlike lifetime `strength`, this decays when a skill goes unused, so it can
/// express "what you're improving at" rather than "what you've done most ever".
pub fn get_recent_strengths(conn: &Connection) -> Result<HashMap<String, f64>, GraphError> {
    let today = Utc::now().date_naive();
    let cutoff_day = (today - chrono::Duration::days(DECAY_WINDOW_DAYS)).to_string();

    let mut stmt = conn.prepare("SELECT tag, day, count FROM skill_events WHERE day >= ?1")?;
    let rows: Vec<(String, String, i64)> = stmt
        .query_map(params![cutoff_day], |row| {
            Ok((row.get(0)?, row.get(1)?, row.get(2)?))
        })?
        .collect::<Result<Vec<_>, _>>()?;

    let mut strengths: HashMap<String, f64> = HashMap::new();
    for (tag, day, count) in rows {
        let age_days = day
            .parse::<chrono::NaiveDate>()
            .map(|d| (today - d).num_days().max(0))
            .unwrap_or(DECAY_WINDOW_DAYS);
        let weight = 0.5_f64.powf(age_days as f64 / DECAY_HALF_LIFE_DAYS);
        *strengths.entry(tag).or_insert(0.0) += count as f64 * weight;
    }
    Ok(strengths)
}

fn compute_direction(recent: i64, prior: i64) -> VelocityDirection {
    if prior == 0 && recent > 0 {
        VelocityDirection::New
    } else if prior == 0 {
        VelocityDirection::Stable
    } else if recent > (prior as f64 * 1.2) as i64 {
        VelocityDirection::Accelerating
    } else if recent < (prior as f64 * 0.8) as i64 {
        VelocityDirection::Declining
    } else {
        VelocityDirection::Stable
    }
}

/// Return the top co-occurring skills for a given skill ID (up to `limit`).
pub fn get_top_co_occurrences(
    conn: &Connection,
    skill_id: &str,
    limit: usize,
) -> Result<Vec<CoOccurrenceSummary>, GraphError> {
    let mut stmt = conn.prepare(
        "SELECT CASE WHEN e.from_id = ?1 THEN s2.tag ELSE s1.tag END AS other_tag,
                e.co_occurrence
         FROM skill_edges e
         JOIN skills s1 ON s1.id = e.from_id
         JOIN skills s2 ON s2.id = e.to_id
         WHERE e.from_id = ?1 OR e.to_id = ?1
         ORDER BY e.co_occurrence DESC
         LIMIT ?2",
    )?;

    let entries = stmt
        .query_map(params![skill_id, limit as i64], |row| {
            Ok(CoOccurrenceSummary {
                tag: row.get(0)?,
                co_occurrence: row.get(1)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;

    Ok(entries)
}

/// Return the top `limit` skills enriched with velocity and co-occurrence data.
pub fn get_skills_with_velocity(
    conn: &Connection,
    limit: usize,
) -> Result<Vec<SkillNodeWithVelocity>, GraphError> {
    let base_skills = get_top_skills(conn, limit)?;
    let velocities = get_skill_velocities(conn, limit)?;

    // Build a velocity lookup by tag for O(1) access.
    let vel_map: HashMap<String, SkillVelocity> =
        velocities.into_iter().map(|v| (v.tag.clone(), v)).collect();

    let mut result = Vec::with_capacity(base_skills.len());
    for node in base_skills {
        let velocity = vel_map.get(&node.tag).cloned().unwrap_or(SkillVelocity {
            tag: node.tag.clone(),
            direction: VelocityDirection::New,
            delta: 0,
            recent_sessions: 0,
        });
        let co_occurrences = get_top_co_occurrences(conn, &node.id, 5)?;
        result.push(SkillNodeWithVelocity {
            id: node.id,
            tag: node.tag,
            strength: node.strength,
            last_seen: node.last_seen,
            session_count: node.session_count,
            velocity,
            co_occurrences,
        });
    }

    Ok(result)
}

/// Return all topic summaries from preferences, newest first.
///
/// Parses keys of the form `topic_summary:<timestamp_ms>` or
/// `topic_summary:<timestamp_ms>:<conversation_id>`.
pub fn get_topic_summaries(conn: &Connection) -> Result<Vec<TopicSummaryEntry>, GraphError> {
    let prefs = get_preferences(conn)?;

    let mut entries: Vec<TopicSummaryEntry> = prefs
        .0
        .iter()
        .filter(|(k, _)| k.starts_with("topic_summary:"))
        .filter_map(|(k, v)| {
            let rest = k.strip_prefix("topic_summary:")?;
            let mut parts = rest.splitn(2, ':');
            let ts_str = parts.next()?;
            let timestamp_ms = ts_str.parse::<i64>().ok()?;
            let conversation_id = parts.next().map(|s| s.to_string());
            Some(TopicSummaryEntry {
                timestamp_ms,
                summary: v.clone(),
                conversation_id,
            })
        })
        .collect();

    entries.sort_by(|a, b| b.timestamp_ms.cmp(&a.timestamp_ms));
    Ok(entries)
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
    fn delete_all_data_clears_nodes_edges_events_and_preferences() {
        let conn = fresh_conn();
        let a = SkillTag::new("rust");
        let b = SkillTag::new("async");
        upsert_skill(&conn, &a).unwrap();
        upsert_skill(&conn, &b).unwrap();
        record_co_occurrence(&conn, &a, &b).unwrap();
        set_preference(&conn, "topic_summary:1000", "sensitive summary").unwrap();

        delete_all_data(&conn).unwrap();

        let skills = get_top_skills(&conn, 10).unwrap();
        assert!(skills.is_empty());
        for table in &["skill_edges", "skill_events", "preferences"] {
            let count: i64 = conn
                .query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |r| r.get(0))
                .unwrap();
            assert_eq!(count, 0, "{table} should be empty after delete_all_data");
        }
    }

    #[test]
    fn record_co_occurrence_does_not_inflate_strength() {
        let conn = fresh_conn();
        let a = SkillTag::new("rust");
        let b = SkillTag::new("async");
        // Ingest path: each tag upserted exactly once, then edges recorded.
        upsert_skill(&conn, &a).unwrap();
        upsert_skill(&conn, &b).unwrap();
        record_co_occurrence(&conn, &a, &b).unwrap();

        let top = get_top_skills(&conn, 10).unwrap();
        for node in top {
            assert_eq!(
                node.session_count, 1,
                "edge recording must not re-count '{}'",
                node.tag
            );
            assert!((node.strength - 1.0).abs() < f64::EPSILON);
        }
    }

    #[test]
    fn get_top_skills_limit_zero_returns_empty() {
        let conn = fresh_conn();
        upsert_skill(&conn, &SkillTag::new("rust")).unwrap();
        let top = get_top_skills(&conn, 0).unwrap();
        assert!(top.is_empty());
    }

    #[test]
    fn get_top_skills_on_empty_table_returns_empty() {
        let conn = fresh_conn();
        let top = get_top_skills(&conn, 10).unwrap();
        assert!(top.is_empty());
    }

    #[test]
    fn delete_preference_nonexistent_key_is_ok() {
        let conn = fresh_conn();
        // Deleting a key that does not exist must not error.
        delete_preference(&conn, "no_such_key").unwrap();
    }

    #[test]
    fn delete_all_data_on_empty_table_is_ok() {
        let conn = fresh_conn();
        // Calling on an already-empty graph must be idempotent.
        delete_all_data(&conn).unwrap();
        delete_all_data(&conn).unwrap();
    }

    #[test]
    fn malformed_datetime_falls_back_gracefully() {
        let conn = fresh_conn();
        // Insert a skill with a deliberately invalid last_seen value.
        conn.execute(
            "INSERT INTO skills (id, tag, strength, last_seen, session_count)
             VALUES ('bad-id', 'rust', 1.0, 'not-a-valid-date', 1)",
            [],
        )
        .unwrap();
        // row_to_skill_node should fall back to Utc::now() rather than panicking.
        let nodes = get_top_skills(&conn, 10).unwrap();
        assert_eq!(nodes.len(), 1);
        assert_eq!(nodes[0].tag, "rust");
    }

    #[test]
    fn set_and_delete_preference_roundtrip() {
        let conn = fresh_conn();
        set_preference(&conn, "key", "value").unwrap();
        let prefs = get_preferences(&conn).unwrap();
        assert_eq!(prefs.get("key"), Some("value"));
        delete_preference(&conn, "key").unwrap();
        let prefs_after = get_preferences(&conn).unwrap();
        assert!(prefs_after.get("key").is_none());
    }

    /// Insert a raw event row on a backdated day (test helper).
    fn insert_event(conn: &Connection, tag: &str, days_ago: i64, count: i64) {
        let day = (Utc::now() - chrono::Duration::days(days_ago))
            .date_naive()
            .to_string();
        conn.execute(
            "INSERT INTO skill_events (tag, day, count) VALUES (?1, ?2, ?3)
             ON CONFLICT(tag, day) DO UPDATE SET count = count + excluded.count",
            params![tag, day, count],
        )
        .unwrap();
    }

    #[test]
    fn upsert_skill_accumulates_same_day_events() {
        let conn = fresh_conn();
        let tag = SkillTag::new("rust");
        upsert_skill(&conn, &tag).unwrap();
        upsert_skill(&conn, &tag).unwrap();
        let (rows, count): (i64, i64) = conn
            .query_row(
                "SELECT COUNT(*), COALESCE(SUM(count), 0) FROM skill_events WHERE tag = 'rust'",
                [],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap();
        assert_eq!(rows, 1, "same-day events share one row");
        assert_eq!(count, 2, "count accumulates per occurrence");
    }

    #[test]
    fn velocity_new_when_no_prior_data() {
        let conn = fresh_conn();
        // upsert_skill records today's event automatically.
        upsert_skill(&conn, &SkillTag::new("rust")).unwrap();
        let velocities = get_skill_velocities(&conn, 10).unwrap();
        assert_eq!(velocities.len(), 1);
        assert_eq!(velocities[0].direction, VelocityDirection::New);
        assert_eq!(velocities[0].recent_sessions, 1);
    }

    #[test]
    fn velocity_stable_when_no_events_in_window() {
        let conn = fresh_conn();
        // Skill row without any events (e.g. activity older than the window).
        conn.execute(
            "INSERT INTO skills (id, tag, strength, last_seen, session_count)
             VALUES ('id-1', 'rust', 5.0, '2020-01-01T00:00:00Z', 5)",
            [],
        )
        .unwrap();
        let velocities = get_skill_velocities(&conn, 10).unwrap();
        assert_eq!(velocities[0].direction, VelocityDirection::Stable);
    }

    #[test]
    fn velocity_accelerating_when_recent_exceeds_prior() {
        let conn = fresh_conn();
        upsert_skill(&conn, &SkillTag::new("rust")).unwrap(); // 1 event today
        insert_event(&conn, "rust", 2, 5); // recent window
        insert_event(&conn, "rust", 10, 2); // prior window
        let velocities = get_skill_velocities(&conn, 10).unwrap();
        assert_eq!(velocities[0].direction, VelocityDirection::Accelerating);
        assert_eq!(velocities[0].recent_sessions, 6);
    }

    #[test]
    fn skill_history_keeps_skill_in_past_weeks_after_reuse() {
        let conn = fresh_conn();
        // Activity three weeks ago AND today — both weeks must appear.
        insert_event(&conn, "rust", 21, 3);
        upsert_skill(&conn, &SkillTag::new("rust")).unwrap();

        let history = get_skill_history(&conn, 8).unwrap();
        let weeks_with_rust = history
            .iter()
            .filter(|w| w.top_tags.contains(&"rust".to_string()))
            .count();
        assert!(
            weeks_with_rust >= 2,
            "reusing a skill must not erase it from past weeks: {history:?}"
        );
        let total: i64 = history.iter().map(|w| w.total_sessions).sum();
        assert_eq!(total, 4, "total_sessions must sum real occurrence counts");
    }

    #[test]
    fn skill_history_excludes_prefixed_tags() {
        let conn = fresh_conn();
        upsert_skill(&conn, &SkillTag::new("rust")).unwrap();
        upsert_skill(&conn, &SkillTag::new("wt:debugging")).unwrap();
        upsert_skill(&conn, &SkillTag::new("dt:medicine")).unwrap();
        upsert_skill(&conn, &SkillTag::new("tool:claude")).unwrap();

        let history = get_skill_history(&conn, 8).unwrap();
        for week in &history {
            for tag in &week.top_tags {
                assert!(
                    !tag.contains(':'),
                    "prefixed tag '{tag}' leaked into history"
                );
            }
        }
    }

    #[test]
    fn recent_strengths_decay_older_activity() {
        let conn = fresh_conn();
        insert_event(&conn, "old_skill", 60, 10);
        insert_event(&conn, "new_skill", 0, 10);
        let strengths = get_recent_strengths(&conn).unwrap();
        let old = strengths["old_skill"];
        let new = strengths["new_skill"];
        assert!(
            old < new,
            "equal activity counts must weigh less when older (old={old}, new={new})"
        );
        // 60 days at 30-day half-life → 25% weight.
        assert!((old - 2.5).abs() < 0.01, "expected ~2.5, got {old}");
        assert!((new - 10.0).abs() < f64::EPSILON);
    }

    #[test]
    fn get_skills_with_velocity_includes_co_occurrences() {
        let conn = fresh_conn();
        let a = SkillTag::new("rust");
        let b = SkillTag::new("async");
        upsert_skill(&conn, &a).unwrap();
        upsert_skill(&conn, &b).unwrap();
        record_co_occurrence(&conn, &a, &b).unwrap();
        let skills = get_skills_with_velocity(&conn, 10).unwrap();
        let rust = skills.iter().find(|s| s.tag == "rust").unwrap();
        assert!(!rust.co_occurrences.is_empty());
        assert_eq!(rust.co_occurrences[0].tag, "async");
    }

    #[test]
    fn get_topic_summaries_parses_conversation_id() {
        let conn = fresh_conn();
        set_preference(&conn, "topic_summary:1000", "first summary").unwrap();
        set_preference(&conn, "topic_summary:2000:conv-abc", "second summary").unwrap();
        let entries = get_topic_summaries(&conn).unwrap();
        assert_eq!(entries.len(), 2);
        // Newest first (timestamp_ms descending).
        assert_eq!(entries[0].timestamp_ms, 2000);
        assert_eq!(entries[0].conversation_id.as_deref(), Some("conv-abc"));
        assert_eq!(entries[1].conversation_id, None);
    }

    #[test]
    fn get_topic_summaries_empty_returns_empty() {
        let conn = fresh_conn();
        let entries = get_topic_summaries(&conn).unwrap();
        assert!(entries.is_empty());
    }
}
