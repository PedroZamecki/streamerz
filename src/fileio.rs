//! File I/O, File Tail, File Watcher and Process Triggers (Phase 17).
//!
//! Sub-actions:
//! - 3: Write To File (file, text, append)
//! - 5: Read Random Line From File (file, parseVariables, autoType, variableName)
//! - 1006: Read Lines From File (file, parseVariables, autoType, variableName)
//! - 1021: Read Specific Line From File (file, parseVariables, autoType, count, variableName)
//! - 1030: Set File Tail State (fileTailId, state)
//! - 1039: File Exists (file, variableName)
//! - 1040: Folder Exists (folder, variableName)
//!
//! Triggers:
//! - 501: FileWatcherChanged
//! - 502: FileWatcherCreated
//! - 503: FileWatcherDeleted
//! - 504: FileWatcherRenamed
//! - 511: FileTailChanged
//! - 703: ProcessStarted
//! - 704: ProcessStopped

use std::fs::OpenOptions;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

pub const S_WRITE_TO_FILE: u32 = 3;
pub const S_READ_RANDOM_LINE: u32 = 5;
pub const S_READ_LINES: u32 = 1006;
pub const S_READ_SPECIFIC_LINE: u32 = 1021;
pub const S_SET_FILE_TAIL_STATE: u32 = 1030;
pub const S_FILE_EXISTS: u32 = 1039;
pub const S_FOLDER_EXISTS: u32 = 1040;

pub const T_FILE_WATCHER_CHANGED: u32 = 501;
pub const T_FILE_WATCHER_CREATED: u32 = 502;
pub const T_FILE_WATCHER_DELETED: u32 = 503;
pub const T_FILE_WATCHER_RENAMED: u32 = 504;
pub const T_FILE_TAIL_CHANGED: u32 = 511;
pub const T_PROCESS_STARTED: u32 = 703;
pub const T_PROCESS_STOPPED: u32 = 704;

/// The line break Append puts before its text: Windows' own there (as Streamer.bot writes it), `\n` here.
const NEWLINE: &str = if cfg!(windows) { "\r\n" } else { "\n" };

/// Write or append text to a file. Append starts a new line when the file is already there (even
/// empty), as Streamer.bot does; a file it makes starts with the text. Writes in place like .NET
/// `File.WriteAllText` so OS file watchers see Created/Changed events instead of a rename.
pub fn write_to_file(path: &Path, text: &str, append: bool) -> io::Result<()> {
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)?;
        }
    }
    let text = if append && path.exists() {
        format!("{NEWLINE}{text}")
    } else {
        text.to_string()
    };
    let mut file = OpenOptions::new()
        .create(true)
        .write(true)
        .append(append)
        .truncate(!append)
        .open(path)?;
    file.write_all(text.as_bytes())?;
    file.flush()
}

fn decode_text(bytes: &[u8]) -> io::Result<String> {
    if let Some(rest) = bytes.strip_prefix(b"\xef\xbb\xbf") {
        String::from_utf8(rest.to_vec()).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))
    } else if let Some(rest) = bytes.strip_prefix(b"\xff\xfe") {
        let (pairs, rem) = rest.as_chunks::<2>();
        if !rem.is_empty() {
            return Err(io::Error::new(io::ErrorKind::InvalidData, "truncated UTF-16 LE text"));
        }
        let units: Vec<u16> = pairs.iter().copied().map(u16::from_le_bytes).collect();
        String::from_utf16(&units).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))
    } else if let Some(rest) = bytes.strip_prefix(b"\xfe\xff") {
        let (pairs, rem) = rest.as_chunks::<2>();
        if !rem.is_empty() {
            return Err(io::Error::new(io::ErrorKind::InvalidData, "truncated UTF-16 BE text"));
        }
        let units: Vec<u16> = pairs.iter().copied().map(u16::from_be_bytes).collect();
        String::from_utf16(&units).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))
    } else {
        String::from_utf8(bytes.to_vec()).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))
    }
}

/// Read all lines from a file (UTF-8, UTF-8 with BOM, or UTF-16 with BOM).
pub fn read_lines(path: &Path) -> io::Result<Vec<String>> {
    let bytes = std::fs::read(path)?;
    let content = decode_text(&bytes)?;
    Ok(content.lines().map(|s| s.to_string()).collect())
}

/// Read a specific 1-indexed line from a file.
pub fn read_specific_line(path: &Path, line_num: usize) -> io::Result<Option<String>> {
    let lines = read_lines(path)?;
    if line_num > 0 && line_num <= lines.len() {
        Ok(Some(lines[line_num - 1].clone()))
    } else {
        Ok(None)
    }
}

