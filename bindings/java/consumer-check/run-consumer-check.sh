#!/usr/bin/env bash
# Installed-artifact consumer check (review follow-up A4, design D3):
# builds the JAR, compiles Consumer.java against ONLY that JAR
# (javac --release 11, no test-scope deps, no repo sources) and runs it on
# one fixture per family, failing unless every decode_status is "ok".
set -euo pipefail

here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
java_dir="$(cd "$here/.." && pwd)"
root="$(cd "$java_dir/../.." && pwd)"

(cd "$java_dir" && gradle jar)

# Gradle names the jar from the directory (no settings.gradle): java-0.2.0.jar.
jar="$java_dir/build/libs/java-0.2.0.jar"
if [ ! -f "$jar" ]; then
    echo "consumer check: built JAR not found at $jar" >&2
    exit 1
fi

work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT

javac --release 11 -cp "$jar" -d "$work" "$here/Consumer.java"

fx="$root/tests/fixtures"
java -cp "$work:$jar" com.openxcvr.consumer.Consumer \
    "$fx/cmis/fboss-cmis-5-3-osfp-800g-zr.lower.bin" \
    "$fx/cmis/fboss-cmis-5-3-osfp-800g-zr.page-00h.bin" \
    "$fx/cmis/fboss-cmis-5-3-osfp-800g-zr.page-01h.bin" \
    "$fx/cmis/fboss-cmis-5-3-osfp-800g-zr.page-11h.bin" \
    "$fx/sff-8636/synthetic-qsfp28-aoc-100g-sr4.lower.bin" \
    "$fx/sff-8636/synthetic-qsfp28-aoc-100g-sr4.page-00h.bin" \
    "$fx/sff-8472/finisar-ftlx8571d3bcl-mup0wb0.a0h.bin" \
    "$fx/sff-8472/finisar-ftlx8571d3bcl-mup0wb0.a2h.bin"
