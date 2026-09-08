import { expect, mock, test } from "bun:test";
import { mountSettings } from "./dom";
import { tauriMock } from "./tauri";
import type { Control } from "../settings";

const SOUND_FILES = ["/usr/share/sounds/freedesktop/stereo/complete.oga"];
const INTEGRATIONS = {
  autostart: { detected: true, installed: true },
  hyprland: { detected: true, installed: false },
  claude: { detected: true, installed: true },
  codex: { detected: false, installed: false },
  opencode: { detected: true, installed: true },
};

const tauri = tauriMock(({ command }) => {
  if (command === "get_config") {
    return {
      config: {
        display: { compact_layout: "clean", monitor: "", ui_scale: 0 },
        island: { hover_dwell_ms: 250 },
        sound: { volume: 0.85, quiet_start: 0, event: null },
        usage: { warn_threshold: 90 },
        filters: { rules: [], launchers: [] },
        notifications: { idle_reminder_after_ms: 0 },
      },
      env_locked: {},
    };
  }
  if (command === "integration_status") return INTEGRATIONS;
  if (command === "sound_theme_files") return SOUND_FILES;
  if (command === "island_metrics") return { scale: 1.45, compact_height: 42 };
  if (command === "user_sound_dir") return "";
  if (command === "list_monitors") return ["HDMI-A-1"];
  return {};
});

mock.module("@tauri-apps/api/core", () => ({ invoke: tauri.invoke }));
mock.module("@tauri-apps/api/event", () => ({ listen: tauri.listen }));

mountSettings();
const settings = await import("../settings");
await new Promise((resolve) => setTimeout(resolve, 50));

type Case<K extends Control["kind"]> = {
  path: string;
  control: Extract<Control, { kind: K }>;
  check: (element: HTMLElement) => void;
};

type Cases = { [K in Control["kind"]]: Case<K> };

function scaleSelect(element: HTMLElement): HTMLSelectElement | null {
  const select = element.querySelector<HTMLSelectElement>("select.select");
  if (select === null) return null;
  const first = select.options[0];
  if (first === undefined) return null;
  return first.value === "0" && (first.textContent ?? "").startsWith("Automática")
    ? select
    : null;
}

const CASES: Cases = {
  switch: {
    path: "display.project",
    control: { kind: "switch" },
    check: (element) => {
      expect(element.querySelector("input.switch")!.getAttribute("type")).toBe("checkbox");
      expect(element.querySelector(".pip")).toBeNull();
    },
  },
  integration: {
    path: "system.autostart",
    control: { kind: "integration", name: "autostart" },
    check: (element) => {
      expect(element.querySelector("input.switch")).not.toBeNull();
      expect(element.querySelector(".pip")!.className).toBe("pip");
    },
  },
  duration: {
    path: "island.hover_dwell_ms",
    control: { kind: "duration", unit: "ms", min: 0, max: 2000, step: 50 },
    check: (element) => {
      expect(element.querySelector("input.field")!.getAttribute("type")).toBe("number");
      expect(element.querySelector(".field-unit")!.textContent).toBe("ms");
      expect(element.querySelector(".stepper")).not.toBeNull();
    },
  },
  percent: {
    path: "usage.warn_threshold",
    control: { kind: "percent", min: 50, max: 100, step: 5 },
    check: (element) => {
      expect(element.querySelector("input.field")!.getAttribute("type")).toBe("number");
      expect(element.querySelector(".field-unit")!.textContent).toBe("%");
    },
  },
  options: {
    path: "display.monitor",
    control: { kind: "options", choices: [["", "Automático"], ["HDMI-A-1", "HDMI-A-1"]] },
    check: (element) => {
      const select = element.querySelector<HTMLSelectElement>("select.select")!;
      expect([...select.options].map((option) => option.value)).toEqual(["", "HDMI-A-1"]);
    },
  },
  picker: {
    path: "display.content_font",
    control: { kind: "picker", values: [9, 10, 11], unit: "px", defaultValue: 11 },
    check: (element) => {
      const select = element.querySelector<HTMLSelectElement>("select.select")!;
      expect([...select.options].map((option) => option.textContent)).toEqual([
        "9px",
        "10px",
        "11px (Padrão)",
      ]);
    },
  },
  sound: {
    path: "sound.event",
    control: { kind: "sound" },
    check: (element) => {
      const select = element.querySelector<HTMLSelectElement>("select.select")!;
      expect([...select.options].map((option) => option.value)).toEqual(["", ...SOUND_FILES]);
      expect(element.querySelector(".row-control button")).not.toBeNull();
    },
  },
  time: {
    path: "sound.quiet_start",
    control: { kind: "time" },
    check: (element) => {
      expect(element.querySelector("input")!.getAttribute("type")).toBe("time");
    },
  },
  volume: {
    path: "sound.volume",
    control: { kind: "volume" },
    check: (element) => {
      expect(element.classList.contains("row-slider")).toBe(true);
      expect(element.querySelector('input[type="range"]')).not.toBeNull();
    },
  },
  size: {
    path: "display.panel_max_height",
    control: { kind: "size", min: 320, max: 1200, step: 20, unit: "px" },
    check: (element) => {
      expect(element.classList.contains("row-slider")).toBe(true);
      expect(element.querySelector<HTMLInputElement>('input[type="range"]')!.max).toBe("1200");
    },
  },
  rules: {
    path: "filters.rules",
    control: { kind: "rules", field: "cwd" },
    check: (element) => {
      expect(element.classList.contains("row-rules")).toBe(true);
      expect(element.querySelector(".rules-empty")).not.toBeNull();
    },
  },
  launchers: {
    path: "filters.launchers",
    control: { kind: "launchers" },
    check: (element) => {
      expect(element.classList.contains("row-rules")).toBe(true);
      expect(element.querySelector(".rules-empty")).not.toBeNull();
      expect(element.querySelector("select.select")).toBeNull();
    },
  },
  tiles: {
    path: "display.compact_layout",
    control: {
      kind: "tiles",
      choices: [
        ["full", "Completo", "sprite, projeto e contagem"],
        ["clean", "Limpo", "só a contagem"],
      ],
    },
    check: (element) => {
      expect(element.classList.contains("row-tiles")).toBe(true);
      const tiles = [...element.querySelectorAll(".tile-card")];
      expect(tiles.length).toBe(2);
      expect(tiles.map((tile) => tile.getAttribute("aria-checked"))).toEqual(["false", "true"]);
    },
  },
  value: {
    path: "about.acknowledgements",
    control: { kind: "value", text: () => "Departure Mono, Tauri, GTK" },
    check: (element) => {
      expect(element.querySelector(".row-value")!.textContent).toBe("Departure Mono, Tauri, GTK");
      expect(element.querySelector(".value-pill")).toBeNull();
      expect(element.querySelector("button")).toBeNull();
    },
  },
  action: {
    path: "about.remove",
    control: { kind: "action", name: "removeAutoConfig", text: "Remover", tone: "danger", confirm: "Confirmar" },
    check: (element) => {
      const button = element.querySelector<HTMLButtonElement>("button.row-button")!;
      expect(button.className).toBe("row-button is-danger");
      expect(button.textContent).toBe("Remover");
      button.click();
      expect(button.textContent).toBe("Confirmar");
      expect(button.classList.contains("is-armed")).toBe(true);
    },
  },
  "row-action": {
    path: "about.quit",
    control: { kind: "row-action", name: "quit", icon: "<svg></svg>", confirm: "Tocar de novo" },
    check: (element) => {
      expect(element.tagName).toBe("BUTTON");
      expect(element.className).toBe("row row-danger");
      expect(element.querySelector(".row-glyph")).not.toBeNull();
      expect(element.querySelector(".row-label")!.textContent).toBe("Linha row-action");
      expect(element.querySelector(".row-control")).toBeNull();
    },
  },
  scale: {
    path: "display.ui_scale",
    control: { kind: "scale" },
    check: (element) => {
      const select = scaleSelect(element)!;
      expect(select.options[0]!.textContent).toBe("Automática (1,45×)");
    },
  },
};

