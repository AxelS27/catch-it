#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod capture;
mod cleanup;
mod drag_drop;
mod gallery;
mod geometry;
mod overlay;
mod settings;
mod storage;
mod thumbnail;
mod tray;
mod worker;

use std::{
    cell::{Cell, RefCell},
    collections::VecDeque,
    rc::Rc,
    sync::mpsc::{self, Receiver},
    time::Instant,
};

use anyhow::{Context, Result};
use windows::{
    Win32::{
        Foundation::*,
        Graphics::{
            Dwm::DwmFlush,
            Gdi::{MONITOR_DEFAULTTONEAREST, MonitorFromPoint},
        },
        System::LibraryLoader::GetModuleHandleW,
        UI::{HiDpi::*, Input::KeyboardAndMouse::*, WindowsAndMessaging::*},
    },
    core::{PCWSTR, w},
};

const CAPTURE_HOTKEY: i32 = 1;
const QUIT_HOTKEY: i32 = 2;
const WORK_READY: u32 = WM_APP + 1;
const THUMBNAIL_TIMER: usize = 1;

thread_local! {
    static EXIT_REQUESTED: Cell<bool> = const { Cell::new(false) };
    static CAPTURE_REQUESTED: Cell<bool> = const { Cell::new(false) };
    static DEFERRED_MESSAGES: RefCell<VecDeque<MSG>> = const { RefCell::new(VecDeque::new()) };
}

fn exit_requested() -> bool {
    EXIT_REQUESTED.get()
}

enum WorkResult {
    Captured(Result<capture::Snapshot>),
    Saved(Result<thumbnail::SavedScreenshot>),
}

struct App {
    controller: HWND,
    tray: Option<Box<tray::Tray>>,
    receiver: Receiver<WorkResult>,
    worker: worker::Worker,
    pending: bool,
    overlay: Option<overlay::ActiveOverlay>,
    gallery: gallery::Gallery,
    compositor: Rc<thumbnail::Compositor>,
    started: Instant,
}

impl App {
    fn start_capture(&mut self) -> Result<()> {
        if self.pending || self.overlay.is_some() {
            return Ok(());
        }
        let mut pointer = POINT::default();
        unsafe {
            GetCursorPos(&mut pointer)?;
        }
        let monitor = unsafe { MonitorFromPoint(pointer, MONITOR_DEFAULTTONEAREST) }.0 as isize;
        // Hide all pending/pinned cards, without discarding them, before DXGI.
        // Their clocks pause for the entire selection and PNG publication.
        if self.gallery.capture_hidden(true)? {
            unsafe {
                DwmFlush()?;
            }
        }
        self.started = Instant::now();
        if let Err(error) = self.worker.capture(monitor) {
            self.gallery.capture_hidden(false)?;
            self.schedule_thumbnail_timer()?;
            return Err(error);
        }
        self.pending = true;
        self.schedule_thumbnail_timer()?;
        Ok(())
    }

    fn receive_work(&mut self) -> Result<()> {
        let result = self
            .receiver
            .try_recv()
            .context("Capture worker returned no result")?;
        self.pending = false;
        match result {
            WorkResult::Captured(result) => {
                let snapshot = match result {
                    Ok(snapshot) => snapshot,
                    Err(error) => {
                        self.gallery.capture_hidden(false)?;
                        self.schedule_thumbnail_timer()?;
                        return Err(error);
                    }
                };
                match overlay::ActiveOverlay::create(self.controller, snapshot) {
                    Ok(overlay) => self.overlay = Some(overlay),
                    Err(error) => {
                        self.gallery.capture_hidden(false)?;
                        self.schedule_thumbnail_timer()?;
                        return Err(error);
                    }
                }
                println!(
                    "Selection ready in {} ms",
                    self.started.elapsed().as_millis()
                );
            }
            WorkResult::Saved(result) => {
                let image = result?;
                println!(
                    "Saved in {} ms: {}",
                    self.started.elapsed().as_millis(),
                    image.path.display()
                );
                let preview = thumbnail::Thumbnail::create(
                    self.controller,
                    Rc::clone(&self.compositor),
                    image,
                    self.gallery.timeout(),
                )?;
                self.gallery.insert(preview)?;
                self.schedule_thumbnail_timer()?;
                println!("Preview ready in {} ms", self.started.elapsed().as_millis());
            }
        }
        Ok(())
    }

