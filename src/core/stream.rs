//! The Stream page: the live's title, category and tags on every platform at once (or on one), ad
//! breaks, YouTube's description and thumbnail, and on Twitch a marker, a clip and snoozing the next ad.
//!
//! What each platform can change:
//!
//! | | Title | Category | Tags | Ads | Other |
//! |---|---|---|---|---|---|
//! | Twitch | yes | yes | yes | yes (affiliates and partners) | marker, clip, snooze the next ad |
//! | Kick | yes | yes | yes | yes (opted in to Kick ads) | |
//! | YouTube | yes | yes | yes (search keywords) | yes (cue point, while live) | description, thumbnail, privacy and health |
//! | TikTok | yes (browser extension, while live; read from the room) | no | no | no | |
//!
//! Twitch and Kick let only the channel owner change these: a moderator sees them, can't change them.
//! The page lists every platform that is set up, with whether it's live (since when, how many watch),
//! what it says now and why it can't be changed when it can't. Ads, markers, clips and snoozes need a
//! live and are off without one. Like the Polls page, what the platforms said last lives here and a
//! read answers from it. Titles, categories and lives arrive by themselves (EventSub, Pusher, the chat
//! readers); the page asks again when opened, after a change, on Refresh, when a live starts or ends,
//! and every minute while open (YouTube every 5: it costs quota). Only platforms already running are
//! asked: this page opens no connection by itself. The study behind it is `docs/STREAM-PAGE.md`.

use super::accounts::same_account as same;
use crate::oauth::token_ready;
use crate::{
    automation::schema::{F, W},
    engine::Engine,
    i18n::{tr, trf},
    model::Obj,
    youtube::VideoUpdate,
};
use serde_json::{json, Value};
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
    time::{Duration, Instant},
};

pub const DOMAIN: &str = "stream";

pub fn supports(domain: &str) -> bool {
    domain == DOMAIN
}

const TWITCH: usize = 0;
const KICK: usize = 1;
const YOUTUBE: usize = 2;
const TIKTOK: usize = 3;
/// The platforms' ids (their check box keys and logos) and names, in the page's order.
const PLATFORMS: [(&str, &str); 4] = [
    ("twitch", "Twitch"),
    ("kick", "Kick"),
    ("youtube", "YouTube"),
    ("tiktok", "TikTok"),
];
/// Who takes what.
const TITLE: [bool; 4] = [true, true, true, true];
const CATEGORY: [bool; 4] = [true, true, false, false];
const TAGS: [bool; 4] = [true, true, true, false];
const ADS: [bool; 4] = [true, true, true, false];
const ONLY_TWITCH: [bool; 4] = [true, false, false, false];
const ONLY_YOUTUBE: [bool; 4] = [false, false, true, false];

/// Ad lengths, in seconds. Twitch takes up to 180, Kick up to 90.
const AD_LENGTHS: [u64; 6] = [30, 60, 90, 120, 150, 180];
const AD_NAMES: &[&str] = &["30 s", "60 s", "90 s", "120 s", "150 s", "180 s"];
const KICK_AD_NAMES: &[&str] = &["30 s", "60 s", "90 s"];

/// The form for every platform: what changes, and where.
const INFO_FIELDS: &[F] = &[
    ("title", "Title", W::Text),
    ("category", "Category", W::Text),
    ("tags", "Tags", W::Lines),
    ("description", "Description", W::Multi),
    ("youtube_category", "YouTube category", W::Text),
    ("privacy", "YouTube privacy", W::Text),
    ("clear_tags", "Remove every tag", W::Bool),
    ("twitch", "Twitch", W::Bool),
    ("kick", "Kick", W::Bool),
    ("youtube", "YouTube", W::Bool),
    ("tiktok", "TikTok", W::Bool),
];
/// One platform's form: only what it takes.
const CHANNEL_FIELDS: &[F] = &[
    ("title", "Title", W::Text),
    ("category", "Category", W::Text),
    ("tags", "Tags", W::Lines),
    ("clear_tags", "Remove every tag", W::Bool),
];
const TWITCH_FIELDS: &[F] = &[
    ("title", "Title", W::Text),
    ("category", "Category", W::Text),
    ("tags", "Tags", W::Lines),
    ("clear_tags", "Remove every tag", W::Bool),
    ("language", "Language", W::Text),
    ("delay", "Stream delay (seconds)", W::Int),
    ("branded", "Branded content", W::Bool),
    ("label_politics", "Politics and sensitive social issues", W::Bool),
    ("label_drugs", "Drugs, intoxication or tobacco", W::Bool),
    ("label_gambling", "Gambling", W::Bool),
    ("label_profanity", "Significant profanity or vulgarity", W::Bool),
    ("label_sexual", "Sexual themes", W::Bool),
    ("label_violence", "Violent or graphic depictions", W::Bool),
];
// MatureGame is applied by Twitch from the category and cannot be changed through Helix.
const TWITCH_LABELS: &[(&str, &str)] = &[
    ("label_politics", "DebatedSocialIssuesAndPolitics"),
    ("label_drugs", "DrugsIntoxication"),
    ("label_gambling", "Gambling"),
    ("label_profanity", "ProfanityVulgarity"),
    ("label_sexual", "SexualThemes"),
    ("label_violence", "ViolentGraphic"),
];
const TWITCH_LANGUAGES: &[(&str, &str)] = &[
    ("ar", "العربية"),
    ("bg", "Български"),
    ("cs", "Čeština"),
    ("da", "Dansk"),
    ("de", "Deutsch"),
    ("el", "Ελληνικά"),
    ("en", "English"),
    ("es", "Español"),
    ("fi", "Suomi"),
    ("fr", "Français"),
    ("hu", "Magyar"),
    ("id", "Bahasa Indonesia"),
    ("it", "Italiano"),
    ("ja", "日本語"),
    ("ko", "한국어"),
    ("ms", "Bahasa Melayu"),
    ("nl", "Nederlands"),
    ("no", "Norsk"),
    ("pl", "Polski"),
    ("pt", "Português"),
    ("ro", "Română"),
    ("ru", "Русский"),
    ("sk", "Slovenčina"),
    ("sv", "Svenska"),
    ("th", "ไทย"),
    ("tl", "Tagalog"),
    ("tr", "Türkçe"),
    ("uk", "Українська"),
    ("vi", "Tiếng Việt"),
    ("zh", "中文"),
    ("asl", "American Sign Language"),
];

const YOUTUBE_FIELDS: &[F] = &[
    ("title", "Title", W::Text),
    ("tags", "Tags", W::Lines),
    ("description", "Description", W::Multi),
    ("youtube_category", "YouTube category", W::Text),
    ("privacy", "YouTube privacy", W::Text),
    ("clear_tags", "Remove every tag", W::Bool),
];
const TIKTOK_FIELDS: &[F] = &[("title", "Title", W::Text)];
const AD_FIELDS: &[F] = &[
    ("duration", "Length", W::Pick(AD_NAMES)),
    ("twitch", "Twitch", W::Bool),
    ("kick", "Kick", W::Bool),
    ("youtube", "YouTube", W::Bool),
];
const ONE_AD_FIELDS: &[F] = &[("duration", "Length", W::Pick(AD_NAMES))];
const KICK_AD_FIELDS: &[F] = &[("duration", "Length", W::Pick(KICK_AD_NAMES))];
const THUMBNAIL_FIELDS: &[F] = &[("file", "Image", W::Text)];

/// Twitch's limits (Helix "Modify Channel Information"), YouTube's (videos.update).
const TWITCH_TITLE_MAX: usize = 140;
const YOUTUBE_TITLE_MAX: usize = 100;
const TWITCH_TAGS_MAX: usize = 10;
const TWITCH_TAG_MAX: usize = 25;
const KICK_TAGS_MAX: usize = 10;
const KICK_AD_MAX: u64 = 90;
/// YouTube counts its tags together: commas between them, quotes around one with a space.
const YOUTUBE_TAGS_MAX: usize = 500;
const YOUTUBE_DESCRIPTION_MAX: usize = 5000;
const THUMBNAIL_MAX: u64 = 2 * 1024 * 1024;

/// Categories and tags seen or used, offered to pick from (kept by `catalog`, in memory).
const KNOWN_CATEGORIES: &str = "stream.categories";
const KNOWN_TAGS: &str = "stream.tags";

/// Adds names to a list of picks, once each (any case).
fn remember(key: &'static str, names: impl IntoIterator<Item = String>) {
    let candidates: Vec<String> = names
        .into_iter()
        .map(|name| name.trim().to_owned())
        .filter(|name| !name.is_empty())
        .collect();
    if candidates.is_empty() {
        return;
    }
    let mut list = crate::catalog::get(key);
    let before = list.len();
    for name in candidates {
        if list.len() < 300 && !list.iter().any(|(_, known)| known.eq_ignore_ascii_case(&name)) {
            list.push((name.clone(), name));
        }
    }
    if list.len() != before {
        crate::catalog::set(key, list);
    }
}

/// A field's picks as the form takes them.
fn picks(key: &str) -> Value {
    json!(crate::catalog::get(key)
        .into_iter()
        .map(|(id, label)| json!({"id": id, "label": label}))
        .collect::<Vec<_>>())
}

/// What a platform says the live is now.
#[derive(Default, Clone, PartialEq, Debug)]
struct Info {
    title: String,
    category: String,
    tags: Vec<String>,
    language: String,
    labels: Vec<String>,
    branded: Option<bool>,
    mature: Option<bool>,
    delay: Option<u64>,
    category_id: String,
    /// Ads in words: the next one, the breaks left ("" when unknown).
    ads: String,
    ads_login: bool,
    /// Ads are unavailable on this channel (non-affiliate Twitch, Kick not opted in).
    ads_unavailable: bool,
    /// It answered (TikTok only while connected to a live; YouTube only says a title for a broadcast).
    known: bool,
    /// Live now (Some(true)), not live (Some(false)), or not known (None).
    live: Option<bool>,
    /// When the live started (Unix seconds).
    since: Option<i64>,
    /// Viewers now, as the platform said when it was read (the chat's count, fresher, wins).
    viewers: Option<u64>,
    /// YouTube: broadcast id, lifecycle, scheduled start, broadcast options and stats.
    broadcast_id: String,
    lifecycle: String,
    scheduled_at: Option<i64>,
    latency: String,
    dvr: Option<bool>,
    auto_start: Option<bool>,
    auto_stop: Option<bool>,
    made_for_kids: Option<bool>,
    likes: Option<u64>,
    coins: Option<u64>,
    paused: bool,
    /// YouTube: the description, privacy (public, unlisted, private) and the stream's health (good,
    /// ok, bad, noData) with what YouTube says is wrong with it.
    description: String,
    privacy: String,
    health: String,
    issue: String,
    stream_status: String,
    stream_video: String,
    /// YouTube: not live, but changes go to its next broadcast (scheduled, or the ready one).
    upcoming: bool,
    /// YouTube: neither live nor anything to come, so there is nothing to change.
    nothing: bool,
    /// Why it couldn't be read.
    error: String,
}

/// What a connected OBS instance says about its stream output right now.
#[derive(Default, Clone, PartialEq, Debug)]
struct ObsState {
    name: String,
    streaming: bool,
    reconnecting: bool,
    congestion: f64,
    skipped_frames: u64,
    total_frames: u64,
}

