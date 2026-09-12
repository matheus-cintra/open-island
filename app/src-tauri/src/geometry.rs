use crate::compositor::{self, Compositor};
use crate::platform;
use std::sync::Mutex;
use std::time::{Duration, Instant};

pub const MONITOR_CACHE_TTL: Duration = Duration::from_millis(1000);
pub const COMPACT_WIDTH: f64 = 232.0;
pub const COMPACT_HEIGHT: f64 = 46.0;

#[derive(Clone, Copy)]
pub struct IslandRect {
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
}

static ISLAND_RECT: Mutex<IslandRect> = Mutex::new(IslandRect {
    x: 0,
    y: 0,
    width: 0,
    height: 0,
});

#[derive(Clone, Copy)]
struct MonitorBox {
    x: i32,
    y: i32,
    width: u32,
}

static MONITOR_BOX: Mutex<Option<(MonitorBox, Instant)>> = Mutex::new(None);
pub static SELECTED_MONITOR: Mutex<Option<String>> = Mutex::new(None);

pub fn selected_monitor() -> Option<String> {
    SELECTED_MONITOR
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .clone()
}

pub fn forget_monitor_box() {
    *MONITOR_BOX
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner()) = None;
}

pub fn remember_rect(rect: IslandRect) {
    let mut guard = ISLAND_RECT
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    *guard = rect;
}

pub fn island_rect() -> IslandRect {
    *ISLAND_RECT
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

pub fn pointer_is_inside(rect: IslandRect, x: i32, y: i32) -> bool {
    x >= rect.x && x < rect.x + rect.width && y >= rect.y && y < rect.y + rect.height
}

pub fn position_island(
    window: &tauri::WebviewWindow,
    width: f64,
    height: f64,
) -> Result<(), String> {
    #[cfg(target_os = "linux")]
    remember_origin(window, width, height);
    let target = window.clone();
    window
        .run_on_main_thread(move || {
            platform::resize(&target, width, height);
        })
        .map_err(|error| error.to_string())
}

pub fn remember_origin(window: &tauri::WebviewWindow, width: f64, height: f64) {
    if let Ok(Some((x, y))) = centered_origin(window, width) {
        remember_rect(IslandRect {
            x,
            y,
            width: width as i32,
            height: height as i32,
        });
    }
}

pub fn compact_size() -> (f64, f64) {
    let monitor = selected_monitor();
    let compositor = compositor::current();
    let scale = compositor.ui_scale(monitor.as_deref());
    let height = compositor
        .compact_height(monitor.as_deref())
        .map(f64::from)
        .unwrap_or((COMPACT_HEIGHT * scale).round());
    ((COMPACT_WIDTH * scale).round(), height)
}

fn monitor_box(window: &tauri::WebviewWindow) -> Result<Option<MonitorBox>, String> {
    let mut guard = MONITOR_BOX
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    if let Some((rect, stamp)) = *guard {
        if stamp.elapsed() < MONITOR_CACHE_TTL {
            return Ok(Some(rect));
        }
    }
    let rect = match compositor::current()
        .monitor_named(selected_monitor().as_deref())
        .map(|monitor| MonitorBox {
            x: monitor.x,
            y: monitor.y,
            width: monitor.width,
        }) {
        Some(rect) => Some(rect),
        None => window
            .available_monitors()
            .map_err(|e| e.to_string())?
            .first()
            .map(|monitor| MonitorBox {
                x: 0,
                y: 0,
                width: monitor.size().width,
            }),
    };
    *guard = rect.map(|value| (value, Instant::now()));
    Ok(rect)
}

pub fn centered_origin(
    window: &tauri::WebviewWindow,
    width: f64,
) -> Result<Option<(i32, i32)>, String> {
    Ok(monitor_box(window)?.map(|rect| (rect.x + (rect.width as i32 - width as i32) / 2, rect.y)))
}

#[cfg(test)]
mod tests {
    use super::{pointer_is_inside, IslandRect};

    fn sample_rect() -> IslandRect {
        IslandRect {
            x: 100,
            y: 50,
            width: 232,
            height: 46,
        }
    }

    #[test]
    fn the_top_left_corner_is_inside() {
        let rect = sample_rect();
        assert!(pointer_is_inside(rect, rect.x, rect.y));
    }

    #[test]
    fn the_last_pixel_before_the_far_edges_is_inside() {
        let rect = sample_rect();
        assert!(pointer_is_inside(
            rect,
            rect.x + rect.width - 1,
            rect.y + rect.height - 1
        ));
    }

    #[test]
    fn the_far_right_edge_is_outside() {
        let rect = sample_rect();
        assert!(!pointer_is_inside(rect, rect.x + rect.width, rect.y));
    }

    #[test]
    fn one_pixel_above_the_top_is_outside() {
        let rect = sample_rect();
        assert!(!pointer_is_inside(rect, rect.x, rect.y - 1));
    }
}
