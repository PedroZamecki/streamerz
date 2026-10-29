//! A chatter's card, a popover: their banner, picture, name and account age on top, as the platforms'
//! own cards show them; the badges they wear (named when pointed at); what their platform says
//! (following, subscription) and what Streamerz has seen; and their messages, live, in the chat's own
//! view (no box to write in), so they can be deleted, timed out or banned from here. The technical values (as
//! the actions get them) are one button away.

use super::chat::{draw_badge, Pane};
use super::popover::Popover;
use super::rows::Rows;
use super::{icons, img, query, tell, Ellipsis};
use crate::engine::chatlog::Line;
use crate::engine::UiCmd;
use crate::i18n::{tr, trf};
use crate::model::Obj;
use fltk::button::Button;
use fltk::enums::{Align, Color, Event, Font, FrameType};
use fltk::frame::Frame;
use fltk::group::{Flex, Group};
use fltk::image::RgbImage;
use fltk::prelude::*;
use fltk::{app, draw};
use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::{Arc, Mutex};
use tokio::sync::mpsc::UnboundedSender;

type Cmds = UnboundedSender<UiCmd>;
type Facts = Vec<(String, String)>;

/// The card's header: banner, picture and name.
const HEAD: i32 = 104;
/// Badges' height in their row.
const BADGE: i32 = 22;

/// Who a card is about.
pub struct Who {
    pub platform: String,
    pub user_id: String,
    pub name: String,
    pub color: Color,
    /// Already in the user's words (broadcaster, moderator...): shown when there are no badges.
    pub roles: Vec<&'static str>,
}

impl Who {
    pub fn from_line(l: &Line, color: Color) -> Who {
        let roles = [
            (l.broadcaster, tr("chat.card_broadcaster")),
            (l.moderator, tr("chat.card_moderator")),
            (l.vip, tr("chat.card_vip")),
            (l.subscriber, tr("chat.card_subscriber")),
        ];
        Who {
            platform: l.platform.clone(),
            user_id: l.user_id.clone(),
            name: l.user.clone(),
            color,
            roles: roles.iter().filter(|r| r.0).map(|r| r.1).collect(),
        }
    }

    /// A user of the Community tab.
    pub fn from_member(u: &Obj) -> Who {
        let s = |k: &str| u.get(k).and_then(|v| v.as_str()).unwrap_or("").to_string();
        let roles = match u.get("role").and_then(|v| v.as_u64()).unwrap_or(1) {
            4 => vec![tr("chat.card_broadcaster")],
            3 => vec![tr("chat.card_moderator")],
            2 => vec![tr("chat.card_vip")],
            _ => Vec::new(),
        };
        // The colour they chat in, as last seen ("#RRGGBB").
        let hex = s("color");
        let color = u32::from_str_radix(hex.trim_start_matches('#'), 16)
            .ok()
            .filter(|_| hex.trim_start_matches('#').len() == 6)
            .map_or(Color::Foreground, |v| {
                Color::from_rgb((v >> 16) as u8, (v >> 8) as u8, v as u8)
            });
        Who {
            platform: s("type"),
            user_id: s("id"),
            name: s("display"),
            color,
            roles,
        }
    }
}

/// Profile picture addresses of chatters seen, kept across runs so a card has the face at once.
static PICS: Mutex<Option<HashMap<String, String>>> = Mutex::new(None);

fn pic_url(key: &str) -> String {
    let Ok(mut p) = PICS.lock() else { return String::new() };
    let map = p.get_or_insert_with(|| {
        let saved = img::saved("chatter-pics").map(|v| v.1).unwrap_or_default();
        serde_json::from_value(saved).unwrap_or_default()
    });
    map.get(key).cloned().unwrap_or_default()
}

fn remember_pic(key: &str, url: &str) {
    let Ok(mut p) = PICS.lock() else { return };
    let map = p.get_or_insert_with(HashMap::new);
    if map.get(key).is_none_or(|u| u != url) {
        if map.len() >= 400 {
            map.clear();
        }
        map.insert(key.to_string(), url.to_string());
        img::save("chatter-pics", &serde_json::json!(map));
    }
}

/// "3 h 20 min", "45 min", "30 s".
fn duration(secs: u64) -> String {
    match secs {
        0..=59 => format!("{secs} s"),
        60..=3599 => format!("{} min", secs / 60),
        _ => format!("{} h {} min", secs / 3600, secs % 3600 / 60),
    }
}

