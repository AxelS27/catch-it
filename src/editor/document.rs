//! Editable image-space annotations. The source screenshot is never modified.
use super::layout::Control;
use crate::storage::Raster;
use anyhow::{Result, ensure};

const MAX_MARKS: usize = 512;
const MAX_POINTS: usize = 4096;
const HISTORY_LIMIT: usize = 96;

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Point {
    pub x: f32,
    pub y: f32,
}
impl Point {
    pub fn moved(self, dx: f32, dy: f32) -> Self {
        Self {
            x: self.x + dx,
            y: self.y + dy,
        }
    }
}
#[derive(Clone, Debug, PartialEq)]
pub enum Shape {
    Pencil(Vec<Point>),
    Rectangle(Point, Point),
    FilledRectangle(Point, Point),
    Highlighter(Point, Point),
    Mosaic(Point, Point),
    Ellipse(Point, Point),
    Line(Point, Point),
    Arrow(Point, Point, Point),
}
#[derive(Clone, Debug, PartialEq)]
pub struct Mark {
    pub shape: Shape,
    pub color: u32,
    pub width: f32,
    pub opacity: f32,
}
impl Mark {
    pub fn from_tool(tool: Control, start: Point, color: u32, width: f32) -> Option<Self> {
        let shape = match tool {
            Control::Pencil => Shape::Pencil(vec![start]),
            Control::Rectangle => Shape::Rectangle(start, start),
            Control::Fill => Shape::FilledRectangle(start, start),
            Control::Highlighter => Shape::Highlighter(start, start),
            Control::Pixelate => Shape::Mosaic(start, start),
            Control::Ellipse => Shape::Ellipse(start, start),
            Control::Line => Shape::Line(start, start),
            Control::Arrow => Shape::Arrow(start, start, start),
            _ => return None,
        };
        Some(Self {
            shape,
            color,
            width,
            opacity: if tool == Control::Highlighter {
                0.35
            } else {
                1.0
            },
        })
    }
    pub fn update(&mut self, point: Point) {
        match &mut self.shape {
            Shape::Pencil(points) => {
                if points.len() < MAX_POINTS
                    && points.last().is_some_and(|last| {
                        (last.x - point.x).powi(2) + (last.y - point.y).powi(2) >= 0.25
                    })
                {
                    points.push(point);
                }
            }
            Shape::Rectangle(_, end)
            | Shape::FilledRectangle(_, end)
            | Shape::Highlighter(_, end)
            | Shape::Mosaic(_, end)
            | Shape::Ellipse(_, end)
            | Shape::Line(_, end) => *end = point,
            Shape::Arrow(start, end, control) => {
                *end = point;
                *control = Point {
                    x: (start.x + point.x) / 2.0,
                    y: (start.y + point.y) / 2.0,
                };
            }
        }
    }
    pub fn finish(&mut self) -> bool {
        if let Shape::Pencil(points) = &mut self.shape {
            if points.len() < 2 {
                return false;
            }
            // Two iterations of corner cutting; preserves both endpoints.
            for _ in 0..2 {
                if points.len() * 2 >= MAX_POINTS {
                    break;
                }
                let mut smooth = Vec::with_capacity(points.len() * 2);
                smooth.push(points[0]);
                for pair in points.windows(2) {
                    let (a, b) = (pair[0], pair[1]);
                    smooth.push(Point {
                        x: a.x * 0.75 + b.x * 0.25,
                        y: a.y * 0.75 + b.y * 0.25,
                    });
                    smooth.push(Point {
                        x: a.x * 0.25 + b.x * 0.75,
                        y: a.y * 0.25 + b.y * 0.75,
                    });
                }
                smooth.push(*points.last().expect("nonempty stroke"));
                *points = smooth;
            }
            return true;
        }
        let (a, b) = self.endpoints().expect("non-pencil shape");
        (a.x - b.x).abs() >= 1.0 || (a.y - b.y).abs() >= 1.0
    }
    pub fn endpoints(&self) -> Option<(Point, Point)> {
        match &self.shape {
            Shape::Pencil(_) => None,
            Shape::Rectangle(a, b)
            | Shape::FilledRectangle(a, b)
            | Shape::Highlighter(a, b)
            | Shape::Mosaic(a, b)
            | Shape::Ellipse(a, b)
            | Shape::Line(a, b) => Some((*a, *b)),
            Shape::Arrow(a, b, _) => Some((*a, *b)),
        }
    }
    pub fn bounds(&self) -> (Point, Point) {
        if let Some((a, b)) = self.endpoints() {
            let mut lo = Point {
                x: a.x.min(b.x),
                y: a.y.min(b.y),
            };
            let mut hi = Point {
                x: a.x.max(b.x),
                y: a.y.max(b.y),
            };
            if let Shape::Arrow(_, _, control) = &self.shape {
                lo.x = lo.x.min(control.x);
                lo.y = lo.y.min(control.y);
                hi.x = hi.x.max(control.x);
                hi.y = hi.y.max(control.y);
            }
            return (lo, hi);
        }
        let Shape::Pencil(points) = &self.shape else {
            unreachable!()
        };
        let mut min = Point {
            x: f32::MAX,
            y: f32::MAX,
        };
        let mut max = Point {
            x: f32::MIN,
            y: f32::MIN,
        };
        for p in points {
            min.x = min.x.min(p.x);
            min.y = min.y.min(p.y);
            max.x = max.x.max(p.x);
            max.y = max.y.max(p.y);
        }
        (min, max)
    }
    pub fn translate(&mut self, dx: f32, dy: f32) {
        match &mut self.shape {
            Shape::Pencil(points) => {
                for p in points {
                    *p = p.moved(dx, dy);
                }
            }
            Shape::Rectangle(a, b)
            | Shape::FilledRectangle(a, b)
            | Shape::Highlighter(a, b)
            | Shape::Mosaic(a, b)
            | Shape::Ellipse(a, b)
            | Shape::Line(a, b) => {
                *a = a.moved(dx, dy);
                *b = b.moved(dx, dy);
            }
            Shape::Arrow(a, b, c) => {
                *a = a.moved(dx, dy);
                *b = b.moved(dx, dy);
                *c = c.moved(dx, dy);
            }
        }
    }
    pub fn handles(&self) -> Vec<Point> {
        match &self.shape {
            Shape::Line(a, b) => vec![*a, *b],
            Shape::Arrow(a, b, c) => vec![*a, *b, *c],
            _ => {
                let (lo, hi) = self.bounds();
                vec![
                    lo,
                    Point { x: hi.x, y: lo.y },
                    hi,
                    Point { x: lo.x, y: hi.y },
                ]
            }
        }
    }
    /// Resize relative to the original mark, not the previous mouse move.
    pub fn resize_handle(&mut self, handle: usize, point: Point) {
        let (old_lo, old_hi) = self.bounds();
        match &mut self.shape {
            Shape::Line(a, b) => {
                if handle == 0 {
                    *a = point;
                } else {
                    *b = point;
                }
            }
            Shape::Arrow(a, b, c) => {
                if handle == 2 {
                    *c = point;
                } else {
                    let (cx, cy) = (c.x - (a.x + b.x) / 2.0, c.y - (a.y + b.y) / 2.0);
                    if handle == 0 {
                        *a = point;
                    } else {
                        *b = point;
                    }
                    *c = Point {
                        x: (a.x + b.x) / 2.0 + cx,
                        y: (a.y + b.y) / 2.0 + cy,
                    };
                }
            }
            Shape::Rectangle(a, b)
            | Shape::FilledRectangle(a, b)
            | Shape::Highlighter(a, b)
            | Shape::Mosaic(a, b)
            | Shape::Ellipse(a, b) => {
                let lo = Point {
                    x: a.x.min(b.x),
                    y: a.y.min(b.y),
                };
                let hi = Point {
                    x: a.x.max(b.x),
                    y: a.y.max(b.y),
                };
                let opposite = [
                    hi,
                    Point { x: lo.x, y: hi.y },
                    lo,
                    Point { x: hi.x, y: lo.y },
                ];
                *a = opposite[handle.min(3)];
                *b = point;
            }
            Shape::Pencil(points) => {
                let (lo, hi) = (old_lo, old_hi);
                let opposite = [
                    hi,
                    Point { x: lo.x, y: hi.y },
                    lo,
                    Point { x: hi.x, y: lo.y },
                ][handle.min(3)];
                let min = Point {
                    x: point.x.min(opposite.x),
                    y: point.y.min(opposite.y),
                };
                let max = Point {
                    x: point.x.max(opposite.x),
                    y: point.y.max(opposite.y),
                };
                for p in points {
                    p.x = min.x + (p.x - lo.x) / (hi.x - lo.x).max(1.0) * (max.x - min.x);
                    p.y = min.y + (p.y - lo.y) / (hi.y - lo.y).max(1.0) * (max.y - min.y);
                }
            }
        }
    }
    pub fn contains(&self, p: Point, tolerance: f32) -> bool {
        let (lo, hi) = self.bounds();
        let r = self.width / 2.0 + tolerance;
        if p.x < lo.x - r || p.x > hi.x + r || p.y < lo.y - r || p.y > hi.y + r {
            return false;
        }
        match &self.shape {
            Shape::Pencil(points) => points
                .windows(2)
                .any(|pair| segment_distance(p, pair[0], pair[1]) <= r),
            Shape::Line(a, b) => segment_distance(p, *a, *b) <= r,
            Shape::Arrow(a, b, c) => {
                let mut last = *a;
                for i in 1..=32 {
                    let next = curve(*a, *c, *b, i as f32 / 32.0);
                    if segment_distance(p, last, next) <= r {
                        return true;
                    }
                    last = next;
                }
                false
            }
            Shape::FilledRectangle(_, _) | Shape::Highlighter(_, _) | Shape::Mosaic(_, _) => true,
            Shape::Rectangle(_, _) => {
                (p.x - lo.x).abs() <= r
                    || (p.x - hi.x).abs() <= r
                    || (p.y - lo.y).abs() <= r
                    || (p.y - hi.y).abs() <= r
            }
            Shape::Ellipse(_, _) => {
                let rx = (hi.x - lo.x) / 2.0;
                let ry = (hi.y - lo.y) / 2.0;
                if rx < 1.0 || ry < 1.0 {
                    return false;
                }
                let distance = (((p.x - (lo.x + hi.x) / 2.0) / rx).powi(2)
                    + ((p.y - (lo.y + hi.y) / 2.0) / ry).powi(2))
                .sqrt();
                (distance - 1.0).abs() * rx.min(ry) <= r
            }
        }
    }
}
/// A content-independent, opaque mosaic. Tiles never sample or retain source pixels.
/// This protects the *export* region; the original capture file remains untouched.
pub fn mosaic_tiles(a: Point, b: Point, mut paint: impl FnMut(Point, Point, u32)) {
    let (x0, x1) = ((a.x.min(b.x).floor() as i32), (a.x.max(b.x).ceil() as i32));
    let (y0, y1) = ((a.y.min(b.y).floor() as i32), (a.y.max(b.y).ceil() as i32));
    if x0 >= x1 || y0 >= y1 {
        return;
    }
    let span = x1.saturating_sub(x0).max(y1.saturating_sub(y0)) as u32;
    let step = span.div_ceil(96).max(8) as i32;
    for y in (y0..y1).step_by(step as usize) {
        for x in (x0..x1).step_by(step as usize) {
            let mut hash =
                (x as u32).wrapping_mul(0x9e3779b9) ^ (y as u32).wrapping_mul(0x85ebca6b);
            hash ^= hash >> 16;
            hash = hash.wrapping_mul(0x7feb352d);
            hash ^= hash >> 15;
            let value = 58 + hash % 73;
            let tint = (value << 16) | ((value + hash % 6) << 8) | (value + hash % 11);
            paint(
                Point {
                    x: x as f32,
                    y: y as f32,
                },
                Point {
                    x: x.saturating_add(step).min(x1) as f32,
                    y: y.saturating_add(step).min(y1) as f32,
                },
                tint,
            );
        }
    }
}
pub fn curve(a: Point, control: Point, b: Point, t: f32) -> Point {
    let u = 1.0 - t;
    Point {
        x: u * u * a.x + 2.0 * u * t * control.x + t * t * b.x,
        y: u * u * a.y + 2.0 * u * t * control.y + t * t * b.y,
    }
}
fn segment_distance(p: Point, a: Point, b: Point) -> f32 {
    let (dx, dy) = (b.x - a.x, b.y - a.y);
    let t = if dx * dx + dy * dy < 0.0001 {
        0.0
    } else {
        ((p.x - a.x) * dx + (p.y - a.y) * dy) / (dx * dx + dy * dy)
    }
    .clamp(0.0, 1.0);
    ((p.x - a.x - dx * t).powi(2) + (p.y - a.y - dy * t).powi(2)).sqrt()
}

