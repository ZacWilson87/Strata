# Strata (formerly Prism, henceforth ignore naming otherwise) MCP Server
## Implementation Spec v1.0 — March 2026

---

## 1. Overview

The Prism MCP server is a local process running on the user's machine that exposes structured skill context to any MCP-compatible AI tool. It is implemented in Rust, embedded within the Prism Tauri desktop client, and communicates with the local skill graph via IPC. It speaks the Model Context Protocol over stdio (primary) and HTTP/SSE (secondary, for tools that require it).

This document covers: protocol compliance, transport layer, endpoint schemas, consent enforcement, the query-as-signal pipeline, the skill graph data model, and integration guidance for tool developers.

---

## 2. Transport Layer

### 2.1 Primary: stdio

MCP's standard transport. The Prism client launches a child process per tool connection. Each tool gets an isolated stdio channel. This is the default for all integrations.

```
[AI Tool Process]
      |
  stdin/stdout (JSON-RPC 2.0)
      |
[Prism MCP Server Process]
      |
  IPC socket (Unix socket / named pipe on Windows)
      |
[Prism Skill Graph (SQLite + FAISS)]
```

### 2.2 Secondary: HTTP/SSE

For tools that cannot use stdio (e.g., web-based tools, remote agents), the MCP server optionally exposes an HTTP server on `localhost:7371`. SSE stream for server-initiated notifications. **Localhost only — never bound to 0.0.0.0.**

```
GET  http://localhost:7371/mcp          → SSE stream (server → tool)
POST http://localhost:7371/mcp          → JSON-RPC request (tool → server)
GET  http://localhost:7371/mcp/health   → liveness check
```

TLS is not used on localhost. Connections from non-localhost addresses are rejected at the socket bind level, not by firewall rule.

### 2.3 Process Isolation

Each connected tool gets its own MCP server subprocess. Subprocesses share the skill graph via the parent process IPC broker — they do not have direct filesystem access to the SQLite database. This means:

- A compromised tool cannot access another tool's session
- Permission revocation for one tool does not require restarting others
- The parent process is the single choke point for consent enforcement

---

## 3. Protocol

