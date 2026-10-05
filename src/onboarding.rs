//! Native first-run setup. Google sign-in is deliberately a preview, never an auth check.
use std::{
    cell::{Cell, RefCell},
    os::windows::ffi::OsStrExt,
    path::PathBuf,
    sync::atomic::{AtomicBool, Ordering},
    time::{Duration, Instant},
};

use anyhow::{Context, Result};
use windows::{
    Win32::{
        Foundation::*,
        Graphics::{
            Dwm::{DWM_TIMING_INFO, DwmGetCompositionTimingInfo},
            Gdi::*,
        },
        Media::{
            TIME_KILL_SYNCHRONOUS, TIME_PERIODIC, timeBeginPeriod, timeEndPeriod, timeKillEvent,
            timeSetEvent,
        },
        System::LibraryLoader::GetModuleHandleW,
        UI::{
            Controls::{DRAWITEMSTRUCT, ODS_FOCUS, WM_MOUSELEAVE},
            HiDpi::{AdjustWindowRectExForDpi, GetDpiForSystem},
            Input::KeyboardAndMouse::{TME_LEAVE, TRACKMOUSEEVENT, TrackMouseEvent},
            Shell::{DefSubclassProc, RemoveWindowSubclass, SetWindowSubclass},
            WindowsAndMessaging::*,
        },
    },
    core::{BOOL, PCWSTR, w},
};

use crate::settings::{Appearance, AutoClose, CaptureShortcut, Placement, Settings};

const WIDTH: i32 = 740;
const HEIGHT: i32 = 600;
const GOOGLE: usize = 101;
const BACK: usize = 102;
const NEXT: usize = 103;
const OPTION_BASE: usize = 200;
const ANIMATION_TIMER: usize = 1;
const ANIMATION_TICK: u32 = WM_APP + 1;
static TICK_POSTED: AtomicBool = AtomicBool::new(false);
const PAGE_DURATION: Duration = Duration::from_millis(210);
const GROUP_SIZES: [usize; 4] = [3, 3, 4, 6];
const TITLES: [&str; 5] = [
    "Sign in to Catch It",
    "Pick your look",
    "One shortcut away",
    "Your corner",
    "How long should it stay?",
];
const SUBTITLES: [&str; 5] = [
    "Use your Google account to get started.",
    "Follow Windows or choose your own.",
    "Choose how you start a capture.",
    "Where should previews appear?",
    "Unpinned previews disappear automatically.",
];
const OPTIONS: [[&str; 6]; 4] = [
    ["Follow Windows", "Light", "Dark", "", "", ""],
    [
        "Alt + Shift + S",
        "Ctrl + Shift + S",
        "Ctrl + Alt + S",
        "",
        "",
        "",
    ],
    [
        "Bottom right",
        "Bottom left",
        "Top right",
        "Top left",
        "",
        "",
    ],
    [
        "5 seconds",
        "15 seconds",
        "30 seconds",
        "5 minutes",
        "10 minutes",
        "Never",
    ],
];

thread_local! {
    static STATE: RefCell<Option<Ui>> = const { RefCell::new(None) };
    static COMPLETED: Cell<bool> = const { Cell::new(false) };
}

struct Visual {
    hwnd: HWND,
    id: usize,
    hover: f32,
    press: f32,
    selection: f32,
    hovering: bool,
    pressing: bool,
}
struct Surface {
    dc: HDC,
    bitmap: HBITMAP,
    original: HGDIOBJ,
}
impl Drop for Surface {
    fn drop(&mut self) {
        unsafe {
            SelectObject(self.dc, self.original);
            let _ = DeleteObject(self.bitmap.into());
            let _ = DeleteDC(self.dc);
        }
    }
}
struct Transition {
    from: usize,
    to: usize,
    started: Instant,
    outgoing: Surface,
    incoming: Surface,
}
struct Frame {
    dirty: Vec<HWND>,
    repaint: bool,
    active: bool,
    finish_page: Option<usize>,
}
struct Ui {
    page: usize,
    picked: [usize; 4],
    visuals: Vec<Visual>,
    transition: Option<Transition>,
    last_tick: Instant,
    dot_position: f32,
    motion_enabled: bool,
    timer_running: bool,
    precise_timer: bool,
    media_timer: u32,
    keyboard_focus: bool,
    groups: [Vec<HWND>; 4],
    google: HWND,
    back: HWND,
    next: HWND,
    body: HFONT,
    bold: HFONT,
    title: HFONT,
    small: HFONT,
    bitmap: Option<HBITMAP>,
    canvas: Option<Surface>,
    marker: PathBuf,
    preview_only: bool,
    dpi: i32,
}
impl Ui {
    fn px(&self, value: i32) -> i32 {
        value * self.dpi / 96
    }
    fn visual(&self, id: usize) -> Option<&Visual> {
        self.visuals.iter().find(|visual| visual.id == id)
    }
    fn visual_mut(&mut self, id: usize) -> Option<&mut Visual> {
        self.visuals.iter_mut().find(|visual| visual.id == id)
    }
}

// Win32 can synchronously send focus/paint messages from ShowWindow or DestroyWindow.
// Temporarily detach UI state so nested callbacks never reborrow the same RefCell.
fn with_ui_mut<R>(callback: impl FnOnce(&mut Ui) -> R) -> Option<R> {
    let mut ui = STATE.with(|state| state.borrow_mut().take())?;
    let result = callback(&mut ui);
    STATE.with(|state| *state.borrow_mut() = Some(ui));
    Some(result)
}

fn tick_interval_for_hz(refresh_hz: f64) -> u32 {
    // Wake a little before each display refresh. Painting is coalesced on the UI thread.
    let hz = if refresh_hz.is_finite() && (30.0..=360.0).contains(&refresh_hz) {
        refresh_hz
    } else {
        60.0
    };
    (700.0 / hz).round().clamp(3.0, 12.0) as u32
}

fn tick_interval(hwnd: HWND) -> u32 {
    let mut info = DWM_TIMING_INFO {
        cbSize: std::mem::size_of::<DWM_TIMING_INFO>() as u32,
        ..Default::default()
    };
    let hz = if unsafe { DwmGetCompositionTimingInfo(hwnd, &mut info) }.is_ok()
        && info.rateRefresh.uiDenominator != 0
    {
        f64::from(info.rateRefresh.uiNumerator) / f64::from(info.rateRefresh.uiDenominator)
    } else {
        // DWM timing can be unavailable (remote desktop, compositor reset).
        // In that case query the monitor containing this window, not the primary display.
        let mut monitor = MONITORINFOEXW::default();
        monitor.monitorInfo.cbSize = std::mem::size_of::<MONITORINFOEXW>() as u32;
        let handle = unsafe { MonitorFromWindow(hwnd, MONITOR_DEFAULTTONEAREST) };
        let device = if unsafe { GetMonitorInfoW(handle, &mut monitor.monitorInfo) }.as_bool() {
            PCWSTR(monitor.szDevice.as_ptr())
        } else {
            PCWSTR::null()
        };
        let mut mode = DEVMODEW {
            dmSize: std::mem::size_of::<DEVMODEW>() as u16,
            ..Default::default()
        };
        if unsafe { EnumDisplaySettingsW(device, ENUM_CURRENT_SETTINGS, &mut mode) }.as_bool() {
            f64::from(mode.dmDisplayFrequency)
        } else {
            60.0
        }
    };
    tick_interval_for_hz(hz)
}

