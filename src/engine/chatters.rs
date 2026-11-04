//! Session-only Kick identities. Buckets keep the resident working set bounded.
use sha2::{Digest, Sha256};
use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;

pub(super) struct Chatters {
    root: PathBuf,
}

impl Chatters {
    pub fn new() -> std::io::Result<Self> {
        let root = crate::config_path().with_file_name("first-words").join(format!(
            "streamerz-chatters-{}-{}",
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

    pub fn insert(&self, id: &str, name: &str) -> std::io::Result<()> {
        let hash = Sha256::digest(id.as_bytes());
        let path = self.root.join(format!("{:02x}{:x}", hash[0], hash[1] >> 4));
        let mut file = std::fs::OpenOptions::new()
            .read(true)
            .append(true)
            .create(true)
            .open(path)?;
        let reader = BufReader::new(&file);
        for line in reader.lines() {
            let line = line?;
            let (saved, _): (String, String) = serde_json::from_str(&line).map_err(std::io::Error::other)?;
            if saved == id {
                return Ok(());
            }
        }
        serde_json::to_writer(&mut file, &(id, name)).map_err(std::io::Error::other)?;
        file.write_all(b"\n")
    }

    pub fn sample(&self, count: usize) -> std::io::Result<Vec<(String, String)>> {
        let mut picked = Vec::new();
        let mut seen = 0u64;
        for entry in std::fs::read_dir(&self.root)? {
            let file = std::fs::File::open(entry?.path())?;
            for line in BufReader::new(file).lines() {
                let (id, name): (String, String) = serde_json::from_str(&line?).map_err(std::io::Error::other)?;
                seen += 1;
                let candidate = (name, id);
                if picked.len() < count {
                    picked.push(candidate);
                } else {
                    let at = crate::math::secure_u64() % seen;
                    if at < picked.len() as u64 {
                        picked[at as usize] = candidate;
                    }
                }
            }
        }
        for i in 0..picked.len() {
            let next = i + (crate::math::secure_u64() as usize % (picked.len() - i));
            picked.swap(i, next);
        }
        Ok(picked)
    }
}

impl Drop for Chatters {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn session_index_deduplicates_and_preserves_ids_without_chat_history() {
        let root = std::env::temp_dir().join(format!("streamerz-chatters-test-{}", crate::engine::uuid()));
        std::fs::create_dir(&root).unwrap();
        let index = Chatters { root: root.clone() };
        for i in 0..2000 {
            index.insert(&format!("id{i}"), &format!("name{i}")).unwrap();
        }
        index.insert("id1", "name1").unwrap();
        let all = index.sample(2001).unwrap();
        assert_eq!(all.len(), 2000);
        assert!(all.contains(&("name1".into(), "id1".into())));
        assert_eq!(index.sample(3).unwrap().len(), 3);
        drop(index);
        assert!(!root.exists());
    }
}
