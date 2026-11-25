//! Persistent metadata cache, independent of image decoding and the frontend.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

static PART_SEQ: AtomicU64 = AtomicU64::new(0);

fn dir() -> PathBuf {
    crate::config_path().with_file_name("cache")
}

fn entry_path(base: &Path, name: &str) -> Option<PathBuf> {
    if name.is_empty() || name == "." || name == ".." || name.contains(['/', '\\', '\0']) || name.contains("..") {
        return None;
    }
    Some(base.join(format!("{name}.json")))
}

fn read_in(base: &Path, name: &str) -> Option<(u64, serde_json::Value)> {
    let file = entry_path(base, name)?;
    let age = file
        .metadata()
        .ok()?
        .modified()
        .ok()?
        .elapsed()
        .map_or(0, |d| d.as_secs());
    Some((age, serde_json::from_slice(&std::fs::read(file).ok()?).ok()?))
}

fn forget_in(base: &Path, name: &str) {
    if let Some(file) = entry_path(base, name) {
        let _ = std::fs::remove_file(file);
    }
}

fn save_in(base: &Path, name: &str, v: &serde_json::Value) {
    let Some(file) = entry_path(base, name) else {
        return;
    };
    let seq = PART_SEQ.fetch_add(1, Ordering::Relaxed);
    let tmp = base.join(format!("{name}.{}.{seq}.part", std::process::id()));
    let _ = std::fs::create_dir_all(base);
    if std::fs::write(&tmp, v.to_string())
        .and_then(|()| std::fs::rename(&tmp, &file))
        .is_err()
    {
        let _ = std::fs::remove_file(&tmp);
    }
}

/// What was saved as `name` in the cache (emote lists, picture URLs...), and how many seconds ago.
pub fn saved(name: &str) -> Option<(u64, serde_json::Value)> {
    read_in(&dir(), name)
}

/// Remove what was saved as `name`.
pub fn forget_saved(name: &str) {
    forget_in(&dir(), name);
}

/// Save `v` as `name` in the cache (whole or nothing).
pub fn save(name: &str, v: &serde_json::Value) {
    let closed = if crate::DEV_DIR.get().is_some() {
        let Ok(closed) = crate::DEV_CACHE_CLOSED.lock() else {
            return;
        };
        if *closed {
            return;
        }
        Some(closed)
    } else {
        None
    };
    let _closed = closed;
    save_in(&dir(), name, v);
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn saves_reads_forgets_and_rejects_unsafe_names() {
        let base = std::env::temp_dir().join(format!("streamerz-cache-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        save_in(&base, "emotes", &json!({"list": ["Kappa"]}));
        let (age, val) = read_in(&base, "emotes").unwrap();
        assert!(age < 60);
        assert_eq!(val["list"][0], "Kappa");

        // Corrupt JSON yields None instead of panicking.
        std::fs::write(base.join("broken.json"), b"{bad").unwrap();
        assert!(read_in(&base, "broken").is_none());

        // Path traversal names are ignored.
        save_in(&base, "../escape", &json!(1));
        save_in(&base, "a/b", &json!(1));
        save_in(&base, "", &json!(1));
        assert!(read_in(&base, "../escape").is_none());
        assert!(!base.parent().unwrap().join("escape.json").exists());

        forget_in(&base, "emotes");
        assert!(read_in(&base, "emotes").is_none());
        let _ = std::fs::remove_dir_all(&base);
    }
}