/// Which platforms are set up (listed), which can be changed now, and why not.
#[derive(Default, Clone, PartialEq, Debug)]
struct Access {
    shown: [bool; 4],
    ready: [bool; 4],
    /// The state in a word or two (a locale key), and the reason in full.
    state: [&'static str; 4],
    why: [String; 4],
}

/// What the platforms said last, and how the last thing asked went.
#[derive(Default, Clone)]
struct Seen {
    access: Access,
    info: [Info; 4],
    obs: Vec<ObsState>,
    obs_inventories: Vec<(String, crate::core::outputs::Inventory)>,
    note: String,
    /// Something asked last didn't go through (the note says what).
    trouble: bool,
    requested: u64,
    received: u64,
    busy: bool,
    /// Viewers by the chat's count, when it has one (set on each read of the page).
    viewers: [Option<u64>; 4],
}

#[derive(Clone, Debug, Default)]
pub struct YouTubePreflight {
    pub nothing: bool,
    pub privacy: String,
}

pub fn youtube_preflight_snapshot() -> YouTubePreflight {
    SEEN.with_borrow(|seen| YouTubePreflight {
        nothing: seen.info[YOUTUBE].nothing,
        privacy: seen.info[YOUTUBE].privacy.clone(),
    })
}

#[cfg(test)]
pub fn set_youtube_preflight_for_test(nothing: bool, privacy: &str) {
    SEEN.with_borrow_mut(|seen| {
        seen.info[YOUTUBE].nothing = nothing;
        seen.info[YOUTUBE].privacy = privacy.to_owned();
    });
}

pub fn cached_obs_inventories() -> Vec<(String, crate::core::outputs::Inventory)> {
    SEEN.with_borrow(|seen| seen.obs_inventories.clone())
}

impl Seen {
    fn begin(&mut self, mutation: bool) -> Option<u64> {
        if self.busy {
            return None;
        }
        self.requested = self.requested.wrapping_add(1).max(1);
        self.busy = mutation;
        Some(self.requested)
    }
    fn finish(&mut self, sequence: u64) -> bool {
        if sequence != self.requested {
            return false;
        }
        self.received = sequence;
        self.busy = false;
        true
    }
    /// The ready platforms that take this.
    fn can(&self, takes: [bool; 4]) -> [bool; 4] {
        std::array::from_fn(|i| self.access.ready[i] && takes[i])
    }
    /// The ready platforms that take this and aren't known to be off air (markers, clips).
    fn can_live(&self, takes: [bool; 4]) -> [bool; 4] {
        let can = self.can(takes);
        std::array::from_fn(|i| can[i] && self.received > 0 && self.info[i].live != Some(false))
    }
    /// The ready platforms that take ads, are live, and are eligible to run ads.
    fn can_ads(&self, takes: [bool; 4]) -> [bool; 4] {
        let live = self.can_live(takes);
        std::array::from_fn(|i| live[i] && !self.info[i].ads_unavailable)
    }
}

thread_local! {
    static SEEN: RefCell<Seen> = RefCell::default();
    static ASKED: Cell<Option<Instant>> = const { Cell::new(None) };
    static OPENED: Cell<bool> = const { Cell::new(false) };
    /// When YouTube was last successfully read (it costs quota).
    static YOUTUBE_ASKED: Cell<Option<Instant>> = const { Cell::new(None) };
    /// What the page depended on when it last asked (see `situation`).
    static SITUATION: Cell<Option<u64>> = const { Cell::new(None) };
}

fn seen() -> Seen {
    SEEN.with(|seen| seen.borrow().clone())
}

fn platform_index(slug: &str) -> Option<usize> {
    PLATFORMS.iter().position(|(key, _)| *key == slug)
}

fn oauth_ready(access: &str, refresh: &str, expires_at: u64, client_id: &str, client_secret: &str) -> bool {
    token_ready(access, refresh, expires_at)
        && (!crate::oauth::stale(expires_at) || (!client_id.is_empty() && !client_secret.is_empty()))
}

fn ads_unavailable_why(info: &Info, fallback: &'static str) -> String {
    if info.ads.is_empty() {
        tr(fallback).to_string()
    } else {
        info.ads.clone()
    }
}

fn text_field(item: &Obj, key: &str) -> String {
    item.get(key)
        .and_then(Value::as_str)
        .unwrap_or_default()
        .trim()
        .to_string()
}

/// A channel or account name, compared without case, spaces or a leading @.
/// Who may change what, from the settings and what is running.
fn access(e: &Engine) -> Access {
    let c = e.config();
    let bridge = &e.tiktok_bridge;
    let mut access = Access::default();
    let off = |platform: &str| {
        (
            "stream.state_disconnected",
            trf("stream.not_connected", &[("where", platform)]),
        )
    };
    let login = |platform: &str| ("stream.state_login", trf("stream.login_owner", &[("where", platform)]));
    let not_owner = |account: &str, channel: &str| {
        (
            "stream.state_read_only",
            trf("stream.not_owner", &[("account", account), ("channel", channel.trim())]),
        )
    };
    let fine = ("stream.state_ready", String::new());
    // Twitch: the channel read must be the logged-in account's own.
    let twitch = c.twitch.broadcaster.as_ref();
    access.shown[TWITCH] = twitch.is_some() || !c.twitch.channel.trim().is_empty();
    let twitch_state = if e.twitch().is_none() {
        off("Twitch")
    } else if !twitch.is_some_and(|a| token_ready(&a.token.access_token, &a.token.refresh_token, a.token.expires_at)) {
        login("Twitch")
    } else if crate::twitch::other_channel(&c) {
        not_owner(
            &twitch.map(|a| a.display_name.clone()).unwrap_or_default(),
            &c.twitch.channel,
        )
    } else {
        fine.clone()
    };
    // Kick: the token changes its owner's channel only.
    let kick = c.kick.broadcaster.as_ref();
    access.shown[KICK] = kick.is_some() || !c.kick.channel.trim().is_empty();
    let kick_state = if e.kick().is_none() {
        off("Kick")
    } else if !kick.is_some_and(|a| {
        oauth_ready(
            &a.token.access_token,
            &a.token.refresh_token,
            a.token.expires_at,
            &c.kick.client_id,
            &c.kick.client_secret,
        )
    }) {
        login("Kick")
    } else if !c.kick.channel.trim().is_empty() && !kick.is_some_and(|a| same(&a.username, &c.kick.channel)) {
        not_owner(&kick.map(|a| a.username.clone()).unwrap_or_default(), &c.kick.channel)
    } else {
        fine.clone()
    };
    // YouTube: the live read must be on the logged-in channel.
    let youtube = c.youtube.broadcaster.as_ref();
    access.shown[YOUTUBE] = youtube.is_some() || !c.youtube.channel_id.trim().is_empty();
    let youtube_state = if e.youtube().is_none() {
        off("YouTube")
    } else if !youtube.is_some_and(|a| {
        oauth_ready(
            &a.token.access_token,
            &a.token.refresh_token,
            a.token.expires_at,
            &c.youtube.client_id,
            &c.youtube.client_secret,
        )
    }) {
        login("YouTube")
    } else if !c.youtube.channel_id.trim().is_empty()
        && !youtube.is_some_and(|a| {
            [&a.channel_id, &a.name, &a.handle]
                .iter()
                .any(|mine| same(mine, &c.youtube.channel_id))
        })
    {
        not_owner(
            &youtube.map(|a| a.name.clone()).unwrap_or_default(),
            &c.youtube.channel_id,
        )
    } else {
        fine.clone()
    };
    // TikTok: a title belongs to a live, set through the browser extension as the account logged in there.
    access.shown[TIKTOK] = !c.tiktok.channel.trim().is_empty() || bridge.is_connected();
    let tiktok_state = if !e.tiktok_connected() {
        ("stream.state_offline", tr("stream.tiktok_offline").to_string())
    } else if !bridge.is_connected() || !bridge.ready() {
        ("stream.state_setup", tr("stream.tiktok_bridge").to_string())
    } else if !bridge.user().is_empty()
        && !c.tiktok.channel.trim().is_empty()
        && !same(&bridge.user(), &c.tiktok.channel)
    {
        not_owner(&bridge.user(), &c.tiktok.channel)
    } else {
        fine
    };
    for (i, (state, why)) in [twitch_state, kick_state, youtube_state, tiktok_state]
        .into_iter()
        .enumerate()
    {
        access.state[i] = state;
        access.why[i] = why;
    }
    access.ready = std::array::from_fn(|i| access.shown[i] && access.why[i].is_empty());
    access
}

/// The page's status line: what is going on, what the last change did, or nothing.
fn status_line(seen: &Seen) -> (&'static str, String) {
    let access = &seen.access;
    if seen.busy {
        ("busy", tr("stream.working").into())
    } else if seen.received == 0 {
        ("busy", tr("stream.checking").into())
    } else if !seen.note.is_empty() {
        (if seen.trouble { "down" } else { "up" }, seen.note.clone())
    } else if !access.shown.iter().any(|shown| *shown) {
        ("off", tr("stream.accounts_needed").into())
    } else {
        // Each card says its own state: nothing to add.
        ("", String::new())
    }
}

/// One change on one platform.
#[derive(Clone, Debug, PartialEq)]
enum Job {
    /// TikTok live title.
    Title(String),
    /// Twitch channel changes in one PATCH (resolving category name -> game_id first when given).
    Twitch {
        category: Option<String>,
        body: Value,
    },
    /// Kick channel changes in one PATCH (resolving category name -> category_id first when given).
    Kick {
        title: Option<String>,
        category: Option<String>,
        tags: Option<Vec<String>>,
    },
    /// YouTube metadata in one update (each costs quota).
    Video(VideoUpdate),
    Ad(u64),
    Thumbnail(String),
    Marker,
    Clip,
    Snooze,
}

fn video(title: Option<String>, description: Option<String>, tags: Option<Vec<String>>) -> Job {
    Job::Video(VideoUpdate {
        title,
        description,
        tags,
        ..VideoUpdate::default()
    })
}

fn twitch_job(category: Option<String>, body: Value) -> Job {
    Job::Twitch { category, body }
}

fn kick_job(title: Option<String>, category: Option<String>, tags: Option<Vec<String>>) -> Job {
    Job::Kick { title, category, tags }
}

impl Job {
    fn what(&self, platform: usize) -> String {
        let key = match self {
            Job::Title(_) => "stream.job_title",
            Job::Twitch { category, body } => {
                let map = body.as_object();
                let count = map.map_or(0, serde_json::Map::len) + usize::from(category.is_some());
                if count == 1 && category.is_some() {
                    "stream.job_category"
                } else if count == 1 && map.is_some_and(|m| m.contains_key("title")) {
                    "stream.job_title"
                } else if count == 1
                    && map.is_some_and(|m| m.get("tags").and_then(Value::as_array).is_some_and(Vec::is_empty))
                {
                    "stream.job_clear_tags"
                } else if count == 1 && map.is_some_and(|m| m.contains_key("tags")) {
                    "stream.job_tags"
                } else if map.is_some_and(|m| {
                    m.keys().all(|k| {
                        matches!(
                            k.as_str(),
                            "broadcaster_language" | "delay" | "is_branded_content" | "content_classification_labels"
                        )
                    })
                }) {
                    "stream.job_channel"
                } else {
                    "stream.job_video"
                }
            }
            Job::Kick { title, category, tags } => match (title, category, tags) {
                (Some(_), None, None) => "stream.job_title",
                (None, Some(_), None) => "stream.job_category",
                (None, None, Some(t)) if t.is_empty() => "stream.job_clear_tags",
                (None, None, Some(_)) => "stream.job_tags",
                _ => "stream.job_video",
            },
            Job::Video(update)
                if update.tags.as_ref().is_some_and(Vec::is_empty)
                    && update.title.is_none()
                    && update.description.is_none()
                    && update.category.is_none()
                    && update.privacy.is_none() =>
            {
                "stream.job_clear_tags"
            }
            Job::Video(_) => "stream.job_video",
            Job::Ad(_) => "stream.job_ad",
            Job::Thumbnail(_) => "stream.job_thumbnail",
            Job::Marker => "stream.job_marker",
            Job::Clip => "stream.job_clip",
            Job::Snooze => "stream.job_snooze",
        };
        trf(key, &[("where", PLATFORMS[platform].1)])
    }
}

type Step = (usize, Job);

fn twitch_error(error: String) -> String {
    crate::twitch::plain_error(&error)
}

fn find_kick_category_id(data: &Value, name: &str) -> Option<u64> {
    let list = data["data"].as_array()?;
    let picked = list
        .iter()
        .find(|c| c["name"].as_str().is_some_and(|x| x.eq_ignore_ascii_case(name)))
        .or_else(|| list.first())?;
    picked["id"].as_u64().filter(|id| *id > 0)
}

/// Runs one change through the platform's own client (only ever a running one); what it made, in
/// words, when there is something to say (a clip's link).
async fn run(e: &Rc<Engine>, platform: usize, job: &Job, youtube_id: &str) -> Result<String, String> {
    let who = || vec![("broadcaster_id", "{broadcaster_id}")];
    let off = || seen().access.why[platform].clone();
    match (platform, job) {
        (TWITCH, Job::Twitch { category, body }) => {
            let twitch = e.twitch().ok_or_else(off)?;
            let mut patch = body.as_object().cloned().unwrap_or_default();
            if let Some(name) = category {
                let games = twitch
                    .helix("GET", "games", vec![("name", name.as_str())], None, false, true)
                    .await
                    .map_err(twitch_error)?;
                let id = games["data"][0]["id"]
                    .as_str()
                    .filter(|id| !id.is_empty())
                    .ok_or_else(|| trf("stream.no_category", &[("name", name)]))?;
                patch.insert("game_id".into(), Value::String(id.to_owned()));
            }
            if !patch.is_empty() {
                twitch
                    .helix("PATCH", "channels", who(), Some(Value::Object(patch)), false, true)
                    .await
                    .map_err(twitch_error)?;
            }
            Ok(String::new())
        }
        (TWITCH, Job::Ad(_) | Job::Marker | Job::Clip | Job::Snooze) => {
            let twitch = e.twitch().ok_or_else(off)?;
            let (method, path, query, body) = match job {
                Job::Ad(length) => (
                    "POST",
                    "channels/commercial",
                    vec![],
                    json!({"broadcaster_id": "{broadcaster_id}", "length": (*length).clamp(1, 180)}),
                ),
                Job::Marker => (
                    "POST",
                    "streams/markers",
                    vec![],
                    json!({"user_id": "{broadcaster_id}"}),
                ),
                Job::Clip => ("POST", "clips", who(), Value::Null),
                Job::Snooze => ("POST", "channels/ads/schedule/snooze", who(), Value::Null),
                _ => unreachable!(),
            };
            let body = (!body.is_null()).then_some(body);
            let answer = twitch
                .helix(method, path, query, body, false, true)
                .await
                .map_err(twitch_error)?;
            Ok(match job {
                Job::Clip => answer["data"][0]["id"]
                    .as_str()
                    .map(|id| format!("https://clips.twitch.tv/{id}"))
                    .unwrap_or_default(),
                Job::Snooze => twitch_ads(&answer["data"][0], Some(true)),
                _ => String::new(),
            })
        }
        (KICK, Job::Kick { title, category, tags }) => {
            let kick = e.kick().ok_or_else(off)?;
            let mut patch = serde_json::Map::new();
            if let Some(name) = category {
                let encoded = crate::commands::url_encode(name);
                let v2 = kick
                    .api(
                        "GET",
                        &format!("https://api.kick.com/public/v2/categories?name={encoded}"),
                        None,
                    )
                    .await;
                let id = match v2.ok().and_then(|v| find_kick_category_id(&v, name)) {
                    Some(id) => Some(id),
                    None => kick
                        .api("GET", &format!("/categories?q={encoded}"), None)
                        .await
                        .ok()
                        .and_then(|v| find_kick_category_id(&v, name)),
                }
                .ok_or_else(|| trf("stream.no_category", &[("name", name)]))?;
                patch.insert("category_id".into(), id.into());
            }
            if let Some(title) = title {
                patch.insert("stream_title".into(), title.clone().into());
            }
            if let Some(tags) = tags {
                let capped: Vec<String> = tags.iter().take(10).cloned().collect();
                patch.insert("custom_tags".into(), json!(capped));
            }
            if !patch.is_empty() {
                kick.api("PATCH", "/channels", Some(Value::Object(patch))).await?;
            }
            Ok(String::new())
        }
        (KICK, Job::Ad(length)) => {
            let kick = e.kick().ok_or_else(off)?;
            kick.start_ad_break(*length).await.map(|_| String::new())
        }
        (YOUTUBE, _) => {
            let youtube = e.youtube().ok_or_else(off)?;
            match job {
                Job::Video(update) => youtube.update_video(youtube_id, update.clone()).await,
                Job::Ad(length) => youtube.run_commercial(youtube_id, *length).await,
                Job::Thumbnail(file) => youtube.set_thumbnail(youtube_id, file).await,
                _ => Err(tr("stream.not_here").into()),
            }
            .map(|()| String::new())
        }
        (TIKTOK, Job::Title(title)) => crate::tiktok_bridge::set_stream_title(e, title)
            .await
            .map(|()| String::new()),
        _ => Err(tr("stream.not_here").into()),
    }
}

fn texts(v: Option<&Value>) -> Vec<String> {
    v.and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .map(|text| text.trim().to_string())
        .filter(|text| !text.is_empty())
        .collect()
}

/// A Unix time Twitch sent as a number, a numeric text or an RFC 3339 text (its docs say one, it
/// sends another); None for nothing set.
fn unix_time(v: &Value) -> Option<i64> {
    let secs = match v {
        Value::Number(n) => n.as_i64(),
        Value::String(t) if t.is_empty() => None,
        Value::String(t) => t.parse().ok().or_else(|| crate::engine::rest_parse_iso(t)),
        _ => None,
    };
    secs.filter(|secs| *secs > 0)
}

/// Twitch's ad schedule (Get Ad Schedule, or a snooze's answer) in words. Ads run only while live:
/// off air it says so, and nothing about a schedule Twitch keeps at zero then.
fn twitch_ads(data: &Value, live: Option<bool>) -> String {
    if live == Some(false) {
        return tr("stream.ads_live_only").into();
    }
    let now = crate::oauth::now() as i64;
    let next = unix_time(&data["next_ad_at"]);
    let mut words = vec![match next {
        Some(at) if at > now => trf("stream.ads_next", &[("time", &crate::fmt::shown_clock(at))]),
        _ => tr("stream.ads_ready").into(),
    }];
    if let Some(duration) = data["duration"].as_u64().filter(|secs| *secs > 0) {
        if let Some(last) = unix_time(&data["last_ad_at"]) {
            words.push(trf(
                "stream.ads_last",
                &[
                    ("seconds", &duration.to_string()),
                    ("time", &crate::fmt::shown_clock(last)),
                ],
            ));
        } else if next.is_some_and(|at| at > now) {
            words.push(trf("stream.ads_length_planned", &[("seconds", &duration.to_string())]));
        }
    }
    if let Some(left) = data["snooze_count"].as_u64() {
        words.push(trf("stream.snoozes_left", &[("count", &left.to_string())]));
    }
    // Seconds without pre-roll ads for new viewers, earned by the last ad.
    if let Some(free) = data["preroll_free_time"].as_u64().filter(|secs| *secs >= 60) {
        words.push(trf("stream.preroll_free", &[("minutes", &(free / 60).to_string())]));
    }
    words.join(" ")
}

/// Kick's ad break status in words.
fn kick_ads(data: &Value, live: Option<bool>) -> String {
    if data["opted_in"].as_bool() == Some(false) {
        tr("stream.ads_kick_out").into()
    } else if live == Some(false) {
        tr("stream.ads_live_only").into()
    } else if data["ads_blocked"].as_bool() == Some(true) {
        tr("stream.ads_blocked").into()
    } else if let Some(left) = data["remaining_ad_breaks"].as_u64() {
        trf("stream.ads_left", &[("count", &left.to_string())])
    } else {
        tr("stream.ads_ready").into()
    }
}

/// Twitch's channel (Get Channel Information) into `info`.
fn twitch_channel(info: &mut Info, data: &Value) {
    info.title = data["title"].as_str().unwrap_or_default().into();
    info.category = data["game_name"].as_str().unwrap_or_default().into();
    info.tags = texts(data.get("tags"));
    info.language = data["broadcaster_language"].as_str().unwrap_or_default().into();
    info.labels = texts(data.get("content_classification_labels"));
    info.branded = data["is_branded_content"].as_bool();
    info.delay = data["delay"].as_u64();
    info.known = data.is_object();
}

/// Twitch's stream (Get Streams, an empty list when it's not live) into `info`.
fn twitch_stream(info: &mut Info, data: &Value) {
    let live = data["type"] == "live";
    info.live = Some(live);
    info.since = live.then(|| unix_time(&data["started_at"])).flatten();
    info.viewers = data["viewer_count"].as_u64().filter(|_| live);
}

/// Kick's channel (the official Get Channels) into `info`. Kick says a title and category only
/// while live: offline they come back empty.
fn kick_channel(info: &mut Info, data: &Value) {
    info.title = data["stream_title"].as_str().unwrap_or_default().into();
    info.category = data["category"]["name"].as_str().unwrap_or_default().into();
    info.tags = texts(data.get("custom_tags"));
    info.known = data.is_object();
    let stream = &data["stream"];
    if stream.is_object() {
        let live = stream["is_live"].as_bool() == Some(true);
        info.live = Some(live);
        info.since = live.then(|| unix_time(&stream["start_time"])).flatten();
        info.viewers = stream["viewer_count"].as_u64().filter(|_| live);
        info.mature = stream["is_mature"].as_bool();
        if let Some(language) = stream["language"].as_str().filter(|s| !s.is_empty()) {
            info.language = language.into();
        }
    }
}

/// Kick's live (Get Livestreams), which has the tags the channel lacks.
fn kick_livestream(info: &mut Info, data: &Value) {
    if !data.is_object() {
        return;
    }
    let tags = texts(data.get("custom_tags"));
    if !tags.is_empty() {
        info.tags = tags;
    }
    if info.title.is_empty() {
        info.title = data["stream_title"].as_str().unwrap_or_default().into();
    }
    info.since = info.since.or_else(|| unix_time(&data["started_at"]));
    if info.language.is_empty() {
        if let Some(language) = data["language"].as_str().filter(|s| !s.is_empty()) {
            info.language = language.into();
        }
    }
}

/// YouTube's own names for its video categories (they never change), by id.
const YOUTUBE_CATEGORIES: &[(&str, &str)] = &[
    ("1", "Film & Animation"),
    ("2", "Autos & Vehicles"),
    ("10", "Music"),
    ("15", "Pets & Animals"),
    ("17", "Sports"),
    ("19", "Travel & Events"),
    ("20", "Gaming"),
    ("22", "People & Blogs"),
    ("23", "Comedy"),
    ("24", "Entertainment"),
    ("25", "News & Politics"),
    ("26", "Howto & Style"),
    ("27", "Education"),
    ("28", "Science & Technology"),
    ("29", "Nonprofits & Activism"),
];

/// From a liveBroadcasts list: the live one, else the soonest still to come.
fn youtube_pick(items: &[Value]) -> Option<&Value> {
    let state = |item: &Value| item["status"]["lifeCycleStatus"].as_str().unwrap_or("").to_owned();
    items
        .iter()
        .find(|item| matches!(state(item).as_str(), "live" | "liveStarting"))
        .or_else(|| {
            items
                .iter()
                .filter(|item| matches!(state(item).as_str(), "ready" | "created" | "testing" | "testStarting"))
                .min_by_key(|item| item["snippet"]["scheduledStartTime"].as_str().unwrap_or("~").to_owned())
        })
}

/// YouTube's broadcast and its video (videos.list) into `info`.
fn youtube_broadcast(info: &mut Info, broadcast: &Value, video: &Value) {
    let lifecycle = broadcast["status"]["lifeCycleStatus"].as_str().unwrap_or_default();
    let live = matches!(lifecycle, "live" | "liveStarting");
    let snippet = if video["snippet"].is_object() {
        &video["snippet"]
    } else {
        &broadcast["snippet"]
    };
    info.broadcast_id = broadcast["id"].as_str().unwrap_or_default().into();
    info.lifecycle = lifecycle.into();
    info.title = snippet["title"].as_str().unwrap_or_default().into();
    info.description = snippet["description"].as_str().unwrap_or_default().into();
    info.tags = texts(snippet.get("tags"));
    let category = snippet["categoryId"].as_str().unwrap_or_default();
    info.category_id = category.into();
    info.category = YOUTUBE_CATEGORIES
        .iter()
        .find(|(id, _)| *id == category)
        .map(|(_, name)| (*name).to_owned())
        .unwrap_or_default();
    info.privacy = video["status"]["privacyStatus"]
        .as_str()
        .or(broadcast["status"]["privacyStatus"].as_str())
        .unwrap_or_default()
        .into();
    info.made_for_kids = video["status"]["madeForKids"]
        .as_bool()
        .or_else(|| video["status"]["selfDeclaredMadeForKids"].as_bool());
    info.known = true;
    info.live = Some(live);
    info.upcoming = !live;
    let details = &video["liveStreamingDetails"];
    info.scheduled_at =
        unix_time(&broadcast["snippet"]["scheduledStartTime"]).or_else(|| unix_time(&details["scheduledStartTime"]));
    let content = &broadcast["contentDetails"];
    info.latency = content["latencyPreference"].as_str().unwrap_or_default().into();
    info.dvr = content["enableDvr"].as_bool();
    info.auto_start = content["enableAutoStart"].as_bool();
    info.auto_stop = content["enableAutoStop"].as_bool();
    info.since = live
        .then(|| unix_time(&details["actualStartTime"]).or_else(|| unix_time(&broadcast["snippet"]["actualStartTime"])))
        .flatten();
    info.viewers = details["concurrentViewers"]
        .as_str()
        .and_then(|n| n.parse().ok())
        .filter(|_| live);
    info.likes = video["statistics"]["likeCount"]
        .as_str()
        .and_then(|n| n.parse().ok())
        .or_else(|| video["statistics"]["likeCount"].as_u64())
        .filter(|n| *n > 0);
}

/// The health of the stream feeding a live broadcast (liveStreams.list), its worst complaint, and
/// its CDN video format when known.
fn youtube_health(info: &mut Info, stream: &Value) {
    info.stream_status = stream["status"]["streamStatus"].as_str().unwrap_or_default().into();
    let health = &stream["status"]["healthStatus"];
    info.health = health["status"].as_str().unwrap_or_default().into();
    let rank = |issue: &Value| match issue["severity"].as_str() {
        Some("error") => 0,
        Some("warning") => 1,
        _ => 2,
    };
    info.issue = health["configurationIssues"]
        .as_array()
        .into_iter()
        .flatten()
        .min_by_key(|issue| rank(issue))
        .and_then(|issue| issue["description"].as_str().or(issue["reason"].as_str()))
        .unwrap_or_default()
        .into();
    let cdn = &stream["cdn"];
    let res = cdn["resolution"].as_str().unwrap_or_default();
    let fps = cdn["frameRate"].as_str().unwrap_or_default();
    let parts: Vec<&str> = [res, fps]
        .into_iter()
        .filter(|s| !s.is_empty() && *s != "variable" && *s != "unknown")
        .collect();
    info.stream_video = parts.join(" ");
}

/// What a platform says the live is now. Twitch and Kick are read for the channel read (a moderator
/// sees the owner's); their ads only by the owner, who alone may see them. A platform's reads run
/// side by side.
async fn look(e: &Rc<Engine>, platform: usize, ready: bool) -> Info {
    let mut info = Info::default();
    let result: Result<(), String> = async {
        match platform {
            TWITCH => {
                let Some(twitch) = e.twitch() else { return Ok(()) };
                if e.config().twitch.broadcaster.is_none() {
                    return Ok(());
                }
                let who = || vec![("broadcaster_id", "{broadcaster_id}")];
                let ads_allowed = crate::twitch::commercials_possible();
                // The schedule needs channel:read:ads and Affiliate/Partner status.
                let ads = async {
                    if ready && ads_allowed {
                        Some(twitch.helix("GET", "channels/ads", who(), None, false, true).await)
                    } else {
                        None
                    }
                };
                let (channel, stream, ads) = tokio::join!(
                    twitch.helix("GET", "channels", who(), None, false, true),
                    twitch.helix(
                        "GET",
                        "streams",
                        vec![("user_id", "{broadcaster_id}")],
                        None,
                        false,
                        true
                    ),
                    ads,
                );
                twitch_channel(&mut info, &channel.map_err(twitch_error)?["data"][0]);
                if let Ok(stream) = stream {
                    twitch_stream(&mut info, &stream["data"][0]);
                }
                if !ads_allowed {
                    info.ads_unavailable = true;
                    info.ads = tr("stream.ads_twitch_ineligible").into();
                } else {
                    match ads {
                        Some(Ok(ads)) => info.ads = twitch_ads(&ads["data"][0], info.live),
                        Some(Err(err))
                            if {
                                let lower = err.to_ascii_lowercase();
                                lower.contains("affiliate")
                                    || lower.contains("partner")
                                    || lower.contains("monetized")
                                    || err.contains("HTTP 403")
                            } =>
                        {
                            info.ads_unavailable = true;
                            info.ads = tr("stream.ads_twitch_ineligible").into();
                        }
                        _ => {}
                    }
                }
                remember(KNOWN_CATEGORIES, [info.category.clone()]);
                remember(KNOWN_TAGS, info.tags.clone());
                // The list of categories to pick from: Twitch's most watched, once.
                if crate::catalog::get(KNOWN_CATEGORIES).len() < 20 {
                    if let Ok(top) = twitch
                        .helix("GET", "games/top", vec![("first", "100")], None, false, true)
                        .await
                    {
                        remember(
                            KNOWN_CATEGORIES,
                            top["data"]
                                .as_array()
                                .into_iter()
                                .flatten()
                                .filter_map(|g| g["name"].as_str())
                                .map(str::to_owned),
                        );
                    }
                }
            }
            KICK => {
                let Some(kick) = e.kick() else { return Ok(()) };
                if e.config().kick.broadcaster.is_none() {
                    return Ok(());
                }
                let slug = e.config().kick.channel.trim().to_string();
                let path = if slug.is_empty() {
                    "/channels".to_string()
                } else {
                    format!("/channels?slug={}", crate::commands::url_encode(&slug))
                };
                let ads = async {
                    if ready {
                        Some(kick.get_ad_status().await)
                    } else {
                        None
                    }
                };
                let (channel, ads) = tokio::join!(kick.api("GET", &path, None), ads);
                let channel = channel?;
                let data = &channel["data"][0];
                kick_channel(&mut info, data);
                // Live: its tags are on the live, not the channel.
                if info.live == Some(true) {
                    if let Some(id) = data["broadcaster_user_id"].as_u64() {
                        if let Ok(live) = kick
                            .api("GET", &format!("/livestreams?broadcaster_user_id={id}"), None)
                            .await
                        {
                            kick_livestream(&mut info, &live["data"][0]);
                        }
                    }
                }
                info.ads = match ads {
                    Some(Ok(status)) => {
                        if status["data"]["opted_in"].as_bool() == Some(false) {
                            info.ads_unavailable = true;
                        }
                        kick_ads(&status["data"], info.live)
                    }
                    // An older login lacks the ads permission.
                    Some(Err(error)) if error.contains("HTTP 401") || error.contains("HTTP 403") => {
                        info.ads_login = true;
                        tr("stream.ads_kick_login").into()
                    }
                    _ => String::new(),
                };
                remember(KNOWN_CATEGORIES, [info.category.clone()]);
                remember(KNOWN_TAGS, info.tags.clone());
            }
            YOUTUBE => {
                let Some(youtube) = e.youtube() else { return Ok(()) };
                if ready {
                    // The owner: the Data API says it all for 2 or 3 units of quota.
                    let list = youtube
                        .api("liveBroadcasts?part=snippet,status,contentDetails&mine=true&maxResults=10")
                        .await?;
                    let items = list["items"].as_array().map_or(&[][..], Vec::as_slice);
                    let Some(broadcast) = youtube_pick(items) else {
                        info.nothing = true;
                        info.live = Some(false);
                        return Ok(());
                    };
                    let id = broadcast["id"].as_str().unwrap_or_default();
                    let video = youtube
                        .api(&format!(
                            "videos?part=snippet,status,liveStreamingDetails,statistics&id={id}"
                        ))
                        .await
                        .map(|v| v["items"][0].clone())
                        .unwrap_or_default();
                    youtube_broadcast(&mut info, broadcast, &video);
                    let bound = broadcast["contentDetails"]["boundStreamId"]
                        .as_str()
                        .unwrap_or_default();
                    if matches!(
                        info.lifecycle.as_str(),
                        "live" | "liveStarting" | "testing" | "testStarting"
                    ) && !bound.is_empty()
                    {
                        if let Ok(streams) = youtube.api(&format!("liveStreams?part=status,cdn&id={bound}")).await {
                            youtube_health(&mut info, &streams["items"][0]);
                        }
                    }
                } else {
                    // Someone else's channel: the watch page, as the chat reader does (no quota).
                    info.title = youtube.live_title().await?;
                    info.known = !info.title.is_empty();
                    info.live = Some(info.known);
                }
                info.ads = if info.live == Some(true) {
                    tr("stream.ads_ready").into()
                } else {
                    tr("stream.ads_youtube_offline").into()
                };
            }
            // TikTok says its live's title when the chat connects to it; else what was set here.
            _ => {
                if e.tiktok_connected() {
                    let room = crate::webcast::room_facts();
                    let (paused, likes, coins) = crate::tiktok::live_metrics();
                    info.title = room.title;
                    info.since = room.since;
                    info.known = !info.title.is_empty();
                    info.live = Some(true);
                    info.paused = paused;
                    info.likes = (likes > 0).then_some(likes);
                    info.coins = (coins > 0).then_some(coins);
                } else {
                    // The chat joins a TikTok live when there is one: not connected means not live.
                    info.live = Some(false);
                }
            }
        }
        Ok(())
    }
    .await;
    if let Err(error) = result {
        info.error = error;
    }
    info
}

/// Reads already-connected OBS Studio instances without opening new connections or background timers.
async fn look_obs(e: &Rc<Engine>) -> (Vec<ObsState>, Vec<(String, crate::core::outputs::Inventory)>) {
    let Some(obs) = e.obs() else {
        return (Vec::new(), Vec::new());
    };
    let conns: Vec<_> = e
        .config()
        .obs
        .connections
        .iter()
        .map(crate::obs::ObsConnInfo::from_obj)
        .collect();
    let targets: Vec<(Option<String>, String)> = if conns.is_empty() {
        if obs.is_connected(None) {
            vec![(None, "OBS".to_string())]
        } else {
            Vec::new()
        }
    } else {
        conns
            .into_iter()
            .filter(|c| obs.is_connected(Some(&c.id)))
            .map(|c| (Some(c.id), c.name))
            .collect()
    };
    let prefs = e.config().obs_output_prefs.clone();
    let mut out = Vec::with_capacity(targets.len());
    let mut inventories = Vec::with_capacity(targets.len());
    for (id, name) in targets {
        let conn_id = id.as_deref().unwrap_or("default");
        if let Ok(mut inv) = crate::core::outputs::discover(obs.as_ref(), conn_id).await {
            crate::core::outputs::apply_prefs(&mut inv, &prefs);
            inventories.push((name.clone(), inv));
        }
        if let Ok(status) = obs.send_request(id.as_deref(), "GetStreamStatus", json!({})).await {
            out.push(ObsState {
                name,
                streaming: status["outputActive"].as_bool() == Some(true),
                reconnecting: status["outputReconnecting"].as_bool() == Some(true),
                congestion: status["outputCongestion"].as_f64().unwrap_or(0.0),
                skipped_frames: status["outputSkippedFrames"].as_u64().unwrap_or(0),
                total_frames: status["outputTotalFrames"].as_u64().unwrap_or(0),
            });
        }
    }
    (out, inventories)
}

/// Runs one platform's changes in order: what each said, and whether one failed.
async fn run_steps(
    e: &Rc<Engine>,
    access: &Access,
    platform: usize,
    jobs: Vec<Job>,
    youtube_id: &str,
) -> (Vec<String>, bool) {
    let mut notes = Vec::new();
    let mut trouble = false;
    for job in jobs {
        let what = job.what(platform);
        if !access.ready[platform] {
            notes.push(trf(
                "stream.failed",
                &[("what", &what), ("error", &access.why[platform])],
            ));
            trouble = true;
            continue;
        }
        match run(e, platform, &job, youtube_id).await {
            Ok(made) => {
                // TikTok never says its title back: what was set here is what it is.
                if let (TIKTOK, Job::Title(title)) = (platform, &job) {
                    crate::webcast::room_titled(title);
                }
                let done = trf("stream.done", &[("what", &what)]);
                notes.push(if made.is_empty() {
                    done
                } else {
                    format!("{done} {made}")
                })
            }
            Err(error) => {
                trouble = true;
                notes.push(trf("stream.failed", &[("what", &what), ("error", &error)]))
            }
        }
    }
    (notes, trouble)
}

/// YouTube is read at most this often unless asked (Refresh, a change, a live starting or ending,
/// or opening the page after the cooldown): each read costs quota, 2 or 3 units of the 10,000 a day.
const YOUTUBE_EVERY: Duration = Duration::from_secs(300);
/// Minimum gap before re-opening the Stream tab forces another YouTube Data API read.
const YOUTUBE_OPEN_COOLDOWN: Duration = Duration::from_secs(30);
/// Twitch and Kick are read this often while the page is open; titles, categories and lives arrive
/// by themselves in between (EventSub, Pusher).
const READ_EVERY: Duration = Duration::from_secs(60);

/// Runs `steps`, then reads the platforms again; the model shows it all when it comes. False when a
/// change is already running. The platforms are changed and read side by side; `force` reads YouTube
/// too when it was read lately.
fn ask(engine: &Rc<Engine>, steps: Vec<Step>, force: bool) -> bool {
    ask_inner(engine, steps, force, false)
}

fn ask_inner(engine: &Rc<Engine>, steps: Vec<Step>, force: bool, clear_note: bool) -> bool {
    let mutation = !steps.is_empty();
    let Some(sequence) = SEEN.with(|seen| seen.borrow_mut().begin(mutation)) else {
        return false;
    };
    if mutation {
        crate::host::changed();
    }
    let e = engine.clone();
    tokio::task::spawn_local(async move {
        let access = access(&e);
        let prev = seen();
        let youtube_id = prev.info[YOUTUBE].broadcast_id.clone();
        let mut jobs: [Vec<Job>; 4] = Default::default();
        for (platform, job) in steps {
            jobs[platform].push(job);
        }
        let [twitch, kick, youtube, tiktok] = jobs;
        let ((n0, t0), (n1, t1), (n2, t2), (n3, t3)) = tokio::join!(
            run_steps(&e, &access, TWITCH, twitch, &youtube_id),
            run_steps(&e, &access, KICK, kick, &youtube_id),
            run_steps(&e, &access, YOUTUBE, youtube, &youtube_id),
            run_steps(&e, &access, TIKTOK, tiktok, &youtube_id),
        );
        let notes: Vec<String> = [n0, n1, n2, n3].concat();
        let trouble = t0 || t1 || t2 || t3;
        let youtube_due = force
            || !YOUTUBE_ASKED.get().is_some_and(|at| at.elapsed() < YOUTUBE_EVERY)
            || prev.info[YOUTUBE] == Info::default()
            || !prev.info[YOUTUBE].error.is_empty();
        let read = |platform: usize| {
            let e = &e;
            let on = access.shown[platform] && (platform != YOUTUBE || youtube_due);
            let ready = access.ready[platform];
            async move {
                if on {
                    Some(look(e, platform, ready).await)
                } else {
                    None
                }
            }
        };
        let (i0, i1, i2, i3, (obs, obs_inventories)) =
            tokio::join!(read(TWITCH), read(KICK), read(YOUTUBE), read(TIKTOK), look_obs(&e));
        let applied = SEEN.with(|seen| {
            let mut seen = seen.borrow_mut();
            if !seen.finish(sequence) {
                return false;
            }
            for (platform, found) in [i0, i1, i2, i3].into_iter().enumerate() {
                match found {
                    Some(found) => {
                        if platform == YOUTUBE {
                            if found.error.is_empty() {
                                YOUTUBE_ASKED.set(Some(Instant::now()));
                            } else {
                                YOUTUBE_ASKED.set(None);
                            }
                        }
                        seen.info[platform] = found;
                    }
                    // YouTube, read lately: what it said then stands.
                    None if platform == YOUTUBE && access.shown[platform] => {}
                    None => seen.info[platform] = Info::default(),
                }
            }
            seen.obs = obs;
            seen.obs_inventories = obs_inventories;
            seen.access = access;
            nothing_to_change(&mut seen);
            if !notes.is_empty() {
                seen.note = notes.join("  ");
                seen.trouble = trouble;
            } else if mutation || clear_note {
                seen.note.clear();
                seen.trouble = false;
            }
            true
        });
        if applied {
            let inventories = cached_obs_inventories();
            if !inventories.is_empty() {
                let _ = crate::core::preflight::evaluate_inventories(&e, &inventories, None, None);
            }
            crate::host::changed();
        }
    });
    true
}

fn apply_push(
    i: usize,
    title: Option<&str>,
    category: Option<&str>,
    twitch_extra: Option<(Option<&str>, Option<Vec<String>>)>,
) {
    let changed = SEEN.with_borrow_mut(|seen| {
        if seen.received == 0 || !seen.access.shown[i] {
            return false;
        }
        let info = &mut seen.info[i];
        let mut changed = false;
        if let Some(title) = title {
            let title = title.trim();
            if info.title != title || !info.known {
                info.title = title.to_owned();
                info.known = true;
                changed = true;
            }
        }
        if let Some(category) = category {
            let category = category.trim();
            if info.category != category {
                info.category = category.to_owned();
                changed = true;
            }
        }
        if let Some((language, labels)) = twitch_extra {
            if let Some(language) = language {
                let language = language.trim();
                if info.language != language {
                    info.language = language.to_owned();
                    changed = true;
                }
            }
            if let Some(labels) = labels {
                if info.labels != labels {
                    info.labels = labels;
                    changed = true;
                }
            }
        }
        changed
    });
    if changed {
        remember(KNOWN_CATEGORIES, category.map(str::to_owned));
        crate::host::changed();
    }
}

/// A platform said its live changed by itself (Twitch's EventSub, Kick's Pusher): the page shows it
/// at once, with no read. Empty values leave what is there.
pub fn pushed(platform: &str, title: Option<&str>, category: Option<&str>) {
    if let Some(i) = platform_index(platform) {
        apply_push(i, title, category, None);
    }
}

/// Twitch EventSub `channel.update` pushed new metadata (title, category, language, content labels).
pub fn pushed_twitch(title: Option<&str>, category: Option<&str>, language: Option<&str>, labels: Option<Vec<String>>) {
    apply_push(TWITCH, title, category, Some((language, labels)));
}

/// An ad break just started on `platform`: show it right away and refresh on next read when open.
pub fn ad_started(platform: &str, length: u64) {
    let Some(i) = platform_index(platform) else {
        return;
    };
    let changed = SEEN.with_borrow_mut(|seen| {
        if seen.received == 0 || !seen.access.shown[i] {
            return false;
        }
        let info = &mut seen.info[i];
        info.ads = if length > 0 {
            trf("stream.ad_running", &[("seconds", &length.to_string())])
        } else {
            tr("stream.ads_ready").into()
        };
        true
    });
    ASKED.set(None);
    if changed {
        crate::host::changed();
    }
}

/// TikTok reported its live stream paused or resumed.
pub fn tiktok_paused(paused: bool) {
    let changed = SEEN.with_borrow_mut(|seen| {
        if seen.received == 0 || !seen.access.shown[TIKTOK] {
            return false;
        }
        let info = &mut seen.info[TIKTOK];
        if info.paused == paused {
            return false;
        }
        info.paused = paused;
        true
    });
    if changed {
        crate::host::changed();
    }
}

/// OBS connected, disconnected, or changed stream state: refresh OBS status next time the page reads.
pub fn obs_changed() {
    ASKED.set(None);
    if seen().received > 0 {
        crate::host::changed();
    }
}

/// A live started or ended on `platform`: the page says so at once (a read follows, with the rest).
pub fn live_changed(platform: &str, live: bool) {
    let Some(i) = platform_index(platform) else {
        return;
    };
    let changed = SEEN.with_borrow_mut(|seen| {
        let info = &mut seen.info[i];
        if info.live == Some(live) {
            return false;
        }
        info.live = Some(live);
        info.since = live.then(|| crate::oauth::now() as i64);
        if live {
            info.nothing = false;
        } else {
            info.viewers = None;
            info.paused = false;
            info.likes = None;
            info.coins = None;
        }
        true
    });
    // The next read asks YouTube too: its live is a new broadcast.
    if i == YOUTUBE {
        YOUTUBE_ASKED.set(None);
    }
    if changed {
        ASKED.set(None);
        crate::host::changed();
    }
}

/// YouTube with no live and nothing to come has nothing to change: it says so instead of offering
/// buttons that would fail.
fn nothing_to_change(seen: &mut Seen) {
    if seen.info[YOUTUBE].nothing && seen.access.ready[YOUTUBE] {
        seen.access.ready[YOUTUBE] = false;
        seen.access.state[YOUTUBE] = "stream.state_offline";
        seen.access.why[YOUTUBE] = tr("stream.youtube_nothing").into();
    }
}

/// What decides what the page can show: who may change what, what is connected, which streams are
/// live (and one that just started, whose title arrives a moment later).
fn situation(e: &Engine) -> u64 {
    use std::hash::{Hash, Hasher};
    let access = access(e);
    let mut hash = std::collections::hash_map::DefaultHasher::new();
    (access.shown, access.ready, access.state).hash(&mut hash);
    [
        e.twitch_connected(),
        e.kick_connected(),
        e.youtube_connected(),
        e.tiktok_connected(),
    ]
    .hash(&mut hash);
    for (slug, _) in PLATFORMS {
        e.chat_stream_ended(slug).hash(&mut hash);
    }
    e.stream_warming_up().hash(&mut hash);
    hash.finish()
}

/// Asks the platforms again when the page is opened, when what it depends on changed (a connection,
/// a login, a live starting or ending), or when it's been a while: titles and lives arrive by
/// themselves, and the rest (viewers, ads) changes slowly.
fn refresh_if_due(engine: &Rc<Engine>) {
    let now = situation(engine);
    let changed = SITUATION.get() != Some(now);
    let just_opened = OPENED.get();
    let due = ASKED.get().is_none_or(|at| at.elapsed() > READ_EVERY);
    if changed || just_opened || due {
        let force_youtube = changed
            || (just_opened
                && !YOUTUBE_ASKED
                    .get()
                    .is_some_and(|at| at.elapsed() < YOUTUBE_OPEN_COOLDOWN));
        if ask(engine, Vec::new(), force_youtube) {
            SITUATION.set(Some(now));
            ASKED.set(Some(Instant::now()));
            OPENED.set(false);
        }
    }
}

/// The page was opened: what it shows is read again at once.
pub fn opened() {
    OPENED.set(true);
    ASKED.set(None);
}

pub fn read(engine: &Rc<Engine>) -> Result<Value, String> {
    refresh_if_due(engine);
    let mut now = seen();
    now.viewers = engine.chat_viewers();
    Ok(json!({"domain": DOMAIN, "sections": model(&now)}))
}

// The page

fn button(id: &str, label: &str, icon: &str) -> Value {
    json!({"id": id, "label": label, "icon": icon})
}
/// A button that opens a form; off with the reason on hover when it can't run.
fn form_button(id: &str, kind: &str, label: &str, icon: &str, why: &str) -> Value {
    let mut button = button(id, label, icon);
    button["form"] = kind.into();
    off_unless(button, why)
}
fn off_unless(mut button: Value, why: &str) -> Value {
    if !why.is_empty() {
        button["enabled"] = false.into();
        button["tooltip"] = why.into();
    }
    button
}
fn buttons(list: Vec<Value>) -> Value {
    json!({"type": "buttons", "buttons": list})
}
fn state(health: &str, text: &str) -> Value {
    json!({"health": health, "text": text})
}
/// Why nothing that takes this can run: busy, else the first platform's reason (off air, for what
/// needs a live), else "".
fn none_why(seen: &Seen, takes: [bool; 4], needs_live: bool) -> String {
    if seen.busy {
        return tr("stream.working").into();
    }
    let can = if needs_live {
        seen.can_live(takes)
    } else {
        seen.can(takes)
    };
    if can.iter().any(|on| *on) {
        return String::new();
    }
    if needs_live && seen.can(takes).iter().any(|on| *on) {
        if seen.received == 0 {
            return tr("stream.checking").into();
        }
        return tr("stream.needs_live").into();
    }
    (0..4)
        .find(|i| takes[*i] && seen.access.shown[*i])
        .map(|i| seen.access.why[i].clone())
        .unwrap_or_else(|| tr("stream.accounts_needed").into())
}

fn none_why_ads(seen: &Seen) -> String {
    if seen.busy {
        return tr("stream.working").into();
    }
    if seen.can_ads(ADS).iter().any(|on| *on) {
        return String::new();
    }
    if seen.received == 0 && seen.can(ADS).iter().any(|on| *on) {
        return tr("stream.checking").into();
    }
    if seen.can_live(ADS).iter().any(|on| *on) {
        return (0..4)
            .find(|i| ADS[*i] && seen.can_live(ADS)[*i] && seen.info[*i].ads_unavailable)
            .map(|i| ads_unavailable_why(&seen.info[i], "stream.ads_unavailable"))
            .unwrap_or_else(|| tr("stream.ads_unavailable").into());
    }
    none_why(seen, ADS, true)
}

/// The Accounts page, where a platform is connected and logged in.
fn accounts_button(id: &str) -> Value {
    let mut button = button(id, tr("nav.page.accounts"), "user-round");
    button["go"] = "Accounts".into();
    button
}

/// "Live since 21:04. 132 watching." for a live, as much of it as is known.
fn live_line(seen: &Seen, i: usize) -> String {
    let info = &seen.info[i];
    let mut words = vec![match info.since {
        Some(at) => trf("stream.live_since", &[("time", &crate::fmt::shown_clock(at))]),
        None => tr("stream.live_now").to_string(),
    }];
    if let Some(count) = seen.viewers[i].or(info.viewers) {
        words.push(trf("stream.watching", &[("count", &count.to_string())]));
    }
    words.join(" ")
}

fn language_name(code: &str) -> String {
    if code == "other" {
        return tr("stream.language_other").to_string();
    }
    TWITCH_LANGUAGES
        .iter()
        .find(|(id, _)| id.eq_ignore_ascii_case(code))
        .map(|(_, name)| (*name).to_string())
        .unwrap_or_else(|| code.to_uppercase())
}

fn label_name(id: &str) -> &'static str {
    match id {
        "DebatedSocialIssuesAndPolitics" => tr("stream.label_politics_short"),
        "DrugsIntoxication" => tr("stream.label_drugs_short"),
        "Gambling" => tr("stream.label_gambling_short"),
        "ProfanityVulgarity" => tr("stream.label_profanity_short"),
        "SexualThemes" => tr("stream.label_sexual_short"),
        "ViolentGraphic" => tr("stream.label_violence_short"),
        "MatureGame" => tr("stream.label_mature_game"),
        _ => "",
    }
}

