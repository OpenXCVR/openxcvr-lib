#!/usr/bin/env bash
# tools/watch-oif-ias.sh — diff the OIF IAs CMIS-family catalog vs the local
# state baseline and report changes.
#
# Usage:
#   tools/watch-oif-ias.sh           # human-readable summary on stdout
#   tools/watch-oif-ias.sh --urls    # one URL per line (new + removed marked)
#
# Exit codes:
#   0  no changes detected
#   1  changes detected (output written to stdout)
#   2  fetch / parse error
#
# Design: pure curl + grep + comm. No firecrawl, gh, or python dependency.
# Suitable for cron or scheduled CI.
#
# State source: docs/status/upstream-publications.md, the "OIF — CMIS family"
# table. The watcher treats each PDF URL as the row identity; new URLs on the
# upstream page that aren't in the table are "new"; URLs in the table that are
# absent upstream are "removed".

set -euo pipefail

# comm/sort collation must agree byte-for-byte; under a non-C locale `sort -u`
# and `comm` disagree and comm aborts with "file is not in sorted order",
# silently breaking the weekly diff. Pin the C locale for the whole script.
export LC_ALL=C

# This script lives in openxcvr-lib/tools/, but the watcher state file lives in
# the sibling openxcvr pointer repo (which owns docs/, docs/stds/, docs/status/).
# Allow OPENXCVR_ROOT to point at the pointer repo explicitly; otherwise resolve
# the sibling checkout.
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
_lib_root="$(cd "$SCRIPT_DIR/.." && pwd)"
# A linked worktree lives under its repository's .worktrees/, so the sibling
# checkouts sit beside the main checkout that git's common directory names,
# not beside this one. Outside git, beside this checkout.
_common="$(git -C "$_lib_root" rev-parse --path-format=absolute --git-common-dir 2>/dev/null || true)"
_workspace="$(dirname "$(dirname "${_common:-$_lib_root/.git}")")"
if [[ -n "${OPENXCVR_ROOT:-}" ]]; then
  ROOT="$OPENXCVR_ROOT"
elif [[ -d "$_workspace/openxcvr/docs/status" ]]; then
  ROOT="$(cd "$_workspace/openxcvr" && pwd)"
else
  ROOT="$_lib_root"
fi
STATE_FILE="$ROOT/docs/status/upstream-publications.md"
URL="https://www.oiforum.com/technical-work/implementation-agreements-ias/"

while [[ $# -gt 0 ]]; do
  case "$1" in
    --state-file)
      [[ $# -ge 2 ]] || { echo "ERROR: --state-file requires a path" >&2; exit 2; }
      STATE_FILE="$2"; shift 2 ;;
    --urls) mode=--urls; shift ;;
    summary) mode=summary; shift ;;
    --help|-h) echo "Usage: $0 [--state-file PATH] [--urls]"; exit 0 ;;
    *) echo "ERROR: unknown argument: $1" >&2; exit 2 ;;
  esac
done
mode="${mode:-summary}"

if [[ ! -f "$STATE_FILE" ]]; then
  echo "ERROR: state file missing: $STATE_FILE (set --state-file)" >&2
  echo "         (docs/status/ lives in the openxcvr pointer repo; set OPENXCVR_ROOT" >&2
  echo "          to its root if no openxcvr checkout sits beside this repository)" >&2
  exit 2
fi

tmpdir="$(mktemp -d)"
trap 'rm -rf "$tmpdir"' EXIT

# 1. Fetch upstream HTML.
if ! curl -fsSL --max-time 60 -o "$tmpdir/page.html" \
     -A "openxcvr-watcher/1.0 (+https://github.com/openxcvr/openxcvr)" \
     "$URL"; then
  echo "ERROR: failed to fetch $URL" >&2
  exit 2
fi

# 2. Extract CMIS-family PDF URLs from the page. The filter requires "CMIS"
#    in the filename — this catches OIF-CMIS, OIF-C-CMIS, OIF-CMIS-FF,
#    OIF-CMIS-VCS, and OIF-ELSFP-CMIS, while excluding non-CMIS adjacent IAs
#    (plain OIF-ELSFP form-factor, OIF-CEI, OIF-FlexE, etc.) which are out
#    of openxcvr's scope.
#
# Both this pipeline and the state extraction below intentionally tolerate
# zero matches: under `set -euo pipefail` a grep that finds nothing returns
# 1 and aborts the script before the parse-failure sanity checks below can
# run. The `|| true` lets execution continue so those checks classify the
# zero-match case as exit 2 (parse failure) instead of crashing silently.
{
  grep -oE 'https?://[^"'\''[:space:]]+\.pdf' "$tmpdir/page.html" \
    | grep -i 'CMIS' \
    | sort -u
} > "$tmpdir/remote.urls" || true

# 3. Extract CMIS-family URLs from the state file's "OIF — CMIS family" table.
{
  awk '
    /^## OIF.*CMIS family/ { in_section = 1; next }
    /^##/                  { in_section = 0 }
    in_section             { print }
  ' "$STATE_FILE" \
    | grep -oE 'https?://[^"'\''[:space:]|]+\.pdf' \
    | sort -u
} > "$tmpdir/state.urls" || true

