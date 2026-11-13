import fs from 'node:fs';
import path from 'node:path';
import test from 'node:test';
import assert from 'node:assert/strict';
import { fileURLToPath } from 'node:url';
import { decodeCmis } from '../dist/index.js';

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const repoRoot = path.resolve(__dirname, '../../..');
const fixturesDir = path.join(repoRoot, 'tests/fixtures/cmis');

function loadBuffers(stem) {
  const lowerPath = path.join(fixturesDir, `${stem}.lower.bin`);
  const p00Path = path.join(fixturesDir, `${stem}.page-00h.bin`);
  const p01Path = path.join(fixturesDir, `${stem}.page-01h.bin`);
  const p11Path = path.join(fixturesDir, `${stem}.page-11h.bin`);

  const pages = {
    lower: new Uint8Array(fs.readFileSync(lowerPath)),
    page00h: new Uint8Array(fs.readFileSync(p00Path)),
  };
  if (fs.existsSync(p01Path)) {
    pages.page01h = new Uint8Array(fs.readFileSync(p01Path));
  }
  if (fs.existsSync(p11Path)) {
    pages.page11h = new Uint8Array(fs.readFileSync(p11Path));
  }
  return pages;
}

test('rejection on anomalous golden and synthetic major 0x60', () => {
  const anomalousPages = loadBuffers('fboss-cmis-anomalous-qsfpdd-400g-cr8');
  const resAnomalous = decodeCmis(anomalousPages);
  assert.strictEqual(resAnomalous.decode_status, 'rejected');
  assert.strictEqual(resAnomalous.rejection.reason, 'unknown_major_revision: 0x05');
  assert.strictEqual(
    resAnomalous.rejection.detail,
    'Lower memory byte 1 = 0x05; CMIS 5.3 § 8.2.1 defines major revisions 1.x, 3.0, 4.0, 5.0, 5.1, 5.3. The fboss test corpus uses 0x05 as an explicit anomalous-rev regression target.'
  );

  const zrPages = loadBuffers('fboss-cmis-5-3-osfp-800g-zr');
  const syntheticLower = new Uint8Array(zrPages.lower);
  syntheticLower[1] = 0x60;
  const resSynthetic = decodeCmis({ ...zrPages, lower: syntheticLower });
  assert.strictEqual(resSynthetic.decode_status, 'rejected');
  assert.strictEqual(resSynthetic.rejection.reason, 'unknown_major_revision: 0x60');
  assert.strictEqual(
    resSynthetic.rejection.detail,
    'Lower memory byte 1 = 0x60; CMIS 5.3 § 8.2.1 defines major revisions 1.x, 3.0, 4.0, 5.0, 5.1, 5.3. The fboss test corpus uses 0x60 as an explicit anomalous-rev regression target.'
  );
});

test('unsupported minor 0x55 produces Unknown<n> for every dp_state', () => {
  const zrPages = loadBuffers('fboss-cmis-5-3-osfp-800g-zr');
  const lower = new Uint8Array(zrPages.lower);
  lower[1] = 0x55;
  const res = decodeCmis({ ...zrPages, lower });
  assert.strictEqual(res.decode_status, 'ok');
  assert.ok(res.lanes && res.lanes.length === 8);
  for (const lane of res.lanes) {
    assert.match(lane.dp_state, /^Unknown\d+$/);
  }
});

test('reserved bias multiplier yields tx_bias_ma null for all lanes', () => {
  const goldenPages = loadBuffers('fboss-cmis-5-3-osfp-800g-zr-bias-reserved');
  const resGolden = decodeCmis(goldenPages);
  assert.strictEqual(resGolden.decode_status, 'ok');
  assert.ok(resGolden.lanes && resGolden.lanes.length === 8);
  for (const lane of resGolden.lanes) {
    assert.strictEqual(lane.tx_bias_ma, null);
  }

  const zrPages = loadBuffers('fboss-cmis-5-3-osfp-800g-zr');
  const mutatedP01 = new Uint8Array(zrPages.page01h);
  // Page 01h byte 160 is buffer offset 32 (160 - 128). Bits 4-3 set to 0b11 (0x18).
  mutatedP01[32] = (mutatedP01[32] & ~0x18) | 0x18;
  const resMutated = decodeCmis({ ...zrPages, page01h: mutatedP01 });
  assert.strictEqual(resMutated.decode_status, 'ok');
  assert.ok(resMutated.lanes && resMutated.lanes.length === 8);
  for (const lane of resMutated.lanes) {
    assert.strictEqual(lane.tx_bias_ma, null);
  }
});

