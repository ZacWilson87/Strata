# ADR 0005 — Derived Friction Signals & Local Insights Engine

**Status:** Accepted
**Date:** 2026-06-11
**Extends:** ADR 0004 (AI-as-taxonomizer)

## Context

Strata's Growth feature has three layers: **activity** (momentum, decay, weekly
history), **trajectory** (how a user's relationship to domains changes over
time), and **craft** — helping users get more out of the AI tooling they
already use: tooling they're not using, prompting that's costing them
iterations, context engineering that makes every session start cold.

Coaching on craft normally requires seeing the interaction itself — but raw
prompts never reach Strata (core invariant #1). We need a signal source that
respects the privacy architecture.

## Decision

Extend the AI-as-taxonomizer pattern: the AI tool already sees the full
session, so it can emit **derived workflow judgments** at work-unit end for a
few extra output tokens. `strata_ingest` gains three optional fields:

| Field | Type | Validation |
|---|---|---|
| `friction_signals` | `string[]` | Closed whitelist: `repeated_context`, `many_corrections`, `restarted_approach`, `manual_repetition`, `context_lost`. Unknown flags dropped. Max 8. |
| `features_used` | `string[]` | Tag-sanitized (`[a-z0-9_+#.-]`, ≤64 chars). Max 8. |
| `outcome` | `string` | Closed enum: `resolved` \| `partial` \| `unresolved`. |

Validated signals are stored in a `session_signals` table (one row per work
unit that reported any signal): day, tool, work type, domains, friction flags,
features, outcome. **No freeform text** — the friction vocabulary is a closed
enum precisely so this channel cannot become a side door for content leakage.

A **local rules engine** (`graph::insights`) evaluates curated rules over a
30-day window of session signals and produces insight cards with evidence
("5 sessions this month flagged repeated context, mostly rust → capture
project context in a CLAUDE.md"). Rules have explicit thresholds, fire rarely,
and attach their evidence. Dismissals persist in `preferences`
(`insight_dismissed:<id>`) and are permanent.

## Privacy properties

- Raw prompts still never reach Strata; the AI tool sends only enum flags.
- The friction vocabulary is closed — freeform strings are dropped at the
  validation boundary in `signals::process_ingest`.
- `session_signals` is wiped by consent revocation (`delete_all_data`) along
  with everything else, and is covered by `secure_delete`/VACUUM scrubbing.
- All insight computation is local; rules ship with the binary.

## Quality bar (product constraint, enforced in review)

- Few insights, high thresholds, evidence attached — never generic advice.
- Dismissals are permanent; no re-nagging.
- Insights surface in the Growth tab and weekly digest only — never
  interruptive.
- Closing the loop is the goal: once a recommendation is adopted, the
  targeted friction signal should measurably drop; future iterations should
  surface that delta ("corrections per session down 40% since adopting plan
  mode").

## Alternatives considered

1. **Analyze raw prompts locally** — requires a local model and weakens the
   "raw content is consumed and discarded" story; rejected.
2. **Heuristics over metadata only** (session counts, durations) — too weak to
   say anything about prompting or context quality; kept as a supplement.
3. **Freeform AI "advice" field** — unbounded text storage violates the
   derived-only principle and is prompt-injectable; rejected in favor of the
   closed enum + local rules.

## Consequences

- The CLAUDE.md ingest instructions (global and project) gain the optional
  fields; older Strata binaries ignore unknown fields, so rollout is safe in
  both directions.
- Insights need a few weeks of accumulated signals before they fire — the
  schema ships ahead of the UI being interesting.
- The rules base is curated Rust code for now; if it grows, it can move to a
  data-driven local rules file without changing the architecture.
