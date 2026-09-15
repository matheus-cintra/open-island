import { expect, test } from "bun:test";
import { MessageComposer } from "../message-controller";
import { VoiceController, sameVoiceTarget, type VoiceState, type VoiceTarget } from "../voice-controller";

const target: VoiceTarget = { session_id: "child", session_instance_id: "child-instance", daemon_epoch: "epoch" };
function state(phase: VoiceState["phase"], revision: number, id = "job"): VoiceState {
  return { revision, job_id: id, target: { ...target }, phase, recorded_ms: 1200,
    worker_active: ["recording", "requesting_permission", "transcribing"].includes(phase), transcript: phase === "ready" ? "fala local" : null, error: null };
}
function fixture() {
  let next = state("requesting_permission", 1);
  let failure = false;
  const calls: { command: string; args?: Record<string, unknown> }[] = [];
  const voice = new VoiceController(async <T>(command: string, args?: Record<string, unknown>): Promise<T> => {
    calls.push({ command, args });
    if (failure) throw new Error("unavailable");
    return next as T;
  });
  return { voice, calls, set(value: VoiceState): void { next = value; }, fail(): void { failure = true; } };
}

test("unchanged draft receives a transcript once, without sending", async () => {
  const f = fixture(); let sends = 0;
  const composer = new MessageComposer(async () => { sends++; }, () => {}, () => {});
  composer.edit("Pedido:");
  const revision = composer.draftRevision();
  await f.voice.start(target, (text) => composer.insertTranscript(text, revision));
  f.set(state("ready", 3)); await f.voice.refresh(); await f.voice.refresh();
  expect(composer.state().text).toBe("Pedido: fala local");
  expect(f.voice.results).toHaveLength(1);
  expect(f.voice.results[0]!.applied).toBe(true);
  expect(sends).toBe(0);
  f.set(state("recording", 2)); await f.voice.refresh();
  expect(f.voice.state.phase).toBe("ready");
  expect(f.calls.some((call) => call.command.includes("send"))).toBe(false);
});

test("events are subscribed before start and ready before its reply inserts exactly once", async (): Promise<void> => {
  let listener: ((state: VoiceState) => void) | undefined;
  let finishStart: ((state: VoiceState) => void) | undefined;
  const calls: string[] = []; let applied = 0;
  const voice = new VoiceController(async <T>(command: string): Promise<T> => {
    calls.push(command); expect(listener).toBeDefined();
    if (command === "voice_state") return { ...state("idle", 0), job_id: null, target: null } as T;
    return await new Promise<VoiceState>((resolve): void => { finishStart = resolve; }) as T;
  }, async (callback): Promise<() => void> => { listener = callback; return (): void => {}; });
  const starting = voice.start(target, (): boolean => { applied += 1; return true; });
  for (let i = 0; i < 10 && !finishStart; i += 1) await Promise.resolve();
  expect(calls).toEqual(["voice_state", "voice_start"]);
  listener!(state("ready", 3));
  expect(applied).toBe(0);
  finishStart!(state("requesting_permission", 1));
  await starting;
  expect(voice.state.phase).toBe("ready");
  expect(applied).toBe(1);
  expect(voice.results).toHaveLength(1);
  listener!(state("recording", 2)); listener!(state("ready", 3));
  expect(applied).toBe(1);
  expect(voice.results).toHaveLength(1);
});

test("late WebView gets preserved result and failed subscription is retried before start", async (): Promise<void> => {
  let subscriptions = 0; const calls: string[] = [];
  const voice = new VoiceController(async <T>(command: string): Promise<T> => {
    calls.push(command); return state("ready", 3) as T;
  }, async (): Promise<() => void> => {
    subscriptions += 1;
    if (subscriptions === 1) throw new Error("listen failed");
    return (): void => {};
  });
  await expect(voice.start(target, (): boolean => true)).rejects.toThrow("listen failed");
  expect(calls).toEqual([]);
  await voice.connect();
  expect(subscriptions).toBe(2);
  expect(voice.results[0].text).toBe("fala local");
  expect(voice.results[0].applied).toBe(false);
  await voice.connect();
  expect(subscriptions).toBe(2);
});

test("a current event survives failure of the initial cache read", async (): Promise<void> => {
  let listener: ((state: VoiceState) => void) | undefined;
  let rejectRead: ((error: Error) => void) | undefined;
  let removed = false;
  const voice = new VoiceController(async <T>(): Promise<T> => await new Promise<VoiceState>((_, reject): void => { rejectRead = reject; }) as T,
    async (callback): Promise<() => void> => { listener = callback; return (): void => { removed = true; }; });
  const connected = voice.connect();
  await Promise.resolve();
  listener!(state("ready", 3));
  rejectRead!(new Error("late read"));
  await connected;
  expect(removed).toBe(false);
  expect(voice.results[0].text).toBe("fala local");
});

