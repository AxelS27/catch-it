//! Native input/callback handling. Keep modal work outside borrowed WindowState.
use super::*;

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
            // Let the native nonclient initialization retain the window title.
            return DefWindowProcW(hwnd, message, wparam, lparam);
        }
        let ptr = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut WindowState;
        if ptr.is_null() {
            return DefWindowProcW(hwnd, message, wparam, lparam);
        }
        match message {
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
                        s.renderer
                            .as_ref()
                            .context("Editor renderer unavailable")?
                            .draw(
                                &s.layout,
                                &s.view,
                                s.image.as_ref(),
                                render::ChromeState {
                                    dark: s.dark,
                                    hovered: s.hover,
                                    focused: s.focus,
                                    palette_open: s.palette_open,
                                    selected_color: s.selected_color,
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
                    s.palette_open = false;
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
                let mut bounds = RECT {
                    left: 0,
                    top: 0,
                    right: (layout::MIN_WIDTH * s) as i32,
                    bottom: (layout::MIN_HEIGHT * s) as i32,
                };
                let _ = AdjustWindowRectExForDpi(
                    &mut bounds,
                    WS_OVERLAPPEDWINDOW,
                    false,
                    WINDOW_EX_STYLE::default(),
                    dpi,
                );
                info.ptMinTrackSize = POINT {
                    x: bounds.right - bounds.left,
                    y: bounds.bottom - bounds.top,
                };
                LRESULT(0)
            }
            WM_DPICHANGED => {
                {
                    let s = &mut *ptr;
                    s.cancel_gesture();
                    s.palette_open = false;
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
                (&mut *ptr).cancel_gesture();
                (&mut *ptr).palette_open = false;
                (&mut *ptr).focus = None;
                (&*ptr).cancel_drag.set(true);
                if GetCapture() == hwnd {
                    let _ = ReleaseCapture();
                }
                let _ = InvalidateRect(Some(hwnd), None, false);
                LRESULT(0)
            }
            WM_CAPTURECHANGED => {
                (&mut *ptr).cancel_gesture();
                let _ = InvalidateRect(Some(hwnd), None, false);
                LRESULT(0)
            }
            WM_KEYDOWN => {
                let s = &mut *ptr;
                let ctrl = GetKeyState(VK_CONTROL.0 as i32) < 0;
                let shift = GetKeyState(VK_SHIFT.0 as i32) < 0;
                if s.palette_open {
                    match wparam.0 as u16 {
                        0x1b | 0x0d => s.palette_open = false,
                        0x26 => {
                            s.selected_color = (s.selected_color + layout::PRESET_COLORS.len() - 1)
                                % layout::PRESET_COLORS.len()
                        }
                        0x28 => {
                            s.selected_color = (s.selected_color + 1) % layout::PRESET_COLORS.len()
                        }
                        _ => {}
                    }
                    let _ = InvalidateRect(Some(hwnd), None, false);
                    return LRESULT(0);
                }
                match wparam.0 as u16 {
                    0x1b => {
                        s.cancel_gesture();
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
                    0x0d | 0x20 => {
                        if let Some(c) = s.focus {
                            invoke(s, hwnd, c);
                        }
                    }
                    0x53 if ctrl && s.ready() => s.request(hwnd, SAVE),
                    0x43 if ctrl && shift && s.ready() => s.request(hwnd, COPY),
                    0x57 if ctrl => s.request(hwnd, CLOSE),
                    0x30 if ctrl => s.view = View::default(),
                    0x31 if ctrl => {
                        s.view = View {
                            zoom: Zoom::Scale(1.0),
                            pan: (0.0, 0.0),
                        }
                    }
                    0xbb | 0xbd if ctrl => {
                        if let Some(i) = &s.image {
                            let scale = s.view.scale(s.layout.canvas, i.width, i.height)
                                * if wparam.0 == 0xbb { 1.25 } else { 0.8 };
                            let c = s.layout.canvas;
                            s.view.zoom_at(
                                scale,
                                (c.w / 2.0, c.y + c.h / 2.0),
                                c,
                                i.width,
                                i.height,
                            );
                        }
                    }
                    _ => {}
                }
                let _ = InvalidateRect(Some(hwnd), None, false);
                LRESULT(0)
            }
            WM_LBUTTONDOWN => {
                let (capture, move_window) = {
                    let s = &mut *ptr;
                    let p = s.point(lparam);
                    if s.palette_open {
                        SetCapture(hwnd);
                        return LRESULT(0);
                    }
                    s.focus = None;
                    s.pressed = s.layout.hit(p.0, p.1).filter(|c| c.enabled() && s.ready());
                    if s.pressed == Some(Control::Drag) {
                        s.drag_start = Some(p);
                    }
                    if s.layout.canvas.contains(p.0, p.1) && s.ready() {
                        s.pan_start = Some((p, s.view.pan));
                    }
                    (
                        s.pressed.is_some() || s.pan_start.is_some(),
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
            WM_MOUSEMOVE => {
                {
                    let s = &mut *ptr;
                    let p = s.point(lparam);
                    let hit = if s.palette_open {
                        None
                    } else {
                        s.layout.hit(p.0, p.1)
                    };
                    if s.hover != hit {
                        s.hover = hit;
                        let _ = InvalidateRect(Some(hwnd), None, false);
                    }
                    if let Some((start, pan)) = s.pan_start {
                        s.view.pan = (pan.0 + p.0 - start.0, pan.1 + p.1 - start.1);
                        if let Some(i) = &s.image {
                            s.view.clamp_pan(s.layout.canvas, i.width, i.height);
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
                (&mut *ptr).hover = None;
                let _ = InvalidateRect(Some(hwnd), None, false);
                LRESULT(0)
            }
            WM_LBUTTONUP => {
                {
                    let s = &mut *ptr;
                    let p = s.point(lparam);
                    s.pan_start = None;
                    s.drag_start = None;
                    if s.palette_open {
                        if let Some(index) = s.layout.palette_hit(p.0, p.1) {
                            if index < layout::PRESET_COLORS.len() {
                                s.selected_color = index;
                                s.palette_open = false;
                            }
                        } else {
                            s.palette_open = false;
                        }
                        let _ = InvalidateRect(Some(hwnd), None, false);
                        if GetCapture() == hwnd {
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
                if GetCapture() == hwnd {
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
                if s.layout.canvas.contains(p.0, p.1)
                    && let Some(i) = &s.image
                {
                    let delta = (wparam.0 >> 16) as u16 as i16 as f32 / 120.0;
                    if GetKeyState(VK_CONTROL.0 as i32) < 0 {
                        let scale =
                            s.view.scale(s.layout.canvas, i.width, i.height) * 1.25_f32.powf(delta);
                        s.view.zoom_at(scale, p, s.layout.canvas, i.width, i.height);
                    } else {
                        s.view.pan.1 += delta * 48.0;
                        s.view.clamp_pan(s.layout.canvas, i.width, i.height);
                    }
                    let _ = InvalidateRect(Some(hwnd), None, false);
                }
                LRESULT(0)
            }
            WM_SETCURSOR if lparam.0 as u16 as u32 == HTCLIENT => {
                let s = &*ptr;
                let cursor = if s.palette_open {
                    IDC_HAND
                } else if s.pan_start.is_some() {
                    IDC_SIZEALL
                } else if s.hover.is_some_and(|c| c.enabled() && s.ready()) {
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
    match control {
        Control::Save => state.request(hwnd, SAVE),
        Control::Copy => state.request(hwnd, COPY),
        Control::Zoom => state.request(hwnd, ZOOM_MENU),
        Control::Color => state.palette_open = !state.palette_open,
        _ => {}
    }
}
