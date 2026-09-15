import { invoke } from "@tauri-apps/api/core";
import { reasonOf } from "./json";
import { strings } from "./strings";
export interface ModelStatus { configured: boolean; error: string | null; }
type Call = <T>(command: string) => Promise<T>;
export class VoiceSettings {
  private status: ModelStatus = { configured: false, error: null };
  private revision = 0;
  private busy = false;
  constructor(private readonly call: Call) {}
  summary(): string {
    return this.status.configured ? strings.voice.configured : this.status.error ? strings.voice.error(this.status.error) : strings.voice.unavailable;
  }
  async refresh(): Promise<void> {
    if (this.busy) return;
    const revision = ++this.revision;
    try {
      const status = await this.call<ModelStatus>("voice_model_status");
      if (revision === this.revision) this.status = status;
    } catch {
      if (revision === this.revision) this.status = { configured: false, error: "model_unavailable" };
    }
  }
  async change(command: "voice_select_model" | "voice_clear_model"): Promise<string | null> {
    if (this.busy) return strings.voice.error("voice_model_busy");
    this.busy = true; this.revision += 1;
    try {
      const status = await this.call<ModelStatus | null>(command);
      if (status) this.status = status;
      return command === "voice_clear_model" ? strings.voice.modelRemoved : null;
    } catch (error) { return strings.voice.error(reasonOf(error)); }
    finally { this.busy = false; }
  }
}
export const voiceSettings = new VoiceSettings(<T>(command: string): Promise<T> => invoke<T>(`plugin:voice|${command}`));
