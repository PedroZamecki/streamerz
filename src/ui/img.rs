//! Pictures: the platforms' logos (built in: Simple Icons, CC0, and the ones Streamer.bot's docs publish for its integrations) and pictures from the web (profile
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

/// A platform's name as it writes it ("youtube" is "YouTube").
pub fn platform_name(slug: &str) -> &str {
    match slug.to_ascii_lowercase().as_str() {
        "twitch" => "Twitch",
        "youtube" => "YouTube",
        "kick" => "Kick",
        "tiktok" => "TikTok",
        _ => slug,
    }
}

/// A platform's or app's logo, `size` px square (sharp at any screen scale: it's an SVG).
pub fn logo(name: &str, size: i32) -> Option<SvgImage> {
    let svg = match name {
        "Twitch" => include_str!("../../assets/icons/twitch.svg"),
        "Kick" => include_str!("../../assets/icons/kick.svg"),
        "YouTube" => include_str!("../../assets/icons/youtube.svg"),
        "TikTok" => include_str!("../../assets/icons/tiktok.svg"),
        "OBS Studio" => include_str!("../../assets/icons/obsstudio.svg"),
        "Streamlabs Desktop" | "Streamlabs" => include_str!("../../assets/icons/streamlabs.svg"),
        "Spotify" => include_str!("../../assets/icons/spotify.svg"),
        "Discord" => include_str!("../../assets/icons/discord.svg"),
        "StreamElements" => include_str!("../../assets/icons/streamelements.svg"),
        "DonorDrive" => include_str!("../../assets/icons/donordrive.svg"),
        "Streamer.bot" => include_str!("../../assets/icons/streamerbot.svg"),
        "Crowd Control" => include_str!("../../assets/icons/crowdcontrol.svg"),
        "Elgato" => include_str!("../../assets/icons/elgato.svg"),
        "Patreon" => include_str!("../../assets/icons/patreon.svg"),
        "Shopify" => include_str!("../../assets/icons/shopify.svg"),
        "Claude Code" | "Claude Desktop" => include_str!("../../assets/icons/claude.svg"),
        "Codex" => include_str!("../../assets/icons/codex.svg"),
        "OpenCode" => include_str!("../../assets/icons/opencode.svg"),
        "Gemini CLI" => include_str!("../../assets/icons/gemini.svg"),
        "Cursor" => include_str!("../../assets/icons/cursor.svg"),
        "Windsurf" => include_str!("../../assets/icons/windsurf.svg"),
        "VS Code (Copilot)" => include_str!("../../assets/icons/vscode.svg"),
        "GitHub Copilot CLI" => include_str!("../../assets/icons/copilot.svg"),
        "LM Studio" => include_str!("../../assets/icons/lmstudio.svg"),
        "Qwen Code" => include_str!("../../assets/icons/qwen.svg"),
        "Amp" => include_str!("../../assets/icons/amp.svg"),
        "Pally.gg" => include_str!("../../assets/icons/pallygg.svg"),
        _ => return None,
    };
    let mut i = SvgImage::from_data(svg).ok()?;
    i.scale(size, size, true, true);
    Some(i)
}

/// The logos that are pictures (48 px PNGs), by the name of the app or service.
fn picture_logo(name: &str) -> Option<&'static [u8]> {
    Some(match name {
        "HypeRate" => include_bytes!("../../assets/icons/hyperate.png"),
        "Pulsoid" => include_bytes!("../../assets/icons/pulsoid.png"),
        "Meld Studio" => include_bytes!("../../assets/icons/meld-studio.png"),
        "Speaker.bot" => include_bytes!("../../assets/icons/speakerbot.png"),
        "VoiceMod" => include_bytes!("../../assets/icons/voicemod.png"),
        "VTube Studio" => include_bytes!("../../assets/icons/vtube-studio.png"),
        "Fourthwall" => include_bytes!("../../assets/icons/fourthwall.png"),
        "Ko-fi" => include_bytes!("../../assets/icons/ko-fi.png"),
        "PolyPop" => include_bytes!("../../assets/icons/polypop.png"),
        _ => return None,
    })
}

