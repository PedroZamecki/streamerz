//! Pictures: the platforms' logos (built in, from Simple Icons, CC0) and pictures from the web (profile
//! pictures, emotes). Web ones are fetched on the engine's thread into `cache/` next to the settings
//! file, and decoded only while the window shows them; hiding the window frees them.

use super::{tell, Msg};
use crate::engine::UiCmd;
use fltk::enums::ColorDepth;
use fltk::image::{JpegImage, PngImage, RgbImage, SvgImage};
use fltk::prelude::*;
use sha2::{Digest, Sha256};
use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use tokio::sync::mpsc::UnboundedSender;

/// A platform's or app's logo, `size` px square (sharp at any screen scale: it's an SVG).
pub fn logo(name: &str, size: i32) -> Option<SvgImage> {
    let svg = match name {
        "Twitch" => include_str!("../../assets/icons/twitch.svg"),
        "Kick" => include_str!("../../assets/icons/kick.svg"),
        "YouTube" => include_str!("../../assets/icons/youtube.svg"),
        "TikTok" => include_str!("../../assets/icons/tiktok.svg"),
        "OBS Studio" => include_str!("../../assets/icons/obsstudio.svg"),
        "Streamlabs Desktop" | "Streamlabs" => include_str!("../../assets/icons/streamlabs.svg"),
        "Discord" => include_str!("../../assets/icons/discord.svg"),
        _ => return None,
    };
    let mut i = SvgImage::from_data(svg).ok()?;
    i.scale(size, size, true, true);
    Some(i)
}

/// A picture as shown: its frames (one when still) with how long each shows, in ms.
#[derive(Clone)]
pub struct Pic {
    frames: Vec<(RgbImage, u32)>,
    total: u32,
}

impl Pic {
    /// The frame to show `ms` into the animation.
    pub fn at(&self, ms: u64) -> &RgbImage {
        let mut t = (ms % u64::from(self.total.max(1))) as u32;
        for (f, d) in &self.frames {
            if t < *d {
                return f;
            }
            t -= d;
        }
        &self.frames[0].0
    }

    pub fn first(&self) -> RgbImage {
        self.frames[0].0.clone()
    }

    pub fn animated(&self) -> bool {
        self.frames.len() > 1
    }

    pub fn w(&self) -> i32 {
        self.frames[0].0.w()
    }

    pub fn h(&self) -> i32 {
        self.frames[0].0.h()
    }
}

thread_local! {
    /// Decoded pictures by (url, height, round); None when the file isn't a picture.
    static SHOWN: RefCell<HashMap<(String, i32, bool), Option<Pic>>> = RefCell::default();
    /// URLs being fetched (or that failed this run: not asked again).
    static ASKED: RefCell<HashSet<String>> = RefCell::default();
}

/// Pictures downloaded this run (the emote picker refills when it changes).
pub static ARRIVED: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

/// Downloads at a time (opening the emoji picker asks for a few hundred pictures).
static FETCHING: tokio::sync::Semaphore = tokio::sync::Semaphore::const_new(6);

fn dir() -> PathBuf {
    crate::config_path().with_file_name("cache")
}

fn path(url: &str) -> PathBuf {
    let h = Sha256::digest(url.as_bytes());
    let name: String = h[..12].iter().map(|b| format!("{b:02x}")).collect();
    dir().join(name)
}

/// What was saved as `name` in the cache (emote lists, picture URLs...), and how many seconds ago.
pub fn saved(name: &str) -> Option<(u64, serde_json::Value)> {
    let file = dir().join(format!("{name}.json"));
    let age = file
        .metadata()
        .ok()?
        .modified()
        .ok()?
        .elapsed()
        .map_or(0, |d| d.as_secs());
    Some((age, serde_json::from_slice(&std::fs::read(file).ok()?).ok()?))
}

/// Save `v` as `name` in the cache (whole or nothing).
pub fn save(name: &str, v: &serde_json::Value) {
    let file = dir().join(format!("{name}.json"));
    let tmp = file.with_extension("part");
    let _ = std::fs::create_dir_all(dir());
    let _ = std::fs::write(&tmp, v.to_string()).and_then(|()| std::fs::rename(&tmp, &file));
}

/// The pictures kept at most, in bytes; past it the least recently used go.
const KEEP_BYTES: u64 = 50 * 1024 * 1024;

