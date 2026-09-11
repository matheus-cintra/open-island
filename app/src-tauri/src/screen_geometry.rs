//! Logical point coordinates: Retina scale must not be applied a second time.
#[derive(Debug, PartialEq)]
pub struct Placement {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}
pub fn place(
    x: f64,
    y: f64,
    screen_width: f64,
    notch_width: f64,
    _safe_top: f64,
    width: f64,
    height: f64,
) -> Placement {
    let width = width.max(notch_width).min(screen_width);
    Placement {
        x: x + (screen_width - width) / 2.0,
        y,
        width,
        // The frontend controls manual height; the camera remains excluded horizontally.
        height,
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn notch_is_inside_the_panel_and_negative_monitors_stay_centered() {
        let p = place(-1728.0, -900.0, 1728.0, 220.0, 32.0, 476.0, 36.0);
        assert_eq!(
            p,
            Placement {
                x: -1102.0,
                y: -900.0,
                width: 476.0,
                height: 36.0
            }
        );
        let expanded = place(-1728.0, -900.0, 1728.0, 220.0, 32.0, 664.0, 300.0);
        assert_eq!(expanded.y, p.y);
        assert_eq!(expanded.height, 300.0);
    }
    #[test]
    fn no_notch_uses_the_visible_top_without_extra_padding() {
        assert_eq!(
            place(0.0, 24.0, 1920.0, 0.0, 0.0, 232.0, 46.0),
            Placement {
                x: 844.0,
                y: 24.0,
                width: 232.0,
                height: 46.0
            }
        );
    }
    #[test]
    fn manual_height_can_be_smaller_than_the_camera_strip() {
        assert_eq!(
            place(0.0, 0.0, 1512.0, 220.0, 40.0, 332.0, 28.0).height,
            28.0
        );
    }
    #[test]
    fn retina_coordinates_are_already_points() {
        let p = place(0.0, 0.0, 1512.0, 180.0, 32.0, 232.0, 46.0);
        assert_eq!(p.x, 640.0);
        assert_eq!(p.width, 232.0);
    }
}
