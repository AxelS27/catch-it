//! Background GitHub Releases checker. Never downloads on the UI thread or runs
//! an unverified installer. Installing is an explicit tray action.
use std::{
    fs::{self, File},
    io::{Read, Write},
    path::{Path, PathBuf},
    sync::mpsc::{self, Receiver, RecvTimeoutError, SyncSender, TrySendError},
    thread,
    time::Duration,
};

use anyhow::{Context, Result, bail, ensure};
use reqwest::blocking::Client;
use semver::Version;
use serde::Deserialize;
use sha2::{Digest, Sha256};
use windows::{
    Win32::{
        Foundation::{CloseHandle, HANDLE, HWND, LPARAM, WAIT_ABANDONED, WAIT_OBJECT_0, WPARAM},
        System::Threading::{CreateMutexW, ReleaseMutex, WaitForSingleObject},
        UI::WindowsAndMessaging::PostMessageW,
    },
    core::w,
};

pub const READY: u32 = windows::Win32::UI::WindowsAndMessaging::WM_APP + 19;
const API: &str = "https://api.github.com/repos/AxelS27/catch-it/releases/latest";
const ASSET_ROOT: &str = "https://github.com/AxelS27/catch-it/releases/download/";
const MAX_INSTALLER: u64 = 150 * 1024 * 1024;
const INTERVAL: Duration = Duration::from_secs(6 * 60 * 60);

// Duplicate app processes share the same per-user cache. An OS mutex survives
// crashes without leaving a stale lock file in the writable Updates directory.
struct CacheLock(HANDLE);
impl CacheLock {
    fn acquire() -> Result<Self> {
        unsafe {
            let handle = CreateMutexW(None, false, w!("Local\\CatchIt.ReleaseCache"))?;
            // The other instance may be downloading a 150 MiB installer with
            // a ten-minute HTTP deadline. Waiting here does not block the UI.
            match WaitForSingleObject(handle, 11 * 60 * 1000) {
                WAIT_OBJECT_0 | WAIT_ABANDONED => Ok(Self(handle)),
                status => {
                    let _ = CloseHandle(handle);
                    bail!("Updater cache lock unavailable ({})", status.0)
                }
            }
        }
    }
}
impl Drop for CacheLock {
    fn drop(&mut self) {
        unsafe {
            let _ = ReleaseMutex(self.0);
            let _ = CloseHandle(self.0);
        }
    }
}

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

pub struct Updater {
    results: Receiver<Result<Option<Update>, String>>,
    requests: SyncSender<()>,
}

impl Updater {
    pub fn try_recv(
        &self,
    ) -> std::result::Result<Result<Option<Update>, String>, mpsc::TryRecvError> {
        self.results.try_recv()
    }
    pub fn check_now(&self) -> Result<()> {
        match self.requests.try_send(()) {
            Ok(()) | Err(TrySendError::Full(())) => Ok(()),
            Err(TrySendError::Disconnected(())) => anyhow::bail!("Updater has stopped"),
        }
    }
}

pub fn start(hwnd: HWND) -> Updater {
    let (sender, receiver) = mpsc::channel();
    let (requests, commands) = mpsc::sync_channel(1);
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
            match commands.recv_timeout(INTERVAL) {
                Ok(()) | Err(RecvTimeoutError::Timeout) => (),
                Err(RecvTimeoutError::Disconnected) => break,
            }
        }
    });
    Updater {
        results: receiver,
        requests,
    }
}

fn installed() -> bool {
    std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(|p| p.join("unins000.exe")))
        .is_some_and(|path| path.is_file())
}

pub fn start_if_installed(hwnd: HWND) -> Option<Updater> {
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
    let download = http_client(Duration::from_secs(10 * 60))?;
    let dir = std::env::var_os("LOCALAPPDATA").context("LOCALAPPDATA unavailable")?;
    let dir = PathBuf::from(dir).join("CatchIt").join("Updates");
    check_with(
        &client,
        &download,
        API,
        ASSET_ROOT,
        &dir,
        &Version::parse(env!("CARGO_PKG_VERSION"))?,
    )
}

