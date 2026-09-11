import { expect, mock, test } from "bun:test";
import { mountSettings } from "./dom";
import { tauriMock } from "./tauri";

type ConfigDocument = Record<string, Record<string, unknown>>;

const INTEGRATIONS = {
  autostart: { detected: true, installed: true },
  hyprland: { detected: true, installed: false },
  claude: { detected: true, installed: true },
  codex: { detected: false, installed: false },
  opencode: { detected: true, installed: true },
};

let onDisk: ConfigDocument = {
  display: { compact_layout: "clean", monitor: "", ui_scale: 0 },
  island: { hover_dwell_ms: 250 },
  sound: { volume: 0.85, quiet_start: 0, event: null },
  usage: { warn_threshold: 90 },
  notifications: { idle_reminder_after_ms: 0 },
  filters: { rules: [], launchers: [{ app_id: "kitty", name: "kitty", enabled: true }] },
};

const tauri = tauriMock(({ command, args }) => {
  if (command === "get_config") return { config: structuredClone(onDisk), env_locked: {} };
  if (command === "save_config") {
    onDisk = structuredClone(args.config as ConfigDocument);
    return null;
  }
  if (command === "integration_status") return INTEGRATIONS;
  if (command === "sound_theme_files") return [];
  if (command === "island_metrics") return { scale: 1, compact_height: null };
  if (command === "user_sound_dir") return "";
  if (command === "list_monitors") return ["HDMI-A-1"];
  if (command === "app_version") return "v0.1.0";
  if (command === "list_sessions") {
    return [{ launcher: "kitty" }, { launcher: "foot" }, { launcher: "" }, { launcher: 7 }];
  }
  if (command === "get_update") return null;
  return {};
});

mock.module("@tauri-apps/api/core", () => ({ invoke: tauri.invoke }));
mock.module("@tauri-apps/api/event", () => ({ listen: tauri.listen }));

mountSettings();
const settings = await import("../settings");
await new Promise((resolve) => setTimeout(resolve, 50));

function rulesRow(): HTMLElement {
  return settings.buildRow({
    path: "filters.rules",
    label: "Diretório",
    control: { kind: "rules", field: "cwd" },
  });
}

function launchersRow(): HTMLElement {
  return settings.buildRow({
    path: "filters.launchers",
    label: "Apps",
    control: { kind: "launchers" },
  });
}

function storedFilters(key: string): Record<string, unknown>[] {
  return onDisk.filters[key] as Record<string, unknown>[];
}

test("asRule rejects anything that is not a plain JSON object", () => {
  expect(settings.asRule(null)).toBeNull();
  expect(settings.asRule(7)).toBeNull();
  expect(settings.asRule("filters.rules")).toBeNull();
  expect(settings.asRule([])).toBeNull();
});

test("asRule rejects a field that is neither cwd nor prompt", () => {
  expect(settings.asRule({ field: "title", match_type: "contains", pattern: "/srv" })).toBeNull();
  expect(settings.asRule({ match_type: "contains", pattern: "/srv" })).toBeNull();
});

test("asRule rejects a match type outside contains, prefix and equals", () => {
  expect(settings.asRule({ field: "cwd", match_type: "regex", pattern: "/srv" })).toBeNull();
  expect(settings.asRule({ field: "cwd", pattern: "/srv" })).toBeNull();
});

test("asRule rejects a pattern that is not a string", () => {
  expect(settings.asRule({ field: "cwd", match_type: "contains", pattern: 7 })).toBeNull();
  expect(settings.asRule({ field: "cwd", match_type: "contains", pattern: null })).toBeNull();
});

test("asRule rejects an empty pattern", () => {
  expect(settings.asRule({ field: "cwd", match_type: "contains", pattern: "" })).toBeNull();
});

test("asRule falls back to the pattern whenever the name is not a non-empty string", () => {
  const base = { field: "cwd", match_type: "contains", pattern: "/srv" };
  expect(settings.asRule(base)?.name).toBe("/srv");
  expect(settings.asRule({ ...base, name: "" })?.name).toBe("/srv");
  expect(settings.asRule({ ...base, name: 7 })?.name).toBe("/srv");
  expect(settings.asRule({ ...base, name: "Servidor" })?.name).toBe("Servidor");
});

test("asRule marks built_in only for a literal true", () => {
  const base = { field: "cwd", match_type: "contains", pattern: "/srv" };
  expect(settings.asRule(base)?.built_in).toBe(false);
  expect(settings.asRule({ ...base, built_in: true })?.built_in).toBe(true);
  expect(settings.asRule({ ...base, built_in: "true" })?.built_in).toBe(false);
  expect(settings.asRule({ ...base, built_in: 1 })?.built_in).toBe(false);
});

test("asRule keeps a rule enabled unless the stored value is a literal false", () => {
  const base = { field: "cwd", match_type: "contains", pattern: "/srv" };
  expect(settings.asRule(base)?.enabled).toBe(true);
  expect(settings.asRule({ ...base, enabled: false })?.enabled).toBe(false);
  expect(settings.asRule({ ...base, enabled: 0 })?.enabled).toBe(true);
  expect(settings.asRule({ ...base, enabled: "false" })?.enabled).toBe(true);
});

test("asJson round-trips an asRule output through the six stored keys", () => {
  const rule = settings.asRule({
    field: "prompt",
    match_type: "equals",
    pattern: "deploy",
    name: "Deploy",
    built_in: true,
    enabled: false,
  });
  expect(rule).not.toBeNull();
  const json = settings.asJson(rule!);
  expect(Object.keys(json)).toEqual([
    "field",
    "match_type",
    "pattern",
    "name",
    "built_in",
    "enabled",
  ]);
  expect(settings.asRule(json)).toEqual(rule);
});

