#!/usr/bin/env bash
# Wave 2b — live codegen watch.
#
# Watches schemas/ and, on any change, regenerates every binding and prints a
# color diff of the generated trees so a schema edit's downstream impact is
# visible immediately.
#
# Requires: inotify-tools (`inotifywait`). Falls back to a one-shot regen+diff
# with a notice if inotifywait is absent.
#
# Usage: tools/watch-codegen.sh
set -euo pipefail
REPO_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$REPO_ROOT"

regen_all() {
    echo "→ regenerating all bindings…"
    # Rust (reuses the fmt-aware helper).
    bash tools/regen-rust.sh >/dev/null
    # Other languages: <lang>:<out-dir>.
    local targets=(
        "go:bindings/go/generated"
        "c:bindings/c/generated"
        "php:bindings/php/src/Generated"
        "openapi:bindings/openapi/generated"
        "python:bindings/python/openxcvr/_generated"
    )
    for t in "${targets[@]}"; do
        local lang="${t%%:*}" out="${t#*:}"
        [ -d "$out" ] || continue
        for spec in cmis sff-8636 sff-8472; do
            cargo run -q -p openxcvr-codegen -- generate \
                --spec "$spec" --lang "$lang" --out "$out" >/dev/null 2>&1 || true
        done
    done
    echo "→ generated-tree diff:"
    git --no-pager diff --color=always -- bindings/ 'crates/*/src/generated' || true
    echo "→ done ($(date +%T)). Watching…"
}

regen_all

if ! command -v inotifywait >/dev/null 2>&1; then
    echo "NOTE: inotifywait not found (install inotify-tools) — ran one-shot regen only." >&2
    exit 0
fi

while inotifywait -r -q -e modify,create,delete,move schemas/ >/dev/null; do
    sleep 0.3   # debounce burst writes
    regen_all
done
