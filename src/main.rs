#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod capture;
mod cleanup;
mod clipboard;
mod drag_drop;
mod editor;
mod export;
mod gallery;
mod geometry;
mod overlay;
mod settings;
mod storage;
mod theme;
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
const CLIPBOARD_TIMER: usize = 2;

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
    EditorLoaded(usize, Result<storage::Raster>),
}

struct PendingClipboard {
    image: clipboard::Image,
    expires: Instant,
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
    clipboard: Option<PendingClipboard>,
    editors: Vec<editor::Editor>,
    editor_capture_focus: Option<(HWND, HWND)>,
}

impl App {
    fn capture_hidden(&mut self, hidden: bool) -> Result<bool> {
        let foreground = unsafe { GetForegroundWindow() };
        let mut visible = self.gallery.capture_hidden(hidden)?;
        let active_editor = self
            .editors
            .iter()
            .any(|editor| editor.hwnd() == foreground);
        for editor in &mut self.editors {
            visible |= editor.capture_hidden(hidden)?;
        }
        if hidden && active_editor {
            self.editor_capture_focus = Some((foreground, unsafe { GetForegroundWindow() }));
        } else if !hidden && let Some((editor, fallback)) = self.editor_capture_focus.take() {
            // Restore intentional editor focus only if the user hasn't switched apps.
            if unsafe { GetForegroundWindow() } == fallback
                && self.editors.iter().any(|e| e.hwnd() == editor)
            {
                unsafe {
                    let _ = SetForegroundWindow(editor);
                }
            }
        }
        Ok(visible)
    }

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
        if self.capture_hidden(true)? {
            unsafe {
                DwmFlush()?;
            }
        }
        self.started = Instant::now();
        if let Err(error) = self.worker.capture(monitor) {
            self.capture_hidden(false)?;
            self.schedule_thumbnail_timer()?;
            return Err(error);
        }
        self.pending = true;
        self.schedule_thumbnail_timer()?;
        Ok(())
    }

    fn receive_work(&mut self) -> Result<()> {
        // Nested loops coalesce wake messages. Drain every completion, including
        // editor loads closed before completion, rather than losing a second result.
        let mut first_error = None;
        while let Ok(result) = self.receiver.try_recv() {
            if let Err(error) = self.receive_result(result) {
                first_error.get_or_insert(error);
            }
        }
        first_error.map_or(Ok(()), Err)
    }

    fn receive_result(&mut self, result: WorkResult) -> Result<()> {
        if !matches!(&result, WorkResult::EditorLoaded(..)) {
            self.pending = false;
        }
        match result {
            WorkResult::EditorLoaded(id, result) => {
                if let Some(index) = self.editors.iter().position(|e| e.matches(WPARAM(id))) {
                    match result.and_then(|image| self.editors[index].load(image)) {
                        Ok(()) => (),
                        Err(error) => {
                            let editor = self.editors.remove(index);
                            self.gallery.set_editing(editor.path(), false)?;
                            self.schedule_thumbnail_timer()?;
                            return Err(error);
                        }
                    }
                }
            }
            WorkResult::Captured(result) => {
                let snapshot = match result {
                    Ok(snapshot) => snapshot,
                    Err(error) => {
                        self.capture_hidden(false)?;
                        self.schedule_thumbnail_timer()?;
                        return Err(error);
                    }
                };
                match overlay::ActiveOverlay::create(self.controller, snapshot) {
                    Ok(overlay) => self.overlay = Some(overlay),
                    Err(error) => {
                        self.capture_hidden(false)?;
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
                // Publish even if preview rendering later fails. A clipboard
                // error must not prevent the saved PNG's thumbnail from appearing.
                let clipboard =
                    clipboard::Image::new(&image.path, &image.pixels, image.width, image.height)
                        .and_then(|image| self.queue_clipboard(image));
                let preview = thumbnail::Thumbnail::create(
                    self.controller,
                    Rc::clone(&self.compositor),
                    image,
                    self.gallery.timeout(),
                )?;
                self.gallery.insert(preview)?;
                self.capture_hidden(false)?;
                self.schedule_thumbnail_timer()?;
                println!("Preview ready in {} ms", self.started.elapsed().as_millis());
                clipboard?;
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
            self.capture_hidden(false)?;
            self.schedule_thumbnail_timer()?;
            anyhow::bail!("{error}");
        }
        let Some(region) = region else {
            drop(overlay);
            self.capture_hidden(false)?;
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
            self.capture_hidden(false)?;
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
        // A copied Quick Access card stays in the gallery just long enough
        // for its leftward exit animation; the lifecycle timer removes it.
        self.gallery.restore(index, thumbnail);
        self.gallery.reflow()?;
        self.schedule_thumbnail_timer()?;
        result.map(|_| ())
    }

    fn save_thumbnail(&mut self, source: WPARAM) -> Result<()> {
        let Some((index, mut thumbnail)) = self.gallery.take(source) else {
            return Ok(());
        };
        self.gallery.pause_all()?;
        thumbnail.set_paused(true)?;
        unsafe {
            let _ = KillTimer(Some(self.controller), THUMBNAIL_TIMER);
        }
        let previous = unsafe { GetForegroundWindow() };
        let result = export::save_as(self.controller, thumbnail.path());
        unsafe {
            if GetForegroundWindow() == self.controller {
                let _ = SetForegroundWindow(previous);
            }
        }
        if EXIT_REQUESTED.get() {
            return Ok(());
        }
        if matches!(result, Ok(Some(_))) && !thumbnail.pinned() {
            thumbnail.dismiss()?;
        }
        self.gallery.restore(index, thumbnail);
        self.gallery.reflow()?;
        self.schedule_thumbnail_timer()?;
        if let Ok(Some(path)) = &result {
            println!("Exported: {}", path.display());
        }
        result.map(|_| ())
    }

    fn annotate_thumbnail(&mut self, source: WPARAM) -> Result<()> {
        let Some(item) = self.gallery.get(source) else {
            return Ok(());
        };
        let path = item.path().to_path_buf();
        if let Some(editor) = self.editors.iter().find(|editor| editor.path() == path) {
            editor.activate();
            return Ok(());
        }
        anyhow::ensure!(
            self.editors.len() < 4,
            "Close an editor before opening another (four concurrent editors maximum)"
        );
        let editor = editor::Editor::create(self.controller, HWND(source.0 as *mut _), &path)?;
        self.worker.load_editor(editor.id(), path.clone())?;
        self.editors.push(editor);
        self.gallery.set_editing(&path, true)?;
        self.schedule_thumbnail_timer()
    }

    fn editor_action(&mut self, source: WPARAM, action: isize) -> Result<()> {
        let Some(index) = self
            .editors
            .iter()
            .position(|editor| editor.matches(source))
        else {
            return Ok(());
        };
        if action == editor::CLOSE {
            println!(
                "Editor close requested: {}",
                self.editors[index].path().display()
            );
            let editor = self.editors.remove(index);
            self.gallery.set_editing(editor.path(), false)?;
            drop(editor);
            return self.schedule_thumbnail_timer();
        }
        if action == editor::COPY {
            let editor = &self.editors[index];
            let image = editor.image().context("Screenshot is still opening")?;
            let clipboard =
                clipboard::Image::new(editor.path(), &image.pixels, image.width, image.height)?;
            return self.queue_clipboard(clipboard);
        }
        if action == editor::ERROR {
            if let Some(error) = self.editors[index].take_error() {
                anyhow::bail!("{error}");
            }
            return Ok(());
        }
        // Take the owner out before entering native modal/reentrant loops.
        self.gallery.pause_all()?;
        let mut editor = self.editors.remove(index);
        unsafe {
            let _ = KillTimer(Some(self.controller), THUMBNAIL_TIMER);
        }
        let result = match action {
            editor::SAVE => export::save_as(editor.hwnd(), editor.path()).map(|_| ()),
            editor::DRAG => editor.run_drag().map(|_| ()),
            editor::ZOOM_MENU => editor.zoom_menu(),
            _ => Ok(()),
        };
        self.editors.insert(index, editor);
        self.gallery.reflow()?;
        self.schedule_thumbnail_timer()?;
        result
    }

    fn copy_thumbnail(&mut self, source: WPARAM) -> Result<()> {
        let Some(item) = self.gallery.get(source) else {
            return Ok(());
        };
        let image = clipboard::Image::from_png(item.path())?;
        self.queue_clipboard(image)
    }

    fn queue_clipboard(&mut self, image: clipboard::Image) -> Result<()> {
        self.clipboard = Some(PendingClipboard {
            image,
            expires: Instant::now() + std::time::Duration::from_secs(2),
        });
        self.publish_clipboard()
    }

    fn publish_clipboard(&mut self) -> Result<()> {
        unsafe {
            let _ = KillTimer(Some(self.controller), CLIPBOARD_TIMER);
        }
        let Some(mut pending) = self.clipboard.take() else {
            return Ok(());
        };
        if pending.image.publish(self.controller)? {
            println!("Clipboard: original PNG and native image copied");
        } else {
            anyhow::ensure!(
                Instant::now() < pending.expires,
                "Clipboard is busy. Screenshot is saved; use Copy to retry."
            );
            self.clipboard = Some(pending);
            if unsafe { SetTimer(Some(self.controller), CLIPBOARD_TIMER, 50, None) } == 0 {
                self.clipboard.take();
                anyhow::bail!("Cannot schedule clipboard retry");
            }
        }
        Ok(())
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
            && let Err(recovery) = self.capture_hidden(false)
        {
            eprintln!("Preview recovery failed: {recovery:#}");
        }
        if let Err(recovery) = self.schedule_thumbnail_timer() {
            eprintln!("Preview timer recovery failed: {recovery:#}");
        }
        show_error(error);
    }

    fn refresh_theme(&mut self) -> Result<()> {
        let dark = theme::dark();
        self.gallery.refresh_theme(dark)?;
        for editor in &mut self.editors {
            editor.refresh_theme(dark);
        }
        Ok(())
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
        unsafe {
            let _ = KillTimer(Some(self.controller), CLIPBOARD_TIMER);
        }
        self.clipboard.take();
        self.editors.clear();
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
        export::cancel_dialog();
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
        WM_SETTINGCHANGE
            | WM_THEMECHANGED
            | WORK_READY
            | overlay::FINISH_SELECTION
            | thumbnail::HOVER_CHANGED
            | thumbnail::DISMISS
            | thumbnail::BEGIN_DRAG
            | thumbnail::PIN
            | thumbnail::CONTEXT_MENU
            | thumbnail::SAVE
            | thumbnail::COPY
            | thumbnail::ANNOTATE
            | editor::ACTION
            | tray::SET_TIMEOUT_REQUEST
            | tray::CLOSE_ALL_REQUEST
    ) || (message == WM_TIMER && matches!(wparam.0, THUMBNAIL_TIMER | CLIPBOARD_TIMER))
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
    editor::register_class()?;
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
        clipboard: None,
        editors: Vec::new(),
        editor_capture_focus: None,
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
        "Preview: auto-copy image, hover for Copy/Save/pin/close, drag to copy file. Timing is provisional."
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
                WM_SETTINGCHANGE | WM_THEMECHANGED => app.refresh_theme(),
                overlay::FINISH_SELECTION => app.finish_selection(),
                WM_TIMER if message.wParam.0 == THUMBNAIL_TIMER => {
                    app.update_thumbnail(None, false)
                }
                thumbnail::HOVER_CHANGED => app.update_thumbnail(Some(message.wParam), false),
                thumbnail::DISMISS => app.update_thumbnail(Some(message.wParam), true),
                thumbnail::BEGIN_DRAG => app.begin_drag(message.wParam),
                thumbnail::PIN => app.pin_thumbnail(message.wParam),
                thumbnail::CONTEXT_MENU => app.context_menu(message.wParam),
                thumbnail::SAVE => app.save_thumbnail(message.wParam),
                thumbnail::COPY => app.copy_thumbnail(message.wParam),
                thumbnail::ANNOTATE => app.annotate_thumbnail(message.wParam),
                editor::ACTION => app.editor_action(message.wParam, message.lParam.0),
                WM_TIMER if message.wParam.0 == CLIPBOARD_TIMER => app.publish_clipboard(),
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
        for (message, source, action) in [
            (WORK_READY, 0, 0),
            (WORK_READY, 0, 0),
            (WM_TIMER, THUMBNAIL_TIMER, 0),
            (WM_TIMER, THUMBNAIL_TIMER, 0),
            (thumbnail::HOVER_CHANGED, 10, 0),
            (thumbnail::HOVER_CHANGED, 10, 0),
            (thumbnail::HOVER_CHANGED, 20, 0),
            (editor::ACTION, 31, editor::COPY),
            (editor::ACTION, 31, editor::COPY),
            (editor::ACTION, 31, editor::CLOSE),
        ] {
            unsafe {
                controller_proc(HWND::default(), message, WPARAM(source), LPARAM(action));
            }
        }
        DEFERRED_MESSAGES.with(|queue| {
            let mut queue = queue.borrow_mut();
            let messages: Vec<_> = queue
                .drain(..)
                .map(|m| (m.message, m.wParam.0, m.lParam.0))
                .collect();
            assert_eq!(
                messages,
                vec![
                    (WORK_READY, 0, 0),
                    (WM_TIMER, THUMBNAIL_TIMER, 0),
                    (thumbnail::HOVER_CHANGED, 10, 0),
                    (thumbnail::HOVER_CHANGED, 20, 0),
                    (editor::ACTION, 31, editor::COPY),
                    (editor::ACTION, 31, editor::CLOSE),
                ]
            );
        });
    }
}