test("storedRules skips every entry asRule rejects", () => {
  settings.setValue("filters.rules", [
    { field: "cwd", match_type: "contains", pattern: "/srv" },
    { field: "title", match_type: "contains", pattern: "/srv" },
    null,
    { field: "prompt", match_type: "prefix", pattern: "" },
    { field: "prompt", match_type: "prefix", pattern: "deploy" },
  ]);
  expect(settings.storedRules().map((rule) => rule.pattern)).toEqual(["/srv", "deploy"]);
});

test("storedRules returns an empty list when the stored value is not an array", () => {
  settings.setValue("filters.rules", "every rule");
  expect(settings.storedRules()).toEqual([]);
});

test("saveRules writes filters.rules through setValue in asJson's shape", async () => {
  settings.saveRules([
    {
      field: "cwd",
      match_type: "equals",
      pattern: "/srv/one",
      name: "Um",
      built_in: false,
      enabled: true,
    },
  ]);
  await settings.flushSave();
  const written = storedFilters("rules");
  expect(written.length).toBe(1);
  expect(Object.keys(written[0]!)).toEqual([
    "field",
    "match_type",
    "pattern",
    "name",
    "built_in",
    "enabled",
  ]);
  expect(written[0]!).toEqual({
    field: "cwd",
    match_type: "equals",
    pattern: "/srv/one",
    name: "Um",
    built_in: false,
    enabled: true,
  });
});

test("storedLaunchers skips every entry without a usable app id", () => {
  settings.setValue("filters.launchers", [
    null,
    7,
    [],
    { app_id: 7 },
    { app_id: "" },
    { app_id: "kitty" },
  ]);
  expect(settings.storedLaunchers().map((launcher) => launcher.app_id)).toEqual(["kitty"]);
});

test("storedLaunchers falls back to the app id and stays enabled unless a literal false", () => {
  settings.setValue("filters.launchers", [
    { app_id: "foot" },
    { app_id: "alacritty", name: "" },
    { app_id: "wezterm", name: "WezTerm", enabled: false },
    { app_id: "ghostty", enabled: 0 },
  ]);
  expect(settings.storedLaunchers()).toEqual([
    { app_id: "foot", name: "foot", enabled: true },
    { app_id: "alacritty", name: "alacritty", enabled: true },
    { app_id: "wezterm", name: "WezTerm", enabled: false },
    { app_id: "ghostty", name: "ghostty", enabled: true },
  ]);
});

test("saveLaunchers writes filters.launchers through setValue in its three-key shape", async () => {
  settings.saveLaunchers([{ app_id: "foot", name: "Foot", enabled: false }]);
  await settings.flushSave();
  const written = storedFilters("launchers");
  expect(written.length).toBe(1);
  expect(Object.keys(written[0]!)).toEqual(["app_id", "name", "enabled"]);
  expect(written[0]!).toEqual({ app_id: "foot", name: "Foot", enabled: false });
});

test("the launcher editor lists every observed launcher that is not blocked yet", () => {
  settings.saveLaunchers([{ app_id: "kitty", name: "kitty", enabled: true }]);
  const element = launchersRow();
  const list = element.querySelector("datalist#launcher-seen");
  expect(list).not.toBeNull();
  expect([...list!.querySelectorAll("option")].map((option) => option.value)).toEqual(["foot"]);
  expect(element.querySelector<HTMLInputElement>("input.rule-input")!.getAttribute("list")).toBe(
    "launcher-seen",
  );
});

test("the launcher editor drops the datalist once every observed launcher is blocked", () => {
  settings.saveLaunchers([
    { app_id: "kitty", name: "kitty", enabled: true },
    { app_id: "foot", name: "foot", enabled: true },
  ]);
  expect(launchersRow().querySelector("datalist#launcher-seen")).toBeNull();
});

test("adding a rule from the editor stores it and rerenders, and removing it goes back", () => {
  const first = settings.asRule({ field: "cwd", match_type: "contains", pattern: "/srv/one" });
  expect(first).not.toBeNull();
  settings.saveRules([first!]);

  const element = rulesRow();
  const add = element.querySelector<HTMLButtonElement>("button.rule-add")!;
  const pattern = element.querySelector<HTMLInputElement>("input.rule-input")!;
  expect(add.disabled).toBe(true);

  pattern.value = "/srv/two";
  pattern.dispatchEvent(new window.Event("input"));
  expect(add.disabled).toBe(false);

  add.click();
  expect(settings.storedRules().map((rule) => rule.pattern)).toEqual(["/srv/one", "/srv/two"]);

  const grown = rulesRow();
  expect([...grown.querySelectorAll(".row-label")].map((label) => label.textContent)).toEqual([
    "/srv/one",
    "/srv/two",
  ]);

  const removes = grown.querySelectorAll<HTMLButtonElement>("button.rule-remove");
  expect(removes.length).toBe(2);
  removes[1]!.click();
  expect(settings.storedRules().map((rule) => rule.pattern)).toEqual(["/srv/one"]);
  expect([...rulesRow().querySelectorAll(".row-label")].map((label) => label.textContent)).toEqual([
    "/srv/one",
  ]);
});

test("the add button refuses a pattern that is only whitespace", () => {
  settings.saveRules([]);
  const element = rulesRow();
  const add = element.querySelector<HTMLButtonElement>("button.rule-add")!;
  const pattern = element.querySelector<HTMLInputElement>("input.rule-input")!;
  pattern.value = "   ";
  pattern.dispatchEvent(new window.Event("input"));
  expect(add.disabled).toBe(true);
  add.click();
  expect(settings.storedRules()).toEqual([]);
});
