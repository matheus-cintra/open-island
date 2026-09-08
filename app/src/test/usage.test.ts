import { expect, mock, test } from "bun:test";
import { mountIsland } from "./dom";
import { tauriMock } from "./tauri";

const tauri = tauriMock(({ command }) => {
  if (command === "get_config") return { config: {} };
  if (command === "island_metrics") return { scale: 1, compact_height: null };
  if (command === "list_sessions") return [];
  if (command === "get_usage") return { providers: [] };
  return {};
});

mock.module("@tauri-apps/api/core", () => ({ invoke: tauri.invoke }));
mock.module("@tauri-apps/api/event", () => ({ listen: tauri.listen }));

mountIsland({ reducedMotion: true });
await import("../main");
await new Promise((resolve) => setTimeout(resolve, 50));

const lane = document.getElementById("header-usage-model")!;

function codex(credits: { balance: number; unlimited: boolean } | undefined): void {
  tauri.emit("usage-updated", {
    providers: [
      {
        provider: "codex",
        detected: true,
        checked_at_ms: Date.now(),
        snapshot: {
          provider: "codex",
          windows: [{ key: "primary", label: "7d", percent: 7 }],
          models: [],
          reset_cards: [],
          credits,
          fetched_at_ms: Date.now(),
        },
      },
    ],
  });
}

function display(mode: string): void {
  tauri.emit("config-changed", { config: { usage: { codex_credit_display: mode } } });
}

function laneText(): string {
  return lane.textContent ?? "";
}

test("the balance the daemon already sends reaches the usage header", () => {
  display("credits");
  codex({ balance: 1250, unlimited: false });
  expect(laneText()).toBe("1.250 créditos");
});

test("the picker switches the balance to dollars at 25 credits to the unit", () => {
  display("dollars");
  codex({ balance: 1250, unlimited: false });
  expect(laneText()).toBe("US$50,00");
});

test("an unlimited account says so in either unit", () => {
  display("credits");
  codex({ balance: 0, unlimited: true });
  expect(laneText()).toBe("créditos ilimitados");
  display("dollars");
  codex({ balance: 0, unlimited: true });
  expect(laneText()).toBe("créditos ilimitados");
});

test("a zero balance draws nothing, and neither does a provider without credits", () => {
  display("credits");
  codex({ balance: 0, unlimited: false });
  expect(laneText()).toBe("");
  expect(lane.hidden).toBe(true);
  codex(undefined);
  expect(laneText()).toBe("");
  expect(lane.hidden).toBe(true);
});