/// Whether `name` has a logo, vector or picture.
pub fn has_logo(name: &str) -> bool {
    picture_logo(name).is_some() || logo(name, 16).is_some()
}

/// The logo of `name` as any kind of image, `size` px square: for trees, which take one icon.
pub fn any_logo(name: &str, size: i32) -> Option<fltk::image::SharedImage> {
    if let Some(l) = logo(name, size) {
        return fltk::image::SharedImage::from_image(&l).ok();
    }
    let mut p = PngImage::from_data(picture_logo(name)?).ok()?.to_rgb().ok()?;
    p.scale(size, size, true, true);
    fltk::image::SharedImage::from_image(&p).ok()
}

/// Platform logos arranged in a row for a combined connection notice.
pub fn platform_logos(platforms: &[&str], size: i32) -> Option<SvgImage> {
    if platforms.is_empty() {
        return None;
    }
    let mut body = String::new();
    for (i, p) in platforms.iter().enumerate() {
        let (svg, fill) = match *p {
            "twitch" => (include_str!("../../assets/icons/twitch.svg"), "#9146FF"),
            "youtube" => (include_str!("../../assets/icons/youtube.svg"), "#FF0000"),
            "kick" => (include_str!("../../assets/icons/kick.svg"), "#53FC18"),
            "tiktok" => (include_str!("../../assets/icons/tiktok.svg"), "#FE2C55"),
            _ => return None,
        };
        let paths = svg.split_once('>')?.1.rsplit_once("</svg>")?.0;
        body.push_str(&format!(
            "<g transform=\"translate({} 0)\" fill=\"{fill}\">{paths}</g>",
            i * 30
        ));
    }
    let width = platforms.len() as i32 * (size + 4) - 4;
    let mut image = SvgImage::from_data(&format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 {} 24\">{body}</svg>",
        platforms.len() * 30 - 6
    ))
    .ok()?;
    image.scale(width, size, false, true);
    Some(image)
}

/// A picture as shown: its frames (one when still) with how long each shows, in ms.
#[derive(Clone)]
pub struct Pic {
    frames: Vec<(RgbImage, u32)>,
    total: u32,
}

#[cfg(test)]
pub(super) fn test_pic() -> Pic {
    Pic {
        frames: vec![(RgbImage::new(&[255, 0, 0], 1, 1, ColorDepth::Rgb8).unwrap(), 1000)],
        total: 1000,
    }
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

    /// What its frames take in memory.
    fn bytes(&self) -> usize {
        self.frames
            .iter()
            .map(|f| (f.0.data_w() * f.0.data_h() * f.0.depth() as i32).max(0) as usize)
            .sum()
    }

    pub fn w(&self) -> i32 {
        self.frames[0].0.w()
    }

    pub fn h(&self) -> i32 {
        self.frames[0].0.h()
    }
}

/// A picture's cache key: its URL, height and whether it is cut round.
type Key = (String, i32, bool);

/// The decoded pictures kept at most, in bytes (animations are a full picture per frame); past it the
/// least recently used go, and are decoded again when something asks.
const SHOWN_BYTES: usize = 8 * 1024 * 1024;

/// Decoded pictures, the least recently used going when they take too much.
#[derive(Default)]
struct Shown {
    /// By key: the picture (None when the file isn't one), when it was last asked for, and its size.
    pics: HashMap<Key, (Option<Pic>, u64, usize)>,
    clock: u64,
    bytes: usize,
}

impl Shown {
    fn get(&mut self, key: &Key) -> Option<Option<Pic>> {
        self.clock += 1;
        let e = self.pics.get_mut(key)?;
        e.1 = self.clock;
        Some(e.0.clone())
    }

