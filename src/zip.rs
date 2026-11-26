//! Zip archives, as much of them as a `.streamerz` template file needs: files stored or deflated,
//! read and written whole in memory with `flate2` (already in the build). No encryption, no
//! zip64 (4 GB and 65,535 files at most), no file times: the same files always make the same bytes.

use flate2::{read::DeflateDecoder, write::DeflateEncoder, Compression, Crc};
use std::collections::HashSet;
use std::io::{Read, Write};

const LOCAL: u32 = 0x0403_4b50;
const CENTRAL: u32 = 0x0201_4b50;
const END: u32 = 0x0605_4b50;
const ZIP64_LOCATOR: u32 = 0x0706_4b50;
/// Bit 3 of a file's flags: CRC and sizes follow the file data in a data descriptor.
const DATA_DESCRIPTOR: u16 = 1 << 3;
/// Bit 11 of a file's flags: its name is UTF-8.
const UTF8: u16 = 1 << 11;

/// IBM CP437 characters for bytes `0x80..=0xFF`, used for non-UTF-8 zip file names.
#[rustfmt::skip]
const CP437: [char; 128] = [
    '\u{00C7}', '\u{00FC}', '\u{00E9}', '\u{00E2}', '\u{00E4}', '\u{00E0}', '\u{00E5}', '\u{00E7}',
    '\u{00EA}', '\u{00EB}', '\u{00E8}', '\u{00EF}', '\u{00EE}', '\u{00EC}', '\u{00C4}', '\u{00C5}',
    '\u{00C9}', '\u{00E6}', '\u{00C6}', '\u{00F4}', '\u{00F6}', '\u{00F2}', '\u{00FB}', '\u{00F9}',
    '\u{00FF}', '\u{00D6}', '\u{00DC}', '\u{00A2}', '\u{00A3}', '\u{00A5}', '\u{20A7}', '\u{0192}',
    '\u{00E1}', '\u{00ED}', '\u{00F3}', '\u{00FA}', '\u{00F1}', '\u{00D1}', '\u{00AA}', '\u{00BA}',
    '\u{00BF}', '\u{2310}', '\u{00AC}', '\u{00BD}', '\u{00BC}', '\u{00A1}', '\u{00AB}', '\u{00BB}',
    '\u{2591}', '\u{2592}', '\u{2593}', '\u{2502}', '\u{2524}', '\u{2561}', '\u{2562}', '\u{2556}',
    '\u{2555}', '\u{2563}', '\u{2551}', '\u{2557}', '\u{255D}', '\u{255C}', '\u{255B}', '\u{2510}',
    '\u{2514}', '\u{2534}', '\u{252C}', '\u{251C}', '\u{2500}', '\u{253C}', '\u{255E}', '\u{255F}',
    '\u{255A}', '\u{2554}', '\u{2569}', '\u{2566}', '\u{2560}', '\u{2550}', '\u{256C}', '\u{2567}',
    '\u{2568}', '\u{2564}', '\u{2565}', '\u{2559}', '\u{2558}', '\u{2552}', '\u{2553}', '\u{256B}',
    '\u{256A}', '\u{2518}', '\u{250C}', '\u{2588}', '\u{2584}', '\u{258C}', '\u{2590}', '\u{2580}',
    '\u{03B1}', '\u{00DF}', '\u{0393}', '\u{03C0}', '\u{03A3}', '\u{03C3}', '\u{00B5}', '\u{03C4}',
    '\u{03A6}', '\u{0398}', '\u{03A9}', '\u{03B4}', '\u{221E}', '\u{03C6}', '\u{03B5}', '\u{2229}',
    '\u{2261}', '\u{00B1}', '\u{2265}', '\u{2264}', '\u{2320}', '\u{2321}', '\u{00F7}', '\u{2248}',
    '\u{00B0}', '\u{2219}', '\u{00B7}', '\u{221A}', '\u{207F}', '\u{00B2}', '\u{25A0}', '\u{00A0}',
];

