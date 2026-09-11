import { expect, test } from "bun:test";
import { linuxCapabilities, supportsRow } from "../platform-capabilities";
test("macOS hides unsupported automatic probes and Hyprland controls", () => {
  const mac = { ...linuxCapabilities, os: "macos", hyprland: false, automatic_dnd: false, screen_off: false, fullscreen_detection: false };
  for (const path of ["integration.hyprland", "sound.follow_dnd", "filters.quiet.focus_mode", "filters.quiet.screen_off", "island.hide_in_fullscreen", "display.notch_width_offset"]) {
    expect(supportsRow(mac, path)).toBe(false);
    expect(supportsRow(linuxCapabilities, path)).toBe(true);
  }
  expect(supportsRow(mac, "sound.quiet_hours")).toBe(true);
  expect(supportsRow(mac, "integration.autostart")).toBe(true);
});
