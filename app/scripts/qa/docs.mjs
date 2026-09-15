import { access, readFile } from 'node:fs/promises';
import { dirname, resolve } from 'node:path';

const requiredFiles = ['docs/post-mvp-evolution.md', 'docs/native-qa.md', 'docs/DESIGN.md', 'README.md'];
const requiredText = [
  ['docs/post-mvp-evolution.md', 'Entrega não confirmada'],
  ['docs/post-mvp-evolution.md', 'voice.json'],
  ['docs/post-mvp-evolution.md', 'não são enviados à nuvem'],
  ['docs/native-qa.md', 'BLOCKED'],
  ['README.md', 'doctor'],
];

export async function checkDocumentation(repoRoot) {
  const checks = [];
  for (const relative of requiredFiles) {
    const path = resolve(repoRoot, relative);
    try {
      await access(path);
      checks.push({name: `file:${relative}`, pass: true});
    } catch {
      checks.push({name: `file:${relative}`, pass: false});
    }
  }
  for (const [relative, expected] of requiredText) {
    const path = resolve(repoRoot, relative);
    let pass = false;
    try { pass = (await readFile(path, 'utf8')).includes(expected); } catch { /* file check reports the cause */ }
    checks.push({name: `text:${relative}:${expected}`, pass});
  }
  const docs = await readFile(resolve(repoRoot, 'docs/post-mvp-evolution.md'), 'utf8');
  const links = [...docs.matchAll(/\]\(([^)]+)\)/g)].map(match => match[1]).filter(value => value.startsWith('../') || value.startsWith('./'));
  for (const link of links) {
    const target = resolve(dirname(resolve(repoRoot, 'docs/post-mvp-evolution.md')), link);
    let pass = false;
    try { await access(target); pass = true; } catch { /* recorded below */ }
    checks.push({name: `link:${link}`, pass});
  }
  const passed = checks.filter(check => check.pass).length;
  return {passed, total: checks.length, checks, ok: passed === checks.length};
}
