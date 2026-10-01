//! Native Shell IDataObject supplies CF_HDROP and Shell metadata; OLE owns the
//! modal drag loop. We do not implement file transfers or terminal text injection.
use std::{cell::Cell, os::windows::ffi::OsStrExt, path::Path, rc::Rc};

use anyhow::{Context, Result};
use windows::{
    Win32::{
        Foundation::*,
        Graphics::Gdi::*,
        System::{
            Com::*,
            LibraryLoader::GetModuleHandleW,
            Ole::*,
            SystemServices::{MK_LBUTTON, MODIFIERKEYS_FLAGS},
        },
        UI::{Shell::*, WindowsAndMessaging::*},
    },
    core::{BOOL, HRESULT, PCWSTR, implement, w},
};

pub struct OleApartment;
impl OleApartment {
    pub fn new() -> Result<Self> {
        // OLE drag-and-drop requires OleInitialize, not just CoInitializeEx.
        unsafe {
            OleInitialize(None)?;
        }
        Ok(Self)
    }
}
impl Drop for OleApartment {
    fn drop(&mut self) {
        unsafe {
            OleUninitialize();
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum Outcome {
    Copied,
    Canceled,
}

// Non-agile: OLE must marshal callbacks back to the UI STA that owns Rc/Cell.
#[implement(IDropSource, Agile = false)]
struct DropSource {
    cancel: Rc<Cell<bool>>,
    visual: Option<Rc<DragVisual>>,
}

impl IDropSource_Impl for DropSource_Impl {
    fn QueryContinueDrag(&self, escape: BOOL, keys: MODIFIERKEYS_FLAGS) -> HRESULT {
        if escape.as_bool() || self.cancel.get() || crate::exit_requested() {
            DRAGDROP_S_CANCEL
        } else if keys.0 & MK_LBUTTON.0 == 0 {
            DRAGDROP_S_DROP
        } else {
            if let Some(visual) = &self.visual
                && let Err(error) = visual.move_to_cursor(false)
            {
                return error.code();
            }
            S_OK
        }
    }

    fn GiveFeedback(&self, _effect: DROPEFFECT) -> HRESULT {
        DRAGDROP_S_USEDEFAULTCURSORS
    }
}

pub fn file_data_object(path: &Path) -> Result<IDataObject> {
    anyhow::ensure!(
        path.is_absolute() && path.is_file(),
        "Screenshot file is no longer available: {}",
        path.display()
    );
    let filename: Vec<u16> = path.as_os_str().encode_wide().chain(Some(0)).collect();
    // SAFETY: Called on an initialized STA; the path buffer lives through parsing.
    unsafe {
        let item: IShellItem = SHCreateItemFromParsingName(PCWSTR(filename.as_ptr()), None)?;
        item.BindToHandler(None, &BHID_DataObject)
            .context("Cannot create Shell file data object")
    }
}

pub struct PreparedDrag {
    data: IDataObject,
    source: IDropSource,
    visual: Rc<DragVisual>,
}

impl PreparedDrag {
    pub fn new(
        path: &Path,
        width: u32,
        height: u32,
        pixels: &[u8],
        offset: POINT,
        cancel: Rc<Cell<bool>>,
    ) -> Result<Self> {
        let data = file_data_object(path)?;
        let bitmap = drag_bitmap(pixels, width, height)?;
        let visual = Rc::new(DragVisual::new(&bitmap, width, height, offset)?);
        let source = DropSource {
            cancel,
            visual: Some(Rc::clone(&visual)),
        }
        .into();
        Ok(Self {
            data,
            source,
            visual,
        })
    }

    pub fn run(&self) -> Result<Outcome> {
        self.visual.move_to_cursor(true)?;
        let mut effect = DROPEFFECT_NONE;
        // OLE is initialized on the UI STA. Objects and visual outlive its
        // nested loop; the transparent layered window is never a drop target.
        let status = unsafe { DoDragDrop(&self.data, &self.source, DROPEFFECT_COPY, &mut effect) };
        unsafe {
            let _ = ShowWindow(self.visual.hwnd, SW_HIDE);
        }
        status.ok().context("Windows drag-and-drop failed")?;
        Ok(if status == DRAGDROP_S_DROP && effect == DROPEFFECT_COPY {
            Outcome::Copied
        } else {
            Outcome::Canceled
        })
    }
}

const DRAG_CLASS: PCWSTR = w!("SimpleScreenshot.DragImage");

pub fn register_class() -> Result<()> {
    unsafe {
        let instance = GetModuleHandleW(None)?;
        let class = WNDCLASSEXW {
            cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
            lpfnWndProc: Some(drag_window_proc),
            hInstance: instance.into(),
            lpszClassName: DRAG_CLASS,
            ..Default::default()
        };
        anyhow::ensure!(
            RegisterClassExW(&class) != 0,
            "Cannot register drag-image window"
        );
    }
    Ok(())
}

unsafe extern "system" fn drag_window_proc(
    hwnd: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    unsafe {
        match message {
            WM_NCHITTEST => LRESULT(HTTRANSPARENT as isize),
            WM_MOUSEACTIVATE => LRESULT(MA_NOACTIVATE as isize),
            WM_ERASEBKGND => LRESULT(1),
            _ => DefWindowProcW(hwnd, message, wparam, lparam),
        }
    }
}

/// One cached, premultiplied bitmap. Windows composes it; pointer movement only
/// changes position, with no CPU frame rendering or idle polling.
/// Layered + transparent makes hit testing pass through across process boundaries.
struct DragVisual {
    hwnd: HWND,
    offset: POINT,
}
impl DragVisual {
    fn new(bitmap: &Bitmap, width: u32, height: u32, offset: POINT) -> Result<Self> {
        unsafe {
            let hwnd = CreateWindowExW(
                WS_EX_LAYERED
                    | WS_EX_TRANSPARENT
                    | WS_EX_NOACTIVATE
                    | WS_EX_TOOLWINDOW
                    | WS_EX_TOPMOST,
                DRAG_CLASS,
                w!(""),
                WS_POPUP,
                0,
                0,
                width as i32,
                height as i32,
                None,
                None,
                Some(GetModuleHandleW(None)?.into()),
                None,
            )?;
            let visual = Self { hwnd, offset };
            let dc = CreateCompatibleDC(None);
            if dc.is_invalid() {
                return Err(windows::core::Error::from_thread().into());
            }
            let old = SelectObject(dc, bitmap.0.into());
            if old.is_invalid() {
                let error = windows::core::Error::from_thread();
                let _ = DeleteDC(dc);
                return Err(error).context("Cannot select drag bitmap");
            }
            let size = SIZE {
                cx: width as i32,
                cy: height as i32,
            };
            let origin = POINT::default();
            let blend = BLENDFUNCTION {
                BlendOp: AC_SRC_OVER as u8,
                SourceConstantAlpha: 255,
                AlphaFormat: AC_SRC_ALPHA as u8,
                ..Default::default()
            };
            let upload = UpdateLayeredWindow(
                hwnd,
                None,
                None,
                Some(&size),
                Some(dc),
                Some(&origin),
                COLORREF(0),
                Some(&blend),
                ULW_ALPHA,
            );
            SelectObject(dc, old);
            let _ = DeleteDC(dc);
            upload.context("Cannot upload native drag image")?;
            Ok(visual)
        }
    }

    fn move_to_cursor(&self, show: bool) -> windows::core::Result<()> {
        unsafe {
            let mut cursor = POINT::default();
            GetCursorPos(&mut cursor)?;
            SetWindowPos(
                self.hwnd,
                Some(HWND_TOPMOST),
                cursor.x - self.offset.x,
                cursor.y - self.offset.y,
                0,
                0,
                SWP_NOACTIVATE
                    | SWP_NOSIZE
                    | if show {
                        SWP_SHOWWINDOW
                    } else {
                        SET_WINDOW_POS_FLAGS(0)
                    },
            )
        }
    }
}
impl Drop for DragVisual {
    fn drop(&mut self) {
        unsafe {
            let _ = DestroyWindow(self.hwnd);
        }
    }
}

struct Bitmap(HBITMAP);
impl Drop for Bitmap {
    fn drop(&mut self) {
        unsafe {
            let _ = DeleteObject(self.0.into());
        }
    }
}

fn drag_bitmap(pixels: &[u8], width: u32, height: u32) -> Result<Bitmap> {
    let len = (width as usize)
        .checked_mul(height as usize)
        .and_then(|size| size.checked_mul(4))
        .context("Drag image is too large")?;
    anyhow::ensure!(
        width > 0
            && height > 0
            && width <= i32::MAX as u32
            && height <= i32::MAX as u32
            && pixels.len() == len,
        "Invalid drag-image dimensions or buffer"
    );
    // GPU card readback is already rounded, bordered and premultiplied. Copy it
    // unchanged to a top-down DIB; no PNG decoding, second crop or alpha conversion.
    unsafe {
        let info = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: width as i32,
                biHeight: -(height as i32),
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB.0,
                ..Default::default()
            },
            ..Default::default()
        };
        let mut bits = std::ptr::null_mut();
        let bitmap = Bitmap(CreateDIBSection(
            None,
            &info,
            DIB_RGB_COLORS,
            &mut bits,
            None,
            0,
        )?);
        anyhow::ensure!(!bits.is_null(), "GDI returned no drag-image pixels");
        std::ptr::copy_nonoverlapping(pixels.as_ptr(), bits.cast(), pixels.len());
        Ok(bitmap)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use windows::core::Interface;

    #[test]
    fn native_source_cancels_drops_and_uses_system_feedback() {
        let cancel = Rc::new(Cell::new(false));
        let source: IDropSource = DropSource {
            cancel: Rc::clone(&cancel),
            visual: None,
        }
        .into();
        unsafe {
            assert!(source.cast::<IAgileObject>().is_err());
            assert_eq!(source.QueryContinueDrag(false, MK_LBUTTON), S_OK);
            assert_eq!(
                source.QueryContinueDrag(true, MK_LBUTTON),
                DRAGDROP_S_CANCEL
            );
            assert_eq!(
                source.QueryContinueDrag(false, MODIFIERKEYS_FLAGS(0)),
                DRAGDROP_S_DROP
            );
            assert_eq!(
                source.GiveFeedback(DROPEFFECT_COPY),
                DRAGDROP_S_USEDEFAULTCURSORS
            );
            cancel.set(true);
            assert_eq!(
                source.QueryContinueDrag(false, MK_LBUTTON),
                DRAGDROP_S_CANCEL
            );
        }
    }

    #[test]
    fn drag_bitmap_rejects_invalid_buffers() {
        assert!(drag_bitmap(&[], 1, 1).is_err());
        assert!(drag_bitmap(&[], 0, 1).is_err());
        assert!(drag_bitmap(&[0; 4], u32::MAX, u32::MAX).is_err());
    }

    #[test]
    fn drag_bitmap_preserves_cached_premultiplied_pixels() -> Result<()> {
        let pixels = [
            0, 0, 0, 0, 32, 64, 128, 128, 100, 0, 255, 255, 0, 255, 0, 255,
        ];
        let bitmap = drag_bitmap(&pixels, 2, 2)?;
        let mut actual = [0u8; 16];
        unsafe {
            assert_eq!(
                GetBitmapBits(bitmap.0, actual.len() as i32, actual.as_mut_ptr().cast()),
                16
            );
        }
        assert_eq!(actual, pixels);
        Ok(())
    }

    #[test]
    fn shell_hdrop_preserves_unicode_spaces_and_file_bytes() -> Result<()> {
        let _ole = OleApartment::new()?;
        let path =
            std::env::temp_dir().join(format!("screenshot drag 日本 {}.png", std::process::id()));
        struct Cleanup(std::path::PathBuf);
        impl Drop for Cleanup {
            fn drop(&mut self) {
                let _ = std::fs::remove_file(&self.0);
            }
        }
        let _cleanup = Cleanup(path.clone());
        std::fs::write(&path, b"native data-object test")?;
        unsafe {
            let data = file_data_object(&path)?;
            let format = FORMATETC {
                cfFormat: 15,
                dwAspect: DVASPECT_CONTENT.0,
                lindex: -1,
                tymed: TYMED_HGLOBAL.0 as u32,
                ..Default::default()
            };
            data.QueryGetData(&format).ok()?;
            let mut medium = data.GetData(&format)?;
            let drop = HDROP(medium.u.hGlobal.0);
            let count = DragQueryFileW(drop, u32::MAX, None);
            let len = DragQueryFileW(drop, 0, None);
            let mut filename = vec![0u16; len as usize + 1];
            DragQueryFileW(drop, 0, Some(&mut filename));
            ReleaseStgMedium(&mut medium);
            assert_eq!(count, 1);
            assert_eq!(
                String::from_utf16(&filename[..len as usize])?,
                path.to_string_lossy()
            );
        }
        assert_eq!(std::fs::read(&path)?, b"native data-object test");
        Ok(())
    }
}
