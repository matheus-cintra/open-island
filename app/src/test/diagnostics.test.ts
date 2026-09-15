import { expect, test } from "bun:test";
import { Diagnostics } from "../settings-diagnostics";
import { strings } from "../strings";
const report = { schema_version: 1 as const, platform: "linux", arch: "x86_64", collector_version: "0.6.3", app_version: "0.6.3", daemon: { state: "unavailable", version: null, pid: null, epoch: null, capabilities: [] }, socket: { exists: false, connectable: false }, model: "not_applicable" };
test("audio metadata never claims a successful capture or hides unknown status", async () => {
  const value = { ...report, audio: { alsa_capture_devices: null as boolean | null, capture_tested: false } };
  const controller = new Diagnostics(async () => value, async () => {});
  await controller.refresh();
  expect(controller.audio()).toBe(strings.settings.about.diagnosticAudioUnchecked);
  value.audio.alsa_capture_devices = true;
  await controller.refresh();
  expect(controller.audio()).toBe(strings.settings.about.diagnosticAudioDetected);
  value.audio.alsa_capture_devices = false;
  await controller.refresh();
  expect(controller.audio()).toBe(strings.settings.about.diagnosticAudioNotDetected);
});
test("offline diagnostic stays available for copy without a daemon", async (): Promise<void> => {
  let copied = "";
  const controller = new Diagnostics(async () => report, async (text) => { copied = text; });
  await controller.refresh();
  expect(controller.summary()).toBe(strings.settings.about.diagnosticOffline);
  expect(await controller.copy()).toBe(strings.settings.about.diagnosticCopied);
  expect(JSON.parse(copied)).toEqual(report);
});
test("diagnostic errors never expose raw process or transport output", async (): Promise<void> => {
  const controller = new Diagnostics(async () => { throw new Error("SECRET_SENTINEL"); }, async () => { throw new Error("PRIVATE_PATH"); });
  await controller.refresh();
  expect(controller.available()).toBe(false);
  expect(controller.summary()).toBe(strings.settings.about.diagnosticFailed);
  expect(await controller.copy()).toBe(strings.settings.about.diagnosticCopyFailed);
});

test("local model status stays visible offline without exposing a filename", async () => {
  const value = { ...report, model: "configured" };
  const controller = new Diagnostics(async () => value, async () => {});
  await controller.refresh();
  expect(controller.model()).toBe(strings.voice.configured);
  value.model = "unavailable"; await controller.refresh();
  expect(controller.model()).toBe(strings.voice.error("model_unavailable"));
});

test("service inactivity and legacy hooks remain visible despite a connected daemon", async (): Promise<void> => {
  const value = { ...report, daemon: { ...report.daemon, state: "ready" }, local: { service: { installed: true, active: false }, daemon_binary: { present: true, version: "0.6.3", probe: "verified" }, app_binary: { present: true, version: "0.6.3", probe: "current_process" } }, hooks: [{ agent: "claude", managed: true, state: "legacy" }] };
  const controller = new Diagnostics(async () => value, async () => {});
  await controller.refresh();
  expect(controller.summary()).toBe(strings.settings.about.diagnosticReady);
  expect(controller.service()).toBe(strings.settings.about.diagnosticServiceInactive);
  expect(controller.hooks()).toBe(strings.settings.about.diagnosticHooksReview("claude"));
});
