//! File watchers and file tails (Streamer.bot's Services > File/Folder Watcher and File Tails): each
//! enabled one is a task waiting on the OS (`fswatch`), none when there are none. Their triggers pass
//! what Streamer.bot 1.0.7 passes (checked with `dev/sb-probe.py`).

use super::{s, Engine, Obj};
use crate::fswatch::{self, Change};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::time::Duration;
use tokio::time::Instant;

pub const T_CHANGED: u32 = 501;
pub const T_CREATED: u32 = 502;
pub const T_DELETED: u32 = 503;
pub const T_RENAMED: u32 = 504;
pub const T_TAIL: u32 = 511;

/// Writes to one file within this long are one change.
const SETTLE: Duration = Duration::from_millis(100);

fn on(o: &Obj, k: &str) -> bool {
    o.get(k).and_then(Value::as_bool).unwrap_or(false)
}

/// A trigger's watcher or tail: any (empty or all zeros) or this one.
fn picked(t: &Obj, key: &str, id: &str) -> bool {
    let want = s(t, key);
    want.is_empty() || want == super::DEFAULT_QUEUE || want == id
}

impl Engine {
    /// (Re)start the file watchers and file tails from the settings.
    pub fn watchers(self: &Rc<Self>) {
        for h in std::mem::take(&mut self.st.borrow_mut().watch_tasks) {
            h.abort();
        }
        let (watchers, tails) = {
            let c = self.cfg.borrow();
            (c.file_watchers.clone(), c.file_tails.clone())
        };
        let mut tasks = Vec::new();
        for w in watchers.into_iter().filter(|w| on(w, "enabled")) {
            tasks.push(tokio::task::spawn_local(self.clone().file_watcher(w)).abort_handle());
        }
        for t in tails.into_iter().filter(|t| on(t, "enabled")) {
            tasks.push(tokio::task::spawn_local(self.clone().file_tail(t)).abort_handle());
        }
        self.st.borrow_mut().watch_tasks = tasks;
    }

    /// Set File Tail State: 0 on, 1 off, 2 toggle.
    pub fn set_file_tail_state(self: &Rc<Self>, tail_id: &str, state: f64) {
        self.change(|c| {
            if let Some(t) = c.file_tails.iter_mut().find(|t| s(t, "id") == tail_id) {
                let now = on(t, "enabled");
                t.insert("enabled".into(), super::new_state(state, now).into());
            }
        });
        self.watchers();
    }

    async fn file_watcher(self: Rc<Self>, w: Obj) {
        let folder = PathBuf::from(s(&w, "folderPath"));
        // A file: its folder, and only it.
        let (dir, filter) = if folder.is_file() {
            let name = fswatch::name_of(&folder);
            (folder.parent().map(Path::to_path_buf).unwrap_or_default(), name)
        } else {
            (folder.clone(), s(&w, "filter").to_string())
        };
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
        let watch = fswatch::watch(dir.clone(), on(&w, "includeSubdirectories"), tx);
        tokio::pin!(watch);
        crate::say(format!("File watcher {}: watching {}", s(&w, "name"), dir.display()));
        let mut pending: HashMap<PathBuf, Instant> = HashMap::new();
        let wanted = |p: &Path| fswatch::matches(&filter, &fswatch::name_of(p));
        loop {
            let next = pending.values().min().copied();
            tokio::select! {
                r = &mut watch => {
                    if let Err(e) = r {
                        crate::warn(format!("File watcher {}: {} ({e})", s(&w, "name"), dir.display()));
                    }
                    return;
                }
                c = rx.recv() => match c {
                    None => return,
                    Some(Change::Modified(p)) if wanted(&p) && !p.is_dir() => {
                        pending.insert(p, Instant::now() + SETTLE);
                    }
                    Some(Change::Created(p)) if wanted(&p) && !p.is_dir() => {
                        self.watcher_fire(&w, &folder, T_CREATED, &p, None).await;
                    }
                    Some(Change::Deleted(p)) if wanted(&p) => {
                        pending.remove(&p);
                        self.watcher_fire(&w, &folder, T_DELETED, &p, None).await;
                    }
                    Some(Change::Renamed(old, new)) if (wanted(&old) || wanted(&new)) && !new.is_dir() => {
                        if let Some(t) = pending.remove(&old) {
                            pending.insert(new.clone(), t);
                        }
                        self.watcher_fire(&w, &folder, T_RENAMED, &new, Some(&old)).await;
                    }
                    Some(_) => {}
                },
                _ = tokio::time::sleep_until(next.unwrap_or_else(Instant::now)), if next.is_some() => {
                    let now = Instant::now();
                    let due: Vec<PathBuf> = pending.iter().filter(|(_, t)| **t <= now).map(|(p, _)| p.clone()).collect();
                    for p in due {
                        pending.remove(&p);
                        self.watcher_fire(&w, &folder, T_CHANGED, &p, None).await;
                    }
                }
            }
        }
    }

