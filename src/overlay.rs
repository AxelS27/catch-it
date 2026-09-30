use anyhow::Result;
use windows::{
    Win32::{
        Foundation::*,
        Graphics::{
            Direct2D::{Common::*, *},
            Dxgi::Common::DXGI_FORMAT_B8G8R8A8_UNORM,
            Gdi::*,
        },
        System::LibraryLoader::GetModuleHandleW,
        UI::{Input::KeyboardAndMouse::*, WindowsAndMessaging::*},
    },
    core::w,
};

use crate::{
    capture::Snapshot,
    geometry::{Point, Region},
};

pub const FINISH_SELECTION: u32 = WM_APP + 2;
const CLASS_NAME: windows::core::PCWSTR = w!("SimpleScreenshot.Selection");

struct Renderer {
    target: ID2D1HwndRenderTarget,
    bitmap: ID2D1Bitmap,
    dim: ID2D1SolidColorBrush,
    outline: ID2D1SolidColorBrush,
}

impl Renderer {
    fn new(hwnd: HWND, snapshot: &Snapshot) -> Result<Self> {
        unsafe {
            let factory: ID2D1Factory = D2D1CreateFactory(D2D1_FACTORY_TYPE_SINGLE_THREADED, None)?;
            let target = factory.CreateHwndRenderTarget(
                &D2D1_RENDER_TARGET_PROPERTIES {
                    pixelFormat: D2D1_PIXEL_FORMAT {
                        format: DXGI_FORMAT_B8G8R8A8_UNORM,
                        alphaMode: D2D1_ALPHA_MODE_IGNORE,
                    },
                    dpiX: 96.0,
                    dpiY: 96.0,
                    ..Default::default()
                },
                &D2D1_HWND_RENDER_TARGET_PROPERTIES {
                    hwnd,
                    pixelSize: D2D_SIZE_U {
                        width: snapshot.width,
                        height: snapshot.height,
                    },
                    presentOptions: D2D1_PRESENT_OPTIONS_NONE,
                },
            )?;
            // At 96 render DPI, one drawing unit is exactly one physical pixel,
            // regardless of the monitor's Windows scaling factor.
            let bitmap = target.CreateBitmap(
                D2D_SIZE_U {
                    width: snapshot.width,
                    height: snapshot.height,
                },
                Some(snapshot.pixels.as_ptr().cast()),
                snapshot.width * 4,
                &D2D1_BITMAP_PROPERTIES {
                    pixelFormat: D2D1_PIXEL_FORMAT {
                        format: DXGI_FORMAT_B8G8R8A8_UNORM,
                        alphaMode: D2D1_ALPHA_MODE_IGNORE,
                    },
                    dpiX: 96.0,
                    dpiY: 96.0,
                },
            )?;
            let dim = target.CreateSolidColorBrush(
                &D2D1_COLOR_F {
                    r: 0.0,
                    g: 0.0,
                    b: 0.0,
                    a: 0.28,
                },
                None,
            )?;
            let outline = target.CreateSolidColorBrush(
                &D2D1_COLOR_F {
                    r: 1.0,
                    g: 1.0,
                    b: 1.0,
                    a: 1.0,
                },
                None,
            )?;
            Ok(Self {
                target,
                bitmap,
                dim,
                outline,
            })
        }
    }

