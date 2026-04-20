# ADR 0003 — JSON-RPC 2.0 over stdio for MCP Transport

**Status**: Accepted

## Context

The MCP server needs a transport layer that:

- Works natively with Claude Desktop and Cursor (both support stdio MCP servers)
- Requires no TCP port management (no port conflicts, no firewall configuration)
- Keeps all communication local (no network calls — core invariant)
- Is simple to implement and test

## Decision

Use JSON-RPC 2.0 over stdio (newline-delimited). The server reads requests line-by-line from stdin and writes responses to stdout.

### Message format

Request:
```json
{"jsonrpc":"2.0","id":1,"method":"strata_skills","params":{}}
```

Response (success):
```json
{"jsonrpc":"2.0","id":1,"result":{"summary":"rust, async, sql","skills":[...]}}
```

Response (error):
```json
{"jsonrpc":"2.0","id":1,"error":{"code":-32601,"message":"Method not found: strata/unknown"}}
```

### Error codes

| Code | Meaning |
|------|---------|
| -32700 | Parse error — malformed JSON |
| -32601 | Method not found |
| -32000 | Application error (consent, graph, etc.) |

### Exposed methods

| Method | Description |
|--------|-------------|
| `strata_skills` | Returns derived skill summary + ranked skill list |
| `strata_context` | Returns current session personalization context |
| `strata_preferences` | Returns stored user workflow preferences |
| `strata_ingest` | Receives raw signals; processes in-memory; discards raw content |

## Consequences

**Positive:**
- Zero-config integration — Claude Desktop and Cursor spawn the process directly
- No port binding means no OS-level permission issues
- Trivially testable — call handler functions directly in unit tests, no server needed
- Keeps all data local by construction (no TCP socket = nothing to intercept)

**Negative:**
- Single-client model — each AI tool spawns its own process instance
- No streaming (one response per request line)
- Harder to use from non-MCP tools that expect HTTP
