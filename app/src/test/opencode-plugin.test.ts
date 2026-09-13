import { afterEach, expect, spyOn, test } from "bun:test";

const source = await Bun.file(new URL("../../crates/open-islandd/templates/open-island-opencode.ts.template", import.meta.url)).text();
const js = new Bun.Transpiler({ loader: "ts" }).transformSync(source.replace("__OPEN_ISLANDD_PATH__", '"/fake/daemon"'));
const { OpenIslandPlugin } = await import(`data:text/javascript;base64,${Buffer.from(js).toString("base64")}`);
let restore: (() => void) | undefined;
afterEach(() => restore?.());

function harness(output = "") {
  const payloads: any[] = [];
  const spawn = spyOn(Bun, "spawn").mockImplementation((() => ({
    stdin: { write: (input: string) => payloads.push(JSON.parse(input)), end() {} },
    stdout: new Blob([output]).stream(), stderr: new Blob([]).stream(), exited: Promise.resolve(0),
  })) as any);
  restore = () => spawn.mockRestore();
  return payloads;
}

test("real info events and concurrent children share ancestor lookup and preserve native reply IDs", async () => {
  const payloads = harness('{"reply":"once"}');
  const queries: string[] = [];
  const replies: unknown[] = [];
  const plugin = await OpenIslandPlugin({
    directory: "/work", serverUrl: new URL("http://localhost"),
    client: {
      session: { get: async ({ path }: any) => {
        queries.push(path.id);
        await new Promise((resolve) => setTimeout(resolve, 5));
        return { data: path.id === "root" ? { id: "root", title: "Main" }
          : { id: path.id, title: path.id, parentID: "root" } };
      }},
      permission: { reply: async (request: unknown) => replies.push(request) },
    },
  });
  await Promise.all(["a", "b"].map((id) => plugin.event({ event: {
    type: "permission.asked", properties: { sessionID: id, id: `request-${id}`, permission: "bash" },
  }})));
  expect(queries.filter((id) => id === "root")).toHaveLength(1);
  expect(payloads).toHaveLength(2);
  expect(payloads[0].session_metadata.map((entry: any) => entry.id)).toEqual(["a", "root"]);
  expect(replies).toContainEqual({ sessionID: "a", requestID: "request-a", reply: "once" });
  expect(replies).toContainEqual({ sessionID: "b", requestID: "request-b", reply: "once" });
  await plugin.event({ event: { type: "session.updated", properties: { info: { id: "a", parentID: "root", title: "Renamed" } } } });
  expect(payloads[2].session_metadata[0].title).toBe("Renamed");
  expect(queries).toHaveLength(3);
});

test("failed metadata lookup forwards questions, retries later, and cache belongs to each plugin", async () => {
  const payloads = harness();
  let attempts = 0;
  const context = {
    directory: "/work", serverUrl: new URL("http://localhost"),
    client: { session: { get: async () => {
      if (++attempts === 1) throw new Error("offline");
      return { data: { id: "a", title: "Recovered" } };
    }}},
  };
  const event = { event: { type: "question.asked", properties: { sessionID: "a", id: "q", questions: [] } } };
  const plugin = await OpenIslandPlugin(context);
  await plugin.event(event);
  expect(payloads[0].session_metadata).toEqual([]);
  await plugin.event(event);
  expect(payloads[1].session_metadata[0].parent_id).toBeNull();
  await plugin.event(event);
  expect(attempts).toBe(2);
  const other = await OpenIslandPlugin(context);
  await other.event(event);
  expect(attempts).toBe(3);
});

test("ancestry traversal stops at cycles and user prompts normalize part session IDs", async () => {
  const payloads = harness();
  const plugin = await OpenIslandPlugin({
    directory: "/work", serverUrl: new URL("http://localhost"),
    client: { session: { get: async ({ path }: any) => ({ data: {
      id: path.id, parentID: path.id === "a" ? "b" : "a", title: path.id,
    }})}},
  });
  await plugin.event({ event: { type: "message.updated", properties: { info: { id: "m", role: "user" } } } });
  const event = { event: { type: "message.part.updated", properties: { part: {
    type: "text", messageID: "m", sessionID: "a", text: "Hello",
  }}}};
  await plugin.event(event);
  await plugin.event(event);
  expect(payloads).toHaveLength(1);
  expect(payloads[0].type).toBe("open-island.prompt");
  expect(payloads[0].properties.sessionID).toBe("a");
  expect(payloads[0].session_metadata).toHaveLength(2);
});

test("the installed v1 SDK receives the original child and permission IDs", async () => {
  harness('{"reply":"always"}');
  let reply: unknown;
  const plugin = await OpenIslandPlugin({
    directory: "/work", serverUrl: new URL("http://localhost"),
    client: {
      session: { get: async ({ path }: any) => ({ data: { id: path.id, ...(path.id === "child" ? { parentID: "root" } : {}) } }) },
      postSessionIdPermissionsPermissionId: async (input: unknown) => { reply = input; },
    },
  });
  await plugin.event({ event: { type: "permission.asked", properties: {
    sessionID: "child", id: "p-original", permission: "bash",
  }}});
  expect(reply).toEqual({
    path: { id: "child", permissionID: "p-original" }, query: { directory: "/work" },
    body: { response: "always" },
  });
});

test("a hanging ancestry request cannot hold up a permission hook", async () => {
  const payloads = harness();
  const plugin = await OpenIslandPlugin({
    directory: "/work", serverUrl: new URL("http://localhost"),
    client: { session: { get: ({ signal }: any) => new Promise((_, reject) => {
      signal.addEventListener("abort", () => reject(new Error("timeout")));
    })}},
  });
  const before = Date.now();
  await plugin.event({ event: { type: "permission.asked", properties: {
    sessionID: "child", id: "p", permission: "bash",
  }}});
  expect(Date.now() - before).toBeLessThan(1800);
  expect(payloads[0].properties.id).toBe("p");
  expect(payloads[0].session_metadata).toEqual([]);
});
