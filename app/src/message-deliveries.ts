import type { Delivery } from "./daemon-state";
import { strings } from "./strings";

interface Actions {
  showOrigin?: boolean;
  cancel(record: Delivery): Promise<unknown>;
  copy(text: string): Promise<void>;
  error(message: string): void;
  changed(): void;
}
interface Clock {
  after(callback: () => void, milliseconds: number): ReturnType<typeof setTimeout>;
  cancel(timer: ReturnType<typeof setTimeout>): void;
}
interface Entry {
  element: HTMLLIElement; text: HTMLTextAreaElement; status: HTMLElement; hint: HTMLElement;
  copy: HTMLButtonElement; discard: HTMLButtonElement; record: Delivery;
  busy: boolean; timer?: ReturnType<typeof setTimeout>;
}
export function deliveryKey(record: Delivery): string {
  return JSON.stringify([record.identity?.daemon_epoch, record.identity?.session_instance_id, record.message_id]);
}
export class DeliveryView {
  private readonly entries = new Map<string, Entry>();
  private readonly expired = new Set<string>();
  private enabled = false;
  constructor(readonly element: HTMLElement, private readonly actions: Actions, private readonly clock: Clock = {
    after: (callback, milliseconds): ReturnType<typeof setTimeout> => setTimeout(callback, milliseconds), cancel: (timer): void => clearTimeout(timer),
  }) { element.classList.add("message-deliveries"); element.hidden = true; }
  update(records: readonly Delivery[], enabled: boolean): void {
    this.enabled = enabled;
    const keys = new Set(records.map(deliveryKey));
    for (const key of this.expired) if (!keys.has(key)) this.expired.delete(key);
    let changed = false;
    for (const [key, entry] of this.entries) {
      if (keys.has(key)) continue;
      if (entry.timer !== undefined) this.clock.cancel(entry.timer);
      entry.element.remove(); this.entries.delete(key); changed = true;
    }
    let index = 0;
    for (const record of records) {
      const key = deliveryKey(record);
      if (this.expired.has(key)) continue;
      let entry = this.entries.get(key);
      if (!entry) {
        const element = document.createElement("li"); element.className = "message-delivery";
        const status = document.createElement("span"); status.setAttribute("role", "status");
        const hint = document.createElement("p");
        const text = document.createElement("textarea"); text.readOnly = true; text.setAttribute("aria-label", strings.recovery.text);
        const copy = document.createElement("button"); copy.type = "button"; copy.textContent = strings.session.messageCopy;
        const discard = document.createElement("button"); discard.type = "button";
        entry = { element, status, hint, text, copy, discard, record, busy: false };
        const current = entry;
        copy.addEventListener("click", (): void => { void this.actions.copy(current.record.text).catch((): void => {
          text.focus(); text.select(); this.actions.error(strings.session.messageCopyFailed);
        }); });
        discard.addEventListener("click", (): void => {
          if (!this.enabled || current.busy || current.record.state === "sending" || current.record.state === "delivered") return;
          current.busy = true; this.controls(current);
          void this.actions.cancel({ ...current.record, identity: current.record.identity ? { ...current.record.identity } : undefined })
            .catch((): void => this.actions.error(strings.session.deliveryDiscardFailed))
            .finally((): void => { current.busy = false; if (this.entries.get(key) === current) this.controls(current); });
        });
        element.append(status, hint, text, copy, discard);
        this.entries.set(key, entry); changed = true;
      }
      entry.record = record;
      if (entry.text.value !== record.text) { entry.text.value = record.text; changed = true; }
      const label = (this.actions.showOrigin ? `${record.session_id} — ` : "") + strings.session.deliveryState[record.state];
      if (entry.status.textContent !== label) { entry.status.textContent = label; changed = true; }
      const hint = record.state === "delivered" ? strings.session.deliveryConfirmed : record.state === "unconfirmed" ? strings.session.messageUnconfirmed : record.state === "sending" ? strings.session.deliverySending : record.state === "failed" ? strings.session.deliveryFailed : "";
      if (entry.hint.textContent !== hint) { entry.hint.textContent = hint; changed = true; }
      if (entry.hint.hidden !== (hint === "")) entry.hint.hidden = hint === "";
      if (entry.element.dataset.state !== record.state) entry.element.dataset.state = record.state;
      this.controls(entry);
      if (record.state === "delivered" && entry.timer === undefined) {
        entry.timer = this.clock.after((): void => {
          if (this.entries.get(key) !== entry) return;
          this.expired.add(key); entry.element.remove(); this.entries.delete(key);
          this.element.hidden = this.entries.size === 0; this.actions.changed();
        }, 5000);
      }
      if (this.element.children[index] !== entry.element) { this.element.insertBefore(entry.element, this.element.children[index] ?? null); changed = true; }
      index += 1;
    }
    if (this.element.hidden !== (this.entries.size === 0)) this.element.hidden = this.entries.size === 0;
    if (changed) this.actions.changed();
  }
  private controls(entry: Entry): void {
    const delivered = entry.record.state === "delivered";
    for (const control of [entry.text, entry.copy, entry.discard]) if (control.hidden !== delivered) control.hidden = delivered;
    const disabled = !this.enabled || entry.busy || !entry.record.identity || entry.record.state === "sending";
    if (entry.discard.disabled !== disabled) entry.discard.disabled = disabled;
    const label = entry.record.state === "queued" ? strings.session.messageCancel : strings.session.deliveryDiscard;
    if (entry.discard.textContent !== label) entry.discard.textContent = label;
  }
  dispose(): void {
    for (const entry of this.entries.values()) if (entry.timer !== undefined) this.clock.cancel(entry.timer);
    this.entries.clear(); this.expired.clear(); this.element.replaceChildren(); this.element.hidden = true;
  }
}
