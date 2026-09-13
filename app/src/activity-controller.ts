export interface Clock {
  setTimeout(callback: () => void, delay: number): number;
  clearTimeout(id: number): void;
}

export interface ActivityControllerOptions {
  clock: Clock;
  open: () => void;
  collapse: () => void;
  isExpanded: () => boolean;
  isEditing: () => boolean;
  expandOnHover: () => boolean;
  collapseOnLeave: () => boolean;
  dwellMs: () => number;
  autoCollapseMs: () => number;
}

/** Separates automatic activity, manual modes, and independent user interactions. */
export class ActivityController {
  private hovered = false;
  private nativeCoordinates = false;
  private lastNative: { x: number; y: number } | null = null;
  private lastDom: { x: number; y: number } | null = null;
  private automatic = false;
  private pointerHold = false;
  private pressHold = false;
  private focusHold = false;
  private dwellTimer = 0;
  private collapseTimer = 0;

  constructor(private readonly options: ActivityControllerOptions) {}

  domEntered(): void { if (!this.nativeCoordinates) this.enter(); }
  domLeft(): void { if (!this.nativeCoordinates) this.leave(); }

  domMoved(x: number, y: number): void {
    if (this.nativeCoordinates) return;
    const moved = this.lastDom !== null && (this.lastDom.x !== x || this.lastDom.y !== y);
    this.lastDom = { x, y };
    if (moved) this.pointerActivity();
  }

  nativePointer(payload: { inside: boolean; x?: number; y?: number }): void {
    if (Number.isFinite(payload.x) && Number.isFinite(payload.y)) {
      this.nativeCoordinates = true;
      const next = { x: payload.x!, y: payload.y! };
      const moved = this.lastNative !== null &&
        (this.lastNative.x !== next.x || this.lastNative.y !== next.y);
      this.lastNative = next;
      if (payload.inside && moved) this.pointerActivity();
    }
    if (payload.inside) this.enter();
    else this.leave();
  }

  relevantEvent(): void {
    if (!this.options.isExpanded()) {
      this.automatic = true;
      this.options.open();
    }
    if (!this.automatic) return;
    if (this.interacting()) this.clearCollapse();
    else this.restartCollapse();
  }

  manualOpened(): void {
    this.automatic = false;
    this.pointerHold = false;
    this.pressHold = false;
    this.focusHold = false;
    this.clearCollapse();
  }

  interaction(): void {
    this.pressHold = true;
    this.clearCollapse();
  }

  interactionEnded(): void {
    this.pressHold = false;
    this.resumeAfterInteraction();
  }

  editorFocused(): void {
    this.focusHold = true;
    this.clearCollapse();
  }

  editingEnded(): void {
    this.focusHold = false;
    this.resumeAfterInteraction();
  }

  editorRemoved(): void {
    if (!this.focusHold) return;
    this.editingEnded();
  }

  collapsed(): void {
    this.options.clock.clearTimeout(this.dwellTimer);
    this.clearCollapse();
    this.automatic = false;
    this.pointerHold = false;
    this.pressHold = false;
    this.focusHold = false;
  }

  private enter(): void {
    if (this.hovered) return;
    this.hovered = true;
    this.options.clock.clearTimeout(this.dwellTimer);
    if (!this.options.expandOnHover()) return;
    this.dwellTimer = this.options.clock.setTimeout(() => {
      if (!this.options.isExpanded()) {
        this.automatic = false;
        this.options.open();
      }
    }, this.options.dwellMs());
  }

  private leave(): void {
    if (!this.hovered) return;
    this.hovered = false;
    this.pointerHold = false;
    this.pressHold = false;
    this.options.clock.clearTimeout(this.dwellTimer);
    if (this.options.collapseOnLeave() && this.options.isExpanded() && !this.options.isEditing()) {
      this.clearCollapse();
      this.options.collapse();
      return;
    }
    if (this.automatic) this.resumeAfterInteraction();
  }

  private restartCollapse(): void {
    this.clearCollapse();
    this.collapseTimer = this.options.clock.setTimeout(() => {
      if (!this.interacting() && !this.options.isEditing() && this.options.isExpanded()) {
        this.options.collapse();
      }
    }, this.options.autoCollapseMs());
  }

  private clearCollapse(): void {
    this.options.clock.clearTimeout(this.collapseTimer);
    this.collapseTimer = 0;
  }

  private pointerActivity(): void {
    this.pointerHold = true;
    this.clearCollapse();
  }

  private interacting(): boolean {
    return this.pointerHold || this.pressHold || this.focusHold;
  }

  private resumeAfterInteraction(): void {
    if (this.automatic && !this.interacting() && !this.options.isEditing()) this.restartCollapse();
  }
}