test('flat memory with well-formed Page 11h gives lanes/flags null without throwing', () => {
  const flatPages = loadBuffers('fboss-cmis-4-0-qsfpdd-flatmem');
  const dummyP11 = new Uint8Array(128);
  const res = decodeCmis({ ...flatPages, page11h: dummyP11 });
  assert.strictEqual(res.decode_status, 'ok');
  assert.strictEqual(res.lanes, null);
  assert.strictEqual(res.flags, null);
});

test('Page 11h supplied without Page 01h throws Error', () => {
  const zrPages = loadBuffers('fboss-cmis-5-3-osfp-800g-zr');
  assert.throws(
    () => decodeCmis({ lower: zrPages.lower, page00h: zrPages.page00h, page11h: zrPages.page11h }),
    /Page 11h present but Page 01h missing \(Tx-bias multiplier\)/
  );
});

test('r3 A2 mixed cases validate input lengths before admission', () => {
  const zrPages = loadBuffers('fboss-cmis-5-3-osfp-800g-zr');
  const flatPages = loadBuffers('fboss-cmis-4-0-qsfpdd-flatmem');

  // 1. Unsupported major + short optional page throws RangeError
  const badRevLower = new Uint8Array(zrPages.lower);
  badRevLower[1] = 0x60;
  assert.throws(
    () => decodeCmis({ lower: badRevLower, page00h: zrPages.page00h, page01h: new Uint8Array(64) }),
    (err) => err instanceof RangeError && err.message.includes('page01h')
  );

  // 2. Flat memory + short Page 11h throws RangeError
  assert.throws(
    () => decodeCmis({ lower: flatPages.lower, page00h: flatPages.page00h, page11h: new Uint8Array(64) }),
    (err) => err instanceof RangeError && err.message.includes('page11h')
  );

  // 3. Short Page 01h without Page 11h throws RangeError
  assert.throws(
    () => decodeCmis({ lower: zrPages.lower, page00h: zrPages.page00h, page01h: new Uint8Array(64) }),
    (err) => err instanceof RangeError && err.message.includes('page01h')
  );

  // 4. Short Page 01h with valid Page 11h throws RangeError
  assert.throws(
    () => decodeCmis({ lower: zrPages.lower, page00h: zrPages.page00h, page01h: new Uint8Array(64), page11h: zrPages.page11h }),
    (err) => err instanceof RangeError && err.message.includes('page01h')
  );
});

test('page lengths boundary checks (0, 127, 128, 129)', () => {
  const zrPages = loadBuffers('fboss-cmis-5-3-osfp-800g-zr');

  const pageKeys = ['lower', 'page00h', 'page01h', 'page11h'];
  for (const key of pageKeys) {
    // 0 bytes throws RangeError
    const pages0 = { ...zrPages, [key]: new Uint8Array(0) };
    assert.throws(
      () => decodeCmis(pages0),
      (err) => err instanceof RangeError && err.message.includes(key),
      `expected RangeError for length 0 on ${key}`
    );

    // 127 bytes throws RangeError
    const pages127 = { ...zrPages, [key]: new Uint8Array(127) };
    assert.throws(
      () => decodeCmis(pages127),
      (err) => err instanceof RangeError && err.message.includes(key),
      `expected RangeError for length 127 on ${key}`
    );

    // 128 bytes accepted
    const pages128 = { ...zrPages, [key]: new Uint8Array(128) };
    if (key === 'lower') pages128.lower[1] = 0x53;
    assert.doesNotThrow(() => decodeCmis(pages128), `expected accept for length 128 on ${key}`);

    // 129 bytes accepted
    const pages129 = { ...zrPages, [key]: new Uint8Array(129) };
    if (key === 'lower') pages129.lower[1] = 0x53;
    assert.doesNotThrow(() => decodeCmis(pages129), `expected accept for length 129 on ${key}`);
  }
});

