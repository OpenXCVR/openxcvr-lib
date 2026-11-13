import fs from 'node:fs';
import path from 'node:path';
import test from 'node:test';
import assert from 'node:assert/strict';
import { fileURLToPath } from 'node:url';
import { decodeSff8636, decodeSff8472 } from '../dist/index.js';

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const repoRoot = path.resolve(__dirname, '../../..');

// Golden family -> golden dir, fixture dir, fixture suffixes (per brief), and
// the pages object the decoder takes.
const FAMILIES = [
  {
    family: 'sff-8636',
    goldenDir: path.join(repoRoot, 'tests/golden/sff-8636'),
    fixtureDir: path.join(repoRoot, 'tests/fixtures/sff-8636'),
    suffixes: ['lower', 'page-00h'],
    decode: (pages) => decodeSff8636(pages),
  },
  {
    family: 'sff-8472',
    goldenDir: path.join(repoRoot, 'tests/golden/sff-8472'),
    fixtureDir: path.join(repoRoot, 'tests/fixtures/sff-8472'),
    suffixes: ['a0h', 'a2h'],
    decode: (pages) => decodeSff8472(pages),
  },
];

function deepCompare(actual, expected, currentPath = '') {
  if (actual === null || expected === null || typeof actual !== 'object' || typeof expected !== 'object') {
    if (actual !== expected) {
      return [`Mismatch at ${currentPath || '<root>'}: expected ${JSON.stringify(expected)}, got ${JSON.stringify(actual)}`];
    }
    return [];
  }

  if (Array.isArray(actual) !== Array.isArray(expected)) {
    return [`Mismatch at ${currentPath || '<root>'}: array vs non-array (actual: ${Array.isArray(actual)}, expected: ${Array.isArray(expected)})`];
  }

  if (Array.isArray(actual)) {
    if (actual.length !== expected.length) {
      return [`Mismatch at ${currentPath || '<root>'}: length expected ${expected.length}, got ${actual.length}`];
    }
    const diffs = [];
    for (let i = 0; i < actual.length; i++) {
      diffs.push(...deepCompare(actual[i], expected[i], `${currentPath}[${i}]`));
    }
    return diffs;
  }

  // SFF parity skips only the root _metadata key: both decoders implement
  // module.module_type, so it is compared like every other field.
  const filterKey = (key) => !(currentPath === '' && key === '_metadata');

  const actualKeys = Object.keys(actual).filter(filterKey).sort();
  const expectedKeys = Object.keys(expected).filter(filterKey).sort();

  const diffs = [];

  const missingInActual = expectedKeys.filter((k) => !Object.prototype.hasOwnProperty.call(actual, k));
  const unexpectedInActual = actualKeys.filter((k) => !Object.prototype.hasOwnProperty.call(expected, k));

  if (missingInActual.length > 0) {
    diffs.push(`Missing keys at ${currentPath || '<root>'}: ${missingInActual.join(', ')}`);
  }
  if (unexpectedInActual.length > 0) {
    diffs.push(`Unexpected keys at ${currentPath || '<root>'}: ${unexpectedInActual.join(', ')}`);
  }

  for (const key of actualKeys) {
    if (Object.prototype.hasOwnProperty.call(expected, key)) {
      diffs.push(...deepCompare(actual[key], expected[key], currentPath ? `${currentPath}.${key}` : key));
    }
  }

  return diffs;
}

