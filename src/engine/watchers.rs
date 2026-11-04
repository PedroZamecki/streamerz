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
            let read = tokio::task::spawn_blocking(move || std::fs::read(&p))
                .await
                .ok()
                .and_then(Result::ok);
            args.insert("fileWasDeleted".into(), read.is_none().into());
            let bytes = read.unwrap_or_default();
            args.insert("fileSize".into(), bytes.len().into());
            args.insert("empty".into(), bytes.is_empty().into());
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
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
        let watch = fswatch::watch(dir.clone(), false, tx);
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
                }
                Change::Created(p) | Change::Modified(p) | Change::Renamed(_, p) if this(&p) => {
                    let (p2, from) = (path.clone(), at);
                    let read = tokio::task::spawn_blocking(move || read_from(&p2, from)).await;
                    let Ok(Some((len, new))) = read else { continue };
                    // A file that got shorter starts again from its new end (as Streamer.bot).
                    if len < at {
                        at = len;
                        partial.clear();
                        continue;
                    }
                    at = len;
                    partial.extend_from_slice(&new);
                    while let Some(i) = partial.iter().position(|b| *b == b'\n') {
                        let line: Vec<u8> = partial.drain(..=i).collect();
                        let line = String::from_utf8_lossy(&line)
                            .trim_end_matches(['\n', '\r'])
                            .to_string();
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