    async fn watcher_fire(self: &Rc<Self>, w: &Obj, folder: &Path, kind: u32, path: &Path, old: Option<&Path>) {
        let change = match kind {
            T_CREATED => "Created",
            T_DELETED => "Deleted",
            T_RENAMED => "Renamed",
            _ => "Changed",
        };
        let mut args = Obj::new();
        args.insert("watcherFolder".into(), folder.to_string_lossy().into_owned().into());
        args.insert("watcherFilter".into(), s(w, "filter").into());
        if let Some(old) = old {
            args.insert("oldFullPath".into(), old.to_string_lossy().into_owned().into());
            args.insert("oldFilename".into(), fswatch::name_of(old).into());
        }
        args.insert("fullPath".into(), path.to_string_lossy().into_owned().into());
        args.insert("fileName".into(), fswatch::name_of(path).into());
        args.insert("changeType".into(), change.into());
        if kind == T_CHANGED {
            let (json, lines) = (on(w, "asJson"), on(w, "addChangedData"));
            let p = path.to_path_buf();
            let data = tokio::task::spawn_blocking(move || changed_data(&p, json, lines))
                .await
                .unwrap_or_default();
            args.extend(data);
        }
        let id = s(w, "id").to_string();
        self.fire(kind, change, "Core/File/Folder Watcher", "filewatcher", &args, |t| {
            picked(t, "watcherId", &id)
        });
        self.emit("FileWatcher", change, || Value::Object(args));
    }

    async fn file_tail(self: Rc<Self>, t: Obj) {
        let path = PathBuf::from(s(&t, "filePath"));
        let (Some(dir), name) = (path.parent().map(Path::to_path_buf), fswatch::name_of(&path)) else {
            return;
        };
        // Lines there before don't fire: it starts at the end.
        let mut at = std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
        let mut partial: Vec<u8> = Vec::new();
        #[cfg(target_os = "linux")]
        let mut replacing = false;
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
        let watch = fswatch::watch_tail(dir.clone(), tx);
        tokio::pin!(watch);
        crate::say(format!("File tail {}: following {}", s(&t, "name"), path.display()));
        loop {
            let c = tokio::select! {
                r = &mut watch => {
                    if let Err(e) = r {
                        crate::warn(format!("File tail {}: {} ({e})", s(&t, "name"), dir.display()));
                    }
                    return;
                }
                c = rx.recv() => match c { Some(c) => c, None => return },
            };
            let this = |p: &Path| fswatch::name_of(p) == name;
            match c {
                Change::Deleted(p) if this(&p) => {
                    at = 0;
                    partial.clear();
                    #[cfg(target_os = "linux")]
                    {
                        replacing = false;
                    }
                }
                #[cfg(target_os = "linux")]
                Change::WriteClosed(p) if this(&p) && replacing => {
                    // O_TRUNC can notify while the writer has not written its replacement yet.
                    // The replacement's end is known when that write closes, not at the transient zero.
                    at = std::fs::metadata(&path).map(|meta| meta.len()).unwrap_or(0);
                    partial.clear();
                    replacing = false;
                }
                Change::Created(p) | Change::Modified(p) | Change::Renamed(_, p) if this(&p) => {
                    #[cfg(target_os = "linux")]
                    if replacing {
                        continue;
                    }
                    let (p2, from) = (path.clone(), at);
                    let read = tokio::task::spawn_blocking(move || read_from(&p2, from)).await;
                    let Ok(Some((len, new))) = read else { continue };
                    // A file that got shorter starts again from its new end (as Streamer.bot).
                    if len < at {
                        #[cfg(target_os = "linux")]
                        {
                            replacing = len == 0;
                        }
                        at = len;
                        partial.clear();
                        continue;
                    }
                    at = len;
                    for line in take_lines(&mut partial, &new) {
                        self.tail_fire(&t, &path, &line);
                    }
                }
                _ => {}
            }
        }
    }

