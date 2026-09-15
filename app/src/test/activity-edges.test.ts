import { expect, test } from "bun:test";
import { CompletionEdges, SubagentEdges } from "../activity-edges";
import type { Session } from "../types";

function session(instance = "one", epoch = "daemon"): Session {
  return { id: "same-id", agent: "codex", cwd: "/fixture", title: "fixture", pid: 42, terminal: "kitty", attention: "working",
    action_identity: { daemon_epoch: epoch, session_instance_id: instance },
    subagents: [{ id: "child", kind: "explore", done: false }] };
}
test("activity caches track live identities and do not inherit history across removal, recycling or hydration", (): void => {
  const completion = new CompletionEdges(); const children = new SubagentEdges();
  for (let i = 0; i < 1000; i += 1) {
    const running = session(String(i)); const done = { ...running, attention: "needs_attention" as const, completion_id: `stop-${i}`, subagents: [{ id: "child", kind: "explore", done: true }] };
    expect(completion.observe([running])).toBe(false);
    expect(children.observe([running], "every_completion")).toBe(false);
    expect(completion.observe([done])).toBe(true);
    expect(children.observe([done], "every_completion")).toBe(true);
    expect(completion.retained).toBe(1); expect(children.retained).toBe(1);
  }
  completion.observe([]); children.observe([], "every_completion");
  expect(completion.retained).toBe(0); expect(children.retained).toBe(0);
  const done = { ...session("999"), attention: "needs_attention" as const, completion_id: "stop-999", subagents: [{ id: "child", kind: "explore", done: true }] };
  expect(completion.observe([done])).toBe(false); expect(children.observe([done], "every_completion")).toBe(false);
  completion.observe([session("999")]); children.observe([session("999")], "every_completion");
  expect(completion.observe([{ ...done, completion_id: "fresh" }], true)).toBe(false);
  expect(children.observe([done], "every_completion", true)).toBe(false);
  expect(completion.observe([{ ...done, action_identity: session("999", "new-daemon").action_identity }])).toBe(false);
});

test("one live session retains only its latest completion across working gaps and rejects duplicate tokens", (): void => {
  const edges = new CompletionEdges(); const running = session();
  edges.observe([running]);
  for (let i = 0; i < 1000; i += 1) {
    const done = { ...running, attention: "needs_attention" as const, completion_id: `opaque:${i}` };
    expect(edges.observe([done])).toBe(true);
    expect(edges.observe([done])).toBe(false);
    expect(edges.observe([running])).toBe(false);
    expect(edges.observe([done])).toBe(false);
  }
  expect(edges.retained).toBe(1);
  const early = { ...running, completion_id: "arrived-while-working" };
  expect(edges.observe([early])).toBe(false);
  expect(edges.observe([{ ...early, attention: "needs_attention" }])).toBe(false);
});
