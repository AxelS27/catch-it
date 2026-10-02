//! Original PNG plus lossless top-down DIBV5 for native image paste, never CF_HDROP.
use anyhow::{Context, Result};
use std::{os::windows::ffi::OsStrExt, path::Path};
use windows::{
    Win32::{
        Foundation::*,
        Graphics::Imaging::*,
        System::{Com::*, DataExchange::*, Memory::*},
    },
    core::{PCWSTR, w},
};

struct GlobalBlock(HGLOBAL);
impl GlobalBlock {
    fn new(bytes: &[u8]) -> Result<Self> {
        unsafe {
            let block = Self(GlobalAlloc(GMEM_MOVEABLE, bytes.len())?);
            let pointer = GlobalLock(block.0);
            anyhow::ensure!(!pointer.is_null(), "Cannot lock clipboard image memory");
            std::ptr::copy_nonoverlapping(bytes.as_ptr(), pointer.cast(), bytes.len());
            // A zero lock count reports false without error; the block is still valid.
            let _ = GlobalUnlock(block.0);
            Ok(block)
        }
    }
    fn transfer(&mut self, format: u32) -> Result<()> {
        unsafe {
            SetClipboardData(format, Some(HANDLE(self.0.0)))?;
        }
        self.0 = HGLOBAL::default(); // The clipboard owns successful transfers.
        Ok(())
    }
}
impl Drop for GlobalBlock {
    fn drop(&mut self) {
        if !self.0.is_invalid() {
            unsafe {
                let _ = GlobalFree(Some(self.0));
            }
        }
    }
}

pub struct Image {
    png: GlobalBlock,
    dib: GlobalBlock,
    format: u32,
}
impl Image {
    pub fn new(path: &Path, pixels: &[u8], width: u32, height: u32) -> Result<Self> {
        let dib = dibv5(pixels, width, height)?;
        let png = std::fs::read(path).context("Cannot read source PNG for clipboard")?;
        let format = unsafe { RegisterClipboardFormatW(w!("PNG")) };
        anyhow::ensure!(format != 0, "Cannot register PNG clipboard format");
        Ok(Self {
            png: GlobalBlock::new(&png)?,
            dib: GlobalBlock::new(&dib)?,
            format,
        })
    }
    pub fn from_png(path: &Path) -> Result<Self> {
        let filename: Vec<u16> = path.as_os_str().encode_wide().chain(Some(0)).collect();
        unsafe {
            let factory: IWICImagingFactory =
                CoCreateInstance(&CLSID_WICImagingFactory, None, CLSCTX_INPROC_SERVER)?;
            let decoder = factory.CreateDecoderFromFilename(
                PCWSTR(filename.as_ptr()),
                None,
                GENERIC_READ,
                WICDecodeMetadataCacheOnLoad,
            )?;
            let frame = decoder.GetFrame(0)?;
            let (mut width, mut height) = (0, 0);
            frame.GetSize(&mut width, &mut height)?;
            let length = pixel_length(width, height)?;
            let mut pixels = vec![0; length];
            let converter = factory.CreateFormatConverter()?;
            converter.Initialize(
                &frame,
                &GUID_WICPixelFormat32bppBGRA,
                WICBitmapDitherTypeNone,
                None,
                0.0,
                WICBitmapPaletteTypeCustom,
            )?;
            converter.CopyPixels(std::ptr::null(), width * 4, &mut pixels)?;
            Self::new(path, &pixels, width, height)
        }
    }
    /// false means another application owns the clipboard lock. Retry on a UI
    /// timer, without sleeping the capture/render thread or reallocating images.
    pub fn publish(&mut self, owner: HWND) -> Result<bool> {
        struct Open;
        impl Drop for Open {
            fn drop(&mut self) {
                unsafe {
                    let _ = CloseClipboard();
                }
            }
        }
        unsafe {
            if let Err(error) = OpenClipboard(Some(owner)) {
                if error.code() == windows::core::HRESULT::from_win32(ERROR_ACCESS_DENIED.0) {
                    return Ok(false);
                }
                return Err(error).context("Cannot open image clipboard");
            }
            let _open = Open;
            EmptyClipboard()?;
            self.png.transfer(self.format)?;
            self.dib.transfer(17)?; // CF_DIBV5
        }
        Ok(true)
    }
}

fn pixel_length(width: u32, height: u32) -> Result<usize> {
    let bytes = u64::from(width)
        .checked_mul(u64::from(height))
        .and_then(|bytes| bytes.checked_mul(4))
        .context("Clipboard dimensions overflow")?;
    anyhow::ensure!(
        width > 0
            && height > 0
            && width <= i32::MAX as u32
            && height <= i32::MAX as u32
            && bytes <= u32::MAX as u64,
        "Invalid clipboard image dimensions"
    );
    Ok(bytes as usize)
}
fn dibv5(pixels: &[u8], width: u32, height: u32) -> Result<Vec<u8>> {
    let length = pixel_length(width, height)?;
    anyhow::ensure!(pixels.len() == length, "Invalid clipboard image buffer");
    let mut dib = vec![0; 124 + length];
    for (offset, value) in [
        (0, 124),
        (4, width),
        (8, (-(height as i32)) as u32),
        (16, 3),
        (20, length as u32), // BI_BITFIELDS
        (40, 0x00ff0000),
        (44, 0x0000ff00),
        (48, 0x000000ff),
        (52, 0xff000000),
        (56, 0x73524742),
        (108, 4), // sRGB, LCS_GM_IMAGES
    ] {
        dib[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
    }
    dib[12..14].copy_from_slice(&1u16.to_le_bytes());
    dib[14..16].copy_from_slice(&32u16.to_le_bytes());
    dib[124..].copy_from_slice(pixels);
    Ok(dib)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn clipboard_pixels_are_original_bgra_top_down_not_preview_crop() -> Result<()> {
        let pixels = [
            0, 0, 255, 255, 0, 255, 0, 255, 255, 0, 0, 255, 12, 34, 56, 128,
        ];
        let dib = dibv5(&pixels, 2, 2)?;
        assert_eq!(dib.len(), 140);
        assert_eq!(i32::from_le_bytes(dib[8..12].try_into()?), -2);
        assert_eq!(&dib[124..], pixels);
        assert_eq!(u32::from_le_bytes(dib[52..56].try_into()?), 0xff000000);
        Ok(())
    }
    #[test]
    fn invalid_clipboard_dimensions_fail_before_allocation() {
        for (w, h) in [(0, 1), (1, 0), (u32::MAX, u32::MAX), (2, 2)] {
            assert!(dibv5(&[0; 4], w, h).is_err());
        }
    }
}
