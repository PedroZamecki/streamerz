//! What changes in a folder, from the OS itself (no polling): inotify on Linux, ReadDirectoryChangesW
//! on Windows. Used by the file watchers and file tails (`engine/watchers.rs`).
//!
//! A watch is a future: it runs until it is dropped (its task aborted), sending each change.

use std::path::{Path, PathBuf};
use tokio::sync::mpsc::UnboundedSender;

/// A change in a watched folder. `Modified` comes for each write; callers wait for it to settle.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Change {
    Created(PathBuf),
    Modified(PathBuf),
    Deleted(PathBuf),
    Renamed(PathBuf, PathBuf),
}

/// Watches `dir` (and the folders in it, `recursive`), sending each change until dropped. An error:
/// the folder can't be watched (missing, no access).
pub async fn watch(dir: PathBuf, recursive: bool, tx: UnboundedSender<Change>) -> std::io::Result<()> {
    imp::watch(dir, recursive, tx).await
}

/// Whether `name` matches a Windows-style filter (`*.txt`, `log?.txt`; `*.*` and empty: anything),
/// without caring about case.
pub fn matches(filter: &str, name: &str) -> bool {
    let f = filter.trim();
    if f.is_empty() || f == "*" || f == "*.*" {
        return true;
    }
    fn go(p: &[char], n: &[char]) -> bool {
        match p.split_first() {
            None => n.is_empty(),
            Some(('*', rest)) => (0..=n.len()).any(|i| go(rest, &n[i..])),
            Some(('?', rest)) => !n.is_empty() && go(rest, &n[1..]),
            Some((c, rest)) => n.first().is_some_and(|x| x.eq_ignore_ascii_case(c)) && go(rest, &n[1..]),
        }
    }
    let (p, n): (Vec<char>, Vec<char>) = (f.chars().collect(), name.chars().collect());
    go(&p, &n)
}

/// A file's name as text.
pub fn name_of(p: &Path) -> String {
    p.file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default()
}

#[cfg(target_os = "linux")]
mod imp {
    use super::Change;
    use std::collections::HashMap;
    use std::ffi::{CString, OsStr};
    use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};
    use std::os::unix::ffi::OsStrExt;
    use std::path::{Path, PathBuf};
    use tokio::io::unix::AsyncFd;
    use tokio::sync::mpsc::UnboundedSender;

    const MASK: u32 = libc::IN_CREATE
        | libc::IN_MODIFY
        | libc::IN_DELETE
        | libc::IN_MOVED_FROM
        | libc::IN_MOVED_TO
        | libc::IN_DELETE_SELF;

    struct Inotify {
        fd: AsyncFd<OwnedFd>,
        dirs: HashMap<i32, PathBuf>,
    }

    impl Inotify {
        fn add(&mut self, dir: &Path, recursive: bool) {
            let Ok(c) = CString::new(dir.as_os_str().as_bytes()) else {
                return;
            };
            // SAFETY: a valid inotify fd and a NUL-terminated path.
            let wd = unsafe { libc::inotify_add_watch(self.fd.get_ref().as_raw_fd(), c.as_ptr(), MASK) };
            if wd < 0 {
                return;
            }
            self.dirs.insert(wd, dir.to_path_buf());
            if recursive {
                for e in std::fs::read_dir(dir).into_iter().flatten().flatten() {
                    if e.file_type().is_ok_and(|t| t.is_dir()) {
                        self.add(&e.path(), true);
                    }
                }
            }
        }
    }

    pub async fn watch(dir: PathBuf, recursive: bool, tx: UnboundedSender<Change>) -> std::io::Result<()> {
        if !dir.is_dir() {
            return Err(std::io::Error::new(std::io::ErrorKind::NotFound, "no such folder"));
        }
        // SAFETY: inotify_init1 returns a new fd or -1.
        let raw = unsafe { libc::inotify_init1(libc::IN_NONBLOCK | libc::IN_CLOEXEC) };
        if raw < 0 {
            return Err(std::io::Error::last_os_error());
        }
        // SAFETY: raw is a fresh fd we own.
        let fd = unsafe { OwnedFd::from_raw_fd(raw) };
        let mut ino = Inotify {
            fd: AsyncFd::new(fd)?,
            dirs: HashMap::new(),
        };
        ino.add(&dir, recursive);
        let mut buf = vec![0u8; 16 * 1024];
        loop {
            let mut guard = ino.fd.readable().await?;
            // SAFETY: reading into our buffer from the inotify fd.
            let n = unsafe { libc::read(ino.fd.get_ref().as_raw_fd(), buf.as_mut_ptr().cast(), buf.len()) };
            if n < 0 {
                let e = std::io::Error::last_os_error();
                if e.kind() == std::io::ErrorKind::WouldBlock {
                    guard.clear_ready();
                    continue;
                }
                return Err(e);
            }
            let mut at = 0usize;
            // A move out of one watched folder waits for its other half in the same read.
            let mut moved: Option<(u32, PathBuf)> = None;
            let head = std::mem::size_of::<libc::inotify_event>();
            while at + head <= n as usize {
                // SAFETY: the kernel wrote whole events; read_unaligned copes with the byte buffer.
                let ev: libc::inotify_event = unsafe { std::ptr::read_unaligned(buf.as_ptr().add(at).cast()) };
                let name = &buf[at + head..at + head + ev.len as usize];
                let name = &name[..name.iter().position(|b| *b == 0).unwrap_or(name.len())];
                at += head + ev.len as usize;
                let Some(base) = ino.dirs.get(&ev.wd).cloned() else {
                    continue;
                };
                if ev.mask & libc::IN_DELETE_SELF != 0 && base == dir {
                    return Ok(());
                }
                let path = base.join(OsStr::from_bytes(name));
                let is_dir = ev.mask & libc::IN_ISDIR != 0;
                if ev.mask & libc::IN_MOVED_FROM != 0 {
                    if let Some((_, old)) = moved.take() {
                        let _ = tx.send(Change::Deleted(old));
                    }
                    moved = Some((ev.cookie, path));
                    continue;
                }
                if ev.mask & libc::IN_MOVED_TO != 0 {
                    match moved.take() {
                        Some((cookie, old)) if cookie == ev.cookie => drop(tx.send(Change::Renamed(old, path.clone()))),
                        other => {
                            if let Some((_, old)) = other {
                                let _ = tx.send(Change::Deleted(old));
                            }
                            let _ = tx.send(Change::Created(path.clone()));
                        }
                    }
                    if is_dir && recursive {
                        ino.add(&path, true);
                    }
                    continue;
                }
                let change = if ev.mask & libc::IN_CREATE != 0 {
                    if is_dir && recursive {
                        ino.add(&path, true);
                    }
                    Change::Created(path)
                } else if ev.mask & libc::IN_DELETE != 0 {
                    Change::Deleted(path)
                } else if ev.mask & libc::IN_MODIFY != 0 {
                    Change::Modified(path)
                } else {
                    continue;
                };
                let _ = tx.send(change);
            }
            if let Some((_, old)) = moved {
                let _ = tx.send(Change::Deleted(old));
            }
        }
    }
}

