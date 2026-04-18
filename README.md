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

Claude will then have access to the `strata/skills`, `strata/context`, `strata/preferences`, and `strata/ingest` tools.

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
