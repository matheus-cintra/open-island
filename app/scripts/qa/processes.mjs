import { spawn } from 'node:child_process';
import { open, readFile } from 'node:fs/promises';
import { checkProcess } from './evidence.mjs';

export async function run(command, args, { log, cwd, timeout = 600000, killGrace = 1000, env = process.env }) {
  const output = await open(log, 'wx', 0o600);
  const started = performance.now();
  const child = spawn(command, args, { cwd, env, detached: true, stdio: ['ignore', output.fd, output.fd] });
  let timedOut = false;
  const signal = (name) => {
    try { process.kill(-child.pid, name); } catch (error) { if (error.code !== 'ESRCH') throw error; }
  };
  let killer;
  const timer = setTimeout(() => {
    timedOut = true;
    signal('SIGTERM');
    killer = setTimeout(() => signal('SIGKILL'), killGrace);
  }, timeout);
  let exit;
  let orphan = false;
  try {
    exit = await new Promise((resolve, reject) => {
      child.once('error', reject);
      child.once('exit', (code, death) => resolve({ code, signal: death }));
    });
    try { process.kill(-child.pid, 0); orphan = true; } catch (error) { if (error.code !== 'ESRCH') throw error; }
  } finally {
    clearTimeout(timer);
    clearTimeout(killer);
    if (child.pid) signal('SIGKILL');
    await output.close();
  }
  return { command: [command, ...args], ...exit, timedOut, orphan, elapsedMs: performance.now() - started, log,
    output: await readFile(log, 'utf8') };
}

export function assertions(result) {
  const check = checkProcess(result);
  if (!check.passed) {
    throw new Error(`QA failed: exit=${result.code}, timeout=${result.timedOut}, orphan=${result.orphan}, count=${check.count}, skipped=${check.skipped}`);
  }
  return check.count;
}
