#!/usr/bin/env bash
# Regenerate all nine binding language surfaces and the Markdown reference.
# Usage: tools/regen-all.sh [spec...]
#   spec: cmis | sff-8636 | sff-8472 | cmis-lt-1.0 | (omit for the binding trio)
# With no spec names, Markdown renders every schemas/*/_index.yaml namespace.
# Binding languages stay on CMIS, SFF-8636 and SFF-8472.
set -euo pipefail
REPO_ROOT="$(cd "$(dirname "$0")/.." && pwd)"

REQUESTED=("${@}")
SPECS=("${REQUESTED[@]}")
if [[ ${#SPECS[@]} -eq 0 ]]; then
    SPECS=(cmis sff-8636 sff-8472)
fi

# Validate the whole list before any child runs. The direct C/Go/PHP CLI
# accepts unknown specs as empty input, and C still rewrites its umbrella.
for spec in "${SPECS[@]}"; do
    case "$spec" in
        cmis|sff-8636|sff-8472|cmis-lt-1.0) ;;
        *)
            echo "Unknown spec: $spec; expected cmis | sff-8636 | sff-8472" >&2
            exit 1
            ;;
    esac
done

cd "$REPO_ROOT"
# typescript also refreshes bindings/typescript/generated/openapi/cmis.ts
# (openapi-typescript). That file is not a cargo --lang typescript output.
for lang in rust python typescript java csharp openapi markdown; do
    if [[ "$lang" == markdown && ${#REQUESTED[@]} -eq 0 ]]; then
        "$REPO_ROOT/tools/regen-markdown.sh"
    else
        "$REPO_ROOT/tools/regen-$lang.sh" "${SPECS[@]}"
    fi
done

for spec in "${SPECS[@]}"; do
    for lang in c go php; do
        case "$lang" in
            c) OUT="$REPO_ROOT/bindings/c/generated" ;;
            go) OUT="$REPO_ROOT/bindings/go/generated" ;;
            php) OUT="$REPO_ROOT/bindings/php/src/Generated" ;;
        esac
        echo "→ $spec ($lang) → $OUT"
        cargo run -q -p openxcvr-codegen -- generate \
            --spec "$spec" \
            --lang "$lang" \
            --out "$OUT"
        if [[ "$lang" == "go" ]] && command -v gofmt >/dev/null 2>&1; then
            gofmt -w "$OUT"
        fi
    done
done
echo "Done."
