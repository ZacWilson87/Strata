# Strata Architecture

## Overview

Strata runs locally on the user's machine. It exposes derived intelligence through an MCP server (JSON-RPC 2.0 over stdio) and a Tauri desktop dashboard. Raw prompts and private content never leave the device.

```text
┌─────────────────────────────────────┐
│         AI Clients                  │
│  Claude / Cursor / Windsurf / any   │
└───────────────┬─────────────────────┘
                │ MCP (JSON-RPC 2.0 / stdio)
┌───────────────▼─────────────────────┐
│         MCP Server (src/server/)    │
│   strata_skills                     │
│   strata_context                    │
│   strata_preferences                │
│   strata_set_preference ◄── writes  │
│   strata_ingest  ◄── receives raw   │
│                      signals here   │
└───────────────┬─────────────────────┘
                │
┌───────────────▼─────────────────────┐
│       Tool Handlers (src/tools/)    │
│   Raw content consumed + discarded  │
│   Only SkillTag / DerivedSummary    │
│   returned upstream                 │
└───────────────┬─────────────────────┘
                │
┌───────────────▼─────────────────────┐
│      Consent Gate (src/consent/)    │
│   check() → blocks if paused/       │
│             revoked                 │
│   Audit log for every operation     │
└───────────────┬─────────────────────┘
                │
┌───────────────▼─────────────────────┐
│     Skill Graph (src/graph/)        │
│   SQLite (WAL mode, local file)     │
│   Nodes: skills + strength scores   │
│   Edges: co-occurrence counts       │
│   Preferences key-value store       │
└───────────────┬─────────────────────┘
                │
┌───────────────▼─────────────────────┐
│    Signal Processing (src/signals/) │
│   In-memory only — no persistence   │
│   RawSignal consumed here           │
│   Outputs: WorkflowSignal +         │
│            Vec<SkillTag>            │
└─────────────────────────────────────┘

┌─────────────────────────────────────┐
│     Tauri Desktop Shell             │
│   src-tauri/ (Rust IPC bridge)      │
│   ui/ (React + TypeScript)          │
│   Reads graph directly via IPC      │
│   Never touches raw signals         │
└─────────────────────────────────────┘
```

---

## Layer Rules

**Data flow is strictly bottom-up.** No layer may skip another.

```
signals → graph → consent → server → tools
```

- `tools/` reads from `graph/` via `consent/` — never directly from `signals/`
- `signals/` consumes `RawSignal` in-memory and returns only `WorkflowSignal` (no raw content)
- `consent/` must be checked before every graph read or write
- Revocation deletes all graph data immediately

---

## Privacy Type System

Compile-time boundaries prevent raw content from crossing module lines:

| Type | Serializable | Persistent | Crosses boundary |
|---|---|---|---|
| `RawSignal` | No | Never | No — consumed in `signals/` |
| `WorkflowSignal` | Yes | No | `signals/` → `graph/` only |
| `SkillTag` | Yes | Yes | All layers |
| `DerivedSummary` | Yes | Yes | All layers |
| `SkillNode` | Yes | Yes | `graph/` → `tools/` |
| `WorkType` | Yes | As `wt:` tag | `signals/` → `graph/` only |

### Tag Namespace Convention

Tags in the `skills` table use prefixes to separate concerns:

| Prefix | Example | Source |
|---|---|---|
| *(none)* | `rust`, `python` | Keyword extraction from content |
| `wt:` | `wt:analysis`, `wt:debugging` | Work type — AI tool or structural fallback |
| `dt:` | `dt:food_science`, `dt:fermentation` | Domain — AI tool pre-classification only |
| `tool:` | `tool:claude-code`, `tool:cursor` | Which AI tool produced the signal |

User workflow preferences live in the `preferences` table under the `pref:`
namespace (written via `strata_set_preference`); Strata's internal storage
shares the table under `topic_summary:` and `insight_dismissed:` keys.

