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
        let mut builder = std::fs::DirBuilder::new();
        builder.recursive(true);
        #[cfg(unix)]
        std::os::unix::fs::DirBuilderExt::mode(&mut builder, 0o700);
        builder.create(&root)?;
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
            .open(&path)?;
        let mut reader = BufReader::with_capacity(4096, &file);
        let mut valid_bytes = 0u64;
        let mut truncated = false;
        loop {
            let mut length = [0u8; 4];
            if reader.read(&mut length[..1])? == 0 {
                break;
            }
            if let Err(e) = reader.read_exact(&mut length[1..]) {
                if e.kind() == std::io::ErrorKind::UnexpectedEof {
                    truncated = true;
                    break;
                }
                return Err(e);
            }
            let len = u32::from_le_bytes(length) as usize;
            let mut scratch = [0u8; 4096];
            let mut offset = 0;
            let mut same = len == key.len();
            while offset < len {
                let count = (len - offset).min(scratch.len());
                if let Err(e) = reader.read_exact(&mut scratch[..count]) {
                    if e.kind() == std::io::ErrorKind::UnexpectedEof {
                        truncated = true;
                        break;
                    }
                    return Err(e);
                }
                if same && key.as_bytes()[offset..offset + count] != scratch[..count] {
                    same = false;
                }
                offset += count;
            }
            if truncated {
                break;
            }
            if same {
                return Ok(false);
            }
            valid_bytes = valid_bytes.saturating_add(4 + len as u64);
        }
        drop(reader);
        if truncated {
            // Trim any partial trailing record from an interrupted write before appending.
            drop(file);
            std::fs::OpenOptions::new()
                .write(true)
                .open(&path)?
                .set_len(valid_bytes)?;
            file = std::fs::OpenOptions::new().append(true).open(&path)?;
        }
        let len_bytes = size.to_le_bytes();
        if 4 + key.len() <= 4096 {
            let mut buf = [0u8; 4096];
            buf[..4].copy_from_slice(&len_bytes);
            buf[4..4 + key.len()].copy_from_slice(key.as_bytes());
            file.write_all(&buf[..4 + key.len()])?;
        } else {
            file.write_all(&len_bytes)?;
            file.write_all(key.as_bytes())?;
        }
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

    #[test]
    fn truncated_trailing_record_is_repaired_on_next_insert() {
        let root = std::env::temp_dir().join(format!("streamerz-speakers-trunc-{}", crate::engine::uuid()));
        std::fs::create_dir(&root).unwrap();
        let disk = Speakers { root: root.clone() };
        assert!(disk.insert("twitch", "alice").unwrap());
        // Append a partial header + payload to the bucket file of "alice".
        let hash = Sha256::digest(b"alice");
        let bucket = disk
            .platform("twitch")
            .join(format!("{:02x}{:x}", hash[0], hash[1] >> 4));
        let mut f = std::fs::OpenOptions::new().append(true).open(&bucket).unwrap();
        f.write_all(&50u32.to_le_bytes()).unwrap();
        f.write_all(b"partial").unwrap();
        drop(f);
        // Existing entry is still recognized before the corrupt tail.
        assert!(!disk.insert("twitch", "alice").unwrap());
        // A different key in the same bucket repairs the tail and appends cleanly.
        let same_bucket = (0..100000)
            .map(|i| format!("user-{i}"))
            .find(|k| {
                let h = Sha256::digest(k.as_bytes());
                h[0] == hash[0] && (h[1] >> 4) == (hash[1] >> 4)
            })
            .unwrap();
        assert!(disk.insert("twitch", &same_bucket).unwrap());
        assert!(!disk.insert("twitch", &same_bucket).unwrap());
        assert!(!disk.insert("twitch", "alice").unwrap());
        drop(disk);
        assert!(!root.exists());
    }
}