fn decode_name(bytes: &[u8], flags: u16) -> Result<String, String> {
    if flags & UTF8 != 0 {
        return std::str::from_utf8(bytes)
            .map(str::to_owned)
            .map_err(|_| "a file name is not text".into());
    }
    if let Ok(s) = std::str::from_utf8(bytes) {
        return Ok(s.to_owned());
    }
    Ok(bytes
        .iter()
        .map(|&b| {
            if b < 0x80 {
                b as char
            } else {
                CP437[(b - 0x80) as usize]
            }
        })
        .collect())
}

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
    let mut seen = HashSet::with_capacity(files.len());
    let mut out = Vec::new();
    let mut central = Vec::new();
    for (name, bytes) in files {
        if !safe_name(name) {
            return Err(format!("invalid file name: {name}"));
        }
        if !seen.insert(name.as_str()) {
            return Err(format!("duplicate file name: {name}"));
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
    let central_len = u32::try_from(central.len()).map_err(|_| "the archive is too large".to_string())?;
    let count = files.len() as u16;
    out.extend(&central);
    out.extend(END.to_le_bytes());
    out.extend([0u8; 4]); // this disk, the disk with the directory
    out.extend(count.to_le_bytes());
    out.extend(count.to_le_bytes());
    out.extend(central_len.to_le_bytes());
    out.extend(start.to_le_bytes());
    out.extend(0u16.to_le_bytes());
    Ok(out)
}

struct At<'a>(&'a [u8]);

impl<'a> At<'a> {
    fn u16(&self, at: usize) -> Result<u16, String> {
        let b = self.bytes(at, 2)?;
        Ok(u16::from_le_bytes([b[0], b[1]]))
    }
    fn u32(&self, at: usize) -> Result<u32, String> {
        let b = self.bytes(at, 4)?;
        Ok(u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
    }
    fn bytes(&self, at: usize, len: usize) -> Result<&'a [u8], String> {
        at.checked_add(len)
            .and_then(|end| self.0.get(at..end))
            .ok_or_else(|| "the archive is cut short".to_string())
    }
}

fn add_offset(base: usize, delta: usize) -> Result<usize, String> {
    base.checked_add(delta)
        .ok_or_else(|| "the archive is cut short".to_string())
}

