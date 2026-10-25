//! Pictures: the platforms' logos (built in, from Simple Icons, CC0) and pictures from the web (profile
//! pictures, emotes). Web ones are fetched on the engine's thread into `cache/` next to the settings
//! file, and decoded only while the window shows them; hiding the window frees them.

use super::{tell, Msg};
use crate::engine::UiCmd;
use fltk::enums::ColorDepth;
use fltk::image::{GifImage, JpegImage, PngImage, RgbImage, SvgImage};
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

fn path(url: &str) -> PathBuf {
    let h = Sha256::digest(url.as_bytes());
    let name: String = h[..12].iter().map(|b| format!("{b:02x}")).collect();
    crate::config_path().with_file_name("cache").join(name)
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
                                ARRIVED.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
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

/// Frames kept of an animation (memory: each is a full picture).
const MAX_FRAMES: usize = 150;

/// PNG, JPEG, GIF (first frame), WebP (animated too) or SVG, as RGB(A) frames with their time in ms; none when
/// it isn't a picture.
pub fn decode(b: &[u8]) -> Vec<(RgbImage, u32)> {
    let still = |i: Option<RgbImage>| i.map(|i| (i, 1000)).into_iter().collect();
    if b.starts_with(b"\x89PNG") {
        still(PngImage::from_data(b).ok().and_then(|i| i.to_rgb().ok()))
    } else if b.starts_with(&[0xff, 0xd8]) {
        still(JpegImage::from_data(b).ok().and_then(|i| i.to_rgb().ok()))
    } else if b.starts_with(b"GIF") {
        // ponytail: first frame only; FLTK's animated GIF doesn't hand out its frames. BTTV, FFZ and 7TV
        // are asked for WebP, which animates; Twitch's own animated emotes would need a GIF decoder.
        still(GifImage::from_data(b).ok().and_then(|i| i.to_rgb().ok()))
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
    let mut i = i.copy_sized(pw, ph);
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