# 4. Parse-failure sanity checks.
#
# Either side coming back empty, or upstream contracting sharply, is almost
# always a parser/fetch problem rather than a real catalog event. Treating
# any of these as exit 1 would either flag every upstream URL as "new"
# (zero-state case) or every state URL as "removed" (zero-upstream case) —
# both are loud, false alarms. Bail out as exit 2 instead so the routine
# files a clearly-distinguished watcher-error issue.
remote_count=$(wc -l < "$tmpdir/remote.urls" | tr -d ' ')
state_count=$(wc -l < "$tmpdir/state.urls" | tr -d ' ')

# Compute "removed" up front for the contraction heuristic.
gone_preview=$(comm -13 "$tmpdir/remote.urls" "$tmpdir/state.urls")
gone_preview_count=0
[[ -n "$gone_preview" ]] && gone_preview_count=$(printf '%s\n' "$gone_preview" | wc -l | tr -d ' ')

# 4a. State-side zero. The state file's "OIF — CMIS family" table is a
#     known precondition: the watcher is seeded against the catalog the
#     project already knows about. Parsing zero URLs from it means either
#     the section header was renamed/removed, the table format changed, or
#     the file is corrupt. Continuing would misread every upstream URL as
#     newly-published — a 13-row false alarm.
if (( state_count == 0 )); then
  {
    echo "ERROR: parsed zero URLs from state file's 'OIF — CMIS family' table."
    echo "  state file : $STATE_FILE"
    echo "  upstream parsed $remote_count URL(s)."
    echo
    echo "Expected at least one tracked row. The section header is matched"
    echo "by the awk regex /^## OIF.*CMIS family/ — confirm that header"
    echo "exists and the table that follows uses the documented format."
    echo "Refusing to alert: all $remote_count upstream URLs would otherwise"
    echo "be reported as new."
  } >&2
  exit 2
fi

# 4b. Upstream-side zero.
if (( remote_count == 0 )); then
  {
    echo "ERROR: parsed zero CMIS-family URLs from upstream page."
    echo "  upstream URL : $URL"
    echo "  state has $state_count tracked URL(s)."
    echo
    echo "Likely a fetch failure (HTTP error swallowed, redirect to login),"
    echo "a JS-only render returning a static skeleton without the IAs"
    echo "table, or a layout change that hides the PDF links from a plain"
    echo "grep. Refusing to alert: all $state_count state URLs would"
    echo "otherwise be reported as removed."
  } >&2
  exit 2
fi

# 4c. Sharp upstream contraction. OIF historically only adds rows; new
#     revisions appear alongside older ones. Treat large negative deltas
#     as parse failures rather than real retractions.
if (( remote_count * 2 < state_count )) || (( gone_preview_count > 3 )); then
  {
    echo "ERROR: suspicious upstream contraction — refusing to alert."
    echo "  remote URLs parsed : $remote_count"
    echo "  state URLs known   : $state_count"
    echo "  apparent removals  : $gone_preview_count"
    echo
    echo "OIF historically only adds rows; this looks like a parse / fetch problem."
    echo
    echo "Remote URLs parsed:"
    sed 's/^/  /' "$tmpdir/remote.urls"
    echo
    echo "State URLs known:"
    sed 's/^/  /' "$tmpdir/state.urls"
  } >&2
  exit 2
fi

# 5. Diff.
new_urls=$(comm -23 "$tmpdir/remote.urls" "$tmpdir/state.urls")
gone_urls="$gone_preview"

if [[ -z "$new_urls" && -z "$gone_urls" ]]; then
  if [[ "$mode" == "--urls" ]]; then
    : # silent in url mode when no changes
  else
    echo "OK: no CMIS-family changes detected on OIF IAs page ($URL)."
  fi
  exit 0
fi

# 6. Report.
if [[ "$mode" == "--urls" ]]; then
  while IFS= read -r u; do [[ -n "$u" ]] && echo "+ $u"; done <<<"$new_urls"
  while IFS= read -r u; do [[ -n "$u" ]] && echo "- $u"; done <<<"$gone_urls"
else
  echo "# OIF IAs catalog change detected"
  echo
  echo "Source: $URL"
  echo "State : docs/status/upstream-publications.md"
  echo
  if [[ -n "$new_urls" ]]; then
    echo "## New documents ($(echo "$new_urls" | wc -l | tr -d ' '))"
    echo
    while IFS= read -r u; do [[ -n "$u" ]] && echo "- $u"; done <<<"$new_urls"
    echo
  fi
  if [[ -n "$gone_urls" ]]; then
    echo "## Removed from upstream catalog ($(echo "$gone_urls" | wc -l | tr -d ' '))"
    echo
    while IFS= read -r u; do [[ -n "$u" ]] && echo "- $u"; done <<<"$gone_urls"
    echo
  fi
fi

exit 1
