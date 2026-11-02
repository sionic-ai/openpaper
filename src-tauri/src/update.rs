//! In-app update: compares this build with the latest GitHub release, then installs it into
//! the folder this app was launched from. macOS reruns the one-line installer, detached.
//! Windows downloads the release's setup program, checks it, and runs it silently; the setup
//! closes the app, installs over it and reopens it.

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::time::Duration;

const REPO: &str = "sionic-ai/openpaper";
const TIMEOUT: Duration = Duration::from_secs(8);
/// Installer command for one release. The script comes from the release tag, not `main`, so a
/// change pushed to `main` never runs on users' machines before it is released. The tag is
/// rebuilt from parsed numbers, so no text from the network reaches the shell. The app path
/// reaches the installer only through `OPENPAPER_DIR`, so the command line never contains
/// `<DIR>/OpenPaper.app/Contents/MacOS/openpaper` and the installer's `pkill -f` on that path
/// cannot match the installer itself.
#[cfg(not(windows))]
fn install_command((major, minor, patch): (u64, u64, u64)) -> (String, String) {
    let tag = format!("v{major}.{minor}.{patch}");
    let cmd = format!(
        "curl -fsSL --proto '=https' https://raw.githubusercontent.com/{REPO}/{tag}/install.sh | bash"
    );
    (cmd, tag)
}
/// File name of the installer log, under the temp dir.
pub const LOG_NAME: &str = "openpaper-update.log";

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateInfo {
    pub current: String,
    pub latest: Option<String>,
    pub available: bool,
}

fn client() -> Option<reqwest::blocking::Client> {
    reqwest::blocking::Client::builder()
        .user_agent(concat!(
            "openpaper/",
            env!("CARGO_PKG_VERSION"),
            " (paper reader)"
        ))
        // Redirects are followed so a renamed repository still resolves, but only on GitHub.
        .redirect(reqwest::redirect::Policy::custom(|attempt| {
            if may_follow(attempt.url(), attempt.previous().len()) {
                attempt.follow()
            } else {
                attempt.stop()
            }
        }))
        .connect_timeout(TIMEOUT)
        .timeout(TIMEOUT)
        .build()
        .ok()
}

const MAX_HOPS: usize = 10;

/// A redirect is followed only over https to github.com or api.github.com, fewer than
/// `MAX_HOPS` times. `hops` counts the URLs already visited.
fn may_follow(url: &reqwest::Url, hops: usize) -> bool {
    hops < MAX_HOPS
        && url.scheme() == "https"
        && matches!(url.host_str(), Some("github.com" | "api.github.com"))
        && url.port().is_none()
}

/// `.../releases/tag/v1.2.3` → `1.2.3`.
fn tag_from_url(url: &str) -> Option<String> {
    let (_, tag) = url.rsplit_once("/releases/tag/")?;
    let tag = tag.split(['?', '#', '/']).next()?.trim();
    let tag = tag.strip_prefix('v').unwrap_or(tag);
    (!tag.is_empty()).then(|| tag.to_string())
}

/// `1.2.3`, `1.2`, `1.2.3-beta.1` → numeric (major, minor, patch). Pre-release and build
/// suffixes are ignored.
fn parse_version(v: &str) -> Option<(u64, u64, u64)> {
    let core = v.trim().trim_start_matches('v').split(['-', '+']).next()?;
    let mut parts = core.split('.').map(|p| p.parse::<u64>().ok());
    let major = parts.next()??;
    let minor = parts.next().unwrap_or(Some(0))?;
    let patch = parts.next().unwrap_or(Some(0))?;
    parts.next().is_none().then_some((major, minor, patch))
}

/// True only when `latest` is strictly newer than `current`.
pub fn is_newer(latest: &str, current: &str) -> bool {
    matches!((parse_version(latest), parse_version(current)), (Some(l), Some(c)) if l > c)
}

