#!/usr/bin/env bash
# Harness v3 pre-tool hook shim
# Delegates to core if available, otherwise no-op
set -euo pipefail
CORE="${CLAUDE_CODE_HARNESS_CORE:-}"
if [[ -n "$CORE" && -x "$CORE" ]]; then
  exec "$CORE" pre-tool "$@"
fi
