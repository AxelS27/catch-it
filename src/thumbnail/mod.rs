mod layout;
mod lifecycle;
mod render;

use std::{
    path::PathBuf,
    rc::Rc,
    time::{Duration, Instant},
};

use anyhow::Result;
use windows::{
    Win32::{
        Foundation::*,
        Graphics::Gdi::*,
        System::LibraryLoader::GetModuleHandleW,
        UI::{
            Controls::WM_MOUSELEAVE, HiDpi::GetDpiForWindow, Input::KeyboardAndMouse::*,
            WindowsAndMessaging::*,
        },
    },
    core::{BOOL, w},
};

use crate::geometry::Point;
use layout::{Layout, WorkArea};
use lifecycle::{Action, Lifecycle, Timing};
pub use render::Compositor;
use render::Surface;

pub const HOVER_CHANGED: u32 = WM_APP + 3;
pub const DISMISS: u32 = WM_APP + 4;
const CLASS_NAME: windows::core::PCWSTR = w!("SimpleScreenshot.Thumbnail");

/// Published only after the complete PNG has been saved by the worker.
pub struct SavedScreenshot {
    pub path: PathBuf,
    pub origin: Point,
    pub width: u32,
    pub height: u32,
    pub pixels: Vec<u8>,
}

struct WindowState {
    controller: HWND,
    layout: Layout,
    hovered: bool,
}

pub struct Thumbnail {
    hwnd: HWND,
    state: Box<WindowState>,
    surface: Option<Surface>,
    lifecycle: Lifecycle,
    // Keep the file independently of the UI. Future drag support uses this path.
    _path: PathBuf,
}

impl Thumbnail {
    pub fn create(
        controller: HWND,
        compositor: Rc<Compositor>,
        image: SavedScreenshot,
    ) -> Result<Self> {
        let mut motion_enabled = BOOL(1);
        unsafe {
            let _ = SystemParametersInfoW(
                SPI_GETCLIENTAREAANIMATION,
                0,
                Some((&mut motion_enabled as *mut BOOL).cast()),
                SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS(0),
            );
        }
        let timing = Timing::for_motion_enabled(motion_enabled.as_bool());
        let mut info = MONITORINFO {
            cbSize: std::mem::size_of::<MONITORINFO>() as u32,
            ..Default::default()
        };
        unsafe {
            let monitor = MonitorFromPoint(
                POINT {
                    x: image.origin.x,
                    y: image.origin.y,
                },
                MONITOR_DEFAULTTONEAREST,
            );
            anyhow::ensure!(
                GetMonitorInfoW(monitor, &mut info).as_bool(),
                "Cannot read capture monitor work area"
            );
        }
        let area = WorkArea {
            left: info.rcWork.left,
            top: info.rcWork.top,
            width: (info.rcWork.right - info.rcWork.left) as u32,
            height: (info.rcWork.bottom - info.rcWork.top) as u32,
        };
        let mut state = Box::new(WindowState {
            controller,
            layout: Layout::new(area, 96, image.width, image.height)?,
            hovered: false,
        });
        unsafe {
            // Create hidden on the capture monitor, then ask Windows for its actual
            // per-monitor DPI. No reliance on system-wide or virtualized DPI values.
            let hwnd = CreateWindowExW(
                WS_EX_TOPMOST | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE | WS_EX_NOREDIRECTIONBITMAP,
                CLASS_NAME,
                w!("Screenshot preview"),
                WS_POPUP,
                image.origin.x,
                image.origin.y,
                1,
                1,
                None,
                None,
                Some(GetModuleHandleW(None)?.into()),
                Some((&mut *state as *mut WindowState).cast()),
            )?;
            let mut thumbnail = Self {
                hwnd,
                state,
                surface: None,
                lifecycle: Lifecycle::new(Instant::now(), timing, false),
                _path: image.path.clone(),
            };
            let dpi = GetDpiForWindow(hwnd);
            thumbnail.state.layout = Layout::new(area, dpi, image.width, image.height)?;
            let layout = thumbnail.state.layout;
            SetWindowPos(
                hwnd,
                Some(HWND_TOPMOST),
                layout.x,
                layout.y,
                layout.width as i32,
                layout.height as i32,
                SWP_NOACTIVATE,
            )?;
            thumbnail.surface = Some(Surface::new(compositor, hwnd, layout, &image, timing)?);
            thumbnail
                .surface
                .as_mut()
                .expect("surface initialized")
                .appear()?;
            SetWindowPos(
                hwnd,
                Some(HWND_TOPMOST),
                0,
                0,
                0,
                0,
                SWP_NOACTIVATE | SWP_NOMOVE | SWP_NOSIZE | SWP_SHOWWINDOW,
            )?;
            // A stationary cursor may already be over the newly shown preview.
            let mut cursor = POINT::default();
            if GetCursorPos(&mut cursor).is_ok()
                && WindowFromPoint(cursor) == hwnd
                && layout.contains((cursor.x - layout.x) as f32, (cursor.y - layout.y) as f32)
            {
                thumbnail.state.hovered = true;
                track_leave(hwnd);
            }
            thumbnail.lifecycle = Lifecycle::new(Instant::now(), timing, thumbnail.state.hovered);
            Ok(thumbnail)
        }
    }

