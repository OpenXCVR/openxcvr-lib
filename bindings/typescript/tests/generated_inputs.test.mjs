import test from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import {
  CmisMediaInterfaceName,
  CmisDataPathStateForRevision,
  decodeCmisPage00hAdministrative,
} from '../dist/index.js';

const INVALID_INPUTS = [-1, 256, 1.5, NaN, Infinity, -Infinity];

test('CmisMediaInterfaceName returns null for invalid inputs', () => {
  for (const val of INVALID_INPUTS) {
    assert.strictEqual(CmisMediaInterfaceName(val, 1), null, `expected null for mediaType=${val}`);
    assert.strictEqual(CmisMediaInterfaceName(1, val), null, `expected null for code=${val}`);
    assert.strictEqual(CmisMediaInterfaceName(val, val), null, `expected null for both=${val}`);
  }
});

test('CmisDataPathStateForRevision returns null for invalid rev bytes', () => {
  for (const val of INVALID_INPUTS) {
    assert.strictEqual(CmisDataPathStateForRevision(val), null, `expected null for rev=${val}`);
  }
});

test('CmisDataPathStateForRevision returned projections return null for invalid raw inputs', () => {
  const revisions = [0x30, 0x40, 0x50, 0x51, 0x52, 0x53];
  for (const rev of revisions) {
    const proj = CmisDataPathStateForRevision(rev);
    assert.ok(typeof proj === 'function', `expected projection function for rev=0x${rev.toString(16)}`);
    for (const val of INVALID_INPUTS) {
      assert.strictEqual(proj(val), null, `expected null for proj(rev=0x${rev.toString(16)}, raw=${val})`);
    }
  }
});

test('decodeCmisPage00hAdministrative trims trailing ASCII spaces only', () => {
  const makeBuf = (str) => {
    const buf = new Uint8Array(128);
    Buffer.from(str, 'latin1').copy(buf, 1);
    return buf;
  };

  // "AB C" + 12 spaces -> "AB C"
  assert.strictEqual(decodeCmisPage00hAdministrative(makeBuf("AB C" + " ".repeat(12))).vendorName, "AB C");

  // 16 spaces -> ""
  assert.strictEqual(decodeCmisPage00hAdministrative(makeBuf(" ".repeat(16))).vendorName, "");

  // "AB\t" + 13 spaces -> "AB\t"
  assert.strictEqual(decodeCmisPage00hAdministrative(makeBuf("AB\t" + " ".repeat(13))).vendorName, "AB\t");

  // "AB" + "\u0000" + 13 spaces -> "AB\u0000"
  assert.strictEqual(decodeCmisPage00hAdministrative(makeBuf("AB\u0000" + " ".repeat(13))).vendorName, "AB\u0000");

  // 16 non-space chars unchanged
  assert.strictEqual(decodeCmisPage00hAdministrative(makeBuf("0123456789ABCDEF")).vendorName, "0123456789ABCDEF");
});

test('no file in bindings/typescript/generated contains regex-based trim patterns', () => {
  const generatedDir = fileURLToPath(new URL('../generated', import.meta.url));
  function findTsFiles(dir) {
    const entries = fs.readdirSync(dir, { withFileTypes: true });
    let files = [];
    for (const entry of entries) {
      const fullPath = path.join(dir, entry.name);
      if (entry.isDirectory()) {
        files = files.concat(findTsFiles(fullPath));
      } else if (entry.isFile() && entry.name.endsWith('.ts')) {
        files.push(fullPath);
      }
    }
    return files;
  }

  const tsFiles = findTsFiles(generatedDir);
  assert.ok(tsFiles.length >= 4, `expected at least 4 generated .ts files, found ${tsFiles.length}`);
  const offending = [];
  for (const file of tsFiles) {
    const content = fs.readFileSync(file, 'utf8');
    if (content.includes('.replace(/') || content.includes('RegExp(') || content.includes(' +$/')) {
      offending.push(path.relative(generatedDir, file));
    }
  }
  assert.deepStrictEqual(offending, [], `Found regex pattern in generated files: ${offending.join(', ')}`);
});
