export interface VoiceTarget { session_id: string; session_instance_id: string; daemon_epoch: string; }
export type VoicePhase = "idle" | "requesting_permission" | "recording" | "transcribing" | "ready" | "cancelled" | "error";
export interface VoiceState {
  revision: number; job_id: string | null; target: VoiceTarget | null; phase: VoicePhase;
  recorded_ms: number; worker_active: boolean; transcript: string | null; error: string | null;
  target_unavailable?: boolean;
}
export interface VoiceResult { id: string; target: VoiceTarget; text: string; applied: boolean; target_unavailable: boolean; }
export interface VoiceDestination { target(): VoiceTarget | null; label(): string; insert(text: string): boolean; }
export type VoiceCall = <T>(command: string, args?: Record<string, unknown>) => Promise<T>;
export type VoiceEvents = (callback: (state: VoiceState) => void) => Promise<() => void>;
const phases = new Set<VoicePhase>(["idle", "requesting_permission", "recording", "transcribing", "ready", "cancelled", "error"]);
export function sameVoiceTarget(a: VoiceTarget | null, b: VoiceTarget | null): boolean {
  return a !== null && b !== null && a.session_id === b.session_id && a.session_instance_id === b.session_instance_id && a.daemon_epoch === b.daemon_epoch;
}
export class VoiceController {
  state: VoiceState = { revision: 0, job_id: null, target: null, phase: "idle", recorded_ms: 0, worker_active: false, transcript: null, error: null };
  results: VoiceResult[] = [];
  destination: VoiceDestination | null = null;
  starting = false;
  private cancelPending = false;
  private startingRequest = false;
  private connection: Promise<void> | null = null;
  private origin: { id: string; target: VoiceTarget; apply(text: string): boolean } | null = null;
  private readonly listeners = new Set<() => void>();
  private readonly seen = new Set<string>();
  constructor(private readonly call: VoiceCall, private readonly events?: VoiceEvents) {}
  async connect(): Promise<void> {
    if (!this.events) return;
    if (!this.connection) this.connection = this.bindEvents();
    const connection = this.connection;
    try { await connection; } catch (error) { if (this.connection === connection) this.connection = null; throw error; }
  }
  private async bindEvents(): Promise<void> {
    let received = 0;
    const unlisten = await this.events!((state): void => { this.accept(state); received += 1; });
    const before = received;
    try { await this.refresh(); }
    catch (error) { if (received === before) { unlisten(); throw error; } }
  }
  active(): boolean { return this.starting || this.state.worker_active; }
  subscribe(callback: () => void): () => void { this.listeners.add(callback); return () => { this.listeners.delete(callback); }; }
  private notify(): void { for (const callback of this.listeners) callback(); }
  select(destination: VoiceDestination): void { this.destination = destination; this.notify(); }
  async start(target: VoiceTarget, apply: (text: string) => boolean): Promise<void> {
    if (this.active()) throw new Error("voice_busy");
    if (this.results.length >= 8) throw new Error("voice_results_full");
    this.starting = true; this.cancelPending = false; this.notify();
    try {
      await this.connect();
      if (this.state.worker_active) throw new Error("voice_busy");
      if (this.results.length >= 8) throw new Error("voice_results_full");
      this.startingRequest = true;
      this.origin = null;
      const state = await this.call<VoiceState>("voice_start", { target: { ...target } });
      if (typeof state.job_id === "string") this.origin = { id: state.job_id, target: { ...target }, apply };
      this.accept(state);
      this.accept(this.state);
      if (this.cancelPending && state.job_id) await this.cancelJob(state.job_id);
    } finally { this.startingRequest = false; this.starting = false; this.accept(this.state); }
  }
  async refresh(): Promise<void> { this.accept(await this.call<VoiceState>("voice_state")); }
  private accept(next: VoiceState): void {
    const validTarget = next.target === null || (typeof next.target === "object" && next.target !== null &&
      [next.target.session_id, next.target.session_instance_id, next.target.daemon_epoch].every((value) => typeof value === "string" && value.length > 0));
    if (!Number.isSafeInteger(next.revision) || next.revision < 0 || !phases.has(next.phase) || typeof next.worker_active !== "boolean" ||
      !Number.isFinite(next.recorded_ms) || next.recorded_ms < 0 || next.recorded_ms > 60000 || !validTarget ||
      !(next.job_id === null || typeof next.job_id === "string") ||
      !(next.transcript === null || typeof next.transcript === "string") || !(next.error === null || typeof next.error === "string")) throw new Error("voice_invalid_state");
    if (next.target_unavailable !== undefined && typeof next.target_unavailable !== "boolean") throw new Error("voice_invalid_state");
    if (next.revision < this.state.revision) return;
    if (next.revision === this.state.revision && next.job_id === this.state.job_id && next.phase === "recording") {
      next = { ...next, recorded_ms: Math.max(next.recorded_ms, this.state.recorded_ms) };
    }
    this.state = next;
    if (next.phase === "ready" && next.job_id && next.target && typeof next.transcript === "string" && !this.seen.has(next.job_id)
      && !(this.startingRequest && this.origin?.id !== next.job_id)) {
      this.seen.add(next.job_id);
      if (this.seen.size > 256) { const oldest = this.seen.values().next().value; if (oldest) this.seen.delete(oldest); }
      if (this.cancelPending) { this.notify(); return; }
      let applied = false;
      if (next.target_unavailable) this.destination = null;
      if (!next.target_unavailable && this.origin?.id === next.job_id && sameVoiceTarget(this.origin.target, next.target) && !this.cancelPending) {
        try { applied = this.origin.apply(next.transcript); } catch { /* Preserve the transcript if the editor disappeared or failed. */ }
      }
      this.results = [...this.results, { id: next.job_id, target: { ...next.target }, text: next.transcript, applied, target_unavailable: next.target_unavailable === true }];
    }
    this.notify();
  }
  async stop(): Promise<void> {
    if (this.starting) { await this.cancel(); return; }
    if (this.state.job_id) this.accept(await this.call<VoiceState>("voice_stop", { jobId: this.state.job_id }));
  }
  async cancel(): Promise<void> {
    this.cancelPending = true;
    if (this.starting) return;
    if (this.state.job_id) await this.cancelJob(this.state.job_id);
  }
  private async cancelJob(id: string): Promise<void> { this.accept(await this.call<VoiceState>("voice_cancel", { jobId: id })); }
  insert(id: string): boolean {
    const result = this.results.find((entry) => entry.id === id);
    if (!result || !this.destination?.target() || !this.destination.insert(result.text)) return false;
    result.applied = true; this.notify(); return true;
  }
  async discard(id: string): Promise<void> {
    if (this.state.job_id === id) await this.cancelJob(id);
    this.results = this.results.filter((entry) => entry.id !== id);
    this.notify();
  }
}
