//! Background Tool model and lossless, non-destructive raster composition.
//! Source pixels stay untouched; the same composed raster drives preview/output.
use super::layout::Rect;
use crate::storage::Raster;
use anyhow::{Context, Result};

pub const PANEL_WIDTH: f32 = 260.0;
pub const PANEL_TOP: f32 = 48.0;
pub const PANEL_BOTTOM: f32 = 48.0;
pub const GRADIENTS: [[u32; 4]; 20] = [
    [0xee547e, 0x934ab8, 0xfc7f90, 0x5677d3],
    [0x292e8b, 0x4935a9, 0xd553a3, 0xeb94ac],
    [0x15215e, 0x00417a, 0x0a59b0, 0x49c8db],
    [0x8be8c9, 0x2cafa4, 0x7cd5c8, 0x69dbef],
    [0xf17c69, 0xc94491, 0xf4af59, 0xe6586a],
    [0x7d2ce1, 0xd339b4, 0x04bfdf, 0x6923c1],
    [0xe7e0de, 0xb4c4cc, 0xdeb1bc, 0xf0d7df],
    [0xa0c2c8, 0xd7ac9c, 0xd2cad0, 0xf5dcbe],
    [0x31b9e3, 0x5c45d1, 0xa926e4, 0x3864cc],
    [0x191449, 0x321674, 0x5328ab, 0x121844],
    [0x1971aa, 0x155789, 0xf0b879, 0xdb82a6],
    [0x6a9abf, 0xa3bed9, 0xecb1b2, 0xe0abca],
    // The reference chooses this lilac/purple/cyan family (row 3, column 3).
    [0xb1d9f6, 0x823bb4, 0x823eb6, 0x60c9ea],
    [0x4f1137, 0x87254a, 0x921c42, 0x641745],
    [0x121241, 0x143b87, 0x1e248f, 0xb72ba0],
    [0x9452bd, 0x487bd6, 0xed68b0, 0x6755c9],
    [0xff6650, 0xef7a31, 0xffb27e, 0xdc234b],
    [0xf27235, 0x9355ce, 0x29acdf, 0xffd365],
    [0x3e206a, 0xd73b74, 0x352f7c, 0xfb7f72],
    [0x4e469b, 0xda3583, 0x65a8d1, 0xf48283],
];
pub const SOLIDS: [u32; 18] = [
    0x101014, 0xffffff, 0xcc1940, 0xf27427, 0xf5aa20, 0x29945a, 0x2186dd, 0x8226c8, 0xeeeeee,
    0x323237, 0xdce5e9, 0xf2a6b0, 0xf8c47e, 0xf2db88, 0x9de5b4, 0xa1d1f9, 0xc9acf0, 0xee5ac0,
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Style {
    None,
    Gradient(usize),
    Wallpaper(usize),
    Solid(usize),
    Blurred(usize),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Target {
    None,
    Gradient(usize),
    Wallpaper(usize),
    Solid(usize),
    Blurred(usize),
    Slider(Slider),
    AutoBalance,
    ToggleGradients,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Slider {
    Padding,
    Inset,
    Shadow,
    Corners,
}

#[derive(Clone, Debug)]
pub struct Background {
    pub open: bool,
    pub expanded: bool,
    pub style: Style,
    pub padding: f32,
    pub inset: f32,
    pub shadow: f32,
    pub corners: f32,
    pub auto_balance: bool,
    pub dragging: Option<Slider>,
    pub scroll_y: f32,
}
impl Default for Background {
    fn default() -> Self {
        Self {
            open: false,
            expanded: true,
            style: Style::None,
            padding: 0.25,
            inset: 0.0,
            shadow: 0.30,
            corners: 0.20,
            auto_balance: false,
            dragging: None,
            scroll_y: 0.0,
        }
    }
}
impl Background {
    pub fn max_scroll(&self, client_height: f32) -> f32 {
        let content_height = if self.expanded { 730.0 } else { 538.0 };
        (content_height - (client_height - PANEL_TOP - PANEL_BOTTOM)).max(0.0)
    }
    pub fn selected(&self) -> bool {
        self.style != Style::None
    }
    pub fn slider_rect(&self, slider: Slider) -> Rect {
        let (x, y, w) = match slider {
            Slider::Padding => (16.0, 602.0, 228.0),
            Slider::Inset => (16.0, 652.0, 102.0),
            Slider::Shadow => (16.0, 701.0, 102.0),
            Slider::Corners => (142.0, 701.0, 102.0),
        };
        Rect { x, y, w, h: 20.0 }
    }
    pub fn slider_value(&self, slider: Slider) -> f32 {
        match slider {
            Slider::Padding => self.padding,
            Slider::Inset => self.inset,
            Slider::Shadow => self.shadow,
            Slider::Corners => self.corners,
        }
    }
    pub fn set_slider(&mut self, slider: Slider, x: f32) -> bool {
        let r = self.slider_rect(slider);
        let value = ((x - r.x) / r.w).clamp(0.0, 1.0);
        let field = match slider {
            Slider::Padding => &mut self.padding,
            Slider::Inset => &mut self.inset,
            Slider::Shadow => &mut self.shadow,
            Slider::Corners => &mut self.corners,
        };
        let changed = (*field - value).abs() > 0.001;
        *field = value;
        changed
    }
    pub fn hit(&self, x: f32, y: f32) -> Option<Target> {
        if !self.open || !(0.0..PANEL_WIDTH).contains(&x) || y < PANEL_TOP {
            return None;
        }
        let y = y - PANEL_TOP + self.scroll_y;
        if (48.0..80.0).contains(&y) && (16.0..244.0).contains(&x) {
            return Some(Target::None);
        }
        if (90.0..120.0).contains(&y) && x > 152.0 {
            return Some(Target::ToggleGradients);
        }
        if self.expanded && (122.0..315.0).contains(&y) {
            let col = ((x - 16.0) / 48.0).floor() as isize;
            let row = ((y - 122.0) / 48.0).floor() as isize;
            if (0..5).contains(&col)
                && (0..4).contains(&row)
                && (x - 16.0 - col as f32 * 48.0) < 38.0
                && (y - 122.0 - row as f32 * 48.0) < 38.0
            {
                return Some(Target::Gradient((row * 5 + col) as usize));
            }
        }
        let shift = if self.expanded { 0.0 } else { -192.0 };
        let y = y - shift;
        if (352.0..391.0).contains(&y) && (16.0..160.0).contains(&x) {
            let slot = ((x - 16.0) / 48.0).floor() as usize;
            if slot < 3 && (x - 16.0 - slot as f32 * 48.0) < 38.0 {
                return Some(Target::Wallpaper(slot));
            }
        }
        if (420.0..461.0).contains(&y) && (16.0..160.0).contains(&x) {
            let slot = ((x - 16.0) / 48.0).floor() as usize;
            if slot < 3 {
                return Some(Target::Blurred(slot));
            }
        }
        if (494.0..538.0).contains(&y) && (16.0..252.0).contains(&x) {
            let row = ((y - 494.0) / 25.0).floor() as usize;
            let col = ((x - 16.0) / 26.0).floor() as usize;
            if row < 2 && col < 9 {
                return Some(Target::Solid(row * 9 + col));
            }
        }
        if (637.0..669.0).contains(&y) && (142.0..250.0).contains(&x) {
            return Some(Target::AutoBalance);
        }
        for slider in [
            Slider::Padding,
            Slider::Inset,
            Slider::Shadow,
            Slider::Corners,
        ] {
            let r = self.slider_rect(slider);
            if (r.x - 7.0..r.x + r.w + 7.0).contains(&x) && (r.y - 5.0..r.y + 25.0).contains(&y) {
                return Some(Target::Slider(slider));
            }
        }
        None
    }
}

fn mix(a: u32, b: u32, t: f32) -> [u8; 3] {
    let component = |shift| {
        let aa = (a >> shift & 255u32) as f32;
        let bb = (b >> shift & 255u32) as f32;
        (aa + (bb - aa) * t).round() as u8
    };
    [component(16), component(8), component(0)]
}

pub fn wallpaper_at(index: usize, x: f32, y: f32) -> [u8; 3] {
    let base = [0x101f4e, 0x251427, 0x6376aa][index];
    let spots = match index {
        0 => [
            (0.1, 0.8, 0x66e7da, 0.55),
            (0.85, 0.2, 0x9a3db8, 0.38),
            (0.65, 1.0, 0x437cec, 0.28),
        ],
        1 => [
            (0.1, 0.1, 0xe85947, 0.48),
            (0.9, 0.8, 0xfbd176, 0.40),
            (0.5, 0.4, 0x883a9f, 0.36),
        ],
        _ => [
            (0.15, 0.15, 0xf3c6bb, 0.53),
            (0.85, 0.3, 0x6068ce, 0.30),
            (0.55, 0.8, 0xffdcad, 0.44),
        ],
    };
    let mut rgb = mix(base, base, 0.0).map(|v| v as f32);
    for (cx, cy, tint, size) in spots {
        let d = ((x - cx) * (x - cx) + (y - cy) * (y - cy)) / (size * size);
        let weight = (-d * 2.2).exp() * 0.9;
        let color = mix(tint, tint, 0.0);
        for c in 0..3 {
            rgb[c] = rgb[c] * (1.0 - weight) + color[c] as f32 * weight;
        }
    }
    rgb.map(|v| v.round() as u8)
}

pub fn gradient_at(index: usize, x: f32, y: f32) -> [u8; 3] {
    let c = GRADIENTS[index];
    let top = mix(c[0], c[1], x);
    let bottom = mix(c[2], c[3], x);
    let smooth = (y * y * (3.0 - 2.0 * y)).clamp(0.0, 1.0);
    let mut out = [0; 3];
    for i in 0..3 {
        out[i] = (top[i] as f32 * (1.0 - smooth) + bottom[i] as f32 * smooth).round() as u8;
    }
    out
}

const BLUR_GRID: usize = 48;
const WALLPAPER_GRID: usize = 96;
fn wallpaper_grid(index: usize) -> Vec<[u8; 3]> {
    let mut pixels = Vec::with_capacity(WALLPAPER_GRID * WALLPAPER_GRID);
    for y in 0..WALLPAPER_GRID {
        for x in 0..WALLPAPER_GRID {
            pixels.push(wallpaper_at(
                index,
                x as f32 / (WALLPAPER_GRID - 1) as f32,
                y as f32 / (WALLPAPER_GRID - 1) as f32,
            ));
        }
    }
    pixels
}
fn sample_grid(grid: &[[u8; 3]], size: usize, x: f32, y: f32) -> [u8; 3] {
    let fx = (x * (size - 1) as f32).clamp(0.0, (size - 1) as f32);
    let fy = (y * (size - 1) as f32).clamp(0.0, (size - 1) as f32);
    let (x0, y0) = (fx as usize, fy as usize);
    let (x1, y1) = ((x0 + 1).min(size - 1), (y0 + 1).min(size - 1));
    let (tx, ty) = (fx - x0 as f32, fy - y0 as f32);
    let mut rgb = [0; 3];
    for c in 0..3 {
        let top = grid[y0 * size + x0][c] as f32 * (1.0 - tx) + grid[y0 * size + x1][c] as f32 * tx;
        let bottom =
            grid[y1 * size + x0][c] as f32 * (1.0 - tx) + grid[y1 * size + x1][c] as f32 * tx;
        rgb[c] = (top * (1.0 - ty) + bottom * ty).round() as u8;
    }
    rgb
}
fn blurred_grid(source: &Raster) -> Vec<[u8; 3]> {
    let mut grid = vec![[0; 3]; BLUR_GRID * BLUR_GRID];
    let radius = (source.width.min(source.height) as f32 * 0.075).max(1.0);
    for gy in 0..BLUR_GRID {
        for gx in 0..BLUR_GRID {
            let cx = (gx as f32 + 0.5) * source.width as f32 / BLUR_GRID as f32;
            let cy = (gy as f32 + 0.5) * source.height as f32 / BLUR_GRID as f32;
            let mut rgb = [0u32; 3];
            for dy in -2..=2 {
                for dx in -2..=2 {
                    let x = (cx + dx as f32 * radius / 2.0).clamp(0.0, (source.width - 1) as f32)
                        as usize;
                    let y = (cy + dy as f32 * radius / 2.0).clamp(0.0, (source.height - 1) as f32)
                        as usize;
                    let p = (y * source.width as usize + x) * 4;
                    rgb[0] += source.pixels[p + 2] as u32;
                    rgb[1] += source.pixels[p + 1] as u32;
                    rgb[2] += source.pixels[p] as u32;
                }
            }
            grid[gy * BLUR_GRID + gx] = [
                (rgb[0] / 25) as u8,
                (rgb[1] / 25) as u8,
                (rgb[2] / 25) as u8,
            ];
        }
    }
    grid
}
fn blurred_at(grid: &[[u8; 3]], x: f32, y: f32, tint: u32) -> [u8; 3] {
    let sample = sample_grid(grid, BLUR_GRID, x, y);
    let tint = mix(tint, tint, 0.0);
    let mut out = [0; 3];
    for c in 0..3 {
        out[c] = (sample[c] as f32 * 0.6 + tint[c] as f32 * 0.4).round() as u8;
    }
    out
}

/// Composite at original-pixel scale. A 256 MiB bound prevents slider drags
/// from allocating an unbounded raster for huge captures.
pub fn compose(source: &Raster, settings: &Background) -> Result<Raster> {
    if !settings.selected() {
        return Ok(Raster {
            width: source.width,
            height: source.height,
            pixels: source.pixels.clone(),
        });
    }
    anyhow::ensure!(source.width > 0 && source.height > 0, "Empty screenshot");
    let shortest = source.width.min(source.height) as f32;
    let pad = (shortest * (0.08 + settings.padding * 0.42)).round() as u32;
    let inset = (shortest * settings.inset * 0.25).round() as u32;
    let frame = pad
        .checked_add(inset)
        .context("Background padding overflow")?;
    let width = source
        .width
        .checked_add(frame.checked_mul(2).context("Background width overflow")?)
        .context("Background width overflow")?;
    let height = source
        .height
        .checked_add(frame.checked_mul(2).context("Background height overflow")?)
        .context("Background height overflow")?;
    let length = u64::from(width) * u64::from(height) * 4;
    anyhow::ensure!(
        length <= 256 * 1024 * 1024,
        "Background output exceeds 256 MiB; reduce padding"
    );
    anyhow::ensure!(
        source.pixels.len() == source.width as usize * source.height as usize * 4,
        "Invalid source raster"
    );
    let blur = matches!(settings.style, Style::Blurred(_)).then(|| blurred_grid(source));
    let wallpaper = if let Style::Wallpaper(index) = settings.style {
        Some(wallpaper_grid(index))
    } else {
        None
    };
    let mut output = vec![0u8; length as usize];
    for y in 0..height {
        let fy = y as f32 / height.max(1) as f32;
        for x in 0..width {
            let fx = x as f32 / width.max(1) as f32;
            let rgb = match settings.style {
                Style::Gradient(i) => gradient_at(i, fx, fy),
                Style::Wallpaper(_) => sample_grid(
                    wallpaper.as_ref().expect("wallpaper grid exists"),
                    WALLPAPER_GRID,
                    fx,
                    fy,
                ),
                Style::Solid(i) => mix(SOLIDS[i], SOLIDS[i], 0.0),
                Style::Blurred(i) => blurred_at(
                    blur.as_ref().expect("blur grid exists"),
                    fx,
                    fy,
                    [0xf4f4f5, 0x9e9ea3, 0x37383d][i],
                ),
                Style::None => unreachable!(),
            };
            let p = ((y * width + x) * 4) as usize;
            output[p..p + 4].copy_from_slice(&[rgb[2], rgb[1], rgb[0], 255]);
        }
    }
    // Keep radii fractional. Rounding here made several slider positions produce
    // identical images, then jump a full pixel when the integer radius changed.
    let radius = (shortest * settings.corners * 0.16)
        .min((source.width.min(source.height) - 1) as f32 * 0.5);
    // Compensate for the downward cast shadow so the visual center aligns with the canvas.
    let balanced_y = frame
        - if settings.auto_balance {
            (shortest * settings.shadow * 0.03).round() as u32
        } else {
            0
        };
    if settings.shadow > 0.0 {
        let extent = shortest * 0.09 * settings.shadow;
        let blur = extent.max(0.5);
        // A nonzero first step used to cast a full-strength shadow because of
        // the minimum blur radius. Fade opacity in before reaching full blur.
        let strength = (extent / 3.0).clamp(0.0, 1.0);
        let offset = extent * 0.35;
        let half_w = (source.width - 1) as f32 * 0.5;
        let half_h = (source.height - 1) as f32 * 0.5;
        for y in 0..height {
            for x in 0..width {
                let sx = x as i32 - frame as i32;
                let sy = y as f32 - balanced_y as f32 - offset;
                // Signed distance to the same rounded screenshot outline used below.
                // Casting a rectangular shadow left square corners when Corners changed.
                let qx = (sx as f32 - half_w).abs() - (half_w - radius);
                let qy = (sy - half_h).abs() - (half_h - radius);
                let distance = ((qx.max(0.0).powi(2) + qy.max(0.0).powi(2)).sqrt()
                    + qx.max(qy).min(0.0)
                    - radius)
                    .max(0.0);
                if distance > blur * 2.5 {
                    continue;
                }
                let opacity = (0.28 * strength * (1.0 - distance / (blur * 2.5))).clamp(0.0, 0.28);
                let p = ((y * width + x) * 4) as usize;
                for channel in 0..3 {
                    output[p + channel] =
                        (output[p + channel] as f32 * (1.0 - opacity)).round() as u8;
                }
            }
        }
    }
    for y in 0..source.height {
        for x in 0..source.width {
            let xx = x as f32;
            let yy = y as f32;
            let cx = xx.clamp(radius, source.width as f32 - radius - 1.0);
            let cy = yy.clamp(radius, source.height as f32 - radius - 1.0);
            let coverage = if radius > 0.0 {
                let edge =
                    (radius + 0.5 - ((xx - cx).powi(2) + (yy - cy).powi(2)).sqrt()).clamp(0.0, 1.0);
                // Fade into rounding when radius is less than one output pixel.
                // At radius zero, an opaque square still covers the entire corner.
                1.0 - (1.0 - edge) * radius.min(1.0)
            } else {
                1.0
            };
            if coverage == 0.0 {
                continue;
            }
            let out = (((y + balanced_y) * width + x + frame) * 4) as usize;
            let input = ((y * source.width + x) * 4) as usize;
            // Apply source alpha and antialiased rounded-corner coverage to the backdrop.
            let alpha = (source.pixels[input + 3] as f32 * coverage).round() as u16;
            for channel in 0..3 {
                output[out + channel] = ((source.pixels[input + channel] as u16 * alpha
                    + output[out + channel] as u16 * (255 - alpha)
                    + 127)
                    / 255) as u8;
            }
        }
    }
    Ok(Raster {
        width,
        height,
        pixels: output,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn shadow_fades_in_at_first_slider_step() -> Result<()> {
        let source = Raster {
            width: 350,
            height: 200,
            pixels: [255, 0, 0, 255].repeat(350 * 200),
        };
        let settings = Background {
            style: Style::Solid(1),
            shadow: 1.0 / 102.0,
            corners: 0.0,
            ..Default::default()
        };
        let image = compose(&source, &settings)?;
        let frame = (image.width - source.width) / 2;
        let edge =
            (((frame + source.height) * image.width + frame + source.width / 2) * 4) as usize;
        assert!(
            image.pixels[edge] >= 235,
            "first shadow step should be subtle"
        );
        Ok(())
    }
    #[test]
    fn shadow_follows_rounded_screenshot_corners() -> Result<()> {
        let source = Raster {
            width: 200,
            height: 120,
            pixels: [255, 0, 0, 255].repeat(200 * 120),
        };
        let square = Background {
            style: Style::Solid(1),
            corners: 0.0,
            shadow: 1.0,
            ..Default::default()
        };
        let rounded = Background {
            corners: 1.0,
            ..square.clone()
        };
        let a = compose(&source, &square)?;
        let b = compose(&source, &rounded)?;
        let frame = (a.width - source.width) / 2;
        let corner = (((frame - 1) * a.width + frame - 1) * 4) as usize;
        let center = (((frame - 1) * a.width + frame + source.width / 2) * 4) as usize;
        assert!(
            b.pixels[corner] > a.pixels[corner],
            "rounded shadow corner should fade sooner"
        );
        assert_eq!(&a.pixels[center..center + 3], &b.pixels[center..center + 3]);
        let no_shadow = Background {
            shadow: 0.0,
            ..square
        };
        assert_eq!(
            &compose(&source, &no_shadow)?.pixels[corner..corner + 3],
            &compose(
                &source,
                &Background {
                    corners: 1.0,
                    ..no_shadow
                }
            )?
            .pixels[corner..corner + 3]
        );
        Ok(())
    }
    #[test]
    fn small_editor_scrolls_to_reach_bottom_controls() {
        let b = Background {
            open: true,
            scroll_y: 350.0,
            ..Default::default()
        };
        assert!(b.max_scroll(460.0) >= 350.0);
        assert_eq!(
            b.hit(50.0, PANEL_TOP + 701.0 - 350.0 + 10.0),
            Some(Target::Slider(Slider::Shadow))
        );
    }
    #[test]
    fn balance_offsets_content_for_downward_shadow() -> Result<()> {
        let source = Raster {
            width: 200,
            height: 200,
            pixels: [0, 0, 255, 255].repeat(200 * 200),
        };
        let b = Background {
            style: Style::Solid(1),
            auto_balance: false,
            ..Default::default()
        };
        let normal = compose(&source, &b)?;
        let balanced = compose(
            &source,
            &Background {
                auto_balance: true,
                ..b
            },
        )?;
        assert_eq!(normal.width, balanced.width);
        assert_ne!(normal.pixels, balanced.pixels);
        Ok(())
    }
    #[test]
    fn blurred_preset_differs_from_unblurred_source() -> Result<()> {
        let mut pixels = [255, 255, 255, 255].repeat(40 * 40);
        pixels[(20 * 40 + 20) * 4..(20 * 40 + 20) * 4 + 4].copy_from_slice(&[0, 0, 0, 255]);
        let source = Raster {
            width: 40,
            height: 40,
            pixels,
        };
        let b = Background {
            style: Style::Blurred(0),
            ..Default::default()
        };
        let composite = compose(&source, &b)?;
        let pad = (composite.width - source.width) / 2;
        let background_pixel = ((pad / 2 * composite.width + pad / 2) * 4) as usize;
        assert_eq!(composite.pixels[background_pixel + 3], 255);
        assert!(blurred_grid(&source).iter().any(|c| c[0] < 255));
        Ok(())
    }
    #[test]
    fn output_expands_without_touching_source() -> Result<()> {
        let source = Raster {
            width: 20,
            height: 20,
            pixels: [0, 0, 255, 255].repeat(20 * 20),
        };
        let before = source.pixels.clone();
        let b = Background {
            style: Style::Gradient(12),
            ..Default::default()
        };
        let composed = compose(&source, &b)?;
        assert!(composed.width > source.width && composed.height > source.height);
        let frame = (composed.width - source.width) / 2;
        let i = (((frame + source.height / 2) * composed.width + frame + source.width / 2) * 4)
            as usize;
        assert_eq!(&composed.pixels[i..i + 4], &[0, 0, 255, 255]);
        assert_eq!(source.pixels, before);
        Ok(())
    }
}
