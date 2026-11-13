#!/usr/bin/env bash
# Regenerate the Markdown reference from the canonical codegen IR/resolver.
# Usage: tools/regen-markdown.sh [--schemas PATH] [--out PATH] [spec...]
# With no spec names, every direct child of the schemas root that has
# _index.yaml is rendered. Nested indexes (for example cdb-commands) are not
# separate namespaces. Discovered namespaces are ordered so a spec that
# imports another is generated first; the imported spec's own run is the last
# writer of its reference, and does not keep a "reached via imports" note.
# An explicit spec list keeps the caller's order. Each spec must have an
# index; there is no allowlist. A generator error fails the run instead of
# being skipped.
# Imported schemas (e.g. sff-8024) retain their owning spec directory.
# --print-default-specs prints the binding default spec set on one line and
# exits. That set is what wrappers such as gen-docs.py select; it is not the
# no-argument reference set above.
set -euo pipefail
DEFAULT_SPECS=(cmis sff-8636 sff-8472)
if [[ "${1:-}" == "--print-default-specs" ]]; then
    echo "${DEFAULT_SPECS[*]}"
    exit 0
fi
REPO_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
SCHEMAS="$REPO_ROOT/schemas"
OUT="$REPO_ROOT/docs/reference"
SPECS=()
DISCOVERED=0
while [[ $# -gt 0 ]]; do
    case "$1" in
        --schemas|--out)
            if [[ $# -lt 2 ]]; then
                echo "Missing path for $1" >&2
                exit 1
            fi
            if [[ "$1" == --schemas ]]; then SCHEMAS="$2"; else OUT="$2"; fi
            shift 2
            ;;
        *) SPECS+=("$1"); shift ;;
    esac
done
# Resolve caller-supplied relative paths before changing to the repository.
SCHEMAS="$(realpath "$SCHEMAS")"
OUT="$(realpath -m "$OUT")"
if [[ ${#SPECS[@]} -eq 0 ]]; then
    DISCOVERED=1
    while IFS= read -r index; do
        [[ -n "$index" ]] || continue
        SPECS+=("$(basename "$(dirname "$index")")")
    done < <(find "$SCHEMAS" -mindepth 2 -maxdepth 2 -name _index.yaml | sort)
    if [[ ${#SPECS[@]} -eq 0 ]]; then
        echo "No namespaces with _index.yaml under $SCHEMAS" >&2
        exit 1
    fi
fi
for spec in "${SPECS[@]}"; do
    if [[ ! -f "$SCHEMAS/$spec/_index.yaml" ]]; then
        echo "Unknown spec: $spec (no _index.yaml under $SCHEMAS/$spec)" >&2
        exit 1
    fi
done
if [[ "$DISCOVERED" -eq 1 ]]; then
    mapfile -t SPECS < <(python3 - "$SCHEMAS" "${SPECS[@]}" <<'PY'
import sys
from pathlib import Path

root = Path(sys.argv[1])
specs = sys.argv[2:]
imports = {}
for spec in specs:
    deps = []
    in_imports = False
    for line in (root / spec / "_index.yaml").read_text(encoding="utf-8").splitlines():
        if line.startswith("imports:"):
            in_imports = True
            continue
        if not in_imports:
            continue
        if line.startswith("  - "):
            target = line.split("../", 1)[-1].split("/", 1)[0].strip().strip('"').strip("'")
            if target in specs and target not in deps:
                deps.append(target)
        elif line and not line.startswith(" "):
            in_imports = False
    imports[spec] = deps
pending = list(specs)
emitted = []
guard = 0
while pending:
    guard += 1
    if guard > len(specs) * len(specs) + 1:
        sys.exit("namespace import cycle while ordering markdown generation")
    spec = pending.pop(0)
    if any(spec in imports[other] for other in pending):
        pending.append(spec)
    else:
        emitted.append(spec)
sys.stdout.write("\n".join(emitted) + ("\n" if emitted else ""))
PY
    )
fi
cd "$REPO_ROOT"
for spec in "${SPECS[@]}"; do
    cargo run -q -p openxcvr-codegen -- generate \
        --schemas "$SCHEMAS" --spec "$spec" --lang markdown --out "$OUT"
done