unsafe extern "system" fn animation_tick(
    _id: u32,
    _message: u32,
    window: usize,
    _reserved1: usize,
    _reserved2: usize,
) {
    if !TICK_POSTED.swap(true, Ordering::AcqRel)
        && unsafe {
            PostMessageW(
                Some(HWND(window as *mut _)),
                ANIMATION_TICK,
                WPARAM(0),
                LPARAM(0),
            )
        }
        .is_err()
    {
        TICK_POSTED.store(false, Ordering::Release);
    }
}

fn start_timer(hwnd: HWND, ui: &mut Ui) -> bool {
    if ui.timer_running {
        return true;
    }
    let precise = unsafe { timeBeginPeriod(1) == 0 };
    TICK_POSTED.store(false, Ordering::Release);
    let media_timer = if precise {
        unsafe {
            timeSetEvent(
                tick_interval(hwnd),
                1,
                Some(animation_tick),
                hwnd.0 as usize,
                TIME_PERIODIC | TIME_KILL_SYNCHRONOUS,
            )
        }
    } else {
        0
    };
    if media_timer == 0 && unsafe { SetTimer(Some(hwnd), ANIMATION_TIMER, 10, None) } == 0 {
        if precise {
            unsafe {
                let _ = timeEndPeriod(1);
            }
        }
        return false;
    }
    ui.media_timer = media_timer;
    ui.precise_timer = precise;
    ui.timer_running = true;
    true
}

fn stop_timer(hwnd: HWND, ui: &mut Ui) {
    if !ui.timer_running {
        return;
    }
    ui.timer_running = false;
    unsafe {
        if ui.media_timer != 0 {
            let _ = timeKillEvent(ui.media_timer);
        } else {
            let _ = KillTimer(Some(hwnd), ANIMATION_TIMER);
        }
        if ui.precise_timer {
            let _ = timeEndPeriod(1);
        }
    }
    ui.media_timer = 0;
    ui.precise_timer = false;
    TICK_POSTED.store(false, Ordering::Release);
}

#[cfg(test)]
#[test]
fn refresh_pacing_tracks_the_display() {
    assert_eq!(tick_interval_for_hz(60.0), 12);
    assert_eq!(tick_interval_for_hz(120.0), 6);
    assert_eq!(tick_interval_for_hz(144.0), 5);
    assert_eq!(tick_interval_for_hz(240.0), 3);
    assert_eq!(tick_interval_for_hz(360.0), 3);
    assert_eq!(tick_interval_for_hz(0.0), 12);
}

fn color(r: u8, g: u8, b: u8) -> COLORREF {
    COLORREF(u32::from(r) | (u32::from(g) << 8) | (u32::from(b) << 16))
}
fn mix(a: COLORREF, b: COLORREF, t: f32) -> COLORREF {
    let t = t.clamp(0.0, 1.0);
    let channel = |shift: u32| {
        let x = (a.0 >> shift) & 255u32;
        let y = (b.0 >> shift) & 255u32;
        (x as f32 + (y as f32 - x as f32) * t).round() as u32
    };
    COLORREF(channel(0) | (channel(8) << 8) | (channel(16) << 16))
}
fn smooth(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}
fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain(Some(0)).collect()
}
fn rect(x: i32, y: i32, width: i32, height: i32) -> RECT {
    RECT {
        left: x,
        top: y,
        right: x + width,
        bottom: y + height,
    }
}

unsafe fn fill(hdc: HDC, area: RECT, shade: COLORREF) {
    unsafe {
        let brush = CreateSolidBrush(shade);
        FillRect(hdc, &area, brush);
        let _ = DeleteObject(brush.into());
    }
}
unsafe fn rounded(hdc: HDC, area: RECT, background: COLORREF, stroke: COLORREF, radius: i32) {
    unsafe {
        let brush = CreateSolidBrush(background);
        let pen = CreatePen(PS_SOLID, 1, stroke);
        let old_brush = SelectObject(hdc, brush.into());
        let old_pen = SelectObject(hdc, pen.into());
        let _ = RoundRect(
            hdc,
            area.left,
            area.top,
            area.right,
            area.bottom,
            radius,
            radius,
        );
        SelectObject(hdc, old_brush);
        SelectObject(hdc, old_pen);
        let _ = DeleteObject(brush.into());
        let _ = DeleteObject(pen.into());
    }
}
unsafe fn text(
    hdc: HDC,
    value: &str,
    area: RECT,
    font: HFONT,
    shade: COLORREF,
    flags: DRAW_TEXT_FORMAT,
) {
    unsafe {
        let old = SelectObject(hdc, font.into());
        SetBkMode(hdc, TRANSPARENT);
        SetTextColor(hdc, shade);
        let mut value = wide(value);
        let len = value.len() - 1;
        let mut area = area;
        DrawTextW(hdc, &mut value[..len], &mut area, flags);
        SelectObject(hdc, old);
    }
}

unsafe fn draw_content(hdc: HDC, ui: &Ui, page: usize) {
    unsafe {
        let s = |value| ui.px(value);
        let x = s;
        text(
            hdc,
            TITLES[page],
            rect(x(42), s(99), s(656), s(54)),
            ui.title,
            color(36, 31, 62),
            (if page == 0 { DT_CENTER } else { DT_LEFT }) | DT_VCENTER | DT_SINGLELINE,
        );
        text(
            hdc,
            SUBTITLES[page],
            rect(x(42), s(158), s(656), s(42)),
            ui.body,
            color(116, 108, 137),
            (if page == 0 { DT_CENTER } else { DT_LEFT }) | DT_VCENTER | DT_SINGLELINE,
        );
        if page == 0 {
            let hero = [
                (
                    98,
                    247,
                    425,
                    114,
                    22,
                    color(234, 226, 255),
                    color(234, 226, 255),
                ),
                (
                    190,
                    224,
                    437,
                    126,
                    22,
                    color(218, 236, 255),
                    color(218, 236, 255),
                ),
                (
                    157,
                    231,
                    426,
                    128,
                    19,
                    color(255, 255, 255),
                    color(223, 216, 243),
                ),
                (
                    183,
                    251,
                    50,
                    50,
                    14,
                    color(245, 239, 255),
                    color(245, 239, 255),
                ),
                (
                    264,
                    255,
                    273,
                    11,
                    5,
                    color(227, 223, 241),
                    color(227, 223, 241),
                ),
                (
                    264,
                    278,
                    207,
                    9,
                    5,
                    color(236, 233, 246),
                    color(236, 233, 246),
                ),
                (
                    264,
                    304,
                    123,
                    25,
                    9,
                    color(119, 88, 244),
                    color(119, 88, 244),
                ),
            ];
            for (hx, hy, w, h, r, fill_color, border_color) in hero {
                rounded(
                    hdc,
                    rect(x(hx), s(hy), s(w), s(h)),
                    fill_color,
                    border_color,
                    s(r),
                );
            }
            text(
                hdc,
                "Google sign-in is not available yet.",
                rect(x(42), s(436), s(656), s(34)),
                ui.small,
                color(116, 108, 137),
                DT_CENTER | DT_VCENTER | DT_SINGLELINE,
            );
        }
    }
}

