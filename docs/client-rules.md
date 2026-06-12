# Connecting AI Tools to Strata

Strata speaks standard MCP, so **any MCP-capable client works**: Claude
Desktop, Claude Code, Cursor, Windsurf, Zed, Cline, and others. The
`tools/list` descriptions tell every client what each tool does and when to
call it — no client-specific setup is strictly required beyond registering
the server.

That said, clients follow their own rules files more reliably than tool
descriptions alone. Below are copy-paste snippets that mirror what Strata's
CLAUDE.md instructions do for Claude, phrased for each client's rules system.

## MCP registration

| Client | Config file | Shape |
|---|---|---|
| Claude Desktop | `~/Library/Application Support/Claude/claude_desktop_config.json` | `mcpServers.strata.command` |
| Claude Code | `claude mcp add --scope user strata -- <path-to-strata>` | managed by CLI |
| Cursor | `~/.cursor/mcp.json` | `mcpServers.strata.command` |
| Windsurf | `~/.codeium/windsurf/mcp_config.json` | `mcpServers.strata.command` |

The dashboard's **Setup** page writes these for you (except Claude Code,
which shows the one-line command).

## Portable rules snippet

Add this to your client's rules: Cursor **Settings → Rules for AI** (or a
`.cursor/rules/strata.mdc` file in projects), Windsurf
`~/.codeium/windsurf/memories/global_rules.md`, Zed `.rules`, Cline
`.clinerules`. Replace `<tool-name>` with the client's name
(`cursor`, `windsurf`, `zed`, …).

```markdown
## Strata Work Unit Logging

Call the `strata_ingest` MCP tool once per completed work unit — when you
finish a task, resolve a bug, complete a research query, or when the topic
shifts significantly. Multiple calls per conversation are expected.

Shape: {"tool_used": "<tool-name>", "content": "",
"work_type": "<research|analysis|creation|debugging|review|planning>",
"domain_tags": ["<2-3 short domain terms>"],
"topic_summary": "<one sentence, no PII, no raw code, no file contents>",
"friction_signals": ["<only clear cases: repeated_context|many_corrections|restarted_approach|manual_repetition|context_lost>"],
"outcome": "<resolved|partial|unresolved>"}

Rules: `content` must be empty; never include raw code, file paths, or
anything sensitive; report friction only for unambiguous cases.

## Strata Preference Capture

When the user states a durable workflow preference — a lasting way they want
AI tools to work, not a one-off instruction — call `strata_set_preference`
with a short stable key and the preference phrased as an instruction any AI
tool can follow. Example: "stop using emojis in commit messages" →
{"key": "commit_emoji", "value": "never use emojis in commit messages"}.
Never store one-off instructions, secrets, or personal data. An empty value
clears a key.

## Strata Session Start

At the start of a session, call `strata_context` and apply the preferences
it returns.
```

## Capture parity status

| Capability | Claude Code | Claude Desktop | Cursor | Windsurf |
|---|---|---|---|---|
| MCP tools (skills/context/preferences/ingest/set_preference) | ✅ | ✅ | ✅ | ✅ |
| Rules-driven logging | ✅ CLAUDE.md | ✅ project instructions | ✅ Rules for AI | ✅ global_rules.md |
| Transcript backfill | ✅ local JSONL (ADR 0006) | — | ⏳ no documented local transcript store | ⏳ no documented local transcript store |
| Deterministic session-end capture | ✅ SessionEnd hook | — | ⏳ no hook API | ⏳ no hook API |

Backfill and hooks are Claude Code-first because it is currently the only
client with a documented local transcript format and lifecycle hooks. For
other clients, the rules-driven `strata_ingest` path is the capture
mechanism; parity items are tracked and will be added as those clients
expose equivalent surfaces.
