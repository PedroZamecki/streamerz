//! The Accounts tab: one row per platform (status, channel, Connect, broadcaster and bot logins),
//! with the rarely needed settings behind Advanced.

use super::lists::{save, title};
use super::{button, dialogs, query, tell, ROW};
use crate::engine::Engine;
use crate::model::Obj;
use crate::Config;
use fltk::button::{Button, CheckButton};
use fltk::dialog;
use fltk::enums::{Align, Color};
use fltk::frame::Frame;
use fltk::group::{Flex, Tabs};
use fltk::input::{Input, SecretInput};
use fltk::prelude::*;
use serde_json::{json, Value};
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use tokio::sync::{mpsc::UnboundedSender, watch};

type Cfg = Arc<watch::Sender<Config>>;
type Cmds = UnboundedSender<crate::engine::UiCmd>;

#[derive(Clone, Copy, PartialEq)]
enum P {
    Twitch,
    YouTube,
    Kick,
    TikTok,
}

const ALL: [P; 4] = [P::Twitch, P::YouTube, P::Kick, P::TikTok];

impl P {
    fn name(self) -> &'static str {
        match self {
            P::Twitch => "Twitch",
            P::YouTube => "YouTube",
            P::Kick => "Kick",
            P::TikTok => "TikTok",
        }
    }

    /// The logged-in broadcaster's or bot's name.
    fn account(self, c: &Config, bot: bool) -> Option<String> {
        match self {
            P::Twitch => if bot { &c.twitch.bot } else { &c.twitch.broadcaster }
                .as_ref()
                .map(|a| {
                    let want = if bot {
                        crate::twitch::BOT_SCOPES
                    } else {
                        crate::twitch::BROADCASTER_SCOPES
                    };
                    if a.token.access_token.is_empty() {
                        format!("{} (login expired: log in again)", a.display_name)
                    // Logged in before streamerz asked for more: some sub-actions would fail.
                    } else if want.iter().any(|w| !a.token.scope.iter().any(|g| g == w)) {
                        format!("{} (log in again: new permissions)", a.display_name)
                    } else {
                        a.display_name.clone()
                    }
                }),
            P::YouTube => if bot { &c.youtube.bot } else { &c.youtube.broadcaster }
                .as_ref()
                .map(|a| a.name.clone()),
            P::Kick => if bot { &c.kick.bot } else { &c.kick.broadcaster }
                .as_ref()
                .map(|a| a.username.clone()),
            P::TikTok => if bot { &c.tiktok.bot } else { &c.tiktok.broadcaster }
                .as_ref()
                .map(|a| a.username.clone()),
        }
    }

    /// Forget an account; its revoke URL and form (Kick and Google need the token in the URL).
    fn logout(self, c: &mut Config, bot: bool) -> Option<String> {
        let e = crate::commands::url_encode;
        let revoke = match self {
            P::Twitch => if bot { &c.twitch.bot } else { &c.twitch.broadcaster }
                .as_ref()
                .map(|a| {
                    format!(
                        "https://id.twitch.tv/oauth2/revoke?client_id={}&token={}",
                        crate::twitch::CLIENT_ID,
                        e(&a.token.access_token)
                    )
                }),
            P::YouTube => if bot { &c.youtube.bot } else { &c.youtube.broadcaster }
                .as_ref()
                .map(|a| {
                    // Revoking the refresh token ends the whole grant.
                    let t = if a.token.refresh_token.is_empty() {
                        &a.token.access_token
                    } else {
                        &a.token.refresh_token
                    };
                    format!("https://oauth2.googleapis.com/revoke?token={}", e(t))
                }),
            P::Kick => if bot { &c.kick.bot } else { &c.kick.broadcaster }.as_ref().map(|a| {
                format!(
                    "https://id.kick.com/oauth/revoke?token={}&token_hint_type=access_token",
                    e(&a.token.access_token)
                )
            }),
            // A cookie: only TikTok's own logout ends the session; forget the window's copy of it.
            P::TikTok => {
                let _ = std::fs::remove_dir_all(crate::tiktok::profile(bot));
                None
            }
        };
        match (self, bot) {
            (P::Twitch, false) => c.twitch.broadcaster = None,
            (P::Twitch, true) => c.twitch.bot = None,
            (P::YouTube, false) => c.youtube.broadcaster = None,
            (P::YouTube, true) => c.youtube.bot = None,
            (P::Kick, false) => c.kick.broadcaster = None,
            (P::Kick, true) => c.kick.bot = None,
            (P::TikTok, false) => c.tiktok.broadcaster = None,
            (P::TikTok, true) => c.tiktok.bot = None,
        }
        revoke
    }

    /// The channel to read; Twitch reads the broadcaster's own.
    fn channel(self, c: &Config) -> &str {
        match self {
            P::Twitch => "",
            P::YouTube => &c.youtube.channel_id,
            P::Kick => &c.kick.channel,
            P::TikTok => &c.tiktok.channel,
        }
    }

    /// Whether the channel read is the broadcaster's own (by id, or by name ignoring case, spaces and @).
    fn own(self, c: &Config) -> bool {
        let norm = |s: &str| s.trim_start_matches('@').replace(' ', "").to_lowercase();
        let ch = norm(self.channel(c));
        let mine = match self {
            P::Twitch => return true,
            P::YouTube => c
                .youtube
                .broadcaster
                .as_ref()
                .map(|b| [norm(&b.channel_id), norm(&b.name)]),
            P::Kick => c.kick.broadcaster.as_ref().map(|b| [norm(&b.username), String::new()]),
            P::TikTok => c
                .tiktok
                .broadcaster
                .as_ref()
                .map(|b| [norm(&b.username), String::new()]),
        };
        ch.is_empty() || mine.is_some_and(|m| m.contains(&ch))
    }

    /// Set the channel from what was typed (a URL or @handle works); what was kept.
    fn set_channel(self, c: &mut Config, raw: &str) -> String {
        let (v, slot) = match self {
            P::Twitch => return String::new(),
            P::YouTube => (crate::youtube::clean_youtube_channel(raw), &mut c.youtube.channel_id),
            P::Kick => (crate::kick::clean_kick_channel(raw), &mut c.kick.channel),
            P::TikTok => (crate::tiktok::clean_tiktok_channel(raw), &mut c.tiktok.channel),
        };
        if *slot != v {
            slot.clone_from(&v);
            if self == P::Kick {
                // The ids looked up for the old channel.
                c.kick.channel_id = None;
                c.kick.chatroom_id = None;
            }
        }
        v
    }

    fn auto(self, c: &Config) -> bool {
        match self {
            P::Twitch => c.twitch.auto_connect,
            P::YouTube => c.youtube.auto_connect,
            P::Kick => c.kick.auto_connect,
            P::TikTok => c.tiktok.auto_connect,
        }
    }

    fn auto_mut(self, c: &mut Config) -> &mut bool {
        match self {
            P::Twitch => &mut c.twitch.auto_connect,
            P::YouTube => &mut c.youtube.auto_connect,
            P::Kick => &mut c.kick.auto_connect,
            P::TikTok => &mut c.tiktok.auto_connect,
        }
    }

    fn connect(self, cmds: &Cmds, on: bool) {
        tell(cmds, move |e| match (self, on) {
            (P::Twitch, true) => e.twitch_start().send(crate::twitch::TwitchCmd::Connect),
            (P::YouTube, true) => e.youtube_start().send(crate::youtube::YouTubeCmd::Connect),
            (P::Kick, true) => e.kick_start().send(crate::kick::KickCmd::Connect),
            (P::TikTok, true) => e.tiktok_start().send(crate::tiktok::TikTokCmd::Connect),
            (P::Twitch, false) => {
                if let Some(t) = e.twitch() {
                    t.send(crate::twitch::TwitchCmd::Disconnect);
                }
            }
            (P::YouTube, false) => {
                if let Some(y) = e.youtube() {
                    y.send(crate::youtube::YouTubeCmd::Disconnect);
                }
            }
            (P::Kick, false) => {
                if let Some(k) = e.kick() {
                    k.send(crate::kick::KickCmd::Disconnect);
                }
            }
            (P::TikTok, false) => {
                if let Some(t) = e.tiktok() {
                    t.send(crate::tiktok::TikTokCmd::Disconnect);
                }
            }
        });
    }
}

