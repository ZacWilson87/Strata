/// SQLite schema migrations for the skill graph.
use rusqlite::Connection;

use super::queries::GraphError;

/// Connection-level pragmas every connection to the Strata database needs,
/// including ones opened outside `migrate` (e.g. the consent gate's): a busy
/// timeout so cross-process writes don't fail with SQLITE_BUSY, and
/// secure_delete so deleted rows are scrubbed rather than just unlinked.
pub fn apply_connection_pragmas(conn: &Connection) -> Result<(), GraphError> {
    conn.execute_batch("PRAGMA busy_timeout=5000; PRAGMA secure_delete=ON;")?;
    Ok(())
}

/// Apply all schema migrations in order. Safe to run on an existing database.
pub fn migrate(conn: &Connection) -> Result<(), GraphError> {
    apply_connection_pragmas(conn)?;
    conn.execute_batch(
        "
        PRAGMA journal_mode=WAL;
        PRAGMA foreign_keys=ON;

        CREATE TABLE IF NOT EXISTS skills (
            id           TEXT PRIMARY KEY,
            tag          TEXT NOT NULL UNIQUE,
            strength     REAL NOT NULL DEFAULT 0.0,
            last_seen    TEXT NOT NULL,
            session_count INTEGER NOT NULL DEFAULT 0
        );

        CREATE TABLE IF NOT EXISTS skill_edges (
            from_id      TEXT NOT NULL REFERENCES skills(id) ON DELETE CASCADE,
            to_id        TEXT NOT NULL REFERENCES skills(id) ON DELETE CASCADE,
            co_occurrence INTEGER NOT NULL DEFAULT 0,
            PRIMARY KEY (from_id, to_id)
        );

        CREATE TABLE IF NOT EXISTS preferences (
            key          TEXT PRIMARY KEY,
            value        TEXT NOT NULL,
            updated_at   TEXT NOT NULL
        );

        CREATE TABLE IF NOT EXISTS audit_log (
            id           INTEGER PRIMARY KEY AUTOINCREMENT,
            event        TEXT NOT NULL,
            detail       TEXT,
            occurred_at  TEXT NOT NULL
        );

        CREATE TABLE IF NOT EXISTS skill_events (
            tag   TEXT NOT NULL,
            day   TEXT NOT NULL,
            count INTEGER NOT NULL DEFAULT 0,
            PRIMARY KEY (tag, day)
        );

        CREATE INDEX IF NOT EXISTS idx_events_day ON skill_events (day);

        CREATE TABLE IF NOT EXISTS session_signals (
            id        INTEGER PRIMARY KEY AUTOINCREMENT,
            day       TEXT NOT NULL,
            tool      TEXT NOT NULL,
            work_type TEXT,
            domains   TEXT NOT NULL DEFAULT '',
            friction  TEXT NOT NULL DEFAULT '',
            features  TEXT NOT NULL DEFAULT '',
            outcome   TEXT
        );

        CREATE INDEX IF NOT EXISTS idx_session_signals_day ON session_signals (day);

        DROP TABLE IF EXISTS skill_snapshots;
        ",
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn migration_runs_on_fresh_db() {
        let conn = Connection::open_in_memory().unwrap();
        migrate(&conn).unwrap();
    }

    #[test]
    fn migration_is_idempotent() {
        let conn = Connection::open_in_memory().unwrap();
        migrate(&conn).unwrap();
        migrate(&conn).unwrap(); // second run must not fail
    }

    #[test]
    fn expected_tables_exist() {
        let conn = Connection::open_in_memory().unwrap();
        migrate(&conn).unwrap();
        for table in &[
            "skills",
            "skill_edges",
            "preferences",
            "audit_log",
            "skill_events",
            "session_signals",
        ] {
            let count: i64 = conn
                .query_row(
                    "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name=?1",
                    rusqlite::params![table],
                    |row| row.get(0),
                )
                .unwrap();
            assert_eq!(count, 1, "table {} should exist", table);
        }
    }
}
