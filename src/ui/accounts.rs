//! The Accounts tab: one row per platform (status, channel, Connect, broadcaster and bot logins),
//! with the rarely needed settings behind Advanced.

use super::fit::Ellipsis;
use super::lists::save;
use super::{button, dialogs, query, tell, ROW};
use crate::engine::Engine;
use crate::i18n::{button_id, tr, trf};
use crate::model::Obj;
use crate::Config;
use fltk::button::{Button, CheckButton};
use fltk::enums::Align;
use fltk::frame::Frame;
use fltk::group::{Flex, Wizard};
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
    fn from_slug(s: &str) -> Option<Self> {
        match s {
            "twitch" => Some(P::Twitch),
            "youtube" => Some(P::YouTube),
            "kick" => Some(P::Kick),
            "tiktok" => Some(P::TikTok),
            _ => None,
        }
    }

    fn name(self) -> &'static str {
        match self {
            P::Twitch => "Twitch",
            P::YouTube => "YouTube",
            P::Kick => "Kick",
            P::TikTok => "TikTok",
        }
    }

    /// Raw display name of the logged-in broadcaster or bot (without status suffixes).
    fn raw_account_name(self, c: &Config, bot: bool) -> Option<String> {
        match self {
            P::Twitch => if bot { &c.twitch.bot } else { &c.twitch.broadcaster }
                .as_ref()
                .map(|a| a.display_name.clone()),
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
        .filter(|s| !s.is_empty())
    }

    /// Optional status warning on the account (locked keyring, expired token, or new scopes).
    fn account_note(self, c: &Config, bot: bool) -> Option<&'static str> {
        let locked = crate::secrets::is_locked();
        match self {
            P::Twitch => {
                let a = if bot { &c.twitch.bot } else { &c.twitch.broadcaster }.as_ref()?;
                let want = if bot {
                    crate::twitch::BOT_SCOPES
                } else {
                    crate::twitch::BROADCASTER_SCOPES
                };
                if a.token.access_token.is_empty() {
                    Some(if locked {
                        tr("accounts.note.keyring_locked")
                    } else {
                        tr("accounts.note.login_expired")
                    })
                } else if want.iter().any(|w| !a.token.scope.iter().any(|g| g == w)) {
                    Some(tr("accounts.note.new_permissions"))
                } else {
                    None
                }
            }
            P::YouTube => {
                let a = if bot { &c.youtube.bot } else { &c.youtube.broadcaster }.as_ref()?;
                (a.token.access_token.is_empty() && a.token.refresh_token.is_empty() && locked)
                    .then_some(tr("accounts.note.keyring_locked"))
            }
            P::Kick => {
                let a = if bot { &c.kick.bot } else { &c.kick.broadcaster }.as_ref()?;
                (a.token.access_token.is_empty() && a.token.refresh_token.is_empty() && locked)
                    .then_some(tr("accounts.note.keyring_locked"))
            }
            P::TikTok => {
                let a = if bot { &c.tiktok.bot } else { &c.tiktok.broadcaster }.as_ref()?;
                (a.session_cookie.is_empty() && locked).then_some(tr("accounts.note.keyring_locked"))
            }
        }
    }

    /// The logged-in broadcaster's or bot's name (with any warning note).
    fn account(self, c: &Config, bot: bool) -> Option<String> {
        let name = self.raw_account_name(c, bot)?;
        match self.account_note(c, bot) {
            Some(note) => Some(format!("{name} ({note})")),
            None => Some(name),
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

/// Information about a logged-in account slot (Broadcaster or Bot) for Chat's account indicator & selector.
pub(super) struct AccountSlot {
    pub name: String,
    pub note: Option<&'static str>,
    pub pic: Option<fltk::image::RgbImage>,
}

/// Look up `[broadcaster, bot]` account info and round avatars for `platform` (`"twitch"`, `"youtube"`, `"kick"`, `"tiktok"`).
pub(super) fn platform_accounts(cmds: &Cmds, cfg: &Cfg, platform: &str, size: i32) -> [Option<AccountSlot>; 2] {
    let Some(p) = P::from_slug(platform) else {
        return [None, None];
    };
    let raw = {
        let c = cfg.borrow();
        [
            p.raw_account_name(&c, false).map(|n| (n, p.account_note(&c, false))),
            p.raw_account_name(&c, true).map(|n| (n, p.account_note(&c, true))),
        ]
    };
    let build = |(bot, item): (usize, Option<(String, Option<&'static str>)>)| {
        let (name, note) = item?;
        let pic = picture(cmds, cfg, p, bot == 1, &name, size);
        Some(AccountSlot { name, note, pic })
    };
    let [b0, b1] = raw;
    [build((0, b0)), build((1, b1))]
}

/// Trigger an interactive keyring unlock on the engine thread and alert only if it fails.
pub(super) fn unlock_logins_interactive(cmds: &Cmds) {
    let (tx, rx) = std::sync::mpsc::channel();
    tell(cmds, move |e| {
        let e = e.clone();
        tokio::task::spawn_local(async move {
            let _ = tx.send(e.unlock_saved_logins(true).await);
        });
    });
    let mut waited = 0;
    loop {
        if let Ok(res) = rx.try_recv() {
            match res {
                Ok(true) => {
                    crate::ui::changed();
                }
                Ok(false) => {}
                Err(err) => {
                    super::dialogs::alert(&trf("accounts.keyring_unlock_failed", &[("error", &err.to_string())]));
                }
            }
            break;
        }
        waited += 1;
        if waited > 300 {
            break;
        }
        let _ = fltk::app::wait_for(0.05);
    }
}

/// Each platform's state, read on the engine's thread: connected, standby, and TikTok's status text.
fn states(cmds: &Cmds) -> ([bool; 4], [bool; 4], String) {
    query(cmds, |e| {
        let tiktok = match e.tiktok().map(|t| t.current_status()) {
            Some(
                crate::tiktok::TikTokStatus::Error(m)
                | crate::tiktok::TikTokStatus::Connecting(m)
                | crate::tiktok::TikTokStatus::Standby(m),
            ) => m,
            _ => String::new(),
        };
        (
            [
                e.twitch_connected(),
                e.youtube_connected(),
                e.kick_connected(),
                e.tiktok_connected(),
            ],
            [false, e.youtube_standby(), false, e.tiktok_standby()],
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
            if t.is_empty() {
                return None;
            }
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
    lock_bar: Flex,
    lock_msg: Frame,
    rows: Vec<Row>,
}

impl Accounts {
    pub fn refresh(&mut self) {
        // Before borrowing the config: the engine may be waiting to write it.
        let (on, standby, tiktok) = states(&self.cmds);
        let locked = crate::secrets::is_locked();
        if locked {
            let detail = match crate::secrets::locked_reason() {
                Some(reason) if !reason.is_empty() => trf("accounts.locked_reason", &[("reason", &reason)]),
                _ => tr("accounts.locked_default").to_string(),
            };
            super::Health::Locked.show(&mut self.lock_msg, &detail);
            self.lock_bar.show();
        } else {
            self.lock_bar.hide();
        }

        let c = self.cfg.borrow();
        for (i, r) in self.rows.iter_mut().enumerate() {
            let active = on[i] || standby[i];
            use super::Health as H;
            let (h, text) = if on[i] {
                (H::Up, tr("conns.connected").to_string())
            } else if standby[i] {
                match r.p {
                    P::TikTok if !tiktok.is_empty() => (H::Standby, tiktok.clone()),
                    _ => (H::Standby, tr("accounts.standby_default").to_string()),
                }
            } else if locked && r.p.raw_account_name(&c, false).is_some() {
                (H::Locked, tr("accounts.keyring_locked_above").to_string())
            } else {
                let why = match r.p {
                    P::Twitch if c.twitch.broadcaster.is_none() => tr("accounts.login_broadcaster_first").into(),
                    P::Twitch => String::new(),
                    P::TikTok if !tiktok.is_empty() => tiktok.clone(),
                    p if p.channel(&c).is_empty() => tr("accounts.login_broadcaster_first").into(),
                    _ => String::new(),
                };
                if why.is_empty() {
                    (H::Off, tr("conns.off").to_string())
                } else {
                    (H::Off, trf("accounts.off_why", &[("why", &why)]))
                }
            };
            h.show(&mut r.status, &text);
            r.status.set_tooltip(&text.replace('@', "@@"));
            r.connect.set_label(if active {
                tr("btn.disconnect")
            } else {
                tr("btn.connect")
            });
            super::with_icon(&mut r.connect);
            r.channel.set_label(&if r.p.own(&c) {
                tr("accounts.channel_yours").to_string()
            } else {
                trf(
                    "accounts.channel_advanced",
                    &[("channel", &r.p.channel(&c).replace('@', "@@"))],
                )
            });
            r.auto.set_checked(r.p.auto(&c));
            for (bot, (pic, name, b)) in r.who.iter_mut().enumerate() {
                let who = if bot == 1 {
                    tr("accounts.who_bot")
                } else {
                    tr("accounts.who_broadcaster")
                };
                let account = r.p.account(&c, bot == 1);
                match &account {
                    Some(n) => {
                        name.set_label(&format!("{who}: {}", n.replace('@', "@@")));
                        name.set_tooltip(&n.replace('@', "@@"));
                        b.set_label(tr("btn.logout"));
                        super::with_icon(b);
                    }
                    None => {
                        name.set_label(&trf("accounts.no_who", &[("who", &who.to_lowercase())]));
                        b.set_label(tr("btn.login"));
                        super::with_icon(b);
                    }
                }
                let raw_name = r.p.raw_account_name(&c, bot == 1);
                pic.set_image(raw_name.and_then(|n| picture(&self.cmds, &self.cfg, r.p, bot == 1, &n, ROW - 4)));
            }
        }
        drop(c);
        self.tab.layout();
        self.tab.redraw();
    }
}

pub fn tab(tabs: &Wizard, cfg: &Cfg, cmds: &Cmds) -> Rc<RefCell<Accounts>> {
    let (scroll, mut col) = super::tab_scrolled(tabs, "Accounts", 460);

    let mut lock_bar = Flex::default().row();
    lock_bar.set_pad(8);
    let lock_msg = Frame::default().with_align(Align::Left | Align::Inside | Align::Clip);
    let mut unlock_btn = button(&mut lock_bar, tr("accounts.unlock_logins"), 120);
    lock_bar.end();
    col.fixed(&lock_bar, ROW);
    lock_bar.hide();

    let mut rows = Vec::new();
    for p in ALL {
        let mut top = Flex::default().row();
        top.set_pad(8);
        let mut name = Frame::default()
            .with_label(&format!("  {}", p.name()))
            .with_align(Align::Left | Align::Inside | Align::ImageNextToText)
            .ellipsis();
        name.set_label_font(fltk::enums::Font::HelveticaBold);
        name.set_image(super::img::logo(p.name(), ROW - 8));
        top.fixed(&name, 110);
        let status = Frame::default().with_align(Align::Left | Align::Inside | Align::Clip);
        let auto = CheckButton::default().with_label(tr("accounts.connect_on_start"));
        top.fixed(&auto, 140);
        let connect = button(&mut top, tr("btn.connect"), 100);
        let adv = button(&mut top, &format!("{}…", tr("btn.advanced")), 100);
        top.end();
        col.fixed(&top, ROW);

        let mut bottom = Flex::default().row();
        bottom.set_pad(8);
        let gap = Frame::default();
        bottom.fixed(&gap, 110);
        let mut channel = Frame::default().with_align(Align::Left | Align::Inside | Align::Clip);
        channel.set_label_color(super::muted());
        bottom.fixed(&channel, 120);
        let mut who = Vec::new();
        for _ in 0..2 {
            let pic = Frame::default();
            bottom.fixed(&pic, ROW);
            let f = Frame::default().with_align(Align::Left | Align::Inside | Align::Clip);
            let b = button(&mut bottom, tr("btn.login"), 80);
            who.push((pic, f, b));
        }
        bottom.end();
        col.fixed(&bottom, ROW);
        let sep = Frame::default();
        col.fixed(&sep, 10);
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
        .with_label(tr("accounts.guide"))
        .with_align(Align::Left | Align::Inside | Align::Wrap);
    guide.clone().set_label_color(super::muted());
    col.fixed(&guide, 72);
    Frame::default();
    col.end();
    scroll.end();

    let me = Rc::new(RefCell::new(Accounts {
        tab: col,
        cfg: cfg.clone(),
        cmds: cmds.clone(),
        lock_bar,
        lock_msg,
        rows: Vec::new(),
    }));
    unlock_btn.set_callback({
        let cmds = cmds.clone();
        let me = me.clone();
        move |_| {
            unlock_logins_interactive(&cmds);
            if let Ok(mut m) = me.try_borrow_mut() {
                m.refresh();
            }
        }
    });
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
                p.connect(&cmds, button_id(&b.label()) == Some("connect"));
                refresh();
            }
        });
        for (bot, (_, _, b)) in r.who.iter_mut().enumerate() {
            let bot = bot == 1;
            let cfg = cfg.clone();
            let cmds = cmds.clone();
            let refresh = refresh.clone();
            b.set_callback(move |b| {
                if button_id(&b.label()) == Some("logout") {
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
    let (tx, rx) = std::sync::mpsc::channel();
    let (htx, hrx) = std::sync::mpsc::channel();
    tell(cmds, move |_| {
        let h = tokio::task::spawn_local(async move {
            let _ = tx.send(f.await);
        });
        let _ = htx.send(h.abort_handle());
    });
    let who_s = if bot {
        tr("accounts.who_bot").to_lowercase()
    } else {
        tr("accounts.who_broadcaster").to_lowercase()
    };
    let (mut win, answer) = dialogs::window(
        420,
        110,
        &trf("accounts.browser_login_title", &[("what", what), ("who", &who_s)]),
    );
    let mut col = Flex::new(10, 10, 400, 90, None).column();
    let msg = Frame::default()
        .with_label(&trf("accounts.browser_login_msg", &[("what", what), ("who", &who_s)]))
        .with_align(Align::Left | Align::Inside | Align::Wrap);
    col.fixed(&msg, 50);
    let mut bar = Flex::default().row();
    bar.set_pad(8);
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
                Err(e) => super::dialogs::alert(&trf(
                    "accounts.browser_login_failed",
                    &[("what", what), ("error", &e.to_string())],
                )),
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
            tr("accounts.twitch.sub_counter_rollover"),
            c.twitch.sub_counter_rollover.to_string(),
            false,
        )],
        P::Kick => vec![
            (tr("accounts.kick.client_id"), c.kick.client_id.clone(), false),
            (tr("accounts.kick.client_secret"), c.kick.client_secret.clone(), true),
        ],
        P::YouTube => vec![
            (tr("accounts.youtube.video_id"), c.youtube.video_id.clone(), false),
            (tr("accounts.youtube.client_id"), c.youtube.client_id.clone(), false),
            (
                tr("accounts.youtube.client_secret"),
                c.youtube.client_secret.clone(),
                true,
            ),
        ],
    };
    let help = match p {
        P::Kick => trf("accounts.kick.help", &[("url", crate::kick::KICK_REDIRECT)]),
        P::YouTube => tr("accounts.youtube.help").into(),
        P::Twitch => tr("accounts.twitch.sub_counter_help").into(),
        P::TikTok => String::new(),
    };
    // Twitch reads only the broadcaster's own channel.
    let help = if p == P::Twitch {
        help
    } else {
        let other = if p.own(&c) { "" } else { p.channel(&c) };
        fields.insert(0, (tr("accounts.other_channel"), other.to_string(), false));
        format!("{help}{}", tr("accounts.other_channel_help"))
            .trim_start()
            .to_string()
    };
    drop(c);
    let h = 132 + fields.len() as i32 * (ROW + 6);
    let (mut win, answer) = dialogs::window(620, h, &trf("accounts.advanced_title", &[("platform", p.name())]));
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
    bar.set_pad(8);
    if p == P::Kick {
        let mut b = button(&mut bar, tr("btn.copy_redirect_url"), 150);
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
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || drop(tx.send(crate::tiktok::login(bot))));
    let who_s = if bot {
        tr("accounts.who_bot").to_lowercase()
    } else {
        tr("accounts.who_broadcaster").to_lowercase()
    };
    let (mut win, answer) = dialogs::window(420, 110, &trf("accounts.tiktok_login_title", &[("who", &who_s)]));
    let mut col = Flex::new(10, 10, 400, 90, None).column();
    let msg = Frame::default()
        .with_label(&trf("accounts.tiktok_login_msg", &[("who", &who_s)]))
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
                Err(e) => super::dialogs::alert(&trf("accounts.tiktok_login_failed", &[("error", &e.to_string())])),
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
            super::dialogs::alert(&trf(
                "accounts.twitch_device.start_failed",
                &[("error", &e.to_string())],
            ));
            return;
        }
        Err(_) => return,
    };

    let user_code = code_resp.user_code.clone();
    let device_code = code_resp.device_code.clone();
    let interval = code_resp.interval.max(1);

    crate::oauth::open_browser(&code_resp.verification_uri);

    let (mut win, answer) = dialogs::window(460, 220, &trf("accounts.twitch_device.title", &[("who", who)]));
    let mut col = Flex::new(10, 10, 440, 200, None).column();
    col.set_pad(8);

    let msg = trf(
        "accounts.twitch_device.msg",
        &[("code", &user_code), ("url", &code_resp.verification_uri)],
    );
    let msg_frame = Frame::default()
        .with_label(&msg)
        .with_align(Align::Left | Align::Inside | Align::Wrap);
    col.fixed(&msg_frame, 140);

    let mut bar = Flex::default().row();
    bar.set_pad(8);
    let mut open_btn = button(&mut bar, tr("btn.open_browser"), 110);
    open_btn.set_callback({
        let uri = code_resp.verification_uri.clone();
        move |_| crate::oauth::open_browser(&uri)
    });
    let mut copy_btn = button(&mut bar, tr("btn.copy_code"), 90);
    copy_btn.set_callback({
        let code = user_code.clone();
        move |_| fltk::app::copy(&code)
    });
    Frame::default();
    let mut cancel_btn = button(&mut bar, tr("dialogs.cancel"), 80);
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
                                    e.twitch_start().connect();
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
                    super::dialogs::message(&trf("accounts.twitch_device.success", &[("who", who), ("name", &name)]));
                    break;
                }
                Err(e) => {
                    win.hide();
                    super::dialogs::alert(&trf("accounts.twitch_device.failed", &[("error", &e.to_string())]));
                    break;
                }
            }
        }
        fltk::app::wait();
    }
    cancelled.store(true, Ordering::Relaxed);
    win.hide();
}