/// Each platform's state, read on the engine's thread: connected, and TikTok's status text.
fn states(cmds: &Cmds) -> ([bool; 4], String) {
    query(cmds, |e| {
        let tiktok = match e.tiktok().map(|t| t.current_status()) {
            Some(crate::tiktok::TikTokStatus::Error(m) | crate::tiktok::TikTokStatus::Connecting(m)) => m,
            _ => String::new(),
        };
        (
            [
                e.twitch_connected(),
                e.youtube_connected(),
                e.kick_connected(),
                e.tiktok_connected(),
            ],
            tiktok,
        )
    })
    .unwrap_or_default()
}

/// Profile picture URLs by "platform:bot:name", looked up once a run; "" while looking (or failed).
static AVATARS: Mutex<Vec<(String, String)>> = Mutex::new(Vec::new());

/// The account's profile picture URL, from the platform (on the engine's thread).
async fn avatar(e: &Rc<Engine>, cfg: &Cfg, p: P, bot: bool) -> Option<String> {
    let get = |url: String, h: Vec<(String, String)>| async move {
        let r = crate::http::request("GET", &url, &h, b"").await.ok()?;
        serde_json::from_slice::<Value>(&r.body).ok()
    };
    let at = |v: Option<Value>, ptr: &str| v?.pointer(ptr).cloned();
    let bearer = |t: &str| ("Authorization".to_string(), format!("Bearer {t}"));
    let v = match p {
        P::Twitch => {
            let (id, t) = {
                let c = cfg.borrow();
                let a = if bot { &c.twitch.bot } else { &c.twitch.broadcaster }.clone()?;
                (a.user_id, a.token.access_token)
            };
            let h = vec![("Client-Id".into(), crate::twitch::CLIENT_ID.into()), bearer(&t)];
            at(
                get(format!("{}/users?id={id}", crate::twitch::HELIX_URL), h).await,
                "/data/0/profile_image_url",
            )
        }
        P::Kick => {
            let t = crate::kick::token(e, cfg, bot, false).await.ok()?;
            at(
                get(format!("{}/users", crate::kick::KICK_API_BASE), vec![bearer(&t)]).await,
                "/data/0/profile_picture",
            )
        }
        P::YouTube => {
            let t = crate::youtube::token(e, cfg, bot, false).await.ok()?;
            let url = format!("{}/channels?part=snippet&mine=true", crate::youtube::YOUTUBE_API_BASE);
            at(
                get(url, vec![bearer(&t)]).await,
                "/items/0/snippet/thumbnails/default/url",
            )
        }
        P::TikTok => {
            let (name, seen) = {
                let c = cfg.borrow();
                let a = if bot { &c.tiktok.bot } else { &c.tiktok.broadcaster }.clone()?;
                (a.username, a.avatar)
            };
            if seen.starts_with("http") {
                return Some(seen);
            }
            let name = name.trim_start_matches('@').to_string();
            if let Some(url) = crate::webcast::avatar(&name).await {
                return Some(url);
            }
            let ttwid = crate::webcast::ttwid().await.ok()?;
            at(
                crate::webcast::user_room(&name, &ttwid).await.ok(),
                "/data/user/avatarThumb",
            )
        }
    };
    v?.as_str().filter(|u| u.starts_with("http")).map(str::to_string)
}

