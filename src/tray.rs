//! Native notification-area icon and menu. No polling or App borrows in modal loops.
use anyhow::Result;
use windows::{
    Win32::{
        Foundation::{HWND, LPARAM, POINT, WPARAM},
        UI::{Shell::*, WindowsAndMessaging::*},
    },
    core::w,
};

pub const CALLBACK: u32 = WM_APP + 6;
pub const CAPTURE_REQUEST: u32 = WM_APP + 7;
pub const QUIT_REQUEST: u32 = WM_APP + 8;
const ICON_ID: u32 = 1;
const NIN_KEYSELECT: u32 = NIN_SELECT | 1; // NINF_KEY
const CAPTURE_ITEM: usize = 1;
const QUIT_ITEM: usize = 2;

pub struct Tray {
    hwnd: HWND,
    icon: HICON,
    taskbar_created: u32,
}

impl Tray {
    /// The controller must be a hidden top-level window, not HWND_MESSAGE,
    /// so it receives Explorer's TaskbarCreated broadcast.
    pub fn new(hwnd: HWND) -> Result<Box<Self>> {
        let pixels = icon_pixels();
        let mask = [0u8; 128]; // 32 x 32 monochrome AND mask, alpha supplies transparency.
        let icon = unsafe { CreateIcon(None, 32, 32, 1, 32, mask.as_ptr(), pixels.as_ptr()) }?;
        let tray = Box::new(Self {
            hwnd,
            icon,
            taskbar_created: unsafe { RegisterWindowMessageW(w!("TaskbarCreated")) },
        });
        anyhow::ensure!(
            tray.taskbar_created != 0,
            "Cannot register taskbar restart message"
        );
        tray.add()?;
        // The Box stays at this address until Drop clears the window's pointer.
        unsafe {
            SetWindowLongPtrW(hwnd, GWLP_USERDATA, (&*tray as *const Self) as isize);
        }
        Ok(tray)
    }

    fn data(&self) -> NOTIFYICONDATAW {
        let mut data = NOTIFYICONDATAW {
            cbSize: std::mem::size_of::<NOTIFYICONDATAW>() as u32,
            hWnd: self.hwnd,
            uID: ICON_ID,
            uFlags: NIF_MESSAGE | NIF_ICON | NIF_TIP | NIF_SHOWTIP,
            uCallbackMessage: CALLBACK,
            hIcon: self.icon,
            ..Default::default()
        };
        let tip: Vec<_> = "Simple Screenshot - Alt + Shift + S"
            .encode_utf16()
            .collect();
        data.szTip[..tip.len()].copy_from_slice(&tip);
        data
    }

    fn add(&self) -> Result<()> {
        let mut data = self.data();
        anyhow::ensure!(
            unsafe { Shell_NotifyIconW(NIM_ADD, &data) }.as_bool(),
            "Cannot add Simple Screenshot to the notification area"
        );
        data.Anonymous.uVersion = NOTIFYICON_VERSION_4;
        if !unsafe { Shell_NotifyIconW(NIM_SETVERSION, &data) }.as_bool() {
            unsafe {
                let _ = Shell_NotifyIconW(NIM_DELETE, &data);
            }
            anyhow::bail!("Cannot configure notification-area interactions");
        }
        Ok(())
    }

    /// Called with an immutable reference from the window callback. Popup menus
    /// and OLE can reenter this callback without borrowing the App or mutating Tray.
    pub fn handle(&self, message: u32, wparam: WPARAM, lparam: LPARAM) -> bool {
        if message == self.taskbar_created {
            if let Err(error) = self.add() {
                eprintln!("Tray recovery failed: {error:#}");
            }
            return true;
        }
        // Accessibility providers may invoke a menu item by sending WM_COMMAND
        // rather than returning a selection from TrackPopupMenu.
        if message == WM_COMMAND && lparam.0 == 0 && wparam.0 >> 16 == 0 {
            let request = match wparam.0 {
                CAPTURE_ITEM => CAPTURE_REQUEST,
                QUIT_ITEM => QUIT_REQUEST,
                _ => return false,
            };
            unsafe {
                let _ = EndMenu();
                if let Err(error) = PostMessageW(Some(self.hwnd), request, WPARAM(0), LPARAM(0)) {
                    eprintln!("Tray command failed: {error}");
                }
            }
            return true;
        }
        if message != CALLBACK {
            return false;
        }
        let event = lparam.0 as u32 & 0xffff;
        let id = (lparam.0 as u32 >> 16) & 0xffff;
        if id != ICON_ID {
            return true;
        }
        if matches!(event, WM_CONTEXTMENU | NIN_SELECT | NIN_KEYSELECT) {
            let result = self.menu(callback_point(wparam));
            if let Err(error) = result {
                eprintln!("Tray menu failed: {error:#}");
            }
        }
        true
    }

