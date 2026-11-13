#!/usr/bin/env bash
# Regenerate Rust bindings for one or all specs.
# Usage: tools/regen-rust.sh [spec...]
#   spec: cmis | sff-8636 | sff-8472 | (omit for all three)
set -euo pipefail
REPO_ROOT="$(cd "$(dirname "$0")/.." && pwd)"

regen_spec() {
    local spec="$1" crate="$2"
    local out="$REPO_ROOT/crates/$crate/src/generated"
    echo "→ $spec → $out"
    cargo run -q -p openxcvr-codegen -- generate \
        --schemas "$REPO_ROOT/schemas" \
        --spec "$spec" \
        --out "$out" \
        --lang rust
    # The Rust emitter writes unformatted code; format the regenerated crate so
    # the tracked generated/ files stay rustfmt-clean and the publish-check
    # `fmt` job (cargo fmt --all -- --check) stays green after every regen.
    echo "  fmt → $crate"
    cargo fmt -p "$crate"
}

SPECS=("${@}")
if [[ ${#SPECS[@]} -eq 0 ]]; then
    SPECS=(cmis sff-8636 sff-8472)
fi

cd "$REPO_ROOT"
for spec in "${SPECS[@]}"; do
    case "$spec" in
        cmis) regen_spec cmis openxcvr-cmis ;;
        sff-8636)  regen_spec sff-8636  openxcvr-sff8636 ;;
        sff-8472)  regen_spec sff-8472  openxcvr-sff8472 ;;
        # Companion namespaces ship no Rust crate (schema + non-Rust
        # surfaces only); accept them as an explicit no-op so
        # tools/regen-all.sh <companion> regenerates every other surface.
        cmis-lt-1.0) echo "→ $spec → skipped (companion spec: no Rust crate)" ;;
        *) echo "Unknown spec: $spec" >&2; exit 1 ;;
    esac
done
echo "Done."
