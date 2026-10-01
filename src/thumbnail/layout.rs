use anyhow::Result;

/// Physical monitor work area, which can have a negative desktop origin.
#[derive(Clone, Copy)]
pub struct WorkArea {
    pub left: i32,
    pub top: i32,
    pub width: u32,
    pub height: u32,
}

/// Geometry is provisional until compared against a native macOS reference.
#[derive(Clone, Copy)]
pub struct Layout {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
    pub scale: f32,
    pub image_left: f32,
    pub image_top: f32,
    pub image_width: f32,
    pub image_height: f32,
    pub radius: f32,
}

impl Layout {
    pub fn new(area: WorkArea, dpi: u32, image_width: u32, image_height: u32) -> Result<Self> {
        anyhow::ensure!(
            area.width > 0 && area.height > 0 && dpi > 0 && image_width > 0 && image_height > 0,
            "Invalid thumbnail dimensions or monitor work area"
        );
        let scale = dpi as f32 / 96.0;
        let margin = (18.0 * scale)
            .round()
            .min(area.width.min(area.height) as f32 / 4.0)
            .floor() as u32;
        let padding = (14.0 * scale).round().min(margin as f32) as u32;
        let max_width = (220.0 * scale).min((area.width - margin * 2).max(1) as f32);
        let max_height = (160.0 * scale).min((area.height - margin * 2).max(1) as f32);
        let fit = (max_width / image_width as f32).min(max_height / image_height as f32);
        let card_width = (image_width as f32 * fit).round().max(1.0) as u32;
        let card_height = (image_height as f32 * fit).round().max(1.0) as u32;
        let width = card_width + padding + margin;
        let height = card_height + padding + margin;
        Ok(Self {
            x: area.left + (area.width - width) as i32,
            y: area.top + (area.height - height) as i32,
            width,
            height,
            scale,
            image_left: padding as f32 / scale,
            image_top: padding as f32 / scale,
            image_width: card_width as f32 / scale,
            image_height: card_height as f32 / scale,
            radius: 6.0_f32.min(card_width.min(card_height) as f32 / (scale * 2.0)),
        })
    }

    pub fn contains(&self, client_x: f32, client_y: f32) -> bool {
        let x = client_x / self.scale - self.image_left;
        let y = client_y / self.scale - self.image_top;
        if x < 0.0 || y < 0.0 || x >= self.image_width || y >= self.image_height {
            return false;
        }
        let near_x = x.clamp(self.radius, self.image_width - self.radius);
        let near_y = y.clamp(self.radius, self.image_height - self.radius);
        (x - near_x).powi(2) + (y - near_y).powi(2) <= self.radius.powi(2)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn anchors_to_work_area_at_all_supported_scales() -> Result<()> {
        let area = WorkArea {
            left: -1920,
            top: -200,
            width: 1920,
            height: 1040,
        };
        for dpi in [96, 120, 144, 192] {
            let layout = Layout::new(area, dpi, 350, 200)?;
            let right = layout.x as f32 + (layout.image_left + layout.image_width) * layout.scale;
            let bottom = layout.y as f32 + (layout.image_top + layout.image_height) * layout.scale;
            let margin = (18.0 * layout.scale).round();
            assert!((right - (area.left as f32 + area.width as f32 - margin)).abs() < 0.01);
            assert!((bottom - (area.top as f32 + area.height as f32 - margin)).abs() < 0.01);
            assert!((layout.image_width / layout.image_height - 1.75).abs() < 0.015);
        }
        Ok(())
    }

    #[test]
    fn extreme_aspects_and_small_work_areas_stay_inside_screen() -> Result<()> {
        for (width, height) in [(1, 1), (80, 60), (1920, 1040)] {
            for (image_width, image_height) in [(1, 10000), (10000, 1), (1, 1)] {
                let layout = Layout::new(
                    WorkArea {
                        left: 0,
                        top: 0,
                        width,
                        height,
                    },
                    192,
                    image_width,
                    image_height,
                )?;
                assert!(layout.x >= 0 && layout.y >= 0);
                assert!(layout.width <= width && layout.height <= height);
            }
        }
        Ok(())
    }

    #[test]
    fn shadow_and_transparent_corners_are_not_interactive() -> Result<()> {
        let layout = Layout::new(
            WorkArea {
                left: 0,
                top: 0,
                width: 1920,
                height: 1040,
            },
            96,
            350,
            200,
        )?;
        assert!(!layout.contains(0.0, 0.0));
        assert!(!layout.contains(layout.image_left, layout.image_top));
        assert!(layout.contains(
            layout.image_left + layout.image_width / 2.0,
            layout.image_top + layout.image_height / 2.0
        ));
        Ok(())
    }
}
