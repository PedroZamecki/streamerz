//! Updates from the GitHub releases: checked once at start (not in dev builds). Settings choose to ask
//! first (the default), update in the background, or not check. A new build replaces this one on disk
//! and runs from the next start (or at once, if asked to restart).

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::path::{Path, PathBuf};

const LATEST: &str = "https://api.github.com/repos/PedroZamecki/streamerz/releases/latest";

#[derive(Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Updates {
    #[default]
    Ask,
    Background,
    Ignore,
}

/// This program's file, as it was at start: once updated, the running one is the renamed .old.
/// In an AppImage, the AppImage file (the program inside is read-only).
fn exe() -> Option<PathBuf> {
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

/// The release newer than this build: its tag and the download URLs of the files this platform uses.
pub async fn newer() -> Option<(String, Vec<(PathBuf, String)>)> {
    let now = semver(running()?)?;
    let headers = [
        ("User-Agent".to_string(), "streamerz".to_string()),
        ("Accept".to_string(), "application/vnd.github+json".to_string()),
    ];
    let r = crate::http::request("GET", LATEST, &headers, b"").await.ok()?;
    let v: Value = serde_json::from_slice(&r.body).ok()?;
    let tag = v["tag_name"].as_str()?.to_string();
    if semver(&tag)? <= now {
        return None;
    }
    let exe = exe()?;
    let dir = exe.parent()?.to_path_buf();
    // This program, and on Linux the TikTok window's next to it if it's there.
    let name = if cfg!(windows) {
        "streamerz.exe"
    } else if appimage().is_some() {
        "streamerz-x86_64.AppImage"
    } else {
        "streamerz"
    };
    let mut want = vec![(exe.clone(), name)];
    let helper = dir.join("streamerz-tiktok");
    if cfg!(not(windows)) && appimage().is_none() && helper.exists() {
        want.push((helper, "streamerz-tiktok"));
    }
    let assets = v["assets"].as_array()?;
    let files = want
        .into_iter()
        .map(|(path, name)| {
            let url = assets.iter().find(|a| a["name"] == name)?["browser_download_url"].as_str()?;
            Some((path, url.to_string()))
        })
        .collect::<Option<Vec<_>>>()?;
    Some((tag, files))
}

/// Downloads and puts the new files in place of the running ones (renamed to .old, removed at the next
/// start: Windows can't overwrite a running program, only rename it).
pub async fn install(files: &[(PathBuf, String)]) -> Result<(), String> {
    let headers = [("User-Agent".to_string(), "streamerz".to_string())];
    let mut got = Vec::new();
    for (path, url) in files {
        let r = crate::http::request("GET", url, &headers, b"").await?;
        // A program, not an error page: ELF or MZ, and not tiny.
        if r.status != 200 || r.body.len() < 100_000 || !(r.body.starts_with(b"\x7fELF") || r.body.starts_with(b"MZ")) {
            return Err(format!("{url}: not a program (HTTP {})", r.status));
        }
        got.push((path, r.body));
    }
    for (path, body) in got {
        let new = path.with_extension("new");
        std::fs::write(&new, &body).map_err(|e| format!("{}: {e}", new.display()))?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = std::fs::set_permissions(&new, std::fs::Permissions::from_mode(0o755));
        }
        let old = path.with_extension("old");
        let _ = std::fs::remove_file(&old);
        std::fs::rename(path, &old).map_err(|e| format!("{}: {e}", path.display()))?;
        if let Err(e) = std::fs::rename(&new, path) {
            // Back as it was.
            let _ = std::fs::rename(&old, path);
            return Err(format!("{}: {e}", path.display()));
        }
    }
    Ok(())
}

/// The files left by the last update.
pub fn tidy() {
    let Some(exe) = exe() else { return };
    let dir = exe.parent().map_or_else(PathBuf::new, Path::to_path_buf);
    for f in [exe.with_extension("old"), dir.join("streamerz-tiktok.old")] {
        let _ = std::fs::remove_file(f);
    }
}

/// The newer release, installed as the settings say (at start); `asked` is Settings' Check now, which
/// asks whatever the settings say and tells when there's nothing new.
pub async fn check(cfg: std::sync::Arc<tokio::sync::watch::Sender<crate::Config>>, asked: bool) {
    let mode = cfg.borrow().updates;
    if mode == Updates::Ignore && !asked {
        return;
    }
    let Some((tag, files)) = newer().await else {
        if asked {
            let what = match running() {
                Some(v) => format!("streamerz {v} is the newest."),
                None => "This is a development build: it doesn't update.".to_string(),
            };
            let _ = crate::ui::confirm(what, "OK", "").await;
        }
        return;
    };
    crate::say(format!("streamerz {tag} is out"));
    let ask = mode != Updates::Background || asked;
    let now = running().unwrap_or("");
    if ask
        && !crate::ui::confirm(
            format!("streamerz {tag} is out (this is {now}).\n\nUpdate now?"),
            "Later",
            "Update",
        )
        .await
        .unwrap_or(false)
    {
        return;
    }
    crate::say(format!("Updating to {tag}..."));
    if let Err(e) = install(&files).await {
        crate::say(format!("Update to {tag} failed: {e}"));
        return;
    }
    if ask
        && crate::ui::confirm(
            format!("Updated to {tag}. Restart now to use it?"),
            "Later",
            "Restart now",
        )
        .await
        .unwrap_or(false)
    {
        crate::ui::restart();
    } else {
        crate::say(format!("Updated to {tag}: it runs from the next start."));
    }
}

/// Starts the new build and quits this one.
pub fn restart() -> ! {
    if let Some(exe) = exe() {
        let _ = std::process::Command::new(exe)
            .args(std::env::args_os().skip(1))
            .spawn();
    }
    std::process::exit(0)
}

#[cfg(test)]
mod tests {
    use super::semver;

    #[test]
    fn versions_compare() {
        assert!(semver("v1.10.0") > semver("v1.9.3"));
        assert_eq!(semver("v2.0.1"), Some((2, 0, 1)));
        assert_eq!(semver("dev"), None);
    }
}