---

## MCP Transport

JSON-RPC 2.0 over **stdio** (newline-delimited). Any MCP-capable client (Claude Desktop/Code, Cursor, Windsurf, Zed, Cline, …) spawns the `strata` binary directly — no TCP port, no firewall config needed. Protocol version: `2024-11-05`.

### Lifecycle (standard MCP handshake)

| Method | Direction | Description |
|---|---|---|
| `initialize` | client → server | Negotiate protocol version; server returns capabilities |
| `notifications/initialized` | client → server | Client confirms ready; no response sent |
| `tools/list` | client → server | Discover available tools |
| `tools/call` | client → server | Invoke a tool by name |

### Tools

| Tool name | Description |
|---|---|
| `strata_skills` | Ranked skill list + work types + domains + derived summary |
| `strata_context` | Session-start briefing: top skills, domains, work mix, recent topics, preferences, insights (ADR 0007) |
| `strata_preferences` | User workflow preferences (`pref:` namespace, set via `strata_set_preference`) |
| `strata_ingest` | Receive signals; AI tool may pre-classify; raw content discarded |
| `strata_set_preference` | Store/clear a durable user workflow preference — the cross-tool memory write path (ADR 0007) |

### AI-as-Taxonomizer Pattern

Rather than running a local model to classify work type and domain, Strata delegates classification to the AI tool the user is **already running**. That tool has full context and can produce a lightweight, accurate taxonomy at session end — costing ~10–20 output tokens.

The AI tool calls `strata_ingest` with pre-classified fields:
```json
{
  "tool_used": "claude",
  "content": "",
  "work_type": "analysis",
  "domain_tags": ["food_science", "fermentation"],
  "topic_summary": "optimizing Maillard reaction in plant-based proteins"
}
```

When these fields are present, `content` may be empty — Strata skips keyword extraction entirely. When absent (e.g. direct API calls), Strata falls back to structural pattern matching for `work_type` and keyword matching for skill tags.

This design is:
- **Universal** — works for any domain (food science, medicine, physics, software, etc.)
- **Private** — Strata receives only derived taxonomy, never raw prompts
- **Token-efficient** — one small JSON object at session end, not a summary of the conversation
- **Hardware-agnostic** — no local model download required

---

## Storage

Single SQLite file, WAL mode. Location:

| Platform | Path |
|---|---|
| macOS | `~/Library/Application Support/Strata/strata.db` |
| Linux | `~/.local/share/strata/strata.db` |
| Windows | `%APPDATA%\Strata\strata.db` |

Tables: `skills`, `skill_edges`, `skill_events`, `session_signals`, `session_metrics`, `ingested_sessions`, `preferences`, `audit_log`

The database file is restricted to owner-only permissions (0600) on Unix.
`secure_delete` is enabled, and consent revocation wipes skills, edges, events,
and preferences (including topic summaries), then truncates the WAL and VACUUMs
so deleted rows are not recoverable from the file.

---

## Module Map

```
src/
├── lib.rs            Public API surface (re-exports all modules)
├── main.rs           Binary entry point — MCP server + `hook session-end` subcommand
├── paths.rs          Data-dir/db-path resolution shared by both binaries
├── private_mode.rs   Privacy newtypes: RawSignal, DerivedSummary, SkillTag, WorkType
├── signals/          In-memory signal processing + skill/work-type/domain extraction
├── backfill/         Local transcript parser — bulk import + session-end hook (ADR 0006)
├── graph/            SQLite skill graph (schema, queries, GraphHandle)
├── consent/          ConsentGate + audit log
├── server/           MCP JSON-RPC server loop + routing
└── tools/            Tool handlers (one per MCP method)

src-tauri/            Tauri v2 desktop shell + IPC command bridge
ui/                   React + TypeScript dashboard
tests/integration/    End-to-end MCP round-trip tests
docs/adr/             Architecture Decision Records
```
