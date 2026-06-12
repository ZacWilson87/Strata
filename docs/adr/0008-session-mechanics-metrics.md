# ADR 0008 — Objective Session Mechanics & Self-Relative Insights

**Status:** Accepted
**Date:** 2026-06-12
**Complements:** ADR 0005 (derived friction signals), ADR 0006 (transcript backfill)

## Context

After dogfooding, the founder's verdict was blunt: "I'm not gaining anything
from it. The extractions are super high level." The data agreed — the skill
graph said *you write Rust and mostly create things*, every session signal
said `resolved` with no friction, and zero insights had ever fired.

Root cause: the friction channel (ADR 0005) depends on the AI tool honestly
self-reporting problems, which is rare, while the transcript parser (ADR
0006) was discarding the richest signal available — the objective mechanics
of how sessions actually run, already sitting in local transcript files.

## Decision

### 1. Session mechanics, measured locally

The transcript parser now computes, in the same line scan it already does:

| Metric | Source |
|---|---|
| `prompts` | genuine typed user prompts |
| `assistant_turns` | assistant messages with content |
| `duration_min` | active time — inter-entry gaps over 30 min (parked session) excluded |
| `interruptions` | `[Request interrupted…` user entries |
| `tool_calls` / `tool_errors` | assistant `tool_use` blocks / `tool_result` blocks with `is_error` |
| `first_prompt_chars` / `avg_prompt_chars` | prompt lengths (lengths only, never text) |

Stored in a new `session_metrics` table keyed by session id. Metrics dedupe
**independently of tag ingestion**: self-reported sessions get mechanics too,
and sessions ingested before this layer existed get their mechanics
backfilled on the next run. Revocation wipes the table.

### 2. Self-relative insights

`compute_metric_insights` adds five rules that compare the user against
**their own baseline** — never an external norm — and always carry the
numbers in the evidence string:

- `debugging_drag` — debugging sessions cost ≥2× the prompts of other work
- `thin_first_prompt` — sessions opened below the personal median prompt
  length take ≥1.5× more back-and-forth (the prompting coach)
- `interruption_habit` — ≥30 % of sessions interrupted mid-task
- `tool_error_drag` — ≥10 % of tool calls fail (≥50 calls)
- `recent_slowdown` — last 14 days need ≥1.5× the baseline prompts

A floor of 6 measured sessions guards against firing on noise. Existing
dismissal mechanics apply unchanged.

### 3. Surfaces

- Growth page gains a **Session Mechanics** strip (sessions measured, prompts
  per session, median active time, interruptions, tool-error rate, average
  opening-prompt length).
- `strata backfill` CLI subcommand runs a headless import — same consent-gated
  path as the Setup page.
- Mechanics insights flow into `strata_context` automatically via the
  existing top-2 insights slot.

## Privacy properties

- Every metric is a count or duration; prompt *lengths* are recorded, prompt
  *text* is not. Nothing in `session_metrics` can reconstruct content.
- Same consent gate, same revocation wipe, same local-only processing.
- Baselines are self-relative, so no comparison data ever needs to exist
  outside the device.

## Consequences

- Insights can fire on day one from backfilled history instead of waiting
  weeks for self-reported friction that rarely comes.
- The friction channel (ADR 0005) remains as a complement: mechanics say
  *what* is slow; AI-reported friction can say *why*.
- Claude Code is the only mechanics source today (only documented transcript
  store — see ADR 0006 parity matrix). Other clients contribute via the
  ingest path until they expose transcripts or hooks.