#[derive(Clone, Debug)]
struct Change {
    index: usize,
    before: Option<Mark>,
    after: Option<Mark>,
}
#[derive(Default)]
pub struct Document {
    pub marks: Vec<Mark>,
    pub selected: Option<usize>,
    undo: Vec<Change>,
    redo: Vec<Change>,
    pub revision: u64,
}
impl Document {
    fn record(&mut self, change: Change) {
        self.undo.push(change);
        if self.undo.len() > HISTORY_LIMIT {
            self.undo.remove(0);
        }
        self.redo.clear();
        self.revision = self.revision.wrapping_add(1);
    }
    pub fn add(&mut self, mark: Mark) -> Result<()> {
        ensure!(
            self.marks.len() < MAX_MARKS,
            "Annotation limit reached ({MAX_MARKS})"
        );
        let i = self.marks.len();
        self.marks.push(mark.clone());
        self.selected = Some(i);
        self.record(Change {
            index: i,
            before: None,
            after: Some(mark),
        });
        Ok(())
    }
    pub fn edit(&mut self, index: usize, original: Mark) {
        if self.marks[index] != original {
            self.record(Change {
                index,
                before: Some(original),
                after: Some(self.marks[index].clone()),
            });
        }
    }
    pub fn recolor(&mut self, index: usize, color: u32) {
        if self.marks[index].color == color {
            return;
        }
        let previous = self.marks[index].clone();
        self.marks[index].color = color;
        self.edit(index, previous);
    }
    pub fn set_width(&mut self, index: usize, width: f32) {
        if self.marks[index].width == width {
            return;
        }
        let previous = self.marks[index].clone();
        self.marks[index].width = width;
        self.edit(index, previous);
    }
    pub fn delete_selected(&mut self) -> bool {
        let Some(index) = self.selected.take() else {
            return false;
        };
        let before = self.marks.remove(index);
        self.record(Change {
            index,
            before: Some(before),
            after: None,
        });
        true
    }
    pub fn hit(&self, point: Point, screen_tolerance: f32) -> Option<usize> {
        self.marks
            .iter()
            .enumerate()
            .rev()
            .find(|(_, mark)| mark.contains(point, screen_tolerance))
            .map(|(i, _)| i)
    }
    fn apply(&mut self, change: &Change, forward: bool) {
        let target = if forward {
            &change.after
        } else {
            &change.before
        };
        let current = if forward {
            &change.before
        } else {
            &change.after
        };
        match (current, target) {
            (None, Some(mark)) => self.marks.insert(change.index, mark.clone()),
            (Some(_), None) => {
                self.marks.remove(change.index);
            }
            (Some(_), Some(mark)) => self.marks[change.index] = mark.clone(),
            (None, None) => unreachable!(),
        }
        self.selected = target.as_ref().map(|_| change.index);
        self.revision = self.revision.wrapping_add(1);
    }
    pub fn undo(&mut self) -> bool {
        let Some(change) = self.undo.pop() else {
            return false;
        };
        self.apply(&change, false);
        self.redo.push(change);
        true
    }
    pub fn redo(&mut self) -> bool {
        let Some(change) = self.redo.pop() else {
            return false;
        };
        self.apply(&change, true);
        self.undo.push(change);
        true
    }
}

