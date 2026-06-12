# Strata — CLAUDE.md

> Privacy-first local intelligence layer. Rust MCP server + Tauri desktop app.
> Raw prompts and private content NEVER leave the device.

@ARCHITECTURE.md
@MVP.md

---

## What This Project Is

Strata runs locally on the user's machine as an MCP server, extracting useful patterns from AI workflows without touching raw prompts or private content. It exposes derived intelligence (skills, preferences, growth signals) to AI tools like Claude and Cursor through a standard MCP interface.

---

## Tech Stack

| Layer | Technology |
|---|---|
| Core server | Rust (stable toolchain) |
| Desktop shell | Tauri v2 |
| Skill graph storage | SQLite (local; owner-only file permissions — at-rest encryption planned, see PRIVACY.md) |
| Dashboard frontend | React + TypeScript (via Tauri webview) |
| MCP protocol | Custom Rust implementation |

---

## Module Map

```
src/
├── main.rs           Entry point — bootstrap server, init graph, start Tauri
├── private_mode.rs   Privacy enforcement — consent gating, data sanitization
├── graph/            Private skill graph (nodes, edges, queries, persistence)
├── signals/          Workflow signal collection + pattern analysis
├── backfill/         Local transcript import + session-end hook capture
├── consent/          User consent management + audit log
├── server/           MCP server implementation (tool handlers, routing)
└── tools/            MCP tools exposed to AI clients
```

**Layer order** (strict, no skipping): `signals` → `graph` → `consent` → `server` → `tools`

---

## MCP Endpoints

| Endpoint | Returns |
|---|---|
| `strata://skills` | Derived skill summary (never raw content) |
| `strata://context/current` | Current session personalization context |
| `strata://preferences` | User workflow preferences |

---

## Core Invariants — NEVER VIOLATE

1. **Raw prompts and private content NEVER leave the device** — not to Strata cloud, not to any third party
2. **All data collection requires explicit user consent** — consent gates live in `src/consent/`
3. **Store only derived summaries** — raw content is processed in-memory and discarded
4. **Local processing only** — `src/` makes no outbound network calls except MCP responses to local clients
5. **Type-system enforcement** — privacy boundaries should be compile-time errors, not runtime checks

---

## Build Commands

```bash
cargo build                      # Development build
cargo build --release            # Production build
cargo test                       # Run all tests
cargo clippy -- -D warnings      # Lint (warnings = errors)
cargo fmt --check                # Format check (CI)
cargo fmt                        # Auto-format
cargo tauri dev                  # Run desktop app in development
cargo tauri build                # Package desktop app for distribution
```

---

## Code Conventions

- **No `unwrap()`** in non-test code — use `?` operator or explicit `match`/`if let`
- **Error handling**: use `thiserror` for library errors, `anyhow` for application errors
- **Async**: `tokio` runtime; async all the way down from server handlers
- **Doc comments**: all `pub` items must have Rust doc comments (`///`)
- **Test files**: every module gets a `#[cfg(test)]` block; integration tests in `tests/`
- **Privacy types**: wrap sensitive data in newtype structs to prevent accidental leakage
- **No silent failures**: log errors at the appropriate level; never swallow `Err(_)`

---

## Architecture Decisions

Decisions live in `docs/adr/`. Before implementing a new module or changing an existing interface, check if a relevant ADR exists. Create one if the decision is non-obvious.

---

## Agent Team

This project uses a defined team of AI personas. See `AGENTS.md` for the full team topology, orchestration rules, and parallel work protocol. Invoke a persona with `/persona-<name>` (e.g., `/persona-architect`).

---

## Strata Work Unit Logging

Call `strata_ingest` once per **completed work unit** — not once per conversation. Multiple calls per session are expected and correct.

**Call when you:**
- Finish implementing a feature or module
- Resolve a bug or error
- Complete a research or analysis task
- Finish a review or planning task
- Detect a significant topic shift mid-conversation (log the prior unit before switching)

**Shape** (~10 output tokens, all fields except `tool_used` are optional):

```json
{
  "tool_used": "claude-code",
  "content": "",
  "work_type": "<research|analysis|creation|debugging|review|planning>",
  "domain_tags": ["<2-3 short domain terms, e.g. rust, mcp-protocol, sqlite>"],
  "topic_summary": "<one sentence, no PII, no raw code, no file contents>",
  "friction_signals": ["<only clear cases: repeated_context|many_corrections|restarted_approach|manual_repetition|context_lost>"],
  "features_used": ["<tool capabilities exercised, snake_case: plan_mode, subagents, hooks, code_review, ...>"],
  "outcome": "<resolved|partial|unresolved>"
}
```

**Rules:**
- `content` must be empty — use the pre-classified fields only
- `domain_tags` reflects the technical domain, not user's private context
- `topic_summary` describes the type of work done, not the specifics
- Never include raw code, file paths, user data, business logic, or anything sensitive
- `friction_signals`: report only unambiguous cases (e.g. the user re-explained project context, corrected output 3+ times, or an approach was abandoned and redone); omit the field when none apply
- `outcome`: resolved = goal achieved, partial = progress but incomplete, unresolved = blocked or abandoned
- Do not wait until the end of the conversation — log each unit as it completes
