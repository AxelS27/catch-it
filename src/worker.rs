//! One sleeping worker owns the GPU session and handles PNG encoding off the UI thread.
use std::{
    sync::mpsc::{self, Sender},
    thread::{self, JoinHandle},
};

use anyhow::{Context, Result};
use windows::Win32::{
    Foundation::{HWND, LPARAM, WPARAM},
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
                        let result = (|| {
                            if session.as_ref().is_none_or(|s| s.monitor() != monitor) {
                                session = None;
                                session = Some(CaptureSession::new(monitor)?);
                            }
                            session
                                .as_mut()
                                .context("Capture session is unavailable")?
                                .capture()
                        })();
                        // Access loss, display reconfiguration, or timeout: recreate on
                        // the next request, rather than retaining a broken session.
                        if result.is_err() {
                            session = None;
                        }
                        WorkResult::Captured(result)
                    }
                    Command::Save(snapshot, region) => WorkResult::Saved((|| {
                        let pixels = snapshot.crop(region)?;
                        let path = storage::save_png(&pixels, region.width, region.height)?;
                        Ok(SavedScreenshot {
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

    pub fn shutdown(&mut self) {
        if let Some(thread) = self.thread.take() {
            let _ = self.commands.send(Command::Stop);
            let _ = thread.join();
        }
    }
}
impl Drop for Worker {
    fn drop(&mut self) {
        self.shutdown();
    }
}
