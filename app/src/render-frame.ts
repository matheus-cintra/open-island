/** Coalesces invalidations, while state and authorization continue updating immediately. */
export class RenderFrame {
  private pending: number | null = null;
  private revision = 0;
  constructor(private readonly paint: () => void) {}
  invalidate(): void {
    if (this.pending !== null) return;
    const revision = ++this.revision;
    this.pending = requestAnimationFrame((): void => {
      if (revision !== this.revision || this.pending === null) return;
      this.pending = null;
      this.paint();
    });
  }
  flush(): void {
    if (this.pending === null) return;
    cancelAnimationFrame(this.pending);
    this.pending = null;
    this.revision += 1;
    this.paint();
  }
}
