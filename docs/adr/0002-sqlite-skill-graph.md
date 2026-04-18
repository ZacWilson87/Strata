# ADR 0002 — SQLite for Skill Graph Storage

**Status**: Accepted

## Context

The skill graph requires persistent local storage for skill nodes, co-occurrence edges, user preferences, and the consent audit log. The storage solution must be:

- Local-only (no cloud sync — core invariant)
- Embeddable in the Rust binary with no external process
- Reliable enough for a long-running desktop app
- Query-able with typed Rust code

## Decision

Use SQLite via the `rusqlite` crate with the `bundled` feature (statically linked). The database file lives at the platform data directory (`~/Library/Application Support/Strata/strata.db` on macOS, `~/.local/share/strata/strata.db` on Linux).

### Schema

```sql
CREATE TABLE skills (
    id           TEXT PRIMARY KEY,       -- UUID v4
    tag          TEXT NOT NULL UNIQUE,   -- e.g. "rust", "async"
    strength     REAL NOT NULL DEFAULT 0.0,
    last_seen    TEXT NOT NULL,          -- RFC3339 timestamp
    session_count INTEGER NOT NULL DEFAULT 0
);

CREATE TABLE skill_edges (
    from_id      TEXT NOT NULL REFERENCES skills(id) ON DELETE CASCADE,
    to_id        TEXT NOT NULL REFERENCES skills(id) ON DELETE CASCADE,
    co_occurrence INTEGER NOT NULL DEFAULT 0,
    PRIMARY KEY (from_id, to_id)
);

CREATE TABLE preferences (
    key          TEXT PRIMARY KEY,
    value        TEXT NOT NULL,
    updated_at   TEXT NOT NULL
);

CREATE TABLE audit_log (
    id           INTEGER PRIMARY KEY AUTOINCREMENT,
    event        TEXT NOT NULL,
    detail       TEXT,
    occurred_at  TEXT NOT NULL
);
```

### Key design choices

- **WAL mode** (`PRAGMA journal_mode=WAL`) — concurrent reads don't block writes, important for the MCP server + Tauri dashboard reading simultaneously.
- **Foreign keys on** (`PRAGMA foreign_keys=ON`) — cascading deletes ensure consent revocation removes all edges when skills are deleted.
- **Skill pairs normalised** (lexicographic order) — `(async, rust)` and `(rust, async)` map to the same edge row, preventing duplicates.
- **Strength as float** — allows fractional increments and decay in future phases without a schema change.

## Consequences

**Positive:**
- Zero external dependencies; ships inside the binary
- Simple backup story (copy one file)
- Proven embeddable database with WAL for concurrency

**Negative:**
- Not suitable for multi-process concurrent writes (single writer at a time)
- No built-in encryption at rest in this version (planned for Phase 2 via SQLCipher)
- Query expressiveness limited vs. a graph-native DB (acceptable for MVP scale)