Prism implements **MCP spec 2025-11-05** (current stable). Full spec: [modelcontextprotocol.io](https://modelcontextprotocol.io).

### 3.1 Supported Primitives

| Primitive | Supported | Notes |
|---|---|---|
| Tools | ✅ | Primary interaction model for skill queries |
| Resources | ✅ | Skill graph exposed as browsable resources |
| Prompts | ✅ | Pre-built prompt templates using skill context |
| Sampling | ❌ | Not supported — Prism does not generate completions |
| Roots | ✅ | Skill domains as root namespaces |

### 3.2 Initialization Handshake

```jsonc
// Tool → Prism
{
  "jsonrpc": "2.0",
  "id": 1,
  "method": "initialize",
  "params": {
    "protocolVersion": "2025-11-05",
    "capabilities": { "tools": {}, "resources": {} },
    "clientInfo": {
      "name": "claude-desktop",
      "version": "1.4.2"
    }
  }
}

// Prism → Tool
{
  "jsonrpc": "2.0",
  "id": 1,
  "result": {
    "protocolVersion": "2025-11-05",
    "capabilities": {
      "tools": { "listChanged": true },
      "resources": { "subscribe": true, "listChanged": true },
      "prompts": { "listChanged": false }
    },
    "serverInfo": {
      "name": "prism-skills",
      "version": "1.0.0"
    }
  }
}
```

After initialization, Prism looks up the `clientInfo.name` against the consent matrix. If the tool is not in the matrix, it defaults to **read-only, skills-only scope** pending user approval. A system notification prompts the user to configure permissions for the new tool.

---

## 4. Tool Definitions

All skill endpoints are exposed as MCP Tools. This is the primary interaction model — tools call `tools/call` with structured arguments.

### 4.1 `prism_get_skills`

Returns the user's full skill vector or a domain-filtered subset.

```jsonc
// Definition
{
  "name": "prism_get_skills",
  "description": "Get the user's skill profile. Use this to calibrate explanation depth, choose appropriate examples, and tailor responses to the user's actual competency level. Call once per session, not per message.",
  "inputSchema": {
    "type": "object",
    "properties": {
      "domains": {
        "type": "array",
        "items": { "type": "string" },
        "description": "Optional. Filter to specific skill domains e.g. ['python', 'kubernetes', 'llm_orchestration']. Omit for full profile.",
        "default": []
      },
      "format": {
        "type": "string",
        "enum": ["full", "summary", "scores_only"],
        "default": "summary",
        "description": "full = all fields; summary = depth + recency + velocity only; scores_only = numeric vectors only"
      }
    }
  }
}

// Example call
{
  "method": "tools/call",
  "params": {
    "name": "prism_get_skills",
    "arguments": { "domains": ["python", "llm_orchestration"], "format": "summary" }
  }
}

// Example response
{
  "content": [{
    "type": "text",
    "text": "{\"skills\":{\"python\":{\"depth\":0.91,\"velocity\":\"rising\",\"last_active\":\"this_week\",\"sub_skills\":{\"async\":0.88,\"type_hints\":0.79,\"testing\":0.72}},\"llm_orchestration\":{\"depth\":0.87,\"velocity\":\"rising\",\"last_active\":\"this_week\",\"sub_skills\":{\"tool_use\":0.91,\"multi_agent\":0.83,\"rag\":0.76}}},\"collab_level\":4,\"profile_age_weeks\":34}"
  }]
}
```

**Query-as-signal:** A call to `prism_get_skills` with `domains: ["kubernetes"]` records `{tool: "cursor", domain: "kubernetes", event: "skill_query", week: "2026-W13"}` in the local signal queue. No content. No timestamp precision below week granularity.

---

### 4.2 `prism_get_context`

Returns the user's current working context — what mode of work they're in right now, inferred from recent activity.

```jsonc
// Definition
{
  "name": "prism_get_context",
  "description": "Get the user's current working context. Use to infer what kind of help is most useful right now — debugging vs. architecture vs. writing vs. learning.",
  "inputSchema": {
    "type": "object",
    "properties": {
      "include_stack": {
        "type": "boolean",
        "default": true,
        "description": "Include current tech stack inference"
      }
    }
  }
}

// Example response
{
  "content": [{
    "type": "text",
    "text": "{\"mode\":\"debugging\",\"confidence\":0.82,\"stack\":[\"python\",\"fastapi\",\"postgres\"],\"collab_level\":3,\"recent_domains\":[\"async\",\"database\",\"error_handling\"],\"session_depth\":\"mid\"}"
  }]
}
```

**`mode` enum values:** `coding`, `debugging`, `architecture`, `writing`, `learning`, `reviewing`, `planning`, `unknown`

**`session_depth` enum values:** `early` (< 10 min), `mid` (10–45 min), `deep` (> 45 min)

---

### 4.3 `prism_get_preferences`

Returns the user's inferred work style and communication preferences.

```jsonc
// Definition
{
  "name": "prism_get_preferences",
  "description": "Get the user's AI collaboration preferences and work style. Use to calibrate verbosity, explanation style, and interaction mode.",
  "inputSchema": {
    "type": "object",
    "properties": {}
  }
}

// Example response
{
  "content": [{
    "type": "text",
    "text": "{\"explanation_depth\":\"terse\",\"code_style\":\"functional\",\"review_mode\":\"critical\",\"prefers_examples\":true,\"autonomy_comfort\":\"high\",\"interrupt_tolerance\":\"low\",\"collab_persona\":\"orchestrator\"}"
  }]
}
```

**`collab_persona` enum:** `learner`, `practitioner`, `integrator`, `orchestrator`, `architect` — maps to AI Collaboration Taxonomy levels 1–5.

---

### 4.4 `prism_get_collab_level`

Returns the user's AI collaboration level with sub-dimension scores.

```jsonc
// Example response
{
  "content": [{
    "type": "text",
    "text": "{\"level\":4,\"label\":\"Agent Orchestrator\",\"dimensions\":{\"delegation\":0.88,\"verification\":0.79,\"iteration\":0.83,\"system_design\":0.71,\"tool_diversity\":0.85},\"confidence\":0.91}"
  }]
}
```

---

### 4.5 `prism_submit_signal` (write — explicit user enable required)

Allows a tool to write a structured outcome signal back to Prism. This is the highest-quality signal in the system — explicit, structured, tool-confirmed.

```jsonc
// Definition
{
  "name": "prism_submit_signal",
  "description": "Submit an outcome signal to Prism after a completed task. Only call this when a task has a clear, verifiable outcome. Do not call speculatively.",
  "inputSchema": {
    "type": "object",
    "required": ["task_type", "outcome"],
    "properties": {
      "task_type": {
        "type": "string",
        "enum": [
          "code_written", "code_debugged", "code_reviewed",
          "architecture_designed", "document_written",
          "test_written", "pipeline_built", "agent_built",
          "refactor_completed", "pr_reviewed"
        ]
      },
      "outcome": {
        "type": "string",
        "enum": ["success", "partial", "abandoned"]
      },
      "domains": {
        "type": "array",
        "items": { "type": "string" },
        "description": "Skill domains exercised. Max 5.",
        "maxItems": 5
      },
      "ai_role": {
        "type": "string",
        "enum": ["generated", "assisted", "reviewed", "delegated", "orchestrated"],
        "description": "How AI was used in this task."
      },
      "complexity": {
        "type": "string",
        "enum": ["low", "medium", "high"],
        "description": "Tool's assessment of task complexity."
      }
    }
  }
}

// Example call (from Cursor after a successful refactor)
{
  "method": "tools/call",
  "params": {
    "name": "prism_submit_signal",
    "arguments": {
      "task_type": "refactor_completed",
      "outcome": "success",
      "domains": ["python", "async", "testing"],
      "ai_role": "assisted",
      "complexity": "high"
    }
  }
}

// Response
{
  "content": [{
    "type": "text",
    "text": "{\"accepted\":true,\"queued_for_digest\":true,\"signal_id\":\"sig_7f3a2b\"}"
  }]
}
```

Submitted signals are queued locally and surface in the user's weekly digest for approval before any upstream transmission. The tool receives `accepted: true` when the signal is queued locally — not when it transmits upstream.

---

## 5. Resource Definitions

Resources expose the skill graph as a browsable namespace. Tools can subscribe to resource changes (skill updates) for session-long context freshness.

### 5.1 Resource Listing

```jsonc
// Tool → Prism
{ "method": "resources/list" }

// Prism → Tool
{
  "result": {
    "resources": [
      {
        "uri": "prism://skills",
        "name": "Full Skill Profile",
        "description": "Complete skill vector across all domains",
        "mimeType": "application/json"
      },
      {
        "uri": "prism://skills/python",
        "name": "Python Skills",
        "mimeType": "application/json"
      },
      {
        "uri": "prism://skills/llm_orchestration",
        "name": "LLM Orchestration Skills",
        "mimeType": "application/json"
      },
      {
        "uri": "prism://context/current",
        "name": "Current Work Context",
        "mimeType": "application/json"
      },
      {
        "uri": "prism://preferences",
        "name": "User Preferences",
        "mimeType": "application/json"
      }
      // ... one resource per skill domain in the graph
    ]
  }
}
```

### 5.2 Resource Read

```jsonc
{ "method": "resources/read", "params": { "uri": "prism://skills/python" } }
```

Response is identical to the `prism_get_skills` tool response for that domain.

### 5.3 Resource Subscriptions

Tools can subscribe to `prism://context/current` to receive notifications when the user's working context changes (e.g., switching from debugging to architecture mode mid-session).

```jsonc
{ "method": "resources/subscribe", "params": { "uri": "prism://context/current" } }

// Prism → Tool (notification, not a response)
{
  "method": "notifications/resources/updated",
  "params": { "uri": "prism://context/current" }
}
```

Context notifications are rate-limited to once per 5 minutes to avoid noise.

---

## 6. Prompt Templates

Prism exposes pre-built prompt templates that tools can use to inject skill context naturally.

```jsonc
{ "method": "prompts/list" }

// Response
{
  "result": {
    "prompts": [
      {
        "name": "calibrate_for_user",
        "description": "System prompt addition that calibrates the AI's behavior for this user's skill level and preferences.",
        "arguments": [
          { "name": "mode", "description": "full | brief", "required": false }
        ]
      },
      {
        "name": "skill_gap_context",
        "description": "Adds context about the user's skill gaps relative to a target role or domain.",
        "arguments": [
          { "name": "target_domain", "description": "e.g. 'kubernetes', 'ml_ops'", "required": true }
        ]
      }
    ]
  }
}
```

### 6.1 `calibrate_for_user` prompt

```jsonc
{ "method": "prompts/get", "params": { "name": "calibrate_for_user", "arguments": { "mode": "brief" } } }

// Response
{
  "result": {
    "messages": [{
      "role": "user",
      "content": {
        "type": "text",
        "text": "The user you're talking to has the following skill profile:\n- Python: expert (depth 0.91, rising)\n- LLM orchestration: advanced (depth 0.87, rising)\n- AI collaboration level: 4/5 (Agent Orchestrator)\n- Prefers terse explanations, functional code style, high autonomy\n\nCalibrate your responses accordingly. Skip basics. Don't over-explain. Treat them as a peer."
      }
    }]
  }
}
```

This is the highest-leverage integration point. A single `prompts/get` call at session start produces a calibrated system prompt addition that transforms how the tool responds for the entire session.

---

## 7. Consent Enforcement

### 7.1 Scope Definitions

```rust
pub enum ToolScope {
    SkillsRead,       // prism://skills/*, prism_get_skills
    ContextRead,      // prism://context/*, prism_get_context
    PreferencesRead,  // prism://preferences, prism_get_preferences
    CollabRead,       // prism_get_collab_level
    SignalWrite,      // prism_submit_signal
}
```

### 7.2 Consent Gate (enforced in MCP server, not skill graph)

```rust
fn check_consent(tool_name: &str, requested_scope: ToolScope) -> ConsentResult {
    let matrix = load_consent_matrix(); // reads from encrypted local config
    
    match matrix.get(tool_name) {
        None => {
            // Unknown tool — default to SkillsRead only, notify user
            notify_new_tool(tool_name);
            if requested_scope == ToolScope::SkillsRead {
                ConsentResult::Allowed
            } else {
                ConsentResult::Denied(DenialReason::ToolNotConfigured)
            }
        },
        Some(perms) => {
            if perms.allows(requested_scope) {
                ConsentResult::Allowed
            } else {
                ConsentResult::Denied(DenialReason::ScopeNotGranted)
            }
        }
    }
}
```

### 7.3 Denial Response

When a tool requests a scope it doesn't have permission for:

```jsonc
{
  "jsonrpc": "2.0",
  "id": 5,
  "error": {
    "code": -32600,
    "message": "Scope not permitted",
    "data": {
      "requested_scope": "preferences_read",
      "tool": "copilot",
      "configure_url": "prism://settings/tools/copilot"
    }
  }
}
```

The tool receives a structured error. The `configure_url` is a custom URI scheme that opens the Prism settings UI to the relevant tool configuration panel.

### 7.4 Private Mode

When Private Mode is active, all MCP endpoints return:

```jsonc
{
  "error": {
    "code": -32000,
    "message": "Prism is in Private Mode. All skill context unavailable.",
    "data": { "private_mode": true }
  }
}
```

Private Mode is implemented as an OS-level process suspend of all MCP server subprocesses. The response above is returned by the parent broker process, which remains minimally alive solely to return this error.

---

## 8. Query-as-Signal Pipeline

Every inbound MCP tool call is a passive signal about what the user is working on. This is processed by the query-as-signal pipeline within the MCP server process, before the response is returned.

### 8.1 Signal Event Schema (local only)

```rust
struct QuerySignalEvent {
    tool_name: String,           // e.g. "cursor"
    endpoint: ToolEndpoint,      // enum: GetSkills, GetContext, etc.
    domains_requested: Vec<String>, // e.g. ["python", "kubernetes"]
    week_bucket: String,         // "2026-W13" — no finer granularity
    session_id: u64,             // ephemeral per-session, not persisted
}
```

No timestamp beyond week granularity. No content. No session linking across weeks.

### 8.2 Signal → Skill Graph Update

```
QuerySignalEvent
    ↓
Domain Activity Scorer
    (domain requested by tool during work session
     → increment activity weight for that domain node in graph)
    ↓
Skill Graph Writer (IPC → parent broker → SQLite)
    ↓
Graph node updated:
    - domain.last_active_week = current_week
    - domain.query_frequency_bucket += 1  (bucketed, not exact)
```

Activity weight increments are small (0.01–0.05 per event) and decay over time. The query frequency is bucketed into five levels (never / rarely / occasionally / regularly / heavily) — exact counts are never stored.

### 8.3 What This Enables

A user who hasn't explicitly tagged any Kubernetes work will still see their Kubernetes skill node become more active if Cursor starts querying `prism://skills/kubernetes` every day. The profile self-updates from tool behavior without requiring the user to do anything.

---

## 9. Skill Graph Data Model

The local skill graph is the source of truth for all MCP responses. It lives in an encrypted SQLite database (`~/.prism/graph.db`, AES-256, key derived from device TPM). The FAISS index lives alongside it for vector similarity queries.

### 9.1 Core Tables

```sql
-- Skill domains and their current state
CREATE TABLE skill_nodes (
    domain_id       TEXT PRIMARY KEY,  -- e.g. "python", "llm_orchestration"
    depth_score     REAL NOT NULL,     -- 0.0–1.0
    velocity        TEXT NOT NULL,     -- 'rising'|'stable'|'declining'
    last_active_week TEXT NOT NULL,    -- ISO week e.g. '2026-W13'
    query_freq_bucket INTEGER NOT NULL, -- 1–5
    created_at      TEXT NOT NULL,
    updated_at      TEXT NOT NULL
);

-- Sub-skills hanging off domain nodes
CREATE TABLE skill_edges (
    parent_domain   TEXT NOT NULL REFERENCES skill_nodes(domain_id),
    sub_skill       TEXT NOT NULL,
    depth_score     REAL NOT NULL,
    updated_at      TEXT NOT NULL,
    PRIMARY KEY (parent_domain, sub_skill)
);

-- AI collaboration level history
CREATE TABLE collab_snapshots (
    week_bucket     TEXT NOT NULL,
    level           INTEGER NOT NULL,  -- 1–5
    delegation      REAL,
    verification    REAL,
    iteration       REAL,
    system_design   REAL,
    tool_diversity  REAL,
    PRIMARY KEY (week_bucket)
);

-- User preferences (inferred + explicit)
CREATE TABLE preferences (
    key             TEXT PRIMARY KEY,
    value           TEXT NOT NULL,
    source          TEXT NOT NULL,  -- 'inferred'|'explicit'
    confidence      REAL,
    updated_at      TEXT NOT NULL
);

-- Signal queue (pending user digest approval)
CREATE TABLE signal_queue (
    signal_id       TEXT PRIMARY KEY,
    source          TEXT NOT NULL,   -- 'passive'|'mcp_write'|'query'
    payload_json    TEXT NOT NULL,   -- the derived signal (no raw content)
    status          TEXT NOT NULL,   -- 'pending'|'approved'|'discarded'
    created_at      TEXT NOT NULL
);
```

### 9.2 FAISS Index

The FAISS flat index stores skill domain vectors for similarity queries. Used by:
- The MCP server to answer "what domains are most similar to X?" queries
- The local inference engine to map raw skill tags to graph nodes
- Future: local role-fit estimation before upstream transmission

Index is rebuilt from the SQLite `skill_nodes` table on each agent startup. Not persisted to disk separately — SQLite is the source of truth.

### 9.3 Purge Policy

```sql
-- Applied on configurable schedule (default: weekly)
DELETE FROM signal_queue WHERE status = 'discarded';
DELETE FROM signal_queue WHERE status = 'approved' AND created_at < datetime('now', '-30 days');

-- Skill nodes are never auto-deleted (user owns their history)
-- But velocity score decays automatically:
UPDATE skill_nodes
SET velocity = 'declining'
WHERE last_active_week < (current_week - 8)
  AND velocity != 'declining';
```

---

## 10. Tool Integration Guide

### 10.1 Recommended Integration Pattern

Tools should call `prism_get_skills` and `prism_get_preferences` once at session start, not on every message. Context is stable within a session. Over-calling wastes latency and produces excessive query signals.

```python
# Pseudocode — Python MCP client example
async def on_session_start(mcp_client):
    # Single call at session start
    try:
        skills = await mcp_client.call_tool("prism_get_skills", {"format": "summary"})
        prefs  = await mcp_client.call_tool("prism_get_preferences", {})
        
        # Inject into system prompt
        system_context = build_calibration_context(skills, prefs)
        session.prepend_system(system_context)
        
    except McpError as e:
        if e.code == PRIVATE_MODE_CODE:
            pass  # Prism in private mode — proceed without context
        elif e.code == SCOPE_NOT_PERMITTED:
            pass  # This tool doesn't have permissions — proceed without context
        else:
            raise  # Unexpected error

# Subscribe to context changes for long sessions
async def on_session_start_long(mcp_client):
    await mcp_client.subscribe_resource("prism://context/current")
    mcp_client.on_resource_updated("prism://context/current", handle_context_change)
```

### 10.2 Graceful Degradation

Tools must handle Prism being absent or in Private Mode gracefully. Prism being unavailable should never break a tool's core functionality. The integration is enhancement, not dependency.

| Condition | Expected Tool Behavior |
|---|---|
| Prism not installed | MCP server not discoverable — tool proceeds normally |
| Prism in Private Mode | `-32000` error returned — tool proceeds without context |
| Scope not granted | `-32600` error returned — tool proceeds without that data |
| Prism process crashed | stdio EOF / HTTP connection refused — tool proceeds normally |

### 10.3 Certification Program

Tool developers who want Prism integration listed in the tool registry must:

1. Implement graceful degradation per the table above
2. Not cache Prism responses beyond the current session
3. Not transmit Prism response data to external servers (context is for local use only)
4. Pass the Prism integration test suite (open source, in the `prism-mcp-sdk` repo)
5. Document which endpoints they call and why in their privacy policy

Certified tools appear in the Prism settings UI with a verified badge, making it easier for users to grant appropriate permissions confidently.

---

## 11. Rust Implementation Structure

```
prism-mcp/
├── src/
│   ├── main.rs                  # Entry point, process management
│   ├── server/
│   │   ├── mod.rs               # MCP server core
│   │   ├── stdio.rs             # stdio transport
│   │   ├── http.rs              # HTTP/SSE transport
│   │   ├── handler.rs           # JSON-RPC method dispatch
│   │   └── session.rs           # Per-tool session state
│   ├── consent/
│   │   ├── mod.rs               # Consent gate
│   │   ├── matrix.rs            # Consent matrix load/save
│   │   └── types.rs             # ToolScope, ConsentResult
│   ├── graph/
│   │   ├── mod.rs               # Skill graph interface
│   │   ├── reader.rs            # IPC read path (MCP → graph)
│   │   ├── writer.rs            # IPC write path (signals → graph)
│   │   └── schema.rs            # SQLite schema, migrations
│   ├── tools/
│   │   ├── mod.rs               # Tool definition registry
│   │   ├── get_skills.rs        # prism_get_skills impl
│   │   ├── get_context.rs       # prism_get_context impl
│   │   ├── get_preferences.rs   # prism_get_preferences impl
│   │   ├── get_collab.rs        # prism_get_collab_level impl
│   │   └── submit_signal.rs     # prism_submit_signal impl
│   ├── signals/
│   │   ├── mod.rs               # Query-as-signal pipeline
│   │   ├── emitter.rs           # Event creation
│   │   └── scorer.rs            # Domain activity weight updates
│   └── private_mode.rs          # Private mode state + OS suspend
├── tests/
│   ├── integration/             # Full MCP protocol tests
│   ├── consent/                 # Consent gate unit tests
│   └── signals/                 # Query-as-signal pipeline tests
├── Cargo.toml
└── README.md
```

---

## 12. Security Threat Model

| Threat | Mitigation |
|---|---|
| Malicious tool reads full skill profile without consent | Consent gate enforced in server before any graph read. Per-scope checks per call. |
| Tool exfiltrates Prism responses to remote server | Certification program prohibits this. Prism cannot enforce at runtime — this is a trust boundary. Logged in query audit trail (local). |
| Another local process connects to `localhost:7371` | HTTP server requires `Origin: null` header (local only). stdio connections require process spawn from Prism parent. No unauthenticated lateral access. |
| Process substitution attack (fake tool pretends to be Claude) | `clientInfo.name` is advisory only — tools are trusted at the consent matrix level. Users grant permissions to tool names they recognize. High-privilege scopes (write) require explicit user enable regardless. |
| SQLite database accessed directly by bypassing IPC | Database file permissions set to owner-only (0600). AES-256 key in TPM. Direct reads without key yield ciphertext only. |
| Private Mode bypass via IPC | Parent broker enforces Private Mode at IPC level. Subprocesses receive the frozen error response regardless of what they send. |
| Signal queue contains raw content via malicious `prism_submit_signal` call | Submit handler validates all fields against enum schemas. Free-text fields do not exist in the write endpoint. PII scanner runs on the `domains` field before queue insertion. |

---

## 13. SDK and Developer Resources

The `prism-mcp-sdk` package will be published to:

- **npm** — `@prism-skills/mcp-client` (TypeScript/JavaScript)
- **PyPI** — `prism-mcp` (Python)
- **crates.io** — `prism-mcp` (Rust)

Each SDK provides:
- Type-safe wrappers around all tool call and resource endpoints
- Graceful degradation helpers (try/catch patterns with fallback behavior)
- Session-start calibration helpers (`calibrate_session_from_prism()`)
- Integration test fixtures for certification suite

---

*Prism MCP Server Implementation Spec v1.0*
*Next: Skill taxonomy definition · Inference model fine-tuning spec · Digest UI spec*
