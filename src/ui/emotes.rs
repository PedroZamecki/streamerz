//! Emotes and emoji for Chat:
//! - Unicode emoji (as Twemoji's pictures, CC-BY 4.0)
//! - Platform-native emotes per account & channel (Twitch Helix `chat/emotes/user`, `chat/emotes` &
//!   `chat/emotes/global`; Kick `/emotes/{slug}`; seen YouTube & TikTok custom emotes)
//! - Third-party emotes per channel & global (7TV, BTTV, FFZ for Twitch; 7TV for Kick; 7TV & BTTV for
//!   YouTube), cached on disk for fast startup and refreshed automatically when accounts change.

use super::{tell, Msg};
use crate::engine::UiCmd;
use crate::Config;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Mutex, OnceLock};
use tokio::sync::mpsc::UnboundedSender;

/// Every fully-qualified emoji (Unicode's emoji-test.txt), one group per line: name, tab, emoji.
const TABLE: &str = include_str!("emoji.txt");
/// The longest emoji, in chars.
const LONGEST: usize = 10;

/// Bumped whenever background emote catalogs finish loading so an open picker refreshes live.
pub static REV: AtomicUsize = AtomicUsize::new(0);

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Provider {
    Favorites,
    Platform,
    SevenTv,
    Bttv,
    Ffz,
    Emoji,
}

impl Provider {
    pub fn label(self, platform: &str) -> &'static str {
        match self {
            Provider::Favorites => "Favorites",
            Provider::Platform => match platform {
                "twitch" => "Twitch",
                "kick" => "Kick",
                "youtube" => "YouTube",
                "tiktok" => "TikTok",
                _ => "Platform",
            },
            Provider::SevenTv => "7TV",
            Provider::Bttv => "BTTV",
            Provider::Ffz => "FFZ",
            Provider::Emoji => "Emoji",
        }
    }
}

/// Ordered provider tabs tailored to `platform`.
pub fn providers_for(platform: &str) -> &'static [Provider] {
    match platform {
        "twitch" => &[
            Provider::Favorites,
            Provider::Platform,
            Provider::SevenTv,
            Provider::Bttv,
            Provider::Ffz,
            Provider::Emoji,
        ],
        "kick" => &[
            Provider::Favorites,
            Provider::Platform,
            Provider::SevenTv,
            Provider::Emoji,
        ],
        "youtube" => &[
            Provider::Favorites,
            Provider::Platform,
            Provider::SevenTv,
            Provider::Bttv,
            Provider::Emoji,
        ],
        "tiktok" => &[Provider::Favorites, Provider::Platform, Provider::Emoji],
        _ => &[
            Provider::Favorites,
            Provider::SevenTv,
            Provider::Bttv,
            Provider::Ffz,
            Provider::Platform,
            Provider::Emoji,
        ],
    }
}

#[derive(Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct EmoteSection {
    pub title: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
    /// `(name, image_url)` pairs.
    pub emotes: Vec<(String, String)>,
}

#[derive(Clone, Default, Serialize, Deserialize)]
#[serde(default)]
struct Catalog {
    /// Twitch native emotes for `[broadcaster, bot]`.
    twitch_native: [Vec<EmoteSection>; 2],
    /// Twitch 7TV `[broadcaster, bot]` (includes personal/account set if different from channel, channel, global).
    twitch_7tv: [Vec<EmoteSection>; 2],
    twitch_bttv: Vec<EmoteSection>,
    twitch_ffz: Vec<EmoteSection>,
    /// Kick native channel/sub/global emotes.
    kick_native: Vec<EmoteSection>,
    /// Kick native `name -> [emote:id:name]` tags for outgoing messages.
    kick_tags: HashMap<String, String>,
    kick_7tv: Vec<EmoteSection>,
    youtube_7tv: Vec<EmoteSection>,
    youtube_bttv: Vec<EmoteSection>,
}

impl Catalog {
    /// Twitch refused the token for the account's emotes: not worth keeping, the next login fixes it.
    fn wants_login(&self) -> bool {
        self.twitch_native.iter().flatten().any(|s| s.title == MISSING_SCOPE)
    }
}

const MISSING_SCOPE: &str = "Personal & Subscriber Emotes";

struct Store {
    last_key: String,
    /// Fast lookup maps for chat line rendering: `[twitch, kick, youtube]`.
    lookup: [HashMap<String, String>; 3],
    kick_tags: HashMap<String, String>,
    cat: Catalog,
}

fn store() -> &'static Mutex<Store> {
    static S: OnceLock<Mutex<Store>> = OnceLock::new();
    S.get_or_init(|| {
        Mutex::new(Store {
            last_key: String::new(),
            lookup: [HashMap::new(), HashMap::new(), HashMap::new()],
            kick_tags: HashMap::new(),
            cat: Catalog::default(),
        })
    })
}

