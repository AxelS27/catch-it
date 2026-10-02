use std::{
    rc::Rc,
    time::{Duration, Instant},
};

use anyhow::{Context, Result};
use windows::{
    Win32::{
        Foundation::{HMODULE, HWND},
        Graphics::{
            Direct2D::{Common::*, *},
            Direct3D::D3D_DRIVER_TYPE_HARDWARE,
            Direct3D11::*,
            DirectComposition::*,
            DirectWrite::*,
            Dxgi::{Common::*, *},
        },
    },
    core::{Interface, w},
};
use windows_numerics::{Matrix3x2, Vector2};

use super::{
    SavedScreenshot,
    layout::{Control, Layout},
    lifecycle::Timing,
};

/// UI-thread-owned, shared native rendering resources. No software animation loop.
pub struct Compositor {
    device: ID3D11Device,
    factory: ID2D1Factory1,
    context: ID2D1DeviceContext,
    dxgi_factory: IDXGIFactory2,
    composition: IDCompositionDevice,
    text_format: IDWriteTextFormat,
}

impl Compositor {
    pub fn new() -> Result<Rc<Self>> {
        unsafe {
            let mut device = None;
            D3D11CreateDevice(
                None,
                D3D_DRIVER_TYPE_HARDWARE,
                HMODULE::default(),
                D3D11_CREATE_DEVICE_BGRA_SUPPORT,
                None,
                D3D11_SDK_VERSION,
                Some(&mut device),
                None,
                None,
            )?;
            let device = device.context("D3D11 returned no compositor device")?;
            let dxgi: IDXGIDevice = device.cast()?;
            let dxgi_factory = dxgi.GetAdapter()?.GetParent::<IDXGIFactory2>()?;
            let factory: ID2D1Factory1 =
                D2D1CreateFactory(D2D1_FACTORY_TYPE_SINGLE_THREADED, None)?;
            let context = factory
                .CreateDevice(&dxgi)?
                .CreateDeviceContext(D2D1_DEVICE_CONTEXT_OPTIONS_NONE)?;
            let composition = DCompositionCreateDevice(&dxgi)?;
            let write: IDWriteFactory = DWriteCreateFactory(DWRITE_FACTORY_TYPE_SHARED)?;
            let text_format = write.CreateTextFormat(
                w!("Segoe UI"),
                None,
                DWRITE_FONT_WEIGHT_SEMI_BOLD,
                DWRITE_FONT_STYLE_NORMAL,
                DWRITE_FONT_STRETCH_NORMAL,
                12.0,
                w!("en-US"),
            )?;
            text_format.SetTextAlignment(DWRITE_TEXT_ALIGNMENT_CENTER)?;
            text_format.SetParagraphAlignment(DWRITE_PARAGRAPH_ALIGNMENT_CENTER)?;
            Ok(Rc::new(Self {
                device,
                factory,
                context,
                dxgi_factory,
                composition,
                text_format,
            }))
        }
    }
}

pub struct Surface {
    compositor: Rc<Compositor>,
    target: IDCompositionTarget,
    visual: IDCompositionVisual,
    opacity: IDCompositionEffectGroup,
    // Keep the content alive until the composition target is detached.
    _swap_chain: IDXGISwapChain1,
    layout: Layout,
    timing: Timing,
    started: Option<Instant>,
    drag_pixels: Vec<u8>,
    card: ID2D1Bitmap1,
    controls: (bool, bool, bool),
}

