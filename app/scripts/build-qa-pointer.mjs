import { spawnSync } from 'node:child_process';
import { mkdirSync } from 'node:fs';
import { resolve } from 'node:path';

if (process.platform !== 'linux') throw new Error('The private Wayland pointer helper requires Linux.');
const root = resolve(import.meta.dirname, '..');
const build = resolve(root, 'target/qa-pointer');
const protocol = resolve(root, 'scripts/qa/protocols/wlr-virtual-pointer-unstable-v1.xml');
mkdirSync(build, { recursive: true });
for (const [program, args] of [
  ['wayland-scanner', ['client-header', protocol, resolve(build, 'virtual-pointer.h')]],
  ['wayland-scanner', ['private-code', protocol, resolve(build, 'virtual-pointer.c')]],
  ['cc', ['-Wall', '-Wextra', '-Werror', `-I${build}`, resolve(root, 'scripts/qa/pointer.c'), resolve(build, 'virtual-pointer.c'), '-lwayland-client', '-o', resolve(build, 'pointer')]],
]) {
  const result = spawnSync(program, args, { stdio: 'inherit' });
  if (result.error || result.status !== 0) throw new Error(`QA pointer build failed: ${program}`);
}
console.log('Built target/qa-pointer/pointer');
