//! Backups of the settings and the Community data, in the `backups` folder next to the settings file.
//! Each is a folder with the files, gzipped (`streamerz.json.gz`), and an `info.json` saying what made it:
//! the first start of a new version (the last `KEEP_AUTO` are kept) or the Backups page (kept until deleted
//! there). Backups made as plain copies by older versions are read as they are, and gzipped at start.

use serde::{Deserialize, Serialize};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

/// `bytes` gzipped.
fn gzip(bytes: &[u8]) -> std::io::Result<Vec<u8>> {
    let mut z = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
    z.write_all(bytes)?;
    z.finish()
}

/// What `gz` was before `gzip`.
fn gunzip(gz: &[u8]) -> std::io::Result<Vec<u8>> {
    let mut out = Vec::new();
    flate2::read::GzDecoder::new(gz).read_to_end(&mut out)?;
    Ok(out)
}

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

fn safe_version(version: &str) -> String {
    let clean: String = version
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-') {
                c
            } else {
                '_'
            }
        })
        .collect();
    let clean = clean.trim_matches(|c| matches!(c, '.' | '-' | '_'));
    if clean.is_empty() || clean.contains("..") {
        "dev".into()
    } else {
        clean.into()
    }
}

fn gz_name(name: &std::ffi::OsStr) -> std::ffi::OsString {
    let mut out = name.to_os_string();
    out.push(".gz");
    out
}

fn tmp_path(target: &Path, suffix: &str) -> PathBuf {
    let mut out = target.as_os_str().to_os_string();
    out.push(suffix);
    PathBuf::from(out)
}

fn make_in(root: &Path, files: &[PathBuf], kind: Kind, version: &str, to: &str, made: i64) -> Result<Backup, String> {
    make_in_keeping(root, files, kind, version, to, made, None)
}

fn make_in_keeping(
    root: &Path,
    files: &[PathBuf],
    kind: Kind,
    version: &str,
    to: &str,
    made: i64,
    protect: Option<&Path>,
) -> Result<Backup, String> {
    let stamp = crate::fmt::date(&crate::fmt::local_time(made), "yyyyMMdd-HHmmss");
    let base = format!("{stamp}-{}", safe_version(version));
    let mut highest = 0u64;
    if let Ok(rd) = std::fs::read_dir(root) {
        for entry in rd.flatten() {
            let name = entry.file_name();
            let name = name.to_string_lossy();
            let (prefix, num) = name_order(&name);
            if prefix == base {
                highest = highest.max(num);
            }
        }
    }
    let dir = if highest == 0 {
        root.join(&base)
    } else {
        root.join(format!("{base}-{}", highest + 1))
    };
    let fail = |e: std::io::Error| format!("{}: {e}", dir.display());
    std::fs::create_dir_all(&dir).map_err(fail)?;
    // It holds the settings file, logins included: only the owner gets in.
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o700));
    }
    let write_all = || -> Result<(Info, u64), String> {
        let mut size = 0;
        for f in files {
            let Some(name) = f.file_name() else { continue };
            if f.is_file() {
                let raw = std::fs::read(f).map_err(fail)?;
                let gz = gzip(&raw).map_err(fail)?;
                size += gz.len() as u64;
                std::fs::write(dir.join(gz_name(name)), gz).map_err(fail)?;
            }
        }
        let info = Info {
            kind,
            version: version.into(),
            to: to.into(),
            made,
        };
        let text = serde_json::to_string_pretty(&info).map_err(|e| format!("{}: {e}", dir.display()))?;
        std::fs::write(dir.join("info.json"), text).map_err(fail)?;
        Ok((info, size))
    };
    let (info, size) = match write_all() {
        Ok(ok) => ok,
        Err(e) => {
            let _ = std::fs::remove_dir_all(&dir);
            return Err(e);
        }
    };
    if kind != Kind::Manual {
        prune(root, Some(&dir), protect);
    }
    Ok(Backup { dir, info, size })
}

/// All backups, newest first.
pub fn list() -> Vec<Backup> {
    list_in(&root())
}

fn name_order(name: &str) -> (&str, u64) {
    match name.rsplit_once('-') {
        Some((base, suffix)) if !base.is_empty() && suffix.bytes().all(|b| b.is_ascii_digit()) => {
            match suffix.parse::<u64>() {
                Ok(n) if n >= 2 => (base, n),
                _ => (name, 1),
            }
        }
        _ => (name, 1),
    }
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
    out.sort_by(|a, b| {
        let (an, bn) = (a.name(), b.name());
        b.info
            .made
            .cmp(&a.info.made)
            .then_with(|| name_order(&bn).cmp(&name_order(&an)))
    });
    out
}

