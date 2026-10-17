//! Background GitHub Releases checker. Never downloads on the UI thread or runs
//! an unverified installer. Installing is an explicit tray action.
use std::{
    fs::{self, File},
    io::{Read, Write},
    path::{Path, PathBuf},
    sync::mpsc::{self, Receiver},
    thread,
    time::Duration,
};

use anyhow::{Context, Result, bail, ensure};
use reqwest::blocking::Client;
use semver::Version;
use serde::Deserialize;
use sha2::{Digest, Sha256};
use windows::Win32::{
    Foundation::{HWND, LPARAM, WPARAM},
    UI::WindowsAndMessaging::PostMessageW,
};

pub const READY: u32 = windows::Win32::UI::WindowsAndMessaging::WM_APP + 19;
const API: &str = "https://api.github.com/repos/AxelS27/catch-it/releases/latest";
const ASSET_ROOT: &str = "https://github.com/AxelS27/catch-it/releases/download/";
const MAX_INSTALLER: u64 = 150 * 1024 * 1024;
const INTERVAL: Duration = Duration::from_secs(6 * 60 * 60);

#[derive(Deserialize)]
struct Release {
    tag_name: String,
    assets: Vec<Asset>,
}
#[derive(Deserialize)]
struct Asset {
    name: String,
    browser_download_url: String,
}

pub struct Update {
    pub version: Version,
    pub installer: PathBuf,
    checksum: [u8; 32],
}

pub fn start(hwnd: HWND) -> Receiver<Result<Option<Update>, String>> {
    let (sender, receiver) = mpsc::channel();
    let hwnd = hwnd.0 as isize;
    thread::spawn(move || {
        // Network outages and missing release assets must never block capture.
        loop {
            let result = check().map_err(|error| format!("{error:#}"));
            if let Err(error) = &result {
                eprintln!("Update check: {error}");
            }
            if sender.send(result).is_err() {
                break;
            }
            if unsafe { PostMessageW(Some(HWND(hwnd as *mut _)), READY, WPARAM(0), LPARAM(0)) }
                .is_err()
            {
                break;
            }
            thread::sleep(INTERVAL);
        }
    });
    receiver
}

fn installed() -> bool {
    std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(|p| p.join("unins000.exe")))
        .is_some_and(|path| path.is_file())
}

pub fn start_if_installed(hwnd: HWND) -> Option<Receiver<Result<Option<Update>, String>>> {
    installed().then(|| start(hwnd))
}

fn http_client(timeout: Duration) -> Result<Client> {
    Ok(Client::builder()
        .connect_timeout(Duration::from_secs(10))
        .timeout(timeout)
        .user_agent(concat!("CatchIt/", env!("CARGO_PKG_VERSION")))
        .build()?)
}

fn check() -> Result<Option<Update>> {
    let client = http_client(Duration::from_secs(60))?;
    let release: Release = client.get(API).send()?.error_for_status()?.json()?;
    ensure!(release.tag_name.len() <= 64, "Release tag is too long");
    let version = Version::parse(release.tag_name.trim_start_matches('v'))?;
    if version <= Version::parse(env!("CARGO_PKG_VERSION"))? {
        return Ok(None);
    }
    let name = format!("CatchIt-Setup-{version}.exe");
    let checksum_name = format!("{name}.sha256");
    let asset = release
        .assets
        .iter()
        .find(|a| a.name == name)
        .context("Release has no matching Windows installer")?;
    let checksum = release
        .assets
        .iter()
        .find(|a| a.name == checksum_name)
        .context("Release has no installer SHA-256 checksum")?;
    let response = client
        .get(asset_url(checksum, &release.tag_name)?)
        .send()?
        .error_for_status()?;
    let mut hash = Vec::new();
    response.take(256).read_to_end(&mut hash)?;
    ensure!(hash.len() < 256, "Checksum file too large");
    let expected = parse_checksum(std::str::from_utf8(&hash)?, &name)?;
    let dir = std::env::var_os("LOCALAPPDATA").context("LOCALAPPDATA unavailable")?;
    let dir = PathBuf::from(dir).join("CatchIt").join("Updates");
    fs::create_dir_all(&dir)?;
    let path = dir.join(&name);
    if path.is_file() && verify(&path, &expected).is_ok() {
        return Ok(Some(Update {
            version,
            installer: path,
            checksum: expected,
        }));
    }
    let partial = dir.join(format!("{name}.partial"));
    let result = (|| {
        // A slow connection should not fail every large update at 60 seconds.
        let download = http_client(Duration::from_secs(10 * 60))?;
        let mut response = download
            .get(asset_url(asset, &release.tag_name)?)
            .send()?
            .error_for_status()?;
        ensure!(
            response
                .content_length()
                .is_none_or(|size| size <= MAX_INSTALLER),
            "Installer too large"
        );
        let mut output = File::create(&partial)?;
        let mut bytes = 0;
        let mut buffer = [0u8; 64 * 1024];
        loop {
            let count = response.read(&mut buffer)?;
            if count == 0 {
                break;
            }
            bytes += count as u64;
            ensure!(bytes <= MAX_INSTALLER, "Installer too large");
            output.write_all(&buffer[..count])?;
        }
        output.flush()?;
        drop(output);
        verify(&partial, &expected)?;
        if path.exists() {
            fs::remove_file(&path)?;
        }
        fs::rename(&partial, &path)?;
        Ok::<_, anyhow::Error>(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&partial);
    }
    result?;
    Ok(Some(Update {
        version,
        installer: path,
        checksum: expected,
    }))
}

fn asset_url<'a>(asset: &'a Asset, tag: &str) -> Result<&'a str> {
    let prefix = format!("{ASSET_ROOT}{tag}/");
    ensure!(
        asset.browser_download_url == format!("{prefix}{}", asset.name),
        "Unexpected release asset URL"
    );
    Ok(&asset.browser_download_url)
}