fn skin_tone(e: &str) -> bool {
    e.chars().any(|c| ('\u{1F3FB}'..='\u{1F3FF}').contains(&c))
}

/// The emoji groups for the picker (without the skin tone variants).
pub fn groups() -> impl Iterator<Item = (&'static str, Vec<&'static str>)> {
    TABLE
        .lines()
        .filter_map(|l| l.split_once('\t'))
        .map(|(g, es)| (g, es.split(' ').filter(|e| !skin_tone(e)).collect()))
}

/// Emoji by their text without U+FE0F (chat often leaves it out), to the fully-qualified one.
fn known() -> &'static HashMap<String, &'static str> {
    static K: OnceLock<HashMap<String, &'static str>> = OnceLock::new();
    K.get_or_init(|| {
        TABLE
            .lines()
            .filter_map(|l| l.split_once('\t'))
            .flat_map(|(_, es)| es.split(' '))
            .map(|e| (e.replace('\u{FE0F}', ""), e))
            .collect()
    })
}

/// `text` cut into emoji (with their picture's URL) and the text between them.
pub fn split(text: &str) -> Vec<(&str, Option<String>)> {
    let at: Vec<usize> = text.char_indices().map(|(i, _)| i).chain([text.len()]).collect();
    let (mut out, mut plain, mut k) = (Vec::new(), 0, 0);
    while k + 1 < at.len() {
        let hit = (1..=LONGEST.min(at.len() - 1 - k)).rev().find_map(|n| {
            let key = text[at[k]..at[k + n]].replace('\u{FE0F}', "");
            known().get(&key).map(|e| (n, *e))
        });
        match hit {
            Some((n, e)) => {
                if plain < at[k] {
                    out.push((&text[plain..at[k]], None));
                }
                out.push((&text[at[k]..at[k + n]], Some(twemoji(e))));
                k += n;
                plain = at[k];
            }
            None => k += 1,
        }
    }
    if plain < text.len() {
        out.push((&text[plain..], None));
    }
    out
}

/// Twemoji's picture of an emoji: its code points in hex, U+FE0F left out unless it's a ZWJ sequence.
pub fn twemoji(e: &str) -> String {
    let zwj = e.contains('\u{200D}');
    let name: Vec<String> = e
        .chars()
        .filter(|&c| zwj || c != '\u{FE0F}')
        .map(|c| format!("{:x}", c as u32))
        .collect();
    format!(
        "https://cdn.jsdelivr.net/gh/jdecked/twemoji@17.0.3/assets/72x72/{}.png",
        name.join("-")
    )
}

/// The third-party or catalog emote called `word` on `platform`, if loaded.
pub fn third(platform: &str, word: &str) -> Option<String> {
    let i = match platform {
        "twitch" => 0,
        "kick" => 1,
        "youtube" => 2,
        _ => return None,
    };
    store().lock().ok()?.lookup[i].get(word).cloned()
}

/// Kick's native `[emote:id:name]` tag for `word`, if loaded from Kick's emote catalog.
pub fn kick_tag(word: &str) -> Option<String> {
    store().lock().ok()?.kick_tags.get(word).cloned()
}

