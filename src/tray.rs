//! Native notification-area icon and menu. No polling or App borrows in modal loops.
use crate::settings::{Appearance, AutoClose, CaptureShortcut, Placement, Settings};
use anyhow::Result;
use std::cell::{Cell, RefCell};
use windows::{
    Win32::{
        Foundation::{HWND, LPARAM, POINT, WPARAM},
        UI::{Shell::*, WindowsAndMessaging::*},
    },
    core::{PCWSTR, w},
};

pub const CALLBACK: u32 = WM_APP + 6;
pub const CAPTURE_REQUEST: u32 = WM_APP + 7;
pub const QUIT_REQUEST: u32 = WM_APP + 8;
pub const SET_TIMEOUT_REQUEST: u32 = WM_APP + 12;
pub const CLOSE_ALL_REQUEST: u32 = WM_APP + 13;
pub const SET_PLACEMENT_REQUEST: u32 = WM_APP + 18;
pub const INSTALL_UPDATE_REQUEST: u32 = WM_APP + 20;
pub const SET_APPEARANCE_REQUEST: u32 = WM_APP + 21;
pub const SET_SHORTCUT_REQUEST: u32 = WM_APP + 22;
const ICON_ID: u32 = 1;
const NIN_KEYSELECT: u32 = NIN_SELECT | 1; // NINF_KEY
const CAPTURE_ITEM: usize = 1;
const QUIT_ITEM: usize = 2;
const CLOSE_ALL_ITEM: usize = 3;
const UPDATE_ITEM: usize = 4;
const TIMER_BASE: usize = 100;
const PLACEMENT_BASE: usize = 120;
const APPEARANCE_BASE: usize = 130;
const SHORTCUT_BASE: usize = 140;

pub struct Tray {
    hwnd: HWND,
    icon: HICON,
    taskbar_created: u32,
    timeout: Cell<AutoClose>,
    placement: Cell<Placement>,
    count: Cell<usize>,
    update: RefCell<Option<String>>,
    update_failed: Cell<bool>,
    updates_supported: bool,
    shortcut: Cell<CaptureShortcut>,
    appearance: Cell<Appearance>,
    shortcut_available: Cell<bool>,
}

