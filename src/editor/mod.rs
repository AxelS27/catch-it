//! Activated native image editor. Modal output actions are dispatched by App,
//! never inside a callback holding WindowState. Drawing tools arrive in later slices.
mod interaction;
mod layout;
mod render;

use crate::{
    drag_drop::{Outcome, PreparedDrag},
    storage::{self, Raster},
};
use anyhow::{Context, Result};
use layout::{Control, Layout, View, Zoom};
use render::Renderer;
use std::{
    cell::Cell,
    path::{Path, PathBuf},
    rc::Rc,
    sync::atomic::{AtomicUsize, Ordering},
};
use windows::{
    Win32::{
        Foundation::*,
        Graphics::{Dwm::*, Gdi::*},
        System::LibraryLoader::GetModuleHandleW,
        UI::{Controls::*, HiDpi::*, Input::KeyboardAndMouse::*, WindowsAndMessaging::*},
    },
    core::{BOOL, PCWSTR, w},
};

pub const ACTION: u32 = WM_APP + 17;
pub const CLOSE: isize = 1;
pub const COPY: isize = 2;
pub const SAVE: isize = 3;
pub const DRAG: isize = 4;
pub const ERROR: isize = 5;
pub const ZOOM_MENU: isize = 6;
const CLASS: PCWSTR = w!("SimpleScreenshot.Editor");
static NEXT_ID: AtomicUsize = AtomicUsize::new(1);
const ZOOM_VALUES: [f32; 8] = [0.1, 0.25, 0.5, 1.0, 1.5, 2.0, 4.0, 8.0];

struct WindowState {
    controller: HWND,
    id: usize,
    layout: Layout,
    dpi: u32,
    renderer: Option<Renderer>,
    image: Option<Raster>,
    view: View,
    dark: bool,
    hover: Option<Control>,
    focus: Option<Control>,
    pressed: Option<Control>,
    palette_open: bool,
    selected_color: usize,
    pan_start: Option<((f32, f32), (f32, f32))>,
    drag_start: Option<(f32, f32)>,
    error: Option<String>,
    tooltip: HWND,
    tooltip_text: Vec<Vec<u16>>,
    hidden: bool,
    capture_restore: bool,
    cancel_drag: Rc<Cell<bool>>,
}
impl WindowState {
    fn request(&self, _hwnd: HWND, action: isize) {
        unsafe {
            let _ = PostMessageW(
                Some(self.controller),
                ACTION,
                WPARAM(self.id),
                LPARAM(action),
            );
        }
    }
    fn ready(&self) -> bool {
        self.image.is_some()
    }
    fn point(&self, lparam: LPARAM) -> (f32, f32) {
        (
            (lparam.0 as u16 as i16) as f32 * 96.0 / self.dpi as f32,
            ((lparam.0 >> 16) as u16 as i16) as f32 * 96.0 / self.dpi as f32,
        )
    }
    fn cancel_gesture(&mut self) {
        // A canceled pan restores view state; it never changes document pixels.
        if let Some((_, pan)) = self.pan_start.take() {
            self.view.pan = pan;
        }
        self.pressed = None;
        self.drag_start = None;
    }
}

