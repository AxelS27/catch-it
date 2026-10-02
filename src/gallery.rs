//! Bounded FIFO preview queue. Overflow permanently drops the oldest unpinned
//! thumbnail, never its PNG. Pins stay in chronological bottom-to-top order.
use crate::{
    settings::AutoClose,
    thumbnail::{self, Thumbnail},
};
use anyhow::Result;
use std::{cell::Cell, collections::BTreeMap, time::Duration};
use windows::{
    Win32::{Foundation::*, UI::WindowsAndMessaging::*},
    core::{PCWSTR, w},
};

const PIN_ITEM: usize = 201;
const CLOSE_ITEM: usize = 202;
const CLOSE_ALL_ITEM: usize = 203;
thread_local! { static MENU_SOURCE: Cell<usize> = const { Cell::new(0) }; }

pub struct Gallery {
    items: Vec<Thumbnail>, // newest first; pins keep their chronological stack position
    capturing: bool,
    timeout: AutoClose,
}

impl Gallery {
    pub fn new(timeout: AutoClose) -> Self {
        Self {
            items: Vec::new(),
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
        self.capturing = false;
    }
    pub fn insert(&mut self, thumbnail: Thumbnail) -> Result<()> {
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
    pub fn get(&self, source: WPARAM) -> Option<&Thumbnail> {
        self.items.iter().find(|item| item.matches(source))
    }
    pub fn set_editing(&mut self, path: &std::path::Path, editing: bool) -> Result<()> {
        if let Some(item) = self.items.iter_mut().find(|item| item.path() == path) {
            item.set_editing(editing)?;
        }
        self.reflow()
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
        self.items[index].toggle_pin()?;
        self.reflow()
    }
    pub fn configure(&mut self, timeout: AutoClose) -> Result<()> {
        self.timeout = timeout;
        for item in &mut self.items {
            item.set_timeout(timeout);
        }
        self.reflow()
    }
    pub fn reflow(&mut self) -> Result<()> {
        loop {
            let mut groups = BTreeMap::<isize, Vec<usize>>::new();
            for (index, item) in self.items.iter().enumerate() {
                groups.entry(item.monitor()).or_default().push(index);
            }
            let mut keep = vec![true; self.items.len()];
            for indices in groups.into_values() {
                let reserved: Vec<_> = indices
                    .iter()
                    .map(|&index| self.items[index].reserved())
                    .collect();
                let capacity = self.items[indices[0]].capacity();
                let slots = stack_slots(&reserved, capacity);
                let head = fifo_head(&reserved, &slots);
                // Gate all timers before resuming visibility. Keep a dismissing
                // head until its exit finishes, so newer cards cannot vanish first.
                for (ordinal, &index) in indices.iter().enumerate() {
                    self.items[index].set_auto_dismiss_blocked(Some(ordinal) != head)?;
                }
                for (index, slot) in indices.into_iter().zip(slots) {
                    let Some(slot) = slot else {
                        keep[index] = false;
                        continue;
                    };
                    let item = &mut self.items[index];
                    if self.capturing {
                        item.hide()?;
                    } else {
                        item.show_slot(slot)?;
                    }
                }
            }
            let before = self.items.len();
            let mut index = 0;
            self.items.retain(|item| {
                let retained = keep[index] && !item.closed();
                index += 1;
                retained
            });
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
        let count = self
            .items
            .iter()
            .filter(|item| !item.pinned() && item.monitor() == monitor)
            .count();
        self.pause_all()?;
        let result = show_menu(controller, source, pinned, count);
        self.reflow()?;
        result
    }
}

/// Select newest pending cards alongside every pin/editor source, then pack oldest-to-newest
/// from the bottom of the dock. None means permanent eviction, not a hidden page.
/// Pinning changes lifetime, never capture order.
fn stack_slots(reserved: &[bool], capacity: usize) -> Vec<Option<usize>> {
    let pending_capacity = capacity.saturating_sub(reserved.iter().filter(|&&slot| slot).count());
    let mut pending = 0;
    let mut selected = Vec::new();
    for (index, &pin) in reserved.iter().enumerate() {
        if pin {
            selected.push(index);
        } else {
            if pending < pending_capacity {
                selected.push(index);
            }
            pending += 1;
        }
    }
    let mut slots = vec![None; reserved.len()];
    for (slot, index) in selected.into_iter().rev().enumerate() {
        slots[index] = Some(slot);
    }
    slots
}

fn fifo_head(reserved: &[bool], slots: &[Option<usize>]) -> Option<usize> {
    reserved
        .iter()
        .zip(slots)
        .rposition(|(&pin, slot)| !pin && slot.is_some())
}

fn send_action(controller: HWND, source: WPARAM, chosen: usize) -> Result<()> {
    let (message, parameter) = match chosen {
        PIN_ITEM => (thumbnail::PIN, 0),
        CLOSE_ITEM => (thumbnail::DISMISS, 0),
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

fn show_menu(controller: HWND, source: WPARAM, pinned: bool, count: usize) -> Result<()> {
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
            if pinned { w!("Unpin") } else { w!("Pin") },
        )?;
        AppendMenuW(menu.0, MF_STRING, CLOSE_ITEM, w!("Close"))?;
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
    fn editing_source_reserves_overflow_slot_and_does_not_block_other_timers() {
        let reserved = [false, false, false, false, false, true];
        let slots = stack_slots(&reserved, 5);
        assert_eq!(
            slots,
            vec![Some(4), Some(3), Some(2), Some(1), None, Some(0)]
        );
        assert_eq!(fifo_head(&reserved, &slots), Some(3));
        // Closing the editor releases the reservation, not capture order.
        assert_eq!(stack_slots(&[false; 6], 5)[5], None);
    }

    #[test]
    fn pin_and_unpin_preserve_slots_and_removal_compacts_surviving_pins() {
        let before = stack_slots(&[false; 5], 5);
        assert_eq!(before, vec![Some(4), Some(3), Some(2), Some(1), Some(0)]);
        assert_eq!(stack_slots(&[false, false, true, false, true], 5), before);
        assert_eq!(
            stack_slots(&[false, false, true, true], 5),
            vec![Some(3), Some(2), Some(1), Some(0)]
        );
    }

    #[test]
    fn overflow_evicts_oldest_unpinned_and_never_resurrects_after_removal() {
        let mut captures: Vec<_> = (1..=6).rev().map(|id| (id, false)).collect();
        let slots = stack_slots(&[false; 6], 5);
        assert_eq!(
            slots,
            vec![Some(4), Some(3), Some(2), Some(1), Some(0), None]
        );
        captures = captures
            .into_iter()
            .zip(slots)
            .filter_map(|(entry, slot)| slot.map(|_| entry))
            .collect();
        assert_eq!(
            captures
                .iter()
                .rev()
                .map(|entry| entry.0)
                .collect::<Vec<_>>(),
            [2, 3, 4, 5, 6]
        );
        captures.pop(); // Screenshot 2 times out or is manually dismissed.
        assert_eq!(
            captures
                .iter()
                .rev()
                .map(|entry| entry.0)
                .collect::<Vec<_>>(),
            [3, 4, 5, 6]
        );
        assert_eq!(
            stack_slots(&[false; 4], 5),
            vec![Some(3), Some(2), Some(1), Some(0)]
        );
    }

    #[test]
    fn overflow_keeps_pins_and_full_pin_dock_rejects_new_preview_not_a_hidden_backlog() {
        assert_eq!(
            stack_slots(&[false, false, false, true, false, true], 5),
            vec![Some(4), Some(3), Some(2), Some(1), None, Some(0)]
        );
        assert_eq!(
            stack_slots(&[false, true, true], 2),
            vec![None, Some(1), Some(0)]
        );
        assert!(stack_slots(&[], 5).is_empty());
    }

    #[test]
    fn fifo_head_is_oldest_surviving_unpinned_card_and_skips_pins() {
        let pinned = [false, false, false, true, false, true];
        assert_eq!(fifo_head(&pinned, &stack_slots(&pinned, 5)), Some(2));
        assert_eq!(
            fifo_head(&[false; 5], &stack_slots(&[false; 5], 5)),
            Some(4)
        );
        assert_eq!(fifo_head(&[true; 5], &stack_slots(&[true; 5], 5)), None);
        assert_eq!(fifo_head(&[], &[]), None);
    }
}
