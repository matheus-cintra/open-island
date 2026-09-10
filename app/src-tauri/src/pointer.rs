use crate::compositor::Compositor;
use crate::geometry::{island_rect, pointer_is_inside};
use std::time::Duration;
use tauri::Emitter;

pub const POINTER_TICK: Duration = Duration::from_millis(100);

#[derive(Clone, serde::Serialize)]
pub struct PointerState {
    inside: bool,
}

#[derive(Clone, serde::Serialize)]
pub struct FullscreenState {
    fullscreen: bool,
}

#[derive(Clone, serde::Serialize)]
pub struct FocusState {
    pid: Option<u32>,
}

pub fn watch_pointer(window: tauri::WebviewWindow, compositor: impl Compositor) {
    if !compositor.available() {
        return;
    }
    let mut last: Option<bool> = None;
    let mut last_fullscreen: Option<bool> = None;
    let mut last_focus: Option<Option<u32>> = None;
    loop {
        std::thread::sleep(POINTER_TICK);
        let focus = compositor.focused_pid();
        if last_focus != Some(focus) {
            last_focus = Some(focus);
            if window
                .emit("island-focus", FocusState { pid: focus })
                .is_err()
            {
                return;
            }
        }
        if let Some(fullscreen) = compositor.any_fullscreen() {
            if last_fullscreen != Some(fullscreen) {
                last_fullscreen = Some(fullscreen);
                if window
                    .emit("island-fullscreen", FullscreenState { fullscreen })
                    .is_err()
                {
                    return;
                }
            }
        }
        let rect = island_rect();
        if rect.width == 0 || rect.height == 0 {
            continue;
        }
        let Some((x, y)) = compositor.cursor_position() else {
            continue;
        };
        let inside = pointer_is_inside(rect, x, y);
        if last == Some(inside) {
            continue;
        }
        last = Some(inside);
        if window
            .emit("island-pointer", PointerState { inside })
            .is_err()
        {
            return;
        }
    }
}
