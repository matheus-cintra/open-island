const portable = args => ['node', 'scripts/portable-build.mjs', 'cargo', ...args];
const bunTests = files => ['bun', 'test', '--isolate', ...files];
const rust = (packageName, ...args) => portable(['test', '-p', packageName, ...args]);

export const taskScenarios = new Map([
  [1, [{name: 'qa-isolation', command: 'cargo', args: ['test', '--target-dir', 'target/qa-harness', '-p', 'open-islandd', '--features', 'qa-harness', '--test', 'qa_isolation_cli']}]],
  [2, [{name: 'delivery-core', command: portable(['test', '-p', 'open-island-core', 'message_delivery'])}]],
  [3, [{name: 'delivery-bridge', command: rust('open-islandd', '--test', 'message_delivery_cli')}]],
  [4, [{name: 'daemon-transport', command: rust('open-island', '--lib', 'daemon_transport')}]],
  [5, [
    {name: 'authoritative-socket', command: rust('open-islandd', '--test', 'socket')},
    {name: 'authoritative-shell', command: rust('open-island', '--lib', 'daemon_transport::refresh')},
  ]],
  [6, [{name: 'server-limits', command: rust('open-islandd', '--test', 'socket')}]],
  [7, [{name: 'state-hydration', command: bunTests(['src/test/daemon-state.test.ts', 'src/test/snapshot-ui.test.ts'])}]],
  [8, [{name: 'message-state', command: bunTests(['src/test/message.test.ts', 'src/test/message-controller.test.ts', 'src/test/message-deliveries.test.ts', 'src/test/message-recovery.test.ts'])}]],
  [9, [{name: 'doctor-cli', command: rust('open-islandd', '--test', 'doctor_cli')}]],
  [10, [{name: 'diagnostics-ui', command: bunTests(['src/test/diagnostics.test.ts', 'src/test/header-update.test.ts'])}]],
  [11, [{name: 'render-budget', command: bunTests(['src/test/render-budget.test.ts', 'src/test/render-frame.test.ts', 'src/test/frame-loop.test.ts', 'src/test/idle-fade.test.ts'])}]],
  [12, [
    {name: 'discovery-cache', command: rust('open-islandd', '--lib', 'discovery_cache')},
    {name: 'snapshot-stream', command: rust('open-islandd', '--lib', 'snapshot_stream')},
  ]],
  [13, [{name: 'resize-queue', command: bunTests(['src/test/resize-queue.test.ts', 'src/test/morph.test.ts', 'src/test/size.test.ts'])}]],
  [14, [{name: 'keyboard-accessibility', command: bunTests(['src/test/island-keyboard.test.ts', 'src/test/linux-keyboard.test.ts', 'src/test/visual-work.test.ts'])}]],
  [15, [{name: 'voice-capture', command: rust('open-island', '--lib', 'voice::capture')}]],
  [16, [
    {name: 'voice-resample', command: rust('open-island', '--lib', 'voice::resample')},
    {name: 'voice-transcribe', command: rust('open-island', '--lib', 'voice::transcribe')},
  ]],
  [17, [{name: 'voice-controller', command: rust('open-island', '--lib', 'voice::controller')}]],
  [18, [
    {name: 'voice-settings', command: rust('open-island', '--lib', 'voice::settings')},
    {name: 'voice-permission', command: rust('open-island', '--lib', 'voice::permission')},
    {name: 'voice-settings-ui', command: bunTests(['src/test/voice-settings.test.ts', 'src/test/voice-settings-ui.test.ts'])},
  ]],
  [19, [{name: 'voice-composer', command: bunTests(['src/test/voice-controller.test.ts', 'src/test/voice-ui.test.ts', 'src/test/message-controller.test.ts'])}]],
  [20, [
    {name: 'frontend-build', command: 'bun', args: ['run', 'build'], requireTests: false},
    {name: 'voice-probe-build', command: portable(['build', '--release', '-p', 'open-island', '--example', 'voice_probe']), requireTests: false},
    {name: 'daemon-build', command: portable(['build', '--release', '-p', 'open-islandd']), requireTests: false},
    {name: 'bundle-content', command: 'python3', args: ['-m', 'unittest', 'discover', '-s', '../scripts', '-p', 'verify_bundle_content_test.py']},
    {name: 'audio-licenses', command: 'python3', args: ['-m', 'unittest', 'discover', '-s', '../scripts', '-p', 'audio_licenses_test.py']},
    {name: 'portable-policy', command: 'node', args: ['--test', 'scripts/portable-paths.test.mjs', 'scripts/portable-build-options.test.mjs', 'scripts/portable-audit.test.mjs']},
  ]],
  [21, [{name: 'documentation', validation: 'documentation'}]],
  [22, [
    {name: 'workspace', command: ['python3', '../scripts/test-processes.py', 'node', 'scripts/portable-build.mjs', 'cargo', 'test', '--workspace']},
    {name: 'frontend', command: 'bun', args: ['test', '--isolate', '--path-ignore-patterns=**/target/**']},
    {name: 'script-tests', command: 'node', args: ['--test', 'scripts/portable-paths.test.mjs', 'scripts/portable-build-options.test.mjs', 'scripts/portable-audit.test.mjs', 'scripts/qa/processes.test.mjs']},
    {name: 'resource-tests', command: 'python3', args: ['-m', 'unittest', 'discover', '-s', '../scripts', '-p', '*_test.py']},
  ]],
]);

export const nativeLinuxCases = [
  'baseline', 'daemon-unavailable', 'reconnect-empty', 'reconnect-draft',
  'message-success', 'message-failure', 'message-unconfirmed', 'many-sessions',
  'render-burst', 'diagnostics-offline', 'diagnostics-ready',
];

export function commandFor(scenario) {
  if (Array.isArray(scenario.command)) {
    return {command: scenario.command[0], args: scenario.command.slice(1)};
  }
  if (typeof scenario.command === 'string') {
    return {command: scenario.command, args: scenario.args ?? []};
  }
  return null;
}
