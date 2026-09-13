import { expect, test } from "bun:test";
import { ActivityController, type Clock } from "../activity-controller";

class ControlledClock implements Clock {
  now = 0;
  private next = 1;
  private timers = new Map<number, { at: number; callback: () => void }>();
  setTimeout(callback: () => void, delay: number): number {
    const id = this.next++;
    this.timers.set(id, { at: this.now + delay, callback });
    return id;
  }
  clearTimeout(id: number): void { this.timers.delete(id); }
  advance(ms: number): void {
    this.now += ms;
    for (;;) {
      const due = [...this.timers.entries()]
        .filter(([, timer]) => timer.at <= this.now)
        .sort(([, left], [, right]) => left.at - right.at)[0];
      if (!due) return;
      this.timers.delete(due[0]);
      due[1].callback();
    }
  }
}

function controller() {
  const clock = new ControlledClock();
  let expanded = false;
  let editing = false;
  let collapseOnLeave = false;
  let opens = 0;
  let collapses = 0;
  const activity = new ActivityController({
    clock,
    open: () => { expanded = true; opens += 1; },
    collapse: () => { expanded = false; collapses += 1; },
    isExpanded: () => expanded,
    isEditing: () => editing,
    expandOnHover: () => true,
    collapseOnLeave: () => collapseOnLeave,
    dwellMs: () => 250,
    autoCollapseMs: () => 2500,
  });
  return { clock, activity, openManual: () => { expanded = true; activity.manualOpened(); }, forceCollapse: () => { expanded = false; activity.collapsed(); }, get expanded() { return expanded; }, set editing(value: boolean) { editing = value; }, set collapseOnLeave(value: boolean) { collapseOnLeave = value; }, get opens() { return opens; }, get collapses() { return collapses; } };
}

test("a relevant event opens once and each new event renews its single deadline", () => {
  const state = controller();
  state.activity.relevantEvent();
  state.clock.advance(2400);
  state.activity.relevantEvent();
  state.clock.advance(2400);
  expect(state.expanded).toBe(true);
  state.clock.advance(100);
  expect(state.expanded).toBe(false);
  expect(state.opens).toBe(1);
});

test("pending-like editing never changes the timer, while real interaction pauses it", () => {
  const state = controller();
  state.activity.relevantEvent();
  state.activity.nativePointer({ inside: true, x: 100, y: 50 });
  state.activity.nativePointer({ inside: true, x: 101, y: 50 });
  state.clock.advance(3000);
  expect(state.expanded).toBe(true);
  state.activity.nativePointer({ inside: false, x: 102, y: 50 });
  state.clock.advance(2500);
  expect(state.expanded).toBe(false);
});

test("collapse on leave is immediate unless an editor is active", () => {
  const state = controller();
  state.collapseOnLeave = true;
  state.activity.relevantEvent();
  state.activity.nativePointer({ inside: true, x: 100, y: 50 });
  state.editing = true;
  state.activity.nativePointer({ inside: false, x: 101, y: 50 });
  expect(state.expanded).toBe(true);
  state.editing = false;
  state.activity.editingEnded();
  state.clock.advance(2500);
  expect(state.expanded).toBe(false);
});

test("a stationary pointer entering by geometry does not pause automatic collapse", () => {
  const state = controller();
  state.activity.relevantEvent();
  state.activity.nativePointer({ inside: true, x: 100, y: 50 });
  state.clock.advance(2500);
  expect(state.expanded).toBe(false);
});

test("manual opening is never converted into an automatic deadline", () => {
  const state = controller();
  state.openManual();
  state.activity.relevantEvent();
  state.clock.advance(3000);
  expect(state.collapses).toBe(0);
});

test("a shortcut collapse does not leave a hold that prevents the next automatic collapse", () => {
  const state = controller();
  state.openManual();
  state.forceCollapse();
  state.activity.relevantEvent();
  state.clock.advance(2500);
  expect(state.expanded).toBe(false);
});

test("focused interactive controls pause automatic collapse and blur gets a full deadline", () => {
  const state = controller();
  state.activity.relevantEvent();
  state.activity.editorFocused();
  state.clock.advance(3000);
  expect(state.expanded).toBe(true);
  state.activity.editingEnded();
  state.clock.advance(2500);
  expect(state.expanded).toBe(false);
});