/// The files of an archive, in its order. Folders are skipped. An archive with a name that would
/// leave the folder, a symbolic link, duplicate names, a file that isn't what its checksum says
/// or that unpacks to more than `limit` bytes in all is refused whole.
pub fn read(zip: &[u8], limit: u64) -> Result<Vec<(String, Vec<u8>)>, String> {
    if zip.len() < 22 {
        return Err("not a zip archive".into());
    }
    let z = At(zip);
    // The end record is the last thing in the file, before a comment of at most 65,535 bytes.
    let end = (0..=zip.len() - 22)
        .rev()
        .take(65_536)
        .find(|&i| {
            z.u32(i) == Ok(END)
                && z.u16(i + 20)
                    .is_ok_and(|comment| i + 22 + comment as usize == zip.len())
                && (z.u16(i + 10) != Ok(0)
                    || z.u32(i + 16)
                        .ok()
                        .zip(z.u32(i + 12).ok())
                        .is_some_and(|(s, l)| (s as usize).checked_add(l as usize) == Some(i)))
        })
        .ok_or("not a zip archive")?;
    let (disk, cd_disk) = (z.u16(end + 4)?, z.u16(end + 6)?);
    let (disk_count, count) = (z.u16(end + 8)?, z.u16(end + 10)?);
    let (cd_size, cd_start) = (z.u32(end + 12)?, z.u32(end + 16)?);
    let has_zip64_locator = end >= 20 && z.u32(end - 20) == Ok(ZIP64_LOCATOR);
    if has_zip64_locator || count == u16::MAX || disk_count == u16::MAX || cd_size == u32::MAX || cd_start == u32::MAX {
        return Err("zip64 archives are not supported".into());
    }
    if disk != 0 || cd_disk != 0 || disk_count != count {
        return Err("multi-disk archives are not supported".into());
    }
    let mut at = cd_start as usize;
    let cd_end = add_offset(at, cd_size as usize)?;
    if cd_end > end {
        return Err("the archive's file list is damaged".into());
    }
    let count = count as usize;
    let mut out = Vec::with_capacity(count.min(1024));
    let mut seen = HashSet::with_capacity(count.min(1024));
    let mut total = 0u64;
    for _ in 0..count {
        if add_offset(at, 46)? > cd_end || z.u32(at)? != CENTRAL {
            return Err("the archive's file list is damaged".into());
        }
        let made_by = z.u16(at + 4)?;
        let (flags, method) = (z.u16(at + 8)?, z.u16(at + 10)?);
        let (sum, packed_raw, full_raw) = (z.u32(at + 16)?, z.u32(at + 20)?, z.u32(at + 24)?);
        let (name_len, extra, comment) = (
            z.u16(at + 28)? as usize,
            z.u16(at + 30)? as usize,
            z.u16(at + 32)? as usize,
        );
        let ext_attr = z.u32(at + 38)?;
        let local_raw = z.u32(at + 42)?;
        if packed_raw == u32::MAX || full_raw == u32::MAX || local_raw == u32::MAX {
            return Err("zip64 archives are not supported".into());
        }
        let (packed, full, local) = (packed_raw as usize, u64::from(full_raw), local_raw as usize);
        let name_bytes = z.bytes(add_offset(at, 46)?, name_len)?;
        let name = decode_name(name_bytes, flags)?;
        let entry_len = 46usize
            .checked_add(name_len)
            .and_then(|n| n.checked_add(extra))
            .and_then(|n| n.checked_add(comment))
            .ok_or_else(|| "the archive's file list is damaged".to_string())?;
        at = add_offset(at, entry_len)?;
        if at > cd_end {
            return Err("the archive's file list is damaged".into());
        }
        let unix_type = if matches!(made_by >> 8, 3 | 19) {
            ((ext_attr >> 16) as u16) & 0o170_000
        } else {
            0
        };
        if unix_type == 0o120_000 {
            return Err(format!("the archive contains a symbolic link: {name}"));
        }
        if let Some(dir) = name.strip_suffix('/') {
            if !safe_name(dir) {
                return Err(format!("the archive has an unsafe file name: {name}"));
            }
            continue;
        }
        if unix_type == 0o040_000 {
            if !safe_name(&name) {
                return Err(format!("the archive has an unsafe file name: {name}"));
            }
            if full == 0 {
                continue;
            }
            return Err(format!("the archive contains an unsupported file type: {name}"));
        }
        if unix_type != 0 && unix_type != 0o100_000 {
            return Err(format!("the archive contains an unsupported file type: {name}"));
        }
        if !safe_name(&name) {
            return Err(format!("the archive has an unsafe file name: {name}"));
        }
        if !seen.insert(name.clone()) {
            return Err(format!("duplicate file in archive: {name}"));
        }
        if flags & 1 != 0 {
            return Err(format!("{name} is encrypted"));
        }
        total = total
            .checked_add(full)
            .ok_or_else(|| "the archive unpacks to more than it may".to_string())?;
        if total > limit {
            return Err("the archive unpacks to more than it may".into());
        }
        if z.u32(local)? != LOCAL {
            return Err(format!("{name} is not where the archive says"));
        }
        let local_flags = z.u16(add_offset(local, 6)?)?;
        let local_method = z.u16(add_offset(local, 8)?)?;
        if local_flags & 1 != 0 {
            return Err(format!("{name} is encrypted"));
        }
        if local_method != method {
            return Err(format!("{name} is damaged"));
        }
        let local_name_len = z.u16(add_offset(local, 26)?)? as usize;
        let local_extra_len = z.u16(add_offset(local, 28)?)? as usize;
        let local_name_bytes = z.bytes(add_offset(local, 30)?, local_name_len)?;
        if local_name_bytes != name_bytes {
            return Err(format!("{name} is damaged"));
        }
        if local_flags & DATA_DESCRIPTOR == 0 {
            let local_crc = z.u32(add_offset(local, 14)?)?;
            let local_packed = z.u32(add_offset(local, 18)?)?;
            let local_full = z.u32(add_offset(local, 22)?)?;
            if local_packed == u32::MAX || local_full == u32::MAX {
                return Err("zip64 archives are not supported".into());
            }
            if local_crc != sum || local_packed != packed_raw || local_full != full_raw {
                return Err(format!("{name} is damaged"));
            }
        }
        let data_start =
            add_offset(add_offset(local, 30)?, local_name_len).and_then(|n| add_offset(n, local_extra_len))?;
        let data = z.bytes(data_start, packed)?;
        let bytes = match method {
            0 => {
                if packed as u64 != full {
                    return Err(format!("{name} is damaged"));
                }
                data.to_vec()
            }
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

    struct HandEntry<'a> {
        name: &'a [u8],
        local_name: Option<&'a [u8]>,
        data: &'a [u8],
        method: u16,
        flags: u16,
        local_flags: Option<u16>,
        made_by: u16,
        ext_attr: u32,
        crc_override: Option<u32>,
        packed_override: Option<u32>,
        full_override: Option<u32>,
        local_crc_override: Option<u32>,
        local_packed_override: Option<u32>,
        local_full_override: Option<u32>,
        data_descriptor: Option<bool>, // Some(true): with sig, Some(false): without sig
    }

    impl<'a> HandEntry<'a> {
        fn stored(name: &'a [u8], data: &'a [u8]) -> Self {
            Self {
                name,
                local_name: None,
                data,
                method: 0,
                flags: UTF8,
                local_flags: None,
                made_by: 20,
                ext_attr: 0,
                crc_override: None,
                packed_override: None,
                full_override: None,
                local_crc_override: None,
                local_packed_override: None,
                local_full_override: None,
                data_descriptor: None,
            }
        }
    }

    fn build_hand_zip(entries: &[HandEntry<'_>], comment: &[u8]) -> Vec<u8> {
        let mut out = Vec::new();
        let mut central = Vec::new();
        for e in entries {
            let offset = out.len() as u32;
            let sum = e.crc_override.unwrap_or_else(|| crc(e.data));
            let packed = e.packed_override.unwrap_or(e.data.len() as u32);
            let full = e.full_override.unwrap_or(e.data.len() as u32);
            let lflags = e.local_flags.unwrap_or(e.flags);
            let lname = e.local_name.unwrap_or(e.name);
            let (lcrc, lpacked, lfull) = if lflags & DATA_DESCRIPTOR != 0 {
                (
                    e.local_crc_override.unwrap_or(0),
                    e.local_packed_override.unwrap_or(0),
                    e.local_full_override.unwrap_or(0),
                )
            } else {
                (
                    e.local_crc_override.unwrap_or(sum),
                    e.local_packed_override.unwrap_or(packed),
                    e.local_full_override.unwrap_or(full),
                )
            };
            out.extend(LOCAL.to_le_bytes());
            for v in [20u16, lflags, e.method, 0, 0x21] {
                out.extend(v.to_le_bytes());
            }
            for v in [lcrc, lpacked, lfull] {
                out.extend(v.to_le_bytes());
            }
            out.extend((lname.len() as u16).to_le_bytes());
            out.extend(0u16.to_le_bytes());
            out.extend(lname);
            out.extend(e.data);
            if let Some(with_sig) = e.data_descriptor {
                if with_sig {
                    out.extend(0x0807_4b50u32.to_le_bytes());
                }
                for v in [sum, packed, full] {
                    out.extend(v.to_le_bytes());
                }
            }
            central.extend(CENTRAL.to_le_bytes());
            central.extend(e.made_by.to_le_bytes());
            for v in [20u16, e.flags, e.method, 0, 0x21] {
                central.extend(v.to_le_bytes());
            }
            for v in [sum, packed, full] {
                central.extend(v.to_le_bytes());
            }
            central.extend((e.name.len() as u16).to_le_bytes());
            central.extend([0u8; 2 + 2 + 2 + 2]);
            central.extend(e.ext_attr.to_le_bytes());
            central.extend(offset.to_le_bytes());
            central.extend(e.name);
        }
        let cd_start = out.len() as u32;
        out.extend(&central);
        out.extend(END.to_le_bytes());
        out.extend([0u8; 4]);
        let count = entries.len() as u16;
        out.extend(count.to_le_bytes());
        out.extend(count.to_le_bytes());
        out.extend((central.len() as u32).to_le_bytes());
        out.extend(cd_start.to_le_bytes());
        out.extend((comment.len() as u16).to_le_bytes());
        out.extend(comment);
        out
    }

    #[test]
    fn hand_built_malicious_and_edge_case_archives() {
        // Symlink (Unix S_IFLNK) is refused.
        let mut sym = HandEntry::stored(b"link.txt", b"/etc/passwd");
        sym.made_by = (3 << 8) | 20;
        sym.ext_attr = (0o120_777u32) << 16;
        assert!(read(&build_hand_zip(&[sym], b""), 1 << 20)
            .unwrap_err()
            .contains("symbolic link"));

        // Special Unix file (FIFO) is refused.
        let mut fifo = HandEntry::stored(b"pipe", b"");
        fifo.made_by = (3 << 8) | 20;
        fifo.ext_attr = (0o010_644u32) << 16;
        assert!(read(&build_hand_zip(&[fifo], b""), 1 << 20)
            .unwrap_err()
            .contains("unsupported file type"));

        // Directory entry with traversal is refused; safe directory entry is skipped.
        let bad_dir = HandEntry::stored(b"../evil/", b"");
        assert!(read(&build_hand_zip(&[bad_dir], b""), 1 << 20)
            .unwrap_err()
            .contains("unsafe"));
        let good_dir = HandEntry::stored(b"files/", b"");
        let file = HandEntry::stored(b"files/a.txt", b"ok");
        assert_eq!(
            read(&build_hand_zip(&[good_dir, file], b""), 1 << 20).unwrap(),
            vec![("files/a.txt".into(), b"ok".to_vec())]
        );

        // Duplicate file names are refused on both write and read.
        assert!(write(&[("a.txt".into(), vec![1]), ("a.txt".into(), vec![2])]).is_err());
        let dup = build_hand_zip(
            &[HandEntry::stored(b"a.txt", b"one"), HandEntry::stored(b"a.txt", b"two")],
            b"",
        );
        assert!(read(&dup, 1 << 20).unwrap_err().contains("duplicate"));

        // CP437 filename (UTF-8 flag clear) decodes properly; invalid UTF-8 with flag set fails.
        let mut cp = HandEntry::stored(b"caf\x82.txt", b"hi");
        cp.flags = 0;
        assert_eq!(
            read(&build_hand_zip(&[cp], b""), 1 << 20).unwrap(),
            vec![("caf\u{00e9}.txt".into(), b"hi".to_vec())]
        );
        let bad_utf8 = HandEntry::stored(b"caf\x82.txt", b"hi");
        assert!(read(&build_hand_zip(&[bad_utf8], b""), 1 << 20).is_err());

        // Data descriptor (bit 3 set, local CRC/sizes zeroed) with and without signature succeeds.
        for with_sig in [true, false] {
            let mut dd = HandEntry::stored(b"dd.txt", b"streamed");
            dd.flags = UTF8 | DATA_DESCRIPTOR;
            dd.data_descriptor = Some(with_sig);
            assert_eq!(
                read(&build_hand_zip(&[dd], b""), 1 << 20).unwrap(),
                vec![("dd.txt".into(), b"streamed".to_vec())]
            );
        }

        // EOCD comment containing PK\x05\x06 signature does not confuse the parser.
        let mut comment = b"fake ".to_vec();
        comment.extend(END.to_le_bytes());
        comment.extend([0u8; 18]);
        let with_comment = build_hand_zip(&[HandEntry::stored(b"a.txt", b"ok")], &comment);
        assert_eq!(
            read(&with_comment, 1 << 20).unwrap(),
            vec![("a.txt".into(), b"ok".to_vec())]
        );

        // Local header filename mismatch (CD vs local smuggling) is refused.
        let mut mismatch = HandEntry::stored(b"safe.txt", b"ok");
        mismatch.local_name = Some(b"../x.txt");
        assert!(read(&build_hand_zip(&[mismatch], b""), 1 << 20).is_err());

        // Local header CRC mismatch (without data descriptor) is refused.
        let mut bad_local_crc = HandEntry::stored(b"a.txt", b"ok");
        bad_local_crc.local_crc_override = Some(0xdead_beef);
        assert!(read(&build_hand_zip(&[bad_local_crc], b""), 1 << 20).is_err());

        // ZIP64 sentinel is explicitly refused.
        let mut z64 = HandEntry::stored(b"big.bin", b"");
        z64.full_override = Some(u32::MAX);
        assert!(read(&build_hand_zip(&[z64], b""), u64::MAX)
            .unwrap_err()
            .contains("zip64"));

        // Deflate bomb claiming smaller uncompressed size than it expands to is refused.
        let zeros = vec![0u8; 4096];
        let mut enc = DeflateEncoder::new(Vec::new(), Compression::default());
        enc.write_all(&zeros).unwrap();
        let deflated = enc.finish().unwrap();
        let mut bomb = HandEntry::stored(b"bomb.bin", &deflated);
        bomb.method = 8;
        bomb.crc_override = Some(crc(&zeros));
        bomb.packed_override = Some(deflated.len() as u32);
        bomb.full_override = Some(16);
        assert!(read(&build_hand_zip(&[bomb], b""), 1 << 20).is_err());

        // STORED size mismatch (packed > full) is refused before allocating.
        let mut bad_stored = HandEntry::stored(b"a.bin", b"0123456789abcdef");
        bad_stored.full_override = Some(4);
        assert!(read(&build_hand_zip(&[bad_stored], b""), 1 << 20).is_err());

        // Multi-disk archive is refused.
        let mut multi = build_hand_zip(&[HandEntry::stored(b"a.txt", b"ok")], b"");
        let eocd = multi.len() - 22;
        multi[eocd + 4] = 1;
        assert!(read(&multi, 1 << 20).unwrap_err().contains("multi-disk"));
    }
}
