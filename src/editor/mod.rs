//! Activated native image editor. Modal output actions are dispatched by App,
//! never inside a callback holding WindowState. The source raster remains immutable.
mod background;
mod document;
mod interaction;
mod layout;
mod render;

use crate::{
    drag_drop::{Outcome, PreparedDrag},
    storage::{self, Raster},
    theme,
};
use anyhow::{Context, Result};
use background::Background;
use document::{Document, Mark, Point};
use layout::{Control, Layout, View, Zoom};
use render::Renderer;
use std::{
    cell::Cell,
    path::{Path, PathBuf},
    rc::Rc,
    sync::atomic::{AtomicUsize, Ordering},
    time::Instant,
};
use windows::{
    Win32::{
        Foundation::*,
        Graphics::{Dwm::*, Gdi::*},
        System::LibraryLoader::GetModuleHandleW,
        UI::{
            Controls::Dialogs::*, Controls::*, HiDpi::*, Input::KeyboardAndMouse::*,
            WindowsAndMessaging::*,
        },
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
pub const BACKGROUND_TOGGLE: isize = 7;
pub const COLOR_PICKER: isize = 8;
const CLASS: PCWSTR = w!("SimpleScreenshot.Editor");
static NEXT_ID: AtomicUsize = AtomicUsize::new(1);
const ZOOM_VALUES: [f32; 8] = [0.1, 0.25, 0.5, 1.0, 1.5, 2.0, 4.0, 8.0];
const HOVER_TIMER: usize = 51;

unsafe extern "system" fn color_dialog_hook(
    hwnd: HWND,
    message: u32,
    _: WPARAM,
    _: LPARAM,
) -> usize {
    if message == WM_INITDIALOG {
        // The editor is temporarily topmost over floating thumbnails. A stock
        // common dialog otherwise opens *behind* its owner and looks hung.
        unsafe {
            let _ = SetWindowPos(
                hwnd,
                Some(HWND_TOPMOST),
                0,
                0,
                0,
                0,
                SWP_NOMOVE | SWP_NOSIZE | SWP_SHOWWINDOW,
            );
            let _ = SetForegroundWindow(hwnd);
        }
    }
    0
}

struct MarkGesture {
    index: usize,
    original: Mark,
    start: Point,
    handle: Option<usize>,
}
impl MarkGesture {
    fn at(&self, point: Point) -> Mark {
        let mut mark = self.original.clone();
        if let Some(handle) = self.handle {
            mark.resize_handle(handle, point);
        } else {
            mark.translate(point.x - self.start.x, point.y - self.start.y);
        }
        mark
    }
}
fn hover_step(levels: &mut Vec<(Control, f32)>, target: Option<Control>, dt: f32) -> bool {
    let blend = 1.0 - (-dt / 0.055).exp();
    let mut moving = false;
    for (control, level) in levels.iter_mut() {
        let goal = if Some(*control) == target { 1.0 } else { 0.0 };
        *level += (goal - *level) * blend;
        if (*level - goal).abs() < 0.012 {
            *level = goal;
        } else {
            moving = true;
        }
    }
    levels.retain(|(_, level)| *level > 0.0);
    moving
}
struct PillMotion {
    from: f32,
    velocity: f32,
    to: f32,
    min: f32,
    max: f32,
    start: Instant,
}
impl PillMotion {
    fn position(&self, now: Instant) -> (f32, f32, bool) {
        // Damped spring: continuous position/velocity on rapid tool changes,
        // gentle overshoot in the middle, no overshoot beyond the tool strip.
        const FREQUENCY: f32 = 14.0;
        const DAMPING: f32 = 0.62;
        let t = now.duration_since(self.start).as_secs_f32().min(0.65);
        let decay = DAMPING * FREQUENCY;
        let wave = FREQUENCY * (1.0 - DAMPING * DAMPING).sqrt();
        let a = self.from - self.to;
        let b = (self.velocity + decay * a) / wave;
        let (s, c) = (wave * t).sin_cos();
        let envelope = (-decay * t).exp();
        let displacement = envelope * (a * c + b * s);
        let velocity = envelope * (-decay * (a * c + b * s) + wave * (b * c - a * s));
        let unclamped = self.to + displacement;
        let x = unclamped.clamp(self.min, self.max);
        let v = if x != unclamped { 0.0 } else { velocity };
        let moving = t < 0.65 && ((x - self.to).abs() > 0.15 || v.abs() > 1.5);
        (
            if moving { x } else { self.to },
            if moving { v } else { 0.0 },
            moving,
        )
    }
}
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
    hover_levels: Vec<(Control, f32)>,
    hover_last_tick: Instant,
    focus: Option<Control>,
    pressed: Option<Control>,
    palette_open: bool,
    selected_color: usize,
    custom_color: u32,
    active_tool: Control,
    pill_center: f32,
    pill_motion: Option<PillMotion>,
    stroke_width: f32,
    document: Document,
    pending_mark: Option<Mark>,
    moving_mark: Option<MarkGesture>,
    exported: Option<Raster>,
    background: Background,
    committed_background: Background,
    composed: Option<Raster>,
    background_revision: u64,
    background_pressed: Option<background::Target>,
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
    fn set_hover(&mut self, hwnd: HWND, hit: Option<Control>) {
        if self.hover == hit {
            return;
        }
        self.advance_hover();
        self.hover = hit;
        if let Some(control) = hit
            && !self
                .hover_levels
                .iter()
                .any(|(existing, _)| *existing == control)
        {
            self.hover_levels.push((control, 0.0));
        }
        unsafe {
            let _ = SetTimer(Some(hwnd), HOVER_TIMER, 10, None);
            let _ = InvalidateRect(Some(hwnd), None, false);
        }
    }
    fn advance_hover(&mut self) -> bool {
        let now = Instant::now();
        let dt = now
            .duration_since(self.hover_last_tick)
            .as_secs_f32()
            .min(0.08);
        self.hover_last_tick = now;
        hover_step(&mut self.hover_levels, self.hover, dt)
    }
    fn select_tool(&mut self, hwnd: HWND, control: Control) {
        if self.active_tool == control {
            return;
        }
        let now = Instant::now();
        let (from, velocity) =
            self.pill_motion
                .as_ref()
                .map_or((self.pill_center, 0.0), |motion| {
                    let (position, velocity, _) = motion.position(now);
                    (position, velocity)
                });
        self.active_tool = control;
        if let Some(r) = self.layout.rect(control) {
            let to = r.x + r.w / 2.0;
            self.pill_center = from;
            let min = self
                .layout
                .rect(Control::Move)
                .map_or(to, |r| r.x + r.w / 2.0);
            let max = self
                .layout
                .rect(Control::Highlighter)
                .map_or(to, |r| r.x + r.w / 2.0);
            self.pill_motion = Some(PillMotion {
                from,
                velocity,
                to,
                min,
                max,
                start: now,
            });
            unsafe {
                let _ = SetTimer(Some(hwnd), HOVER_TIMER, 10, None);
            }
        }
    }
    fn advance_pill(&mut self) -> bool {
        let Some(motion) = &self.pill_motion else {
            return false;
        };
        let (center, _, moving) = motion.position(Instant::now());
        self.pill_center = center;
        if !moving {
            self.pill_motion = None;
        }
        moving
    }
    fn drawing_color(&self) -> u32 {
        layout::PRESET_COLORS
            .get(self.selected_color)
            .copied()
            .unwrap_or(self.custom_color)
    }
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
        self.pending_mark = None;
        if let Some(gesture) = self.moving_mark.take() {
            self.document.marks[gesture.index] = gesture.original;
        }
        self.background_pressed = None;
        self.background.dragging = None;
    }
}

