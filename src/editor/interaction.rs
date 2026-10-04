//! Native input/callback handling. Keep modal work outside borrowed WindowState.
use super::*;
use windows::Win32::UI::Shell::{DefSubclassProc, SetWindowSubclass};

const TEXT_DONE: u32 = WM_APP + 52;
pub(super) unsafe extern "system" fn text_subclass(
    hwnd: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
    _id: usize,
    parent: usize,
) -> LRESULT {
    unsafe {
        if message == WM_KEYDOWN
            && (wparam.0 as u16 == 0x1b
                || (wparam.0 as u16 == 0x0d && GetKeyState(VK_CONTROL.0 as i32) < 0))
        {
            let _ = PostMessageW(
                Some(HWND(parent as *mut _)),
                TEXT_DONE,
                WPARAM(usize::from(wparam.0 as u16 != 0x1b)),
                LPARAM(0),
            );
            return LRESULT(0);
        }
        if message == WM_ERASEBKGND {
            let ptr =
                GetWindowLongPtrW(HWND(parent as *mut _), GWLP_USERDATA) as *const WindowState;
            if !ptr.is_null()
                && let Some(edit) = &(*ptr).text_editing
            {
                let mut rect = RECT::default();
                let _ = GetClientRect(hwnd, &mut rect);
                FillRect(HDC(wparam.0 as *mut _), &rect, edit.brush);
                return LRESULT(1);
            }
        }
        if message == WM_KILLFOCUS {
            let _ = PostMessageW(
                Some(HWND(parent as *mut _)),
                TEXT_DONE,
                WPARAM(1),
                LPARAM(0),
            );
        }
        DefSubclassProc(hwnd, message, wparam, lparam)
    }
}
fn start_text(hwnd: HWND, s: &mut WindowState, point: Point, index: Option<usize>) -> Result<()> {
    use std::os::windows::ffi::OsStrExt;
    let source = s.image.as_ref().context("Screenshot is still opening")?;
    let base = s.composed.as_ref().unwrap_or(source);
    let (ox, oy) = background::source_origin(source.width, source.height, &s.background)?;
    let (cx, cy, ex, ey) = s
        .document
        .crop_pixels(
            base.width,
            base.height,
            Point {
                x: ox as f32,
                y: oy as f32,
            },
        )
        .unwrap_or((0, 0, base.width, base.height));
    let r = s.view.image_rect(s.layout.canvas, ex - cx, ey - cy);
    let scale = s.view.scale(s.layout.canvas, ex - cx, ey - cy);
    let x = r.x + (point.x + ox as f32 - cx as f32) * scale;
    let y = r.y + (point.y + oy as f32 - cy as f32) * scale;
    let width = (300.0_f32).min((s.layout.width - x - 12.0).max(120.0));
    let text = index
        .and_then(|i| match &s.document.marks[i].shape {
            document::Shape::Text(_, text) => Some(text.as_str()),
            _ => None,
        })
        .unwrap_or("");
    let wide: Vec<u16> = std::ffi::OsStr::new(text)
        .encode_wide()
        .chain(Some(0))
        .collect();
    let dpi = s.dpi as f32 / 96.0;
    unsafe {
        let edit = CreateWindowExW(
            WINDOW_EX_STYLE::default(),
            w!("EDIT"),
            PCWSTR(wide.as_ptr()),
            WS_CHILD
                | WS_VISIBLE
                | WS_TABSTOP
                | WS_BORDER
                | WINDOW_STYLE((ES_MULTILINE | ES_AUTOVSCROLL | ES_WANTRETURN) as u32),
            (x * dpi).round() as i32,
            (y * dpi).round() as i32,
            (width.min(150.0) * dpi).round() as i32,
            (38.0 * dpi).round() as i32,
            Some(hwnd),
            None,
            Some(GetModuleHandleW(None)?.into()),
            None,
        )?;
        let _ = SendMessageW(edit, EM_SETLIMITTEXT, Some(WPARAM(4096)), Some(LPARAM(0)));
        let font = CreateFontW(
            -((20.0 * scale * dpi).round() as i32).max(8),
            0,
            0,
            0,
            FW_SEMIBOLD.0 as i32,
            0,
            0,
            0,
            DEFAULT_CHARSET,
            OUT_DEFAULT_PRECIS,
            CLIP_DEFAULT_PRECIS,
            ANTIALIASED_QUALITY,
            DEFAULT_PITCH.0 as u32,
            w!("Segoe UI"),
        );
        let brush = CreateSolidBrush(if s.dark {
            COLORREF(0x0025252d)
        } else {
            COLORREF(0x00ffffff)
        });
        if !SetWindowSubclass(edit, Some(text_subclass), 1, hwnd.0 as usize).as_bool() {
            let _ = DestroyWindow(edit);
            let _ = DeleteObject(HGDIOBJ(font.0));
            let _ = DeleteObject(HGDIOBJ(brush.0));
            anyhow::bail!("Cannot attach native text editor");
        }
        let _ = SendMessageW(
            edit,
            WM_SETFONT,
            Some(WPARAM(font.0 as usize)),
            Some(LPARAM(1)),
        );
        s.text_editing = Some(TextEdit {
            hwnd: edit,
            font,
            brush,
            point,
            index,
        });
        let _ = SetFocus(Some(edit));
        let _ = InvalidateRect(Some(hwnd), None, false);
    }
    Ok(())
}

