# Architecture Decision Records

This directory contains Architecture Decision Records (ADRs) for Strata.

An ADR documents a significant design decision: what was decided, why, and what consequences follow. ADRs are the primary communication channel between the Architect persona and the rest of the agent team.

## When to Write an ADR

- Adding a new module to `src/`
- Changing a public interface (struct, enum, trait, function signature)
- Adopting a new dependency
- Making a decision that affects privacy boundaries
- Any decision that is non-obvious or that future readers might question

## File Naming

```
NNNN-short-title.md
```

Where `NNNN` is a zero-padded sequential number (0001, 0002, ...).

## Template

```markdown
# NNNN — Title

## Status
Proposed | Accepted | Deprecated | Superseded by NNNN

## Context
What problem are we solving? What constraints apply?
Include relevant invariants from CLAUDE.md.

## Decision
What did we decide, and why?
Be specific — this is the record future agents read.

## Consequences
What becomes easier? What becomes harder?
What invariants must hold because of this decision?

## Rust Interfaces
(pub types, traits, function signatures — no implementation bodies)
```

## Index

| ADR | Title | Status |
|---|---|---|
| [0001](0001-use-rust-for-core.md) | Use Rust for core server implementation | Accepted |
| [0002](0002-sqlite-skill-graph.md) | SQLite for the local skill graph | Accepted |
| [0003](0003-mcp-stdio-transport.md) | MCP over stdio transport | Accepted |
| [0004](0004-ai-as-taxonomizer.md) | AI-as-taxonomizer classification | Accepted |
| [0005](0005-derived-friction-signals.md) | Derived friction signals & local insights engine | Accepted |
| [0006](0006-transcript-backfill-and-hooks.md) | Local transcript backfill & session-end hook capture | Accepted |
| [0007](0007-preference-write-path-and-context-briefing.md) | Preference write path & context briefing | Accepted |