test('non-Uint8Array throws TypeError', () => {
  const zrPages = loadBuffers('fboss-cmis-5-3-osfp-800g-zr');
  assert.throws(() => decodeCmis({ ...zrPages, lower: [1, 2, 3] }), TypeError);
  assert.throws(() => decodeCmis({ ...zrPages, page00h: 'string' }), TypeError);
  assert.throws(() => decodeCmis({ ...zrPages, page01h: {} }), TypeError);
  assert.throws(() => decodeCmis({ ...zrPages, page11h: 12345 }), TypeError);
});

test('subarray view with byteOffset 7 supported', () => {
  const zrPages = loadBuffers('fboss-cmis-5-3-osfp-800g-zr');

  const makeOffsetView = (orig) => {
    const big = new Uint8Array(orig.length + 20);
    big.set(orig, 7);
    return big.subarray(7, 7 + orig.length);
  };

  const pagesOffset = {
    lower: makeOffsetView(zrPages.lower),
    page00h: makeOffsetView(zrPages.page00h),
    page01h: makeOffsetView(zrPages.page01h),
    page11h: makeOffsetView(zrPages.page11h),
  };

  const resOrig = decodeCmis(zrPages);
  const resOffset = decodeCmis(pagesOffset);
  assert.deepStrictEqual(resOffset, resOrig);
});

test('inputs unchanged after decode', () => {
  const zrPages = loadBuffers('fboss-cmis-5-3-osfp-800g-zr');
  const lowerCopy = new Uint8Array(zrPages.lower);
  const p00Copy = new Uint8Array(zrPages.page00h);
  const p01Copy = new Uint8Array(zrPages.page01h);
  const p11Copy = new Uint8Array(zrPages.page11h);

  decodeCmis(zrPages);

  assert.deepStrictEqual(zrPages.lower, lowerCopy);
  assert.deepStrictEqual(zrPages.page00h, p00Copy);
  assert.deepStrictEqual(zrPages.page01h, p01Copy);
  assert.deepStrictEqual(zrPages.page11h, p11Copy);
});

const TEST_CASES_68 = [
  [0x01, 0x01, "base_10g_sw"],
  [0x01, 0x1C, "fc_128g_mm"],
  [0x01, 0xC0, "vendor_specific"],
  [0x02, 0x01, "base_10g_lw"],
  [0x02, 0x1C, "base_400g_dr4"],
  [0x02, 0x68, "flexo_8e_dpo_16qam_foic8e_8_dpo"],
  [0x02, 0xC0, "vendor_specific"],
  [0x02, 0xC1, "vendor_specific"],
  [0x02, 0xF7, "vendor_specific_f7"],
  [0x03, 0x01, "copper_cable"],
  [0x03, 0x1C, "reserved_02_be"],
  [0x03, 0xC0, "linear_active_copper_loopback_module"],
  [0x03, 0xC1, "vendor_specific"],
  [0x04, 0x01, "active_cable_ber_1e_12"],
  [0x04, 0x1C, "reserved_05_be"],
  [0x04, 0xF7, "vendor_specific"],
  [0x05, 0x01, "base_1000_t"],
  [0x05, 0xC1, "vendor_specific"],
  [0x00, 0x1C, "unknown"],
  [0x06, 0x1C, "unknown"],
  [0x40, 0x1C, "unknown"],
  [0x90, 0x1C, "unknown"],
  [0xFF, 0x1C, "unknown"],
];

test('#68 media table: Lower caller', () => {
  const dr4Pages = loadBuffers('fboss-cmis-4-0-qsfpdd-400g-dr4');
  for (const [mediaType, mif, expected] of TEST_CASES_68) {
    const lower = new Uint8Array(dr4Pages.lower);
    lower[85] = mediaType;
    lower[87] = mif;
    const res = decodeCmis({ ...dr4Pages, lower });
    assert.ok(res.applications && res.applications.length > 0);
    assert.strictEqual(res.applications[0].media_interface_name, expected);
  }
});

