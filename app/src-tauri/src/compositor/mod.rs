#[cfg(target_os = "linux")]
pub mod hyprland;

pub const UI_SCALE_MIN: f64 = 1.0;
pub const UI_SCALE_MAX: f64 = 2.0;
const DPI_BASE: f64 = 96.0;
const DPI_SANE_MIN: f64 = 60.0;
const DPI_SANE_MAX: f64 = 250.0;
const MM_PER_INCH: f64 = 25.4;
const COMPACT_HEIGHT_MIN: u64 = 16;
const COMPACT_HEIGHT_MAX: u64 = 200;
const COMPACT_OVERHANG: u32 = 4;

pub fn ui_scale_from(width_px: u32, physical_width_mm: u32, compositor_scale: f64) -> f64 {
    if width_px == 0 || physical_width_mm == 0 || compositor_scale <= 0.0 {
        return UI_SCALE_MIN;
    }
    let dpi = f64::from(width_px) / (f64::from(physical_width_mm) / MM_PER_INCH);
    if !(DPI_SANE_MIN..=DPI_SANE_MAX).contains(&dpi) {
        return UI_SCALE_MIN;
    }
    let scale = dpi / DPI_BASE / compositor_scale;
    (scale.clamp(UI_SCALE_MIN, UI_SCALE_MAX) * 100.0).round() / 100.0
}

#[derive(Clone, Debug, PartialEq)]
pub struct MonitorInfo {
    pub name: String,
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub physical_width_mm: u32,
    pub scale: f64,
    pub reserved_top: u32,
    pub focused: bool,
}

pub trait Compositor {
    fn available(&self) -> bool;
    fn monitors(&self) -> Vec<MonitorInfo>;
    fn cursor_position(&self) -> Option<(i32, i32)>;
    fn any_fullscreen(&self) -> Option<bool>;
    fn focused_pid(&self) -> Option<u32>;
    fn window_class(&self, pid: u32) -> Option<String>;

    fn monitor_named(&self, name: Option<&str>) -> Option<MonitorInfo> {
        let list = self.monitors();
        if let Some(wanted) = name.filter(|value| !value.is_empty()) {
            if let Some(found) = list.iter().find(|entry| entry.name == wanted) {
                return Some(found.clone());
            }
        }
        list.iter()
            .find(|entry| entry.focused)
            .or_else(|| list.first())
            .cloned()
    }

    fn monitor_names(&self) -> Vec<String> {
        self.monitors()
            .into_iter()
            .map(|entry| entry.name)
            .collect()
    }

    fn reserved_top(&self, name: Option<&str>) -> Option<u32> {
        let top = self.monitor_named(name)?.reserved_top;
        (COMPACT_HEIGHT_MIN..=COMPACT_HEIGHT_MAX)
            .contains(&u64::from(top))
            .then_some(top)
    }

    fn compact_height(&self, name: Option<&str>) -> Option<u32> {
        self.reserved_top(name).map(|top| top + COMPACT_OVERHANG)
    }

    fn ui_scale(&self, name: Option<&str>) -> f64 {
        let Some(monitor) = self.monitor_named(name) else {
            return UI_SCALE_MIN;
        };
        ui_scale_from(monitor.width, monitor.physical_width_mm, monitor.scale)
    }
}

pub fn current() -> impl Compositor {
    #[cfg(target_os = "linux")]
    {
        hyprland::HyprlandBackend
    }
    #[cfg(target_os = "macos")]
    {
        crate::platform::macos::MacCompositor
    }
}

#[cfg(test)]
mod tests {
    use super::{ui_scale_from, Compositor, MonitorInfo, UI_SCALE_MAX, UI_SCALE_MIN};

    #[test]
    fn ui_scale_tracks_dpi_not_resolution() {
        let cases = [
            ("1080p 24in", 1920u32, 531u32, 1.0, 1.00),
            ("1440p 27in", 2560, 597, 1.0, 1.13),
            ("4k 32in", 3840, 700, 1.0, 1.45),
            ("4k 27in", 3840, 597, 1.0, 1.70),
            ("4k 55in tv", 3840, 1210, 1.0, 1.00),
        ];
        for (label, width, physical, compositor, expected) in cases {
            let scale = ui_scale_from(width, physical, compositor);
            assert!(
                (scale - expected).abs() < 0.01,
                "{label}: expected {expected}, got {scale}"
            );
        }
    }

    #[test]
    fn a_4k_tv_and_a_4k_monitor_disagree() {
        let tv = ui_scale_from(3840, 1210, 1.0);
        let monitor = ui_scale_from(3840, 700, 1.0);
        assert!(monitor > tv, "same resolution must not mean the same scale");
    }

    #[test]
    fn a_missing_edid_size_falls_back() {
        assert_eq!(ui_scale_from(3840, 0, 1.0), UI_SCALE_MIN);
        assert_eq!(ui_scale_from(0, 700, 1.0), UI_SCALE_MIN);
        assert_eq!(ui_scale_from(3840, 700, 0.0), UI_SCALE_MIN);
    }