    fn finish_selection(&mut self) -> Result<()> {
        let Some(mut overlay) = self.overlay.take() else {
            return Ok(());
        };
        let (region, error) = overlay.result();
        if let Some(error) = error {
            drop(overlay);
            self.gallery.capture_hidden(false)?;
            self.schedule_thumbnail_timer()?;
            anyhow::bail!("{error}");
        }
        let Some(region) = region else {
            drop(overlay);
            self.gallery.capture_hidden(false)?;
            self.schedule_thumbnail_timer()?;
            println!("Selection canceled");
            return Ok(());
        };
        let snapshot = overlay.into_snapshot();
        println!(
            "Selected {} x {} physical pixels",
            region.width, region.height
        );
        self.started = Instant::now();
        if let Err(error) = self.worker.save(snapshot, region) {
            self.gallery.capture_hidden(false)?;
            self.schedule_thumbnail_timer()?;
            return Err(error);
        }
        self.pending = true;
        Ok(())
    }
}
impl App {
    fn begin_drag(&mut self, source: WPARAM) -> Result<()> {
        let Some((index, mut thumbnail)) = self.gallery.take(source) else {
            return Ok(());
        };
        // Owner out of the collection before OLE's modal/reentrant loop.
        self.gallery.pause_all()?;
        unsafe {
            let _ = KillTimer(Some(self.controller), THUMBNAIL_TIMER);
        }
        let result = thumbnail.run_drag();
        if EXIT_REQUESTED.replace(false) {
            drop(thumbnail);
            unsafe {
                PostQuitMessage(0);
            }
            return Ok(());
        }
        match &result {
            Ok(drag_drop::Outcome::Copied) => println!("Drag result: copied"),
            Ok(drag_drop::Outcome::Canceled) => println!("Drag result: canceled or rejected"),
            Err(_) => (),
        }
        if thumbnail.pinned() || !matches!(result, Ok(drag_drop::Outcome::Copied)) {
            self.gallery.restore(index, thumbnail);
        }
        self.gallery.reflow()?;
        self.schedule_thumbnail_timer()?;
        result.map(|_| ())
    }

    fn clear_thumbnail(&mut self) -> bool {
        unsafe {
            let _ = KillTimer(Some(self.controller), THUMBNAIL_TIMER);
        }
        let had_items = self.gallery.len() > 0;
        self.gallery.clear();
        if let Some(tray) = &self.tray {
            tray.update(self.gallery.timeout(), 0);
        }
        had_items
    }

    fn schedule_thumbnail_timer(&mut self) -> Result<()> {
        unsafe {
            let _ = KillTimer(Some(self.controller), THUMBNAIL_TIMER);
        }
        if let Some(tray) = &self.tray {
            tray.update(self.gallery.timeout(), self.gallery.len());
        }
        if let Some(delay) = self.gallery.next_wake() {
            let millis = delay
                .as_millis()
                .saturating_add(1)
                .clamp(1, u32::MAX as u128) as u32;
            let timer = unsafe { SetTimer(Some(self.controller), THUMBNAIL_TIMER, millis, None) };
            if timer == 0 {
                let error = windows::core::Error::from_thread();
                self.clear_thumbnail();
                return Err(error).context("Cannot schedule thumbnail timer");
            }
        }
        Ok(())
    }

    fn update_thumbnail(&mut self, source: Option<WPARAM>, dismiss: bool) -> Result<()> {
        self.gallery.tick(source, dismiss)?;
        self.schedule_thumbnail_timer()
    }

    fn pin_thumbnail(&mut self, source: WPARAM) -> Result<()> {
        self.gallery.toggle_pin(source)?;
        self.schedule_thumbnail_timer()
    }

    fn context_menu(&mut self, source: WPARAM) -> Result<()> {
        self.gallery.context_menu(source, self.controller)?;
        self.schedule_thumbnail_timer()
    }

    fn report_error(&mut self, error: &anyhow::Error) {
        // Failed capture/PNG/render work must not strand older previews hidden
        // or leave their resumed clocks without a timer.
        if !self.pending
            && self.overlay.is_none()
            && let Err(recovery) = self.gallery.capture_hidden(false)
        {
            eprintln!("Preview recovery failed: {recovery:#}");
        }
        if let Err(recovery) = self.schedule_thumbnail_timer() {
            eprintln!("Preview timer recovery failed: {recovery:#}");
        }
        show_error(error);
    }

    fn configure_timeout(&mut self, index: usize) -> Result<()> {
        let Some(timeout) = settings::AutoClose::ALL.get(index).copied() else {
            return Ok(());
        };
        timeout.save()?;
        self.gallery.configure(timeout)?;
        self.schedule_thumbnail_timer()
    }
}

impl Drop for App {
    fn drop(&mut self) {
        self.overlay.take();
        self.clear_thumbnail();
        self.worker.shutdown();
        self.tray.take();
        unsafe {
            let _ = UnregisterHotKey(Some(self.controller), CAPTURE_HOTKEY);
            let _ = UnregisterHotKey(Some(self.controller), QUIT_HOTKEY);
            let _ = DestroyWindow(self.controller);
        }
    }
}

