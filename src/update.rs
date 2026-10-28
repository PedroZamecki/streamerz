//! Updates from the GitHub releases: checked at start and then every hour while open (not in dev
//! builds). Settings choose to ask first (the default), download in the background (it goes in place at
//! the next start), or not check. Files are downloaded as `.new`, checked, and renamed over the old ones;
//! no helper program is needed, because the running program is never the one written to.

use crate::i18n::{tr, trf};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::path::{Path, PathBuf};

fn latest() -> String {
    format!("https://api.github.com/repos/{}/releases/latest", crate::brand::REPO)
}

#[derive(Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Updates {
    #[default]
    Ask,
    Background,
    Ignore,
}

/// This program's file, as it was at start (also what autostart and the shortcuts' desktop file run): once updated, the running one is the renamed .old.
/// In an AppImage, the AppImage file (the program inside is read-only).
pub fn exe() -> Option<PathBuf> {
    static EXE: std::sync::OnceLock<Option<PathBuf>> = std::sync::OnceLock::new();
    EXE.get_or_init(|| appimage().or_else(|| std::env::current_exe().ok()))
        .clone()
}

/// The AppImage this runs from, if it does.
fn appimage() -> Option<PathBuf> {
    std::env::var_os("APPIMAGE").map(PathBuf::from).filter(|p| p.is_file())
}

/// "v1.2.3" as numbers; None for "dev" and the like.
fn semver(v: &str) -> Option<(u64, u64, u64)> {
    let mut n = v
        .trim()
        .trim_start_matches('v')
        .splitn(3, '.')
        .map(|p| p.parse::<u64>().ok());
    Some((n.next()??, n.next()??, n.next()??))
}

/// This build's version, if it's a release.
fn running() -> Option<&'static str> {
    option_env!("STREAMERZ_VERSION").filter(|v| semver(v).is_some())
}

/// A release newer than this build: its tag and the file this install is made from.
pub struct Release {
    pub tag: String,
    /// Where it lives now (and where the new one goes).
    path: PathBuf,
    url: String,
    /// The file's SHA-256 as GitHub lists it, when it does.
    sha256: Option<String>,
}

/// What updates: the Windows exe, or the AppImage. Any other way of running (a dev build, a copied
/// binary) has no update, as it isn't what a release ships.
fn installed() -> Option<(PathBuf, &'static str)> {
    if cfg!(windows) {
        Some((exe()?, "Streamerz-windows-x86_64.exe"))
    } else {
        Some((appimage()?, "Streamerz-linux-x86_64.AppImage"))
    }
}

/// The newest release found, for Settings to show.
static FOUND: std::sync::Mutex<Option<String>> = std::sync::Mutex::new(None);

pub fn found() -> Option<String> {
    FOUND.lock().ok()?.clone()
}

/// The release newer than this build, if there is one.
pub async fn newer() -> Option<Release> {
    let now = semver(running()?)?;
    let headers = [
        ("User-Agent".to_string(), "Streamerz".to_string()),
        ("Accept".to_string(), "application/vnd.github+json".to_string()),
    ];
    let r = crate::http::request("GET", &latest(), &headers, b"").await.ok()?;
    let v: Value = serde_json::from_slice(&r.body).ok()?;
    let tag = v["tag_name"].as_str()?.to_string();
    if semver(&tag)? <= now {
        return None;
    }
    let (path, name) = installed()?;
    let a = v["assets"].as_array()?.iter().find(|a| a["name"] == name)?;
    Some(Release {
        url: a["browser_download_url"].as_str()?.to_string(),
        sha256: a["digest"]
            .as_str()
            .and_then(|d| d.strip_prefix("sha256:"))
            .map(str::to_string),
        tag,
        path,
    })
}

/// `path` with `ext` added to its whole name (Streamerz.AppImage.new).
fn beside(path: &Path, ext: &str) -> PathBuf {
    let mut n = path.as_os_str().to_owned();
    n.push(".");
    n.push(ext);
    PathBuf::from(n)
}