    fn menu(&self, mut point: POINT) -> Result<()> {
        // Keyboard activation may have no supplied screen position.
        if point.x == -1 && point.y == -1 {
            unsafe {
                GetCursorPos(&mut point)?;
            }
        }
        let menu = Menu::new()?;
        let previous = unsafe { GetForegroundWindow() };
        // Required by the shell for outside-click/Escape dismissal of tray menus.
        unsafe {
            let _ = SetForegroundWindow(self.hwnd);
        }
        let chosen = unsafe {
            TrackPopupMenu(
                menu.0,
                TPM_RETURNCMD | TPM_NONOTIFY | TPM_RIGHTBUTTON,
                point.x,
                point.y,
                None,
                self.hwnd,
                None,
            )
        }
        .0 as usize;
        unsafe {
            // Restore only our own activation, never override a deliberate switch.
            if GetForegroundWindow() == self.hwnd
                && previous != self.hwnd
                && IsWindow(Some(previous)).as_bool()
            {
                let _ = SetForegroundWindow(previous);
            }
            PostMessageW(Some(self.hwnd), WM_NULL, WPARAM(0), LPARAM(0))?;
            match chosen {
                CAPTURE_ITEM => {
                    PostMessageW(Some(self.hwnd), CAPTURE_REQUEST, WPARAM(0), LPARAM(0))?
                }
                QUIT_ITEM => PostMessageW(Some(self.hwnd), QUIT_REQUEST, WPARAM(0), LPARAM(0))?,
                _ => (),
            }
        }
        Ok(())
    }
}

impl Drop for Tray {
    fn drop(&mut self) {
        unsafe {
            SetWindowLongPtrW(self.hwnd, GWLP_USERDATA, 0);
            let _ = Shell_NotifyIconW(NIM_DELETE, &self.data());
            let _ = DestroyIcon(self.icon);
        }
    }
}

struct Menu(HMENU);
impl Menu {
    fn new() -> Result<Self> {
        let menu = Self(unsafe { CreatePopupMenu() }?);
        unsafe {
            AppendMenuW(
                menu.0,
                MF_STRING,
                CAPTURE_ITEM,
                w!("Take screenshot\tAlt+Shift+S"),
            )?;
            AppendMenuW(menu.0, MF_SEPARATOR, 0, None)?;
            AppendMenuW(menu.0, MF_STRING, QUIT_ITEM, w!("Quit"))?;
        }
        Ok(menu)
    }
}
impl Drop for Menu {
    fn drop(&mut self) {
        unsafe {
            let _ = DestroyMenu(self.0);
        }
    }
}

fn callback_point(wparam: WPARAM) -> POINT {
    POINT {
        x: (wparam.0 & 0xffff) as u16 as i16 as i32,
        y: ((wparam.0 >> 16) & 0xffff) as u16 as i16 as i32,
    }
}

/// Supersampled blue tile with white selection corners, premultiplied BGRA.
/// A 32px source stays crisp in the small notification area without an asset runtime.
fn icon_pixels() -> Vec<u8> {
    let mut pixels = vec![0; 32 * 32 * 4];
    for y in 0..32 {
        for x in 0..32 {
            let mut channels = [0u32; 4];
            for sy in 0..4 {
                for sx in 0..4 {
                    let px = x as f32 + (sx as f32 + 0.5) / 4.0;
                    let py = y as f32 + (sy as f32 + 0.5) / 4.0;
                    let dx = (px - 16.0).abs() - 9.0;
                    let dy = (py - 16.0).abs() - 9.0;
                    if dx.max(0.0).hypot(dy.max(0.0)) > 6.0 {
                        continue;
                    }
                    let ax = (px - 16.0).abs();
                    let ay = (py - 16.0).abs();
                    let white = ((6.0..=8.0).contains(&ax) && (2.0..=8.0).contains(&ay))
                        || ((6.0..=8.0).contains(&ay) && (2.0..=8.0).contains(&ax));
                    let color = if white {
                        [255, 255, 255, 255]
                    } else {
                        [235, 125, 35, 255]
                    };
                    for i in 0..4 {
                        channels[i] += color[i];
                    }
                }
            }
            let offset = (y * 32 + x) * 4;
            for i in 0..4 {
                pixels[offset + i] = (channels[i] / 16) as u8;
            }
        }
    }
    pixels
}

#[cfg(test)]
mod tests {
    use super::*;
    use anyhow::Context;

    #[test]
    fn callback_coordinates_preserve_negative_monitors_and_keyboard_sentinel() {
        let point = callback_point(WPARAM(
            ((-300i16 as u16 as usize) << 16) | (-150i16 as u16 as usize),
        ));
        assert_eq!((point.x, point.y), (-150, -300));
        let point = callback_point(WPARAM(u32::MAX as usize));
        assert_eq!((point.x, point.y), (-1, -1));
    }

    #[test]
    fn icon_has_transparent_corners_and_premultiplied_edges() {
        let pixels = icon_pixels();
        assert_eq!(pixels.len(), 32 * 32 * 4);
        assert_eq!(&pixels[..4], &[0, 0, 0, 0]);
        assert_eq!(
            &pixels[(16 * 32 + 16) * 4..(16 * 32 + 16) * 4 + 4],
            &[235, 125, 35, 255]
        );
        assert!(pixels.chunks_exact(4).any(|p| p[3] > 0 && p[3] < 255));
        assert!(
            pixels
                .chunks_exact(4)
                .all(|p| p[..3].iter().all(|c| *c <= p[3]))
        );
    }

    #[test]
    fn native_menu_contains_only_capture_separator_and_quit() -> Result<()> {
        let menu = Menu::new().context("Cannot create tray menu")?;
        unsafe {
            assert_eq!(GetMenuItemCount(Some(menu.0)), 3);
            assert_eq!(GetMenuItemID(menu.0, 0), CAPTURE_ITEM as u32);
            assert_eq!(GetMenuItemID(menu.0, 2), QUIT_ITEM as u32);
        }
        Ok(())
    }
}
