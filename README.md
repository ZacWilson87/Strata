# Strata

[![CI](https://github.com/zacwilson87/strata/actions/workflows/ci.yml/badge.svg)](https://github.com/zacwilson87/strata/actions/workflows/ci.yml)
[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE.md)
[![Rust](https://img.shields.io/badge/rust-stable-orange.svg)](https://www.rust-lang.org)
[![MCP](https://img.shields.io/badge/MCP-2024--11--05-purple.svg)](https://modelcontextprotocol.io)

> **Alpha** — MCP server, desktop dashboard, history import, and session capture are functional.

## Your AI tools should know who you are.

Strata is a privacy-first local intelligence layer that helps AI tools understand your skills, workflow style, preferred tools, strengths, and growth over time.

It runs as a local MCP server, extracting useful patterns from your AI workflows — without sending raw prompts, private files, or sensitive data anywhere. Everything stays on your machine.

**Install once. Every AI tool gets smarter about you.**

---

## Why Strata?

- **Private by design** — raw prompts never leave the device; only derived summaries are stored
- **Starts useful in the first minute** — imports your existing Claude Code session history locally, so the dashboard is populated at install, not in two weeks
- **Cross-tool memory** — say "no emojis in commit messages" once in any tool; every connected tool follows it (`strata_set_preference`)
- **Works with tools you already use** — Claude, Cursor, Windsurf, any MCP-compatible client
- **No local model required** — delegates classification to the AI tool already running (~10 tokens per session)
- **Type-system enforced privacy** — Rust's type system prevents raw content from crossing module boundaries at compile time
- **Standard protocol** — MCP 2024-11-05 over stdio; zero config, no firewall changes

---

## What It Looks Like

After a few sessions, calling `strata/skills` returns something like:

```json
{
  "skills": [
    { "tag": "rust",          "strength": 0.91, "sessions": 47 },
    { "tag": "system-design", "strength": 0.78, "sessions": 31 },
    { "tag": "wt:debugging",  "strength": 0.85, "sessions": 39 },
    { "tag": "dt:compilers",  "strength": 0.62, "sessions": 18 }
  ],
  "summary": "Strong systems programmer with growing compiler toolchain depth. Most active in debugging and code review workflows.",
  "top_work_types": ["debugging", "analysis", "creation"]
}
```

That context is available to any MCP client — Claude, Cursor, or your own tool — so they can personalize responses without you explaining yourself every session.

---

## Quick Start

**Requirements:** Rust stable (1.84+) — install via [rustup](https://rustup.rs)

```bash
git clone https://github.com/zacwilson87/strata
cd strata
cargo build --release
```

The binary at `./target/release/strata` is a self-contained MCP server over stdio.

### One-click setup (recommended)

Run the desktop app (`cargo tauri dev`) and open the **Setup** page: it
imports your existing local history and writes the MCP configs for Claude
Desktop, Cursor, and Windsurf — plus a Claude Code session-capture hook —
with one click each.

### Connect to Claude Desktop manually

Add to `~/Library/Application Support/Claude/claude_desktop_config.json` (macOS) or `%APPDATA%\Claude\claude_desktop_config.json` (Windows):

```json
{
  "mcpServers": {
    "strata": {
      "command": "/path/to/strata/target/release/strata"
    }
  }
}
```

Restart Claude Desktop. Strata will appear in your connected MCP servers.

### Connect to Claude Code CLI

```bash
claude mcp add strata /path/to/strata/target/release/strata --scope user
```

---

## How It Works

Strata uses the AI tool you're **already running** as the classifier. At session end, the AI calls `strata/ingest` with a small pre-classified payload — no raw content, no summaries, just taxonomy:

```json
{
  "tool_used": "claude-code",
  "content": "",
  "work_type": "debugging",
  "domain_tags": ["rust", "async", "tokio"],
  "topic_summary": "Diagnosed and fixed a race condition in an async task handler"
}
```

Strata stores derived signals in a local SQLite skill graph. Raw content is discarded immediately in memory — it never reaches the graph layer.

**Data flow (strict, no skipping):**
```
signals → graph → consent → server → tools
```

See [ARCHITECTURE.md](ARCHITECTURE.md) and [docs/adr/](docs/adr/) for the full design.

---

## MCP Tools

| Tool | Description |
|---|---|
| `strata_skills` | Ranked skill list, work types, domains, derived summary |
| `strata_context` | Session-start briefing: top skills, active domains, work mix, recent topics, preferences, insights |
| `strata_preferences` | Stored user workflow preferences |
| `strata_set_preference` | Store a durable preference once; every connected tool follows it |
| `strata_ingest` | Receive signals; raw content discarded immediately |

---

## Integrating Your AI Tool

> Copy-paste rules snippets for Cursor, Windsurf, Zed, and Cline live in
> [docs/client-rules.md](docs/client-rules.md) — they cover work-unit logging,
> preference capture, and session-start context.

Strata uses the AI tool you're already running as the taxonomizer. The AI calls `strata_ingest` once per **completed work unit** — not once per conversation. A work unit is a discrete task: a bug fixed, a feature built, a research query completed, or a significant topic shift. Multiple calls per conversation are expected and correct.

Raw content never leaves your machine — Strata only receives derived taxonomy (~10–20 output tokens per work unit).

**Payload shape:**
```json
{
  "tool_used": "your-tool-name",
  "content": "",
  "work_type": "analysis",
  "domain_tags": ["food_science", "fermentation"],
  "topic_summary": "one sentence, no PII, no raw content"
}
```

`work_type` must be one of: `research`, `analysis`, `creation`, `debugging`, `review`, `planning`

**When to call:**
- You complete a feature, fix, or analysis task
- You finish answering a research question
- The topic shifts significantly mid-conversation (log the prior unit first)
- You complete a review or planning exercise


### Claude Code

Add to your `CLAUDE.md` (or `~/.claude/CLAUDE.md` for all projects):

```markdown
## Strata Work Unit Logging
Call strata_ingest once per completed work unit — when you finish a task, resolve a bug,
complete a research query, or when the topic shifts significantly. Multiple calls per
conversation are expected.
{"tool_used":"claude-code","content":"","work_type":"<type>","domain_tags":["<2-3 terms>"],"topic_summary":"<one sentence>"}
Never include raw code, file contents, or sensitive data. Do not wait until end of conversation.
```

### Claude Desktop

Add to your Claude system prompt:

```
Call strata_ingest once per completed work unit — when you finish a task, answer a research
question, resolve a problem, or when the topic changes significantly. Do not wait until the
end of the conversation.
{"tool_used":"claude","content":"","work_type":"<research|analysis|creation|debugging|review|planning>","domain_tags":["<2-3 domain terms>"],"topic_summary":"<one sentence, no PII, no raw content>"}
```

### Cursor

Add to `.cursorrules` in your project root:

```
After completing each significant task or when topic shifts, call strata_ingest via MCP:
tool_used: "cursor"
content: ""  (leave empty)
work_type: one of research/analysis/creation/debugging/review/planning
domain_tags: 2-3 short terms describing the technical domain
topic_summary: one sentence, no code, no file names, no sensitive data
Call once per work unit — multiple calls per session are correct.
```

### GitHub Copilot / Generic MCP Tool

Add to your `AGENT.md` or equivalent instructions file:

```
Strata MCP integration: call strata_ingest once per completed work unit (task, fix, analysis,
or topic shift). Multiple calls per conversation are correct and expected.
{"tool_used":"<tool-name>","content":"","work_type":"<type>","domain_tags":["<terms>"],"topic_summary":"<sentence>"}
Keep it under 20 output tokens. Never send raw content.
```

### Any MCP-compatible tool

Strata implements the MCP 2024-11-05 protocol. Any tool supporting `tools/call` can call `strata_ingest` directly. The `content` field is optional when pre-classified fields are provided. An optional `conversation_id` field groups multiple work units from the same conversation.

### Run the desktop dashboard

```bash
cargo tauri dev
```

The dashboard includes the skill map, growth timeline (momentum, weekly
strata, insights, work journal), privacy controls with the audit log and
preference manager, and the Setup page (history import + tool connections).

### Run tests

```bash
cargo test          # Rust unit + integration tests
cd ui && npm test   # React component tests
```

---

## Development

```bash
cargo build              # development build
cargo test               # unit + integration tests
cargo clippy -- -D warnings   # lint (warnings = errors)
cargo fmt                # auto-format
cargo tauri dev          # desktop dashboard
```

Storage location:

| Platform | Path |
|---|---|
| macOS | `~/Library/Application Support/Strata/strata.db` |
| Linux | `~/.local/share/strata/strata.db` |
| Windows | `%APPDATA%\Strata\strata.db` |

---

## Roadmap

- [x] MCP server (skills, context, preferences, set_preference, ingest)
- [x] Local SQLite skill graph
- [x] Privacy type system (compile-time enforcement)
- [x] Consent gate + audit log
- [x] Desktop dashboard (Tauri + React): skill map, growth, insights, privacy
- [x] Local transcript backfill — populated dashboard in the first minute
- [x] Claude Code session-end hook (deterministic capture)
- [x] One-click integrations: Claude Desktop, Cursor, Windsurf
- [x] Cross-tool preference memory (write path + session briefing)
- [ ] Weekly growth digest
- [ ] Per-client consent + at-rest encryption
- [ ] Team skill maps (Pro)
- [ ] Portable capability profile (Pro)

See [ROADMAP.md](ROADMAP.md) and [VISION.md](VISION.md) for the full plan.

---

## License

MIT — see [LICENSE.md](LICENSE.md).
