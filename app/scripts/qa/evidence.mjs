import { readFile, writeFile } from 'node:fs/promises';

const rustResult = /test result: (?:ok|FAILED)\. (\d+) passed; (\d+) failed; (\d+) ignored/g;
const nodePass = /^# pass (\d+)$/m;
const nodeSpecPass = /^\s*ℹ pass (\d+)$/m;
const bunPass = /^\s*(\d+) pass$/m;
const pythonResult = /^Ran (\d+) tests? in/m;

export function testCount(output) {
  const rust = [...output.matchAll(rustResult)];
  const rustCount = rust.reduce((total, match) => total + Number(match[1]), 0);
  const node = Number(output.match(nodePass)?.[1] ?? output.match(nodeSpecPass)?.[1] ?? 0);
  const bun = Number(output.match(bunPass)?.[1] ?? 0);
  const python = Number(output.match(pythonResult)?.[1] ?? 0);
  return rustCount + node + bun + python;
}

export function hasSkippedTests(output) {
  return [...output.matchAll(rustResult)].some(match => Number(match[3]) > 0)
    || /# skipped [1-9]/.test(output)
    || /^\s*[1-9]+ skip(?:ped)?$/m.test(output)
    || /\bskipped,?\s*[1-9]/i.test(output);
}

export function checkProcess(result, { requireTests = true } = {}) {
  const count = testCount(result.output ?? '');
  const skipped = hasSkippedTests(result.output ?? '');
  const passed = result.code === 0 && !result.timedOut && !result.orphan && (!requireTests || (count > 0 && !skipped));
  return {
    passed,
    count,
    skipped,
    exitCode: result.code,
    signal: result.signal,
    timedOut: result.timedOut,
    orphan: result.orphan,
    elapsedMs: Math.round(result.elapsedMs),
    command: result.command,
    log: result.log,
  };
}

export async function writeJson(path, value) {
  await writeFile(path, `${JSON.stringify(value, null, 2)}\n`, { flag: 'wx', mode: 0o600 });
}

export async function readJson(path) {
  return JSON.parse(await readFile(path, 'utf8'));
}
