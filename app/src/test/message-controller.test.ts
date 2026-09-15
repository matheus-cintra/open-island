import { expect, test } from "bun:test";
import { MessageComposer } from "../message-controller";

test("late failure preserves a newer draft separately and never resends", async () => {
  let reject: (error: Error) => void = (): void => {};
  let calls = 0;
  const composer = new MessageComposer(() => {
    calls += 1;
    return new Promise((_resolve, fail) => { reject = fail; });
  }, () => {}, () => {});
  composer.edit("original");
  const pending = composer.submit();
  await composer.submit();
  expect(calls).toBe(1);
  composer.edit("new draft");
  reject(new Error("timeout"));
  await pending;
  expect(composer.state().text).toBe("new draft");
  expect(composer.state().recovered[0]?.text).toBe("original");
  expect(composer.state().pending).toBe(false);
  expect(calls).toBe(1);
});

test("admission clears only the submitted revision and a refusal retains it", async () => {
  let accept: () => void = (): void => {};
  const composer = new MessageComposer(() => new Promise<void>((resolve) => { accept = resolve; }), () => {}, () => {});
  composer.edit("first");
  const pending = composer.submit();
  expect(composer.state().text).toBe("first");
  composer.edit("second");
  accept();
  await pending;
  expect(composer.state().text).toBe("second");
  const next = composer.submit();
  accept();
  await next;
  expect(composer.state().text).toBe("");
  const refused = new MessageComposer(async () => { throw new Error("queue_full"); }, () => {}, () => {});
  refused.edit("kept");
  await refused.submit();
  expect(refused.state().text).toBe("kept");
});