    pub fn matches(&self, source: WPARAM) -> bool {
        source.0 == self.hwnd.0 as usize
    }

    pub fn tick(&mut self) -> Result<bool> {
        let action = self.lifecycle.update(Instant::now(), self.state.hovered);
        self.apply(action)
    }

    pub fn dismiss(&mut self) -> Result<bool> {
        let action = self.lifecycle.dismiss(Instant::now());
        self.apply(action)
    }

    fn apply(&mut self, action: Action) -> Result<bool> {
        match action {
            Action::BeginDismiss => self
                .surface
                .as_ref()
                .expect("surface initialized")
                .dismiss()?,
            Action::Close => return Ok(true),
            Action::None => {}
        }
        Ok(false)
    }

    pub fn next_wake(&self) -> Option<Duration> {
        self.lifecycle.next_wake(Instant::now())
    }
}

impl Drop for Thumbnail {
    fn drop(&mut self) {
        unsafe {
            // Disable callbacks before releasing the composition target or HWND.
            SetWindowLongPtrW(self.hwnd, GWLP_USERDATA, 0);
            self.surface.take();
            let _ = DestroyWindow(self.hwnd);
        }
        // Intentionally do not delete the PNG when the thumbnail disappears.
    }
}

pub fn register_class() -> Result<()> {
    unsafe {
        let class = WNDCLASSW {
            lpfnWndProc: Some(window_proc),
            hInstance: GetModuleHandleW(None)?.into(),
            hCursor: LoadCursorW(None, IDC_ARROW)?,
            lpszClassName: CLASS_NAME,
            ..Default::default()
        };
        anyhow::ensure!(
            RegisterClassW(&class) != 0,
            "Cannot register floating thumbnail: {}",
            windows::core::Error::from_thread()
        );
    }
    Ok(())
}

unsafe fn track_leave(hwnd: HWND) {
    unsafe {
        let mut tracking = TRACKMOUSEEVENT {
            cbSize: std::mem::size_of::<TRACKMOUSEEVENT>() as u32,
            dwFlags: TME_LEAVE,
            hwndTrack: hwnd,
            dwHoverTime: 0,
        };
        let _ = TrackMouseEvent(&mut tracking);
    }
}

unsafe extern "system" fn window_proc(
    hwnd: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    // SAFETY: Userdata is a stable Box owned by Thumbnail. Callbacks only update
    // hover state and post events; they never destroy their own window/state.
    unsafe {
        if message == WM_NCCREATE {
            let create = &*(lparam.0 as *const CREATESTRUCTW);
            SetWindowLongPtrW(hwnd, GWLP_USERDATA, create.lpCreateParams as isize);
            return LRESULT(1);
        }
        let ptr = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut WindowState;
        if ptr.is_null() {
            return DefWindowProcW(hwnd, message, wparam, lparam);
        }
        match message {
            WM_MOUSEACTIVATE => LRESULT(MA_NOACTIVATE as isize),
            WM_NCHITTEST => {
                let state = &*ptr;
                let x = lparam.0 as u16 as i16 as i32 - state.layout.x;
                let y = (lparam.0 >> 16) as u16 as i16 as i32 - state.layout.y;
                LRESULT(if state.layout.contains(x as f32, y as f32) {
                    HTCLIENT as isize
                } else {
                    HTTRANSPARENT as isize
                })
            }
            WM_MOUSEMOVE | WM_MOUSELEAVE => {
                let state = &mut *ptr;
                let hovered = message != WM_MOUSELEAVE
                    && state.layout.contains(
                        (lparam.0 as u16 as i16) as f32,
                        ((lparam.0 >> 16) as u16 as i16) as f32,
                    );
                if state.hovered != hovered {
                    state.hovered = hovered;
                    let _ = PostMessageW(
                        Some(state.controller),
                        HOVER_CHANGED,
                        WPARAM(hwnd.0 as usize),
                        LPARAM(0),
                    );
                }
                if message == WM_MOUSEMOVE {
                    track_leave(hwnd);
                }
                LRESULT(0)
            }
            WM_RBUTTONUP | WM_CLOSE | WM_DISPLAYCHANGE | WM_DPICHANGED | WM_SETTINGCHANGE => {
                let state = &*ptr;
                let _ = PostMessageW(
                    Some(state.controller),
                    DISMISS,
                    WPARAM(hwnd.0 as usize),
                    LPARAM(0),
                );
                LRESULT(0)
            }
            WM_PAINT => {
                // The image is retained by the compositor. No per-frame CPU repaint.
                let mut paint = PAINTSTRUCT::default();
                let _ = BeginPaint(hwnd, &mut paint);
                let _ = EndPaint(hwnd, &paint);
                LRESULT(0)
            }
            WM_ERASEBKGND => LRESULT(1),
            _ => DefWindowProcW(hwnd, message, wparam, lparam),
        }
    }
}
