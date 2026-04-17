---
name: privacy-review
description: Run a full privacy audit on staged or recent changes. Checks for data leakage, missing consent gates, raw content storage, and outbound network calls. Invoke with /privacy-review before any merge touching user data.
user-invocable: true
disable-model-invocation: false
effort: high
agent: general-purpose
---

# Privacy Review Skill

Run a full privacy audit on the current branch's changes against Strata's five core invariants.

## Invariants Under Review

1. Raw prompts and private content NEVER leave the device
2. All data collection requires explicit user consent
3. Store only derived summaries — no raw content in SQLite or files
4. Local processing only — no outbound network calls from `src/`
5. Privacy boundaries enforced at the type level (compile-time, not runtime)

## Audit Steps

### Step 1 — Identify Changed Files
```bash
git diff --name-only main...HEAD
```
Focus on files under `src/signals/`, `src/graph/`, `src/consent/`, `src/server/`, `src/tools/`.

### Step 2 — Raw Content Storage Check
Search for patterns that suggest raw content storage:
```bash
git diff main...HEAD -- 'src/**/*.rs' | grep -E '(INSERT|raw_|prompt|content|text)' | head -40
```
Flag any new SQLite inserts that store string content longer than a derived label.

### Step 3 — Consent Gate Check
Verify every new write path to the graph passes through `src/consent/`:
```bash
git diff main...HEAD -- 'src/graph/' | grep -E '(write|insert|update|save)'
```
Each write must have a corresponding consent check — trace the call chain.

### Step 4 — Network Call Check
Look for new outbound connections:
```bash
git diff main...HEAD -- 'src/**/*.rs' | grep -E '(http|reqwest|ureq|TcpStream|UdpSocket|connect)'
```
Any outbound connection from `src/` (except MCP responses to localhost) is a violation.

### Step 5 — Type Boundary Check
Verify `RawSignal` and equivalent sensitive types don't cross layer boundaries:
```bash
git diff main...HEAD -- 'src/**/*.rs' | grep -E 'RawSignal|raw_prompt|raw_content'
```
These types must only appear in `src/signals/` — never in `graph/`, `server/`, or `tools/`.

### Step 6 — Logging Safety Check
Verify no log statements emit raw user content:
```bash
git diff main...HEAD -- 'src/**/*.rs' | grep -E '(log::|tracing::|println!|eprintln!)' | head -20
```
Inspect each log call — raw prompt text, file contents, or user identifiers must not appear.

### Step 7 — MCP Response Check
For any new MCP tool implementations, verify response payloads:
- Response must only serialize types marked as safe (`DerivedInsight`, `SkillSummary`, etc.)
- Run the tool in a test context and inspect the output shape

## Reporting

Produce a summary in this format:

```
PRIVACY REVIEW REPORT
Branch: [branch name]
Date: [today]
Reviewed by: Privacy Review Skill

FINDINGS:
[List each finding with severity and location, or "None"]

VERDICT:
✅ APPROVED — no privacy violations found
❌ BLOCKED — N finding(s) must be resolved before merge
```

Severity levels:
- **Critical**: Raw content storage, outbound network call, consent bypass — block immediately
- **High**: Type boundary violation, unsafe logging — block until fixed
- **Medium**: Missing audit log entry, unclear data lifecycle — acknowledge and track

## After the Review

- Approved: proceed with commit/merge
- Blocked: surface findings to the Privacy Guardian persona or the user for resolution