enum Image<'a> {
    Original(&'a SavedScreenshot),
    Cached(&'a [u8]),
}

fn cached_card(compositor: &Compositor, layout: Layout, pixels: &[u8]) -> Result<ID2D1Bitmap1> {
    let width = (layout.card_width * layout.scale).round() as usize;
    let height = (layout.card_height * layout.scale).round() as usize;
    anyhow::ensure!(pixels.len() == width * height * 4, "Invalid cached card");
    let left = (layout.card_left * layout.scale).round() as usize;
    let top = (layout.card_top * layout.scale).round() as usize;
    let stride = layout.width as usize * 4;
    let mut padded = vec![0; stride * layout.height as usize];
    for row in 0..height {
        let dst = (top + row) * stride + left * 4;
        padded[dst..dst + width * 4]
            .copy_from_slice(&pixels[row * width * 4..(row + 1) * width * 4]);
    }
    unsafe {
        compositor
            .context
            .CreateBitmap(
                D2D_SIZE_U {
                    width: layout.width,
                    height: layout.height,
                },
                Some(padded.as_ptr().cast()),
                layout.width * 4,
                &D2D1_BITMAP_PROPERTIES1 {
                    pixelFormat: D2D1_PIXEL_FORMAT {
                        format: DXGI_FORMAT_B8G8R8A8_UNORM,
                        alphaMode: D2D1_ALPHA_MODE_PREMULTIPLIED,
                    },
                    dpiX: 96.0 * layout.scale,
                    dpiY: 96.0 * layout.scale,
                    ..Default::default()
                },
            )
            .map_err(Into::into)
    }
}

fn compose(
    compositor: &Compositor,
    swap_chain: &IDXGISwapChain1,
    layout: Layout,
    card: &ID2D1Bitmap1,
    hovered: bool,
    pinned: bool,
    highlighted: bool,
) -> Result<()> {
    unsafe {
        let context = &compositor.context;
        let dpi = 96.0 * layout.scale;
        context.SetDpi(dpi, dpi);
        let surface: IDXGISurface = swap_chain.GetBuffer(0)?;
        let target = context.CreateBitmapFromDxgiSurface(
            &surface,
            Some(&D2D1_BITMAP_PROPERTIES1 {
                pixelFormat: D2D1_PIXEL_FORMAT {
                    format: DXGI_FORMAT_B8G8R8A8_UNORM,
                    alphaMode: D2D1_ALPHA_MODE_PREMULTIPLIED,
                },
                dpiX: dpi,
                dpiY: dpi,
                bitmapOptions: D2D1_BITMAP_OPTIONS_TARGET | D2D1_BITMAP_OPTIONS_CANNOT_DRAW,
                ..Default::default()
            }),
        )?;
        let shadow = context.CreateEffect(&CLSID_D2D1Shadow)?;
        shadow.SetInput(0, card, true);
        shadow.SetValue(
            D2D1_SHADOW_PROP_BLUR_STANDARD_DEVIATION.0 as u32,
            D2D1_PROPERTY_TYPE_FLOAT,
            &3.0_f32.to_ne_bytes(),
        )?;
        let color: Vec<u8> = [0.0_f32, 0.0, 0.0, 0.32]
            .into_iter()
            .flat_map(f32::to_ne_bytes)
            .collect();
        shadow.SetValue(
            D2D1_SHADOW_PROP_COLOR.0 as u32,
            D2D1_PROPERTY_TYPE_VECTOR4,
            &color,
        )?;
        let shadow_output = shadow.GetOutput()?;
        context.SetTarget(&target);
        context.BeginDraw();
        context.Clear(Some(&D2D1_COLOR_F::default()));
        context.DrawImage(
            &shadow_output,
            Some(&Vector2 { X: 0.0, Y: 4.0 }),
            None,
            D2D1_INTERPOLATION_MODE_LINEAR,
            D2D1_COMPOSITE_MODE_SOURCE_OVER,
        );
        context.DrawImage(
            card,
            None,
            None,
            D2D1_INTERPOLATION_MODE_LINEAR,
            D2D1_COMPOSITE_MODE_SOURCE_OVER,
        );
        let controls_result = (|| {
            if highlighted {
                let blue = context.CreateSolidColorBrush(
                    &D2D1_COLOR_F {
                        r: 0.08,
                        g: 0.42,
                        b: 0.92,
                        a: 1.0,
                    },
                    None,
                )?;
                let inset = 1.5;
                context.DrawRoundedRectangle(
                    &D2D1_ROUNDED_RECT {
                        rect: D2D_RECT_F {
                            left: layout.card_left + inset,
                            top: layout.card_top + inset,
                            right: layout.card_left + layout.card_width - inset,
                            bottom: layout.card_top + layout.card_height - inset,
                        },
                        radiusX: layout.radius - inset,
                        radiusY: layout.radius - inset,
                    },
                    &blue,
                    3.0,
                    None,
                );
            }
            draw_controls(compositor, layout, hovered, pinned)
        })();
        let draw_result = context.EndDraw(None, None);
        context.SetTarget(None);
        controls_result?;
        draw_result.context("Cannot compose thumbnail and shadow")?;
        swap_chain.Present(1, DXGI_PRESENT(0)).ok()?;
    }
    Ok(())
}

fn draw_controls(
    compositor: &Compositor,
    layout: Layout,
    hovered: bool,
    pinned: bool,
) -> Result<()> {
    if (!hovered && !pinned) || layout.control_rect(Control::Copy).is_none() {
        return Ok(());
    }
    unsafe {
        let context = &compositor.context;
        let background = context.CreateSolidColorBrush(
            &D2D1_COLOR_F {
                r: 0.9,
                g: 0.9,
                b: 0.91,
                a: 0.96,
            },
            None,
        )?;
        let ink = context.CreateSolidColorBrush(
            &D2D1_COLOR_F {
                r: 0.09,
                g: 0.09,
                b: 0.1,
                a: 1.0,
            },
            None,
        )?;
        let disabled = context.CreateSolidColorBrush(
            &D2D1_COLOR_F {
                r: 0.09,
                g: 0.09,
                b: 0.1,
                a: 0.32,
            },
            None,
        )?;
        let blue = context.CreateSolidColorBrush(
            &D2D1_COLOR_F {
                r: 0.12,
                g: 0.48,
                b: 0.95,
                a: 1.0,
            },
            None,
        )?;
        let white = context.CreateSolidColorBrush(
            &D2D1_COLOR_F {
                r: 1.0,
                g: 1.0,
                b: 1.0,
                a: 1.0,
            },
            None,
        )?;
        if hovered {
            let dim = context.CreateSolidColorBrush(
                &D2D1_COLOR_F {
                    r: 0.0,
                    g: 0.0,
                    b: 0.0,
                    a: 0.48,
                },
                None,
            )?;
            context.FillRoundedRectangle(
                &D2D1_ROUNDED_RECT {
                    rect: D2D_RECT_F {
                        left: layout.card_left,
                        top: layout.card_top,
                        right: layout.card_left + layout.card_width,
                        bottom: layout.card_top + layout.card_height,
                    },
                    radiusX: layout.radius,
                    radiusY: layout.radius,
                },
                &dim,
            );
        }
        for control in Control::ALL {
            if !hovered && control != Control::Pin {
                continue;
            }
            let bounds = layout.control_rect(control).expect("controls fit the card");
            let rect = D2D_RECT_F {
                left: bounds.left,
                top: bounds.top,
                right: bounds.left + bounds.width,
                bottom: bounds.top + bounds.height,
            };
            let active_pin = control == Control::Pin && pinned;
            let brush = if active_pin {
                &white
            } else if control.enabled() {
                &ink
            } else {
                &disabled
            };
            context.FillRoundedRectangle(
                &D2D1_ROUNDED_RECT {
                    rect,
                    radiusX: bounds.radius,
                    radiusY: bounds.radius,
                },
                if active_pin { &blue } else { &background },
            );
            let (x, y) = (
                bounds.left + bounds.width / 2.0,
                bounds.top + bounds.height / 2.0,
            );
            let line = |a: (f32, f32), b: (f32, f32)| {
                context.DrawLine(
                    Vector2 {
                        X: x + a.0,
                        Y: y + a.1,
                    },
                    Vector2 {
                        X: x + b.0,
                        Y: y + b.1,
                    },
                    brush,
                    1.5,
                    None,
                );
            };
            match control {
                Control::Copy | Control::Save => {
                    let text: Vec<u16> = if control == Control::Copy {
                        "Copy"
                    } else {
                        "Save"
                    }
                    .encode_utf16()
                    .collect();
                    context.DrawText(
                        &text,
                        &compositor.text_format,
                        &rect,
                        brush,
                        D2D1_DRAW_TEXT_OPTIONS_CLIP,
                        DWRITE_MEASURING_MODE_NATURAL,
                    );
                }
                Control::Close => {
                    line((-3.0, -3.0), (3.0, 3.0));
                    line((3.0, -3.0), (-3.0, 3.0));
                }
                Control::Pin => {
                    for (a, b) in [
                        ((0.0, -5.0), (5.0, 0.0)),
                        ((1.0, -4.0), (-2.0, -1.0)),
                        ((4.0, -1.0), (1.0, 2.0)),
                        ((-3.0, -2.0), (2.0, 3.0)),
                        ((-0.5, 0.5), (-4.0, 4.0)),
                    ] {
                        line(a, b);
                    }
                }
                Control::Annotate => {
                    for (a, b) in [
                        ((-4.0, 2.0), (2.0, -4.0)),
                        ((-2.0, 4.0), (4.0, -2.0)),
                        ((2.0, -4.0), (4.0, -2.0)),
                        ((-4.0, 2.0), (-5.0, 5.0)),
                        ((-5.0, 5.0), (-2.0, 4.0)),
                        ((0.5, -2.5), (2.5, -0.5)),
                    ] {
                        line(a, b);
                    }
                }
                Control::Upload => {
                    let geometry = compositor.factory.CreatePathGeometry()?;
                    let sink = geometry.Open()?;
                    sink.BeginFigure(
                        Vector2 {
                            X: x - 4.0,
                            Y: y + 3.0,
                        },
                        D2D1_FIGURE_BEGIN_HOLLOW,
                    );
                    for (p1, p2, p3) in [
                        ((-9.0, 3.0), (-8.0, -3.0), (-4.0, -3.0)),
                        ((-3.0, -9.0), (5.0, -8.0), (5.0, -3.0)),
                        ((9.0, -3.0), (9.0, 3.0), (4.0, 3.0)),
                    ] {
                        sink.AddBezier(&D2D1_BEZIER_SEGMENT {
                            point1: Vector2 {
                                X: x + p1.0,
                                Y: y + p1.1,
                            },
                            point2: Vector2 {
                                X: x + p2.0,
                                Y: y + p2.1,
                            },
                            point3: Vector2 {
                                X: x + p3.0,
                                Y: y + p3.1,
                            },
                        });
                    }
                    sink.EndFigure(D2D1_FIGURE_END_OPEN);
                    sink.Close()?;
                    context.DrawGeometry(&geometry, brush, 1.5, None);
                    line((0.0, 6.0), (0.0, -1.0));
                    line((0.0, -1.0), (-2.5, 1.5));
                    line((0.0, -1.0), (2.5, 1.5));
                }
            }
        }
    }
    Ok(())
}
impl Surface {
    pub fn new(
        compositor: Rc<Compositor>,
        hwnd: HWND,
        layout: Layout,
        image: &SavedScreenshot,
        timing: Timing,
    ) -> Result<Self> {
        Self::build(compositor, hwnd, layout, Image::Original(image), timing)
    }

    pub fn from_cached(
        compositor: Rc<Compositor>,
        hwnd: HWND,
        layout: Layout,
        pixels: &[u8],
        timing: Timing,
    ) -> Result<Self> {
        Self::build(compositor, hwnd, layout, Image::Cached(pixels), timing)
    }

    fn build(
        compositor: Rc<Compositor>,
        hwnd: HWND,
        layout: Layout,
        image: Image<'_>,
        timing: Timing,
    ) -> Result<Self> {
        unsafe {
            let swap_chain = compositor.dxgi_factory.CreateSwapChainForComposition(
                &compositor.device,
                &DXGI_SWAP_CHAIN_DESC1 {
                    Width: layout.width,
                    Height: layout.height,
                    Format: DXGI_FORMAT_B8G8R8A8_UNORM,
                    SampleDesc: DXGI_SAMPLE_DESC {
                        Count: 1,
                        Quality: 0,
                    },
                    BufferUsage: DXGI_USAGE_RENDER_TARGET_OUTPUT,
                    BufferCount: 2,
                    Scaling: DXGI_SCALING_STRETCH,
                    SwapEffect: DXGI_SWAP_EFFECT_FLIP_SEQUENTIAL,
                    AlphaMode: DXGI_ALPHA_MODE_PREMULTIPLIED,
                    ..Default::default()
                },
                None,
            )?;
            let (drag_pixels, card) = match image {
                Image::Original(image) => render_card(&compositor, layout, image)?,
                Image::Cached(pixels) => {
                    (pixels.to_vec(), cached_card(&compositor, layout, pixels)?)
                }
            };
            compose(&compositor, &swap_chain, layout, &card, false, false, false)?;
            let target = compositor.composition.CreateTargetForHwnd(hwnd, true)?;
            let visual = compositor.composition.CreateVisual()?;
            let opacity = compositor.composition.CreateEffectGroup()?;
            visual.SetContent(&swap_chain)?;
            visual.SetEffect(&opacity)?;
            // Commit a transparent initial state before showing the popup.
            opacity.SetOpacity2(0.0)?;
            visual.SetOffsetX2(-24.0 * layout.scale)?;
            target.SetRoot(&visual)?;
            compositor.composition.Commit()?;
            Ok(Self {
                compositor,
                target,
                visual,
                opacity,
                _swap_chain: swap_chain,
                layout,
                timing,
                started: None,
                drag_pixels,
                card,
                controls: (false, false, false),
            })
        }
    }

    /// The exact rendered, premultiplied card, cached once per screenshot.
    pub fn drag_pixels(&self) -> &[u8] {
        &self.drag_pixels
    }

    pub fn set_controls(&mut self, hovered: bool, pinned: bool, highlighted: bool) -> Result<()> {
        if self.controls != (hovered, pinned, highlighted) {
            compose(
                &self.compositor,
                &self._swap_chain,
                self.layout,
                &self.card,
                hovered,
                pinned,
                highlighted,
            )?;
            self.controls = (hovered, pinned, highlighted);
        }
        Ok(())
    }

    pub fn set_dragging(&mut self, dragging: bool) -> Result<()> {
        // Keep the source in place but subdued behind the pointer-following
        // miniature, as in the supplied video.
        unsafe {
            self.opacity
                .SetOpacity2(if dragging { 0.45 } else { 1.0 })?;
            self.compositor.composition.Commit()?;
        }
        Ok(())
    }

    pub fn appear(&mut self) -> Result<()> {
        self.started = Some(Instant::now());
        self.animate(-24.0 * self.layout.scale, 0.0, 0.0, 1.0, self.timing.appear)
    }

    pub fn dismiss_after_drag(&self) -> Result<()> {
        // The original remains at 45% throughout OLE's modal loop, then
        // exits toward the left without flashing back to full opacity.
        self.animate(
            0.0,
            -(self.layout.width as f32),
            0.45,
            0.0,
            self.timing.dismiss,
        )
    }

    pub fn dismiss(&self) -> Result<()> {
        // A manual dismissal can interrupt entrance. Start from the current
        // easing sample instead of flashing back to a fully opaque image.
        let progress = self.started.map_or(0.0, |started| {
            let t = (started.elapsed().as_secs_f32() / self.timing.appear.as_secs_f32())
                .clamp(0.0, 1.0);
            1.0 - (1.0 - t).powi(3)
        });
        self.animate(
            -24.0 * self.layout.scale * (1.0 - progress),
            -(self.layout.width as f32),
            progress,
            0.0,
            self.timing.dismiss,
        )
    }

    fn animate(
        &self,
        from_x: f32,
        to_x: f32,
        from_opacity: f32,
        to_opacity: f32,
        duration: Duration,
    ) -> Result<()> {
        unsafe {
            let slide = cubic_out(&self.compositor.composition, from_x, to_x, duration)?;
            let fade = cubic_out(
                &self.compositor.composition,
                from_opacity,
                to_opacity,
                duration,
            )?;
            self.visual.SetOffsetX(&slide)?;
            self.opacity.SetOpacity(&fade)?;
            self.compositor.composition.Commit()?;
        }
        Ok(())
    }
}

impl Drop for Surface {
    fn drop(&mut self) {
        unsafe {
            let _ = self.target.SetRoot(None);
            let _ = self.compositor.composition.Commit();
        }
    }
}

fn cubic_out(
    device: &IDCompositionDevice,
    from: f32,
    to: f32,
    duration: Duration,
) -> Result<IDCompositionAnimation> {
    let seconds = duration.as_secs_f64();
    anyhow::ensure!(seconds > 0.0, "Animation duration must be positive");
    let d = seconds as f32;
    let delta = to - from;
    unsafe {
        let animation = device.CreateAnimation()?;
        animation.AddCubic(
            0.0,
            from,
            3.0 * delta / d,
            -3.0 * delta / (d * d),
            delta / (d * d * d),
        )?;
        animation.End(seconds, to)?;
        Ok(animation)
    }
}

fn render_card(
    compositor: &Compositor,
    layout: Layout,
    image: &SavedScreenshot,
) -> Result<(Vec<u8>, ID2D1Bitmap1)> {
    unsafe {
        let context = &compositor.context;
        let dpi = 96.0 * layout.scale;
        context.SetDpi(dpi, dpi);
        let source = context.CreateBitmap(
            D2D_SIZE_U {
                width: image.width,
                height: image.height,
            },
            Some(image.pixels.as_ptr().cast()),
            image.width * 4,
            &D2D1_BITMAP_PROPERTIES1 {
                pixelFormat: D2D1_PIXEL_FORMAT {
                    format: DXGI_FORMAT_B8G8R8A8_UNORM,
                    alphaMode: D2D1_ALPHA_MODE_IGNORE,
                },
                dpiX: 96.0,
                dpiY: 96.0,
                ..Default::default()
            },
        )?;
        let rect = D2D_RECT_F {
            left: layout.card_left,
            top: layout.card_top,
            right: layout.card_left + layout.card_width,
            bottom: layout.card_top + layout.card_height,
        };
        let source_rect = D2D_RECT_F {
            left: layout.crop.x,
            top: layout.crop.y,
            right: layout.crop.x + layout.crop.width,
            bottom: layout.crop.y + layout.crop.height,
        };
        let rounded = D2D1_ROUNDED_RECT {
            rect,
            radiusX: layout.radius,
            radiusY: layout.radius,
        };
        let geometry = compositor
            .factory
            .CreateRoundedRectangleGeometry(&rounded)?;
        // Rasterize the clipped card once. Replaying a layered command list as
        // both Shadow input and foreground causes D2DERR_LAYER_ALREADY_IN_USE
        // for large downscales (reproduced with a full-screen screenshot).
        let card = context.CreateBitmap(
            D2D_SIZE_U {
                width: layout.width,
                height: layout.height,
            },
            None,
            0,
            &D2D1_BITMAP_PROPERTIES1 {
                pixelFormat: D2D1_PIXEL_FORMAT {
                    format: DXGI_FORMAT_B8G8R8A8_UNORM,
                    alphaMode: D2D1_ALPHA_MODE_PREMULTIPLIED,
                },
                dpiX: dpi,
                dpiY: dpi,
                bitmapOptions: D2D1_BITMAP_OPTIONS_TARGET,
                ..Default::default()
            },
        )?;
        let border = context.CreateSolidColorBrush(
            &D2D1_COLOR_F {
                r: 1.0,
                g: 1.0,
                b: 1.0,
                a: 0.85,
            },
            None,
        )?;
        let mut layer = D2D1_LAYER_PARAMETERS1 {
            contentBounds: rect,
            geometricMask: std::mem::ManuallyDrop::new(Some(geometry.cast()?)),
            maskAntialiasMode: D2D1_ANTIALIAS_MODE_PER_PRIMITIVE,
            maskTransform: Matrix3x2::identity(),
            opacity: 1.0,
            ..Default::default()
        };
        context.SetTarget(&card);
        context.BeginDraw();
        context.Clear(Some(&D2D1_COLOR_F::default()));
        context.PushLayer(&layer, None);
        context.DrawBitmap(
            &source,
            Some(&rect),
            1.0,
            D2D1_INTERPOLATION_MODE_HIGH_QUALITY_CUBIC,
            Some(&source_rect),
            None,
        );
        context.PopLayer();
        let stroke = 1.0_f32.min(layout.card_width).min(layout.card_height);
        let inset = stroke / 2.0;
        context.DrawRoundedRectangle(
            &D2D1_ROUNDED_RECT {
                rect: D2D_RECT_F {
                    left: rect.left + inset,
                    top: rect.top + inset,
                    right: rect.right - inset,
                    bottom: rect.bottom - inset,
                },
                radiusX: (layout.radius - inset).max(0.0),
                radiusY: (layout.radius - inset).max(0.0),
            },
            &border,
            stroke,
            None,
        );
        let draw_result = context.EndDraw(None, None);
        context.SetTarget(None);
        // windows-rs uses ManuallyDrop for COM fields in native parameter structs.
        std::mem::ManuallyDrop::drop(&mut layer.geometricMask);
        draw_result.context("Cannot rasterize thumbnail card")?;

        // Cache only the small card, without shadow/padding. Dragging reuses these
        // exact GPU-rendered pixels, including fractional crop, border and alpha.
        let width = (layout.card_width * layout.scale).round() as u32;
        let height = (layout.card_height * layout.scale).round() as u32;
        let readback = context.CreateBitmap(
            D2D_SIZE_U { width, height },
            None,
            0,
            &D2D1_BITMAP_PROPERTIES1 {
                pixelFormat: D2D1_PIXEL_FORMAT {
                    format: DXGI_FORMAT_B8G8R8A8_UNORM,
                    alphaMode: D2D1_ALPHA_MODE_PREMULTIPLIED,
                },
                bitmapOptions: D2D1_BITMAP_OPTIONS_CPU_READ | D2D1_BITMAP_OPTIONS_CANNOT_DRAW,
                dpiX: dpi,
                dpiY: dpi,
                ..Default::default()
            },
        )?;
        let left = (layout.card_left * layout.scale).round() as u32;
        let top = (layout.card_top * layout.scale).round() as u32;
        readback.CopyFromBitmap(
            None,
            &card,
            Some(&D2D_RECT_U {
                left,
                top,
                right: left + width,
                bottom: top + height,
            }),
        )?;
        let stride = width as usize * 4;
        let mut pixels = vec![0; stride * height as usize];
        let mapped = readback.Map(D2D1_MAP_OPTIONS_READ)?;
        if mapped.bits.is_null() || (mapped.pitch as usize) < stride {
            let _ = readback.Unmap();
            anyhow::bail!("Invalid thumbnail readback buffer");
        }
        for y in 0..height as usize {
            let row =
                std::slice::from_raw_parts(mapped.bits.add(y * mapped.pitch as usize), stride);
            pixels[y * stride..(y + 1) * stride].copy_from_slice(row);
        }
        readback.Unmap()?;
        Ok((pixels, card))
    }
}
