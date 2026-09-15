import { parseArgs } from 'node:util';
import { mkdir, readFile, writeFile } from 'node:fs/promises';
import { resolve, dirname } from 'node:path';
import { run } from './qa/processes.mjs';
const { values } = parseArgs({options: {platform: {type: 'string'}, case: {type: 'string'}, out: {type: 'string'}, preflight: {type: 'boolean'}, app: {type: 'string'}, daemon: {type: 'string'}, compositor: {type: 'string'}}});
const cases = ['baseline', 'full', 'daemon-unavailable', 'reconnect-empty', 'reconnect-draft', 'reconnect', 'message-success', 'message-failure', 'message-unconfirmed', 'render-burst', 'rapid-morph', 'monitor-scale', 'notch', 'keyboard-pending', 'many-sessions', 'capture-start-stop', 'voice-cancel-race', 'voice-permission', 'voice-compose', 'voice-draft-race', 'voice-destination-lost', 'diagnostics-offline', 'diagnostics-ready'];
const linuxCases = ['baseline', 'daemon-unavailable', 'reconnect-empty', 'reconnect-draft', 'message-success', 'message-failure', 'message-unconfirmed', 'many-sessions', 'render-burst', 'diagnostics-offline', 'diagnostics-ready'];
const fullLinuxCases = ['baseline', 'daemon-unavailable', 'reconnect-empty', 'reconnect-draft', 'message-success', 'message-failure', 'message-unconfirmed', 'many-sessions', 'render-burst', 'diagnostics-offline', 'diagnostics-ready'];
const fullMacCases = [...fullLinuxCases];
const daemonCases = new Set(['reconnect-empty', 'reconnect-draft', 'message-success', 'message-failure', 'message-unconfirmed', 'many-sessions', 'render-burst', 'diagnostics-ready']);
if (!['linux', 'macos'].includes(values.platform) || (!values.preflight && !cases.includes(values.case)) || !values.out) throw new Error('Expected --platform linux|macos --case <scenario>|--preflight --out <directory>');
const app = values.app ?? process.env.OPEN_ISLAND_QA_APP;
const daemon = values.daemon ?? process.env.OPEN_ISLAND_QA_DAEMON;
const compositor = values.compositor ?? process.env.OPEN_ISLAND_QA_COMPOSITOR;
const requestedLinuxCases = values.case === 'full' ? fullLinuxCases : [values.case];
const canRunLinux = !values.preflight && values.platform === 'linux' && process.platform === 'linux'
  && (values.case === 'full' || linuxCases.includes(values.case)) && app && compositor
  && requestedLinuxCases.every(name => !daemonCases.has(name) || daemon);