for (const { family, goldenDir, fixtureDir, suffixes, decode } of FAMILIES) {
  test(`${family} golden parity`, () => {
    const goldenFiles = fs.readdirSync(goldenDir)
      .filter((f) => f.endsWith('.json') && !f.startsWith('_'))
      .sort();
    assert.ok(goldenFiles.length > 0, `Discovered ${family} goldens must be > 0`);
    let executed = 0;

    for (const file of goldenFiles) {
      const stem = path.basename(file, '.json');
      const expected = JSON.parse(fs.readFileSync(path.join(goldenDir, file), 'utf8'));

      const pageBuffers = suffixes.map((suffix) => {
        const fixturePath = path.join(fixtureDir, `${stem}.${suffix}.bin`);
        if (!fs.existsSync(fixturePath)) {
          throw new Error(`Required fixture missing for golden ${file}: ${fixturePath}`);
        }
        return new Uint8Array(fs.readFileSync(fixturePath));
      });

      const actual = decode({
        lower: pageBuffers[0],
        page00h: pageBuffers[1],
        a0h: pageBuffers[0],
        a2h: pageBuffers[1],
      });
      executed++;

      const diffs = deepCompare(JSON.parse(JSON.stringify(actual)), expected);
      assert.strictEqual(diffs.length, 0, `Parity mismatch in ${family}/${file}:\n${diffs.join('\n')}`);
    }

    assert.strictEqual(
      executed,
      goldenFiles.length,
      `${family}: executed count (${executed}) must equal discovered count (${goldenFiles.length})`
    );
  });
}

test('SFF-8636 alarm-only interrupt witness (design-r3 R2)', () => {
  const fixtureDir = path.join(repoRoot, 'tests/fixtures/sff-8636');
  const lower = new Uint8Array(fs.readFileSync(path.join(fixtureDir, 'finisar-ftl410qe3c-40g-sr4.lower.bin')));
  const page00h = new Uint8Array(fs.readFileSync(path.join(fixtureDir, 'finisar-ftl410qe3c-40g-sr4.page-00h.bin')));

  // Copy of the fixture's lower page with the LOS byte (byte 3) cleared and
  // the lane alarm/warning nibble bytes 9-14 set nonzero: interrupt_asserted
  // must then be driven by the alarm nibbles alone.
  lower[3] = 0;
  for (let i = 9; i <= 14; i++) {
    lower[i] = 0x11;
  }

  const result = decodeSff8636({ lower, page00h });

  // Expected value computed once with the Python facade on the same bytes:
  //   cd bindings/python && python3 -c '
  //   from openxcvr import decode_sff8636
  //   lower = bytearray(open("../../tests/fixtures/sff-8636/finisar-ftl410qe3c-40g-sr4.lower.bin","rb").read())
  //   lower[3] = 0
  //   for i in range(9, 15): lower[i] = 0x11
  //   pages = {"lower": bytes(lower), "00h": open("../../tests/fixtures/sff-8636/finisar-ftl410qe3c-40g-sr4.page-00h.bin","rb").read()}
  //   print(decode_sff8636(pages)["flags"]["interrupt_asserted"])'
  // which printed: interrupt_asserted: True
  assert.strictEqual(result.flags.interrupt_asserted, true);
});

test('SFF-8636 interrupt witness, zeroed alarm nibbles (design-r3 R2 second half)', () => {
  const fixtureDir = path.join(repoRoot, 'tests/fixtures/sff-8636');
  const lower = new Uint8Array(fs.readFileSync(path.join(fixtureDir, 'finisar-ftl410qe3c-40g-sr4.lower.bin')));
  const page00h = new Uint8Array(fs.readFileSync(path.join(fixtureDir, 'finisar-ftl410qe3c-40g-sr4.page-00h.bin')));

  // Copy of the fixture's lower page with the LOS byte (byte 3) cleared and
  // the lane alarm/warning nibble bytes 9-14 also zeroed: no interrupt source
  // remains, so interrupt_asserted must be false.
  lower[3] = 0;
  for (let i = 9; i <= 14; i++) {
    lower[i] = 0;
  }

  const result = decodeSff8636({ lower, page00h });

  // Expected value computed once with the Python facade on the same bytes:
  //   cd bindings/python && python3 -c '
  //   from openxcvr import decode_sff8636
  //   lower = bytearray(open("../../tests/fixtures/sff-8636/finisar-ftl410qe3c-40g-sr4.lower.bin","rb").read())
  //   lower[3] = 0
  //   for i in range(9, 15): lower[i] = 0
  //   pages = {"lower": bytes(lower), "00h": open("../../tests/fixtures/sff-8636/finisar-ftl410qe3c-40g-sr4.page-00h.bin","rb").read()}
  //   print(decode_sff8636(pages)["flags"]["interrupt_asserted"])'
  // which printed: interrupt_asserted: False
  assert.strictEqual(result.flags.interrupt_asserted, false);
});