/// Sections for `(platform, provider, as_broadcaster)`, merged with `seen` emotes from live chat.
pub fn sections(
    platform: &str,
    provider: Provider,
    as_broadcaster: bool,
    seen: &[(String, String)],
    favorites: &[(String, String)],
) -> Vec<EmoteSection> {
    let slot = usize::from(!as_broadcaster);
    match provider {
        Provider::Favorites => vec![EmoteSection {
            title: "Favorite Emotes".into(),
            note: favorites
                .is_empty()
                .then(|| "Right-click any emote or emoji to ★ save it to your Favorites.".into()),
            emotes: favorites.to_vec(),
        }],
        Provider::Emoji => groups()
            .map(|(g, es)| EmoteSection {
                title: g.to_string(),
                note: None,
                emotes: es.into_iter().map(|e| (e.to_string(), twemoji(e))).collect(),
            })
            .collect(),
        Provider::Platform => {
            let Ok(st) = store().lock() else {
                return Vec::new();
            };
            let mut out = Vec::new();
            match platform {
                "twitch" => {
                    let acct_secs = &st.cat.twitch_native[slot];
                    let fallback_secs = &st.cat.twitch_native[0];
                    let base = if !acct_secs.is_empty() {
                        acct_secs
                    } else {
                        fallback_secs
                    };
                    if !seen.is_empty() {
                        out.push(EmoteSection {
                            title: "Recently Seen in Chat".into(),
                            note: None,
                            emotes: seen.to_vec(),
                        });
                    }
                    out.extend(base.iter().cloned());
                    if out.is_empty() {
                        out.push(EmoteSection {
                            title: "Twitch Emotes".into(),
                            note: Some("Loading Twitch emotes… (make sure an account is logged in)".into()),
                            emotes: Vec::new(),
                        });
                    }
                }
                "kick" => {
                    if !seen.is_empty() {
                        out.push(EmoteSection {
                            title: "Recently Seen in Chat".into(),
                            note: None,
                            emotes: seen.to_vec(),
                        });
                    }
                    for mut sec in st.cat.kick_native.iter().cloned() {
                        if !as_broadcaster && sec.title == "Kick Subscriber Emotes" && sec.note.is_none() {
                            sec.note =
                                Some("Sending as Bot: subscriber-only emotes require the bot to be subscribed.".into());
                        }
                        out.push(sec);
                    }
                    if out.is_empty() {
                        out.push(EmoteSection {
                            title: "Kick Emotes".into(),
                            note: Some("No Kick emotes loaded yet for this channel.".into()),
                            emotes: Vec::new(),
                        });
                    }
                }
                "youtube" => {
                    out.push(EmoteSection {
                        title: "Channel & Seen Custom Emoji".into(),
                        note: seen.is_empty().then(|| {
                            "Custom YouTube channel emoji appear here automatically as they are used in live chat."
                                .into()
                        }),
                        emotes: seen.to_vec(),
                    });
                }
                "tiktok" => {
                    out.push(EmoteSection {
                        title: "Seen TikTok Live Emotes".into(),
                        note: seen.is_empty().then(|| {
                            "Custom TikTok subscriber stickers/emotes appear here as they are used in the live room."
                                .into()
                        }),
                        emotes: seen.to_vec(),
                    });
                }
                _ => {
                    if !seen.is_empty() {
                        out.push(EmoteSection {
                            title: "Seen Across Connected Chats".into(),
                            note: None,
                            emotes: seen.to_vec(),
                        });
                    }
                    out.extend(st.cat.twitch_native[0].iter().cloned());
                    out.extend(st.cat.kick_native.iter().cloned());
                }
            }
            out
        }
        Provider::SevenTv => {
            let Ok(st) = store().lock() else {
                return Vec::new();
            };
            match platform {
                "kick" => st.cat.kick_7tv.clone(),
                "youtube" => st.cat.youtube_7tv.clone(),
                _ => {
                    let secs = &st.cat.twitch_7tv[slot];
                    if !secs.is_empty() {
                        secs.clone()
                    } else {
                        st.cat.twitch_7tv[0].clone()
                    }
                }
            }
        }
        Provider::Bttv => {
            let Ok(st) = store().lock() else {
                return Vec::new();
            };
            match platform {
                "youtube" => st.cat.youtube_bttv.clone(),
                _ => st.cat.twitch_bttv.clone(),
            }
        }
        Provider::Ffz => store().lock().map(|s| s.cat.twitch_ffz.clone()).unwrap_or_default(),
    }
}

fn rebuild_lookups(cat: &Catalog) -> [HashMap<String, String>; 3] {
    let mut tw = HashMap::new();
    let put = |m: &mut HashMap<String, String>, secs: &[EmoteSection]| {
        for s in secs {
            for (k, u) in &s.emotes {
                m.entry(k.clone()).or_insert_with(|| u.clone());
            }
        }
    };
    // Channel & third-party sets first so channel overrides take precedence over globals.
    put(&mut tw, &cat.twitch_7tv[0]);
    put(&mut tw, &cat.twitch_7tv[1]);
    put(&mut tw, &cat.twitch_bttv);
    put(&mut tw, &cat.twitch_ffz);
    put(&mut tw, &cat.twitch_native[0]);
    put(&mut tw, &cat.twitch_native[1]);

    let mut kick = HashMap::new();
    put(&mut kick, &cat.kick_7tv);
    put(&mut kick, &cat.kick_native);

    let mut yt = HashMap::new();
    put(&mut yt, &cat.youtube_7tv);
    put(&mut yt, &cat.youtube_bttv);

    [tw, kick, yt]
}

fn apply_catalog(cat: Catalog) {
    let lookup = rebuild_lookups(&cat);
    if let Ok(mut st) = store().lock() {
        st.lookup = lookup;
        st.kick_tags.clone_from(&cat.kick_tags);
        st.cat = cat;
    }
    REV.fetch_add(1, Ordering::Relaxed);
    super::wake(Msg::Image);
}

#[derive(Clone)]
struct LoadParams {
    cache_key: String,
    file_name: String,
    x: u8,
    tw_chan_id: String,
    tw_broadcaster: Option<(String, String)>,
    tw_bot: Option<(String, String)>,
    kick_slug: String,
    kick_uid: String,
    yt_chan_id: String,
}

