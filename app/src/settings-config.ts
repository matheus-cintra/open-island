import { invoke } from "@tauri-apps/api/core";
import { strings } from "./strings";
import { showToast } from "./settings-toast";
import { ConfigPayload, JsonObject, JsonValue } from "./settings-types";

const copy = strings.settings;

const SAVE_DEBOUNCE_MS = 250;

export let config: JsonObject = {};

export let envLocked: Record<string, string> = {};

let saveTimer = 0;
export let savePending = false;
const dirtyPaths = new Set<string>();

export function isJsonObject(value: JsonValue | undefined): value is JsonObject {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

export function readPath(root: JsonObject, path: string): JsonValue | undefined {
  const parts = path.split(".");
  let cursor: JsonValue | undefined = root;
  for (const part of parts) {
    if (!isJsonObject(cursor)) return undefined;
    cursor = cursor[part];
  }
  return cursor;
}

function writePath(root: JsonObject, path: string, value: JsonValue): void {
  const parts = path.split(".");
  const last = parts.pop();
  if (last === undefined) return;
  let cursor: JsonObject = root;
  for (const part of parts) {
    const next = cursor[part];
    if (!isJsonObject(next)) return;
    cursor = next;
  }
  cursor[last] = value;
}

export function setValue(path: string, value: JsonValue): void {
  writePath(config, path, value);
  dirtyPaths.add(path);
  queueSave();
}

export async function flushSave(): Promise<void> {
  const payload = await invoke<ConfigPayload>("get_config");
  for (const path of dirtyPaths) {
    const value = readPath(config, path);
    if (value !== undefined) writePath(payload.config, path, value);
  }
  config = payload.config;
  envLocked = payload.env_locked;
  await invoke("save_config", { config });
  dirtyPaths.clear();
}

function queueSave(): void {
  clearTimeout(saveTimer);
  savePending = true;
  saveTimer = window.setTimeout(() => {
    void flushSave()
      .catch((error: unknown) => {
        showToast(copy.saveFailed(String(error)));
      })
      .finally(() => {
        savePending = false;
      });
  }, SAVE_DEBOUNCE_MS);
}

export async function loadConfig(): Promise<boolean> {
  const payload = await invoke<ConfigPayload>("get_config");
  const changed = JSON.stringify(payload.config) !== JSON.stringify(config);
  config = payload.config;
  envLocked = payload.env_locked;
  return changed;
}