/// Drop the least recently used pictures (by modified time: a picture shown is touched) until the
/// pictures take at most `keep` bytes. The saved lists (`.json`) stay.
fn prune(dir: &std::path::Path, keep: u64) {
    let Ok(rd) = std::fs::read_dir(dir) else { return };
    let mut files: Vec<(std::time::SystemTime, u64, PathBuf)> = rd
        .flatten()
        .filter(|e| e.path().extension().is_none())
        .filter_map(|e| {
            let m = e.metadata().ok()?;
            Some((m.modified().ok()?, m.len(), e.path()))
        })
        .collect();
    let mut total: u64 = files.iter().map(|f| f.1).sum();
    if total <= keep {
        return;
    }
    files.sort_unstable_by_key(|f| f.0);
    for (_, len, p) in files {
        if total <= keep {
            break;
        }
        if std::fs::remove_file(&p).is_ok() {
            total -= len;
        }
    }
}

/// The picture at `url`, `h` px high (round for profile pictures). None until it's on disk: then
/// it's fetched, and the window gets `Msg::Image` when it's there.
pub fn get(cmds: &UnboundedSender<UiCmd>, url: &str, h: i32, round: bool) -> Option<Pic> {
    if url.is_empty() {
        return None;
    }
    let key = (url.to_string(), h, round);
    if let Some(i) = SHOWN.with_borrow(|m| m.get(&key).cloned()) {
        return i;
    }
    let file = path(url);
    let Ok(bytes) = std::fs::read(&file) else {
        // The first download of a run, then every 200th, keeps the folder in size.
        if ASKED.with_borrow(HashSet::is_empty) {
            prune(&dir(), KEEP_BYTES);
        }
        if ASKED.with_borrow_mut(|a| a.insert(url.to_string())) {
            let url = url.to_string();
            tell(cmds, move |_| {
                tokio::task::spawn_local(async move {
                    let _one = FETCHING.acquire().await;
                    match crate::http::request("GET", &url, &[], b"").await {
                        Ok(r) if r.status == 200 => {
                            let _ = std::fs::create_dir_all(file.parent().unwrap_or(&file));
                            // Whole or nothing: a half-written file would be a broken picture forever.
                            let tmp = file.with_extension("part");
                            if std::fs::write(&tmp, &r.body)
                                .and_then(|()| std::fs::rename(&tmp, &file))
                                .is_ok()
                            {
                                let n = ARRIVED.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                                if n % 200 == 199 {
                                    prune(&dir(), KEEP_BYTES);
                                }
                                super::wake(Msg::Image);
                            }
                        }
                        _ => {}
                    }
                });
            });
        }
        return None;
    };
    // Shown: recently used, so pruning keeps it.
    let _ = std::fs::File::options()
        .write(true)
        .open(&file)
        .and_then(|f| f.set_modified(std::time::SystemTime::now()));
    let frames: Vec<_> = decode(&bytes).into_iter().map(|(i, d)| (fit(i, h, round), d)).collect();
    let pic = (!frames.is_empty()).then(|| Pic {
        total: frames.iter().map(|f| f.1).sum(),
        frames,
    });
    SHOWN.with_borrow_mut(|m| m.insert(key, pic.clone()));
    pic
}

/// The window was hidden: free the decoded pictures (the files stay).
pub fn forget() {
    SHOWN.with_borrow_mut(HashMap::clear);
}

/// The PNG without its colour profile: libpng warns on stderr about many emotes' broken one, and
/// colours are drawn as sRGB anyway.
fn no_iccp(b: &[u8]) -> Vec<u8> {
    let mut out = b[..8.min(b.len())].to_vec();
    let mut i = 8;
    while let Some(len) = b
        .get(i..i + 4)
        .map(|l| u32::from_be_bytes(l.try_into().unwrap()) as usize)
    {
        let end = (i + 12 + len).min(b.len());
        if b.get(i + 4..i + 8) != Some(b"iCCP") {
            out.extend_from_slice(&b[i..end]);
        }
        i = end;
    }
    out
}

/// Frames kept of an animation (memory: each is a full picture).
const MAX_FRAMES: usize = 150;

