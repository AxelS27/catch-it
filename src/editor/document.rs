//! Editable image-space annotations. The source screenshot is never modified.
use super::layout::Control;
use crate::storage::Raster;
use anyhow::{Result, ensure};
use std::sync::Arc;

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
    Spotlight(Point, Point),
    Counter(Point, u32),
    Text(Point, String),
    Crop(Point, Point),
    Image(Point, Point, Arc<Raster>),
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
    pub angle: f32,
}
pub fn rotate(point: Point, center: Point, angle: f32) -> Point {
    let (s, c) = angle.sin_cos();
    let (x, y) = (point.x - center.x, point.y - center.y);
    Point {
        x: center.x + x * c - y * s,
        y: center.y + x * s + y * c,
    }
}
impl Mark {
    pub fn from_tool(tool: Control, start: Point, color: u32, width: f32) -> Option<Self> {
        let shape = match tool {
            Control::Pencil => Shape::Pencil(vec![start]),
            Control::Rectangle => Shape::Rectangle(start, start),
            Control::Fill => Shape::FilledRectangle(start, start),
            Control::Highlighter => Shape::Highlighter(start, start),
            Control::Pixelate => Shape::Mosaic(start, start),
            Control::Spotlight => Shape::Spotlight(start, start),
            Control::Counter => Shape::Counter(start, 1),
            Control::Text => Shape::Text(start, String::new()),
            Control::Crop => Shape::Crop(start, start),
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
            angle: 0.0,
        })
    }
    pub fn update(&mut self, point: Point) {
        match &mut self.shape {
            Shape::Counter(center, _) | Shape::Text(center, _) => *center = point,
            Shape::Image(_, end, _) => *end = point,
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
            | Shape::Spotlight(_, end)
            | Shape::Crop(_, end)
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
        if let Shape::Text(_, ref text) = self.shape {
            return !text.trim().is_empty();
        }
        if matches!(self.shape, Shape::Counter(..)) {
            return true;
        }
        let (a, b) = self.endpoints().expect("non-pencil shape");
        if matches!(self.shape, Shape::Crop(..)) {
            return (a.x - b.x).abs() >= 4.0 && (a.y - b.y).abs() >= 4.0;
        }
        (a.x - b.x).abs() >= 1.0 || (a.y - b.y).abs() >= 1.0
    }
    pub fn endpoints(&self) -> Option<(Point, Point)> {
        match &self.shape {
            Shape::Pencil(_) | Shape::Counter(..) | Shape::Text(..) => None,
            Shape::Rectangle(a, b)
            | Shape::FilledRectangle(a, b)
            | Shape::Highlighter(a, b)
            | Shape::Mosaic(a, b)
            | Shape::Spotlight(a, b)
            | Shape::Crop(a, b)
            | Shape::Image(a, b, _)
            | Shape::Ellipse(a, b)
            | Shape::Line(a, b) => Some((*a, *b)),
            Shape::Arrow(a, b, _) => Some((*a, *b)),
        }
    }
    pub fn counter_radius(&self) -> f32 {
        14.0 + (self.width - 3.0) * 0.5
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
        if let Shape::Text(point, ref text) = self.shape {
            let w = text
                .lines()
                .map(|line| line.chars().count())
                .max()
                .unwrap_or(0) as f32
                * self.width
                * 0.65
                + 8.0;
            let h = text.lines().count().max(1) as f32 * self.width * 1.35 + 6.0;
            return (
                point,
                Point {
                    x: point.x + w.clamp(32.0, 2048.0),
                    y: point.y + h.clamp(24.0, 4096.0),
                },
            );
        }
        if let Shape::Counter(center, _) = self.shape {
            let radius = self.counter_radius();
            return (
                Point {
                    x: center.x - radius,
                    y: center.y - radius,
                },
                Point {
                    x: center.x + radius,
                    y: center.y + radius,
                },
            );
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
            Shape::Counter(center, _) | Shape::Text(center, _) => *center = center.moved(dx, dy),
            Shape::Pencil(points) => {
                for p in points {
                    *p = p.moved(dx, dy);
                }
            }
            Shape::Rectangle(a, b)
            | Shape::FilledRectangle(a, b)
            | Shape::Highlighter(a, b)
            | Shape::Mosaic(a, b)
            | Shape::Spotlight(a, b)
            | Shape::Crop(a, b)
            | Shape::Image(a, b, _)
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
            Shape::Crop(..) => Vec::new(),
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
        .into_iter()
        .map(|point| rotate(point, self.center(), self.angle))
        .collect()
    }
    pub fn center(&self) -> Point {
        let (lo, hi) = self.bounds();
        Point {
            x: (lo.x + hi.x) / 2.0,
            y: (lo.y + hi.y) / 2.0,
        }
    }
    pub fn can_rotate(&self) -> bool {
        !matches!(
            self.shape,
            Shape::Crop(..) | Shape::Mosaic(..) | Shape::Spotlight(..) | Shape::Highlighter(..)
        )
    }
    pub fn rotation_handle(&self, scale: f32) -> Option<Point> {
        if !self.can_rotate() {
            return None;
        }
        let (lo, hi) = self.bounds();
        Some(rotate(
            Point {
                x: (lo.x + hi.x) / 2.0,
                y: lo.y - 36.0 / scale.max(0.05),
            },
            self.center(),
            self.angle,
        ))
    }
    /// Resize relative to the original mark, not the previous mouse move.
    pub fn resize_handle(&mut self, handle: usize, point: Point) {
        let (old_lo, old_hi) = self.bounds();
        match &mut self.shape {
            Shape::Counter(center, _) => {
                let opposite = [
                    old_hi,
                    Point {
                        x: old_lo.x,
                        y: old_hi.y,
                    },
                    old_lo,
                    Point {
                        x: old_hi.x,
                        y: old_lo.y,
                    },
                ][handle.min(3)];
                let radius = (point.x - opposite.x)
                    .abs()
                    .max((point.y - opposite.y).abs())
                    / 2.0;
                self.width = ((radius - 14.0) * 2.0 + 3.0).clamp(3.0, 72.0);
                *center = Point {
                    x: (point.x + opposite.x) / 2.0,
                    y: (point.y + opposite.y) / 2.0,
                };
            }
            Shape::Text(anchor, text) => {
                let opposite = [
                    old_hi,
                    Point {
                        x: old_lo.x,
                        y: old_hi.y,
                    },
                    old_lo,
                    Point {
                        x: old_hi.x,
                        y: old_lo.y,
                    },
                ][handle.min(3)];
                let lines = text.lines().count().max(1) as f32;
                self.width =
                    (((point.y - opposite.y).abs() - 6.0) / (lines * 1.35)).clamp(8.0, 72.0);
                *anchor = Point {
                    x: point.x.min(opposite.x),
                    y: point.y.min(opposite.y),
                };
            }
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
            | Shape::Spotlight(a, b)
            | Shape::Crop(a, b)
            | Shape::Image(a, b, _)
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
        let p = rotate(p, self.center(), -self.angle);
        let (lo, hi) = self.bounds();
        let r = self.width / 2.0 + tolerance;
        if p.x < lo.x - r || p.x > hi.x + r || p.y < lo.y - r || p.y > hi.y + r {
            return false;
        }
        match &self.shape {
            Shape::Text(..) => true,
            Shape::Crop(..) => false,
            Shape::Counter(center, _) => {
                (p.x - center.x).hypot(p.y - center.y) <= self.counter_radius() + tolerance
            }
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
            Shape::FilledRectangle(_, _)
            | Shape::Highlighter(_, _)
            | Shape::Mosaic(_, _)
            | Shape::Spotlight(_, _)
            | Shape::Image(..) => true,
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
/// Image-derived visual pixelation. Never use this for confidential redaction:
/// block averages retain information about the underlying source image.
pub fn mosaic_tiles(
    source: &Raster,
    a: Point,
    b: Point,
    offset: Point,
    mut paint: impl FnMut(Point, Point, u32),
) {
    let (x0, x1) = ((a.x.min(b.x).floor() as i32), (a.x.max(b.x).ceil() as i32));
    let (y0, y1) = ((a.y.min(b.y).floor() as i32), (a.y.max(b.y).ceil() as i32));
    if x0 >= x1 || y0 >= y1 {
        return;
    }
    let span = x1.saturating_sub(x0).max(y1.saturating_sub(y0)) as u32;
    let step = span.div_ceil(96).max(16) as i32;
    for y in (y0..y1).step_by(step as usize) {
        for x in (x0..x1).step_by(step as usize) {
            let (right, bottom) = (
                x.saturating_add(step).min(x1),
                y.saturating_add(step).min(y1),
            );
            let mut channels = [0u32; 3];
            let mut samples = 0u32;
            // Sample a stratified grid so preview and full-resolution export
            // share exactly the same image-derived tiles without a per-frame
            // allocation or downscaled/blurred intermediate bitmap.
            for row in 0..4 {
                for column in 0..4 {
                    let sx =
                        (x as f32 + (right - x) as f32 * (column as f32 + 0.5) / 4.0 + offset.x)
                            .floor() as i32;
                    let sy = (y as f32 + (bottom - y) as f32 * (row as f32 + 0.5) / 4.0 + offset.y)
                        .floor() as i32;
                    if sx < 0 || sy < 0 || sx >= source.width as i32 || sy >= source.height as i32 {
                        continue;
                    }
                    let i = ((sy as u32 * source.width + sx as u32) * 4) as usize;
                    for (channel, sum) in channels.iter_mut().enumerate() {
                        *sum += u32::from(source.pixels[i + channel]);
                    }
                    samples += 1;
                }
            }
            if samples == 0 {
                continue;
            }
            let rgb = ((channels[2] / samples) << 16)
                | ((channels[1] / samples) << 8)
                | (channels[0] / samples);
            paint(
                Point {
                    x: x as f32,
                    y: y as f32,
                },
                Point {
                    x: right as f32,
                    y: bottom as f32,
                },
                rgb,
            );
        }
    }
}
/// Four nonoverlapping rectangles covering everything except the spotlight.
pub fn spotlight_regions(
    width: u32,
    height: u32,
    a: Point,
    b: Point,
    offset: Point,
) -> [(u32, u32, u32, u32); 4] {
    let (left, right) = (
        (a.x.min(b.x) + offset.x).floor().clamp(0.0, width as f32) as u32,
        (a.x.max(b.x) + offset.x).ceil().clamp(0.0, width as f32) as u32,
    );
    let (top, bottom) = (
        (a.y.min(b.y) + offset.y).floor().clamp(0.0, height as f32) as u32,
        (a.y.max(b.y) + offset.y).ceil().clamp(0.0, height as f32) as u32,
    );
    [
        (0, 0, width, top),
        (0, bottom, width, height),
        (0, top, left, bottom),
        (right, top, width, bottom),
    ]
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
        self.selected = (!matches!(mark.shape, Shape::Pencil(..))).then_some(i);
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
    pub fn asset_bytes(&self) -> usize {
        let mut seen = std::collections::HashSet::new();
        let mut bytes = 0;
        let mut count = |mark: &Mark| {
            if let Shape::Image(_, _, image) = &mark.shape
                && seen.insert(Arc::as_ptr(image) as usize)
            {
                bytes += image.pixels.len();
            }
        };
        for mark in &self.marks {
            count(mark);
        }
        for change in self.undo.iter().chain(self.redo.iter()) {
            if let Some(mark) = &change.before {
                count(mark);
            }
            if let Some(mark) = &change.after {
                count(mark);
            }
        }
        bytes
    }
    pub fn crop(&self) -> Option<(Point, Point)> {
        self.marks.iter().rev().find_map(|mark| {
            if let Shape::Crop(a, b) = mark.shape {
                Some((
                    Point {
                        x: a.x.min(b.x),
                        y: a.y.min(b.y),
                    },
                    Point {
                        x: a.x.max(b.x),
                        y: a.y.max(b.y),
                    },
                ))
            } else {
                None
            }
        })
    }
    pub fn crop_pixels(
        &self,
        width: u32,
        height: u32,
        offset: Point,
    ) -> Option<(u32, u32, u32, u32)> {
        let (lo, hi) = self.crop()?;
        let (x0, x1) = (
            (lo.x + offset.x).floor().clamp(0.0, width as f32) as u32,
            (hi.x + offset.x).ceil().clamp(0.0, width as f32) as u32,
        );
        let (y0, y1) = (
            (lo.y + offset.y).floor().clamp(0.0, height as f32) as u32,
            (hi.y + offset.y).ceil().clamp(0.0, height as f32) as u32,
        );
        (x1 > x0 && y1 > y0).then_some((x0, y0, x1, y1))
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
        self.selected = target.as_ref().and_then(|mark| {
            (!matches!(mark.shape, Shape::Crop(..) | Shape::Pencil(..))).then_some(change.index)
        });
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
fn paint_mark(image: &mut Raster, source: &Raster, mark: &Mark, offset: Point) -> Result<()> {
    if mark.angle.abs() > 0.0001 && mark.can_rotate() {
        return paint_rotated(image, source, mark, offset);
    }
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
            mosaic_tiles(source, *a, *b, offset, |lo, hi, rgb| {
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
        Shape::Counter(center, number) => {
            let radius = mark.counter_radius();
            let diameter = (radius * 2.0).ceil() as u32;
            let x0 = (center.x + offset.x - radius).floor() as i32;
            let y0 = (center.y + offset.y - radius).floor() as i32;
            let text = super::text_raster::glyph_mask(
                &number.to_string(),
                diameter,
                diameter,
                radius * 1.12,
                true,
            )?;
            for y in 0..diameter {
                for x in 0..diameter {
                    let (px, py) = (x0 + x as i32, y0 + y as i32);
                    if px < 0 || py < 0 || px >= image.width as i32 || py >= image.height as i32 {
                        continue;
                    }
                    let coverage = (radius + 0.5
                        - ((px as f32 + 0.5 - center.x - offset.x)
                            .hypot(py as f32 + 0.5 - center.y - offset.y)))
                    .clamp(0.0, 1.0);
                    let i = ((py as u32 * image.width + px as u32) * 4) as usize;
                    blend(&mut image.pixels, i, mark.color, coverage);
                    blend(
                        &mut image.pixels,
                        i,
                        0xffffff,
                        text[(y * diameter + x) as usize] as f32 / 255.0,
                    );
                }
            }
        }
        Shape::Crop(..) => {}
        Shape::Image(a, b, overlay) => {
            let (lo, hi) = (
                Point {
                    x: a.x.min(b.x) + offset.x,
                    y: a.y.min(b.y) + offset.y,
                },
                Point {
                    x: a.x.max(b.x) + offset.x,
                    y: a.y.max(b.y) + offset.y,
                },
            );
            let (w, h) = (hi.x - lo.x, hi.y - lo.y);
            if w >= 1.0 && h >= 1.0 {
                let x0 = lo.x.floor().max(0.0) as u32;
                let y0 = lo.y.floor().max(0.0) as u32;
                let x1 = hi.x.ceil().min(image.width as f32) as u32;
                let y1 = hi.y.ceil().min(image.height as f32) as u32;
                for y in y0..y1 {
                    for x in x0..x1 {
                        let sx = (((x as f32 + 0.5 - lo.x) / w * overlay.width as f32) as u32)
                            .min(overlay.width - 1);
                        let sy = (((y as f32 + 0.5 - lo.y) / h * overlay.height as f32) as u32)
                            .min(overlay.height - 1);
                        let source = ((sy * overlay.width + sx) * 4) as usize;
                        let target = ((y * image.width + x) * 4) as usize;
                        let pixel = &overlay.pixels[source..source + 4];
                        let rgb = (u32::from(pixel[2]) << 16)
                            | (u32::from(pixel[1]) << 8)
                            | u32::from(pixel[0]);
                        blend(&mut image.pixels, target, rgb, pixel[3] as f32 / 255.0);
                    }
                }
            }
        }
        Shape::Text(point, content) => {
            let (_, hi) = mark.bounds();
            let w = ((hi.x - point.x).ceil() as u32).min(image.width).max(1);
            let h = ((hi.y - point.y).ceil() as u32).min(image.height).max(1);
            let mask = super::text_raster::glyph_mask(content, w, h, mark.width, false)?;
            let (px, py) = (
                (point.x + offset.x).floor() as i32,
                (point.y + offset.y).floor() as i32,
            );
            for y in 0..h {
                for x in 0..w {
                    let (dx, dy) = (px + x as i32, py + y as i32);
                    if dx < 0 || dy < 0 || dx >= image.width as i32 || dy >= image.height as i32 {
                        continue;
                    }
                    let coverage = mask[(y * w + x) as usize] as f32 / 255.0;
                    if coverage > 0.0 {
                        blend(
                            &mut image.pixels,
                            ((dy as u32 * image.width + dx as u32) * 4) as usize,
                            mark.color,
                            coverage,
                        );
                    }
                }
            }
        }
        Shape::Spotlight(a, b) => {
            for (x0, y0, x1, y1) in spotlight_regions(image.width, image.height, *a, *b, offset) {
                for y in y0..y1 {
                    for x in x0..x1 {
                        blend(
                            &mut image.pixels,
                            ((y * image.width + x) * 4) as usize,
                            0x080912,
                            0.64,
                        );
                    }
                }
            }
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
    Ok(())
}
fn paint_rotated(image: &mut Raster, source: &Raster, mark: &Mark, offset: Point) -> Result<()> {
    let (lo, hi) = mark.bounds();
    let padding = mark.width.max(20.0).ceil() + 4.0;
    let origin = Point {
        x: (lo.x + offset.x - padding).floor(),
        y: (lo.y + offset.y - padding).floor(),
    };
    let end = Point {
        x: (hi.x + offset.x + padding).ceil(),
        y: (hi.y + offset.y + padding).ceil(),
    };
    let (w, h) = ((end.x - origin.x) as u32, (end.y - origin.y) as u32);
    ensure!(
        w > 0 && h > 0 && u64::from(w) * u64::from(h) <= 64 * 1024 * 1024,
        "Rotated annotation exceeds the image budget"
    );
    let mut tile = Raster {
        width: w,
        height: h,
        pixels: vec![0; w as usize * h as usize * 4],
    };
    let mut unrotated = mark.clone();
    unrotated.angle = 0.0;
    paint_mark(
        &mut tile,
        source,
        &unrotated,
        Point {
            x: offset.x - origin.x,
            y: offset.y - origin.y,
        },
    )?;
    let center = mark.center().moved(offset.x, offset.y);
    let corners = [
        origin,
        Point {
            x: end.x,
            y: origin.y,
        },
        end,
        Point {
            x: origin.x,
            y: end.y,
        },
    ];
    let mut bounds = (f32::MAX, f32::MAX, f32::MIN, f32::MIN);
    for corner in corners {
        let p = rotate(corner, center, mark.angle);
        bounds.0 = bounds.0.min(p.x);
        bounds.1 = bounds.1.min(p.y);
        bounds.2 = bounds.2.max(p.x);
        bounds.3 = bounds.3.max(p.y);
    }
    let (x0, y0) = (
        (bounds.0.floor().max(0.0) as u32),
        (bounds.1.floor().max(0.0) as u32),
    );
    let (x1, y1) = (
        (bounds.2.ceil().min(image.width as f32) as u32),
        (bounds.3.ceil().min(image.height as f32) as u32),
    );
    for y in y0..y1 {
        for x in x0..x1 {
            let p = rotate(
                Point {
                    x: x as f32 + 0.5,
                    y: y as f32 + 0.5,
                },
                center,
                -mark.angle,
            );
            let (sx, sy) = (p.x - origin.x - 0.5, p.y - origin.y - 0.5);
            let (ix, iy) = (sx.floor() as i32, sy.floor() as i32);
            let (fx, fy) = (sx - ix as f32, sy - iy as f32);
            let (mut alpha, mut b, mut g, mut r) = (0.0, 0.0, 0.0, 0.0);
            for (dy, wy) in [(0, 1.0 - fy), (1, fy)] {
                for (dx, wx) in [(0, 1.0 - fx), (1, fx)] {
                    let (px, py) = (ix + dx, iy + dy);
                    if px < 0 || py < 0 || px >= w as i32 || py >= h as i32 {
                        continue;
                    }
                    let i = ((py as u32 * w + px as u32) * 4) as usize;
                    let weight = wx * wy * tile.pixels[i + 3] as f32 / 255.0;
                    alpha += weight;
                    b += weight * tile.pixels[i] as f32;
                    g += weight * tile.pixels[i + 1] as f32;
                    r += weight * tile.pixels[i + 2] as f32;
                }
            }
            if alpha > 0.0 {
                let rgb = ((r / alpha).round() as u32).min(255) << 16
                    | ((g / alpha).round() as u32).min(255) << 8
                    | ((b / alpha).round() as u32).min(255);
                blend(
                    &mut image.pixels,
                    ((y * image.width + x) * 4) as usize,
                    rgb,
                    alpha.clamp(0.0, 1.0),
                );
            }
        }
    }
    Ok(())
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
        paint_mark(&mut image, base, mark, offset)?;
    }
    if let Some((a, b)) = marks.iter().rev().find_map(|mark| {
        if let Shape::Crop(a, b) = mark.shape {
            Some((a, b))
        } else {
            None
        }
    }) {
        let x0 = (a.x.min(b.x) + offset.x)
            .floor()
            .clamp(0.0, image.width as f32) as u32;
        let y0 = (a.y.min(b.y) + offset.y)
            .floor()
            .clamp(0.0, image.height as f32) as u32;
        let x1 = (a.x.max(b.x) + offset.x)
            .ceil()
            .clamp(0.0, image.width as f32) as u32;
        let y1 = (a.y.max(b.y) + offset.y)
            .ceil()
            .clamp(0.0, image.height as f32) as u32;
        ensure!(x1 > x0 && y1 > y0, "Empty crop");
        let (width, height) = (x1 - x0, y1 - y0);
        let mut pixels = Vec::with_capacity(width as usize * height as usize * 4);
        for y in y0..y1 {
            let start = ((y * image.width + x0) * 4) as usize;
            pixels.extend_from_slice(&image.pixels[start..start + width as usize * 4]);
        }
        image = Raster {
            width,
            height,
            pixels,
        };
    }
    Ok(image)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn mosaic_export_samples_source_and_does_not_modify_original() -> Result<()> {
        let source = Raster {
            width: 64,
            height: 32,
            pixels: (0..64 * 32)
                .flat_map(|i| {
                    if i % 64 < 32 {
                        [0, 0, 255, 255]
                    } else {
                        [0, 255, 0, 255]
                    }
                })
                .collect(),
        };
        let mut mark =
            Mark::from_tool(Control::Pixelate, Point { x: 4.0, y: 4.0 }, 0xffde00, 3.0).unwrap();
        mark.update(Point { x: 60.0, y: 28.0 });
        let result = flatten(&source, &[mark], Point::default())?;
        let pixel = |x: usize, y: usize| &result.pixels[(y * 64 + x) * 4..(y * 64 + x) * 4 + 4];
        assert_eq!(pixel(10, 10), &[0, 0, 255, 255]);
        assert_eq!(pixel(45, 10), &[0, 255, 0, 255]);
        assert_eq!(pixel(0, 0), &[0, 0, 255, 255]);
        assert_eq!(
            source.pixels[(10 * 64 + 45) * 4..(10 * 64 + 45) * 4 + 4],
            [0, 255, 0, 255]
        );
        Ok(())
    }
    #[test]
    fn rotated_rectangle_exports_without_baking_or_changing_the_original() -> Result<()> {
        let base = Raster {
            width: 64,
            height: 64,
            pixels: [255, 0, 0, 255].repeat(64 * 64),
        };
        let mut mark = Mark::from_tool(
            Control::Rectangle,
            Point { x: 20.0, y: 20.0 },
            0xff0000,
            2.0,
        )
        .unwrap();
        mark.update(Point { x: 40.0, y: 30.0 });
        mark.angle = std::f32::consts::FRAC_PI_2;
        assert!(mark.contains(Point { x: 35.0, y: 25.0 }, 1.0));
        let output = flatten(&base, &[mark], Point::default())?;
        let at = |x: usize, y: usize| &output.pixels[(y * 64 + x) * 4..(y * 64 + x) * 4 + 4];
        assert!(
            at(35, 25)[2] > 200,
            "Rotated edge must render at its new position"
        );
        assert_eq!(
            at(30, 20),
            [255, 0, 0, 255],
            "Unrotated edge must not remain baked in"
        );
        assert_eq!(
            &base.pixels[(25 * 64 + 35) * 4..(25 * 64 + 35) * 4 + 4],
            &[255, 0, 0, 255]
        );
        Ok(())
    }
    #[test]
    fn finished_pencil_does_not_force_edit_handles_but_remains_reselectable() -> Result<()> {
        let mut document = Document::default();
        let mut pencil =
            Mark::from_tool(Control::Pencil, Point { x: 4.0, y: 4.0 }, 0xf92d3a, 3.0).unwrap();
        pencil.update(Point { x: 24.0, y: 4.0 });
        assert!(pencil.finish());
        document.add(pencil)?;
        assert_eq!(document.selected, None);
        assert_eq!(document.hit(Point { x: 14.0, y: 4.0 }, 3.0), Some(0));
        assert!(document.undo());
        assert!(document.redo());
        assert_eq!(document.selected, None);
        Ok(())
    }
    #[test]
    fn rotated_imported_image_keeps_alpha_on_export() -> Result<()> {
        let base = Raster {
            width: 20,
            height: 20,
            pixels: [0, 255, 0, 255].repeat(400),
        };
        let imported = Arc::new(Raster {
            width: 2,
            height: 2,
            pixels: [0, 0, 255, 128].repeat(4),
        });
        let mark = Mark {
            shape: Shape::Image(
                Point { x: 4.0, y: 6.0 },
                Point { x: 12.0, y: 10.0 },
                imported,
            ),
            color: 0,
            width: 1.0,
            opacity: 1.0,
            angle: std::f32::consts::FRAC_PI_2,
        };
        let result = flatten(&base, &[mark], Point::default())?;
        let pixel = &result.pixels[(8 * 20 + 8) * 4..(8 * 20 + 8) * 4 + 4];
        assert!((125..=130).contains(&pixel[2]) && (125..=130).contains(&pixel[1]));
        assert_eq!(
            &base.pixels[(8 * 20 + 8) * 4..(8 * 20 + 8) * 4 + 4],
            &[0, 255, 0, 255]
        );
        Ok(())
    }
    #[test]
    fn imported_image_alpha_resizes_and_remains_editable() -> Result<()> {
        let base = Raster {
            width: 10,
            height: 10,
            pixels: [255, 0, 0, 255].repeat(100),
        };
        let imported = Arc::new(Raster {
            width: 2,
            height: 2,
            pixels: [0, 0, 255, 128].repeat(4),
        });
        let mut document = Document::default();
        document.add(Mark {
            shape: Shape::Image(Point { x: 2.0, y: 2.0 }, Point { x: 6.0, y: 6.0 }, imported),
            color: 0,
            width: 1.0,
            opacity: 1.0,
            angle: 0.0,
        })?;
        assert_eq!(document.asset_bytes(), 16);
        let result = flatten(&base, &document.marks, Point::default())?;
        assert_eq!(
            result.pixels[(3 * 10 + 3) * 4..(3 * 10 + 3) * 4 + 4],
            [127, 0, 128, 255]
        );
        assert_eq!(result.pixels[0..4], [255, 0, 0, 255]);
        assert_eq!(
            base.pixels[(3 * 10 + 3) * 4..(3 * 10 + 3) * 4 + 4],
            [255, 0, 0, 255]
        );
        assert!(document.hit(Point { x: 3.0, y: 3.0 }, 0.0).is_some());
        assert!(document.undo());
        assert!(document.marks.is_empty());
        assert_eq!(document.asset_bytes(), 16);
        assert!(document.redo());
        assert_eq!(document.asset_bytes(), 16);
        Ok(())
    }
    #[test]
    fn crop_changes_export_dimensions_but_keeps_source_and_history() -> Result<()> {
        let source = Raster {
            width: 20,
            height: 16,
            pixels: [1, 2, 3, 255].repeat(320),
        };
        let mut doc = Document::default();
        let mut mark = Mark::from_tool(Control::Crop, Point { x: 3.0, y: 2.0 }, 0, 1.0).unwrap();
        mark.update(Point { x: 17.0, y: 11.0 });
        assert!(mark.finish());
        doc.add(mark)?;
        let image = flatten(&source, &doc.marks, Point::default())?;
        assert_eq!((image.width, image.height), (14, 9));
        assert_eq!(
            &image.pixels[0..4],
            &source.pixels[(2 * 20 + 3) * 4..(2 * 20 + 3) * 4 + 4]
        );
        assert!(doc.undo());
        assert!(doc.crop().is_none());
        assert!(doc.redo());
        assert_eq!(
            doc.crop_pixels(20, 16, Point::default()),
            Some((3, 2, 17, 11))
        );
        assert_eq!((source.width, source.height), (20, 16));
        Ok(())
    }
    #[test]
    fn spotlight_keeps_focus_and_does_not_modify_source() -> Result<()> {
        let source = Raster {
            width: 20,
            height: 20,
            pixels: [255, 0, 0, 255].repeat(400),
        };
        let mut mark =
            Mark::from_tool(Control::Spotlight, Point { x: 5.0, y: 5.0 }, 0xffffff, 3.0).unwrap();
        mark.update(Point { x: 15.0, y: 15.0 });
        assert!(mark.finish());
        let result = flatten(&source, &[mark], Point::default())?;
        assert_eq!(
            &result.pixels[(10 * 20 + 10) * 4..(10 * 20 + 10) * 4 + 4],
            &[255, 0, 0, 255]
        );
        assert!(result.pixels[0] < 130);
        assert_eq!(source.pixels[0], 255);
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
