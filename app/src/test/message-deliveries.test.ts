import { expect, test } from "bun:test";
import { mountIsland } from "./dom";
import { DeliveryView } from "../message-deliveries";
import type { Delivery } from "../daemon-state";
import { strings } from "../strings";
import { detachedDeliveries, snapshotSessions } from "../island-state";
import type { UiSnapshot } from "../daemon-state";
mountIsland();
const record: Delivery = { message_id: 1, session_id: "session", text: "preservar", queued_at_ms: 1, state: "queued", identity: { daemon_epoch: "epoch", session_instance_id: "instance" } };

test("old-instance messages stay detached instead of appearing under a reused session", () => {
  const snapshot: UiSnapshot = { schema_version: 1, daemon_epoch: "epoch", publication_revision: 1,
    sessions: [{ id: "session", agent: "codex", cwd: "/fixture", title: "new", pid: 42, terminal: "kitty", session_instance_id: "new-instance" }],
    approvals: [], questions: [], message_deliveries: [record], config: {}, usage: { providers: [] }, update: null, quiet_scenes: { active: false, focus_mode: false, screen_off: false } };
  expect(snapshotSessions(snapshot)[0].message_deliveries).toEqual([]);
  expect(detachedDeliveries(snapshot)).toEqual([record]);
  snapshot.child_sessions = [{ ...snapshot.sessions[0], id: record.session_id, session_instance_id: "instance" }];
  expect(detachedDeliveries(snapshot)).toEqual([record]);
});

test("deliveries attach to the matching instance and only queued ones wait", () => {
  const waiting: Delivery = record;
  const settled: Delivery = { message_id: 2, session_id: "session", text: "ok", queued_at_ms: 2, state: "delivered", identity: record.identity };
  const orphan: Delivery = { message_id: 3, session_id: "gone", text: "órfã", queued_at_ms: 3, state: "queued", identity: record.identity };
  const snapshot: UiSnapshot = { schema_version: 1, daemon_epoch: "epoch", publication_revision: 1,
    sessions: [{ id: "session", agent: "codex", cwd: "/fixture", title: "sessão", pid: 42, terminal: "kitty", session_instance_id: "instance" }],
    approvals: [], questions: [], message_deliveries: [waiting, settled, orphan], config: {}, usage: { providers: [] }, update: null, quiet_scenes: { active: false, focus_mode: false, screen_off: false } };
  const session = snapshotSessions(snapshot)[0];
  expect(session.message_deliveries).toEqual([waiting, settled]);
  expect(session.queued_messages).toEqual([{ id: 1, text: "preservar", queued_at_ms: 1 }]);
  expect(detachedDeliveries(snapshot)).toEqual([orphan]);
});

test("all delivery states preserve nodes, prohibit sending cancellation and never retry", async () => {
  const root = document.createElement("ul"); document.body.append(root);
  const cancelled: Delivery[] = []; let copied = "";
  const view = new DeliveryView(root, { cancel: async (record) => { cancelled.push(record); }, copy: async (text) => { copied = text; }, error: () => {}, changed: () => {} });
  view.update([record], true);
  const item = root.firstElementChild!;
  const text = item.querySelector<HTMLTextAreaElement>("textarea")!;
  text.focus(); text.setSelectionRange(1, 4);
  const observer = new window.MutationObserver((): void => {});
  observer.observe(root, { subtree: true, attributes: true, childList: true, characterData: true });
  for (let i = 0; i < 100; i++) view.update([{ ...record }], true);
  expect(observer.takeRecords()).toHaveLength(0);
  observer.disconnect();
  expect(root.firstElementChild).toBe(item); expect(text.selectionStart).toBe(1); expect(text.selectionEnd).toBe(4);
  const buttons = item.querySelectorAll<HTMLButtonElement>("button");
  view.update([{ ...record, state: "sending" }], true);
  expect(buttons[1].disabled).toBe(true); buttons[1].click(); expect(cancelled).toHaveLength(0);
  view.update([{ ...record, state: "unconfirmed" }], true);
  expect(item.textContent).toContain(strings.session.messageUnconfirmed);
  buttons[0].click(); await Promise.resolve(); expect(copied).toBe(record.text);
  buttons[1].click(); await Promise.resolve(); expect(cancelled[0].identity).toEqual(record.identity);
  expect(cancelled[0].state).toBe("unconfirmed");
  view.update([{ ...record, state: "failed" }], false);
  expect(buttons[1].disabled).toBe(true); expect(text.value).toBe(record.text);
  view.dispose(); root.remove();
});

test("delivered confirmation expires once after five seconds and cannot erase another epoch", () => {
  const root = document.createElement("ul");
  const timers: (() => void)[] = []; const delays: number[] = []; const cleared: number[] = [];
  const view = new DeliveryView(root, { cancel: async () => {}, copy: async () => {}, error: () => {}, changed: () => {} }, {
    after: (callback, delay) => { timers.push(callback); delays.push(delay); return timers.length as unknown as ReturnType<typeof setTimeout>; },
    cancel: (timer) => { cleared.push(Number(timer)); },
  });
  const delivered = { ...record, state: "delivered" as const, text: "" };
  view.update([delivered], true);
  for (let i = 0; i < 100; i++) view.update([delivered], true);
  expect(delays).toEqual([5000]);
  expect(root.textContent).toContain(strings.session.deliveryConfirmed);
  timers[0](); expect(root.hidden).toBe(true);
  view.update([delivered], true); expect(root.children.length).toBe(0);
  const next = { ...record, identity: { ...record.identity!, daemon_epoch: "next" } };
  view.update([next], true); timers[0]();
  expect(root.querySelector("textarea")!.value).toBe(record.text);
  view.update([{ ...next, state: "delivered", text: "" }], true);
  view.dispose(); expect(cleared).toEqual([2]);
});

test("failed discard keeps text and clipboard failure leaves it selectable", async () => {
  const root = document.createElement("ul"); document.body.append(root);
  const errors: string[] = [];
  const view = new DeliveryView(root, { cancel: async () => { throw new Error("PRIVATE_PATH"); }, copy: async () => { throw new Error("clipboard"); }, error: (error) => errors.push(error), changed: () => {} });
  view.update([{ ...record, state: "failed" }], true);
  const buttons = root.querySelectorAll<HTMLButtonElement>("button");
  buttons[1].click(); buttons[0].click();
  await new Promise((resolve) => setTimeout(resolve, 0));
  expect(root.querySelector("textarea")!.value).toBe(record.text);
  expect(root.querySelector("textarea")!.selectionEnd).toBe(record.text.length);
  expect(errors).toContain(strings.session.deliveryDiscardFailed);
  expect(errors.join()).not.toContain("PRIVATE_PATH");
  view.dispose(); root.remove();
});
