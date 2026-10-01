//! Selection coordinates are physical pixels, independent of Windows display scaling.

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Point {
    pub x: i32,
    pub y: i32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Region {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}

impl Point {
    /// Mouse coordinates cannot reach the exclusive right/bottom monitor edge.
    /// Snap the last addressable pixel there so corner-to-corner selects every pixel.
    pub fn at_monitor_edge(self, width: u32, height: u32) -> Self {
        Self {
            x: if i64::from(self.x) == i64::from(width) - 1 {
                width as i32
            } else {
                self.x
            },
            y: if i64::from(self.y) == i64::from(height) - 1 {
                height as i32
            } else {
                self.y
            },
        }
    }
}

impl Region {
    pub fn between(a: Point, b: Point, width: u32, height: u32) -> Option<Self> {
        let clamp_x = |x: i32| i64::from(x).clamp(0, i64::from(width)) as u32;
        let clamp_y = |y: i32| i64::from(y).clamp(0, i64::from(height)) as u32;
        let left = clamp_x(a.x.min(b.x));
        let top = clamp_y(a.y.min(b.y));
        let right = clamp_x(a.x.max(b.x));
        let bottom = clamp_y(a.y.max(b.y));
        (right > left && bottom > top).then_some(Self {
            x: left,
            y: top,
            width: right - left,
            height: bottom - top,
        })
    }
}

/// Centered source rectangle for cover/fill. Fractional pixels preserve the exact
/// image aspect ratio, even for tiny or extreme-aspect screenshots.
#[derive(Clone, Copy, Debug)]
pub struct ImageCrop {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

pub fn aspect_fill(
    source_width: u32,
    source_height: u32,
    width: u32,
    height: u32,
) -> Option<ImageCrop> {
    if source_width == 0 || source_height == 0 || width == 0 || height == 0 {
        return None;
    }
    let scale = (f64::from(width) / f64::from(source_width))
        .max(f64::from(height) / f64::from(source_height));
    let crop_width = (f64::from(width) / scale).min(f64::from(source_width));
    let crop_height = (f64::from(height) / scale).min(f64::from(source_height));
    Some(ImageCrop {
        x: ((f64::from(source_width) - crop_width) / 2.0) as f32,
        y: ((f64::from(source_height) - crop_height) / 2.0) as f32,
        width: crop_width as f32,
        height: crop_height as f32,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn supports_every_drag_direction() {
        let expected = Some(Region {
            x: 10,
            y: 20,
            width: 90,
            height: 60,
        });
        for (a, b) in [
            (Point { x: 10, y: 20 }, Point { x: 100, y: 80 }),
            (Point { x: 100, y: 80 }, Point { x: 10, y: 20 }),
            (Point { x: 100, y: 20 }, Point { x: 10, y: 80 }),
            (Point { x: 10, y: 80 }, Point { x: 100, y: 20 }),
        ] {
            assert_eq!(Region::between(a, b, 1920, 1080), expected);
        }
    }

    #[test]
    fn clamps_pointer_outside_monitor() {
        assert_eq!(
            Region::between(
                Point { x: -20, y: -10 },
                Point { x: 2100, y: 1200 },
                1920,
                1080
            ),
            Some(Region {
                x: 0,
                y: 0,
                width: 1920,
                height: 1080
            })
        );
    }

    #[test]
    fn corner_to_corner_includes_every_monitor_pixel_in_both_directions() {
        let edge = Point { x: 1919, y: 1079 }.at_monitor_edge(1920, 1080);
        let origin = Point { x: 0, y: 0 };
        let expected = Some(Region {
            x: 0,
            y: 0,
            width: 1920,
            height: 1080,
        });
        assert_eq!(Region::between(origin, edge, 1920, 1080), expected);
        assert_eq!(Region::between(edge, origin, 1920, 1080), expected);
        assert_eq!(
            Point { x: 100, y: 200 }.at_monitor_edge(1920, 1080),
            Point { x: 100, y: 200 }
        );
    }

    #[test]
    fn aspect_fill_crops_center_without_stretching_or_letterboxing() {
        for (w, h) in [
            (350, 200),
            (200, 350),
            (100, 100),
            (1, 10000),
            (10000, 1),
            (10, 10),
        ] {
            let crop = aspect_fill(w, h, 220, 160).unwrap();
            assert!(crop.x >= 0.0 && crop.y >= 0.0);
            assert!(crop.x + crop.width <= w as f32 + 0.001);
            assert!(crop.y + crop.height <= h as f32 + 0.001);
            assert!((crop.width / crop.height - 220.0 / 160.0).abs() < 0.0001);
            assert!((crop.x + crop.width / 2.0 - w as f32 / 2.0).abs() < 0.001);
            assert!((crop.y + crop.height / 2.0 - h as f32 / 2.0).abs() < 0.001);
        }
        let portrait = aspect_fill(200, 350, 220, 160).unwrap();
        assert!(portrait.x.abs() < 0.001);
        assert!((portrait.y - 102.27273).abs() < 0.001);
        assert_eq!(aspect_fill(0, 10, 220, 160).map(|_| ()), None);
        assert_eq!(aspect_fill(10, 10, 0, 160).map(|_| ()), None);
    }

    #[test]
    fn rejects_zero_area() {
        assert_eq!(
            Region::between(Point { x: 5, y: 5 }, Point { x: 5, y: 30 }, 100, 100),
            None
        );
        assert_eq!(
            Region::between(Point { x: 5, y: 5 }, Point { x: 30, y: 5 }, 100, 100),
            None
        );
    }
}