/// Deletes the oldest automatic backups past `KEEP_AUTO`, never deleting `keep` or `protect`.
fn prune(root: &Path, keep: Option<&Path>, protect: Option<&Path>) {
    let all = list_in(root);
    let pinned = |dir: &Path| keep.is_some_and(|k| k == dir) || protect.is_some_and(|p| p == dir);
    let pinned_auto = all.iter().filter(|b| !b.kept() && pinned(&b.dir)).count();
    let mut slots = KEEP_AUTO.saturating_sub(pinned_auto);
    for b in all.iter().filter(|b| !b.kept()) {
        if pinned(&b.dir) {
            continue;
        }
        if slots > 0 {
            slots -= 1;
        } else {
            let _ = std::fs::remove_dir_all(&b.dir);
        }
    }
}

pub fn delete(b: &Backup) -> Result<(), String> {
    delete_in(&root(), b)
}

fn delete_in(root: &Path, b: &Backup) -> Result<(), String> {
    // Only normal folders directly inside the backups folder, never `..` or symlinks.
    let normal_leaf = matches!(b.dir.components().next_back(), Some(std::path::Component::Normal(_)));
    if !normal_leaf || b.dir.parent() != Some(root) || b.dir.is_symlink() {
        return Err(format!("{}: not a backup", b.dir.display()));
    }
    std::fs::remove_dir_all(&b.dir).map_err(|e| format!("{}: {e}", b.dir.display()))
}

/// Puts `b`'s files back, after backing up the ones there now. Streamerz must restart after it (what it
/// holds in memory would be saved over them).
pub fn restore(b: &Backup) -> Result<(), String> {
    restore_in(&root(), &b.dir, &files(), crate::brand::VERSION, now())
}

fn read_backup_files(from: &Path, to: &[PathBuf]) -> Result<Vec<(PathBuf, Vec<u8>)>, String> {
    let mut loaded = Vec::new();
    for f in to {
        let Some(name) = f.file_name() else { continue };
        let (plain, gz) = (from.join(name), from.join(gz_name(name)));
        let bytes = if gz.is_file() {
            std::fs::read(&gz).and_then(|b| gunzip(&b))
        } else if plain.is_file() {
            std::fs::read(&plain)
        } else {
            continue;
        }
        .map_err(|e| format!("{}: {e}", f.display()))?;
        loaded.push((f.clone(), bytes));
    }
    if loaded.is_empty() {
        return Err(format!("{}: backup has no files to restore", from.display()));
    }
    Ok(loaded)
}

fn restore_in(root: &Path, from: &Path, to: &[PathBuf], version: &str, made: i64) -> Result<(), String> {
    // Read and verify the backup before touching live files or making the pre-restore backup.
    let loaded = read_backup_files(from, to)?;
    make_in_keeping(root, to, Kind::Restore, version, "", made, Some(from))?;
    apply_restored_files(&loaded)
}

#[cfg(test)]
fn restore_files(from: &Path, to: &[PathBuf]) -> Result<(), String> {
    let loaded = read_backup_files(from, to)?;
    apply_restored_files(&loaded)
}

fn apply_restored_files(loaded: &[(PathBuf, Vec<u8>)]) -> Result<(), String> {
    // Stage every file first so a read or write failure leaves current files untouched.
    let mut staged: Vec<(&Path, PathBuf, Option<Vec<u8>>)> = Vec::with_capacity(loaded.len());
    for (target, bytes) in loaded {
        let tmp = tmp_path(target, ".restore.tmp");
        if let Err(e) = std::fs::write(&tmp, bytes) {
            for (_, old_tmp, _) in &staged {
                let _ = std::fs::remove_file(old_tmp);
            }
            let _ = std::fs::remove_file(&tmp);
            return Err(format!("{}: {e}", target.display()));
        }
        let prev = std::fs::read(target).ok();
        staged.push((target.as_path(), tmp, prev));
    }
    // Swap each staged file into place; roll back earlier swaps if a later rename fails.
    for i in 0..staged.len() {
        let (target, tmp, _) = &staged[i];
        if let Err(e) = std::fs::rename(tmp, target) {
            for (_, leftover, _) in &staged[i..] {
                let _ = std::fs::remove_file(leftover);
            }
            for (prev_target, _, prev_bytes) in staged[..i].iter().rev() {
                match prev_bytes {
                    Some(bytes) => {
                        let rb = tmp_path(prev_target, ".restore.tmp");
                        if std::fs::write(&rb, bytes)
                            .and_then(|()| std::fs::rename(&rb, prev_target))
                            .is_err()
                        {
                            let _ = std::fs::remove_file(&rb);
                            let _ = std::fs::write(prev_target, bytes);
                        }
                    }
                    None => {
                        let _ = std::fs::remove_file(prev_target);
                    }
                }
            }
            return Err(format!("{}: {e}", target.display()));
        }
    }
    Ok(())
}

