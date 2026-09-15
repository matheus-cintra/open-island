import { parseArgs } from 'node:util';
import { mkdir } from 'node:fs/promises';
import { resolve } from 'node:path';
import { run } from './qa/processes.mjs';
import { checkDocumentation } from './qa/docs.mjs';
import { checkProcess, readJson, writeJson } from './qa/evidence.mjs';
import { commandFor, taskScenarios } from './qa/scenarios.mjs';

const { values } = parseArgs({
  options: {
    task: {type: 'string'},
    case: {type: 'string'},
    out: {type: 'string'},
    'audit-ledger': {type: 'string'},
  },
});

const root = resolve(import.meta.dirname, '..');
const repoRoot = resolve(root, '..');

function usage() {
  return 'Expected --task 1..22 --case happy|failure --out <new directory> or --audit-ledger <evidence directory>';
}

async function auditLedger(directory) {
  const final = resolve(directory, 'final');
  await mkdir(final, {recursive: true, mode: 0o700});
  const entries = [];
  const failures = [];
  for (let task = 1; task <= 22; task += 1) {
    const taskDirectory = resolve(directory, `task-${task}`);
    let found = false;
    try {
      const happy = await readJson(resolve(taskDirectory, 'happy.json'));
      found = true;
      entries.push({task, status: happy.status, count: happy.count ?? 0});
      if (happy.status !== 'PASS' || !(happy.count > 0)) failures.push(`task-${task}/happy.json is not a passing, non-empty result`);
    } catch {
      failures.push(`task-${task}/happy.json is missing or invalid`);
    }
    try {
      const failure = await readJson(resolve(taskDirectory, 'failure.json'));
      found = true;
      if (failure.status !== 'PASS' || !(failure.count > 0)) failures.push(`task-${task}/failure.json did not verify negative guards`);
    } catch {
      failures.push(`task-${task}/failure.json is missing or invalid`);
    }
    if (!found) failures.push(`task-${task} has no ledger entries`);
  }
  const report = {
    status: failures.length === 0 ? 'PASS' : 'FAIL',
    checked_tasks: entries,
    present_tasks: entries.length,
    passing_tasks: entries.filter(entry => entry.status === 'PASS' && entry.count > 0).length,
    failures,
    source: 'post-mvp-qa task ledger',
  };
  await writeJson(resolve(final, 'F1.json'), report);
  const markdown = [
    '# F1 — Plan compliance audit',
    '',
    `Status: **${report.status}**`,
    '',
    `Task ledgers inspected: ${entries.length}/22; passing happy ledgers: ${report.passing_tasks}/22.`,
    '',
    ...(failures.length === 0 ? ['All task ledgers are present and non-empty.'] : ['Findings:', '', ...failures.map(failure => `- ${failure}`)]),
    '',
  ].join('\n');
  const { writeFile } = await import('node:fs/promises');
  await writeFile(resolve(final, 'F1.md'), markdown, {flag: 'wx', mode: 0o600});
  if (failures.length > 0) {
    console.error(`${failures.length} ledger audit finding(s); see ${resolve(final, 'F1.md')}`);
    process.exitCode = 1;
    return;
  }
  console.log('22 task ledgers verified');
}

const failureFixtures = [
  {name: 'nonzero-exit', script: 'process.exit(1)', timeout: 1000},
  {name: 'zero-tests', script: 'process.exit(0)', timeout: 1000},
  {name: 'timeout', script: 'setInterval(() => {}, 1000)', timeout: 200},
  {name: 'orphaned-child', script: "require('child_process').spawn(process.execPath, ['-e', 'setInterval(() => {}, 1000)'], {stdio: 'ignore'}).unref()", timeout: 200},
];

async function runFailureGuards(out, task) {
  const results = [];
  for (const fixture of failureFixtures) {
    const result = await run(process.execPath, ['-e', fixture.script], {
      cwd: root,
      log: resolve(out, `${fixture.name}.log`),
      timeout: fixture.timeout,
    });
    const check = checkProcess(result);
    results.push({task, fixture: fixture.name, ...check});
  }
  const rejected = results.filter(result => !result.passed).length;
  const errors = results.filter(result => result.passed).map(result => `${result.fixture} unexpectedly passed`);
  return {
    status: errors.length === 0 && rejected === failureFixtures.length ? 'PASS' : 'FAIL',
    count: rejected,
    expected: failureFixtures.length,
    errors,
    guards: results,
  };
}