/// A program, not an error page (an ELF or an exe, not tiny), with the SHA-256 GitHub lists when it
/// lists one.
fn sound(body: &[u8], sha256: Option<&str>) -> bool {
    use sha2::{Digest, Sha256};
    let program = body.starts_with(b"\x7fELF") || body.starts_with(b"MZ");
    program
        && body.len() > 100_000
        && sha256.is_none_or(|want| {
            let got: String = Sha256::digest(body).iter().map(|b| format!("{b:02x}")).collect();
            got == want.to_lowercase()
        })
}

/// Downloads the release next to the file it replaces, as `.new` (a half download never has that name),
/// with the tag in `.version`; nothing else changes until `apply`.
pub async fn stage(r: &Release) -> Result<(), String> {
    let (new, tag) = (beside(&r.path, "new"), beside(&r.path, "version"));
    if new.is_file() && std::fs::read_to_string(&tag).is_ok_and(|t| t == r.tag) {
        return Ok(());
    }
    let headers = [("User-Agent".to_string(), "Streamerz".to_string())];
    let got = crate::http::request("GET", &r.url, &headers, b"").await?;
    if got.status != 200 || !sound(&got.body, r.sha256.as_deref()) {
        return Err(format!("{}: not the program it should be (HTTP {})", r.url, got.status));
    }
    let part = beside(&r.path, "part");
    std::fs::write(&part, &got.body).map_err(|e| format!("{}: {e}", part.display()))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&part, std::fs::Permissions::from_mode(0o755));
    }
    std::fs::rename(&part, &new).map_err(|e| format!("{}: {e}", new.display()))?;
    std::fs::write(&tag, &r.tag).map_err(|e| format!("{}: {e}", tag.display()))
}

/// Puts the staged file in place. On Linux that's one atomic rename (the running program keeps its old
/// copy open, so nothing breaks); Windows can't replace a running program, only rename it, so the old
/// one waits as `.old` until the next start.
fn apply(path: &Path) -> Result<(), String> {
    let new = beside(path, "new");
    if cfg!(windows) {
        let old = beside(path, "old");
        let _ = std::fs::remove_file(&old);
        std::fs::rename(path, &old).map_err(|e| format!("{}: {e}", path.display()))?;
        if let Err(e) = std::fs::rename(&new, path) {
            let _ = std::fs::rename(&old, path);
            return Err(format!("{}: {e}", path.display()));
        }
    } else {
        std::fs::rename(&new, path).map_err(|e| format!("{}: {e}", path.display()))?;
    }
    let _ = std::fs::remove_file(beside(path, "version"));
    Ok(())
}

/// At start: leftovers of the last update are cleaned, and a release the background mode staged is put
/// in place and started (this process quits).
pub fn tidy() {
    let Some((path, _)) = installed() else { return };
    for ext in ["old", "part"] {
        let _ = std::fs::remove_file(beside(&path, ext));
    }
    let Ok(tag) = std::fs::read_to_string(beside(&path, "version")) else {
        return;
    };
    let newer = matches!((semver(&tag), running().and_then(semver)), (Some(t), Some(r)) if t > r);
    if newer && apply(&path).is_ok() {
        restart();
    }
    // Stale: this build is that release already, or newer.
    let _ = std::fs::remove_file(beside(&path, "new"));
    let _ = std::fs::remove_file(beside(&path, "version"));
}

/// The release tag last announced, so each is told about once.
static ANNOUNCED: std::sync::Mutex<String> = std::sync::Mutex::new(String::new());