/// Backups made as plain copies are gzipped (what a restore reads either way).
fn compact_old(root: &Path) {
    for b in list_in(root) {
        let Ok(rd) = std::fs::read_dir(&b.dir) else { continue };
        for f in rd.flatten().map(|e| e.path()) {
            let plain = f.extension().is_some_and(|e| e == "json") && f.file_name().is_some_and(|n| n != "info.json");
            if !plain {
                continue;
            }
            let gz = tmp_path(&f, ".gz");
            let tmp = tmp_path(&gz, ".tmp");
            let ok = std::fs::read(&f)
                .and_then(|b| gzip(&b))
                .and_then(|z| std::fs::write(&tmp, z))
                .and_then(|()| std::fs::rename(&tmp, &gz))
                .is_ok();
            if ok {
                let _ = std::fs::remove_file(&f);
            } else {
                let _ = std::fs::remove_file(&tmp);
            }
        }
    }
}

/// At start, before anything is saved: the first run of a new version backs up the files the last
/// version left. Dev builds don't.
pub fn on_start() {
    compact_old(&root());
    let marker = root().join("last-version");
    let last = std::fs::read_to_string(&marker).unwrap_or_default();
    let now = crate::brand::VERSION;
    if now == "dev" {
        return;
    }
    if due(last.trim(), now, crate::config_path().is_file()) {
        match make(Kind::Update, last.trim(), now) {
            Ok(b) => crate::say(format!("Backed up the settings before {now} ({})", b.name())),
            Err(e) => {
                crate::error(format!("Backup before {now}: {e}"));
                return;
            }
        }
    }
    let _ = record_version(&marker, now);
}