/// The account's profile picture, `size` px and round. Its URL is kept in the cache: used at once,
/// looked up again (in the background) when older than a day or not known yet.
fn picture(cmds: &Cmds, cfg: &Cfg, p: P, bot: bool, name: &str, size: i32) -> Option<fltk::image::RgbImage> {
    const FRESH: u64 = 24 * 60 * 60;
    let key = format!("{}:{bot}:{name}", p.name());
    let mut a = AVATARS.lock().ok()?;
    let url = match a.iter().find(|(k, _)| *k == key) {
        Some((_, url)) => url.clone(),
        None => {
            let now = crate::fmt::unix_now().as_secs();
            let saved = super::img::saved("avatars").map(|v| v.1).unwrap_or_default();
            let (url, at) = (
                saved[&key][0].as_str().unwrap_or("").to_string(),
                saved[&key][1].as_u64().unwrap_or(0),
            );
            a.push((key.clone(), url.clone()));
            if now.saturating_sub(at) >= FRESH {
                let cfg = cfg.clone();
                tell(cmds, move |e| {
                    let e = e.clone();
                    tokio::task::spawn_local(async move {
                        let Some(url) = avatar(&e, &cfg, p, bot).await else {
                            return;
                        };
                        if let Ok(mut a) = AVATARS.lock() {
                            if let Some(slot) = a.iter_mut().find(|(k, _)| *k == key) {
                                slot.1.clone_from(&url);
                            }
                        }
                        let mut all = super::img::saved("avatars").map(|v| v.1).unwrap_or_default();
                        if !all.is_object() {
                            all = serde_json::json!({});
                        }
                        all[&key] = serde_json::json!([url, crate::fmt::unix_now().as_secs()]);
                        super::img::save("avatars", &all);
                        super::wake(super::Msg::Image);
                    });
                });
            }
            url
        }
    };
    drop(a);
    super::img::get(cmds, &url, size, true).map(|p| p.first())
}

