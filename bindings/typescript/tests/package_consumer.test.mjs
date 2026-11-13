import fs from 'node:fs';
import path from 'node:path';
import test from 'node:test';
import assert from 'node:assert/strict';
import os from 'node:os';
import { execFileSync, execSync } from 'node:child_process';
import { createRequire } from 'node:module';
import { fileURLToPath } from 'node:url';

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const pkgRoot = path.resolve(__dirname, '..');
const repoRoot = path.resolve(pkgRoot, '../..');
const tscBin = path.join(pkgRoot, 'node_modules/.bin/tsc');

test('packed package consumer tests', () => {
  const tmpDir = fs.mkdtempSync(path.join(os.tmpdir(), 'pkg-consumer-'));
  try {
    // 1. npm pack --pack-destination <tmp>
    execSync(`npm pack --pack-destination "${tmpDir}"`, {
      cwd: pkgRoot,
      stdio: 'pipe',
    });

    const tgzName = fs.readdirSync(tmpDir).find((f) => f.endsWith('.tgz'));
    assert.ok(tgzName, 'Expected .tgz file from npm pack');
    const tgzPath = path.join(tmpDir, tgzName);

    // Create consumer
    const consumerDir = path.join(tmpDir, 'consumer');
    fs.mkdirSync(consumerDir, { recursive: true });
    fs.writeFileSync(
      path.join(consumerDir, 'package.json'),
      JSON.stringify({ private: true, type: 'module' }, null, 2)
    );

    const destDir = path.join(consumerDir, 'node_modules/@openxcvr/transceivers');
    fs.mkdirSync(destDir, { recursive: true });

    // Extract tarball
    execFileSync('tar', ['-xzf', tgzPath, '--strip-components=1', '-C', destDir], {
      stdio: 'pipe',
    });

    // Check (1): consumer .mjs decodes fboss-cmis-5-3-osfp-800g-zr to golden
    const consumerMjs = `
import fs from 'node:fs';
import path from 'node:path';
import assert from 'node:assert/strict';
import { decodeCmis, decodeSff8636, decodeSff8472 } from '@openxcvr/transceivers';

const fixtureStem = 'fboss-cmis-5-3-osfp-800g-zr';
const repoRoot = ${JSON.stringify(repoRoot)};
const fixturesDir = path.join(repoRoot, 'tests/fixtures/cmis');
const goldenPath = path.join(repoRoot, 'tests/golden/cmis', fixtureStem + '.json');

const pages = {
  lower: new Uint8Array(fs.readFileSync(path.join(fixturesDir, fixtureStem + '.lower.bin'))),
  page00h: new Uint8Array(fs.readFileSync(path.join(fixturesDir, fixtureStem + '.page-00h.bin'))),
  page01h: new Uint8Array(fs.readFileSync(path.join(fixturesDir, fixtureStem + '.page-01h.bin'))),
  page11h: new Uint8Array(fs.readFileSync(path.join(fixturesDir, fixtureStem + '.page-11h.bin'))),
};

const result = decodeCmis(pages);
assert.strictEqual(result.decode_status, 'ok');
assert.strictEqual(result.module.module_type, null);

const expected = JSON.parse(fs.readFileSync(goldenPath, 'utf8'));

function deepCompare(actual, expected, currentPath = '', skipModuleType = true) {
  if (actual === null || expected === null || typeof actual !== 'object' || typeof expected !== 'object') {
    if (actual !== expected) {
      return ['Mismatch at ' + (currentPath || '<root>') + ': expected ' + JSON.stringify(expected) + ', got ' + JSON.stringify(actual)];
    }
    return [];
  }
  if (Array.isArray(actual) !== Array.isArray(expected)) {
    return ['Mismatch at ' + (currentPath || '<root>') + ': array vs non-array'];
  }
  if (Array.isArray(actual)) {
    if (actual.length !== expected.length) {
      return ['Mismatch at ' + (currentPath || '<root>') + ': length expected ' + expected.length + ', got ' + actual.length];
    }
    const diffs = [];
    for (let i = 0; i < actual.length; i++) {
      diffs.push(...deepCompare(actual[i], expected[i], currentPath + '[' + i + ']', skipModuleType));
    }
    return diffs;
  }
  const isRoot = currentPath === '';
  const isModule = currentPath === 'module';
  const filterKey = (key) => !(isRoot && key === '_metadata') && !(skipModuleType && isModule && key === 'module_type');
  const actualKeys = Object.keys(actual).filter(filterKey).sort();
  const expectedKeys = Object.keys(expected).filter(filterKey).sort();
  const diffs = [];
  for (const k of expectedKeys) {
    if (!Object.prototype.hasOwnProperty.call(actual, k)) diffs.push('Missing key: ' + k);
  }
  for (const k of actualKeys) {
    if (!Object.prototype.hasOwnProperty.call(expected, k)) diffs.push('Unexpected key: ' + k);
  }
  for (const k of actualKeys) {
    if (Object.prototype.hasOwnProperty.call(expected, k)) {
      diffs.push(...deepCompare(actual[k], expected[k], currentPath ? currentPath + '.' + k : k, skipModuleType));
    }
  }
  return diffs;
}

const diffs = deepCompare(JSON.parse(JSON.stringify(result)), expected);
assert.strictEqual(diffs.length, 0, 'Parity diffs in consumer: ' + diffs.join('; '));

// SFF-8636 packed-consumer check: decode to golden (module_type compared too).
const sff8636Stem = 'finisar-ftlc9551repm-100g-sr4';
const sff8636Result = decodeSff8636({
  lower: new Uint8Array(fs.readFileSync(path.join(repoRoot, 'tests/fixtures/sff-8636', sff8636Stem + '.lower.bin'))),
  page00h: new Uint8Array(fs.readFileSync(path.join(repoRoot, 'tests/fixtures/sff-8636', sff8636Stem + '.page-00h.bin'))),
});
const sff8636Expected = JSON.parse(fs.readFileSync(path.join(repoRoot, 'tests/golden/sff-8636', sff8636Stem + '.json'), 'utf8'));
const sff8636Diffs = deepCompare(JSON.parse(JSON.stringify(sff8636Result)), sff8636Expected, '', false);
assert.strictEqual(sff8636Diffs.length, 0, 'SFF-8636 parity diffs in consumer: ' + sff8636Diffs.join('; '));

// SFF-8472 packed-consumer check on the externally calibrated module: the
// uncalibrated telemetry contract (raw x fixed scale) is the golden shape.
const sff8472Stem = 'synthetic-extcal-sfp10g-sr';
const sff8472Result = decodeSff8472({
  a0h: new Uint8Array(fs.readFileSync(path.join(repoRoot, 'tests/fixtures/sff-8472', sff8472Stem + '.a0h.bin'))),
  a2h: new Uint8Array(fs.readFileSync(path.join(repoRoot, 'tests/fixtures/sff-8472', sff8472Stem + '.a2h.bin'))),
});
assert.strictEqual(sff8472Result.module.calibration.externally_calibrated, true);
const sff8472Expected = JSON.parse(fs.readFileSync(path.join(repoRoot, 'tests/golden/sff-8472', sff8472Stem + '.json'), 'utf8'));
const sff8472Diffs = deepCompare(JSON.parse(JSON.stringify(sff8472Result)), sff8472Expected, '', false);
assert.strictEqual(sff8472Diffs.length, 0, 'SFF-8472 parity diffs in consumer: ' + sff8472Diffs.join('; '));
`;

    fs.writeFileSync(path.join(consumerDir, 'consumer.mjs'), consumerMjs);
    execFileSync(process.execPath, ['consumer.mjs'], { cwd: consumerDir, stdio: 'pipe' });

    // Check (2): require("@openxcvr/transceivers") fails with ERR_PACKAGE_PATH_NOT_EXPORTED
    const consumerRequire = createRequire(path.join(consumerDir, 'dummy.js'));
    let requireErr;
    try {
      consumerRequire('@openxcvr/transceivers');
    } catch (err) {
      requireErr = err;
    }
    assert.ok(requireErr, 'require should throw');
    assert.strictEqual(requireErr.code, 'ERR_PACKAGE_PATH_NOT_EXPORTED');

    // Check (3): compile consumer.ts with tsc
    const consumerTs = `
import { decodeCmis, type CmisDecodeResult } from "@openxcvr/transceivers";

const dummy = new Uint8Array(128);
const result: CmisDecodeResult = decodeCmis({ lower: dummy, page00h: dummy });

if (result.decode_status === "ok") {
  const modType: string | null = result.module.module_type;
  if (modType !== null) {
    console.log(modType);
  }
}
`;
    fs.writeFileSync(path.join(consumerDir, 'consumer.ts'), consumerTs);
    execFileSync(
      tscBin,
      ['--strict', '--module', 'nodenext', '--moduleResolution', 'nodenext', '--noEmit', 'consumer.ts'],
      { cwd: consumerDir, stdio: 'pipe' }
    );

    // Check (4): compile consumer-negative.ts must fail naming each of the 3 lines
    const line1Comment = '// line 1: identity without narrowing';
    const line2Comment = '// line 2: applications assigned to non-null array';
    const line3Comment = '// line 3: decodeCmis called with number[]';

    const consumerNegativeTs = `
import { decodeCmis, type CmisDecodeResult, type CmisApplication } from "@openxcvr/transceivers";

const dummy = new Uint8Array(128);
const result: CmisDecodeResult = decodeCmis({ lower: dummy, page00h: dummy });

${line1Comment}
const ident = result.identity;

if (result.decode_status === "ok") {
  ${line2Comment}
  const apps: CmisApplication[] = result.applications;
}

${line3Comment}
const nums: number[] = [1, 2, 3];
decodeCmis(nums);
`;
    fs.writeFileSync(path.join(consumerDir, 'consumer-negative.ts'), consumerNegativeTs);

    const negLines = consumerNegativeTs.split('\n');
    const line1Num = negLines.findIndex((l) => l.includes('const ident = result.identity;')) + 1;
    const line2Num = negLines.findIndex((l) => l.includes('const apps: CmisApplication[] = result.applications;')) + 1;
    const line3Num = negLines.findIndex((l) => l.includes('decodeCmis(nums);')) + 1;

    let negErr;
    try {
      execFileSync(
        tscBin,
        ['--strict', '--module', 'nodenext', '--moduleResolution', 'nodenext', '--noEmit', 'consumer-negative.ts'],
        { cwd: consumerDir, stdio: 'pipe' }
      );
    } catch (err) {
      negErr = err;
    }
    assert.ok(negErr, 'consumer-negative.ts should fail tsc compilation');
    const diagOutput = (negErr.stdout ? negErr.stdout.toString() : '') + (negErr.stderr ? negErr.stderr.toString() : '');

    assert.ok(
      diagOutput.includes(`consumer-negative.ts(${line1Num},`),
      `Expected diagnostic for line ${line1Num} in output:\n${diagOutput}`
    );
    assert.ok(
      diagOutput.includes(`consumer-negative.ts(${line2Num},`),
      `Expected diagnostic for line ${line2Num} in output:\n${diagOutput}`
    );
    assert.ok(
      diagOutput.includes(`consumer-negative.ts(${line3Num},`),
      `Expected diagnostic for line ${line3Num} in output:\n${diagOutput}`
    );
  } finally {
    fs.rmSync(tmpDir, { recursive: true, force: true });
  }
});