    fn put(&mut self, key: Key, pic: Option<Pic>) {
        self.clock += 1;
        let size = pic.as_ref().map_or(0, Pic::bytes);
        self.bytes += size;
        if let Some(old) = self.pics.insert(key, (pic, self.clock, size)) {
            self.bytes -= old.2;
        }
        while self.bytes > SHOWN_BYTES && self.pics.len() > 1 {
            let Some(oldest) = self.pics.iter().min_by_key(|e| e.1 .1).map(|e| e.0.clone()) else {
                break;
            };
            if let Some(gone) = self.pics.remove(&oldest) {
                self.bytes -= gone.2;
            }
        }
    }

    fn trim(&mut self) {
        const TARGET: usize = 2 * 1024 * 1024;
        while self.bytes > TARGET && self.pics.len() > 1 {
            let Some(oldest) = self.pics.iter().min_by_key(|e| e.1 .1).map(|e| e.0.clone()) else {
                break;
            };
            if let Some(gone) = self.pics.remove(&oldest) {
                self.bytes -= gone.2;
            }
        }
        self.pics.shrink_to_fit();
    }
}

/// Evict decoded pictures down to idle target and trim heap memory.
pub fn trim() {
    SHOWN.with_borrow_mut(|s| s.trim());
    crate::trim_memory();
}

thread_local! {
    static SHOWN: RefCell<Shown> = RefCell::default();
    /// URLs being fetched (or that failed this run: not asked again).
    static ASKED: RefCell<HashSet<String>> = RefCell::default();
    /// Pictures being decoded on another thread.
    static DECODING: RefCell<HashSet<Key>> = RefCell::default();
}

/// What the decoding thread made of a file: the frames as plain pixels (FLTK's images belong to the
/// window's thread), or the bytes of one it leaves to the window's thread (an SVG).
enum Prepared {
    Frames(Vec<Raw>),
    Svg(Vec<u8>),
}

/// A frame as pixels: the data's size and depth, the size it shows at, and how long it shows (ms).
struct Raw {
    px: Vec<u8>,
    size: (i32, i32),
    depth: ColorDepth,
    shown: (i32, i32),
    ms: u32,
}

/// Pictures decoded and waiting for the window's thread to take them in.
static READY: std::sync::Mutex<Vec<(Key, Prepared)>> = std::sync::Mutex::new(Vec::new());
/// A hide invalidates decoded results and queued work from the previous visible window.
static GENERATION: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
/// A wakeup for the window is already on its way (they come in bursts: one per 150 ms is plenty).
static WAKING: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Decodes at a time: a burst of new pictures mustn't fill the machine.
static DECODERS: tokio::sync::Semaphore = tokio::sync::Semaphore::const_new(2);

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

/// Remove what was saved as `name`.
pub fn forget_saved(name: &str) {
    let _ = std::fs::remove_file(dir().join(format!("{name}.json")));
}