async function runScenario(scenario, out, index) {
  if (scenario.blocked) {
    return {name: scenario.name, kind: 'blocked', status: 'BLOCKED', count: 0, blockers: scenario.blockers};
  }
  if (scenario.validation === 'documentation') {
    const validation = await checkDocumentation(repoRoot);
    return {
      name: scenario.name,
      kind: 'validation',
      status: validation.ok ? 'PASS' : 'FAIL',
      count: validation.passed,
      expected: validation.total,
      checks: validation.checks,
    };
  }
  const command = commandFor(scenario);
  if (!command) throw new Error(`Scenario ${scenario.name} has no executable command`);
  const log = resolve(out, `${String(index + 1).padStart(2, '0')}-${scenario.name}.log`);
  const processResult = await run(command.command, command.args, {
    cwd: root,
    log,
    timeout: scenario.timeout ?? 600000,
    env: {...process.env, OPEN_ISLAND_TEST_LOG: resolve(out, `${String(index + 1).padStart(2, '0')}-${scenario.name}.process.log`)},
  });
  const check = checkProcess(processResult, {requireTests: scenario.requireTests !== false});
  const count = check.count || (scenario.requireTests === false && check.passed ? 1 : 0);
  return {name: scenario.name, kind: 'process', status: check.passed ? 'PASS' : 'FAIL', ...check, count};
}

function scenariosForTask(task, out) {
  const scenarios = [...taskScenarios.get(task)];
  if (task === 16) {
    const model = process.env.OPEN_ISLAND_QA_MODEL;
    const probe = process.env.OPEN_ISLAND_QA_PROBE;
    if (model && probe) {
      scenarios.push({
        name: 'voice-model-quality',
        command: 'python3',
        args: ['scripts/voice-qa.py', '--probe', resolve(probe), '--model', resolve(model), '--evidence', resolve(out, 'voice-model'), '--case', 'quality'],
        requireTests: false,
        timeout: 300000,
      });
    } else {
      scenarios.push({
        name: 'voice-model-quality',
        blocked: true,
        blockers: ['OPEN_ISLAND_QA_MODEL and OPEN_ISLAND_QA_PROBE are required for the pinned real-model check.'],
      });
    }
  }
  return scenarios;
}