    fn draw(&self, snapshot: &Snapshot, region: Option<Region>) -> Result<()> {
        unsafe {
            self.target.BeginDraw();
            self.target.DrawBitmap(
                &self.bitmap,
                None,
                1.0,
                D2D1_BITMAP_INTERPOLATION_MODE_NEAREST_NEIGHBOR,
                None,
            );
            self.target.FillRectangle(
                &D2D_RECT_F {
                    left: 0.0,
                    top: 0.0,
                    right: snapshot.width as f32,
                    bottom: snapshot.height as f32,
                },
                &self.dim,
            );
            if let Some(region) = region {
                let rect = D2D_RECT_F {
                    left: region.x as f32,
                    top: region.y as f32,
                    right: (region.x + region.width) as f32,
                    bottom: (region.y + region.height) as f32,
                };
                self.target.DrawBitmap(
                    &self.bitmap,
                    Some(&rect),
                    1.0,
                    D2D1_BITMAP_INTERPOLATION_MODE_NEAREST_NEIGHBOR,
                    Some(&rect),
                );
                self.target.DrawRectangle(&rect, &self.outline, 1.0, None);
            }
            self.target.EndDraw(None, None)?;
        }
        Ok(())
    }
}

struct OverlayState {
    controller: HWND,
    snapshot: Snapshot,
    renderer: Option<Renderer>,
    start: Option<Point>,
    pointer: Point,
    finished: bool,
    selected: Option<Region>,
    error: Option<String>,
}

impl OverlayState {
    fn selection(&self) -> Option<Region> {
        self.start.and_then(|start| {
            Region::between(
                start,
                self.pointer,
                self.snapshot.width,
                self.snapshot.height,
            )
        })
    }

    fn finish(&mut self, select: bool) {
        if self.finished {
            return;
        }
        self.finished = true;
        self.selected = if select { self.selection() } else { None };
        // Defer teardown to the controller. Do not destroy this state during its callback.
        unsafe {
            let _ = PostMessageW(
                Some(self.controller),
                FINISH_SELECTION,
                WPARAM(0),
                LPARAM(0),
            );
        }
    }

    fn update_pointer(&mut self, position: LPARAM) {
        // Use coordinates attached to the input event. GetCursorPos can already
        // refer to a later movement when the UI processes a queued button event.
        self.pointer = Point {
            x: (position.0 as u16 as i16) as i32,
            y: ((position.0 >> 16) as u16 as i16) as i32,
        };
    }
}

pub struct ActiveOverlay {
    hwnd: HWND,
    state: Box<OverlayState>,
    previous_focus: HWND,
}

impl ActiveOverlay {
    pub fn create(controller: HWND, snapshot: Snapshot) -> Result<Self> {
        let mut state = Box::new(OverlayState {
            controller,
            snapshot,
            renderer: None,
            start: None,
            pointer: Point::default(),
            finished: false,
            selected: None,
            error: None,
        });
        unsafe {
            let previous_focus = GetForegroundWindow();
            let hwnd = CreateWindowExW(
                WS_EX_TOPMOST | WS_EX_TOOLWINDOW,
                CLASS_NAME,
                w!("Select screenshot region"),
                WS_POPUP,
                state.snapshot.left,
                state.snapshot.top,
                state.snapshot.width as i32,
                state.snapshot.height as i32,
                None,
                None,
                Some(GetModuleHandleW(None)?.into()),
                Some((&mut *state as *mut OverlayState).cast()),
            )?;
            let mut overlay = Self {
                hwnd,
                state,
                previous_focus,
            };
            overlay.state.renderer = Some(Renderer::new(hwnd, &overlay.state.snapshot)?);
            let _ = ShowWindow(hwnd, SW_SHOW);
            let _ = SetForegroundWindow(hwnd);
            let _ = SetFocus(Some(hwnd));
            let _ = UpdateWindow(hwnd);
            Ok(overlay)
        }
    }

    pub fn result(&mut self) -> (Option<Region>, Option<String>) {
        (self.state.selected, self.state.error.take())
    }

    pub fn into_snapshot(self) -> Snapshot {
        // Teardown first, then move the CPU pixels out of the state.
        let mut this = self;
        this.close();
        std::mem::replace(
            &mut this.state.snapshot,
            Snapshot {
                left: 0,
                top: 0,
                width: 0,
                height: 0,
                pixels: Vec::new(),
            },
        )
    }

