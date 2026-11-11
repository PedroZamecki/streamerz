//! Emotes and emoji for Chat:
//! - Unicode emoji (as Twemoji's pictures, CC-BY 4.0)
//! - Platform-native emotes per account & channel (Twitch Helix `chat/emotes/user`, `chat/emotes` &
//!   `chat/emotes/global`; Kick `/emotes/{slug}`; seen YouTube & TikTok custom emotes)
//! - Third-party emotes per channel & global (7TV, BTTV, FFZ for Twitch; 7TV for Kick; 7TV & BTTV for
//!   YouTube), cached on disk for fast startup and refreshed automatically when accounts change.

use crate::core::submit as tell;
use crate::i18n::{tr, trf};
use crate::Config;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Mutex, OnceLock};

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
            Provider::Favorites => tr("emotes.favorites"),
            Provider::Platform => match platform {
                "twitch" => "Twitch",
                "kick" => "Kick",
                "youtube" => "YouTube",
                "tiktok" => "TikTok",
                _ => tr("emotes.platform"),
            },
            Provider::SevenTv => "7TV",
            Provider::Bttv => "BTTV",
            Provider::Ffz => "FFZ",
            Provider::Emoji => tr("emotes.emoji"),
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
        "youtube" => &[Provider::Favorites, Provider::SevenTv, Provider::Bttv, Provider::Emoji],
        "tiktok" => &[Provider::Favorites, Provider::Emoji],
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
        self.twitch_native
            .iter()
            .flatten()
            .any(|s| s.title == tr("emotes.missing_scope"))
    }
}

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
    if crate::memory_saver() {
        return vec![(text, None)];
    }
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
    let s = store().lock().ok()?;
    if platform == "all" {
        return s.lookup[0]
            .get(word)
            .or_else(|| s.lookup[1].get(word))
            .or_else(|| s.lookup[2].get(word))
            .cloned();
    }
    let i = match platform {
        "twitch" => 0,
        "kick" => 1,
        "youtube" => 2,
        _ => return None,
    };
    s.lookup[i].get(word).cloned()
}

/// What a chatter's own 7TV emotes are: `Some(name -> url)` once looked up, `None` while it's asked.
type Personal = Option<HashMap<String, String>>;

/// Chatters looked up before the oldest are forgotten (their sets are small, but chats are long).
const PERSONAL_MAX: usize = 400;

fn personal_store() -> &'static Mutex<HashMap<String, Personal>> {
    static S: OnceLock<Mutex<HashMap<String, Personal>>> = OnceLock::new();
    S.get_or_init(Mutex::default)
}

/// The emote called `word` in the personal 7TV set of chatter `uid` on `platform`, if it's known.
pub fn personal(platform: &str, uid: &str, word: &str) -> Option<String> {
    personal_store()
        .lock()
        .ok()?
        .get(&format!("{platform}:{uid}"))?
        .as_ref()?
        .get(word)
        .cloned()
}

/// Looks up the personal 7TV set of a chatter, once: their own emotes, which everyone with 7TV sees in
/// their messages. Draws again when it arrives.
pub fn want_personal(platform: &str, uid: &str) {
    let plat = match platform {
        "twitch" => "TWITCH",
        "kick" => "KICK",
        "youtube" => "YOUTUBE",
        _ => return,
    };
    let Some(cmds) = crate::engine::commands() else { return };
    if uid.is_empty() || crate::memory_saver() {
        return;
    }
    let key = format!("{platform}:{uid}");
    {
        let Ok(mut m) = personal_store().lock() else { return };
        if m.contains_key(&key) {
            return;
        }
        if m.len() >= PERSONAL_MAX {
            m.clear();
        }
        m.insert(key.clone(), None);
    }
    let x = crate::host::image_scale();
    let uid = uid.to_string();
    tell(cmds, move |_| {
        tokio::task::spawn_local(async move {
            let set = fetch_7tv_personal(plat, &uid, x).await;
            if let Ok(mut m) = personal_store().lock() {
                m.insert(key, Some(set.into_iter().collect()));
            }
            REV.fetch_add(1, Ordering::Relaxed);
            crate::host::images_changed();
        });
    });
}

