//! Dev mode's eyes and hands for the window (`STREAMERZ_DEV=1`, the `StreamerzUi` request and the MCP
//! `ui` tool): screenshots of the app's own windows (taken from inside, so they work hidden, off screen
//! or behind others), the widget tree with where each thing is, and going to a page.

use super::nav;
use fltk::group::Wizard;
use fltk::prelude::*;
use fltk::{app, draw};
use std::fmt::Write;

/// Runs one command and says what happened.
///   shot PATH [N]   saves window N (0, the top one, by default) as a PNG
///   windows         the windows: index, title, position and size
///   tree [N]        the widgets of window N: depth, label, x y w h (in the window), hidden ones marked
///   find TEXT [N]   where widgets with TEXT in their label are, as screen coordinates to click
///   page LABEL      goes to a page (Chat, Settings...)
///   chat            the last chat line the chat page took in (its seq)
///   community       how many users the Community tab lists
///   panic           panics on the window's thread (to try crash reports and saving on a crash)
pub fn run(cmd: &str, pages: Option<&Wizard>) -> String {
    let mut words = cmd.split_whitespace();
    let (what, a, b) = (words.next().unwrap_or(""), words.next(), words.next());
    let mut windows = app::windows().unwrap_or_default();
    let at = |i: Option<&str>| i.and_then(|s| s.parse::<usize>().ok()).unwrap_or(0);
    match what {
        "page" => match pages {
            Some(p) => {
                nav::go(p, cmd.trim_start_matches("page").trim());
                "ok".into()
            }
            None => "a dialog or popover is open: close it first".into(),
        },
        "windows" => windows
            .iter()
            .enumerate()
            .map(|(i, w)| {
                format!(
                    "{i}: '{}' at {},{} {}x{} {}",
                    w.label(),
                    w.x(),
                    w.y(),
                    w.w(),
                    w.h(),
                    if w.visible() { "shown" } else { "hidden" }
                )
            })
            .collect::<Vec<_>>()
            .join("\n"),
        "shot" => {
            let (Some(path), Some(w)) = (a, windows.get_mut(at(b))) else {
                return "usage: shot PATH [N]".into();
            };
            match capture(w) {
                Some(img) => match png(&img.to_rgb_data(), img.w() as u32, img.h() as u32, img.depth() as usize) {
                    Ok(bytes) => std::fs::write(path, bytes).map_or_else(
                        |e| e.to_string(),
                        |()| format!("saved {path} ({}x{})", img.w(), img.h()),
                    ),
                    Err(e) => e,
                },
                None => "can't capture".into(),
            }
        }
        "tree" => {
            let Some(w) = windows.get(at(a)) else {
                return "no such window".into();
            };
            let mut out = String::new();
            walk(&w.as_base_widget().as_group(), 0, &mut out);
            out
        }
        "find" => {
            let Some(text) = a else {
                return "usage: find TEXT [N]".into();
            };
            let Some(w) = windows.get(at(b)) else {
                return "no such window".into();
            };
            let mut out = String::new();
            find(
                &w.as_base_widget().as_group(),
                &text.to_lowercase(),
                (w.x(), w.y()),
                &mut out,
            );
            out
        }
        "chat" => super::chat::SHOWN
            .load(std::sync::atomic::Ordering::Relaxed)
            .to_string(),
        "community" => super::lists::USERS_SHOWN
            .load(std::sync::atomic::Ordering::Relaxed)
            .to_string(),
        "panic" => panic!("dev panic"),
        _ => "commands: shot, windows, tree, find, page, chat, community, panic".into(),
    }
}