impl LoadParams {
    fn from_cfg(cfg: &Config) -> Self {
        let x: u8 = if fltk::app::screen_scale(0) > 1.25 { 2 } else { 1 };
        let tw_broadcaster = cfg
            .twitch
            .broadcaster
            .as_ref()
            .map(|a| (a.user_id.clone(), a.token.access_token.clone()));
        let tw_bot = cfg
            .twitch
            .bot
            .as_ref()
            .map(|a| (a.user_id.clone(), a.token.access_token.clone()));
        let tw_chan_id = tw_broadcaster.as_ref().map(|(id, _)| id.clone()).unwrap_or_default();
        let kick_slug = crate::kick::clean_kick_channel(&cfg.kick.channel);
        let kick_uid = cfg
            .kick
            .channel_id
            .map(|id| id.to_string())
            .or_else(|| cfg.kick.broadcaster.as_ref().map(|a| a.user_id.to_string()))
            .unwrap_or_default();
        let yt_chan_id = crate::youtube::clean_youtube_channel(&cfg.youtube.channel_id);
        let b_id = tw_broadcaster.as_ref().map_or("", |(id, _)| id.as_str());
        // The tokens themselves (this key stays in memory): a new login loads again.
        let b_tok = tw_broadcaster.as_ref().map_or("", |(_, t)| t.as_str());
        let bot_id = tw_bot.as_ref().map_or("", |(id, _)| id.as_str());
        let bot_tok = tw_bot.as_ref().map_or("", |(_, t)| t.as_str());
        let cache_key =
            format!("{tw_chan_id}:{b_id}:{b_tok}:{bot_id}:{bot_tok}:{kick_slug}:{kick_uid}:{yt_chan_id}:{x}x");
        let file_id = format!("{b_id}-{bot_id}-{kick_slug}-{yt_chan_id}-{x}x")
            .chars()
            .map(|c| {
                if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                    c
                } else {
                    '_'
                }
            })
            .collect::<String>();
        Self {
            cache_key,
            file_name: format!("emote-catalog-{file_id}"),
            x,
            tw_chan_id,
            tw_broadcaster,
            tw_bot,
            kick_slug,
            kick_uid,
            yt_chan_id,
        }
    }
}

/// How long saved emote catalogs are considered fresh before refreshing in the background.
const FRESH: u64 = 12 * 60 * 60;

/// Ensure emote catalogs are loaded for the current configuration and logged-in accounts.
pub fn load_for_config(cmds: &UnboundedSender<UiCmd>, cfg: &tokio::sync::watch::Sender<Config>) {
    let p = LoadParams::from_cfg(&cfg.borrow());
    {
        let Ok(mut st) = store().lock() else { return };
        if st.last_key == p.cache_key {
            return;
        }
        st.last_key.clone_from(&p.cache_key);
    }
    let fresh = match super::img::saved(&p.file_name) {
        Some((age, v)) => match serde_json::from_value::<Catalog>(v) {
            Ok(cat) => {
                let has_data = !cat.twitch_7tv[0].is_empty()
                    || !cat.kick_7tv.is_empty()
                    || !cat.twitch_native[0].is_empty()
                    || !cat.kick_native.is_empty();
                let fresh = has_data && age < FRESH && !cat.wants_login();
                apply_catalog(cat);
                fresh
            }
            Err(_) => false,
        },
        None => false,
    };
    if fresh {
        return;
    }
    tell(cmds, move |_| {
        tokio::task::spawn_local(async move {
            let cat = fetch_all(&p).await;
            if let (false, Ok(val)) = (cat.wants_login(), serde_json::to_value(&cat)) {
                super::img::save(&p.file_name, &val);
            }
            apply_catalog(cat);
        });
    });
}

async fn get_json(url: &str, headers: &[(String, String)]) -> (u16, Value) {
    match crate::http::request("GET", url, headers, b"").await {
        Ok(r) => (r.status, serde_json::from_slice(&r.body).unwrap_or_default()),
        Err(_) => (0, Value::Null),
    }
}

fn parse_bttv_vec(list: &Value, x: u8) -> Vec<(String, String)> {
    let mut out = Vec::new();
    for e in list.as_array().into_iter().flatten() {
        if let (Some(code), Some(id)) = (e["code"].as_str(), e["id"].as_str()) {
            out.push((code.into(), format!("https://cdn.betterttv.net/emote/{id}/{x}x.webp")));
        }
    }
    out
}

