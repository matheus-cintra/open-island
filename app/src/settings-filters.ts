import { strings } from "./strings";
import { config, isJsonObject, readPath, setValue } from "./settings-config";
import { ICON_PLUS, ICON_TRASH } from "./settings-icons";
import {
  JsonObject,
  JsonValue,
  LauncherRule,
  MatchType,
  Row,
  RuleField,
  SilenceRule,
} from "./settings-types";
import { observedLaunchers, renderPane } from "./settings";

const copy = strings.settings;

const MATCH_LABELS: Record<MatchType, string> = {
  contains: copy.filters.matchContains,
  prefix: copy.filters.matchPrefix,
  equals: copy.filters.matchEquals,
};

export function asRule(value: JsonValue): SilenceRule | null {
  if (!isJsonObject(value)) return null;
  const { field, match_type, pattern, name, built_in, enabled } = value;
  if (field !== "cwd" && field !== "prompt") return null;
  if (match_type !== "contains" && match_type !== "prefix" && match_type !== "equals") {
    return null;
  }
  if (typeof pattern !== "string" || pattern === "") return null;
  return {
    field,
    match_type,
    pattern,
    name: typeof name === "string" && name !== "" ? name : pattern,
    built_in: built_in === true,
    enabled: enabled !== false,
  };
}

export function asJson(rule: SilenceRule): JsonObject {
  return {
    field: rule.field,
    match_type: rule.match_type,
    pattern: rule.pattern,
    name: rule.name,
    built_in: rule.built_in,
    enabled: rule.enabled,
  };
}

export function storedRules(): SilenceRule[] {
  const stored = readPath(config, "filters.rules");
  if (!Array.isArray(stored)) return [];
  const rules: SilenceRule[] = [];
  for (const entry of stored) {
    const rule = asRule(entry);
    if (rule !== null) rules.push(rule);
  }
  return rules;
}

export function saveRules(rules: SilenceRule[]): void {
  setValue("filters.rules", rules.map(asJson));
  renderPane();
}

export function buildRulesRow(row: Row, field: RuleField): HTMLElement {
  const element = document.createElement("div");
  element.className = "row-rules";

  const rules = storedRules();
  const mine = rules
    .map((rule, index) => ({ rule, index }))
    .filter((entry) => entry.rule.field === field);

  if (mine.length === 0) {
    const empty = document.createElement("p");
    empty.className = "rules-empty";
    empty.textContent = copy.filters.empty;
    element.append(empty);
  }

  for (const { rule, index } of mine) {
    element.append(buildRuleEntry(rule, index));
  }

  element.append(buildRuleEditor(row, field));
  return element;
}

function buildRuleEntry(rule: SilenceRule, index: number): HTMLElement {
  const entry = document.createElement("div");
  entry.className = "rule";

  const copyBlock = document.createElement("label");
  copyBlock.className = "row-copy";
  const title = document.createElement("span");
  title.className = "row-label";
  title.textContent = rule.name;
  copyBlock.append(title);
  if (rule.built_in) {
    const badge = document.createElement("span");
    badge.className = "rule-preset";
    badge.textContent = copy.filters.preset;
    title.append(" ", badge);
  }
  const hint = document.createElement("span");
  hint.className = "row-hint rule-pattern";
  hint.textContent = `${MATCH_LABELS[rule.match_type]} · ${rule.pattern}`;
  copyBlock.append(hint);
  entry.append(copyBlock);

  if (!rule.built_in) {
    const remove = document.createElement("button");
    remove.type = "button";
    remove.className = "rule-remove";
    remove.title = copy.filters.remove;
    remove.setAttribute("aria-label", `${copy.filters.remove}: ${rule.name}`);
    remove.innerHTML = ICON_TRASH;
    remove.addEventListener("click", () => {
      const next = storedRules();
      next.splice(index, 1);
      saveRules(next);
    });
    entry.append(remove);
  }

  const toggle = document.createElement("input");
  toggle.type = "checkbox";
  toggle.className = "switch";
  toggle.checked = rule.enabled;
  toggle.id = `rule-${index}`;
  copyBlock.htmlFor = toggle.id;
  toggle.addEventListener("change", () => {
    const next = storedRules();
    const target = next[index];
    if (target === undefined) return;
    target.enabled = toggle.checked;
    saveRules(next);
  });
  entry.append(toggle);
  return entry;
}

export function storedLaunchers(): LauncherRule[] {
  const stored = readPath(config, "filters.launchers");
  if (!Array.isArray(stored)) return [];
  const launchers: LauncherRule[] = [];
  for (const entry of stored) {
    if (entry === null || typeof entry !== "object" || Array.isArray(entry)) continue;
    const appId = entry.app_id;
    if (typeof appId !== "string" || appId === "") continue;
    const name = entry.name;
    launchers.push({
      app_id: appId,
      name: typeof name === "string" && name !== "" ? name : appId,
      enabled: entry.enabled !== false,
    });
  }
  return launchers;
}

export function saveLaunchers(launchers: LauncherRule[]): void {
  setValue(
    "filters.launchers",
    launchers.map((launcher) => ({
      app_id: launcher.app_id,
      name: launcher.name,
      enabled: launcher.enabled,
    })),
  );
  renderPane();
}

