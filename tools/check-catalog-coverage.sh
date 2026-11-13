#!/usr/bin/env bash
# Check field-vocabulary catalog files for basic completeness signals.

set -euo pipefail

usage() {
  cat <<'USAGE'
Usage:
  tools/check-catalog-coverage.sh [file-or-directory ...]

Default target:
  docs/briefs in this repo, else $OPENXCVR_ROOT/docs/briefs

Checks markdown field-vocabulary files for:
  - Pass 2 files without field headings
  - Pass 2 files without spec citations
Warns about:
  - files over 300 lines
  - catalog files without table or field-heading structure
USAGE
}

if [[ "${1:-}" == "-h" || "${1:-}" == "--help" ]]; then
  usage
  exit 0
fi

root="$(cd "$(dirname "$0")/.." && pwd)"
targets=("$@")
if [[ ${#targets[@]} -eq 0 ]]; then
  if [[ -d "$root/docs/briefs" ]]; then
    targets=("$root/docs/briefs")
  elif [[ -n "${OPENXCVR_ROOT:-}" && -d "$OPENXCVR_ROOT/docs/briefs" ]]; then
    targets=("$OPENXCVR_ROOT/docs/briefs")
  else
    echo "ERROR: no docs/briefs in this repository; set OPENXCVR_ROOT to the checkout that holds them" >&2
    exit 2
  fi
fi

files=()
for target in "${targets[@]}"; do
  if [[ -d "$target" ]]; then
    while IFS= read -r file; do
      files+=("$file")
    done < <(find "$target" -type f -name '*field-vocabulary*.md' | sort)
  elif [[ -f "$target" ]]; then
    files+=("$target")
  else
    echo "ERROR: target not found: $target" >&2
    exit 2
  fi
done

if [[ ${#files[@]} -eq 0 ]]; then
  echo "ERROR: no field-vocabulary markdown files found." >&2
  exit 2
fi

failures=0
warnings=0

for file in "${files[@]}"; do
  line_count=$(wc -l < "$file" | tr -d ' ')
  if (( line_count > 300 )); then
    echo "WARN $file: $line_count lines exceeds 300-line target"
    warnings=$((warnings + 1))
  fi

  if [[ "$file" == *pass2* ]]; then
    if ! grep -q '^### ' "$file"; then
      echo "FAIL $file: Pass 2 file has no field headings"
      failures=$((failures + 1))
    fi
    if ! grep -Eq '(CMIS|SFF)-[0-9A-Za-z ._-]+' "$file"; then
      echo "FAIL $file: Pass 2 file has no spec citation"
      failures=$((failures + 1))
    fi
  fi

  if ! grep -Eq '^(### |\|.*\|)' "$file"; then
    echo "WARN $file: no markdown table rows or field headings found"
    warnings=$((warnings + 1))
  fi
done

if (( failures > 0 )); then
  echo "Catalog coverage check failed: $failures issue(s)." >&2
  exit 1
fi

echo "Catalog coverage check passed for ${#files[@]} file(s) with $warnings warning(s)."