/// Read a random line from a file.
pub fn read_random_line(path: &Path) -> io::Result<Option<String>> {
    let lines = read_lines(path)?;
    if lines.is_empty() {
        Ok(None)
    } else {
        let idx = (crate::math::secure_u64() as usize) % lines.len();
        Ok(Some(lines[idx].clone()))
    }
}

/// Check if a file exists.
pub fn file_exists(path: &Path) -> bool {
    path.is_file()
}

/// Check if a folder exists.
pub fn folder_exists(path: &Path) -> bool {
    path.is_dir()
}

/// Makes the folder `path` goes in, when it has one.
pub fn make_parent(path: &Path) -> io::Result<()> {
    match path.parent() {
        Some(parent) if !parent.as_os_str().is_empty() => std::fs::create_dir_all(parent),
        _ => Ok(()),
    }
}

/// Writes `bytes` to `path` whole or not at all (making its folder): see `replace`.
pub fn write_atomic(path: &Path, bytes: &[u8]) -> io::Result<()> {
    make_parent(path)?;
    replace(path, None, |f| f.write_all(bytes))
}

/// `value` as JSON, written to `path` like `write_atomic` but a buffer at a time (no copy in memory).
pub fn write_json(path: &Path, value: &impl serde::Serialize) -> io::Result<()> {
    make_parent(path)?;
    replace(path, None, |f| {
        let mut w = io::BufWriter::new(f);
        serde_json::to_writer(&mut w, value)?;
        w.flush()
    })
}

/// Replaces `path` whole or not at all: `fill` writes a new file beside it, which is flushed to the
/// disk and renamed over it, so a crash or a power cut leaves the old file or the new one, never an
/// empty or half one. `perms`, when given, are set before anything is written. The folder must exist.
pub fn replace(
    path: &Path,
    perms: Option<std::fs::Permissions>,
    fill: impl FnOnce(&mut std::fs::File) -> io::Result<()>,
) -> io::Result<()> {
    let tmp = stage(path, perms, fill)?;
    commit(&tmp, path)
}

/// `replace`'s first half: the new file, written and flushed beside `path` but not in its place yet.
pub fn stage(
    path: &Path,
    perms: Option<std::fs::Permissions>,
    fill: impl FnOnce(&mut std::fs::File) -> io::Result<()>,
) -> io::Result<PathBuf> {
    let tmp = temp_beside(path);
    let mut o = OpenOptions::new();
    o.write(true).create_new(true);
    #[cfg(unix)]
    if let Some(p) = &perms {
        use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
        o.mode(p.mode() & 0o7777);
    }
    let done = o.open(&tmp).and_then(|mut f| {
        if let Some(p) = perms {
            f.set_permissions(p)?;
        }
        fill(&mut f)?;
        f.sync_all()
    });
    match done {
        Ok(()) => Ok(tmp),
        Err(e) => {
            let _ = std::fs::remove_file(&tmp);
            Err(e)
        }
    }
}

/// `replace`'s second half: puts a staged file in `path`'s place (removing it when that fails).
pub fn commit(tmp: &Path, path: &Path) -> io::Result<()> {
    let done = retry_rename(
        || std::fs::rename(tmp, path),
        || std::thread::sleep(std::time::Duration::from_millis(50)),
    );
    if done.is_err() {
        let _ = std::fs::remove_file(tmp);
    }
    done
}

/// Puts a finished download (`part`, written by someone else) in `path`'s place, flushed to the disk
/// first so a power cut can't leave an empty file under the final name.
pub fn settle(part: &Path, path: &Path) -> io::Result<()> {
    if let Err(e) = OpenOptions::new().write(true).open(part).and_then(|f| f.sync_all()) {
        let _ = std::fs::remove_file(part);
        return Err(e);
    }
    commit(part, path)
}

/// A new name beside `path` that no other writer (another thread or instance) uses. It starts with a
/// dot and ends in `.tmp`, so folder listings and watchers pass over it.
fn temp_beside(path: &Path) -> PathBuf {
    use std::sync::atomic::{AtomicU64, Ordering};
    static SEQ: AtomicU64 = AtomicU64::new(0);
    let seq = SEQ.fetch_add(1, Ordering::Relaxed);
    let mut name = std::ffi::OsString::from(".");
    name.push(path.file_name().unwrap_or_default());
    name.push(format!(".{}.{seq}.tmp", std::process::id()));
    path.with_file_name(name)
}