fn record_version(marker: &Path, now: &str) -> std::io::Result<()> {
    if let Some(parent) = marker.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(marker, now)
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
    fn fresh_install_records_its_version_before_settings_exist() {
        let dir = scratch("first-version");
        let marker = dir.join("backups/last-version");
        assert!(!due("", "1.7.0", false));
        record_version(&marker, "1.7.0").unwrap();
        let last = std::fs::read_to_string(&marker).unwrap();
        assert!(due(&last, "1.8.0", true));
        assert_eq!(last, "1.7.0");
        std::fs::remove_dir_all(dir).unwrap();
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
        assert!(kept.dir.join("streamerz.json.gz").is_file() && !kept.dir.join("streamerz.json").exists());
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

    #[test]
    fn a_backup_is_named_by_the_local_time_it_was_made() {
        let d = scratch("stamp");
        let cfg = d.join("streamerz.json");
        std::fs::write(&cfg, "{}").unwrap();
        // A summer moment keeps its own offset in the name, whatever today's is.
        let made = 1_752_580_800;
        let b = make_in(
            &d.join("backups"),
            std::slice::from_ref(&cfg),
            Kind::Manual,
            "1.0.0",
            "",
            made,
        )
        .unwrap();
        let stamp = crate::fmt::date(&crate::fmt::local_time(made), "yyyyMMdd-HHmmss");
        let name = b.dir.file_name().unwrap().to_string_lossy().into_owned();
        assert!(name.starts_with(&stamp), "{name} {stamp}");
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn backups_are_gzipped_and_plain_ones_still_restore() {
        let d = scratch("gz");
        let cfg = d.join("streamerz.json");
        let big = format!("{{\"users\":\"{}\"}}", "chatter ".repeat(2000));
        std::fs::write(&cfg, &big).unwrap();
        let root = d.join("backups");
        let b = make_in(&root, std::slice::from_ref(&cfg), Kind::Manual, "1.0.0", "", 1).unwrap();
        assert!(b.size < big.len() as u64 / 10, "{} of {}", b.size, big.len());
        std::fs::write(&cfg, "{}").unwrap();
        restore_files(&b.dir, std::slice::from_ref(&cfg)).unwrap();
        assert_eq!(std::fs::read_to_string(&cfg).unwrap(), big);
        // An old backup: a plain copy. It restores, and is gzipped at start.
        let old = root.join("20260101-000000-1.0.0");
        std::fs::create_dir_all(&old).unwrap();
        std::fs::write(old.join("streamerz.json"), &big).unwrap();
        std::fs::write(old.join("info.json"), "{\"kind\":\"update\",\"made\":5}").unwrap();
        std::fs::write(&cfg, "{}").unwrap();
        restore_files(&old, std::slice::from_ref(&cfg)).unwrap();
        assert_eq!(std::fs::read_to_string(&cfg).unwrap(), big);
        compact_old(&root);
        assert!(old.join("streamerz.json.gz").is_file() && !old.join("streamerz.json").exists());
        std::fs::write(&cfg, "{}").unwrap();
        restore_files(&old, std::slice::from_ref(&cfg)).unwrap();
        assert_eq!(std::fs::read_to_string(&cfg).unwrap(), big);
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn restore_reads_oldest_auto_backup_before_pruning_and_keeps_live_files_on_error() {
        let d = scratch("restore-safe");
        let (cfg, com) = (d.join("streamerz.json"), d.join("community.json"));
        let root = d.join("backups");
        let files = [cfg.clone(), com.clone()];
        std::fs::write(&cfg, "{\"v\":0}").unwrap();
        std::fs::write(&com, "{\"c\":0}").unwrap();
        // Fill all KEEP_AUTO slots so the oldest automatic backup would be pruned on the next make.
        let oldest = make_in(&root, &files, Kind::Update, "1.0.0", "1.1.0", 100).unwrap();
        for i in 1..KEEP_AUTO {
            std::fs::write(&cfg, format!("{{\"v\":{i}}}")).unwrap();
            make_in(&root, &files, Kind::Update, "1.0.0", "1.1.0", 100 + i as i64).unwrap();
        }
        std::fs::write(&cfg, "{\"v\":99}").unwrap();
        std::fs::write(&com, "{\"c\":99}").unwrap();
        // Restoring the 10th oldest automatic backup must not prune it before reading.
        restore_in(&root, &oldest.dir, &files, "1.1.0", 500).unwrap();
        assert_eq!(std::fs::read_to_string(&cfg).unwrap(), "{\"v\":0}");
        assert_eq!(std::fs::read_to_string(&com).unwrap(), "{\"c\":0}");

        // A corrupt second file fails restore without modifying the first live file or adding a backup.
        let corrupt = make_in(&root, &files, Kind::Manual, "1.1.0", "", 600).unwrap();
        std::fs::write(corrupt.dir.join("community.json.gz"), b"not-gzip-data").unwrap();
        std::fs::write(&cfg, "{\"v\":42}").unwrap();
        std::fs::write(&com, "{\"c\":42}").unwrap();
        let count_before = list_in(&root).len();
        assert!(restore_in(&root, &corrupt.dir, &files, "1.1.0", 700).is_err());
        assert_eq!(std::fs::read_to_string(&cfg).unwrap(), "{\"v\":42}");
        assert_eq!(std::fs::read_to_string(&com).unwrap(), "{\"c\":42}");
        assert_eq!(list_in(&root).len(), count_before);

        // An empty backup directory fails restore instead of silently succeeding.
        let empty = root.join("empty-backup");
        std::fs::create_dir_all(&empty).unwrap();
        assert!(restore_in(&root, &empty, &files, "1.1.0", 800).is_err());
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn pruning_orders_same_second_suffixes_numerically_and_keeps_newest() {
        let d = scratch("prune-order");
        let cfg = d.join("streamerz.json");
        std::fs::write(&cfg, "{\"v\":1}").unwrap();
        let root = d.join("backups");
        let files = [cfg.clone()];
        let mut made = Vec::new();
        for _ in 0..12 {
            made.push(make_in(&root, &files, Kind::Update, "1.0.0", "1.1.0", 1000).unwrap());
        }
        let all = list_in(&root);
        assert_eq!(all.len(), KEEP_AUTO);
        // The 12th and 11th backups (-12 and -11) must be kept at the front, not deleted before -9.
        assert_eq!(all[0].dir, made[11].dir);
        assert_eq!(all[1].dir, made[10].dir);
        assert!(!made[0].dir.exists() && !made[1].dir.exists());

        // Clock skew backwards must never cause the backup just made to be pruned immediately.
        let skewed = make_in(&root, &files, Kind::Update, "1.0.0", "1.1.0", 50).unwrap();
        assert!(skewed.dir.exists());
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn delete_rejects_traversal_and_make_sanitizes_version() {
        let d = scratch("safe-paths");
        let cfg = d.join("streamerz.json");
        std::fs::write(&cfg, "{}").unwrap();
        let root = d.join("backups");
        let b = make_in(&root, std::slice::from_ref(&cfg), Kind::Manual, "../../escape", "", 10).unwrap();
        assert_eq!(b.dir.parent(), Some(root.as_path()));
        let traversal = Backup {
            dir: root.join(".."),
            info: Info::default(),
            size: 0,
        };
        assert!(delete_in(&root, &traversal).is_err());
        assert!(d.exists());
        assert!(delete_in(&root, &b).is_ok());
        assert!(!b.dir.exists());
        let _ = std::fs::remove_dir_all(&d);
    }
}
