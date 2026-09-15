import { describe, expect, test } from "bun:test";
import { bindDaemonState, DaemonState, type UiCache, type UiSnapshot } from "../daemon-state";

function snapshot(epoch: string, revision = 1): UiSnapshot {
  return { schema_version: 1, daemon_epoch: epoch, publication_revision: revision, sessions: [], approvals: [], questions: [], message_deliveries: [], config: {}, usage: { providers: [] }, update: null, quiet_scenes: { active: false, focus_mode: false, screen_off: false } };
}
function connected(generation: number, epoch: string, revision = 1): UiCache {
  return { phase: "connected", generation, snapshot: { generation, snapshot: snapshot(epoch, revision) } };
}
describe("authoritative daemon state", (): void => {
  test("a failed late cache read cannot override a current event", async (): Promise<void> => {
    let listener: ((cache: UiCache) => void) | undefined;
    let reject: ((reason: Error) => void) | undefined;
    const phases: string[] = [];
    const failures: unknown[] = [];
    const bound = bindDaemonState({
      listen: async (callback): Promise<() => void> => { listener = callback; return (): void => {}; },
      read: (): Promise<UiCache> => new Promise((_, fail): void => { reject = fail; }),
    }, (update): void => { phases.push(update.phase); }, (error): void => { failures.push(error); });
    await Promise.resolve();
    listener?.(connected(2, "current"));
    reject?.(new Error("old read failed"));
    const dispose = await bound;
    expect(phases).toEqual(["connected"]);
    expect(failures).toEqual([]);
    dispose();
  });
  test("reconnection preserves stale state and empty snapshot is authoritative", (): void => {
    const state = new DaemonState();
    const first = state.accept(connected(1, "one"));
    expect(first?.hydrate).toBe(true);
    expect(state.accept({ phase: "reconnecting", generation: 1, snapshot: null })?.snapshot).toBe(first?.snapshot ?? null);
    expect(state.accept(connected(2, "two"))?.hydrate).toBe(true);
    expect(state.accept(connected(1, "one", 100))).toBeNull();
    expect(state.accept(connected(2, "two", 2))?.hydrate).toBe(false);
    expect(state.accept(connected(2, "two", 1))).toBeNull();
  });
  test("listeners precede cache read and late cache cannot overwrite event", async (): Promise<void> => {
    let listener: ((cache: UiCache) => void) | undefined;
    let resolve: ((cache: UiCache) => void) | undefined;
    const phases: string[] = [];
    let removed = false;
    const bound = bindDaemonState({
      listen: async (callback): Promise<() => void> => { listener = callback; return (): void => { removed = true; }; },
      read: (): Promise<UiCache> => { expect(listener).toBeDefined(); return new Promise((done): void => { resolve = done; }); },
    }, (update): void => { phases.push(update.snapshot?.daemon_epoch ?? update.phase); }, (): never => { throw new Error("unexpected failure"); });
    await Promise.resolve();
    listener?.(connected(2, "current"));
    resolve?.(connected(1, "old"));
    const dispose = await bound;
    expect(phases).toEqual(["current"]);
    dispose();
    listener?.(connected(3, "after-disposal"));
    expect(phases).toEqual(["current"]);
    expect(removed).toBe(true);
  });
});
