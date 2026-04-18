# Strata Architecture

## Overview

Strata runs locally on the user's machine. It exposes derived intelligence through an MCP server (JSON-RPC 2.0 over stdio) and a Tauri desktop dashboard. Raw prompts and private content never leave the device.

```text
┌─────────────────────────────────────┐
│         AI Clients                  │
│   Claude Desktop  /  Cursor         │
└───────────────┬─────────────────────┘
                │ MCP (JSON-RPC 2.0 / stdio)
┌───────────────▼─────────────────────┐
│         MCP Server (src/server/)    │
│   strata/skills                     │
│   strata/context                    │
│   strata/preferences                │
│   strata/ingest  ◄── receives raw   │
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

---

## MCP Transport

JSON-RPC 2.0 over **stdio** (newline-delimited). AI clients spawn the `strata` binary directly — no TCP port, no firewall config needed.

| Method | Description |
|---|---|
| `strata/skills` | Ranked skill list + derived summary |
| `strata/context` | Current session personalization context |
| `strata/preferences` | Stored workflow preferences |
| `strata/ingest` | Receive raw signals; process in-memory; discard |

---

## Storage

Single SQLite file, WAL mode. Location:

| Platform | Path |
|---|---|
| macOS | `~/Library/Application Support/Strata/strata.db` |
| Linux | `~/.local/share/strata/strata.db` |
| Windows | `%APPDATA%\Strata\strata.db` |

Tables: `skills`, `skill_edges`, `preferences`, `audit_log`

---

## Module Map

```
src/
├── lib.rs            Public API surface (re-exports all modules)
├── main.rs           Binary entry point — opens DB, starts MCP server
├── private_mode.rs   Privacy newtypes: RawSignal, DerivedSummary, SkillTag
├── signals/          In-memory signal processing + skill extraction
├── graph/            SQLite skill graph (schema, queries, GraphHandle)
├── consent/          ConsentGate + audit log
├── server/           MCP JSON-RPC server loop + routing
└── tools/            Tool handlers (one per MCP method)

src-tauri/            Tauri v2 desktop shell + IPC command bridge
ui/                   React + TypeScript dashboard
tests/integration/    End-to-end MCP round-trip tests
docs/adr/             Architecture Decision Records
```