impl Tray {
    /// The controller must be a hidden top-level window, not HWND_MESSAGE,
    /// so it receives Explorer's TaskbarCreated broadcast.
    pub fn new(
        hwnd: HWND,
        timeout: AutoClose,
        placement: Placement,
        shortcut: CaptureShortcut,
        appearance: Appearance,
        shortcut_available: bool,
        updates_supported: bool,
    ) -> Result<Box<Self>> {
        let pixels = icon_pixels();
        let mask = [0u8; 128]; // 32 x 32 monochrome AND mask, alpha supplies transparency.
        let icon = unsafe { CreateIcon(None, 32, 32, 1, 32, mask.as_ptr(), pixels.as_ptr()) }?;
        let tray = Box::new(Self {
            hwnd,
            icon,
            taskbar_created: unsafe { RegisterWindowMessageW(w!("TaskbarCreated")) },
            timeout: Cell::new(timeout),
            placement: Cell::new(placement),
            count: Cell::new(0),
            update: RefCell::new(None),
            update_failed: Cell::new(false),
            updates_supported,
            shortcut: Cell::new(shortcut),
            appearance: Cell::new(appearance),
            shortcut_available: Cell::new(shortcut_available),
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

    pub fn update(&self, timeout: AutoClose, placement: Placement, count: usize) {
        self.timeout.set(timeout);
        self.placement.set(placement);
        self.count.set(count);
    }

    pub fn set_update_failure(&self, failed: bool) {
        self.update_failed.set(failed);
    }

    pub fn set_update(&self, version: Option<String>) {
        *self.update.borrow_mut() = version;
    }

    pub fn is_update_announced(&self, version: &str) -> bool {
        self.update.borrow().as_deref() == Some(version)
    }

    pub fn notify_update(&self, version: &str) -> Result<()> {
        let mut data = self.data();
        data.uFlags = NIF_INFO;
        data.dwInfoFlags = NIIF_INFO;
        let title: Vec<u16> = "Catch It update ready".encode_utf16().collect();
        data.szInfoTitle[..title.len()].copy_from_slice(&title);
        let message: Vec<u16> =
            format!("Version {version} is ready. Open the tray menu to install.")
                .encode_utf16()
                .collect();
        data.szInfo[..message.len()].copy_from_slice(&message);
        anyhow::ensure!(
            unsafe { Shell_NotifyIconW(NIM_MODIFY, &data) }.as_bool(),
            "Cannot show update notification"
        );
        Ok(())
    }

    pub fn set_appearance(&self, appearance: Appearance) {
        self.appearance.set(appearance);
    }
    pub fn set_shortcut(&self, shortcut: CaptureShortcut, available: bool) -> Result<()> {
        self.shortcut.set(shortcut);
        self.shortcut_available.set(available);
        anyhow::ensure!(
            unsafe { Shell_NotifyIconW(NIM_MODIFY, &self.data()) }.as_bool(),
            "Cannot update tray shortcut hint"
        );
        Ok(())
    }

    fn post_choice(&self, chosen: usize) -> Result<bool> {
        let (message, parameter) = match chosen {
            CAPTURE_ITEM => (CAPTURE_REQUEST, 0),
            QUIT_ITEM => (QUIT_REQUEST, 0),
            CLOSE_ALL_ITEM => (CLOSE_ALL_REQUEST, 0),
            UPDATE_ITEM if self.update.borrow().is_some() => (INSTALL_UPDATE_REQUEST, 0),
            id if (TIMER_BASE..TIMER_BASE + AutoClose::ALL.len()).contains(&id) => {
                (SET_TIMEOUT_REQUEST, id - TIMER_BASE)
            }
            id if (PLACEMENT_BASE..PLACEMENT_BASE + Placement::ALL.len()).contains(&id) => {
                (SET_PLACEMENT_REQUEST, id - PLACEMENT_BASE)
            }
            id if (APPEARANCE_BASE..APPEARANCE_BASE + Appearance::ALL.len()).contains(&id) => {
                (SET_APPEARANCE_REQUEST, id - APPEARANCE_BASE)
            }
            id if (SHORTCUT_BASE..SHORTCUT_BASE + CaptureShortcut::ALL.len()).contains(&id) => {
                (SET_SHORTCUT_REQUEST, id - SHORTCUT_BASE)
            }
            _ => return Ok(false),
        };
        unsafe {
            PostMessageW(Some(self.hwnd), message, WPARAM(parameter), LPARAM(0))?;
        }
        Ok(true)
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
        let tip: Vec<_> = if self.shortcut_available.get() {
            format!("Catch It - {}", self.shortcut.get().label())
        } else {
            "Catch It - shortcut unavailable; change in Settings".to_string()
        }
        .encode_utf16()
        .collect();
        data.szTip[..tip.len()].copy_from_slice(&tip);
        data
    }

    fn add(&self) -> Result<()> {
        let mut data = self.data();
        anyhow::ensure!(
            unsafe { Shell_NotifyIconW(NIM_ADD, &data) }.as_bool(),
            "Cannot add Catch It to the notification area"
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
            match self.post_choice(wparam.0) {
                Ok(true) => unsafe {
                    let _ = EndMenu();
                },
                Ok(false) => return false,
                Err(error) => eprintln!("Tray command failed: {error}"),
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
        let menu = Menu::new(
            Settings {
                auto_close: self.timeout.get(),
                placement: self.placement.get(),
                shortcut: self.shortcut.get(),
                appearance: self.appearance.get(),
            },
            self.count.get(),
            self.shortcut_available.get(),
            self.update.borrow().as_deref(),
            self.update_failed.get(),
            self.updates_supported,
        )?;
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
            self.post_choice(chosen)?;
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
    fn new(
        settings: Settings,
        count: usize,
        shortcut_available: bool,
        update: Option<&str>,
        update_failed: bool,
        updates_supported: bool,
    ) -> Result<Self> {
        let Settings {
            auto_close: timeout,
            placement,
            shortcut,
            appearance,
        } = settings;
        let menu = Self(unsafe { CreatePopupMenu() }?);
        unsafe {
            let capture_label: Vec<u16> = format!("Take screenshot\t{}", shortcut.label())
                .encode_utf16()
                .chain(Some(0))
                .collect();
            AppendMenuW(
                menu.0,
                MF_STRING,
                CAPTURE_ITEM,
                PCWSTR(capture_label.as_ptr()),
            )?;
            let timers = Self(CreatePopupMenu()?);
            for (index, choice) in AutoClose::ALL.into_iter().enumerate() {
                let label: Vec<u16> = choice.label().encode_utf16().chain(Some(0)).collect();
                AppendMenuW(
                    timers.0,
                    MF_STRING
                        | if timeout == choice {
                            MF_CHECKED
                        } else {
                            MF_UNCHECKED
                        },
                    TIMER_BASE + index,
                    PCWSTR(label.as_ptr()),
                )?;
            }
            AppendMenuW(menu.0, MF_POPUP, timers.0.0 as usize, w!("Auto-close"))?;
            std::mem::forget(timers); // parent menu owns and destroys its submenu
            let positions = Self(CreatePopupMenu()?);
            for (index, choice) in Placement::ALL.into_iter().enumerate() {
                let label: Vec<u16> = choice.label().encode_utf16().chain(Some(0)).collect();
                AppendMenuW(
                    positions.0,
                    MF_STRING
                        | if placement == choice {
                            MF_CHECKED
                        } else {
                            MF_UNCHECKED
                        },
                    PLACEMENT_BASE + index,
                    PCWSTR(label.as_ptr()),
                )?;
            }
            AppendMenuW(
                menu.0,
                MF_POPUP,
                positions.0.0 as usize,
                w!("Quick Access position"),
            )?;
            std::mem::forget(positions);
            let label: Vec<u16> = format!("Close all screenshots ({count})")
                .encode_utf16()
                .chain(Some(0))
                .collect();
            AppendMenuW(
                menu.0,
                MF_STRING | if count > 0 { MF_ENABLED } else { MF_GRAYED },
                CLOSE_ALL_ITEM,
                PCWSTR(label.as_ptr()),
            )?;
            let settings = Self(CreatePopupMenu()?);
            let appearances = Self(CreatePopupMenu()?);
            for (index, choice) in Appearance::ALL.into_iter().enumerate() {
                let label: Vec<u16> = choice.label().encode_utf16().chain(Some(0)).collect();
                AppendMenuW(
                    appearances.0,
                    MF_STRING
                        | if appearance == choice {
                            MF_CHECKED
                        } else {
                            MF_UNCHECKED
                        },
                    APPEARANCE_BASE + index,
                    PCWSTR(label.as_ptr()),
                )?;
            }
            AppendMenuW(
                settings.0,
                MF_POPUP,
                appearances.0.0 as usize,
                w!("Appearance"),
            )?;
            std::mem::forget(appearances);
            let shortcuts = Self(CreatePopupMenu()?);
            for (index, choice) in CaptureShortcut::ALL.into_iter().enumerate() {
                let label: Vec<u16> = choice.label().encode_utf16().chain(Some(0)).collect();
                AppendMenuW(
                    shortcuts.0,
                    MF_STRING
                        | if shortcut == choice && shortcut_available {
                            MF_CHECKED
                        } else {
                            MF_UNCHECKED
                        },
                    SHORTCUT_BASE + index,
                    PCWSTR(label.as_ptr()),
                )?;
            }
            AppendMenuW(
                settings.0,
                MF_POPUP,
                shortcuts.0.0 as usize,
                w!("Capture shortcut"),
            )?;
            std::mem::forget(shortcuts);
            AppendMenuW(menu.0, MF_POPUP, settings.0.0 as usize, w!("Settings"))?;
            std::mem::forget(settings);
            AppendMenuW(menu.0, MF_SEPARATOR, 0, None)?;
            if let Some(version) = update {
                let label: Vec<u16> = format!("Install update v{version}")
                    .encode_utf16()
                    .chain(Some(0))
                    .collect();
                AppendMenuW(menu.0, MF_STRING, UPDATE_ITEM, PCWSTR(label.as_ptr()))?;
            } else if !updates_supported {
                AppendMenuW(
                    menu.0,
                    MF_STRING | MF_GRAYED,
                    UPDATE_ITEM,
                    w!("Updates require installed copy"),
                )?;
            } else if update_failed {
                AppendMenuW(
                    menu.0,
                    MF_STRING | MF_GRAYED,
                    UPDATE_ITEM,
                    w!("Update check unavailable (retrying)"),
                )?;
            } else {
                AppendMenuW(
                    menu.0,
                    MF_STRING | MF_GRAYED,
                    UPDATE_ITEM,
                    w!("Checking for updates / up to date"),
                )?;
            }
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
    fn update_menu_item_only_appears_when_ready() -> Result<()> {
        let menu = Menu::new(
            Settings {
                auto_close: AutoClose::Never,
                placement: Placement::TopLeft,
                shortcut: CaptureShortcut::AltShiftS,
                appearance: Appearance::System,
            },
            0,
            true,
            Some("0.2.0"),
            false,
            true,
        )?;
        unsafe {
            assert_eq!(GetMenuItemCount(Some(menu.0)), 8);
            assert_eq!(GetMenuItemID(menu.0, 6), UPDATE_ITEM as u32);
            assert_eq!(GetMenuItemID(menu.0, 7), QUIT_ITEM as u32);
        }
        Ok(())
    }

    #[test]
    fn native_menu_has_checked_timer_and_placement_submenus() -> Result<()> {
        let menu = Menu::new(
            Settings {
                auto_close: AutoClose::Never,
                placement: Placement::TopLeft,
                shortcut: CaptureShortcut::AltShiftS,
                appearance: Appearance::System,
            },
            5,
            true,
            None,
            false,
            true,
        )
        .context("Cannot create tray menu")?;
        unsafe {
            assert_eq!(GetMenuItemCount(Some(menu.0)), 8);
            assert_eq!(GetMenuItemID(menu.0, 0), CAPTURE_ITEM as u32);
            assert_eq!(GetMenuItemID(menu.0, 6), UPDATE_ITEM as u32);
            assert_eq!(GetMenuItemID(menu.0, 7), QUIT_ITEM as u32);
            let timers = GetSubMenu(menu.0, 1);
            assert_eq!(GetMenuItemCount(Some(timers)), 6);
            assert!(GetMenuState(timers, TIMER_BASE as u32 + 5, MF_BYCOMMAND) & MF_CHECKED.0 != 0);
            let positions = GetSubMenu(menu.0, 2);
            assert_eq!(GetMenuItemCount(Some(positions)), 4);
            assert!(
                GetMenuState(positions, PLACEMENT_BASE as u32, MF_BYCOMMAND) & MF_CHECKED.0 != 0
            );
            let settings = GetSubMenu(menu.0, 4);
            assert_eq!(GetMenuItemCount(Some(settings)), 2);
            let appearances = GetSubMenu(settings, 0);
            let shortcuts = GetSubMenu(settings, 1);
            assert_eq!(GetMenuItemCount(Some(appearances)), 3);
            assert_eq!(GetMenuItemCount(Some(shortcuts)), 3);
            assert!(
                GetMenuState(appearances, APPEARANCE_BASE as u32, MF_BYCOMMAND) & MF_CHECKED.0 != 0
            );
            assert!(
                GetMenuState(shortcuts, SHORTCUT_BASE as u32, MF_BYCOMMAND) & MF_CHECKED.0 != 0
            );
        }
        Ok(())
    }
}
