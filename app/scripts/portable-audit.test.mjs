import { test } from 'node:test';
import assert from 'node:assert/strict';
import { mkdtempSync, mkdirSync, writeFileSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { audit } from './portable-audit.mjs';
test('native audit rejects compiler extensions even when CMake native is off', () => {
  const root = mkdtempSync(join(tmpdir(), 'oi-isa-'));
  try {
    const build = join(root, 'whisper-rs-sys-fixture', 'out', 'build');
    mkdirSync(build, { recursive: true });
    writeFileSync(join(build, 'CMakeCache.txt'), 'GGML_NATIVE:BOOL=OFF\n');
    const commands = flag => writeFileSync(join(build, 'compile_commands.json'), JSON.stringify([{ command: `cc -m64 -march=x86-64 ${flag} -c ggml.c` }]));
    commands('-mtune=generic');
    assert.equal(audit(root, 'x86_64-unknown-linux-gnu', { GGML_NATIVE: 'OFF' }).compilation_units, 1);
    for (const flag of ['-march=native', '-mavx2', '-mfma', '-mf16c', '-msse4.2', '-mcpu=haswell']) {
      commands(flag);
      assert.throws(() => audit(root, 'x86_64-unknown-linux-gnu', { GGML_NATIVE: 'OFF' }), /nonbaseline/);
    }
    commands('-mtune=generic');
    writeFileSync(join(build, 'CMakeCache.txt'), 'GGML_NATIVE:BOOL=ON\n');
    assert.throws(() => audit(root, 'x86_64-unknown-linux-gnu', { GGML_NATIVE: 'OFF' }), /mismatch/);
  } finally { rmSync(root, { recursive: true, force: true }); }
});
test('native audit accepts only the pinned macOS deployment target on Apple targets', () => {
  const root = mkdtempSync(join(tmpdir(), 'oi-isa-'));
  try {
    const build = join(root, 'whisper-rs-sys-fixture', 'out', 'build');
    mkdirSync(build, { recursive: true });
    writeFileSync(join(build, 'CMakeCache.txt'), 'GGML_NATIVE:BOOL=OFF\n');
    const commands = flag => writeFileSync(join(build, 'compile_commands.json'), JSON.stringify([{ command: `cc -march=armv8-a ${flag} -c ggml.c` }]));
    commands('-mmacosx-version-min=12.0');
    assert.equal(audit(root, 'aarch64-apple-darwin', { GGML_NATIVE: 'OFF' }).compilation_units, 1);
    commands('-mmacosx-version-min=11.0');
    assert.throws(() => audit(root, 'aarch64-apple-darwin', { GGML_NATIVE: 'OFF' }), /nonbaseline/);
    commands('-mmacosx-version-min=12.0');
    assert.throws(() => audit(root, 'x86_64-unknown-linux-gnu', { GGML_NATIVE: 'OFF' }), /nonbaseline/);
  } finally { rmSync(root, { recursive: true, force: true }); }
});
