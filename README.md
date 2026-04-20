# Strata

[![CI](https://github.com/zacwilson87/strata/actions/workflows/ci.yml/badge.svg)](https://github.com/zacwilson87/strata/actions/workflows/ci.yml)
[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE.md)
[![Rust](https://img.shields.io/badge/rust-stable-orange.svg)](https://www.rust-lang.org)
[![MCP](https://img.shields.io/badge/MCP-2024--11--05-purple.svg)](https://modelcontextprotocol.io)

> **Alpha** — core MCP server is functional. Desktop dashboard is in progress.

## Your AI tools should know who you are.

Strata is a privacy-first local intelligence layer that helps AI tools understand your skills, workflow style, preferred tools, strengths, and growth over time.

It runs as a local MCP server, extracting useful patterns from your AI workflows — without sending raw prompts, private files, or sensitive data anywhere. Everything stays on your machine.

**Install once. Every AI tool gets smarter about you.**

---

## Why Strata?

- **Private by design** — raw prompts never leave the device; only derived summaries are stored
- **Works with tools you already use** — Claude, Cursor, any MCP-compatible client
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

### Connect to Claude Desktop

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
| `strata/skills` | Ranked skill list, work types, domains, derived summary |
| `strata/context` | Current session personalization context |
| `strata/preferences` | Stored workflow preferences |
| `strata/ingest` | Receive signals; raw content discarded immediately |

---

## Integrating Your AI Tool

### Claude Code

Add to your `CLAUDE.md` (or `~/.claude/CLAUDE.md` for all projects):

```markdown
## Strata Session Logging
At the end of each work session, call strata/ingest with a brief classification.
Keep it minimal — use this shape:
{"tool_used":"claude-code","content":"","work_type":"<type>","domain_tags":["<2-3 terms>"],"topic_summary":"<one sentence>"}
Never include raw code, file contents, or sensitive data.
```

### Claude Desktop

Add to your Claude system prompt:

```
At the end of each conversation, call strata/ingest with:
{"tool_used":"claude","content":"","work_type":"<research|analysis|creation|debugging|review|planning>","domain_tags":["<2-3 domain terms>"],"topic_summary":"<one sentence summary of work type only>"}
No raw content. No PII. One call per session.
```

### Cursor

Add to `.cursorrules` in your project root:

```
After completing a significant task, call strata/ingest via MCP:
tool_used: "cursor"
content: ""  (leave empty)
work_type: one of research/analysis/creation/debugging/review/planning
domain_tags: 2-3 short terms describing the technical domain
topic_summary: one sentence, no code, no file names, no sensitive data
```

### Any MCP-compatible tool

Strata implements MCP 2024-11-05. Any tool supporting `tools/call` can call `strata/ingest` directly. The `content` field is optional when pre-classified fields are provided.

---

## Development

```bash
cargo build              # development build
cargo test               # unit + integration tests
cargo clippy -- -D warnings   # lint (warnings = errors)
cargo fmt                # auto-format
cargo tauri dev          # desktop dashboard (WIP)
```

Storage location:

| Platform | Path |
|---|---|
| macOS | `~/Library/Application Support/Strata/strata.db` |
| Linux | `~/.local/share/strata/strata.db` |
| Windows | `%APPDATA%\Strata\strata.db` |

---

## Roadmap

- [x] MCP server (skills, context, preferences, ingest)
- [x] Local SQLite skill graph
- [x] Privacy type system (compile-time enforcement)
- [x] Consent gate + audit log
- [ ] Desktop dashboard (Tauri + React)
- [ ] Weekly growth digest
- [ ] Claude and Cursor native integrations
- [ ] Team skill maps (Pro)
- [ ] Portable capability profile (Pro)

See [ROADMAP.md](ROADMAP.md) and [VISION.md](VISION.md) for the full plan.

---

## License

MIT — see [LICENSE.md](LICENSE.md).