/// PNG, JPEG, GIF and WebP (animated too) or SVG, as RGB(A) frames with their time in ms; none when
/// it isn't a picture.
pub fn decode(b: &[u8]) -> Vec<(RgbImage, u32)> {
    let still = |i: Option<RgbImage>| i.map(|i| (i, 1000)).into_iter().collect();
    if b.starts_with(b"\x89PNG") {
        still(PngImage::from_data(&no_iccp(b)).ok().and_then(|i| i.to_rgb().ok()))
    } else if b.starts_with(&[0xff, 0xd8]) {
        still(JpegImage::from_data(b).ok().and_then(|i| i.to_rgb().ok()))
    } else if b.starts_with(b"GIF") {
        gif(b).unwrap_or_default()
    } else if b.starts_with(b"RIFF") && b.get(8..12) == Some(b"WEBP") {
        webp(b).unwrap_or_default()
    } else {
        let Some(s) = std::str::from_utf8(b).ok().filter(|s| s.contains("<svg")) else {
            return Vec::new();
        };
        let Ok(mut i) = SvgImage::from_data(s) else {
            return Vec::new();
        };
        // Rasterized at a size that stays sharp once fitted.
        i.scale(128, 128, true, true);
        // SVGs are drawn on first draw; this draws it now.
        i.normalize();
        still(i.to_rgb().ok())
    }
}

/// A GIF's frames, each painted on the canvas as the previous ones left it.
fn gif(b: &[u8]) -> Option<Vec<(RgbImage, u32)>> {
    let mut o = gif::DecodeOptions::new();
    o.set_color_output(gif::ColorOutput::RGBA);
    let mut d = o.read_info(b).ok()?;
    let (w, h) = (usize::from(d.width()), usize::from(d.height()));
    let mut canvas = vec![0u8; w * h * 4];
    let mut out = Vec::new();
    while let Ok(Some(f)) = d.read_next_frame() {
        let before = (f.dispose == gif::DisposalMethod::Previous).then(|| canvas.clone());
        let (fx, fy, fw) = (usize::from(f.left), usize::from(f.top), usize::from(f.width));
        for (i, px) in f.buffer.as_chunks::<4>().0.iter().enumerate() {
            let (x, y) = (fx + i % fw, fy + i / fw);
            if px[3] != 0 && x < w && y < h {
                canvas[(y * w + x) * 4..][..4].copy_from_slice(px);
            }
        }
        if let Ok(i) = RgbImage::new(&canvas, w as i32, h as i32, ColorDepth::Rgba8) {
            // In 10 ms; browsers show 0 and 1 as 100 ms.
            let ms = u32::from(f.delay) * 10;
            out.push((i, if ms < 20 { 100 } else { ms }));
        }
        if out.len() == MAX_FRAMES {
            break;
        }
        match (f.dispose, before) {
            (_, Some(b)) => canvas = b,
            (gif::DisposalMethod::Background, _) => {
                for y in fy..(fy + usize::from(f.height)).min(h) {
                    for x in fx..(fx + fw).min(w) {
                        canvas[(y * w + x) * 4..][..4].fill(0);
                    }
                }
            }
            _ => {}
        }
    }
    Some(out)
}

fn webp(b: &[u8]) -> Option<Vec<(RgbImage, u32)>> {
    let mut d = image_webp::WebPDecoder::new(std::io::Cursor::new(b)).ok()?;
    let (w, h) = d.dimensions();
    let depth = if d.has_alpha() {
        ColorDepth::Rgba8
    } else {
        ColorDepth::Rgb8
    };
    let mut px = vec![0; d.output_buffer_size()?];
    if !d.is_animated() {
        d.read_image(&mut px).ok()?;
        return Some(vec![(RgbImage::new(&px, w as i32, h as i32, depth).ok()?, 1000)]);
    }
    // Frames come painted on the animation's background colour, often opaque black.
    d.set_background_color([0, 0, 0, 0]).ok()?;
    let mut out = Vec::new();
    for _ in 0..d.num_frames().min(MAX_FRAMES as u32) {
        let Ok(ms) = d.read_frame(&mut px) else { break };
        if let Ok(f) = RgbImage::new(&px, w as i32, h as i32, depth) {
            out.push((f, if ms < 20 { 100 } else { ms }));
        }
    }
    Some(out)
}

