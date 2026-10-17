//! One sleeping worker owns the GPU session and handles PNG encoding off the UI thread.
use std::{
    sync::mpsc::{self, Sender},
    thread::{self, JoinHandle},
    time::Duration,
};

use anyhow::{Context, Result};
use windows::Win32::{
    Foundation::{HWND, LPARAM, WPARAM},
    Graphics::Dxgi::{DXGI_ERROR_ACCESS_LOST, DXGI_ERROR_WAIT_TIMEOUT},
    UI::WindowsAndMessaging::PostMessageW,
};

use crate::{
    WORK_READY, WorkResult,
    capture::{CaptureSession, Snapshot},
    geometry::{Point, Region},
    storage,
    thumbnail::SavedScreenshot,
};

enum Command {
    Capture(isize),
    Save(Snapshot, Region),
    LoadEditor(usize, std::path::PathBuf),
    Stop,
}

pub struct Worker {
    commands: Sender<Command>,
    thread: Option<JoinHandle<()>>,
}

impl Worker {
    pub fn new(controller: HWND, initial_monitor: isize, results: Sender<WorkResult>) -> Self {
        let (commands, receiver) = mpsc::channel();
        let hwnd = controller.0 as isize;
        let thread = thread::spawn(move || {
            // Warm the GPU session without blocking startup or acquiring idle frames.
            let mut session = CaptureSession::new(initial_monitor).ok();
            while let Ok(command) = receiver.recv() {
                let result = match command {
                    Command::Capture(monitor) => {
                        WorkResult::Captured(capture_with_recovery(&mut session, monitor))
                    }
                    Command::Save(snapshot, region) => WorkResult::Saved((|| {
                        let pixels = snapshot.crop(region)?;
                        let path = storage::save_png(&pixels, region.width, region.height)?;
                        let protection = storage::protect_png(&path)?;
                        Ok(SavedScreenshot {
                            protection,
                            path,
                            origin: Point {
                                x: snapshot.left + region.x as i32,
                                y: snapshot.top + region.y as i32,
                            },
                            width: region.width,
                            height: region.height,
                            pixels,
                        })
                    })()),
                    Command::LoadEditor(id, path) => {
                        WorkResult::EditorLoaded(id, storage::load_png(&path))
                    }
                    Command::Stop => break,
                };
                if results.send(result).is_err() {
                    break;
                }
                unsafe {
                    if PostMessageW(Some(HWND(hwnd as *mut _)), WORK_READY, WPARAM(0), LPARAM(0))
                        .is_err()
                    {
                        break;
                    }
                }
            }
        });
        Self {
            commands,
            thread: Some(thread),
        }
    }

    pub fn capture(&self, monitor: isize) -> Result<()> {
        self.commands
            .send(Command::Capture(monitor))
            .context("Capture worker stopped")
    }

    pub fn save(&self, snapshot: Snapshot, region: Region) -> Result<()> {
        self.commands
            .send(Command::Save(snapshot, region))
            .context("Capture worker stopped")
    }

    pub fn load_editor(&self, id: usize, path: std::path::PathBuf) -> Result<()> {
        self.commands
            .send(Command::LoadEditor(id, path))
            .context("Capture worker stopped")
    }

    pub fn shutdown(&mut self) {
        if let Some(thread) = self.thread.take() {
            let _ = self.commands.send(Command::Stop);
            let _ = thread.join();
        }
    }
}
// Full-screen transitions (including Valorant) and sign-in can invalidate an
// existing duplication. Recreate it on the same request, not only the next hotkey.
fn capture_with_recovery(session: &mut Option<CaptureSession>, monitor: isize) -> Result<Snapshot> {
    for attempt in 0..3 {
        let result = (|| {
            if session.as_ref().is_none_or(|s| s.monitor() != monitor) {
                *session = Some(CaptureSession::new(monitor)?);
            }
            session
                .as_mut()
                .context("Capture session is unavailable")?
                .capture()
        })();
        match result {
            Ok(snapshot) => return Ok(snapshot),
            Err(error) => {
                let retry = is_transient_capture_error(&error);
                *session = None; // Drop the invalid duplication before creating another.
                if !retry || attempt == 2 {
                    return Err(error);
                }
                thread::sleep(Duration::from_millis(200 * (attempt + 1)));
            }
        }
    }
    unreachable!("three attempts always return")
}

fn is_transient_capture_error(error: &anyhow::Error) -> bool {
    error
        .chain()
        .filter_map(|cause| cause.downcast_ref::<windows::core::Error>())
        .any(|cause| {
            matches!(
                cause.code(),
                DXGI_ERROR_ACCESS_LOST | DXGI_ERROR_WAIT_TIMEOUT
            )
        })
}

impl Drop for Worker {
    fn drop(&mut self) {
        self.shutdown();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use windows::Win32::Graphics::Dxgi::DXGI_ERROR_NOT_FOUND;

    #[test]
    fn recovers_only_transient_dxgi_failures_even_when_wrapped() {
        let lost = anyhow::Error::new(windows::core::Error::from(DXGI_ERROR_ACCESS_LOST))
            .context("No desktop frame available");
        assert!(is_transient_capture_error(&lost));
        let timeout = anyhow::Error::new(windows::core::Error::from(DXGI_ERROR_WAIT_TIMEOUT));
        assert!(is_transient_capture_error(&timeout));
        let unrelated = anyhow::Error::new(windows::core::Error::from(DXGI_ERROR_NOT_FOUND));
        assert!(!is_transient_capture_error(&unrelated));
    }
}
