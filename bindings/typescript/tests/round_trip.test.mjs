// Round-trip tests (encode-round-trip Wave 3, TypeScript): set<Type><Field> is
// the exact inverse of decode. Runs on the compiled dist/ output.
import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { dirname, join } from "node:path";
import {
  decodeCmisPage00hAdministrative as decode,
  setCmisPage00hAdministrativeMaxPower as setMaxPower,
  setCmisPage00hAdministrativeVendorName as setVendorName,
  setCmisPage00hAdministrativeVendorOui as setVendorOui,
  setCmisPage00hAdministrativeVendorPn as setVendorPn,
  setCmisPage00hAdministrativeMediaLaneUnsupportedLane1 as setLane1,
  setCmisPage00hAdministrativeMediaLaneUnsupportedLane2 as setLane2,
} from "../dist/generated/cmis_page00h_administrative.js";

const here = dirname(fileURLToPath(import.meta.url));
const fixture = join(
  here,
  "../../../tests/fixtures/cmis/fboss-cmis-4-0-qsfpdd-400g-dr4.page-00h.bin",
);

test("u8 setter is decode inverse", () => {
  const buf = new Uint8Array(128);
  setMaxPower(buf, 42);
  assert.equal(decode(buf).maxPower, 42);
});

test("writing decoded values back is identity", () => {
  const orig = new Uint8Array(readFileSync(fixture)).slice(0, 128);
  const d = decode(orig);
  const copy = orig.slice();
  setMaxPower(copy, d.maxPower);
  setVendorName(copy, d.vendorName);
  setVendorOui(copy, d.vendorOui);
  setVendorPn(copy, d.vendorPn);
  assert.deepEqual(copy, orig);
});

test("bit-field setter preserves neighbors", () => {
  const buf = new Uint8Array(128);
  buf[82] = 0xff;
  setLane1(buf, false);
  assert.equal(buf[82], 0xfe);
  setLane2(buf, false);
  assert.equal(buf[82], 0xfc);
});
