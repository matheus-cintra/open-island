import { expect, mock, test } from "bun:test";
import { mountSettings } from "./dom";
import { tauriMock } from "./tauri";

const ON_DISK = {
  display: {
    content_font: 20,
    completion_card_height: 320,
    panel_max_height: 1800,
    panel_max_width: 1200,
  },
  island: { hover_dwell_ms: 1500 },
  usage: { warn_threshold: 20 },
  notifications: { idle_reminder_after_ms: 300_000 },
};

const writes: Record<string, Record<string, unknown>>[] = [];

const tauri = tauriMock(({ command, args }) => {
  if (command === "get_config") return { config: structuredClone(ON_DISK), env_locked: {} };
  if (command === "island_metrics") return { scale: 1, compact_height: null };
  if (command === "save_config") {
    writes.push(structuredClone(args.config as Record<string, Record<string, unknown>>));
    return null;
  }
  return {};
});

mock.module("@tauri-apps/api/core", () => ({ invoke: tauri.invoke }));
mock.module("@tauri-apps/api/event", () => ({ listen: tauri.listen }));

mountSettings();
const settings = await import("../settings");
await new Promise((resolve) => setTimeout(resolve, 50));

const SLIDERS = [
  { path: "display.completion_card_height", min: 60, max: 240, step: 10, stored: 320 },
  { path: "display.panel_max_height", min: 320, max: 1200, step: 20, stored: 1800 },
  { path: "display.panel_max_width", min: 480, max: 1000, step: 8, stored: 1200 },
] as const;

const CONTENT_FONT = {
  kind: "picker",
  values: [9, 10, 11, 12, 13, 14, 15, 16],
  unit: "px",
  defaultValue: 11,
} as const;

const HOVER_DWELL = { kind: "size", min: 0, max: 800, step: 50, unit: "s", scale: 1000 } as const;

for (const { path, min, max, step, stored } of SLIDERS) {
  test(`${path}: the pill shows the ${stored} that is on disk, not the slider's edge`, () => {
    const element = settings.buildRow({
      path,
      label: path,
      control: { kind: "size", min, max, step, unit: "px" },
    });
    expect(element.querySelector(".value-pill")!.textContent).toBe(`${stored}px`);
  });

  test(`${path}: the knob sits on the ${stored} the pill claims`, () => {
    const element = settings.buildRow({
      path,
      label: path,
      control: { kind: "size", min, max, step, unit: "px" },
    });
    const slider = element.querySelector<HTMLInputElement>('input[type="range"]')!;
    expect(slider.value).toBe(String(stored));
  });
}

test("a value inside the range leaves the slider's own bounds alone", () => {
  const element = settings.buildRow({
    path: "display.panel_max_width",
    label: "largura",
    control: { kind: "size", min: 480, max: 1000, step: 8, unit: "px" },
  });
  const slider = element.querySelector<HTMLInputElement>('input[type="range"]')!;
  expect(slider.min).toBe("480");
  expect(Number(slider.max)).toBeGreaterThanOrEqual(1000);
});

test("the content font picker offers the 20 that is on disk, outside its own list", () => {
  const element = settings.buildRow({
    path: "display.content_font",
    label: "fonte",
    control: CONTENT_FONT,
  });
  const select = element.querySelector<HTMLSelectElement>("select.select")!;
  expect([...select.options].map((option) => option.value)).toEqual([
    "9",
    "10",
    "11",
    "12",
    "13",
    "14",
    "15",
    "16",
    "20",
  ]);
  expect(select.value).toBe("20");
});

test("the content font picker marks 11 as the default and writes a number", async () => {
  const element = settings.buildRow({
    path: "display.content_font",
    label: "fonte",
    control: CONTENT_FONT,
  });
  const select = element.querySelector<HTMLSelectElement>("select.select")!;
  expect([...select.options].find((option) => option.value === "11")!.textContent).toBe(
    "11px (Padrão)",
  );
  select.value = "13";
  select.dispatchEvent(new window.Event("change"));
  await settings.flushSave();
  expect(writes[writes.length - 1]!.display.content_font).toBe(13);
});