/// Save `v` as `name` in the cache (whole or nothing).
pub fn save(name: &str, v: &serde_json::Value) {
    let closed = if crate::DEV_DIR.get().is_some() {
        let Ok(closed) = crate::DEV_CACHE_CLOSED.lock() else {
            return;
        };
        if *closed {
            return;
        }
        Some(closed)
    } else {
        None
    };
    let _closed = closed;
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

/// The picture at `url`, `h` px high (round for profile pictures). None until it's on disk and decoded:
/// downloading and decoding happen off the window's thread, and the window gets `Msg::Image` when it's
/// there.
pub fn get(cmds: &UnboundedSender<UiCmd>, url: &str, h: i32, round: bool) -> Option<Pic> {
    if crate::memory_saver() || url.is_empty() {
        return None;
    }
    take_ready();
    let key = (url.to_string(), h, round);
    if let Some(i) = SHOWN.with_borrow_mut(|m| m.get(&key)) {
        return i;
    }
    if DECODING.with_borrow(|d| d.contains(&key)) {
        return None;
    }
    let file = path(url);
    if !file.is_file() {
        // The first download of a run, then every 200th, keeps the folder in size.
        if ASKED.with_borrow(HashSet::is_empty) {
            std::thread::Builder::new()
                .name("prune-cache".into())
                .spawn(|| prune(&dir(), KEEP_BYTES))
                .ok();
        }
        if ASKED.with_borrow_mut(|a| a.insert(url.to_string())) {
            let url = url.to_string();
            tell(cmds, move |_| {
                tokio::task::spawn_local(async move {
                    let _one = FETCHING.acquire().await;
                    match crate::http::request("GET", &url, &[], b"").await {
                        Ok(r) if r.status == 200 => {
                            let closed = if crate::DEV_DIR.get().is_some() {
                                let Ok(closed) = crate::DEV_CACHE_CLOSED.lock() else {
                                    return;
                                };
                                if *closed {
                                    return;
                                }
                                Some(closed)
                            } else {
                                None
                            };
                            let _closed = closed;
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
                                arrived();
                            }
                        }
                        _ => {}
                    }
                });
            });
        }
        return None;
    }
    DECODING.with_borrow_mut(|d| d.insert(key.clone()));
    let scale = fltk::app::screen_scale(0);
    let generation = GENERATION.load(std::sync::atomic::Ordering::Relaxed);
    tell(cmds, move |_| {
        tokio::task::spawn_local(async move {
            if GENERATION.load(std::sync::atomic::Ordering::Relaxed) != generation {
                return;
            }
            let _one = DECODERS.acquire().await;
            if GENERATION.load(std::sync::atomic::Ordering::Relaxed) != generation {
                return;
            }
            let made = tokio::task::spawn_blocking(move || {
                let bytes = std::fs::read(&file).ok()?;
                // Shown: recently used, so pruning keeps it.
                let _ = std::fs::File::options()
                    .write(true)
                    .open(&file)
                    .and_then(|f| f.set_modified(std::time::SystemTime::now()));
                Some(prepare(&bytes, h, round, scale))
            })
            .await
            .ok()
            .flatten()
            .unwrap_or(Prepared::Frames(Vec::new()));
            if publish(generation, key, made) {
                arrived();
            }
        });
    });
    None
}

fn publish(generation: usize, key: Key, made: Prepared) -> bool {
    let Ok(mut ready) = READY.lock() else { return false };
    // Check under the same lock as forget: a completion cannot refill the cleared queue.
    if GENERATION.load(std::sync::atomic::Ordering::Relaxed) != generation {
        drop(ready);
        drop(made);
        drop(key);
        crate::trim_memory();
        return false;
    }
    ready.push((key, made));
    true
}

/// Something came in for the window (a picture downloaded or decoded): it is told, once for a burst.
fn arrived() {
    if !WAKING.swap(true, std::sync::atomic::Ordering::Relaxed) {
        tokio::task::spawn_local(async {
            tokio::time::sleep(std::time::Duration::from_millis(150)).await;
            WAKING.store(false, std::sync::atomic::Ordering::Relaxed);
            super::wake(Msg::Image);
        });
    }
}

/// The window's thread takes in the pictures decoded for it.
fn take_ready() {
    let Some(done) = READY
        .lock()
        .ok()
        .filter(|r| !r.is_empty())
        .map(|mut r| std::mem::take(&mut *r))
    else {
        return;
    };
    for (key, made) in done {
        let frames: Vec<(RgbImage, u32)> = match made {
            Prepared::Frames(raws) => raws.into_iter().filter_map(|r| build(&r).map(|i| (i, r.ms))).collect(),
            Prepared::Svg(bytes) => decode(&bytes)
                .into_iter()
                .map(|(i, ms)| (fit(i, key.1, key.2), ms))
                .collect(),
        };
        let pic = (!frames.is_empty()).then(|| Pic {
            total: frames.iter().map(|f| f.1).sum(),
            frames,
        });
        DECODING.with_borrow_mut(|d| d.remove(&key));
        SHOWN.with_borrow_mut(|m| m.put(key, pic));
    }
}

