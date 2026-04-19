# ADR 0004: AI Tool as Taxonomizer for Work Type and Domain Classification

## Status

Accepted

## Context

Strata needs to classify what kind of work a user is doing (work type) and what domain they are working in (food science, physics, medicine, software engineering, etc.) in order to provide meaningful intelligence beyond simple technology keyword matching.

Three approaches were evaluated:

**Option A: Hardcoded keyword lists**
The initial implementation uses ~20 hardcoded technology keywords (rust, python, react, etc.). This covers software engineers adequately but fails completely for researchers, scientists, doctors, and anyone outside software development. A food scientist's vocabulary (fermentation, Maillard reaction, emulsification) is not in any reasonable hardcoded list, and the universe of domains is too large to enumerate.

**Option B: Local LLM inference**
Run a small model (phi-3-mini, llama-3.2-1b) on the user's device to classify signals. Hardware feasibility analysis shows ~25–40% of real-world machines (older laptops, budget devices, shared research workstations, corporate machines) cannot run even 1B-parameter models with acceptable UX. A 1B quantized model requires 4GB+ RAM and produces 100–500ms inference latency per call. Binary size increases by 600MB–1.2GB minimum. This approach excludes a large portion of the intended user base, including many researchers and domain experts who are the primary target beyond software engineers.

**Option C: Delegate to the AI tool already in use**
The user is already running an AI tool (Claude, Cursor, Copilot, etc.) that has full context of the session. That tool can classify the session at the end using ~10–20 output tokens — a negligible cost. Strata receives only the derived taxonomy (work_type, domain_tags, topic_summary), never raw content.

## Decision

Use **Option C**: the AI tool the user is already running acts as the taxonomizer.

At session end, the AI tool calls `strata/ingest` with pre-classified fields:

```json
{
  "tool_used": "claude",
  "content": "",
  "work_type": "analysis",
  "domain_tags": ["food_science", "fermentation"],
  "topic_summary": "optimizing Maillard reaction in plant-based proteins"
}
```

When these fields are present, `content` may be empty. Strata skips keyword extraction on classification dimensions and stores the AI-provided taxonomy directly using prefixed tags (`wt:` for work type, `dt:` for domain).

A structural pattern-matching fallback (question patterns, error patterns, creation patterns, etc.) runs when `work_type` is not provided, for backwards compatibility with direct API callers.

## Consequences

**Positive:**
- Works on every machine — no local model download, no hardware requirements
- Classification quality is far higher than any keyword list; the AI tool understands context
- Universally domain-agnostic — food science, physics, medicine, law, any field the user works in
- Privacy guarantee is stronger: Strata receives only derived taxonomy, not raw prompts
- Token cost is negligible: ~10–20 output tokens per session, called once
- Compatible with any MCP-capable AI tool (Claude, Cursor, Copilot, others)
- No schema migration required — prefixed tags flow through the existing `skills` table

**Negative / trade-offs:**
- Classification only happens if the AI tool is configured to call `strata/ingest` at session end. Users must add instructions to CLAUDE.md / .cursorrules / AGENT.md / system prompt for their tool.
- Quality depends on the AI tool following instructions correctly. A tool that skips the ingest call produces no classification for that session.
- The structural fallback for `work_type` is less accurate than AI classification, especially for ambiguous content.

## Alternatives Rejected

**Option A (keyword lists):** Domain vocabulary is effectively infinite. Maintaining comprehensive lists is impossible and the approach is inherently brittle for non-software domains.

**Option B (local LLM):** Excludes ~25–40% of machines. Adds 600MB–1.2GB to binary size. Introduces 100–500ms inference latency. Creates a model download step on first run that is friction for non-technical users — exactly the demographic (researchers, scientists) this feature is designed to serve.

## Implementation Notes

- `src/private_mode.rs`: Added `WorkType` enum with `from_str_loose()` and `as_tag()` helpers
- `src/signals/mod.rs`: Extended `IngestPayload` with `work_type`, `domain_tags`, `topic_summary`; added `detect_work_type()` structural fallback
- `src/tools/mod.rs`: `handle_skills` now returns categorized response with `skills`, `work_types`, `domains` buckets; `handle_ingest` stores `topic_summary` in preferences (max 10 retained)
- Documentation: `CLAUDE.md`, `README.md`, `AGENTS.md` all include per-tool integration instructions