pub struct Editor {
    hwnd: HWND,
    state: Box<WindowState>,
    path: PathBuf,
    _protection: std::fs::File,
    drag_image: Option<(u32, u32, Vec<u8>)>,
    output_path: Option<PathBuf>,
    output_revision: (u64, u64),
}
impl Editor {
    pub fn create(controller: HWND, source: HWND, path: &Path) -> Result<Self> {
        let protection = storage::protect_png(path)?;
        // The reference video is light, but the installed Windows appearance
        // is authoritative for the actual user's editor.
        let dark = theme::dark();
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
            hover_levels: Vec::new(),
            hover_last_tick: Instant::now(),
            focus: None,
            pressed: None,
            palette_open: false,
            selected_color: 4,
            custom_color: 0x006dfd,
            active_tool: Control::Move,
            pill_center: 0.0,
            pill_motion: None,
            stroke_width: 3.0,
            document: Document::default(),
            pending_mark: None,
            moving_mark: None,
            exported: None,
            background: Background::default(),
            committed_background: Background::default(),
            composed: None,
            background_revision: 0,
            background_pressed: None,
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
                output_path: None,
                output_revision: (0, 0),
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
        self.state
            .exported
            .as_ref()
            .or(self.state.composed.as_ref())
            .or(self.state.image.as_ref())
    }
    pub fn output(&mut self) -> Result<(&Path, &Raster)> {
        let revision = (self.state.background_revision, self.state.document.revision);
        if self.output_revision != revision {
            self.output_path = None;
            self.state.exported = None;
            self.output_revision = revision;
        }
        if !self.state.document.marks.is_empty() && self.state.exported.is_none() {
            let source = self
                .state
                .image
                .as_ref()
                .context("Screenshot is still opening")?;
            let base = self.state.composed.as_ref().unwrap_or(source);
            let (x, y) =
                background::source_origin(source.width, source.height, &self.state.background)?;
            self.state.exported = Some(document::flatten(
                base,
                &self.state.document.marks,
                Point {
                    x: x as f32,
                    y: y as f32,
                },
            )?);
        }
        if self.output_path.is_none()
            && let Some(composed) = self
                .state
                .exported
                .as_ref()
                .or(self.state.composed.as_ref())
        {
            self.output_path = Some(storage::save_png(
                &composed.pixels,
                composed.width,
                composed.height,
            )?);
        }
        Ok((
            self.output_path.as_deref().unwrap_or(&self.path),
            self.image().context("Screenshot is still opening")?,
        ))
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
        if self.state.background.selected() {
            refresh_background(self.hwnd, &mut self.state)?;
        }
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

    pub fn choose_color(&mut self) -> Result<()> {
        let current = if self.state.selected_color < layout::PRESET_COLORS.len() {
            layout::PRESET_COLORS[self.state.selected_color]
        } else {
            self.state.custom_color
        };
        let mut colors = [COLORREF(0); 16];
        let mut choose = CHOOSECOLORW {
            lStructSize: std::mem::size_of::<CHOOSECOLORW>() as u32,
            hwndOwner: self.hwnd,
            rgbResult: COLORREF(
                (current & 0xff00) | ((current & 0xff) << 16) | ((current >> 16) & 0xff),
            ),
            lpCustColors: colors.as_mut_ptr(),
            Flags: CC_FULLOPEN | CC_RGBINIT | CC_ENABLEHOOK,
            lpfnHook: Some(color_dialog_hook),
            ..Default::default()
        };
        if unsafe { ChooseColorW(&mut choose) }.as_bool() {
            let bgr = choose.rgbResult.0;
            self.state.custom_color = ((bgr & 0xff) << 16) | (bgr & 0xff00) | ((bgr >> 16) & 0xff);
            self.state.selected_color = layout::PRESET_COLORS.len();
            if let Some(index) = self.state.document.selected {
                self.state.document.recolor(index, self.state.custom_color);
            }
            unsafe {
                let _ = InvalidateRect(Some(self.hwnd), None, false);
            }
        } else {
            let error = unsafe { CommDlgExtendedError() };
            anyhow::ensure!(error.0 == 0, "Color dialog failed: {}", error.0);
        }
        Ok(())
    }
    pub fn toggle_background(&mut self) -> Result<()> {
        self.state.background.open = !self.state.background.open;
        self.state.committed_background.open = self.state.background.open;
        if self.state.background.open && self.state.layout.height < 820.0 {
            unsafe {
                let mut bounds = RECT::default();
                GetWindowRect(self.hwnd, &mut bounds)?;
                let monitor = MonitorFromWindow(self.hwnd, MONITOR_DEFAULTTONEAREST);
                let mut info = MONITORINFO {
                    cbSize: std::mem::size_of::<MONITORINFO>() as u32,
                    ..Default::default()
                };
                if GetMonitorInfoW(monitor, &mut info).as_bool() {
                    let desired = (820.0 * self.state.dpi as f32 / 96.0).round() as i32;
                    let h = desired.min(info.rcWork.bottom - info.rcWork.top);
                    let top = bounds.top.min(info.rcWork.bottom - h).max(info.rcWork.top);
                    SetWindowPos(
                        self.hwnd,
                        None,
                        bounds.left,
                        top,
                        bounds.right - bounds.left,
                        h,
                        SWP_NOACTIVATE | SWP_NOZORDER,
                    )?;
                }
            }
        }
        self.update_size()?;
        unsafe {
            let _ = InvalidateRect(Some(self.hwnd), None, false);
        }
        Ok(())
    }

    pub fn refresh_theme(&mut self, dark: bool) {
        if self.state.dark != dark {
            self.state.dark = dark;
            self.theme();
            unsafe {
                let _ = InvalidateRect(Some(self.hwnd), None, false);
            }
        }
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
            fit_limit: if self.state.background.selected() {
                1.6
            } else {
                1.0
            },
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
        let composed = self.state.background.selected() || !self.state.document.marks.is_empty();
        let (path, image) = self.output()?;
        let path = path.to_path_buf();
        let preview = if composed {
            drag_preview(image)
        } else {
            self.drag_image
                .as_ref()
                .context("Image is not ready for dragging")?
                .clone()
        };
        let (width, height, pixels) = preview;
        self.state.cancel_drag.set(false);
        let drag = PreparedDrag::new(
            &path,
            width,
            height,
            &pixels,
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
    state.pill_motion = None;
    state.pill_center = state
        .layout
        .rect(state.active_tool)
        .map_or(0.0, |r| r.x + r.w / 2.0);
    if state.background.open {
        state.background.scroll_y = state
            .background
            .scroll_y
            .min(state.background.max_scroll(state.layout.height));
        let shift = background::PANEL_WIDTH.min((state.layout.canvas.w - 1.0).max(0.0));
        state.layout.canvas.x += shift;
        state.layout.canvas.w -= shift;
    }
    if let Some(image) = state.composed.as_ref().or(state.image.as_ref()) {
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
            state.composed.as_ref().or(state.image.as_ref()),
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
fn refresh_background(hwnd: HWND, state: &mut WindowState) -> Result<()> {
    let result = (|| {
        let composed = if let Some(image) = &state.image
            && state.background.selected()
        {
            Some(background::compose(image, &state.background)?)
        } else {
            None
        };
        if let Some(image) = composed.as_ref().or(state.image.as_ref()) {
            if let Some(renderer) = &mut state.renderer {
                renderer.set_image(image)?;
            }
            state
                .view
                .clamp_pan(state.layout.canvas, image.width, image.height);
        }
        state.composed = composed;
        state.exported = None;
        state.view.fit_limit = if state.background.selected() {
            1.6
        } else {
            1.0
        };
        state.background_revision = state.background_revision.wrapping_add(1);
        state.committed_background = state.background.clone();
        Ok(())
    })();
    if result.is_err() {
        state.background = state.committed_background.clone();
    }
    unsafe {
        let _ = InvalidateRect(Some(hwnd), None, false);
    }
    result
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
    fn hover_eases_and_cleans_up_without_an_idle_timer() {
        let mut levels = vec![(Control::Fill, 0.0)];
        assert!(hover_step(&mut levels, Some(Control::Fill), 0.016));
        let first = levels[0].1;
        assert!(first > 0.0 && first < 1.0);
        assert!(hover_step(&mut levels, Some(Control::Fill), 0.016));
        assert!(levels[0].1 > first);
        for _ in 0..25 {
            hover_step(&mut levels, Some(Control::Fill), 0.016);
        }
        assert_eq!(levels[0].1, 1.0);
        for _ in 0..25 {
            hover_step(&mut levels, None, 0.016);
        }
        assert!(levels.is_empty());
        assert!(!hover_step(&mut levels, None, 0.016));
        let now = Instant::now();
        let motion = PillMotion {
            from: 100.0,
            velocity: 0.0,
            to: 129.0,
            min: 90.0,
            max: 150.0,
            start: now,
        };
        assert_eq!(motion.position(now), (100.0, 0.0, true));
        let (middle, velocity, _) = motion.position(now + std::time::Duration::from_millis(85));
        assert!(middle > 100.0 && middle < 129.0 && velocity > 0.0);
        let (overshoot, _, _) = motion.position(now + std::time::Duration::from_millis(285));
        assert!(overshoot > 130.0 && overshoot < 134.0);
        assert_eq!(
            motion.position(now + std::time::Duration::from_millis(700)),
            (129.0, 0.0, false)
        );
        let retarget = PillMotion {
            from: middle,
            velocity,
            to: 105.0,
            min: 90.0,
            max: 150.0,
            start: now + std::time::Duration::from_millis(85),
        };
        assert!((retarget.position(retarget.start).0 - middle).abs() < 0.001);
        let edge = PillMotion {
            to: 150.0,
            ..motion
        };
        assert!(edge.position(now + std::time::Duration::from_millis(285)).0 <= 150.0);
    }
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
