import { expect, test } from "bun:test";
import { VoiceSettings, type ModelStatus } from "../settings-voice";
import { strings } from "../strings";

test("late status cannot overwrite a newly selected model, and cancelling preserves selection", async () => {
  let statusReply!: (value: ModelStatus) => void;
  let cancel = false;
  const settings = new VoiceSettings(async <T>(command: string): Promise<T> => {
    if (command === "voice_model_status") return await new Promise<ModelStatus>((resolve) => { statusReply = resolve; }) as T;
    return (cancel ? null : { configured: true, error: null }) as T;
  });
  const loading = settings.refresh();
  await settings.change("voice_select_model");
  statusReply({ configured: false, error: "model_unavailable" }); await loading;
  expect(settings.summary()).toBe(strings.voice.configured);
  cancel = true; await settings.change("voice_select_model");
  expect(settings.summary()).toBe(strings.voice.configured);
});

test("selection and removal exclude each other, and local failures expose no paths", async () => {
  const calls: string[] = [];
  let finish!: (value: ModelStatus) => void;
  const settings = new VoiceSettings(async <T>(command: string): Promise<T> => {
    calls.push(command);
    if (command === "voice_select_model") return await new Promise<ModelStatus>((resolve) => { finish = resolve; }) as T;
    throw new Error("PRIVATE_PATH");
  });
  const selecting = settings.change("voice_select_model");
  expect(await settings.change("voice_clear_model")).toBe(strings.voice.error("voice_model_busy"));
  expect(calls).toEqual(["voice_select_model"]);
  finish({ configured: true, error: null }); await selecting;
  expect(await settings.change("voice_clear_model")).not.toContain("PRIVATE_PATH");
  expect(settings.summary()).toBe(strings.voice.configured);
});

test("successful removal exposes unavailable state without touching shared daemon config", async () => {
  const calls: string[] = [];
  const settings = new VoiceSettings(async <T>(command: string): Promise<T> => {
    calls.push(command);
    return { configured: false, error: "model_unavailable" } as T;
  });
  expect(await settings.change("voice_clear_model")).toBe(strings.voice.modelRemoved);
  expect(settings.summary()).toBe(strings.voice.error("model_unavailable"));
  expect(calls).toEqual(["voice_clear_model"]);
});
