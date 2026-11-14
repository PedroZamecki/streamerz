//! The Stream page: the live's title, category and tags on every platform at once (or on one), ad
//! breaks, YouTube's description and thumbnail, and on Twitch a marker, a clip and snoozing the next ad.
//!
//! What each platform can change:
//!
//! | | Title | Category | Tags | Ads | Other |
//! |---|---|---|---|---|---|
//! | Twitch | yes | yes | yes | yes (affiliates and partners) | marker, clip, snooze the next ad |
//! | Kick | yes | yes | yes | yes (opted in to Kick ads) | |
//! | YouTube | yes | no | yes (search keywords) | yes (cue point, while live) | description, thumbnail |
//! | TikTok | yes (browser extension, while live) | no | no | no | |
//!
//! Twitch and Kick let only the channel owner change these: a moderator sees them, can't change them.
//! The page lists every platform that is set up, with what it says now and why it can't be changed when
//! it can't. Like the Polls page, what the platforms said last lives here and a read answers from it; it
//! asks again when it's been a while, after a change, or on Refresh. Only platforms already running are
//! asked: this page opens no connection by itself.

use crate::{
    automation::schema::{F, W},
    engine::Engine,
    i18n::{tr, trf},
    model::Obj,
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
const YOUTUBE_FIELDS: &[F] = &[
    ("title", "Title", W::Text),
    ("tags", "Tags", W::Lines),
    ("description", "Description", W::Multi),
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
/// YouTube counts its tags together: commas between them, quotes around one with a space.
const YOUTUBE_TAGS_MAX: usize = 500;
const YOUTUBE_DESCRIPTION_MAX: usize = 5000;
const THUMBNAIL_MAX: u64 = 2 * 1024 * 1024;

/// Categories and tags seen or used, offered to pick from (kept by `catalog`, in memory).
const KNOWN_CATEGORIES: &str = "stream.categories";
const KNOWN_TAGS: &str = "stream.tags";

/// Adds names to a list of picks, once each (any case).
fn remember(key: &'static str, names: impl IntoIterator<Item = String>) {
    let mut list = crate::catalog::get(key);
    for name in names {
        let name = name.trim().to_owned();
        if !name.is_empty() && list.len() < 300 && !list.iter().any(|(_, known)| known.eq_ignore_ascii_case(&name)) {
            list.push((name.clone(), name));
        }
    }
    crate::catalog::set(key, list);
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
    /// Ads in words: the next one, the breaks left ("" when unknown).
    ads: String,
    /// It answered (TikTok never does; YouTube only says a title while live).
    known: bool,
    /// YouTube: not live, but changes go to its next broadcast (scheduled, or the ready one).
    upcoming: bool,
    /// YouTube: neither live nor anything to come, so there is nothing to change.
    nothing: bool,
    /// Why it couldn't be read.
    error: String,
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
    note: String,
    /// Something asked last didn't go through (the note says what).
    trouble: bool,
    requested: u64,
    received: u64,
    busy: bool,
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
}

thread_local! {
    static SEEN: RefCell<Seen> = RefCell::default();
    static ASKED: Cell<Option<Instant>> = const { Cell::new(None) };
    /// What the page depended on when it last asked (see `situation`).
    static SITUATION: Cell<Option<u64>> = const { Cell::new(None) };
}

fn seen() -> Seen {
    SEEN.with(|seen| seen.borrow().clone())
}

fn token_ready(access: &str, refresh: &str, expires_at: u64) -> bool {
    !access.is_empty() && (!crate::oauth::stale(expires_at) || !refresh.is_empty())
}

/// A channel or account name, compared without case, spaces or a leading @.
fn same(a: &str, b: &str) -> bool {
    let norm = |s: &str| s.trim().trim_start_matches('@').replace(' ', "").to_lowercase();
    !a.trim().is_empty() && norm(a) == norm(b)
}

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
        token_ready(&a.token.access_token, &a.token.refresh_token, a.token.expires_at)
            && (!crate::oauth::stale(a.token.expires_at)
                || (!c.kick.client_id.is_empty() && !c.kick.client_secret.is_empty()))
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
        token_ready(&a.token.access_token, &a.token.refresh_token, a.token.expires_at)
            && (!crate::oauth::stale(a.token.expires_at)
                || (!c.youtube.client_id.is_empty() && !c.youtube.client_secret.is_empty()))
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
    Title(String),
    Category(String),
    Tags(Vec<String>),
    /// YouTube's title, description and tags, in one update (each costs quota).
    Video {
        title: Option<String>,
        description: Option<String>,
        tags: Option<Vec<String>>,
    },
    Ad(u64),
    Thumbnail(String),
    Marker,
    Clip,
    Snooze,
}

fn video(title: Option<String>, description: Option<String>, tags: Option<Vec<String>>) -> Job {
    Job::Video {
        title,
        description,
        tags,
    }
}

impl Job {
    fn what(&self, platform: usize) -> String {
        let key = match self {
            Job::Title(_) => "stream.job_title",
            Job::Category(_) => "stream.job_category",
            Job::Tags(tags) if tags.is_empty() => "stream.job_clear_tags",
            Job::Tags(_) => "stream.job_tags",
            Job::Video {
                tags: Some(tags),
                title: None,
                description: None,
            } if tags.is_empty() => "stream.job_clear_tags",
            Job::Video { .. } => "stream.job_video",
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

/// Runs one change through the platform's own client (only ever a running one); what it made, in
/// words, when there is something to say (a clip's link).
async fn run(e: &Rc<Engine>, platform: usize, job: &Job) -> Result<String, String> {
    let who = || vec![("broadcaster_id", "{broadcaster_id}")];
    let off = || seen().access.why[platform].clone();
    match (platform, job) {
        (TWITCH, Job::Title(_) | Job::Tags(_) | Job::Ad(_) | Job::Marker | Job::Clip | Job::Snooze) => {
            let twitch = e.twitch().ok_or_else(off)?;
            let (method, path, query, body) = match job {
                Job::Title(title) => ("PATCH", "channels", who(), json!({"title": title})),
                Job::Tags(tags) => ("PATCH", "channels", who(), json!({"tags": tags})),
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
                Job::Snooze => twitch_ads(&answer["data"][0]),
                _ => String::new(),
            })
        }
        // A category is found by name first: the sub-actions do it and say how it went.
        (TWITCH | KICK, Job::Category(name)) => {
            let (code, settings, flag) = if platform == TWITCH {
                (16, json!({"source": 0, "game": name}), "gameSuccess")
            } else {
                (
                    35031,
                    json!({"categoryName": name, "categoryId": 0}),
                    "category.success",
                )
            };
            if platform == KICK && e.kick().is_none() || platform == TWITCH && e.twitch().is_none() {
                return Err(off());
            }
            let settings = settings.as_object().cloned().unwrap_or_default();
            let done = e.run_one(code, settings).await;
            match done.get(flag).and_then(Value::as_bool) {
                Some(true) => Ok(String::new()),
                _ => Err(trf("stream.no_category", &[("name", name)])),
            }
        }
        (KICK, _) => {
            let kick = e.kick().ok_or_else(off)?;
            match job {
                Job::Title(title) => kick.set_channel_title(title).await,
                Job::Tags(tags) => kick.set_channel_tags(tags.clone()).await,
                Job::Ad(length) => kick.start_ad_break(*length).await.map(drop),
                _ => Err(tr("stream.not_here").into()),
            }
            .map(|()| String::new())
        }
        (YOUTUBE, _) => {
            let youtube = e.youtube().ok_or_else(off)?;
            match job {
                Job::Video {
                    title,
                    description,
                    tags,
                } => {
                    youtube
                        .update_video("", title.clone(), description.clone(), tags.clone())
                        .await
                }
                Job::Ad(length) => youtube.run_commercial("", *length).await,
                Job::Thumbnail(file) => youtube.set_thumbnail("", file).await,
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

/// Twitch's ad schedule (Get Ad Schedule, or a snooze's answer) in words.
fn twitch_ads(data: &Value) -> String {
    let now = crate::oauth::now() as i64;
    let next = match unix_time(&data["next_ad_at"]) {
        Some(at) if at > now => trf("stream.ads_next", &[("time", &crate::fmt::shown_clock(at))]),
        _ => tr("stream.ads_ready").into(),
    };
    match data["snooze_count"].as_u64() {
        Some(left) => format!("{next} {}", trf("stream.snoozes_left", &[("count", &left.to_string())])),
        None => next,
    }
}

/// Kick's ad break status in words.
fn kick_ads(data: &Value) -> String {
    if data["opted_in"].as_bool() == Some(false) {
        tr("stream.ads_kick_out").into()
    } else if data["ads_blocked"].as_bool() == Some(true) {
        tr("stream.ads_blocked").into()
    } else if let Some(left) = data["remaining_ad_breaks"].as_u64() {
        trf("stream.ads_left", &[("count", &left.to_string())])
    } else {
        tr("stream.ads_ready").into()
    }
}

/// What a platform says the live is now. Twitch and Kick are read for the channel read (a moderator
/// sees the owner's); their ads only by the owner, who alone may see them.
async fn look(e: &Rc<Engine>, platform: usize, ready: bool) -> Info {
    let mut info = Info::default();
    let result: Result<(), String> = async {
        match platform {
            TWITCH => {
                let Some(twitch) = e.twitch() else { return Ok(()) };
                if e.config().twitch.broadcaster.is_none() {
                    return Ok(());
                }
                let who = vec![("broadcaster_id", "{broadcaster_id}")];
                let channel = twitch
                    .helix("GET", "channels", who.clone(), None, false, true)
                    .await
                    .map_err(twitch_error)?;
                let data = &channel["data"][0];
                info.title = data["title"].as_str().unwrap_or_default().into();
                info.category = data["game_name"].as_str().unwrap_or_default().into();
                info.tags = texts(data.get("tags"));
                info.known = data.is_object();
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
                // The schedule needs channel:read:ads; an older login without it reads as unknown.
                if ready {
                    if let Ok(ads) = twitch.helix("GET", "channels/ads", who, None, false, true).await {
                        info.ads = twitch_ads(&ads["data"][0]);
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
                let channel = kick.api("GET", &path, None).await?;
                let data = &channel["data"][0];
                info.title = data["stream_title"].as_str().unwrap_or_default().into();
                info.category = data["category"]["name"].as_str().unwrap_or_default().into();
                info.tags = texts(data.get("custom_tags"));
                info.known = data.is_object();
                remember(KNOWN_CATEGORIES, [info.category.clone()]);
                remember(KNOWN_TAGS, info.tags.clone());
                if ready {
                    if let Ok(status) = kick.get_ad_status().await {
                        info.ads = kick_ads(&status["data"]);
                    }
                }
            }
            YOUTUBE => {
                let Some(youtube) = e.youtube() else { return Ok(()) };
                // The watch page, as the chat reader does: no API quota spent.
                info.title = youtube.live_title().await?;
                info.known = !info.title.is_empty();
                // Not live: a change goes to the next broadcast, if there is one (a logged-in owner only).
                if !info.known && ready {
                    match youtube.next_broadcast().await? {
                        Some((_, title)) => {
                            info.title = title;
                            info.upcoming = true;
                        }
                        None => info.nothing = true,
                    }
                }
                info.ads = if info.known {
                    tr("stream.ads_ready").into()
                } else {
                    tr("stream.ads_youtube_offline").into()
                };
            }
            // TikTok shares no title with apps; the page says what was set from here.
            _ => {}
        }
        Ok(())
    }
    .await;
    if let Err(error) = result {
        info.error = error;
    }
    info
}

/// Runs `steps`, then reads the platforms again; the model shows it all when it comes. False when a
/// change is already running.
fn ask(engine: &Rc<Engine>, steps: Vec<Step>) -> bool {
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
        let mut notes = Vec::new();
        let mut trouble = false;
        // What was set here, kept where the platform doesn't say (TikTok's title).
        let mut set_titles: [Option<String>; 4] = Default::default();
        for (platform, job) in steps {
            let what = job.what(platform);
            if !access.ready[platform] {
                notes.push(trf(
                    "stream.failed",
                    &[("what", &what), ("error", &access.why[platform])],
                ));
                trouble = true;
                continue;
            }
            match run(&e, platform, &job).await {
                Ok(made) => {
                    if let Job::Title(title) = &job {
                        set_titles[platform] = Some(title.clone());
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
        let mut info: [Info; 4] = Default::default();
        for platform in (0..4).filter(|i| access.shown[*i]) {
            info[platform] = look(&e, platform, access.ready[platform]).await;
        }
        let applied = SEEN.with(|seen| {
            let mut seen = seen.borrow_mut();
            if !seen.finish(sequence) {
                return false;
            }
            for (platform, found) in info.into_iter().enumerate() {
                let before = std::mem::take(&mut seen.info[platform].title);
                seen.info[platform] = found;
                // TikTok's title is what was set from here last.
                if platform == TIKTOK {
                    seen.info[platform].title = set_titles[platform].take().unwrap_or(before);
                }
            }
            seen.access = access;
            nothing_to_change(&mut seen);
            if !notes.is_empty() {
                seen.note = notes.join("  ");
                seen.trouble = trouble;
            } else if mutation {
                seen.note.clear();
            }
            true
        });
        if applied {
            crate::host::changed();
        }
    });
    true
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
/// a login, a live starting or ending), or when it's been a while: titles and tags change rarely.
fn refresh_if_due(engine: &Rc<Engine>) {
    let now = situation(engine);
    let changed = SITUATION.get() != Some(now);
    if changed || ASKED.get().is_none_or(|at| at.elapsed() > Duration::from_secs(30)) {
        SITUATION.set(Some(now));
        ASKED.set(Some(Instant::now()));
        ask(engine, Vec::new());
    }
}

/// The page was opened: what it shows is read again at once.
pub fn opened() {
    ASKED.set(None);
}

pub fn read(engine: &Rc<Engine>) -> Result<Value, String> {
    refresh_if_due(engine);
    Ok(json!({"domain": DOMAIN, "sections": model(&seen())}))
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
/// Why nothing that takes this can run: busy, else the first platform's reason, else "".
fn none_why(seen: &Seen, takes: [bool; 4]) -> String {
    if seen.busy {
        return tr("stream.working").into();
    }
    if seen.can(takes).iter().any(|on| *on) {
        return String::new();
    }
    (0..4)
        .find(|i| takes[*i] && seen.access.shown[*i])
        .map(|i| seen.access.why[i].clone())
        .unwrap_or_else(|| tr("stream.accounts_needed").into())
}

/// The Accounts page, where a platform is connected and logged in.
fn accounts_button(id: &str) -> Value {
    let mut button = button(id, tr("nav.page.accounts"), "user-round");
    button["go"] = "Accounts".into();
    button
}

/// One platform's card: its state in a word (the reason on hover), what it shows now (the title, then
/// the category and tags as chips, then its ads), and its buttons.
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
    let (health, short, tip) = if !ready {
        ("off", tr(access.state[i]), access.why[i].clone())
    } else if !info.error.is_empty() {
        ("down", tr("stream.state_read_failed"), info.error.clone())
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
            _ if i == TIKTOK => (tr("stream.tiktok_title_unknown"), true),
            _ if i == YOUTUBE => (tr("stream.not_live"), true),
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
    if info.upcoming {
        lines.push(json!({"icon": "calendar", "text": tr("stream.youtube_upcoming"), "muted": true}));
    }
    if ADS[i] && ready && !info.ads.is_empty() {
        // YouTube's ads wait for a live: quiet until then.
        let waiting = i == YOUTUBE && !info.known;
        lines.push(json!({"icon": "megaphone", "text": info.ads, "muted": waiting}));
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
            &why,
        ));
    }
    if i == TWITCH {
        list.push(off_unless(button(&key("snooze"), tr("btn.snooze_ad"), "clock"), &why));
        list.push(off_unless(button(&key("marker"), tr("btn.add_marker"), "pin"), &why));
        list.push(off_unless(
            button(&key("clip"), tr("btn.create_clip"), "clapperboard"),
            &why,
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
    card["buttons"] = list.into();
    card
}

fn model(seen: &Seen) -> Vec<Value> {
    let (health, text) = status_line(seen);
    let mut top = vec![
        form_button(
            "edit",
            "stream_info",
            tr("btn.edit_stream"),
            "pencil",
            &none_why(seen, TITLE),
        ),
        form_button(
            "run_ad",
            "stream_ad",
            tr("btn.run_ad"),
            "megaphone",
            &none_why(seen, ADS),
        ),
        button("refresh", tr("btn.refresh"), "refresh-cw"),
    ];
    if !seen.access.shown.iter().any(|shown| *shown) {
        top = vec![accounts_button("accounts")];
    }
    // The introduction beside the buttons for every platform; under them, what is going on, if anything.
    let mut rows = vec![json!({"about": tr("stream.hint"), "control": buttons(top)})];
    if !text.is_empty() {
        rows.push(json!({"status": state(health, &text)}));
    }
    let mut sections = vec![json!({"title": "", "icon": "", "rows": rows})];
    sections.extend((0..4).filter(|i| seen.access.shown[*i]).map(|i| platform_card(seen, i)));
    sections
}

/// Tags to clear on every ready platform that has them.
fn clear_tags_steps(seen: &Seen) -> Vec<Step> {
    let on = seen.can(TAGS);
    (0..4)
        .filter(|i| on[*i])
        .map(|i| match i {
            YOUTUBE => (i, video(None, None, Some(Vec::new()))),
            _ => (i, Job::Tags(Vec::new())),
        })
        .collect()
}

/// Run one button. Changing the stream, running an ad and setting the thumbnail are forms
/// (`entity_form` kinds `stream_info`, `stream_ad`, `stream_thumbnail`, and `stream_info.kick` or
/// `stream_ad.twitch` for one platform).
pub fn act(engine: &Rc<Engine>, action: &str) -> Result<Value, String> {
    let now = seen();
    let go = |steps: Vec<Step>| {
        if ask(engine, steps) {
            Ok(json!({"accepted": true}))
        } else {
            Err(tr("stream.working").to_string())
        }
    };
    let action = action.strip_prefix("twitch.").unwrap_or(action);
    match action {
        "refresh" => {
            ASKED.set(Some(Instant::now()));
            ask(engine, Vec::new());
            Ok(json!({"accepted": true}))
        }
        "clear_tags" => {
            let steps = clear_tags_steps(&now);
            if steps.is_empty() {
                return Err(none_why(&now, TAGS));
            }
            go(steps)
        }
        "marker" | "clip" | "snooze" => {
            if !now.can(ONLY_TWITCH)[TWITCH] {
                return Err(none_why(&now, ONLY_TWITCH));
            }
            go(vec![(
                TWITCH,
                match action {
                    "marker" => Job::Marker,
                    "clip" => Job::Clip,
                    _ => Job::Snooze,
                },
            )])
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
        Some(slug) => Some(PLATFORMS.iter().position(|(key, _)| *key == slug)?),
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
        if now.access.ready[i] {
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
    // A box none of the platforms can take is hidden.
    let unused = |on: bool| if on { String::new() } else { "-".to_string() };
    let labelled: Vec<(&F, String)> = fields
        .iter()
        .map(|field| {
            let why = match field.0 {
                "category" => unused(any(CATEGORY)),
                "tags" | "clear_tags" => unused(any(TAGS)),
                "description" => unused(any(ONLY_YOUTUBE)),
                key => PLATFORMS
                    .iter()
                    .position(|(slug, _)| *slug == key)
                    .map(off)
                    .unwrap_or_default(),
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
            item["description"] = "".into();
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
            (title, tr("btn.save"), vec![note.to_string()])
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
            let takes = if base == "stream_ad" { ADS } else { TITLE };
            for (i, on) in now.can(takes).into_iter().enumerate() {
                if takes[i] {
                    item[PLATFORMS[i].0] = on.into();
                }
            }
        }
    }
    Some(Ok(form_json(kind, &title, ok, &labelled, item, notes)))
}

/// The platforms a form picked, among the ready ones that take it.
fn picked(item: &Obj, seen: &Seen, takes: [bool; 4]) -> [bool; 4] {
    let can = seen.can(takes);
    std::array::from_fn(|i| can[i] && item.get(PLATFORMS[i].0).and_then(Value::as_bool) == Some(true))
}

/// How long YouTube counts tags: with the commas between them and quotes around one with a space.
fn youtube_tags_length(tags: &[String]) -> usize {
    let quoted = tags.iter().filter(|tag| tag.contains(' ')).count();
    tags.iter().map(|tag| tag.chars().count()).sum::<usize>() + tags.len().saturating_sub(1) + 2 * quoted
}

/// The changes a stream info form asks for, or what's wrong with it. An empty field keeps what is there.
fn info_steps(item: &Obj, seen: &Seen) -> Result<Vec<Step>, String> {
    let text = |key: &str| {
        item.get(key)
            .and_then(Value::as_str)
            .unwrap_or_default()
            .trim()
            .to_string()
    };
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
    let changes = !title.is_empty()
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
    if on[YOUTUBE] && (description.contains(['<', '>']) || title.contains(['<', '>'])) {
        return Err(tr("stream.youtube_brackets").into());
    }
    remember(KNOWN_CATEGORIES, [category.clone()]);
    remember(KNOWN_TAGS, tags.clone());
    let given = |text: &String| (!text.is_empty()).then(|| text.clone());
    let new_tags = (clear || !tags.is_empty()).then(|| tags.clone());
    let mut steps = Vec::new();
    for i in (0..4).filter(|i| on[*i]) {
        if i == YOUTUBE {
            if !title.is_empty() || !description.is_empty() || new_tags.is_some() {
                steps.push((i, video(given(&title), given(&description), new_tags.clone())));
            }
            continue;
        }
        if !title.is_empty() {
            steps.push((i, Job::Title(title.clone())));
        }
        if categorised[i] && !category.is_empty() {
            steps.push((i, Job::Category(category.clone())));
        }
        if let (true, Some(tags)) = (tagged[i], &new_tags) {
            steps.push((i, Job::Tags(tags.clone())));
        }
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
    Ok((0..4).filter(|i| on[*i]).map(|i| (i, Job::Ad(length))).collect())
}

fn thumbnail_steps(item: &Obj, seen: &Seen) -> Result<Vec<Step>, String> {
    if !seen.access.ready[YOUTUBE] {
        return Err(none_why(seen, ONLY_YOUTUBE));
    }
    let file = item.get("file").and_then(Value::as_str).unwrap_or_default().trim();
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
        if ask(engine, steps) {
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
                (TWITCH, Job::Title("Hello".into())),
                (TWITCH, Job::Category("Just Chatting".into())),
                (TWITCH, Job::Tags(vec!["English".into()])),
                (KICK, Job::Title("Hello".into())),
                (KICK, Job::Category("Just Chatting".into())),
                (KICK, Job::Tags(vec!["English".into()])),
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
                (TWITCH, Job::Category("Art".into())),
                (YOUTUBE, video(None, Some("About".into()), None)),
            ]
        );
        // An empty field keeps what is there.
        let only_title = info_steps(&item(json!({"title": "Hi", "kick": true})), &seen).unwrap();
        assert_eq!(only_title, vec![(KICK, Job::Title("Hi".into()))]);
        // Remove every tag: whatever the tags box says.
        let cleared = info_steps(
            &item(json!({"tags": ["Kept"], "clear_tags": true, "kick": true, "youtube": true})),
            &seen,
        )
        .unwrap();
        assert_eq!(
            cleared,
            vec![(KICK, Job::Tags(vec![])), (YOUTUBE, video(None, None, Some(vec![])))]
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
                (TWITCH, Job::Tags(vec![])),
                (KICK, Job::Tags(vec![])),
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
        assert_eq!(kick_ads(&json!({"opted_in": false})), tr("stream.ads_kick_out"));
        assert_eq!(
            kick_ads(&json!({"opted_in": true, "ads_blocked": true})),
            tr("stream.ads_blocked")
        );
        assert_eq!(
            kick_ads(&json!({"opted_in": true, "remaining_ad_breaks": 3})),
            trf("stream.ads_left", &[("count", "3")])
        );
        assert_eq!(twitch_ads(&json!({"next_ad_at": 0})), tr("stream.ads_ready"));
        assert_eq!(twitch_ads(&json!({"next_ad_at": ""})), tr("stream.ads_ready"));
        assert_eq!(
            twitch_ads(&json!({"next_ad_at": 0, "snooze_count": 2})),
            format!(
                "{} {}",
                tr("stream.ads_ready"),
                trf("stream.snoozes_left", &[("count", "2")])
            )
        );
        let later = crate::oauth::now() as i64 + 600;
        for next in [json!(later), json!(later.to_string())] {
            assert_eq!(
                twitch_ads(&json!({"next_ad_at": next})),
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
        // TikTok offline: its card says so, its title is unknown.
        let tiktok = card(&sections, "tiktok");
        assert_eq!(tiktok["state"]["text"], tr("stream.state_offline"));
        assert_eq!(tiktok["headline"], tr("stream.tiktok_title_unknown"));
        // The top: the buttons for every platform, and no status while each card says its own.
        assert_eq!(sections[0]["rows"].as_array().unwrap().len(), 1);
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
        assert_eq!(keys(&youtube), ["title", "tags", "description", "clear_tags"]);
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
                    "POST clips": {"data": [{"id": "Abc"}]}},
                "kick": {"GET /channels?slug=me": {"data": [{"stream_title": "Kick title", "category": {"name": "IRL"}}]}},
                "youtube": {"liveVideo": "", "nextBroadcast": {"id": "v2", "title": "Next live"}}}),
                None,
            );
            reset(Seen::default());
            let settle = || async {
                for _ in 0..40 {
                    tokio::task::yield_now().await;
                }
            };
            ask(&engine, Vec::new());
            settle().await;
            assert_eq!(seen().access.ready, [true, true, true, false], "TikTok isn't connected");
            assert_eq!(seen().info[TWITCH].title, "Old");
            assert_eq!(seen().info[KICK].title, "Kick title", "read by the channel's slug");
            assert_eq!(seen().info[KICK].category, "IRL");
            // Not live on YouTube: what changes go to is the next live, shown with its title.
            assert!(seen().info[YOUTUBE].upcoming);
            assert_eq!(seen().info[YOUTUBE].title, "Next live");
            fakes.calls.borrow_mut().clear();
            let form = item(json!({"title": "New title", "tags": ["Rust"], "description": "About",
                "twitch": true, "kick": true, "youtube": true, "tiktok": false}));
            assert!(save_external(&engine, "stream_info", form).unwrap().is_ok());
            settle().await;
            let calls = asked(&fakes);
            for wanted in [
                r#"twitch PATCH channels {"title":"New title"}"#,
                r#"twitch PATCH channels {"tags":["Rust"]}"#,
                r#"kick title title="New title""#,
                r#"kick tags tags=["Rust"]"#,
                r#"youtube updateVideo title="New title" tags=["Rust"] description="About""#,
            ] {
                assert!(calls.iter().any(|call| call == wanted), "{wanted} not in {calls:#?}");
            }
            assert!(!seen().trouble, "{}", seen().note);
            // One platform's form changes that platform only, whatever its item says.
            fakes.calls.borrow_mut().clear();
            let kick_only = item(json!({"title": "Only Kick", "twitch": true}));
            assert!(save_external(&engine, "stream_info.kick", kick_only).unwrap().is_ok());
            settle().await;
            let calls = asked(&fakes);
            assert!(
                calls.iter().any(|call| call == r#"kick title title="Only Kick""#),
                "{calls:#?}"
            );
            assert!(!calls.iter().any(|call| call.starts_with("twitch PATCH")), "{calls:#?}");
            assert!(act(&engine, "twitch.clip").is_ok());
            settle().await;
            assert!(seen().note.contains("https://clips.twitch.tv/Abc"), "{}", seen().note);
            crate::scenario::fakes::uninstall();
        });
    }
}
