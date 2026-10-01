//! File-backed pending queue. Only visible cards retain GPU surfaces; queued cards
//! retain small cached card pixels and a source-file deletion guard, not full captures.
use crate::{
    settings::AutoClose,
    thumbnail::{self, Thumbnail},
};
use anyhow::Result;
use std::{
    cell::Cell,
    collections::{BTreeMap, BTreeSet},
    time::Duration,
};
use windows::{
    Win32::{Foundation::*, UI::WindowsAndMessaging::*},
    core::{PCWSTR, w},
};

const PIN_ITEM: usize = 201;
const CLOSE_ITEM: usize = 202;
const OLDER_ITEM: usize = 203;
const NEWER_ITEM: usize = 204;
const CLOSE_ALL_ITEM: usize = 205;
thread_local! { static MENU_SOURCE: Cell<usize> = const { Cell::new(0) }; }

pub struct Gallery {
    items: Vec<Thumbnail>, // newest first, pins excluded from paging
    pages: BTreeMap<isize, usize>,
    capturing: bool,
    timeout: AutoClose,
}

impl Gallery {
    pub fn new(timeout: AutoClose) -> Self {
        Self {
            items: Vec::new(),
            pages: BTreeMap::new(),
            capturing: false,
            timeout,
        }
    }
    pub fn len(&self) -> usize {
        self.items.len()
    }
    pub fn timeout(&self) -> AutoClose {
        self.timeout
    }
    pub fn clear(&mut self) {
        self.items.clear();
        self.pages.clear();
        self.capturing = false;
    }
    pub fn insert(&mut self, thumbnail: Thumbnail) -> Result<()> {
        self.pages.insert(thumbnail.monitor(), 0);
        self.items.insert(0, thumbnail);
        self.capturing = false;
        self.reflow()
    }
    pub fn capture_hidden(&mut self, hidden: bool) -> Result<bool> {
        let had_visible = self.items.iter().any(Thumbnail::visible);
        self.capturing = hidden;
        self.reflow()?;
        Ok(had_visible)
    }
    pub fn pause_all(&mut self) -> Result<()> {
        for item in &mut self.items {
            item.set_paused(true)?;
        }
        Ok(())
    }
    pub fn take(&mut self, source: WPARAM) -> Option<(usize, Thumbnail)> {
        let index = self.items.iter().position(|item| item.matches(source))?;
        Some((index, self.items.remove(index)))
    }
    pub fn restore(&mut self, index: usize, item: Thumbnail) {
        self.items.insert(index.min(self.items.len()), item);
    }
    pub fn next_wake(&self) -> Option<Duration> {
        self.items.iter().filter_map(Thumbnail::next_wake).min()
    }
    pub fn tick(&mut self, source: Option<WPARAM>, dismiss: bool) -> Result<()> {
        let mut index = 0;
        while index < self.items.len() {
            if source.is_some_and(|s| !self.items[index].matches(s)) {
                index += 1;
                continue;
            }
            let result = if dismiss {
                self.items[index].dismiss()
            } else {
                self.items[index].tick()
            };
            match result {
                Ok(true) => {
                    self.items.remove(index);
                }
                Ok(false) => index += 1,
                Err(error) => {
                    self.items.remove(index);
                    self.reflow()?;
                    return Err(error);
                }
            }
        }
        self.reflow()
    }
    pub fn toggle_pin(&mut self, source: WPARAM) -> Result<()> {
        let Some(index) = self.items.iter().position(|item| item.matches(source)) else {
            return Ok(());
        };
        let monitor = self.items[index].monitor();
        let ordinal = free_pin_slot(
            self.items
                .iter()
                .filter(|item| item.pinned() && item.monitor() == monitor)
                .filter_map(Thumbnail::pin_slot),
        );
        self.items[index].toggle_pin()?;
        if self.items[index].pinned() {
            self.items[index].place_pin(ordinal)?;
        } else {
            let item = self.items.remove(index);
            self.items.insert(0, item);
        }
        self.pages.insert(monitor, 0);
        self.reflow()
    }
    pub fn configure(&mut self, timeout: AutoClose) -> Result<()> {
        self.timeout = timeout;
        for item in &mut self.items {
            item.set_timeout(timeout);
        }
        self.reflow()
    }
    pub fn navigate(&mut self, source: WPARAM, older: bool) -> Result<()> {
        let Some(item) = self.items.iter().find(|item| item.matches(source)) else {
            return Ok(());
        };
        let monitor = item.monitor();
        let capacity = item.capacity();
        let count = self
            .items
            .iter()
            .filter(|item| !item.pinned() && item.monitor() == monitor)
            .count();
        let current = self.pages.get(&monitor).copied().unwrap_or(0);
        self.pages
            .insert(monitor, page_after(current, count, capacity, older));
        self.reflow()
    }
    pub fn reflow(&mut self) -> Result<()> {
        loop {
            let mut counts = BTreeMap::<isize, (usize, usize)>::new();
            for item in &self.items {
                if !item.pinned() {
                    let group = counts.entry(item.monitor()).or_insert((0, item.capacity()));
                    group.0 += 1;
                }
            }
            for (monitor, (count, capacity)) in &counts {
                let page = self.pages.entry(*monitor).or_default();
                *page = clamp_page(*page, *count, *capacity);
            }
            let mut ordinals = BTreeMap::<isize, usize>::new();
            for item in &mut self.items {
                if self.capturing {
                    item.hide(false)?;
                } else if item.pinned() {
                    item.show()?;
                } else {
                    let ordinal = ordinals.entry(item.monitor()).or_default();
                    let page = self.pages.get(&item.monitor()).copied().unwrap_or(0);
                    if *ordinal >= page && *ordinal < page + item.capacity() {
                        item.show_slot(*ordinal - page)?;
                    } else {
                        item.hide(true)?;
                    }
                    *ordinal += 1;
                }
            }
            let before = self.items.len();
            self.items.retain(|item| !item.closed());
            if self.items.len() == before {
                return Ok(());
            }
        }
    }
    pub fn context_menu(&mut self, source: WPARAM, controller: HWND) -> Result<()> {
        let Some(item) = self.items.iter().find(|item| item.matches(source)) else {
            return Ok(());
        };
        let pinned = item.pinned();
        let monitor = item.monitor();
        let capacity = item.capacity();
        let count = self
            .items
            .iter()
            .filter(|item| !item.pinned() && item.monitor() == monitor)
            .count();
        let page = self.pages.get(&monitor).copied().unwrap_or(0);
        self.pause_all()?;
        let result = show_menu(
            controller,
            source,
            pinned,
            page > 0,
            page + capacity < count,
            count,
        );
        self.reflow()?;
        result
    }
}

