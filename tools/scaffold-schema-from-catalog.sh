#!/usr/bin/env bash
# Emit draft YAML-like field snippets from a field-vocabulary markdown table.
# Output is intentionally marked REVIEW and is not authoritative schema.

set -euo pipefail

usage() {
  cat <<'USAGE'
Usage:
  tools/scaffold-schema-from-catalog.sh <catalog.md> [output-file]

Reads markdown table rows with columns like:
  | Name | Loc | Size | Type | RW | Spec | Description |

The output is a draft aid only. Review every field against the source PDF.
USAGE
}

if [[ "${1:-}" == "-h" || "${1:-}" == "--help" ]]; then
  usage
  exit 0
fi

if [[ $# -lt 1 || $# -gt 2 ]]; then
  usage >&2
  exit 2
fi

catalog="$1"
output_file="${2:-}"

if [[ ! -f "$catalog" ]]; then
  echo "ERROR: catalog not found: $catalog" >&2
  exit 2
fi

emit() {
  awk -F'|' -v catalog="$catalog" '
    function trim(s) {
      gsub(/^[[:space:]]+|[[:space:]]+$/, "", s)
      return s
    }
    function snake(s, out, i, ch, prev) {
      out = ""
      prev = ""
      gsub(/<[^>]+>/, "", s)
      for (i = 1; i <= length(s); i++) {
        ch = substr(s, i, 1)
        if (ch ~ /[A-Z]/ && prev ~ /[a-z0-9]/) {
          out = out "_"
        }
        out = out ch
        prev = ch
      }
      gsub(/[^A-Za-z0-9]+/, "_", out)
      gsub(/^_+|_+$/, "", out)
      return tolower(out)
    }
    BEGIN {
      print "# DRAFT generated from " catalog
      print "# REVIEW REQUIRED: verify every field against the source PDF."
      print "fields:"
    }
    /^\|/ {
      name = trim($2)
      loc = trim($3)
      size = trim($4)
      type = trim($5)
      access = trim($6)
      spec = trim($7)
      desc = trim($8)

      if (name == "" || name == "Name" || name ~ /^-+$/) next
      if (loc == "" || loc == "Loc") next

      yaml_name = snake(name)
      gsub(/"/, "\\\"", desc)
      gsub(/"/, "\\\"", spec)

      print "  - name: " yaml_name
      print "    display_name: \"" name "\""
      print "    location: \"" loc "\""
      print "    size: \"" size "\""
      print "    type: \"" type "\""
      print "    access: \"" access "\""
      print "    description: \"" desc "\""
      print "    field_type: REVIEW"
      print "    tags: [REVIEW]"
      print "    scope: REVIEW"
      print "    provenance:"
      print "      spec: \"" spec "\""
      print "      status: REVIEW"
      print ""
    }
  ' "$catalog"
}

if [[ -n "$output_file" ]]; then
  mkdir -p "$(dirname "$output_file")"
  emit > "$output_file"
else
  emit
fi
