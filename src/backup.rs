//! Backups of the settings and the Community data, in the `backups` folder next to the settings file.
//! Each is a folder with copies of the files and an `info.json` saying what made it: the first start of
//! a new version (the last `KEEP_AUTO` are kept) or the Backups page (kept until deleted there).

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// Automatic backups kept; the oldest go first.
const KEEP_AUTO: usize = 10;

/// What made a backup.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    /// A new version's first start, before it changed anything.
    #[default]
    Update,
    /// Back up now, on the Backups page.
    Manual,
    /// The files as they were before a restore.
    Restore,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Info {
    pub kind: Kind,
    /// The Streamerz version the files are from.
    pub version: String,
    /// For an update: the version that runs them now.
    pub to: String,
    /// When it was made (unix seconds).
    pub made: i64,
}

#[derive(Clone, Debug)]
pub struct Backup {
    pub dir: PathBuf,
    pub info: Info,
    /// Its files' size together, in bytes.
    pub size: u64,
}

impl Backup {
    pub fn name(&self) -> String {
        self.dir
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default()
    }

    /// Kept until deleted by hand.
    fn kept(&self) -> bool {
        self.info.kind == Kind::Manual
    }
}

/// The folder all backups are in.
pub fn root() -> PathBuf {
    crate::config_path().with_file_name("backups")
}

/// The files a backup copies: the settings file and the Community data next to it.
fn files() -> [PathBuf; 2] {
    let cfg = crate::config_path();
    [cfg.clone(), cfg.with_file_name("community.json")]
}

fn now() -> i64 {
    crate::fmt::unix_now().as_secs() as i64
}

/// Backs up the files now, into `root`.
pub fn make(kind: Kind, version: &str, to: &str) -> Result<Backup, String> {
    make_in(&root(), &files(), kind, version, to, now())
}

fn make_in(root: &Path, files: &[PathBuf], kind: Kind, version: &str, to: &str, made: i64) -> Result<Backup, String> {
    let stamp = crate::fmt::date(
        &crate::engine::civil_time(made + i64::from(crate::fmt::now().offset)),
        "yyyyMMdd-HHmmss",
    );
    let base = format!("{stamp}-{}", if version.is_empty() { "dev" } else { version });
    let mut dir = root.join(&base);
    let mut n = 2;
    while dir.exists() {
        dir = root.join(format!("{base}-{n}"));
        n += 1;
    }
    let fail = |e: std::io::Error| format!("{}: {e}", dir.display());
    std::fs::create_dir_all(&dir).map_err(fail)?;
    // It holds the settings file, logins included: only the owner gets in.
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o700));
    }
    let mut size = 0;
    for f in files {
        let Some(name) = f.file_name() else { continue };
        if f.is_file() {
            size += std::fs::copy(f, dir.join(name)).map_err(fail)?;
        }
    }
    let info = Info {
        kind,
        version: version.into(),
        to: to.into(),
        made,
    };
    let text = serde_json::to_string_pretty(&info).unwrap_or_default();
    std::fs::write(dir.join("info.json"), text).map_err(fail)?;
    if kind != Kind::Manual {
        prune(root);
    }
    Ok(Backup { dir, info, size })
}

/// All backups, newest first.
pub fn list() -> Vec<Backup> {
    list_in(&root())
}

fn list_in(root: &Path) -> Vec<Backup> {
    let Ok(rd) = std::fs::read_dir(root) else {
        return Vec::new();
    };
    let mut out: Vec<Backup> = rd
        .flatten()
        .filter(|e| e.path().is_dir())
        .filter_map(|e| {
            let dir = e.path();
            let info: Info = serde_json::from_str(&std::fs::read_to_string(dir.join("info.json")).ok()?).ok()?;
            let size = std::fs::read_dir(&dir)
                .map(|r| {
                    r.flatten()
                        .filter(|f| f.file_name() != "info.json")
                        .filter_map(|f| f.metadata().ok())
                        .map(|m| m.len())
                        .sum()
                })
                .unwrap_or(0);
            Some(Backup { dir, info, size })
        })
        .collect();
    out.sort_by(|a, b| b.info.made.cmp(&a.info.made).then_with(|| b.name().cmp(&a.name())));
    out
}