export function buildLaunchersRow(row: Row): HTMLElement {
  const element = document.createElement("div");
  element.className = "row-rules";
  const launchers = storedLaunchers();

  if (launchers.length === 0) {
    const empty = document.createElement("p");
    empty.className = "rules-empty";
    empty.textContent = copy.filters.launchersEmpty;
    element.append(empty);
  }

  launchers.forEach((launcher, index) => {
    element.append(buildLauncherEntry(launcher, index));
  });

  element.append(buildLauncherEditor(row));
  return element;
}

function buildLauncherEntry(launcher: LauncherRule, index: number): HTMLElement {
  const entry = document.createElement("div");
  entry.className = "rule";

  const copyBlock = document.createElement("label");
  copyBlock.className = "row-copy";
  const title = document.createElement("span");
  title.className = "row-label";
  title.textContent = launcher.name;
  copyBlock.append(title);
  const hint = document.createElement("span");
  hint.className = "row-hint rule-pattern";
  hint.textContent = launcher.app_id;
  copyBlock.append(hint);
  entry.append(copyBlock);

  const remove = document.createElement("button");
  remove.type = "button";
  remove.className = "rule-remove";
  remove.title = copy.filters.remove;
  remove.setAttribute("aria-label", `${copy.filters.remove}: ${launcher.name}`);
  remove.innerHTML = ICON_TRASH;
  remove.addEventListener("click", () => {
    const next = storedLaunchers();
    next.splice(index, 1);
    saveLaunchers(next);
  });
  entry.append(remove);

  const toggle = document.createElement("input");
  toggle.type = "checkbox";
  toggle.className = "switch";
  toggle.checked = launcher.enabled;
  toggle.id = `launcher-${index}`;
  copyBlock.htmlFor = toggle.id;
  toggle.addEventListener("change", () => {
    const next = storedLaunchers();
    const target = next[index];
    if (target === undefined) return;
    target.enabled = toggle.checked;
    saveLaunchers(next);
  });
  entry.append(toggle);
  return entry;
}

function buildLauncherEditor(row: Row): HTMLElement {
  const editor = document.createElement("div");
  editor.className = "rule-editor";

  const input = document.createElement("input");
  input.type = "text";
  input.className = "field rule-input";
  input.placeholder = copy.filters.launcherPlaceholder;
  input.setAttribute("aria-label", row.label);

  const blocked = new Set(storedLaunchers().map((launcher) => launcher.app_id));
  const seen = observedLaunchers.filter((app_id) => !blocked.has(app_id));
  if (seen.length > 0) {
    const list = document.createElement("datalist");
    list.id = "launcher-seen";
    for (const app_id of seen) {
      const option = document.createElement("option");
      option.value = app_id;
      option.label = copy.filters.launcherSeen;
      list.append(option);
    }
    editor.append(list);
    input.setAttribute("list", list.id);
  }

  const add = document.createElement("button");
  add.type = "button";
  add.className = "rule-add";
  add.innerHTML = ICON_PLUS;
  add.append(document.createTextNode(copy.filters.add));
  add.disabled = true;

  const commit = (): void => {
    const text = input.value.trim();
    if (text === "") return;
    const next = storedLaunchers();
    if (next.some((launcher) => launcher.app_id === text)) return;
    next.push({ app_id: text, name: text, enabled: true });
    saveLaunchers(next);
  };

  input.addEventListener("input", () => {
    add.disabled = input.value.trim() === "";
  });
  input.addEventListener("keydown", (keyEvent) => {
    if (keyEvent.key === "Enter") {
      keyEvent.preventDefault();
      commit();
    }
  });
  add.addEventListener("click", commit);

  editor.append(input, add);
  return editor;
}

function buildRuleEditor(row: Row, field: RuleField): HTMLElement {
  const editor = document.createElement("div");
  editor.className = "rule-editor";

  const match = document.createElement("select");
  match.className = "select";
  match.setAttribute(
    "aria-label",
    field === "cwd" ? copy.filters.directory : copy.filters.prompt,
  );
  for (const value of ["contains", "prefix", "equals"] as const) {
    const option = document.createElement("option");
    option.value = value;
    option.textContent = MATCH_LABELS[value];
    match.append(option);
  }
  match.value = field === "cwd" ? "contains" : "prefix";

  const pattern = document.createElement("input");
  pattern.type = "text";
  pattern.className = "field rule-input";
  pattern.placeholder =
    field === "cwd" ? copy.filters.patternCwd : copy.filters.patternPrompt;
  pattern.setAttribute("aria-label", row.label);

  const add = document.createElement("button");
  add.type = "button";
  add.className = "rule-add";
  add.innerHTML = ICON_PLUS;
  add.append(document.createTextNode(copy.filters.add));
  add.disabled = true;

  const commit = (): void => {
    const text = pattern.value.trim();
    if (text === "") return;
    const next = storedRules();
    next.push({
      field,
      match_type: match.value as MatchType,
      pattern: text,
      name: text,
      built_in: false,
      enabled: true,
    });
    saveRules(next);
  };

  pattern.addEventListener("input", () => {
    add.disabled = pattern.value.trim() === "";
  });
  pattern.addEventListener("keydown", (keyEvent) => {
    if (keyEvent.key === "Enter") {
      keyEvent.preventDefault();
      commit();
    }
  });
  add.addEventListener("click", commit);

  editor.append(match, pattern, add);
  return editor;
}