/// 7TV's personal emote set of a chatter (only its GraphQL API has it).
async fn fetch_7tv_personal(platform: &str, uid: &str, x: u8) -> Vec<(String, String)> {
    let v = gql_7tv(&format!(
        "{{ users {{ userByConnection(platform: {platform}, platformId: {}) {{ personalEmoteSet {{ emotes(perPage: 1000) {{ items {{ alias emote {{ id defaultName }} }} }} }} }} }} }}",
        json!(uid)
    ))
    .await;
    parse_7tv_personal(&v, x)
}

fn parse_7tv_personal(v: &Value, x: u8) -> Vec<(String, String)> {
    parse_7tv_items(
        &v.pointer("/data/users/userByConnection/personalEmoteSet/emotes/items")
            .cloned()
            .unwrap_or_default(),
        x,
    )
}

/// Kick's native `[emote:id:name]` tag for `word`, if loaded from Kick's emote catalog.
pub fn kick_tag(word: &str) -> Option<String> {
    store().lock().ok()?.kick_tags.get(word).cloned()
}

/// Remember a Kick native emote tag dynamically (from chat messages or API).
#[allow(dead_code)]
pub fn remember_kick_tag(name: &str, id: &str) {
    if id.is_empty() || name.is_empty() {
        return;
    }
    if let Ok(mut st) = store().lock() {
        let tag = format!("[emote:{id}:{name}]");
        let url = format!("https://files.kick.com/emotes/{id}/fullsize");
        st.kick_tags.entry(name.to_string()).or_insert(tag);
        st.lookup[1].entry(name.to_string()).or_insert(url);
    }
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
            title: tr("emotes.favorite_emotes").into(),
            note: favorites.is_empty().then(|| tr("emotes.favorites_hint").into()),
            emotes: favorites
                .iter()
                .filter(|(name, url)| {
                    (!matches!(platform, "youtube" | "all") || !youtube_image(url)) && !name.starts_with("[tiktok:")
                })
                .cloned()
                .collect(),
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
                            title: tr("emotes.seen_in_chat").into(),
                            note: None,
                            emotes: seen.to_vec(),
                        });
                    }
                    out.extend(base.iter().cloned());
                    if out.is_empty() {
                        out.push(EmoteSection {
                            title: tr("emotes.twitch_emotes").into(),
                            note: Some(tr("emotes.twitch_loading").into()),
                            emotes: Vec::new(),
                        });
                    }
                }
                "kick" => {
                    if !seen.is_empty() {
                        out.push(EmoteSection {
                            title: tr("emotes.seen_in_chat").into(),
                            note: None,
                            emotes: seen.to_vec(),
                        });
                    }
                    for mut sec in st.cat.kick_native.iter().cloned() {
                        if !as_broadcaster && sec.title == tr("emotes.kick_subscriber_emotes") && sec.note.is_none() {
                            sec.note = Some(tr("emotes.kick_bot_sub_warning").into());
                        }
                        out.push(sec);
                    }
                    if out.is_empty() {
                        out.push(EmoteSection {
                            title: tr("emotes.kick_emotes").into(),
                            note: Some(tr("emotes.kick_none_loaded").into()),
                            emotes: Vec::new(),
                        });
                    }
                }
                "youtube" => {
                    out.push(EmoteSection {
                        title: tr("emotes.channel_and_seen").into(),
                        note: seen.is_empty().then(|| tr("emotes.youtube_seen_hint").into()),
                        emotes: seen.to_vec(),
                    });
                }

                _ => {
                    if !seen.is_empty() {
                        out.push(EmoteSection {
                            title: tr("emotes.seen_across_chats").into(),
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

// YouTube's native emoji cannot be sent through the text-only Data API.
fn youtube_image(url: &str) -> bool {
    url.split('/')
        .nth(2)
        .is_some_and(|host| host == "ggpht.com" || host.ends_with(".ggpht.com"))
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

/// Pictures of each platform's own emotes put on disk ahead of the list (two screens of them), so it
/// opens with them drawn. Files only: nothing is decoded until the list shows it.
const PREFETCH: usize = 96;

fn prefetch(cat: &Catalog) {
    for sections in [&cat.twitch_native[0], &cat.twitch_native[1], &cat.kick_native] {
        for (_, url) in sections.iter().flat_map(|section| &section.emotes).take(PREFETCH) {
            crate::core::images::lookup(url);
        }
    }
}

fn apply_catalog(cat: Catalog) {
    prefetch(&cat);
    let lookup = rebuild_lookups(&cat);
    if let Ok(mut st) = store().lock() {
        st.lookup = lookup;
        st.kick_tags.clone_from(&cat.kick_tags);
        st.cat = cat;
    }
    REV.fetch_add(1, Ordering::Relaxed);
    crate::host::images_changed();
}

#[derive(Clone)]
#[allow(dead_code)]
struct LoadParams {
    cache_key: String,
    file_name: String,
    x: u8,
    tw_chan_id: String,
    tw_broadcaster: Option<(String, String)>,
    tw_bot: Option<(String, String)>,
    kick_slug: String,
    kick_uid: String,
    kick_user_slug: Option<String>,
    kick_token: Option<String>,
    yt_chan_id: String,
}

/// The channel read, or when none is set, the logged-in broadcaster's own.
fn kick_slug(cfg: &Config) -> String {
    let slug = crate::kick::clean_kick_channel(&cfg.kick.channel).to_lowercase();
    if !slug.is_empty() {
        return slug;
    }
    cfg.kick
        .broadcaster
        .as_ref()
        .map(|b| b.username.to_lowercase())
        .unwrap_or_default()
}

impl LoadParams {
    fn from_cfg(cfg: &Config) -> Self {
        let x = crate::host::image_scale();
        Self::with_scale(cfg, x)
    }

    fn with_scale(cfg: &Config, x: u8) -> Self {
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
        // The channel read (someone else's when one is set), not the logged-in account's: its 7TV, BTTV
        // and FFZ sets are the ones chat uses.
        let tw_chan_id = crate::twitch::channel_id(cfg);
        let kick_slug = kick_slug(cfg);
        let kick_user_slug = cfg
            .kick
            .broadcaster
            .as_ref()
            .map(|b| b.username.to_lowercase())
            .filter(|s| !s.is_empty());
        let kick_token = cfg
            .kick
            .broadcaster
            .as_ref()
            .map(|b| b.token.access_token.clone())
            .or_else(|| cfg.kick.bot.as_ref().map(|b| b.token.access_token.clone()))
            .filter(|t| !t.is_empty());
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
        let k_user = kick_user_slug.as_deref().unwrap_or("");
        let k_has_tok = kick_token.is_some();
        let cache_key =
            format!("{tw_chan_id}:{b_id}:{b_tok}:{bot_id}:{bot_tok}:{kick_slug}:{kick_uid}:{k_user}:{k_has_tok}:{yt_chan_id}:{x}x");
        let file_id = format!("{tw_chan_id}-{b_id}-{bot_id}-{kick_slug}-{k_user}-{yt_chan_id}-{x}x")
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
            kick_user_slug,
            kick_token,
            yt_chan_id,
        }
    }
}

/// How long saved emote catalogs are considered fresh before refreshing in the background.
const FRESH: u64 = 12 * 60 * 60;

/// Like `load_for_config`, for a caller that already runs on the engine's local set (the core).
pub fn load_local(config: &Config) {
    if let Some(p) = stale_params(config) {
        tokio::task::spawn_local(fetch_and_keep(p));
    }
}

async fn fetch_and_keep(p: LoadParams) {
    let cat = fetch_all(&p).await;
    // A catalog the network left empty isn't kept as fresh.
    let loaded = cat.twitch_7tv[0].iter().any(|s| !s.emotes.is_empty());
    if let (true, false, Ok(val)) = (loaded, cat.wants_login(), serde_json::to_value(&cat)) {
        crate::cache::save(&p.file_name, &val);
    }
    apply_catalog(cat);
}

/// What to fetch for `config`, or `None` when the catalog in use is the right one and fresh enough.
/// A catalog saved on disk is applied meanwhile.
fn stale_params(config: &Config) -> Option<LoadParams> {
    if crate::memory_saver() || !config.window.chat.save_history {
        return None;
    }
    let p = LoadParams::from_cfg(config);
    {
        let Ok(mut st) = store().lock() else { return None };
        if st.last_key == p.cache_key {
            return None;
        }
        st.last_key.clone_from(&p.cache_key);
    }
    let fresh = match crate::cache::saved(&p.file_name) {
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
    (!fresh).then_some(p)
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
/// The display names of the channels that own emotes (Helix `users`, up to 100 a time).
async fn owner_names(
    by_owner: &std::collections::BTreeMap<String, Vec<(String, String)>>,
    headers: &[(String, String)],
) -> HashMap<String, String> {
    let ids: Vec<&String> = by_owner.keys().collect();
    let mut names = HashMap::new();
    for chunk in ids.chunks(100) {
        let q: Vec<String> = chunk
            .iter()
            .map(|id| format!("id={}", crate::commands::url_encode(id)))
            .collect();
        let url = format!("{}/users?{}", crate::twitch::HELIX_URL, q.join("&"));
        let (_, body) = get_json(&url, headers).await;
        for u in body["data"].as_array().into_iter().flatten() {
            if let (Some(id), Some(name)) = (u["id"].as_str(), u["display_name"].as_str()) {
                names.insert(id.to_string(), name.to_string());
            }
        }
    }
    names
}

async fn fetch_twitch_account_emotes(
    chan_id: &str,
    acct: Option<&(String, String)>,
    is_bot: bool,
    x: u8,
) -> Vec<EmoteSection> {
    let Some((uid, token)) = acct.filter(|(_, t)| !t.is_empty()) else {
        return if is_bot {
            vec![EmoteSection {
                title: tr("emotes.bot_emotes").into(),
                note: Some(tr("emotes.no_twitch_bot").into()),
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
    // The unlocked ones of each channel that owns them (subscriptions, follower emotes...).
    let mut by_owner: std::collections::BTreeMap<String, Vec<(String, String)>> = Default::default();
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
            } else if !owner.is_empty() {
                by_owner.entry(owner.to_string()).or_default()
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
                title: tr("emotes.channel_emotes").into(),
                note: None,
                emotes: channel_emotes,
            });
        }
        // One section per channel the emotes come from, named by the channel.
        let names = owner_names(&by_owner, &headers).await;
        let mut owners: Vec<(String, Vec<(String, String)>)> = by_owner
            .into_iter()
            .map(|(id, e)| (names.get(&id).cloned().unwrap_or(id), e))
            .collect();
        owners.sort_by_key(|o| o.0.to_lowercase());
        for (name, emotes) in owners {
            secs.push(EmoteSection {
                title: trf("emotes.from_channel", &[("channel", &name)]),
                note: None,
                emotes,
            });
        }
        if !unlocked_emotes.is_empty() {
            secs.push(EmoteSection {
                title: if is_bot {
                    tr("emotes.bot_unlocked_emotes").into()
                } else {
                    tr("emotes.user_unlocked_emotes").into()
                },
                note: None,
                emotes: unlocked_emotes,
            });
        } else if secs.is_empty() {
            secs.push(EmoteSection {
                title: tr("emotes.user_unlocked_emotes").into(),
                note: Some(tr("emotes.no_unlocked_emotes").into()),
                emotes: Vec::new(),
            });
        }
        if !global_emotes.is_empty() {
            secs.push(EmoteSection {
                title: tr("emotes.twitch_global_emotes").into(),
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
            title: tr("emotes.missing_scope").into(),
            note: Some(tr("emotes.missing_scope_note").into()),
            emotes: Vec::new(),
        });
    }
    if !channel_emotes.is_empty() {
        secs.push(EmoteSection {
            title: tr("emotes.channel_emotes").into(),
            note: None,
            emotes: channel_emotes,
        });
    }
    if !global_emotes.is_empty() {
        secs.push(EmoteSection {
            title: tr("emotes.twitch_global_emotes").into(),
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
    let mut collectibles = Vec::new();
    let mut user_unlocked = Vec::new();
    let mut globals = Vec::new();
    let mut tags = HashMap::new();
    for set in v.as_array().into_iter().flatten() {
        let id_str = set["id"].as_str().unwrap_or_default();
        let name_str = set["name"].as_str().unwrap_or_default();
        let is_global = id_str.eq_ignore_ascii_case("global")
            || id_str.eq_ignore_ascii_case("emojis")
            || name_str.eq_ignore_ascii_case("global")
            || name_str.eq_ignore_ascii_case("emojis");
        let is_collectible_set =
            id_str.to_lowercase().contains("collectible") || name_str.to_lowercase().contains("collectible");
        let is_user_set = id_str.eq_ignore_ascii_case("user")
            || id_str.eq_ignore_ascii_case("unlocked")
            || name_str.to_lowercase().contains("user");

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
            } else if is_collectible_set || name.to_lowercase().starts_with("collectibles") {
                &mut collectibles
            } else if is_user_set {
                &mut user_unlocked
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
            title: tr("emotes.kick_channel_emotes").into(),
            note: None,
            emotes: chan_free,
        });
    }
    if !collectibles.is_empty() {
        secs.push(EmoteSection {
            title: tr("emotes.kick_collectibles").into(),
            note: None,
            emotes: collectibles,
        });
    }
    if !chan_sub.is_empty() {
        secs.push(EmoteSection {
            title: tr("emotes.kick_subscriber_emotes").into(),
            note: None,
            emotes: chan_sub,
        });
    }
    if !user_unlocked.is_empty() {
        secs.push(EmoteSection {
            title: tr("emotes.user_unlocked_emotes").into(),
            note: None,
            emotes: user_unlocked,
        });
    }
    if !globals.is_empty() {
        secs.push(EmoteSection {
            title: tr("emotes.kick_global_emotes").into(),
            note: None,
            emotes: globals,
        });
    }
    (secs, tags)
}

async fn fetch_kick_native(
    slug: &str,
    user_slug: Option<&str>,
    token: Option<&str>,
) -> (Vec<EmoteSection>, HashMap<String, String>) {
    let mut headers = vec![
        ("Accept".to_string(), "application/json".to_string()),
        (
            "User-Agent".to_string(),
            "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 Chrome/131.0.0.0 Safari/537.36".to_string(),
        ),
    ];
    if let Some(tok) = token.filter(|t| !t.is_empty()) {
        headers.push(("Authorization".to_string(), format!("Bearer {tok}")));
    }

    let mut all_secs: Vec<EmoteSection> = Vec::new();
    let mut all_tags: HashMap<String, String> = HashMap::new();

    let mut merge = |(new_secs, new_tags): (Vec<EmoteSection>, HashMap<String, String>)| {
        all_tags.extend(new_tags);
        for s in new_secs {
            if let Some(existing) = all_secs.iter_mut().find(|e| e.title == s.title) {
                for (name, url) in s.emotes {
                    if !existing.emotes.iter().any(|(n, _)| n == &name) {
                        existing.emotes.push((name, url));
                    }
                }
            } else {
                all_secs.push(s);
            }
        }
    };

    // 1. If user is authenticated, query /emotes to get user unlocked sets / collectibles
    if token.is_some() {
        let (status, body) = get_json("https://kick.com/emotes", &headers).await;
        if status == 200 {
            merge(parse_kick_emotes_payload(&body));
        }
    }

    // 2. Fetch channel emotes for the channel slug (or user_slug if slug is empty)
    let target_slug = if !slug.is_empty() {
        slug
    } else {
        user_slug.unwrap_or_default()
    };

    if !target_slug.is_empty() {
        let url = format!("https://kick.com/emotes/{}", crate::commands::url_encode(target_slug));
        let (status, body) = get_json(&url, &headers).await;
        if status == 200 {
            merge(parse_kick_emotes_payload(&body));
        }
    }

    // 3. If broadcaster slug is different from channel slug, also fetch broadcaster's own channel emotes
    if let Some(u_slug) = user_slug {
        if !u_slug.is_empty() && u_slug != target_slug {
            let url = format!("https://kick.com/emotes/{}", crate::commands::url_encode(u_slug));
            let (status, body) = get_json(&url, &headers).await;
            if status == 200 {
                merge(parse_kick_emotes_payload(&body));
            }
        }
    }

    (all_secs, all_tags)
}

/// A 7TV GraphQL answer (its REST answers carry every emote's whole description: 2 MB for a big set,
/// where names and ids are 60 KB).
async fn gql_7tv(query: &str) -> Value {
    let headers = [("Content-Type".to_string(), "application/json".to_string())];
    let body = json!({ "query": query }).to_string();
    match crate::http::request("POST", "https://7tv.io/v4/gql", &headers, body.as_bytes()).await {
        Ok(r) => serde_json::from_slice(&r.body).unwrap_or_default(),
        Err(_) => Value::Null,
    }
}

/// The emotes of a 7TV set, as `(name, url)`, from its GraphQL `items`.
fn parse_7tv_items(items: &Value, x: u8) -> Vec<(String, String)> {
    items
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|e| {
            let id = e.pointer("/emote/id")?.as_str()?;
            let name = e["alias"].as_str().or(e.pointer("/emote/defaultName")?.as_str())?;
            Some((name.to_string(), format!("https://cdn.7tv.app/emote/{id}/{x}x.webp")))
        })
        .collect()
}

/// The name and emotes of the set a user has active on `platform` ("twitch", "kick", "youtube").
async fn fetch_7tv_user_set(platform: &str, uid: &str, x: u8) -> (String, Vec<(String, String)>) {
    if uid.is_empty() {
        return (String::new(), Vec::new());
    }
    let (mut name, mut out) = (String::new(), Vec::new());
    for page in 1..=10 {
        let v = gql_7tv(&format!(
            "{{ users {{ userByConnection(platform: {}, platformId: {}) {{ style {{ activeEmoteSet {{ name emotes(page: {page}, perPage: 1000) {{ items {{ alias emote {{ id defaultName }} }} pageCount }} }} }} }} }} }}",
            platform.to_uppercase(),
            json!(uid)
        ))
        .await;
        let set = v.pointer("/data/users/userByConnection/style/activeEmoteSet");
        let Some(set) = set.filter(|s| s.is_object()) else {
            break;
        };
        name = set["name"].as_str().unwrap_or("").to_string();
        out.extend(parse_7tv_items(&set["emotes"]["items"], x));
        if page >= set["emotes"]["pageCount"].as_u64().unwrap_or(1) {
            break;
        }
    }
    (name, out)
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
        fetch_kick_native(&p.kick_slug, p.kick_user_slug.as_deref(), p.kick_token.as_deref()),
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
                title: tr("emotes.channel_ffz").into(),
                note: None,
                emotes: ffz_ch,
            });
        }
        let mut bttv_ch = parse_bttv_vec(&bttv_room["channelEmotes"], x);
        bttv_ch.extend(parse_bttv_vec(&bttv_room["sharedEmotes"], x));
        if !bttv_ch.is_empty() {
            twitch_bttv.push(EmoteSection {
                title: tr("emotes.channel_shared_bttv").into(),
                note: None,
                emotes: bttv_ch,
            });
        }
    }
    if !bttv_global.is_empty() {
        twitch_bttv.push(EmoteSection {
            title: tr("emotes.global_bttv").into(),
            note: None,
            emotes: bttv_global.clone(),
        });
    }
    if !ffz_global.is_empty() {
        twitch_ffz.push(EmoteSection {
            title: tr("emotes.global_ffz").into(),
            note: None,
            emotes: ffz_global,
        });
    }

    let build_tw_7tv = |personal: Option<Vec<(String, String)>>| -> Vec<EmoteSection> {
        let mut secs = Vec::new();
        if let Some(p_emotes) = personal {
            if !p_emotes.is_empty() {
                secs.push(EmoteSection {
                    title: tr("emotes.account_7tv").into(),
                    note: None,
                    emotes: p_emotes,
                });
            }
        }
        secs.push(EmoteSection {
            title: tr("emotes.channel_7tv").into(),
            note: tw_chan_7tv.is_empty().then(|| tr("emotes.no_7tv_channel_set").into()),
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

    let broad_7tv_personal = match p.tw_broadcaster.as_ref().map(|(id, _)| id.as_str()) {
        Some(b_id) if !b_id.is_empty() => {
            let set = fetch_7tv_personal("TWITCH", b_id, x).await;
            (!set.is_empty()).then_some(set)
        }
        _ => None,
    };
    let bot_7tv_extra = match p.tw_bot.as_ref().map(|(id, _)| id.as_str()) {
        Some(bot_id) if !bot_id.is_empty() => {
            let mut set = fetch_7tv_personal("TWITCH", bot_id, x).await;
            if bot_id != p.tw_chan_id {
                let (_, user_set) = fetch_7tv_user_set("twitch", bot_id, x).await;
                for (k, v) in user_set {
                    if !set.iter().any(|(n, _)| n == &k) {
                        set.push((k, v));
                    }
                }
            }
            (!set.is_empty()).then_some(set)
        }
        _ => None,
    };
    let twitch_7tv = [build_tw_7tv(broad_7tv_personal), build_tw_7tv(bot_7tv_extra)];

    // Kick 7TV
    let mut kick_7tv = Vec::new();
    if !p.kick_uid.is_empty() {
        let personal = fetch_7tv_personal("KICK", &p.kick_uid, x).await;
        if !personal.is_empty() {
            kick_7tv.push(EmoteSection {
                title: tr("emotes.account_7tv").into(),
                note: None,
                emotes: personal,
            });
        }
        let (_, k7) = fetch_7tv_user_set("kick", &p.kick_uid, x).await;
        kick_7tv.push(EmoteSection {
            title: tr("emotes.channel_7tv").into(),
            note: k7.is_empty().then(|| tr("emotes.no_7tv_kick_set").into()),
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
                title: tr("emotes.channel_7tv").into(),
                note: None,
                emotes: y7,
            });
        }
        let mut yb_ch = parse_bttv_vec(&yb["channelEmotes"], x);
        yb_ch.extend(parse_bttv_vec(&yb["sharedEmotes"], x));
        if !yb_ch.is_empty() {
            youtube_bttv.push(EmoteSection {
                title: tr("emotes.channel_shared_bttv").into(),
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
            title: tr("emotes.global_bttv").into(),
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
    fn tiktok_only_offers_sendable_emoji_and_favorites() {
        assert!(!providers_for("tiktok").contains(&Provider::Platform));
        let favorites = vec![
            ("[tiktok:123]".into(), "https://example.test/sticker.png".into()),
            ("🙂".into(), "https://example.test/emoji.png".into()),
        ];
        let sections = sections("tiktok", Provider::Favorites, true, &[], &favorites);
        assert_eq!(sections[0].emotes.len(), 1);
        assert_eq!(sections[0].emotes[0].0, "🙂");
    }

    #[test]
    fn youtube_native_emotes_are_not_offered_for_sending() {
        assert!(!providers_for("youtube").contains(&Provider::Platform));
        let favorites = vec![
            (":yt:".into(), "https://yt3.ggpht.com/image".into()),
            ("😀".into(), twemoji("😀")),
        ];
        for platform in ["youtube", "all"] {
            let items = sections(platform, Provider::Favorites, true, &[], &favorites);
            assert_eq!(items[0].emotes.len(), 1);
            assert_eq!(items[0].emotes[0].0, "😀");
        }
    }

    #[test]
    fn kick_emotes_are_the_logged_in_channels_when_no_other_is_set() {
        let mut cfg = Config::default();
        cfg.kick.broadcaster = Some(crate::kick::KickAccount {
            user_id: 5,
            username: "PedroZamecki".into(),
            token: Default::default(),
        });
        assert_eq!(kick_slug(&cfg), "pedrozamecki");
        cfg.kick.channel = "https://kick.com/Other".into();
        assert_eq!(kick_slug(&cfg), "other");
        assert_eq!(kick_slug(&Config::default()), "");
    }

    #[test]
    fn twitch_third_party_emotes_are_the_channel_read() {
        let mut cfg = Config::default();
        cfg.twitch.broadcaster = Some(crate::twitch::TwitchAccount {
            user_id: "10".into(),
            login: "me".into(),
            ..Default::default()
        });
        assert_eq!(LoadParams::with_scale(&cfg, 1).tw_chan_id, "10");
        cfg.twitch.channel = "other".into();
        cfg.twitch.channel_id = "20".into();
        assert_eq!(LoadParams::with_scale(&cfg, 1).tw_chan_id, "20");
    }

    #[test]
    fn reads_a_chatters_personal_7tv_set() {
        let v = json!({"data": {"users": {"userByConnection": {"personalEmoteSet": {"emotes": {"items": [
            {"alias": "mine", "emote": {"id": "A1", "defaultName": "orig"}},
            {"alias": null, "emote": {"id": "B2", "defaultName": "plain"}}
        ]}}}}}});
        assert_eq!(
            parse_7tv_personal(&v, 2),
            vec![
                ("mine".into(), "https://cdn.7tv.app/emote/A1/2x.webp".into()),
                ("plain".into(), "https://cdn.7tv.app/emote/B2/2x.webp".into())
            ]
        );
        assert!(parse_7tv_personal(&json!({"data": {"users": {"userByConnection": null}}}), 1).is_empty());
    }

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

    #[test]
    fn parses_kick_collectibles_and_remembers_tags() {
        let payload = json!([
            {
                "id": "collectibles",
                "name": "Collectibles",
                "emotes": [
                    { "id": 5748016, "name": "collectiblesWIDEGIGACHAD", "subscribers_only": false }
                ]
            }
        ]);
        let (secs, tags) = parse_kick_emotes_payload(&payload);
        assert_eq!(secs.len(), 1);
        assert_eq!(secs[0].title, tr("emotes.kick_collectibles"));
        assert_eq!(secs[0].emotes[0].0, "collectiblesWIDEGIGACHAD");
        assert_eq!(
            tags.get("collectiblesWIDEGIGACHAD").map(String::as_str),
            Some("[emote:5748016:collectiblesWIDEGIGACHAD]")
        );

        remember_kick_tag("collectiblesWIDEGIGACHAD", "5748016");
        assert_eq!(
            kick_tag("collectiblesWIDEGIGACHAD"),
            Some("[emote:5748016:collectiblesWIDEGIGACHAD]".into())
        );
    }
}
