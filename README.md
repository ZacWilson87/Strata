# Strata

## Your AI tools should know who you are.

Strata is a privacy-first local intelligence layer that helps AI tools understand your skills, workflow style, preferred tools, strengths, and growth over time.

It runs locally on your machine through an MCP server, safely extracting useful patterns from your workflows without sending raw prompts, private files, or sensitive data to the cloud.

Install once. Every AI tool gets better.

---

## What Strata Does

Strata helps users:

- Personalize AI tools instantly
- Stop repeating preferences across tools
- Track skill growth over time
- Discover strengths and blind spots
- Improve workflows with actionable insights
- Build a portable capability profile

---

## How It Works

### On Device

- Local MCP server
- Private skill graph
- Workflow pattern analysis
- Tool usage signals
- Growth tracking

### Shared With Permission Only

- Derived summaries
- Capability indicators
- Personalized tool context
- Optional exportable profile data

Raw prompts and private content never leave the device.

---

## Example Insights

- You perform strongest in systems debugging and structured writing
- Marketing strategy work increased 28% this month
- You rely heavily on Claude + Cursor for technical workflows
- You are improving rapidly in research synthesis
- Repetitive tasks detected that could be automated

---

## Core Product Layers

1. AI Context Engine  
2. Personal Skill Graph  
3. Workflow Intelligence  
4. Portable Capability Identity  
5. Career / Opportunity Layer (future)

---

## MVP

- Desktop app
- Local MCP server
- Claude integration
- Cursor integration
- Dashboard
- Weekly growth digest

---

## Getting Started

### Requirements

- Rust stable (1.84+) — install via [rustup](https://rustup.rs)
- Node.js 18+ — for the dashboard UI

### Run the MCP server

```bash
cargo build --release
./target/release/strata
```

The server implements the MCP 2024-11-05 protocol over **stdio** (JSON-RPC 2.0, newline-delimited). It handles the standard `initialize` / `tools/list` / `tools/call` handshake, so any compliant MCP client connects automatically.

### Connect to Claude Desktop

Add to `~/Library/Application Support/Claude/claude_desktop_config.json`:

```json
{
  "mcpServers": {
    "strata": {
      "command": "/path/to/strata"
    }
  }
}
```

### Connect to Claude Code CLI

```bash
claude mcp add strata /path/to/strata --scope user
```

---

## Integrating Your AI Tool

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

Add to your project's `CLAUDE.md` (or `~/.claude/CLAUDE.md` for all projects):

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

### Run tests

```bash
cargo test          # Rust unit + integration tests
cd ui && npm test   # React component tests
```

---

## Vision

Every person using AI tools will need a private, persistent intelligence layer that software can personalize around.

Strata aims to become that layer.
