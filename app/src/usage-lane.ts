import { strings } from "./strings";
export interface UsagePart {
  key: string; label: string; marker?: boolean; value?: string; percent?: number; severity?: string; reset?: string;
}
interface Entry {
  element: HTMLElement; label: HTMLElement; track: HTMLElement; fill: HTMLElement; value: HTMLElement; reset: HTMLElement; separator: HTMLElement;
}
function setChildren(parent: HTMLElement, wanted: readonly HTMLElement[]): void {
  const current = [...parent.children];
  if (current.length === wanted.length && current.every((child, index): boolean => child === wanted[index])) return;
  parent.replaceChildren(...wanted);
}
function text(element: HTMLElement, value: string): void {
  if (element.textContent !== value) element.textContent = value;
}
export class UsageLane {
  private readonly entries = new Map<string, Entry>();
  constructor(private readonly root: HTMLElement) {}
  update(parts: readonly UsagePart[], stale: boolean): void {
    const counts = new Map<string, number>();
    const keyed = parts.map((part): { part: UsagePart; key: string } => {
      const index = counts.get(part.key) ?? 0; counts.set(part.key, index + 1);
      return { part, key: JSON.stringify([part.key, index]) };
    });
    const keys = new Set(keyed.map((entry): string => entry.key));
    for (const [key, entry] of this.entries) {
      if (keys.has(key)) continue;
      entry.element.remove(); entry.separator.remove(); this.entries.delete(key);
    }
    let position = 0; let windows = 0;
    const place = (element: HTMLElement): void => {
      if (this.root.children[position] !== element) this.root.insertBefore(element, this.root.children[position] ?? null);
      position += 1;
    };
    for (const { part, key } of keyed) {
      let entry = this.entries.get(key);
      if (!entry) {
        const element = document.createElement("span");
        const label = document.createElement("span"); label.className = "usage-name";
        const track = document.createElement("span"); track.className = "usage-track"; track.setAttribute("aria-hidden", "true");
        const fill = document.createElement("span"); fill.className = "usage-fill"; track.append(fill);
        const value = document.createElement("span");
        const reset = document.createElement("span"); reset.className = "usage-reset";
        const separator = document.createElement("span"); separator.className = "usage-separator"; separator.textContent = strings.usage.separator;
        element.append(label);
        entry = { element, label, track, fill, value, reset, separator }; this.entries.set(key, entry);
      }
      const kind = part.marker ? "usage-stale" : `usage-window ${part.severity ?? ""}`.trim();
      if (entry.element.className !== kind) entry.element.className = kind;
      const labelClass = part.marker ? "" : "usage-name";
      if (entry.label.className !== labelClass) entry.label.className = labelClass;
      text(entry.label, part.label);
      const children: HTMLElement[] = [entry.label];
      if (part.percent !== undefined) {
        const width = `${Math.max(0, Math.min(100, Math.round(part.percent)))}%`;
        if (entry.fill.style.width !== width) entry.fill.style.width = width;
        children.push(entry.track);
      }
      if (part.value !== undefined) {
        text(entry.value, part.value);
        const className = `usage-percent ${part.severity ?? ""}`.trim();
        if (entry.value.className !== className) entry.value.className = className;
        children.push(entry.value);
      }
      if (part.reset !== undefined) {
        text(entry.reset, part.reset);
        children.push(entry.reset);
      }
      setChildren(entry.element, children);
      const title = part.value === undefined ? "" : strings.usage.windowTitle(part.label, part.value, part.reset);
      if (entry.element.title !== title) entry.element.title = title;
      if (!part.marker && windows++ > 0) place(entry.separator); else entry.separator.remove();
      place(entry.element);
    }
    if (this.root.classList.contains("is-stale") !== stale) this.root.classList.toggle("is-stale", stale);
    if (this.root.hidden !== (parts.length === 0)) this.root.hidden = parts.length === 0;
  }
}
