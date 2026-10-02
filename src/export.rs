//! Native PNG Save As, with atomic publication outside the temporary source.
use anyhow::{Context, Result};
use std::{
    cell::RefCell,
    ffi::OsString,
    os::windows::ffi::OsStringExt,
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};
use windows::{
    Win32::{
        Foundation::*,
        System::{Com::*, Ole::IOleWindow},
        UI::{
            Shell::{Common::COMDLG_FILTERSPEC, *},
            WindowsAndMessaging::{PostMessageW, WM_CLOSE},
        },
    },
    core::{HRESULT, Interface, w},
};

thread_local! { static DIALOG: RefCell<Option<IFileSaveDialog>> = const { RefCell::new(None) }; }
static SEQUENCE: AtomicU64 = AtomicU64::new(0);

pub fn cancel_dialog() {
    let dialog = DIALOG.with(|active| active.borrow().clone());
    if let Some(dialog) = dialog {
        unsafe {
            // Post the native close gesture instead of reentrantly calling Close
            // while the shell is still initializing its just-visible dialog.
            let window = dialog
                .cast::<IOleWindow>()
                .and_then(|window| window.GetWindow());
            if let Ok(window) = window {
                let _ = PostMessageW(Some(window), WM_CLOSE, WPARAM(0), LPARAM(0));
            } else {
                let _ = dialog.Close(HRESULT::from_win32(ERROR_CANCELLED.0));
            }
        }
    }
}

pub fn save_as(owner: HWND, source: &Path) -> Result<Option<PathBuf>> {
    struct Active;
    impl Drop for Active {
        fn drop(&mut self) {
            DIALOG.with(|active| active.borrow_mut().take());
        }
    }
    unsafe {
        let dialog: IFileSaveDialog =
            CoCreateInstance(&FileSaveDialog, None, CLSCTX_INPROC_SERVER)?;
        dialog.SetTitle(w!("Save screenshot"))?;
        dialog.SetFileTypes(&[COMDLG_FILTERSPEC {
            pszName: w!("PNG image"),
            pszSpec: w!("*.png"),
        }])?;
        dialog.SetDefaultExtension(w!("png"))?;
        dialog.SetFileName(w!("Screenshot.png"))?;
        dialog.SetOptions(
            FOS_FORCEFILESYSTEM
                | FOS_PATHMUSTEXIST
                | FOS_OVERWRITEPROMPT
                | FOS_STRICTFILETYPES
                | FOS_NOCHANGEDIR,
        )?;
        DIALOG.with(|active| *active.borrow_mut() = Some(dialog.clone()));
        let _active = Active;
        match dialog.Show(Some(owner)) {
            Ok(()) => (),
            Err(error) if error.code() == HRESULT::from_win32(ERROR_CANCELLED.0) => {
                return Ok(None);
            }
            Err(error) => return Err(error).context("Cannot open Save As dialog"),
        }
        let name = dialog.GetResult()?.GetDisplayName(SIGDN_FILESYSPATH)?;
        let destination = PathBuf::from(OsString::from_wide(name.as_wide()));
        CoTaskMemFree(Some(name.0.cast()));
        save_copy(source, &destination)?;
        Ok(Some(destination))
    }
}

pub fn save_copy(source: &Path, destination: &Path) -> Result<()> {
    anyhow::ensure!(
        destination
            .extension()
            .is_some_and(|extension| extension.eq_ignore_ascii_case("png")),
        "Choose a .png filename for this PNG screenshot"
    );
    if destination.exists() {
        anyhow::ensure!(
            std::fs::canonicalize(source)? != std::fs::canonicalize(destination)?,
            "Choose a different location from the temporary source screenshot"
        );
    }
    let folder = destination
        .parent()
        .context("Save destination has no folder")?;
    let seq = SEQUENCE.fetch_add(1, Ordering::Relaxed);
    let pending = folder.join(format!(
        ".simple-screenshot-save-{}-{seq}.tmp",
        std::process::id()
    ));
    // Never truncate the chosen target. The temporary copy must be fully written
    // and closed before an atomic same-directory rename replaces an approved file.
    let file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&pending)
        .context("Cannot create screenshot in the selected folder")?;
    let result = (|| -> Result<()> {
        let mut file = file;
        let mut input = std::fs::File::open(source).context("Cannot read original screenshot")?;
        std::io::copy(&mut input, &mut file).context("Cannot write screenshot copy")?;
        file.sync_all()?;
        drop(file);
        std::fs::rename(&pending, destination).context("Cannot publish saved screenshot")?;
        Ok(())
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(&pending);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    struct Folder(PathBuf);
    impl Drop for Folder {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
    #[test]
    fn save_preserves_source_and_overwrites_only_with_complete_original_bytes() -> Result<()> {
        let folder = Folder(std::env::temp_dir().join(format!(
            "screenshot export 日本 {} {}",
            std::process::id(),
            SEQUENCE.fetch_add(1, Ordering::Relaxed)
        )));
        std::fs::create_dir(&folder.0)?;
        let source = folder.0.join("source.png");
        let destination = folder.0.join("saved 日本 image.png");
        std::fs::write(&source, b"original PNG bytes")?;
        let guard = crate::storage::protect_png(&source)?;
        std::fs::write(&destination, b"previous contents")?;
        save_copy(&source, &destination)?;
        assert_eq!(std::fs::read(&destination)?, std::fs::read(&source)?);
        assert!(save_copy(&source, &source).is_err());
        assert!(save_copy(&source, &folder.0.join("wrong.jpg")).is_err());
        assert!(save_copy(&source, &folder.0.join("absent/saved.png")).is_err());
        assert_eq!(std::fs::read_dir(&folder.0)?.count(), 2);
        drop(guard);
        Ok(())
    }
    #[test]
    fn failed_publication_keeps_existing_target_and_cleans_private_copy() -> Result<()> {
        let folder = Folder(std::env::temp_dir().join(format!(
            "screenshot export failure {} {}",
            std::process::id(),
            SEQUENCE.fetch_add(1, Ordering::Relaxed)
        )));
        std::fs::create_dir(&folder.0)?;
        let source = folder.0.join("source.png");
        let target = folder.0.join("target.png");
        std::fs::write(&source, b"new")?;
        std::fs::write(&target, b"keep")?;
        let locked = crate::storage::protect_png(&target)?;
        assert!(save_copy(&source, &target).is_err());
        assert_eq!(std::fs::read(&target)?, b"keep");
        assert_eq!(std::fs::read_dir(&folder.0)?.count(), 2);
        drop(locked);
        Ok(())
    }
}
