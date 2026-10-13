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

/// Write or append text to a file.
pub fn write_to_file(path: &Path, text: &str, append: bool) -> io::Result<()> {
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            let _ = std::fs::create_dir_all(parent);
        }
    }
    let mut file = OpenOptions::new()
        .create(true)
        .write(true)
        .append(append)
        .truncate(!append)
        .open(path)?;
    file.write_all(text.as_bytes())?;
    file.flush()
}

/// Read all lines from a file.
pub fn read_lines(path: &Path) -> io::Result<Vec<String>> {
    let content = std::fs::read_to_string(path)?;
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

    #[test]
    fn file_write_and_read_lines() {
        let dir = std::env::temp_dir().join(format!("streamerz_fileio_test_{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let file = dir.join("test.txt");

        assert!(!file_exists(&file));
        assert!(folder_exists(&dir));

        assert!(write_to_file(&file, "line1\nline2\nline3\n", false).is_ok());
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

        // Append
        assert!(write_to_file(&file, "line4\n", true).is_ok());
        let lines2 = read_lines(&file).unwrap();
        assert_eq!(lines2.len(), 4);
        assert_eq!(lines2[3], "line4");

        let _ = std::fs::remove_dir_all(&dir);
    }
}
