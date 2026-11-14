#!/usr/bin/env bash
# Extract a page range from a local standards PDF with layout preserved.

set -euo pipefail

usage() {
  cat <<'USAGE'
Usage:
  tools/extract-pdf-range.sh [--stds-root DIR] <alias-or-pdf-path> <start-page> <end-page> [output-file]

Aliases:
  cmis-3.0            cmis-5.1                cmis-5.2-changebars   cmis-5.4-changebars
  cmis-4.0            cmis-5.2                cmis-5.3              cmis-errata
  cmis-5.0            cmis-5.3-changebars     cmis-5.4
  sff-8024            sff-8472                sff-8636

Environment:
  OPENXCVR_ROOT   openxcvr pointer repo root (owns docs/stds/). Defaults to
                  the openxcvr checkout beside this lib repo's main checkout.
                  Set this if PDFs are not co-located with this lib repo.

Examples:
  tools/extract-pdf-range.sh cmis-5.3 215 244
  tools/extract-pdf-range.sh sff-8636 30 50 /tmp/sff8636.txt
USAGE
}

stds_root=""
if [[ "${1:-}" == "--stds-root" ]]; then
  [[ $# -ge 2 ]] || { echo "ERROR: --stds-root requires a directory" >&2; exit 2; }
  stds_root="$2"
  shift 2
fi

if [[ "${1:-}" == "-h" || "${1:-}" == "--help" ]]; then
  usage
  exit 0
fi

if [[ $# -lt 3 || $# -gt 4 ]]; then
  usage >&2
  exit 2
fi

root="$(cd "$(dirname "$0")/.." && pwd)"
# PDFs live in the sibling openxcvr pointer repo's docs/stds/. This script
# lives in openxcvr-lib/tools/. Allow OPENXCVR_ROOT to point at the pointer
# repo explicitly; otherwise resolve the sibling checkout.
_lib_root="$root"
# A linked worktree lives under its repository's .worktrees/, so the sibling
# checkouts sit beside the main checkout that git's common directory names,
# not beside this one. Outside git, beside this checkout.
_common="$(git -C "$_lib_root" rev-parse --path-format=absolute --git-common-dir 2>/dev/null || true)"
_workspace="$(dirname "$(dirname "${_common:-$_lib_root/.git}")")"
if [[ -n "${OPENXCVR_ROOT:-}" ]]; then
  root="$OPENXCVR_ROOT"
elif [[ -d "$_workspace/openxcvr/docs/stds" ]]; then
  root="$(cd "$_workspace/openxcvr" && pwd)"
fi
stds_root="${stds_root:-$root/docs/stds}"
pdf_arg="$1"
start_page="$2"
end_page="$3"
output_file="${4:-}"

case "$pdf_arg" in
  cmis-3.0)
    pdf="$stds_root/cmis/CMIS3p0_Third_Party_Spec.pdf"
    ;;
  cmis-4.0)
    pdf="$stds_root/cmis/CMIS4p0_Third_Party_Spec.pdf"
    ;;
  cmis-5.0)
    pdf="$stds_root/cmis/CMIS5p0_Third_Party_Spec.pdf"
    ;;
  cmis-5.1)
    pdf="$stds_root/cmis/CMIS5p1_Third_Party_Spec.pdf"
    ;;
  cmis-5.2)
    pdf="$stds_root/cmis/OIF-CMIS-05.2.pdf"
    ;;
  cmis-5.2-changebars)
    pdf="$stds_root/cmis/OIF-CMIS-05.2-changebars.pdf"
    ;;
  cmis-5.3)
    pdf="$stds_root/cmis/OIF-CMIS-05.3.pdf"
    ;;
  cmis-5.3-changebars)
    pdf="$stds_root/cmis/OIF-CMIS-05.3-changebars.pdf"
    ;;
  cmis-5.4)
    pdf="$stds_root/cmis/OIF-CMIS-05.4.pdf"
    ;;
  cmis-5.4-changebars)
    pdf="$stds_root/cmis/OIF-CMIS-05.4-changebars.pdf"
    ;;
  cmis-errata)
    pdf="$stds_root/cmis/OIF-CMIS-05-Errata-01.0.pdf"
    ;;
  sff-8024)
    pdf="$stds_root/sff-8024/SFF-8024-Rev4.14.pdf"
    ;;
  sff-8472)
    pdf="$stds_root/sff-8472/SFF-8472-Rev12.5a.pdf"
    ;;
  sff-8636)
    pdf="$stds_root/sff-8636/SFF-8636-Rev2.12.pdf"
    ;;
  *)
    pdf="$pdf_arg"
    ;;
esac

if ! command -v pdftotext >/dev/null 2>&1; then
  echo "ERROR: pdftotext not found. Install Poppler tools first." >&2
  exit 127
fi

if [[ ! -f "$pdf" ]]; then
  echo "ERROR: PDF not found: $pdf (set --stds-root or provide a PDF path)" >&2
  exit 2
fi

if [[ ! "$start_page" =~ ^[0-9]+$ || ! "$end_page" =~ ^[0-9]+$ ]]; then
  echo "ERROR: start-page and end-page must be positive integers." >&2
  exit 2
fi

if (( start_page < 1 || end_page < start_page )); then
  echo "ERROR: invalid page range: $start_page-$end_page" >&2
  exit 2
fi

if [[ -n "$output_file" ]]; then
  mkdir -p "$(dirname "$output_file")"
  pdftotext -layout -f "$start_page" -l "$end_page" "$pdf" "$output_file"
else
  pdftotext -layout -f "$start_page" -l "$end_page" "$pdf" -
fi
