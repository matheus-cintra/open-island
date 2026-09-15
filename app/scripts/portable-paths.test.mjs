import { test } from 'node:test';
import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { resolve } from 'node:path';
import { existsSync } from 'node:fs';

const script = resolve(import.meta.dirname, 'portable-build.mjs');
function query(...args) {
  const result = spawnSync(process.execPath, [script, 'paths', ...args], { encoding: 'utf8' });
  assert.equal(result.status, 0, result.stderr);
  return result.stdout.trim();
}
test('artifact paths use the same target and separate QA policy directories', () => {
  for (const target of ['x86_64-unknown-linux-gnu', 'aarch64-apple-darwin', 'x86_64-apple-darwin']) {
    const normal = JSON.parse(query('--target', target));
    const qa = JSON.parse(query('--target', target, '--features=qa-webdriver'));
    assert.equal(normal.target, target);
    assert.equal(normal.qa, false);
    assert.equal(qa.qa, true);
    assert.match(normal.targetDirectory, /\/target\/portable\//);
    assert.match(qa.targetDirectory, /\/target\/portable-qa\//);
    assert.equal(normal.releaseDirectory, resolve(normal.targetDirectory, target, 'release'));
    assert.equal(qa.releaseDirectory, resolve(qa.targetDirectory, target, 'release'));
    assert.equal(query('--target', target, '--format=github-env'), `OPEN_ISLAND_RELEASE_DIR=${normal.releaseDirectory}`);
    const existed = existsSync(qa.releaseDirectory);
    query('--target', target, '--features=qa-webdriver');
    assert.equal(existsSync(qa.releaseDirectory), existed, 'query must not create build artifacts');
  }
});
test('path query refuses an unsupported target before reporting an artifact directory', () => {
  const result = spawnSync(process.execPath, [script, 'paths', '--target', 'aarch64-unknown-linux-gnu'], { encoding: 'utf8' });
  assert.notEqual(result.status, 0);
  assert.match(result.stderr, /unsupported portable target/);
  assert.equal(result.stdout, '');
});