#[cfg(windows)]
mod imp {
    use super::Change;
    use std::os::windows::ffi::{OsStrExt, OsStringExt};
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicBool, AtomicIsize, Ordering};
    use std::sync::Arc;
    use tokio::sync::mpsc::UnboundedSender;
    use windows_sys::Win32::Foundation::{CloseHandle, HANDLE, INVALID_HANDLE_VALUE};
    use windows_sys::Win32::Storage::FileSystem::{
        CreateFileW, ReadDirectoryChangesW, FILE_ACTION_ADDED, FILE_ACTION_MODIFIED, FILE_ACTION_REMOVED,
        FILE_ACTION_RENAMED_NEW_NAME, FILE_ACTION_RENAMED_OLD_NAME, FILE_FLAG_BACKUP_SEMANTICS, FILE_LIST_DIRECTORY,
        FILE_NOTIFY_CHANGE_DIR_NAME, FILE_NOTIFY_CHANGE_FILE_NAME, FILE_NOTIFY_CHANGE_LAST_WRITE,
        FILE_NOTIFY_CHANGE_SIZE, FILE_NOTIFY_INFORMATION, FILE_SHARE_DELETE, FILE_SHARE_READ, FILE_SHARE_WRITE,
        OPEN_EXISTING,
    };
    use windows_sys::Win32::System::IO::CancelIoEx;

    /// Stops the blocking read when the watch is dropped.
    struct Stop(Arc<AtomicIsize>, Arc<AtomicBool>);

    impl Drop for Stop {
        fn drop(&mut self) {
            self.1.store(true, Ordering::Relaxed);
            let h = self.0.load(Ordering::Relaxed);
            if h != 0 {
                // SAFETY: the handle stays open until the reading thread sees `stopped`.
                unsafe { CancelIoEx(h as HANDLE, std::ptr::null()) };
            }
        }
    }

    pub async fn watch(dir: PathBuf, recursive: bool, tx: UnboundedSender<Change>) -> std::io::Result<()> {
        let wide: Vec<u16> = dir.as_os_str().encode_wide().chain(Some(0)).collect();
        // SAFETY: a NUL-terminated path; the handle is closed by the reading thread.
        let h = unsafe {
            CreateFileW(
                wide.as_ptr(),
                FILE_LIST_DIRECTORY,
                FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
                std::ptr::null(),
                OPEN_EXISTING,
                FILE_FLAG_BACKUP_SEMANTICS,
                std::ptr::null_mut(),
            )
        };
        if h == INVALID_HANDLE_VALUE {
            return Err(std::io::Error::last_os_error());
        }
        let raw = h as isize;
        let handle = Arc::new(AtomicIsize::new(raw));
        let stopped = Arc::new(AtomicBool::new(false));
        let _stop = Stop(handle.clone(), stopped.clone());
        let read = tokio::task::spawn_blocking(move || {
            let h = raw as HANDLE;
            let mut buf = vec![0u32; 16 * 1024];
            let mut old: Option<PathBuf> = None;
            let filter = FILE_NOTIFY_CHANGE_FILE_NAME
                | FILE_NOTIFY_CHANGE_DIR_NAME
                | FILE_NOTIFY_CHANGE_LAST_WRITE
                | FILE_NOTIFY_CHANGE_SIZE;
            let out = loop {
                let mut got = 0u32;
                // SAFETY: a directory handle and our aligned buffer; synchronous call.
                let ok = unsafe {
                    ReadDirectoryChangesW(
                        h,
                        buf.as_mut_ptr().cast(),
                        (buf.len() * 4) as u32,
                        recursive as i32,
                        filter,
                        &mut got,
                        std::ptr::null_mut(),
                        None,
                    )
                };
                if stopped.load(Ordering::Relaxed) {
                    break Ok(());
                }
                if ok == 0 {
                    break Err(std::io::Error::last_os_error());
                }
                let bytes = buf.as_ptr().cast::<u8>();
                let mut at = 0usize;
                // 0 bytes: more changed than the buffer holds (they are lost).
                if got > 0 {
                    loop {
                        // SAFETY: the OS filled whole, DWORD-aligned records.
                        let rec = unsafe { &*(bytes.add(at) as *const FILE_NOTIFY_INFORMATION) };
                        let len = rec.FileNameLength as usize / 2;
                        // SAFETY: FileName holds `len` UTF-16 units.
                        let name = unsafe { std::slice::from_raw_parts(rec.FileName.as_ptr(), len) };
                        let path = dir.join(std::ffi::OsString::from_wide(name));
                        let change = match rec.Action {
                            FILE_ACTION_ADDED => Some(Change::Created(path)),
                            FILE_ACTION_REMOVED => Some(Change::Deleted(path)),
                            FILE_ACTION_MODIFIED => Some(Change::Modified(path)),
                            FILE_ACTION_RENAMED_OLD_NAME => {
                                old = Some(path);
                                None
                            }
                            FILE_ACTION_RENAMED_NEW_NAME => Some(
                                old.take()
                                    .map_or(Change::Created(path.clone()), |o| Change::Renamed(o, path)),
                            ),
                            _ => None,
                        };
                        if let Some(c) = change {
                            if tx.send(c).is_err() {
                                break;
                            }
                        }
                        if rec.NextEntryOffset == 0 {
                            break;
                        }
                        at += rec.NextEntryOffset as usize;
                    }
                }
            };
            handle.store(0, Ordering::Relaxed);
            // SAFETY: our handle, closed once.
            unsafe { CloseHandle(h) };
            out
        });
        read.await.unwrap_or(Ok(()))
    }
}

