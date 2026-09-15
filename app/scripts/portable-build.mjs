import { audit } from './portable-audit.mjs';
import { qaBuild } from './portable-build-options.mjs';
import { spawnSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import { existsSync, readFileSync, mkdirSync, writeFileSync } from 'node:fs';
import { resolve, dirname } from 'node:path';
import { fileURLToPath } from 'node:url';
const root = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const [program, ...args] = process.argv.slice(2);
if (!program) throw new Error('usage: portable-build.mjs <program> [arguments]');
const host = spawnSync('rustc', ['-vV'], { encoding: 'utf8' });
if (host.status !== 0) throw new Error('rustc host unavailable');
const index = args.findIndex(arg => arg === '--target' || arg === '-t');
const target = (index >= 0 ? args[index + 1] : args.find(arg => arg.startsWith('--target='))?.slice(9))
  ?? process.env.CARGO_BUILD_TARGET ?? host.stdout.match(/^host: (.+)$/m)?.[1];
const x86 = /^x86_64-(unknown-linux-gnu|apple-darwin)$/.test(target ?? '');
const arm = target === 'aarch64-apple-darwin';
if (!x86 && !arm) throw new Error('unsupported portable target');
const off = ['NATIVE','SSE42','AVX','AVX2','BMI2','AVX_VNNI','AVX512','AVX512_VBMI','AVX512_VNNI','AVX512_BF16','FMA','F16C','AMX_TILE','AMX_INT8','AMX_BF16','CPU_ALL_VARIANTS','CPU_KLEIDIAI','OPENMP','CUDA','VULKAN','METAL','HIP','SYCL','BLAS'];
const policy = Object.fromEntries(off.map(key => [`GGML_${key}`, 'OFF']));
Object.assign(policy, { CMAKE_EXPORT_COMPILE_COMMANDS: 'ON', WHISPER_COREML: 'OFF', GGML_ACCELERATE: target.endsWith('apple-darwin') ? 'ON' : 'OFF' });
if (arm) policy.GGML_CPU_ARM_ARCH = 'armv8-a';
const env = { ...process.env };
if (args.some(arg => /target-(?:cpu|feature)|-m(?:arch|cpu|avx|sse)|(?:cuda|vulkan|metal|coreml|openmp)/i.test(arg))) throw new Error('conflicting command-line CPU/GPU option');
for (const [key, value] of Object.entries(policy)) {
  if (env[key] !== undefined && env[key] !== value) throw new Error(`conflicting portable option: ${key}`);
}
for (const key of Object.keys(env)) {
  if (/^(CFLAGS|CXXFLAGS|RUSTFLAGS|CARGO_ENCODED_RUSTFLAGS|CMAKE_|GGML_|WHISPER_)/.test(key) || /^CARGO_TARGET_.*_RUSTFLAGS$/.test(key) || /^(?:HOST|TARGET)_(?:CFLAGS|CXXFLAGS)$/.test(key)) delete env[key];
}
// Keep the build self-contained when the QA toolchain is staged in the
// repository.  Callers may still provide a system toolchain; this only adds
// the relative, private cache location when it exists.
const localTools = resolve(root, 'target', 'qa-tools', 'bin');
if (existsSync(localTools)) env.PATH = `${localTools}${process.platform === 'win32' ? ';' : ':'}${env.PATH ?? ''}`;
Object.assign(env, policy, { CFLAGS: x86 ? '-march=x86-64 -mtune=generic' : '-march=armv8-a', CXXFLAGS: x86 ? '-march=x86-64 -mtune=generic' : '-march=armv8-a', RUSTFLAGS: `-C target-cpu=${x86 ? 'x86-64' : 'generic'}`, CARGO_BUILD_TARGET: target });
const lock = readFileSync(resolve(root, 'Cargo.lock'), 'utf8');
if (!/name = "whisper-rs-sys"\nversion = "0\.15\.0"/.test(lock)) throw new Error('unexpected whisper-rs-sys version');
const hash = createHash('sha256').update(JSON.stringify({ target, policy, c: env.CFLAGS, rust: env.RUSTFLAGS })).digest('hex').slice(0, 16);
const qa = qaBuild(args);
const directory = resolve(root, 'target', qa ? 'portable-qa' : 'portable', target, hash);
env.CARGO_TARGET_DIR = directory;
if (program === 'paths') {
  const paths = { target, qa, targetDirectory: directory, releaseDirectory: resolve(directory, target, 'release') };
  if (args.includes('--format=github-env')) {
    console.log(`OPEN_ISLAND_RELEASE_DIR=${paths.releaseDirectory}`);
  } else {
    console.log(JSON.stringify(paths));
  }
  process.exit(0);
}
mkdirSync(directory, { recursive: true });
writeFileSync(resolve(directory, 'policy.json'), JSON.stringify({ target, policy, cflags: env.CFLAGS, rustflags: env.RUSTFLAGS, qa }, null, 2));
const commandArgs = program === 'tauri' && ['build', 'dev'].includes(args[0]) && index < 0 && !args.some(arg => arg.startsWith('--target='))
  ? [...args, '--target', target] : args;
const result = spawnSync(program, commandArgs, { cwd: root, env, stdio: 'inherit' });
if (result.error) throw result.error;
if (result.status === 0) {
  const report = audit(directory, target, policy);
  const nativeCommand = args.some(arg => ['build', 'test', 'check', 'clippy'].includes(arg));
  const appScope = program === 'tauri' || args.includes('--workspace') || args.some(arg => /^(?:-p|--package=)?open-island$/.test(arg));
  if (nativeCommand && appScope && !report.caches) throw new Error('missing native build evidence');
}
process.exit(result.status ?? 1);