unsafe fn draw_main(hdc: HDC, ui: &Ui) {
    unsafe {
        let s = |value| ui.px(value);
        fill(hdc, rect(0, 0, s(WIDTH), s(HEIGHT)), color(251, 250, 254));
        text(
            hdc,
            &format!("{} / 5", ui.page + 1),
            rect(s(624), s(28), s(74), s(34)),
            ui.small,
            color(126, 111, 165),
            DT_RIGHT | DT_VCENTER | DT_SINGLELINE,
        );
        if let Some(transition) = &ui.transition {
            let t =
                smooth(transition.started.elapsed().as_secs_f32() / PAGE_DURATION.as_secs_f32());
            let top = s(90);
            let height = s(419);
            let width = s(WIDTH);
            // Swap pages while almost invisible, so their text never overlaps.
            let (surface, opacity) = if t < 0.5 {
                (&transition.outgoing, 0.06 + 0.94 * (1.0 - smooth(t * 2.0)))
            } else {
                (&transition.incoming, 0.06 + 0.94 * smooth((t - 0.5) * 2.0))
            };
            let _ = AlphaBlend(
                hdc,
                0,
                top,
                width,
                height,
                surface.dc,
                0,
                top,
                width,
                height,
                BLENDFUNCTION {
                    BlendOp: AC_SRC_OVER as u8,
                    BlendFlags: 0,
                    SourceConstantAlpha: (opacity * 255.0).round() as u8,
                    AlphaFormat: 0,
                },
            );
        } else {
            draw_content(hdc, ui, ui.page);
        }
        fill(hdc, rect(0, s(509), s(WIDTH), 1), color(232, 229, 239));
        for index in 0..5 {
            rounded(
                hdc,
                rect(s(306 + index * 28), s(544), s(8), s(8)),
                color(215, 208, 231),
                color(215, 208, 231),
                s(5),
            );
        }
        rounded(
            hdc,
            rect(
                s(306) + (s(28) as f32 * ui.dot_position) as i32,
                s(544),
                s(22),
                s(8),
            ),
            color(119, 88, 244),
            color(119, 88, 244),
            s(5),
        );
    }
}

unsafe fn draw_option(item: &DRAWITEMSTRUCT, ui: &Ui, group: usize, index: usize) {
    unsafe {
        let hdc = item.hDC;
        let r = item.rcItem;
        let visual = ui.visual(OPTION_BASE + group * 10 + index);
        let selection = visual.map_or(0.0, |item| item.selection);
        let hover = visual.map_or(0.0, |item| item.hover);
        let press = visual.map_or(0.0, |item| item.press);
        fill(hdc, r, color(251, 250, 254));
        let bg = mix(
            mix(color(255, 255, 255), color(246, 243, 254), hover),
            color(244, 240, 255),
            selection,
        );
        let border = mix(
            mix(color(222, 220, 231), color(167, 151, 226), hover),
            color(119, 89, 244),
            selection,
        );
        let inset = ui.px(2) + (ui.px(2) as f32 * press).round() as i32;
        rounded(
            hdc,
            rect(
                r.left + inset,
                r.top + inset,
                r.right - r.left - inset * 2,
                r.bottom - r.top - inset * 2,
            ),
            bg,
            border,
            ui.px(14),
        );
        let w = r.right - r.left;
        let h = r.bottom - r.top;
        if group == 0 {
            let dark = index == 2;
            let preview_bg = if dark {
                color(39, 40, 57)
            } else {
                color(247, 248, 253)
            };
            rounded(
                hdc,
                rect(ui.px(17), ui.px(23), w - ui.px(34), ui.px(99)),
                preview_bg,
                if dark {
                    color(84, 85, 105)
                } else {
                    color(216, 219, 231)
                },
                ui.px(9),
            );
            if index == 0 {
                fill(
                    hdc,
                    rect(w / 2, ui.px(44), w / 2 - ui.px(18), ui.px(77)),
                    color(39, 40, 57),
                );
            }
            fill(
                hdc,
                rect(ui.px(18), ui.px(24), w - ui.px(36), ui.px(19)),
                if dark {
                    color(58, 59, 77)
                } else {
                    color(231, 231, 243)
                },
            );
            rounded(
                hdc,
                rect(ui.px(30), ui.px(60), w - ui.px(89), ui.px(39)),
                if dark {
                    color(77, 77, 99)
                } else {
                    color(255, 255, 255)
                },
                if dark {
                    color(77, 77, 99)
                } else {
                    color(236, 234, 247)
                },
                ui.px(7),
            );
            if index == 0 {
                rounded(
                    hdc,
                    rect(w - ui.px(67), ui.px(64), ui.px(34), ui.px(31)),
                    color(77, 77, 99),
                    color(77, 77, 99),
                    ui.px(5),
                );
            }
            text(
                hdc,
                OPTIONS[group][index],
                rect(ui.px(16), h - ui.px(59), w - ui.px(32), ui.px(29)),
                ui.bold,
                color(39, 37, 59),
                DT_LEFT | DT_VCENTER | DT_SINGLELINE,
            );
            let sub = ["Match your system", "Bright and clean", "Easy on the eyes"][index];
            text(
                hdc,
                sub,
                rect(ui.px(16), h - ui.px(35), w - ui.px(25), ui.px(23)),
                ui.small,
                color(115, 112, 132),
                DT_LEFT | DT_VCENTER | DT_SINGLELINE,
            );
        } else {
            if group == 2 {
                let screen_x = w - ui.px(110);
                rounded(
                    hdc,
                    rect(screen_x, ui.px(23), ui.px(79), ui.px(51)),
                    color(244, 242, 251),
                    color(215, 212, 232),
                    ui.px(7),
                );
                let left = index == 1 || index == 3;
                let top = index == 2 || index == 3;
                rounded(
                    hdc,
                    rect(
                        screen_x + ui.px(if left { 7 } else { 49 }),
                        ui.px(if top { 30 } else { 52 }),
                        ui.px(23),
                        ui.px(13),
                    ),
                    color(124, 97, 249),
                    color(124, 97, 249),
                    ui.px(4),
                );
            }
            let label = OPTIONS[group][index];
            text(
                hdc,
                label,
                rect(ui.px(20), ui.px(8), w - ui.px(62), h - ui.px(16)),
                ui.bold,
                color(39, 37, 59),
                DT_LEFT | DT_VCENTER | DT_SINGLELINE,
            );
        }
        if selection > 0.02 {
            let size = (ui.px(18) as f32 * selection).max(2.0).round() as i32;
            let center_x = w - ui.px(24);
            let center_y = ui.px(24);
            rounded(
                hdc,
                rect(center_x - size / 2, center_y - size / 2, size, size),
                color(119, 89, 244),
                color(119, 89, 244),
                size,
            );
            if selection > 0.55 {
                text(
                    hdc,
                    "✓",
                    rect(
                        center_x - ui.px(9),
                        center_y - ui.px(11),
                        ui.px(18),
                        ui.px(20),
                    ),
                    ui.small,
                    color(255, 255, 255),
                    DT_CENTER | DT_VCENTER | DT_SINGLELINE,
                );
            }
        }
        if ui.keyboard_focus && item.itemState.0 & ODS_FOCUS.0 != 0 {
            let focus = RECT {
                left: r.left + ui.px(6),
                top: r.top + ui.px(6),
                right: r.right - ui.px(6),
                bottom: r.bottom - ui.px(6),
            };
            let _ = DrawFocusRect(hdc, &focus);
        }
    }
}

