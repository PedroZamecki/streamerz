//! Zip archives, as much of them as a `.streamerz` template file needs: files stored or deflated,
//! read and written whole in memory with `flate2` (already in the build). No encryption, no
//! zip64 (4 GB and 65,535 files at most), no file times: the same files always make the same bytes.

use flate2::{read::DeflateDecoder, write::DeflateEncoder, Compression, Crc};
use std::io::{Read, Write};

const LOCAL: u32 = 0x0403_4b50;
const CENTRAL: u32 = 0x0201_4b50;
const END: u32 = 0x0605_4b50;
/// Bit 11 of a file's flags: its name is UTF-8.
const UTF8: u16 = 1 << 11;

fn crc(bytes: &[u8]) -> u32 {
    let mut c = Crc::new();
    c.update(bytes);
    c.sum()
}

/// A name that stays inside the folder it is unpacked to, on every system: `/` between its parts,
/// no empty part, no `.` or `..`, no drive, no backslash.
pub fn safe_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 1024
        && !name.contains(['\\', ':', '\0'])
        && name
            .split('/')
            .all(|part| !part.is_empty() && part != "." && part != "..")
}

/// An archive of these files (name with `/` between folders, contents), in this order.
pub fn write(files: &[(String, Vec<u8>)]) -> Result<Vec<u8>, String> {
    if files.len() > u16::MAX as usize {
        return Err("too many files for one archive".into());
    }
    let mut out = Vec::new();
    let mut central = Vec::new();
    for (name, bytes) in files {
        if !safe_name(name) {
            return Err(format!("invalid file name: {name}"));
        }
        let mut deflated = DeflateEncoder::new(Vec::new(), Compression::default());
        let packed = deflated
            .write_all(bytes)
            .and_then(|_| deflated.finish())
            .map_err(|e| e.to_string())?;
        // What is already compressed (a picture, a video) is kept as it is.
        let (method, data) = if packed.len() < bytes.len() {
            (8u16, &packed)
        } else {
            (0u16, bytes)
        };
        let size = |n: usize| u32::try_from(n).map_err(|_| format!("{name} is too large for an archive"));
        let (offset, packed_size, full_size) = (size(out.len())?, size(data.len())?, size(bytes.len())?);
        // version needed, flags, method, time, date, crc, sizes, name length, extra length
        let mut head = Vec::new();
        for v in [20u16, UTF8, method, 0, 0x21] {
            head.extend(v.to_le_bytes());
        }
        for v in [crc(bytes), packed_size, full_size] {
            head.extend(v.to_le_bytes());
        }
        head.extend((name.len() as u16).to_le_bytes());
        head.extend(0u16.to_le_bytes());
        out.extend(LOCAL.to_le_bytes());
        out.extend(&head);
        out.extend(name.as_bytes());
        out.extend(data);
        central.extend(CENTRAL.to_le_bytes());
        central.extend(20u16.to_le_bytes()); // made by
        central.extend(&head);
        // comment length, disk, internal and external attributes, where its local header is
        central.extend([0u8; 2 + 2 + 2 + 4]);
        central.extend(offset.to_le_bytes());
        central.extend(name.as_bytes());
    }
    let start = u32::try_from(out.len()).map_err(|_| "the archive is too large".to_string())?;
    let count = files.len() as u16;
    out.extend(&central);
    out.extend(END.to_le_bytes());
    out.extend([0u8; 4]); // this disk, the disk with the directory
    out.extend(count.to_le_bytes());
    out.extend(count.to_le_bytes());
    out.extend((central.len() as u32).to_le_bytes());
    out.extend(start.to_le_bytes());
    out.extend(0u16.to_le_bytes());
    Ok(out)
}

struct At<'a>(&'a [u8]);

impl At<'_> {
    fn u16(&self, at: usize) -> Result<u16, String> {
        let b = self.0.get(at..at + 2).ok_or("the archive is cut short")?;
        Ok(u16::from_le_bytes([b[0], b[1]]))
    }
    fn u32(&self, at: usize) -> Result<u32, String> {
        let b = self.0.get(at..at + 4).ok_or("the archive is cut short")?;
        Ok(u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
    }
    fn bytes(&self, at: usize, len: usize) -> Result<&[u8], String> {
        at.checked_add(len)
            .and_then(|end| self.0.get(at..end))
            .ok_or_else(|| "the archive is cut short".to_string())
    }
}