    fn close(&mut self) {
        if self.hwnd.is_invalid() {
            return;
        }
        unsafe {
            let owns_focus = GetForegroundWindow() == self.hwnd;
            // Disable callbacks before releasing mouse capture or destroying the window.
            SetWindowLongPtrW(self.hwnd, GWLP_USERDATA, 0);
            if GetCapture() == self.hwnd {
                let _ = ReleaseCapture();
            }
            let _ = DestroyWindow(self.hwnd);
            if owns_focus && !self.previous_focus.is_invalid() {
                let _ = SetForegroundWindow(self.previous_focus);
            }
        }
        self.hwnd = HWND::default();
    }
}
impl Drop for ActiveOverlay {
    fn drop(&mut self) {
        self.close();
    }
}

pub fn register_class() -> Result<()> {
    unsafe {
        let class = WNDCLASSW {
            lpfnWndProc: Some(window_proc),
            hInstance: GetModuleHandleW(None)?.into(),
            hCursor: LoadCursorW(None, IDC_CROSS)?,
            lpszClassName: CLASS_NAME,
            ..Default::default()
        };
        anyhow::ensure!(
            RegisterClassW(&class) != 0,
            "Cannot register selection window: {}",
            windows::core::Error::from_thread()
        );
    }
    Ok(())
}

unsafe extern "system" fn window_proc(
    hwnd: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    // SAFETY: Userdata points at a stable Box owned by ActiveOverlay. It is cleared
    // before teardown, and destruction is deferred outside this callback.
    unsafe {
        if message == WM_NCCREATE {
            let create = &*(lparam.0 as *const CREATESTRUCTW);
            SetWindowLongPtrW(hwnd, GWLP_USERDATA, create.lpCreateParams as isize);
            return LRESULT(1);
        }
        let ptr = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut OverlayState;
        if ptr.is_null() {
            return DefWindowProcW(hwnd, message, wparam, lparam);
        }
        match message {
            WM_PAINT => {
                let mut paint = PAINTSTRUCT::default();
                let _ = BeginPaint(hwnd, &mut paint);
                let state = &mut *ptr;
                if let Some(renderer) = &state.renderer
                    && let Err(error) = renderer.draw(&state.snapshot, state.selection())
                {
                    state.error = Some(format!("Cannot render selection: {error:#}"));
                    state.finish(false);
                }
                let _ = EndPaint(hwnd, &paint);
                LRESULT(0)
            }
            WM_ERASEBKGND => LRESULT(1),
            WM_LBUTTONDOWN => {
                {
                    let state = &mut *ptr;
                    if state.finished {
                        return LRESULT(0);
                    }
                    state.update_pointer(lparam);
                    state.start = Some(state.pointer);
                }
                SetCapture(hwnd);
                let _ = InvalidateRect(Some(hwnd), None, false);
                LRESULT(0)
            }
            WM_MOUSEMOVE => {
                let redraw = {
                    let state = &mut *ptr;
                    state.update_pointer(lparam);
                    state.start.is_some() && !state.finished
                };
                if redraw {
                    let _ = InvalidateRect(Some(hwnd), None, false);
                }
                LRESULT(0)
            }
            WM_LBUTTONUP => {
                let state = &mut *ptr;
                state.update_pointer(lparam);
                state.finish(true);
                LRESULT(0)
            }
            WM_KEYDOWN if wparam.0 == VK_ESCAPE.0 as usize => {
                (&mut *ptr).finish(false);
                LRESULT(0)
            }
            WM_RBUTTONDOWN | WM_CLOSE | WM_DISPLAYCHANGE | WM_DPICHANGED | WM_CAPTURECHANGED => {
                (&mut *ptr).finish(false);
                LRESULT(0)
            }
            WM_ACTIVATE if wparam.0 & 0xffff == WA_INACTIVE as usize => {
                (&mut *ptr).finish(false);
                LRESULT(0)
            }
            _ => DefWindowProcW(hwnd, message, wparam, lparam),
        }
    }
}