async function runNativeGate(out) {
  const app = process.env.OPEN_ISLAND_QA_APP;
  const daemon = process.env.OPEN_ISLAND_QA_DAEMON;
  const compositor = process.env.OPEN_ISLAND_QA_COMPOSITOR;
  const linux = {platform: 'linux', status: 'BLOCKED', blockers: []};
  if (process.platform !== 'linux') linux.blockers.push('Linux native host is unavailable.');
  if (!app || !daemon || !compositor) linux.blockers.push('OPEN_ISLAND_QA_APP, OPEN_ISLAND_QA_DAEMON and OPEN_ISLAND_QA_COMPOSITOR are required.');
  const macApp = process.env.OPEN_ISLAND_QA_MACOS_APP ?? (process.platform === 'darwin' ? app : undefined);
  const macDaemon = process.env.OPEN_ISLAND_QA_MACOS_DAEMON ?? (process.platform === 'darwin' ? daemon : undefined);
  const macos = {platform: 'macos', status: 'BLOCKED', blockers: []};
  if (process.platform !== 'darwin') macos.blockers.push('macOS native host is unavailable in this run.');
  if (!macApp || !macDaemon) macos.blockers.push('OPEN_ISLAND_QA_MACOS_APP and OPEN_ISLAND_QA_MACOS_DAEMON are required for the full macOS matrix.');
  const reports = [linux, macos];
  if (linux.blockers.length === 0) {
    const nativeOut = resolve(out, 'native', 'linux', 'full');
    await mkdir(resolve(out, 'native', 'linux'), {recursive: true, mode: 0o700});
    const result = await run(process.execPath, ['scripts/native-qa.mjs', '--platform', 'linux', '--case', 'full', '--app', resolve(app), '--daemon', resolve(daemon), '--compositor', resolve(compositor), '--out', nativeOut], {
      cwd: root,
      log: resolve(out, 'native-linux-full.runner.log'),
      timeout: 1200000,
      killGrace: 30000,
    });
    let nativeReport;
    try { nativeReport = await readJson(resolve(nativeOut, 'result.json')); } catch { nativeReport = {status: 'FAIL', error: 'missing_result'}; }
    reports[0] = {
      platform: 'linux',
      status: nativeReport.status,
      assertions: nativeReport.scenarios?.reduce((sum, scenario) => sum + (scenario.assertions ?? 0), 0) ?? 0,
      runner: {code: result.code, signal: result.signal, timedOut: result.timedOut, orphan: result.orphan, log: result.log},
    };
  }
  if (macos.blockers.length === 0) {
    const nativeOut = resolve(out, 'native', 'macos', 'full');
    await mkdir(resolve(out, 'native', 'macos'), {recursive: true, mode: 0o700});
    const result = await run(process.execPath, ['scripts/native-qa.mjs', '--platform', 'macos', '--case', 'full', '--app', resolve(macApp), '--daemon', resolve(macDaemon), '--out', nativeOut], {
      cwd: root,
      log: resolve(out, 'native-macos-full.runner.log'),
      timeout: 1800000,
      killGrace: 30000,
    });
    let nativeReport;
    try { nativeReport = await readJson(resolve(nativeOut, 'result.json')); } catch { nativeReport = {status: 'FAIL', error: 'missing_result'}; }
    reports[1] = {
      platform: 'macos',
      status: nativeReport.status,
      assertions: nativeReport.scenarios?.reduce((sum, scenario) => sum + (scenario.assertions ?? 0), 0) ?? 0,
      runner: {code: result.code, signal: result.signal, timedOut: result.timedOut, orphan: result.orphan, log: result.log},
    };
  }
  return reports;
}

async function runTask(task, kind, out) {
  if (!taskScenarios.has(task)) throw new Error(`No scenarios registered for task ${task}`);
  if (kind === 'failure') {
    const result = await runFailureGuards(out, task);
    await writeJson(resolve(out, 'failure.json'), {
      ...result,
      task,
      case: 'failure',
      scope: 'runner process isolation and negative-result guards',
    });
    if (result.status !== 'PASS') throw new Error(result.errors.join('; ') || 'failure guards did not reject every fixture');
    console.log(`${result.count} failure guards verified for task ${task}`);
    return;
  }
  const scenarios = scenariosForTask(task, out);
  const results = [];
  for (let index = 0; index < scenarios.length; index += 1) {
    results.push(await runScenario(scenarios[index], out, index));
  }
  const native = task === 22 ? await runNativeGate(out) : [];
  const failed = results.filter(result => result.status === 'FAIL');
  const blocked = [...results.filter(result => result.status === 'BLOCKED'), ...native.filter(result => result.status === 'BLOCKED')];
  const count = results.reduce((total, result) => total + (result.count ?? 0), 0);
  const report = {
    task,
    case: 'happy',
    status: failed.length > 0 ? 'FAIL' : blocked.length > 0 ? 'BLOCKED' : 'PASS',
    count,
    scenarios: results,
    ...(native.length > 0 ? {native} : {}),
    scope: 'automated contracts; native platform prerequisites remain explicit',
  };
  await writeJson(resolve(out, 'happy.json'), report);
  if (failed.length > 0) throw new Error(`task ${task} failed: ${failed.map(result => result.name).join(', ')}`);
  if (blocked.length > 0) {
    process.exitCode = 2;
    console.error(`${count} tests/checks verified for task ${task}; native prerequisites remain BLOCKED`);
    return;
  }
  console.log(`${count} tests/checks verified for task ${task}`);
}

if (values['audit-ledger']) {
  await auditLedger(resolve(values['audit-ledger']));
} else {
  const task = Number(values.task);
  if (!Number.isInteger(task) || task < 1 || task > 22 || !['happy', 'failure'].includes(values.case) || !values.out) throw new Error(usage());
  const out = resolve(values.out);
  await mkdir(out, {recursive: true, mode: 0o700});
  await runTask(task, values.case, out);
}