/// `i` at `w`x`h`. Shrinking averages every pixel each one covers (alpha-weighted, so no dark
/// fringes): FLTK's copy picks one pixel, which looks jagged; growing is left to FLTK (bilinear).
fn shrink(i: &RgbImage, w: i32, h: i32) -> RgbImage {
    let (sw, sh, d) = (i.data_w() as usize, i.data_h() as usize, i.depth() as usize);
    let (w, h) = (w.max(1) as usize, h.max(1) as usize);
    if w >= sw || h >= sh || d == 0 {
        return i.copy_sized(w as i32, h as i32);
    }
    let src = i.to_rgb_data();
    // The alpha channel, if any: grey+alpha or RGBA.
    let alpha = matches!(d, 2 | 4).then_some(d - 1);
    let mut out = vec![0u8; w * h * d];
    for y in 0..h {
        let (y0, y1) = (y * sh / h, ((y + 1) * sh / h).max(y * sh / h + 1));
        for x in 0..w {
            let (x0, x1) = (x * sw / w, ((x + 1) * sw / w).max(x * sw / w + 1));
            let (mut sum, mut weight, mut n) = ([0u64; 4], 0u64, 0u64);
            for sy in y0..y1 {
                for sx in x0..x1 {
                    let px = &src[(sy * sw + sx) * d..][..d];
                    let a = alpha.map_or(255, |k| u64::from(px[k]));
                    for c in 0..d {
                        sum[c] += u64::from(px[c]) * if Some(c) == alpha { 1 } else { a };
                    }
                    weight += a;
                    n += 1;
                }
            }
            let o = &mut out[(y * w + x) * d..][..d];
            for c in 0..d {
                o[c] = if Some(c) == alpha {
                    (sum[c] / n) as u8
                } else {
                    sum[c].checked_div(weight).unwrap_or(0) as u8
                };
            }
        }
    }
    let depth = match d {
        1 => ColorDepth::L8,
        2 => ColorDepth::La8,
        3 => ColorDepth::Rgb8,
        _ => ColorDepth::Rgba8,
    };
    RgbImage::new(&out, w as i32, h as i32, depth).unwrap_or_else(|_| i.copy_sized(w as i32, h as i32))
}

/// Resized to `h` high (in screen pixels, so it's sharp on scaled screens), cut round if asked.
fn fit(i: RgbImage, h: i32, round: bool) -> RgbImage {
    fit_at(i, h, round, fltk::app::screen_scale(0))
}