fn free_pin_slot(slots: impl Iterator<Item = usize>) -> usize {
    let used: BTreeSet<_> = slots.collect();
    (0..)
        .find(|slot| !used.contains(slot))
        .expect("A finite gallery has a free pin slot")
}

fn clamp_page(page: usize, count: usize, capacity: usize) -> usize {
    if count == 0 {
        0
    } else {
        page.min((count - 1) / capacity.max(1) * capacity.max(1))
    }
}
fn page_after(page: usize, count: usize, capacity: usize, older: bool) -> usize {
    let step = capacity.max(1);
    clamp_page(
        if older {
            page.saturating_add(step)
        } else {
            page.saturating_sub(step)
        },
        count,
        step,
    )
}

fn send_action(controller: HWND, source: WPARAM, chosen: usize) -> Result<()> {
    let (message, parameter) = match chosen {
        PIN_ITEM => (thumbnail::PIN, 0),
        CLOSE_ITEM => (thumbnail::DISMISS, 0),
        OLDER_ITEM => (thumbnail::NAVIGATE, 1),
        NEWER_ITEM => (thumbnail::NAVIGATE, -1),
        CLOSE_ALL_ITEM => (crate::tray::CLOSE_ALL_REQUEST, 0),
        _ => return Ok(()),
    };
    unsafe {
        PostMessageW(Some(controller), message, source, LPARAM(parameter))?;
    }
    Ok(())
}