fn parse_ffz_vec(sets: &Value, x: u8) -> Vec<(String, String)> {
    let mut out = Vec::new();
    for set in sets.as_object().into_iter().flat_map(|o| o.values()) {
        for e in set["emoticons"].as_array().into_iter().flatten() {
            let urls = if e["animated"].is_object() {
                &e["animated"]
            } else {
                &e["urls"]
            };
            let url = urls[x.to_string()].as_str().or(urls["1"].as_str());
            if let (Some(name), Some(url)) = (e["name"].as_str(), url) {
                let full = if url.starts_with("//") {
                    format!("https:{url}")
                } else {
                    url.to_string()
                };
                out.push((name.into(), full));
            }
        }
    }
    out
}

fn parse_7tv_vec(set: &Value, x: u8) -> Vec<(String, String)> {
    let mut out = Vec::new();
    for e in set["emotes"].as_array().into_iter().flatten() {
        if let (Some(name), Some(host)) = (e["name"].as_str(), e.pointer("/data/host/url").and_then(Value::as_str)) {
            let base = if host.starts_with("http") {
                host.to_string()
            } else {
                format!("https:{host}")
            };
            out.push((name.into(), format!("{base}/{x}x.webp")));
        }
    }
    out
}

fn twitch_emote_url(id: &str, x: u8) -> String {
    let scale = if x >= 2 { "2.0" } else { "1.0" };
    format!("https://static-cdn.jtvnw.net/emoticons/v2/{id}/default/dark/{scale}")
}

/// Fetch Twitch Helix emotes for a specific account (`user_id` + `token`), falling back to channel +
/// global emotes when `user:read:emotes` hasn't been granted yet.
async fn fetch_twitch_account_emotes(
    chan_id: &str,
    acct: Option<&(String, String)>,
    is_bot: bool,
    x: u8,
) -> Vec<EmoteSection> {
    let Some((uid, token)) = acct.filter(|(_, t)| !t.is_empty()) else {
        return if is_bot {
            vec![EmoteSection {
                title: "Bot Account Emotes".into(),
                note: Some("No Twitch bot account logged in — using channel & global emotes.".into()),
                emotes: Vec::new(),
            }]
        } else {
            Vec::new()
        };
    };
    let headers = vec![
        ("Client-Id".to_string(), crate::twitch::CLIENT_ID.to_string()),
        ("Authorization".to_string(), format!("Bearer {token}")),
    ];

    // 1. Try `GET chat/emotes/user` (all unlocked account + sub + follower + global emotes).
    let mut channel_emotes = Vec::new();
    let mut unlocked_emotes = Vec::new();
    let mut global_emotes = Vec::new();
    let mut cursor = String::new();
    let mut user_ep_ok = false;
    let mut missing_scope = false;
    for _ in 0..6 {
        let mut url = format!(
            "{}/chat/emotes/user?user_id={}",
            crate::twitch::HELIX_URL,
            crate::commands::url_encode(uid)
        );
        if !chan_id.is_empty() {
            url.push_str(&format!("&broadcaster_id={}", crate::commands::url_encode(chan_id)));
        }
        if !cursor.is_empty() {
            url.push_str(&format!("&after={}", crate::commands::url_encode(&cursor)));
        }
        let (status, body) = get_json(&url, &headers).await;
        if status == 401 || status == 403 {
            missing_scope = true;
            break;
        }
        if status != 200 {
            break;
        }
        user_ep_ok = true;
        for e in body["data"].as_array().into_iter().flatten() {
            let (Some(id), Some(name)) = (e["id"].as_str(), e["name"].as_str()) else {
                continue;
            };
            let img = twitch_emote_url(id, x);
            let kind = e["emote_type"].as_str().unwrap_or("");
            let owner = e["owner_id"].as_str().unwrap_or("");
            let target = if !chan_id.is_empty() && owner == chan_id {
                &mut channel_emotes
            } else if kind == "globals" || owner == "0" || owner == "twitch" {
                &mut global_emotes
            } else {
                &mut unlocked_emotes
            };
            if !target.iter().any(|(n, _): &(String, String)| n == name) {
                target.push((name.to_string(), img));
            }
        }
        match body.pointer("/pagination/cursor").and_then(Value::as_str) {
            Some(c) if !c.is_empty() => cursor = c.to_string(),
            _ => break,
        }
    }

    if user_ep_ok {
        let mut secs = Vec::new();
        if !channel_emotes.is_empty() {
            secs.push(EmoteSection {
                title: "Channel Emotes".into(),
                note: None,
                emotes: channel_emotes,
            });
        }
        secs.push(EmoteSection {
            title: if is_bot {
                "Bot Unlocked & Subscriber Emotes".into()
            } else {
                "Your Unlocked & Subscriber Emotes".into()
            },
            note: unlocked_emotes
                .is_empty()
                .then(|| "No additional subscriber or Prime/Turbo emotes unlocked on this account.".into()),
            emotes: unlocked_emotes,
        });
        if !global_emotes.is_empty() {
            secs.push(EmoteSection {
                title: "Twitch Global Emotes".into(),
                note: None,
                emotes: global_emotes,
            });
        }
        return secs;
    }

    // 2. Fallback: `chat/emotes` (channel) + `chat/emotes/global` (works with any valid Twitch token).
    if !chan_id.is_empty() {
        let url = format!(
            "{}/chat/emotes?broadcaster_id={}",
            crate::twitch::HELIX_URL,
            crate::commands::url_encode(chan_id)
        );
        let (_, body) = get_json(&url, &headers).await;
        for e in body["data"].as_array().into_iter().flatten() {
            if let (Some(id), Some(name)) = (e["id"].as_str(), e["name"].as_str()) {
                channel_emotes.push((name.to_string(), twitch_emote_url(id, x)));
            }
        }
    }
    let (_, gbody) = get_json(&format!("{}/chat/emotes/global", crate::twitch::HELIX_URL), &headers).await;
    for e in gbody["data"].as_array().into_iter().flatten() {
        if let (Some(id), Some(name)) = (e["id"].as_str(), e["name"].as_str()) {
            global_emotes.push((name.to_string(), twitch_emote_url(id, x)));
        }
    }
    let mut secs = Vec::new();
    if missing_scope {
        secs.push(EmoteSection {
            title: MISSING_SCOPE.into(),
            note: Some(
                "Log in again on the Accounts tab to grant `user:read:emotes` and load all your personal & sub emotes."
                    .into(),
            ),
            emotes: Vec::new(),
        });
    }
    if !channel_emotes.is_empty() {
        secs.push(EmoteSection {
            title: "Channel Emotes".into(),
            note: None,
            emotes: channel_emotes,
        });
    }
    if !global_emotes.is_empty() {
        secs.push(EmoteSection {
            title: "Twitch Global Emotes".into(),
            note: None,
            emotes: global_emotes,
        });
    }
    secs
}