if (canRunLinux) {
  const out = resolve(values.out);
  await mkdir(dirname(out), {recursive: true, mode: 0o700});
  if (values.case !== 'full') {
    const result = await run('python3', [resolve(import.meta.dirname, 'qa/native_linux.py'), '--app', resolve(app), '--compositor', resolve(compositor), '--case', values.case, '--out', out, ...(daemon ? ['--daemon', resolve(daemon)] : [])], {
      log: `${out}.runner.log`, cwd: resolve(import.meta.dirname, '..'), timeout: 120000, killGrace: 30000,
    });
    process.exitCode = result.code === 0 && !result.timedOut && !result.orphan ? 0 : 1;
  } else {
    await mkdir(out, {mode: 0o700});
    const scenarios = [];
    for (const name of fullLinuxCases) {
      const scenarioOut = resolve(out, name);
      const result = await run('python3', [resolve(import.meta.dirname, 'qa/native_linux.py'), '--app', resolve(app), '--compositor', resolve(compositor), '--case', name, '--out', scenarioOut, ...(daemon ? ['--daemon', resolve(daemon)] : [])], {
        log: resolve(out, `${name}.runner.log`), cwd: resolve(import.meta.dirname, '..'), timeout: 120000, killGrace: 30000,
      });
      let nativeReport;
      try { nativeReport = JSON.parse(await readFile(resolve(scenarioOut, 'result.json'), 'utf8')); } catch { nativeReport = {status: 'FAIL', error: 'missing_result'}; }
      const runnerPass = result.code === 0 && !result.timedOut && !result.orphan;
      scenarios.push({case: name, status: runnerPass ? nativeReport.status : 'FAIL', assertions: nativeReport.assertions?.length ?? 0, runner: {
        code: result.code, signal: result.signal, timedOut: result.timedOut, orphan: result.orphan, log: result.log,
      }});
    }
    const failed = scenarios.filter(scenario => scenario.status !== 'PASS');
    const assertions = scenarios.reduce((total, scenario) => total + scenario.assertions, 0);
    await writeFile(resolve(out, 'result.json'), `${JSON.stringify({status: failed.length === 0 ? 'PASS' : 'FAIL', platform: 'linux', case: 'full', assertions, scenarios}, null, 2)}\n`, {flag: 'wx', mode: 0o600});
    process.exitCode = failed.length === 0 ? 0 : 1;
  }
} else if (!values.preflight && values.platform === 'macos' && process.platform === 'darwin' && app
  && (values.case !== 'full' || daemon)
  && (values.case === 'full' || !daemonCases.has(values.case) || daemon)) {
  const out = resolve(values.out);
  await mkdir(dirname(out), {recursive: true, mode: 0o700});
  const requested = values.case === 'full' ? fullMacCases : [values.case];
  if (values.case !== 'full') {
    const args = [resolve(import.meta.dirname, 'qa/native_macos.py'), '--app', resolve(app), '--case', values.case, '--out', out];
    if (daemon) args.push('--daemon', resolve(daemon));
    const result = await run('python3', args, {
      log: `${out}.runner.log`, cwd: resolve(import.meta.dirname, '..'), timeout: 180000, killGrace: 30000,
    });
    process.exitCode = result.code === 0 && !result.timedOut && !result.orphan ? 0 : 1;
  } else {
    await mkdir(out, {mode: 0o700});
    const scenarios = [];
    for (const name of requested) {
      const scenarioOut = resolve(out, name);
      const result = await run('python3', [resolve(import.meta.dirname, 'qa/native_macos.py'), '--app', resolve(app), '--daemon', resolve(daemon), '--case', name, '--out', scenarioOut], {
        log: resolve(out, `${name}.runner.log`), cwd: resolve(import.meta.dirname, '..'), timeout: 180000, killGrace: 30000,
      });
      let nativeReport;
      try { nativeReport = JSON.parse(await readFile(resolve(scenarioOut, 'result.json'), 'utf8')); } catch { nativeReport = {status: 'FAIL', error: 'missing_result'}; }
      const runnerPass = result.code === 0 && !result.timedOut && !result.orphan;
      scenarios.push({case: name, status: runnerPass ? nativeReport.status : 'FAIL', assertions: nativeReport.assertions?.length ?? 0, runner: {
        code: result.code, signal: result.signal, timedOut: result.timedOut, orphan: result.orphan, log: result.log,
      }});
    }
    const failed = scenarios.filter(scenario => scenario.status !== 'PASS');
    const assertions = scenarios.reduce((total, scenario) => total + scenario.assertions, 0);
    await writeFile(resolve(out, 'result.json'), `${JSON.stringify({status: failed.length === 0 ? 'PASS' : 'FAIL', platform: 'macos', case: 'full', assertions, scenarios}, null, 2)}\n`, {flag: 'wx', mode: 0o600});
    process.exitCode = failed.length === 0 ? 0 : 1;
  }
} else {
  const blockers = [(linuxCases.includes(values.case) || values.case === 'full') && values.platform === 'linux'
    ? 'Provide --app and --compositor; Session, reconnect, delivery and diagnostics-ready cases also require a qa-harness --daemon.'
    : values.platform === 'macos' && (values.case === 'full' || daemonCases.has(values.case))
      ? 'Provide a QA macOS app and qa-harness daemon; full runs need both.'
      : 'This native scenario is not implemented; no application was launched.'];
  if ((values.platform === 'macos' ? 'darwin' : 'linux') !== process.platform) blockers.push('Requested native host is unavailable.');
  if ((values.case?.startsWith('voice-') || values.case?.startsWith('capture-')) && !process.env.OPEN_ISLAND_QA_AUDIO_DEVICE) blockers.push('No authorized private audio device was supplied.');
  const out = resolve(values.out);
  await mkdir(out, {recursive: true, mode: 0o700});
  await writeFile(`${out}/preflight.json`, JSON.stringify({status: 'BLOCKED', platform: values.platform, host: process.platform, blockers, capturePerformed: false}, null, 2), {flag: 'wx', mode: 0o600});
  console.error(blockers.join('\n'));
  process.exitCode = 2;
}