/// Platform-specific metadata or live telemetry summary line, empty when nothing notable is set.
fn details_line(info: &Info, i: usize) -> Option<(&'static str, String)> {
    let mut parts = Vec::new();
    match i {
        TWITCH | KICK => {
            if !info.language.is_empty() {
                parts.push(trf(
                    "stream.summary_language",
                    &[("language", &language_name(&info.language))],
                ));
            }
            if let Some(delay) = info.delay.filter(|d| *d > 0) {
                parts.push(trf("stream.summary_delay", &[("seconds", &delay.to_string())]));
            }
            if info.branded == Some(true) {
                parts.push(tr("stream.summary_branded").to_string());
            }
            if info.mature == Some(true) {
                parts.push(tr("stream.summary_mature").to_string());
            }
            if !info.labels.is_empty() {
                let names: Vec<String> = info
                    .labels
                    .iter()
                    .map(|id| {
                        let known = label_name(id);
                        if known.is_empty() {
                            id.clone()
                        } else {
                            known.to_string()
                        }
                    })
                    .collect();
                parts.push(trf("stream.summary_labels", &[("labels", &names.join(", "))]));
            }
            (!parts.is_empty()).then(|| ("sliders-horizontal", parts.join(" ")))
        }
        YOUTUBE => {
            match info.latency.as_str() {
                "ultraLow" => parts.push(tr("stream.latency_ultra_low").to_string()),
                "low" => parts.push(tr("stream.latency_low").to_string()),
                "normal" => parts.push(tr("stream.latency_normal").to_string()),
                _ => {}
            }
            if let Some(dvr) = info.dvr {
                parts.push(tr(if dvr { "stream.dvr_on" } else { "stream.dvr_off" }).to_string());
            }
            if info.auto_start == Some(true) {
                parts.push(tr("stream.auto_start_on").to_string());
            }
            if info.auto_stop == Some(true) {
                parts.push(tr("stream.auto_stop_on").to_string());
            }
            if let Some(likes) = info.likes {
                parts.push(trf("stream.likes_count", &[("count", &likes.to_string())]));
            }
            (!parts.is_empty()).then(|| ("sliders-horizontal", parts.join(" ")))
        }
        TIKTOK => {
            if info.live == Some(true) {
                if let (Some(likes), Some(coins)) = (info.likes, info.coins) {
                    return Some((
                        "heart",
                        trf(
                            "stream.tiktok_metrics",
                            &[("likes", &likes.to_string()), ("coins", &coins.to_string())],
                        ),
                    ));
                }
                if let Some(likes) = info.likes {
                    return Some(("heart", trf("stream.tiktok_likes", &[("count", &likes.to_string())])));
                }
                if let Some(coins) = info.coins {
                    return Some(("gift", trf("stream.tiktok_coins", &[("count", &coins.to_string())])));
                }
            }
            None
        }
        _ => None,
    }
}