unsafe fn draw_google(item: &DRAWITEMSTRUCT, ui: &Ui) {
    unsafe {
        let area = item.rcItem;
        let width = area.right - area.left;
        let height = area.bottom - area.top;
        let visual = ui.visual(GOOGLE);
        let hover = visual.map_or(0.0, |item| item.hover);
        let press = visual.map_or(0.0, |item| item.press);
        fill(item.hDC, area, color(251, 250, 254));
        if let Some(bitmap) = ui.bitmap {
            let memory = CreateCompatibleDC(Some(item.hDC));
            if !memory.0.is_null() {
                let old = SelectObject(memory, bitmap.into());
                let inset = (press * ui.px(2) as f32).round() as i32;
                let _ = StretchBlt(
                    item.hDC,
                    area.left + inset,
                    area.top + inset,
                    width - inset * 2,
                    height - inset * 2,
                    Some(memory),
                    0,
                    0,
                    width,
                    height,
                    SRCCOPY,
                );
                SelectObject(memory, old);
                let _ = DeleteDC(memory);
            }
        } else {
            rounded(
                item.hDC,
                rect(area.left + 1, area.top + 1, width - 2, height - 2),
                color(255, 255, 255),
                color(211, 205, 225),
                ui.px(5),
            );
            text(
                item.hDC,
                "Sign in with Google",
                area,
                ui.body,
                color(39, 37, 59),
                DT_CENTER | DT_VCENTER | DT_SINGLELINE,
            );
        }
        if hover > 0.01 {
            let brush = CreateSolidBrush(mix(color(217, 211, 230), color(144, 119, 241), hover));
            let outline = rect(area.left + 1, area.top + 1, width - 2, height - 2);
            let _ = FrameRect(item.hDC, &outline, brush);
            let _ = DeleteObject(brush.into());
        }
        if ui.keyboard_focus && item.itemState.0 & ODS_FOCUS.0 != 0 {
            let focus = rect(
                area.left + ui.px(4),
                area.top + ui.px(4),
                width - ui.px(8),
                height - ui.px(8),
            );
            let _ = DrawFocusRect(item.hDC, &focus);
        }
    }
}

unsafe fn draw_action(item: &DRAWITEMSTRUCT, ui: &Ui) {
    unsafe {
        let primary = item.CtlID as usize == NEXT;
        let area = item.rcItem;
        let focused = item.itemState.0 & ODS_FOCUS.0 != 0;
        let visual = ui.visual(item.CtlID as usize);
        let hover = visual.map_or(0.0, |item| item.hover);
        let press = visual.map_or(0.0, |item| item.press);
        let label = if primary {
            if ui.page == 0 {
                "Continue without login"
            } else if ui.page == 4 {
                "Start using Catch It"
            } else {
                "Continue"
            }
        } else {
            "Back"
        };
        rounded(
            item.hDC,
            rect(
                area.left + 1,
                area.top + 1,
                area.right - area.left - 2,
                area.bottom - area.top - 2,
            ),
            if primary {
                mix(
                    mix(color(114, 85, 242), color(132, 106, 250), hover),
                    color(96, 69, 215),
                    press,
                )
            } else {
                mix(
                    mix(color(255, 255, 255), color(247, 244, 255), hover),
                    color(235, 229, 251),
                    press,
                )
            },
            if primary {
                color(114, 85, 242)
            } else {
                mix(color(218, 216, 230), color(163, 147, 225), hover)
            },
            ui.px(11),
        );
        text(
            item.hDC,
            label,
            area,
            ui.bold,
            if primary {
                color(255, 255, 255)
            } else {
                color(65, 59, 91)
            },
            DT_CENTER | DT_VCENTER | DT_SINGLELINE,
        );
        if focused && ui.keyboard_focus {
            let focus = rect(
                area.left + ui.px(5),
                area.top + ui.px(5),
                area.right - area.left - ui.px(10),
                area.bottom - area.top - ui.px(10),
            );
            let _ = DrawFocusRect(item.hDC, &focus);
        }
    }
}

unsafe extern "system" fn button_proc(
    hwnd: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
    _subclass_id: usize,
    id: usize,
) -> LRESULT {
    if message == WM_NCDESTROY {
        unsafe {
            let _ = RemoveWindowSubclass(hwnd, Some(button_proc), 1);
        }
    }
    if message == WM_KEYDOWN {
        STATE.with(|state| {
            if let Some(ui) = state.borrow_mut().as_mut() {
                ui.keyboard_focus = true;
            }
        });
        unsafe {
            let _ = InvalidateRect(Some(hwnd), None, false);
        }
    }
    if matches!(
        message,
        WM_MOUSEMOVE | WM_MOUSELEAVE | WM_LBUTTONDOWN | WM_LBUTTONUP | WM_KILLFOCUS
    ) {
        if message == WM_MOUSEMOVE
            && STATE.with(|state| {
                state
                    .borrow()
                    .as_ref()
                    .and_then(|ui| ui.visual(id))
                    .is_some_and(|visual| !visual.hovering)
            })
        {
            let mut tracking = TRACKMOUSEEVENT {
                cbSize: std::mem::size_of::<TRACKMOUSEEVENT>() as u32,
                dwFlags: TME_LEAVE,
                hwndTrack: hwnd,
                dwHoverTime: 0,
            };
            unsafe {
                let _ = TrackMouseEvent(&mut tracking);
            }
        }
        STATE.with(|state| {
            if let Some(ui) = state.borrow_mut().as_mut() {
                let motion_enabled = ui.motion_enabled;
                if message == WM_LBUTTONDOWN {
                    ui.keyboard_focus = false;
                }
                if let Some(visual) = ui.visual_mut(id) {
                    let was_hovering = visual.hovering;
                    let was_pressing = visual.pressing;
                    match message {
                        WM_MOUSEMOVE => visual.hovering = true,
                        WM_MOUSELEAVE => {
                            visual.hovering = false;
                            visual.pressing = false;
                        }
                        WM_LBUTTONDOWN => visual.pressing = true,
                        WM_LBUTTONUP | WM_KILLFOCUS => visual.pressing = false,
                        _ => (),
                    }
                    let changed =
                        was_hovering != visual.hovering || was_pressing != visual.pressing;
                    if motion_enabled && id < OPTION_BASE && changed {
                        if let Ok(parent) = unsafe { GetParent(hwnd) } {
                            let _ = start_timer(parent, ui);
                        }
                    } else if changed {
                        visual.hover = f32::from(visual.hovering);
                        visual.press = f32::from(visual.pressing);
                        unsafe {
                            let _ = InvalidateRect(Some(hwnd), None, false);
                        }
                    }
                }
            }
        });
    }
    unsafe { DefSubclassProc(hwnd, message, wparam, lparam) }
}