/// Windows refuses a rename while another program (an antivirus, a backup tool) has the file open:
/// that passes in a moment, so it is tried a few times.
pub(crate) fn retry_rename(mut rename: impl FnMut() -> io::Result<()>, mut wait: impl FnMut()) -> io::Result<()> {
    for _ in 0..4 {
        match rename() {
            Ok(()) => return Ok(()),
            Err(e)
                if matches!(e.kind(), io::ErrorKind::PermissionDenied | io::ErrorKind::WouldBlock)
                    || (cfg!(windows) && matches!(e.raw_os_error(), Some(32 | 33))) =>
            {
                wait();
            }
            Err(e) => return Err(e),
        }
    }
    rename()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Checked against Streamer.bot 1.0.7: append to a new file "x", then "y": `x\r\ny`; to an empty
    /// file: `\r\nz`.
    #[test]
    fn append_starts_a_new_line_only_in_a_file_already_there() {
        let d = std::env::temp_dir().join(format!("streamerz-append-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&d);
        let (new, empty) = (d.join("new.txt"), d.join("empty.txt"));
        write_to_file(&new, "x", true).unwrap();
        write_to_file(&new, "y", true).unwrap();
        write_to_file(&empty, "", false).unwrap();
        write_to_file(&empty, "z", true).unwrap();
        assert_eq!(std::fs::read_to_string(&new).unwrap(), format!("x{NEWLINE}y"));
        assert_eq!(std::fs::read_to_string(&empty).unwrap(), format!("{NEWLINE}z"));
        let _ = std::fs::remove_dir_all(d);
    }

    #[test]
    fn file_write_and_read_lines() {
        let dir = std::env::temp_dir().join(format!("streamerz_fileio_test_{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let file = dir.join("test.txt");

        assert!(!file_exists(&file));
        assert!(folder_exists(&dir));

        assert!(write_to_file(&file, "line1\nline2\nline3", false).is_ok());
        assert!(file_exists(&file));

        let lines = read_lines(&file).unwrap();
        assert_eq!(lines.len(), 3);
        assert_eq!(lines[0], "line1");
        assert_eq!(lines[1], "line2");
        assert_eq!(lines[2], "line3");

        assert_eq!(read_specific_line(&file, 2).unwrap(), Some("line2".into()));
        assert_eq!(read_specific_line(&file, 5).unwrap(), None);

        let rand = read_random_line(&file).unwrap();
        assert!(rand.is_some());
        assert!(lines.contains(&rand.unwrap()));

        // Append: on a new line, as Streamer.bot writes it.
        assert!(write_to_file(&file, "line4", true).is_ok());
        let lines2 = read_lines(&file).unwrap();
        assert_eq!(lines2.len(), 4);
        assert_eq!(lines2[3], "line4");

        // Overwrite truncates in place so file watchers still see Created/Changed events.
        write_to_file(&file, "fresh", false).unwrap();
        assert_eq!(read_lines(&file).unwrap(), vec!["fresh"]);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn read_lines_strips_utf8_and_utf16_boms_and_handles_empty() {
        let dir = std::env::temp_dir().join(format!("streamerz_fileio_bom_{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);

        let utf8 = dir.join("utf8.txt");
        std::fs::write(&utf8, b"\xef\xbb\xbf42\r\nsecond").unwrap();
        assert_eq!(read_lines(&utf8).unwrap(), vec!["42", "second"]);

        let utf16le = dir.join("utf16le.txt");
        let mut le = vec![0xff, 0xfe];
        for u in "hello\r\nworld".encode_utf16() {
            le.extend_from_slice(&u.to_le_bytes());
        }
        std::fs::write(&utf16le, le).unwrap();
        assert_eq!(read_lines(&utf16le).unwrap(), vec!["hello", "world"]);

        let utf16be = dir.join("utf16be.txt");
        let mut be = vec![0xfe, 0xff];
        for u in "one\ntwo".encode_utf16() {
            be.extend_from_slice(&u.to_be_bytes());
        }
        std::fs::write(&utf16be, be).unwrap();
        assert_eq!(read_lines(&utf16be).unwrap(), vec!["one", "two"]);

        let empty = dir.join("empty.txt");
        std::fs::write(&empty, b"").unwrap();
        assert!(read_lines(&empty).unwrap().is_empty());
        assert_eq!(read_random_line(&empty).unwrap(), None);
        assert_eq!(read_specific_line(&empty, 0).unwrap(), None);
        assert_eq!(read_specific_line(&empty, 1).unwrap(), None);

        let _ = std::fs::remove_dir_all(&dir);
    }
    fn temp_dir(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("streamerz-atomic-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        d
    }

    fn names(dir: &Path) -> Vec<String> {
        let mut out: Vec<String> = std::fs::read_dir(dir)
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        out.sort();
        out
    }

    #[test]
    fn write_atomic_makes_the_folder_and_replaces_the_file_whole() {
        let d = temp_dir("write");
        let file = d.join("deep").join("a.json");
        write_atomic(&file, b"one").unwrap();
        write_atomic(&file, b"two").unwrap();
        assert_eq!(std::fs::read(&file).unwrap(), b"two");
        // Nothing left beside it.
        assert_eq!(names(&d.join("deep")), vec!["a.json"]);
        write_json(&file, &serde_json::json!({"k": [1, 2]})).unwrap();
        assert_eq!(std::fs::read_to_string(&file).unwrap(), r#"{"k":[1,2]}"#);
        let _ = std::fs::remove_dir_all(d);
    }

    #[test]
    fn a_failed_write_keeps_the_old_file_and_leaves_no_temp() {
        let d = temp_dir("fail");
        let file = d.join("a.txt");
        write_atomic(&file, b"old").unwrap();
        let err = replace(&file, None, |f| {
            f.write_all(b"half")?;
            Err(io::Error::other("cut"))
        })
        .unwrap_err();
        assert_eq!(err.to_string(), "cut");
        assert_eq!(std::fs::read(&file).unwrap(), b"old");
        assert_eq!(names(&d), vec!["a.txt"]);
        // The folder must exist for `replace`.
        assert!(replace(&d.join("missing").join("b"), None, |_| Ok(())).is_err());
        let _ = std::fs::remove_dir_all(d);
    }

    #[test]
    fn staged_files_have_their_own_hidden_names_until_committed() {
        let d = temp_dir("stage");
        std::fs::create_dir_all(&d).unwrap();
        let file = d.join("a.txt");
        let one = stage(&file, None, |f| f.write_all(b"1")).unwrap();
        let two = stage(&file, None, |f| f.write_all(b"2")).unwrap();
        assert_ne!(one, two);
        for t in [&one, &two] {
            let name = t.file_name().unwrap().to_string_lossy().into_owned();
            assert!(name.starts_with(".a.txt.") && name.ends_with(".tmp"), "{name}");
        }
        assert!(!file.exists());
        commit(&two, &file).unwrap();
        commit(&one, &file).unwrap();
        assert_eq!(std::fs::read(&file).unwrap(), b"1");
        assert_eq!(names(&d), vec!["a.txt"]);
        // A download made elsewhere is flushed and moved in.
        let part = d.join("a.part");
        std::fs::write(&part, b"3").unwrap();
        settle(&part, &file).unwrap();
        assert_eq!(std::fs::read(&file).unwrap(), b"3");
        assert!(settle(&part, &file).is_err());
        assert_eq!(names(&d), vec!["a.txt"]);
        let _ = std::fs::remove_dir_all(d);
    }

    #[cfg(unix)]
    #[test]
    fn replace_sets_the_permissions_it_is_given() {
        use std::os::unix::fs::PermissionsExt;
        let d = temp_dir("perms");
        std::fs::create_dir_all(&d).unwrap();
        let file = d.join("secret");
        std::fs::write(&file, b"old").unwrap();
        std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o644)).unwrap();
        replace(&file, Some(std::fs::Permissions::from_mode(0o600)), |f| {
            f.write_all(b"new")
        })
        .unwrap();
        assert_eq!(std::fs::metadata(&file).unwrap().permissions().mode() & 0o777, 0o600);
        assert_eq!(std::fs::read(&file).unwrap(), b"new");
        let _ = std::fs::remove_dir_all(d);
    }

    #[test]
    fn rename_retries_a_transient_failure() {
        let mut attempts = 0;
        let mut waits = 0;
        retry_rename(
            || {
                attempts += 1;
                if attempts < 3 {
                    Err(io::ErrorKind::PermissionDenied.into())
                } else {
                    Ok(())
                }
            },
            || waits += 1,
        )
        .unwrap();
        assert_eq!((attempts, waits), (3, 2));
        let mut tries = 0;
        let err = retry_rename(
            || {
                tries += 1;
                Err(io::ErrorKind::NotFound.into())
            },
            || {},
        )
        .unwrap_err();
        assert_eq!((err.kind(), tries), (io::ErrorKind::NotFound, 1));
    }
}