/// One platform's card: its state in a word (the reason on hover), what it shows now (the title, then
/// the category and tags as chips, then the live, its ads and what's wrong), and its buttons.
fn platform_card(seen: &Seen, i: usize) -> Value {
    let (slug, name) = PLATFORMS[i];
    let info = &seen.info[i];
    let access = &seen.access;
    let ready = access.ready[i];
    let why = if !ready {
        access.why[i].clone()
    } else if seen.busy {
        tr("stream.working").to_string()
    } else {
        String::new()
    };
    // Ads, markers, clips and snoozes need a live.
    let live_why = if !why.is_empty() {
        why.clone()
    } else if seen.received == 0 {
        tr("stream.checking").to_string()
    } else if info.live == Some(false) {
        tr("stream.needs_live").to_string()
    } else {
        String::new()
    };
    let ad_why = if !live_why.is_empty() {
        live_why.clone()
    } else if info.ads_unavailable {
        ads_unavailable_why(info, "stream.ads_unavailable")
    } else {
        String::new()
    };
    let (health, short, tip) = if !ready {
        ("off", tr(access.state[i]), access.why[i].clone())
    } else if !info.error.is_empty() {
        ("down", tr("stream.state_read_failed"), info.error.clone())
    } else if info.live == Some(true) && i == TIKTOK && info.paused {
        (
            "attention",
            tr("stream.state_paused"),
            tr("stream.state_paused_tip").to_string(),
        )
    } else if info.live == Some(true) {
        ("up", tr("stream.state_live"), String::new())
    } else if i == YOUTUBE && matches!(info.lifecycle.as_str(), "testing" | "testStarting") {
        (
            "busy",
            tr("stream.state_testing"),
            tr("stream.state_testing_tip").to_string(),
        )
    } else if info.live == Some(false) {
        (
            "standby",
            tr("stream.state_not_live"),
            tr("stream.state_not_live_tip").to_string(),
        )
    } else {
        ("up", tr("stream.state_ready"), String::new())
    };
    let mut card = json!({"card": true, "brand": slug, "title": name,
        "state": {"health": health, "text": short, "tip": tip}});
    let key = |what: &str| format!("{slug}.{what}");
    // Not connected or not logged in: nothing to show but why, and the way to Accounts.
    if matches!(
        access.state[i],
        "stream.state_disconnected" | "stream.state_login" | "stream.state_setup"
    ) {
        card["lines"] = json!([{"icon": "info", "text": access.why[i], "muted": true}]);
        card["buttons"] = json!([accounts_button(&key("accounts"))]);
        return card;
    }
    let mut lines = Vec::new();
    if !info.error.is_empty() {
        lines.push(json!({"icon": "circle-alert", "text": trf("stream.read_failed", &[("error", &info.error)]), "muted": true}));
    } else {
        let (headline, muted) = match () {
            _ if !info.title.is_empty() => (info.title.as_str(), false),
            _ if i == TIKTOK && info.live == Some(true) => (tr("stream.tiktok_title_unknown"), true),
            _ if i == TIKTOK => (tr("stream.not_live"), true),
            // Kick and YouTube say a title only for a live (or a broadcast to come).
            _ if info.live == Some(false) && i != TWITCH => (tr("stream.not_live"), true),
            _ => (tr("stream.no_title"), true),
        };
        card["headline"] = headline.into();
        card["headlineMuted"] = muted.into();
        let mut chips = Vec::new();
        if !info.category.is_empty() {
            chips.push(json!({"icon": "layers", "text": info.category}));
        }
        chips.extend(info.tags.iter().map(|tag| json!({"text": tag})));
        card["chips"] = chips.into();
    }
    if info.live == Some(true) {
        lines.push(json!({"icon": "radio", "text": live_line(seen, i)}));
    }
    if i == TIKTOK && info.live == Some(true) && info.paused {
        lines.push(json!({"icon": "circle-pause", "text": tr("stream.tiktok_paused"), "muted": false}));
    }
    if info.upcoming {
        let text = match info.scheduled_at {
            Some(at) => trf("stream.youtube_scheduled", &[("when", &crate::fmt::shown_when(at))]),
            None => tr("stream.youtube_upcoming").to_string(),
        };
        lines.push(json!({"icon": "calendar", "text": text, "muted": true}));
    }
    // YouTube: who can watch, when it isn't everyone.
    match info.privacy.as_str() {
        "private" => lines.push(json!({"icon": "lock", "text": tr("stream.youtube_private")})),
        "unlisted" => lines.push(json!({"icon": "eye-off", "text": tr("stream.youtube_unlisted")})),
        _ => {}
    }
    if i == YOUTUBE && info.made_for_kids == Some(true) {
        lines.push(json!({"icon": "shield", "text": tr("stream.youtube_kids"), "muted": true}));
    }
    if !info.health.is_empty() {
        let (key, quiet) = match info.health.as_str() {
            "good" => ("stream.health_good", true),
            "ok" => ("stream.health_ok", true),
            "bad" => ("stream.health_bad", false),
            _ => ("stream.health_none", false),
        };
        let mut text = tr(key).to_string();
        if !info.issue.is_empty() {
            text = format!("{text} {}", info.issue);
        }
        if !info.stream_video.is_empty() {
            text = format!("{text} ({})", info.stream_video);
        }
        lines.push(json!({"icon": "activity", "text": text, "muted": quiet}));
    } else if !info.stream_video.is_empty() {
        lines.push(json!({"icon": "video", "text": info.stream_video, "muted": true}));
    }
    if let Some((icon, text)) = details_line(info, i) {
        lines.push(json!({"icon": icon, "text": text, "muted": true}));
    }
    if ADS[i] && ready && !info.ads.is_empty() {
        lines.push(json!({"icon": "megaphone", "text": info.ads, "muted": info.live != Some(true)}));
    }
    if !ready {
        lines.push(json!({"icon": "info", "text": access.why[i], "muted": true}));
    }
    card["lines"] = lines.into();
    let mut list = vec![form_button(
        &key("edit"),
        &format!("stream_info.{slug}"),
        tr("btn.edit"),
        "pencil",
        &why,
    )];
    if ADS[i] {
        list.push(form_button(
            &key("ad"),
            &format!("stream_ad.{slug}"),
            tr("btn.run_ad"),
            "megaphone",
            &ad_why,
        ));
    }
    if i == TWITCH {
        list.push(off_unless(
            button(&key("snooze"), tr("btn.snooze_ad"), "clock"),
            &ad_why,
        ));
        list.push(off_unless(
            button(&key("marker"), tr("btn.add_marker"), "pin"),
            &live_why,
        ));
        list.push(off_unless(
            button(&key("clip"), tr("btn.create_clip"), "clapperboard"),
            &live_why,
        ));
    }
    if i == YOUTUBE {
        list.push(form_button(
            &key("set_thumbnail"),
            "stream_thumbnail",
            tr("btn.set_thumbnail"),
            "image",
            &why,
        ));
    }
    if i == KICK && info.ads_login {
        list.push(accounts_button(&key("accounts")));
    }
    card["buttons"] = list.into();
    card
}

fn overview(seen: &Seen) -> Option<String> {
    let mut live = 0u64;
    let mut offline = 0;
    let mut unknown = 0;
    let mut viewers = Some(0u64);
    for i in (0..4).filter(|i| seen.access.shown[*i]) {
        let info = &seen.info[i];
        match info.live.filter(|_| info.error.is_empty()) {
            Some(true) => {
                live += 1;
                viewers = viewers
                    .zip(seen.viewers[i].or(info.viewers))
                    .map(|(sum, count)| sum.saturating_add(count));
            }
            Some(false) => offline += 1,
            None => {
                unknown += 1;
                viewers = None;
            }
        }
    }
    if live + offline + unknown == 0 {
        return None;
    }
    let mut text = trf(
        "stream.overview",
        &[
            ("live", &live.to_string()),
            ("offline", &offline.to_string()),
            ("unknown", &unknown.to_string()),
        ],
    );
    if let Some(count) = viewers.filter(|_| live > 0) {
        text.push(' ');
        text.push_str(&trf("stream.total_viewers", &[("count", &count.to_string())]));
    }
    Some(text)
}

fn obs_summary(seen: &Seen) -> Option<(&'static str, String)> {
    if seen.obs.is_empty() {
        return None;
    }
    let any_live = seen
        .info
        .iter()
        .any(|info| info.live == Some(true) && info.error.is_empty());
    let mut best = "standby";
    let mut texts = Vec::with_capacity(seen.obs.len());
    for obs in &seen.obs {
        let (health, text) = if obs.reconnecting {
            ("attention", trf("stream.obs_reconnecting", &[("name", &obs.name)]))
        } else if obs.streaming {
            let dropped_pct = if obs.total_frames >= 100 {
                (obs.skipped_frames as f64 / obs.total_frames as f64) * 100.0
            } else {
                0.0
            };
            if obs.congestion > 0.25 || dropped_pct >= 2.0 {
                (
                    "attention",
                    trf(
                        "stream.obs_streaming_warn",
                        &[
                            ("name", &obs.name),
                            ("dropped", &obs.skipped_frames.to_string()),
                            ("total", &obs.total_frames.to_string()),
                        ],
                    ),
                )
            } else if obs.skipped_frames > 0 {
                (
                    "up",
                    trf(
                        "stream.obs_streaming_dropped",
                        &[("name", &obs.name), ("dropped", &obs.skipped_frames.to_string())],
                    ),
                )
            } else {
                ("up", trf("stream.obs_streaming", &[("name", &obs.name)]))
            }
        } else if any_live {
            (
                "attention",
                trf("stream.obs_stopped_while_live", &[("name", &obs.name)]),
            )
        } else {
            ("standby", trf("stream.obs_standby", &[("name", &obs.name)]))
        };
        if health == "attention" || (health == "up" && best != "attention") {
            best = health;
        }
        texts.push(text);
    }
    Some((best, texts.join("  ")))
}

fn companion_button() -> Value {
    let mut b = button("obs_companion", tr("preflight.btn_companion"), "shield");
    b["dialog"] = "obs_companion".into();
    b
}

fn obs_outputs_section(seen: &Seen) -> Option<Value> {
    if seen.obs_inventories.is_empty() {
        return None;
    }
    let multi_conn = seen.obs_inventories.len() > 1;
    let any_controllable = seen
        .obs_inventories
        .iter()
        .flat_map(|(_, inv)| &inv.outputs)
        .any(|o| o.required && o.controllable);
    let control_why = if seen.busy {
        tr("stream.working").to_owned()
    } else if !any_controllable {
        tr("preflight.state_ignored").to_owned()
    } else {
        String::new()
    };
    let mut rows = vec![json!({
        "about": tr("preflight.outputs_hint"),
        "control": buttons(vec![
            off_unless(button("obs.start_all", tr("stream.obs_start_all"), "play"), &control_why),
            off_unless(button("obs.stop_all", tr("stream.obs_stop_all"), "square"), &control_why),
            companion_button(),
        ]),
    })];
    let last = crate::core::preflight::last_report();
    for (conn_name, inv) in &seen.obs_inventories {
        for o in &inv.outputs {
            let base_name = if o.source == "main" {
                tr("stream.obs_main_output").to_owned()
            } else {
                o.name.clone()
            };
            let label = if multi_conn {
                format!("{conn_name}: {base_name}")
            } else {
                base_name
            };
            let source_label = match o.source.as_str() {
                "main" => "OBS Studio",
                "aitum-stream-suite" => "Aitum Stream Suite",
                "aitum-multistream" => "Aitum Multistream",
                "aitum-vertical" => "Aitum Vertical",
                "obs-multi-rtmp" => "Multiple RTMP",
                other => other,
            };
            let active_label = match (o.active, o.reconnecting) {
                (_, Some(true)) => tr("stream.state_paused"),
                (Some(true), _) => tr("stream.state_live"),
                (Some(false), _) => tr("preflight.state_standby"),
                (None, _) => "",
            };
            let about = if active_label.is_empty() {
                source_label.to_owned()
            } else {
                format!("{source_label} · {active_label}")
            };
            let matched = last
                .as_ref()
                .and_then(|r| r.outputs.iter().find(|item| item.id == o.id));
            let (health, status_text, choice_index) = if let Some(m) = matched {
                let h = match m.status.as_str() {
                    "ready" => "up",
                    "warning" => "attention",
                    "blocked" => "down",
                    _ => "off",
                };
                (h, m.detail.clone(), m.choice_index)
            } else if !o.required {
                ("off", tr("preflight.state_ignored").to_owned(), 5)
            } else if let Some(plat) = o.platform.as_deref().filter(|p| !p.is_empty()) {
                let idx = platform_index(plat);
                let (h, detail) = match idx {
                    Some(i) if seen.access.ready[i] => {
                        ("up", trf("preflight.output_ready_on", &[("platform", PLATFORMS[i].1)]))
                    }
                    Some(i) => ("down", seen.access.why[i].clone()),
                    None => ("attention", tr("preflight.state_unmapped").to_owned()),
                };
                let choice = if o.platform_source.as_deref() == Some("user") {
                    idx.map_or(0, |i| i + 1)
                } else {
                    0
                };
                (h, detail, choice)
            } else {
                ("attention", tr("preflight.state_unmapped").to_owned(), 0)
            };
            let auto_plat = if o.platform_source.as_deref() == Some("user") {
                None
            } else {
                o.platform.as_deref()
            };
            rows.push(json!({
                "key": format!("output.{}", o.id),
                "label": label,
                "about": about,
                "status": state(health, &status_text),
                "control": {
                    "type": "select",
                    "value": choice_index,
                    "options": crate::core::preflight::option_labels(auto_plat),
                },
            }));
        }
    }
    Some(json!({
        "title": tr("preflight.outputs_title"),
        "icon": "radio",
        "rows": rows,
    }))
}

fn model(seen: &Seen) -> Vec<Value> {
    let (health, text) = status_line(seen);
    let mut top = vec![
        form_button(
            "edit",
            "stream_info",
            tr("btn.edit_stream"),
            "pencil",
            &none_why(seen, TITLE, false),
        ),
        form_button(
            "run_ad",
            "stream_ad",
            tr("btn.run_ad"),
            "megaphone",
            &none_why_ads(seen),
        ),
        companion_button(),
        button("refresh", tr("btn.refresh"), "refresh-cw"),
    ];
    if !seen.access.shown.iter().any(|shown| *shown) {
        top = vec![accounts_button("accounts"), companion_button()];
    }
    // The introduction beside the buttons for every platform; under them, what is going on, if anything.
    let mut rows = vec![json!({"about": tr("stream.hint"), "control": buttons(top)})];
    if !text.is_empty() {
        rows.push(json!({"status": state(health, &text)}));
    }
    if let Some(text) = overview(seen) {
        rows.push(json!({"status": state("", &text)}));
    }
    if let Some((obs_health, obs_text)) = obs_summary(seen) {
        rows.push(json!({"status": state(obs_health, &obs_text)}));
    }
    let mut sections = vec![json!({"title": "", "icon": "", "rows": rows})];
    sections.extend((0..4).filter(|i| seen.access.shown[*i]).map(|i| platform_card(seen, i)));
    if let Some(obs_section) = obs_outputs_section(seen) {
        sections.push(obs_section);
    }
    sections
}

/// Tags to clear on every ready platform that has them.
fn clear_tags_steps(seen: &Seen) -> Vec<Step> {
    let on = seen.can(TAGS);
    (0..4)
        .filter(|i| on[*i])
        .map(|i| match i {
            TWITCH => (i, twitch_job(None, json!({"tags": []}))),
            KICK => (i, kick_job(None, None, Some(Vec::new()))),
            _ => (i, video(None, None, Some(Vec::new()))),
        })
        .collect()
}

/// Run one button. Changing the stream, running an ad and setting the thumbnail are forms
/// (`entity_form` kinds `stream_info`, `stream_ad`, `stream_thumbnail`, and `stream_info.kick` or
/// `stream_ad.twitch` for one platform).
pub fn act(engine: &Rc<Engine>, action: &str) -> Result<Value, String> {
    let now = seen();
    let go = |steps: Vec<Step>| {
        if ask(engine, steps, true) {
            Ok(json!({"accepted": true}))
        } else {
            Err(tr("stream.working").to_string())
        }
    };
    if matches!(action, "obs.start_all" | "obs.stop_all") {
        let op = if action == "obs.start_all" { "start" } else { "stop" };
        let targets: Vec<(String, Vec<String>)> = now
            .obs_inventories
            .iter()
            .filter_map(|(_, inv)| {
                let ids: Vec<String> = inv
                    .outputs
                    .iter()
                    .filter(|o| o.required && o.controllable)
                    .map(|o| o.id.clone())
                    .collect();
                (!ids.is_empty()).then(|| (inv.connection_id.clone(), ids))
            })
            .collect();
        if targets.is_empty() {
            return Err(tr("preflight.state_ignored").into());
        }
        let Some(sequence) = SEEN.with(|seen| seen.borrow_mut().begin(true)) else {
            return Err(tr("stream.working").to_string());
        };
        crate::host::changed();
        let e = engine.clone();
        tokio::task::spawn_local(async move {
            let mut errors = Vec::new();
            for (conn_id, ids) in targets {
                match crate::core::outputs::request(&e, &json!({"op": op, "connectionId": conn_id, "outputs": ids}))
                    .await
                {
                    Ok(value) => {
                        for item in value["results"].as_array().into_iter().flatten() {
                            if item["state"] == "error" {
                                let id = item["id"].as_str().unwrap_or("output");
                                let err = item["error"].as_str().unwrap_or("failed");
                                errors.push(format!("{id}: {err}"));
                            }
                        }
                    }
                    Err(error) => errors.push(error),
                }
            }
            let applied = SEEN.with(|seen| {
                let mut seen = seen.borrow_mut();
                if !seen.finish(sequence) {
                    return false;
                }
                if !errors.is_empty() {
                    seen.note = errors.join("  ");
                    seen.trouble = true;
                } else {
                    seen.note.clear();
                    seen.trouble = false;
                }
                true
            });
            if applied {
                ASKED.set(None);
                ask(&e, Vec::new(), false);
                crate::host::changed();
            }
        });
        return Ok(json!({"accepted": true}));
    }
    let action = action.strip_prefix("twitch.").unwrap_or(action);
    match action {
        "refresh" => {
            if ask_inner(engine, Vec::new(), true, true) {
                ASKED.set(Some(Instant::now()));
            }
            Ok(json!({"accepted": true}))
        }
        "clear_tags" => {
            let steps = clear_tags_steps(&now);
            if steps.is_empty() {
                return Err(none_why(&now, TAGS, false));
            }
            go(steps)
        }
        "snooze" => {
            if !now.can_ads(ONLY_TWITCH)[TWITCH] {
                if now.can_live(ONLY_TWITCH)[TWITCH] && now.info[TWITCH].ads_unavailable {
                    return Err(ads_unavailable_why(&now.info[TWITCH], "stream.ads_twitch_ineligible"));
                }
                return Err(none_why(&now, ONLY_TWITCH, true));
            }
            go(vec![(TWITCH, Job::Snooze)])
        }
        "marker" | "clip" => {
            if !now.can_live(ONLY_TWITCH)[TWITCH] {
                return Err(none_why(&now, ONLY_TWITCH, true));
            }
            go(vec![(TWITCH, if action == "marker" { Job::Marker } else { Job::Clip })])
        }
        _ => Err(tr("stream.use_form").into()),
    }
}

// The forms

/// A form kind: what it is and the one platform it's for (None: the platforms are picked).
fn kind_of(kind: &str) -> Option<(&str, Option<usize>)> {
    let (base, slug) = kind
        .split_once('.')
        .map_or((kind, None), |(base, slug)| (base, Some(slug)));
    if !matches!(base, "stream_info" | "stream_ad" | "stream_thumbnail") {
        return None;
    }
    let platform = match slug {
        None => None,
        Some(slug) => Some(platform_index(slug)?),
    };
    match (base, platform) {
        ("stream_ad", Some(i)) if !ADS[i] => None,
        ("stream_thumbnail", Some(_)) => None,
        _ => Some((base, platform)),
    }
}

pub fn external_fields(kind: &str) -> Option<&'static [F]> {
    Some(match kind_of(kind)? {
        ("stream_info", None) => INFO_FIELDS,
        ("stream_info", Some(YOUTUBE)) => YOUTUBE_FIELDS,
        ("stream_info", Some(TWITCH)) => TWITCH_FIELDS,
        ("stream_info", Some(TIKTOK)) => TIKTOK_FIELDS,
        ("stream_info", Some(_)) => CHANNEL_FIELDS,
        ("stream_ad", None) => AD_FIELDS,
        ("stream_ad", Some(KICK)) => KICK_AD_FIELDS,
        ("stream_ad", Some(_)) => ONE_AD_FIELDS,
        _ => THUMBNAIL_FIELDS,
    })
}