/// The window as drawn, with the windows inside it (popovers). Not `draw::capture_window`: it reads the
/// screen and crashes on windows inside windows.
fn capture(win: &impl WindowExt) -> Option<fltk::image::RgbImage> {
    use fltk::surface::ImageSurface;
    let surface = ImageSurface::new(win.w(), win.h(), false);
    ImageSurface::push_current(&surface);
    draw::set_draw_color(fltk::enums::Color::Background);
    draw::draw_rectf(0, 0, win.w(), win.h());
    surface.draw(win, 0, 0);
    fn inside(g: &Option<fltk::group::Group>, at: (i32, i32), surface: &ImageSurface) {
        let Some(g) = g else { return };
        for i in 0..g.children() {
            let Some(c) = g.child(i) else { continue };
            if c.as_window().is_some() {
                c.clone().redraw();
                surface.draw(&c, at.0 + c.x(), at.1 + c.y());
            }
            inside(&c.as_group(), at, surface);
        }
    }
    inside(&win.as_base_widget().as_group(), (0, 0), &surface);
    ImageSurface::pop_current();
    surface.image()
}

fn walk(g: &Option<fltk::group::Group>, depth: usize, out: &mut String) {
    let Some(g) = g else { return };
    for i in 0..g.children() {
        let Some(c) = g.child(i) else { continue };
        let _ = writeln!(
            out,
            "{}{:?} {},{} {}x{}{}",
            "  ".repeat(depth),
            c.label(),
            c.x(),
            c.y(),
            c.w(),
            c.h(),
            if c.visible() { "" } else { " (hidden)" }
        );
        walk(&c.as_group(), depth + 1, out);
    }
}

fn find(g: &Option<fltk::group::Group>, text: &str, at: (i32, i32), out: &mut String) {
    let Some(g) = g else { return };
    for i in 0..g.children() {
        let Some(c) = g.child(i) else { continue };
        if c.visible() && c.label().to_lowercase().contains(text) {
            let _ = writeln!(
                out,
                "{:?} at {},{}",
                c.label(),
                at.0 + c.x() + c.w() / 2,
                at.1 + c.y() + c.h() / 2
            );
        }
        find(&c.as_group(), text, at, out);
    }
}

/// A PNG of `rgb` pixels (`depth` bytes each): no compression to speak of, and no library.
fn png(rgb: &[u8], w: u32, h: u32, depth: usize) -> Result<Vec<u8>, String> {
    use flate2::write::ZlibEncoder;
    use std::io::Write as _;
    if !(3..=4).contains(&depth) || rgb.len() < w as usize * h as usize * depth {
        return Err("odd picture".into());
    }
    let mut raw = Vec::with_capacity((w as usize * 3 + 1) * h as usize);
    for row in rgb.chunks(w as usize * depth).take(h as usize) {
        raw.push(0);
        for px in row.chunks(depth) {
            raw.extend_from_slice(&px[..3]);
        }
    }
    let mut z = ZlibEncoder::new(Vec::new(), flate2::Compression::fast());
    z.write_all(&raw).map_err(|e| e.to_string())?;
    let idat = z.finish().map_err(|e| e.to_string())?;
    let mut out = b"\x89PNG\r\n\x1a\n".to_vec();
    let mut chunk = |kind: &[u8; 4], data: &[u8]| {
        out.extend_from_slice(&(data.len() as u32).to_be_bytes());
        out.extend_from_slice(kind);
        out.extend_from_slice(data);
        let mut crc = flate2::Crc::new();
        crc.update(kind);
        crc.update(data);
        out.extend_from_slice(&crc.sum().to_be_bytes());
    };
    let mut head = Vec::new();
    head.extend_from_slice(&w.to_be_bytes());
    head.extend_from_slice(&h.to_be_bytes());
    head.extend_from_slice(&[8, 2, 0, 0, 0]);
    chunk(b"IHDR", &head);
    chunk(b"IDAT", &idat);
    chunk(b"IEND", &[]);
    Ok(out)
}

#[cfg(test)]
mod tests {
    #[test]
    fn a_png_has_its_parts() {
        let p = super::png(&[255, 0, 0, 0, 255, 0], 2, 1, 3).unwrap();
        assert_eq!(&p[..8], b"\x89PNG\r\n\x1a\n");
        assert!(p.windows(4).any(|w| w == b"IHDR") && p.ends_with(&[0xAE, 0x42, 0x60, 0x82]));
        assert!(super::png(&[0], 2, 1, 3).is_err());
    }
}