/// Deletes the oldest automatic backups past `KEEP_AUTO`.
fn prune(root: &Path) {
    for b in list_in(root).iter().filter(|b| !b.kept()).skip(KEEP_AUTO) {
        let _ = std::fs::remove_dir_all(&b.dir);
    }
}

pub fn delete(b: &Backup) -> Result<(), String> {
    // Only folders in the backups folder.
    if b.dir.parent() != Some(root().as_path()) {
        return Err(format!("{}: not a backup", b.dir.display()));
    }
    std::fs::remove_dir_all(&b.dir).map_err(|e| format!("{}: {e}", b.dir.display()))
}

/// Puts `b`'s files back, after backing up the ones there now. Streamerz must restart after it (what it
/// holds in memory would be saved over them).
pub fn restore(b: &Backup) -> Result<(), String> {
    make(Kind::Restore, crate::brand::VERSION, "")?;
    restore_files(&b.dir, &files())
}

fn restore_files(from: &Path, to: &[PathBuf]) -> Result<(), String> {
    for f in to {
        let Some(name) = f.file_name() else { continue };
        let src = from.join(name);
        if !src.is_file() {
            continue;
        }
        // Copied beside it, then renamed over it: never half a file.
        let tmp = f.with_extension("restore.tmp");
        std::fs::copy(&src, &tmp)
            .and_then(|_| std::fs::rename(&tmp, f))
            .map_err(|e| format!("{}: {e}", f.display()))?;
    }
    Ok(())
}

/// At start, before anything is saved: the first run of a new version backs up the files the last
/// version left. Dev builds don't.
pub fn on_start() {
    let marker = root().join("last-version");
    let last = std::fs::read_to_string(&marker).unwrap_or_default();
    let now = crate::brand::VERSION;
    if !due(last.trim(), now, crate::config_path().is_file()) {
        return;
    }
    match make(Kind::Update, last.trim(), now) {
        Ok(b) => crate::say(format!("Backed up the settings before {now} ({})", b.name())),
        Err(e) => crate::error(format!("Backup before {now}: {e}")),
    }
    let _ = std::fs::create_dir_all(root());
    let _ = std::fs::write(marker, now);
}

/// A version's first start makes a backup, when there are settings to keep and it's a release.
fn due(last: &str, now: &str, have_settings: bool) -> bool {
    now != "dev" && last != now && have_settings
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("streamerz-backup-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn a_new_version_backs_up_once() {
        assert!(due("1.0.0", "1.1.0", true));
        assert!(due("", "1.1.0", true));
        assert!(!due("1.1.0", "1.1.0", true));
        assert!(!due("1.0.0", "dev", true));
        assert!(!due("", "1.1.0", false));
    }

    #[test]
    fn backups_are_made_listed_pruned_and_restored() {
        let d = scratch("all");
        let (cfg, com) = (d.join("streamerz.json"), d.join("community.json"));
        std::fs::write(&cfg, "{\"v\":1}").unwrap();
        std::fs::write(&com, "{\"users\":[]}").unwrap();
        let root = d.join("backups");
        let files = [cfg.clone(), com.clone()];
        let kept = make_in(&root, &files, Kind::Manual, "1.0.0", "", 100).unwrap();
        assert_eq!(kept.size, 19);
        for i in 0..12 {
            make_in(&root, &files, Kind::Update, "1.0.0", "1.1.0", 200 + i).unwrap();
        }
        let all = list_in(&root);
        // Ten automatic ones, newest first, and the one made by hand.
        assert_eq!(all.len(), 11);
        assert_eq!(all[0].info.made, 211);
        assert!(all.iter().any(|b| b.info.kind == Kind::Manual));
        std::fs::write(&cfg, "{\"v\":2}").unwrap();
        restore_files(&kept.dir, &files).unwrap();
        assert_eq!(std::fs::read_to_string(&cfg).unwrap(), "{\"v\":1}");
        let _ = std::fs::remove_dir_all(&d);
    }
}
