//! A chatter's card, a popover: who they are (picture, name, roles), what their platform says (account
//! age, following, subscription), what Streamerz has seen (messages, first and last time, time chatting)
//! and what they said. The technical values (as the actions get them) are one button away.

use super::popover::Popover;
use super::rows::Rows;
use super::{button, img, query, tell, Ellipsis};
use crate::engine::chatlog::{Line, Said};
use crate::engine::UiCmd;
use crate::i18n::{tr, trf};
use crate::model::Obj;
use fltk::enums::{Align, Color, Font};
use fltk::frame::Frame;
use fltk::group::Flex;
use fltk::prelude::*;
use fltk::{app, draw};
use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::{Arc, Mutex};
use tokio::sync::mpsc::UnboundedSender;

type Cmds = UnboundedSender<UiCmd>;
type Facts = Vec<(String, String)>;

/// Who a card is about.
pub struct Who {
    pub platform: String,
    pub user_id: String,
    pub name: String,
    pub color: Color,
    /// Already in the user's words (broadcaster, moderator...).
    pub roles: Vec<&'static str>,
    /// The badges they wear: key and picture.
    pub badges: Vec<(String, String)>,
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
            badges: l.badges.iter().filter(|b| !b.1.is_empty()).cloned().collect(),
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
            badges: Vec::new(),
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

/// What they said: a line each, time then words with the emotes as pictures; scrolls, starting at the end.
fn said_list(cmds: &Cmds, said: Vec<Said>) -> Frame {
    const LINE: i32 = 28;
    let mut f = Frame::default();
    let top = Rc::new(Cell::new(i32::MAX));
    let (said, cmds) = (Rc::new(said), cmds.clone());
    f.draw({
        let (said, top) = (said.clone(), top.clone());
        move |f| {
            let (x, y, w, h) = (f.x(), f.y(), f.w(), f.h());
            draw::set_draw_color(Color::Background2);
            draw::draw_rounded_rectf(x, y, w, h, 6);
            draw::set_draw_color(Color::color_average(Color::Foreground, Color::Background, 0.2));
            draw::draw_rounded_rect(x, y, w, h, 6);
            if said.is_empty() {
                draw::set_font(Font::Helvetica, 14);
                draw::set_draw_color(super::muted());
                draw::draw_text2(tr("card.no_messages"), x, y, w, h, Align::Center);
                return;
            }
            let max = (said.len() as i32 * LINE + 8 - h).max(0);
            top.set(top.get().clamp(0, max));
            draw::push_clip(x + 2, y + 2, w - 4, h - 4);
            draw::set_font(Font::Helvetica, 14);
            for (i, m) in said.iter().enumerate() {
                let ly = y + 4 + i as i32 * LINE - top.get();
                if ly + LINE < y || ly > y + h {
                    continue;
                }
                let base = ly + (LINE + draw::height()) / 2 - draw::descent();
                draw::set_draw_color(super::muted());
                draw::draw_text(&m.time, x + 12, base);
                let mut cx = x + 12 + 80;
                for word in m.text.split(' ') {
                    let picture = m
                        .emotes
                        .iter()
                        .find(|e| e.0 == word)
                        .and_then(|e| img::get(&cmds, &e.1, 20, false));
                    match picture {
                        Some(p) => {
                            let mut i = p.first();
                            i.draw(cx, ly + (LINE - i.h()) / 2, i.w(), i.h());
                            cx += i.w() + 4;
                        }
                        None => {
                            draw::set_draw_color(Color::Foreground);
                            draw::draw_text(word, cx, base);
                            cx += draw::width(word) as i32 + draw::width(" ") as i32;
                        }
                    }
                    if cx > x + w {
                        break;
                    }
                }
            }
            draw::pop_clip();
        }
    });
    f.handle({
        let (said, top) = (said, top);
        move |f, ev| {
            if ev == fltk::enums::Event::MouseWheel {
                top.set((top.get().min(said.len() as i32 * LINE) + app::event_dy_value() * LINE).max(0));
                f.redraw();
                return true;
            }
            ev == fltk::enums::Event::Enter
        }
    });
    f
}

/// Opens the card near `at` (a point in the window).
pub fn open(cmds: &Cmds, who: Who, at: (i32, i32)) {
    let Some(host) = app::first_window() else { return };
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
    let said = query(cmds, {
        let (p, id) = (p.clone(), id.clone());
        move |e| e.chatter_lines(&p, &id)
    })
    .unwrap_or_default();
    let member = query(cmds, {
        let (p, id) = (p.clone(), id.clone());
        move |e| e.community_user(&p, &id)
    })
    .flatten();

    let mut pop = Popover::start(Box::new(host), at.0 + 12, at.1 - 24, 400, 560);
    let (pw, ph) = pop.size();
    let mut col = Flex::new(0, 0, pw, ph, None).column();
    col.set_margin(14);
    col.set_pad(8);

    // The channel's banner, when its platform has one (it arrives with the platform's answer).
    let mut banner = Frame::default();
    let banner_pic: Rc<RefCell<Option<fltk::image::RgbImage>>> = Rc::default();
    banner.draw({
        let pic = banner_pic.clone();
        move |f| {
            if let Some(i) = pic.borrow_mut().as_mut() {
                draw::push_clip(f.x(), f.y(), f.w(), f.h());
                i.draw(f.x() + (f.w() - i.w()) / 2, f.y() + (f.h() - i.h()) / 2, i.w(), i.h());
                draw::pop_clip();
            }
        }
    });
    // 1, not 0: a size of 0 means "share what is left".
    col.fixed(&banner, 1);

    let mut head = Flex::default().row();
    head.set_pad(12);
    let mut pic = Frame::default();
    // Until the picture is there (or when there is none): a circle with their initial.
    let initial: String = who
        .name
        .chars()
        .next()
        .map(|c| c.to_uppercase().collect())
        .unwrap_or_default();
    let tint = who.color;
    pic.draw(move |f| {
        if f.image().is_none() {
            draw::set_draw_color(Color::color_average(tint, Color::Background, 0.25));
            draw::draw_pie(f.x(), f.y(), 56, 56, 0.0, 360.0);
            draw::set_font(Font::HelveticaBold, 24);
            draw::set_draw_color(tint);
            draw::draw_text2(&initial, f.x(), f.y(), 56, 56, Align::Center);
        }
    });
    head.fixed(&pic, 60);
    let mut who_col = Flex::default().column();
    who_col.set_pad(0);
    let mut name = Frame::default()
        .with_label(&format!("  {}", who.name.replace('@', "@@")))
        .with_align(Align::Left | Align::Inside | Align::ImageNextToText)
        .ellipsis();
    name.set_label_font(Font::HelveticaBold);
    name.set_label_size(18);
    name.set_label_color(who.color);
    name.set_image(img::logo(platform_name, 18));
    let roles = if who.roles.is_empty() {
        tr("chat.card_viewer").to_string()
    } else {
        who.roles.join(", ")
    };
    let mut role_line = Frame::default()
        .with_label(&roles)
        .with_align(Align::Left | Align::Inside)
        .ellipsis();
    role_line.set_label_color(super::muted());
    who_col.fixed(&name, 26);
    who_col.fixed(&role_line, 18);
    // Their badges, as pictures.
    let mut badge_row = Frame::default();
    badge_row.draw({
        let (badges, cmds) = (who.badges.clone(), cmds.clone());
        move |f| {
            let mut x = f.x();
            for (_, url) in &badges {
                if let Some(p) = img::get(&cmds, url, 18, false) {
                    let mut i = p.first();
                    i.draw(x, f.y() + (f.h() - i.h()) / 2, i.w(), i.h());
                    x += i.w() + 4;
                }
            }
        }
    });
    who_col.end();
    head.end();
    col.fixed(&head, 60);

    // What's known about them: label, value.
    let mut facts = Rows::new("");
    facts.set_column_widths(&[160]);
    col.fixed(&*facts, 8 * 28);

    let mut title = Frame::default()
        .with_label(tr("card.recent"))
        .with_align(Align::Left | Align::Inside)
        .ellipsis();
    title.set_label_font(Font::HelveticaBold);
    title.set_label_size(13);
    col.fixed(&title, 20);
    let messages = said_list(cmds, said);
    let mut details = Rows::new("");
    details.set_column_widths(&[170]);
    details.hide();

    let mut bottom = Flex::default().row();
    bottom.set_pad(8);
    Frame::default();
    let mut open_profile = button(&mut bottom, tr("btn.open_profile"), 120);
    let mut more = button(&mut bottom, tr("card.details"), 90);
    let mut close = button(&mut bottom, tr("dialogs.close"), 80);
    bottom.end();
    col.fixed(&bottom, 28);
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
    let showing_details = Rc::new(Cell::new(false));
    more.set_callback({
        let (mut messages, mut details, mut title, col, mut more) = (
            messages.clone(),
            details.clone(),
            title.clone(),
            col.clone(),
            more.clone(),
        );
        let on = showing_details.clone();
        let mut pw = pop.window();
        move |_| {
            on.set(!on.get());
            if on.get() {
                messages.hide();
                details.show();
                title.set_label(tr("card.details"));
                more.set_label(tr("card.recent_short"));
            } else {
                details.hide();
                messages.show();
                title.set_label(tr("card.recent"));
                more.set_label(tr("card.details"));
            }
            col.layout();
            pw.redraw();
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
    let answer: Arc<Mutex<Option<Facts>>> = Arc::default();
    let asks = matches!(who.platform.as_str(), "twitch" | "kick");
    if asks {
        let (p, id, answer) = (who.platform.clone(), who.user_id.clone(), answer.clone());
        tell(cmds, move |e| {
            let e = e.clone();
            tokio::task::spawn_local(async move {
                let r = e.chatter_lookup(&p, &id).await;
                if let Ok(mut a) = answer.lock() {
                    *a = Some(r);
                }
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
        row(tr("card.created"), &when(known("Account created")));
        row(tr("card.following"), &when(known("Following since")));
        row(tr("card.subscribed"), known("Subscribed"));
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
    let mut win = pop.window();
    let mut fit = |col: &mut Flex, facts: &Rows| {
        col.fixed(&**facts, facts.size().max(1) * 28 + 8);
        col.layout();
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

    fit(&mut col, &facts);
    let (mut pic_frame, mut facts_t, mut tries, mut col_t) = (pic.clone(), facts.clone(), 0, col.clone());
    let mut waiting = asks;
    let (mut have_pic, mut banner_url) = (false, String::new());
    let banner_t = banner.clone();
    pop.run(|| {
        tries += 1;
        if waiting {
            if let Some(found) = answer.lock().ok().and_then(|mut a| a.take()) {
                waiting = false;
                if let Some((_, v)) = found.iter().find(|f| f.0 == "Picture") {
                    picture.clone_from(v);
                    remember_pic(&pic_key, v);
                }
                if let Some((_, v)) = found.iter().find(|f| f.0 == "Banner") {
                    banner_url.clone_from(v);
                    col_t.fixed(&banner_t, 72);
                }
                if let Some((_, v)) = found.iter().find(|f| f.0 == "Channel") {
                    *url.borrow_mut() = profile_url(&who.platform, v, &who.user_id);
                }
                fill(&mut facts_t, &found);
                if found.is_empty() {
                    facts_t.add(&format!(
                        "{}\t{}",
                        trf("chat.card_platform_says", &[("platform", platform_name)]),
                        tr("card.nothing")
                    ));
                }
                fit(&mut col_t, &facts_t);
            } else if tries > 75 {
                waiting = false;
            }
        }
        if !banner_url.is_empty() && banner_pic.borrow().is_none() {
            if let Some(p) = img::get(cmds, &banner_url, 210, false) {
                *banner_pic.borrow_mut() = Some(p.first());
                banner_t.clone().redraw();
            }
        }
        if !have_pic && !picture.is_empty() {
            if let Some(p) = img::get(cmds, &picture, 56, true) {
                pic_frame.set_image(Some(p.first()));
                pic_frame.redraw();
                have_pic = true;
            }
        }
    });
    pop.close();
}
