export interface PlatformCapabilities {
  os: string;
  experimental: boolean;
  hyprland: boolean;
  automatic_dnd: boolean;
  screen_off: boolean;
  fullscreen_detection: boolean;
  global_shortcut: boolean;
  manual_update: boolean;
}
export const linuxCapabilities: PlatformCapabilities = {
  os: "linux", experimental: false, hyprland: true, automatic_dnd: true,
  screen_off: true, fullscreen_detection: true, global_shortcut: false, manual_update: false,
};
export function supportsRow(capabilities: PlatformCapabilities, path: string): boolean {
  switch (path) {
    case "integrations.macos_terminal": return capabilities.os === "macos";
    case "integration.hyprland": return capabilities.hyprland;
    case "sound.follow_dnd":
    case "filters.quiet.focus_mode": return capabilities.automatic_dnd;
    case "filters.quiet.screen_off": return capabilities.screen_off;
    case "island.hide_in_fullscreen": return capabilities.fullscreen_detection;
    default: return true;
  }
}