/// An ISO time as a date the user reads, or the text as it is (the platform said something else).
fn when(v: &str) -> String {
    let d = crate::fmt::shown_iso(v, false);
    if d.is_empty() {
        v.to_string()
    } else {
        d
    }
}

/// The platform's page for the chatter.
fn profile_url(platform: &str, login: &str, id: &str) -> String {
    match platform {
        "twitch" => format!("https://twitch.tv/{}", login.to_lowercase()),
        "youtube" => format!("https://youtube.com/channel/{id}"),
        "kick" => format!("https://kick.com/{}", login.to_lowercase()),
        _ => format!("https://tiktok.com/@{login}"),
    }
}

/// `pic` darkened to `k` (0 to 1), so white text reads over it.
fn darker(pic: &RgbImage, k: f32) -> Option<RgbImage> {
    let d = pic.depth() as usize;
    let mut px = pic.to_rgb_data();
    for p in px.chunks_mut(d.max(1)) {
        for c in p.iter_mut().take(3) {
            *c = (f32::from(*c) * k) as u8;
        }
    }
    RgbImage::new(&px, pic.w(), pic.h(), pic.depth()).ok()
}

/// What the header shows, filled in as answers come.
#[derive(Default)]
struct Head {
    banner: Option<RgbImage>,
    picture: Option<RgbImage>,
    /// "Account created on ...", once known.
    created: String,
}

/// The header: the banner across (cut to fit, darkened), the picture and the name over it.
fn header(who: &Who, platform_name: &'static str, head: &Rc<RefCell<Head>>) -> Frame {
    let mut f = Frame::default();
    let initial: String = who
        .name
        .chars()
        .next()
        .map(|c| c.to_uppercase().collect())
        .unwrap_or_default();
    let (name, tint) = (who.name.clone(), who.color);
    let head = head.clone();
    f.draw(move |f| {
        let (x, y, w, h) = (f.x(), f.y(), f.w(), f.h());
        let st = head.borrow();
        draw::push_clip(x, y, w, h);
        match &st.banner {
            Some(b) => {
                let mut b = b.clone();
                b.draw(x + (w - b.w()) / 2, y + (h - b.h()) / 2, b.w(), b.h());
            }
            None => draw::draw_rect_fill(x, y, w, h, Color::color_average(tint, Color::Background2, 0.18)),
        }
        // Picture (or their initial in a circle) and name, along the bottom.
        let (px, py) = (x + 14, y + h - 56 - 12);
        match &st.picture {
            Some(p) => p.clone().draw(px, py, 56, 56),
            None => {
                draw::set_draw_color(Color::color_average(tint, Color::Background, 0.35));
                draw::draw_pie(px, py, 56, 56, 0.0, 360.0);
                draw::set_font(Font::HelveticaBold, 24);
                draw::set_draw_color(tint);
                draw::draw_text2(&initial, px, py, 56, 56, Align::Center);
            }
        }
        let text = if st.banner.is_some() {
            Color::White
        } else {
            Color::Foreground
        };
        let tx = px + 56 + 12;
        if let Some(mut logo) = img::logo(platform_name, 16) {
            logo.draw(tx, py + 8, 16, 16);
        }
        draw::set_font(Font::HelveticaBold, 18);
        draw::set_draw_color(if st.banner.is_some() { Color::White } else { tint });
        draw::draw_text2(&name, tx + 22, py + 4, w - (tx - x) - 60, 24, Align::Left | Align::Clip);
        if !st.created.is_empty() {
            if let Some(mut i) = icons::icon("calendar", text, 14) {
                i.draw(tx, py + 34, 14, 14);
            }
            draw::set_font(Font::Helvetica, 13);
            draw::set_draw_color(text);
            draw::draw_text2(
                &st.created,
                tx + 20,
                py + 31,
                w - (tx - x) - 30,
                20,
                Align::Left | Align::Clip,
            );
        }
        draw::pop_clip();
    });
    f
}

/// A small icon button over the header, at its top right.
fn corner(icon: &str, tip: &str) -> Button {
    let mut b = Button::default().with_size(28, 28);
    b.set_frame(FrameType::NoBox);
    b.set_image(icons::icon(icon, Color::White, 18));
    b.set_tooltip(tip);
    b.clear_visible_focus();
    b
}

