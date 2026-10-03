use super::{
    background::{self, Background, Slider, Style},
    document::{self, Document, Mark, Point, Shape},
    layout::{Control, Layout, PRESET_COLORS, Rect, View},
};
use crate::storage::Raster;
use anyhow::{Context, Result};
use windows::{
    Win32::{
        Foundation::HWND,
        Graphics::{
            Direct2D::{Common::*, *},
            DirectWrite::*,
            Dxgi::Common::DXGI_FORMAT_B8G8R8A8_UNORM,
        },
    },
    core::w,
};
use windows_numerics::Vector2;

pub struct ChromeState<'a> {
    pub source: Option<&'a Raster>,
    pub document: &'a Document,
    pub pending: Option<&'a Mark>,
    pub active_tool: Control,
    pub pill_center: f32,
    pub stroke_width: f32,
    pub dark: bool,
    pub hovered: Option<Control>,
    pub hover_levels: &'a [(Control, f32)],
    pub focused: Option<Control>,
    pub palette_open: bool,
    pub selected_color: usize,
    pub custom_color: u32,
    pub maximized: bool,
}

pub struct Renderer {
    target: ID2D1HwndRenderTarget,
    brush: ID2D1SolidColorBrush,
    font: IDWriteTextFormat,
    label: IDWriteTextFormat,
    large: IDWriteTextFormat,
    bitmap: Option<ID2D1Bitmap>,
    swatches: Vec<ID2D1Bitmap>,
}
fn rect(r: Rect) -> D2D_RECT_F {
    D2D_RECT_F {
        left: r.x,
        top: r.y,
        right: r.x + r.w,
        bottom: r.y + r.h,
    }
}
fn color(rgb: u32) -> D2D1_COLOR_F {
    D2D1_COLOR_F {
        r: ((rgb >> 16) & 255) as f32 / 255.0,
        g: ((rgb >> 8) & 255) as f32 / 255.0,
        b: (rgb & 255) as f32 / 255.0,
        a: 1.0,
    }
}

pub fn premultiply(pixels: &[u8]) -> Vec<u8> {
    pixels
        .chunks_exact(4)
        .flat_map(|p| {
            let a = u32::from(p[3]);
            [
                ((u32::from(p[0]) * a + 127) / 255) as u8,
                ((u32::from(p[1]) * a + 127) / 255) as u8,
                ((u32::from(p[2]) * a + 127) / 255) as u8,
                p[3],
            ]
        })
        .collect()
}