/// `releases/latest` redirects to `releases/tag/<tag>`; after following every redirect
/// (including one from a renamed repository) the final URL names the tag. The GitHub API,
/// also followed through redirects, is the fallback.
fn latest_tag() -> Option<String> {
    let http = client()?;
    let from_redirect = http
        .get(format!("https://github.com/{REPO}/releases/latest"))
        .send()
        .ok()
        .and_then(|r| tag_from_url(r.url().as_str()));
    if from_redirect.is_some() {
        return from_redirect;
    }
    #[derive(Deserialize)]
    struct Release {
        tag_name: String,
    }
    let release: Release = http
        .get(format!(
            "https://api.github.com/repos/{REPO}/releases/latest"
        ))
        .header(reqwest::header::ACCEPT, "application/vnd.github+json")
        .send()
        .ok()?
        .error_for_status()
        .ok()?
        .bytes()
        .ok()
        .and_then(|b| serde_json::from_slice(&b).ok())?;
    let tag = release.tag_name.trim();
    let tag = tag.strip_prefix('v').unwrap_or(tag);
    (!tag.is_empty()).then(|| tag.to_string())
}

/// Never fails: without network the answer is "no update known".
pub fn check() -> UpdateInfo {
    let current = env!("CARGO_PKG_VERSION").to_string();
    let latest = latest_tag();
    let available = latest
        .as_deref()
        .is_some_and(|l| is_newer(l, &current) && has_installer(l));
    UpdateInfo {
        current,
        latest,
        available,
    }
}

/// `<DIR>/OpenPaper.app/Contents/MacOS/openpaper` → `<DIR>`. `None` outside an `.app` bundle.
#[cfg(not(windows))]
pub fn install_dir_from_exe(exe: &Path) -> Option<PathBuf> {
    let macos = exe.parent()?;
    let contents = macos.parent()?;
    let app = contents.parent()?;
    let is_app = app
        .extension()
        .is_some_and(|e| e.eq_ignore_ascii_case("app"));
    (macos.file_name()? == "MacOS" && contents.file_name()? == "Contents" && is_app)
        .then(|| app.parent().map(Path::to_path_buf))
        .flatten()
        .filter(|d| !d.as_os_str().is_empty())
}

const DEV_BUILD: &str = "Development builds can't update from inside the app";

/// Installs the newest release when it is newer than this build. Checks again instead of
/// trusting the webview, so the installer only ever runs for a real, newer release tag.
pub fn install_latest(exe: &Path, log: &Path) -> Result<(), String> {
    install_dir_from_exe(exe).ok_or_else(|| DEV_BUILD.to_string())?;
    let info = check();
    let latest = info
        .latest
        .filter(|_| info.available)
        .and_then(|l| parse_version(&l))
        .ok_or_else(|| "No newer release found. Check your connection and try again".to_string())?;
    install(exe, log, latest)
}

/// Starts the installer for release `version` in its own process group so it outlives this
/// app, which the installer quits, replaces and reopens. Output is appended to `log`.
#[cfg(not(windows))]
pub fn install(exe: &Path, log: &Path, version: (u64, u64, u64)) -> Result<(), String> {
    use std::os::unix::process::CommandExt;
    use std::process::{Command, Stdio};
    let dir = install_dir_from_exe(exe).ok_or_else(|| DEV_BUILD.to_string())?;
    let (cmd, tag) = install_command(version);
    let out = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(log)
        .map_err(|_| "Couldn't create the update log".to_string())?;
    let err = out
        .try_clone()
        .map_err(|_| "Couldn't open the update log".to_string())?;
    let mut child = Command::new("/bin/bash")
        .arg("-c")
        .arg(cmd)
        .env("OPENPAPER_DIR", &dir)
        .env("OPENPAPER_VERSION", tag)
        .env("OPENPAPER_NO_CLAUDE", "1")
        .env("PATH", "/usr/bin:/bin:/usr/sbin:/sbin")
        .stdin(Stdio::null())
        .stdout(out)
        .stderr(err)
        .process_group(0)
        .spawn()
        .map_err(|_| "Couldn't start the update".to_string())?;
    // Reap the child if the app is still running when it ends.
    std::thread::spawn(move || {
        let _ = child.wait();
    });
    Ok(())
}

/// The setup program each release carries for Windows, made by `tauri build --bundles nsis`.
pub const WINDOWS_ASSET: &str = "OpenPaper-windows-x64-setup.exe";