fn register_visual(ui: &mut Ui, hwnd: HWND, id: usize, selection: f32) -> Result<()> {
    anyhow::ensure!(
        unsafe { SetWindowSubclass(hwnd, Some(button_proc), 1, id) }.as_bool(),
        "Cannot attach onboarding button interactions"
    );
    ui.visuals.push(Visual {
        hwnd,
        id,
        hover: 0.0,
        press: 0.0,
        selection,
        hovering: false,
        pressing: false,
    });
    Ok(())
}

fn page_controls(ui: &Ui, page: usize) -> &[HWND] {
    if page == 0 {
        std::slice::from_ref(&ui.google)
    } else {
        &ui.groups[page - 1]
    }
}

// Keep painting off-screen so the window never exposes a half-drawn frame.
fn blank_surface(hwnd: HWND, ui: &Ui) -> Option<Surface> {
    unsafe {
        let screen = GetDC(Some(hwnd));
        if screen.0.is_null() {
            return None;
        }
        let dc = CreateCompatibleDC(Some(screen));
        let bitmap = CreateCompatibleBitmap(screen, ui.px(WIDTH), ui.px(HEIGHT));
        let _ = ReleaseDC(Some(hwnd), screen);
        if dc.0.is_null() || bitmap.0.is_null() {
            if !dc.0.is_null() {
                let _ = DeleteDC(dc);
            }
            if !bitmap.0.is_null() {
                let _ = DeleteObject(bitmap.into());
            }
            return None;
        }
        let original = SelectObject(dc, bitmap.into());
        if original.0.is_null() {
            let _ = DeleteObject(bitmap.into());
            let _ = DeleteDC(dc);
            return None;
        }
        Some(Surface {
            dc,
            bitmap,
            original,
        })
    }
}

// Capture both complete pages before the transition starts.
fn snapshot(hwnd: HWND, ui: &Ui, page: usize) -> Option<Surface> {
    let surface = blank_surface(hwnd, ui)?;
    let dc = surface.dc;
    unsafe {
        fill(
            dc,
            rect(0, 0, ui.px(WIDTH), ui.px(HEIGHT)),
            color(251, 250, 254),
        );
        draw_content(dc, ui, page);
        if page == 0 {
            let item = DRAWITEMSTRUCT {
                hDC: dc,
                CtlID: GOOGLE as u32,
                rcItem: rect(0, 0, ui.px(238), ui.px(54)),
                ..Default::default()
            };
            let mut previous = POINT::default();
            let _ = SetViewportOrgEx(dc, ui.px(251), ui.px(372), Some(&mut previous));
            draw_google(&item, ui);
            let _ = SetViewportOrgEx(dc, previous.x, previous.y, None);
        } else {
            let group = page - 1;
            for index in 0..GROUP_SIZES[group] {
                let (x, y, w, h) = layout(group, index);
                let item = DRAWITEMSTRUCT {
                    hDC: dc,
                    CtlID: (OPTION_BASE + group * 10 + index) as u32,
                    rcItem: rect(0, 0, ui.px(w), ui.px(h)),
                    ..Default::default()
                };
                let mut previous = POINT::default();
                let _ = SetViewportOrgEx(dc, ui.px(x), ui.px(y), Some(&mut previous));
                draw_option(&item, ui, group, index);
                let _ = SetViewportOrgEx(dc, previous.x, previous.y, None);
            }
        }
        Some(surface)
    }
}

fn begin_page(hwnd: HWND, ui: &mut Ui, page: usize) {
    if ui.transition.is_some() || ui.page == page {
        return;
    }
    if !ui.motion_enabled {
        show_page(hwnd, ui, page);
        return;
    }
    let (Some(outgoing), Some(incoming)) = (snapshot(hwnd, ui, ui.page), snapshot(hwnd, ui, page))
    else {
        show_page(hwnd, ui, page);
        return;
    };
    if !start_timer(hwnd, ui) {
        show_page(hwnd, ui, page);
        return;
    }
    let from = ui.page;
    for &control in page_controls(ui, from) {
        unsafe {
            let _ = ShowWindow(control, SW_HIDE);
        }
    }
    ui.page = page;
    ui.transition = Some(Transition {
        from,
        to: page,
        started: Instant::now(),
        outgoing,
        incoming,
    });
    unsafe {
        let _ = ShowWindow(ui.back, if page == 0 { SW_HIDE } else { SW_SHOW });
        let _ = InvalidateRect(Some(hwnd), None, false);
        let _ = InvalidateRect(Some(ui.next), None, false);
    }
}

fn show_page(hwnd: HWND, ui: &mut Ui, page: usize) {
    ui.page = page;
    ui.dot_position = page as f32;
    unsafe {
        let _ = ShowWindow(ui.google, if page == 0 { SW_SHOW } else { SW_HIDE });
        let _ = ShowWindow(ui.back, if page == 0 { SW_HIDE } else { SW_SHOW });
        for (group, controls) in ui.groups.iter().enumerate() {
            for &control in controls {
                let _ = ShowWindow(control, if page == group + 1 { SW_SHOW } else { SW_HIDE });
            }
        }
        let _ = InvalidateRect(Some(hwnd), None, true);
        let _ = InvalidateRect(Some(ui.next), None, true);
    }
}