/// Parse Kick `/emotes/{slug}` payload into `(sections, name -> [emote:id:name] map)`.
pub fn parse_kick_emotes_payload(v: &Value) -> (Vec<EmoteSection>, HashMap<String, String>) {
    let mut chan_free = Vec::new();
    let mut chan_sub = Vec::new();
    let mut globals = Vec::new();
    let mut tags = HashMap::new();
    for set in v.as_array().into_iter().flatten() {
        let is_global = set["id"]
            .as_str()
            .is_some_and(|s| s.eq_ignore_ascii_case("global") || s.eq_ignore_ascii_case("emojis"))
            || set["name"]
                .as_str()
                .is_some_and(|s| s.eq_ignore_ascii_case("global") || s.eq_ignore_ascii_case("emojis"));
        for e in set["emotes"].as_array().into_iter().flatten() {
            let id = e["id"]
                .as_u64()
                .map(|n| n.to_string())
                .or_else(|| e["id"].as_str().map(String::from))
                .unwrap_or_default();
            let Some(name) = e["name"].as_str().filter(|s| !s.is_empty()) else {
                continue;
            };
            if id.is_empty() {
                continue;
            }
            let url = format!("https://files.kick.com/emotes/{id}/fullsize");
            tags.entry(name.to_string())
                .or_insert_with(|| format!("[emote:{id}:{name}]"));
            let sub_only = e["subscribers_only"].as_bool().unwrap_or(false);
            let dst = if is_global {
                &mut globals
            } else if sub_only {
                &mut chan_sub
            } else {
                &mut chan_free
            };
            if !dst.iter().any(|(n, _): &(String, String)| n == name) {
                dst.push((name.to_string(), url));
            }
        }
    }
    let mut secs = Vec::new();
    if !chan_free.is_empty() {
        secs.push(EmoteSection {
            title: "Kick Channel Emotes".into(),
            note: None,
            emotes: chan_free,
        });
    }
    if !chan_sub.is_empty() {
        secs.push(EmoteSection {
            title: "Kick Subscriber Emotes".into(),
            note: None,
            emotes: chan_sub,
        });
    }
    if !globals.is_empty() {
        secs.push(EmoteSection {
            title: "Kick Global Emotes".into(),
            note: None,
            emotes: globals,
        });
    }
    (secs, tags)
}

