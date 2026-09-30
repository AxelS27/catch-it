use std::{
    os::windows::ffi::OsStrExt,
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
    time::{SystemTime, UNIX_EPOCH},
};

use anyhow::{Context, Result};
use windows::{
    Win32::{Graphics::Imaging::*, System::Com::*},
    core::PCWSTR,
};

use crate::{capture::Snapshot, geometry::Region};

static SEQUENCE: AtomicU64 = AtomicU64::new(0);

pub struct ComApartment;
impl ComApartment {
    pub fn new() -> Result<Self> {
        // SAFETY: Each participating thread initializes and balances its own apartment.
        unsafe {
            CoInitializeEx(None, COINIT_APARTMENTTHREADED).ok()?;
        }
        Ok(Self)
    }
}
impl Drop for ComApartment {
    fn drop(&mut self) {
        unsafe {
            CoUninitialize();
        }
    }
}

pub fn save_png(snapshot: &Snapshot, region: Region) -> Result<PathBuf> {
    let pixels = snapshot.crop(region)?;
    let folder =
        PathBuf::from(std::env::var_os("LOCALAPPDATA").context("LOCALAPPDATA is not set")?)
            .join("SimpleScreenshot")
            .join("Temp");
    std::fs::create_dir_all(&folder)?;
    let timestamp = SystemTime::now().duration_since(UNIX_EPOCH)?.as_millis();
    let seq = SEQUENCE.fetch_add(1, Ordering::Relaxed);
    let path = folder.join(format!("shot_{timestamp}_{}_{seq}.png", std::process::id()));
    // Write privately first; never expose an incomplete .png to future drag consumers.
    let pending = path.with_extension("png.part");
    let result = encode(&pending, region.width, region.height, &pixels)
        .and_then(|()| std::fs::rename(&pending, &path).context("Cannot finalize PNG"));
    if let Err(error) = result {
        let _ = std::fs::remove_file(&pending);
        return Err(error);
    }
    Ok(path)
}

fn encode(path: &std::path::Path, width: u32, height: u32, pixels: &[u8]) -> Result<()> {
    let _com = ComApartment::new()?;
    let filename: Vec<u16> = path.as_os_str().encode_wide().chain(Some(0)).collect();
    // SAFETY: COM is initialized on this thread. Pixel storage and UTF-16 filename
    // remain alive for each synchronous WIC call. Streams close before rename.
    unsafe {
        let factory: IWICImagingFactory =
            CoCreateInstance(&CLSID_WICImagingFactory, None, CLSCTX_INPROC_SERVER)?;
        let stream = factory.CreateStream()?;
        stream.InitializeFromFilename(PCWSTR(filename.as_ptr()), 0x40000000)?; // GENERIC_WRITE
        let encoder = factory.CreateEncoder(&GUID_ContainerFormatPng, std::ptr::null())?;
        encoder.Initialize(&stream, WICBitmapEncoderNoCache)?;
        let mut frame = None;
        let mut properties = None;
        encoder.CreateNewFrame(&mut frame, &mut properties)?;
        let frame = frame.context("WIC returned no frame")?;
        frame.Initialize(properties.as_ref())?;
        frame.SetSize(width, height)?;
        let mut format = GUID_WICPixelFormat32bppBGRA;
        frame.SetPixelFormat(&mut format)?;
        anyhow::ensure!(
            format == GUID_WICPixelFormat32bppBGRA,
            "PNG encoder changed the requested BGRA format"
        );
        frame.WritePixels(height, width * 4, pixels)?;
        frame.Commit()?;
        encoder.Commit()?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use windows::Win32::Foundation::GENERIC_READ;

    struct TestFile(PathBuf);
    impl Drop for TestFile {
        fn drop(&mut self) {
            let _ = std::fs::remove_file(&self.0);
        }
    }

    #[test]
    fn wic_round_trip_preserves_bgra_and_unicode_paths() -> Result<()> {
        let timestamp = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
        let file = TestFile(std::env::temp_dir().join(format!(
            "simple screenshot 日本 {} {timestamp}.png",
            std::process::id()
        )));
        let original = [
            0, 0, 255, 255, 0, 255, 0, 255, 255, 0, 0, 255, 12, 34, 56, 255,
        ];
        encode(&file.0, 2, 2, &original)?;
        let _com = ComApartment::new()?;
        let filename: Vec<u16> = file.0.as_os_str().encode_wide().chain(Some(0)).collect();
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
            let mut width = 0;
            let mut height = 0;
            frame.GetSize(&mut width, &mut height)?;
            assert_eq!((width, height), (2, 2));
            let converter = factory.CreateFormatConverter()?;
            converter.Initialize(
                &frame,
                &GUID_WICPixelFormat32bppBGRA,
                WICBitmapDitherTypeNone,
                None,
                0.0,
                WICBitmapPaletteTypeCustom,
            )?;
            let mut decoded = [0; 16];
            converter.CopyPixels(std::ptr::null(), 8, &mut decoded)?;
            assert_eq!(decoded, original);
        }
        Ok(())
    }
}