for (const [kind, entry] of Object.entries(CASES) as [Control["kind"], Case<Control["kind"]>][]) {
  test(`buildRow renders the ${kind} control and never falls through to the scale select`, () => {
    const element = settings.buildRow({
      path: entry.path,
      label: `Linha ${kind}`,
      control: entry.control,
    });
    expect(scaleSelect(element) !== null).toBe(kind === "scale");
    entry.check(element);
  });
}

test("an integration the machine does not have is locked and reads Não detectado", () => {
  const element = settings.buildRow({
    path: "integrations.codex",
    label: "Codex CLI",
    control: { kind: "integration", name: "codex" },
  });
  expect(element.classList.contains("is-locked")).toBe(true);
  expect(element.querySelector<HTMLInputElement>("input.switch")!.disabled).toBe(true);
  expect(element.querySelector(".pip")!.className).toBe("pip is-off");
  expect(element.querySelector(".pip")!.textContent).toBe("Não detectado");
});

test("an integration that is detected and off keeps its switch usable and shows no pip", () => {
  const element = settings.buildRow({
    path: "integrations.hyprland",
    label: "Hyprland",
    control: { kind: "integration", name: "hyprland" },
  });
  expect(element.querySelector<HTMLInputElement>("input.switch")!.disabled).toBe(false);
  expect(element.querySelector(".pip")).toBeNull();
});

test("a destructive row needs two presses before it calls anything", () => {
  const before = tauri.calls.length;
  const element = settings.buildRow({
    path: "about.remove",
    label: "Remover Toda a Configuração Automática",
    control: {
      kind: "row-action",
      name: "removeAutoConfig",
      icon: "<svg></svg>",
      confirm: "Tocar de novo para remover",
    },
  }) as HTMLButtonElement;
  const label = element.querySelector(".row-label")!;
  element.click();
  expect(tauri.calls.length).toBe(before);
  expect(label.textContent).toBe("Tocar de novo para remover");
  element.click();
  expect(tauri.calls.map((call) => call.command)).toContain("remove_auto_configuration");
  expect(label.textContent).toBe("Remover Toda a Configuração Automática");
});

test("leaving the pane disarms a destructive row instead of leaving it hot", () => {
  const element = settings.buildRow({
    path: "about.quit",
    label: "Sair do Open Island",
    control: { kind: "row-action", name: "quit", icon: "<svg></svg>", confirm: "Tocar de novo" },
  }) as HTMLButtonElement;
  element.click();
  expect(element.querySelector(".row-label")!.textContent).toBe("Tocar de novo");
  const fresh = settings.buildRow({
    path: "about.quit",
    label: "Sair do Open Island",
    control: { kind: "row-action", name: "quit", icon: "<svg></svg>", confirm: "Tocar de novo" },
  });
  expect(fresh.querySelector(".row-label")!.textContent).toBe("Sair do Open Island");
});