pub fn handle_menu_command(controller: HWND, wparam: WPARAM, lparam: LPARAM) -> bool {
    if lparam.0 != 0 || wparam.0 >> 16 != 0 || !(PIN_ITEM..=CLOSE_ALL_ITEM).contains(&wparam.0) {
        return false;
    }
    let source = MENU_SOURCE.get();
    if source == 0 {
        return false;
    }
    unsafe {
        let _ = EndMenu();
    }
    if let Err(error) = send_action(controller, WPARAM(source), wparam.0) {
        eprintln!("Screenshot menu failed: {error}");
    }
    true
}

fn show_menu(
    controller: HWND,
    source: WPARAM,
    pinned: bool,
    newer: bool,
    older: bool,
    count: usize,
) -> Result<()> {
    struct Menu(HMENU);
    impl Drop for Menu {
        fn drop(&mut self) {
            unsafe {
                let _ = DestroyMenu(self.0);
            }
        }
    }
    struct SourceGuard(usize);
    impl Drop for SourceGuard {
        fn drop(&mut self) {
            MENU_SOURCE.set(self.0);
        }
    }
    let _source = SourceGuard(MENU_SOURCE.replace(source.0));
    unsafe {
        let menu = Menu(CreatePopupMenu()?);
        let title = if count == 0 {
            "Pinned screenshot".to_owned()
        } else if older || newer {
            format!("{count} pending screenshots - wheel to browse")
        } else {
            format!("{count} pending screenshots")
        };
        let heading: Vec<u16> = title.encode_utf16().chain(Some(0)).collect();
        AppendMenuW(menu.0, MF_STRING | MF_DISABLED, 0, PCWSTR(heading.as_ptr()))?;
        AppendMenuW(menu.0, MF_SEPARATOR, 0, None)?;
        AppendMenuW(
            menu.0,
            MF_STRING,
            PIN_ITEM,
            if pinned {
                w!("Unpin")
            } else {
                w!("Pin (Alt+drag to move)")
            },
        )?;
        AppendMenuW(menu.0, MF_STRING, CLOSE_ITEM, w!("Close"))?;
        AppendMenuW(
            menu.0,
            MF_STRING | if older { MF_ENABLED } else { MF_GRAYED },
            OLDER_ITEM,
            w!("Older screenshots"),
        )?;
        AppendMenuW(
            menu.0,
            MF_STRING | if newer { MF_ENABLED } else { MF_GRAYED },
            NEWER_ITEM,
            w!("Newer screenshots"),
        )?;
        AppendMenuW(menu.0, MF_SEPARATOR, 0, None)?;
        AppendMenuW(
            menu.0,
            MF_STRING,
            CLOSE_ALL_ITEM,
            w!("Close all screenshots"),
        )?;
        let mut point = POINT::default();
        GetCursorPos(&mut point)?;
        let previous = GetForegroundWindow();
        let _ = SetForegroundWindow(controller);
        let chosen = TrackPopupMenu(
            menu.0,
            TPM_RETURNCMD | TPM_NONOTIFY | TPM_RIGHTBUTTON,
            point.x,
            point.y,
            None,
            controller,
            None,
        )
        .0 as usize;
        if GetForegroundWindow() == controller && IsWindow(Some(previous)).as_bool() {
            let _ = SetForegroundWindow(previous);
        }
        PostMessageW(Some(controller), WM_NULL, WPARAM(0), LPARAM(0))?;
        send_action(controller, source, chosen)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn pin_slots_do_not_overlap_survivors_after_an_earlier_pin_is_closed() {
        assert_eq!(free_pin_slot([].into_iter()), 0);
        assert_eq!(free_pin_slot([0, 1, 2].into_iter()), 3);
        assert_eq!(free_pin_slot([1, 2].into_iter()), 0);
        assert_eq!(free_pin_slot([0, 2].into_iter()), 1);
    }
    #[test]
    fn paging_keeps_fourth_and_later_captures_accessible_and_clamps_after_removal() {
        assert_eq!(page_after(0, 8, 3, true), 3);
        assert_eq!(page_after(3, 8, 3, true), 6);
        assert_eq!(page_after(6, 8, 3, true), 6);
        assert_eq!(page_after(6, 8, 3, false), 3);
        assert_eq!(clamp_page(6, 4, 3), 3);
        assert_eq!(clamp_page(3, 3, 3), 0);
        assert_eq!(clamp_page(3, 0, 3), 0);
        assert_eq!(page_after(0, 5, 1, true), 1);
    }
}
