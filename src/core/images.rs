//! Web pictures for a frontend: emotes, badges and profile pictures are downloaded once by the core
//! into `cache/img` next to the settings file, and a frontend is told the file to draw. It decodes
//! only what it shows; the core never decodes a picture.

use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

/// Downloads at a time (a long chat shows a few hundred pictures at once).
const AT_ONCE: usize = 6;
/// The folder is cut down to this many bytes, the least recently written going first.
const KEEP_BYTES: u64 = 50 * 1024 * 1024;
/// Extensions a picture is stored with, as sniffed from its first bytes.
const EXTENSIONS: [&str; 5] = ["png", "gif", "webp", "jpg", "svg"];

thread_local! {
    /// Pictures on disk, by URL (this thread is the engine's: every request comes through it).
    static KNOWN: RefCell<HashMap<String, PathBuf>> = RefCell::default();
    /// URLs being downloaded or already tried this run.
    static ASKED: RefCell<HashSet<String>> = RefCell::default();
    static ARRIVED: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}
static FETCHING: tokio::sync::Semaphore = tokio::sync::Semaphore::const_new(AT_ONCE);

fn dir() -> PathBuf {
    crate::config_path().with_file_name("cache").join("img")
}

fn stem(url: &str) -> String {
    Sha256::digest(url.as_bytes())[..12]
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

/// The extension a picture's bytes call for (none: not a picture).
fn sniff(bytes: &[u8]) -> Option<&'static str> {
    match bytes {
        [0x89, b'P', b'N', b'G', ..] => Some("png"),
        [b'G', b'I', b'F', ..] => Some("gif"),
        [0xFF, 0xD8, ..] => Some("jpg"),
        [b'R', b'I', b'F', b'F', _, _, _, _, b'W', b'E', b'B', b'P', ..] => Some("webp"),
        _ => {
            let head = String::from_utf8_lossy(&bytes[..bytes.len().min(512)]).to_lowercase();
            (head.contains("<svg") || head.contains("<?xml")).then_some("svg")
        }
    }
}

/// The file holding `url`, if it is on disk; the download starts when it is not. `None` until it lands
/// (the core then tells the frontend with the images topic).
pub fn lookup(url: &str) -> Option<PathBuf> {
    if url.is_empty() || !url.starts_with("http") || crate::memory_saver() {
        return None;
    }
    if let Some(path) = KNOWN.with_borrow(|known| known.get(url).cloned()) {
        if path.is_file() {
            return Some(path);
        }
        KNOWN.with_borrow_mut(|known| known.remove(url));
    }
    let stem = stem(url);
    for extension in EXTENSIONS {
        let path = dir().join(format!("{stem}.{extension}"));
        if path.is_file() {
            KNOWN.with_borrow_mut(|known| {
                if known.len() > 4000 {
                    known.clear();
                }
                known.insert(url.to_owned(), path.clone());
            });
            return Some(path);
        }
    }
    if ASKED.with_borrow_mut(|asked| {
        if asked.len() > 20_000 {
            asked.clear();
        }
        asked.insert(url.to_owned())
    }) {
        fetch(url.to_owned(), stem);
    }
    None
}

fn fetch(url: String, stem: String) {
    tokio::task::spawn_local(async move {
        let _one = FETCHING.acquire().await;
        let Ok(response) = crate::http::request("GET", &url, &[], b"").await else {
            return;
        };
        if response.status != 200 {
            return;
        }
        let Some(extension) = sniff(&response.body) else {
            return;
        };
        let folder = dir();
        let file = folder.join(format!("{stem}.{extension}"));
        let part = folder.join(format!("{stem}.part"));
        // Whole or nothing: a half-written file would be a broken picture for good.
        let saved = std::fs::create_dir_all(&folder)
            .and_then(|()| std::fs::write(&part, &response.body))
            .and_then(|()| std::fs::rename(&part, &file));
        if saved.is_ok() {
            let count = ARRIVED.with(|arrived| {
                arrived.set(arrived.get() + 1);
                arrived.get()
            });
            if count % 200 == 1 {
                prune(&folder, KEEP_BYTES);
            }
            crate::host::images_changed();
        }
    });
}

/// Drop the least recently written pictures until the folder holds at most `keep` bytes.
fn prune(folder: &std::path::Path, keep: u64) {
    let Ok(entries) = std::fs::read_dir(folder) else {
        return;
    };
    let mut files: Vec<_> = entries
        .flatten()
        .filter_map(|entry| {
            let meta = entry.metadata().ok()?;
            Some((meta.modified().ok()?, meta.len(), entry.path()))
        })
        .collect();
    let mut total: u64 = files.iter().map(|file| file.1).sum();
    files.sort_unstable_by_key(|file| file.0);
    for (_, length, path) in files {
        if total <= keep {
            break;
        }
        if std::fs::remove_file(&path).is_ok() {
            total -= length;
        }
    }
}

/// `{url: file}` for the pictures that are on disk now; the others are fetched.
pub fn files(urls: &[&str]) -> Value {
    let mut found = serde_json::Map::new();
    for url in urls.iter().take(256) {
        if let Some(path) = lookup(url) {
            found.insert((*url).to_owned(), json!(path.display().to_string()));
        }
    }
    Value::Object(found)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pictures_are_named_by_what_they_are_not_by_their_address() {
        assert_eq!(sniff(b"\x89PNG\r\n\x1a\n"), Some("png"));
        assert_eq!(sniff(b"GIF89a"), Some("gif"));
        assert_eq!(sniff(b"\xFF\xD8\xFF\xE0"), Some("jpg"));
        assert_eq!(sniff(b"RIFF\0\0\0\0WEBPVP8 "), Some("webp"));
        assert_eq!(sniff(b"<svg xmlns=\"x\"/>"), Some("svg"));
        assert_eq!(sniff(b"<html>nope</html>"), None);
        assert_eq!(stem("https://a/b").len(), 24);
        assert_ne!(stem("https://a/b"), stem("https://a/c"));
    }

    #[test]
    fn a_folder_over_its_budget_loses_the_oldest_pictures() {
        let folder = std::env::temp_dir().join(format!("streamerz-images-{}", std::process::id()));
        std::fs::create_dir_all(&folder).unwrap();
        for (name, age) in [("old.png", 30), ("mid.png", 20), ("new.png", 10)] {
            let path = folder.join(name);
            std::fs::write(&path, [0u8; 100]).unwrap();
            let time = std::time::SystemTime::now() - std::time::Duration::from_secs(age);
            std::fs::File::options()
                .write(true)
                .open(&path)
                .unwrap()
                .set_modified(time)
                .unwrap();
        }
        prune(&folder, 150);
        assert!(!folder.join("old.png").exists() && !folder.join("mid.png").exists());
        assert!(folder.join("new.png").exists());
        let _ = std::fs::remove_dir_all(&folder);
    }

    #[test]
    fn nothing_is_fetched_for_a_non_web_address() {
        assert!(lookup("").is_none());
        assert!(lookup("file:///etc/passwd").is_none());
        assert_eq!(files(&["ftp://x"]), json!({}));
    }
}
