use super::layout::{Control, Layout, PRESET_COLORS, Rect, View};
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

pub struct ChromeState {
    pub dark: bool,
    pub hovered: Option<Control>,
    pub focused: Option<Control>,
    pub palette_open: bool,
    pub selected_color: usize,
}

pub struct Renderer {
    target: ID2D1HwndRenderTarget,
    brush: ID2D1SolidColorBrush,
    font: IDWriteTextFormat,
    large: IDWriteTextFormat,
    bitmap: Option<ID2D1Bitmap>,
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
            let mut renderer = Self {
                target,
                brush,
                font: make_font(12.0)?,
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
        unsafe {
            self.brush.SetColor(&color(rgb));
            self.target.FillRectangle(&rect(r), &self.brush);
        }
    }
    fn pill(&self, r: Rect, rgb: u32, radius: f32) {
        unsafe {
            self.brush.SetColor(&color(rgb));
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
    fn icon(&self, control: Control, r: Rect, rgb: u32, selected_color: usize) {
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
                self.line((x - 6.0, y + 5.0), (x + 5.0, y - 5.0), rgb, 1.8);
                self.line((x + 5.0, y - 5.0), (x + 5.0, y + 1.0), rgb, 1.8);
                self.line((x + 5.0, y - 5.0), (x - 1.0, y - 5.0), rgb, 1.8);
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
            Control::Pencil | Control::Highlighter => {
                self.line((x - 5.0, y + 5.0), (x + 4.0, y - 4.0), rgb, 3.0);
                self.line((x + 2.0, y - 6.0), (x + 6.0, y - 2.0), rgb, 1.5);
                if control == Control::Highlighter {
                    self.line((x - 7.0, y + 7.0), (x + 6.0, y + 7.0), rgb, 2.0);
                }
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
            Control::Color => {
                self.circle(x - 4.0, y, 6.0, PRESET_COLORS[selected_color], true);
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
    fn palette(&self, layout: &Layout, selected_color: usize) {
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
            0xd0cad1,
            18.0,
        );
        self.pill(r, 0xf6f6f6, 17.0);
        self.rounded_outline(r, 0xc8c7c9, 17.0);
        for (index, &swatch) in PRESET_COLORS.iter().enumerate() {
            let (x, y) = (r.x + r.w / 2.0, r.y + 20.0 + 32.0 * index as f32);
            if index == selected_color {
                self.circle(x, y, 13.0, 0xb5d1be, true);
                self.circle(x, y, 11.0, 0xf6f6f6, true);
            }
            self.circle(x, y, 10.0, swatch, true);
            self.circle(
                x,
                y,
                10.0,
                if index == 9 { 0xc7c7c7 } else { 0x6e6a74 },
                false,
            );
        }
        let (x, y) = (r.x + r.w / 2.0, r.y + 20.0 + 32.0 * 10.0);
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
    pub fn draw(
        &self,
        layout: &Layout,
        view: &View,
        image: Option<&Raster>,
        chrome: ChromeState,
    ) -> Result<()> {
        let ChromeState {
            dark,
            hovered,
            focused,
            palette_open,
            selected_color,
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
        if layout.width >= super::layout::MIN_WIDTH {
            self.pill(
                Rect {
                    x: 132.0,
                    y: 8.0,
                    w: 352.0,
                    h: 32.0,
                },
                group,
                16.0,
            );
            for x in [164.5, 222.5, 280.5, 367.5, 425.5] {
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
                        if view.scale(layout.canvas, image.width, image.height) >= 1.0 {
                            D2D1_BITMAP_INTERPOLATION_MODE_NEAREST_NEIGHBOR
                        } else {
                            D2D1_BITMAP_INTERPOLATION_MODE_LINEAR
                        },
                        None,
                    );
                }
            }
            self.outline(r, if dark { 0x4b4b52 } else { 0xd8d6de }, 1.0);
        } else {
            self.text("Opening screenshot...", layout.canvas, muted, false);
        }
        unsafe {
            self.target.PopAxisAlignedClip();
        }
        for &(control, r) in &layout.controls {
            let enabled = control.enabled() && (image.is_some() || control == Control::Move);
            let active = control == Control::Move || control == Control::Save;
            let background = if active && enabled {
                0x007aff
            } else if hovered == Some(control) && enabled {
                if dark { 0x4b4b55 } else { 0xf6f5fa }
            } else {
                pill
            };
            if !matches!(
                control,
                Control::Rectangle
                    | Control::Fill
                    | Control::Ellipse
                    | Control::Line
                    | Control::Arrow
                    | Control::Text
                    | Control::Pixelate
                    | Control::Spotlight
                    | Control::Counter
                    | Control::Pencil
                    | Control::Highlighter
            ) || active
            {
                self.pill(
                    r,
                    background,
                    if control == Control::Move { 9.0 } else { 14.0 },
                );
            }
            let ink = if !enabled {
                if dark { muted } else { 0x55505b }
            } else if active {
                0xffffff
            } else {
                fg
            };
            match control {
                Control::Save => self.text("Save as...", r, ink, false),
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
                _ => self.icon(control, r, ink, selected_color),
            }
            if focused == Some(control) {
                self.outline(
                    Rect {
                        x: r.x - 2.0,
                        y: r.y - 2.0,
                        w: r.w + 4.0,
                        h: r.h + 4.0,
                    },
                    0x007aff,
                    1.0,
                );
            }
        }
        if palette_open {
            self.palette(layout, selected_color);
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