test("cancel while start reply is pending suppresses an early ready event", async (): Promise<void> => {
  let listener: ((state: VoiceState) => void) | undefined;
  let finishStart: ((state: VoiceState) => void) | undefined;
  const cancelled: unknown[] = []; let applied = false;
  const voice = new VoiceController(async <T>(command: string, args?: Record<string, unknown>): Promise<T> => {
    if (command === "voice_state") return { ...state("idle", 0), job_id: null, target: null } as T;
    if (command === "voice_cancel") { cancelled.push(args?.jobId); return state("cancelled", 4) as T; }
    return await new Promise<VoiceState>((resolve): void => { finishStart = resolve; }) as T;
  }, async (callback): Promise<() => void> => { listener = callback; return (): void => {}; });
  const starting = voice.start(target, (): boolean => { applied = true; return true; });
  for (let i = 0; i < 10 && !finishStart; i += 1) await Promise.resolve();
  await voice.cancel();
  listener!(state("ready", 3));
  finishStart!(state("requesting_permission", 1));
  await starting;
  expect(cancelled).toEqual(["job"]);
  expect(applied).toBe(false);
  expect(voice.results).toHaveLength(0);
  expect(voice.state.phase).toBe("cancelled");
});

test("native unavailable target preserves text and requires a new explicit selection", async (): Promise<void> => {
  const f = fixture(); let applied = 0; let inserted = "";
  const destination = { target: (): VoiceTarget => target, label: (): string => "Original", insert: (text: string): boolean => { inserted = text; return true; } };
  f.voice.select(destination);
  await f.voice.start(target, (): boolean => { applied += 1; return true; });
  f.set({ ...state("ready", 3), target_unavailable: true });
  await f.voice.refresh();
  expect(applied).toBe(0);
  expect(f.voice.results[0].text).toBe("fala local");
  expect(f.voice.results[0].target_unavailable).toBe(true);
  expect(f.voice.insert("job")).toBe(false);
  expect(inserted).toBe("");
  f.voice.select(destination);
  expect(f.voice.insert("job")).toBe(true);
  expect(inserted).toBe("fala local");
  expect(f.calls.some((call): boolean => call.command.includes("send"))).toBe(false);
});

test("editing away and back or recycling the target preserves a separate result", async () => {
  for (const change of ["draft", "target"]) {
    const f = fixture();
    const composer = new MessageComposer(async () => {}, () => {}, () => {});
    composer.edit("rascunho"); const revision = composer.draftRevision();
    let current = { ...target };
    await f.voice.start(target, (text) => sameVoiceTarget(target, current) && composer.insertTranscript(text, revision));
    if (change === "draft") { composer.edit("outro"); composer.edit("rascunho"); }
    else current = { ...target, session_instance_id: "recycled" };
    f.set(state("ready", 3)); await f.voice.refresh();
    expect(composer.state().text).toBe("rascunho");
    expect(f.voice.results[0]!.applied).toBe(false);
    expect(f.voice.insert("job")).toBe(false);
    f.voice.select({ target: () => current, label: () => "Destino escolhido", insert: (text) => composer.insertTranscript(text) });
    expect(f.voice.insert("job")).toBe(true);
    expect(composer.state().text).toBe("rascunho fala local");
  }
});

test("cancel before start reply uses its job ID and never applies a late result", async () => {
  let resolve!: (value: VoiceState) => void;
  const pending = new Promise<VoiceState>((done) => { resolve = done; });
  const calls: Record<string, unknown>[] = []; let applied = 0;
  const voice = new VoiceController(async <T>(command: string, args?: Record<string, unknown>): Promise<T> => {
    if (command === "voice_start") return await pending as T;
    calls.push(args ?? {}); return state("cancelled", 3) as T;
  });
  const started = voice.start(target, () => { applied++; return true; });
  await voice.cancel();
  resolve(state("ready", 2)); await started;
  expect(calls).toEqual([{ jobId: "job" }]);
  expect(applied).toBe(0); expect(voice.results).toHaveLength(0);
  expect(voice.active()).toBe(false);
});

test("failed discard retains text; late webview state recovers without automatic insertion", async () => {
  const f = fixture(); f.set(state("ready", 5)); await f.voice.refresh();
  expect(f.voice.results[0]!.applied).toBe(false);
  f.fail(); await expect(f.voice.discard("job")).rejects.toThrow();
  expect(f.voice.results[0]!.text).toBe("fala local");
});

test("successful send and pending submission invalidate automatic voice insertion", async () => {
  let release!: () => void;
  const pending = new Promise<void>((done) => { release = done; });
  const composer = new MessageComposer(() => pending, () => {}, () => {});
  composer.edit("enviar"); const revision = composer.draftRevision();
  const submitted = composer.submit();
  expect(composer.insertTranscript("fala", revision)).toBe(false);
  release(); await submitted;
  expect(composer.state().text).toBe("");
  expect(composer.insertTranscript("fala", revision)).toBe(false);
});

test("editor failure retains the transcript and recording time never moves backwards", async () => {
  const f = fixture(); await f.voice.start(target, () => { throw new Error("editor removed"); });
  f.set({ ...state("recording", 2), recorded_ms: 1500 }); await f.voice.refresh();
  f.set({ ...state("recording", 2), recorded_ms: 1000 }); await f.voice.refresh();
  expect(f.voice.state.recorded_ms).toBe(1500);
  f.set(state("ready", 3)); await f.voice.refresh();
  expect(f.voice.results[0]!.applied).toBe(false);
});
