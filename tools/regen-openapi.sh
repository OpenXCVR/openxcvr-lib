#!/usr/bin/env bash
# Regenerate OpenAPI / JSON Schema bindings for one or all specs.
# Usage: tools/regen-openapi.sh [spec...]
#   spec: cmis | sff-8636 | sff-8472 | (omit for all three)
set -euo pipefail
REPO_ROOT="$(cd "$(dirname "$0")/.." && pwd)"

OUT="$REPO_ROOT/bindings/openapi/generated"

SPECS=("${@}")
if [[ ${#SPECS[@]} -eq 0 ]]; then
    SPECS=(cmis sff-8636 sff-8472)
fi

cd "$REPO_ROOT"
for spec in "${SPECS[@]}"; do
    case "$spec" in
        cmis|sff-8636|sff-8472|cmis-lt-1.0)
            echo "→ $spec → $OUT"
            cargo run -q -p openxcvr-codegen -- generate \
                --spec "$spec" \
                --lang openapi \
                --out "$OUT"
            ;;
        *) echo "Unknown spec: $spec" >&2; exit 1 ;;
    esac
done
echo "Done."
