import fs from 'node:fs';
import path from 'node:path';
import test from 'node:test';
import assert from 'node:assert/strict';
import os from 'node:os';
import { execFileSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const pkgRoot = path.resolve(__dirname, '..');
const binPath = path.join(pkgRoot, 'node_modules/.bin/openapi-typescript');
const schemaPath = path.resolve(pkgRoot, '../openapi/openxcvr.openapi.yaml');
const committedPath = path.join(pkgRoot, 'generated/openapi/cmis.ts');

test('openapi types drift check', () => {
  const tmpDir = fs.mkdtempSync(path.join(os.tmpdir(), 'openapi-types-'));
  try {
    const tmpOut = path.join(tmpDir, 'cmis.ts');
    execFileSync(binPath, [schemaPath, '-o', tmpOut], {
      cwd: pkgRoot,
      encoding: 'utf8',
      stdio: ['pipe', 'pipe', 'pipe'],
    });

    const generated = fs.readFileSync(tmpOut);
    const committed = fs.readFileSync(committedPath);

    assert.ok(
      Buffer.compare(generated, committed) === 0,
      'generated/openapi/cmis.ts is stale compared to bindings/openapi/openxcvr.openapi.yaml. Run "npm run gen:types" to regenerate.'
    );
  } finally {
    fs.rmSync(tmpDir, { recursive: true, force: true });
  }
});
