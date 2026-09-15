import { invoke } from "@tauri-apps/api/core";
import { strings } from "./strings";

interface Capacity { active: number; high_water: number; rejected: number; limit: number }
interface Counters {
  connections_legacy: Capacity;
  connections_managed: Capacity;
  fast_requests: Capacity;
  blocking_requests: Capacity;
  bulk_requests: Capacity;
  no_island: number;
  pending_approvals: number;
  pending_questions: number;
  outbox: { max_connection_items: number; max_connection_bytes: number; overflow_disconnects: number; item_limit: number; byte_limit: number };
}
interface Report {
  schema_version: 1;
  platform: string;
  arch: string;
  collector_version: string;
  app_version: string | null;
  daemon: { state: string; version: string | null; pid: number | null; epoch: string | null; capabilities: string[] };
  socket: { exists: boolean; connectable: boolean };
  model: string;
  audio?: { alsa_capture_devices: boolean | null; capture_tested: boolean };
  counters?: Counters | null;
  local?: { service: { installed: boolean; active: boolean | null }; daemon_binary: { present: boolean; version: string | null; probe: string }; app_binary: { present: boolean; version: string | null; probe: string } };
  hooks?: { agent: string; managed: boolean; state: string }[];
}
const copy = strings.settings.about;
export class Diagnostics {
  private report: Report | null = null;
  private pending = false;
  private failed = false;
  constructor(private readonly read: () => Promise<Report>, private readonly clipboard: (text: string) => Promise<void>) {}
  async refresh(): Promise<void> {
    if (this.pending) return;
    this.pending = true;
    this.report = null;
    this.failed = false;
    try { this.report = await this.read(); }
    catch { this.failed = true; }
    finally { this.pending = false; }
  }
  available(): boolean { return this.report !== null; }
  audio(): string {
    switch (this.report?.audio?.alsa_capture_devices) {
      case true: return copy.diagnosticAudioDetected;
      case false: return copy.diagnosticAudioNotDetected;
      default: return copy.diagnosticAudioUnchecked;
    }
  }
  model(): string {
    if (this.report?.model === "configured") return strings.voice.configured;
    if (this.report?.model === "unavailable") return strings.voice.error("model_unavailable");
    return copy.diagnosticUnchecked;
  }
  summary(): string {
    if (this.failed) return copy.diagnosticFailed;
    if (!this.report) return copy.diagnosticEmpty;
    switch (this.report.daemon.state) {
      case "ready": return copy.diagnosticReady;
      case "incompatible": return copy.diagnosticIncompatible;
      case "unavailable": return copy.diagnosticOffline;
      default: return copy.diagnosticInvalid;
    }
  }
  countersAvailable(): boolean { return this.report?.counters != null; }
  capacity(): string {
    const counters = this.report?.counters;
    if (!counters) return "—";
    return copy.diagnosticCapacity(counters.connections_managed.active + counters.connections_legacy.active,
      counters.pending_approvals + counters.pending_questions, counters.no_island);
  }
  localAvailable(): boolean { return this.report?.local !== undefined; }
  service(): string {
    const service = this.report?.local?.service;
    if (!service || service.active === null) return copy.diagnosticUnchecked;
    if (service.active) return copy.diagnosticServiceActive;
    return service.installed ? copy.diagnosticServiceInactive : copy.diagnosticServiceMissing;
  }
  hooks(): string {
    const hooks = this.report?.hooks;
    if (!hooks) return copy.diagnosticUnchecked;
    const review = hooks.filter((hook) => !["missing", "current"].includes(hook.state)).map((hook) => hook.agent);
    return review.length ? copy.diagnosticHooksReview(review.join(", ")) : copy.diagnosticHooksCurrent;
  }
  versions(): string { return copy.diagnosticVersions(this.report?.app_version ?? "—", this.report?.daemon.version ?? "—"); }
  async copy(): Promise<string> {
    if (!this.report) return copy.diagnosticCopyFailed;
    try { await this.clipboard(JSON.stringify(this.report, null, 2)); return copy.diagnosticCopied; }
    catch { return copy.diagnosticCopyFailed; }
  }
}
export const diagnostics = new Diagnostics(() => invoke<Report>("diagnostic_report"), (text) => navigator.clipboard.writeText(text));