fn blend(pixels: &mut [u8], pixel: usize, rgb: u32, coverage: f32) {
    let a = coverage.clamp(0.0, 1.0);
    if a <= 0.0 {
        return;
    }
    let dst_a = pixels[pixel + 3] as f32 / 255.0;
    let out_a = a + dst_a * (1.0 - a);
    let c = [
        (rgb & 255) as f32,
        ((rgb >> 8) & 255) as f32,
        ((rgb >> 16) & 255) as f32,
    ];
    for k in 0..3 {
        pixels[pixel + k] =
            ((c[k] * a + pixels[pixel + k] as f32 * dst_a * (1.0 - a)) / out_a).round() as u8;
    }
    pixels[pixel + 3] = (out_a * 255.0).round() as u8;
}
fn paint_segment(image: &mut Raster, a: Point, b: Point, width: f32, color: u32, offset: Point) {
    let a = a.moved(offset.x, offset.y);
    let b = b.moved(offset.x, offset.y);
    let radius = (width / 2.0).max(0.5);
    let (left, right) = (
        ((a.x.min(b.x) - radius - 1.0).floor() as i32).max(0),
        ((a.x.max(b.x) + radius + 1.0).ceil() as i32).min(image.width as i32 - 1),
    );
    let (top, bottom) = (
        ((a.y.min(b.y) - radius - 1.0).floor() as i32).max(0),
        ((a.y.max(b.y) + radius + 1.0).ceil() as i32).min(image.height as i32 - 1),
    );
    for y in top..=bottom {
        for x in left..=right {
            let distance = segment_distance(
                Point {
                    x: x as f32 + 0.5,
                    y: y as f32 + 0.5,
                },
                a,
                b,
            );
            let coverage = (radius + 0.5 - distance).clamp(0.0, 1.0);
            if coverage > 0.0 {
                blend(
                    &mut image.pixels,
                    ((y as u32 * image.width + x as u32) * 4) as usize,
                    color,
                    coverage,
                );
            }
        }
    }
}
fn paint_mark(image: &mut Raster, mark: &Mark, offset: Point) {
    match &mark.shape {
        Shape::Pencil(points) => {
            for pair in points.windows(2) {
                paint_segment(image, pair[0], pair[1], mark.width, mark.color, offset);
            }
        }
        Shape::Line(a, b) => paint_segment(image, *a, *b, mark.width, mark.color, offset),
        Shape::Arrow(a, b, control) => {
            let steps = (((b.x - a.x).hypot(b.y - a.y)) / 4.0).clamp(16.0, 128.0) as usize;
            let mut last = *a;
            for i in 1..=steps {
                let next = curve(*a, *control, *b, i as f32 / steps as f32);
                paint_segment(image, last, next, mark.width, mark.color, offset);
                last = next;
            }
            let angle = (b.y - control.y).atan2(b.x - control.x);
            let len = (mark.width * 5.0).max(12.0);
            for direction in [-0.55_f32, 0.55] {
                let end = Point {
                    x: b.x - len * (angle + direction).cos(),
                    y: b.y - len * (angle + direction).sin(),
                };
                paint_segment(image, *b, end, mark.width, mark.color, offset);
            }
        }
        Shape::Mosaic(a, b) => {
            mosaic_tiles(*a, *b, |lo, hi, rgb| {
                let x0 = (lo.x + offset.x).floor().max(0.0) as u32;
                let x1 = (hi.x + offset.x).ceil().min(image.width as f32) as u32;
                let y0 = (lo.y + offset.y).floor().max(0.0) as u32;
                let y1 = (hi.y + offset.y).ceil().min(image.height as f32) as u32;
                for y in y0..y1 {
                    for x in x0..x1 {
                        let i = ((y * image.width + x) * 4) as usize;
                        image.pixels[i..i + 4].copy_from_slice(&[
                            (rgb & 255) as u8,
                            ((rgb >> 8) & 255) as u8,
                            ((rgb >> 16) & 255) as u8,
                            255,
                        ]);
                    }
                }
            });
        }
        Shape::Rectangle(a, b) | Shape::FilledRectangle(a, b) | Shape::Highlighter(a, b) => {
            let lo = Point {
                x: a.x.min(b.x),
                y: a.y.min(b.y),
            };
            let hi = Point {
                x: a.x.max(b.x),
                y: a.y.max(b.y),
            };
            if matches!(
                mark.shape,
                Shape::FilledRectangle(_, _) | Shape::Highlighter(_, _)
            ) {
                let (x0, x1) = (
                    ((lo.x + offset.x).floor() as i32).max(0),
                    ((hi.x + offset.x).ceil() as i32).min(image.width as i32),
                );
                let (y0, y1) = (
                    ((lo.y + offset.y).floor() as i32).max(0),
                    ((hi.y + offset.y).ceil() as i32).min(image.height as i32),
                );
                for y in y0..y1 {
                    for x in x0..x1 {
                        blend(
                            &mut image.pixels,
                            ((y as u32 * image.width + x as u32) * 4) as usize,
                            mark.color,
                            mark.opacity,
                        );
                    }
                }
            } else {
                let p = [
                    lo,
                    Point { x: hi.x, y: lo.y },
                    hi,
                    Point { x: lo.x, y: hi.y },
                    lo,
                ];
                for pair in p.windows(2) {
                    paint_segment(image, pair[0], pair[1], mark.width, mark.color, offset);
                }
            }
        }
        Shape::Ellipse(a, b) => {
            let cx = (a.x + b.x) / 2.0;
            let cy = (a.y + b.y) / 2.0;
            let rx = (a.x - b.x).abs() / 2.0;
            let ry = (a.y - b.y).abs() / 2.0;
            let steps = ((rx + ry) * 2.0).clamp(24.0, 720.0) as usize;
            let mut prev = Point { x: cx + rx, y: cy };
            for i in 1..=steps {
                let t = std::f32::consts::TAU * i as f32 / steps as f32;
                let p = Point {
                    x: cx + rx * t.cos(),
                    y: cy + ry * t.sin(),
                };
                paint_segment(image, prev, p, mark.width, mark.color, offset);
                prev = p;
            }
        }
    }
}
/// Flatten only on output; preview objects remain editable and the capture stays untouched.
pub fn flatten(base: &Raster, marks: &[Mark], offset: Point) -> Result<Raster> {
    ensure!(
        base.pixels.len() == base.width as usize * base.height as usize * 4,
        "Invalid annotation base raster"
    );
    let mut image = Raster {
        width: base.width,
        height: base.height,
        pixels: base.pixels.clone(),
    };
    for mark in marks {
        paint_mark(&mut image, mark, offset);
    }
    Ok(image)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn mosaic_export_replaces_source_pixels_with_content_independent_opaque_tiles() -> Result<()> {
        let red = Raster {
            width: 40,
            height: 40,
            pixels: [0, 0, 255, 255].repeat(40 * 40),
        };
        let blue = Raster {
            width: 40,
            height: 40,
            pixels: [255, 0, 0, 255].repeat(40 * 40),
        };
        let mut mark =
            Mark::from_tool(Control::Pixelate, Point { x: 4.0, y: 5.0 }, 0xffde00, 3.0).unwrap();
        mark.update(Point { x: 32.0, y: 33.0 });
        assert!(mark.finish());
        let a = flatten(&red, &[mark.clone()], Point::default())?;
        let b = flatten(&blue, &[mark], Point::default())?;
        for y in 5..33 {
            for x in 4..32 {
                let i = ((y * 40 + x) * 4) as usize;
                assert_eq!(&a.pixels[i..i + 4], &b.pixels[i..i + 4]);
                assert_eq!(a.pixels[i + 3], 255);
                assert_ne!(&a.pixels[i..i + 4], &red.pixels[i..i + 4]);
            }
        }
        assert_eq!(&a.pixels[0..4], &red.pixels[0..4]);
        assert_eq!(&red.pixels[0..4], &[0, 0, 255, 255]);
        Ok(())
    }
    #[test]
    fn manual_highlight_blends_without_mutating_the_source() -> Result<()> {
        let source = Raster {
            width: 12,
            height: 12,
            pixels: [255, 0, 0, 255].repeat(12 * 12),
        };
        let mut mark = Mark::from_tool(
            Control::Highlighter,
            Point { x: 2.0, y: 2.0 },
            0xffde00,
            3.0,
        )
        .unwrap();
        mark.update(Point { x: 10.0, y: 10.0 });
        assert!(mark.finish());
        let image = flatten(&source, &[mark.clone()], Point::default())?;
        assert_eq!(
            image.pixels[((6 * 12 + 6) * 4)..((6 * 12 + 6) * 4 + 4)],
            [166, 78, 89, 255]
        );
        assert_eq!(image.pixels[0..4], [255, 0, 0, 255]);
        assert_eq!(
            source.pixels[((6 * 12 + 6) * 4)..((6 * 12 + 6) * 4 + 4)],
            [255, 0, 0, 255]
        );
        mark.resize_handle(0, Point { x: 1.0, y: 1.0 });
        assert_eq!(mark.opacity, 0.35);
        assert_eq!(mark.bounds().0, Point { x: 1.0, y: 1.0 });
        Ok(())
    }
    #[test]
    fn selected_handles_resize_shapes_and_curve_arrows_without_baking_pixels() -> Result<()> {
        let mut rectangle = Mark::from_tool(
            Control::Rectangle,
            Point { x: 10.0, y: 10.0 },
            0xf92d3a,
            3.0,
        )
        .unwrap();
        rectangle.update(Point { x: 40.0, y: 40.0 });
        assert_eq!(rectangle.handles().len(), 4);
        rectangle.resize_handle(0, Point { x: 5.0, y: 5.0 });
        assert_eq!(rectangle.bounds().0, Point { x: 5.0, y: 5.0 });
        assert_eq!(rectangle.bounds().1, Point { x: 40.0, y: 40.0 });
        let mut arrow =
            Mark::from_tool(Control::Arrow, Point { x: 10.0, y: 40.0 }, 0xff0000, 4.0).unwrap();
        arrow.update(Point { x: 50.0, y: 40.0 });
        arrow.resize_handle(2, Point { x: 30.0, y: 5.0 });
        assert_eq!(arrow.handles()[2], Point { x: 30.0, y: 5.0 });
        assert!(arrow.contains(Point { x: 30.0, y: 22.5 }, 2.0));
        assert!(!arrow.contains(Point { x: 30.0, y: 40.0 }, 2.0));
        let source = Raster {
            width: 60,
            height: 60,
            pixels: [255, 255, 255, 255].repeat(60 * 60),
        };
        let flattened = flatten(&source, &[arrow], Point::default())?;
        let index = ((22 * 60 + 30) * 4) as usize;
        assert_ne!(
            &flattened.pixels[index..index + 4],
            &source.pixels[index..index + 4]
        );
        Ok(())
    }
    #[test]
    fn pencil_smooths_endpoints_and_remains_editable() -> Result<()> {
        let mut mark =
            Mark::from_tool(Control::Pencil, Point { x: 2.0, y: 2.0 }, 0xf92d3a, 3.0).unwrap();
        mark.update(Point { x: 9.0, y: 15.0 });
        mark.update(Point { x: 20.0, y: 7.0 });
        assert!(mark.finish());
        let Shape::Pencil(points) = &mark.shape else {
            panic!("wrong shape")
        };
        assert!(points.len() > 3);
        assert_eq!(points[0], Point { x: 2.0, y: 2.0 });
        assert_eq!(*points.last().unwrap(), Point { x: 20.0, y: 7.0 });
        let mut doc = Document::default();
        doc.add(mark)?;
        assert!(doc.undo());
        assert!(doc.marks.is_empty());
        assert!(doc.redo());
        assert_eq!(doc.marks.len(), 1);
        Ok(())
    }
    #[test]
    fn edit_color_move_delete_history_and_flatten_preserve_source() -> Result<()> {
        let original = Raster {
            width: 50,
            height: 50,
            pixels: [255, 255, 255, 255].repeat(50 * 50),
        };
        let mut doc = Document::default();
        let start = Point { x: 5.0, y: 5.0 };
        let mut shape = Mark::from_tool(Control::Fill, start, 0xff0000, 2.0).unwrap();
        shape.update(Point { x: 20.0, y: 20.0 });
        doc.add(shape)?;
        doc.recolor(0, 0x0000ff);
        let before = doc.marks[0].clone();
        doc.marks[0].translate(5.0, 0.0);
        doc.edit(0, before);
        assert_eq!(doc.hit(Point { x: 15.0, y: 10.0 }, 2.0), Some(0));
        let out = flatten(&original, &doc.marks, Point::default())?;
        let pos = ((12 * 50 + 15) * 4) as usize;
        assert_eq!(&out.pixels[pos..pos + 4], &[255, 0, 0, 255]);
        assert_eq!(&original.pixels[pos..pos + 4], &[255, 255, 255, 255]);
        assert!(doc.undo());
        assert!(doc.undo());
        assert_eq!(doc.marks[0].color, 0xff0000);
        assert!(doc.redo());
        assert!(doc.delete_selected());
        assert!(doc.undo());
        assert_eq!(doc.marks.len(), 1);
        Ok(())
    }
}