/// A form from its fields, each with, when it can't be used now, why ("-": it doesn't apply, hidden).
fn form_json(kind: &str, title: &str, ok: &str, fields: &[(&F, String)], item: Value, notes: Vec<String>) -> Value {
    let object = item.as_object().cloned().unwrap_or_default();
    let fields: Vec<Value> = fields
        .iter()
        .map(|(&(key, label, ref widget), off)| {
            let label = match key {
                "title" => tr("stream.title"),
                "category" => tr("stream.category"),
                "tags" => tr("stream.tags_field"),
                "description" => tr("stream.description"),
                "youtube_category" => tr("stream.youtube_category"),
                "privacy" => tr("stream.privacy"),
                "language" => tr("stream.language"),
                "delay" => tr("stream.delay"),
                "branded" => tr("stream.branded"),
                "label_politics" => tr("stream.label_politics"),
                "label_drugs" => tr("stream.label_drugs"),
                "label_gambling" => tr("stream.label_gambling"),
                "label_profanity" => tr("stream.label_profanity"),
                "label_sexual" => tr("stream.label_sexual"),
                "label_violence" => tr("stream.label_violence"),
                "clear_tags" => tr("stream.clear_tags_field"),
                "duration" => tr("stream.ad_length"),
                "file" => tr("stream.thumbnail_file"),
                _ => label,
            };
            let mut field = json!({"key": key, "label": label, "widget": widget, "holds": widget.about(),
                "value": crate::automation::edits::shown(widget, object.get(key))});
            if let W::Pick(options) = widget {
                field["widget"]["options"] = json!(options);
            }
            if PLATFORMS.iter().any(|(slug, _)| *slug == key) {
                field["brand"] = key.into();
            }
            match key {
                "category" => {
                    field["placeholder"] = tr("stream.category_hint").into();
                    // Pick from the list; typing a new one is the last choice.
                    field["choices"] = picks(KNOWN_CATEGORIES);
                    field["other"] = true.into();
                }
                "youtube_category" | "privacy" | "language" => {
                    let options = match key {
                        "youtube_category" => json!(YOUTUBE_CATEGORIES.iter().map(|(id, name)| json!({"id": id, "label": name})).collect::<Vec<_>>()),
                        "privacy" => json!([{"id": "public", "label": tr("stream.public")},
                            {"id": "unlisted", "label": tr("stream.unlisted")}, {"id": "private", "label": tr("stream.private")}]),
                        _ => {
                            let mut options: Vec<Value> = TWITCH_LANGUAGES.iter().map(|(id, name)| json!({"id": id, "label": name})).collect();
                            options.push(json!({"id": "other", "label": tr("stream.language_other")}));
                            json!(options)
                        }
                    };
                    field["choices"] = options;
                    field["strict"] = true.into();
                    if key == "privacy" { field["tip"] = tr("stream.privacy_tip").into(); }
                    if key == "language" { field["section"] = tr("stream.twitch_options").into(); }
                }
                "delay" => field["tip"] = tr("stream.delay_tip").into(),
                "label_politics" => field["section"] = tr("stream.content_labels").into(),
                "tags" => field["suggest"] = picks(KNOWN_TAGS),
                "file" => {
                    field["browse"] = "image".into();
                    field["placeholder"] = tr("stream.thumbnail_hint").into();
                }
                _ => {}
            }
            if off == "-" {
                field["hidden"] = true.into();
                if matches!(widget, W::Bool) {
                    field["disabled"] = true.into();
                }
            } else if !off.is_empty() {
                // A platform that can't run it greys out, the reason on hover.
                field["disabled"] = true.into();
                field["tip"] = off.clone().into();
            }
            field
        })
        .collect();
    json!({"kind": kind, "title": title, "okLabel": ok, "json": false, "notes": notes, "new": item.clone(),
        "fields": fields, "item": item})
}

/// The values a platform shows now, for its form: title, category and tags.
fn current(seen: &Seen, platform: Option<usize>) -> (String, String, Vec<String>) {
    let pick = |want: fn(&Info) -> bool| match platform {
        Some(i) => Some(i),
        // The first ready platform that says it, Twitch first.
        None => (0..4).find(|i| seen.access.ready[*i] && want(&seen.info[*i])),
    };
    let info = |i: Option<usize>| i.map(|i| seen.info[i].clone()).unwrap_or_default();
    (
        info(pick(|info| !info.title.is_empty())).title,
        info(pick(|info| !info.category.is_empty())).category,
        info(pick(|info| !info.tags.is_empty())).tags,
    )
}

/// The form for one of the page's kinds, filled with what the platforms say now.
pub fn external_form(kind: &str) -> Option<Result<Value, String>> {
    let (base, platform) = kind_of(kind)?;
    let fields = external_fields(kind)?;
    let now = seen();
    let off = |i: usize| {
        if now.access.ready[i] && base == "stream_ad" && now.received == 0 {
            tr("stream.checking").into()
        } else if now.access.ready[i] && base == "stream_ad" && now.info[i].live == Some(false) {
            tr("stream.needs_live").into()
        } else if now.access.ready[i] && base == "stream_ad" && now.info[i].ads_unavailable {
            ads_unavailable_why(&now.info[i], "stream.ads_unavailable")
        } else if now.access.ready[i] {
            String::new()
        } else if now.access.shown[i] {
            now.access.why[i].clone()
        } else {
            tr("stream.not_set_up").into()
        }
    };
    let any = |takes: [bool; 4]| match platform {
        Some(i) => takes[i],
        None => now.can(takes).iter().any(|on| *on),
    };
    let delay_shown =
        platform == Some(TWITCH) && (crate::twitch::is_partner() || now.info[TWITCH].delay.is_some_and(|d| d > 0));
    // A box none of the platforms can take is hidden.
    let unused = |on: bool| if on { String::new() } else { "-".to_string() };
    let labelled: Vec<(&F, String)> = fields
        .iter()
        .map(|field| {
            let why = match field.0 {
                "category" => unused(any(CATEGORY)),
                "tags" | "clear_tags" => unused(any(TAGS)),
                "description" | "youtube_category" | "privacy" => unused(any(ONLY_YOUTUBE)),
                "delay" => unused(delay_shown),
                key => platform_index(key).map(off).unwrap_or_default(),
            };
            (field, why)
        })
        .collect();
    let mut item = json!({"id": ""});
    let name = platform.map(|i| PLATFORMS[i].1).unwrap_or_default();
    let (title, ok, notes) = match base {
        "stream_info" => {
            let (title, category, tags) = current(&now, platform);
            item["title"] = title.into();
            item["category"] = category.into();
            item["tags"] = json!(tags);
            // YouTube's description, the only one there is.
            item["description"] = match platform {
                None | Some(YOUTUBE) => now.info[YOUTUBE].description.clone(),
                Some(_) => String::new(),
            }
            .into();
            item["youtube_category"] = now.info[YOUTUBE].category_id.clone().into();
            item["privacy"] = now.info[YOUTUBE].privacy.clone().into();
            if platform == Some(TWITCH) {
                let info = &now.info[TWITCH];
                item["language"] = info.language.clone().into();
                if delay_shown {
                    item["delay"] = info.delay.unwrap_or(0).into();
                }
                if let Some(branded) = info.branded {
                    item["branded"] = branded.into();
                }
                if info.known && info.error.is_empty() {
                    for (key, id) in TWITCH_LABELS {
                        item[*key] = info.labels.iter().any(|label| label == id).into();
                    }
                }
            }
            item["clear_tags"] = false.into();
            let note = if platform.is_some() {
                tr("stream.one_note")
            } else {
                tr("stream.info_note")
            };
            let title = match platform {
                Some(_) => trf("stream.edit_on", &[("where", name)]),
                None => tr("btn.edit_stream").to_string(),
            };
            let mut notes = vec![note.to_string()];
            if platform == Some(YOUTUBE) || platform.is_none() && now.access.ready[YOUTUBE] {
                notes.push(tr("stream.privacy_tip").into());
            }
            (title, tr("btn.save"), notes)
        }
        "stream_ad" => {
            item["duration"] = 0.into();
            let title = match platform {
                Some(_) => trf("stream.ad_on", &[("where", name)]),
                None => tr("btn.run_ad").to_string(),
            };
            (title, tr("btn.run_ad"), vec![tr("stream.ad_note").to_string()])
        }
        _ => {
            item["file"] = "".into();
            (
                tr("btn.set_thumbnail").to_string(),
                tr("btn.set_thumbnail"),
                vec![tr("stream.thumbnail_note").to_string()],
            )
        }
    };
    match platform {
        Some(i) => item[PLATFORMS[i].0] = true.into(),
        None => {
            let (takes, can) = if base == "stream_ad" {
                (ADS, now.can_ads(ADS))
            } else {
                (TITLE, now.can(TITLE))
            };
            for (i, on) in can.into_iter().enumerate() {
                if takes[i] {
                    item[PLATFORMS[i].0] = on.into();
                }
            }
        }
    }
    Some(Ok(form_json(kind, &title, ok, &labelled, item, notes)))
}

/// The platforms a form picked, among the ready ones that take it (and are live/eligible, for an ad).
fn picked(item: &Obj, seen: &Seen, takes: [bool; 4]) -> [bool; 4] {
    let can = if takes == ADS {
        seen.can_ads(takes)
    } else {
        seen.can(takes)
    };
    std::array::from_fn(|i| can[i] && item.get(PLATFORMS[i].0).and_then(Value::as_bool) == Some(true))
}

/// How long YouTube counts tags: with the commas between them and quotes around one with a space.
fn youtube_tags_length(tags: &[String]) -> usize {
    let quoted = tags.iter().filter(|tag| tag.contains(' ')).count();
    tags.iter().map(|tag| tag.chars().count()).sum::<usize>() + tags.len().saturating_sub(1) + 2 * quoted
}

/// The changes a stream info form asks for, or what's wrong with it. An empty field keeps what is there.
fn info_steps(item: &Obj, seen: &Seen) -> Result<Vec<Step>, String> {
    let text = |key: &str| text_field(item, key);
    let clear = item.get("clear_tags").and_then(Value::as_bool) == Some(true);
    let (title, category, description) = (text("title"), text("category"), text("description"));
    let tags = if clear { Vec::new() } else { texts(item.get("tags")) };
    let on = picked(item, seen, TITLE);
    if !on.iter().any(|on| *on) {
        return Err(tr("stream.pick_where").into());
    }
    let takes = |what: [bool; 4]| std::array::from_fn::<bool, 4, _>(|i| on[i] && what[i]);
    let (categorised, tagged) = (takes(CATEGORY), takes(TAGS));
    let any = |on: [bool; 4]| on.iter().any(|on| *on);
    let retag = (clear || !tags.is_empty()) && any(tagged);
    let mut twitch_body = serde_json::Map::new();
    if on[TWITCH] {
        let now = &seen.info[TWITCH];
        let language = text("language");
        if !language.is_empty() && language != now.language {
            if language != "other" && !TWITCH_LANGUAGES.iter().any(|(id, _)| *id == language) {
                return Err(tr("stream.language_bad").into());
            }
            twitch_body.insert("broadcaster_language".into(), language.into());
        }
        if let Some(delay_val) = item.get("delay").filter(|v| !v.is_null()) {
            let delay = delay_val
                .as_i64()
                .or_else(|| delay_val.as_str().and_then(|s| s.trim().parse().ok()))
                .filter(|d| (0..=900).contains(d))
                .ok_or_else(|| tr("stream.delay_bad").to_string())? as u64;
            if now.delay.map_or(delay > 0, |prev| delay != prev) {
                twitch_body.insert("delay".into(), delay.into());
            }
        }
        if let Some(branded) = item.get("branded").and_then(Value::as_bool) {
            if Some(branded) != now.branded {
                twitch_body.insert("is_branded_content".into(), branded.into());
            }
        }
        let labels: Vec<Value> = TWITCH_LABELS
            .iter()
            .filter_map(|(key, id)| {
                let enabled = item.get(*key)?.as_bool()?;
                (enabled != now.labels.iter().any(|label| label == id))
                    .then(|| json!({"id": id, "is_enabled": enabled}))
            })
            .collect();
        if !labels.is_empty() {
            twitch_body.insert("content_classification_labels".into(), labels.into());
        }
    }
    let youtube_category = text("youtube_category");
    let privacy = text("privacy");
    if on[YOUTUBE] {
        if !youtube_category.is_empty() && !YOUTUBE_CATEGORIES.iter().any(|(id, _)| *id == youtube_category) {
            return Err(tr("stream.youtube_category_bad").into());
        }
        if !privacy.is_empty() && !["public", "unlisted", "private"].contains(&privacy.as_str()) {
            return Err(tr("stream.privacy_bad").into());
        }
    }
    let changes = !twitch_body.is_empty()
        || on[YOUTUBE] && (!youtube_category.is_empty() || !privacy.is_empty())
        || !title.is_empty()
        || !category.is_empty() && any(categorised)
        || retag
        || !description.is_empty() && on[YOUTUBE];
    if !changes {
        return Err(tr("stream.nothing").into());
    }
    let limit = |platform: &str, count: usize| {
        Err(trf(
            "stream.title_limit",
            &[("where", platform), ("count", &count.to_string())],
        ))
    };
    let chars = title.chars().count();
    if on[TWITCH] && chars > TWITCH_TITLE_MAX {
        return limit("Twitch", TWITCH_TITLE_MAX);
    }
    if on[YOUTUBE] && chars > YOUTUBE_TITLE_MAX {
        return limit("YouTube", YOUTUBE_TITLE_MAX);
    }
    if on[TWITCH] && tags.len() > TWITCH_TAGS_MAX {
        return Err(tr("stream.tags_limit").into());
    }
    if on[KICK] && tags.len() > KICK_TAGS_MAX {
        return Err("Kick takes up to ten tags.".into());
    }
    if on[TWITCH] {
        if let Some(tag) = tags
            .iter()
            .find(|tag| tag.chars().count() > TWITCH_TAG_MAX || !tag.chars().all(char::is_alphanumeric))
        {
            return Err(trf("stream.tag_bad", &[("tag", tag)]));
        }
    }
    if on[YOUTUBE] && youtube_tags_length(&tags) > YOUTUBE_TAGS_MAX {
        return Err(tr("stream.youtube_tags_limit").into());
    }
    if on[YOUTUBE] && description.chars().count() > YOUTUBE_DESCRIPTION_MAX {
        return Err(tr("stream.description_limit").into());
    }
    if on[YOUTUBE]
        && (description.contains(['<', '>'])
            || title.contains(['<', '>'])
            || tags.iter().any(|tag| tag.contains(['<', '>'])))
    {
        return Err(tr("stream.youtube_brackets").into());
    }
    let given = |text: &String| (!text.is_empty()).then(|| text.clone());
    let new_tags = (clear || !tags.is_empty()).then(|| tags.clone());
    let mut steps = Vec::new();
    for i in (0..4).filter(|i| on[*i]) {
        // What a platform already says isn't sent again (each change is a request, YouTube's 50
        // units of quota).
        let now = &seen.info[i];
        let known = now.known && now.error.is_empty();
        let title = given(&title).filter(|title| !known || *title != now.title);
        let category = given(&category).filter(|category| !known || !category.eq_ignore_ascii_case(&now.category));
        let tags = new_tags.clone().filter(|tags| !known || *tags != now.tags);
        match i {
            TWITCH => {
                if let Some(title) = title {
                    twitch_body.insert("title".into(), title.into());
                }
                if let (true, Some(tags)) = (tagged[i], tags) {
                    twitch_body.insert("tags".into(), json!(tags));
                }
                let category = categorised[i].then_some(category).flatten();
                if category.is_some() || !twitch_body.is_empty() {
                    steps.push((i, twitch_job(category, Value::Object(std::mem::take(&mut twitch_body)))));
                }
            }
            KICK => {
                let category = categorised[i].then_some(category).flatten();
                let tags = tagged[i].then_some(tags).flatten();
                if title.is_some() || category.is_some() || tags.is_some() {
                    steps.push((i, kick_job(title, category, tags)));
                }
            }
            YOUTUBE => {
                let description = given(&description).filter(|text| !known || *text != now.description);
                let category = given(&youtube_category).filter(|id| !known || *id != now.category_id);
                let privacy = given(&privacy).filter(|privacy| !known || *privacy != now.privacy);
                if title.is_some() || description.is_some() || tags.is_some() || category.is_some() || privacy.is_some()
                {
                    steps.push((
                        i,
                        Job::Video(VideoUpdate {
                            title,
                            description,
                            tags,
                            category,
                            privacy,
                        }),
                    ));
                }
            }
            _ => {
                if let Some(title) = title {
                    steps.push((i, Job::Title(title)));
                }
            }
        }
    }
    if steps.is_empty() {
        return Err(tr("stream.unchanged").into());
    }
    if any(categorised) {
        remember(KNOWN_CATEGORIES, [category]);
    }
    if retag {
        remember(KNOWN_TAGS, tags);
    }
    Ok(steps)
}

fn ad_steps(item: &Obj, seen: &Seen) -> Result<Vec<Step>, String> {
    let on = picked(item, seen, ADS);
    if !on.iter().any(|on| *on) {
        return Err(tr("stream.pick_where").into());
    }
    let index = item.get("duration").and_then(Value::as_u64).unwrap_or(0) as usize;
    let length = AD_LENGTHS[index.min(AD_LENGTHS.len() - 1)];
    Ok((0..4)
        .filter(|i| on[*i])
        .map(|i| {
            let secs = if i == KICK { length.min(KICK_AD_MAX) } else { length };
            (i, Job::Ad(secs))
        })
        .collect())
}

fn thumbnail_steps(item: &Obj, seen: &Seen) -> Result<Vec<Step>, String> {
    if !seen.access.ready[YOUTUBE] {
        return Err(none_why(seen, ONLY_YOUTUBE, false));
    }
    let file = text_field(item, "file");
    let file = file.trim_matches('"');
    let lower = file.to_lowercase();
    if ![".jpg", ".jpeg", ".png"].iter().any(|end| lower.ends_with(end)) {
        return Err(tr("stream.thumbnail_kind").into());
    }
    match std::fs::metadata(file) {
        Err(_) => Err(trf("stream.thumbnail_missing", &[("file", file)])),
        Ok(meta) if meta.len() > THUMBNAIL_MAX => Err(tr("stream.thumbnail_size").into()),
        Ok(_) => Ok(vec![(YOUTUBE, Job::Thumbnail(file.into()))]),
    }
}