/// Connect platform `i` (Twitch, YouTube, Kick, TikTok) now.
pub(super) fn connect(cmds: &Cmds, i: usize) {
    ALL[i].connect(cmds, true);
}

struct Row {
    p: P,
    status: Frame,
    channel: Frame,
    auto: CheckButton,
    connect: Button,
    adv: Button,
    /// Broadcaster, then bot: profile picture, name and Login/Logout.
    who: [(Frame, Frame, Button); 2],
}

pub struct Accounts {
    pub tab: Flex,
    cfg: Cfg,
    cmds: Cmds,
    rows: Vec<Row>,
}

impl Accounts {
    pub fn refresh(&mut self) {
        // Before borrowing the config: the engine may be waiting to write it.
        let (on, tiktok) = states(&self.cmds);
        let c = self.cfg.borrow();
        for (i, r) in self.rows.iter_mut().enumerate() {
            let (text, color) = if on[i] {
                ("● Connected".to_string(), Color::from_rgb(0x3c, 0xb3, 0x71))
            } else {
                let why = match r.p {
                    P::Twitch if c.twitch.broadcaster.is_none() => "log in the broadcaster first".into(),
                    P::Twitch => String::new(),
                    P::TikTok if !tiktok.is_empty() => tiktok.clone(),
                    p if p.channel(&c).is_empty() => "log in the broadcaster first".into(),
                    _ => String::new(),
                };
                let t = if why.is_empty() {
                    "○ Not connected".to_string()
                } else {
                    format!("○ Not connected: {why}")
                };
                (t, Color::from_rgb(0x99, 0x99, 0x99))
            };
            // "@name" would be read as a symbol and cut the label.
            r.status.set_label(&text.replace('@', "@@"));
            r.status.set_tooltip(&text.replace('@', "@@"));
            r.status.set_label_color(color);
            r.connect.set_label(if on[i] { "Disconnect" } else { "Connect" });
            r.channel.set_label(&if r.p.own(&c) {
                "Channel: the broadcaster's".to_string()
            } else {
                format!("Channel: {} (Advanced)", r.p.channel(&c).replace('@', "@@"))
            });
            r.auto.set_checked(r.p.auto(&c));
            for (bot, (pic, name, b)) in r.who.iter_mut().enumerate() {
                let who = if bot == 1 { "Bot" } else { "Broadcaster" };
                let account = r.p.account(&c, bot == 1);
                match &account {
                    Some(n) => {
                        name.set_label(&format!("{who}: {}", n.replace('@', "@@")));
                        b.set_label("Logout");
                    }
                    None => {
                        name.set_label(&format!("{who}: not logged in"));
                        b.set_label("Login");
                    }
                }
                // The name without the "(log in again…)" note: the lookup is per account.
                let id = account.as_deref().map(|n| n.split(" (").next().unwrap_or(n));
                pic.set_image(id.and_then(|n| picture(&self.cmds, &self.cfg, r.p, bot == 1, n, ROW - 4)));
            }
        }
        drop(c);
        self.tab.redraw();
    }
}

