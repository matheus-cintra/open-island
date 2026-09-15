import type { ApprovalRequest, QuestionRequest, QuietScenes, Session, UsageReport } from "./types";

export type ConnectionPhase = "connecting" | "connected" | "reconnecting" | "incompatible";
export interface ActionIdentity {
  daemon_epoch: string;
  session_instance_id: string;
}
export interface Delivery {
  message_id: number;
  session_id: string;
  text: string;
  state: "queued" | "sending" | "delivered" | "failed" | "unconfirmed";
  queued_at_ms: number;
  identity?: ActionIdentity;
  client_submission_id?: string;
  error_code?: string;
}
export interface UiSession extends Session { session_instance_id: string | null }
export interface UiApproval extends ApprovalRequest {
  session_instance_id: string | null;
  pending_generation: number;
}
export interface UiQuestion extends QuestionRequest {
  session_instance_id: string | null;
  pending_generation: number;
}
export interface UiSnapshot {
  schema_version: 1;
  discovering?: boolean;
  daemon_epoch: string;
  publication_revision: number;
  sessions: UiSession[];
  child_sessions?: UiSession[];
  approvals: UiApproval[];
  questions: UiQuestion[];
  message_deliveries: Delivery[];
  config: Record<string, unknown>;
  usage: UsageReport;
  update: { version: string } | null;
  quiet_scenes: QuietScenes & { focus_mode: boolean; screen_off: boolean };
}
export interface UiCache {
  phase: ConnectionPhase;
  generation: number;
  snapshot: { generation: number; snapshot: UiSnapshot } | null;
}
export interface StateUpdate {
  phase: ConnectionPhase;
  generation: number;
  snapshot: UiSnapshot | null;
  hydrate: boolean;
}
export class DaemonState {
  private generation = 0;
  private phase: ConnectionPhase = "connecting";
  private snapshot: UiSnapshot | null = null;
  private hydratedGeneration = -1;

  accept(cache: UiCache): StateUpdate | null {
    if (!cache || !Number.isSafeInteger(cache.generation) || cache.generation < 0
      || !["connecting", "connected", "reconnecting", "incompatible"].includes(cache.phase)) return null;
    if (cache.generation < this.generation) return null;
    if (cache.phase === "connected") {
      const incoming = cache.snapshot;
      if (!incoming || incoming.generation !== cache.generation || incoming.snapshot.schema_version !== 1) return null;
      if (cache.generation === this.generation && incoming.snapshot.daemon_epoch === this.snapshot?.daemon_epoch
        && incoming.snapshot.publication_revision < this.snapshot.publication_revision) return null;
      const hydrate = this.hydratedGeneration !== cache.generation || this.snapshot?.daemon_epoch !== incoming.snapshot.daemon_epoch;
      this.snapshot = incoming.snapshot;
      this.generation = cache.generation;
      this.hydratedGeneration = cache.generation;
      this.phase = cache.phase;
      return { phase: this.phase, generation: this.generation, snapshot: this.snapshot, hydrate };
    }
    this.generation = cache.generation;
    this.phase = cache.phase;
    return { phase: this.phase, generation: this.generation, snapshot: this.snapshot, hydrate: false };
  }
  identity(session: UiSession, originEpoch: string): ActionIdentity | null {
    if (this.phase !== "connected" || !session.session_instance_id || originEpoch !== this.snapshot?.daemon_epoch) return null;
    return { daemon_epoch: originEpoch, session_instance_id: session.session_instance_id };
  }
}
export interface StateSource {
  listen(callback: (cache: UiCache) => void): Promise<() => void>;
  read(): Promise<UiCache>;
}
export async function bindDaemonState(source: StateSource, changed: (update: StateUpdate) => void, failed: (error: unknown) => void): Promise<() => void> {
  const state = new DaemonState();
  let received = 0;
  let active = true;
  const apply = (cache: UiCache): void => {
    if (!active) return;
    const update = state.accept(cache);
    if (update) changed(update);
  };
  const unlisten = await source.listen((cache: UiCache): void => { received += 1; apply(cache); });
  const beforeRead = received;
  try {
    const cache = await source.read();
    if (received === beforeRead) apply(cache);
  } catch (error: unknown) { if (active && received === beforeRead) failed(error); }
  return (): void => { active = false; unlisten(); };
}