    #[test]
    fn an_absurd_dpi_falls_back() {
        assert_eq!(ui_scale_from(3840, 4000, 1.0), UI_SCALE_MIN);
        assert_eq!(ui_scale_from(3840, 100, 1.0), UI_SCALE_MIN);
    }

    #[test]
    fn the_compositor_scale_is_divided_out() {
        assert_eq!(ui_scale_from(3840, 700, 1.5), UI_SCALE_MIN);
        assert!(ui_scale_from(3840, 700, 1.0) > ui_scale_from(3840, 700, 1.25));
    }

    #[test]
    fn the_scale_is_bounded() {
        let dense = ui_scale_from(3840, 400, 1.0);
        assert!(dense <= UI_SCALE_MAX);
    }

    struct FakeCompositor {
        monitors: Vec<MonitorInfo>,
    }

    impl Compositor for FakeCompositor {
        fn available(&self) -> bool {
            true
        }

        fn monitors(&self) -> Vec<MonitorInfo> {
            self.monitors.clone()
        }

        fn cursor_position(&self) -> Option<(i32, i32)> {
            None
        }

        fn any_fullscreen(&self) -> Option<bool> {
            None
        }

        fn focused_pid(&self) -> Option<u32> {
            None
        }

        fn window_class(&self, _pid: u32) -> Option<String> {
            None
        }
    }

    fn monitor(name: &str, focused: bool, reserved_top: u32) -> MonitorInfo {
        MonitorInfo {
            name: name.to_owned(),
            x: 0,
            y: 0,
            width: 1920,
            physical_width_mm: 300,
            scale: 1.0,
            reserved_top,
            focused,
        }
    }

    #[test]
    fn a_named_monitor_wins_over_the_focused_one() {
        let compositor = FakeCompositor {
            monitors: vec![monitor("DP-1", false, 32), monitor("eDP-1", true, 32)],
        };
        assert_eq!(
            compositor.monitor_named(Some("DP-1")),
            Some(monitor("DP-1", false, 32))
        );
    }

    #[test]
    fn an_unknown_or_empty_name_falls_back_to_the_focused_monitor() {
        let compositor = FakeCompositor {
            monitors: vec![monitor("DP-1", false, 32), monitor("eDP-1", true, 32)],
        };
        let focused = Some(monitor("eDP-1", true, 32));
        assert_eq!(compositor.monitor_named(Some("nope")), focused);
        assert_eq!(compositor.monitor_named(Some("")), focused);
    }

    #[test]
    fn with_nothing_focused_the_first_monitor_is_used() {
        let compositor = FakeCompositor {
            monitors: vec![monitor("DP-1", false, 32), monitor("eDP-1", false, 32)],
        };
        assert_eq!(
            compositor.monitor_named(None),
            Some(monitor("DP-1", false, 32))
        );
    }

    #[test]
    fn no_monitors_means_no_monitor() {
        let compositor = FakeCompositor { monitors: vec![] };
        assert_eq!(compositor.monitor_named(None), None);
        assert_eq!(compositor.ui_scale(None), UI_SCALE_MIN);
        assert_eq!(compositor.compact_height(None), None);
        assert!(compositor.monitor_names().is_empty());
    }

    #[test]
    fn the_compact_height_is_the_reserved_top_plus_the_overhang() {
        let compositor = FakeCompositor {
            monitors: vec![monitor("eDP-1", true, 32)],
        };
        assert_eq!(compositor.compact_height(None), Some(36));
    }

    #[test]
    fn a_reserved_top_outside_the_sane_range_is_ignored() {
        for reserved_top in [15u32, 201] {
            let compositor = FakeCompositor {
                monitors: vec![monitor("eDP-1", true, reserved_top)],
            };
            assert_eq!(
                compositor.reserved_top(None),
                None,
                "reserved {reserved_top}"
            );
            assert_eq!(
                compositor.compact_height(None),
                None,
                "reserved {reserved_top}"
            );
        }
        for reserved_top in [16u32, 200] {
            let compositor = FakeCompositor {
                monitors: vec![monitor("eDP-1", true, reserved_top)],
            };
            assert_eq!(
                compositor.reserved_top(None),
                Some(reserved_top),
                "reserved {reserved_top}"
            );
        }
    }

    #[test]
    fn monitor_names_keep_the_compositor_order() {
        let compositor = FakeCompositor {
            monitors: vec![
                monitor("DP-2", false, 32),
                monitor("eDP-1", true, 32),
                monitor("DP-1", false, 32),
            ],
        };
        assert_eq!(compositor.monitor_names(), vec!["DP-2", "eDP-1", "DP-1"]);
    }

    #[test]
    fn ui_scale_reads_the_selected_monitor() {
        let compositor = FakeCompositor {
            monitors: vec![monitor("eDP-1", true, 32)],
        };
        let scale = compositor.ui_scale(Some("eDP-1"));
        assert!((scale - 1.69).abs() < 0.01, "expected 1.69, got {scale}");
    }
}