pub fn tab(tabs: &Tabs, cfg: &Cfg, cmds: &Cmds) -> Rc<RefCell<Accounts>> {
    let mut col = super::tab(tabs, "Accounts");
    let mut rows = Vec::new();
    for p in ALL {
        title(&mut col, p.name());
        let mut top = Flex::default().row();
        let status = Frame::default().with_align(Align::Left | Align::Inside | Align::Clip);
        let auto = CheckButton::default().with_label("Connect on start");
        top.fixed(&auto, 140);
        let connect = button(&mut top, "Connect", 100);
        let adv = button(&mut top, "Advanced…", 100);
        top.end();
        col.fixed(&top, ROW);

        let mut bottom = Flex::default().row();
        let channel = Frame::default().with_align(Align::Left | Align::Inside | Align::Clip);
        bottom.fixed(&channel, 300);
        let gap = Frame::default();
        bottom.fixed(&gap, 20);
        let mut who = Vec::new();
        for _ in 0..2 {
            let pic = Frame::default();
            bottom.fixed(&pic, ROW);
            let f = Frame::default().with_align(Align::Left | Align::Inside | Align::Clip);
            let b = button(&mut bottom, "Login", 80);
            who.push((pic, f, b));
        }
        bottom.end();
        col.fixed(&bottom, ROW);
        let sep = Frame::default();
        col.fixed(&sep, 6);
        let who: [(Frame, Frame, Button); 2] = [who.remove(0), who.remove(0)];
        rows.push(Row {
            p,
            status,
            channel,
            adv,
            auto,
            connect,
            who,
        });
    }
    let guide = Frame::default()
        .with_label(
            "• Login opens your browser (Twitch shows a code to type at twitch.tv/activate).\n\
             • Chat is sent as the bot when one is logged in, else as the broadcaster.\n\
             • Kick and YouTube send with your own app: Advanced shows how to make one (once).\n\
             • TikTok reads the channel's live by itself; Login opens TikTok's own page.\n\
             • Each platform reads the broadcaster's channel; another one can be set in Advanced.",
        )
        .with_align(Align::Left | Align::Inside | Align::Wrap);
    col.fixed(&guide, 88);
    Frame::default();
    col.end();

    let me = Rc::new(RefCell::new(Accounts {
        tab: col,
        cfg: cfg.clone(),
        cmds: cmds.clone(),
        rows: Vec::new(),
    }));
    for r in &mut rows {
        let p = r.p;
        let refresh = {
            let me = me.clone();
            move || {
                if let Ok(mut m) = me.try_borrow_mut() {
                    m.refresh();
                }
            }
        };
        r.adv.set_callback({
            let cfg = cfg.clone();
            let refresh = refresh.clone();
            move |_| {
                advanced(&cfg, p);
                refresh();
            }
        });
        r.auto.set_callback({
            let cfg = cfg.clone();
            move |b| {
                let on = b.is_checked();
                save(&cfg, |c| *p.auto_mut(c) = on);
            }
        });
        r.connect.set_callback({
            let cmds = cmds.clone();
            let refresh = refresh.clone();
            move |b| {
                p.connect(&cmds, b.label() == "Connect");
                refresh();
            }
        });
        for (bot, (_, _, b)) in r.who.iter_mut().enumerate() {
            let bot = bot == 1;
            let cfg = cfg.clone();
            let cmds = cmds.clone();
            let refresh = refresh.clone();
            b.set_callback(move |b| {
                if b.label() == "Logout" {
                    let mut revoke = None;
                    save(&cfg, |c| revoke = p.logout(c, bot));
                    if let Some(url) = revoke {
                        // Best effort: the token is gone here either way.
                        tell(&cmds, move |_| {
                            tokio::task::spawn_local(async move {
                                let h = [(
                                    "Content-Type".to_string(),
                                    "application/x-www-form-urlencoded".to_string(),
                                )];
                                // Kick reads the token from the query; Twitch and Google from a form.
                                let (u, q) = match url.split_once('?') {
                                    Some((u, q)) if !u.contains("kick.com") => (u, q),
                                    _ => (url.as_str(), ""),
                                };
                                let r = crate::http::request("POST", u, &h, q.as_bytes()).await;
                                let ok = r.as_ref().is_ok_and(|r| (200..300).contains(&r.status));
                                crate::say(format!(
                                    "{}: login revoked{}",
                                    p.name(),
                                    if ok { "" } else { " (the server didn't confirm)" }
                                ));
                            });
                        });
                    }
                    if p == P::Twitch && !bot {
                        p.connect(&cmds, false);
                    }
                } else {
                    login(&cfg, &cmds, p, bot);
                }
                refresh();
            });
        }
    }
    me.borrow_mut().rows = rows;
    me
}

fn login(cfg: &Cfg, cmds: &Cmds, p: P, bot: bool) {
    match p {
        P::Twitch => device_login(cfg, cmds, bot),
        P::TikTok => tiktok_login(cfg, bot),
        P::Kick => {
            let c = cfg.clone();
            if let Some(a) = browser_login(cmds, "Kick", bot, async move { crate::kick::login(&c, bot).await }) {
                save(cfg, |c| {
                    *if bot { &mut c.kick.bot } else { &mut c.kick.broadcaster } = Some(a)
                });
            }
        }
        P::YouTube => {
            let c = cfg.clone();
            if let Some(a) = browser_login(cmds, "YouTube", bot, async move { crate::youtube::login(&c).await }) {
                save(cfg, |c| {
                    *if bot {
                        &mut c.youtube.bot
                    } else {
                        &mut c.youtube.broadcaster
                    } = Some(a)
                });
            }
        }
    }
}

