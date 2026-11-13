// Checksum-recompute tests for the TypeScript bindings.
//
// Strategy: load a real binary fixture, capture the stored checksum byte(s),
// call recomputeChecksums*, verify the recomputed value equals the stored one.
//
// Page 00h (CMIS): checksum at buf[94] covering buf[0..=93].
//   5 fixtures with valid sum8 checksums (same set as the Rust test suite).
//
// Page 02h (CMIS): checksum at buf[127] covering buf[0..=126].
//   15 fixtures, all valid.
//
// Page 04h / 05h (CMIS): no fixtures in the corpus — omitted.
//
// Page 00h upper (SFF-8636): CC_BASE at buf[63] covers buf[0..=62];
//   CC_EXT at buf[95] covers buf[64..=94].
//   All 5 SFF-8636 fixtures are valid.
//
// PageA0h (SFF-8472): CC_BASE at buf[63] covers buf[0..=62];
//   CC_EXT at buf[95] covers buf[64..=94].
//   All 5 SFF-8472 fixtures are valid.
//
// PageA2h (SFF-8472): checksum at buf[95] covering buf[0..=94].
//   All 5 SFF-8472 a2h fixtures are valid.
//
// PageA2h03h (SFF-8472 high accuracy timing): checksum at buf[255]
//   covering buf[128..=254].  All 5 fixtures are valid.

import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { dirname, join } from "node:path";

import {
  recomputeChecksumsCmisPage00hAdministrative,
} from "../dist/generated/cmis_page00h_administrative.js";
import {
  recomputeChecksumsCmisPage02hThresholds,
} from "../dist/generated/cmis_page02h_thresholds.js";
import {
  recomputeChecksumsCmisPage04hLaserCapabilities,
} from "../dist/generated/cmis_page04h_laser_capabilities.js";
import {
  recomputeChecksumsCmisPage05hFormFactorSignals,
} from "../dist/generated/cmis_page05h_form_factor_signals.js";
import {
  recomputeChecksumsSff8636Page00hUpperAdministrative,
} from "../dist/generated/sff8636_page00h_upper_administrative.js";
import {
  recomputeChecksumsPageA0hBaseMemory,
} from "../dist/generated/page_a0h_base_memory.js";
import {
  recomputeChecksumsPageA2hDiagnosticsBase,
} from "../dist/generated/page_a2h_diagnostics_base.js";
import {
  recomputeChecksumsPageA2h03hHighAccuracyTiming,
} from "../dist/generated/page_a2h_03h_high_accuracy_timing.js";

const here = dirname(fileURLToPath(import.meta.url));
const repoRoot = join(here, "../../..");

function loadBin(rel) {
  return new Uint8Array(readFileSync(join(repoRoot, rel)));
}

// ─── Page 00h (CMIS) ──────────────────────────────────────────────────────────
// Checksum at buf[94] covering buf[0..=93].
// Only these 5 fixtures carry valid sum8 checksums (the others have
// vendor-specific stored values).

const page00hStems = [
  "fboss-cmis-4-0-qsfp112-200g-fr4",
  "fboss-cmis-4-0-qsfpdd-400g-fr4-multiport",
  "fboss-cmis-4-0-qsfpdd-400g-lr4",
  "synthetic-cmis-3-0-qsfp-dd",
  "synthetic-cmis-5-2-qsfp-dd",
];

for (const stem of page00hStems) {
  test(`page-00h recompute matches stored: ${stem}`, () => {
    const buf = loadBin(`tests/fixtures/cmis/${stem}.page-00h.bin`);
    const stored = buf[94];
    recomputeChecksumsCmisPage00hAdministrative(buf);
    assert.equal(
      buf[94],
      stored,
      `page-00h checksum: recomputed 0x${buf[94].toString(16).padStart(2, "0")} != stored 0x${stored.toString(16).padStart(2, "0")}`,
    );
  });
}

// ─── Page 02h (CMIS) ──────────────────────────────────────────────────────────
// Checksum at buf[127] covering buf[0..=126].  All 15 fixtures are valid.

const page02hStems = [
  "fboss-cmis-4-0-osfp-2x400g-dr4",
  "fboss-cmis-4-0-qsfp112-200g-fr4",
  "fboss-cmis-4-0-qsfpdd-400g-dr4",
  "fboss-cmis-4-0-qsfpdd-400g-fr4-multiport",
  "fboss-cmis-4-0-qsfpdd-400g-lr4",
  "fboss-cmis-5-0-osfp-2x400g-fr4",
  "fboss-cmis-5-1-osfp-2x400g-fr4-lite",
  "fboss-cmis-5-1-osfp-2x400g-fr4-lpo",
  "fboss-cmis-5-1-osfp-2x800g-dr4",
  "fboss-cmis-5-1-osfp-credo-800g-aec",
  "fboss-cmis-5-3-osfp-800g-zr",
  "fboss-cmis-5-3-osfp-800g-zr-bias-reserved",
  "fboss-cmis-5-3-osfp-800g-zr-dp-reserved",
  "synthetic-cmis-3-0-qsfp-dd",
  "synthetic-cmis-5-2-qsfp-dd",
];

for (const stem of page02hStems) {
  test(`page-02h recompute matches stored: ${stem}`, () => {
    const buf = loadBin(`tests/fixtures/cmis/${stem}.page-02h.bin`);
    const stored = buf[127];
    recomputeChecksumsCmisPage02hThresholds(buf);
    assert.equal(
      buf[127],
      stored,
      `page-02h checksum: recomputed 0x${buf[127].toString(16).padStart(2, "0")} != stored 0x${stored.toString(16).padStart(2, "0")}`,
    );
  });
}

