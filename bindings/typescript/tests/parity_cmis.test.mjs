import fs from 'node:fs';
import path from 'node:path';
import test from 'node:test';
import assert from 'node:assert/strict';
import { fileURLToPath } from 'node:url';
import { decodeCmis } from '../dist/index.js';

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const repoRoot = path.resolve(__dirname, '../../..');
const goldenDir = path.join(repoRoot, 'tests/golden/cmis');
const fixtureDir = path.join(repoRoot, 'tests/fixtures/cmis');

const goldenFiles = fs.readdirSync(goldenDir)
  .filter((f) => f.endsWith('.json') && !f.startsWith('_'))
  .sort();

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
      const itemPath = `${currentPath}[${i}]`;
      diffs.push(...deepCompare(actual[i], expected[i], itemPath));
    }
    return diffs;
  }

  const isRoot = currentPath === '';
  const isModule = currentPath === 'module';

  const filterKey = (key) => {
    if (isRoot && key === '_metadata') return false;
    if (isModule && key === 'module_type') return false;
    return true;
  };

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
      const fieldPath = currentPath ? `${currentPath}.${key}` : key;
      diffs.push(...deepCompare(actual[key], expected[key], fieldPath));
    }
  }

  return diffs;
}

test('comparator self-test detects mutation with path label', () => {
  assert.ok(goldenFiles.length > 0, 'Must have at least one golden file for self-test');
  const sample = JSON.parse(fs.readFileSync(path.join(goldenDir, goldenFiles[0]), 'utf8'));
  const mutated = structuredClone(sample);
  if (mutated.identity && typeof mutated.identity === 'object') {
    mutated.identity.vendor_name = 'MUTATED_VENDOR_FOR_TEST';
    const diffs = deepCompare(mutated, sample);
    assert.ok(diffs.length > 0, 'Comparator self-test must detect mutation');
    assert.ok(
      diffs.some((d) => d.includes('identity.vendor_name')),
      `Expected diff mentioning identity.vendor_name, got: ${diffs.join('; ')}`
    );
  } else {
    mutated.__extra_test_key = 123;
    const diffs = deepCompare(mutated, sample);
    assert.ok(diffs.length > 0, 'Comparator self-test must detect mutation');
  }
});

// Pin the per-lane status latches (rx_los ← LOSFlagRx, Table 8-88; tx_fault ←
// FailureFlagTx, Table 8-87) on the one fixture whose Page 11h carries real
// latched values, independently of the deep golden compare:
// fboss-cmis-4-0-qsfp112-200g-fr4 has rx_los latched on lanes 1/3 and
// tx_fault on lanes 2/4, everything else clear.
test('CMIS lane status latches', () => {
  const stem = 'fboss-cmis-4-0-qsfp112-200g-fr4';
  const read = (suffix) =>
    new Uint8Array(fs.readFileSync(path.join(fixtureDir, `${stem}.${suffix}.bin`)));
  const result = decodeCmis({
    lower: read('lower'),
    page00h: read('page-00h'),
    page01h: read('page-01h'),
    page11h: read('page-11h'),
  });
  assert.ok(Array.isArray(result.lanes) && result.lanes.length === 8,
    `expected 8 lanes, got ${typeof result.lanes}`);
  for (let i = 0; i < 8; i++) {
    const idx = i + 1;
    assert.equal(result.lanes[i].rx_los, idx === 1 || idx === 3, `lane ${idx} rx_los`);
    assert.equal(result.lanes[i].tx_fault, idx === 2 || idx === 4, `lane ${idx} tx_fault`);
  }
});

test('CMIS golden parity', () => {
  assert.ok(goldenFiles.length > 0, 'Discovered goldens must be > 0');
  let executed = 0;

  for (const file of goldenFiles) {
    const stem = path.basename(file, '.json');
    const goldenPath = path.join(goldenDir, file);
    const expected = JSON.parse(fs.readFileSync(goldenPath, 'utf8'));

    const lowerPath = path.join(fixtureDir, `${stem}.lower.bin`);
    const page00hPath = path.join(fixtureDir, `${stem}.page-00h.bin`);

    if (!fs.existsSync(lowerPath)) {
      throw new Error(`Required fixture missing for golden ${file}: ${lowerPath}`);
    }
    if (!fs.existsSync(page00hPath)) {
      throw new Error(`Required fixture missing for golden ${file}: ${page00hPath}`);
    }

    const pages = {
      lower: new Uint8Array(fs.readFileSync(lowerPath)),
      page00h: new Uint8Array(fs.readFileSync(page00hPath)),
    };

    const page01hPath = path.join(fixtureDir, `${stem}.page-01h.bin`);
    if (fs.existsSync(page01hPath)) {
      pages.page01h = new Uint8Array(fs.readFileSync(page01hPath));
    }

    const page11hPath = path.join(fixtureDir, `${stem}.page-11h.bin`);
    if (fs.existsSync(page11hPath)) {
      pages.page11h = new Uint8Array(fs.readFileSync(page11hPath));
    }

    // VDM pages (CMIS § 8.27), loaded when the fixture carries them: the
    // complete set gates the `vdm` block in decodeCmis. 2Eh is not part of
    // the VDM feature and has no slice.
    for (const tag of [
      '20h', '21h', '22h', '23h', '24h', '25h', '26h', '27h',
      '28h', '29h', '2ah', '2bh', '2ch', '2dh', '2fh',
    ]) {
      const vdmPath = path.join(fixtureDir, `${stem}.page-${tag}.bin`);
      if (fs.existsSync(vdmPath)) {
        pages[`page${tag}`] = new Uint8Array(fs.readFileSync(vdmPath));
      }
    }

    const actual = decodeCmis(pages);
    executed++;

    if (actual.decode_status === 'ok') {
      assert.strictEqual(
        actual.module?.module_type,
        null,
        `module.module_type must be null for ok result in ${file}`
      );
    }

    const normalizedActual = JSON.parse(JSON.stringify(actual));
    const diffs = deepCompare(normalizedActual, expected);
    assert.strictEqual(
      diffs.length,
      0,
      `Parity mismatch in ${file}:\n${diffs.join('\n')}`
    );
  }

  assert.strictEqual(
    executed,
    goldenFiles.length,
    `Executed count (${executed}) must equal discovered count (${goldenFiles.length})`
  );
});