/// Run a browser login on the engine's thread, with a dialog to cancel it; the account.
fn browser_login<T: Send + 'static>(
    cmds: &Cmds,
    what: &str,
    bot: bool,
    f: impl std::future::Future<Output = Result<T, String>> + 'static + Send,
) -> Option<T> {
    let who = if bot { "bot" } else { "broadcaster" };
    let (tx, rx) = std::sync::mpsc::channel();
    let (htx, hrx) = std::sync::mpsc::channel();
    tell(cmds, move |_| {
        let h = tokio::task::spawn_local(async move {
            let _ = tx.send(f.await);
        });
        let _ = htx.send(h.abort_handle());
    });
    let (mut win, answer) = dialogs::window(420, 110, &format!("{what} login ({who})"));
    let mut col = Flex::new(10, 10, 400, 90, None).column();
    let msg = Frame::default()
        .with_label(&format!(
            "Log in with your {what} {who} account in the browser window that opened.\nThis closes by itself when you're done."
        ))
        .with_align(Align::Left | Align::Inside | Align::Wrap);
    col.fixed(&msg, 50);
    let mut bar = Flex::default().row();
    Frame::default();
    let mut cancel = button(&mut bar, "Cancel", 80);
    cancel.set_callback({
        let a = answer.clone();
        move |_| a.set(Some(false))
    });
    bar.end();
    col.fixed(&bar, ROW);
    col.end();
    win.end();
    win.show();
    let mut out = None;
    while win.shown() && answer.get().is_none() {
        if let Ok(res) = rx.try_recv() {
            match res {
                Ok(a) => out = Some(a),
                Err(e) => dialog::alert_default(&format!("{what} login failed: {e}")),
            }
            break;
        }
        fltk::app::wait_for(0.1).ok();
    }
    win.hide();
    if out.is_none() {
        // Cancelled: stop listening, so the port is free for the next try.
        if let Ok(h) = hrx.try_recv() {
            h.abort();
        }
    }
    out
}

/// The settings few people need, per platform.
fn advanced(cfg: &Cfg, p: P) {
    let c = cfg.borrow();
    // (label, value, secret)
    let mut fields: Vec<(&str, String, bool)> = match p {
        P::TikTok => Vec::new(),
        P::Twitch => vec![(
            "Sub counter rollover (0: never)",
            c.twitch.sub_counter_rollover.to_string(),
            false,
        )],
        P::Kick => vec![
            ("Client ID", c.kick.client_id.clone(), false),
            ("Client secret", c.kick.client_secret.clone(), true),
        ],
        P::YouTube => vec![
            ("Video ID (optional)", c.youtube.video_id.clone(), false),
            ("Google client ID", c.youtube.client_id.clone(), false),
            ("Google client secret", c.youtube.client_secret.clone(), true),
        ],
    };
    let help = match p {
        P::Kick => format!(
            "Sending on Kick needs your own Kick app: kick.com > Settings > Developer > Create app, \
             redirect URL {}, every scope ticked. Paste its client ID and secret here.",
            crate::kick::KICK_REDIRECT
        ),
        P::YouTube => "Sending on YouTube needs your own Google client: console.cloud.google.com > new project > \
             enable YouTube Data API v3 > OAuth consent screen (External, Publish) > Credentials > OAuth client, Desktop app. \
             Leave Video ID empty to follow the channel's current live.".into(),
        P::Twitch => "The sub counter counts every sub, resub and gifted sub (Get/Set Twitch Sub Counter \
            Count). Every time it reaches the rollover it starts again and Sub Counter Rollover fires."
            .into(),
        P::TikTok => String::new(),
    };
    // Twitch reads only the broadcaster's own channel.
    let help = if p == P::Twitch {
        help
    } else {
        let other = if p.own(&c) { "" } else { p.channel(&c) };
        fields.insert(0, ("Other channel", other.to_string(), false));
        format!("{help}\nOther channel: read this channel instead of the broadcaster's (empty: theirs).")
            .trim_start()
            .to_string()
    };
    drop(c);
    let h = 132 + fields.len() as i32 * (ROW + 6);
    let (mut win, answer) = dialogs::window(620, h, &format!("{} - Advanced", p.name()));
    let mut col = Flex::new(10, 10, 600, h - 20, None).column();
    col.set_pad(6);
    let f = Frame::default()
        .with_label(&help)
        .with_align(Align::Left | Align::Inside | Align::Wrap);
    col.fixed(&f, 76);
    let mut inputs: Vec<Box<dyn Fn() -> String>> = Vec::new();
    for (label, value, secret) in &fields {
        let r = dialogs::row(label);
        if *secret {
            let mut i = SecretInput::default();
            i.set_value(value);
            inputs.push(Box::new(move || i.value()));
        } else {
            let mut i = Input::default();
            i.set_value(value);
            inputs.push(Box::new(move || i.value()));
        }
        r.end();
        col.fixed(&r, ROW);
    }
    let mut bar = Flex::default().row();
    if p == P::Kick {
        let mut b = button(&mut bar, "Copy redirect URL", 150);
        b.set_callback(|_| fltk::app::copy(crate::kick::KICK_REDIRECT));
    }
    dialogs::ok_cancel(&mut bar, &answer, true);
    bar.end();
    col.fixed(&bar, ROW);
    col.end();
    win.end();
    win.show();
    while win.shown() && answer.get().is_none() {
        if !fltk::app::wait() {
            break;
        }
    }
    win.hide();
    if answer.get() != Some(true) {
        return;
    }
    let mut v: Vec<String> = inputs.iter().map(|i| i().trim().to_string()).collect();
    save(cfg, |c| {
        if p != P::Twitch {
            // Emptied: the save puts the broadcaster's back.
            p.set_channel(c, &v.remove(0));
        }
        match p {
            P::Kick => {
                c.kick.client_id.clone_from(&v[0]);
                c.kick.client_secret.clone_from(&v[1]);
            }
            P::YouTube => {
                c.youtube.video_id = crate::youtube::clean_youtube_video(&v[0]);
                c.youtube.client_id.clone_from(&v[1]);
                c.youtube.client_secret.clone_from(&v[2]);
            }
            P::Twitch => c.twitch.sub_counter_rollover = v[0].parse().unwrap_or(0),
            P::TikTok => {}
        }
    });
}

