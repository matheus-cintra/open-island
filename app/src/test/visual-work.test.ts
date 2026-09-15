import { expect, mock, test } from "bun:test";
import { mountIsland } from "./dom";
import { tauriMock } from "./tauri";
import { strings } from "../strings";
const tauri = tauriMock();
mock.module("@tauri-apps/api/core", (): object => ({ invoke: tauri.invoke }));
mock.module("@tauri-apps/api/event", (): object => ({ listen: tauri.listen }));
mountIsland();
let reduced = false; let next = 0;
const changes: (() => void)[] = [];
const pending = new Map<number, FrameRequestCallback>();
const callbacks = new Map<number, FrameRequestCallback>();
function paint(): void {
  for (const [id, callback] of [...pending]) {
    if (!pending.delete(id)) continue;
    callback(performance.now());
  }
}
window.matchMedia = (() => ({ get matches(): boolean { return reduced; }, addEventListener: (_: string, callback: () => void): void => { changes.push(callback); } })) as unknown as Window["matchMedia"];
globalThis.requestAnimationFrame = (callback): number => { const id = ++next; pending.set(id, callback); callbacks.set(id, callback); return id; };
globalThis.cancelAnimationFrame = (id): void => { pending.delete(id); };
const main = await import("../main");
const { tickElapsed } = await import("../row");
await new Promise((resolve): void => { setTimeout(resolve, 30); });
paint();

test("visibility, fullscreen and reduced motion suspend the island's drift", (): void => {
  expect(pending.size).toBe(1);
  const old = [...pending.values()][0];
  Object.defineProperty(document, "hidden", { configurable: true, value: true });
  document.dispatchEvent(new window.Event("visibilitychange"));
  expect(pending.size).toBe(0);
  old(100);
  expect(pending.size).toBe(0);
  Object.defineProperty(document, "hidden", { configurable: true, value: false });
  document.dispatchEvent(new window.Event("visibilitychange"));
  expect(pending.size).toBe(1);
  tauri.emit("island-fullscreen", { fullscreen: true });
  expect(pending.size).toBe(0);
  tauri.emit("island-fullscreen", { fullscreen: false });
  expect(pending.size).toBe(1);
  reduced = true; for (const change of changes) change();
  expect(pending.size).toBe(0);
  reduced = false; for (const change of changes) change();
  expect(pending.size).toBe(1);
  for (const [id, callback] of callbacks) if (!pending.has(id)) callback(200);
  expect(pending.size).toBe(1);
});

test("idle fade suspends drift and pointer activity resumes one loop", async (): Promise<void> => {
  main.applyConfig({ island: { idle_fade: true, idle_fade_ms: 5 } });
  paint();
  main.resetIdle();
  await new Promise((resolve): void => { setTimeout(resolve, 30); });
  expect(pending.size).toBe(0);
  main.resetIdle();
  expect(pending.size).toBe(1);
  main.applyConfig({ island: { idle_fade: false } });
  paint();
});

test("screen-off snapshots suspend drift; a stale offline snapshot cannot keep it suspended", (): void => {
  const cache = tauri.state.read();
  const snapshot = cache.snapshot!.snapshot;
  snapshot.publication_revision += 100;
  snapshot.quiet_scenes.screen_off = true;
  tauri.emit("daemon-ui-state", structuredClone(cache));
  paint();
  expect(pending.size).toBe(0);
  tauri.emit("daemon-ui-state", { phase: "reconnecting", generation: cache.generation, snapshot: cache.snapshot });
  paint();
  expect(pending.size).toBe(1);
  snapshot.publication_revision += 1;
  snapshot.quiet_scenes.screen_off = false;
  tauri.emit("daemon-ui-state", structuredClone(cache));
  paint();
  expect(pending.size).toBe(1);
});

test("returning to a visible expanded panel refreshes elapsed immediately without replaying renders", (): void => {
  reduced = true; for (const change of changes) change();
  tauri.emit("island-toggle", {});
  expect(main.expanded).toBe(true);
  const elapsed = document.createElement("span");
  elapsed.className = "agent-elapsed";
  const since = Date.now() - 65_000;
  elapsed.dataset.since = String(since);
  document.getElementById("session-list")!.append(elapsed);
  Object.defineProperty(document, "hidden", { configurable: true, value: true });
  document.dispatchEvent(new window.Event("visibilitychange"));
  elapsed.textContent = "old";
  Object.defineProperty(document, "hidden", { configurable: true, value: false });
  document.dispatchEvent(new window.Event("visibilitychange"));
  expect(elapsed.textContent).toBe(strings.session.elapsed(Date.now() - since));
  const observer = new window.MutationObserver((): void => {});
  observer.observe(elapsed, { childList: true, characterData: true, subtree: true });
  document.dispatchEvent(new window.Event("visibilitychange"));
  tickElapsed();
  expect(observer.takeRecords()).toHaveLength(0);
  observer.disconnect(); elapsed.remove();
  tauri.emit("island-toggle", {});
});
