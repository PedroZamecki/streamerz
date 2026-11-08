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

/// Directory notifications may be packed at two-byte boundaries (including under Wine).
#[cfg(any(windows, test))]
fn notify_header(bytes: &[u8]) -> Option<(usize, u32, usize)> {
    let word = |at| Some(u32::from_le_bytes(bytes.get(at..at + 4)?.try_into().ok()?));
    let next = word(0)? as usize;
    let action = word(4)?;
    let length = word(8)? as usize;
    if !length.is_multiple_of(2) || length > bytes.len().checked_sub(12)? {
        return None;
    }
    if next != 0 && (next < 12 + length || !next.is_multiple_of(2) || next > bytes.len().checked_sub(12)?) {
        return None;
    }
    Some((next, action, length / 2))
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

    #[derive(Default)]
    struct Moves(HashMap<u32, (PathBuf, tokio::time::Instant)>);

    impl Moves {
        fn from(&mut self, cookie: u32, path: PathBuf, deadline: tokio::time::Instant) -> Option<Change> {
            self.0
                .insert(cookie, (path, deadline))
                .map(|(old, _)| Change::Deleted(old))
        }

        fn to(&mut self, cookie: u32, path: PathBuf) -> Change {
            match self.0.remove(&cookie) {
                Some((old, _)) => Change::Renamed(old, path),
                None => Change::Created(path),
            }
        }

        fn deadline(&self) -> Option<tokio::time::Instant> {
            self.0.values().map(|(_, deadline)| *deadline).min()
        }

        fn expire(&mut self, now: tokio::time::Instant, tx: &UnboundedSender<Change>) {
            self.0.retain(|_, (path, deadline)| {
                if *deadline <= now {
                    let _ = tx.send(Change::Deleted(path.clone()));
                    false
                } else {
                    true
                }
            });
        }
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
        watch_with_buffer(dir, recursive, tx, 16 * 1024).await
    }

    pub(super) async fn watch_with_buffer(
        dir: PathBuf,
        recursive: bool,
        tx: UnboundedSender<Change>,
        size: usize,
    ) -> std::io::Result<()> {
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
        let mut buf = vec![0u8; size];
        let mut moved = Moves::default();
        loop {
            // IN_MOVED_FROM and IN_MOVED_TO may be returned by separate reads. Only a move out
            // needs a short timeout; idle watchers have no timer.
            let mut guard = if let Some(deadline) = moved.deadline() {
                match tokio::time::timeout_at(deadline, ino.fd.readable()).await {
                    Ok(ready) => ready?,
                    Err(_) => {
                        moved.expire(tokio::time::Instant::now(), &tx);
                        continue;
                    }
                }
            } else {
                ino.fd.readable().await?
            };
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
                    if let Some(change) = moved.from(
                        ev.cookie,
                        path,
                        tokio::time::Instant::now() + std::time::Duration::from_millis(50),
                    ) {
                        let _ = tx.send(change);
                    }
                    continue;
                }
                if ev.mask & libc::IN_MOVED_TO != 0 {
                    let _ = tx.send(moved.to(ev.cookie, path.clone()));
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
        }
    }
    #[cfg(test)]
    mod tests {
        use super::*;
        #[test]
        fn interleaved_renames_keep_their_own_cookies() {
            let mut moves = Moves::default();
            let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(1);
            assert!(moves.from(1, "a".into(), deadline).is_none());
            assert!(moves.from(2, "b".into(), deadline).is_none());
            assert_eq!(moves.to(2, "d".into()), Change::Renamed("b".into(), "d".into()));
            assert_eq!(moves.to(1, "c".into()), Change::Renamed("a".into(), "c".into()));
            assert!(moves.deadline().is_none());
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
        FILE_NOTIFY_CHANGE_SIZE, FILE_SHARE_DELETE, FILE_SHARE_READ, FILE_SHARE_WRITE, OPEN_EXISTING,
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
                        if got as usize > buf.len() * 4 || at >= got as usize {
                            break;
                        }
                        // SAFETY: the initialized buffer holds `got` bytes; `at` is in bounds.
                        let remaining = unsafe { std::slice::from_raw_parts(bytes.add(at), got as usize - at) };
                        let Some((next, action, len)) = super::notify_header(remaining) else {
                            break;
                        };
                        // SAFETY: header validation bounds the UTF-16 name and keeps offsets even.
                        // The u32 buffer is aligned; no reference to a misaligned u32 record is made.
                        let name = unsafe { std::slice::from_raw_parts(bytes.add(at + 12).cast::<u16>(), len) };
                        let path = dir.join(std::ffi::OsString::from_wide(name));
                        let change = match action {
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
                        if next == 0 {
                            break;
                        }
                        at += next;
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
    #[test]
    fn packed_windows_notifications_do_not_require_u32_alignment() {
        let record = |next: u32, name: u16| {
            [
                next.to_le_bytes().as_slice(),
                &1u32.to_le_bytes(),
                &2u32.to_le_bytes(),
                &name.to_le_bytes(),
            ]
            .concat()
        };
        let data = [record(14, 65), record(0, 66)].concat();
        assert_eq!(super::notify_header(&data), Some((14, 1, 1)));
        assert_eq!(super::notify_header(&data[14..]), Some((0, 1, 1)));
        assert!(super::notify_header(&data[..13]).is_none());
        assert!(super::notify_header(&record(3, 65)).is_none());
        assert!(super::notify_header(&record(u32::MAX, 65)).is_none());
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn rename_pairs_events_from_separate_reads() {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        rt.block_on(async {
            let d = std::env::temp_dir().join(format!("streamerz-watch-split-{}", crate::engine::uuid()));
            std::fs::create_dir(&d).unwrap();
            std::fs::write(d.join("a"), b"test").unwrap();
            let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
            // An inotify event for either single-character name occupies 32 bytes.
            let task = tokio::spawn(super::imp::watch_with_buffer(d.clone(), false, tx, 32));
            tokio::task::yield_now().await;
            std::fs::rename(d.join("a"), d.join("b")).unwrap();
            let change = tokio::time::timeout(std::time::Duration::from_secs(2), rx.recv())
                .await
                .unwrap();
            assert_eq!(change, Some(super::Change::Renamed(d.join("a"), d.join("b"))));
            let outside = d.with_extension("outside");
            std::fs::rename(d.join("b"), &outside).unwrap();
            let change = tokio::time::timeout(std::time::Duration::from_secs(2), rx.recv())
                .await
                .unwrap();
            assert_eq!(change, Some(super::Change::Deleted(d.join("b"))));
            task.abort();
            let _ = std::fs::remove_file(outside);
            let _ = std::fs::remove_dir_all(d);
        });
    }
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
