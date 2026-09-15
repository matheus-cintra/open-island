import { expect, test } from "bun:test";
import { mountIsland } from "./dom";
import { RecoveryView, type RecoveryRecord } from "../message-recovery";
import { strings } from "../strings";
mountIsland({ reducedMotion: true });
const record: RecoveryRecord = { client_submission_id: "submission-1", origin_epoch: "old", session_instance_id: "old-session", text: "texto completo\nsegunda linha", last_state: "unconfirmed", previous_state: "sending", message_id: 1, local: true };

test("missing current-epoch delivery remains copyable and is distinguished from restart", async (): Promise<void> => {
  const element = document.createElement("section");
  document.body.append(element);
  const copied: string[] = [];
  const view = new RecoveryView(element, {
    copy: async (text): Promise<void> => { copied.push(text); },
    discard: async (): Promise<boolean> => true,
    error: (): never => { throw new Error("unexpected error"); }, changed: (): void => {},
  });
  view.update([{ ...record, previous_epoch: false }]);
  expect(element.hidden).toBe(false);
  expect(element.textContent).toContain(strings.recovery.missingRecord);
  expect(element.textContent).not.toContain(strings.recovery.previousConnection);
  element.querySelector("button")!.click();
  await Promise.resolve();
  expect(copied).toEqual([record.text]);
  view.update([{ ...record, previous_epoch: true }]);
  expect(element.textContent).toContain(strings.recovery.previousConnection);
  expect(element.querySelector("textarea")!.value).toBe(record.text);
});

test("recovery survives repeated snapshots, preserves selection and never sends", async (): Promise<void> => {
  const element = document.createElement("section");
  document.body.append(element);
  const copied: string[] = [];
  const discarded: string[] = [];
  const view = new RecoveryView(element, {
    copy: async (text): Promise<void> => { copied.push(text); },
    discard: async (id): Promise<boolean> => { discarded.push(id); return true; },
    error: (): never => { throw new Error("unexpected error"); }, changed: (): void => {},
  });
  view.update([record]);
  const input = element.querySelector("textarea")!;
  input.focus();
  input.setSelectionRange(2, 8);
  for (let index = 0; index < 100; index += 1) view.update([{ ...record }]);
  expect(element.querySelector("textarea")).toBe(input);
  expect(input.selectionStart).toBe(2);
  expect(input.selectionEnd).toBe(8);
  element.querySelectorAll("button")[0]?.click();
  await Promise.resolve();
  expect(copied).toEqual([record.text]);
  element.querySelectorAll("button")[1]?.click();
  await Promise.resolve();
  expect(discarded).toEqual([record.client_submission_id]);
  view.update([record]);
  expect(element.hidden).toBe(true);
  expect(element.querySelector("textarea")).toBeNull();
});

test("failed discard retains text and failed clipboard offers manual selection", async (): Promise<void> => {
  const element = document.createElement("section");
  document.body.append(element);
  const errors: string[] = [];
  const view = new RecoveryView(element, { copy: async (): Promise<void> => { throw new Error("clipboard"); }, discard: async (): Promise<boolean> => { throw new Error("daemon"); }, error: (message): void => { errors.push(message); }, changed: (): void => {} });
  view.update([record]);
  element.querySelectorAll("button")[0]?.click();
  await Promise.resolve();
  await Promise.resolve();
  const input = element.querySelector("textarea")!;
  expect(input.selectionStart).toBe(0);
  expect(input.selectionEnd).toBe(record.text.length);
  element.querySelectorAll("button")[1]?.click();
  await Promise.resolve();
  await Promise.resolve();
  expect(input.value).toBe(record.text);
  expect(element.hidden).toBe(false);
  expect(errors.length).toBe(2);
});
