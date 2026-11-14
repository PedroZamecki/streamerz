//! Persistent metadata cache, independent of image decoding and the frontend.

fn dir() -> std::path::PathBuf {
    crate::config_path().with_file_name("cache")
}

/// What was saved as `name` in the cache (emote lists, picture URLs...), and how many seconds ago.
pub fn saved(name: &str) -> Option<(u64, serde_json::Value)> {
    let file = dir().join(format!("{name}.json"));
    let age = file
        .metadata()
        .ok()?
        .modified()
        .ok()?
        .elapsed()
        .map_or(0, |d| d.as_secs());
    Some((age, serde_json::from_slice(&std::fs::read(file).ok()?).ok()?))
}

/// Remove what was saved as `name`.
pub fn forget_saved(name: &str) {
    let _ = std::fs::remove_file(dir().join(format!("{name}.json")));
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
    let file = dir().join(format!("{name}.json"));
    let tmp = file.with_extension("part");
    let _ = std::fs::create_dir_all(dir());
    let _ = std::fs::write(&tmp, v.to_string()).and_then(|()| std::fs::rename(&tmp, &file));
}