    fn tail_fire(self: &Rc<Self>, t: &Obj, path: &Path, line: &str) {
        let mut args = Obj::new();
        args.insert("filePath".into(), path.to_string_lossy().into_owned().into());
        args.insert("fileName".into(), fswatch::name_of(path).into());
        args.insert("line".into(), line.into());
        let id = s(t, "id").to_string();
        self.fire(T_TAIL, "Changed", "Core/File IO/File Tail", "filetail", &args, |x| {
            picked(x, "tailId", &id)
        });
        self.emit("FileTail", "Changed", || json!(args));
    }
}

/// What Changed says of a file: whether it's gone, its size and, when the watcher adds it, what it
/// holds (as JSON or as lines). Only then is the file read: a big file that keeps changing isn't.
fn changed_data(path: &Path, json: bool, lines: bool) -> Obj {
    let mut args = Obj::new();
    let (size, bytes) = if json || lines {
        match std::fs::read(path) {
            Ok(bytes) => (Some(bytes.len() as u64), bytes),
            Err(_) => (None, Vec::new()),
        }
    } else {
        (std::fs::metadata(path).ok().map(|m| m.len()), Vec::new())
    };
    args.insert("fileWasDeleted".into(), size.is_none().into());
    let size = size.unwrap_or(0);
    args.insert("fileSize".into(), size.into());
    args.insert("empty".into(), (size == 0).into());
    let text = String::from_utf8_lossy(&bytes);
    if json {
        if let Ok(v) = serde_json::from_str::<Value>(&text) {
            match &v {
                Value::Object(o) => o.iter().for_each(|(k, x)| super::rest::flatten(k, x, &mut args)),
                other => super::rest::flatten("json", other, &mut args),
            }
        }
    } else if lines {
        let all: Vec<&str> = text.lines().collect();
        args.insert("lineCount".into(), all.len().into());
        for (i, l) in all.iter().enumerate() {
            args.insert(format!("line{i}"), (*l).into());
            args.insert(format!("lineEscaped{i}"), crate::commands::url_encode(l).into());
        }
    }
    args
}

/// The longest line a tail keeps waiting for its end: a file that never writes a newline would
/// otherwise grow the memory for ever. Past it, what came is a line of its own.
const MAX_LINE: usize = 64 * 1024;

/// Adds what came to the line not ended yet; the lines now whole (no line end), oldest first.
fn take_lines(partial: &mut Vec<u8>, new: &[u8]) -> Vec<String> {
    let mut lines = Vec::new();
    let mut emit = |bytes: &[u8]| {
        let line = String::from_utf8_lossy(bytes);
        lines.push(line.trim_end_matches(['\n', '\r']).to_string());
    };
    partial.extend_from_slice(new);
    let mut start = 0;
    while let Some(i) = partial[start..].iter().position(|b| *b == b'\n') {
        emit(&partial[start..=start + i]);
        start += i + 1;
    }
    while partial.len() - start > MAX_LINE {
        emit(&partial[start..start + MAX_LINE]);
        start += MAX_LINE;
    }
    partial.drain(..start);
    lines
}