test('#68 media table: Page 01h caller', () => {
  const osfpPages = loadBuffers('fboss-cmis-4-0-osfp-2x400g-dr4');
  for (const [mediaType, mif, expected] of TEST_CASES_68) {
    const lower = new Uint8Array(osfpPages.lower);
    lower[85] = mediaType;
    const p01 = new Uint8Array(osfpPages.page01h);
    p01[96] = mif;
    const res = decodeCmis({ ...osfpPages, lower, page01h: p01 });
    assert.ok(res.applications && res.applications.length > 0);
    const entry = res.applications.find((a) => a.app_code === 9);
    assert.ok(entry, 'expected app_code 9');
    assert.strictEqual(entry.media_interface_name, expected);
  }
});

const EXPECTED_HIF_0x53 = [81, 82, 79, 80, 77, 78, 75, 76, 81, 82, 79, 80, 77, 78, 75];
const REVISIONS_69 = [0x30, 0x40, 0x50, 0x51, 0x52, 0x53];

test('#69 application numbering: cases a-d', () => {
  const zrPages = loadBuffers('fboss-cmis-5-3-osfp-800g-zr');

  // (a) For each Lower:1 revision byte 0x30..0x53: app codes [1..15], and for 0x53 HIF matches.
  for (const rev of REVISIONS_69) {
    const lower = new Uint8Array(zrPages.lower);
    lower[1] = rev;
    const res = decodeCmis({ ...zrPages, lower });
    const appCodes = (res.applications || []).map((a) => a.app_code);
    assert.deepStrictEqual(appCodes, Array.from({ length: 15 }, (_, i) => i + 1));
    if (rev === 0x53) {
      const hifs = (res.applications || []).map((a) => a.host_interface_id);
      assert.deepStrictEqual(hifs, EXPECTED_HIF_0x53);
    }
  }

  // (b) Page 01h not supplied: app codes exactly [1..8].
  {
    const res = decodeCmis({ lower: zrPages.lower, page00h: zrPages.page00h });
    const appCodes = (res.applications || []).map((a) => a.app_code);
    assert.deepStrictEqual(appCodes, Array.from({ length: 8 }, (_, i) => i + 1));
  }

  // (c) Lower slot 3 HIF set to 0xFF (terminator): Lower yields [1, 2], Page 01h yields [9..15].
  {
    const lower = new Uint8Array(zrPages.lower);
    lower[94] = 0xFF; // Lower slot 3 HIF
    const res = decodeCmis({ ...zrPages, lower });
    const appCodes = (res.applications || []).map((a) => a.app_code);
    assert.deepStrictEqual(appCodes, [1, 2, 9, 10, 11, 12, 13, 14, 15]);
  }

  // (d) Every Page 01h descriptor MIF set to 0: app codes [1..8].
  {
    const p01 = new Uint8Array(zrPages.page01h);
    for (let i = 0; i < 7; i++) {
      p01[95 + i * 4 + 1] = 0;
    }
    const res = decodeCmis({ ...zrPages, page01h: p01 });
    const appCodes = (res.applications || []).map((a) => a.app_code);
    assert.deepStrictEqual(appCodes, Array.from({ length: 8 }, (_, i) => i + 1));
  }
});

test('module_state pins raw 0..7 to expected state names', () => {
  const zrPages = loadBuffers('fboss-cmis-5-3-osfp-800g-zr');
  const expectedStates = [
    'Reserved0',
    'ModuleLowPwr',
    'ModulePwrUp',
    'ModuleReady',
    'ModulePwrDn',
    'ModuleFault',
    'Unknown6',
    'Unknown7',
  ];
  for (let raw = 0; raw < 8; raw++) {
    const lower = new Uint8Array(zrPages.lower);
    lower[3] = (lower[3] & ~0x0e) | (raw << 1);
    const res = decodeCmis({ ...zrPages, lower });
    assert.strictEqual(res.decode_status, 'ok');
    assert.strictEqual(res.module.module_state, expectedStates[raw]);
  }
});
