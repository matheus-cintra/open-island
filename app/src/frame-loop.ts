interface Frames {
  request(callback: FrameRequestCallback): number;
  cancel(id: number): void;
}
export class FrameLoop {
  private enabled = false;
  private generation = 0;
  private pending: number | null = null;
  constructor(private readonly draw: FrameRequestCallback, private readonly frames: Frames = {
    request: (callback): number => requestAnimationFrame(callback),
    cancel: (id): void => cancelAnimationFrame(id),
  }) {}
  setEnabled(enabled: boolean): void {
    if (this.enabled === enabled) return;
    this.enabled = enabled;
    this.generation += 1;
    if (this.pending !== null) this.frames.cancel(this.pending);
    this.pending = null;
    if (enabled) this.schedule(this.generation);
  }
  private schedule(generation: number): void {
    this.pending = this.frames.request((time): void => {
      if (!this.enabled || generation !== this.generation) return;
      this.pending = null;
      this.draw(time);
      if (this.enabled && generation === this.generation) this.schedule(generation);
    });
  }
}