/// The file's length, and what is past `from` (nothing when it got shorter).
fn read_from(path: &Path, from: u64) -> Option<(u64, Vec<u8>)> {
    use std::io::{Read, Seek, SeekFrom};
    let mut f = std::fs::File::open(path).ok()?;
    let len = f.metadata().ok()?.len();
    if len <= from {
        return Some((len, Vec::new()));
    }
    f.seek(SeekFrom::Start(from)).ok()?;
    let mut out = Vec::new();
    f.take(len - from).read_to_end(&mut out).ok()?;
    Some((len, out))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("streamerz-watchers-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn a_change_gives_the_size_and_reads_the_file_only_when_asked() {
        let dir = scratch("changed");
        let file = dir.join("a.txt");
        std::fs::write(&file, "one\ntwo\n").unwrap();
        let plain = changed_data(&file, false, false);
        assert_eq!(plain["fileSize"], 8);
        assert_eq!(plain["empty"], false);
        assert_eq!(plain["fileWasDeleted"], false);
        assert!(!plain.contains_key("lineCount"));
        let lines = changed_data(&file, false, true);
        assert_eq!(lines["lineCount"], 2);
        assert_eq!(lines["line1"], "two");
        std::fs::write(&file, r#"{"score": 3}"#).unwrap();
        let json = changed_data(&file, true, false);
        assert_eq!(json["score"], 3);
        std::fs::remove_file(&file).unwrap();
        let gone = changed_data(&file, false, false);
        assert_eq!(gone["fileWasDeleted"], true);
        assert_eq!(gone["fileSize"], 0);
        assert_eq!(gone["empty"], true);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_tail_reads_only_what_came_after() {
        let dir = scratch("tail");
        let file = dir.join("log.txt");
        std::fs::write(&file, "old\nnew\n").unwrap();
        assert_eq!(read_from(&file, 4), Some((8, b"new\n".to_vec())));
        assert_eq!(read_from(&file, 8), Some((8, Vec::new())));
        // It got shorter: nothing new, and the new length says so.
        assert_eq!(read_from(&file, 20), Some((8, Vec::new())));
        assert_eq!(read_from(&dir.join("none"), 0), None);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_tail_gives_whole_lines_and_keeps_the_rest_for_later() {
        let mut partial = Vec::new();
        assert_eq!(take_lines(&mut partial, b"one\r\ntw"), ["one"]);
        assert_eq!(partial, b"tw");
        assert_eq!(take_lines(&mut partial, b"o\n\nthree"), ["two", ""]);
        assert_eq!(take_lines(&mut partial, b"\n"), ["three"]);
        assert!(partial.is_empty());
    }

    /// A file that never writes a newline doesn't grow the memory: past 64 KB, what came is a line.
    #[test]
    fn a_line_with_no_end_is_given_once_it_is_too_long() {
        let mut partial = Vec::new();
        assert!(take_lines(&mut partial, &vec![b'a'; MAX_LINE]).is_empty());
        assert_eq!(partial.len(), MAX_LINE);
        let lines = take_lines(&mut partial, &vec![b'b'; MAX_LINE + 10]);
        assert_eq!(lines.len(), 2);
        assert_eq!(lines[0], "a".repeat(MAX_LINE));
        assert_eq!(lines[1], "b".repeat(MAX_LINE));
        assert_eq!(partial, vec![b'b'; 10]);
        assert_eq!(take_lines(&mut partial, b"c\n"), ["bbbbbbbbbbc"]);
        assert!(partial.is_empty());
    }

    #[test]
    fn a_trigger_picks_any_watcher_or_its_own() {
        let t = |v: Value| v.as_object().unwrap().clone();
        assert!(picked(&t(json!({})), "watcherId", "w1"));
        assert!(picked(
            &t(json!({"watcherId": super::super::DEFAULT_QUEUE})),
            "watcherId",
            "w1"
        ));
        assert!(picked(&t(json!({"watcherId": "w1"})), "watcherId", "w1"));
        assert!(!picked(&t(json!({"watcherId": "w2"})), "watcherId", "w1"));
    }
}
