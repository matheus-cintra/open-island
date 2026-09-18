import { strings } from "./strings";
import type { Session } from "./types";

const MAX_CELLS = 12;

export class StateStrip {
  private readonly cells = new Map<string, HTMLElement>();
  private more: HTMLElement | null = null;

  constructor(private readonly root: HTMLElement) {}

  update(sessions: readonly Session[], off: boolean): void {
    const shown = sessions.slice(0, MAX_CELLS);
    const wanted = new Set(shown.map((session): string => session.id));
    for (const [id, cell] of this.cells) {
      if (wanted.has(id)) continue;
      cell.remove();
      this.cells.delete(id);
    }
    shown.forEach((session, index): void => {
      let cell = this.cells.get(session.id);
      if (cell === undefined) {
        cell = document.createElement("i");
        this.cells.set(session.id, cell);
      }
      const className = `strip-cell ${session.attention ?? "working"}`;
      if (cell.className !== className) cell.className = className;
      const order = String(index);
      if (cell.style.getPropertyValue("--i") !== order) cell.style.setProperty("--i", order);
      if (this.root.children[index] !== cell) this.root.insertBefore(cell, this.root.children[index] ?? null);
    });
    if (sessions.length > MAX_CELLS) {
      if (this.more === null) {
        this.more = document.createElement("i");
        this.more.className = "strip-cell more";
      }
      if (this.root.lastElementChild !== this.more) this.root.append(this.more);
    } else if (this.more !== null) {
      this.more.remove();
      this.more = null;
    }
    if (this.root.classList.contains("is-off") !== off) this.root.classList.toggle("is-off", off);
    const label = strings.island.sessions(sessions.length);
    if (this.root.getAttribute("aria-label") !== label) this.root.setAttribute("aria-label", label);
    if (this.root.hidden !== (sessions.length === 0)) this.root.hidden = sessions.length === 0;
  }
}
