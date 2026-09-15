export interface RecoveredDraft {
  id: number;
  text: string;
  error: unknown;
}
export interface ComposerState {
  text: string;
  pending: boolean;
  recovered: readonly RecoveredDraft[];
}
export class MessageComposer {
  private text = "";
  private revision = 0;
  private pending = false;
  private recovered: RecoveredDraft[] = [];
  private nextId = 1;

  constructor(
    private readonly send: (text: string) => Promise<unknown>,
    private readonly changed: (state: ComposerState) => void,
    private readonly failed: (error: unknown) => void,
  ) {}

  edit(text: string): void {
    if (text === this.text) return;
    this.text = text;
    this.revision += 1;
  }

  state(): ComposerState {
    return { text: this.text, pending: this.pending, recovered: this.recovered };
  }

  draftRevision(): number { return this.revision; }

  insertTranscript(text: string, expectedRevision?: number): boolean {
    if (this.pending || (expectedRevision !== undefined && expectedRevision !== this.revision)) return false;
    const next = this.text + (this.text && !/\s$/.test(this.text) ? " " : "") + text;
    if (new TextEncoder().encode(next).length > 65536) return false;
    this.edit(next);
    this.changed(this.state());
    return true;
  }

  async submit(): Promise<void> {
    if (this.pending || this.text.trim() === "") return;
    if (new TextEncoder().encode(this.text).length > 65536) {
      this.failed("message_too_large");
      return;
    }
    if (this.recovered.length >= 32) {
      this.failed("recovery_full");
      return;
    }
    const text = this.text;
    const revision = this.revision;
    this.pending = true;
    this.changed(this.state());
    try {
      await this.send(text);
      if (this.revision === revision) this.edit("");
    } catch (error: unknown) {
      if (this.revision !== revision) this.recovered = [...this.recovered, { id: this.nextId++, text, error }];
      this.failed(error);
    } finally {
      this.pending = false;
      this.changed(this.state());
    }
  }

  discard(id: number): void {
    this.recovered = this.recovered.filter((entry) => entry.id !== id);
    this.changed(this.state());
  }
}
