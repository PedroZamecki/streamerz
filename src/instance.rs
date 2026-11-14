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

/// A restarted process allows the previous process time to release its lock.
pub fn acquire_after_restart(dir: &Path) -> io::Result<File> {
    for _ in 0..50 {
        match acquire(dir) {
            Ok(lock) => return Ok(lock),
            Err(e) if e.kind() == io::ErrorKind::WouldBlock => {
                std::thread::sleep(std::time::Duration::from_millis(100))
            }
            Err(e) => return Err(e),
        }
    }
    acquire(dir)
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

    #[test]
    fn restart_waits_for_the_previous_lock() {
        let dir = std::env::temp_dir().join(format!("streamerz-restart-lock-{}", std::process::id()));
        let first = acquire(&dir).unwrap();
        let path = dir.clone();
        let child = std::thread::spawn(move || acquire_after_restart(&path));
        std::thread::sleep(std::time::Duration::from_millis(150));
        drop(first);
        drop(child.join().unwrap().unwrap());
        std::fs::remove_dir_all(dir).unwrap();
    }
}