fn tick(ui: &mut Ui) -> Frame {
    let now = Instant::now();
    let dt = now.duration_since(ui.last_tick).as_secs_f32().min(0.05);
    ui.last_tick = now;
    let mut frame = Frame {
        dirty: Vec::new(),
        repaint: false,
        active: false,
        finish_page: None,
    };
    for visual in &mut ui.visuals {
        let selected = if (OPTION_BASE..OPTION_BASE + 40).contains(&visual.id) {
            let group = (visual.id - OPTION_BASE) / 10;
            let index = (visual.id - OPTION_BASE) % 10;
            ui.picked[group] == index
        } else {
            false
        };
        let mut changed = false;
        for (value, target, tau) in [
            (
                &mut visual.hover,
                if visual.hovering { 1.0 } else { 0.0 },
                0.075,
            ),
            (
                &mut visual.press,
                if visual.pressing { 1.0 } else { 0.0 },
                0.055,
            ),
            (
                &mut visual.selection,
                if selected { 1.0 } else { 0.0 },
                0.085,
            ),
        ] {
            if (*value - target).abs() > 0.004 {
                *value += (target - *value) * (1.0 - (-dt / tau).exp());
                changed = true;
                frame.active = true;
            } else if *value != target {
                *value = target;
                changed = true;
            }
        }
        if changed {
            frame.dirty.push(visual.hwnd);
        }
    }
    if let Some(transition) = &ui.transition {
        let raw = (now.duration_since(transition.started).as_secs_f32()
            / PAGE_DURATION.as_secs_f32())
        .min(1.0);
        ui.dot_position =
            transition.from as f32 + (transition.to as f32 - transition.from as f32) * smooth(raw);
        frame.repaint = true;
        frame.active |= raw < 1.0;
        if raw >= 1.0 {
            frame.finish_page = Some(transition.to);
            ui.dot_position = ui.page as f32;
        }
    }
    frame
}

fn save(ui: &Ui) -> Result<()> {
    let prefs = Settings {
        appearance: [Appearance::System, Appearance::Light, Appearance::Dark][ui.picked[0]],
        shortcut: [
            CaptureShortcut::AltShiftS,
            CaptureShortcut::CtrlShiftS,
            CaptureShortcut::CtrlAltS,
        ][ui.picked[1]],
        placement: [
            Placement::BottomRight,
            Placement::BottomLeft,
            Placement::TopRight,
            Placement::TopLeft,
        ][ui.picked[2]],
        auto_close: AutoClose::ALL[ui.picked[3]],
    };
    if !ui.preview_only {
        prefs.save()?;
        std::fs::write(&ui.marker, b"preview-mode\n")?;
    }
    COMPLETED.set(true);
    Ok(())
}

unsafe fn draw_button(item: &DRAWITEMSTRUCT, ui: &Ui) {
    unsafe {
        let width = item.rcItem.right - item.rcItem.left;
        let height = item.rcItem.bottom - item.rcItem.top;
        let dc = CreateCompatibleDC(Some(item.hDC));
        let bitmap = CreateCompatibleBitmap(item.hDC, width, height);
        let buffer = if !dc.0.is_null() && !bitmap.0.is_null() {
            let original = SelectObject(dc, bitmap.into());
            if original.0.is_null() {
                let _ = DeleteObject(bitmap.into());
                let _ = DeleteDC(dc);
                None
            } else {
                Some(Surface {
                    dc,
                    bitmap,
                    original,
                })
            }
        } else {
            if !bitmap.0.is_null() {
                let _ = DeleteObject(bitmap.into());
            }
            if !dc.0.is_null() {
                let _ = DeleteDC(dc);
            }
            None
        };
        let mut target = *item;
        if let Some(buffer) = &buffer {
            target.hDC = buffer.dc;
            target.rcItem = rect(0, 0, width, height);
            fill(buffer.dc, target.rcItem, color(251, 250, 254));
        }
        let id = target.CtlID as usize;
        if id == BACK || id == NEXT {
            draw_action(&target, ui);
        } else if id == GOOGLE {
            draw_google(&target, ui);
        } else if (OPTION_BASE..OPTION_BASE + 40).contains(&id) {
            let group = (id - OPTION_BASE) / 10;
            let index = (id - OPTION_BASE) % 10;
            if index < GROUP_SIZES[group] {
                draw_option(&target, ui, group, index);
            }
        }
        if let Some(buffer) = &buffer {
            let _ = BitBlt(
                item.hDC,
                item.rcItem.left,
                item.rcItem.top,
                width,
                height,
                Some(buffer.dc),
                0,
                0,
                SRCCOPY,
            );
        }
    }
}