async fn fetch_kick_native(slug: &str) -> (Vec<EmoteSection>, HashMap<String, String>) {
    if slug.is_empty() {
        return (Vec::new(), HashMap::new());
    }
    let headers = [
        ("Accept".to_string(), "application/json".to_string()),
        (
            "User-Agent".to_string(),
            "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 Chrome/131.0.0.0 Safari/537.36".to_string(),
        ),
    ];
    let url = format!("https://kick.com/emotes/{}", crate::commands::url_encode(slug));
    let (status, body) = get_json(&url, &headers).await;
    if status == 200 {
        parse_kick_emotes_payload(&body)
    } else {
        (Vec::new(), HashMap::new())
    }
}

async fn fetch_7tv_user_set(platform: &str, uid: &str, x: u8) -> (String, Vec<(String, String)>) {
    if uid.is_empty() {
        return (String::new(), Vec::new());
    }
    let url = format!(
        "https://7tv.io/v3/users/{platform}/{}",
        crate::commands::url_encode(uid)
    );
    let (_, v) = get_json(&url, &[]).await;
    let set_name = v
        .pointer("/emote_set/name")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    (set_name, parse_7tv_vec(&v["emote_set"], x))
}

async fn fetch_all(p: &LoadParams) -> Catalog {
    let x = p.x;
    let (
        (_, ffz_global_v),
        (_, bttv_global_v),
        (_, seven_global_v),
        tw_broad_native,
        tw_bot_native,
        (kick_native, kick_tags),
    ) = tokio::join!(
        get_json("https://api.frankerfacez.com/v1/set/global", &[]),
        get_json("https://api.betterttv.net/3/cached/emotes/global", &[]),
        get_json("https://7tv.io/v3/emote-sets/global", &[]),
        fetch_twitch_account_emotes(&p.tw_chan_id, p.tw_broadcaster.as_ref(), false, x),
        fetch_twitch_account_emotes(&p.tw_chan_id, p.tw_bot.as_ref(), true, x),
        fetch_kick_native(&p.kick_slug),
    );

    let ffz_global = parse_ffz_vec(&ffz_global_v["sets"], x);
    let bttv_global = parse_bttv_vec(&bttv_global_v, x);
    let seven_global_title = seven_global_v["name"]
        .as_str()
        .filter(|s| !s.is_empty())
        .unwrap_or("7TV Global Emotes")
        .to_string();
    let seven_global = parse_7tv_vec(&seven_global_v, x);

    // Twitch channel 7TV / BTTV / FFZ
    let mut twitch_bttv = Vec::new();
    let mut twitch_ffz = Vec::new();
    let mut tw_chan_7tv = Vec::new();
    if !p.tw_chan_id.is_empty() {
        let ffz_url = format!("https://api.frankerfacez.com/v1/room/id/{}", p.tw_chan_id);
        let bttv_url = format!("https://api.betterttv.net/3/cached/users/twitch/{}", p.tw_chan_id);
        let ((_, ffz_room), (_, bttv_room), (_, s7_chan)) = tokio::join!(
            get_json(&ffz_url, &[]),
            get_json(&bttv_url, &[]),
            fetch_7tv_user_set("twitch", &p.tw_chan_id, x),
        );
        tw_chan_7tv = s7_chan;
        let ffz_ch = parse_ffz_vec(&ffz_room["sets"], x);
        if !ffz_ch.is_empty() {
            twitch_ffz.push(EmoteSection {
                title: "Channel FFZ Emotes".into(),
                note: None,
                emotes: ffz_ch,
            });
        }
        let mut bttv_ch = parse_bttv_vec(&bttv_room["channelEmotes"], x);
        bttv_ch.extend(parse_bttv_vec(&bttv_room["sharedEmotes"], x));
        if !bttv_ch.is_empty() {
            twitch_bttv.push(EmoteSection {
                title: "Channel & Shared BTTV Emotes".into(),
                note: None,
                emotes: bttv_ch,
            });
        }
    }
    if !bttv_global.is_empty() {
        twitch_bttv.push(EmoteSection {
            title: "Global BTTV Emotes".into(),
            note: None,
            emotes: bttv_global.clone(),
        });
    }
    if !ffz_global.is_empty() {
        twitch_ffz.push(EmoteSection {
            title: "Global FFZ Emotes".into(),
            note: None,
            emotes: ffz_global,
        });
    }

    let build_tw_7tv = |personal: Option<Vec<(String, String)>>| -> Vec<EmoteSection> {
        let mut secs = Vec::new();
        if let Some(p_emotes) = personal {
            if !p_emotes.is_empty() {
                secs.push(EmoteSection {
                    title: "Account 7TV Emotes".into(),
                    note: None,
                    emotes: p_emotes,
                });
            }
        }
        secs.push(EmoteSection {
            title: "Channel 7TV Emotes".into(),
            note: tw_chan_7tv
                .is_empty()
                .then(|| "No 7TV channel emote set configured for this channel.".into()),
            emotes: tw_chan_7tv.clone(),
        });
        if !seven_global.is_empty() {
            secs.push(EmoteSection {
                title: seven_global_title.clone(),
                note: None,
                emotes: seven_global.clone(),
            });
        }
        secs
    };

    let bot_7tv_extra = match p.tw_bot.as_ref().map(|(id, _)| id.as_str()) {
        Some(bot_id) if !bot_id.is_empty() && bot_id != p.tw_chan_id => {
            Some(fetch_7tv_user_set("twitch", bot_id, x).await.1)
        }
        _ => None,
    };
    let twitch_7tv = [build_tw_7tv(None), build_tw_7tv(bot_7tv_extra)];

    // Kick 7TV
    let mut kick_7tv = Vec::new();
    if !p.kick_uid.is_empty() {
        let (_, k7) = fetch_7tv_user_set("kick", &p.kick_uid, x).await;
        kick_7tv.push(EmoteSection {
            title: "Channel 7TV Emotes".into(),
            note: k7
                .is_empty()
                .then(|| "No 7TV emote set linked to this Kick channel.".into()),
            emotes: k7,
        });
    }
    if !seven_global.is_empty() {
        kick_7tv.push(EmoteSection {
            title: seven_global_title.clone(),
            note: None,
            emotes: seven_global.clone(),
        });
    }

    // YouTube 7TV + BTTV
    let mut youtube_7tv = Vec::new();
    let mut youtube_bttv = Vec::new();
    if p.yt_chan_id.starts_with("UC") {
        let yb_url = format!("https://api.betterttv.net/3/cached/users/youtube/{}", p.yt_chan_id);
        let ((_, y7), (_, yb)) = tokio::join!(fetch_7tv_user_set("youtube", &p.yt_chan_id, x), get_json(&yb_url, &[]),);
        if !y7.is_empty() {
            youtube_7tv.push(EmoteSection {
                title: "Channel 7TV Emotes".into(),
                note: None,
                emotes: y7,
            });
        }
        let mut yb_ch = parse_bttv_vec(&yb["channelEmotes"], x);
        yb_ch.extend(parse_bttv_vec(&yb["sharedEmotes"], x));
        if !yb_ch.is_empty() {
            youtube_bttv.push(EmoteSection {
                title: "Channel & Shared BTTV Emotes".into(),
                note: None,
                emotes: yb_ch,
            });
        }
    }
    if !seven_global.is_empty() {
        youtube_7tv.push(EmoteSection {
            title: seven_global_title,
            note: None,
            emotes: seven_global,
        });
    }
    if !bttv_global.is_empty() {
        youtube_bttv.push(EmoteSection {
            title: "Global BTTV Emotes".into(),
            note: None,
            emotes: bttv_global,
        });
    }

    let _ = json!({}); // keep json import used cleanly
    Catalog {
        twitch_native: [tw_broad_native, tw_bot_native],
        twitch_7tv,
        twitch_bttv,
        twitch_ffz,
        kick_native,
        kick_tags,
        kick_7tv,
        youtube_7tv,
        youtube_bttv,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splits_emoji_out_of_text() {
        let s = split("hi😀 ❤ ok👍🏽!");
        let shown: Vec<(&str, bool)> = s.iter().map(|(t, u)| (*t, u.is_some())).collect();
        assert_eq!(
            shown,
            [
                ("hi", false),
                ("😀", true),
                (" ", false),
                ("❤", true),
                (" ok", false),
                ("👍🏽", true),
                ("!", false)
            ]
        );
        assert!(s[3].1.as_deref().unwrap().ends_with("/2764.png"));
        assert!(twemoji("👨‍👩‍👧").ends_with("/1f468-200d-1f469-200d-1f467.png"));
        assert!(split("plain text").iter().all(|x| x.1.is_none()));
        assert!(groups().any(|(g, es)| g == "Smileys & Emotion" && es.contains(&"😀")));
    }

    #[test]
    fn parses_kick_channel_and_global_emotes() {
        let payload = json!([
            {
                "id": "channel_1",
                "name": "Channel",
                "emotes": [
                    { "id": 1001, "name": "chanFree", "subscribers_only": false },
                    { "id": 1002, "name": "chanSub", "subscribers_only": true }
                ]
            },
            {
                "id": "Global",
                "name": "Global",
                "emotes": [
                    { "id": 37226, "name": "KEKW", "subscribers_only": false }
                ]
            }
        ]);
        let (secs, tags) = parse_kick_emotes_payload(&payload);
        assert_eq!(secs.len(), 3);
        assert_eq!(tags.get("KEKW").map(String::as_str), Some("[emote:37226:KEKW]"));
        assert_eq!(tags.get("chanSub").map(String::as_str), Some("[emote:1002:chanSub]"));
    }
}
