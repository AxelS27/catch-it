use anyhow::{Context, Result, bail};
use windows::{
    Win32::{
        Foundation::HMODULE,
        Graphics::{
            Direct3D::D3D_DRIVER_TYPE_UNKNOWN,
            Direct3D11::*,
            Dxgi::{Common::*, *},
        },
    },
    core::Interface,
};

use crate::geometry::Region;

/// CPU-owned snapshot, in physical monitor coordinates and tightly packed BGRA.
/// Captured before showing the overlay so our UI never appears in the PNG.
pub struct Snapshot {
    pub left: i32,
    pub top: i32,
    pub width: u32,
    pub height: u32,
    pub pixels: Vec<u8>,
}

impl Snapshot {
    pub fn crop(&self, region: Region) -> Result<Vec<u8>> {
        anyhow::ensure!(
            region.width > 0
                && region.height > 0
                && region
                    .x
                    .checked_add(region.width)
                    .is_some_and(|x| x <= self.width)
                && region
                    .y
                    .checked_add(region.height)
                    .is_some_and(|y| y <= self.height),
            "Selection is outside the snapshot"
        );
        let stride = self.width as usize * 4;
        let row_len = region.width as usize * 4;
        let mut cropped = Vec::with_capacity(row_len * region.height as usize);
        for y in region.y..region.y + region.height {
            let start = y as usize * stride + region.x as usize * 4;
            cropped.extend_from_slice(&self.pixels[start..start + row_len]);
        }
        Ok(cropped)
    }
}

struct AcquiredFrame<'a>(&'a IDXGIOutputDuplication);
impl Drop for AcquiredFrame<'_> {
    fn drop(&mut self) {
        // SAFETY: Constructed only after a successful acquisition. Release exactly once.
        unsafe {
            let _ = self.0.ReleaseFrame();
        }
    }
}

/// Reused on a single worker thread so every hotkey does not recreate a GPU device.
pub struct CaptureSession {
    monitor: isize,
    desc: DXGI_OUTPUT_DESC,
    device: ID3D11Device,
    context: ID3D11DeviceContext,
    duplication: IDXGIOutputDuplication,
    staging: Option<ID3D11Texture2D>,
}

impl CaptureSession {
    pub fn monitor(&self) -> isize {
        self.monitor
    }

    pub fn new(monitor: isize) -> Result<Self> {
        unsafe {
            let factory: IDXGIFactory1 = CreateDXGIFactory1()?;
            let mut adapter_index = 0;
            loop {
                let adapter = match factory.EnumAdapters1(adapter_index) {
                    Ok(adapter) => adapter,
                    Err(error) if error.code() == DXGI_ERROR_NOT_FOUND => break,
                    Err(error) => return Err(error.into()),
                };
                adapter_index += 1;
                let mut output_index = 0;
                loop {
                    let output = match adapter.EnumOutputs(output_index) {
                        Ok(output) => output,
                        Err(error) if error.code() == DXGI_ERROR_NOT_FOUND => break,
                        Err(error) => return Err(error.into()),
                    };
                    output_index += 1;
                    let desc = output.GetDesc()?;
                    if desc.Monitor.0 as isize != monitor || !desc.AttachedToDesktop.as_bool() {
                        continue;
                    }
                    let output1: IDXGIOutput1 = output.cast()?;
                    let mut device = None;
                    let mut context = None;
                    D3D11CreateDevice(
                        &adapter,
                        D3D_DRIVER_TYPE_UNKNOWN,
                        HMODULE::default(),
                        D3D11_CREATE_DEVICE_BGRA_SUPPORT,
                        None,
                        D3D11_SDK_VERSION,
                        Some(&mut device),
                        None,
                        Some(&mut context),
                    )?;
                    let device = device.context("D3D11 returned no device")?;
                    let context = context.context("D3D11 returned no context")?;
                    let duplication = output1.DuplicateOutput(&device).context(
                        "Cannot capture this display (secure desktop or unsupported GPU/session)",
                    )?;
                    return Ok(Self {
                        monitor,
                        desc,
                        device,
                        context,
                        duplication,
                        staging: None,
                    });
                }
            }
        }
        bail!("No capturable DXGI output found for the selected monitor")
    }