// End-to-end testable with an isolated local release server; production cannot
// override the pinned GitHub endpoint or installed version at runtime.
fn check_with(
    client: &Client,
    download: &Client,
    api: &str,
    asset_root: &str,
    dir: &Path,
    current: &Version,
) -> Result<Option<Update>> {
    let release: Release = client.get(api).send()?.error_for_status()?.json()?;
    ensure!(release.tag_name.len() <= 64, "Release tag is too long");
    let version = Version::parse(release.tag_name.trim_start_matches('v'))?;
    if &version <= current {
        let _lock = CacheLock::acquire()?;
        prune_cache(dir, None);
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
        .get(asset_url(checksum, &release.tag_name, asset_root)?)
        .send()?
        .error_for_status()?;
    let mut hash = Vec::new();
    response.take(256).read_to_end(&mut hash)?;
    ensure!(hash.len() < 256, "Checksum file too large");
    let expected = parse_checksum(std::str::from_utf8(&hash)?, &name)?;
    fs::create_dir_all(dir)?;
    let _lock = CacheLock::acquire()?;
    let path = dir.join(&name);
    if path.is_file() && verify(&path, &expected).is_ok() {
        prune_cache(dir, Some(&path));
        return Ok(Some(Update {
            version,
            installer: path,
            checksum: expected,
        }));
    }
    let partial = dir.join(format!("{name}.partial"));
    let result = (|| {
        // A slow connection should not fail every large update at 60 seconds.
        let mut response = download
            .get(asset_url(asset, &release.tag_name, asset_root)?)
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
    prune_cache(dir, Some(&path));
    Ok(Some(Update {
        version,
        installer: path,
        checksum: expected,
    }))
}

// Only remove files with the exact generated installer naming scheme; unrelated
// files or directories in the user-owned folder are never touched. Cache cleanup
// is best-effort and must not turn a valid update into a capture-blocking error.
fn prune_cache(dir: &Path, keep: Option<&Path>) {
    let entries = match fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return,
        Err(error) => {
            eprintln!("Update cache cleanup: {error}");
            return;
        }
    };
    for entry in entries {
        let result = (|| -> std::io::Result<()> {
            let entry = entry?;
            let path = entry.path();
            if keep.is_some_and(|keep| keep == path) || !entry.file_type()?.is_file() {
                return Ok(());
            }
            let Some(name) = entry.file_name().to_str().map(str::to_owned) else {
                return Ok(());
            };
            let Some(stem) = name.strip_prefix("CatchIt-Setup-") else {
                return Ok(());
            };
            let Some(stem) = stem
                .strip_suffix(".exe")
                .or_else(|| stem.strip_suffix(".exe.partial"))
            else {
                return Ok(());
            };
            if !Version::parse(stem).is_ok_and(|version| version.to_string() == stem) {
                return Ok(());
            }
            fs::remove_file(path)
        })();
        if let Err(error) = result {
            eprintln!("Update cache cleanup: {error}");
        }
    }
}

fn asset_url<'a>(asset: &'a Asset, tag: &str, root: &str) -> Result<&'a str> {
    let prefix = format!("{root}{tag}/");
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
    for (i, pair) in bytes.as_chunks::<2>().0.iter().enumerate() {
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
    use std::{
        io::{BufRead, BufReader},
        net::TcpListener,
        time::{Instant, SystemTime, UNIX_EPOCH},
    };

    struct TestFolder(PathBuf);
    impl TestFolder {
        fn new() -> Self {
            let stamp = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos();
            let path = std::env::temp_dir().join(format!(
                "catch-it-update-test-{}-{stamp}",
                std::process::id()
            ));
            fs::create_dir(&path).unwrap();
            Self(path)
        }
    }
    impl Drop for TestFolder {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn release_server(
        installer: &'static [u8],
        expected: String,
        requests: usize,
        reported_installer_size: Option<u64>,
    ) -> (String, thread::JoinHandle<()>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let root = format!("http://{}/", listener.local_addr().unwrap());
        let base = root.clone();
        let worker = thread::spawn(move || {
            let name = "CatchIt-Setup-0.1.2.exe";
            let path = format!("/download/v0.1.2/{name}");
            let manifest = format!(
                "{{\"tag_name\":\"v0.1.2\",\"assets\":[{{\"name\":\"{name}\",\"browser_download_url\":\"{base}download/v0.1.2/{name}\"}},{{\"name\":\"{name}.sha256\",\"browser_download_url\":\"{base}download/v0.1.2/{name}.sha256\"}}]}}"
            );
            let hash = format!("{expected}  {name}\n");
            let deadline = Instant::now() + Duration::from_secs(5);
            let mut handled = 0;
            while handled < requests && Instant::now() < deadline {
                let (mut socket, _) = match listener.accept() {
                    Ok(accepted) => accepted,
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                        thread::sleep(Duration::from_millis(5));
                        continue;
                    }
                    Err(error) => panic!("Test server failed: {error}"),
                };
                // Winsock can inherit the listener's nonblocking mode on an
                // accepted socket. Read complete requests in blocking mode.
                socket.set_nonblocking(false).unwrap();
                socket
                    .set_read_timeout(Some(Duration::from_secs(3)))
                    .unwrap();
                let mut first = String::new();
                {
                    let mut reader = BufReader::new(&mut socket);
                    reader.read_line(&mut first).unwrap();
                    loop {
                        let mut line = String::new();
                        reader.read_line(&mut line).unwrap();
                        if line == "\r\n" || line.is_empty() {
                            break;
                        }
                    }
                }
                let request = first.split_whitespace().nth(1).unwrap_or("");
                let body: &[u8] = if request == "/latest" {
                    manifest.as_bytes()
                } else if request == format!("{path}.sha256") {
                    hash.as_bytes()
                } else if request == path {
                    installer
                } else {
                    panic!("Unexpected request: {request}");
                };
                let length = if request == path {
                    reported_installer_size.unwrap_or(body.len() as u64)
                } else {
                    body.len() as u64
                };
                write!(
                    socket,
                    "HTTP/1.1 200 OK\r\nContent-Length: {length}\r\nConnection: close\r\n\r\n"
                )
                .unwrap();
                socket.write_all(body).unwrap();
                handled += 1;
            }
            assert_eq!(
                handled, requests,
                "Updater requested the wrong number of assets"
            );
        });
        (root, worker)
    }

    fn local_client() -> Client {
        Client::builder()
            .no_proxy()
            .timeout(Duration::from_secs(3))
            .build()
            .unwrap()
    }

    #[test]
    fn downloads_verifies_caches_and_replaces_corrupt_cached_installer() {
        let folder = TestFolder::new();
        let old = folder.0.join("CatchIt-Setup-0.1.0.exe");
        let abandoned = folder.0.join("CatchIt-Setup-0.1.0.exe.partial");
        let unrelated = folder.0.join("personal.png");
        let directory = folder.0.join("CatchIt-Setup-0.0.1.exe");
        fs::write(&old, b"old installer").unwrap();
        fs::write(&abandoned, b"old partial").unwrap();
        fs::write(&unrelated, b"untouched").unwrap();
        fs::create_dir(&directory).unwrap();
        let bytes = b"fixture installer for updater";
        let hash = format!("{:x}", Sha256::digest(bytes));
        let (root, server) = release_server(bytes, hash, 8, None);
        let client = local_client();
        let current = Version::new(0, 1, 1);
        let check = || {
            check_with(
                &client,
                &client,
                &format!("{root}latest"),
                &format!("{root}download/"),
                &folder.0,
                &current,
            )
            .unwrap()
            .unwrap()
        };
        let first = check();
        assert_eq!(first.version, Version::new(0, 1, 2));
        assert_eq!(fs::read(&first.installer).unwrap(), bytes);
        assert!(!old.exists());
        assert!(!abandoned.exists());
        assert_eq!(fs::read(&unrelated).unwrap(), b"untouched");
        assert!(directory.is_dir());
        assert_eq!(check().installer, first.installer); // No second binary transfer.
        fs::write(&first.installer, b"changed after verification").unwrap();
        assert_eq!(fs::read(check().installer).unwrap(), bytes);
        server.join().unwrap();
    }

    #[test]
    fn up_to_date_check_removes_obsolete_cache_without_downloading() {
        let folder = TestFolder::new();
        let old = folder.0.join("CatchIt-Setup-0.1.0.exe");
        let unrelated = folder.0.join("personal.png");
        fs::write(&old, b"old").unwrap();
        fs::write(&unrelated, b"keep").unwrap();
        let (root, server) = release_server(b"not fetched", "00".repeat(32), 1, None);
        let client = local_client();
        assert!(
            check_with(
                &client,
                &client,
                &format!("{root}latest"),
                &format!("{root}download/"),
                &folder.0,
                &Version::new(0, 1, 2)
            )
            .unwrap()
            .is_none()
        );
        assert!(!old.exists());
        assert_eq!(fs::read(unrelated).unwrap(), b"keep");
        server.join().unwrap();
    }

    #[test]
    fn two_instances_download_to_shared_cache_only_once() {
        let folder = TestFolder::new();
        let bytes = b"shared installer";
        let hash = format!("{:x}", Sha256::digest(bytes));
        let (root, server) = release_server(bytes, hash, 5, None);
        let client = local_client();
        let run = || {
            check_with(
                &client,
                &client,
                &format!("{root}latest"),
                &format!("{root}download/"),
                &folder.0,
                &Version::new(0, 1, 1),
            )
            .unwrap()
            .unwrap()
        };
        let (first, second) = thread::scope(|scope| {
            let first = scope.spawn(run);
            let second = scope.spawn(run);
            (first.join().unwrap(), second.join().unwrap())
        });
        assert_eq!(first.installer, second.installer);
        assert_eq!(fs::read(first.installer).unwrap(), bytes);
        server.join().unwrap();
    }

    #[test]
    fn rejects_corrupted_download_without_publishing_executable() {
        let folder = TestFolder::new();
        let hash = format!("{:x}", Sha256::digest(b"expected installer"));
        let (root, server) = release_server(b"bad installer", hash, 3, None);
        let client = local_client();
        assert!(
            check_with(
                &client,
                &client,
                &format!("{root}latest"),
                &format!("{root}download/"),
                &folder.0,
                &Version::new(0, 1, 1)
            )
            .is_err()
        );
        assert!(!folder.0.join("CatchIt-Setup-0.1.2.exe").exists());
        assert!(!folder.0.join("CatchIt-Setup-0.1.2.exe.partial").exists());
        server.join().unwrap();
    }
    #[test]
    fn repeated_manual_checks_coalesce_without_blocking_ui() {
        let (requests, queue) = mpsc::sync_channel(1);
        let (_sender, results) = mpsc::channel();
        let updater = Updater { results, requests };
        updater.check_now().unwrap();
        updater.check_now().unwrap();
        assert!(queue.try_recv().is_ok());
        assert!(queue.try_recv().is_err());
    }

    #[test]
    fn refuses_oversized_installer_before_writing_any_file() {
        let folder = TestFolder::new();
        let hash = format!("{:x}", Sha256::digest(b"small installer"));
        let (root, server) = release_server(b"small installer", hash, 3, Some(MAX_INSTALLER + 1));
        let client = local_client();
        assert!(
            check_with(
                &client,
                &client,
                &format!("{root}latest"),
                &format!("{root}download/"),
                &folder.0,
                &Version::new(0, 1, 1)
            )
            .is_err()
        );
        assert!(!folder.0.join("CatchIt-Setup-0.1.2.exe").exists());
        assert!(!folder.0.join("CatchIt-Setup-0.1.2.exe.partial").exists());
        server.join().unwrap();
    }

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
        assert!(asset_url(&asset, "v0.2.0", ASSET_ROOT).is_ok());
        assert!(asset_url(&asset, "v0.3.0", ASSET_ROOT).is_err());
    }
}
