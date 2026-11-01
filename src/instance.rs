//! Keep one production instance per user, without depending on a server port.

use std::fs::{File, OpenOptions};
use std::io;
use std::path::Path;

/// The operating system releases the lock even if the process crashes.
pub fn acquire(dir: &Path) -> io::Result<File> {
    std::fs::create_dir_all(dir)?;
    let file = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(dir.join("instance.lock"))?;
    file.try_lock().map_err(io::Error::from)?;
    Ok(file)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lock_is_exclusive_and_released_on_close() {
        let dir = std::env::temp_dir().join(format!("streamerz-lock-test-{}", std::process::id()));
        let first = acquire(&dir).unwrap();
        assert!(acquire(&dir).is_err());
        drop(first);
        assert!(acquire(&dir).is_ok());
        std::fs::remove_dir_all(dir).unwrap();
    }
}
