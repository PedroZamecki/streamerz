//! Exact session-only First Words membership with fixed-size read scratch storage.
//! Hashes select buckets only; full keys are compared, so collisions cannot change behavior.
use sha2::{Digest, Sha256};
use std::io::{BufReader, Read, Write};
use std::path::PathBuf;

pub(super) struct Speakers {
    root: PathBuf,
}

impl Speakers {
    pub fn new() -> std::io::Result<Self> {
        let root = crate::config_path().with_file_name("first-words").join(format!(
            "streamerz-speakers-{}-{}",
            std::process::id(),
            crate::engine::uuid()
        ));
        std::fs::create_dir_all(&root)?;
        Ok(Self { root })
    }

    fn platform(&self, platform: &str) -> PathBuf {
        self.root.join(
            Sha256::digest(platform.as_bytes())
                .iter()
                .map(|b| format!("{b:02x}"))
                .collect::<String>(),
        )
    }

    pub fn insert(&self, platform: &str, key: &str) -> std::io::Result<bool> {
        let size = u32::try_from(key.len()).map_err(|_| std::io::Error::other("First Words key is too large"))?;
        let dir = self.platform(platform);
        std::fs::create_dir_all(&dir)?;
        let hash = Sha256::digest(key.as_bytes());
        // 4096 buckets per platform. No resident index or collection of open files.
        let path = dir.join(format!("{:02x}{:x}", hash[0], hash[1] >> 4));
        let mut file = std::fs::OpenOptions::new()
            .read(true)
            .append(true)
            .create(true)
            .open(path)?;
        let mut reader = BufReader::with_capacity(4096, &file);
        loop {
            let mut length = [0u8; 4];
            if reader.read(&mut length[..1])? == 0 {
                break;
            }
            reader.read_exact(&mut length[1..])?;
            let len = u32::from_le_bytes(length) as usize;
            let mut scratch = [0u8; 4096];
            let mut offset = 0;
            let mut same = len == key.len();
            while offset < len {
                let count = (len - offset).min(scratch.len());
                reader.read_exact(&mut scratch[..count])?;
                if same && key.as_bytes()[offset..offset + count] != scratch[..count] {
                    same = false;
                }
                offset += count;
            }
            if same {
                return Ok(false);
            }
        }
        drop(reader);
        file.write_all(&size.to_le_bytes())?;
        file.write_all(key.as_bytes())?;
        Ok(true)
    }

    pub fn reset(&self, platform: Option<&str>) -> std::io::Result<()> {
        let path = platform.map_or_else(|| self.root.clone(), |p| self.platform(p));
        match std::fs::remove_dir_all(path) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(e),
        }
        std::fs::create_dir_all(&self.root)
    }
}

impl Drop for Speakers {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn disk_membership_is_exact_and_resets_by_platform() {
        let root = std::env::temp_dir().join(format!("streamerz-speakers-test-{}", crate::engine::uuid()));
        std::fs::create_dir(&root).unwrap();
        let disk = Speakers { root };
        let root = disk.root.clone();
        // Many keys share buckets. Newlines and large keys exercise the framed comparison.
        for i in 0..10000 {
            assert!(disk.insert("twitch", &format!("user{i}\n")).unwrap());
        }
        for i in 0..10000 {
            assert!(!disk.insert("twitch", &format!("user{i}\n")).unwrap());
        }
        let large = "x".repeat(10000);
        assert!(disk.insert("kick", &large).unwrap());
        assert!(!disk.insert("kick", &large).unwrap());
        disk.reset(Some("twitch")).unwrap();
        assert!(disk.insert("twitch", "user1\n").unwrap());
        assert!(!disk.insert("kick", &large).unwrap());
        disk.reset(None).unwrap();
        assert!(disk.insert("kick", &large).unwrap());
        drop(disk);
        assert!(!root.exists());
    }
}