/// TikTok: the TikTok window (its own browser profile per account) returns the cookies.
fn tiktok_login(cfg: &Cfg, bot: bool) {
    let who = if bot { "bot" } else { "broadcaster" };
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || drop(tx.send(crate::tiktok::login(bot))));
    let (mut win, answer) = dialogs::window(420, 110, &format!("TikTok login ({who})"));
    let mut col = Flex::new(10, 10, 400, 90, None).column();
    let msg = Frame::default()
        .with_label(&format!(
            "Log in with your TikTok {who} account in the window that opened.\nThis closes by itself when you're done."
        ))
        .with_align(Align::Left | Align::Inside | Align::Wrap);
    col.fixed(&msg, 50);
    col.end();
    win.end();
    win.show();
    while win.shown() && answer.get().is_none() {
        if let Ok(res) = rx.try_recv() {
            match res {
                Ok(a) => save(cfg, |c| {
                    let a = Some(a);
                    *if bot {
                        &mut c.tiktok.bot
                    } else {
                        &mut c.tiktok.broadcaster
                    } = a;
                }),
                Err(e) => dialog::alert_default(&format!("TikTok login failed: {e}")),
            }
            break;
        }
        fltk::app::wait_for(0.1).ok();
    }
    win.hide();
}

fn device_login(cfg: &Cfg, cmds: &Cmds, as_bot: bool) {
    let who = if as_bot { "Bot" } else { "Broadcaster" };
    let scopes = if as_bot {
        crate::twitch::BOT_SCOPES
    } else {
        crate::twitch::BROADCASTER_SCOPES
    };

    let (tx, rx) = std::sync::mpsc::channel();
    tell(cmds, move |_| {
        tokio::task::spawn_local(async move {
            let res = crate::twitch::device_flow_start(crate::twitch::CLIENT_ID, scopes).await;
            let _ = tx.send(res);
        });
    });

    let code_resp = match rx.recv() {
        Ok(Ok(resp)) => resp,
        Ok(Err(e)) => {
            dialog::alert_default(&format!("Failed to start login: {e}"));
            return;
        }
        Err(_) => return,
    };

    let user_code = code_resp.user_code.clone();
    let device_code = code_resp.device_code.clone();
    let interval = code_resp.interval.max(1);

    crate::oauth::open_browser(&code_resp.verification_uri);

    let (mut win, answer) = dialogs::window(460, 220, &format!("Twitch Login - {who}"));
    let mut col = Flex::new(10, 10, 440, 200, None).column();
    col.set_pad(8);

    let msg = format!(
        "Please authorize streamerz on Twitch.\n\nActivation code:\n  {}\n\nBrowser opened to:\n  {}\n\nWaiting for authorization...",
        user_code, code_resp.verification_uri
    );
    let msg_frame = Frame::default()
        .with_label(&msg)
        .with_align(Align::Left | Align::Inside | Align::Wrap);
    col.fixed(&msg_frame, 140);

    let mut bar = Flex::default().row();
    let mut open_btn = button(&mut bar, "Open Browser", 110);
    open_btn.set_callback({
        let uri = code_resp.verification_uri.clone();
        move |_| crate::oauth::open_browser(&uri)
    });
    let mut copy_btn = button(&mut bar, "Copy Code", 90);
    copy_btn.set_callback({
        let code = user_code.clone();
        move |_| fltk::app::copy(&code)
    });
    Frame::default();
    let mut cancel_btn = button(&mut bar, "Cancel", 80);
    cancel_btn.set_callback({
        let a = answer.clone();
        move |_| a.set(Some(false))
    });
    bar.end();
    col.fixed(&bar, ROW + 4);
    col.end();
    win.end();

    let (done_tx, done_rx) = std::sync::mpsc::channel();
    let cancelled = Arc::new(AtomicBool::new(false));
    let is_cancelled = cancelled.clone();
    let cfg_clone = cfg.clone();

    tell(cmds, move |e| {
        let e = e.clone();
        tokio::task::spawn_local(async move {
            let mut ticker = tokio::time::interval(std::time::Duration::from_secs(interval));
            for _ in 0..(code_resp.expires_in / interval) {
                ticker.tick().await;
                if is_cancelled.load(Ordering::Relaxed) {
                    break;
                }
                match crate::twitch::device_flow_poll(crate::twitch::CLIENT_ID, &device_code).await {
                    Ok(Some(token)) => {
                        match crate::twitch::fetch_user(crate::twitch::CLIENT_ID, &token.access_token, None).await {
                            Ok(mut acct) => {
                                let expires_at = std::time::SystemTime::now()
                                    .duration_since(std::time::UNIX_EPOCH)
                                    .map_or(0, |d| d.as_secs())
                                    + token.expires_in;
                                acct.token = crate::twitch::TwitchToken {
                                    access_token: token.access_token,
                                    refresh_token: token.refresh_token,
                                    scope: token.scope,
                                    expires_at,
                                };
                                let (name, user_id) = (acct.display_name.clone(), acct.user_id.clone());
                                save(&cfg_clone, |c| {
                                    if as_bot {
                                        c.twitch.bot = Some(acct.clone());
                                    } else {
                                        c.twitch.broadcaster = Some(acct.clone());
                                    }
                                });
                                if !as_bot {
                                    let mut args = Obj::new();
                                    args.insert("user".into(), name.clone().into());
                                    args.insert("userId".into(), user_id.into());
                                    e.fire(
                                        crate::twitch::T_AUTHED,
                                        "Broadcaster Authenticated",
                                        "Twitch/Connections",
                                        "twitch",
                                        &args,
                                        |_| true,
                                    );
                                    e.emit("Twitch", "BroadcasterAuthenticated", || json!({"user": name}));
                                }
                                crate::say(format!("Twitch {who} logged in as {name}"));
                                let _ = done_tx.send(Ok(name));
                            }
                            Err(e) => {
                                let _ = done_tx.send(Err(e));
                            }
                        }
                        break;
                    }
                    Ok(None) => {}
                    Err(e) => {
                        let _ = done_tx.send(Err(e));
                        break;
                    }
                }
            }
        });
    });

    win.show();
    while win.shown() && answer.get().is_none() {
        if let Ok(res) = done_rx.try_recv() {
            match res {
                Ok(name) => {
                    win.hide();
                    dialog::message_default(&format!("Successfully logged in {who} as {name}!"));
                    break;
                }
                Err(e) => {
                    win.hide();
                    dialog::alert_default(&format!("Login failed: {e}"));
                    break;
                }
            }
        }
        fltk::app::wait();
    }
    cancelled.store(true, Ordering::Relaxed);
    win.hide();
}