#[cfg(not(any(target_os = "linux", windows)))]
mod imp {
    pub async fn watch(
        _: std::path::PathBuf,
        _: bool,
        _: tokio::sync::mpsc::UnboundedSender<super::Change>,
    ) -> std::io::Result<()> {
        Err(std::io::Error::new(std::io::ErrorKind::Unsupported, "file watching"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn filters_match_like_windows() {
        assert!(matches("*.*", "README"));
        assert!(matches("", "a.txt"));
        assert!(matches("*.TXT", "notes.txt"));
        assert!(matches("log?.txt", "log1.txt"));
        assert!(!matches("log?.txt", "log10.txt"));
        assert!(!matches("*.json", "a.txt"));
    }

    /// Created, written, renamed and deleted, in a folder, one inside it and one made while watching.
    #[test]
    fn a_folder_reports_its_changes() {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        rt.block_on(async {
            let d = std::env::temp_dir().join(format!("streamerz-watch-{}", std::process::id()));
            let _ = std::fs::remove_dir_all(&d);
            std::fs::create_dir_all(d.join("sub")).unwrap();
            let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
            let w = tokio::spawn(watch(d.clone(), true, tx));
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
            std::fs::write(d.join("a.txt"), "hi").unwrap();
            std::fs::rename(d.join("a.txt"), d.join("b.txt")).unwrap();
            std::fs::remove_file(d.join("b.txt")).unwrap();
            std::fs::write(d.join("sub").join("deep.txt"), "x").unwrap();
            // A folder made while watching is watched too.
            std::fs::create_dir(d.join("new")).unwrap();
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
            std::fs::write(d.join("new").join("late.txt"), "x").unwrap();
            let mut got = Vec::new();
            while let Ok(Some(c)) = tokio::time::timeout(std::time::Duration::from_millis(300), rx.recv()).await {
                got.push(c);
            }
            w.abort();
            let want = [
                Change::Created(d.join("a.txt")),
                Change::Modified(d.join("a.txt")),
                Change::Renamed(d.join("a.txt"), d.join("b.txt")),
                Change::Deleted(d.join("b.txt")),
                Change::Created(d.join("sub").join("deep.txt")),
                Change::Created(d.join("new").join("late.txt")),
            ];
            for x in want {
                // Wine watches only the folders made while watching, not those already there.
                #[cfg(windows)]
                if crate::under_wine() && x == Change::Created(d.join("sub").join("deep.txt")) {
                    continue;
                }
                assert!(got.contains(&x), "{x:?} missing from {got:?}");
            }
            let _ = std::fs::remove_dir_all(&d);
        });
    }
}