fn show_error(error: &anyhow::Error) {
    eprintln!("{error:#}");
    let message: Vec<u16> = format!("{error:#}").encode_utf16().chain(Some(0)).collect();
    unsafe {
        MessageBoxW(
            None,
            PCWSTR(message.as_ptr()),
            w!("Simple Screenshot"),
            MB_OK | MB_ICONERROR,
        );
    }
}

unsafe extern "system" fn controller_proc(
    hwnd: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    if message == tray::QUIT_REQUEST || (message == WM_HOTKEY && wparam.0 == QUIT_HOTKEY as usize) {
        // DoDragDrop dispatches directly to this callback rather than our outer
        // loop. IDropSource observes this flag and cancels safely before exit.
        EXIT_REQUESTED.set(true);
        // Also unwind a tray popup's nested loop. OLE observes the same flag.
        unsafe {
            let _ = EndMenu();
        }
        return LRESULT(0);
    }
    if message == tray::CAPTURE_REQUEST
        || (message == WM_HOTKEY && wparam.0 == CAPTURE_HOTKEY as usize)
    {
        // Defer capture until the nested menu/drag loop unwinds. Never reenter App.
        CAPTURE_REQUESTED.set(true);
        unsafe {
            let _ = EndMenu();
        }
        return LRESULT(0);
    }
    if matches!(
        message,
        WORK_READY
            | overlay::FINISH_SELECTION
            | thumbnail::HOVER_CHANGED
            | thumbnail::DISMISS
            | thumbnail::BEGIN_DRAG
            | thumbnail::PIN
            | thumbnail::CONTEXT_MENU
            | tray::SET_TIMEOUT_REQUEST
            | tray::CLOSE_ALL_REQUEST
    ) || (message == WM_TIMER && wparam.0 == THUMBNAIL_TIMER)
    {
        // Modal Windows loops dispatch messages directly, bypassing run(). Keep
        // worker completions and lifecycle changes for the outer App loop.
        DEFERRED_MESSAGES.with(|queue| {
            let mut queue = queue.borrow_mut();
            if !queue
                .iter()
                .any(|m| m.message == message && m.wParam == wparam && m.lParam == lparam)
            {
                queue.push_back(MSG {
                    hwnd,
                    message,
                    wParam: wparam,
                    lParam: lparam,
                    ..Default::default()
                });
            }
        });
        return LRESULT(0);
    }
    if message == WM_COMMAND && gallery::handle_menu_command(hwnd, wparam, lparam) {
        return LRESULT(0);
    }
    // Tray callbacks can run inside TrackPopupMenu or DoDragDrop. This stable
    // immutable owner never borrows App and is detached before window teardown.
    let tray = unsafe { GetWindowLongPtrW(hwnd, GWLP_USERDATA) } as *const tray::Tray;
    if !tray.is_null() && unsafe { &*tray }.handle(message, wparam, lparam) {
        return LRESULT(0);
    }
    unsafe { DefWindowProcW(hwnd, message, wparam, lparam) }
}