/// `fit` for a screen scaled by `scale`.
fn fit_at(i: RgbImage, h: i32, round: bool, scale: f32) -> RgbImage {
    let scale = scale.max(1.0);
    let ph = (h as f32 * scale).ceil() as i32;
    let pw = (i.data_w() * ph / i.data_h().max(1)).max(1);
    let w = (pw as f32 / scale).round() as i32;
    let mut i = shrink(&i, pw, ph);
    if round {
        if let Ok(rgba) = i.convert(ColorDepth::Rgba8) {
            let mut d = rgba.to_rgb_data();
            let r = ph as f32 / 2.0;
            for y in 0..ph {
                for x in 0..pw {
                    let (dx, dy) = (x as f32 + 0.5 - r, y as f32 + 0.5 - r);
                    // Soft edge: 1 px of partial alpha.
                    let a = (r - (dx * dx + dy * dy).sqrt()).clamp(0.0, 1.0);
                    let p = ((y * pw + x) * 4 + 3) as usize;
                    d[p] = (d[p] as f32 * a) as u8;
                }
            }
            if let Ok(r) = RgbImage::new(&d, pw, ph, ColorDepth::Rgba8) {
                i = r;
            }
        }
    }
    i.scale(w, h, false, true);
    i
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn drops_only_the_colour_profile() {
        let chunk = |t: &[u8], d: &[u8]| {
            let mut c = (d.len() as u32).to_be_bytes().to_vec();
            c.extend_from_slice(t);
            c.extend_from_slice(d);
            c.extend_from_slice(&[0; 4]);
            c
        };
        let sig = b"\x89PNG\r\n\x1a\n".to_vec();
        let (ihdr, iccp, iend) = (chunk(b"IHDR", &[1; 13]), chunk(b"iCCP", &[2; 5]), chunk(b"IEND", &[]));
        let png = [sig.clone(), ihdr.clone(), iccp, iend.clone()].concat();
        assert_eq!(no_iccp(&png), [sig, ihdr, iend].concat());
    }

    #[test]
    fn shrinks_by_averaging_without_dark_edges() {
        // 2x2 to 1x1: red, transparent black, red, transparent black.
        let px = [255, 0, 0, 255, 0, 0, 0, 0, 255, 0, 0, 255, 0, 0, 0, 0];
        let i = RgbImage::new(&px, 2, 2, ColorDepth::Rgba8).unwrap();
        let s = shrink(&i, 1, 1);
        assert_eq!(s.to_rgb_data(), [255, 0, 0, 127], "pure red, half see-through");
    }

    #[test]
    fn prunes_least_recently_used_pictures() {
        let d = std::env::temp_dir().join(format!("szprune{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        let now = std::time::SystemTime::now();
        for (i, name) in ["old", "mid", "new"].iter().enumerate() {
            let f = d.join(name);
            std::fs::write(&f, [0u8; 100]).unwrap();
            let t = now - std::time::Duration::from_secs(300 - i as u64 * 100);
            std::fs::File::options()
                .write(true)
                .open(&f)
                .unwrap()
                .set_modified(t)
                .unwrap();
        }
        std::fs::write(d.join("list.json"), [0u8; 500]).unwrap();
        prune(&d, 200);
        let left: Vec<bool> = ["old", "mid", "new", "list.json"]
            .iter()
            .map(|n| d.join(n).exists())
            .collect();
        assert_eq!(left, [false, true, true, true]);
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn decodes_animated_gif() {
        // 2x2, two frames: 50 ms, then 200 ms.
        let gif = [
            0x47, 0x49, 0x46, 0x38, 0x39, 0x61, 0x02, 0x00, 0x02, 0x00, 0x81, 0x00, 0x00, 0xff, 0x00, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x21, 0xff, 0x0b, 0x4e, 0x45, 0x54, 0x53, 0x43, 0x41, 0x50, 0x45,
            0x32, 0x2e, 0x30, 0x03, 0x01, 0x00, 0x00, 0x00, 0x21, 0xf9, 0x04, 0x00, 0x05, 0x00, 0x00, 0x00, 0x2c, 0x00,
            0x00, 0x00, 0x00, 0x02, 0x00, 0x02, 0x00, 0x00, 0x08, 0x06, 0x00, 0x01, 0x08, 0x04, 0x10, 0x10, 0x00, 0x21,
            0xf9, 0x04, 0x01, 0x14, 0x00, 0x01, 0x00, 0x2c, 0x00, 0x00, 0x00, 0x00, 0x02, 0x00, 0x02, 0x00, 0x81, 0x00,
            0x00, 0xff, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x08, 0x06, 0x00, 0x01, 0x08, 0x04, 0x10,
            0x10, 0x00, 0x3b,
        ];
        let f = decode(&gif);
        assert_eq!(f.iter().map(|f| f.1).collect::<Vec<_>>(), [50, 200]);
        assert_eq!(f[0].0.to_rgb_data()[..4], [0xff, 0, 0, 0xff]);
    }

    #[test]
    fn decodes_webp_and_svg_and_cuts_round() {
        // 1x1 lossless WebP, opaque red.
        let webp = [
            0x52, 0x49, 0x46, 0x46, 0x1c, 0x00, 0x00, 0x00, 0x57, 0x45, 0x42, 0x50, 0x56, 0x50, 0x38, 0x4c, 0x0f, 0x00,
            0x00, 0x00, 0x2f, 0x00, 0x00, 0x00, 0x00, 0x07, 0x10, 0xfd, 0x8f, 0xfe, 0x07, 0x22, 0xa2, 0xff, 0x01, 0x00,
        ];
        let i = &decode(&webp)[0].0;
        assert_eq!((i.data_w(), i.data_h()), (1, 1));
        let svg = include_bytes!("../../assets/icons/kick.svg");
        let r = fit_at(decode(svg).remove(0).0, 8, true, 2.0);
        assert_eq!((r.w(), r.h(), r.data_w()), (8, 8, 16));
        let d = r.to_rgb_data();
        assert_eq!(d[3], 0, "corner cut away");
        assert!(decode(b"not a picture").is_empty());
    }
}