fn source_point(s: &WindowState, p: (f32, f32)) -> Option<(Point, f32)> {
    let source = s.image.as_ref()?;
    let image = s.composed.as_ref().unwrap_or(source);
    let (ox, oy) = background::source_origin(source.width, source.height, &s.background).ok()?;
    let full = (0, 0, image.width, image.height);
    let (cx, cy, ex, ey) = if s.crop_mode {
        full
    } else {
        s.document
            .crop_pixels(
                image.width,
                image.height,
                Point {
                    x: ox as f32,
                    y: oy as f32,
                },
            )
            .unwrap_or(full)
    };
    let r = s.view.image_rect(s.layout.canvas, ex - cx, ey - cy);
    let scale = s.view.scale(s.layout.canvas, ex - cx, ey - cy);
    Some((
        Point {
            x: (p.0 - r.x) / scale + cx as f32 - ox as f32,
            y: (p.1 - r.y) / scale + cy as f32 - oy as f32,
        },
        scale,
    ))
}
#[derive(Clone, Copy)]
pub(super) struct CropGrab {
    edge: (i8, i8),
    start: Point,
    lo: Point,
    hi: Point,
    min_size: f32,
}
fn crop_hit(mark: &Mark, point: Point, tolerance: f32) -> Option<(i8, i8)> {
    let (lo, hi) = mark.bounds();
    for (x, y) in [
        (0, 0),
        (2, 0),
        (0, 2),
        (2, 2),
        (1, 0),
        (0, 1),
        (2, 1),
        (1, 2),
    ] {
        let hx = [lo.x, (lo.x + hi.x) / 2.0, hi.x][x];
        let hy = [lo.y, (lo.y + hi.y) / 2.0, hi.y][y];
        if (point.x - hx).abs() <= tolerance && (point.y - hy).abs() <= tolerance {
            return Some((x as i8 - 1, y as i8 - 1));
        }
    }
    if point.x > lo.x && point.x < hi.x && point.y > lo.y && point.y < hi.y {
        Some((0, 0))
    } else {
        None
    }
}
fn crop_adjust(grab: CropGrab, point: Point, width: f32, height: f32) -> (Point, Point) {
    let dx = (point.x - grab.start.x).round();
    let dy = (point.y - grab.start.y).round();
    let (mut lo, mut hi) = (grab.lo, grab.hi);
    if grab.edge == (0, 0) {
        let dx = dx.clamp(-lo.x, width - hi.x);
        let dy = dy.clamp(-lo.y, height - hi.y);
        lo.x += dx;
        hi.x += dx;
        lo.y += dy;
        hi.y += dy;
    } else {
        if grab.edge.0 < 0 {
            lo.x = (lo.x + dx).clamp(0.0, hi.x - grab.min_size.min(hi.x))
        }
        if grab.edge.0 > 0 {
            hi.x = (hi.x + dx).clamp(lo.x + grab.min_size.min(width - lo.x), width)
        }
        if grab.edge.1 < 0 {
            lo.y = (lo.y + dy).clamp(0.0, hi.y - grab.min_size.min(hi.y))
        }
        if grab.edge.1 > 0 {
            hi.y = (hi.y + dy).clamp(lo.y + grab.min_size.min(height - lo.y), height)
        }
    }
    (lo, hi)
}
fn apply_crop(s: &mut WindowState, hwnd: HWND) {
    let Some(mark) = s.crop_drag.take() else {
        return;
    };
    s.crop_grab = None;
    let Some(source) = s.image.as_ref() else {
        return;
    };
    let full = mark.bounds()
        == (
            Point::default(),
            Point {
                x: source.width as f32,
                y: source.height as f32,
            },
        );
    let existing = s
        .document
        .marks
        .iter()
        .rposition(|m| matches!(m.shape, document::Shape::Crop(..)));
    let result = if let Some(index) = existing {
        s.document.selected = Some(index);
        if full {
            s.document.delete_selected();
        } else {
            let original = s.document.marks[index].clone();
            s.document.marks[index] = mark;
            s.document.edit(index, original);
        }
        Ok(())
    } else if !full {
        s.document.add(mark)
    } else {
        Ok(())
    };
    s.document.selected = None;
    s.crop_mode = false;
    s.view.zoom = Zoom::Fit;
    s.view.pan = (0.0, 0.0);
    if let Err(error) = result {
        s.error = Some(format!("{error:#}"));
        s.request(hwnd, ERROR);
    }
    unsafe {
        let _ = InvalidateRect(Some(hwnd), None, false);
    }
}
fn crop_point(s: &WindowState, p: (f32, f32)) -> Option<Point> {
    let source = s.image.as_ref()?;
    let (point, _) = source_point(s, p)?;
    let lo = Point::default();
    let hi = Point {
        x: source.width as f32,
        y: source.height as f32,
    };
    Some(Point {
        x: point.x.clamp(lo.x, hi.x),
        y: point.y.clamp(lo.y, hi.y),
    })
}
fn redaction_point(mark: &Mark, point: Point, source: Option<&Raster>) -> Point {
    if matches!(
        mark.shape,
        document::Shape::Mosaic(..) | document::Shape::Spotlight(..) | document::Shape::Counter(..)
    ) && let Some(source) = source
    {
        return Point {
            x: point.x.clamp(0.0, source.width as f32),
            y: point.y.clamp(0.0, source.height as f32),
        };
    }
    point
}
fn inside_source(s: &WindowState, p: Point) -> bool {
    s.image.as_ref().is_some_and(|source| {
        let within_image =
            p.x >= 0.0 && p.y >= 0.0 && p.x < source.width as f32 && p.y < source.height as f32;
        within_image
            && (s.crop_mode
                || s.document
                    .crop()
                    .is_none_or(|(lo, hi)| p.x >= lo.x && p.y >= lo.y && p.x < hi.x && p.y < hi.y))
    })
}

