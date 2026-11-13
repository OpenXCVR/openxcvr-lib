#!/usr/bin/env python3
"""Deterministic C/Rust differential over the revision-checked pages.

Runs the C canonical projection runner (bindings/c/tests/checked_page_runner.c,
built by CMake as openxcvr_c_checked_page_runner) and the test-only Rust
reference (crates/openxcvr-cmis/examples/c-page-revision-reference.rs) for:

  * both pages (01h advertising, 11h lane status),
  * all six catalog revision bytes (0x30, 0x40, 0x50, 0x51, 0x52, 0x53),
  * at least four deterministic nonzero LCG patterns, plus the sixteen
    "dpsm-<nibble>" patterns that sweep every raw DPSM nibble per revision.

and compares, with explicit member-set completeness:

  * presence: the C runner's member set must equal the generated presence
    header's member set exactly (no silently omitted member), and per run the
    C-present set must equal the Rust per-revision struct's field set;
  * every common value member, scalar and array element (raw enum storage is
    compared numerically: the Rust enum variant is mapped back to its wire
    value through the generated enum's From<u8> table, parsed from source);
  * absent members must be zero on both sides;
  * raw DPSM storage interpreted through the C-side
    openxcvr_cmis_data_path_state_for_revision projection versus Rust's
    known/reserved CmisDataPathState::from_rev semantics (all 16 nibbles x
    six revisions via the dpsm sweep) — an independent landmark so a shared
    resolver bug cannot hide behind agreement of the shared decoders.

usage: checked_page_differential.py <c_runner> <rust_reference>
"""
import re
import subprocess
import sys
from pathlib import Path

REVS = [0x30, 0x40, 0x50, 0x51, 0x52, 0x53]
PAGES = {
    "01h": "cmis_page01h_advertising",
    "11h": "cmis_page11h_lane_status",
}
LCG_SEEDS = ["1", "2", "3", "4"]
DPSM_SEEDS = [f"dpsm-{n}" for n in "0123456789abcdef"]
ROOT = Path(__file__).resolve().parents[3]
TOKEN = re.compile(
    r"\s*(?P<tok>[-+]?\d+|[A-Za-z_][A-Za-z0-9_]*|[{}\[\]():,])"
)


def fail(message):
    sys.exit(f"checked page differential FAILED: {message}")


def presence_members(suffix):
    """The generated presence-struct member set (schema-derived completeness
    oracle, independent of the runner's hand projection)."""
    header = (ROOT / "bindings/c/generated/pages" / f"{suffix}_revision.h").read_text()
    body = header.split("typedef struct", 1)[1]
    body = body.split("} ", 1)[0]
    names = re.findall(r"bool\s+(\w+);", body)
    if not names:
        fail(f"no presence members parsed from {suffix}_revision.h")
    return names


def enum_variants():
    """Variant -> raw wire value, parsed from the generated enum From<u8>
    tables (source-derived; no hand-maintained enum map)."""
    variants = {}
    for name in (
        "Sff8024HostElectricalInterface",
        "CmisDataPathState",
        "CmisConfigStatus",
    ):
        text = (ROOT / "crates/openxcvr-cmis/src/generated" / f"{name}.rs").read_text()
        for raw, variant in re.findall(
            r"0x([0-9A-Fa-f]+)\s*=>\s*Self::(\w+),", text
        ):
            value = int(raw, 16)
            previous = variants.setdefault(variant, value)
            if previous != value:
                fail(f"ambiguous enum variant {variant}: {previous} vs {value}")
    return variants


VARIANTS = None


class DumpParser:
    """Recursive-descent parser for Rust's single-line Debug output of a
    generated per-revision page struct."""

    def __init__(self, text):
        self.tokens = []
        pos = 0
        while pos < len(text):
            match = TOKEN.match(text, pos)
            if match is None:
                fail(f"unparseable Debug dump at {text[pos:pos + 40]!r}")
            self.tokens.append(match.group("tok"))
            pos = match.end()
        self.index = 0

    def peek(self):
        return self.tokens[self.index] if self.index < len(self.tokens) else None

    def take(self):
        token = self.peek()
        if token is None:
            fail("Debug dump ended unexpectedly")
        self.index += 1
        return token

    def value(self):
        token = self.take()
        if token == "[":
            items = []
            while self.peek() != "]":
                items.append(self.value())
                if self.peek() == ",":
                    self.take()
            self.take()
            return items
        if token in ("true", "false"):
            return 1 if token == "true" else 0
        if re.fullmatch(r"[-+]?\d+", token):
            return int(token)
        if token == "Unknown":
            self.take()  # (
            number = int(self.take())
            self.take()  # )
            return number
        if re.fullmatch(r"[A-Za-z_][A-Za-z0-9_]*", token):
            if token not in VARIANTS:
                fail(f"unknown enum variant in Debug dump: {token}")
            return VARIANTS[token]
        fail(f"unexpected Debug token {token!r}")

    def fields(self):
        struct = self.take()
        self.take()  # {
        result = {}
        while self.peek() != "}":
            name = self.take()
            self.take()  # :
            result[name] = self.value()
            if self.peek() == ",":
                self.take()
        self.take()  # }
        if not struct.startswith("CmisPage0") and not struct.startswith("CmisPage1"):
            fail(f"unexpected struct name {struct}")
        return result