/// The window was hidden: free the decoded pictures (the files stay).
pub fn forget() {
    GENERATION.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    SHOWN.with_borrow_mut(|m| *m = Shown::default());
    DECODING.with_borrow_mut(HashSet::clear);
    if let Ok(mut r) = READY.lock() {
        *r = Vec::new();
    }
    crate::trim_memory();
}

/// What a file is as frames of pixels, fitted to `h` (on another thread than the window's).
fn prepare(b: &[u8], h: i32, round: bool, scale: f32) -> Prepared {
    let drawn =
        b.starts_with(b"\x89PNG") || b.starts_with(&[0xff, 0xd8]) || b.starts_with(b"GIF") || b.starts_with(b"RIFF");
    if !drawn && std::str::from_utf8(b).is_ok_and(|s| s.contains("<svg")) {
        return Prepared::Svg(b.to_vec());
    }
    let mut out = Vec::new();
    decode_each(b, &mut |i, ms| out.push(fit_raw(&i, h, round, scale, ms)));
    Prepared::Frames(out)
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
    let mut out = Vec::new();
    decode_each(b, &mut |i, ms| out.push((i, ms)));
    out
}

/// `decode`, one frame at a time: an animation's frames needn't all be there at full size at once.
fn decode_each(b: &[u8], each: &mut dyn FnMut(RgbImage, u32)) {
    let mut still = |i: Option<RgbImage>| {
        if let Some(i) = i {
            each(i, 1000);
        }
    };
    if b.starts_with(b"\x89PNG") {
        still(PngImage::from_data(&no_iccp(b)).ok().and_then(|i| i.to_rgb().ok()))
    } else if b.starts_with(&[0xff, 0xd8]) {
        still(JpegImage::from_data(b).ok().and_then(|i| i.to_rgb().ok()))
    } else if b.starts_with(b"GIF") {
        gif(b, each);
    } else if b.starts_with(b"RIFF") && b.get(8..12) == Some(b"WEBP") {
        webp(b, each);
    } else {
        let Some(s) = std::str::from_utf8(b).ok().filter(|s| s.contains("<svg")) else {
            return;
        };
        let Ok(mut i) = SvgImage::from_data(s) else {
            return;
        };
        // Rasterized at a size that stays sharp once fitted.
        i.scale(128, 128, true, true);
        // SVGs are drawn on first draw; this draws it now.
        i.normalize();
        still(i.to_rgb().ok())
    }
}

/// A GIF's frames, each painted on the canvas as the previous ones left it.
fn gif(b: &[u8], each: &mut dyn FnMut(RgbImage, u32)) -> Option<()> {
    let mut o = gif::DecodeOptions::new();
    o.set_color_output(gif::ColorOutput::RGBA);
    let mut d = o.read_info(b).ok()?;
    let (w, h) = (usize::from(d.width()), usize::from(d.height()));
    let mut canvas = vec![0u8; w * h * 4];
    let mut n = 0;
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
            each(i, if ms < 20 { 100 } else { ms });
            n += 1;
        }
        if n == MAX_FRAMES {
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
    Some(())
}

fn webp(b: &[u8], each: &mut dyn FnMut(RgbImage, u32)) -> Option<()> {
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
        each(RgbImage::new(&px, w as i32, h as i32, depth).ok()?, 1000);
        return Some(());
    }
    // Frames come painted on the animation's background colour, often opaque black.
    d.set_background_color([0, 0, 0, 0]).ok()?;
    for _ in 0..d.num_frames().min(MAX_FRAMES as u32) {
        let Ok(ms) = d.read_frame(&mut px) else { break };
        if let Ok(f) = RgbImage::new(&px, w as i32, h as i32, depth) {
            each(f, if ms < 20 { 100 } else { ms });
        }
    }
    Some(())
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
    build(&fit_raw(&i, h, round, scale, 0)).unwrap_or(i)
}