pub(super) unsafe extern "system" fn window_proc(
    hwnd: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    unsafe {
        if message == WM_NCCREATE {
            let create = &*(lparam.0 as *const CREATESTRUCTW);
            SetWindowLongPtrW(hwnd, GWLP_USERDATA, create.lpCreateParams as isize);
            // Keep native frame semantics, but render the entire title bar in D2D.
            return DefWindowProcW(hwnd, message, wparam, lparam);
        }
        let ptr = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut WindowState;
        if ptr.is_null() {
            return DefWindowProcW(hwnd, message, wparam, lparam);
        }
        match message {
            TEXT_DONE => {
                let s = &mut *ptr;
                s.finish_text(wparam.0 != 0);
                let _ = InvalidateRect(Some(hwnd), None, false);
                LRESULT(0)
            }
            WM_COMMAND if (wparam.0 >> 16) as u16 == EN_CHANGE as u16 => {
                let s = &mut *ptr;
                if let Some(edit) = &s.text_editing
                    && lparam.0 == edit.hwnd.0 as isize
                {
                    let len = GetWindowTextLengthW(edit.hwnd).max(0) as usize;
                    let mut buf = vec![0u16; len + 1];
                    let read = GetWindowTextW(edit.hwnd, &mut buf).max(0) as usize;
                    let text = String::from_utf16_lossy(&buf[..read]);
                    let longest = text
                        .lines()
                        .map(|line| line.chars().count())
                        .max()
                        .unwrap_or(0) as i32;
                    let lines = SendMessageW(edit.hwnd, EM_GETLINECOUNT, None, None).0 as i32;
                    let dpi = s.dpi as f32 / 96.0;
                    let width = ((longest * 13 + 28).clamp(150, 300) as f32 * dpi).round() as i32;
                    let height = ((lines * 26 + 12).clamp(38, 140) as f32 * dpi).round() as i32;
                    let mut rect = RECT::default();
                    let _ = GetWindowRect(edit.hwnd, &mut rect);
                    let mut p = POINT {
                        x: rect.left,
                        y: rect.top,
                    };
                    let _ = ScreenToClient(hwnd, &mut p);
                    let _ = SetWindowPos(
                        edit.hwnd,
                        None,
                        p.x,
                        p.y,
                        width,
                        height,
                        SWP_NOZORDER | SWP_NOACTIVATE,
                    );
                }
                LRESULT(0)
            }
            WM_CTLCOLOREDIT => {
                let s = &*ptr;
                if let Some(edit) = &s.text_editing {
                    let dc = HDC(wparam.0 as *mut _);
                    SetTextColor(
                        dc,
                        if s.dark {
                            COLORREF(0x00ffffff)
                        } else {
                            COLORREF(0x00181818)
                        },
                    );
                    SetBkColor(
                        dc,
                        if s.dark {
                            COLORREF(0x0025252d)
                        } else {
                            COLORREF(0x00ffffff)
                        },
                    );
                    LRESULT(edit.brush.0 as isize)
                } else {
                    DefWindowProcW(hwnd, message, wparam, lparam)
                }
            }
            WM_NCCALCSIZE => {
                // Both forms begin with the proposed client RECT. Creation uses
                // wParam=0, frame changes use wParam=1. Handle both to avoid a
                // second native title row on the initial window.
                let rect = &mut *(lparam.0 as *mut RECT);
                if IsZoomed(hwnd).as_bool() {
                    let dpi = GetDpiForWindow(hwnd).max(96);
                    let pad = GetSystemMetricsForDpi(SM_CXPADDEDBORDER, dpi);
                    let dx = GetSystemMetricsForDpi(SM_CXSIZEFRAME, dpi) + pad;
                    let dy = GetSystemMetricsForDpi(SM_CYSIZEFRAME, dpi) + pad;
                    rect.left += dx;
                    rect.right -= dx;
                    rect.top += dy;
                    rect.bottom -= dy;
                }
                LRESULT(0)
            }
            WM_NCHITTEST => {
                let mut window = RECT::default();
                if GetWindowRect(hwnd, &mut window).is_err() {
                    return DefWindowProcW(hwnd, message, wparam, lparam);
                }
                let x = lparam.0 as u16 as i16 as i32 - window.left;
                let y = (lparam.0 >> 16) as u16 as i16 as i32 - window.top;
                let dpi = GetDpiForWindow(hwnd).max(96);
                if !IsZoomed(hwnd).as_bool() {
                    let pad = GetSystemMetricsForDpi(SM_CXPADDEDBORDER, dpi);
                    let edge_x = GetSystemMetricsForDpi(SM_CXSIZEFRAME, dpi) + pad;
                    let edge_y = GetSystemMetricsForDpi(SM_CYSIZEFRAME, dpi) + pad;
                    let left = x < edge_x;
                    let right = x >= window.right - window.left - edge_x;
                    let top = y < edge_y;
                    let bottom = y >= window.bottom - window.top - edge_y;
                    let hit = match (left, right, top, bottom) {
                        (true, _, true, _) => Some(HTTOPLEFT),
                        (_, true, true, _) => Some(HTTOPRIGHT),
                        (true, _, _, true) => Some(HTBOTTOMLEFT),
                        (_, true, _, true) => Some(HTBOTTOMRIGHT),
                        (true, _, _, _) => Some(HTLEFT),
                        (_, true, _, _) => Some(HTRIGHT),
                        (_, _, true, _) => Some(HTTOP),
                        (_, _, _, true) => Some(HTBOTTOM),
                        _ => None,
                    };
                    if let Some(hit) = hit {
                        return LRESULT(hit as isize);
                    }
                }
                let s = &*ptr;
                let mut point = POINT {
                    x: lparam.0 as u16 as i16 as i32,
                    y: (lparam.0 >> 16) as u16 as i16 as i32,
                };
                let _ = ScreenToClient(hwnd, &mut point);
                let px = point.x as f32 * 96.0 / dpi as f32;
                let py = point.y as f32 * 96.0 / dpi as f32;
                if (0.0..layout::TOP).contains(&py) && s.layout.hit(px, py).is_none() {
                    LRESULT(HTCAPTION as isize)
                } else {
                    LRESULT(HTCLIENT as isize)
                }
            }
            WM_ERASEBKGND => LRESULT(1),
            WM_ACTIVATE => {
                // Preview cards are topmost. Float only while this editor is
                // active, so a source card cannot cover the large editor footer
                // and the editor won't obscure unrelated apps after focus leaves.
                let active = wparam.0 as u16 != WA_INACTIVE as u16;
                let _ = SetWindowPos(
                    hwnd,
                    Some(if active { HWND_TOPMOST } else { HWND_NOTOPMOST }),
                    0,
                    0,
                    0,
                    0,
                    SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE,
                );
                DefWindowProcW(hwnd, message, wparam, lparam)
            }
            WM_CLOSE => {
                (&*ptr).cancel_drag.set(true);
                (&*ptr).request(hwnd, CLOSE);
                LRESULT(0)
            }
            WM_PAINT => {
                let mut paint = PAINTSTRUCT::default();
                BeginPaint(hwnd, &mut paint);
                let result = {
                    let s = &mut *ptr;
                    if s.renderer.is_none() {
                        let mut r = RECT::default();
                        let _ = GetClientRect(hwnd, &mut r);
                        resize_state(hwnd, s, r.right as u32, r.bottom as u32)
                    } else {
                        Ok(())
                    }
                    .and_then(|()| {
                        // Sample the spring at paint time rather than displaying
                        // a stale position from an earlier low-priority WM_TIMER.
                        s.advance_pill();
                        s.renderer
                            .as_ref()
                            .context("Editor renderer unavailable")?
                            .draw(
                                &s.layout,
                                &s.view,
                                s.preview_geometry
                                    .as_ref()
                                    .or(s.composed.as_ref())
                                    .or(s.image.as_ref()),
                                &s.background,
                                render::ChromeState {
                                    source: s.image.as_ref(),
                                    document: &s.document,
                                    pending: s.pending_mark.as_ref(),
                                    editing_text: s
                                        .text_editing
                                        .as_ref()
                                        .and_then(|edit| edit.index),
                                    crop_mode: s.crop_mode,
                                    crop_drag: s.crop_drag.as_ref(),
                                    active_tool: s.active_tool,
                                    pill_center: s.pill_center,
                                    stroke_width: s.stroke_width,
                                    dark: s.dark,
                                    hovered: s.hover,
                                    hover_levels: &s.hover_levels,
                                    focused: s.focus,
                                    palette_open: s.palette_open,
                                    picker: &s.picker,
                                    stroke_open: s.stroke_open,
                                    selected_color: s.selected_color,
                                    custom_color: s.custom_color,
                                    maximized: IsZoomed(hwnd).as_bool(),
                                },
                            )
                    })
                };
                let _ = EndPaint(hwnd, &paint);
                if let Err(error) = result {
                    let s = &mut *ptr;
                    if error
                        .downcast_ref::<windows::core::Error>()
                        .is_some_and(|e| e.code().0 == 0x8899000c_u32 as i32)
                    {
                        s.renderer = None;
                        let _ = InvalidateRect(Some(hwnd), None, false);
                    } else if s.error.is_none() {
                        s.error = Some(format!("{error:#}"));
                        s.request(hwnd, ERROR);
                    }
                }
                LRESULT(0)
            }
            WM_SIZE => {
                if wparam.0 != SIZE_MINIMIZED as usize {
                    let s = &mut *ptr;
                    s.cancel_gesture();
                    s.finish_text(true);
                    s.finish_color();
                    s.finish_stroke();
                    s.palette_open = false;
                    s.picker.open = false;
                    s.stroke_open = false;
                    if let Err(error) = resize_state(
                        hwnd,
                        s,
                        lparam.0 as u16 as u32,
                        (lparam.0 >> 16) as u16 as u32,
                    ) {
                        s.error = Some(format!("{error:#}"));
                        s.request(hwnd, ERROR);
                    }
                    let _ = InvalidateRect(Some(hwnd), None, false);
                }
                LRESULT(0)
            }
            WM_GETMINMAXINFO => {
                let info = &mut *(lparam.0 as *mut MINMAXINFO);
                let dpi = GetDpiForWindow(hwnd).max(96);
                let s = dpi as f32 / 96.0;
                info.ptMinTrackSize = POINT {
                    x: (layout::MIN_WIDTH * s).ceil() as i32,
                    y: (layout::MIN_HEIGHT * s).ceil() as i32,
                };
                LRESULT(0)
            }
            WM_DPICHANGED => {
                {
                    let s = &mut *ptr;
                    s.cancel_gesture();
                    s.finish_text(true);
                    s.finish_color();
                    s.finish_stroke();
                    s.palette_open = false;
                    s.picker.open = false;
                    s.stroke_open = false;
                    s.dpi = wparam.0 as u16 as u32;
                }
                if GetCapture() == hwnd {
                    let _ = ReleaseCapture();
                }
                let r = &*(lparam.0 as *const RECT);
                let _ = SetWindowPos(
                    hwnd,
                    None,
                    r.left,
                    r.top,
                    r.right - r.left,
                    r.bottom - r.top,
                    SWP_NOZORDER | SWP_NOACTIVATE,
                );
                LRESULT(0)
            }
            WM_KILLFOCUS | WM_CANCELMODE => {
                let s = &mut *ptr;
                s.cancel_gesture();
                if message == WM_CANCELMODE {
                    s.finish_text(false);
                }
                s.finish_color();
                s.finish_stroke();
                s.picker.open = false;
                s.picker.dragging = None;
                s.stroke_open = false;
                s.stroke_dragging = false;
                s.palette_open = false;
                s.focus = None;
                (&*ptr).cancel_drag.set(true);
                if GetCapture() == hwnd {
                    let _ = ReleaseCapture();
                }
                let _ = InvalidateRect(Some(hwnd), None, false);
                LRESULT(0)
            }
            WM_CAPTURECHANGED => {
                (&mut *ptr).finish_color();
                (&mut *ptr).finish_stroke();
                (&mut *ptr).picker.dragging = None;
                (&mut *ptr).stroke_dragging = false;
                (&mut *ptr).cancel_gesture();
                let _ = InvalidateRect(Some(hwnd), None, false);
                LRESULT(0)
            }
            WM_KEYDOWN => {
                let s = &mut *ptr;
                let ctrl = GetKeyState(VK_CONTROL.0 as i32) < 0;
                let shift = GetKeyState(VK_SHIFT.0 as i32) < 0;
                if s.picker.open {
                    match wparam.0 as u16 {
                        0x1b => {
                            if s.picker.editing_hex {
                                s.picker.hex = format!("{:06X}", s.picker.color());
                                s.picker.editing_hex = false;
                            } else {
                                s.picker.open = false;
                                s.finish_color();
                            }
                        }
                        0x0d => {
                            if s.picker.editing_hex {
                                if let Some(rgb) = s.picker.commit_hex() {
                                    s.live_color(rgb);
                                    s.finish_color();
                                }
                            } else {
                                s.picker.open = false;
                                s.finish_color();
                            }
                        }
                        0x41 if ctrl && s.picker.editing_hex => s.picker.hex.clear(),
                        _ => {}
                    }
                    let _ = InvalidateRect(Some(hwnd), None, false);
                    return LRESULT(0);
                }
                if s.stroke_open {
                    match wparam.0 as u16 {
                        0x1b | 0x0d => {
                            s.stroke_open = false;
                            s.finish_stroke();
                        }
                        0x25 | 0x27 => {
                            let amount = if shift { 0.25 } else { 1.0 };
                            let next = s.stroke_width
                                + if wparam.0 as u16 == 0x27 {
                                    amount
                                } else {
                                    -amount
                                };
                            s.live_stroke(next);
                            s.finish_stroke();
                        }
                        _ => {}
                    }
                    let _ = InvalidateRect(Some(hwnd), None, false);
                    return LRESULT(0);
                }
                if s.palette_open {
                    match wparam.0 as u16 {
                        0x1b => s.palette_open = false,
                        0x0d => {
                            s.palette_open = false;
                            if s.selected_color == layout::PRESET_COLORS.len() {
                                s.picker.open(s.drawing_color());
                            } else if let Some(index) = s.document.selected {
                                s.document.recolor(index, s.drawing_color());
                            }
                        }
                        0x26 => {
                            s.selected_color = (s.selected_color + layout::PRESET_COLORS.len())
                                % (layout::PRESET_COLORS.len() + 1)
                        }
                        0x28 => {
                            s.selected_color =
                                (s.selected_color + 1) % (layout::PRESET_COLORS.len() + 1)
                        }
                        _ => {}
                    }
                    let _ = InvalidateRect(Some(hwnd), None, false);
                    return LRESULT(0);
                }
                match wparam.0 as u16 {
                    0x1b => {
                        s.cancel_gesture();
                        s.crop_mode = false;
                        s.crop_drag = None;
                        if GetCapture() == hwnd {
                            let _ = ReleaseCapture();
                        }
                    }
                    0x09 => {
                        let list: Vec<_> = s
                            .layout
                            .controls
                            .iter()
                            .map(|(c, _)| *c)
                            .filter(|c| c.enabled() && s.ready())
                            .collect();
                        if !list.is_empty() {
                            let current = s.focus.and_then(|c| list.iter().position(|x| *x == c));
                            let i = match current {
                                Some(i) => {
                                    if shift {
                                        (i + list.len() - 1) % list.len()
                                    } else {
                                        (i + 1) % list.len()
                                    }
                                }
                                None => {
                                    if shift {
                                        list.len() - 1
                                    } else {
                                        0
                                    }
                                }
                            };
                            s.focus = Some(list[i]);
                        }
                    }
                    0x0d if s.crop_mode && s.focus.is_none() => apply_crop(s, hwnd),
                    0x0d | 0x20 => {
                        if let Some(c) = s.focus {
                            invoke(s, hwnd, c);
                        }
                    }
                    0x5a if ctrl && shift => {
                        s.document.redo();
                    }
                    0x5a if ctrl => {
                        s.document.undo();
                    }
                    0x59 if ctrl => {
                        s.document.redo();
                    }
                    0x71 if s.document.selected.is_some() => {
                        let index = s.document.selected.unwrap();
                        if let document::Shape::Text(point, _) =
                            s.document.marks[index].shape.clone()
                            && let Err(error) = start_text(hwnd, s, point, Some(index))
                        {
                            s.error = Some(format!("{error:#}"));
                            s.request(hwnd, ERROR);
                        }
                    }
                    0x2e | 0x08 if s.document.selected.is_some() => {
                        s.document.delete_selected();
                    }
                    0x53 if ctrl && s.ready() => s.request(hwnd, SAVE),
                    0x43 if ctrl && shift && s.ready() => s.request(hwnd, COPY),
                    0x57 if ctrl => s.request(hwnd, CLOSE),
                    0x30 if ctrl => {
                        s.view = View {
                            fit_limit: s.view.fit_limit,
                            ..View::default()
                        }
                    }
                    0x31 if ctrl => {
                        s.view = View {
                            zoom: Zoom::Scale(1.0),
                            pan: (0.0, 0.0),
                            fit_limit: s.view.fit_limit,
                        }
                    }
                    0xbb | 0xbd if ctrl => {
                        if let Some((w, h)) = s.view_size() {
                            let scale = s.view.scale(s.layout.canvas, w, h)
                                * if wparam.0 == 0xbb { 1.25 } else { 0.8 };
                            let c = s.layout.canvas;
                            s.view.zoom_at(scale, (c.w / 2.0, c.y + c.h / 2.0), c, w, h);
                        }
                    }
                    _ => {}
                }
                let _ = InvalidateRect(Some(hwnd), None, false);
                LRESULT(0)
            }
            WM_CHAR => {
                let s = &mut *ptr;
                if s.picker.open && s.picker.editing_hex {
                    let ch = char::from_u32(wparam.0 as u32);
                    match ch {
                        Some('\u{8}') => {
                            s.picker.hex.pop();
                        }
                        Some(c) if c.is_ascii_hexdigit() && s.picker.hex.len() < 6 => {
                            s.picker.hex.push(c.to_ascii_uppercase())
                        }
                        _ => {}
                    }
                    let _ = InvalidateRect(Some(hwnd), None, false);
                    return LRESULT(0);
                }
                DefWindowProcW(hwnd, message, wparam, lparam)
            }
            WM_LBUTTONDOWN => {
                let (capture, move_window) = {
                    let s = &mut *ptr;
                    let p = s.point(lparam);
                    if s.text_editing.is_some() {
                        s.finish_text(true);
                    }
                    if s.picker.open {
                        match color_picker::hit(&s.layout, p.0, p.1) {
                            Some(
                                region @ (color_picker::Region::Spectrum
                                | color_picker::Region::Hue),
                            ) => {
                                s.picker.dragging = Some(region);
                                if let Some(rgb) = s.picker.update(region, p.0, p.1, &s.layout) {
                                    s.live_color(rgb);
                                }
                                let _ = SetFocus(Some(hwnd));
                                SetCapture(hwnd);
                            }
                            Some(color_picker::Region::Preset(index))
                                if index < layout::PRESET_COLORS.len() =>
                            {
                                s.finish_color();
                                s.selected_color = index;
                                let rgb = s.drawing_color();
                                s.picker.open(rgb);
                                if let Some(selected) = s.document.selected {
                                    s.document.recolor(selected, rgb);
                                }
                            }
                            Some(color_picker::Region::Hex) => {
                                s.picker.editing_hex = true;
                                s.picker.hex.clear();
                                let _ = SetFocus(Some(hwnd));
                            }
                            Some(color_picker::Region::Done) | None => {
                                s.finish_color();
                                s.picker.open = false;
                                s.picker.dragging = None;
                            }
                            _ => {}
                        }
                        let _ = InvalidateRect(Some(hwnd), None, false);
                        return LRESULT(0);
                    }
                    if s.stroke_open {
                        if let Some(r) = s.layout.stroke_rect()
                            && r.contains(p.0, p.1)
                        {
                            if p.1 >= r.y + 103.0 && p.1 <= r.y + 143.0 {
                                s.stroke_dragging = true;
                                if let Some(width) = s.layout.stroke_at(p.0, s.is_text_property()) {
                                    s.live_stroke(width);
                                }
                                let _ = SetFocus(Some(hwnd));
                                SetCapture(hwnd);
                            }
                        } else {
                            s.finish_stroke();
                            s.stroke_open = false;
                        }
                        let _ = InvalidateRect(Some(hwnd), None, false);
                        return LRESULT(0);
                    }
                    if s.palette_open {
                        SetCapture(hwnd);
                        return LRESULT(0);
                    }
                    if s.background.open
                        && p.0 < background::PANEL_WIDTH
                        && p.1 >= layout::TOP
                        && p.1 < s.layout.height - layout::BOTTOM
                    {
                        s.background_pressed = s.background.hit(p.0, p.1);
                        if let Some(background::Target::Slider(slider)) = s.background_pressed {
                            s.background.dragging = Some(slider);
                            if s.background.set_slider(slider, p.0)
                                && let Err(error) = refresh_background_drag(hwnd, s)
                            {
                                s.error = Some(format!("{error:#}"));
                                s.request(hwnd, ERROR);
                            }
                        }
                        let _ = SetFocus(Some(hwnd));
                        SetCapture(hwnd);
                        return LRESULT(0);
                    }
                    if s.crop_mode {
                        let (cancel, apply) = layout::crop_actions(s.layout.canvas);
                        if cancel.contains(p.0, p.1) {
                            s.crop_mode = false;
                            s.crop_drag = None;
                            s.crop_grab = None;
                            let _ = InvalidateRect(Some(hwnd), None, false);
                            return LRESULT(0);
                        }
                        if apply.contains(p.0, p.1) {
                            apply_crop(s, hwnd);
                            return LRESULT(0);
                        }
                        if s.layout.canvas.contains(p.0, p.1)
                            && let Some((point, scale)) = source_point(s, p)
                            && let Some(mark) = &s.crop_drag
                            && let Some(edge) = crop_hit(mark, point, 9.0 / scale)
                        {
                            let (lo, hi) = mark.bounds();
                            s.crop_grab = Some(CropGrab {
                                edge,
                                start: point,
                                lo,
                                hi,
                                // Keep the crop visibly sizable at Fit and at small zoom levels.
                                min_size: (32.0 / scale).ceil().max(32.0),
                            });
                            let _ = SetFocus(Some(hwnd));
                            SetCapture(hwnd);
                        }
                        return LRESULT(0);
                    }
                    if s.layout.canvas.contains(p.0, p.1)
                        && let Some((point, scale)) = source_point(s, p)
                        && let Some(index) = s.document.selected
                        && let Some(mark) = s.document.marks.get(index)
                    {
                        let rotation = mark.rotation_handle(scale).is_some_and(|target| {
                            (point.x - target.x).hypot(point.y - target.y) <= 16.0 / scale
                        });
                        let handle = mark.handles().iter().position(|target| {
                            (point.x - target.x).hypot(point.y - target.y) <= 8.0 / scale
                        });
                        let local = document::rotate(point, mark.center(), -mark.angle);
                        let (lo, hi) = mark.bounds();
                        let within = local.x >= lo.x - 3.0 / scale
                            && local.x <= hi.x + 3.0 / scale
                            && local.y >= lo.y - 3.0 / scale
                            && local.y <= hi.y + 3.0 / scale;
                        if rotation || handle.is_some() || within {
                            s.moving_mark = Some(MarkGesture {
                                index,
                                original: mark.clone(),
                                start: point,
                                handle,
                                rotating: rotation,
                            });
                            let _ = SetFocus(Some(hwnd));
                            SetCapture(hwnd);
                            let _ = InvalidateRect(Some(hwnd), None, false);
                            return LRESULT(0);
                        }
                    }
                    if s.layout.canvas.contains(p.0, p.1)
                        && let Some((point, scale)) = source_point(s, p)
                        && inside_source(s, point)
                    {
                        if s.active_tool == Control::Move {
                            let handle = s.document.selected.and_then(|index| {
                                s.document.marks[index].handles().iter().position(|target| {
                                    (target.x - point.x).hypot(target.y - point.y) <= 7.0 / scale
                                })
                            });
                            s.document.selected = if handle.is_some() {
                                s.document.selected
                            } else {
                                s.document.hit(point, 6.0 / scale)
                            };
                            if let Some(index) = s.document.selected {
                                s.stroke_width = if matches!(
                                    s.document.marks[index].shape,
                                    document::Shape::Text(..)
                                ) {
                                    s.document.marks[index].width
                                } else {
                                    s.drawing_stroke_width
                                };
                                s.moving_mark = Some(MarkGesture {
                                    index,
                                    original: s.document.marks[index].clone(),
                                    start: point,
                                    handle,
                                    rotating: false,
                                });
                                SetCapture(hwnd);
                            }
                            let _ = SetFocus(Some(hwnd));
                            let _ = InvalidateRect(Some(hwnd), None, false);
                            return LRESULT(0);
                        }
                        if s.active_tool == Control::Text {
                            if let Err(error) = start_text(hwnd, s, point, None) {
                                s.error = Some(format!("{error:#}"));
                                s.request(hwnd, ERROR);
                            }
                            return LRESULT(0);
                        }
                        if let Some(mut mark) =
                            Mark::from_tool(s.active_tool, point, s.drawing_color(), s.stroke_width)
                        {
                            if let document::Shape::Counter(_, ref mut number) = mark.shape {
                                *number = s.next_counter;
                            }
                            s.pending_mark = Some(mark);
                            s.document.selected = None;
                            let _ = SetFocus(Some(hwnd));
                            SetCapture(hwnd);
                            return LRESULT(0);
                        }
                    }
                    s.focus = None;
                    s.pressed = s.layout.hit(p.0, p.1).filter(|c| {
                        c.enabled()
                            && (s.ready()
                                || matches!(
                                    c,
                                    Control::Minimize | Control::Maximize | Control::Close
                                ))
                    });
                    if s.pressed == Some(Control::Drag) {
                        s.drag_start = Some(p);
                    }
                    (
                        s.pressed.is_some(),
                        p.1 >= s.layout.height - layout::BOTTOM && s.layout.hit(p.0, p.1).is_none(),
                    )
                };
                let _ = SetFocus(Some(hwnd));
                if capture {
                    SetCapture(hwnd);
                }
                if move_window {
                    let _ = SendMessageW(
                        hwnd,
                        WM_NCLBUTTONDOWN,
                        Some(WPARAM(HTCAPTION as usize)),
                        Some(LPARAM(0)),
                    );
                }
                LRESULT(0)
            }
            WM_MBUTTONDOWN => {
                let s = &mut *ptr;
                let p = s.point(lparam);
                if !s.palette_open
                    && !s.picker.open
                    && !s.stroke_open
                    && s.pressed.is_none()
                    && s.drag_start.is_none()
                    && s.pending_mark.is_none()
                    && s.moving_mark.is_none()
                    && s.layout.canvas.contains(p.0, p.1)
                    && s.ready()
                {
                    s.pan_start = Some((p, s.view.pan));
                    let _ = SetFocus(Some(hwnd));
                    SetCapture(hwnd);
                }
                LRESULT(0)
            }
            WM_MBUTTONUP => {
                let s = &mut *ptr;
                if s.pan_start.take().is_some()
                    && GetCapture() == hwnd
                    && s.pressed.is_none()
                    && s.drag_start.is_none()
                {
                    let _ = ReleaseCapture();
                }
                LRESULT(0)
            }
            WM_TIMER if wparam.0 == HOVER_TIMER => {
                let s = &mut *ptr;
                let hovering = s.advance_hover();
                let sliding = s.advance_pill();
                if !hovering && !sliding {
                    let _ = KillTimer(Some(hwnd), HOVER_TIMER);
                }
                let _ = InvalidateRect(Some(hwnd), None, false);
                LRESULT(0)
            }
            WM_MOUSEMOVE => {
                {
                    let s = &mut *ptr;
                    let p = s.point(lparam);
                    if let Some(grab) = s.crop_grab {
                        let point = crop_point(s, p);
                        if let (Some(mark), Some(point), Some(source)) =
                            (&mut s.crop_drag, point, &s.image)
                        {
                            let (lo, hi) =
                                crop_adjust(grab, point, source.width as f32, source.height as f32);
                            mark.shape = document::Shape::Crop(lo, hi);
                        }
                        let _ = InvalidateRect(Some(hwnd), None, false);
                        return LRESULT(0);
                    }
                    if let Some(region) = s.picker.dragging {
                        if let Some(rgb) = s.picker.update(region, p.0, p.1, &s.layout) {
                            s.live_color(rgb);
                        }
                        let _ = InvalidateRect(Some(hwnd), None, false);
                        return LRESULT(0);
                    }
                    if s.stroke_dragging {
                        if let Some(width) = s.layout.stroke_at(p.0, s.is_text_property()) {
                            s.live_stroke(width);
                        }
                        let _ = InvalidateRect(Some(hwnd), None, false);
                        return LRESULT(0);
                    }
                    let hit = if s.palette_open || s.picker.open || s.stroke_open {
                        None
                    } else {
                        s.layout.hit(p.0, p.1)
                    };
                    s.set_hover(hwnd, hit);
                    if s.pill_motion.is_some() {
                        s.advance_pill();
                        let _ = InvalidateRect(Some(hwnd), None, false);
                    }
                    if let Some(slider) = s.background.dragging
                        && s.background.set_slider(slider, p.0)
                        && let Err(error) = refresh_background_drag(hwnd, s)
                    {
                        s.error = Some(format!("{error:#}"));
                        s.request(hwnd, ERROR);
                    }
                    if let Some((point, _)) = source_point(s, p) {
                        if let Some(mark) = &mut s.pending_mark {
                            mark.update(redaction_point(mark, point, s.image.as_ref()));
                            let _ = InvalidateRect(Some(hwnd), None, false);
                        }
                        if let Some(gesture) = &s.moving_mark {
                            let point = redaction_point(&gesture.original, point, s.image.as_ref());
                            s.document.marks[gesture.index] = gesture.at(point);
                            let _ = InvalidateRect(Some(hwnd), None, false);
                        }
                    }
                    if let Some((start, pan)) = s.pan_start {
                        s.view.pan = (pan.0 + p.0 - start.0, pan.1 + p.1 - start.1);
                        if let Some((w, h)) = s.view_size() {
                            s.view.clamp_pan(s.layout.canvas, w, h);
                        }
                        let _ = InvalidateRect(Some(hwnd), None, false);
                    }
                    if let Some(start) = s.drag_start {
                        let dx =
                            GetSystemMetricsForDpi(SM_CXDRAG, s.dpi) as f32 * 96.0 / s.dpi as f32;
                        let dy =
                            GetSystemMetricsForDpi(SM_CYDRAG, s.dpi) as f32 * 96.0 / s.dpi as f32;
                        if (p.0 - start.0).abs() >= dx || (p.1 - start.1).abs() >= dy {
                            s.drag_start = None;
                            s.request(hwnd, DRAG);
                        }
                    }
                }
                let mut track = TRACKMOUSEEVENT {
                    cbSize: std::mem::size_of::<TRACKMOUSEEVENT>() as u32,
                    dwFlags: TME_LEAVE,
                    hwndTrack: hwnd,
                    dwHoverTime: 0,
                };
                let _ = TrackMouseEvent(&mut track);
                LRESULT(0)
            }
            WM_MOUSELEAVE => {
                (&mut *ptr).set_hover(hwnd, None);
                LRESULT(0)
            }
            WM_LBUTTONUP => {
                {
                    let s = &mut *ptr;
                    let p = s.point(lparam);
                    s.drag_start = None;
                    if s.crop_grab.take().is_some() {
                        if GetCapture() == hwnd {
                            let _ = ReleaseCapture();
                        }
                        let _ = InvalidateRect(Some(hwnd), None, false);
                        return LRESULT(0);
                    }
                    if let Some(region) = s.picker.dragging.take() {
                        if let Some(rgb) = s.picker.update(region, p.0, p.1, &s.layout) {
                            s.live_color(rgb);
                        }
                        s.finish_color();
                        if GetCapture() == hwnd {
                            let _ = ReleaseCapture();
                        }
                        let _ = InvalidateRect(Some(hwnd), None, false);
                        return LRESULT(0);
                    }
                    if s.stroke_dragging {
                        s.stroke_dragging = false;
                        if let Some(width) = s.layout.stroke_at(p.0, s.is_text_property()) {
                            s.live_stroke(width);
                        }
                        s.finish_stroke();
                        if GetCapture() == hwnd {
                            let _ = ReleaseCapture();
                        }
                        let _ = InvalidateRect(Some(hwnd), None, false);
                        return LRESULT(0);
                    }
                    if let Some(mut mark) = s.pending_mark.take() {
                        if let Some((point, _)) = source_point(s, p) {
                            mark.update(redaction_point(&mark, point, s.image.as_ref()));
                        }
                        if mark.finish() {
                            let counter = matches!(mark.shape, document::Shape::Counter(..));
                            if let Err(error) = s.document.add(mark) {
                                s.error = Some(format!("{error:#}"));
                                s.request(hwnd, ERROR);
                            } else if counter {
                                s.next_counter = s.next_counter.saturating_add(1);
                            }
                        }
                        if GetCapture() == hwnd {
                            let _ = ReleaseCapture();
                        }
                        let _ = InvalidateRect(Some(hwnd), None, false);
                        return LRESULT(0);
                    }
                    if let Some(gesture) = s.moving_mark.take() {
                        if let Some((point, _)) = source_point(s, p) {
                            let point = redaction_point(&gesture.original, point, s.image.as_ref());
                            s.document.marks[gesture.index] = gesture.at(point);
                        }
                        s.document.edit(gesture.index, gesture.original);
                        if GetCapture() == hwnd {
                            let _ = ReleaseCapture();
                        }
                        let _ = InvalidateRect(Some(hwnd), None, false);
                        return LRESULT(0);
                    }
                    if let Some(pressed) = s.background_pressed.take() {
                        s.background.dragging = None;
                        if s.background.hit(p.0, p.1) == Some(pressed)
                            || matches!(pressed, background::Target::Slider(_))
                        {
                            match pressed {
                                background::Target::None => {
                                    s.background.style = background::Style::None
                                }
                                background::Target::Gradient(i) => {
                                    s.background.style = background::Style::Gradient(i)
                                }
                                background::Target::Wallpaper(i) => {
                                    s.background.style = background::Style::Wallpaper(i)
                                }
                                background::Target::Solid(i) => {
                                    s.background.style = background::Style::Solid(i)
                                }
                                background::Target::Blurred(i) => {
                                    s.background.style = background::Style::Blurred(i)
                                }
                                background::Target::AutoBalance => {
                                    s.background.auto_balance = !s.background.auto_balance
                                }
                                background::Target::Reset => s.background.reset_adjustments(),
                                background::Target::ToggleGradients => {
                                    s.background.expanded = !s.background.expanded;
                                    s.background.scroll_y = s
                                        .background
                                        .scroll_y
                                        .min(s.background.max_scroll(s.layout.height));
                                }
                                background::Target::Slider(_) => {}
                            }
                            if let Err(error) = refresh_background(hwnd, s) {
                                s.error = Some(format!("{error:#}"));
                                s.request(hwnd, ERROR);
                            }
                        }
                        if GetCapture() == hwnd {
                            let _ = ReleaseCapture();
                        }
                        return LRESULT(0);
                    }
                    if s.palette_open {
                        if let Some(index) = s.layout.palette_hit(p.0, p.1) {
                            if index < layout::PRESET_COLORS.len() {
                                s.selected_color = index;
                                if let Some(selected) = s.document.selected {
                                    s.document.recolor(selected, s.drawing_color());
                                }
                                s.palette_open = false;
                            } else {
                                s.palette_open = false;
                                s.picker.open(s.drawing_color());
                            }
                        } else {
                            s.palette_open = false;
                        }
                        let _ = InvalidateRect(Some(hwnd), None, false);
                        if GetCapture() == hwnd && s.pan_start.is_none() {
                            let _ = ReleaseCapture();
                        }
                        return LRESULT(0);
                    }
                    if let Some(c) = s.pressed.take()
                        && s.layout.hit(p.0, p.1) == Some(c)
                        && c != Control::Drag
                    {
                        invoke(s, hwnd, c);
                    }
                }
                if GetCapture() == hwnd && (&*ptr).pan_start.is_none() {
                    let _ = ReleaseCapture();
                }
                LRESULT(0)
            }
            WM_MOUSEWHEEL => {
                let s = &mut *ptr;
                let mut point = POINT {
                    x: lparam.0 as u16 as i16 as i32,
                    y: (lparam.0 >> 16) as u16 as i16 as i32,
                };
                let _ = ScreenToClient(hwnd, &mut point);
                let p = (
                    point.x as f32 * 96.0 / s.dpi as f32,
                    point.y as f32 * 96.0 / s.dpi as f32,
                );
                if (s.picker.open
                    && color_picker::rect(&s.layout).is_some_and(|r| r.contains(p.0, p.1)))
                    || (s.stroke_open
                        && s.layout.stroke_rect().is_some_and(|r| r.contains(p.0, p.1)))
                {
                    return LRESULT(0);
                }
                if s.background.open
                    && p.0 < background::PANEL_WIDTH
                    && p.1 >= background::PANEL_TOP
                    && p.1 < s.layout.height - background::PANEL_BOTTOM
                {
                    let delta = (wparam.0 >> 16) as u16 as i16 as f32 / 120.0;
                    s.background.scroll_y = (s.background.scroll_y - delta * 64.0)
                        .clamp(0.0, s.background.max_scroll(s.layout.height));
                    let _ = InvalidateRect(Some(hwnd), None, false);
                    return LRESULT(0);
                }
                if s.layout.canvas.contains(p.0, p.1)
                    && let Some((w, h)) = s.view_size()
                {
                    let delta = (wparam.0 >> 16) as u16 as i16 as f32 / 120.0;
                    if GetKeyState(VK_CONTROL.0 as i32) < 0 {
                        let scale = s.view.scale(s.layout.canvas, w, h) * 1.25_f32.powf(delta);
                        s.view.zoom_at(scale, p, s.layout.canvas, w, h);
                    } else {
                        s.view.pan.1 += delta * 48.0;
                        s.view.clamp_pan(s.layout.canvas, w, h);
                    }
                    let _ = InvalidateRect(Some(hwnd), None, false);
                }
                LRESULT(0)
            }
            WM_SETCURSOR if lparam.0 as u16 as u32 == HTCLIENT => {
                let s = &*ptr;
                let mut point = POINT::default();
                let _ = GetCursorPos(&mut point);
                let _ = ScreenToClient(hwnd, &mut point);
                let on_background = s.background.open
                    && s.background
                        .hit(
                            point.x as f32 * 96.0 / s.dpi as f32,
                            point.y as f32 * 96.0 / s.dpi as f32,
                        )
                        .is_some();
                let px = point.x as f32 * 96.0 / s.dpi as f32;
                let py = point.y as f32 * 96.0 / s.dpi as f32;
                let edit_cursor = s.document.selected.and_then(|index| {
                    let mark = s.document.marks.get(index)?;
                    let (p, scale) = source_point(s, (px, py))?;
                    if mark
                        .rotation_handle(scale)
                        .is_some_and(|h| (p.x - h.x).hypot(p.y - h.y) <= 16.0 / scale)
                    {
                        return Some(IDC_CROSS);
                    }
                    if let Some(handle) = mark
                        .handles()
                        .iter()
                        .find(|h| (p.x - h.x).hypot(p.y - h.y) <= 8.0 / scale)
                    {
                        let c = mark.center();
                        let (dx, dy) = (handle.x - c.x, handle.y - c.y);
                        return Some(if dx.abs() < 4.0 / scale {
                            IDC_SIZENS
                        } else if dy.abs() < 4.0 / scale {
                            IDC_SIZEWE
                        } else if dx * dy >= 0.0 {
                            IDC_SIZENWSE
                        } else {
                            IDC_SIZENESW
                        });
                    }
                    let p = document::rotate(p, mark.center(), -mark.angle);
                    let (lo, hi) = mark.bounds();
                    (p.x >= lo.x && p.x <= hi.x && p.y >= lo.y && p.y <= hi.y)
                        .then_some(IDC_SIZEALL)
                });
                let cursor = if s.picker.open
                    && color_picker::hit(&s.layout, px, py) == Some(color_picker::Region::Hex)
                {
                    IDC_IBEAM
                } else if s.palette_open
                    || s.picker.open
                    || s.stroke_open
                    || s.background.dragging.is_some()
                    || on_background
                {
                    IDC_HAND
                } else if s.pan_start.is_some() {
                    IDC_SIZEALL
                } else if s.crop_mode && {
                    let (cancel, apply) = layout::crop_actions(s.layout.canvas);
                    cancel.contains(px, py) || apply.contains(px, py)
                } {
                    IDC_HAND
                } else if s.crop_mode {
                    let edge = source_point(s, (px, py)).and_then(|(p, scale)| {
                        s.crop_drag
                            .as_ref()
                            .and_then(|m| crop_hit(m, p, 9.0 / scale))
                    });
                    match edge {
                        Some((-1, -1) | (1, 1)) => IDC_SIZENWSE,
                        Some((1, -1) | (-1, 1)) => IDC_SIZENESW,
                        Some((0, -1) | (0, 1)) => IDC_SIZENS,
                        Some((-1, 0) | (1, 0)) => IDC_SIZEWE,
                        Some((0, 0)) => IDC_SIZEALL,
                        _ => IDC_ARROW,
                    }
                } else if let Some(edit_cursor) = edit_cursor {
                    edit_cursor
                } else if s.hover.is_some_and(|c| {
                    c.enabled()
                        && (s.ready()
                            || matches!(c, Control::Minimize | Control::Maximize | Control::Close))
                }) {
                    IDC_HAND
                } else {
                    IDC_ARROW
                };
                if let Ok(cursor) = LoadCursorW(None, cursor) {
                    SetCursor(Some(cursor));
                }
                LRESULT(1)
            }
            _ => DefWindowProcW(hwnd, message, wparam, lparam),
        }
    }
}
fn invoke(state: &mut WindowState, hwnd: HWND, control: Control) {
    if state.crop_mode
        && (control.is_drawing_tool() || matches!(control, Control::AddImage | Control::Background))
    {
        state.crop_mode = false;
        state.crop_drag = None;
        state.crop_grab = None;
    }
    match control {
        Control::Save => state.request(hwnd, SAVE),
        Control::Copy => state.request(hwnd, COPY),
        Control::Zoom => state.request(hwnd, ZOOM_MENU),
        Control::Crop => {
            state.finish_text(true);
            if state.crop_mode {
                state.crop_mode = false;
                state.crop_drag = None;
                state.crop_grab = None;
            } else if let Some(source) = &state.image {
                let (lo, hi) = state.document.crop().unwrap_or((
                    Point::default(),
                    Point {
                        x: source.width as f32,
                        y: source.height as f32,
                    },
                ));
                state.crop_drag = Some(Mark {
                    shape: document::Shape::Crop(lo, hi),
                    color: 0,
                    width: 1.0,
                    opacity: 1.0,
                    angle: 0.0,
                });
                state.crop_mode = true;
                state.view.zoom = Zoom::Fit;
                state.view.pan = (0.0, 0.0);
                state.palette_open = false;
                state.picker.open = false;
                state.stroke_open = false;
            }
        }
        Control::Color => {
            state.finish_stroke();
            state.stroke_open = false;
            state.palette_open = !state.palette_open;
        }
        Control::Stroke => {
            state.finish_color();
            state.picker.open = false;
            state.palette_open = false;
            state.stroke_open = !state.stroke_open;
        }
        Control::Move
        | Control::Rectangle
        | Control::Fill
        | Control::Ellipse
        | Control::Line
        | Control::Arrow
        | Control::Pencil
        | Control::Pixelate
        | Control::Spotlight
        | Control::Text
        | Control::Counter => {
            if state.active_tool == Control::Text {
                state.text_size = state.stroke_width;
            } else if !state.is_text_property() {
                state.drawing_stroke_width = state.stroke_width;
            }
            state.stroke_width = if control == Control::Text {
                state.text_size
            } else {
                state.drawing_stroke_width
            };
            state.select_tool(hwnd, control);
            if matches!(control, Control::Counter | Control::Text) {
                state.selected_color = 8;
            }
            if control != Control::Move {
                state.document.selected = None;
            }
            state.palette_open = false;
            state.finish_color();
            state.finish_stroke();
            state.picker.open = false;
            state.stroke_open = false;
        }
        Control::Background => state.request(hwnd, BACKGROUND_TOGGLE),
        Control::AddImage => state.request(hwnd, ADD_IMAGE),
        Control::Minimize => unsafe {
            let _ = PostMessageW(
                Some(hwnd),
                WM_SYSCOMMAND,
                WPARAM(SC_MINIMIZE as usize),
                LPARAM(0),
            );
        },
        Control::Maximize => unsafe {
            let command = if IsZoomed(hwnd).as_bool() {
                SC_RESTORE
            } else {
                SC_MAXIMIZE
            };
            let _ = PostMessageW(
                Some(hwnd),
                WM_SYSCOMMAND,
                WPARAM(command as usize),
                LPARAM(0),
            );
        },
        Control::Close => state.request(hwnd, CLOSE),
        _ => {}
    }
}

