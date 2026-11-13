// Consumer example: synthetic canonical pages, no hardware, fed to the
// public facade. Requires `npm run build` first (npm test does that).
import test from "node:test";
import assert from "node:assert/strict";
import {
  decodeSff8472LaneSignalFlags,
  decodeSff8636LaneSignalFlags,
  evaluateSff8636Thresholds,
  sff8472PagesFromCanonical,
  sff8636PagesFromCanonical,
} from "../dist/index.js";

function syntheticSff8636Lower() {
  const page = new Uint8Array(128);
  page[1] = 0x07;
  page[3] = 0x11;
  page[86] = 0x01;
  page[22] = 0x02;
  page[23] = 0x00;
  return page;
}

function syntheticSff8636Page03h() {
  const page = new Uint8Array(128);
  page.set([0x01, 0x00, 0xff, 0x00, 0x00, 0x80, 0xff, 0x80], 0);
  return page;
}

test("synthetic SFF-8636 canonical pages drive lane flags and thresholds", () => {
  const pages = sff8636PagesFromCanonical({
    lower: syntheticSff8636Lower(),
    "00h": new Uint8Array(128),
    "03h": syntheticSff8636Page03h(),
  });
  const lanes = decodeSff8636LaneSignalFlags(pages);
  assert.equal(lanes.length, 4);
  assert.equal(lanes[0].index, 1);
  assert.equal(lanes[0].rx_los, true);
  assert.equal(lanes[0].tx_los, true);
  assert.equal(lanes[0].tx_disable, true);
  assert.equal(lanes[0].tx_fault, false);
  assert.equal(lanes[0].tx_adaptive_eq_fault, false);
  assert.equal(lanes[1].rx_los, false);

  const evaluated = evaluateSff8636Thresholds(pages);
  assert.deepEqual(evaluated.temperature, {
    high_alarm: true,
    low_alarm: false,
    high_warning: true,
    low_warning: false,
  });
  assert.deepEqual(evaluated.supply_voltage, {
    high_alarm: false,
    low_alarm: false,
    high_warning: false,
    low_warning: false,
  });
});

test("canonical SFF-8636 map fails closed on a missing or unknown key", () => {
  assert.throws(
    () => sff8636PagesFromCanonical({ "00h": new Uint8Array(128) }),
    /lower/,
  );
  assert.throws(
    () =>
      sff8636PagesFromCanonical({
        lower: syntheticSff8636Lower(),
        "00h": new Uint8Array(128),
        "99h": new Uint8Array([1]),
      }),
    /99h/,
  );
  const pages = sff8636PagesFromCanonical({
    lower: syntheticSff8636Lower(),
    "00h": new Uint8Array(128),
  });
  assert.equal(pages.page03h, undefined);
  assert.throws(() => evaluateSff8636Thresholds(pages), /page03h/);
});

test("synthetic SFF-8472 lane flags omit unmodeled adaptive EQ", () => {
  const a0h = new Uint8Array(256);
  a0h[92] = 0x20;
  a0h[94] = 0x01;
  const a2h = new Uint8Array(256);
  a2h[110] = 0x02;
  a2h[118] = 0x10;
  const pages = sff8472PagesFromCanonical({ a0h, a2h });
  const lanes = decodeSff8472LaneSignalFlags(pages);
  assert.deepEqual(lanes, [{ index: 1, rx_los: true, tx_disable: false, tx_fault: false }]);
});