/// Looks for a newer release and acts as the settings say. `asked` is Settings' Check now: it asks
/// whatever the settings say and tells when there's nothing new.
///   Ask: a question first, then install (and offer a restart).
///   Background: only downloads it; it goes in place at the next start.
///   Ignore: nothing, unless asked.
pub async fn check(cfg: std::sync::Arc<tokio::sync::watch::Sender<crate::Config>>, asked: bool) {
    let mode = cfg.borrow().updates;
    if mode == Updates::Ignore && !asked {
        return;
    }
    let Some(release) = newer().await else {
        if asked {
            let what = match running() {
                Some(v) => trf("update.newest", &[("version", v)]),
                None => tr("update.dev").to_string(),
            };
            let _ = crate::ui::confirm(what, tr("dialogs.ok"), "").await;
        }
        return;
    };
    let tag = release.tag.clone();
    *FOUND.lock().unwrap() = Some(tag.clone());
    // Told once per release, however many times it's looked for.
    let fresh = std::mem::replace(&mut *ANNOUNCED.lock().unwrap(), tag.clone()) != tag;
    if fresh {
        crate::say(format!("streamerz {tag} is out"));
    }
    if mode == Updates::Background && !asked {
        crate::say(format!("Downloading {tag}..."));
        match stage(&release).await {
            Ok(()) if fresh => crate::notify::send(
                &trf("update.ready.title", &[("version", &tag)]),
                tr("update.ready.body"),
            ),
            Ok(()) => {}
            Err(e) => crate::say(format!("Download of {tag} failed: {e}")),
        }
        return;
    }
    if !fresh && !asked {
        return;
    }
    if fresh {
        crate::notify::send(&trf("update.out.title", &[("version", &tag)]), tr("update.out.body"));
    }
    let now = running().unwrap_or("");
    let question = trf("update.ask", &[("version", &tag), ("now", now)]);
    if !crate::ui::confirm(question, tr("update.later"), tr("update.now"))
        .await
        .unwrap_or(false)
    {
        return;
    }
    crate::say(format!("Updating to {tag}..."));
    if let Err(e) = stage(&release).await.and_then(|()| apply(&release.path)) {
        crate::say(format!("Update to {tag} failed: {e}"));
        let _ = crate::ui::confirm(trf("update.failed", &[("error", &e)]), tr("dialogs.ok"), "").await;
        return;
    }
    let done = trf("update.done", &[("version", &tag)]);
    if crate::ui::confirm(done, tr("update.later"), tr("update.restart"))
        .await
        .unwrap_or(false)
    {
        crate::ui::restart();
    } else {
        crate::say(format!("Updated to {tag}: it runs from the next start."));
    }
}

/// Checks now and then once an hour while Streamerz is open.
pub async fn watch(cfg: std::sync::Arc<tokio::sync::watch::Sender<crate::Config>>) {
    loop {
        check(cfg.clone(), false).await;
        tokio::time::sleep(std::time::Duration::from_secs(3600)).await;
    }
}

/// Starts the new build and quits this one.
pub fn restart() -> ! {
    if let Some(exe) = exe() {
        let mut cmd = std::process::Command::new(exe);
        cmd.args(std::env::args_os().skip(1));
        // The AppImage this ran from mounted itself and told its children where; the new one makes its own.
        for v in ["APPDIR", "APPIMAGE", "ARGV0", "OWD"] {
            cmd.env_remove(v);
        }
        let _ = cmd.spawn();
    }
    crate::crash::quit(0)
}

#[cfg(test)]
mod tests {
    use super::{beside, semver, sound};
    use sha2::{Digest, Sha256};
    use std::path::Path;

    #[test]
    fn versions_compare() {
        assert!(semver("v1.10.0") > semver("v1.9.3"));
        assert_eq!(semver("v2.0.1"), Some((2, 0, 1)));
        assert_eq!(semver("dev"), None);
    }

    #[test]
    fn downloads_are_checked() {
        let mut elf = b"\x7fELF".to_vec();
        elf.resize(200_000, 1);
        let hash: String = Sha256::digest(&elf).iter().map(|b| format!("{b:02x}")).collect();
        assert!(sound(&elf, None));
        assert!(sound(&elf, Some(&hash)));
        assert!(!sound(&elf, Some("00")));
        assert!(!sound(b"<html>not found</html>", None));
        assert!(!sound(&elf[..1000], None));
    }

    #[test]
    fn staged_names_keep_the_whole_name() {
        assert_eq!(
            beside(Path::new("/a/x.AppImage"), "new"),
            Path::new("/a/x.AppImage.new")
        );
    }
}