#[cfg(test)]
mod crop_tests {
    use super::*;
    #[test]
    fn crop_handles_keep_a_visible_minimum_at_fit_and_zoom_out() {
        let grab = CropGrab {
            edge: (1, 1),
            start: Point { x: 200.0, y: 100.0 },
            lo: Point { x: 20.0, y: 20.0 },
            hi: Point { x: 200.0, y: 100.0 },
            min_size: 32.0,
        };
        let (lo, hi) = crop_adjust(grab, Point { x: 20.0, y: 20.0 }, 350.0, 200.0);
        assert_eq!((hi.x - lo.x, hi.y - lo.y), (32.0, 32.0));
        let zoomed = CropGrab {
            min_size: 160.0,
            ..grab
        };
        let (lo, hi) = crop_adjust(zoomed, Point { x: 20.0, y: 20.0 }, 350.0, 200.0);
        assert_eq!((hi.x - lo.x, hi.y - lo.y), (160.0, 160.0));
        let small = CropGrab {
            lo: Point::default(),
            hi: Point { x: 12.0, y: 8.0 },
            min_size: 32.0,
            start: Point { x: 12.0, y: 8.0 },
            ..grab
        };
        let (lo, hi) = crop_adjust(small, Point::default(), 12.0, 8.0);
        assert_eq!((hi.x - lo.x, hi.y - lo.y), (12.0, 8.0));
    }
}