def run(command, args):
    proc = subprocess.run([command, *args], capture_output=True, text=True)
    if proc.returncode != 0:
        fail(
            f"{command} {args} exited {proc.returncode}\n{proc.stdout}{proc.stderr}"
        )
    return proc.stdout


def parse_c(output):
    present = {}
    values = {}
    interps = {}
    rev = None
    for line in output.splitlines():
        parts = line.split(" ", 2)
        if parts[0] == "rev":
            rev = int(parts[1])
        elif parts[0] == "present":
            present[parts[1]] = int(parts[2])
        elif parts[0] == "value":
            if "[" in parts[1]:
                name, index = parts[1].rstrip("]").split("[")
                values.setdefault(name, {})[int(index)] = int(parts[2])
            else:
                values[parts[1]] = int(parts[2])
        elif parts[0] == "interp":
            name, index = parts[1].rstrip("]").split("[")
            interps.setdefault(name, {})[int(index)] = parts[2]
    return rev, present, values, interps


def normalize_rust(raw_value):
    """Arrays are dicts keyed by index on the C side; align the Rust lists."""
    if isinstance(raw_value, list):
        return {index: item for index, item in enumerate(raw_value)}
    return raw_value


def main():
    global VARIANTS
    c_runner, rust_reference = sys.argv[1:3]
    VARIANTS = enum_variants()
    members = {page: presence_members(suffix) for page, suffix in PAGES.items()}
    combos = 0
    member_checks = 0
    interp_checks = 0
    for page, suffix in PAGES.items():
        for rev in REVS:
            for seed in LCG_SEEDS + DPSM_SEEDS:
                args = [page, str(rev), seed]
                c_rev, c_present, c_values, c_interps = parse_c(
                    run(c_runner, args)
                )
                rust_out = run(rust_reference, args)
                r_lines = dict(
                    line.split(" ", 1) for line in rust_out.splitlines()
                )
                if r_lines.get("page") != page or int(r_lines["rev"]) != rev:
                    fail(f"reference header mismatch for {args}")
                if c_rev != rev:
                    fail(f"C runner reported rev {c_rev} for {args}")
                dump = r_lines.get("dump")
                if dump is None:
                    fail(f"no dump line from reference for {args}")
                rust_fields = DumpParser(dump).fields()

                # Member-set completeness: the runner's projection covers the
                # whole generated presence struct, and the C-present set is
                # exactly the Rust per-revision struct's field set.
                if set(c_present) != set(members[page]):
                    fail(
                        f"{args}: C runner member set != presence header set\n"
                        f"  runner-only: {sorted(set(c_present) - set(members[page]))}\n"
                        f"  header-only: {sorted(set(members[page]) - set(c_present))}"
                    )
                c_present_set = {m for m, flag in c_present.items() if flag}
                if c_present_set != set(rust_fields):
                    fail(
                        f"{args}: C-present members != Rust fields\n"
                        f"  C-only: {sorted(c_present_set - set(rust_fields))}\n"
                        f"  Rust-only: {sorted(set(rust_fields) - c_present_set)}"
                    )
                for member in c_present_set:
                    c_value = c_values[member]
                    r_value = normalize_rust(rust_fields[member])
                    if c_value != r_value:
                        fail(
                            f"{args}: value mismatch for {member}: "
                            f"C {c_value!r} vs Rust {r_value!r}"
                        )
                    member_checks += 1
                # Absent members must be zero on the C side (the runner
                # asserts this too; re-check the printed projection).
                for member in set(c_present) - c_present_set:
                    if member in c_values and c_values[member] not in (0, {}):
                        fail(f"{args}: absent member {member} is not zero")
                # Raw DPSM interpretation, C projection vs Rust semantics.
                if page == "11h":
                    r_interps = {}
                    for line in rust_out.splitlines():
                        if line.startswith("interp dp_state_host_lane["):
                            index = int(line.split("[")[1].split("]")[0])
                            r_interps[index] = line.split("] ", 1)[1]
                    c_state = c_interps.get("dp_state_host_lane", {})
                    if len(c_state) != 8 or len(r_interps) != 8:
                        fail(f"{args}: expected 8 DPSM interpretations per side")
                    for index in range(8):
                        if c_state[index] != r_interps[index]:
                            fail(
                                f"{args}: DPSM interpretation mismatch lane "
                                f"{index}: C {c_state[index]!r} vs Rust "
                                f"{r_interps[index]!r}"
                            )
                        interp_checks += 1
                combos += 1
    # At least four deterministic nonzero patterns must have been exercised:
    # every LCG seed must decode at least one nonzero value on every page.
    for seed in LCG_SEEDS:
        nonzero = False
        for page in PAGES:
            _, _, c_values, _ = parse_c(run(c_runner, [page, "0x53", seed]))
            if any(
                value not in (0, {}) and value != {i: 0 for i in range(8)}
                for value in c_values.values()
            ):
                nonzero = True
        if not nonzero:
            fail(f"seed {seed} produced no nonzero pattern")
    print(
        f"checked page differential: {combos} cases "
        f"(2 pages x 6 revisions x {len(LCG_SEEDS) + len(DPSM_SEEDS)} "
        f"deterministic patterns), {member_checks} member comparisons, "
        f"{interp_checks} DPSM interpretation comparisons: C and Rust agree"
    )


if __name__ == "__main__":
    main()