/// The folder of an installed copy: the setup program puts `uninstall.exe` next to the app.
/// `None` for a development build.
#[cfg(windows)]
pub fn install_dir_from_exe(exe: &Path) -> Option<PathBuf> {
    let dir = exe.parent()?;
    dir.join("uninstall.exe")
        .is_file()
        .then(|| dir.to_path_buf())
}

#[cfg(windows)]
fn download_client(timeout: Duration) -> Option<reqwest::blocking::Client> {
    reqwest::blocking::Client::builder()
        .user_agent(concat!(
            "openpaper/",
            env!("CARGO_PKG_VERSION"),
            " (paper reader)"
        ))
        .redirect(reqwest::redirect::Policy::custom(|attempt| {
            if may_follow_download(attempt.url(), attempt.previous().len()) {
                attempt.follow()
            } else {
                attempt.stop()
            }
        }))
        .connect_timeout(TIMEOUT)
        .timeout(timeout)
        .build()
        .ok()
}

/// The Windows installer is uploaded a few minutes after a release is published, and older
/// releases have none. Until it is there, there is nothing to update to. GET, not HEAD: the
/// asset host signs its redirect URLs for GET only.
#[cfg(windows)]
fn has_installer(version: &str) -> bool {
    let Some((major, minor, patch)) = parse_version(version) else {
        return false;
    };
    let url = format!(
        "https://github.com/{REPO}/releases/download/v{major}.{minor}.{patch}/{WINDOWS_ASSET}.sha256"
    );
    download_client(TIMEOUT)
        .and_then(|http| http.get(url).send().ok())
        .is_some_and(|r| r.status().is_success())
}

/// macOS releases always carry the app; the release job fails before publishing otherwise.
#[cfg(not(windows))]
fn has_installer(_version: &str) -> bool {
    true
}

/// Release downloads redirect from github.com to GitHub's asset hosts.
#[cfg(any(windows, test))]
fn may_follow_download(url: &reqwest::Url, hops: usize) -> bool {
    hops < MAX_HOPS
        && url.scheme() == "https"
        && url.port().is_none()
        && url.host_str().is_some_and(|h| {
            h == "github.com"
                || h == "objects.githubusercontent.com"
                || h.ends_with(".githubusercontent.com")
        })
}

/// `<sha256 hex>  <name>` (as written by `sha256sum` or `shasum`) → the lowercase hex.
#[cfg(any(windows, test))]
fn checksum_of(file: &str) -> Option<String> {
    let hex = file.split_whitespace().next()?.to_ascii_lowercase();
    (hex.len() == 64 && hex.bytes().all(|b| b.is_ascii_hexdigit())).then_some(hex)
}

