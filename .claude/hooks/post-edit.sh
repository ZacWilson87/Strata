#!/usr/bin/env bash
# Runs after every Edit or Write tool call.
# If a Rust file was modified, auto-formats and lints the workspace.
# Always exits 0 (non-blocking) — output is injected into Claude's context.

INPUT=$(cat)
FILE=$(echo "$INPUT" | python3 -c "
import sys, json
try:
    d = json.load(sys.stdin)
    print(d.get('tool_input', {}).get('file_path', ''))
except Exception:
    print('')
" 2>/dev/null)

if [[ "$FILE" == *.rs ]]; then
  cd /home/user/Strata || exit 0
  echo "--- cargo fmt ---"
  cargo fmt 2>&1 | head -20
  echo "--- cargo clippy ---"
  cargo clippy --quiet 2>&1 | head -40
fi

exit 0