unsafe extern "system" fn window_proc(
    hwnd: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    match message {
        WM_ERASEBKGND => return LRESULT(1),
        WM_PAINT => {
            let mut ps = PAINTSTRUCT::default();
            let hdc = unsafe { BeginPaint(hwnd, &mut ps) };
            STATE.with(|state| {
                if let Some(ui) = state.borrow().as_ref() {
                    unsafe {
                        if let Some(canvas) = &ui.canvas {
                            draw_main(canvas.dc, ui);
                            let _ = BitBlt(
                                hdc,
                                0,
                                0,
                                ui.px(WIDTH),
                                ui.px(HEIGHT),
                                Some(canvas.dc),
                                0,
                                0,
                                SRCCOPY,
                            );
                        } else {
                            draw_main(hdc, ui);
                        }
                    }
                }
            });
            unsafe {
                let _ = EndPaint(hwnd, &ps);
            }
            return LRESULT(0);
        }
        WM_DRAWITEM => {
            let item = unsafe { &*(lparam.0 as *const DRAWITEMSTRUCT) };
            STATE.with(|state| {
                if let Some(ui) = state.borrow().as_ref() {
                    unsafe {
                        draw_button(item, ui);
                    }
                }
            });
            return LRESULT(1);
        }
        WM_TIMER | ANIMATION_TICK if message == ANIMATION_TICK || wparam.0 == ANIMATION_TIMER => {
            if message == ANIMATION_TICK {
                TICK_POSTED.store(false, Ordering::Release);
            }
            if !STATE.with(|state| state.borrow().as_ref().is_some_and(|ui| ui.timer_running)) {
                return LRESULT(0);
            }
            let frame = STATE.with(|state| state.borrow_mut().as_mut().map(tick));
            if let Some(frame) = frame {
                unsafe {
                    if frame.repaint {
                        let _ = InvalidateRect(Some(hwnd), None, false);
                    }
                    if let Some(page) = frame.finish_page {
                        // Keep the completed page snapshot visible until every card is painted.
                        let _ = UpdateWindow(hwnd);
                        let controls = STATE.with(|state| {
                            state
                                .borrow()
                                .as_ref()
                                .map(|ui| page_controls(ui, page).to_vec())
                                .unwrap_or_default()
                        });
                        let shown = BeginDeferWindowPos(controls.len() as i32)
                            .and_then(|batch| {
                                controls.iter().try_fold(batch, |batch, &control| {
                                    DeferWindowPos(
                                        batch,
                                        control,
                                        None,
                                        0,
                                        0,
                                        0,
                                        0,
                                        SWP_SHOWWINDOW
                                            | SWP_NOMOVE
                                            | SWP_NOSIZE
                                            | SWP_NOZORDER
                                            | SWP_NOACTIVATE
                                            | SWP_NOREDRAW,
                                    )
                                })
                            })
                            .and_then(|batch| EndDeferWindowPos(batch))
                            .is_ok();
                        if !shown {
                            for &control in &controls {
                                let _ = ShowWindow(control, SW_SHOW);
                            }
                        }
                        for control in controls {
                            let _ = RedrawWindow(
                                Some(control),
                                None,
                                None,
                                RDW_INVALIDATE | RDW_UPDATENOW | RDW_NOERASE,
                            );
                        }
                        STATE.with(|state| {
                            if let Some(ui) = state.borrow_mut().as_mut() {
                                ui.transition = None;
                            }
                        });
                        let _ = InvalidateRect(Some(hwnd), None, false);
                    }
                    for button in frame.dirty {
                        let _ = InvalidateRect(Some(button), None, false);
                    }
                    if !frame.active {
                        STATE.with(|state| {
                            if let Some(ui) = state.borrow_mut().as_mut() {
                                stop_timer(hwnd, ui);
                            }
                        });
                    }
                }
            }
            return LRESULT(0);
        }
        WM_COMMAND => {
            let id = wparam.0 & 0xffff;
            if STATE.with(|state| {
                state
                    .borrow()
                    .as_ref()
                    .is_some_and(|ui| ui.transition.is_some())
            }) {
                return LRESULT(0);
            }
            if (OPTION_BASE..OPTION_BASE + 40).contains(&id) {
                let group = (id - OPTION_BASE) / 10;
                let index = (id - OPTION_BASE) % 10;
                let _ = with_ui_mut(|ui| {
                    if group < 4 && index < GROUP_SIZES[group] {
                        ui.picked[group] = index;
                        if !ui.motion_enabled || !start_timer(hwnd, ui) {
                            for visual in &mut ui.visuals {
                                if visual.id / 10 == (OPTION_BASE + group * 10) / 10 {
                                    visual.selection =
                                        f32::from((visual.id - OPTION_BASE) % 10 == index);
                                }
                            }
                            for &control in &ui.groups[group] {
                                unsafe {
                                    let _ = InvalidateRect(Some(control), None, false);
                                }
                            }
                        }
                    }
                });
            } else if id == GOOGLE {
                unsafe {
                    MessageBoxW(
                        Some(hwnd),
                        w!(
                            "Google sign-in is not available yet. No account or purchase has been verified. Choose Continue without login to try Catch It."
                        ),
                        w!("Google connection coming soon"),
                        MB_OK | MB_ICONINFORMATION,
                    );
                }
            } else if id == NEXT {
                let should_close = with_ui_mut(|ui| {
                    if ui.page < 4 {
                        begin_page(hwnd, ui, ui.page + 1);
                        false
                    } else if let Err(error) = save(ui) {
                        let msg = wide(&format!("Could not save your preferences: {error:#}"));
                        unsafe {
                            MessageBoxW(
                                Some(hwnd),
                                PCWSTR(msg.as_ptr()),
                                w!("Catch It"),
                                MB_OK | MB_ICONERROR,
                            );
                        }
                        false
                    } else {
                        true
                    }
                })
                .unwrap_or(false);
                unsafe {
                    if should_close {
                        let _ = DestroyWindow(hwnd);
                    } else {
                        let _ = UpdateWindow(hwnd);
                    }
                }
            } else if id == BACK {
                let _ = with_ui_mut(|ui| {
                    if ui.page > 0 {
                        begin_page(hwnd, ui, ui.page - 1);
                    }
                });
                unsafe {
                    let _ = UpdateWindow(hwnd);
                }
            }
            return LRESULT(0);
        }
        WM_CLOSE => {
            unsafe {
                let _ = DestroyWindow(hwnd);
            }
            return LRESULT(0);
        }
        WM_DESTROY => {
            STATE.with(|state| {
                if let Some(ui) = state.borrow_mut().as_mut() {
                    stop_timer(hwnd, ui);
                }
            });
            unsafe {
                PostQuitMessage(0);
            }
            return LRESULT(0);
        }
        _ => (),
    }
    unsafe { DefWindowProcW(hwnd, message, wparam, lparam) }
}

// The native CreateWindow boundary carries a caption, id, rectangle and DPI.
#[allow(clippy::too_many_arguments)]
fn create_button(
    hwnd: HWND,
    text: &str,
    id: usize,
    x: i32,
    y: i32,
    w: i32,
    h: i32,
    dpi: i32,
) -> Result<HWND> {
    let text = wide(text);
    unsafe {
        let button = CreateWindowExW(
            WINDOW_EX_STYLE::default(),
            w!("BUTTON"),
            PCWSTR(text.as_ptr()),
            WS_CHILD | WS_VISIBLE | WS_TABSTOP | WINDOW_STYLE(BS_OWNERDRAW as u32),
            x * dpi / 96,
            y * dpi / 96,
            w * dpi / 96,
            h * dpi / 96,
            Some(hwnd),
            Some(HMENU(id as *mut _)),
            Some(GetModuleHandleW(None)?.into()),
            None,
        )?;
        Ok(button)
    }
}
fn layout(group: usize, index: usize) -> (i32, i32, i32, i32) {
    match group {
        0 => (42 + index as i32 * 218, 222, 207, 224),
        1 => (42, 215 + index as i32 * 76, 656, 65),
        2 => (
            42 + (index as i32 % 2) * 336,
            214 + (index as i32 / 2) * 116,
            320,
            99,
        ),
        _ => (
            42 + (index as i32 % 3) * 218,
            212 + (index as i32 / 3) * 116,
            207,
            99,
        ),
    }
}
fn marker_path() -> Result<PathBuf> {
    Ok(
        PathBuf::from(std::env::var_os("LOCALAPPDATA").context("LOCALAPPDATA is not set")?)
            .join("CatchIt")
            .join("onboarding-complete.txt"),
    )
}

