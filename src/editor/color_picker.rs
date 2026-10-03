//! Native, image-independent color controls for the annotation toolbar.
use super::layout::{Layout, Rect};

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Region {
    Spectrum,
    Hue,
    Hex,
    Done,
    Preset(usize),
}

#[derive(Default)]
pub struct Picker {
    pub open: bool,
    pub hue: f32,
    pub saturation: f32,
    pub value: f32,
    pub hex: String,
    pub editing_hex: bool,
    pub dragging: Option<Region>,
}
impl Picker {
    pub fn open(&mut self, rgb: u32) {
        (self.hue, self.saturation, self.value) = rgb_to_hsv(rgb);
        self.hex = format!("{rgb:06X}");
        self.open = true;
        self.editing_hex = false;
        self.dragging = None;
    }
    pub fn color(&self) -> u32 {
        hsv_to_rgb(self.hue, self.saturation, self.value)
    }
    pub fn update(&mut self, region: Region, x: f32, y: f32, layout: &Layout) -> Option<u32> {
        let r = rect(layout)?;
        match region {
            Region::Spectrum => {
                self.saturation = ((x - (r.x + 62.0)) / 252.0).clamp(0.0, 1.0);
                self.value = (1.0 - (y - (r.y + 17.0)) / 162.0).clamp(0.0, 1.0);
            }
            Region::Hue => self.hue = ((x - (r.x + 62.0)) / 252.0).clamp(0.0, 1.0),
            _ => return None,
        }
        let rgb = self.color();
        self.hex = format!("{rgb:06X}");
        Some(rgb)
    }
    pub fn commit_hex(&mut self) -> Option<u32> {
        let value = self.hex.trim().trim_start_matches('#');
        let color = if value.len() == 6 {
            u32::from_str_radix(value, 16).ok()?
        } else if value.len() == 3 {
            let digits = u32::from_str_radix(value, 16).ok()?;
            ((digits & 0xf00) * 0x1100) | ((digits & 0x0f0) * 0x110) | ((digits & 0x00f) * 0x11)
        } else {
            return None;
        };
        (self.hue, self.saturation, self.value) = rgb_to_hsv(color);
        self.hex = format!("{color:06X}");
        self.editing_hex = false;
        Some(color)
    }
}
pub fn rect(layout: &Layout) -> Option<Rect> {
    let color = layout.rect(super::layout::Control::Color)?;
    let w = 340.0;
    let h = 360.0;
    Some(Rect {
        x: (color.x - 220.0).clamp(8.0, (layout.width - w - 8.0).max(8.0)),
        y: (layout.height - super::layout::BOTTOM - h).clamp(8.0, super::layout::TOP + 4.0),
        w,
        h,
    })
}
pub fn hit(layout: &Layout, x: f32, y: f32) -> Option<Region> {
    let r = rect(layout)?;
    if !r.contains(x, y) {
        return None;
    }
    let (px, py) = (x - r.x, y - r.y);
    if (55.0..=322.0).contains(&px) && (10.0..=187.0).contains(&py) {
        return Some(Region::Spectrum);
    }
    if (55.0..=322.0).contains(&px) && (192.0..=232.0).contains(&py) {
        return Some(Region::Hue);
    }
    if (55.0..=322.0).contains(&px) && (235.0..=281.0).contains(&py) {
        return Some(Region::Hex);
    }
    if (230.0..=322.0).contains(&px) && (321.0..=354.0).contains(&py) {
        return Some(Region::Done);
    }
    if (4.0..=50.0).contains(&px) && (5.0..=344.0).contains(&py) {
        let row = ((py - 7.0) / 30.0).floor() as isize;
        if (0..=10).contains(&row) {
            return Some(Region::Preset(row as usize));
        }
    }
    None
}
pub fn hsv_to_rgb(h: f32, s: f32, v: f32) -> u32 {
    let h = h.rem_euclid(1.0) * 6.0;
    let (s, v) = (s.clamp(0.0, 1.0), v.clamp(0.0, 1.0));
    let c = v * s;
    let x = c * (1.0 - (h % 2.0 - 1.0).abs());
    let m = v - c;
    let (r, g, b) = match h as u32 {
        0 => (c, x, 0.0),
        1 => (x, c, 0.0),
        2 => (0.0, c, x),
        3 => (0.0, x, c),
        4 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };
    (((r + m) * 255.0).round() as u32) << 16
        | (((g + m) * 255.0).round() as u32) << 8
        | ((b + m) * 255.0).round() as u32
}
pub fn rgb_to_hsv(rgb: u32) -> (f32, f32, f32) {
    let (r, g, b) = (
        ((rgb >> 16) & 255) as f32 / 255.0,
        ((rgb >> 8) & 255) as f32 / 255.0,
        (rgb & 255) as f32 / 255.0,
    );
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let d = max - min;
    let hue = if d < 0.00001 {
        0.0
    } else if max == r {
        ((g - b) / d).rem_euclid(6.0)
    } else if max == g {
        (b - r) / d + 2.0
    } else {
        (r - g) / d + 4.0
    };
    (hue / 6.0, if max == 0.0 { 0.0 } else { d / max }, max)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn picker_roundtrips_colors_and_hex() {
        for rgb in [0x000000, 0xffffff, 0xf92d3a, 0x006dfd, 0xffde00, 0x808080] {
            let (h, s, v) = rgb_to_hsv(rgb);
            assert_eq!(hsv_to_rgb(h, s, v), rgb);
        }
        let mut picker = Picker::default();
        picker.open(0x112233);
        picker.hex = "A1B2C3".into();
        assert_eq!(picker.commit_hex(), Some(0xa1b2c3));
        picker.hex = "BAD!".into();
        assert_eq!(picker.commit_hex(), None);
    }
}
