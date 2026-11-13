#!/usr/bin/env bash
# Regenerate TypeScript bindings for one or all specs, then the hand-authored
# OpenAPI document's TypeScript types.
# Usage: tools/regen-typescript.sh [spec...]
#   spec: cmis | sff-8636 | sff-8472 | (omit for all three)
#
# Page and enum files come from openxcvr-codegen --lang typescript.
# bindings/typescript/generated/openapi/cmis.ts comes from `npm run gen:types`
# (openapi-typescript, pinned in bindings/typescript/package.json).
# tools/regen-all.sh calls this script, so tools/check-regen-equality.sh fails
# when that artifact is stale.
#
# If node_modules/.bin/openapi-typescript is missing and npm is on PATH, this
# script runs `npm install --no-audit --no-fund` in bindings/typescript.
# npm absent is a hard failure: skipping the generator would let a stale
# cmis.ts pass regeneration equality. The codegen-equality CI job must provide
# Node.js and npm (GitHub-hosted ubuntu images do); this script installs the
# pinned devDependency itself.
#
# Test overrides, not an operator mode: OPENXCVR_REGEN_TYPESCRIPT_OPENAPI_ONLY=1
# skips cargo emission. OPENXCVR_OPENAPI_SCHEMA and OPENXCVR_TS_OPENAPI_OUT
# override the paths consumed by `npm run gen:types`.
set -euo pipefail
REPO_ROOT="$(cd "$(dirname "$0")/.." && pwd)"

OUT="$REPO_ROOT/bindings/typescript/generated"

refresh_openapi_typescript_types() {
    local ts_dir="$REPO_ROOT/bindings/typescript"
    local bin="$ts_dir/node_modules/.bin/openapi-typescript"
    if [[ ! -x "$bin" ]]; then
        if ! command -v npm >/dev/null 2>&1; then
            echo "error: npm is required to regenerate bindings/typescript/generated/openapi/cmis.ts (openapi-typescript)." >&2
            echo "Install Node.js and npm. codegen-equality must have both; this script then runs npm install in bindings/typescript." >&2
            exit 1
        fi
        echo "→ npm install (bindings/typescript) — openapi-typescript is not installed"
        ( cd "$ts_dir" && npm install --no-audit --no-fund )
    fi
    echo "→ openapi-typescript → ${OPENXCVR_TS_OPENAPI_OUT:-$ts_dir/generated/openapi/cmis.ts}"
    ( cd "$ts_dir" && npm run gen:types )
}

if [[ "${OPENXCVR_REGEN_TYPESCRIPT_OPENAPI_ONLY:-}" == "1" ]]; then
    refresh_openapi_typescript_types
    exit 0
fi

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
                --lang typescript \
                --out "$OUT"
            ;;
        *) echo "Unknown spec: $spec" >&2; exit 1 ;;
    esac
done

# After cargo, so a page regen cannot leave the OpenAPI-derived artifact stale.
refresh_openapi_typescript_types
echo "Done."
