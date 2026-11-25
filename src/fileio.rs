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
use std::path::Path;

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
}
