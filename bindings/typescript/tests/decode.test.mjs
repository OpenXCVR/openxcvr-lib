// Runtime parity smoke test for the generated TypeScript bindings.
// Plain ESM JS (no @types/node needed); runs against the compiled dist/ output.
// Build first: `npm run build` (tsc). Full golden-fixture parity lands in Wave 5.
import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { dirname, join } from "node:path";
import { decodeCmisLowerMemory } from "../dist/generated/cmis_lower_memory.js";

const here = dirname(fileURLToPath(import.meta.url));
const fixture = join(
  here,
  "../../../tests/fixtures/cmis/fboss-cmis-4-0-qsfpdd-400g-dr4.lower.bin",
);

test("decodeCmisLowerMemory reads a real CMIS 4.0 fixture at correct offsets", () => {
  const buf = new Uint8Array(readFileSync(fixture));
  assert.ok(buf.length >= 128, `fixture too small: ${buf.length}`);

  const m = decodeCmisLowerMemory(buf);

  // Field placement must match the raw bytes (proves offsets).
  assert.equal(m.sff8024Identifier, buf[0], "sff8024Identifier offset");
  assert.equal(m.cmisRevision, buf[1], "cmisRevision offset");

  // The identifier for a QSFP-DD/OSFP CMIS module is non-zero.
  assert.ok(m.sff8024Identifier > 0, "identifier should be non-zero");
  assert.equal(typeof m.moduleState, "number");
});