// ─── Mutation test (page-02h) ─────────────────────────────────────────────────

test("page-02h recompute tracks data mutation", () => {
  const buf = loadBin(
    "tests/fixtures/cmis/fboss-cmis-5-3-osfp-800g-zr.page-02h.bin",
  );
  recomputeChecksumsCmisPage02hThresholds(buf);
  const before = buf[127];
  buf[0] = (buf[0] + 1) & 0xff;
  recomputeChecksumsCmisPage02hThresholds(buf);
  assert.equal(buf[127], (before + 1) & 0xff);
});

test("page-02h recompute zeroed buffer is zero", () => {
  const buf = new Uint8Array(128);
  recomputeChecksumsCmisPage02hThresholds(buf);
  assert.equal(buf[127], 0);
});

// ─── Page 04h / 05h (CMIS) ────────────────────────────────────────────────────
// No fixtures for these pages in the corpus; confirm the functions are exported
// and that a zeroed buffer produces a zero checksum (buf[127]).

test("page-04h recomputeChecksums exported and zeroed → 0", () => {
  const buf = new Uint8Array(128);
  recomputeChecksumsCmisPage04hLaserCapabilities(buf);
  assert.equal(buf[127], 0);
});

test("page-05h recomputeChecksums exported and zeroed → 0", () => {
  const buf = new Uint8Array(128);
  recomputeChecksumsCmisPage05hFormFactorSignals(buf);
  assert.equal(buf[127], 0);
});

// ─── SFF-8636 Page 00h Upper ──────────────────────────────────────────────────
// CC_BASE at buf[63] covers buf[0..=62]; CC_EXT at buf[95] covers buf[64..=94].

const sff8636Stems = [
  "finisar-ftl410qe3c-40g-sr4",
  "finisar-ftlc9551repm-100g-sr4",
  "synthetic-qsfp-dac-40g-passive-cu",
  "synthetic-qsfp28-aoc-100g-sr4",
  "synthetic-qsfp28-bidi-100g-cwdm4",
];

for (const stem of sff8636Stems) {
  test(`sff8636 page-00h recompute matches stored: ${stem}`, () => {
    const buf = loadBin(`tests/fixtures/sff-8636/${stem}.page-00h.bin`);
    const storedBase = buf[63];
    const storedExt = buf[95];
    recomputeChecksumsSff8636Page00hUpperAdministrative(buf);
    assert.equal(
      buf[63],
      storedBase,
      `CC_BASE mismatch: recomputed 0x${buf[63].toString(16).padStart(2, "0")} != stored 0x${storedBase.toString(16).padStart(2, "0")}`,
    );
    assert.equal(
      buf[95],
      storedExt,
      `CC_EXT mismatch: recomputed 0x${buf[95].toString(16).padStart(2, "0")} != stored 0x${storedExt.toString(16).padStart(2, "0")}`,
    );
  });
}

// ─── SFF-8472 PageA0h ─────────────────────────────────────────────────────────
// CC_BASE at buf[63] covers buf[0..=62]; CC_EXT at buf[95] covers buf[64..=94].

const sff8472Stems = [
  "finisar-ftlx8571d3bcl-mup0wb0",
  "finisar-ftlx8571d3bcl-muq1bzb",
  "synthetic-dac-sfp10g-passive-cu",
  "synthetic-extcal-sfp10g-sr",
  "synthetic-sfp28-25g-sr",
];

for (const stem of sff8472Stems) {
  test(`sff-8472 a0h recompute matches stored: ${stem}`, () => {
    const buf = loadBin(`tests/fixtures/sff-8472/${stem}.a0h.bin`);
    const storedBase = buf[63];
    const storedExt = buf[95];
    recomputeChecksumsPageA0hBaseMemory(buf);
    assert.equal(
      buf[63],
      storedBase,
      `CC_BASE mismatch: recomputed 0x${buf[63].toString(16).padStart(2, "0")} != stored 0x${storedBase.toString(16).padStart(2, "0")}`,
    );
    assert.equal(
      buf[95],
      storedExt,
      `CC_EXT mismatch: recomputed 0x${buf[95].toString(16).padStart(2, "0")} != stored 0x${storedExt.toString(16).padStart(2, "0")}`,
    );
  });
}

// ─── SFF-8472 PageA2h (diagnostics base) ─────────────────────────────────────
// Checksum at buf[95] covering buf[0..=94].

for (const stem of sff8472Stems) {
  test(`sff-8472 a2h diagnostics recompute matches stored: ${stem}`, () => {
    const buf = loadBin(`tests/fixtures/sff-8472/${stem}.a2h.bin`);
    const stored = buf[95];
    recomputeChecksumsPageA2hDiagnosticsBase(buf);
    assert.equal(
      buf[95],
      stored,
      `a2h checksum mismatch: recomputed 0x${buf[95].toString(16).padStart(2, "0")} != stored 0x${stored.toString(16).padStart(2, "0")}`,
    );
  });
}

// ─── SFF-8472 PageA2h03h (high accuracy timing) ───────────────────────────────
// Checksum at buf[255] covering buf[128..=254].

for (const stem of sff8472Stems) {
  test(`sff-8472 a2h_03h high-accuracy timing recompute matches stored: ${stem}`, () => {
    const buf = loadBin(`tests/fixtures/sff-8472/${stem}.a2h.bin`);
    const stored = buf[255];
    recomputeChecksumsPageA2h03hHighAccuracyTiming(buf);
    assert.equal(
      buf[255],
      stored,
      `a2h_03h checksum mismatch: recomputed 0x${buf[255].toString(16).padStart(2, "0")} != stored 0x${stored.toString(16).padStart(2, "0")}`,
    );
  });
}
