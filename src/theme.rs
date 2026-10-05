//! Follow the Windows *app* appearance, not wallpaper or system taskbar color.
use std::sync::OnceLock;

use windows::{
    Win32::{Foundation::ERROR_SUCCESS, System::Registry::*},
    core::w,
};

pub fn dark() -> bool {
    static APPEARANCE: OnceLock<crate::settings::Appearance> = OnceLock::new();
    match *APPEARANCE.get_or_init(|| crate::settings::Settings::load().appearance) {
        crate::settings::Appearance::Dark => return true,
        crate::settings::Appearance::Light => return false,
        crate::settings::Appearance::System => {}
    }
    let mut value = 1u32; // Windows defaults to light apps if this key is absent.
    let mut size = std::mem::size_of::<u32>() as u32;
    let status = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            w!("Software\\Microsoft\\Windows\\CurrentVersion\\Themes\\Personalize"),
            w!("AppsUseLightTheme"),
            RRF_RT_REG_DWORD,
            None,
            Some((&mut value as *mut u32).cast()),
            Some(&mut size),
        )
    };
    dark_from_value((status == ERROR_SUCCESS && size == 4).then_some(value))
}

fn dark_from_value(value: Option<u32>) -> bool {
    value == Some(0)
}

/// Match the Windows app appearance: dark frame in dark mode, light in light mode.
pub fn border_rgb(dark: bool) -> u32 {
    if dark { 0x000000 } else { 0xffffff }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn border_matches_both_windows_appearances() {
        assert_eq!(border_rgb(true), 0x000000);
        assert_eq!(border_rgb(false), 0xffffff);
        assert!(dark_from_value(Some(0)));
        assert!(!dark_from_value(Some(1)));
        assert!(!dark_from_value(None));
    }
}