#[cfg(any(windows, test))]
fn sha256_hex(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

/// Arguments for Tauri's NSIS setup: silent (`/S`; it force-closes the running app), as an
/// update (`/UPDATE`; keeps the user's shortcut choices), and reopen the app when done (`/R`).
pub const SETUP_ARGS: [&str; 3] = ["/S", "/UPDATE", "/R"];

/// Downloads the setup program of release `version`, checks it against the release's
/// `.sha256`, and starts it detached with [`SETUP_ARGS`]. The setup closes this app, installs
/// over it and reopens it; the caller quits the app as well. A line is appended to `log`.
#[cfg(windows)]
pub fn install(
    exe: &Path,
    log: &Path,
    (major, minor, patch): (u64, u64, u64),
) -> Result<(), String> {
    use std::io::{Read, Write};
    use std::os::windows::process::CommandExt;
    use std::process::{Command, Stdio};
    const MAX_SETUP: u64 = 200 * 1024 * 1024;
    const DOWNLOAD_FAILED: &str =
        "Couldn't download the update. Check your connection and try again";
    install_dir_from_exe(exe).ok_or_else(|| DEV_BUILD.to_string())?;
    let tag = format!("v{major}.{minor}.{patch}");
    let url = format!("https://github.com/{REPO}/releases/download/{tag}/{WINDOWS_ASSET}");
    let http =
        download_client(Duration::from_secs(300)).ok_or_else(|| DOWNLOAD_FAILED.to_string())?;
    let fetch = |url: &str| -> Result<Vec<u8>, String> {
        let mut body = Vec::new();
        http.get(url)
            .send()
            .and_then(|r| r.error_for_status())
            .map_err(|_| DOWNLOAD_FAILED.to_string())?
            .take(MAX_SETUP)
            .read_to_end(&mut body)
            .map_err(|_| DOWNLOAD_FAILED.to_string())?;
        Ok(body)
    };
    let want = checksum_of(&String::from_utf8_lossy(&fetch(&format!("{url}.sha256"))?))
        .ok_or_else(|| DOWNLOAD_FAILED.to_string())?;
    let setup = fetch(&url)?;
    if sha256_hex(&setup) != want {
        return Err("The download does not match its checksum. Nothing was installed".to_string());
    }
    // A new folder and a new file, so nothing placed in the temp dir beforehand can be what runs.
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or_default();
    let dir = std::env::temp_dir().join(format!("openpaper-update-{}-{nanos}", std::process::id()));
    let saved = std::fs::create_dir(&dir).and_then(|()| {
        let mut f = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(dir.join(WINDOWS_ASSET))?;
        f.write_all(&setup)?;
        f.sync_all()
    });
    saved.map_err(|_| "Couldn't save the update".to_string())?;
    let path = dir.join(WINDOWS_ASSET);
    if let Ok(mut f) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(log)
    {
        let _ = writeln!(f, "Running {} {}", path.display(), SETUP_ARGS.join(" "));
    }
    const CREATE_NEW_PROCESS_GROUP: u32 = 0x0000_0200;
    Command::new(&path)
        .args(SETUP_ARGS)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .creation_flags(CREATE_NEW_PROCESS_GROUP)
        .spawn()
        .map_err(|_| "Couldn't start the update".to_string())?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_tag_from_url() {
        assert_eq!(
            tag_from_url("https://github.com/sionic-ai/openpaper/releases/tag/v0.1.2").as_deref(),
            Some("0.1.2")
        );
        assert_eq!(
            tag_from_url("/sionic-ai/openpaper/releases/tag/0.2.0?x=1").as_deref(),
            Some("0.2.0")
        );
        assert_eq!(
            tag_from_url("https://github.com/sionic-ai/openpaper/releases"),
            None
        );
    }

    #[test]
    fn reads_tag_from_followed_final_url() {
        // What `Response::url()` holds after following releases/latest, also across a rename.
        for final_url in [
            "https://github.com/sionic-ai/openpaper/releases/tag/v0.2.0",
            "https://github.com/someone/renamed-repo/releases/tag/v0.2.0#notes",
        ] {
            let url = url::Url::parse(final_url).unwrap();
            assert_eq!(tag_from_url(url.as_str()).as_deref(), Some("0.2.0"));
        }
        // Without any release GitHub lands on the releases list: no tag, so the API is tried.
        let url = url::Url::parse("https://github.com/sionic-ai/openpaper/releases").unwrap();
        assert_eq!(tag_from_url(url.as_str()), None);
    }

    #[test]
    fn redirects_stay_on_github() {
        let u = |s: &str| reqwest::Url::parse(s).unwrap();
        assert!(may_follow(
            &u("https://github.com/sionic-ai/openpaper/releases/tag/v0.2.0"),
            1
        ));
        assert!(may_follow(
            &u("https://api.github.com/repositories/1/releases/latest"),
            1
        ));
        for bad in [
            "http://github.com/x/y/releases/tag/v1",
            "https://evil.example/x/releases/tag/v1",
            "https://github.com.evil.example/x",
            "https://objects.githubusercontent.com/x",
            "https://github.com:8443/x/releases/tag/v1",
            "file:///etc/passwd",
        ] {
            assert!(!may_follow(&u(bad), 1), "{bad}");
        }
        assert!(may_follow(&u("https://github.com/x"), MAX_HOPS - 1));
        assert!(!may_follow(&u("https://github.com/x"), MAX_HOPS));
    }

    #[test]
    fn compares_versions_numerically() {
        assert!(is_newer("0.1.10", "0.1.9"));
        assert!(is_newer("1.0", "0.9.9"));
        assert!(!is_newer("0.1.2", "0.1.2"));
        assert!(!is_newer("0.1.1", "0.1.2"));
        assert!(!is_newer("0.1.2-beta", "0.1.2"));
        assert!(is_newer("v0.2.0-rc.1", "0.1.2"));
        assert!(!is_newer("garbage", "0.1.2"));
        assert!(!is_newer("1.2.3.4", "0.1.2"));
    }

    #[test]
    #[cfg(not(windows))]
    fn install_dir_needs_app_bundle() {
        assert_eq!(
            install_dir_from_exe(Path::new(
                "/Applications/OpenPaper.app/Contents/MacOS/openpaper"
            )),
            Some(PathBuf::from("/Applications"))
        );
        assert_eq!(
            install_dir_from_exe(Path::new(
                "/Users/me/Applications/OpenPaper.app/Contents/MacOS/openpaper"
            )),
            Some(PathBuf::from("/Users/me/Applications"))
        );
        assert_eq!(
            install_dir_from_exe(Path::new("/x/openpaper/src-tauri/target/debug/openpaper")),
            None
        );
        assert_eq!(
            install_dir_from_exe(Path::new(
                "/Applications/OpenPaper/Contents/MacOS/openpaper"
            )),
            None
        );
        assert_eq!(
            install_dir_from_exe(Path::new("OpenPaper.app/Contents/MacOS/openpaper")),
            None
        );
    }

    #[test]
    fn dev_build_cannot_self_update() {
        let log = std::env::temp_dir().join("openpaper-update-test.log");
        let exe = Path::new("/x/target/debug/openpaper");
        assert_eq!(install(exe, &log, (9, 9, 9)).unwrap_err(), DEV_BUILD);
        // The dev-build check comes before any network call.
        assert_eq!(install_latest(exe, &log).unwrap_err(), DEV_BUILD);
    }

    #[test]
    #[cfg(not(windows))]
    fn installer_runs_the_release_tag_not_main() {
        let (cmd, tag) = install_command((0, 2, 1));
        assert_eq!(tag, "v0.2.1");
        assert_eq!(
            cmd,
            "curl -fsSL --proto '=https' https://raw.githubusercontent.com/sionic-ai/openpaper/v0.2.1/install.sh | bash"
        );
        assert!(!cmd.contains("/main/"));
        assert!(!cmd.contains("OpenPaper.app") && !cmd.contains("MacOS"));
    }

    #[test]
    fn windows_downloads_stay_on_github() {
        let ok = |u: &str| may_follow_download(&reqwest::Url::parse(u).unwrap(), 0);
        assert!(ok(
            "https://github.com/sionic-ai/openpaper/releases/download/v1.0.0/x.exe"
        ));
        assert!(ok(
            "https://release-assets.githubusercontent.com/github-production-release-asset/1"
        ));
        assert!(ok("https://objects.githubusercontent.com/x"));
        assert!(!ok("http://objects.githubusercontent.com/x"));
        assert!(!ok("https://githubusercontent.com.evil.example/x"));
        assert!(!ok("https://evil.example/x"));
        assert!(!may_follow_download(
            &reqwest::Url::parse("https://github.com/x").unwrap(),
            MAX_HOPS
        ));
    }

    #[test]
    fn reads_checksum_files() {
        let hex = "a".repeat(64);
        assert_eq!(
            checksum_of(&format!("{hex}  OpenPaper.exe\n")),
            Some(hex.clone())
        );
        assert_eq!(
            checksum_of(&format!("{}  *OpenPaper.exe", hex.to_uppercase())),
            Some(hex)
        );
        assert_eq!(checksum_of("not a checksum"), None);
        assert_eq!(checksum_of(""), None);
        assert_eq!(
            sha256_hex(b"abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }

    #[test]
    fn windows_setup_updates_silently_and_reopens() {
        assert_eq!(SETUP_ARGS, ["/S", "/UPDATE", "/R"]);
    }
}
