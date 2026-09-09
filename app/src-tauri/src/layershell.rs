use gtk::gdk::prelude::MonitorExt;
use gtk::glib::object::ObjectType;
use gtk::prelude::{Cast, GtkWindowExt, WidgetExt};
use std::os::raw::c_char;

const LAYER_OVERLAY: i32 = 3;
const EDGE_TOP: i32 = 2;
const KEYBOARD_NONE: i32 = 0;
const KEYBOARD_ON_DEMAND: i32 = 2;
const NAMESPACE: &[u8] = b"open-island\0";

#[link(name = "gtk-layer-shell")]
extern "C" {
    fn gtk_layer_is_supported() -> i32;
    fn gtk_layer_init_for_window(window: *mut gtk::ffi::GtkWindow);
    fn gtk_layer_is_layer_window(window: *mut gtk::ffi::GtkWindow) -> i32;
    fn gtk_layer_set_namespace(window: *mut gtk::ffi::GtkWindow, name_space: *const c_char);
    fn gtk_layer_set_layer(window: *mut gtk::ffi::GtkWindow, layer: i32);
    fn gtk_layer_set_anchor(window: *mut gtk::ffi::GtkWindow, edge: i32, anchor: i32);
    fn gtk_layer_set_exclusive_zone(window: *mut gtk::ffi::GtkWindow, zone: i32);
    fn gtk_layer_set_keyboard_mode(window: *mut gtk::ffi::GtkWindow, mode: i32);
    fn gtk_layer_set_monitor(
        window: *mut gtk::ffi::GtkWindow,
        monitor: *mut gtk::gdk::ffi::GdkMonitor,
    );
}

fn handle(window: &gtk::ApplicationWindow) -> *mut gtk::ffi::GtkWindow {
    window.upcast_ref::<gtk::Window>().as_ptr()
}

pub fn init(window: &gtk::ApplicationWindow) -> Result<(), String> {
    if unsafe { gtk_layer_is_supported() } == 0 {
        return Err("wlr-layer-shell is not available on this compositor".to_string());
    }
    let window = handle(window);
    unsafe {
        gtk_layer_init_for_window(window);
        gtk_layer_set_namespace(window, NAMESPACE.as_ptr().cast::<c_char>());
        gtk_layer_set_layer(window, LAYER_OVERLAY);
        gtk_layer_set_anchor(window, EDGE_TOP, 1);
        gtk_layer_set_exclusive_zone(window, -1);
        gtk_layer_set_keyboard_mode(window, KEYBOARD_NONE);
    }
    if unsafe { gtk_layer_is_layer_window(window) } == 0 {
        return Err("gtk_layer_init_for_window refused the window".to_string());
    }
    Ok(())
}

pub fn set_keyboard(window: &gtk::ApplicationWindow, interactive: bool) {
    let mode = if interactive {
        KEYBOARD_ON_DEMAND
    } else {
        KEYBOARD_NONE
    };
    unsafe { gtk_layer_set_keyboard_mode(handle(window), mode) }
}

pub struct MonitorTarget<'a> {
    pub name: &'a str,
    pub x: i32,
    pub y: i32,
}

fn monitor_for(target: &MonitorTarget<'_>) -> Option<gtk::gdk::Monitor> {
    let display = gtk::gdk::Display::default()?;
    let named = (0..display.n_monitors()).find_map(|index| {
        let monitor = display.monitor(index)?;
        (monitor.model().as_deref() == Some(target.name)).then_some(monitor)
    });
    if named.is_some() {
        return named;
    }
    (0..display.n_monitors()).find_map(|index| {
        let monitor = display.monitor(index)?;
        let rect = monitor.geometry();
        (rect.x() == target.x && rect.y() == target.y).then_some(monitor)
    })
}

pub fn set_monitor(window: &gtk::ApplicationWindow, target: Option<MonitorTarget<'_>>) -> bool {
    let handle = handle(window);
    let chosen = target.as_ref().and_then(monitor_for);
    match &chosen {
        Some(monitor) => unsafe { gtk_layer_set_monitor(handle, monitor.as_ptr()) },
        None => unsafe { gtk_layer_set_monitor(handle, std::ptr::null_mut()) },
    }
    chosen.is_some()
}

pub fn monitor_report() -> Vec<String> {
    let Some(display) = gtk::gdk::Display::default() else {
        return Vec::new();
    };
    (0..display.n_monitors())
        .filter_map(|index| {
            let monitor = display.monitor(index)?;
            let rect = monitor.geometry();
            Some(format!(
                "model={:?} geometry={}x{}+{}+{}",
                monitor.model(),
                rect.width(),
                rect.height(),
                rect.x(),
                rect.y()
            ))
        })
        .collect()
}

pub fn resize(window: &gtk::ApplicationWindow, width: i32, height: i32) {
    window.set_size_request(width, height);
    window.resize(1, 1);
}
