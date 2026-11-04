//! Move older selected settings to the global directory without overwriting them.
use std::io;
use std::path::{Path, PathBuf};

pub fn copy_tree(source: &Path, dest: &Path) -> io::Result<()> {
    let meta = std::fs::symlink_metadata(source)?;
    if meta.file_type().is_symlink() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "a settings path is a symbolic link",
        ));
    }
    if meta.is_dir() {
        std::fs::create_dir_all(dest)?;
        for entry in std::fs::read_dir(source)? {
            let entry = entry?;
            let name = entry.file_name();
            if name.to_string_lossy().starts_with("Singleton") {
                continue;
            }
            copy_tree(&entry.path(), &dest.join(name))?;
        }
    } else if meta.is_file() && !dest.exists() {
        std::fs::copy(source, dest)?;
    }
    Ok(())
}

/// Preserve an older global copy before migrating the user's selected settings over it.
pub fn migrate_selected(source: &Path, target: &Path) -> io::Result<()> {
    if target.exists() && source != target && key_source(target) != source {
        let dir = target
            .parent()
            .ok_or_else(|| io::Error::other("target has no directory"))?;
        let from = source
            .parent()
            .ok_or_else(|| io::Error::other("source has no directory"))?;
        let backup = (0..1000)
            .map(|n| dir.join(format!("migration-backup-{n}")))
            .find(|p| !p.exists())
            .ok_or_else(|| io::Error::other("no backup name available"))?;
        #[cfg_attr(not(unix), allow(unused_mut))]
        let mut builder = std::fs::DirBuilder::new();
        #[cfg(unix)]
        std::os::unix::fs::DirBuilderExt::mode(&mut builder, 0o700);
        builder.create(&backup)?;
        let names = [
            "streamerz.json",
            "streamerz.json.bak",
            "community.json",
            "plugins",
            "tiktok-bot",
            "tiktok-broadcaster",
            "backups",
            "key-source",
        ];
        for name in names {
            let old = dir.join(name);
            if old.exists() {
                copy_tree(&old, &backup.join(name))?;
            }
        }
        // The old global data is safely copied. Only replace related data present at the selected
        // location. Leave the old settings file until last so an interrupted backup changes nothing.
        if from != dir {
            for name in [
                "streamerz.json.bak",
                "community.json",
                "plugins",
                "tiktok-bot",
                "tiktok-broadcaster",
                "backups",
            ] {
                let old = dir.join(name);
                if from.join(name).exists() && old.exists() {
                    if old.is_dir() {
                        std::fs::remove_dir_all(old)?;
                    } else {
                        std::fs::remove_file(old)?;
                    }
                }
            }
        }
        std::fs::remove_file(target)?;
    }
    migrate(source, target)
}

/// Copy data associated with settings; settings are copied last so an interrupted migration retries.
pub fn migrate(source: &Path, target: &Path) -> io::Result<()> {
    if target.exists() || !source.is_file() || source == target {
        return Ok(());
    }
    let from = source
        .parent()
        .ok_or_else(|| io::Error::other("source has no directory"))?;
    let to = target
        .parent()
        .ok_or_else(|| io::Error::other("target has no directory"))?;
    std::fs::create_dir_all(to)?;
    for name in [
        "community.json",
        "plugins",
        "tiktok-bot",
        "tiktok-broadcaster",
        "backups",
    ] {
        let path = from.join(name);
        if path.exists() {
            copy_tree(&path, &to.join(name))?;
        }
    }
    let backup = source.with_file_name("streamerz.json.bak");
    if backup.exists() {
        copy_tree(&backup, &to.join("streamerz.json.bak"))?;
    }
    // Linux keyring entries are keyed by the original path. Keep using that key for migrated data.
    std::fs::write(to.join("key-source"), source.to_string_lossy().as_bytes())?;
    let pending = to.join("streamerz.json.migrating");
    std::fs::copy(source, &pending)?;
    std::fs::rename(pending, target)?;
    Ok(())
}

pub fn key_source(target: &Path) -> PathBuf {
    std::fs::read_to_string(target.with_file_name("key-source"))
        .ok()
        .map(|s| PathBuf::from(s.trim()))
        .filter(|p| p.is_absolute())
        .unwrap_or_else(|| target.to_path_buf())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[cfg(unix)]
    #[test]
    fn migration_rejects_linked_data_without_creating_settings() {
        use std::os::unix::fs::symlink;
        let root = std::env::temp_dir().join(format!("streamerz-migration-link-{}", std::process::id()));
        let old = root.join("old");
        let new = root.join("new");
        std::fs::create_dir_all(&old).unwrap();
        std::fs::write(old.join("streamerz.json"), b"original").unwrap();
        std::fs::create_dir_all(root.join("outside")).unwrap();
        symlink(root.join("outside"), old.join("plugins")).unwrap();
        assert!(migrate(&old.join("streamerz.json"), &new.join("streamerz.json")).is_err());
        assert!(!new.join("streamerz.json").exists());
        assert_eq!(std::fs::read(old.join("streamerz.json")).unwrap(), b"original");
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn selected_settings_replace_an_old_global_copy_after_backup() {
        let root = std::env::temp_dir().join(format!("streamerz-migration-selected-{}", std::process::id()));
        let old = root.join("selected");
        let new = root.join("global");
        std::fs::create_dir_all(&old).unwrap();
        std::fs::create_dir_all(&new).unwrap();
        std::fs::write(old.join("streamerz.json"), b"selected").unwrap();
        std::fs::write(old.join("community.json"), b"selected community").unwrap();
        std::fs::write(new.join("streamerz.json"), b"old global").unwrap();
        std::fs::write(new.join("community.json"), b"old community").unwrap();
        let source = old.join("streamerz.json");
        let target = new.join("streamerz.json");
        migrate_selected(&source, &target).unwrap();
        assert_eq!(std::fs::read(&target).unwrap(), b"selected");
        assert_eq!(
            std::fs::read(new.join("community.json")).unwrap(),
            b"selected community"
        );
        assert_eq!(
            std::fs::read(new.join("migration-backup-0/streamerz.json")).unwrap(),
            b"old global"
        );
        assert_eq!(
            std::fs::read(new.join("migration-backup-0/community.json")).unwrap(),
            b"old community"
        );
        migrate_selected(&source, &target).unwrap();
        assert!(!new.join("migration-backup-1").exists());
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn migration_copies_without_changing_source() {
        let root = std::env::temp_dir().join(format!("streamerz-migration-{}", std::process::id()));
        let old = root.join("old");
        let new = root.join("new");
        std::fs::create_dir_all(&old).unwrap();
        std::fs::write(old.join("streamerz.json"), b"original").unwrap();
        std::fs::write(old.join("community.json"), b"community").unwrap();
        let src = old.join("streamerz.json");
        let dst = new.join("streamerz.json");
        migrate(&src, &dst).unwrap();
        assert_eq!(std::fs::read(&dst).unwrap(), b"original");
        assert_eq!(std::fs::read(new.join("community.json")).unwrap(), b"community");
        assert_eq!(key_source(&dst), src);
        std::fs::write(&dst, b"changed").unwrap();
        migrate(&src, &dst).unwrap();
        assert_eq!(std::fs::read(&src).unwrap(), b"original");
        assert_eq!(std::fs::read(&dst).unwrap(), b"changed");
        std::fs::remove_dir_all(root).unwrap();
    }
}