/// The badges they wear, as pictures; pointing at one says its name.
fn badge_row(cmds: &Cmds, platform: &str, badges: Rc<Vec<(String, String, String)>>) -> Frame {
    let mut f = Frame::default();
    let at: Rc<RefCell<Vec<(i32, i32)>>> = Rc::default();
    f.draw({
        let (cmds, platform, badges, at) = (cmds.clone(), platform.to_string(), badges.clone(), at.clone());
        move |f| {
            let mut x = f.x();
            let mut spans = Vec::new();
            for (key, url, _) in badges.iter() {
                let w = draw_badge(&cmds, &platform, key, url, x, f.y() + (f.h() - BADGE) / 2, BADGE);
                spans.push((x, w));
                if w > 0 {
                    x += w + 6;
                }
            }
            *at.borrow_mut() = spans;
        }
    });
    f.handle(move |f, ev| match ev {
        Event::Enter => true,
        Event::Move => {
            let ex = app::event_x();
            let i = at.borrow().iter().position(|(x, w)| (*x..x + w).contains(&ex));
            f.set_tooltip(i.and_then(|i| badges.get(i)).map_or("", |b| b.2.as_str()));
            true
        }
        _ => false,
    });
    f
}

/// Opens the card near `at` (a point in the window).
pub fn open(cmds: &Cmds, who: Who, at: (i32, i32)) {
    let Some(host) = app::first_window() else { return };
    crate::crash::step(&format!("user card ({})", who.platform));
    let platform_name = match who.platform.as_str() {
        "twitch" => "Twitch",
        "youtube" => "YouTube",
        "kick" => "Kick",
        _ => "TikTok",
    };
    let (p, id) = (who.platform.clone(), who.user_id.clone());
    let seen = query(cmds, {
        let (p, id) = (p.clone(), id.clone());
        move |e| e.chatter(&p, &id)
    })
    .flatten()
    .unwrap_or_default();
    let member = query(cmds, {
        let (p, id) = (p.clone(), id.clone());
        move |e| e.community_user(&p, &id)
    })
    .flatten();
    let badges = Rc::new(
        query(cmds, {
            let (p, id) = (p.clone(), id.clone());
            move |e| e.chatter_badges(&p, &id)
        })
        .unwrap_or_default(),
    );

    let mut pop = Popover::start(Box::new(host), at.0 + 12, at.1 - 24, 420, 640);
    let (pw, ph) = pop.size();
    let mut col = Flex::new(0, 0, pw, ph, None).column();
    col.set_margin(1);
    col.set_pad(0);

    // Header, with its buttons in the corner.
    let head = Rc::new(RefCell::new(Head::default()));
    let top = Group::default().with_size(pw - 2, HEAD);
    let banner = header(&who, platform_name, &head);
    let mut banner = banner;
    banner.resize(top.x(), top.y(), top.w(), top.h());
    let mut open_profile = corner("external-link", tr("btn.open_profile"));
    let mut close = corner("x", tr("dialogs.close"));
    open_profile.set_pos(top.x() + top.w() - 64, top.y() + 6);
    close.set_pos(top.x() + top.w() - 34, top.y() + 6);
    top.resizable(&banner);
    top.end();
    col.fixed(&top, HEAD);

    let mut body = Flex::default().column();
    body.set_margin(12);
    body.set_pad(8);
    // Badges; without any, their roles in words.
    let mut badges_f = badge_row(cmds, &who.platform, badges.clone());
    let roles = if who.roles.is_empty() {
        tr("chat.card_viewer").to_string()
    } else {
        who.roles.join(", ")
    };
    if badges.is_empty() {
        badges_f.set_label(&roles);
        badges_f.set_align(Align::Left | Align::Inside);
        badges_f.set_label_color(super::muted());
    }
    body.fixed(&badges_f, BADGE + 4);

    // What's known about them: label, value.
    let mut facts = Rows::new("");
    facts.set_column_widths(&[160]);
    body.fixed(&*facts, 6 * 28);

    let mut title_row = Flex::default().row();
    let mut title = Frame::default()
        .with_label(tr("card.recent"))
        .with_align(Align::Left | Align::Inside)
        .ellipsis();
    title.set_label_font(Font::HelveticaBold);
    title.set_label_size(13);
    let mut more = Button::default().with_label(tr("card.details"));
    super::with_icon(&mut more);
    title_row.fixed(&more, 90);
    title_row.end();
    body.fixed(&title_row, 24);

    // Their messages, in the chat's view: hover one to delete, time out or ban.
    let (pane_group, pane) = Pane::new(cmds);
    pane.no_cards();
    let mut details = Rows::new("");
    details.set_column_widths(&[170]);
    details.hide();

    body.end();
    col.end();
    pop.finish();

    // What the actions get (%userName%...), for the curious.
    let text_of = |v: &serde_json::Value| v.as_str().map_or_else(|| v.to_string(), str::to_string);
    for (k, v) in seen.args.iter().filter(|a| a.0 != "user") {
        details.add(&format!("{k}\t{}", text_of(v).replace(['\n', '\t'], " ")));
    }
    if !seen.args.contains_key("userId") {
        details.add(&format!("userId\t{}", who.user_id));
    }
    details.set_callback(|b| {
        if app::event_clicks() {
            if let Some(t) = b.selected_text() {
                app::copy(t.split_once('\t').map_or(t.as_str(), |x| x.1));
                crate::say(tr("chat.copied").into());
            }
        }
    });

    let login = seen
        .args
        .get("userName")
        .map(text_of)
        .unwrap_or_else(|| who.name.clone());
    let url = Rc::new(RefCell::new(profile_url(&who.platform, &login, &who.user_id)));
    open_profile.set_callback({
        let url = url.clone();
        move |_| crate::oauth::open_browser(&url.borrow())
    });
    let mut win = pop.window();
    let showing_details = Rc::new(Cell::new(false));
    more.set_callback({
        let (mut pane_group, mut details, mut title, body, mut more) = (
            pane_group.clone(),
            details.clone(),
            title.clone(),
            body.clone(),
            more.clone(),
        );
        let on = showing_details.clone();
        let mut win = win.clone();
        move |_| {
            on.set(!on.get());
            if on.get() {
                pane_group.hide();
                details.show();
                title.set_label(tr("card.details"));
                more.set_label(tr("card.recent_short"));
            } else {
                details.hide();
                pane_group.show();
                title.set_label(tr("card.recent"));
                more.set_label(tr("card.details"));
            }
            body.clone().layout();
            win.redraw();
        }
    });
    close.set_callback({
        let done = pop.done.clone();
        move |_| done.set(true)
    });

    // The platform's own answer arrives later.
    let pic_key = format!("{}:{}", who.platform, who.user_id);
    let mut picture = seen
        .args
        .iter()
        .find(|(k, v)| k.contains("rofile") && text_of(v).starts_with("http"))
        .map(|(_, v)| text_of(v))
        .unwrap_or_else(|| pic_url(&pic_key));
    // What the platform said so far, and whether that is all of it.
    let answer: Arc<Mutex<(Facts, bool)>> = Arc::default();
    let asks = matches!(who.platform.as_str(), "twitch" | "kick");
    if asks {
        let (p, id, answer) = (who.platform.clone(), who.user_id.clone(), answer.clone());
        tell(cmds, move |e| {
            let e = e.clone();
            tokio::task::spawn_local(async move {
                let early = answer.clone();
                let r = e
                    .chatter_lookup(&p, &id, &move |part| {
                        if let Ok(mut a) = early.lock() {
                            a.0 = part.to_vec();
                        }
                        crate::ui::wake_card();
                    })
                    .await;
                if let Ok(mut a) = answer.lock() {
                    *a = (r, true);
                }
                crate::ui::wake_card();
            });
        });
    }

    // What there is now; again when the platform answers.
    let fill = |facts: &mut Rows, platform_facts: &Facts| {
        facts.clear();
        let mut row = |k: &str, v: &str| {
            if !v.is_empty() {
                facts.add(&format!("{k}\t{}", v.replace(['\n', '\t'], " ")));
            }
        };
        let known = |k: &str| {
            platform_facts
                .iter()
                .find(|f| f.0 == k)
                .map(|f| f.1.as_str())
                .unwrap_or("")
        };
        row(tr("card.following"), &when(known("Following since")));
        row(tr("card.subscribed"), known("Subscribed"));
        row(
            tr("card.banned"),
            if known("Banned").is_empty() {
                ""
            } else {
                tr("card.banned_yes")
            },
        );
        row(tr("card.category"), known("Their last category"));
        row(tr("card.category"), known("Their category"));
        if let Some(m) = &member {
            let get = |k: &str| m.get(k).and_then(|v| v.as_str()).unwrap_or("");
            let num = |k: &str| m.get(k).and_then(|v| v.as_u64()).unwrap_or(0);
            row(
                tr("card.first_seen"),
                &crate::fmt::shown_iso(get("firstMessage"), false),
            );
            row(tr("card.last_seen"), &crate::fmt::shown_iso(get("lastActive"), false));
            if num("messages") > 0 {
                row(tr("card.messages"), &num("messages").to_string());
            }
            if num("activeSecs") > 0 {
                row(tr("card.chatting"), &duration(num("activeSecs")));
            }
        }
        row(tr("chat.card_messages_session"), &seen.messages.to_string());
    };
    // The table is as tall as its lines.
    let mut fit = |body: &mut Flex, facts: &Rows| {
        body.fixed(&**facts, facts.size().max(1) * 28 + 8);
        body.layout();
        win.redraw();
    };
    fill(&mut facts, &Vec::new());
    if asks {
        facts.add(&format!(
            "{}\t{}",
            trf("chat.card_platform_says", &[("platform", platform_name)]),
            tr("chat.card_looking_up")
        ));
    }
    fit(&mut body, &facts);

    // Their messages, now and as they come (and as they're deleted).
    let (mut gen, mut last) = (u64::MAX, 0u64);
    let mut sync = {
        let (cmds, pane, p, id) = (cmds.clone(), pane.clone(), p.clone(), id.clone());
        move || {
            let (p, id, g, s) = (p.clone(), id.clone(), gen, last);
            let Some((g, full, lines)) = query(&cmds, move |e| e.chatter_sync(&p, &id, g, s)) else {
                return;
            };
            if full {
                pane.replace_all(lines.iter().cloned());
            } else {
                for l in &lines {
                    pane.add(l);
                }
            }
            gen = g;
            last = lines.last().map_or(last, |l| l.seq);
        }
    };
    sync();

    let (mut facts_t, mut body_t) = (facts.clone(), body.clone());
    // Done once the platform said all of it (it always does within 20 s; the rest is for a dead engine).
    let (mut finished, mut applied, started) = (!asks, 0usize, std::time::Instant::now());
    let (mut have_pic, mut banner_url) = (false, String::new());
    let mut top_t = top.clone();
    pop.run(|| {
        sync();
        if !finished {
            let (found, done) = answer.lock().map(|a| a.clone()).unwrap_or_default();
            if found.len() != applied || done {
                applied = found.len();
                if let Some((_, v)) = found.iter().find(|f| f.0 == "Picture") {
                    picture.clone_from(v);
                    remember_pic(&pic_key, v);
                }
                if let Some((_, v)) = found.iter().find(|f| f.0 == "Banner") {
                    banner_url.clone_from(v);
                }
                if let Some((_, v)) = found.iter().find(|f| f.0 == "Channel") {
                    *url.borrow_mut() = profile_url(&who.platform, v, &who.user_id);
                }
                if let Some((_, v)) = found.iter().find(|f| f.0 == "Account created") {
                    head.borrow_mut().created = trf("card.created_on", &[("date", &when(v))]);
                    top_t.redraw();
                }
                fill(&mut facts_t, &found);
                // Still asking: the rest is on its way.
                let (key, text) = match (done, found.is_empty()) {
                    (true, true) => (true, tr("card.nothing")),
                    (false, _) => (true, tr("chat.card_looking_up")),
                    _ => (false, ""),
                };
                if key {
                    facts_t.add(&format!(
                        "{}\t{}",
                        trf("chat.card_platform_says", &[("platform", platform_name)]),
                        text
                    ));
                }
                fit(&mut body_t, &facts_t);
            }
            if done {
                finished = true;
            } else if started.elapsed() > std::time::Duration::from_secs(30) {
                finished = true;
                facts_t.add(&format!(
                    "{}\t{}",
                    trf("chat.card_platform_says", &[("platform", platform_name)]),
                    tr("card.nothing")
                ));
                fit(&mut body_t, &facts_t);
            }
        }
        if !banner_url.is_empty() && head.borrow().banner.is_none() {
            // As wide as the card at least: asked at the header's height first, then taller if narrow.
            if let Some(p) = img::get(cmds, &banner_url, HEAD, false) {
                let first = p.first();
                let tall = if first.w() < top_t.w() {
                    (HEAD * top_t.w() + first.w() - 1) / first.w().max(1)
                } else {
                    HEAD
                };
                let pic = if tall == HEAD {
                    Some(first)
                } else {
                    img::get(cmds, &banner_url, tall, false).map(|p| p.first())
                };
                if let Some(b) = pic.and_then(|b| darker(&b, 0.5)) {
                    head.borrow_mut().banner = Some(b);
                    top_t.redraw();
                }
            }
        }
        if !have_pic && !picture.is_empty() {
            if let Some(p) = img::get(cmds, &picture, 56, true) {
                head.borrow_mut().picture = Some(p.first());
                top_t.redraw();
                have_pic = true;
            }
        }
    });
    pop.close();
}
