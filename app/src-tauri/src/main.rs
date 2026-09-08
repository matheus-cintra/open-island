// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    // WebKitGTK's DMABUF renderer crashes on transparent windows under some
    // GPU/Hyprland setups: "Error 71 (Protocol error) dispatching to Wayland
    // display" (native Wayland) or "Failed to create GBM buffer ... Invalid
    // argument" (X11). Disabling it falls back to the EGL compositing path, which
    // renders the island panel correctly. Must be set before WebKit initializes.
    std::env::set_var("WEBKIT_DISABLE_DMABUF_RENDERER", "1");
    open_island_lib::run()
}
