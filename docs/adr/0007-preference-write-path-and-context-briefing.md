# ADR 0007 — Preference Write Path & Session-Start Context Briefing

**Status:** Accepted
**Date:** 2026-06-12
**Complements:** ADR 0004 (AI-as-taxonomizer), ADR 0006 (transcript backfill)

## Context

Strata's product promise is "your AI tools know you." Before this decision,
the read side was hollow: `strata_context` returned one sentence ("Active in:
rust, async") and `strata_preferences` served a table nothing ever wrote.
The cross-tool memory loop — state a preference once in Claude, have Cursor
respect it tomorrow — had no write path. That loop is the moat: each vendor's
memory stops at its own walls.

## Decision

### 1. `strata_set_preference` — the write path

A fifth MCP tool. The AI client calls it when the user states a durable
workflow preference:

```json
{ "key": "commit_style", "value": "never use emojis in commit messages" }
```

- Keys are validated like tags (lowercase `[a-z0-9_.-]`, ≤64 chars, colons
  rejected) and stored under the `pref:` namespace, so clients cannot write
  into Strata's internal preference storage (`topic_summary:`,
  `insight_dismissed:`).
- Values are truncated at 500 chars; at most 100 preferences are kept;
  an empty value clears the key.
- Audit logs `preference_set` with **the key only** — values may be personal
  and never reach the audit log.
- `strata_preferences` now returns only the `pref:` namespace (stripped),
  no longer the raw preferences table.
- The dashboard's Privacy page lists stored preferences with add/remove —
  the user can always see and edit exactly what the AI tools will read.

### 2. `strata_context` — a real session-start briefing

Replaces the one-line summary with a structured payload assembled entirely
from data Strata already stores:

| Field | Source |
|---|---|
| `skills` (top 8, recency-weighted) | decayed strengths over `skill_events` |
| `domains` (top 5) | `dt:` tag strengths |
| `work_mix_30d` | `session_signals` work-type counts |
| `recent_topics` (5) | stored topic summaries |
| `preferences` | the `pref:` namespace |
| `insights` (top 2) | local insights engine (ADR 0005) |
| `context` | compact prose rendering of all of the above |

The tool description instructs clients to call it once at session start and
apply the preferences. Size is deliberately capped (8/5/5/2) — the briefing
is read every session, so every entry costs the user tokens.

## Privacy properties

- No new data is collected; both tools recombine already-derived state.
- Preference writes are consent-gated, validated, size-capped, audited
  (key-only), wiped on revocation, and fully visible/editable in the UI.

## Alternatives considered

- **Free-form preference text blob:** one giant "how I work" string is easier
  to write but impossible to update incrementally or display/delete per-item.
  Keyed entries give the user item-level control (privacy requirement).
- **Letting clients write arbitrary preference keys without a namespace:**
  rejected — internal storage shares the table; namespace separation is the
  same forgery defense used for tags (`wt:`/`dt:`/`tool:`).