    pub fn capture(&mut self) -> Result<Snapshot> {
        // SAFETY: All GPU resources stay on this worker thread. Acquired frames
        // have an RAII guard and mapped rows are copied before unmapping.
        unsafe {
            let mut frame_info = DXGI_OUTDUPL_FRAME_INFO::default();
            let mut resource = None;
            self.duplication
                .AcquireNextFrame(1000, &mut frame_info, &mut resource)
                .context("No desktop frame available; try the shortcut again")?;
            let frame = AcquiredFrame(&self.duplication);
            let texture: ID3D11Texture2D = resource.context("DXGI returned no frame")?.cast()?;
            let mut texture_desc = D3D11_TEXTURE2D_DESC::default();
            texture.GetDesc(&mut texture_desc);
            if self.staging.is_none() {
                let staging_desc = D3D11_TEXTURE2D_DESC {
                    Usage: D3D11_USAGE_STAGING,
                    BindFlags: 0,
                    CPUAccessFlags: D3D11_CPU_ACCESS_READ.0 as u32,
                    MiscFlags: 0,
                    ..texture_desc
                };
                self.device
                    .CreateTexture2D(&staging_desc, None, Some(&mut self.staging))?;
            }
            let staging = self
                .staging
                .as_ref()
                .context("D3D11 returned no staging texture")?;
            self.context.CopyResource(staging, &texture);
            drop(frame);
            let mut mapped = D3D11_MAPPED_SUBRESOURCE::default();
            self.context
                .Map(staging, 0, D3D11_MAP_READ, 0, Some(&mut mapped))?;
            let width = texture_desc.Width;
            let height = texture_desc.Height;
            let stride = width as usize * 4;
            let mut pixels = vec![0; stride * height as usize];
            for y in 0..height as usize {
                let src = std::slice::from_raw_parts(
                    mapped.pData.cast::<u8>().add(y * mapped.RowPitch as usize),
                    stride,
                );
                pixels[y * stride..(y + 1) * stride].copy_from_slice(src);
            }
            self.context.Unmap(staging, 0);
            for pixel in pixels.chunks_exact_mut(4) {
                pixel[3] = 255;
            }
            let desktop_width =
                (self.desc.DesktopCoordinates.right - self.desc.DesktopCoordinates.left) as u32;
            let desktop_height =
                (self.desc.DesktopCoordinates.bottom - self.desc.DesktopCoordinates.top) as u32;
            let (pixels, width, height) = orient_pixels(pixels, width, height, self.desc.Rotation)?;
            anyhow::ensure!(
                width == desktop_width && height == desktop_height,
                "Display changed during capture; please retry"
            );
            Ok(Snapshot {
                left: self.desc.DesktopCoordinates.left,
                top: self.desc.DesktopCoordinates.top,
                width,
                height,
                pixels,
            })
        }
    }
}

/// Duplication surfaces are unrotated, even for portrait monitors.
fn orient_pixels(
    pixels: Vec<u8>,
    width: u32,
    height: u32,
    rotation: DXGI_MODE_ROTATION,
) -> Result<(Vec<u8>, u32, u32)> {
    if rotation == DXGI_MODE_ROTATION_IDENTITY || rotation == DXGI_MODE_ROTATION_UNSPECIFIED {
        return Ok((pixels, width, height));
    }
    let (out_width, out_height) = match rotation {
        DXGI_MODE_ROTATION_ROTATE90 | DXGI_MODE_ROTATION_ROTATE270 => (height, width),
        DXGI_MODE_ROTATION_ROTATE180 => (width, height),
        _ => bail!("Unsupported display rotation"),
    };
    let mut output = vec![0; pixels.len()];
    for y in 0..height {
        for x in 0..width {
            let (dx, dy) = match rotation {
                DXGI_MODE_ROTATION_ROTATE90 => (height - 1 - y, x),
                DXGI_MODE_ROTATION_ROTATE180 => (width - 1 - x, height - 1 - y),
                _ => (y, width - 1 - x),
            };
            let src = ((y * width + x) * 4) as usize;
            let dst = ((dy * out_width + dx) * 4) as usize;
            output[dst..dst + 4].copy_from_slice(&pixels[src..src + 4]);
        }
    }
    Ok((output, out_width, out_height))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crop_preserves_exact_pixels_without_overlay() {
        let pixels: Vec<u8> = (0..48).collect();
        let snapshot = Snapshot {
            left: -100,
            top: 0,
            width: 4,
            height: 3,
            pixels,
        };
        assert_eq!(
            snapshot
                .crop(Region {
                    x: 1,
                    y: 1,
                    width: 2,
                    height: 2
                })
                .unwrap(),
            [
                20, 21, 22, 23, 24, 25, 26, 27, 36, 37, 38, 39, 40, 41, 42, 43
            ]
        );
    }

    #[test]
    fn crop_rejects_invalid_bounds() {
        let snapshot = Snapshot {
            left: 0,
            top: 0,
            width: 4,
            height: 3,
            pixels: vec![0; 48],
        };
        for region in [
            Region {
                x: 4,
                y: 0,
                width: 1,
                height: 1,
            },
            Region {
                x: u32::MAX,
                y: 0,
                width: 2,
                height: 1,
            },
            Region {
                x: 0,
                y: 0,
                width: 0,
                height: 1,
            },
        ] {
            assert!(snapshot.crop(region).is_err());
        }
    }

    #[test]
    fn rotates_portrait_and_upside_down_displays() {
        let pixels: Vec<u8> = [1, 2, 3, 4, 5, 6]
            .into_iter()
            .flat_map(|p| [p, p, p, 255])
            .collect();
        for (rotation, expected, size) in [
            (DXGI_MODE_ROTATION_ROTATE90, vec![5, 3, 1, 6, 4, 2], (3, 2)),
            (DXGI_MODE_ROTATION_ROTATE180, vec![6, 5, 4, 3, 2, 1], (2, 3)),
            (DXGI_MODE_ROTATION_ROTATE270, vec![2, 4, 6, 1, 3, 5], (3, 2)),
        ] {
            let (result, w, h) = orient_pixels(pixels.clone(), 2, 3, rotation).unwrap();
            assert_eq!((w, h), size);
            assert_eq!(
                result.chunks_exact(4).map(|p| p[0]).collect::<Vec<_>>(),
                expected
            );
        }
    }
}
