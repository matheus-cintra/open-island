import type { Delivery } from "./daemon-state";
import { strings } from "./strings";

export interface RecoveryRecord {
  client_submission_id: string;
  origin_epoch: string;
  session_instance_id: string;
  text: string;
  last_state: Delivery["state"];
  previous_state: Delivery["state"] | null;
  message_id: number | null;
  local: boolean;
  previous_epoch?: boolean;
}
export interface RecoveryActions {
  copy(text: string): Promise<void>;
  discard(id: string): Promise<boolean>;
  error(message: string): void;
  changed(): void;
}
export class RecoveryView {
  private readonly entries = new Map<string, { element: HTMLElement; input: HTMLTextAreaElement; label: HTMLElement; record: RecoveryRecord }>();
  private readonly list: HTMLElement;
  private readonly discarded = new Set<string>();

  constructor(readonly element: HTMLElement, private readonly actions: RecoveryActions) {
    element.classList.add("shell-message-recovery");
    element.hidden = true;
    const title = document.createElement("h3");
    title.textContent = strings.recovery.title;
    const explanation = document.createElement("p");
    explanation.textContent = strings.recovery.memory;
    this.list = document.createElement("div");
    element.append(title, explanation, this.list);
  }
  update(records: readonly RecoveryRecord[]): void {
    const visible = records.filter((record): boolean => !this.discarded.has(record.client_submission_id) && (record.local || record.last_state === "failed" || record.last_state === "unconfirmed"));
    const ids = new Set(visible.map((record): string => record.client_submission_id));
    let changed = this.element.hidden !== (visible.length === 0);
    for (const [id, entry] of this.entries) {
      if (ids.has(id)) continue;
      entry.element.remove();
      this.entries.delete(id);
      changed = true;
    }
    for (const record of visible) {
      let entry = this.entries.get(record.client_submission_id);
      if (!entry) {
        const element = document.createElement("section");
        const label = document.createElement("p");
        const input = document.createElement("textarea");
        input.readOnly = true;
        input.setAttribute("aria-label", strings.recovery.text);
        const copy = document.createElement("button");
        copy.type = "button";
        copy.textContent = strings.session.messageCopy;
        const discard = document.createElement("button");
        discard.type = "button";
        discard.textContent = strings.session.messageDiscard;
        entry = { element, input, label, record };
        const current = entry;
        copy.addEventListener("click", (): void => {
          void this.actions.copy(current.record.text).catch((): void => {
            input.focus();
            input.select();
            this.actions.error(strings.session.messageCopyFailed);
          });
        });
        discard.addEventListener("click", (): void => {
          if (discard.disabled) return;
          discard.disabled = true;
          void this.actions.discard(current.record.client_submission_id).then((): void => {
            this.discarded.add(current.record.client_submission_id);
            if (this.discarded.size > 512) {
              const oldest = this.discarded.values().next().value;
              if (oldest !== undefined) this.discarded.delete(oldest);
            }
            if (this.entries.get(current.record.client_submission_id) !== current) return;
            current.element.remove();
            this.entries.delete(current.record.client_submission_id);
            this.element.hidden = this.entries.size === 0;
            this.actions.changed();
          }).catch((): void => { this.actions.error(strings.recovery.discardFailed); })
            .finally((): void => { discard.disabled = false; });
        });
        element.append(label, input, copy, discard);
        this.entries.set(record.client_submission_id, entry);
        this.list.append(element);
        changed = true;
      }
      entry.record = record;
      if (entry.input.value !== record.text) { entry.input.value = record.text; changed = true; }
      const label = record.local ? (record.previous_epoch === false ? strings.recovery.missingRecord : strings.recovery.previousConnection) : record.last_state === "failed" ? strings.recovery.failed : strings.recovery.unconfirmed;
      if (entry.label.textContent !== label) { entry.label.textContent = label; changed = true; }
    }
    this.element.hidden = visible.length === 0;
    if (changed) this.actions.changed();
  }
}
export interface RecoverySource {
  listen(callback: (records: RecoveryRecord[]) => void): Promise<() => void>;
  read(): Promise<RecoveryRecord[]>;
}
export async function bindRecovery(source: RecoverySource, view: RecoveryView, failed: () => void): Promise<() => void> {
  let active = true;
  let received = 0;
  const unlisten = await source.listen((records): void => { received += 1; if (active) view.update(records); });
  const before = received;
  try {
    const records = await source.read();
    if (active && received === before) view.update(records);
  } catch { if (active) failed(); }
  return (): void => { active = false; unlisten(); };
}
