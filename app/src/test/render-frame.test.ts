import { expect, test } from "bun:test";
import { RenderFrame } from "../render-frame";

test("invalidations coalesce; flushing invalidates old callbacks and paint can request one following frame", (): void => {
  const callbacks = new Map<number, FrameRequestCallback>();
  const pending = new Set<number>(); let id = 0; let paints = 0;
  const request = globalThis.requestAnimationFrame; const cancel = globalThis.cancelAnimationFrame;
  globalThis.requestAnimationFrame = (callback): number => { callbacks.set(++id, callback); pending.add(id); return id; };
  globalThis.cancelAnimationFrame = (id): void => { pending.delete(id); };
  try {
    let again = false;
    const frame = new RenderFrame((): void => { paints += 1; if (again) { again = false; frame.invalidate(); } });
    for (let i = 0; i < 200; i += 1) frame.invalidate();
    expect(pending.size).toBe(1); expect(paints).toBe(0);
    const old = callbacks.get(id)!;
    frame.flush(); expect(paints).toBe(1); expect(pending.size).toBe(0);
    old(0); expect(paints).toBe(1);
    again = true; frame.invalidate();
    const active = id; pending.delete(active); callbacks.get(active)!(1);
    expect(paints).toBe(2); expect(pending.size).toBe(1);
    old(2); expect(paints).toBe(2); expect(pending.size).toBe(1);
    frame.flush(); expect(paints).toBe(3); expect(pending.size).toBe(0);
  } finally { globalThis.requestAnimationFrame = request; globalThis.cancelAnimationFrame = cancel; }
});
