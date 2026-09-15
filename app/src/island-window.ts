import type { Size } from "./types";

const same = (a: Size | null, b: Size): boolean => a?.w === b.w && a.h === b.h;

export class NativeResize {
  private confirmed: Size | null = null;
  private pending: Size | null = null;
  private running: Promise<void> | null = null;
  private failed = false;
  constructor(private readonly invoke: (size: Size) => Promise<void>) {}
  needsRetry(): boolean { return this.failed; }
  request(size: Size): Promise<void> {
    this.pending = { ...size };
    if (this.running) return this.running;
    this.running = this.drain().finally(() => {
      this.running = null;
      if (this.pending) return this.request(this.pending);
    });
    return this.running;
  }
  private async drain(): Promise<void> {
    while (this.pending) {
      const size = this.pending;
      this.pending = null;
      if (same(this.confirmed, size) && !this.failed) continue;
      try {
        await this.invoke(size);
        this.confirmed = size;
        this.failed = false;
      } catch {
        this.failed = true;
        // A later layout event can retry; a failure alone never spins.
        if (this.pending && same(this.pending, size)) this.pending = null;
      }
    }
  }
}

interface AnimationClock {
  now(): number;
  request(callback: FrameRequestCallback): number;
  cancel(id: number): void;
}
export class IslandWindow {
  current: Size;
  from: Size;
  goal: Size;
  private frame = 0;
  private generation = 0;
  constructor(
    initial: Size,
    readonly resize: NativeResize,
    private readonly reduced: () => boolean,
    private readonly paint: (height: number, from: Size, goal: Size) => void,
    private readonly clock: AnimationClock = {
      now: () => performance.now(),
      request: (callback) => requestAnimationFrame(callback),
      cancel: (id) => cancelAnimationFrame(id),
    },
  ) { this.current = { ...initial }; this.from = { ...initial }; this.goal = { ...initial }; }
  snap(size: Size): void {
    this.clock.cancel(this.frame); this.generation += 1;
    this.current = { ...size }; this.from = { ...size }; this.goal = { ...size };
    void this.resize.request(size);
  }
  morph(target: Size, onStart: () => void, onEnd: () => void): void {
    this.clock.cancel(this.frame);
    const generation = ++this.generation;
    if (same(this.current, target)) {
      this.goal = { ...target };
      this.paint(target.h, this.from, this.goal);
      void this.resize.request(target);
      onEnd();
      return;
    }
    this.from = { ...this.current }; this.goal = { ...target };
    const started = this.clock.now();
    onStart();
    const apply = (size: Size): void => {
      this.current = size;
      this.paint(size.h, this.from, this.goal);
      void this.resize.request(size);
    };
    if (this.reduced()) { apply({ ...target }); onEnd(); return; }
    const step = (now: number): void => {
      if (generation !== this.generation) return;
      const t = Math.min(1, (now - started) / 320);
      const e = spring(t);
      apply({ w: Math.round(this.from.w + (target.w - this.from.w) * e), h: Math.round(this.from.h + (target.h - this.from.h) * e) });
      if (t < 1) this.frame = this.clock.request(step);
      else { apply({ ...target }); onEnd(); }
    };
    this.frame = this.clock.request(step);
  }
}
function spring(t: number): number {
  if (t >= 1) return 1;
  const damping = 0.7;
  const omega = 10;
  const damped = omega * Math.sqrt(1 - damping * damping);
  return 1 - Math.exp(-damping * omega * t) * (Math.cos(damped * t) + damping / Math.sqrt(1 - damping * damping) * Math.sin(damped * t));
}
