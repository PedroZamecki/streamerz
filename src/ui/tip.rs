//! What pointing at an emote or a badge says: its picture big, its name, where it comes from (the
//! source's own icon, no words) and what else is known. A small panel in the window, near the pointer,
//! made when it's shown and taken away when the pointer moves off.

use super::chat::icon_sized;
use super::{icons, img, query};
use crate::engine::UiCmd;
use fltk::enums::{Align, Color, Font, FrameType};
use fltk::image::SvgImage;
use fltk::prelude::*;
use fltk::window::Window;
use fltk::{app, draw};
use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;
use tokio::sync::mpsc::UnboundedSender;

type Cmds = UnboundedSender<UiCmd>;

/// What a tip is about.
#[derive(Clone, PartialEq)]
pub(super) enum Tip {
    Emote { platform: String, name: String, url: String },
    Badge { platform: String, key: String, url: String },
}

/// The picture's side.
const PIC: i32 = 56;
const PAD: i32 = 10;
const ROW: i32 = 18;

/// Where an emote's picture lives tells where it is from: the icon to show for it.
fn source(platform: &str, url: &str) -> Option<&'static str> {
    const HOSTS: [(&str, &str); 7] = [
        ("7tv.app", "ui:7tv"),
        ("betterttv.net", "ui:bttv"),
        ("frankerfacez.com", "ui:ffz"),
        ("jtvnw.net", "twitch"),
        ("kick.com", "kick"),
        ("ggpht.com", "youtube"),
        ("twemoji", "smile"),
    ];
    HOSTS.iter().find(|h| url.contains(h.0)).map(|h| h.1).or(match platform {
        "twitch" => Some("twitch"),
        "kick" => Some("kick"),
        "youtube" => Some("youtube"),
        "tiktok" => Some("tiktok"),
        _ => None,
    })
}

/// The emote's id in its picture's address, when it has one (".../emote/ID/2x.webp").
fn emote_id(url: &str) -> Option<String> {
    let path = url.split("//").nth(1)?.split_once('/')?.1;
    let parts: Vec<&str> = path.split(['/', '?']).collect();
    let at = parts.iter().rposition(|p| matches!(*p, "emote" | "emotes" | "emoticons"))?;
    let id = parts.get(at + 1).filter(|id| **id != "v2").or(parts.get(at + 2))?;
    Some((*id).to_string())
}

/// An icon key as an image: Lucide ones in the muted ink, the rest (logos) as they are.
fn picture(key: &str, size: i32) -> Option<SvgImage> {
    if matches!(key, "smile" | "hash" | "tag" | "play") {
        icons::icon(key, super::muted(), size)
    } else {
        icon_sized(key, size)
    }
}

/// What the panel holds.
struct Content {
    pic: Option<fltk::image::RgbImage>,
    title: String,
    source: Option<SvgImage>,
    rows: Vec<(SvgImage, String)>,
}

impl Content {
    fn size(&self) -> (i32, i32) {
        draw::set_font(Font::HelveticaBold, 14);
        let title = draw::width(&self.title) as i32 + self.source.as_ref().map_or(0, |_| 22);
        draw::set_font(Font::Helvetica, 12);
        let rows = self.rows.iter().map(|r| draw::width(&r.1) as i32 + 22).max().unwrap_or(0);
        let side = if self.pic.is_some() { PIC + PAD } else { 0 };
        let text_h = 22 + ROW * self.rows.len() as i32;
        let w = (PAD * 2 + side + title.max(rows)).clamp(120, 360);
        let h = PAD * 2 + text_h.max(if self.pic.is_some() { PIC } else { 0 });
        (w, h)
    }

    fn draw(&self, w: i32) {
        draw::set_draw_color(Color::color_average(Color::Foreground, Color::Background, 0.25));
        draw::draw_rounded_rect(0, 0, w, self.size().1, 6);
        let mut x = PAD;
        if let Some(p) = &self.pic {
            let mut p = p.clone();
            p.draw(x + (PIC - p.w()) / 2, PAD + (PIC - p.h()) / 2, p.w(), p.h());
            x += PIC + PAD;
        }
        draw::set_font(Font::HelveticaBold, 14);
        draw::set_draw_color(Color::Foreground);
        let tw = draw::width(&self.title) as i32;
        let room = w - x - PAD - self.source.as_ref().map_or(0, |_| 22);
        draw::draw_text2(&self.title, x, PAD, tw.min(room), 18, Align::Left);
        if let Some(s) = &self.source {
            s.clone().draw(x + tw.min(room) + 6, PAD + 1, 16, 16);
        }
        draw::set_font(Font::Helvetica, 12);
        draw::set_draw_color(super::muted());
        let mut y = PAD + 22;
        for (icon, text) in &self.rows {
            icon.clone().draw(x, y + 2, 14, 14);
            draw::draw_text2(text, x + 20, y, w - x - 20 - PAD, ROW, Align::Left);
            y += ROW;
        }
    }
}

