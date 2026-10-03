//! Chrome is in DIPs; image coordinates are independent of zoom and display DPI.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}
impl Rect {
    pub fn contains(self, x: f32, y: f32) -> bool {
        x >= self.x && y >= self.y && x < self.x + self.w && y < self.y + self.h
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Control {
    Crop,
    AddImage,
    Background,
    Move,
    Rectangle,
    Fill,
    Ellipse,
    Line,
    Arrow,
    Text,
    Pixelate,
    Spotlight,
    Counter,
    Pencil,
    Highlighter,
    Color,
    Stroke,
    Style,
    Save,
    Minimize,
    Maximize,
    Close,
    Zoom,
    Drag,
    Present,
    Share,
    Pin,
    Copy,
    Upload,
}
impl Control {
    pub fn enabled(self) -> bool {
        matches!(
            self,
            Self::Move
                | Self::Rectangle
                | Self::Fill
                | Self::Ellipse
                | Self::Line
                | Self::Arrow
                | Self::Pencil
                | Self::Highlighter
                | Self::Pixelate
                | Self::Stroke
                | Self::Background
                | Self::Color
                | Self::Save
                | Self::Minimize
                | Self::Maximize
                | Self::Close
                | Self::Zoom
                | Self::Drag
                | Self::Copy
        )
    }
    pub fn label(self) -> &'static str {
        match self {
            Self::Crop => "Crop",
            Self::AddImage => "Add image",
            Self::Background => "Background",
            Self::Move => "Move / pan (hold middle mouse button)",
            Self::Rectangle => "Rectangle",
            Self::Fill => "Filled rectangle",
            Self::Ellipse => "Ellipse",
            Self::Line => "Line",
            Self::Arrow => "Arrow",
            Self::Text => "Text",
            Self::Pixelate => {
                "Opaque content-independent mosaic redaction (not blur; original capture remains)"
            }
            Self::Spotlight => "Spotlight",
            Self::Counter => "Counter",
            Self::Pencil => "Pencil (smoothed freehand stroke)",
            Self::Highlighter => {
                "Manual translucent highlight (drag a rectangle; no text snapping)"
            }
            Self::Color => "Annotation colors and Windows custom color picker",
            Self::Stroke => "Annotation stroke width",
            Self::Style => "Tool style",
            Self::Save => "Save as... (Ctrl+S)",
            Self::Minimize => "Minimize window",
            Self::Maximize => "Maximize or restore window",
            Self::Close => "Close editor (Ctrl+W)",
            Self::Zoom => "Zoom",
            Self::Drag => "Drag Me",
            Self::Present => "Presentation - not available yet",
            Self::Share => "Share - not available yet",
            Self::Pin => "Pin - not available yet",
            Self::Copy => "Copy image (Ctrl+Shift+C)",
            Self::Upload => "Cloud upload (not available)",
        }
    }
}

pub const TOP: f32 = 48.0;
pub const BOTTOM: f32 = 48.0;
pub const MIN_WIDTH: f32 = 760.0;
pub const MIN_HEIGHT: f32 = 460.0;
pub const PRESET_COLORS: [u32; 10] = [
    0x000000, 0xf92d3a, 0xfe8101, 0xffde00, 0x37d147, 0x28c8bb, 0x006dfd, 0x7f47ff, 0xfd265f,
    0xffffff,
];

pub struct Layout {
    pub width: f32,
    pub height: f32,
    pub canvas: Rect,
    pub controls: Vec<(Control, Rect)>,
}
impl Layout {
    fn tool_origin(width: f32) -> f32 {
        // Center the drawing strip together with its contextual properties,
        // as seen in markup.mp4. Reserve utilities at left and Save/window
        // controls at right. Narrow windows omit the properties instead.
        let span = if width >= 870.0 { 484.0 } else { 348.0 };
        ((width - span) / 2.0).max(132.0).min(width - 242.0 - span)
    }
    pub fn tool_strip_rect(&self) -> Option<Rect> {
        let first = self.rect(Control::Move)?;
        let last = self.rect(Control::Highlighter)?;
        Some(Rect {
            x: first.x - 4.0,
            y: first.y,
            w: last.x + last.w + 4.0 - (first.x - 4.0),
            h: first.h,
        })
    }
    pub fn new(width: f32, height: f32) -> Self {
        let mut controls = Vec::new();
        let tool_origin = Self::tool_origin(width);
        for (i, control) in [Control::Crop, Control::AddImage, Control::Background]
            .into_iter()
            .enumerate()
        {
            controls.push((
                control,
                Rect {
                    x: 12.0 + i as f32 * 38.0,
                    y: 8.0,
                    w: 32.0,
                    h: 32.0,
                },
            ));
        }
        for (i, control) in [
            Control::Move,
            Control::Rectangle,
            Control::Fill,
            Control::Ellipse,
            Control::Line,
            Control::Arrow,
            Control::Text,
            Control::Pixelate,
            Control::Spotlight,
            Control::Counter,
            Control::Pencil,
            Control::Highlighter,
        ]
        .into_iter()
        .enumerate()
        {
            controls.push((
                control,
                Rect {
                    x: tool_origin + i as f32 * 29.0,
                    y: 8.0,
                    w: 28.0,
                    h: 32.0,
                },
            ));
        }
        for (i, control) in [Control::Color, Control::Stroke, Control::Style]
            .into_iter()
            .enumerate()
        {
            controls.push((
                control,
                Rect {
                    x: tool_origin + 362.0 + i as f32 * 42.0,
                    y: 8.0,
                    w: 38.0,
                    h: 32.0,
                },
            ));
        }
        controls.push((
            Control::Save,
            Rect {
                x: width - 234.0,
                y: 8.0,
                w: 96.0,
                h: 32.0,
            },
        ));
        for (i, control) in [Control::Minimize, Control::Maximize, Control::Close]
            .into_iter()
            .enumerate()
        {
            controls.push((
                control,
                Rect {
                    x: width - 126.0 + i as f32 * 42.0,
                    y: 0.0,
                    w: 42.0,
                    h: TOP,
                },
            ));
        }
        let y = height - BOTTOM + 10.0;
        controls.extend([
            (
                Control::Zoom,
                Rect {
                    x: 12.0,
                    y,
                    w: 88.0,
                    h: 28.0,
                },
            ),
            (
                Control::Drag,
                Rect {
                    x: width / 2.0 - 50.0,
                    y,
                    w: 100.0,
                    h: 28.0,
                },
            ),
            (
                Control::Present,
                Rect {
                    x: width - 170.0,
                    y,
                    w: 28.0,
                    h: 28.0,
                },
            ),
            (
                Control::Share,
                Rect {
                    x: width - 138.0,
                    y,
                    w: 28.0,
                    h: 28.0,
                },
            ),
            (
                Control::Pin,
                Rect {
                    x: width - 106.0,
                    y,
                    w: 28.0,
                    h: 28.0,
                },
            ),
            (
                Control::Copy,
                Rect {
                    x: width - 74.0,
                    y,
                    w: 28.0,
                    h: 28.0,
                },
            ),
            (
                Control::Upload,
                Rect {
                    x: width - 42.0,
                    y,
                    w: 28.0,
                    h: 28.0,
                },
            ),
        ]);
        // At unexpectedly small sizes, omit entire targets instead of overlapping.
        controls
            .retain(|(_, r)| r.x >= 0.0 && r.x + r.w <= width && r.y >= 0.0 && r.y + r.h <= height);
        if width < 870.0 {
            controls.retain(|(control, _)| {
                !matches!(control, Control::Color | Control::Stroke | Control::Style)
            });
        }
        if width < MIN_WIDTH {
            controls.retain(|(c, _)| {
                matches!(
                    c,
                    Control::Save
                        | Control::Minimize
                        | Control::Maximize
                        | Control::Close
                        | Control::Zoom
                        | Control::Drag
                        | Control::Present
                        | Control::Share
                        | Control::Pin
                        | Control::Copy
                        | Control::Upload
                )
            });
        }
        Self {
            width,
            height,
            canvas: Rect {
                x: 0.0,
                y: TOP,
                w: width,
                h: (height - TOP - BOTTOM).max(0.0),
            },
            controls,
        }
    }
    pub fn palette_rect(&self) -> Option<Rect> {
        let color = self.rect(Control::Color)?;
        Some(Rect {
            x: (color.x + color.w / 2.0 - 27.0)
                .min(self.width - 54.0)
                .max(0.0),
            y: TOP + 2.0,
            w: 54.0,
            h: 360.0,
        })
    }
    pub fn palette_hit(&self, x: f32, y: f32) -> Option<usize> {
        let r = self.palette_rect()?;
        if !r.contains(x, y) {
            return None;
        }
        let row = ((y - r.y - 4.0) / 32.0).floor() as isize;
        (0..=10).contains(&row).then_some(row as usize)
    }
    pub fn hit(&self, x: f32, y: f32) -> Option<Control> {
        self.controls
            .iter()
            .find(|(_, r)| r.contains(x, y))
            .map(|(c, _)| *c)
    }
    pub fn rect(&self, control: Control) -> Option<Rect> {
        self.controls
            .iter()
            .find(|(c, _)| *c == control)
            .map(|(_, r)| *r)
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Zoom {
    Fit,
    Scale(f32),
}
#[derive(Clone, Copy, Debug)]
pub struct View {
    pub zoom: Zoom,
    pub pan: (f32, f32),
    pub fit_limit: f32,
}
impl Default for View {
    fn default() -> Self {
        Self {
            zoom: Zoom::Fit,
            pan: (0.0, 0.0),
            fit_limit: 1.0,
        }
    }
}
impl View {
    pub fn scale(&self, canvas: Rect, width: u32, height: u32) -> f32 {
        match self.zoom {
            Zoom::Fit => ((canvas.w - 32.0).max(1.0) / width as f32)
                .min((canvas.h - 32.0).max(1.0) / height as f32)
                .min(self.fit_limit),
            Zoom::Scale(s) => s,
        }
    }
    pub fn image_rect(&self, canvas: Rect, width: u32, height: u32) -> Rect {
        let scale = self.scale(canvas, width, height);
        let (w, h) = (width as f32 * scale, height as f32 * scale);
        Rect {
            x: canvas.x + (canvas.w - w) / 2.0 + self.pan.0,
            y: canvas.y + (canvas.h - h) / 2.0 + self.pan.1,
            w,
            h,
        }
    }
    pub fn zoom_at(
        &mut self,
        scale: f32,
        anchor: (f32, f32),
        canvas: Rect,
        width: u32,
        height: u32,
    ) {
        let old = self.image_rect(canvas, width, height);
        let u = (anchor.0 - old.x) / old.w;
        let v = (anchor.1 - old.y) / old.h;
        self.zoom = Zoom::Scale(scale.clamp(0.1, 8.0));
        self.pan = (0.0, 0.0);
        let next = self.image_rect(canvas, width, height);
        self.pan = (
            anchor.0 - next.x - u * next.w,
            anchor.1 - next.y - v * next.h,
        );
        self.clamp_pan(canvas, width, height);
    }
    pub fn clamp_pan(&mut self, canvas: Rect, width: u32, height: u32) {
        let scale = self.scale(canvas, width, height);
        let x = ((width as f32 * scale - canvas.w) / 2.0 + 32.0).max(0.0);
        let y = ((height as f32 * scale - canvas.h) / 2.0 + 32.0).max(0.0);
        self.pan.0 = self.pan.0.clamp(-x, x);
        self.pan.1 = self.pan.1.clamp(-y, y);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn controls_never_overlap_and_hit_tests_share_dip_geometry() {
        for scale in [1.0, 1.25, 1.5, 2.0] {
            for width in [MIN_WIDTH, 960.0, 1400.0] {
                let l = Layout::new(width, 600.0);
                for (i, (c, r)) in l.controls.iter().enumerate() {
                    assert_eq!(
                        l.hit(
                            (r.x + r.w / 2.0) * scale / scale,
                            (r.y + r.h / 2.0) * scale / scale
                        ),
                        Some(*c)
                    );
                    for (_, other) in &l.controls[i + 1..] {
                        assert!(
                            r.x + r.w <= other.x
                                || other.x + other.w <= r.x
                                || r.y + r.h <= other.y
                                || other.y + other.h <= r.y
                        );
                    }
                }
            }
        }
    }
    #[test]
    fn drawing_tools_and_properties_are_centered_when_space_allows() {
        for width in [1040.0, 1200.0, 1400.0] {
            let l = Layout::new(width, 700.0);
            let first = l.rect(Control::Move).unwrap();
            let last = l.rect(Control::Style).unwrap();
            assert!(((first.x + last.x + last.w) / 2.0 - width / 2.0).abs() < 0.01);
            assert!(first.x >= l.rect(Control::Background).unwrap().x + 44.0);
            assert!(last.x + last.w + 8.0 <= l.rect(Control::Save).unwrap().x);
        }
        for width in [MIN_WIDTH, 800.0, 870.0, 960.0] {
            let l = Layout::new(width, 700.0);
            let last = l
                .rect(Control::Style)
                .or_else(|| l.rect(Control::Highlighter))
                .unwrap();
            assert!(last.x + last.w + 8.0 <= l.rect(Control::Save).unwrap().x);
            assert!(l.rect(Control::Move).unwrap().x >= 132.0);
        }
    }
    #[test]
    fn custom_caption_controls_keep_clear_tool_and_drag_regions() {
        for width in [MIN_WIDTH, 800.0, 960.0, 1200.0] {
            let layout = Layout::new(width, 640.0);
            assert_eq!(layout.hit(width - 21.0, 24.0), Some(Control::Close));
            assert_eq!(layout.hit(width - 63.0, 24.0), Some(Control::Maximize));
            assert_eq!(layout.hit(width - 105.0, 24.0), Some(Control::Minimize));
            assert!(layout.hit(width - 130.0, 24.0).is_none());
            assert_eq!(layout.hit(width - 186.0, 24.0), Some(Control::Save));
        }
    }
    #[test]
    fn palette_geometry_and_hit_testing_stay_in_client_at_minimum_size() {
        assert!(Layout::new(MIN_WIDTH, MIN_HEIGHT).palette_rect().is_none());
        for width in [870.0, 960.0, 1600.0] {
            let l = Layout::new(width, MIN_HEIGHT);
            let p = l.palette_rect().unwrap();
            assert!(p.x >= 0.0 && p.x + p.w <= width);
            assert!(p.y + p.h <= MIN_HEIGHT - BOTTOM);
            for i in 0..=10 {
                assert_eq!(
                    l.palette_hit(p.x + 24.0, p.y + 20.0 + i as f32 * 32.0),
                    Some(i)
                );
            }
            assert_eq!(l.palette_hit(p.x - 1.0, p.y + 20.0), None);
        }
    }
    #[test]
    fn fit_preserves_full_image_aspect_and_does_not_upscale() {
        let c = Layout::new(960.0, 600.0).canvas;
        for (w, h) in [(350, 200), (200, 350), (4000, 10), (10, 4000)] {
            let v = View::default();
            let r = v.image_rect(c, w, h);
            assert!((r.w / r.h - w as f32 / h as f32).abs() < 0.001);
            assert!(r.x >= c.x && r.y >= c.y && r.x + r.w <= c.x + c.w && r.y + r.h <= c.y + c.h);
            assert!(v.scale(c, w, h) <= 1.0);
        }
    }
    #[test]
    fn pointer_anchored_zoom_and_pan_are_bounded() {
        let c = Layout::new(960.0, 600.0).canvas;
        let mut v = View {
            zoom: Zoom::Scale(1.0),
            pan: (0.0, 0.0),
            fit_limit: 1.0,
        };
        let anchor = (510.0, 300.0);
        let before = v.image_rect(c, 1600, 1200);
        let u = (anchor.0 - before.x) / before.w;
        v.zoom_at(2.0, anchor, c, 1600, 1200);
        let after = v.image_rect(c, 1600, 1200);
        assert!(((anchor.0 - after.x) / after.w - u).abs() < 0.0001);
        v.pan = (1e6, -1e6);
        v.clamp_pan(c, 1600, 1200);
        assert!(v.pan.0 < 2000.0 && v.pan.1 > -2000.0);
    }
}