fn parse_checksum(text: &str, name: &str) -> Result<[u8; 32]> {
    let mut fields = text.split_whitespace();
    let hex = fields.next().context("Missing SHA-256")?;
    ensure!(
        fields.next() == Some(name) && fields.next().is_none(),
        "Unexpected checksum file"
    );
    let bytes = hex.as_bytes();
    ensure!(bytes.len() == 64, "Invalid SHA-256 length");
    let mut hash = [0; 32];
    for (i, pair) in bytes.chunks_exact(2).enumerate() {
        hash[i] = (pair[0] as char).to_digit(16).context("Invalid SHA-256")? as u8 * 16
            + (pair[1] as char).to_digit(16).context("Invalid SHA-256")? as u8;
    }
    Ok(hash)
}

fn verify(path: &Path, expected: &[u8; 32]) -> Result<()> {
    let mut file = File::open(path)?;
    ensure!(
        file.metadata()?.len() <= MAX_INSTALLER,
        "Installer too large"
    );
    let mut hash = Sha256::new();
    std::io::copy(&mut file, &mut hash)?;
    if hash.finalize().as_slice() != expected {
        bail!("Installer checksum mismatch");
    }
    Ok(())
}

pub fn install(update: &Update) -> Result<()> {
    let dir = std::env::current_exe()?
        .parent()
        .context("Executable has no directory")?
        .to_path_buf();
    ensure!(
        dir.join("unins000.exe").is_file(),
        "Updates require an installed copy"
    );
    verify(&update.installer, &update.checksum)?;
    // Inno's Restart Manager can close the current executable if it has not
    // finished exiting; the installer launches the new version when done.
    // /DIR preserves the exact installation location.
    std::process::Command::new(&update.installer)
        .arg("/SILENT") // Progress and installer errors remain visible.
        .arg("/NORESTART")
        .arg("/CLOSEAPPLICATIONS")
        .arg(format!("/DIR={}", dir.display()))
        .spawn()
        .context("Cannot launch update installer")?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn checksum_requires_exact_name_and_valid_hash() {
        let name = "CatchIt-Setup-0.2.0.exe";
        let value = format!("{}  {name}\n", "ab".repeat(32));
        assert_eq!(parse_checksum(&value, name).unwrap(), [0xab; 32]);
        assert!(parse_checksum(&value, "other.exe").is_err());
        assert!(parse_checksum("oops  CatchIt-Setup-0.2.0.exe", name).is_err());
    }
    #[test]
    fn release_asset_url_must_match_tag_and_filename() {
        let asset = Asset {
            name: "foo.exe".into(),
            browser_download_url: format!("{ASSET_ROOT}v0.2.0/foo.exe"),
        };
        assert!(asset_url(&asset, "v0.2.0").is_ok());
        assert!(asset_url(&asset, "v0.3.0").is_err());
    }
}
