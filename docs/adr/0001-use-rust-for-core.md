# 0001 — Use Rust for Core Server Implementation

## Status

Accepted

## Context

Strata is a local MCP server that runs as a persistent background process on the user's machine. It must:

1. Run continuously without significant memory or CPU overhead
2. Process workflow signals with minimal latency
3. Enforce privacy boundaries at compile time — not runtime checks
4. Integrate with Tauri for the desktop shell
5. Ship as a single self-contained binary (no runtime dependencies)

The alternative candidates were Python (fast iteration, weak privacy guarantees), Go (good performance, weaker type system for privacy newtypes), and TypeScript/Node (high memory overhead, poor Tauri integration).

## Decision

Use **Rust (stable toolchain)** for all code in `src/`.

The Rust type system allows privacy invariants to be enforced at compile time via newtype wrappers (`RawSignal` vs `DerivedInsight`). If code attempts to pass raw user content across a module boundary, it fails to compile — not just fails a runtime check. This is the strongest possible privacy guarantee short of formal verification.

Tauri natively uses Rust for its backend, making this a zero-friction integration. The resulting binary has minimal memory footprint, suitable for always-on background operation.

## Consequences

**Easier**:
- Privacy boundaries are compile-time errors
- Single binary distribution with no runtime install required
- Native Tauri integration
- Excellent async performance via `tokio`
- Memory safety without GC pauses

**Harder**:
- Longer initial development cycles than Python/TypeScript
- Steeper onboarding for contributors unfamiliar with Rust
- Compile times are slower than interpreted languages

**Invariants that must hold**:
- All modules in `src/` must be Rust — no FFI to scripting languages in the core privacy path
- Error handling uses `thiserror` (library errors) + `anyhow` (application errors) — no `unwrap()` in non-test code
- Async runtime is `tokio` — no blocking calls on async threads

## Rust Interfaces

```rust
// Core error type pattern for each module
#[derive(Debug, thiserror::Error)]
pub enum ModuleError {
    #[error("...")]
    Variant(/* ... */),
}

// Privacy newtype pattern
pub struct RawSignal(/* private field */);
pub struct DerivedInsight(/* private field */);
```
