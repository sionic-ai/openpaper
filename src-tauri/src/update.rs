//! In-app update: compares this build with the latest GitHub release and reruns the
//! one-line installer, detached, into the folder this app was launched from.

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::time::Duration;

const REPO: &str = "sionic-ai/openpaper";
const TIMEOUT: Duration = Duration::from_secs(8);
/// The installer script. The app path reaches it only through `OPENPAPER_DIR`, so the command
/// line never contains `<DIR>/OpenPaper.app/Contents/MacOS/openpaper` and the installer's
/// `pkill -f` on that path cannot match the installer itself.
const INSTALL: &str =
    "curl -fsSL https://raw.githubusercontent.com/sionic-ai/openpaper/main/install.sh | bash";
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
    let available = latest.as_deref().is_some_and(|l| is_newer(l, &current));
    UpdateInfo {
        current,
        latest,
        available,
    }
}

/// `<DIR>/OpenPaper.app/Contents/MacOS/openpaper` → `<DIR>`. `None` outside an `.app` bundle.
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

/// Starts the installer in its own process group so it outlives this app, which the
/// installer quits, replaces and reopens. Output is appended to `log`.
pub fn install(exe: &Path, log: &Path) -> Result<(), String> {
    use std::os::unix::process::CommandExt;
    use std::process::{Command, Stdio};
    let dir = install_dir_from_exe(exe).ok_or_else(|| DEV_BUILD.to_string())?;
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
        .arg(INSTALL)
        .env("OPENPAPER_DIR", &dir)
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
        let err = install(
            Path::new("/x/target/debug/openpaper"),
            &std::env::temp_dir().join("openpaper-update-test.log"),
        )
        .unwrap_err();
        assert_eq!(err, DEV_BUILD);
    }

    #[test]
    fn installer_command_has_no_app_path() {
        assert!(INSTALL.contains("sionic-ai/openpaper/"));
        assert!(!INSTALL.contains("OpenPaper.app"));
        assert!(!INSTALL.contains("MacOS"));
    }
}