/// Returns false if onboarding was dismissed, leaving the next launch pending.
pub fn run_if_needed() -> Result<bool> {
    let exe = std::env::current_exe()?;
    let installed = exe
        .parent()
        .is_some_and(|dir| dir.join("install-layout.txt").is_file());
    let preview_only = std::env::args_os().any(|arg| arg == "--onboarding-preview");
    let marker = marker_path()?;
    if !preview_only && (!installed || marker.exists()) {
        return Ok(true);
    }
    COMPLETED.set(false);
    let dpi = unsafe { GetDpiForSystem() as i32 };
    let style = WS_OVERLAPPED | WS_CAPTION | WS_SYSMENU | WS_MINIMIZEBOX | WS_CLIPCHILDREN;
    let mut bounds = rect(0, 0, WIDTH * dpi / 96, HEIGHT * dpi / 96);
    unsafe {
        AdjustWindowRectExForDpi(
            &mut bounds,
            style,
            false,
            WINDOW_EX_STYLE::default(),
            dpi as u32,
        )?;
    }
    let width = bounds.right - bounds.left;
    let height = bounds.bottom - bounds.top;
    let class = w!("CatchIt.Onboarding");
    let icon_path = exe
        .parent()
        .context("Missing executable folder")?
        .join("assets/catch-it.ico");
    #[cfg(debug_assertions)]
    let icon_path = if icon_path.is_file() {
        icon_path
    } else {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("assets/brand/catch-it.ico")
    };
    let icon_name: Vec<u16> = icon_path.as_os_str().encode_wide().chain(Some(0)).collect();
    let hwnd = unsafe {
        let instance = GetModuleHandleW(None)?;
        let icon = LoadImageW(
            None,
            PCWSTR(icon_name.as_ptr()),
            IMAGE_ICON,
            32 * dpi / 96,
            32 * dpi / 96,
            LR_LOADFROMFILE,
        )
        .map(|handle| HICON(handle.0))
        .unwrap_or_default();
        let wc = WNDCLASSW {
            lpfnWndProc: Some(window_proc),
            hInstance: instance.into(),
            lpszClassName: class,
            hCursor: LoadCursorW(None, IDC_ARROW)?,
            hIcon: icon,
            ..Default::default()
        };
        anyhow::ensure!(
            RegisterClassW(&wc) != 0,
            "Cannot register onboarding window"
        );
        CreateWindowExW(
            WINDOW_EX_STYLE::default(),
            class,
            w!("Catch It - Welcome"),
            style,
            (GetSystemMetrics(SM_CXSCREEN) - width) / 2,
            (GetSystemMetrics(SM_CYSCREEN) - height) / 2,
            width,
            height,
            None,
            None,
            Some(instance.into()),
            None,
        )?
    };
    if let Err(error) = setup(hwnd, dpi, marker, preview_only) {
        unsafe {
            let _ = DestroyWindow(hwnd);
        }
        return Err(error);
    }
    unsafe {
        let _ = ShowWindow(hwnd, SW_SHOW);
        let _ = SetForegroundWindow(hwnd);
        let mut msg = MSG::default();
        while GetMessageW(&mut msg, None, 0, 0).0 > 0 {
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }
    STATE.with(|state| {
        if let Some(ui) = state.borrow_mut().take() {
            unsafe {
                for font in [ui.body, ui.bold, ui.title, ui.small] {
                    let _ = DeleteObject(font.into());
                }
                if let Some(bitmap) = ui.bitmap {
                    let _ = DeleteObject(bitmap.into());
                }
            }
        }
    });
    Ok(COMPLETED.get() && !preview_only)
}

fn setup(hwnd: HWND, dpi: i32, marker: PathBuf, preview_only: bool) -> Result<()> {
    unsafe {
        let font = |size: i32, weight: i32| {
            CreateFontW(
                -size * dpi / 96,
                0,
                0,
                0,
                weight,
                0,
                0,
                0,
                FONT_CHARSET(0),
                FONT_OUTPUT_PRECISION(0),
                FONT_CLIP_PRECISION(0),
                CLEARTYPE_QUALITY,
                0,
                w!("Segoe UI"),
            )
        };
        let current = Settings::load();
        let mut animation_enabled = BOOL(1);
        let _ = SystemParametersInfoW(
            SPI_GETCLIENTAREAANIMATION,
            0,
            Some((&mut animation_enabled as *mut BOOL).cast()),
            SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS(0),
        );
        let mut ui = Ui {
            page: 0,
            visuals: Vec::new(),
            transition: None,
            last_tick: Instant::now(),
            dot_position: 0.0,
            motion_enabled: animation_enabled.as_bool(),
            timer_running: false,
            precise_timer: false,
            media_timer: 0,
            keyboard_focus: false,
            picked: [
                match current.appearance {
                    Appearance::System => 0,
                    Appearance::Light => 1,
                    Appearance::Dark => 2,
                },
                match current.shortcut {
                    CaptureShortcut::AltShiftS => 0,
                    CaptureShortcut::CtrlShiftS => 1,
                    CaptureShortcut::CtrlAltS => 2,
                },
                match current.placement {
                    Placement::BottomRight => 0,
                    Placement::BottomLeft => 1,
                    Placement::TopRight => 2,
                    Placement::TopLeft => 3,
                },
                AutoClose::ALL
                    .iter()
                    .position(|item| *item == current.auto_close)
                    .unwrap_or(0),
            ],
            groups: std::array::from_fn(|_| Vec::new()),
            google: HWND::default(),
            back: HWND::default(),
            next: HWND::default(),
            body: font(17, 400),
            bold: font(17, 600),
            title: font(31, 600),
            small: font(13, 600),
            bitmap: None,
            canvas: None,
            marker,
            preview_only,
            dpi,
        };
        for group in 0..4 {
            for (index, &label) in OPTIONS[group].iter().take(GROUP_SIZES[group]).enumerate() {
                let (x, y, w, h) = layout(group, index);
                let id = OPTION_BASE + group * 10 + index;
                let button = create_button(hwnd, label, id, x, y, w, h, dpi)?;
                let selected = if ui.picked[group] == index { 1.0 } else { 0.0 };
                register_visual(&mut ui, button, id, selected)?;
                ui.groups[group].push(button);
            }
        }
        ui.back = create_button(hwnd, "Back", BACK, 42, 528, 112, 43, dpi)?;
        let back = ui.back;
        register_visual(&mut ui, back, BACK, 0.0)?;
        ui.next = create_button(hwnd, "Continue without login", NEXT, 470, 525, 228, 47, dpi)?;
        let next = ui.next;
        register_visual(&mut ui, next, NEXT, 0.0)?;
        let path = std::env::current_exe()?
            .parent()
            .context("Missing installation folder")?
            .join("assets/google-signin-button.bmp");
        #[cfg(debug_assertions)]
        let path = if path.is_file() {
            path
        } else {
            PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("assets/third-party/google-signin-button.bmp")
        };
        let image = if path.is_file() {
            let name: Vec<u16> = path.as_os_str().encode_wide().chain(Some(0)).collect();
            LoadImageW(
                None,
                PCWSTR(name.as_ptr()),
                IMAGE_BITMAP,
                238 * dpi / 96,
                54 * dpi / 96,
                LR_LOADFROMFILE,
            )
            .ok()
        } else {
            None
        };
        ui.google = create_button(hwnd, "Sign in with Google", GOOGLE, 251, 372, 238, 54, dpi)?;
        ui.bitmap = image.map(|bitmap| HBITMAP(bitmap.0));
        let google = ui.google;
        register_visual(&mut ui, google, GOOGLE, 0.0)?;
        ui.canvas = blank_surface(hwnd, &ui);
        if ui.canvas.is_none() {
            ui.motion_enabled = false;
        }
        STATE.with(|state| *state.borrow_mut() = Some(ui));
        let _ = with_ui_mut(|ui| show_page(hwnd, ui, 0));
    }
    Ok(())
}
