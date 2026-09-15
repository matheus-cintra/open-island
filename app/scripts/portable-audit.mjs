import { readFileSync, readdirSync, writeFileSync } from 'node:fs';
import { join, relative } from 'node:path';
import { createHash } from 'node:crypto';
export function audit(directory, target, policy) {
  const files = [];
  function walk(path) {
    for (const entry of readdirSync(path, { withFileTypes: true })) {
      const file = join(path, entry.name);
      if (entry.isDirectory()) walk(file);
      else if (entry.isFile() && /(?:CMakeCache.txt|compile_commands.json|lib(?:ggml[^/]*|whisper)\.a)$/.test(entry.name)) files.push(file);
    }
  }
  walk(directory);
  const caches = files.filter(file => file.endsWith('CMakeCache.txt') && file.includes('whisper-rs-sys'));
  const allowed = new Set(target.startsWith('x86_64') ? ['-m64', '-march=x86-64', '-mtune=generic', '-msse', '-msse2', '-mfpmath=sse'] : ['-march=armv8-a', '-mtune=generic']);
  const checkFlags = text => {
    for (const match of text.matchAll(/(?:^|\s)(-m[^\s"']+)/g)) if (!allowed.has(match[1])) throw new Error(`nonbaseline machine flag: ${match[1]}`);
  };
  let units = 0;
  for (const cache of caches) {
    const content = readFileSync(cache, 'utf8');
    const values = new Map([...content.matchAll(/^([^/#\n][^:\n]*):[^=\n]+=(.*)$/gm)].map(match => [match[1], match[2]]));
    for (const [key, expected] of Object.entries(policy)) {
      if (values.get(key) !== expected) throw new Error(`portable cache mismatch: ${key}`);
    }
    for (const [key, value] of values) if (/CMAKE_(?:C|CXX|ASM)_FLAGS/.test(key)) checkFlags(value);
    const commands = JSON.parse(readFileSync(join(cache.slice(0, -'CMakeCache.txt'.length), 'compile_commands.json'), 'utf8'));
    if (!commands.length) throw new Error('empty native compile manifest');
    for (const entry of commands) { checkFlags(entry.command ?? entry.arguments.join(' ')); units++; }
  }
  const hashes = Object.fromEntries(files.filter(file => file.includes('whisper-rs-sys')).map(file => [relative(directory, file), createHash('sha256').update(readFileSync(file)).digest('hex')]));
  const report = { target, caches: caches.length, compilation_units: units, hashes, scope: 'compiled C/C++ flags and archives; runtime ISA/inference not proven' };
  writeFileSync(join(directory, 'native-audit.json'), JSON.stringify(report, null, 2));
  return report;
}