/// The files of an archive, in its order. Folders are skipped. An archive with a name that would
/// leave the folder, a file that isn't what its checksum says or that unpacks to more than `limit`
/// bytes in all is refused whole.
pub fn read(zip: &[u8], limit: u64) -> Result<Vec<(String, Vec<u8>)>, String> {
    let z = At(zip);
    // The end record is the last thing in the file, before a comment of at most 65,535 bytes.
    let end = (0..=zip.len().saturating_sub(22))
        .rev()
        .take(65_536)
        .find(|&i| z.u32(i) == Ok(END))
        .ok_or("not a zip archive")?;
    let (count, mut at) = (z.u16(end + 10)? as usize, z.u32(end + 16)? as usize);
    let mut out = Vec::with_capacity(count.min(1024));
    let mut total = 0u64;
    for _ in 0..count {
        if z.u32(at)? != CENTRAL {
            return Err("the archive's file list is damaged".into());
        }
        let (flags, method) = (z.u16(at + 8)?, z.u16(at + 10)?);
        let (sum, packed, full) = (z.u32(at + 16)?, z.u32(at + 20)? as usize, z.u32(at + 24)? as u64);
        let (name_len, extra, comment) = (
            z.u16(at + 28)? as usize,
            z.u16(at + 30)? as usize,
            z.u16(at + 32)? as usize,
        );
        let local = z.u32(at + 42)? as usize;
        let name = String::from_utf8(z.bytes(at + 46, name_len)?.to_vec()).map_err(|_| "a file name is not text")?;
        at += 46 + name_len + extra + comment;
        if name.ends_with('/') {
            continue;
        }
        if !safe_name(&name) {
            return Err(format!("the archive has an unsafe file name: {name}"));
        }
        if flags & 1 != 0 {
            return Err(format!("{name} is encrypted"));
        }
        total += full;
        if total > limit {
            return Err("the archive unpacks to more than it may".into());
        }
        if z.u32(local)? != LOCAL {
            return Err(format!("{name} is not where the archive says"));
        }
        let data = z.bytes(
            local + 30 + z.u16(local + 26)? as usize + z.u16(local + 28)? as usize,
            packed,
        )?;
        let bytes = match method {
            0 => data.to_vec(),
            8 => {
                let mut bytes = Vec::new();
                // One byte past its size, to notice a file that unpacks to more than it says.
                DeflateDecoder::new(data)
                    .take(full + 1)
                    .read_to_end(&mut bytes)
                    .map_err(|e| format!("{name} is damaged ({e})"))?;
                bytes
            }
            _ => return Err(format!("{name} is packed in a way that isn't supported")),
        };
        if bytes.len() as u64 != full || crc(&bytes) != sum {
            return Err(format!("{name} is damaged"));
        }
        out.push((name, bytes));
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn files() -> Vec<(String, Vec<u8>)> {
        vec![
            ("template.json".into(), br#"{"name": "x"}"#.to_vec()),
            ("files/page.html".into(), "<p>ol\u{e1}</p>".repeat(200).into_bytes()),
            ("files/img/dot.png".into(), (0..=255u8).collect()),
            ("files/empty.txt".into(), Vec::new()),
        ]
    }

    #[test]
    fn what_is_written_is_read_back() {
        let zip = write(&files()).unwrap();
        assert_eq!(read(&zip, 1 << 20).unwrap(), files());
        assert_eq!(write(&files()).unwrap(), zip, "the same files make the same bytes");
        // Text is deflated, what doesn't shrink is stored.
        assert!(zip.len() < files().iter().map(|f| f.1.len()).sum::<usize>());
        assert_eq!(read(&write(&[]).unwrap(), 0).unwrap(), Vec::new());
    }

    #[test]
    fn other_programs_read_it() {
        let path = std::env::temp_dir().join(format!("streamerz-zip-{}.zip", std::process::id()));
        std::fs::write(&path, write(&files()).unwrap()).unwrap();
        // Where `unzip` is installed it lists and checks the archive; elsewhere this proves nothing.
        if let Ok(out) = std::process::Command::new("unzip").arg("-t").arg(&path).output() {
            let text = String::from_utf8_lossy(&out.stdout);
            assert!(out.status.success() && text.contains("files/img/dot.png"), "{text}");
        }
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn unsafe_and_damaged_archives_are_refused() {
        for name in ["../x", "/etc/x", "a/../b", "a\\b", "C:/x", "a//b", "./a", ""] {
            assert!(!safe_name(name), "{name}");
            assert!(write(&[(name.into(), vec![1])]).is_err(), "{name}");
        }
        assert!(safe_name("files/img/a b.png"));
        // A name changed inside a finished archive (same length) is caught on reading.
        let zip = write(&[("ab/x.txt".into(), b"hello hello hello hello".to_vec())]).unwrap();
        let swap = |from: &[u8], to: &[u8]| {
            let mut z = zip.clone();
            let mut i = 0;
            while let Some(p) = z[i..].windows(from.len()).position(|w| w == from) {
                z[i + p..i + p + from.len()].copy_from_slice(to);
                i += p + from.len();
            }
            z
        };
        assert!(read(&swap(b"ab/x.txt", b"../x.txt"), 1 << 20)
            .unwrap_err()
            .contains("unsafe"));
        assert!(read(&zip, 5).unwrap_err().contains("more than"));
        assert!(read(&zip[..zip.len() - 30], 1 << 20).is_err());
        assert!(read(b"not a zip at all, just some text here", 1 << 20).is_err());
        let mut broken = zip.clone();
        broken[40] ^= 0xff;
        assert!(read(&broken, 1 << 20).is_err());
    }
}
