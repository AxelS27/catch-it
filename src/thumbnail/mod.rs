mod layout;
mod lifecycle;
mod render;

use std::{
    cell::Cell,
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
            Controls::WM_MOUSELEAVE,
            HiDpi::{GetDpiForWindow, GetSystemMetricsForDpi},
            Input::KeyboardAndMouse::*,
            WindowsAndMessaging::*,
        },
    },
    core::{BOOL, w},
};

use crate::{
    drag_drop::{Outcome, PreparedDrag},
    geometry::Point,
};
use layout::{Layout, WorkArea};
use lifecycle::{Action, Lifecycle, Timing};
pub use render::Compositor;
use render::Surface;

pub const HOVER_CHANGED: u32 = WM_APP + 3;
pub const DISMISS: u32 = WM_APP + 4;
pub const BEGIN_DRAG: u32 = WM_APP + 5;
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
    press: Option<Point>,
    drag_anchor: Point,
    drag_pending: bool,
    dragging: bool,
    cancel_drag: Rc<Cell<bool>>,
    drag_width: i32,
    drag_height: i32,
}

pub struct Thumbnail {
    hwnd: HWND,
    state: Box<WindowState>,
    surface: Option<Surface>,
    lifecycle: Lifecycle,
    // Source PNG remains available independently of the UI and copy operation.
    path: PathBuf,
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
            press: None,
            drag_anchor: Point::default(),
            drag_pending: false,
            dragging: false,
            cancel_drag: Rc::new(Cell::new(false)),
            drag_width: 4,
            drag_height: 4,
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
                path: image.path.clone(),
            };
            let dpi = GetDpiForWindow(hwnd);
            thumbnail.state.drag_width = GetSystemMetricsForDpi(SM_CXDRAG, dpi).max(1);
            thumbnail.state.drag_height = GetSystemMetricsForDpi(SM_CYDRAG, dpi).max(1);
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

    pub fn run_drag(&mut self) -> Result<Outcome> {
        if !self.state.drag_pending {
            return Ok(Outcome::Canceled);
        }
        self.state.drag_pending = false;
        self.state.press = None;
        unsafe {
            if GetCapture() == self.hwnd {
                let _ = ReleaseCapture();
            }
        }
        if !self.lifecycle.can_drag() || unsafe { GetAsyncKeyState(VK_LBUTTON.0 as i32) } >= 0 {
            return Ok(Outcome::Canceled);
        }
        let action = self
            .lifecycle
            .begin_drag(Instant::now(), self.state.hovered);
        self.apply(action)?;
        if !self.lifecycle.can_drag() {
            return Ok(Outcome::Canceled);
        }
        self.state.dragging = true;
        self.state.cancel_drag.set(false);
        let layout = self.state.layout;
        let width = (layout.image_width * layout.scale).round() as u32;
        let height = (layout.image_height * layout.scale).round() as u32;
        let anchor = self.state.drag_anchor;
        let offset = POINT {
            x: (anchor.x - (layout.image_left * layout.scale).round() as i32)
                .clamp(0, width as i32 - 1),
            y: (anchor.y - (layout.image_top * layout.scale).round() as i32)
                .clamp(0, height as i32 - 1),
        };
        let result = (|| {
            let drag = PreparedDrag::new(
                &self.path,
                width,
                height,
                layout.radius * layout.scale,
                layout.scale,
                offset,
                Rc::clone(&self.state.cancel_drag),
            )?;
            // Release during setup is a canceled gesture, not a drop somewhere else.
            if unsafe { GetAsyncKeyState(VK_LBUTTON.0 as i32) } >= 0 {
                return Ok(Outcome::Canceled);
            }
            unsafe {
                let _ = ShowWindow(self.hwnd, SW_HIDE);
            }
            println!("Drag started: {}", self.path.display());
            drag.run()
        })();
        self.state.dragging = false;
        if matches!(result, Ok(Outcome::Copied)) || crate::exit_requested() {
            return result;
        }
        let mut hovered = false;
        if !self.state.cancel_drag.get() {
            unsafe {
                let _ = ShowWindow(self.hwnd, SW_SHOWNOACTIVATE);
                let mut cursor = POINT::default();
                hovered = GetCursorPos(&mut cursor).is_ok()
                    && WindowFromPoint(cursor) == self.hwnd
                    && layout.contains((cursor.x - layout.x) as f32, (cursor.y - layout.y) as f32);
                if hovered {
                    track_leave(self.hwnd);
                }
            }
        }
        self.state.hovered = hovered;
        let action = self.lifecycle.end_drag(Instant::now(), hovered);
        self.apply(action)?;
        if self.state.cancel_drag.get() {
            self.dismiss()?;
        }
        result
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

fn mouse_point(position: LPARAM) -> Point {
    Point {
        x: (position.0 as u16 as i16) as i32,
        y: ((position.0 >> 16) as u16 as i16) as i32,
    }
}

fn crossed_drag_threshold(start: Point, point: Point, width: i32, height: i32) -> bool {
    let left = i64::from(start.x) - i64::from(width) / 2;
    let top = i64::from(start.y) - i64::from(height) / 2;
    i64::from(point.x) < left
        || i64::from(point.x) >= left + i64::from(width)
        || i64::from(point.y) < top
        || i64::from(point.y) >= top + i64::from(height)
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
            WM_LBUTTONDOWN => {
                {
                    let state = &mut *ptr;
                    let point = mouse_point(lparam);
                    if state.dragging || !state.layout.contains(point.x as f32, point.y as f32) {
                        return LRESULT(0);
                    }
                    state.press = Some(point);
                    state.drag_anchor = point;
                    if !state.hovered {
                        state.hovered = true;
                        let _ = PostMessageW(
                            Some(state.controller),
                            HOVER_CHANGED,
                            WPARAM(hwnd.0 as usize),
                            LPARAM(0),
                        );
                    }
                }
                SetCapture(hwnd);
                LRESULT(0)
            }
            WM_LBUTTONUP => {
                (&mut *ptr).press = None;
                if GetCapture() == hwnd {
                    let _ = ReleaseCapture();
                }
                LRESULT(0)
            }
            WM_CAPTURECHANGED => {
                (&mut *ptr).press = None;
                LRESULT(0)
            }
            WM_MOUSEMOVE | WM_MOUSELEAVE => {
                let state = &mut *ptr;
                if message == WM_MOUSEMOVE
                    && wparam.0 & 1 != 0
                    && state.press.is_some_and(|start| {
                        crossed_drag_threshold(
                            start,
                            mouse_point(lparam),
                            state.drag_width,
                            state.drag_height,
                        )
                    })
                    && !state.drag_pending
                    && !state.dragging
                {
                    state.press = None;
                    state.drag_pending = true;
                    // Begin OLE outside this callback: its modal loop dispatches
                    // messages and must never reenter a borrowed WindowState.
                    let _ = PostMessageW(
                        Some(state.controller),
                        BEGIN_DRAG,
                        WPARAM(hwnd.0 as usize),
                        LPARAM(0),
                    );
                }
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
                if state.dragging {
                    state.cancel_drag.set(true);
                }
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

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn drag_uses_the_system_sized_centered_rectangle() {
        let start = Point { x: 100, y: 100 };
        assert!(!crossed_drag_threshold(
            start,
            Point { x: 101, y: 99 },
            4,
            4
        ));
        assert!(!crossed_drag_threshold(start, Point { x: 98, y: 98 }, 4, 4));
        assert!(crossed_drag_threshold(
            start,
            Point { x: 102, y: 100 },
            4,
            4
        ));
        assert!(crossed_drag_threshold(start, Point { x: 97, y: 100 }, 4, 4));
    }
}