pub struct Editor {
    hwnd: HWND,
    state: Box<WindowState>,
    path: PathBuf,
    _protection: std::fs::File,
    drag_image: Option<(u32, u32, Vec<u8>)>,
}
impl Editor {
    pub fn create(controller: HWND, source: HWND, path: &Path) -> Result<Self> {
        let protection = storage::protect_png(path)?;
        // The supplied markup.mp4 is a light-appearance reference. Pin the
        // editor to that appearance rather than inheriting Windows dark mode.
        let dark = false;
        let mut state = Box::new(WindowState {
            controller,
            id: NEXT_ID.fetch_add(1, Ordering::Relaxed),
            layout: Layout::new(960.0, 600.0),
            dpi: 96,
            renderer: None,
            image: None,
            view: View::default(),
            dark,
            hover: None,
            focus: None,
            pressed: None,
            palette_open: false,
            selected_color: 4,
            pan_start: None,
            drag_start: None,
            error: None,
            tooltip: HWND::default(),
            tooltip_text: Vec::new(),
            hidden: false,
            capture_restore: false,
            cancel_drag: Rc::new(Cell::new(false)),
        });
        unsafe {
            let monitor = MonitorFromWindow(source, MONITOR_DEFAULTTONEAREST);
            let mut info = MONITORINFO {
                cbSize: std::mem::size_of::<MONITORINFO>() as u32,
                ..Default::default()
            };
            anyhow::ensure!(
                GetMonitorInfoW(monitor, &mut info).as_bool(),
                "Cannot find editor monitor"
            );
            let dpi = GetDpiForWindow(source).max(96);
            let scale = dpi as f32 / 96.0;
            let work = info.rcWork;
            // Comfortable viewer size without taking over the desktop.
            let work_width = work.right - work.left;
            let work_height = work.bottom - work.top;
            let width = (1040.0 * scale).round() as i32;
            let height = (700.0 * scale).round() as i32;
            let width = width.min(work_width);
            let height = height.min(work_height);
            let hwnd = CreateWindowExW(
                WINDOW_EX_STYLE::default(),
                CLASS,
                // Retain an accessible window name; custom chrome never draws it.
                w!("Simple Screenshot editor"),
                WS_OVERLAPPEDWINDOW | WS_CLIPCHILDREN,
                work.left + (work.right - work.left - width) / 2,
                work.top + (work.bottom - work.top - height) / 2,
                width,
                height,
                None,
                None,
                Some(GetModuleHandleW(None)?.into()),
                Some((&mut *state as *mut WindowState).cast()),
            )?;
            let mut editor = Self {
                hwnd,
                state,
                path: path.to_path_buf(),
                _protection: protection,
                drag_image: None,
            };
            editor.state.dpi = GetDpiForWindow(hwnd);
            editor.theme();
            editor.update_size()?;
            editor.create_tooltips()?;
            // SetWindowPos intentionally shows the editor even when a launcher
            // supplied STARTUPINFO SW_HIDE (the first ShowWindow ignores SW_SHOW).
            SetWindowPos(
                hwnd,
                None,
                0,
                0,
                0,
                0,
                SWP_NOMOVE | SWP_NOSIZE | SWP_NOZORDER | SWP_SHOWWINDOW,
            )?;
            let _ = SetForegroundWindow(hwnd);
            let _ = SetFocus(Some(hwnd));
            let _ = InvalidateRect(Some(hwnd), None, false);
            println!("Editor opened: {}", path.display());
            Ok(editor)
        }
    }
    pub fn hwnd(&self) -> HWND {
        self.hwnd
    }
    pub fn id(&self) -> usize {
        self.state.id
    }
    pub fn path(&self) -> &Path {
        &self.path
    }
    pub fn image(&self) -> Option<&Raster> {
        self.state.image.as_ref()
    }
    pub fn matches(&self, source: WPARAM) -> bool {
        source.0 == self.state.id
    }
    pub fn activate(&self) {
        unsafe {
            let _ = ShowWindow(self.hwnd, SW_RESTORE);
            let _ = SetForegroundWindow(self.hwnd);
            let _ = SetFocus(Some(self.hwnd));
        }
    }
    pub fn load(&mut self, image: Raster) -> Result<()> {
        self.drag_image = Some(drag_preview(&image));
        if let Some(renderer) = &mut self.state.renderer {
            renderer.set_image(&image)?;
        }
        println!(
            "Editor loaded: {} x {} original pixels",
            image.width, image.height
        );
        self.state.image = Some(image);
        unsafe {
            let _ = InvalidateRect(Some(self.hwnd), None, false);
        }
        Ok(())
    }
    pub fn take_error(&mut self) -> Option<String> {
        self.state.error.take()
    }
    fn update_size(&mut self) -> Result<()> {
        let mut client = RECT::default();
        unsafe {
            GetClientRect(self.hwnd, &mut client)?;
        }
        resize_state(
            self.hwnd,
            &mut self.state,
            client.right as u32,
            client.bottom as u32,
        )
    }
    fn theme(&self) {
        apply_theme(self.hwnd, self.state.dark);
    }
    pub fn capture_hidden(&mut self, hidden: bool) -> Result<bool> {
        if self.state.hidden == hidden {
            return Ok(false);
        }
        self.state.cancel_gesture();
        self.state.palette_open = false;
        unsafe {
            if GetCapture() == self.hwnd {
                let _ = ReleaseCapture();
            }
            let visible = IsWindowVisible(self.hwnd).as_bool();
            if hidden {
                self.state.cancel_drag.set(true);
                self.state.capture_restore = visible;
                if visible {
                    SetWindowPos(
                        self.hwnd,
                        None,
                        0,
                        0,
                        0,
                        0,
                        SWP_NOMOVE | SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE | SWP_HIDEWINDOW,
                    )
                    .context("Cannot hide editor before capture")?;
                }
            } else if self.state.capture_restore {
                // Hiding never changes WS_MINIMIZE/WS_MAXIMIZE. Reveal that exact
                // state, without issuing a second minimize/restore animation.
                SetWindowPos(
                    self.hwnd,
                    None,
                    0,
                    0,
                    0,
                    0,
                    SWP_NOMOVE | SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE | SWP_SHOWWINDOW,
                )
                .context("Cannot restore editor after capture")?;
                self.state.capture_restore = false;
            }
            self.state.hidden = hidden;
            Ok(visible)
        }
    }
    pub fn zoom_menu(&mut self) -> Result<()> {
        struct Menu(HMENU);
        impl Drop for Menu {
            fn drop(&mut self) {
                unsafe {
                    let _ = DestroyMenu(self.0);
                }
            }
        }
        let (r, current) = (
            self.state
                .layout
                .rect(Control::Zoom)
                .context("Zoom control hidden")?,
            self.state.view.zoom,
        );
        unsafe {
            let menu = Menu(CreatePopupMenu()?);
            AppendMenuW(
                menu.0,
                MF_STRING
                    | if current == Zoom::Fit {
                        MF_CHECKED
                    } else {
                        MF_UNCHECKED
                    },
                100,
                w!("Fit to window"),
            )?;
            for (i, value) in ZOOM_VALUES.into_iter().enumerate() {
                let text: Vec<u16> = format!("{}%", (value * 100.0) as u32)
                    .encode_utf16()
                    .chain(Some(0))
                    .collect();
                AppendMenuW(
                    menu.0,
                    MF_STRING
                        | if current == Zoom::Scale(value) {
                            MF_CHECKED
                        } else {
                            MF_UNCHECKED
                        },
                    101 + i,
                    PCWSTR(text.as_ptr()),
                )?;
            }
            let mut point = POINT {
                x: (r.x * self.state.dpi as f32 / 96.0) as i32,
                y: (r.y * self.state.dpi as f32 / 96.0) as i32,
            };
            let _ = ClientToScreen(self.hwnd, &mut point);
            let selected = TrackPopupMenuEx(
                menu.0,
                (TPM_RETURNCMD | TPM_NONOTIFY | TPM_LEFTALIGN | TPM_BOTTOMALIGN).0,
                point.x,
                point.y,
                self.hwnd,
                None,
            )
            .0;
            if selected >= 100 {
                self.set_zoom(selected as usize - 100);
            }
        }
        Ok(())
    }
    pub fn set_zoom(&mut self, index: usize) {
        self.state.view = View {
            zoom: if index == 0 {
                Zoom::Fit
            } else {
                Zoom::Scale(ZOOM_VALUES.get(index - 1).copied().unwrap_or(1.0))
            },
            pan: (0.0, 0.0),
        };
        unsafe {
            let _ = InvalidateRect(Some(self.hwnd), None, false);
        }
    }
    pub fn run_drag(&mut self) -> Result<Outcome> {
        self.state.drag_start = None;
        self.state.pressed = None;
        unsafe {
            if GetCapture() == self.hwnd {
                let _ = ReleaseCapture();
            }
        }
        if unsafe { GetAsyncKeyState(VK_LBUTTON.0 as i32) } >= 0 {
            return Ok(Outcome::Canceled);
        }
        let (width, height, pixels) = self
            .drag_image
            .as_ref()
            .context("Image is not ready for dragging")?;
        self.state.cancel_drag.set(false);
        let drag = PreparedDrag::new(
            &self.path,
            *width,
            *height,
            pixels,
            POINT {
                x: (width / 2) as i32,
                y: (height / 2) as i32,
            },
            Rc::clone(&self.state.cancel_drag),
        )?;
        if unsafe { GetAsyncKeyState(VK_LBUTTON.0 as i32) } >= 0 {
            return Ok(Outcome::Canceled);
        }
        println!("Editor drag started: {}", self.path.display());
        let outcome = drag.run()?;
        println!(
            "Editor drag result: {}",
            if outcome == Outcome::Copied {
                "copied"
            } else {
                "canceled"
            }
        );
        Ok(outcome)
    }
    fn create_tooltips(&mut self) -> Result<()> {
        unsafe {
            let init = INITCOMMONCONTROLSEX {
                dwSize: std::mem::size_of::<INITCOMMONCONTROLSEX>() as u32,
                dwICC: ICC_WIN95_CLASSES,
            };
            anyhow::ensure!(
                InitCommonControlsEx(&init).as_bool(),
                "Cannot initialize editor tooltips"
            );
            self.state.tooltip = CreateWindowExW(
                WS_EX_TOPMOST,
                TOOLTIPS_CLASSW,
                None,
                WS_POPUP | WINDOW_STYLE(TTS_ALWAYSTIP | TTS_NOPREFIX),
                CW_USEDEFAULT,
                CW_USEDEFAULT,
                CW_USEDEFAULT,
                CW_USEDEFAULT,
                Some(self.hwnd),
                None,
                Some(GetModuleHandleW(None)?.into()),
                None,
            )?;
            self.state.tooltip_text = self
                .state
                .layout
                .controls
                .iter()
                .map(|(c, _)| {
                    let text = if c.enabled()
                        || matches!(
                            c,
                            Control::Upload | Control::Present | Control::Share | Control::Pin
                        ) {
                        c.label().to_string()
                    } else {
                        format!("{} - not implemented yet", c.label())
                    };
                    text.encode_utf16().chain(Some(0)).collect()
                })
                .collect();
            for ((c, r), text) in self
                .state
                .layout
                .controls
                .iter()
                .zip(self.state.tooltip_text.iter_mut())
            {
                let info = TTTOOLINFOW {
                    // V2 works with both system comctl32 v5 and manifested v6.
                    cbSize: std::mem::offset_of!(TTTOOLINFOW, lpReserved) as u32,
                    uFlags: TTF_SUBCLASS,
                    hwnd: self.hwnd,
                    uId: *c as usize + 1,
                    rect: physical_rect(*r, self.state.dpi),
                    lpszText: windows::core::PWSTR(text.as_mut_ptr()),
                    ..Default::default()
                };
                anyhow::ensure!(
                    SendMessageW(
                        self.state.tooltip,
                        TTM_ADDTOOLW,
                        Some(WPARAM(0)),
                        Some(LPARAM((&info as *const TTTOOLINFOW) as isize))
                    )
                    .0 != 0,
                    "Cannot add editor tooltip"
                );
            }
        }
        Ok(())
    }
}
impl Drop for Editor {
    fn drop(&mut self) {
        println!("Editor closed: {}", self.path.display());
        self.state.cancel_drag.set(true);
        unsafe {
            // Tooltip owns a native subclass of the editor: remove it first while userdata lives.
            if !self.state.tooltip.is_invalid() {
                let _ = DestroyWindow(self.state.tooltip);
            }
            SetWindowLongPtrW(self.hwnd, GWLP_USERDATA, 0);
            self.state.renderer.take();
            let _ = DestroyWindow(self.hwnd);
        }
    }
}
fn apply_theme(hwnd: HWND, dark: bool) {
    unsafe {
        let value = BOOL::from(dark);
        let _ = DwmSetWindowAttribute(
            hwnd,
            DWMWA_USE_IMMERSIVE_DARK_MODE,
            (&value as *const BOOL).cast(),
            std::mem::size_of::<BOOL>() as u32,
        );
        let corners = DWMWCP_ROUND;
        let _ = DwmSetWindowAttribute(
            hwnd,
            DWMWA_WINDOW_CORNER_PREFERENCE,
            (&corners as *const DWM_WINDOW_CORNER_PREFERENCE).cast(),
            std::mem::size_of::<DWM_WINDOW_CORNER_PREFERENCE>() as u32,
        );
    }
}
fn physical_rect(r: layout::Rect, dpi: u32) -> RECT {
    let s = dpi as f32 / 96.0;
    RECT {
        left: (r.x * s).round() as i32,
        top: (r.y * s).round() as i32,
        right: ((r.x + r.w) * s).round() as i32,
        bottom: ((r.y + r.h) * s).round() as i32,
    }
}
fn resize_state(hwnd: HWND, state: &mut WindowState, width: u32, height: u32) -> Result<()> {
    state.layout = Layout::new(
        width as f32 * 96.0 / state.dpi as f32,
        height as f32 * 96.0 / state.dpi as f32,
    );
    if let Some(image) = &state.image {
        state
            .view
            .clamp_pan(state.layout.canvas, image.width, image.height);
    }
    if let Some(renderer) = &state.renderer {
        renderer.resize(width, height, state.dpi)?;
    } else {
        state.renderer = Some(Renderer::new(
            hwnd,
            width,
            height,
            state.dpi,
            state.image.as_ref(),
        )?);
    }
    for (c, r) in &state.layout.controls {
        unsafe {
            let info = TTTOOLINFOW {
                cbSize: std::mem::offset_of!(TTTOOLINFOW, lpReserved) as u32,
                hwnd,
                uId: *c as usize + 1,
                rect: physical_rect(*r, state.dpi),
                ..Default::default()
            };
            if !state.tooltip.is_invalid() {
                SendMessageW(
                    state.tooltip,
                    TTM_NEWTOOLRECTW,
                    Some(WPARAM(0)),
                    Some(LPARAM((&info as *const TTTOOLINFOW) as isize)),
                );
            }
        }
    }
    Ok(())
}
fn drag_preview(image: &Raster) -> (u32, u32, Vec<u8>) {
    let scale = (220.0 / image.width as f32)
        .min(140.0 / image.height as f32)
        .min(1.0);
    let width = (image.width as f32 * scale).round().max(1.0) as u32;
    let height = (image.height as f32 * scale).round().max(1.0) as u32;
    let mut pixels = Vec::with_capacity((width * height * 4) as usize);
    for y in 0..height {
        for x in 0..width {
            let src = ((u64::from(y) * u64::from(image.height) / u64::from(height))
                * u64::from(image.width)
                + u64::from(x) * u64::from(image.width) / u64::from(width))
                as usize
                * 4;
            pixels.extend_from_slice(&image.pixels[src..src + 4]);
        }
    }
    (width, height, render::premultiply(&pixels))
}
pub fn register_class() -> Result<()> {
    unsafe {
        let class = WNDCLASSW {
            lpfnWndProc: Some(interaction::window_proc),
            hInstance: GetModuleHandleW(None)?.into(),
            hCursor: LoadCursorW(None, IDC_ARROW)?,
            lpszClassName: CLASS,
            ..Default::default()
        };
        anyhow::ensure!(
            RegisterClassW(&class) != 0,
            "Cannot register annotation editor"
        );
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn drag_preview_preserves_aspect_alpha_and_original() {
        let image = Raster {
            width: 350,
            height: 200,
            pixels: vec![128; 350 * 200 * 4],
        };
        let (w, h, p) = drag_preview(&image);
        assert_eq!((w, h), (220, 126));
        assert_eq!(p.len(), (w * h * 4) as usize);
        assert_eq!(&p[..4], &[64, 64, 64, 128]);
        assert_eq!(image.pixels[0], 128);
    }
}
