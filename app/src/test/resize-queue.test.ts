import { expect, test } from "bun:test";
import { IslandWindow, NativeResize } from "../island-window";
import type { Size } from "../types";

const tick = async (): Promise<void> => { await Promise.resolve(); await Promise.resolve(); };
test("twenty reversals keep one native resize in flight and settle at the latest target", async () => {
  const sent: Size[] = [];
  const finish: (() => void)[] = [];
  let active = 0; let peak = 0;
  const resize = new NativeResize((size) => {
    sent.push(size); active += 1; peak = Math.max(active, peak);
    return new Promise<void>((resolve) => finish.push(() => { active -= 1; resolve(); }));
  });
  const draining = resize.request({ w: 200, h: 46 });
  for (let i = 0; i < 20; i++) void resize.request({ w: 664, h: i % 2 ? 400 : 46 });
  expect(sent).toEqual([{ w: 200, h: 46 }]);
  finish.shift()!(); await tick();
  expect(sent).toEqual([{ w: 200, h: 46 }, { w: 664, h: 400 }]);
  finish.shift()!(); await draining;
  await resize.request({ w: 664, h: 400 });
  expect(sent).toHaveLength(2); expect(peak).toBe(1); expect(active).toBe(0);
});

test("failed dimension retries only on a later layout and never skips the latest target", async () => {
  const sent: Size[] = [];
  let reject!: () => void;
  const resize = new NativeResize((size) => {
    sent.push(size);
    if (sent.length === 1) return new Promise<void>((_, fail) => { reject = () => fail(new Error("fixture")); });
    return Promise.resolve();
  });
  const first = resize.request({ w: 200, h: 46 });
  void resize.request({ w: 200, h: 46 });
  reject(); await first; await tick();
  expect(sent).toHaveLength(1); expect(resize.needsRetry()).toBe(true);
  await resize.request({ w: 664, h: 400 });
  expect(sent[sent.length - 1]).toEqual({ w: 664, h: 400 }); expect(resize.needsRetry()).toBe(false);
});

test("cancelled animation callbacks cannot restore an obsolete compact target", async () => {
  const callbacks: FrameRequestCallback[] = [];
  const sent: Size[] = [];
  const endings: string[] = [];
  let now = 0;
  const window = new IslandWindow({ w: 664, h: 400 }, new NativeResize(async (size) => { sent.push(size); }), () => false, () => {}, {
    now: () => now, request: (callback) => { callbacks.push(callback); return callbacks.length; }, cancel: () => {},
  });
  window.morph({ w: 200, h: 46 }, () => {}, () => endings.push("compact"));
  callbacks[0](160); await tick(); now = 160;
  window.morph({ w: 664, h: 420 }, () => {}, () => endings.push("expanded"));
  callbacks[1](500);
  callbacks[callbacks.length - 1]!(500); await tick();
  expect(endings).toEqual(["expanded"]);
  expect(window.current).toEqual({ w: 664, h: 420 });
  expect(sent[sent.length - 1]).toEqual({ w: 664, h: 420 });
});

test("reduced motion applies the final geometry without scheduling a frame", async () => {
  let frames = 0; let end = 0;
  const sent: Size[] = [];
  const window = new IslandWindow({ w: 200, h: 46 }, new NativeResize(async (size) => { sent.push(size); }), () => true, () => {}, {
    now: () => 0, request: () => ++frames, cancel: () => {},
  });
  window.morph({ w: 664, h: 400 }, () => {}, () => end++);
  expect(window.current).toEqual({ w: 664, h: 400 });
  expect(end).toBe(1); expect(frames).toBe(0); expect(sent).toHaveLength(1);
});
