//! rcWork can incorrectly include the visible taskbar (observed on a remote desktop).
//! Reserve the real shell taskbar thickness too, including its auto-hide reveal area.
use anyhow::Result;
use windows::Win32::{
    Foundation::{HWND, LPARAM, RECT},
    Graphics::Gdi::{
        GetMonitorInfoW, HMONITOR, MONITOR_DEFAULTTONEAREST, MONITORINFO, MonitorFromWindow,
    },
    UI::WindowsAndMessaging::{EnumWindows, GetClassNameW, GetWindowRect},
};
use windows::core::BOOL;

use super::layout::WorkArea;

struct Probe {
    monitor: HMONITOR,
    bounds: RECT,
    work: RECT,
}

pub fn for_monitor(monitor: HMONITOR) -> Result<WorkArea> {
    let mut info = MONITORINFO {
        cbSize: std::mem::size_of::<MONITORINFO>() as u32,
        ..Default::default()
    };
    unsafe {
        anyhow::ensure!(
            GetMonitorInfoW(monitor, &mut info).as_bool(),
            "Cannot read capture monitor work area"
        );
    }
    let mut probe = Probe {
        monitor,
        bounds: info.rcMonitor,
        work: info.rcWork,
    };
    // Synchronous enumeration; the stack probe outlives every callback.
    unsafe {
        EnumWindows(
            Some(taskbar_window),
            LPARAM((&mut probe as *mut Probe) as isize),
        )?;
    }
    anyhow::ensure!(
        probe.work.right > probe.work.left && probe.work.bottom > probe.work.top,
        "Taskbar leaves no usable thumbnail work area"
    );
    Ok(WorkArea {
        left: probe.work.left,
        top: probe.work.top,
        width: (probe.work.right - probe.work.left) as u32,
        height: (probe.work.bottom - probe.work.top) as u32,
    })
}

unsafe extern "system" fn taskbar_window(hwnd: HWND, data: LPARAM) -> BOOL {
    unsafe {
        let mut name = [0u16; 64];
        let len = GetClassNameW(hwnd, &mut name);
        if len <= 0 {
            return BOOL(1);
        }
        let name = String::from_utf16_lossy(&name[..len as usize]);
        if name != "Shell_TrayWnd" && name != "Shell_SecondaryTrayWnd" {
            return BOOL(1);
        }
        let probe = &mut *(data.0 as *mut Probe);
        if MonitorFromWindow(hwnd, MONITOR_DEFAULTTONEAREST) != probe.monitor {
            return BOOL(1);
        }
        let mut taskbar = RECT::default();
        if GetWindowRect(hwnd, &mut taskbar).is_ok() {
            probe.work = reserve_taskbar(probe.bounds, probe.work, taskbar);
        }
    }
    BOOL(1)
}

fn reserve_taskbar(bounds: RECT, mut work: RECT, bar: RECT) -> RECT {
    let width = bar.right - bar.left;
    let height = bar.bottom - bar.top;
    if width <= 0 || height <= 0 {
        return work;
    }
    if width >= height {
        if bar.right <= bounds.left || bar.left >= bounds.right {
            return work;
        }
        let top_distance = (i64::from(bar.bottom) - i64::from(bounds.top)).abs();
        let bottom_distance = (i64::from(bar.top) - i64::from(bounds.bottom)).abs();
        if top_distance.min(bottom_distance) > i64::from(height) {
            return work;
        }
        if top_distance < bottom_distance {
            work.top = work.top.max(bounds.top.saturating_add(height));
        } else {
            work.bottom = work.bottom.min(bounds.bottom.saturating_sub(height));
        }
    } else {
        if bar.bottom <= bounds.top || bar.top >= bounds.bottom {
            return work;
        }
        let left_distance = (i64::from(bar.right) - i64::from(bounds.left)).abs();
        let right_distance = (i64::from(bar.left) - i64::from(bounds.right)).abs();
        if left_distance.min(right_distance) > i64::from(width) {
            return work;
        }
        if left_distance < right_distance {
            work.left = work.left.max(bounds.left.saturating_add(width));
        } else {
            work.right = work.right.min(bounds.right.saturating_sub(width));
        }
    }
    work
}

#[cfg(test)]
mod tests {
    use super::*;
    fn rect(left: i32, top: i32, right: i32, bottom: i32) -> RECT {
        RECT {
            left,
            top,
            right,
            bottom,
        }
    }

    #[test]
    fn reserves_actual_bottom_taskbar_when_rcwork_is_wrong_without_double_insetting() {
        let bounds = rect(0, 0, 1920, 1080);
        let bar = rect(0, 1028, 1920, 1080);
        let expected = rect(0, 0, 1920, 1028);
        assert_eq!(reserve_taskbar(bounds, bounds, bar), expected);
        assert_eq!(reserve_taskbar(bounds, expected, bar), expected);
        assert_eq!(
            reserve_taskbar(bounds, rect(0, 0, 1920, 1000), bar).bottom,
            1000
        );
    }

    #[test]
    fn reserves_full_reveal_thickness_for_auto_hidden_taskbar() {
        let bounds = rect(0, 0, 1920, 1080);
        assert_eq!(
            reserve_taskbar(bounds, bounds, rect(0, 1078, 1920, 1130)),
            rect(0, 0, 1920, 1028)
        );
    }

    #[test]
    fn supports_all_docked_edges_and_negative_monitor_origins() {
        let b = rect(-1920, -200, 0, 880);
        for (bar, expected) in [
            (rect(-1920, -200, 0, -148), rect(-1920, -148, 0, 880)),
            (rect(-1920, 828, 0, 880), rect(-1920, -200, 0, 828)),
            (rect(-1920, -200, -1868, 880), rect(-1868, -200, 0, 880)),
            (rect(-52, -200, 0, 880), rect(-1920, -200, -52, 880)),
        ] {
            assert_eq!(reserve_taskbar(b, b, bar), expected);
        }
        assert_eq!(reserve_taskbar(b, b, rect(0, 1028, 1920, 1080)), b);
    }
}