test("the hover slider reaches the 1500 on disk and reads it in seconds", () => {
  const element = settings.buildRow({
    path: "island.hover_dwell_ms",
    label: "hover",
    control: HOVER_DWELL,
  });
  const slider = element.querySelector<HTMLInputElement>('input[type="range"]')!;
  expect(slider.value).toBe("1500");
  expect(element.querySelector(".value-pill")!.textContent).toBe("1,5s");
});

test("the hover slider draws a 0 to 0,8 s range", () => {
  const element = settings.buildRow({
    path: "island.auto_collapse_ms",
    label: "hover",
    control: HOVER_DWELL,
  });
  const slider = element.querySelector<HTMLInputElement>('input[type="range"]')!;
  expect(slider.min).toBe("0");
  expect(slider.max).toBe("800");
  slider.value = "150";
  slider.dispatchEvent(new window.Event("input"));
  expect(element.querySelector(".value-pill")!.textContent).toBe("0,15s");
});

test("the percent field keeps the file's value across one stepper press", () => {
  const element = settings.buildRow({
    path: "usage.warn_threshold",
    label: "limiar",
    control: { kind: "percent", min: 50, max: 100, step: 5 },
  });
  const field = element.querySelector<HTMLInputElement>("input.field")!;
  expect(field.value).toBe("20");
  element.querySelectorAll<HTMLButtonElement>(".stepper button")[0]!.click();
  expect(field.value).toBe("25");
});

const REMINDER = {
  kind: "picker",
  values: [0, 600_000, 1_800_000, 3_600_000],
  unit: "",
  defaultValue: 300_000,
  label: (value: number): string => {
    if (value === 0) return "Desativado";
    const minutes = Math.round(value / 60_000);
    if (minutes % 60 === 0) {
      const hours = minutes / 60;
      return `Depois de ${hours === 1 ? "1 hora" : `${hours} horas`}`;
    }
    return `Depois de ${minutes} minutos`;
  },
} as const;

test("the reminder picker keeps the 5 minutes on disk beside the four preset stops", () => {
  const element = settings.buildRow({
    path: "notifications.idle_reminder_after_ms",
    label: "lembrar",
    control: REMINDER,
  });
  const select = element.querySelector<HTMLSelectElement>("select.select")!;
  expect([...select.options].map((option) => option.value)).toEqual([
    "0",
    "300000",
    "600000",
    "1800000",
    "3600000",
  ]);
  expect(select.value).toBe("300000");
});

test("the reminder picker labels zero as off and never as a duration", () => {
  const element = settings.buildRow({
    path: "notifications.idle_reminder_after_ms",
    label: "lembrar",
    control: REMINDER,
  });
  const select = element.querySelector<HTMLSelectElement>("select.select")!;
  const labels = [...select.options].map((option) => option.textContent);
  expect(labels[0]).toBe("Desativado");
  expect(labels).toContain("Depois de 10 minutos");
  expect(labels).toContain("Depois de 1 hora");
});

function includeSection(): HTMLElement {
  const pane = document.querySelector<HTMLElement>('[data-pane="filters"]')!;
  pane.click();
  return [...document.querySelectorAll<HTMLElement>(".section")].find(
    (section) => section.querySelector(".section-title")?.textContent === "Quando ativado, incluir",
  )!;
}

test("the reminder scopes wake at a real delay and dim the moment it reads off", async () => {
  expect(includeSection().classList.contains("is-muted")).toBe(false);

  const delay = [...document.querySelectorAll<HTMLSelectElement>("select.select")].find((select) =>
    [...select.options].some((option) => option.textContent === "Desativado"),
  )!;
  delay.value = "0";
  delay.dispatchEvent(new window.Event("change"));
  await new Promise((resolve) => setTimeout(resolve, 0));

  expect(includeSection().classList.contains("is-muted")).toBe(true);
});
