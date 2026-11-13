#!/usr/bin/env bash
# Wrapper to execute the Regeneration Equality gate.
set -euo pipefail
REPO_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
exec python3 "$REPO_ROOT/tools/check-regen-equality.py" "$@"