impl Renderer {
    pub fn new(
        hwnd: HWND,
        width: u32,
        height: u32,
        dpi: u32,
        image: Option<&Raster>,
    ) -> Result<Self> {
        unsafe {
            let factory: ID2D1Factory = D2D1CreateFactory(D2D1_FACTORY_TYPE_SINGLE_THREADED, None)?;
            let target = factory.CreateHwndRenderTarget(
                &D2D1_RENDER_TARGET_PROPERTIES {
                    pixelFormat: D2D1_PIXEL_FORMAT {
                        format: DXGI_FORMAT_B8G8R8A8_UNORM,
                        alphaMode: D2D1_ALPHA_MODE_IGNORE,
                    },
                    dpiX: dpi as f32,
                    dpiY: dpi as f32,
                    ..Default::default()
                },
                &D2D1_HWND_RENDER_TARGET_PROPERTIES {
                    hwnd,
                    pixelSize: D2D_SIZE_U {
                        width: width.max(1),
                        height: height.max(1),
                    },
                    presentOptions: D2D1_PRESENT_OPTIONS_NONE,
                },
            )?;
            let brush = target.CreateSolidColorBrush(&color(0x24252a), None)?;
            let write: IDWriteFactory = DWriteCreateFactory(DWRITE_FACTORY_TYPE_SHARED)?;
            let make_font = |size| -> Result<IDWriteTextFormat> {
                let font = write.CreateTextFormat(
                    w!("Segoe UI"),
                    None,
                    DWRITE_FONT_WEIGHT_MEDIUM,
                    DWRITE_FONT_STYLE_NORMAL,
                    DWRITE_FONT_STRETCH_NORMAL,
                    size,
                    w!("en-US"),
                )?;
                font.SetTextAlignment(DWRITE_TEXT_ALIGNMENT_CENTER)?;
                font.SetParagraphAlignment(DWRITE_PARAGRAPH_ALIGNMENT_CENTER)?;
                Ok(font)
            };
            let label = make_font(12.0)?;
            label.SetTextAlignment(DWRITE_TEXT_ALIGNMENT_LEADING)?;
            let mut swatches = Vec::with_capacity(background::GRADIENTS.len() + 3);
            for index in 0..background::GRADIENTS.len() + 3 {
                let mut pixels = vec![0u8; 38 * 38 * 4];
                for y in 0..38 {
                    for x in 0..38 {
                        let rgb = if index < background::GRADIENTS.len() {
                            background::gradient_at(index, x as f32 / 37.0, y as f32 / 37.0)
                        } else {
                            background::wallpaper_at(
                                index - background::GRADIENTS.len(),
                                x as f32 / 37.0,
                                y as f32 / 37.0,
                            )
                        };
                        let offset = (y * 38 + x) * 4;
                        pixels[offset..offset + 4].copy_from_slice(&[rgb[2], rgb[1], rgb[0], 255]);
                    }
                }
                swatches.push(target.CreateBitmap(
                    D2D_SIZE_U {
                        width: 38,
                        height: 38,
                    },
                    Some(pixels.as_ptr().cast()),
                    38 * 4,
                    &D2D1_BITMAP_PROPERTIES {
                        pixelFormat: D2D1_PIXEL_FORMAT {
                            format: DXGI_FORMAT_B8G8R8A8_UNORM,
                            alphaMode: D2D1_ALPHA_MODE_PREMULTIPLIED,
                        },
                        dpiX: 96.0,
                        dpiY: 96.0,
                    },
                )?);
            }
            let mut renderer = Self {
                target,
                brush,
                swatches,
                font: make_font(12.0)?,
                label,
                large: make_font(16.0)?,
                bitmap: None,
            };
            if let Some(image) = image {
                renderer.set_image(image)?;
            }
            Ok(renderer)
        }
    }
    pub fn set_image(&mut self, image: &Raster) -> Result<()> {
        let pixels = premultiply(&image.pixels);
        unsafe {
            let maximum = self.target.GetMaximumBitmapSize();
            anyhow::ensure!(
                image.width <= maximum && image.height <= maximum,
                "Image exceeds this graphics device's {maximum}-pixel bitmap limit"
            );
            self.bitmap = Some(self.target.CreateBitmap(
                D2D_SIZE_U {
                    width: image.width,
                    height: image.height,
                },
                Some(pixels.as_ptr().cast()),
                image.width * 4,
                &D2D1_BITMAP_PROPERTIES {
                    pixelFormat: D2D1_PIXEL_FORMAT {
                        format: DXGI_FORMAT_B8G8R8A8_UNORM,
                        alphaMode: D2D1_ALPHA_MODE_PREMULTIPLIED,
                    },
                    dpiX: 96.0,
                    dpiY: 96.0,
                },
            )?);
        }
        Ok(())
    }
    pub fn resize(&self, width: u32, height: u32, dpi: u32) -> Result<()> {
        unsafe {
            self.target.SetDpi(dpi as f32, dpi as f32);
            self.target.Resize(&D2D_SIZE_U {
                width: width.max(1),
                height: height.max(1),
            })?;
        }
        Ok(())
    }
    fn soft_chrome(&self, y: f32, width: f32, height: f32) {
        // Light lavender-to-neutral material seen in the supplied markup video.
        // Draw in bands to avoid introducing a device-dependent acrylic layer.
        for band in 0..48 {
            let x = width * band as f32 / 48.0;
            let next = width * (band + 1) as f32 / 48.0;
            let t = band as f32 / 47.0;
            let from = [224.0, 212.0, 224.0];
            let to = [217.0, 217.0, 217.0];
            let r = (from[0] + (to[0] - from[0]) * t) as u32;
            // Slight lavender is retained across the gradient.
            let g = (from[1] + (to[1] - from[1]) * t) as u32;
            let b = (from[2] + (to[2] - from[2]) * t) as u32;
            self.fill(
                Rect {
                    x,
                    y,
                    w: next - x + 1.0,
                    h: height,
                },
                (r << 16) | (g << 8) | b,
            );
        }
    }
    fn fill(&self, r: Rect, rgb: u32) {
        self.fill_opacity(r, rgb, 1.0);
    }
    fn fill_opacity(&self, r: Rect, rgb: u32, opacity: f32) {
        unsafe {
            let mut tint = color(rgb);
            tint.a = opacity;
            self.brush.SetColor(&tint);
            self.target.FillRectangle(&rect(r), &self.brush);
        }
    }
    fn pill(&self, r: Rect, rgb: u32, radius: f32) {
        self.pill_opacity(r, rgb, radius, 1.0);
    }
    fn pill_opacity(&self, r: Rect, rgb: u32, radius: f32, opacity: f32) {
        unsafe {
            let mut tint = color(rgb);
            tint.a = opacity;
            self.brush.SetColor(&tint);
            self.target.FillRoundedRectangle(
                &D2D1_ROUNDED_RECT {
                    rect: rect(r),
                    radiusX: radius,
                    radiusY: radius,
                },
                &self.brush,
            );
        }
    }
    fn line(&self, a: (f32, f32), b: (f32, f32), rgb: u32, width: f32) {
        unsafe {
            self.brush.SetColor(&color(rgb));
            self.target.DrawLine(
                Vector2 { X: a.0, Y: a.1 },
                Vector2 { X: b.0, Y: b.1 },
                &self.brush,
                width,
                None,
            );
        }
    }
    fn text(&self, text: &str, r: Rect, rgb: u32, large: bool) {
        unsafe {
            self.brush.SetColor(&color(rgb));
            let chars: Vec<u16> = text.encode_utf16().collect();
            self.target.DrawText(
                &chars,
                if large { &self.large } else { &self.font },
                &rect(r),
                &self.brush,
                D2D1_DRAW_TEXT_OPTIONS_CLIP,
                DWRITE_MEASURING_MODE_NATURAL,
            );
        }
    }
    fn label(&self, text: &str, r: Rect, rgb: u32) {
        unsafe {
            self.brush.SetColor(&color(rgb));
            let chars: Vec<u16> = text.encode_utf16().collect();
            self.target.DrawText(
                &chars,
                &self.label,
                &rect(r),
                &self.brush,
                D2D1_DRAW_TEXT_OPTIONS_CLIP,
                DWRITE_MEASURING_MODE_NATURAL,
            );
        }
    }
    fn outline(&self, r: Rect, rgb: u32, width: f32) {
        unsafe {
            self.brush.SetColor(&color(rgb));
            self.target
                .DrawRectangle(&rect(r), &self.brush, width, None);
        }
    }
    fn rounded_outline(&self, r: Rect, rgb: u32, radius: f32) {
        unsafe {
            self.brush.SetColor(&color(rgb));
            self.target.DrawRoundedRectangle(
                &D2D1_ROUNDED_RECT {
                    rect: rect(r),
                    radiusX: radius,
                    radiusY: radius,
                },
                &self.brush,
                1.0,
                None,
            );
        }
    }
    fn circle(&self, x: f32, y: f32, r: f32, rgb: u32, fill: bool) {
        unsafe {
            self.brush.SetColor(&color(rgb));
            let e = D2D1_ELLIPSE {
                point: Vector2 { X: x, Y: y },
                radiusX: r,
                radiusY: r,
            };
            if fill {
                self.target.FillEllipse(&e, &self.brush);
            } else {
                self.target.DrawEllipse(&e, &self.brush, 1.5, None);
            }
        }
    }
    fn icon(&self, control: Control, r: Rect, rgb: u32, drawing_color: u32) {
        let (x, y) = (r.x + r.w / 2.0, r.y + r.h / 2.0);
        let square = Rect {
            x: x - 5.5,
            y: y - 5.5,
            w: 11.0,
            h: 11.0,
        };
        match control {
            Control::Move => {
                self.line((x - 4.0, y - 6.0), (x - 4.0, y + 5.0), rgb, 2.0);
                self.line((x - 4.0, y - 6.0), (x + 4.0, y + 1.0), rgb, 2.0);
                self.line((x - 4.0, y + 5.0), (x + 4.0, y + 1.0), rgb, 1.5);
                self.line((x, y + 2.0), (x + 3.0, y + 7.0), rgb, 2.0);
            }
            Control::Rectangle | Control::Spotlight => {
                self.outline(square, rgb, 1.6);
                if control == Control::Spotlight {
                    self.outline(
                        Rect {
                            x: x - 3.0,
                            y: y - 3.0,
                            w: 6.0,
                            h: 6.0,
                        },
                        rgb,
                        1.0,
                    );
                }
            }
            Control::Fill => self.fill(square, rgb),
            Control::Ellipse => self.circle(x, y, 6.0, rgb, false),
            Control::Line | Control::Stroke => self.line(
                (x - 5.0, y + 5.0),
                (x + 5.0, y - 5.0),
                rgb,
                if control == Control::Stroke { 3.0 } else { 1.5 },
            ),
            Control::Arrow | Control::Style => {
                self.line((x + 5.5, y - 5.5), (x - 5.0, y + 5.0), rgb, 1.8);
                self.line((x - 5.0, y + 5.0), (x - 5.0, y - 1.0), rgb, 1.8);
                self.line((x - 5.0, y + 5.0), (x + 1.0, y + 5.0), rgb, 1.8);
            }
            Control::Text => self.text("A", r, rgb, true),
            Control::Pixelate => {
                for row in 0..3 {
                    for col in 0..3 {
                        self.fill(
                            Rect {
                                x: x - 6.0 + col as f32 * 4.0,
                                y: y - 6.0 + row as f32 * 4.0,
                                w: 4.0,
                                h: 4.0,
                            },
                            if (row + col) % 2 == 0 { rgb } else { 0x9da0aa },
                        );
                    }
                }
            }
            Control::Counter => {
                self.circle(x, y, 7.0, rgb, true);
                self.text("1", r, 0xffffff, false);
            }
            Control::Pencil => {
                self.line((x - 5.0, y + 3.5), (x + 3.0, y - 5.5), rgb, 1.8);
                self.line((x - 5.0, y + 3.5), (x - 4.0, y + 6.0), rgb, 1.8);
                self.line((x - 4.0, y + 6.0), (x - 1.5, y + 5.0), rgb, 1.8);
                self.line((x - 1.5, y + 5.0), (x + 6.0, y - 3.5), rgb, 1.8);
                self.line((x + 3.0, y - 5.5), (x + 6.0, y - 3.5), rgb, 1.8);
                self.line((x - 2.5, y + 1.0), (x + 1.0, y + 4.0), rgb, 1.4);
            }
            Control::Highlighter => {
                self.line((x - 5.5, y + 4.5), (x - 1.0, y - 5.0), rgb, 2.3);
                self.line((x - 1.0, y - 5.0), (x + 5.5, y + 4.5), rgb, 2.3);
                self.line((x - 3.5, y + 1.0), (x + 3.5, y + 1.0), rgb, 1.7);
                self.line((x - 6.5, y + 6.5), (x + 6.5, y + 6.5), rgb, 2.1);
            }
            Control::Crop => {
                self.line((x - 7.0, y - 4.0), (x + 4.0, y - 4.0), rgb, 1.8);
                self.line((x + 4.0, y - 4.0), (x + 4.0, y + 8.0), rgb, 1.8);
                self.line((x - 4.0, y - 8.0), (x - 4.0, y + 4.0), rgb, 1.8);
                self.line((x - 4.0, y + 4.0), (x + 8.0, y + 4.0), rgb, 1.8);
            }
            Control::AddImage | Control::Background => {
                self.outline(
                    Rect {
                        x: x - 7.0,
                        y: y - 5.0,
                        w: 14.0,
                        h: 10.0,
                    },
                    rgb,
                    1.5,
                );
                self.line((x - 5.0, y + 3.0), (x - 1.0, y - 1.0), rgb, 1.5);
                self.line((x - 1.0, y - 1.0), (x + 4.0, y + 3.0), rgb, 1.5);
                self.circle(x + 3.0, y - 2.0, 1.2, rgb, true);
                if control == Control::AddImage {
                    self.line((x + 5.0, y + 3.0), (x + 5.0, y + 8.0), rgb, 1.3);
                    self.line((x + 2.5, y + 5.5), (x + 7.5, y + 5.5), rgb, 1.3);
                }
            }
            Control::Minimize => self.line((x - 5.0, y - 0.5), (x + 5.0, y - 0.5), rgb, 1.5),
            Control::Maximize => self.outline(
                Rect {
                    x: x - 5.0,
                    y: y - 5.0,
                    w: 10.0,
                    h: 10.0,
                },
                rgb,
                1.1,
            ),
            Control::Close => {
                self.line((x - 5.0, y - 5.0), (x + 5.0, y + 5.0), rgb, 1.1);
                self.line((x + 5.0, y - 5.0), (x - 5.0, y + 5.0), rgb, 1.1);
            }
            Control::Color => {
                self.circle(x - 4.0, y, 6.0, drawing_color, true);
                self.line((x + 9.0, y - 2.0), (x + 12.0, y + 1.0), 0x817a86, 1.1);
                self.line((x + 12.0, y + 1.0), (x + 15.0, y - 2.0), 0x817a86, 1.1);
            }
            Control::Present => {
                self.circle(x, y, 5.0, rgb, false);
                self.line((x - 7.0, y + 6.0), (x - 2.0, y + 2.0), rgb, 1.5);
                self.line((x + 3.0, y - 3.0), (x + 7.0, y - 7.0), rgb, 1.5);
            }
            Control::Share => {
                self.outline(
                    Rect {
                        x: x - 6.0,
                        y: y - 2.0,
                        w: 12.0,
                        h: 9.0,
                    },
                    rgb,
                    1.5,
                );
                self.line((x, y + 1.0), (x, y - 7.0), rgb, 1.7);
                self.line((x, y - 7.0), (x - 3.0, y - 4.0), rgb, 1.7);
                self.line((x, y - 7.0), (x + 3.0, y - 4.0), rgb, 1.7);
            }
            Control::Pin => {
                self.line((x - 3.0, y - 5.0), (x + 5.0, y + 3.0), rgb, 3.0);
                self.line((x - 5.0, y + 5.0), (x + 2.0, y - 2.0), rgb, 1.5);
                self.line((x - 6.0, y + 8.0), (x - 3.0, y + 3.0), rgb, 1.6);
            }
            Control::Copy => {
                self.outline(
                    Rect {
                        x: x - 5.0,
                        y: y - 6.0,
                        w: 8.0,
                        h: 10.0,
                    },
                    rgb,
                    1.5,
                );
                self.line((x - 3.0, y + 6.0), (x + 6.0, y + 6.0), rgb, 1.5);
                self.line((x + 6.0, y + 6.0), (x + 6.0, y - 3.0), rgb, 1.5);
            }
            Control::Upload => {
                self.line((x, y + 5.0), (x, y - 5.0), rgb, 1.5);
                self.line((x, y - 5.0), (x - 4.0, y - 1.0), rgb, 1.5);
                self.line((x, y - 5.0), (x + 4.0, y - 1.0), rgb, 1.5);
                self.line((x - 6.0, y + 3.0), (x - 6.0, y + 7.0), rgb, 1.5);
                self.line((x - 6.0, y + 7.0), (x + 6.0, y + 7.0), rgb, 1.5);
                self.line((x + 6.0, y + 7.0), (x + 6.0, y + 3.0), rgb, 1.5);
            }
            _ => {}
        }
    }
    fn palette(&self, layout: &Layout, selected_color: usize, dark: bool) {
        let Some(r) = layout.palette_rect() else {
            return;
        };
        self.pill(
            Rect {
                x: r.x + 2.0,
                y: r.y + 5.0,
                w: r.w,
                h: r.h,
            },
            if dark { 0x141418 } else { 0xd0cad1 },
            18.0,
        );
        self.pill(r, if dark { 0x303036 } else { 0xf6f6f6 }, 17.0);
        self.rounded_outline(r, if dark { 0x686870 } else { 0xc8c7c9 }, 17.0);
        for (index, &swatch) in PRESET_COLORS.iter().enumerate() {
            let (x, y) = (r.x + r.w / 2.0, r.y + 20.0 + 32.0 * index as f32);
            if index == selected_color {
                self.circle(x, y, 14.5, if dark { 0x7cb897 } else { 0xb5d1be }, true);
                self.circle(x, y, 12.5, if dark { 0x303036 } else { 0xf6f6f6 }, true);
            }
            self.circle(x, y, 11.0, swatch, true);
            if index == 0 || index == 9 {
                self.circle(x, y, 11.0, if dark { 0xa8a8ae } else { 0xc7c7c7 }, false);
            }
        }
        let (x, y) = (r.x + r.w / 2.0, r.y + 20.0 + 32.0 * 10.0);
        if selected_color == PRESET_COLORS.len() {
            self.circle(x, y, 14.5, if dark { 0x7cb897 } else { 0xb5d1be }, true);
            self.circle(x, y, 12.5, if dark { 0x303036 } else { 0xf6f6f6 }, true);
        }
        for (i, hue) in [
            0xf92d3a, 0xfe8101, 0xffde00, 0x37d147, 0x28c8bb, 0x006dfd, 0x7f47ff, 0xfd265f,
        ]
        .into_iter()
        .enumerate()
        {
            let angle = std::f32::consts::TAU * i as f32 / 8.0;
            self.line(
                (x, y),
                (x + angle.cos() * 8.0, y + angle.sin() * 8.0),
                hue,
                4.0,
            );
        }
        self.circle(x, y, 10.0, 0x999399, false);
    }
    fn mark(&self, mark: &Mark, origin: Point, scale: f32) {
        let map = |p: Point| (origin.x + p.x * scale, origin.y + p.y * scale);
        let width = (mark.width * scale).max(0.5);
        let segment = |a: Point, b: Point| {
            let (a, b) = (map(a), map(b));
            self.line(a, b, mark.color, width);
            self.circle(a.0, a.1, width / 2.0, mark.color, true);
            self.circle(b.0, b.1, width / 2.0, mark.color, true);
        };
        match &mark.shape {
            Shape::Pencil(points) => {
                for pair in points.windows(2) {
                    segment(pair[0], pair[1]);
                }
            }
            Shape::Line(a, b) => segment(*a, *b),
            Shape::Arrow(a, b, control) => {
                let steps = (((b.x - a.x).hypot(b.y - a.y)) / 4.0).clamp(16.0, 128.0) as usize;
                let mut last = *a;
                for i in 1..=steps {
                    let next = document::curve(*a, *control, *b, i as f32 / steps as f32);
                    segment(last, next);
                    last = next;
                }
                let angle = (b.y - control.y).atan2(b.x - control.x);
                let len = (mark.width * 5.0).max(12.0);
                for direction in [-0.55_f32, 0.55] {
                    let end = Point {
                        x: b.x - len * (angle + direction).cos(),
                        y: b.y - len * (angle + direction).sin(),
                    };
                    segment(*b, end);
                }
            }
            Shape::Mosaic(a, b) => {
                unsafe {
                    self.target.SetAntialiasMode(D2D1_ANTIALIAS_MODE_ALIASED);
                }
                document::mosaic_tiles(*a, *b, |lo, hi, rgb| {
                    self.fill(
                        Rect {
                            x: origin.x + lo.x * scale,
                            y: origin.y + lo.y * scale,
                            w: (hi.x - lo.x) * scale,
                            h: (hi.y - lo.y) * scale,
                        },
                        rgb,
                    );
                });
                unsafe {
                    self.target
                        .SetAntialiasMode(D2D1_ANTIALIAS_MODE_PER_PRIMITIVE);
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
                let r = Rect {
                    x: origin.x + lo.x * scale,
                    y: origin.y + lo.y * scale,
                    w: (hi.x - lo.x) * scale,
                    h: (hi.y - lo.y) * scale,
                };
                if matches!(
                    mark.shape,
                    Shape::FilledRectangle(_, _) | Shape::Highlighter(_, _)
                ) {
                    self.fill_opacity(r, mark.color, mark.opacity);
                } else {
                    self.line((r.x, r.y), (r.x + r.w, r.y), mark.color, width);
                    self.line((r.x + r.w, r.y), (r.x + r.w, r.y + r.h), mark.color, width);
                    self.line((r.x + r.w, r.y + r.h), (r.x, r.y + r.h), mark.color, width);
                    self.line((r.x, r.y + r.h), (r.x, r.y), mark.color, width);
                }
            }
            Shape::Ellipse(a, b) => {
                let x = (a.x + b.x) / 2.0;
                let y = (a.y + b.y) / 2.0;
                unsafe {
                    self.brush.SetColor(&color(mark.color));
                    self.target.DrawEllipse(
                        &D2D1_ELLIPSE {
                            point: Vector2 {
                                X: origin.x + x * scale,
                                Y: origin.y + y * scale,
                            },
                            radiusX: (a.x - b.x).abs() * scale / 2.0,
                            radiusY: (a.y - b.y).abs() * scale / 2.0,
                        },
                        &self.brush,
                        width,
                        None,
                    );
                }
            }
        }
    }
    fn panel(&self, layout: &Layout, model: &Background, dark: bool) {
        if !model.open {
            return;
        }
        let top = background::PANEL_TOP;
        let base = top - model.scroll_y;
        let bottom = layout.height - background::PANEL_BOTTOM;
        unsafe {
            self.target.PushAxisAlignedClip(
                &rect(Rect {
                    x: 0.0,
                    y: top,
                    w: background::PANEL_WIDTH,
                    h: (bottom - top).max(0.0),
                }),
                D2D1_ANTIALIAS_MODE_ALIASED,
            );
        }
        let surface = if dark { 0x202024 } else { 0xf3f3f5 };
        let fg = if dark { 0xf2f2f5 } else { 0x202025 };
        let subtle = if dark { 0x9c9ca3 } else { 0x66666b };
        self.fill(
            Rect {
                x: 0.0,
                y: top,
                w: background::PANEL_WIDTH,
                h: (bottom - top).max(0.0),
            },
            surface,
        );
        self.line(
            (background::PANEL_WIDTH - 0.5, top),
            (background::PANEL_WIDTH - 0.5, bottom),
            if dark { 0x343439 } else { 0xd7d7da },
            1.0,
        );
        self.label(
            "Background Tool",
            Rect {
                x: 16.0,
                y: base + 8.0,
                w: 228.0,
                h: 27.0,
            },
            fg,
        );
        self.line(
            (16.0, base + 38.0),
            (244.0, base + 38.0),
            if dark { 0x414145 } else { 0xd5d5d8 },
            1.0,
        );
        let none = Rect {
            x: 16.0,
            y: base + 48.0,
            w: 228.0,
            h: 32.0,
        };
        self.pill(
            none,
            if model.style == Style::None {
                if dark { 0x45454a } else { 0xdedee4 }
            } else {
                surface
            },
            6.0,
        );
        self.label(
            "None",
            Rect {
                x: 25.0,
                y: none.y,
                w: 205.0,
                h: 32.0,
            },
            fg,
        );
        self.label(
            "Gradients",
            Rect {
                x: 16.0,
                y: base + 91.0,
                w: 142.0,
                h: 28.0,
            },
            fg,
        );
        self.text(
            if model.expanded { "⌄" } else { "›" },
            Rect {
                x: 217.0,
                y: base + 91.0,
                w: 26.0,
                h: 28.0,
            },
            subtle,
            false,
        );
        if model.expanded {
            for (i, _) in background::GRADIENTS.iter().enumerate() {
                let x = 16.0 + (i % 5) as f32 * 48.0;
                let y = base + 122.0 + (i / 5) as f32 * 48.0;
                unsafe {
                    self.target.DrawBitmap(
                        &self.swatches[i],
                        Some(&rect(Rect {
                            x,
                            y,
                            w: 38.0,
                            h: 38.0,
                        })),
                        1.0,
                        D2D1_BITMAP_INTERPOLATION_MODE_LINEAR,
                        None,
                    );
                }
                if model.style == Style::Gradient(i) {
                    self.rounded_outline(
                        Rect {
                            x: x - 2.0,
                            y: y - 2.0,
                            w: 42.0,
                            h: 42.0,
                        },
                        0x1596f9,
                        6.0,
                    );
                }
            }
        }
        let offset = if model.expanded { 0.0 } else { -192.0 };
        self.label(
            "Wallpapers",
            Rect {
                x: 16.0,
                y: base + 323.0 + offset,
                w: 228.0,
                h: 27.0,
            },
            fg,
        );
        for i in 0..3 {
            let x = 16.0 + i as f32 * 48.0;
            let y = base + 352.0 + offset;
            unsafe {
                self.target.DrawBitmap(
                    &self.swatches[background::GRADIENTS.len() + i],
                    Some(&rect(Rect {
                        x,
                        y,
                        w: 38.0,
                        h: 38.0,
                    })),
                    1.0,
                    D2D1_BITMAP_INTERPOLATION_MODE_LINEAR,
                    None,
                );
            }
            if model.style == Style::Wallpaper(i) {
                self.rounded_outline(
                    Rect {
                        x: x - 2.0,
                        y: y - 2.0,
                        w: 42.0,
                        h: 42.0,
                    },
                    0x1596f9,
                    7.0,
                );
            }
        }
        self.label(
            "Blurred",
            Rect {
                x: 16.0,
                y: base + 391.0 + offset,
                w: 228.0,
                h: 27.0,
            },
            fg,
        );
        for i in 0..3 {
            let x = 16.0 + i as f32 * 48.0;
            let y = base + 420.0 + offset;
            let c = [0xdadadc, 0x85858a, 0x303035][i];
            self.pill(
                Rect {
                    x,
                    y,
                    w: 38.0,
                    h: 38.0,
                },
                c,
                5.0,
            );
            if model.style == Style::Blurred(i) {
                self.rounded_outline(
                    Rect {
                        x: x - 2.0,
                        y: y - 2.0,
                        w: 42.0,
                        h: 42.0,
                    },
                    0x1596f9,
                    7.0,
                );
            }
        }
        self.label(
            "Plain Colors",
            Rect {
                x: 16.0,
                y: base + 466.0 + offset,
                w: 228.0,
                h: 27.0,
            },
            fg,
        );
        for (i, &c) in background::SOLIDS.iter().enumerate() {
            let x = 29.0 + (i % 9) as f32 * 26.0;
            let y = base + 505.0 + (i / 9) as f32 * 25.0 + offset;
            self.circle(x, y, 9.5, c, true);
            if model.style == Style::Solid(i) {
                self.circle(x, y, 11.5, 0x1596f9, false);
            }
        }
        self.line(
            (16.0, base + 559.0 + offset),
            (244.0, base + 559.0 + offset),
            if dark { 0x414145 } else { 0xd5d5d8 },
            1.0,
        );
        for (slider, name) in [
            (Slider::Padding, "Padding"),
            (Slider::Inset, "Inset"),
            (Slider::Shadow, "Shadow"),
            (Slider::Corners, "Corners"),
        ] {
            let r = model.slider_rect(slider);
            self.label(
                name,
                Rect {
                    x: r.x,
                    y: base + r.y - 31.0 + offset,
                    w: r.w,
                    h: 22.0,
                },
                fg,
            );
            self.pill(
                Rect {
                    x: r.x,
                    y: base + r.y + 7.0 + offset,
                    w: r.w,
                    h: 4.0,
                },
                if dark { 0x5b5b61 } else { 0xc6c6ca },
                2.0,
            );
            self.pill(
                Rect {
                    x: r.x,
                    y: base + r.y + 7.0 + offset,
                    w: r.w * model.slider_value(slider),
                    h: 4.0,
                },
                0x1596f9,
                2.0,
            );
            self.circle(
                r.x + r.w * model.slider_value(slider),
                base + r.y + 9.0 + offset,
                7.0,
                if dark { 0xffffff } else { 0xf8f8fa },
                true,
            );
        }
        self.label(
            "Auto-balance",
            Rect {
                x: 145.0,
                y: base + 623.0 + offset,
                w: 100.0,
                h: 26.0,
            },
            subtle,
        );
        self.pill(
            Rect {
                x: 212.0,
                y: base + 647.0 + offset,
                w: 32.0,
                h: 19.0,
            },
            if model.auto_balance {
                0x1596f9
            } else {
                0x606069
            },
            9.0,
        );
        self.circle(
            if model.auto_balance { 234.0 } else { 222.0 },
            base + 656.5 + offset,
            7.0,
            0xffffff,
            true,
        );
        unsafe {
            self.target.PopAxisAlignedClip();
        }
    }
    pub fn draw(
        &self,
        layout: &Layout,
        view: &View,
        image: Option<&Raster>,
        background: &Background,
        chrome: ChromeState<'_>,
    ) -> Result<()> {
        let ChromeState {
            source,
            document,
            pending,
            active_tool,
            pill_center,
            stroke_width,
            dark,
            hovered,
            hover_levels,
            focused,
            palette_open,
            selected_color,
            custom_color,
            maximized,
        } = chrome;
        let (canvas, fg, muted, pill, group) = if dark {
            (0x19191d, 0xf3f3f5, 0x797982, 0x3b3b43, 0x222226)
        } else {
            (0xffffff, 0x242127, 0x79737e, 0xffffff, 0xded5df)
        };
        unsafe {
            self.target.BeginDraw();
            self.target.Clear(Some(&color(canvas)));
        }
        if dark {
            self.fill(
                Rect {
                    x: 0.0,
                    y: 0.0,
                    w: layout.width,
                    h: 48.0,
                },
                0x28282d,
            );
            self.fill(
                Rect {
                    x: 0.0,
                    y: layout.height - 48.0,
                    w: layout.width,
                    h: 48.0,
                },
                0x28282d,
            );
        } else {
            self.soft_chrome(0.0, layout.width, 48.0);
            self.soft_chrome(layout.height - 48.0, layout.width, 48.0);
        }
        if let Some(strip) = layout.tool_strip_rect() {
            self.pill(strip, group, 16.0);
            let first = strip.x + 4.0;
            for offset in [28.5, 86.5, 144.5, 231.5, 289.5] {
                let x = first + offset;
                self.line(
                    (x, 17.0),
                    (x, 31.0),
                    if dark { 0x454349 } else { 0xc5bbc7 },
                    1.0,
                );
            }
        }
        unsafe {
            self.target
                .PushAxisAlignedClip(&rect(layout.canvas), D2D1_ANTIALIAS_MODE_ALIASED);
        }
        if let Some(image) = image {
            let r = view.image_rect(layout.canvas, image.width, image.height);
            let visible = Rect {
                x: r.x.max(layout.canvas.x),
                y: r.y.max(layout.canvas.y),
                w: (r.x + r.w).min(layout.canvas.x + layout.canvas.w) - r.x.max(layout.canvas.x),
                h: (r.y + r.h).min(layout.canvas.y + layout.canvas.h) - r.y.max(layout.canvas.y),
            };
            // Limit checkerboard work to the visible viewport, even at 800% zoom.
            if visible.w > 0.0 && visible.h > 0.0 {
                self.fill(visible, if dark { 0x424249 } else { 0xffffff });
                for row in 0..(visible.h / 12.0).ceil() as i32 {
                    for col in 0..(visible.w / 12.0).ceil() as i32 {
                        if (row + col) % 2 == 0 {
                            self.fill(
                                Rect {
                                    x: visible.x + col as f32 * 12.0,
                                    y: visible.y + row as f32 * 12.0,
                                    w: 12.0_f32.min(visible.w - col as f32 * 12.0),
                                    h: 12.0_f32.min(visible.h - row as f32 * 12.0),
                                },
                                if dark { 0x36363d } else { 0xe7e7ec },
                            );
                        }
                    }
                }
            }
            if let Some(bitmap) = &self.bitmap {
                unsafe {
                    self.target.DrawBitmap(
                        bitmap,
                        Some(&rect(r)),
                        1.0,
                        if !background.selected()
                            && view.scale(layout.canvas, image.width, image.height) >= 1.0
                        {
                            D2D1_BITMAP_INTERPOLATION_MODE_NEAREST_NEIGHBOR
                        } else {
                            D2D1_BITMAP_INTERPOLATION_MODE_LINEAR
                        },
                        None,
                    );
                }
            }
            if let Some(source) = source {
                let (ox, oy) = background::source_origin(source.width, source.height, background)?;
                let scale = view.scale(layout.canvas, image.width, image.height);
                let origin = Point {
                    x: r.x + ox as f32 * scale,
                    y: r.y + oy as f32 * scale,
                };
                for mark in &document.marks {
                    self.mark(mark, origin, scale);
                }
                if let Some(mark) = pending {
                    self.mark(mark, origin, scale);
                }
                if let Some(index) = document.selected
                    && active_tool == Control::Move
                    && let Some(mark) = document.marks.get(index)
                {
                    let (lo, hi) = mark.bounds();
                    self.rounded_outline(
                        Rect {
                            x: origin.x + lo.x * scale - 4.0,
                            y: origin.y + lo.y * scale - 4.0,
                            w: ((hi.x - lo.x) * scale + 8.0).max(8.0),
                            h: ((hi.y - lo.y) * scale + 8.0).max(8.0),
                        },
                        0x007aff,
                        3.0,
                    );
                    for (i, p) in mark.handles().into_iter().enumerate() {
                        let (x, y) = (origin.x + p.x * scale, origin.y + p.y * scale);
                        self.circle(x, y, 5.0, 0xffffff, true);
                        self.circle(
                            x,
                            y,
                            5.0,
                            if i == 2 && matches!(mark.shape, Shape::Arrow(_, _, _)) {
                                0xf92d3a
                            } else {
                                0x007aff
                            },
                            false,
                        );
                    }
                }
            }
            self.outline(r, crate::theme::border_rgb(dark), 1.0);
        } else {
            self.text("Opening screenshot...", layout.canvas, muted, false);
        }
        unsafe {
            self.target.PopAxisAlignedClip();
        }
        self.panel(layout, background, dark);
        // Hover surfaces stay centered on their icons. Draw them before the
        // sliding selection so an adjacent/fading hover never covers blue.
        for &(control, r) in &layout.controls {
            if !control.is_drawing_tool()
                || control == active_tool
                || !control.enabled()
                || image.is_none()
            {
                continue;
            }
            let strength = hover_levels
                .iter()
                .find(|(c, _)| *c == control)
                .map_or(0.0, |(_, a)| *a);
            if strength > 0.0 {
                self.pill_opacity(
                    Rect {
                        x: r.x + (r.w - 20.0) / 2.0,
                        y: r.y + (r.h - 20.0) / 2.0,
                        w: 20.0,
                        h: 20.0,
                    },
                    if dark { 0x787883 } else { 0xffffff },
                    10.0,
                    strength,
                );
            }
        }
        if let Some(r) = layout.rect(active_tool)
            && (image.is_some() || active_tool == Control::Move)
        {
            self.pill(
                Rect {
                    x: pill_center - 18.0,
                    y: r.y + 3.5,
                    w: 36.0,
                    h: 25.0,
                },
                0x007aff,
                12.5,
            );
        }
        for &(control, r) in &layout.controls {
            let enabled = control.enabled()
                && (image.is_some()
                    || matches!(
                        control,
                        Control::Move | Control::Minimize | Control::Maximize | Control::Close
                    ));
            let active = control == Control::Save
                || (control == Control::Background && background.open)
                || control == active_tool;
            let background = if active && enabled {
                0x007aff
            } else if hovered == Some(control) && enabled {
                if dark { 0x4b4b55 } else { 0xf6f5fa }
            } else {
                pill
            };
            if matches!(
                control,
                Control::Minimize | Control::Maximize | Control::Close
            ) {
                if hovered == Some(control) {
                    self.fill(
                        r,
                        if control == Control::Close {
                            0xe81123
                        } else {
                            0xc8c0cd
                        },
                    );
                }
            } else {
                if !control.is_drawing_tool() {
                    self.pill(r, background, 14.0);
                }
            }
            let ink = if control == Control::Close && hovered == Some(control) {
                0xffffff
            } else if !enabled {
                if dark { muted } else { 0x55505b }
            } else if control.is_drawing_tool() {
                if ((r.x + r.w / 2.0) - pill_center).abs() < 12.0 {
                    0xffffff
                } else {
                    fg
                }
            } else if active {
                0xffffff
            } else {
                fg
            };
            match control {
                Control::Save => self.text("Save as...", r, ink, false),
                Control::Stroke => self.text(
                    &format!("{} px", stroke_width.round() as u32),
                    r,
                    ink,
                    false,
                ),
                Control::Zoom => {
                    let label = image
                        .map(|i| {
                            format!(
                                "{}%  ▾",
                                (view.scale(layout.canvas, i.width, i.height) * 100.0).round()
                                    as u32
                            )
                        })
                        .unwrap_or_else(|| "Fit  ▾".into());
                    self.text(&label, r, ink, false);
                }
                Control::Maximize if maximized => {
                    self.outline(
                        Rect {
                            x: r.x + r.w / 2.0 - 6.0,
                            y: r.y + r.h / 2.0 - 3.0,
                            w: 9.0,
                            h: 9.0,
                        },
                        ink,
                        1.1,
                    );
                    self.line(
                        (r.x + r.w / 2.0 - 3.0, r.y + r.h / 2.0 - 5.0),
                        (r.x + r.w / 2.0 + 6.0, r.y + r.h / 2.0 - 5.0),
                        ink,
                        1.1,
                    );
                    self.line(
                        (r.x + r.w / 2.0 + 6.0, r.y + r.h / 2.0 - 5.0),
                        (r.x + r.w / 2.0 + 6.0, r.y + r.h / 2.0 + 4.0),
                        ink,
                        1.1,
                    );
                }
                Control::Drag => {
                    self.text("Drag Me", r, ink, false);
                    self.line(
                        (r.x + 10.0, r.y + 11.0),
                        (r.x + 15.0, r.y + 11.0),
                        muted,
                        1.0,
                    );
                    self.line(
                        (r.x + 10.0, r.y + 15.0),
                        (r.x + 15.0, r.y + 15.0),
                        muted,
                        1.0,
                    );
                    self.line(
                        (r.x + 85.0, r.y + 11.0),
                        (r.x + 90.0, r.y + 11.0),
                        muted,
                        1.0,
                    );
                    self.line(
                        (r.x + 85.0, r.y + 15.0),
                        (r.x + 90.0, r.y + 15.0),
                        muted,
                        1.0,
                    );
                }
                _ => self.icon(
                    control,
                    r,
                    ink,
                    PRESET_COLORS
                        .get(selected_color)
                        .copied()
                        .unwrap_or(custom_color),
                ),
            }
            if focused == Some(control) {
                self.outline(
                    Rect {
                        x: r.x - 2.0,
                        y: r.y - 2.0,
                        w: r.w + 4.0,
                        h: r.h + 4.0,
                    },
                    crate::theme::border_rgb(dark),
                    1.0,
                );
            }
        }
        if palette_open {
            self.palette(layout, selected_color, dark);
        }
        unsafe {
            self.target
                .EndDraw(None, None)
                .context("Cannot draw annotation editor")?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rendering_premultiplies_without_mutating_export_pixels() {
        let p = [200, 100, 50, 128, 10, 20, 30, 0, 7, 8, 9, 255];
        assert_eq!(
            premultiply(&p),
            vec![100, 50, 25, 128, 0, 0, 0, 0, 7, 8, 9, 255]
        );
        assert_eq!(p[0], 200);
    }
}