/// Run what a form describes; what is wrong with it comes back as the form's error.
pub fn save_external(engine: &Rc<Engine>, kind: &str, mut item: Obj) -> Option<Result<Value, String>> {
    let (base, platform) = kind_of(kind)?;
    // One platform's form is for that platform alone.
    if let Some(i) = platform {
        for (j, (slug, _)) in PLATFORMS.iter().enumerate() {
            item.insert((*slug).into(), (i == j).into());
        }
    }
    let now = seen();
    let steps = match base {
        "stream_info" => info_steps(&item, &now),
        "stream_ad" => ad_steps(&item, &now),
        _ => thumbnail_steps(&item, &now),
    };
    Some(steps.and_then(|steps| {
        if ask(engine, steps, true) {
            Ok(json!({"accepted": true}))
        } else {
            Err(tr("stream.working").into())
        }
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn reset(seen: Seen) {
        SEEN.with(|state| *state.borrow_mut() = seen);
        ASKED.set(Some(Instant::now()));
    }

    fn item(value: Value) -> Obj {
        value.as_object().unwrap().clone()
    }

    fn ready(on: [bool; 4]) -> Seen {
        Seen {
            access: Access {
                shown: [true; 4],
                ready: on,
                state: std::array::from_fn(|i| {
                    if on[i] {
                        "stream.state_ready"
                    } else {
                        "stream.state_read_only"
                    }
                }),
                why: std::array::from_fn(|i| if on[i] { String::new() } else { format!("no {i}") }),
            },
            received: 1,
            requested: 1,
            ..Seen::default()
        }
    }

    /// A platform's card, by its logo.
    fn card<'a>(sections: &'a [Value], brand: &str) -> &'a Value {
        sections
            .iter()
            .find(|section| section["card"] == true && section["brand"] == brand)
            .unwrap_or_else(|| panic!("no card {brand}"))
    }

    /// A button anywhere on the page, by id.
    fn find_button<'a>(sections: &'a [Value], id: &str) -> &'a Value {
        sections
            .iter()
            .flat_map(|section| {
                section["buttons"].as_array().into_iter().flatten().chain(
                    section["rows"]
                        .as_array()
                        .into_iter()
                        .flatten()
                        .flat_map(|row| row["control"]["buttons"].as_array().into_iter().flatten()),
                )
            })
            .find(|button| button["id"] == id)
            .unwrap_or_else(|| panic!("no button {id}"))
    }

    #[test]
    fn the_form_offers_known_categories_and_tags_to_pick() {
        let _lock = crate::TEST_LOCK.lock().unwrap_or_else(|error| error.into_inner());
        // Other stream tests remember their categories too. Start this fixture with empty picks.
        crate::catalog::set(KNOWN_CATEGORIES, Vec::new());
        crate::catalog::set(KNOWN_TAGS, Vec::new());
        reset(Seen::default());
        remember(
            KNOWN_CATEGORIES,
            ["Art".to_string(), "art".to_string(), "Music".to_string()],
        );
        remember(KNOWN_TAGS, ["English".to_string()]);
        let form = external_form("stream_info").unwrap().unwrap();
        let field = |key: &str| {
            form["fields"]
                .as_array()
                .unwrap()
                .iter()
                .find(|f| f["key"] == key)
                .unwrap()
                .clone()
        };
        let category = field("category");
        assert_eq!(category["other"], true);
        let labels: Vec<_> = category["choices"]
            .as_array()
            .unwrap()
            .iter()
            .map(|c| c["label"].as_str().unwrap())
            .collect();
        assert_eq!(labels, ["Art", "Music"]);
        assert_eq!(field("tags")["suggest"][0]["label"], "English");
    }

    #[test]
    fn an_older_read_cannot_replace_a_finished_change() {
        let mut seen = Seen::default();
        let before = seen.begin(false).unwrap();
        let change = seen.begin(true).unwrap();
        assert_eq!(seen.begin(false), None, "nothing starts while a change runs");
        assert!(seen.finish(change));
        assert!(!seen.finish(before));
        assert!(!seen.busy);
    }

    #[test]
    fn a_title_goes_to_every_platform_picked_and_the_rest_only_where_it_exists() {
        let _lock = crate::TEST_LOCK.lock().unwrap_or_else(|error| error.into_inner());
        let seen = ready([true, true, true, true]);
        let steps = info_steps(
            &item(
                json!({"title": "Hello", "category": "Just Chatting", "tags": ["English"],
                "twitch": true, "kick": true, "youtube": true, "tiktok": false}),
            ),
            &seen,
        )
        .unwrap();
        assert_eq!(
            steps,
            vec![
                (
                    TWITCH,
                    twitch_job(
                        Some("Just Chatting".into()),
                        json!({"title": "Hello", "tags": ["English"]})
                    )
                ),
                (
                    KICK,
                    kick_job(
                        Some("Hello".into()),
                        Some("Just Chatting".into()),
                        Some(vec!["English".into()])
                    )
                ),
                // YouTube's in one update, tags included.
                (YOUTUBE, video(Some("Hello".into()), None, Some(vec!["English".into()]))),
            ]
        );
        // The description goes only to YouTube, with what else it takes.
        let described = info_steps(
            &item(json!({"description": "About", "category": "Art", "twitch": true, "youtube": true})),
            &seen,
        )
        .unwrap();
        assert_eq!(
            described,
            vec![
                (TWITCH, twitch_job(Some("Art".into()), json!({}))),
                (YOUTUBE, video(None, Some("About".into()), None)),
            ]
        );
        // An empty field keeps what is there.
        let only_title = info_steps(&item(json!({"title": "Hi", "kick": true})), &seen).unwrap();
        assert_eq!(only_title, vec![(KICK, kick_job(Some("Hi".into()), None, None))]);
        // Remove every tag: whatever the tags box says.
        let cleared = info_steps(
            &item(json!({"tags": ["Kept"], "clear_tags": true, "kick": true, "youtube": true})),
            &seen,
        )
        .unwrap();
        assert_eq!(
            cleared,
            vec![
                (KICK, kick_job(None, None, Some(vec![]))),
                (YOUTUBE, video(None, None, Some(vec![])))
            ]
        );
    }

    #[test]
    fn the_info_form_refuses_what_cannot_run_and_says_why() {
        let _lock = crate::TEST_LOCK.lock().unwrap_or_else(|error| error.into_inner());
        let seen = ready([true, false, true, false]);
        let err = |value: Value| info_steps(&item(value), &seen).unwrap_err();
        // A platform that isn't ready is off, whatever the check box says.
        assert_eq!(err(json!({"title": "x", "kick": true})), tr("stream.pick_where"));
        assert_eq!(err(json!({"twitch": true})), tr("stream.nothing"));
        // A category alone means nothing to YouTube, a description nothing to Twitch.
        assert_eq!(err(json!({"category": "Art", "youtube": true})), tr("stream.nothing"));
        assert_eq!(err(json!({"description": "x", "twitch": true})), tr("stream.nothing"));
        let long = "x".repeat(101);
        assert!(err(json!({"title": long, "youtube": true})).contains("YouTube"));
        assert!(info_steps(&item(json!({"title": long, "twitch": true})), &seen).is_ok());
        assert!(err(json!({"tags": ["two words"], "twitch": true})).contains("two words"));
        let eleven: Vec<String> = (0..11).map(|i| format!("t{i}")).collect();
        assert_eq!(err(json!({"tags": eleven, "twitch": true})), tr("stream.tags_limit"));
        assert_eq!(
            err(json!({"description": "x".repeat(5001), "youtube": true})),
            tr("stream.description_limit")
        );
        assert_eq!(
            err(json!({"title": "a <b>", "youtube": true})),
            tr("stream.youtube_brackets")
        );
        assert!(info_steps(&item(json!({"title": "a <b>", "twitch": true})), &seen).is_ok());
        let many: Vec<String> = (0..60).map(|i| format!("tag number {i}")).collect();
        assert_eq!(
            err(json!({"tags": many, "youtube": true})),
            tr("stream.youtube_tags_limit")
        );
    }

    #[test]
    fn ads_run_only_where_ads_exist_with_the_length_picked() {
        let seen = ready([true, true, true, true]);
        let steps = ad_steps(
            &item(json!({"duration": 2, "twitch": true, "kick": true, "youtube": false, "tiktok": true})),
            &seen,
        )
        .unwrap();
        assert_eq!(steps, vec![(TWITCH, Job::Ad(90)), (KICK, Job::Ad(90))]);
        assert_eq!(
            ad_steps(&item(json!({"tiktok": true})), &seen).unwrap_err(),
            tr("stream.pick_where")
        );
        let last = ad_steps(&item(json!({"duration": 99, "youtube": true})), &seen).unwrap();
        assert_eq!(last, vec![(YOUTUBE, Job::Ad(180))]);
    }

    #[test]
    fn youtube_with_no_live_and_nothing_scheduled_offers_nothing_that_would_fail() {
        let mut seen = ready([true; 4]);
        seen.info[YOUTUBE].upcoming = true;
        nothing_to_change(&mut seen);
        assert!(seen.access.ready[YOUTUBE], "a scheduled live takes the changes");
        seen.info[YOUTUBE] = Info {
            nothing: true,
            ..Info::default()
        };
        nothing_to_change(&mut seen);
        assert!(!seen.access.ready[YOUTUBE]);
        assert_eq!(seen.access.why[YOUTUBE], tr("stream.youtube_nothing"));
        let sections = model(&seen);
        let thumbnail = find_button(&sections, "youtube.set_thumbnail");
        assert_eq!(thumbnail["enabled"], false);
        assert!(thumbnail_steps(&item(json!({"file": "a.png"})), &seen).is_err());
    }

    #[test]
    fn the_page_reads_again_when_opened_or_when_what_it_shows_changes() {
        let _lock = crate::TEST_LOCK.lock().unwrap_or_else(|error| error.into_inner());
        crate::engine::tests::run(async {
            let engine = crate::engine::tests::engine(json!({"actions": []}));
            reset(Seen::default());
            refresh_if_due(&engine);
            let asked = seen().requested;
            // Nothing changed and it read a moment ago: no new read.
            refresh_if_due(&engine);
            assert_eq!(seen().requested, asked);
            // Opened again: read at once.
            opened();
            refresh_if_due(&engine);
            assert_eq!(seen().requested, asked + 1);
            // A login (or a connection, or a live) changes what it can show: read at once.
            engine.change(|c| {
                c.kick.broadcaster = Some(crate::kick::KickAccount {
                    username: "ana".into(),
                    ..Default::default()
                })
            });
            refresh_if_due(&engine);
            assert_eq!(seen().requested, asked + 2);
        });
    }

    #[test]
    fn a_thumbnail_must_be_a_small_jpg_or_png_that_exists() {
        let seen = ready([false, false, true, false]);
        let dir = std::env::temp_dir().join(format!("streamerz-thumb-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let small = dir.join("small.png");
        std::fs::write(&small, [0u8; 16]).unwrap();
        let path = small.display().to_string();
        let file = |f: &str| item(json!({"file": f}));
        assert_eq!(
            thumbnail_steps(&file(&path), &seen).unwrap(),
            vec![(YOUTUBE, Job::Thumbnail(path.clone()))]
        );
        // Quoted, as a file manager copies it.
        assert!(thumbnail_steps(&file(&format!("\"{path}\"")), &seen).is_ok());
        assert_eq!(
            thumbnail_steps(&file("a.gif"), &seen).unwrap_err(),
            tr("stream.thumbnail_kind")
        );
        assert!(thumbnail_steps(&file("/nowhere/a.jpg"), &seen).is_err());
        let big = dir.join("big.jpg");
        std::fs::write(&big, vec![0u8; THUMBNAIL_MAX as usize + 1]).unwrap();
        assert_eq!(
            thumbnail_steps(&file(&big.display().to_string()), &seen).unwrap_err(),
            tr("stream.thumbnail_size")
        );
        assert_eq!(thumbnail_steps(&file(&path), &ready([false; 4])).unwrap_err(), "no 2");
        std::fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn clearing_tags_asks_twitch_kick_and_youtube() {
        let steps = clear_tags_steps(&ready([true, true, true, true]));
        assert_eq!(
            steps,
            vec![
                (TWITCH, twitch_job(None, json!({"tags": []}))),
                (KICK, kick_job(None, None, Some(vec![]))),
                (YOUTUBE, video(None, None, Some(vec![]))),
            ]
        );
        assert_eq!(
            steps[2].1.what(YOUTUBE),
            trf("stream.job_clear_tags", &[("where", "YouTube")])
        );
        assert!(clear_tags_steps(&ready([false, false, false, true])).is_empty());
    }

    #[test]
    fn ads_in_words() {
        assert_eq!(
            kick_ads(&json!({"opted_in": false}), Some(true)),
            tr("stream.ads_kick_out")
        );
        assert_eq!(
            kick_ads(&json!({"opted_in": true, "ads_blocked": true}), Some(true)),
            tr("stream.ads_blocked")
        );
        assert_eq!(
            kick_ads(&json!({"opted_in": true, "remaining_ad_breaks": 3}), Some(true)),
            trf("stream.ads_left", &[("count", "3")])
        );
        assert_eq!(
            twitch_ads(&json!({"next_ad_at": 0}), Some(true)),
            tr("stream.ads_ready")
        );
        assert_eq!(
            twitch_ads(&json!({"next_ad_at": ""}), Some(true)),
            tr("stream.ads_ready")
        );
        assert_eq!(
            twitch_ads(&json!({"next_ad_at": 0, "snooze_count": 2}), Some(true)),
            format!(
                "{} {}",
                tr("stream.ads_ready"),
                trf("stream.snoozes_left", &[("count", "2")])
            )
        );
        // Off air, Twitch's schedule is all zeros: it says ads wait for a live, not "Ready. 0 snoozes".
        let off = json!({"next_ad_at": 0, "snooze_count": 0, "preroll_free_time": 0});
        assert_eq!(twitch_ads(&off, Some(false)), tr("stream.ads_live_only"));
        assert_eq!(
            kick_ads(&json!({"opted_in": true}), Some(false)),
            tr("stream.ads_live_only")
        );
        assert_eq!(
            kick_ads(&json!({"opted_in": false}), Some(false)),
            tr("stream.ads_kick_out"),
            "not opted in says so, live or not"
        );
        assert_eq!(
            twitch_ads(&json!({"next_ad_at": 0, "preroll_free_time": 1200}), Some(true)),
            format!(
                "{} {}",
                tr("stream.ads_ready"),
                trf("stream.preroll_free", &[("minutes", "20")])
            )
        );
        let later = crate::oauth::now() as i64 + 600;
        for next in [json!(later), json!(later.to_string())] {
            assert_eq!(
                twitch_ads(&json!({"next_ad_at": next}), Some(true)),
                trf("stream.ads_next", &[("time", &crate::fmt::shown_clock(later))])
            );
        }
        assert_eq!(
            unix_time(&json!("2026-11-10T21:00:00Z")),
            crate::engine::rest_parse_iso("2026-11-10T21:00:00Z")
        );
    }

    #[test]
    fn every_platform_set_up_is_a_card_with_what_it_shows_and_why_it_cant_change() {
        let _lock = crate::TEST_LOCK.lock().unwrap_or_else(|error| error.into_inner());
        let mut seen = ready([false, true, false, false]);
        seen.access.shown = [true, true, true, true];
        seen.access.why[TWITCH] = "moderator".into();
        seen.access.state[YOUTUBE] = "stream.state_disconnected";
        seen.access.why[YOUTUBE] = "connect it".into();
        seen.access.state[TIKTOK] = "stream.state_offline";
        seen.info[TWITCH] = Info {
            title: "Coding".into(),
            category: "Software and Game Development".into(),
            tags: vec!["Rust".into(), "English".into()],
            known: true,
            ..Info::default()
        };
        seen.info[KICK] = Info {
            known: true,
            ads: trf("stream.ads_left", &[("count", "2")]),
            ..Info::default()
        };
        let sections = model(&seen);
        let cards: Vec<&str> = sections[1..].iter().map(|s| s["brand"].as_str().unwrap()).collect();
        assert_eq!(cards, ["twitch", "kick", "youtube", "tiktok"]);
        // A moderator sees the owner's stream: the title first, the category and tags as chips, and why
        // it can't be changed, in a word and in full.
        let twitch = card(&sections, "twitch");
        assert_eq!(twitch["headline"], "Coding");
        assert_eq!(twitch["headlineMuted"], false);
        assert_eq!(
            twitch["chips"],
            json!([{"icon": "layers", "text": "Software and Game Development"}, {"text": "Rust"}, {"text": "English"}])
        );
        assert_eq!(twitch["state"]["text"], tr("stream.state_read_only"));
        assert_eq!(twitch["state"]["tip"], "moderator");
        assert_eq!(twitch["lines"][0]["text"], "moderator");
        let edit = find_button(&sections, "twitch.edit");
        assert_eq!(
            (edit["enabled"].as_bool(), edit["tooltip"].as_str()),
            (Some(false), Some("moderator"))
        );
        assert_eq!(find_button(&sections, "twitch.marker")["enabled"], false);
        // Kick can be changed: no title is said as such, its ads too.
        let kick = card(&sections, "kick");
        assert_eq!(kick["state"]["health"], "up");
        assert_eq!(
            (kick["headline"].as_str(), kick["headlineMuted"].as_bool()),
            (Some(tr("stream.no_title")), Some(true))
        );
        assert_eq!(kick["lines"][0]["text"], trf("stream.ads_left", &[("count", "2")]));
        assert_eq!(find_button(&sections, "kick.edit")["form"], "stream_info.kick");
        assert!(find_button(&sections, "kick.edit")["enabled"].is_null());
        // Not connected: only why, and the way to Accounts.
        let youtube = card(&sections, "youtube");
        assert!(youtube["headline"].is_null());
        assert_eq!(youtube["lines"][0]["text"], "connect it");
        assert_eq!(youtube["buttons"].as_array().unwrap().len(), 1);
        assert_eq!(youtube["buttons"][0]["go"], "Accounts");
        // TikTok offline: its card says so, and that it isn't live (not that the title is unknown).
        let tiktok = card(&sections, "tiktok");
        assert_eq!(tiktok["state"]["text"], tr("stream.state_offline"));
        assert_eq!(tiktok["headline"], tr("stream.not_live"));
        // The top: the buttons for every platform and the combined overview.
        assert_eq!(sections[0]["rows"].as_array().unwrap().len(), 2);
        assert_eq!(sections[0]["rows"][1]["status"]["text"], overview(&seen).unwrap());
        assert!(find_button(&sections, "edit")["enabled"].is_null());
        // Nothing set up: no card, and the way to Accounts.
        let none = model(&Seen {
            received: 1,
            ..Seen::default()
        });
        assert_eq!(none.len(), 1);
        assert_eq!(none[0]["rows"][1]["status"]["text"], tr("stream.accounts_needed"));
        assert_eq!(find_button(&none, "accounts")["go"], "Accounts");
        // Busy: nothing starts.
        let mut busy = seen.clone();
        busy.busy = true;
        let busy = model(&busy);
        assert_eq!(find_button(&busy, "kick.edit")["enabled"], false);
        assert_eq!(busy[0]["rows"][1]["status"]["health"], "busy");
    }

    #[test]
    fn whose_channel_decides_what_can_change() {
        let _lock = crate::TEST_LOCK.lock().unwrap_or_else(|error| error.into_inner());
        crate::engine::tests::run(async {
            let token = json!({"accessToken": "a", "refreshToken": "r", "expiresAt": u64::MAX / 2});
            let engine = |config: Value| {
                let config: crate::Config = serde_json::from_value(config).unwrap();
                let e = Engine::new(std::sync::Arc::new(tokio::sync::watch::channel(config).0), |_| {});
                crate::scenario::fakes::install(&e, &json!({"twitch": {}, "kick": {}, "youtube": {}}), None);
                e
            };
            // Logged in as a moderator of someone else's Kick and Twitch channel.
            let guest = engine(json!({
                "twitch": {"channel": "owner", "broadcaster": {"userId": "1", "login": "me", "displayName": "Me", "token": token}},
                "kick": {"channel": "owner", "broadcaster": {"userId": 2, "username": "me", "token": token}}}));
            let a = access(&guest);
            assert_eq!(a.shown, [true, true, false, false]);
            assert_eq!(a.ready, [false; 4]);
            assert_eq!(
                a.why[KICK],
                trf("stream.not_owner", &[("account", "me"), ("channel", "owner")])
            );
            assert_eq!(
                a.why[TWITCH],
                trf("stream.not_owner", &[("account", "Me"), ("channel", "owner")])
            );
            // Their own channels, typed or not.
            let own = engine(json!({
                "twitch": {"broadcaster": {"userId": "1", "login": "me", "displayName": "Me", "token": token}},
                "kick": {"channel": "@Me", "broadcaster": {"userId": 2, "username": "me", "token": token}},
                "youtube": {"channelId": "UC1", "broadcaster": {"channelId": "UC1", "name": "Me", "token": token}}}));
            assert_eq!(access(&own).ready, [true, true, true, false]);
            // A channel typed but no login: listed, not changeable.
            let anonymous = engine(json!({"kick": {"channel": "someone"}}));
            let a = access(&anonymous);
            assert!(a.shown[KICK] && !a.ready[KICK]);
            assert_eq!(a.why[KICK], trf("stream.login_owner", &[("where", "Kick")]));
            crate::scenario::fakes::uninstall();
        });
    }

    #[test]
    fn each_form_asks_only_what_its_platforms_take() {
        let mut seen = ready([false, true, true, false]);
        seen.access.shown = [true, true, true, false];
        seen.info[KICK].title = "Live now".into();
        seen.info[KICK].tags = vec!["chill".into()];
        reset(seen);
        let keys = |form: &Value| -> Vec<String> {
            form["fields"]
                .as_array()
                .unwrap()
                .iter()
                .filter(|f| f["hidden"] != true)
                .map(|f| f["key"].as_str().unwrap().to_owned())
                .collect()
        };
        let all = external_form("stream_info").unwrap().unwrap();
        assert_eq!(all["item"]["title"], "Live now");
        assert_eq!(all["item"]["tags"], json!(["chill"]));
        assert_eq!(
            (all["item"]["twitch"].as_bool(), all["item"]["kick"].as_bool()),
            (Some(false), Some(true))
        );
        let twitch = all["fields"]
            .as_array()
            .unwrap()
            .iter()
            .find(|f| f["key"] == "twitch")
            .unwrap();
        assert_eq!(
            (twitch["disabled"].as_bool(), twitch["brand"].as_str()),
            (Some(true), Some("twitch"))
        );
        assert_eq!(twitch["tip"], "no 0");
        // One platform's form: only its fields, no platform boxes.
        let kick = external_form("stream_info.kick").unwrap().unwrap();
        assert_eq!(keys(&kick), ["title", "category", "tags", "clear_tags"]);
        assert_eq!(kick["item"]["kick"], true);
        assert_eq!(kick["title"], trf("stream.edit_on", &[("where", "Kick")]));
        let youtube = external_form("stream_info.youtube").unwrap().unwrap();
        assert_eq!(
            keys(&youtube),
            [
                "title",
                "tags",
                "description",
                "youtube_category",
                "privacy",
                "clear_tags"
            ]
        );
        assert_eq!(keys(&external_form("stream_info.tiktok").unwrap().unwrap()), ["title"]);
        let ad = external_form("stream_ad").unwrap().unwrap();
        assert_eq!(
            keys(&ad),
            ["duration", "twitch", "kick", "youtube"],
            "no TikTok box for ads"
        );
        let kick_ad = external_form("stream_ad.kick").unwrap().unwrap();
        assert_eq!(kick_ad["fields"][0]["widget"]["options"], json!(KICK_AD_NAMES));
        assert!(external_form("stream_ad.tiktok").is_none());
        let thumbnail = external_form("stream_thumbnail").unwrap().unwrap();
        assert_eq!(thumbnail["fields"][0]["browse"], "image");
        assert!(external_form("poll").is_none());
        assert!(external_fields("stream_info.youtube").is_some());
        assert!(external_fields("stream_info.myspace").is_none());
    }

    #[test]
    fn the_page_is_readable_and_its_buttons_answer_without_a_frontend() {
        let _lock = crate::TEST_LOCK.lock().unwrap_or_else(|error| error.into_inner());
        crate::engine::tests::run(async {
            let engine = crate::engine::tests::engine(json!({"actions": []}));
            reset(Seen::default());
            ASKED.set(None);
            let view = json!({"request": "StreamerzCoreView", "domain": "stream"});
            let first = crate::servers::answer(&engine, &view, true).unwrap();
            assert_eq!(
                first["sections"][0]["rows"][1]["status"]["health"], "busy",
                "still checking"
            );
            for _ in 0..4 {
                tokio::task::yield_now().await;
            }
            let second = crate::servers::answer(&engine, &view, true).unwrap();
            assert_eq!(
                second["sections"][0]["rows"][1]["status"]["text"],
                tr("stream.accounts_needed")
            );
            let act = |action: &str| {
                let request = json!({"request": "StreamerzCoreAction", "domain": "stream", "action": action});
                crate::servers::answer(&engine, &request, true).unwrap()
            };
            assert_eq!(act("refresh")["status"], "ok");
            assert_eq!(act("clear_tags")["status"], "error", "no Twitch, Kick or YouTube");
            assert_eq!(act("twitch.clip")["status"], "error");
            // The forms through the same requests an assistant uses.
            let form = json!({"request": "StreamerzEntityForm", "kind": "stream_ad", "itemId": "new"});
            let form = crate::servers::answer(&engine, &form, true).unwrap();
            assert_eq!(form["status"], "ok", "{form}");
        });
    }

    /// What the platforms were asked, as the fakes kept it.
    fn asked(fakes: &crate::scenario::fakes::Fakes) -> Vec<String> {
        fakes
            .calls
            .borrow()
            .iter()
            .map(|call| {
                let body = &call["body"];
                let mut line = format!(
                    "{} {}",
                    call["platform"].as_str().unwrap_or(""),
                    call["call"].as_str().unwrap_or("")
                );
                if !body.is_null() {
                    line.push_str(&format!(" {body}"));
                }
                for key in ["title", "tags", "description"] {
                    if !call[key].is_null() {
                        line.push_str(&format!(" {key}={}", call[key]));
                    }
                }
                line
            })
            .collect()
    }

    #[test]
    fn a_saved_form_reaches_each_platform_and_a_clip_says_its_link() {
        let _lock = crate::TEST_LOCK.lock().unwrap_or_else(|error| error.into_inner());
        crate::engine::tests::run(async {
            let token = json!({"accessToken": "a", "refreshToken": "r", "expiresAt": u64::MAX / 2});
            let config: crate::Config = serde_json::from_value(json!({
                "twitch": {"broadcaster": {"userId": "1", "login": "me", "displayName": "Me", "token": token}},
                "kick": {"channel": "me", "broadcaster": {"userId": 2, "username": "me", "token": token}},
                "youtube": {"broadcaster": {"channelId": "UC1", "name": "Me", "token": token}}}))
            .unwrap();
            let engine = Engine::new(std::sync::Arc::new(tokio::sync::watch::channel(config).0), |_| {});
            let fakes = crate::scenario::fakes::install(
                &engine,
                &json!({"twitch": {
                    "GET channels": {"data": [{"title": "Old", "game_name": "Art", "tags": ["English"]}]},
                    "GET streams": {"data": [{"type": "live", "started_at": "2026-11-10T21:00:00Z", "viewer_count": 12}]},
                    "POST clips": {"data": [{"id": "Abc"}]}},
                "kick": {"GET /channels?slug=me": {"data": [{"stream_title": "Kick title", "category": {"name": "IRL"}}]}},
                "youtube": {
                    "GET liveBroadcasts?part=snippet,status,contentDetails&mine=true&maxResults=10": {"items": [
                        {"id": "old", "snippet": {"title": "Done"}, "status": {"lifeCycleStatus": "complete"}},
                        {"id": "v2", "snippet": {"title": "Next live"},
                         "status": {"lifeCycleStatus": "ready", "privacyStatus": "private"},
                         "contentDetails": {"boundStreamId": "s1"}}]},
                    "GET videos?part=snippet,status,liveStreamingDetails,statistics&id=v2": {"items": [{
                        "snippet": {"title": "Next live", "description": "Old about", "tags": ["Old"], "categoryId": "20"},
                        "status": {"privacyStatus": "private"}}]}}}),
                None,
            );
            reset(Seen::default());
            let settle = || async {
                for _ in 0..40 {
                    tokio::task::yield_now().await;
                }
            };
            ask(&engine, Vec::new(), true);
            settle().await;
            assert_eq!(seen().access.ready, [true, true, true, false], "TikTok isn't connected");
            assert_eq!(seen().info[TWITCH].title, "Old");
            assert_eq!(seen().info[TWITCH].live, Some(true));
            assert_eq!(seen().info[TWITCH].viewers, Some(12));
            assert_eq!(
                seen().info[TWITCH].since,
                crate::engine::rest_parse_iso("2026-11-10T21:00:00Z")
            );
            assert_eq!(seen().info[KICK].title, "Kick title", "read by the channel's slug");
            assert_eq!(seen().info[KICK].category, "IRL");
            // Not live on YouTube: what changes go to is the next live, shown with its title.
            let youtube = seen().info[YOUTUBE].clone();
            assert!(youtube.upcoming);
            assert_eq!(youtube.live, Some(false));
            assert_eq!(youtube.broadcast_id, "v2");
            assert_eq!(
                youtube.title, "Next live",
                "the broadcast still to come, not the finished one"
            );
            // Read back through the Data API: what the form starts with.
            assert_eq!(youtube.description, "Old about");
            assert_eq!(youtube.tags, ["Old"]);
            assert_eq!(youtube.category, "Gaming");
            assert_eq!(youtube.privacy, "private");
            assert!(youtube.health.is_empty(), "a stream's health is read only while live");
            let form = external_form("stream_info.youtube").unwrap().unwrap();
            assert_eq!(form["item"]["description"], "Old about");
            assert_eq!(form["item"]["tags"], json!(["Old"]));
            let page = model(&seen());
            let lines = card(&page, "youtube")["lines"].to_string();
            assert!(lines.contains(tr("stream.youtube_private")), "{lines}");
            // Off air: ads can't run anywhere, and the buttons say so before they're pressed.
            assert_eq!(find_button(&page, "youtube.ad")["enabled"], false);
            fakes.calls.borrow_mut().clear();
            let form = item(json!({"title": "New title", "tags": ["Rust"], "description": "About",
                "twitch": true, "kick": true, "youtube": true, "tiktok": false}));
            assert!(save_external(&engine, "stream_info", form).unwrap().is_ok());
            settle().await;
            let calls = asked(&fakes);
            for wanted in [
                r#"twitch PATCH channels {"title":"New title","tags":["Rust"]}"#,
                r#"kick PATCH /channels {"stream_title":"New title","custom_tags":["Rust"]}"#,
                r#"youtube updateVideo title="New title" tags=["Rust"] description="About""#,
            ] {
                assert!(calls.iter().any(|call| call == wanted), "{wanted} not in {calls:#?}");
            }
            let yt_call = fakes
                .calls
                .borrow()
                .iter()
                .find(|call| call["platform"] == "youtube" && call["call"] == "updateVideo")
                .cloned()
                .unwrap();
            assert_eq!(yt_call["broadcast"], "v2");
            assert!(!seen().trouble, "{}", seen().note);
            fakes.calls.borrow_mut().clear();
            assert!(save_external(
                &engine,
                "stream_info.youtube",
                item(json!({"youtube_category": "10", "privacy": "unlisted"}))
            )
            .unwrap()
            .is_ok());
            settle().await;
            let call = fakes
                .calls
                .borrow()
                .iter()
                .find(|call| call["call"] == "updateVideo")
                .cloned()
                .unwrap();
            assert_eq!(call["category"], "10");
            assert_eq!(call["privacy"], "unlisted");
            fakes.calls.borrow_mut().clear();
            assert!(save_external(
                &engine,
                "stream_info.twitch",
                item(json!({"language": "en", "branded": false, "label_gambling": true}))
            )
            .unwrap()
            .is_ok());
            settle().await;
            let calls = asked(&fakes);
            assert!(
                calls.iter().any(|call| call.contains("broadcaster_language")
                    && call.contains("is_branded_content")
                    && call.contains("Gambling")),
                "{calls:?}"
            );
            // One platform's form changes that platform only, whatever its item says.
            fakes.calls.borrow_mut().clear();
            let kick_only = item(json!({"title": "Only Kick", "twitch": true}));
            assert!(save_external(&engine, "stream_info.kick", kick_only).unwrap().is_ok());
            settle().await;
            let calls = asked(&fakes);
            assert!(
                calls
                    .iter()
                    .any(|call| call == r#"kick PATCH /channels {"stream_title":"Only Kick"}"#),
                "{calls:#?}"
            );
            assert!(!calls.iter().any(|call| call.starts_with("twitch PATCH")), "{calls:#?}");
            assert!(act(&engine, "twitch.clip").is_ok());
            settle().await;
            assert!(seen().note.contains("https://clips.twitch.tv/Abc"), "{}", seen().note);
            crate::scenario::fakes::uninstall();
        });
    }

    #[test]
    fn what_needs_a_live_is_off_until_one_starts() {
        let _lock = crate::TEST_LOCK.lock().unwrap_or_else(|error| error.into_inner());
        let mut now = ready([true, true, true, false]);
        for i in [TWITCH, KICK, YOUTUBE] {
            now.info[i].live = Some(false);
            now.info[i].known = true;
        }
        reset(now.clone());
        let page = model(&now);
        for id in [
            "twitch.ad",
            "twitch.snooze",
            "twitch.marker",
            "twitch.clip",
            "kick.ad",
            "youtube.ad",
            "run_ad",
        ] {
            let button = find_button(&page, id);
            assert_eq!(button["enabled"], false, "{id}");
            assert_eq!(button["tooltip"], tr("stream.needs_live"), "{id}");
        }
        // Changing the title doesn't need a live.
        assert!(find_button(&page, "twitch.edit").get("enabled").is_none());
        assert!(find_button(&page, "edit").get("enabled").is_none());
        assert_eq!(card(&page, "twitch")["state"]["text"], tr("stream.state_not_live"));
        // The same through the requests, and the ad form picks nothing that would fail.
        crate::engine::tests::run(async {
            let engine = crate::engine::tests::engine(json!({"actions": []}));
            assert_eq!(act(&engine, "twitch.clip").unwrap_err(), tr("stream.needs_live"));
        });
        let form = external_form("stream_ad").unwrap().unwrap();
        assert_eq!(form["item"]["twitch"], false);
        let ad = item(json!({"duration": 0, "twitch": true, "kick": true}));
        assert_eq!(ad_steps(&ad, &now).unwrap_err(), tr("stream.pick_where"));
        // Live on Twitch: its card says since when and how many watch (the chat's count wins).
        let mut live = now.clone();
        live.info[TWITCH].live = Some(true);
        live.info[TWITCH].since = Some(1_800_000_000);
        live.info[TWITCH].viewers = Some(5);
        live.viewers[TWITCH] = Some(7);
        let page = model(&live);
        let twitch = card(&page, "twitch");
        assert_eq!(twitch["state"]["text"], tr("stream.state_live"));
        let text = twitch["lines"][0]["text"].as_str().unwrap();
        assert!(text.contains(&crate::fmt::shown_clock(1_800_000_000)), "{text}");
        assert!(text.contains(&trf("stream.watching", &[("count", "7")])), "{text}");
        assert!(find_button(&page, "twitch.clip").get("enabled").is_none());
        assert!(ad_steps(&ad, &live).is_ok());
    }

    #[test]
    fn overview_keeps_unknown_counts_and_kick_login_has_an_accounts_route() {
        let _lock = crate::TEST_LOCK.lock().unwrap_or_else(|error| error.into_inner());
        let mut now = ready([true, true, true, false]);
        now.access.shown[TIKTOK] = false;
        now.info[TWITCH].live = Some(true);
        now.info[TWITCH].viewers = Some(10);
        now.info[KICK].live = Some(false);
        now.info[YOUTUBE].live = Some(true);
        now.info[YOUTUBE].viewers = Some(20);
        now.viewers[TWITCH] = Some(15);
        let text = overview(&now).unwrap();
        assert!(text.contains(&trf("stream.total_viewers", &[("count", "35")])));
        now.info[YOUTUBE].viewers = None;
        assert!(!overview(&now).unwrap().contains("35"));
        now.info[KICK].ads_login = true;
        let card = platform_card(&now, KICK);
        assert!(card["buttons"]
            .as_array()
            .unwrap()
            .iter()
            .any(|button| button["go"] == "Accounts"));
        assert_eq!(overview(&Seen::default()), None);
    }

    #[test]
    fn a_tiktok_chat_not_joined_to_a_live_counts_as_not_live() {
        let _lock = crate::TEST_LOCK.lock().unwrap_or_else(|error| error.into_inner());
        crate::engine::tests::run(async {
            let engine = crate::engine::tests::engine(json!({"actions": []}));
            let info = look(&engine, TIKTOK, false).await;
            assert_eq!(info.live, Some(false), "not unknown: the card says Not live too");
            let mut now = ready([false; 4]);
            for i in [TWITCH, KICK, YOUTUBE] {
                now.info[i].live = Some(false);
            }
            now.info[TIKTOK] = info;
            assert_eq!(
                overview(&now).unwrap(),
                trf("stream.overview", &[("live", "0"), ("offline", "4"), ("unknown", "0")])
            );
        });
    }

    #[test]
    fn platform_options_are_prefilled_validated_and_only_changed_values_are_sent() {
        let _lock = crate::TEST_LOCK.lock().unwrap_or_else(|error| error.into_inner());
        let mut now = ready([true, true, true, false]);
        twitch_channel(
            &mut now.info[TWITCH],
            &json!({"title": "Same", "broadcaster_language": "pt",
            "content_classification_labels": ["Gambling", "MatureGame"], "is_branded_content": true}),
        );
        now.info[YOUTUBE] = Info {
            known: true,
            category_id: "20".into(),
            privacy: "private".into(),
            ..Info::default()
        };
        reset(now.clone());
        let form = external_form("stream_info.twitch").unwrap().unwrap();
        assert_eq!(form["item"]["language"], "pt");
        assert_eq!(form["item"]["branded"], true);
        assert_eq!(form["item"]["label_gambling"], true);
        assert_eq!(form["item"]["label_drugs"], false);
        let mut unchanged = item(form["item"].clone());
        unchanged.insert("twitch".into(), true.into());
        assert!(info_steps(&unchanged, &now).is_err());
        let edits = item(json!({"twitch": true, "language": "en", "branded": false,
            "label_gambling": false, "label_drugs": true}));
        assert_eq!(
            info_steps(&edits, &now).unwrap(),
            vec![(
                TWITCH,
                twitch_job(
                    None,
                    json!({
                        "broadcaster_language": "en",
                        "is_branded_content": false,
                        "content_classification_labels": [
                            {"id": "DrugsIntoxication", "is_enabled": true},
                            {"id": "Gambling", "is_enabled": false}
                        ]
                    })
                )
            )]
        );
        // An omitted label, including Twitch's automatic MatureGame label, is never disabled.
        assert_eq!(
            info_steps(&item(json!({"twitch": true, "language": "xx"})), &now).unwrap_err(),
            tr("stream.language_bad")
        );
        let form = external_form("stream_info.youtube").unwrap().unwrap();
        assert_eq!(form["item"]["youtube_category"], "20");
        assert_eq!(form["item"]["privacy"], "private");
        assert!(form["fields"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|f| matches!(f["key"].as_str(), Some("privacy" | "youtube_category")))
            .all(|f| f["strict"] == true));
        let edits = item(json!({"youtube": true, "title": "New", "youtube_category": "10", "privacy": "unlisted"}));
        assert_eq!(
            info_steps(&edits, &now).unwrap(),
            vec![(
                YOUTUBE,
                Job::Video(VideoUpdate {
                    title: Some("New".into()),
                    category: Some("10".into()),
                    privacy: Some("unlisted".into()),
                    ..VideoUpdate::default()
                })
            )]
        );
        assert_eq!(
            info_steps(
                &item(json!({"youtube": true, "youtube_category": "20", "privacy": "private"})),
                &now
            )
            .unwrap_err(),
            tr("stream.unchanged")
        );
        for bad in [
            json!({"youtube": true, "youtube_category": "999"}),
            json!({"youtube": true, "privacy": "friends"}),
        ] {
            assert!(info_steps(&item(bad), &now).is_err());
        }
        // Platform-specific fields do not spill into a different selected platform.
        assert!(info_steps(
            &item(json!({"kick": true, "privacy": "public", "branded": false})),
            &now
        )
        .is_err());
    }

    #[test]
    fn what_a_platform_already_says_is_not_sent_again() {
        let mut now = ready([true, true, true, false]);
        now.info[TWITCH] = Info {
            title: "Same".into(),
            category: "Art".into(),
            known: true,
            ..Info::default()
        };
        now.info[YOUTUBE] = Info {
            title: "Same".into(),
            description: "About".into(),
            known: true,
            ..Info::default()
        };
        let form = item(json!({"title": "Same", "category": "art", "description": "About",
            "twitch": true, "kick": false, "youtube": true}));
        assert_eq!(info_steps(&form, &now).unwrap_err(), tr("stream.unchanged"));
        let form = item(json!({"title": "New", "category": "Art", "description": "About",
            "twitch": true, "kick": false, "youtube": true}));
        assert_eq!(
            info_steps(&form, &now).unwrap(),
            vec![
                (TWITCH, twitch_job(None, json!({"title": "New"}))),
                (YOUTUBE, video(Some("New".into()), None, None)),
            ],
            "only the title changed, and YouTube's description stays out of its update"
        );
    }

    #[test]
    fn titles_and_lives_pushed_by_the_platforms_show_at_once() {
        let _lock = crate::TEST_LOCK.lock().unwrap_or_else(|error| error.into_inner());
        reset(ready([true, true, true, true]));
        pushed("twitch", Some("From the dashboard"), Some("Just Chatting"));
        assert_eq!(seen().info[TWITCH].title, "From the dashboard");
        assert_eq!(seen().info[TWITCH].category, "Just Chatting");
        pushed("kick", Some("Kick title"), None);
        assert_eq!(seen().info[KICK].title, "Kick title");
        live_changed("kick", true);
        assert_eq!(seen().info[KICK].live, Some(true));
        assert!(seen().info[KICK].since.is_some());
        live_changed("kick", false);
        assert_eq!(seen().info[KICK].live, Some(false));
        assert_eq!(seen().info[KICK].since, None);
        // Before the page has read anything, a push waits for that read.
        reset(Seen::default());
        pushed("twitch", Some("Early"), None);
        assert_eq!(seen().info[TWITCH].title, "");
    }

    /// Real answers (trimmed), as the platforms gave them on 2026-11-15.
    #[test]
    fn each_platform_answer_reads_into_what_the_card_shows() {
        let mut twitch = Info::default();
        twitch_channel(
            &mut twitch,
            &json!({"broadcaster_language": "pt", "game_id": "509660", "game_name": "Art",
            "title": "Fazendo os Emotes (finalmente)", "delay": 0, "tags": [], "content_classification_labels": [],
            "is_branded_content": false}),
        );
        twitch_stream(&mut twitch, &Value::Null);
        assert_eq!(
            (twitch.title.as_str(), twitch.category.as_str()),
            ("Fazendo os Emotes (finalmente)", "Art")
        );
        assert_eq!(twitch.live, Some(false), "Get Streams lists nothing off air");
        let mut kick = Info::default();
        kick_channel(
            &mut kick,
            &json!({"broadcaster_user_id": 130795575, "slug": "pedrozamecki",
            "stream": {"is_live": false, "start_time": "0001-01-01T00:00:00Z", "viewer_count": 0},
            "stream_title": "", "category": {"id": 0, "name": ""}}),
        );
        assert_eq!(kick.live, Some(false));
        assert_eq!(kick.since, None, "Kick's year-one start means none");
        kick_channel(
            &mut kick,
            &json!({"stream": {"is_live": true, "start_time": "2026-11-15T01:00:00Z",
            "viewer_count": 40}, "stream_title": "Live", "category": {"name": "Art"}}),
        );
        kick_livestream(
            &mut kick,
            &json!({"custom_tags": ["Brasil", "Art"], "started_at": "2026-11-15T01:00:00Z"}),
        );
        assert_eq!((kick.live, kick.viewers), (Some(true), Some(40)));
        assert_eq!(kick.tags, ["Brasil", "Art"], "the live has the tags");
        let items = [
            json!({"id": "477T5BH175Q", "snippet": {"title": "Old"}, "status": {"lifeCycleStatus": "complete"}}),
            json!({"id": "5ZsNztCxdhk", "snippet": {"title": "Transmissão teste"},
                "status": {"lifeCycleStatus": "ready", "privacyStatus": "private"}}),
        ];
        assert_eq!(youtube_pick(&items).unwrap()["id"], "5ZsNztCxdhk");
        let live = json!({"id": "x", "snippet": {"title": "On", "actualStartTime": "2026-11-15T01:00:00Z"},
            "status": {"lifeCycleStatus": "live", "privacyStatus": "public"}});
        let video = json!({"snippet": {"title": "On", "description": "Hi", "tags": ["a"], "categoryId": "28"},
            "status": {"privacyStatus": "public"}, "liveStreamingDetails": {"concurrentViewers": "31",
            "actualStartTime": "2026-11-15T01:00:00Z"}});
        let mut youtube = Info::default();
        youtube_broadcast(&mut youtube, &live, &video);
        assert_eq!((youtube.live, youtube.viewers), (Some(true), Some(31)));
        assert_eq!(youtube.category, "Science & Technology");
        assert!(!youtube.upcoming);
        youtube_health(
            &mut youtube,
            &json!({"status": {"healthStatus": {"status": "bad", "configurationIssues": [
            {"severity": "info", "description": "Fine"},
            {"severity": "error", "description": "The bitrate is too low."}]}}}),
        );
        assert_eq!(
            (youtube.health.as_str(), youtube.issue.as_str()),
            ("bad", "The bitrate is too low.")
        );
    }

    #[test]
    fn batched_category_and_title_resolve_and_patch_once_per_platform() {
        let _lock = crate::TEST_LOCK.lock().unwrap_or_else(|error| error.into_inner());
        crate::engine::tests::run(async {
            let token = json!({"accessToken": "a", "refreshToken": "r", "expiresAt": u64::MAX / 2});
            let config: crate::Config = serde_json::from_value(json!({
                "twitch": {"broadcaster": {"userId": "1", "login": "me", "displayName": "Me", "token": token}},
                "kick": {"channel": "me", "broadcaster": {"userId": 2, "username": "me", "token": token}}}))
            .unwrap();
            let engine = Engine::new(std::sync::Arc::new(tokio::sync::watch::channel(config).0), |_| {});
            let fakes = crate::scenario::fakes::install(
                &engine,
                &json!({
                    "twitch": {
                        "GET channels": {"data": [{"title": "Old", "game_name": "Art", "tags": ["English"]}]},
                        "GET streams": {"data": []},
                        "GET games": {"data": [{"id": "509658", "name": "Just Chatting"}]}
                    },
                    "kick": {
                        "GET /channels?slug=me": {"data": [{"stream_title": "Old", "category": {"name": "Art"}, "stream": {"is_live": false}}]},
                        "GET https://api.kick.com/public/v2/categories?name=Art+Game": {"data": []},
                        "GET /categories?q=Art+Game": {"data": [{"id": 42, "name": "Art Game"}]}
                    }
                }),
                None,
            );
            reset(Seen::default());
            let settle = || async {
                for _ in 0..40 {
                    tokio::task::yield_now().await;
                }
            };
            ask(&engine, Vec::new(), true);
            settle().await;
            fakes.calls.borrow_mut().clear();

            assert!(save_external(
                &engine,
                "stream_info.twitch",
                item(json!({
                    "title": "New Twitch",
                    "category": "Just Chatting",
                    "tags": ["Coding"],
                    "language": "en",
                    "delay": 15
                }))
            )
            .unwrap()
            .is_ok());
            settle().await;
            let twitch_patches: Vec<Value> = fakes
                .calls
                .borrow()
                .iter()
                .filter(|c| c["platform"] == "twitch" && c["call"] == "PATCH channels")
                .cloned()
                .collect();
            assert_eq!(twitch_patches.len(), 1, "batched into one Twitch PATCH");
            let body = &twitch_patches[0]["body"];
            assert_eq!(body["game_id"], "509658");
            assert_eq!(body["title"], "New Twitch");
            assert_eq!(body["tags"], json!(["Coding"]));
            assert_eq!(body["broadcaster_language"], "en");
            assert_eq!(body["delay"], 15);

            fakes.calls.borrow_mut().clear();
            assert!(save_external(
                &engine,
                "stream_info.kick",
                item(json!({
                    "title": "New Kick",
                    "category": "Art Game",
                    "tags": ["Chill"]
                }))
            )
            .unwrap()
            .is_ok());
            settle().await;
            let calls = asked(&fakes);
            assert!(calls
                .iter()
                .any(|c| c == "kick GET https://api.kick.com/public/v2/categories?name=Art+Game"));
            assert!(calls.iter().any(|c| c == "kick GET /categories?q=Art+Game"));
            let kick_patches: Vec<Value> = fakes
                .calls
                .borrow()
                .iter()
                .filter(|c| c["platform"] == "kick" && c["call"] == "PATCH /channels")
                .cloned()
                .collect();
            assert_eq!(kick_patches.len(), 1, "batched into one Kick PATCH");
            assert_eq!(
                kick_patches[0]["body"],
                json!({"category_id": 42, "stream_title": "New Kick", "custom_tags": ["Chill"]})
            );
            crate::scenario::fakes::uninstall();
        });
    }

    #[test]
    fn delay_field_prefills_when_positive_and_validates_bounds() {
        let _lock = crate::TEST_LOCK.lock().unwrap_or_else(|error| error.into_inner());
        let mut now = ready([true, false, false, false]);
        twitch_channel(&mut now.info[TWITCH], &json!({"title": "S", "delay": 0}));
        reset(now.clone());
        let form = external_form("stream_info.twitch").unwrap().unwrap();
        let delay_field = form["fields"]
            .as_array()
            .unwrap()
            .iter()
            .find(|f| f["key"] == "delay")
            .unwrap();
        assert_eq!(delay_field["hidden"], true, "delay=0 hidden for non-partner");

        now.info[TWITCH].delay = Some(30);
        reset(now.clone());
        let form = external_form("stream_info.twitch").unwrap().unwrap();
        let delay_field = form["fields"]
            .as_array()
            .unwrap()
            .iter()
            .find(|f| f["key"] == "delay")
            .unwrap();
        assert!(delay_field["hidden"].is_null());
        assert_eq!(form["item"]["delay"], 30);

        assert_eq!(
            info_steps(&item(json!({"twitch": true, "delay": 901})), &now).unwrap_err(),
            tr("stream.delay_bad")
        );
        assert_eq!(
            info_steps(&item(json!({"twitch": true, "delay": -1})), &now).unwrap_err(),
            tr("stream.delay_bad")
        );
        assert_eq!(
            info_steps(&item(json!({"twitch": true, "title": "S", "delay": 30})), &now).unwrap_err(),
            tr("stream.unchanged")
        );
        assert_eq!(
            info_steps(&item(json!({"twitch": true, "delay": 45})), &now).unwrap(),
            vec![(TWITCH, twitch_job(None, json!({"delay": 45})))]
        );
    }

    #[test]
    fn youtube_quota_pacing_testing_state_and_cached_broadcast_id() {
        let _lock = crate::TEST_LOCK.lock().unwrap_or_else(|error| error.into_inner());
        crate::engine::tests::run(async {
            let token = json!({"accessToken": "a", "refreshToken": "r", "expiresAt": u64::MAX / 2});
            let config: crate::Config = serde_json::from_value(json!({
                "youtube": {"broadcaster": {"channelId": "UC1", "name": "Me", "token": token}}
            }))
            .unwrap();
            let engine = Engine::new(std::sync::Arc::new(tokio::sync::watch::channel(config).0), |_| {});
            let scheduled = crate::oauth::now() as i64 + 3600;
            let fakes = crate::scenario::fakes::install(
                &engine,
                &json!({
                    "youtube": {
                        "GET liveBroadcasts?part=snippet,status,contentDetails&mine=true&maxResults=10": {
                            "$seq": [
                                {"error": "HTTP 500"},
                                {"items": [{
                                    "id": "yt-test-1",
                                    "snippet": {"title": "Testing stream", "scheduledStartTime": "2026-11-15T20:00:00Z"},
                                    "status": {"lifeCycleStatus": "testing", "privacyStatus": "unlisted"},
                                    "contentDetails": {
                                        "boundStreamId": "s-test",
                                        "latencyPreference": "ultraLow",
                                        "enableDvr": false,
                                        "enableAutoStart": true,
                                        "enableAutoStop": false
                                    }
                                }]}
                            ]
                        },
                        "GET videos?part=snippet,status,liveStreamingDetails,statistics&id=yt-test-1": {"items": [{
                            "snippet": {"title": "Testing stream", "description": "Desc", "tags": ["rust"], "categoryId": "28"},
                            "status": {"privacyStatus": "unlisted", "madeForKids": true},
                            "liveStreamingDetails": {"scheduledStartTime": scheduled},
                            "statistics": {"likeCount": "42"}
                        }]},
                        "GET liveStreams?part=status,cdn&id=s-test": {"items": [{
                            "status": {"streamStatus": "active", "healthStatus": {"status": "good"}},
                            "cdn": {"resolution": "1080p", "frameRate": "60fps"}
                        }]}
                    }
                }),
                None,
            );
            let settle = || async {
                for _ in 0..40 {
                    tokio::task::yield_now().await;
                }
            };
            reset(Seen::default());
            YOUTUBE_ASKED.set(None);
            SITUATION.set(None);

            // Initial read fails: YOUTUBE_ASKED must remain None so re-opening retries immediately.
            refresh_if_due(&engine);
            settle().await;
            assert!(
                YOUTUBE_ASKED.get().is_none(),
                "error must not set YOUTUBE_ASKED cooldown"
            );
            assert!(!seen().info[YOUTUBE].error.is_empty());

            // Re-opening after an error retries YouTube immediately and populates YOUTUBE_ASKED on success.
            opened();
            refresh_if_due(&engine);
            settle().await;
            assert!(YOUTUBE_ASKED.get().is_some());
            let yt = seen().info[YOUTUBE].clone();
            assert_eq!(yt.broadcast_id, "yt-test-1");
            assert_eq!(yt.lifecycle, "testing");
            assert_eq!(yt.latency, "ultraLow");
            assert_eq!(yt.dvr, Some(false));
            assert_eq!(yt.auto_start, Some(true));
            assert_eq!(yt.auto_stop, Some(false));
            assert_eq!(yt.made_for_kids, Some(true));
            assert_eq!(yt.likes, Some(42));
            assert_eq!(yt.stream_video, "1080p 60fps");
            assert_eq!(yt.health, "good");

            let page = model(&seen());
            let yt_card = card(&page, "youtube");
            assert_eq!(yt_card["state"]["health"], "busy");
            assert_eq!(yt_card["state"]["text"], tr("stream.state_testing"));
            let lines_text = yt_card["lines"].to_string();
            assert!(lines_text.contains(tr("stream.youtube_kids")));
            assert!(lines_text.contains("1080p 60fps"));
            assert!(lines_text.contains(tr("stream.latency_ultra_low")));
            assert!(lines_text.contains(tr("stream.dvr_off")));
            assert!(lines_text.contains(tr("stream.auto_start_on")));
            assert!(lines_text.contains(&trf("stream.likes_count", &[("count", "42")])));

            // Re-opening within YOUTUBE_OPEN_COOLDOWN (30s) does NOT hit YouTube Data API again.
            fakes.calls.borrow_mut().clear();
            opened();
            refresh_if_due(&engine);
            settle().await;
            assert!(
                !fakes
                    .calls
                    .borrow()
                    .iter()
                    .any(|c| c["platform"] == "youtube" && c["call"].as_str().is_some_and(|s| s.starts_with("GET"))),
                "rapid opened() must respect YOUTUBE_OPEN_COOLDOWN"
            );

            // Pressing Refresh DOES force a YouTube read.
            assert!(act(&engine, "refresh").is_ok());
            settle().await;
            assert!(fakes.calls.borrow().iter().any(|c| c["platform"] == "youtube"
                && c["call"].as_str().is_some_and(|s| s.starts_with("GET liveBroadcasts"))));

            // Cached broadcast_id is passed into setThumbnail and runCommercial (no extra lookup).
            fakes.calls.borrow_mut().clear();
            let dir = std::env::temp_dir().join(format!("streamerz-yt-thumb-{}", std::process::id()));
            std::fs::create_dir_all(&dir).unwrap();
            let thumb = dir.join("t.png");
            std::fs::write(&thumb, [0u8; 8]).unwrap();
            assert!(save_external(
                &engine,
                "stream_thumbnail",
                item(json!({"file": thumb.display().to_string()}))
            )
            .unwrap()
            .is_ok());
            settle().await;
            let thumb_call = fakes
                .calls
                .borrow()
                .iter()
                .find(|c| c["platform"] == "youtube" && c["call"] == "setThumbnail")
                .cloned()
                .unwrap();
            assert_eq!(thumb_call["broadcast"], "yt-test-1");
            std::fs::remove_dir_all(dir).ok();

            fakes.calls.borrow_mut().clear();
            live_changed("youtube", true);
            assert!(YOUTUBE_ASKED.get().is_none(), "live_changed clears YouTube cooldown");
            assert!(
                save_external(&engine, "stream_ad.youtube", item(json!({"duration": 0})))
                    .unwrap()
                    .is_ok()
            );
            settle().await;
            let ad_call = fakes
                .calls
                .borrow()
                .iter()
                .find(|c| c["platform"] == "youtube" && c["call"] == "runCommercial")
                .cloned()
                .unwrap();
            assert_eq!(ad_call["broadcast"], "yt-test-1");
            assert_eq!(ad_call["duration"], 30);

            crate::scenario::fakes::uninstall();
        });
    }

    #[test]
    fn initial_guard_eventsub_kick_tiktok_and_obs_status() {
        let _lock = crate::TEST_LOCK.lock().unwrap_or_else(|error| error.into_inner());

        // Before initial read (received == 0), live-only actions & ads must be disabled.
        let mut initial = ready([true, true, true, true]);
        initial.received = 0;
        reset(initial.clone());
        let page = model(&initial);
        for id in [
            "twitch.ad",
            "twitch.snooze",
            "twitch.marker",
            "twitch.clip",
            "kick.ad",
            "youtube.ad",
            "run_ad",
        ] {
            let btn = find_button(&page, id);
            assert_eq!(btn["enabled"], false, "{id} disabled before initial read");
            assert_eq!(btn["tooltip"], tr("stream.checking"));
        }
        crate::engine::tests::run(async {
            let engine = crate::engine::tests::engine(json!({"actions": []}));
            assert_eq!(act(&engine, "twitch.clip").unwrap_err(), tr("stream.checking"));
            assert_eq!(act(&engine, "twitch.snooze").unwrap_err(), tr("stream.checking"));
        });

        // EventSub pushed_twitch + ad_started + twitch_ads with duration/last_ad_at.
        let mut now = ready([true, true, false, true]);
        now.info[TWITCH].live = Some(true);
        now.info[KICK].live = Some(true);
        now.info[TIKTOK].live = Some(true);
        now.info[TIKTOK].paused = true;
        now.info[TIKTOK].likes = Some(500);
        now.info[TIKTOK].coins = Some(25);
        reset(now.clone());

        pushed_twitch(
            Some("EventSub Title"),
            Some("Music"),
            Some("es"),
            Some(vec!["Gambling".into()]),
        );
        assert_eq!(seen().info[TWITCH].title, "EventSub Title");
        assert_eq!(seen().info[TWITCH].category, "Music");
        assert_eq!(seen().info[TWITCH].language, "es");
        assert_eq!(seen().info[TWITCH].labels, vec!["Gambling".to_string()]);

        ad_started("twitch", 90);
        assert_eq!(seen().info[TWITCH].ads, trf("stream.ad_running", &[("seconds", "90")]));

        let past = crate::oauth::now() as i64 - 120;
        let future = crate::oauth::now() as i64 + 600;
        let ads_text = twitch_ads(
            &json!({"next_ad_at": future, "last_ad_at": past, "duration": 60, "snooze_count": 1}),
            Some(true),
        );
        assert!(ads_text.contains(&trf(
            "stream.ads_last",
            &[("seconds", "60"), ("time", &crate::fmt::shown_clock(past))]
        )));

        // Ineligible Twitch & Kick opted_out disable ad/snooze even while live.
        let mut ineligible = seen();
        ineligible.info[TWITCH].ads_unavailable = true;
        ineligible.info[TWITCH].ads = tr("stream.ads_twitch_ineligible").into();
        ineligible.info[KICK].ads_unavailable = true;
        ineligible.info[KICK].ads = tr("stream.ads_kick_out").into();
        ineligible.info[KICK].mature = Some(true);
        ineligible.info[KICK].language = "pt".into();
        reset(ineligible.clone());
        let page = model(&ineligible);
        assert_eq!(find_button(&page, "twitch.ad")["enabled"], false);
        assert_eq!(find_button(&page, "twitch.snooze")["enabled"], false);
        assert_eq!(find_button(&page, "kick.ad")["enabled"], false);
        assert_eq!(find_button(&page, "run_ad")["enabled"], false);
        crate::engine::tests::run(async {
            let engine = crate::engine::tests::engine(json!({"actions": []}));
            assert_eq!(
                act(&engine, "twitch.snooze").unwrap_err(),
                tr("stream.ads_twitch_ineligible")
            );
        });
        let kick_lines = card(&page, "kick")["lines"].to_string();
        assert!(kick_lines.contains(tr("stream.summary_mature")));
        assert!(kick_lines.contains("Português"));

        // TikTok paused state & likes/coins details.
        let tt_card = card(&page, "tiktok");
        assert_eq!(tt_card["state"]["health"], "attention");
        assert_eq!(tt_card["state"]["text"], tr("stream.state_paused"));
        let tt_lines = tt_card["lines"].to_string();
        assert!(tt_lines.contains(tr("stream.tiktok_paused")));
        assert!(tt_lines.contains(&trf("stream.tiktok_metrics", &[("likes", "500"), ("coins", "25")])));
        tiktok_paused(false);
        assert!(!seen().info[TIKTOK].paused);
        assert_eq!(
            card(&model(&seen()), "tiktok")["state"]["text"],
            tr("stream.state_live")
        );

        // OBS summary variants.
        let mut s = ready([true, false, false, false]);
        s.info[TWITCH].live = Some(false);
        s.obs = vec![ObsState {
            name: "Studio".into(),
            streaming: false,
            reconnecting: false,
            congestion: 0.0,
            skipped_frames: 0,
            total_frames: 0,
        }];
        assert_eq!(
            obs_summary(&s),
            Some(("standby", trf("stream.obs_standby", &[("name", "Studio")])))
        );
        s.info[TWITCH].live = Some(true);
        assert_eq!(
            obs_summary(&s),
            Some(("attention", trf("stream.obs_stopped_while_live", &[("name", "Studio")])))
        );
        s.obs[0].streaming = true;
        s.obs[0].skipped_frames = 3;
        s.obs[0].total_frames = 1000;
        assert_eq!(
            obs_summary(&s),
            Some((
                "up",
                trf("stream.obs_streaming_dropped", &[("name", "Studio"), ("dropped", "3")])
            ))
        );
        s.obs[0].congestion = 0.5;
        assert_eq!(
            obs_summary(&s),
            Some((
                "attention",
                trf(
                    "stream.obs_streaming_warn",
                    &[("name", "Studio"), ("dropped", "3"), ("total", "1000")]
                )
            ))
        );
        s.obs[0].reconnecting = true;
        assert_eq!(
            obs_summary(&s),
            Some(("attention", trf("stream.obs_reconnecting", &[("name", "Studio")])))
        );

        // Connected OBS queried during ask() via fake ObsPort.
        crate::engine::tests::run(async {
            let config: crate::Config = serde_json::from_value(json!({
                "obs": {"connections": [{"id": "main", "name": "Main OBS"}]}
            }))
            .unwrap();
            let engine = Engine::new(std::sync::Arc::new(tokio::sync::watch::channel(config).0), |_| {});
            let _fakes = crate::scenario::fakes::install(
                &engine,
                &json!({
                    "obs": {
                        "GetStreamStatus": {
                            "outputActive": true,
                            "outputReconnecting": false,
                            "outputCongestion": 0.0,
                            "outputSkippedFrames": 0,
                            "outputTotalFrames": 600
                        }
                    }
                }),
                None,
            );
            reset(Seen::default());
            ask(&engine, Vec::new(), true);
            for _ in 0..20 {
                tokio::task::yield_now().await;
            }
            assert_eq!(seen().obs.len(), 1);
            assert_eq!(seen().obs[0].name, "Main OBS");
            assert!(seen().obs[0].streaming);
            obs_changed();
            assert!(ASKED.get().is_none());
            crate::scenario::fakes::uninstall();
        });
    }

    #[test]
    fn live_changed_refreshes_immediately_and_clears_offline_or_nothing_state() {
        let _lock = crate::TEST_LOCK.lock().unwrap_or_else(|error| error.into_inner());
        let mut s = ready([true, true, true, true]);
        s.info[TIKTOK].live = Some(true);
        s.info[TIKTOK].likes = Some(120);
        s.info[TIKTOK].coins = Some(50);
        s.info[YOUTUBE].live = Some(false);
        s.info[YOUTUBE].nothing = true;
        reset(s);
        assert!(ASKED.get().is_some());

        live_changed("tiktok", false);
        assert!(ASKED.get().is_none(), "live_changed must schedule a fresh read");
        assert_eq!(seen().info[TIKTOK].live, Some(false));
        assert_eq!(seen().info[TIKTOK].likes, None);
        assert_eq!(seen().info[TIKTOK].coins, None);

        ASKED.set(Some(Instant::now()));
        live_changed("youtube", true);
        assert!(ASKED.get().is_none());
        assert_eq!(seen().info[YOUTUBE].live, Some(true));
        assert!(!seen().info[YOUTUBE].nothing);
    }

    #[test]
    fn refresh_if_due_preserves_pending_open_while_busy_and_refresh_clears_stale_note() {
        let _lock = crate::TEST_LOCK.lock().unwrap_or_else(|error| error.into_inner());
        crate::engine::tests::run(async {
            let engine = crate::engine::tests::engine(json!({"actions": []}));
            let mut s = ready([false; 4]);
            s.busy = true;
            s.note = "Old error".into();
            s.trouble = true;
            reset(s);
            opened();
            refresh_if_due(&engine);
            assert!(OPENED.get(), "OPENED must not be lost while busy");

            SEEN.with_borrow_mut(|st| st.busy = false);
            refresh_if_due(&engine);
            assert!(!OPENED.get());
            for _ in 0..8 {
                tokio::task::yield_now().await;
            }
            // Background refresh keeps the mutation note; explicit Refresh clears it.
            assert_eq!(seen().note, "Old error");
            assert!(seen().trouble);
            act(&engine, "refresh").unwrap();
            for _ in 0..8 {
                tokio::task::yield_now().await;
            }
            assert!(seen().note.is_empty());
            assert!(!seen().trouble);
        });
    }

    #[test]
    fn kick_tags_youtube_tag_brackets_kick_ad_cap_and_catalog_hygiene() {
        let _lock = crate::TEST_LOCK.lock().unwrap_or_else(|error| error.into_inner());
        crate::catalog::set(KNOWN_CATEGORIES, Vec::new());
        crate::catalog::set(KNOWN_TAGS, Vec::new());
        let seen = ready([true, true, true, false]);
        // Kick rejects more than 10 tags instead of silently dropping extras.
        let eleven: Vec<String> = (0..11).map(|i| format!("t{i}")).collect();
        assert!(info_steps(&item(json!({"kick": true, "tags": eleven})), &seen).is_err());
        // YouTube rejects angle brackets in tags too.
        assert_eq!(
            info_steps(&item(json!({"youtube": true, "tags": ["rust", "<script>"]})), &seen).unwrap_err(),
            tr("stream.youtube_brackets")
        );
        // Unused category on YouTube-only save does not pollute KNOWN_CATEGORIES.
        assert!(info_steps(
            &item(json!({"youtube": true, "title": "Hi", "category": "UnusedCat"})),
            &seen
        )
        .is_ok());
        assert!(crate::catalog::get(KNOWN_CATEGORIES).is_empty());
        // Multi-platform ad > 90s clamps Kick's job duration to 90s while keeping Twitch at 180s.
        let ad = ad_steps(
            &item(json!({"duration": 5, "twitch": true, "kick": true, "youtube": true})),
            &seen,
        )
        .unwrap();
        assert_eq!(
            ad,
            vec![(TWITCH, Job::Ad(180)), (KICK, Job::Ad(90)), (YOUTUBE, Job::Ad(180))]
        );
    }

    #[test]
    fn obs_outputs_section_uses_colon_separator_user_choice_fallback_and_reports_errors() {
        let _lock = crate::TEST_LOCK.lock().unwrap_or_else(|error| error.into_inner());
        let mut s = ready([true, true, false, false]);
        let inv = |conn_id: &str, id: &str, plat: &str, src: &str| crate::core::outputs::Inventory {
            connection_id: conn_id.into(),
            outputs: vec![crate::core::outputs::Output {
                id: id.into(),
                source: "main".into(),
                name: "Main".into(),
                platform: Some(plat.into()),
                platform_source: Some(src.into()),
                active: Some(false),
                reconnecting: Some(false),
                vertical: None,
                controllable: true,
                limitation: None,
                required: true,
                key: id.into(),
            }],
            canvases: Vec::new(),
            capabilities: Vec::new(),
        };
        s.obs_inventories = vec![
            ("Studio A".into(), inv("a", "obs:a", "kick", "user")),
            ("Studio B".into(), inv("b", "obs:b", "twitch", "auto")),
        ];
        let section = obs_outputs_section(&s).unwrap();
        let rows = section["rows"].as_array().unwrap();
        let first_label = rows[1]["label"].as_str().unwrap();
        assert!(first_label.starts_with("Studio A: "), "{first_label}");
        assert!(
            !first_label.contains('—') && !first_label.contains('–'),
            "{first_label}"
        );
        assert_eq!(rows[1]["control"]["value"], 2, "user-pinned Kick choice is 2");
        assert_eq!(rows[2]["control"]["value"], 0, "auto-detected Twitch choice is 0");

        // obs.start_all respects busy and surfaces errors instead of swallowing them.
        crate::engine::tests::run(async {
            let engine = crate::engine::tests::engine(json!({"actions": []}));
            let mut busy_state = s.clone();
            busy_state.busy = true;
            reset(busy_state);
            assert_eq!(act(&engine, "obs.start_all").unwrap_err(), tr("stream.working"));

            reset(s);
            assert!(act(&engine, "obs.start_all").is_ok());
            for _ in 0..20 {
                tokio::task::yield_now().await;
            }
            assert!(seen().trouble, "disconnected OBS error must surface in status note");
            assert!(!seen().note.is_empty());
            assert!(!seen().busy);
        });
    }
}