thread_local! {
    static SHOWING: RefCell<Option<(Tip, Window)>> = const { RefCell::new(None) };
    /// Badge names the engine gave, by (platform, key).
    static NAMES: RefCell<HashMap<(String, String), String>> = RefCell::default();
}

fn content(cmds: &Cmds, tip: &Tip) -> Content {
    let ink = super::muted();
    let row = |icon: &str| icons::icon(icon, ink, 14);
    match tip {
        Tip::Emote { platform, name, url } => {
            let src = source(platform, url);
            let pic = img::get(cmds, url, PIC, false).or_else(|| img::get(cmds, url, 28, false));
            let mut rows = Vec::new();
            if let Some(id) = emote_id(url) {
                rows.extend(row("hash").map(|i| (i, id)));
            }
            if pic.as_ref().is_some_and(img::Pic::animated) {
                rows.extend(row("play").map(|i| (i, crate::i18n::tr("tip.animated").to_string())));
            }
            Content {
                pic: pic.map(|p| p.first()),
                title: name.clone(),
                source: src.and_then(|k| picture(k, 16)),
                rows,
            }
        }
        Tip::Badge { platform, key, url } => {
            let k = (platform.clone(), key.clone());
            let title = NAMES.with_borrow(|n| n.get(&k).cloned()).unwrap_or_else(|| {
                let (p, key) = (platform.clone(), key.clone());
                let t = query(cmds, move |e| e.badge_label(&p, &key)).unwrap_or_default();
                NAMES.with_borrow_mut(|n| n.insert(k, t.clone()));
                t
            });
            let pic = img::get(cmds, url, PIC / 2, false).map(|p| p.first());
            Content {
                pic,
                title,
                source: picture(platform, 16),
                rows: row("tag").map(|i| (i, key.clone())).into_iter().collect(),
            }
        }
    }
}

thread_local! {
    /// The app's window the panel sits in, got once (a window from `window()` is the app's own: never dropped).
    static HOST: RefCell<Option<std::mem::ManuallyDrop<Box<dyn WindowExt>>>> = const { RefCell::new(None) };
    /// Counts pointer moves onto a new target, so a late timer knows it's stale.
    static ASKED: Cell<u64> = const { Cell::new(0) };
}

/// How long the pointer rests on something before its panel shows.
const WAIT: f64 = 0.3;

/// The pointer is over `tip` (None: over nothing) at `(x, y)` in the window `from` is in: shows its panel
/// after a moment, or takes it away.
pub(super) fn point(cmds: &Cmds, from: &impl WidgetExt, tip: Option<Rc<Tip>>) {
    let Some(tip) = tip else {
        ASKED.set(ASKED.get() + 1);
        hide();
        return;
    };
    if SHOWING.with_borrow(|s| s.as_ref().is_some_and(|s| s.0 == *tip)) {
        return;
    }
    // Where the pointer is in the app's own window, whatever popover the pane is in.
    let (mut x, mut y) = (app::event_x(), app::event_y());
    let mut cur = from.window();
    let mut top = None;
    while let Some(win) = cur {
        if win.window().is_none() {
            top = Some(win);
            break;
        }
        x += win.x();
        y += win.y();
        cur = win.window();
    }
    let Some(top) = top else { return };
    HOST.with_borrow_mut(|h| {
        if h.is_none() {
            *h = Some(std::mem::ManuallyDrop::new(top));
        }
    });
    hide();
    let id = ASKED.get() + 1;
    ASKED.set(id);
    let cmds = cmds.clone();
    app::add_timeout3(WAIT, move |_| {
        if ASKED.get() == id {
            show(&cmds, &tip, x, y);
        }
    });
}

fn show(cmds: &Cmds, tip: &Tip, x: i32, y: i32) {
    let Some((hw, hh)) = HOST.with_borrow(|h| h.as_ref().map(|h| (h.w(), h.h()))) else {
        return;
    };
    let c = Rc::new(content(cmds, tip));
    let (w, h) = c.size();
    let (x, y) = (
        (x + 14).min(hw - w - 4).max(4),
        if y + 22 + h > hh { y - h - 8 } else { y + 22 }.max(4),
    );
    let win = HOST.with_borrow(|hs| {
        let hs = hs.as_ref()?;
        hs.begin();
        let mut win = Window::new(x, y, w, h, None);
        win.set_frame(FrameType::FlatBox);
        win.set_color(Color::Background);
        win.draw({
            let c = c.clone();
            move |win| c.draw(win.w())
        });
        win.end();
        hs.end();
        Some(win)
    });
    if let Some(mut win) = win {
        win.show();
        SHOWING.with_borrow_mut(|s| *s = Some((tip.clone(), win)));
        app::redraw();
    }
}

/// Takes the panel away, if it's up.
pub(super) fn hide() {
    if let Some((_, win)) = SHOWING.with_borrow_mut(Option::take) {
        app::delete_widget(win);
        HOST.with_borrow_mut(|h| {
            if let Some(h) = h.as_mut() {
                h.redraw();
            }
        });
    }
}
