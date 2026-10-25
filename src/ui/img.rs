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
        "Streamlabs Desktop" => include_str!("../../assets/icons/streamlabs.svg"),
        "Discord" => include_str!("../../assets/icons/discord.svg"),
        _ => return None,
    };
    let mut i = SvgImage::from_data(svg).ok()?;
    i.scale(size, size, true, true);
    Some(i)
}

thread_local! {
    /// Decoded pictures by (url, size, round); None when the file isn't a picture.
    static SHOWN: RefCell<HashMap<(String, i32, bool), Option<RgbImage>>> = RefCell::default();
    /// URLs being fetched (or that failed this run: not asked again).
    static ASKED: RefCell<HashSet<String>> = RefCell::default();
}

fn path(url: &str) -> PathBuf {
    let h = Sha256::digest(url.as_bytes());
    let name: String = h[..12].iter().map(|b| format!("{b:02x}")).collect();
    crate::config_path().with_file_name("cache").join(name)
}

/// The picture at `url`, `size` px square (round for profile pictures). None until it's on disk:
/// then it's fetched, and the window gets `Msg::Image` when it's there.
pub fn get(cmds: &UnboundedSender<UiCmd>, url: &str, size: i32, round: bool) -> Option<RgbImage> {
    if url.is_empty() {
        return None;
    }
    let key = (url.to_string(), size, round);
    if let Some(i) = SHOWN.with_borrow(|m| m.get(&key).cloned()) {
        return i;
    }
    let file = path(url);
    let Ok(bytes) = std::fs::read(&file) else {
        if ASKED.with_borrow_mut(|a| a.insert(url.to_string())) {
            let url = url.to_string();
            tell(cmds, move |_| {
                tokio::task::spawn_local(async move {
                    match crate::http::request("GET", &url, &[], b"").await {
                        Ok(r) if r.status == 200 => {
                            let _ = std::fs::create_dir_all(file.parent().unwrap_or(&file));
                            // Whole or nothing: a half-written file would be a broken picture forever.
                            let tmp = file.with_extension("part");
                            if std::fs::write(&tmp, &r.body)
                                .and_then(|()| std::fs::rename(&tmp, &file))
                                .is_ok()
                            {
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
    let i = decode(&bytes).map(|i| fit(i, size, round));
    SHOWN.with_borrow_mut(|m| m.insert(key, i.clone()));
    i
}

/// The window was hidden: free the decoded pictures (the files stay).
pub fn forget() {
    SHOWN.with_borrow_mut(HashMap::clear);
}

/// PNG, JPEG, GIF (first frame), WebP (first frame) or SVG, as RGB(A).
pub fn decode(b: &[u8]) -> Option<RgbImage> {
    if b.starts_with(b"\x89PNG") {
        PngImage::from_data(b).ok()?.to_rgb().ok()
    } else if b.starts_with(&[0xff, 0xd8]) {
        JpegImage::from_data(b).ok()?.to_rgb().ok()
    } else if b.starts_with(b"GIF") {
        GifImage::from_data(b).ok()?.to_rgb().ok()
    } else if b.starts_with(b"RIFF") && b.get(8..12) == Some(b"WEBP") {
        let mut d = image_webp::WebPDecoder::new(std::io::Cursor::new(b)).ok()?;
        let (w, h) = d.dimensions();
        let mut px = vec![0; d.output_buffer_size()?];
        d.read_image(&mut px).ok()?;
        let depth = if d.has_alpha() {
            ColorDepth::Rgba8
        } else {
            ColorDepth::Rgb8
        };
        RgbImage::new(&px, w as i32, h as i32, depth).ok()
    } else {
        let s = std::str::from_utf8(b).ok().filter(|s| s.contains("<svg"))?;
        // Rasterized at a size that stays sharp once fitted.
        let mut i = SvgImage::from_data(s).ok()?;
        i.scale(128, 128, true, true);
        // SVGs are drawn on first draw; this draws it now.
        i.normalize();
        i.to_rgb().ok()
    }
}

/// Resized to `size` (twice that in pixels, so it's sharp on scaled screens), cut round if asked.
fn fit(i: RgbImage, size: i32, round: bool) -> RgbImage {
    let px = size * 2;
    let mut i = i.copy_sized(px, px);
    if round {
        if let Ok(rgba) = i.convert(ColorDepth::Rgba8) {
            let mut d = rgba.to_rgb_data();
            let r = px as f32 / 2.0;
            for y in 0..px {
                for x in 0..px {
                    let (dx, dy) = (x as f32 + 0.5 - r, y as f32 + 0.5 - r);
                    // Soft edge: 1 px of partial alpha.
                    let a = (r - (dx * dx + dy * dy).sqrt()).clamp(0.0, 1.0);
                    let p = ((y * px + x) * 4 + 3) as usize;
                    d[p] = (d[p] as f32 * a) as u8;
                }
            }
            if let Ok(r) = RgbImage::new(&d, px, px, ColorDepth::Rgba8) {
                i = r;
            }
        }
    }
    i.scale(size, size, true, true);
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
        let i = decode(&webp).expect("webp");
        assert_eq!((i.data_w(), i.data_h()), (1, 1));
        let svg = include_bytes!("../../assets/icons/kick.svg");
        let r = fit(decode(svg).expect("svg"), 8, true);
        assert_eq!((r.w(), r.h(), r.data_w()), (8, 8, 16));
        let d = r.to_rgb_data();
        assert_eq!(d[3], 0, "corner cut away");
        assert!(decode(b"not a picture").is_none());
    }
}
