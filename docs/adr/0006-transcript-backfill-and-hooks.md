# ADR 0006 — Local Transcript Backfill & Session-End Hook Capture

**Status:** Accepted
**Date:** 2026-06-12
**Complements:** ADR 0004 (AI-as-taxonomizer), ADR 0005 (derived friction signals)

## Context

Two product-critical gaps share one root:

1. **Cold start.** A fresh install shows an empty dashboard and asks the user
   to come back in two weeks. Nobody comes back. Time-to-value must drop from
   weeks to the first minute.
2. **Capture reliability.** The CLAUDE.md-prompted `strata_ingest` call is
   best-effort — the model can forget, sessions can end abruptly. If only a
   fraction of sessions are logged, every dashboard number lies.

Claude Code already stores complete session transcripts locally at
`~/.claude/projects/<project>/<session-id>.jsonl`. That history never leaves
the machine, and Strata runs on the same machine.

## Decision

Add `src/backfill/`: a local transcript parser with two consent-gated entry
points sharing one pipeline.

**Bulk backfill** (dashboard Setup page): scan the transcript root, parse each
not-yet-seen session, extract genuine user-prompt text in memory, and feed it
through the existing `signals::process_ingest` boundary. Only derived skill
tags are written, attributed to the day the session actually happened
(`upsert_skill_on_day`), so history, velocity, and decayed strength reflect
real dates. A populated skill map, growth timeline, and journal appear within
the first minute of install.

**Session-end hook** (`strata hook session-end`): Claude Code's `SessionEnd`
hook pipes its event JSON to the strata binary, which ingests that single
transcript deterministically. Capture no longer depends on the model
remembering to call `strata_ingest`.

Supporting mechanics:

- **`ingested_sessions` table** keys on the transcript file stem (the session
  id). Backfill and hook share it, so the two paths can never double-ingest a
  session. Consent revocation wipes it with everything else.
- **Self-report detection:** while parsing, any assistant `tool_use` block
  whose name ends in `strata_ingest` marks the session as already self-reported
  through the taxonomizer path (which has better taxonomy than keyword
  fallback). Such sessions are marked seen and skipped — the transcript path
  defers to the richer signal.
- **Parser filtering:** only `type == "user"` entries that are real typed
  prompts — tool results, subagent sidechains, meta entries, and
  harness-injected `<`-prefixed content are skipped. Per-session content is
  capped at the live-ingest limit (256 KB).
- **Hook safety:** the hook validates that `transcript_path` is a `.jsonl`
  file inside `~/.claude` before reading, and always exits 0 so a Strata
  failure can never break a Claude Code session.
- **Integrations setup** (`src-tauri/src/integrations.rs`): the dashboard
  writes `mcpServers.strata` into Claude Desktop's and Cursor's local configs
  and appends the SessionEnd hook to `~/.claude/settings.json` — atomic
  temp-file-rename writes, never touching a file that doesn't parse as JSON.
  `~/.claude.json` (the CLI's own state file) is read-only to Strata; that
  integration shows a copyable `claude mcp add` command instead.

## Privacy properties

- Raw transcript text is held in the non-serializable `RawSignal` newtype and
  consumed in-memory by the same code path as live ingest. It is never
  persisted, logged, or returned across the module boundary.
- Both entry points call `ConsentGate::check()` first; backfill records a
  `backfill_run` audit event, the hook records `skill_ingested`.
- The scan step reads file names and modification times only — no content.
- Everything happens on-device; no new network surface.

## Trade-offs

- A session resumed after its hook fired is deduped by session id, so the
  resumed portion is not re-counted (slight undercount, never overcount).
- Keyword extraction is coarser than AI pre-classification; backfilled history
  has lower taxonomy quality than taxonomizer-reported sessions. Acceptable:
  backfill is a one-time bootstrap, and self-reported sessions are preferred
  whenever they exist.
- Sessions from other tools (Cursor, Claude Desktop) have no local transcript
  store; they continue to rely on the MCP ingest path.

## Alternatives considered

- **Watch transcripts with a background daemon:** continuous filesystem
  watching adds a long-running process and permission surface for marginal
  gain over the SessionEnd hook.
- **Editing `~/.claude.json` to register the MCP server:** rejected — the CLI
  rewrites that file while running; a concurrent write could corrupt CLI
  state. Manual `claude mcp add` is safe and one-time.
- **LLM-based transcript classification:** would require a local model
  download (hardware-dependent) or violate invariant #1. Keyword extraction
  is the honest fallback (ADR 0004).
