#!/usr/bin/env bash
# Installed-artifact consumer check (review follow-up A5): pack
# OpenXcvr.Transceivers into a transient local feed, restore/build/run the
# consumer against ONLY that nupkg (nuget.config clears every other source),
# and decode one fixture per family. Exits nonzero unless every decode_status
# is "ok".
set -euo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="${OPENXCVR_REPO_ROOT:-$(cd "$HERE/../../../.." && pwd)}"
FEED="$HERE/feed"

rm -rf "$FEED"
trap 'rm -rf "$FEED"' EXIT

dotnet pack "$REPO_ROOT/bindings/csharp/OpenXcvr.Transceivers.csproj" -c Release -o "$FEED"
dotnet run --project "$HERE/Consumer.csproj" -c Release -- "$REPO_ROOT"
