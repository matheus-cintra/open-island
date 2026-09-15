import { expect, test } from "bun:test";
import { FrameLoop } from "../frame-loop";

test("hidden work cancels its frame and stale callbacks cannot create duplicate loops", (): void => {
  let next = 0; let draws = 0; let peak = 0;
  const pending = new Map<number, FrameRequestCallback>();
  const callbacks: FrameRequestCallback[] = [];
  const loop = new FrameLoop((): void => { draws += 1; }, {
    request: (callback): number => { const id = ++next; pending.set(id, callback); callbacks.push(callback); peak = Math.max(peak, pending.size); return id; },
    cancel: (id): void => { pending.delete(id); },
  });
  for (let i = 0; i < 100; i += 1) { loop.setEnabled(true); loop.setEnabled(true); loop.setEnabled(false); }
  for (const callback of callbacks) callback(1);
  expect(draws).toBe(0);
  expect(pending.size).toBe(0);
  loop.setEnabled(true);
  const [id, callback] = [...pending][0]; pending.delete(id); callback(2);
  expect(draws).toBe(1);
  expect(pending.size).toBe(1);
  expect(peak).toBe(1);
  loop.setEnabled(false);
  expect(pending.size).toBe(0);
});
