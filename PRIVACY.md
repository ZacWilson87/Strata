
# PRIVACY.md

# Privacy Principles

## Local First

Strata processes activity locally whenever possible.

## User Owned

Users control what is stored, shared, or deleted.

## No Raw Prompt Uploads

Raw prompts, chats, files, and sensitive content are never required to leave the device.

## Derived Signals Only

Optional shared data includes:

- skill trends
- strengths summaries
- growth indicators
- capability metadata

## Transparent Controls

Users can:

- pause collection
- delete local history
- disable integrations
- export data
- revoke sharing

## Trust by Design

Privacy is not a feature.

It is the foundation of Strata.

## Current Limitations (honest status)

Trust requires accuracy about what is and isn't protected today:

- **No at-rest encryption yet.** The local database relies on your OS user
  account isolation and owner-only file permissions (0600 on macOS/Linux).
  Anyone with access to your OS user account can read it. Database encryption
  keyed from the OS keychain is planned.
- **Consent is global per device.** Any MCP client you configure can read the
  derived skill profile and write signals. Per-client consent is planned.
- **Topic summaries are written by your AI tool.** Strata instructs tools to
  send no PII and caps summaries at 500 characters (max 50 retained), but the
  content quality depends on the tool honoring that instruction. Summaries are
  stored locally only and are fully wiped on revocation.
- **What revocation deletes:** all skills, co-occurrence edges, activity
  events, and preferences (including topic summaries). Deleted data is
  scrubbed (`secure_delete` + WAL truncate + VACUUM), not just unlinked. The
  consent audit log is retained as the record of consent decisions.
