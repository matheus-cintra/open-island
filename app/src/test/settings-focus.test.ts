import { expect, mock, test } from "bun:test";
import { mountIsland } from "./dom";
import { tauriMock } from "./tauri";
const tauri = tauriMock(() => ({}));
mock.module("@tauri-apps/api/core", () => ({ invoke: tauri.invoke }));
mountIsland({ reducedMotion: true });
const { focusPermissionSection } = await import("../settings-focus");

test("Focus permission is requested only after an explicit click", () => {
  const section = focusPermissionSection({ authorization: 0, silenced: null });
  expect(tauri.calls).toHaveLength(0);
  section.querySelector("button")!.click();
  expect(tauri.calls.map((call) => call.command)).toEqual(["request_focus_permission"]);
  expect(section.querySelector("button")!.disabled).toBe(true);
});

test("denied permission and an unshared status are explained without claiming DND is off", () => {
  const denied = focusPermissionSection({ authorization: 2, silenced: null });
  expect(denied.textContent).toContain("Acesso negado");
  expect(denied.querySelector("button")).toBeNull();
  const unknown = focusPermissionSection({ authorization: 3, silenced: null });
  expect(unknown.textContent).toContain("não foi compartilhado");
  const authorized = focusPermissionSection({ authorization: 3, silenced: false });
  expect(authorized.textContent).toContain("Autorizado.");
});
