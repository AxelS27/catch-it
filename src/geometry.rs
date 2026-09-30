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