/// `fit` as plain pixels, that can go between threads: the image to show is made from it by `build`.
fn fit_raw(i: &RgbImage, h: i32, round: bool, scale: f32, ms: u32) -> Raw {
    let scale = scale.max(1.0);
    let ph = (h as f32 * scale).ceil() as i32;
    let pw = (i.data_w() * ph / i.data_h().max(1)).max(1);
    let w = (pw as f32 / scale).round() as i32;
    let mut i = shrink(i, pw, ph);
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
    Raw {
        px: i.to_rgb_data(),
        size: (i.data_w(), i.data_h()),
        depth: i.depth(),
        shown: (w, h),
        ms,
    }
}

/// The image `r` describes, at the size it shows at.
fn build(r: &Raw) -> Option<RgbImage> {
    let mut i = RgbImage::new(&r.px, r.size.0, r.size.1, r.depth).ok()?;
    i.scale(r.shown.0, r.shown.1, false, true);
    Some(i)
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

    #[test]
    fn decoded_pictures_past_the_budget_free_the_least_recently_used() {
        let key = |n: i32| (format!("u{n}"), 20, false);
        let big = || {
            let px = vec![0u8; 1024 * 1024 * 4];
            let i = RgbImage::new(&px, 1024, 1024, ColorDepth::Rgba8).unwrap();
            Some(Pic {
                frames: vec![(i, 1000)],
                total: 1000,
            })
        };
        let mut shown = Shown::default();
        // 4 MB each: past 8 MB, the least recently used go.
        shown.put(key(0), big());
        shown.put(key(1), big());
        // Asking for the first makes it the newest.
        assert!(shown.get(&key(0)).is_some());
        shown.put(key(2), big());
        assert!(shown.get(&key(1)).is_none(), "the least recently used is gone");
        assert!(shown.get(&key(0)).is_some() && shown.get(&key(2)).is_some());
        assert!(shown.bytes <= SHOWN_BYTES);
    }

    #[test]
    fn a_file_is_made_into_frames_of_pixels_off_the_window_thread() {
        let svg = include_bytes!("../../assets/icons/kick.svg");
        assert!(matches!(prepare(svg, 8, false, 1.0), Prepared::Svg(_)));
        assert!(matches!(prepare(b"not a picture", 8, false, 1.0), Prepared::Frames(f) if f.is_empty()));
        let webp = [
            0x52, 0x49, 0x46, 0x46, 0x1c, 0x00, 0x00, 0x00, 0x57, 0x45, 0x42, 0x50, 0x56, 0x50, 0x38, 0x4c, 0x0f, 0x00,
            0x00, 0x00, 0x2f, 0x00, 0x00, 0x00, 0x00, 0x07, 0x10, 0xfd, 0x8f, 0xfe, 0x07, 0x22, 0xa2, 0xff, 0x01, 0x00,
        ];
        let Prepared::Frames(f) = prepare(&webp, 8, false, 2.0) else {
            panic!("frames")
        };
        // 1x1 grown to 8 logical px, 16 on a screen scaled 2: pixels and the size they show at.
        assert_eq!((f.len(), f[0].shown), (1, (8, 8)));
        assert!(build(&f[0]).is_some());
    }

    #[test]
    fn hiding_discards_late_decode_results_and_releases_ready_pixels() {
        let _guard = crate::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let pixels = || {
            Prepared::Frames(vec![Raw {
                px: vec![17; 1024 * 1024 * 4],
                size: (1024, 1024),
                depth: ColorDepth::Rgba8,
                shown: (20, 20),
                ms: 1000,
            }])
        };
        let key = ("test-picture".to_string(), 20, false);
        let old = GENERATION.load(std::sync::atomic::Ordering::Relaxed);
        forget();
        assert!(!publish(old, key.clone(), pixels()));
        assert!(
            READY.lock().unwrap().is_empty(),
            "an old completion refilled the hidden cache"
        );
        let current = GENERATION.load(std::sync::atomic::Ordering::Relaxed);
        assert!(publish(current, key, pixels()));
        assert_eq!(READY.lock().unwrap().len(), 1);
        forget();
        assert!(READY.lock().unwrap().is_empty(), "ready pixels survived hide");
    }
}