fn run() -> Result<()> {
    // Must be set before creating any window or reading virtualized coordinates.
    unsafe {
        SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2)?;
    }
    let _ole = drag_drop::OleApartment::new()?;
    let _cleanup = cleanup::CleanupWorker::start(storage::temp_directory()?)?;
    overlay::register_class()?;
    thumbnail::register_class()?;
    drag_drop::register_class()?;
    let compositor = thumbnail::Compositor::new()?;
    let controller = unsafe {
        let class = WNDCLASSW {
            lpfnWndProc: Some(controller_proc),
            hInstance: GetModuleHandleW(None)?.into(),
            lpszClassName: w!("SimpleScreenshot.Controller"),
            ..Default::default()
        };
        anyhow::ensure!(
            RegisterClassW(&class) != 0,
            "Cannot register controller window"
        );
        CreateWindowExW(
            WINDOW_EX_STYLE::default(),
            class.lpszClassName,
            w!("Simple Screenshot"),
            WINDOW_STYLE::default(),
            0,
            0,
            0,
            0,
            None, // Hidden top-level controller receives TaskbarCreated broadcasts.
            None,
            Some(class.hInstance),
            None,
        )?
    };
    let (sender, receiver) = mpsc::channel();
    let mut pointer = POINT::default();
    unsafe {
        GetCursorPos(&mut pointer)?;
    }
    let initial_monitor = unsafe { MonitorFromPoint(pointer, MONITOR_DEFAULTTONEAREST) }.0 as isize;
    let worker = worker::Worker::new(controller, initial_monitor, sender);
    let mut app = App {
        controller,
        tray: None,
        receiver,
        worker,
        pending: false,
        overlay: None,
        gallery: gallery::Gallery::new(settings::AutoClose::load()),
        compositor,
        started: Instant::now(),
    };
    unsafe {
        RegisterHotKey(
            Some(controller),
            CAPTURE_HOTKEY,
            MOD_ALT | MOD_SHIFT | MOD_NOREPEAT,
            u32::from(b'S'),
        )
        .context("Alt + Shift + S is already registered by another application")?;
        RegisterHotKey(
            Some(controller),
            QUIT_HOTKEY,
            MOD_CONTROL | MOD_ALT | MOD_NOREPEAT,
            u32::from(b'Q'),
        )
        .context("Prototype exit shortcut Ctrl + Alt + Q is unavailable")?;
    }
    app.tray = Some(tray::Tray::new(controller, app.gallery.timeout())?);
    println!("Simple Screenshot - running in the notification area");
    println!("Alt + Shift + S: select a region on the monitor under the pointer");
    println!("Esc / right-click: cancel. Ctrl + Alt + Q: quit.");
    println!("Output: %LOCALAPPDATA%\\SimpleScreenshot\\Temp\\");
    println!(
        "Preview: drag to copy, hover for pin/close, right-click for actions. Timing is provisional."
    );
    let mut message = MSG::default();
    loop {
        // A quit hotkey may have been dispatched by a tray menu's modal loop.
        if EXIT_REQUESTED.replace(false) {
            break;
        }
        if CAPTURE_REQUESTED.replace(false)
            && let Err(error) = app.start_capture()
        {
            app.report_error(&error);
        }
        let status = if let Some(deferred) =
            DEFERRED_MESSAGES.with(|queue| queue.borrow_mut().pop_front())
        {
            message = deferred;
            1
        } else {
            unsafe { GetMessageW(&mut message, None, 0, 0) }.0
        };
        if status == -1 {
            return Err(windows::core::Error::from_thread().into());
        }
        if status == 0 {
            break;
        }
        if message.hwnd == controller {
            let result = match message.message {
                tray::CAPTURE_REQUEST => app.start_capture(),
                tray::QUIT_REQUEST => break,
                WM_HOTKEY if message.wParam.0 == CAPTURE_HOTKEY as usize => app.start_capture(),
                WM_HOTKEY if message.wParam.0 == QUIT_HOTKEY as usize => break,
                WORK_READY => app.receive_work(),
                overlay::FINISH_SELECTION => app.finish_selection(),
                WM_TIMER if message.wParam.0 == THUMBNAIL_TIMER => {
                    app.update_thumbnail(None, false)
                }
                thumbnail::HOVER_CHANGED => app.update_thumbnail(Some(message.wParam), false),
                thumbnail::DISMISS => app.update_thumbnail(Some(message.wParam), true),
                thumbnail::BEGIN_DRAG => app.begin_drag(message.wParam),
                thumbnail::PIN => app.pin_thumbnail(message.wParam),
                thumbnail::CONTEXT_MENU => app.context_menu(message.wParam),
                tray::SET_TIMEOUT_REQUEST => app.configure_timeout(message.wParam.0),
                tray::CLOSE_ALL_REQUEST => {
                    app.clear_thumbnail();
                    Ok(())
                }
                _ => {
                    unsafe {
                        DispatchMessageW(&message);
                    }
                    Ok(())
                }
            };
            if let Err(error) = result {
                app.report_error(&error);
            }
        } else {
            unsafe {
                let _ = TranslateMessage(&message);
                DispatchMessageW(&message);
            }
        }
    }
    Ok(())
}

fn main() {
    if let Err(error) = run() {
        show_error(&error);
        std::process::exit(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nested_loop_preserves_completions_and_coalesces_lifecycle_messages() {
        DEFERRED_MESSAGES.with(|queue| queue.borrow_mut().clear());
        for (message, source) in [
            (WORK_READY, 0),
            (WM_TIMER, THUMBNAIL_TIMER),
            (WM_TIMER, THUMBNAIL_TIMER),
            (thumbnail::HOVER_CHANGED, 10),
            (thumbnail::HOVER_CHANGED, 10),
            (thumbnail::HOVER_CHANGED, 20),
        ] {
            unsafe {
                controller_proc(HWND::default(), message, WPARAM(source), LPARAM(0));
            }
        }
        DEFERRED_MESSAGES.with(|queue| {
            let mut queue = queue.borrow_mut();
            let messages: Vec<_> = queue.drain(..).map(|m| (m.message, m.wParam.0)).collect();
            assert_eq!(
                messages,
                vec![
                    (WORK_READY, 0),
                    (WM_TIMER, THUMBNAIL_TIMER),
                    (thumbnail::HOVER_CHANGED, 10),
                    (thumbnail::HOVER_CHANGED, 20),
                ]
            );
        });
    }
}
