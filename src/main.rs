mod capture;
mod cleanup;
mod drag_drop;
mod geometry;
mod overlay;
mod storage;
mod thumbnail;
mod worker;

use std::{
    cell::Cell,
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

thread_local! { static EXIT_REQUESTED: Cell<bool> = const { Cell::new(false) }; }

fn exit_requested() -> bool {
    EXIT_REQUESTED.get()
}

enum WorkResult {
    Captured(Result<capture::Snapshot>),
    Saved(Result<thumbnail::SavedScreenshot>),
}

struct App {
    controller: HWND,
    receiver: Receiver<WorkResult>,
    worker: worker::Worker,
    pending: bool,
    overlay: Option<overlay::ActiveOverlay>,
    thumbnail: Option<thumbnail::Thumbnail>,
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
        // Remove the old preview before capture and wait for DWM to present
        // its removal. The PNG must not include our previous thumbnail.
        if self.clear_thumbnail() {
            unsafe {
                DwmFlush()?;
            }
        }
        self.started = Instant::now();
        self.worker.capture(monitor)?;
        self.pending = true;
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
                let snapshot = result?;
                self.overlay = Some(overlay::ActiveOverlay::create(self.controller, snapshot)?);
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
                self.clear_thumbnail();
                self.thumbnail = Some(thumbnail::Thumbnail::create(
                    self.controller,
                    Rc::clone(&self.compositor),
                    image,
                )?);
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
            anyhow::bail!("{error}");
        }
        let Some(region) = region else {
            drop(overlay);
            println!("Selection canceled");
            return Ok(());
        };
        let snapshot = overlay.into_snapshot();
        println!(
            "Selected {} x {} physical pixels",
            region.width, region.height
        );
        self.started = Instant::now();
        self.worker.save(snapshot, region)?;
        self.pending = true;
        Ok(())
    }
}
impl App {
    fn begin_drag(&mut self, source: WPARAM) -> Result<()> {
        if !self
            .thumbnail
            .as_ref()
            .is_some_and(|thumbnail| thumbnail.matches(source))
        {
            return Ok(());
        }
        // Move the owner out of App before entering OLE's reentrant message loop.
        // The stable window state stays alive; no callback borrows the App.
        let mut thumbnail = self.thumbnail.take().expect("thumbnail checked");
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
        match result {
            Ok(drag_drop::Outcome::Copied) => {
                println!("Drag result: copied");
                drop(thumbnail);
            }
            Ok(drag_drop::Outcome::Canceled) => {
                println!("Drag result: canceled or rejected");
                self.thumbnail = Some(thumbnail);
                self.schedule_thumbnail_timer()?;
            }
            Err(error) => {
                self.thumbnail = Some(thumbnail);
                self.schedule_thumbnail_timer()?;
                return Err(error);
            }
        }
        Ok(())
    }

    fn clear_thumbnail(&mut self) -> bool {
        unsafe {
            let _ = KillTimer(Some(self.controller), THUMBNAIL_TIMER);
        }
        self.thumbnail.take().is_some()
    }

    fn schedule_thumbnail_timer(&mut self) -> Result<()> {
        unsafe {
            let _ = KillTimer(Some(self.controller), THUMBNAIL_TIMER);
        }
        if let Some(delay) = self
            .thumbnail
            .as_ref()
            .and_then(|thumbnail| thumbnail.next_wake())
        {
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
        let Some(thumbnail) = self.thumbnail.as_mut() else {
            return Ok(());
        };
        if source.is_some_and(|source| !thumbnail.matches(source)) {
            return Ok(());
        }
        let result = if dismiss {
            thumbnail.dismiss()
        } else {
            thumbnail.tick()
        };
        match result {
            Ok(true) => {
                self.clear_thumbnail();
            }
            Ok(false) => self.schedule_thumbnail_timer()?,
            Err(error) => {
                self.clear_thumbnail();
                return Err(error);
            }
        }
        Ok(())
    }
}

impl Drop for App {
    fn drop(&mut self) {
        self.overlay.take();
        self.clear_thumbnail();
        self.worker.shutdown();
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
    if message == WM_HOTKEY && wparam.0 == QUIT_HOTKEY as usize {
        // DoDragDrop dispatches directly to this callback rather than our outer
        // loop. IDropSource observes this flag and cancels safely before exit.
        EXIT_REQUESTED.set(true);
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
            Some(HWND_MESSAGE),
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
        receiver,
        worker,
        pending: false,
        overlay: None,
        thumbnail: None,
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
    println!("Simple Screenshot floating-thumbnail prototype");
    println!("Alt + Shift + S: select a region on the monitor under the pointer");
    println!("Esc / right-click: cancel. Ctrl + Alt + Q: quit.");
    println!("Output: %LOCALAPPDATA%\\SimpleScreenshot\\Temp\\");
    println!(
        "Preview: drag to copy a file, hover to pause, right-click to dismiss. Timing is provisional."
    );
    let mut message = MSG::default();
    loop {
        let status = unsafe { GetMessageW(&mut message, None, 0, 0) }.0;
        if status == -1 {
            return Err(windows::core::Error::from_thread().into());
        }
        if status == 0 {
            break;
        }
        if message.hwnd == controller {
            let result = match message.message {
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
                _ => {
                    unsafe {
                        DispatchMessageW(&message);
                    }
                    Ok(())
                }
            };
            if let Err(error) = result {
                show_error(&error);
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
