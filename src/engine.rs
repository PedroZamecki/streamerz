//! The engine, as in Streamer.bot: triggers queue actions, actions run their sub-actions in order
//! (or one at random), in action queues (blocking: one at a time; or not), with arguments, globals,
//! signals, history and pending runs. It all runs on the runtime thread (a LocalSet); actions and
//! persisted globals live in the config, so the window and the file see the same thing.

use crate::model::{Action, Obj};
use crate::types::{name, SUB_ACTIONS, TRIGGERS};
use crate::vars::{self, auto_type, text};
use crate::Config;
use serde_json::{json, Value};
use std::cell::RefCell;
use std::collections::{HashMap, HashSet, VecDeque};
use std::rc::Rc;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{oneshot, watch};

pub mod builtins;
pub mod chatlog;
mod chatters;
pub use chatlog::can_send;
pub mod community;
pub mod events;
pub mod globals;
mod midi_in;
pub mod overlays;
pub mod plugins;
mod processes;
mod rest;
mod songs;
mod watchers;

pub const DEFAULT_QUEUE: &str = "00000000-0000-0000-0000-000000000000";
const HISTORY: usize = 100;

// Trigger types (types.rs TRIGGERS) the engine fires itself.
pub const T_TEST: u32 = 702;
pub const T_STARTED: u32 = 706;
const T_CUSTOM: u32 = 18001;

/// How a sub-action list ended (Streamer.bot's own names).
#[derive(Clone, Copy, PartialEq, Debug)]
enum Flow {
    Continue,
    /// Break: stops the whole action, through inline Run Actions too.
    BreakToRoot,
    LoopBreak,
    LoopContinue,
}

/// A queued or running action.
struct Run {
    id: String,
    action: String,
    name: String,
    args: Obj,
    queued_at: String,
}

/// One past run, for the history view (newest last).
#[derive(Clone)]
pub struct Record {
    pub outcome: RunOutcome,
    pub action_id: String,
    pub name: String,
    /// The arguments it started with, and those it ended with.
    pub args: Obj,
    pub after: Obj,
    pub queued_at: String,
    pub ms: f64,
}

/// Observed execution errors, separate from the action's public arguments.
#[derive(Clone, Default, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RunOutcome {
    pub errors: u32,
    pub skipped: u32,
    pub failed_step: Option<u64>,
    pub error: Option<String>,
    #[serde(skip)]
    step: u64,
}

impl RunOutcome {
    pub fn status(&self) -> &'static str {
        if self.errors > 0 {
            "failed"
        } else if self.skipped > 0 {
            "completed_with_skips"
        } else {
            "completed"
        }
    }
}

tokio::task_local! { static RUN_OUTCOME: RefCell<RunOutcome>; }

pub fn record_error(message: &str) {
    let _ = RUN_OUTCOME.try_with(|state| {
        let mut state = state.borrow_mut();
        state.errors = state.errors.saturating_add(1);
        if state.error.is_none() {
            state.error = Some(message.into());
            state.failed_step = Some(state.step);
        }
    });
}

#[derive(Default)]
struct Queue {
    paused: bool,
    waiting: VecDeque<Run>,
    /// Running runs: id and action name.
    running: Vec<(String, String)>,
}

#[derive(Default)]
struct State {
    /// Non-persisted globals, and non-persisted user globals by user ("platform:id").
    temp: Obj,
    temp_writes: Obj,
    user_temp: Obj,
    queues: HashMap<String, Queue>,
    history: VecDeque<Record>,
    waiters: HashMap<String, Vec<oneshot::Sender<Obj>>>,
    signals: HashMap<String, VecDeque<Obj>>,
    remote_waiters: HashMap<(String, String), Vec<oneshot::Sender<Obj>>>,
    remote_signals: HashMap<(String, String), VecDeque<Obj>>,
    /// Sub-action types already reported as unsupported.
    warned: HashSet<u64>,
    save_pending: bool,
    community: community::Community,
    /// Command cooldowns: when each command (id) and each command per user ("id:user id") is
    /// usable again; the non-persisted counters, keyed like Config.counters.
    cooldowns: HashMap<String, std::time::Instant>,
    counters: Obj,
    /// Chat lines seen, for timers' Lines.
    chat_lines: u64,
    /// The running timers, stopped when timers change.
    timers: Vec<tokio::task::AbortHandle>,
    /// The Stream Deck plugins connected (`streamdeck.rs`): their ids and what pushes to them.
    stream_decks: Vec<(u64, tokio::sync::mpsc::UnboundedSender<String>)>,
    /// The running file watchers and tails (`watchers.rs`).
    watch_tasks: Vec<tokio::task::AbortHandle>,
    /// The process watch, while an action uses Process Started or Stopped (`processes.rs`).
    process_task: Option<tokio::task::AbortHandle>,
    /// The open MIDI inputs, while an action uses MIDI Message (`midi_in.rs`).
    midi_tasks: Vec<tokio::task::AbortHandle>,
    /// Remote instances: WebSocket clients connected to a Streamer.bot or Streamerz server (id, name,
    /// what sends to it).
    remotes: Vec<(String, String, tokio::sync::mpsc::UnboundedSender<String>)>,
    /// Dev mode (`STREAMERZ_DEV=1`): chat lines kept here instead of sent, for `GetChatOut`.
    dev_out: Option<Vec<Value>>,
    /// Every platform's chat, for the chat window.
    chat_log: chatlog::Log,
    chat_modes: [Option<ChatModes>; 2],
    kick_chatters: Option<chatters::Chatters>,
    kick_chatters_failed: bool,
}

/// Known chat restrictions. Unknown settings are not presented as disabled.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ChatModes {
    pub emote_only: bool,
    pub slow_seconds: Option<u64>,
}

pub struct Engine {
    local_spotify: RefCell<Option<tokio::task::JoinHandle<()>>>,
    music: &'static dyn crate::ports::MusicPort,
    /// Song requests, and the lock that makes each look at them and change them in one turn.
    songs: RefCell<crate::songrequests::Requests>,
    song_turn: tokio::sync::Mutex<()>,
    song_watching: std::cell::Cell<bool>,
    song_pace: std::cell::Cell<Option<Duration>>,
    /// The votes to skip the song that plays: its URI, and who voted.
    song_votes: RefCell<(String, HashSet<String>)>,
    song_last_skip: std::cell::Cell<Option<std::time::Instant>>,
    /// The song that played when the engine last looked.
    song_playing: RefCell<Option<String>>,
    /// What Spotify's player last showed (volume, shuffle, repeat, device), to say what changed.
    spotify_seen: RefCell<std::collections::HashMap<u32, String>>,
    /// The last value of each overlay topic.
    overlay_state: RefCell<std::collections::HashMap<String, Value>>,
    /// Captured at startup, independent of pending settings changes.
    pub(crate) memory_saver: bool,
    cfg: Arc<watch::Sender<Config>>,
    /// Writes the config (a no-op in tests).
    save: fn(&Config),
    st: RefCell<State>,
    /// WebSocket connections listening to events.
    hub: RefCell<Vec<events::Listener>>,
    twitch: RefCell<Option<crate::twitch::TwitchHandle>>,
    obs: RefCell<Option<Rc<dyn crate::ports::ObsPort>>>,
    kick: RefCell<Option<crate::kick::KickHandle>>,
    youtube: RefCell<Option<crate::youtube::YouTubeHandle>>,
    tiktok: RefCell<Option<crate::tiktok::TikTokHandle>>,
    /// The Streamerz for TikTok extension, when connected (sending through the browser).
    pub(crate) tiktok_bridge: crate::tiktok_bridge::Bridge,
    pub(crate) slobs: crate::slobs::Slobs,
    pub(crate) donations: RefCell<crate::donations::Live>,
    pub(crate) hotkeys: RefCell<crate::hotkeys::Live>,
    pub(crate) live_wake: tokio::sync::Notify,
    /// A message was just sent to YouTube: its chat is read now, not after the wait between reads.
    pub(crate) chat_poke: tokio::sync::Notify,
    streaming_since: std::cell::Cell<Option<std::time::Instant>>,
    /// The plugins that are on.
    plugins: RefCell<Vec<plugins::Loaded>>,
}

impl Engine {
    pub fn set_chat_modes(&self, platform: &str, modes: Option<ChatModes>) {
        let index = match platform {
            "twitch" => 0,
            "kick" => 1,
            _ => return,
        };
        let mut state = self.st.borrow_mut();
        if state.chat_modes[index] != modes {
            state.chat_modes[index] = modes;
            crate::ui::changed();
        }
    }

    pub fn chat_modes(&self) -> [Option<ChatModes>; 2] {
        self.st.borrow().chat_modes
    }
}

/// A running action's context.
struct Ctx {
    args: Obj,
    group: String,
}

/// The action's arguments plus its group's settings as `group.name`. A text setting with variables in
/// it (a reply such as `@%user% added %spotifyTitle%`) is filled in once, with this run's arguments;
/// what the variables hold is never read as variables again.
fn group_args<'a>(ctx: &'a Ctx, c: &crate::Config) -> std::borrow::Cow<'a, Obj> {
    let values = crate::groups::values(c, &ctx.group);
    if values.is_empty() {
        return std::borrow::Cow::Borrowed(&ctx.args);
    }
    let mut args = ctx.args.clone();
    for (name, value) in values {
        let value = match value {
            Value::String(t) if t.contains('%') => Value::String(vars::expand(&t, &ctx.args, &c.globals)),
            v => v,
        };
        args.insert(format!("group.{name}"), value);
    }
    std::borrow::Cow::Owned(args)
}

use crate::ports::Fut;

fn s<'a>(o: &'a Obj, k: &str) -> &'a str {
    o.get(k).and_then(Value::as_str).unwrap_or("")
}

fn b(o: &Obj, k: &str, default: bool) -> bool {
    o.get(k).and_then(Value::as_bool).unwrap_or(default)
}

fn n(o: &Obj, k: &str) -> f64 {
    match o.get(k) {
        Some(Value::String(t)) => t.trim().parse().unwrap_or(0.0),
        Some(v) => v.as_f64().unwrap_or(0.0),
        None => 0.0,
    }
}

fn subs(o: &Obj) -> &[Value] {
    o.get("subActions").and_then(Value::as_array).map_or(&[], Vec::as_slice)
}

/// What the window asks of the engine: run this on the engine's thread.
pub type UiCmd = Box<dyn FnOnce(&Rc<Engine>) + Send>;

thread_local! {
    /// The engine, on its own thread (for `save_on_crash`).
    static CURRENT: RefCell<std::rc::Weak<Engine>> = const { RefCell::new(std::rc::Weak::new()) };
}
/// How other threads reach the engine (for `save_on_crash`).
static CMDS: std::sync::OnceLock<tokio::sync::mpsc::UnboundedSender<UiCmd>> = std::sync::OnceLock::new();

impl Engine {
    pub fn spotify_local_start(self: &Rc<Self>) {
        if self
            .local_spotify
            .borrow()
            .as_ref()
            .is_some_and(|task| !task.is_finished())
        {
            return;
        }
        let engine = self.clone();
        let task = tokio::task::spawn_local(async move {
            if let Err(error) = crate::spotify::local::monitor(engine.clone()).await {
                crate::warn(format!("Spotify local: {error}"));
                engine.change(|c| c.spotify.local_monitor = false);
                crate::spotify::event(&engine, 99806, &error);
            }
        });
        *self.local_spotify.borrow_mut() = Some(task);
    }

    pub fn spotify_local_stop(&self) {
        if let Some(task) = self.local_spotify.borrow_mut().take() {
            task.abort();
        }
    }
    /// Notify overlays after an installed countdown changes.
    pub fn countdown_changed(&self, id: &str) {
        let snapshot = crate::templates::clock::get(&self.config(), id, crate::templates::clock::now_ms());
        self.emit(
            "Misc",
            "StreamerzCountdownChanged",
            || json!({"clockId": id, "snapshot": snapshot}),
        );
    }

    /// Runs one sub-action of `kind` with the settings `o`, as an action would, and gives the variables
    /// it set (the Polls page drives Twitch's and YouTube's poll and prediction sub-actions so).
    pub async fn run_one(self: &Rc<Self>, kind: u64, o: Obj) -> Obj {
        let mut ctx = Ctx {
            args: Obj::new(),
            group: String::new(),
        };
        self.rest(&mut ctx, &o, kind).await;
        ctx.args
    }
}

/// Lets `save_on_crash` reach `e`. Call on the engine's thread.
pub fn set_current(e: &Rc<Engine>, cmds: tokio::sync::mpsc::UnboundedSender<UiCmd>) {
    CURRENT.with(|c| *c.borrow_mut() = Rc::downgrade(e));
    let _ = CMDS.set(cmds);
}

/// Before the process ends (a panic, the session ending): writes the settings and what isn't saved as
/// it changes (Community's users and credits). On the engine's own thread directly, if its state isn't
/// mid-change; from another thread, the engine is asked and given 2 seconds.
pub fn save_and_wait() {
    save_and_wait_inner(false);
}

/// Save and snapshot countdowns before orderly process shutdown.
pub fn shutdown_and_wait() {
    save_and_wait_inner(true);
}

fn save_and_wait_inner(shutdown: bool) {
    let here = CURRENT.try_with(|c| c.borrow().upgrade()).ok().flatten();
    if let Some(e) = here {
        if e.st.try_borrow_mut().is_ok() {
            if shutdown {
                e.save_shutdown();
            } else {
                e.save_all();
            }
        }
        return;
    }
    let Some(cmds) = CMDS.get() else { return };
    let (done, wait) = std::sync::mpsc::channel();
    let ask: UiCmd = Box::new(move |e| {
        if shutdown {
            e.save_shutdown();
        } else {
            e.save_all();
        }
        let _ = done.send(());
    });
    if cmds.send(ask).is_ok() {
        let _ = wait.recv_timeout(std::time::Duration::from_secs(2));
    }
}

/// Whether the engine runs this sub-action type (the rest are kept, logged once and skipped).
pub fn supported(kind: u32) -> bool {
    matches!(
        kind,
        2 | 4 | 6 | 8 | 9 | 10..=29 | 30..=49 | 50..=52 | 120..=130 | 200..=202 | 204..=208 | 213..=215 | 251..=254 | 321..=329 | 401..=415 | 417 | 500..=513 | 550..=573 | 575..=580
            | 3 | 5 | 1001..=1007 | 1009..=1025 | 1028 | 1030..=1034 | 1038..=1040 | 4001..=4012 | 7001 | 8001 | 8002 | 9001..=9003 | 10001 | 11001 | 11002 | 11004..=11008 | 15001 | 20001..=20003
            | 35001 | 35002 | 35010..=35012 | 35020..=35022 | 35030 | 35031
            | 35040..=35043 | 35101..=35103 | 35105..=35108 | 35113 | 43001..=43003 | 99900..=99904
            | 1000 | 1026 | 1027 | 1035..=1037 | 99800 | 99802 | 99803 | 99880 | 99810..=99824 | 99827..=99833 | 99840..=99875
    )
}

pub(crate) use rest::{poll_args, prediction_args};

/// A UTC civil date and time from Unix seconds.
pub fn civil_time(secs: i64) -> crate::fmt::Time {
    rest::civil_pub(secs)
}

/// Unix seconds from an ISO 8601 UTC time ("2024-08-17T12:34:56Z").
pub fn rest_parse_iso(t: &str) -> Option<i64> {
    rest::parse_iso_pub(t)
}

/// A random version 4 UUID.
pub fn uuid() -> String {
    let r = |bits: u32| crate::math::secure_u64() >> (64 - bits);
    let (a, b2, c, d, e) = (r(32), r(16), r(12), r(14), r(48));
    format!("{a:08x}-{b2:04x}-4{c:03x}-{:04x}-{e:012x}", 0x8000 | d)
}

/// Enabled / Disabled / Toggle (0, 1, 2) applied to a current state.
fn new_state(state: f64, on: bool) -> bool {
    match state as i64 {
        0 => true,
        1 => false,
        _ => !on,
    }
}

/// Compare for If/Else and While: numbers when both sides are (with Auto Type), else text.
fn compare(a: &str, b2: &str, auto: bool) -> std::cmp::Ordering {
    if auto {
        if let (Some(x), Some(y)) = (auto_type(a).as_f64(), auto_type(b2).as_f64()) {
            return x.partial_cmp(&y).unwrap_or(std::cmp::Ordering::Equal);
        }
    }
    a.cmp(b2)
}

/// .NET regexes are mostly this syntax; (?'name') becomes (?<name>).
pub fn regex(p: &str) -> Option<regex_lite::Regex> {
    let p = p.replace("(?'", "(?<").replace("'>", ">");
    let p = fix_named(&p);
    match regex_lite::Regex::new(&p) {
        Ok(r) => Some(r),
        Err(e) => {
            crate::error(format!("regex {p}: {e}"));
            None
        }
    }
}

/// `(?'name'...)` is rewritten above to `(?<name>...)` but leaves the closing quote as `'`: fix it.
fn fix_named(p: &str) -> String {
    let mut out = String::new();
    let mut rest = p;
    while let Some(i) = rest.find("(?<") {
        out += &rest[..i + 3];
        rest = &rest[i + 3..];
        // Lookbehind (?<= (?<! isn't a name.
        if rest.starts_with('=') || rest.starts_with('!') {
            continue;
        }
        if let Some(j) = rest.find(['>', '\'']) {
            out += &rest[..j];
            out.push('>');
            rest = &rest[j + 1..];
        }
    }
    out + rest
}

/// Regex Match's arguments: match.count, match[i] numbered like .NET (unnamed groups first, then
/// named ones), and one argument per named group.
pub fn match_args(r: &regex_lite::Regex, text: &str, args: &mut Obj) -> bool {
    let Some(c) = r.captures(text) else { return false };
    let names: Vec<Option<&str>> = r.capture_names().collect();
    let order = (0..names.len())
        .filter(|&i| i == 0 || names[i].is_none())
        .chain((1..names.len()).filter(|&i| names[i].is_some()));
    for (k, i) in order.enumerate() {
        let v = c.get(i).map_or("", |m| m.as_str());
        args.insert(format!("match[{k}]"), v.into());
        if let Some(name) = names[i] {
            args.insert(name.into(), v.into());
        }
    }
    args.insert("match.count".into(), names.len().into());
    true
}

impl Engine {
    /// Writes the settings and the Community data now.
    pub fn save_all(&self) {
        (self.save)(&self.cfg.borrow());
        self.save_community();
    }

    pub fn save_shutdown(&self) {
        if self.cfg.borrow().rest.contains_key("templateCountdowns") {
            self.cfg
                .send_modify(|c| crate::templates::clock::pause_all(c, crate::templates::clock::now_ms()));
        }
        self.save_all();
    }

    pub fn new(cfg: Arc<watch::Sender<Config>>, save: fn(&Config)) -> Rc<Self> {
        Self::with_music(cfg, save, &crate::spotify::SPOTIFY)
    }

    pub(crate) fn with_music(
        cfg: Arc<watch::Sender<Config>>,
        save: fn(&Config),
        music: &'static dyn crate::ports::MusicPort,
    ) -> Rc<Self> {
        if cfg.borrow().rest.contains_key("templateCountdowns") {
            cfg.send_modify(|c| crate::templates::clock::resume_all(c, crate::templates::clock::now_ms()));
        }
        let memory_saver = cfg.borrow().memory_saver;
        Rc::new(Engine {
            local_spotify: RefCell::new(None),
            music,
            songs: RefCell::default(),
            song_turn: tokio::sync::Mutex::new(()),
            song_watching: std::cell::Cell::new(false),
            song_pace: std::cell::Cell::new(None),
            song_votes: RefCell::default(),
            song_last_skip: std::cell::Cell::new(None),
            song_playing: RefCell::default(),
            spotify_seen: RefCell::default(),
            overlay_state: RefCell::default(),
            memory_saver,
            cfg,
            save,
            st: RefCell::new(State {
                dev_out: std::env::var_os("STREAMERZ_DEV").map(|_| Vec::new()),
                ..Default::default()
            }),
            hub: RefCell::default(),
            twitch: RefCell::default(),
            obs: RefCell::default(),
            kick: RefCell::default(),
            youtube: RefCell::default(),
            tiktok: RefCell::default(),
            tiktok_bridge: Default::default(),
            slobs: Default::default(),
            donations: RefCell::default(),
            hotkeys: RefCell::default(),
            live_wake: tokio::sync::Notify::new(),
            chat_poke: tokio::sync::Notify::new(),
            streaming_since: std::cell::Cell::new(None),
            plugins: RefCell::default(),
        })
    }

    /// Wake YouTube and TikTok live monitors immediately when OBS/Streamlabs starts streaming.
    pub fn notify_stream_started(&self) {
        self.streaming_since.set(Some(std::time::Instant::now()));
        self.live_wake.notify_waiters();
    }

    /// Whether a stream started recently (< 90 s), so YouTube/TikTok poll faster while ingesting.
    pub fn stream_warming_up(&self) -> bool {
        self.streaming_since
            .get()
            .is_some_and(|t| t.elapsed() < Duration::from_secs(90))
    }

    /// Start or reconnect every opt-in service configured to auto-connect.
    pub fn auto_connect(self: &Rc<Self>) {
        if self.cfg.borrow().spotify.local_monitor {
            self.spotify_local_start();
        }
        let (tw, obs_on, yt, kick, tt) = {
            let c = self.cfg.borrow();
            (
                // A channel with no login is read only (irc.rs).
                !c.twitch.manual_connect && (c.twitch.broadcaster.is_some() || !c.twitch.channel.trim().is_empty()),
                c.obs
                    .connections
                    .iter()
                    .any(|o| o.get("autoConnect").and_then(Value::as_bool).unwrap_or(false)),
                !c.youtube.manual_connect
                    && (!c.youtube.video_id.trim().is_empty() || !c.youtube.channel_id.trim().is_empty()),
                !c.kick.manual_connect && !c.kick.channel.trim().is_empty(),
                !c.tiktok.manual_connect && !c.tiktok.channel.trim().is_empty(),
            )
        };
        if tw {
            if let Some(t) = self.twitch() {
                t.connect();
            } else {
                self.twitch_start();
            }
        }
        if obs_on {
            let o = self.obs_start();
            let ids: Vec<String> = self
                .cfg
                .borrow()
                .obs
                .connections
                .iter()
                .filter(|c| c.get("autoConnect").and_then(Value::as_bool).unwrap_or(false))
                .filter_map(|c| c.get("id").and_then(Value::as_str).map(String::from))
                .collect();
            for id in ids {
                if !self.obs_connected(Some(&id)) {
                    o.connect(Some(&id));
                }
            }
        }
        self.slobs_sync(true);
        if yt {
            self.youtube_start().send(crate::youtube::YouTubeCmd::Connect);
        }
        if kick {
            if let Some(k) = self.kick() {
                if !k.is_connected() {
                    k.send(crate::kick::KickCmd::Connect);
                }
            } else {
                self.kick_start();
            }
        }
        if tt {
            if let Some(t) = self.tiktok() {
                if !t.is_connected() {
                    t.send(crate::tiktok::TikTokCmd::Connect);
                }
            } else {
                self.tiktok_start();
            }
        }
        self.donations_sync();
    }

    /// Attempt to unlock saved keyring logins and, when unlocked, merge, save and auto-connect.
    pub async fn unlock_saved_logins(self: &Rc<Self>, interactive: bool) -> Result<bool, String> {
        let mut v = serde_json::to_value(&*self.cfg.borrow()).unwrap_or_default();
        let (res, v) = tokio::task::spawn_blocking(move || {
            let r = crate::secrets::retry_unlock(&mut v, interactive);
            (r, v)
        })
        .await
        .map_err(|e| e.to_string())?;
        let unlocked = res?;
        if unlocked {
            if let Ok(mut next) = serde_json::from_value::<Config>(v) {
                next.own_channels();
                self.cfg.send_replace(next);
                (self.save)(&self.cfg.borrow());
                self.auto_connect();
                crate::ui::changed();
            }
        }
        Ok(unlocked)
    }

    pub fn set_youtube(&self, handle: crate::youtube::YouTubeHandle) {
        *self.youtube.borrow_mut() = Some(handle);
    }

    pub fn youtube(&self) -> Option<crate::youtube::YouTubeHandle> {
        self.youtube.borrow().clone()
    }

    pub fn youtube_start(self: &Rc<Self>) -> crate::youtube::YouTubeHandle {
        if let Some(y) = self.youtube() {
            return y;
        }
        let y = crate::youtube::start(self.clone(), &self.cfg);
        self.set_youtube(y.clone());
        y
    }

    pub fn youtube_connected(&self) -> bool {
        self.youtube().is_some_and(|y| y.is_connected())
    }

    pub fn youtube_standby(&self) -> bool {
        self.youtube().is_some_and(|y| y.is_standby())
    }

    pub fn set_kick(&self, handle: crate::kick::KickHandle) {
        *self.kick.borrow_mut() = Some(handle);
    }

    pub fn kick(&self) -> Option<crate::kick::KickHandle> {
        self.kick.borrow().clone()
    }

    pub fn kick_start(self: &Rc<Self>) -> crate::kick::KickHandle {
        if let Some(k) = self.kick() {
            return k;
        }
        let k = crate::kick::start(self.clone(), &self.cfg);
        self.set_kick(k.clone());
        k
    }

    pub fn kick_connected(&self) -> bool {
        self.kick().is_some_and(|k| k.is_connected())
    }

    pub fn set_tiktok(&self, handle: crate::tiktok::TikTokHandle) {
        *self.tiktok.borrow_mut() = Some(handle);
    }

    pub fn tiktok(&self) -> Option<crate::tiktok::TikTokHandle> {
        self.tiktok.borrow().clone()
    }

    pub fn tiktok_start(self: &Rc<Self>) -> crate::tiktok::TikTokHandle {
        if let Some(t) = self.tiktok() {
            return t;
        }
        let t = crate::tiktok::start(self.clone(), &self.cfg);
        self.set_tiktok(t.clone());
        t
    }

    pub fn tiktok_connected(&self) -> bool {
        self.tiktok().is_some_and(|t| t.is_connected())
    }

    pub fn tiktok_standby(&self) -> bool {
        self.tiktok().is_some_and(|t| t.is_standby())
    }

    /// Plugs in the OBS adapter (the real one starts on first use; tests set a fake).
    pub fn set_obs(&self, port: Rc<dyn crate::ports::ObsPort>) {
        *self.obs.borrow_mut() = Some(port);
    }

    pub fn obs(&self) -> Option<Rc<dyn crate::ports::ObsPort>> {
        self.obs.borrow().clone()
    }

    pub fn obs_start(self: &Rc<Self>) -> Rc<dyn crate::ports::ObsPort> {
        if let Some(o) = self.obs() {
            return o;
        }
        let o: Rc<dyn crate::ports::ObsPort> = Rc::new(crate::obs::start(self.clone(), &self.cfg));
        self.set_obs(o.clone());
        o
    }

    pub fn music(&self) -> &'static dyn crate::ports::MusicPort {
        self.music
    }

    pub fn obs_connected(&self, conn_id: Option<&str>) -> bool {
        self.obs().is_some_and(|o| o.is_connected(conn_id))
    }

    pub fn set_twitch(&self, handle: crate::twitch::TwitchHandle) {
        *self.twitch.borrow_mut() = Some(handle);
    }

    pub fn twitch(&self) -> Option<crate::twitch::TwitchHandle> {
        self.twitch.borrow().clone()
    }

    /// The Twitch runner, started on first use: nothing runs for Twitch until it's turned on.
    pub fn twitch_start(self: &Rc<Self>) -> crate::twitch::TwitchHandle {
        if let Some(t) = self.twitch() {
            return t;
        }
        let t = crate::twitch::start(self.clone(), &self.cfg);
        self.set_twitch(t.clone());
        t
    }

    pub fn twitch_connected(&self) -> bool {
        self.twitch().is_some_and(|t| t.is_connected())
    }

    pub fn twitch_broadcaster(&self) -> Option<crate::twitch::TwitchAccount> {
        self.cfg.borrow().twitch.broadcaster.clone()
    }

    pub fn send_twitch_message(
        &self,
        text: &str,
        reply_id: Option<&str>,
        use_bot: bool,
        fallback: bool,
    ) -> Result<(), String> {
        if self.dev_out("twitch", text, use_bot) {
            return Ok(());
        }
        let tw = self.twitch().ok_or("Twitch is not connected")?;
        let text = text.to_string();
        let reply_id = reply_id.map(String::from);
        tokio::task::spawn_local(async move {
            if let Err(e) = tw.send_message(text, reply_id, use_bot, fallback).await {
                crate::error(e);
            }
        });
        Ok(())
    }

    /// Run `f` on the config, and save it soon (once for a burst of changes).
    pub fn change(self: &Rc<Self>, f: impl FnOnce(&mut Config)) {
        self.cfg.send_modify(f);
        crate::ui::changed();
        let mut st = self.st.borrow_mut();
        if st.save_pending {
            return;
        }
        st.save_pending = true;
        let me = self.clone();
        tokio::task::spawn_local(async move {
            tokio::time::sleep(Duration::from_secs(1)).await;
            me.st.borrow_mut().save_pending = false;
            (me.save)(&me.cfg.borrow());
        });
    }

    /// The settings' sender, for code that takes it (platform event handlers in scenarios).
    pub(crate) fn cfg_tx(&self) -> &Arc<watch::Sender<Config>> {
        &self.cfg
    }

    pub fn config(&self) -> watch::Ref<'_, Config> {
        self.cfg.borrow()
    }

    pub fn subscribe(&self) -> watch::Receiver<Config> {
        self.cfg.subscribe()
    }
    pub fn fire_remote_trigger(self: &Rc<Self>, instance_id: &str, event_name: &str, mut args: Obj) {
        let inst = if instance_id.is_empty() { "Any" } else { instance_id };
        args.insert("instanceId".into(), inst.into());
        args.insert("eventName".into(), event_name.into());
        let inst_id_str = instance_id.to_string();
        let ev_str = event_name.to_string();
        self.fire(
            crate::remote::T_REMOTE_TRIGGER,
            "Remote Instance Trigger",
            "Integrations/Streamer.bot Remote",
            "streamerbotremote",
            &args,
            |t| {
                let want_inst = s(t, "instanceId");
                let want_ev = s(t, "eventName");
                (want_inst.is_empty() || want_inst == "Any" || want_inst == inst_id_str)
                    && (want_ev.is_empty() || want_ev == ev_str)
            },
        );
        self.emit("StreamerbotRemote", "InstanceTrigger", || {
            json!({
                "instanceId": instance_id,
                "eventName": event_name,
                "arguments": args
            })
        });
    }

    pub fn deliver_remote_signal(self: &Rc<Self>, instance_id: &str, signal_name: &str, mut args: Obj) {
        let inst = if instance_id.is_empty() { "Any" } else { instance_id };
        args.insert("instanceId".into(), inst.into());
        args.insert("signalName".into(), signal_name.into());
        let mut delivered = false;
        let mut st = self.st.borrow_mut();
        for key in [
            (instance_id.to_string(), signal_name.to_string()),
            ("".to_string(), signal_name.to_string()),
            ("Any".to_string(), signal_name.to_string()),
        ] {
            if let Some(waiters) = st.remote_waiters.remove(&key) {
                for w in waiters {
                    delivered |= w.send(args.clone()).is_ok();
                }
            }
        }
        if !delivered {
            st.remote_signals
                .entry((instance_id.to_string(), signal_name.to_string()))
                .or_default()
                .push_back(args.clone());
        }
        drop(st);

        let inst_id_str = instance_id.to_string();
        let sig_str = signal_name.to_string();
        self.fire(
            crate::remote::T_REMOTE_SIGNAL,
            "Remote Instance Signal",
            "Integrations/Streamer.bot Remote",
            "streamerbotremote",
            &args,
            |t| {
                let want_inst = s(t, "instanceId");
                let want_sig = s(t, "signalName");
                (want_inst.is_empty() || want_inst == "Any" || want_inst == inst_id_str)
                    && (want_sig.is_empty() || want_sig == sig_str)
            },
        );
        self.emit("StreamerbotRemote", "InstanceSignal", || {
            json!({
                "instanceId": instance_id,
                "signalName": signal_name,
                "arguments": args
            })
        });
    }

    pub async fn wait_remote_signal(&self, instance_id: &str, signal_name: &str, timeout_ms: u64) -> Option<Obj> {
        let (queued, rx) = {
            let mut st = self.st.borrow_mut();
            let key = (instance_id.to_string(), signal_name.to_string());
            let queued = if let Some(q) = st.remote_signals.get_mut(&key) {
                q.pop_front()
            } else if instance_id.is_empty() || instance_id == "Any" {
                st.remote_signals
                    .iter_mut()
                    .find_map(|(k, v)| if k.1 == signal_name { v.pop_front() } else { None })
            } else {
                None
            };
            if let Some(p) = queued {
                (Some(p), None)
            } else {
                let (tx, rx) = oneshot::channel();
                st.remote_waiters.entry(key).or_default().push(tx);
                (None, Some(rx))
            }
        };
        if let Some(p) = queued {
            return Some(p);
        }
        let rx = rx?;
        let ms = match timeout_ms {
            0 => 30_000,
            t => t.min(60_000),
        };
        tokio::time::timeout(std::time::Duration::from_millis(ms), rx)
            .await
            .ok()
            .and_then(Result::ok)
    }

    pub fn stream_deck_add(&self, id: u64, tx: tokio::sync::mpsc::UnboundedSender<String>) {
        self.st.borrow_mut().stream_decks.push((id, tx));
    }

    pub fn stream_deck_remove(&self, id: u64) {
        self.st.borrow_mut().stream_decks.retain(|d| d.0 != id);
    }

    /// Push to every connected Stream Deck plugin; how many got it.
    pub fn stream_deck_push(&self, msg: &Value) -> usize {
        let text = msg.to_string();
        self.st
            .borrow()
            .stream_decks
            .iter()
            .filter(|d| d.1.send(text.clone()).is_ok())
            .count()
    }

    /// A WebSocket client reached a Streamer.bot or Streamerz server: a remote instance.
    pub fn remote_add(&self, id: &str, name: &str, tx: tokio::sync::mpsc::UnboundedSender<String>) {
        let mut st = self.st.borrow_mut();
        st.remotes.retain(|r| r.0 != id);
        st.remotes.push((id.to_string(), name.to_string(), tx));
    }

    /// It went away; its name.
    pub fn remote_remove(&self, id: &str) -> String {
        let mut st = self.st.borrow_mut();
        let name = st
            .remotes
            .iter()
            .find(|r| r.0 == id)
            .map(|r| r.1.clone())
            .unwrap_or_default();
        st.remotes.retain(|r| r.0 != id);
        name
    }

    /// Send a request to the remote instance `instance` (its id or name; empty or "Any": all of them).
    /// How many got it.
    pub fn remote_send(&self, instance: &str, request: &Value) -> usize {
        let text = request.to_string();
        let all = instance.is_empty() || instance.eq_ignore_ascii_case("any");
        self.st
            .borrow()
            .remotes
            .iter()
            .filter(|r| all || r.0 == instance || r.1.eq_ignore_ascii_case(instance))
            .filter(|r| r.2.send(text.clone()).is_ok())
            .count()
    }

    pub fn remote_connected(self: &Rc<Self>, instance_id: &str, instance_name: &str) {
        let mut args = Obj::new();
        args.insert("instanceId".into(), instance_id.into());
        args.insert("instanceName".into(), instance_name.into());
        let inst_id_str = instance_id.to_string();
        self.fire(
            crate::remote::T_REMOTE_CONNECTED,
            "Remote Instance Connected",
            "Integrations/Streamer.bot Remote",
            "streamerbotremote",
            &args,
            |t| {
                let want = s(t, "instanceId");
                want.is_empty() || want == "Any" || want == inst_id_str
            },
        );
        self.emit("StreamerbotRemote", "InstanceConnected", || {
            json!({
                "instanceId": instance_id,
                "instanceName": instance_name
            })
        });
    }

    pub fn fire_chat_window_action(
        self: &Rc<Self>,
        platform: &str,
        user: &str,
        user_id: &str,
        text: &str,
        message_id: &str,
        action_name: &str,
    ) {
        let mut args = Obj::new();
        args.insert("platform".into(), platform.into());
        args.insert("user".into(), user.into());
        args.insert("userId".into(), user_id.into());
        args.insert("text".into(), text.into());
        args.insert("messageId".into(), message_id.into());
        args.insert("actionName".into(), action_name.into());
        let act_str = action_name.to_string();
        self.fire(
            crate::decks::T_CHAT_WINDOW_ACTION,
            "Chat Window Action",
            "Misc",
            "misc",
            &args,
            |t| {
                let want = s(t, "actionName");
                want.is_empty() || want == act_str
            },
        );
        self.emit("Misc", "ChatWindowAction", || {
            json!({
                "platform": platform,
                "user": user,
                "userId": user_id,
                "text": text,
                "messageId": message_id,
                "actionName": action_name
            })
        });
    }

    pub fn remote_disconnected(self: &Rc<Self>, instance_id: &str, instance_name: &str) {
        let mut args = Obj::new();
        args.insert("instanceId".into(), instance_id.into());
        args.insert("instanceName".into(), instance_name.into());
        let inst_id_str = instance_id.to_string();
        self.fire(
            crate::remote::T_REMOTE_DISCONNECTED,
            "Remote Instance Disconnected",
            "Integrations/Streamer.bot Remote",
            "streamerbotremote",
            &args,
            |t| {
                let want = s(t, "instanceId");
                want.is_empty() || want == "Any" || want == inst_id_str
            },
        );
        self.emit("StreamerbotRemote", "InstanceDisconnected", || {
            json!({
                "instanceId": instance_id,
                "instanceName": instance_name
            })
        });
    }

    fn action<T>(&self, id: &str, f: impl FnOnce(&Action) -> T) -> Option<T> {
        self.cfg.borrow().actions.actions.iter().find(|a| a.id == id).map(f)
    }

    fn expand(&self, ctx: &Ctx, t: &str) -> String {
        let c = self.cfg.borrow();
        if !t.contains("%group.") {
            return vars::expand(t, &ctx.args, &c.globals);
        }
        let args = group_args(ctx, &c);
        vars::expand(t, &args, &c.globals)
    }

    fn duration_number(&self, ctx: &Ctx, object: &Obj, key: &str) -> f64 {
        let value = object
            .get(key)
            .and_then(Value::as_str)
            .map(|value| self.expand(ctx, value).trim().parse::<f64>().unwrap_or(0.0))
            .unwrap_or_else(|| n(object, key));
        if value.is_finite() {
            value
        } else {
            0.0
        }
    }

    fn simple(&self, ctx: &Ctx, t: &str) -> String {
        let c = self.cfg.borrow();
        if !t.contains("%group.") {
            return vars::simple(t, &ctx.args, &c.globals);
        }
        let args = group_args(ctx, &c);
        vars::simple(t, &args, &c.globals)
    }

    /// Fire a trigger: every enabled action with an enabled trigger of this type that `matches`
    /// gets queued with `args` plus the trigger's own (triggerId, triggerName, ...). How many.
    pub fn fire(
        self: &Rc<Self>,
        kind: u32,
        trigger_name: &str,
        category: &str,
        source: &str,
        args: &Obj,
        matches: impl Fn(&Obj) -> bool,
    ) -> usize {
        self.record(kind, args);
        self.chat_event(kind, args);
        if !self.plugins.borrow().is_empty() {
            self.plugins_hear(name(TRIGGERS, kind).unwrap_or(trigger_name), args);
        }
        let hits: Vec<(String, String)> = {
            let c = self.cfg.borrow();
            c.actions
                .actions
                .iter()
                .filter(|a| a.enabled)
                .flat_map(|a| {
                    a.triggers
                        .iter()
                        .filter(|t| t.get("type").and_then(Value::as_u64) == Some(kind as u64))
                        .filter(|t| b(t, "enabled", true) && matches(t))
                        .map(|t| (a.id.clone(), s(t, "id").to_string()))
                })
                .collect()
        };
        for (action, trigger) in &hits {
            let mut a = args.clone();
            a.insert("triggerId".into(), trigger.as_str().into());
            a.insert("triggerName".into(), trigger_name.into());
            a.insert("triggerCategory".into(), category.into());
            a.insert("eventSource".into(), source.into());
            a.insert("__source".into(), name(TRIGGERS, kind).unwrap_or("").into());
            self.queue(action, a);
        }
        hits.len()
    }

    /// Whether an enabled action has an enabled trigger of `kind` (what's polled only when used).
    pub fn uses_trigger(&self, kind: u32) -> bool {
        self.cfg.borrow().actions.actions.iter().filter(|a| a.enabled).any(|a| {
            a.triggers
                .iter()
                .any(|t| t.get("type").and_then(Value::as_u64) == Some(kind as u64) && b(t, "enabled", true))
        })
    }

    /// Streamerz started (Streamer.bot's Started trigger).
    pub fn started(self: &Rc<Self>) {
        self.fire(
            T_STARTED,
            "Streamerz Started",
            "Core",
            "streamerbot",
            &Obj::new(),
            |_| true,
        );
        self.emit("Misc", "StreamerbotStarted", || json!({}));
    }

    /// Run an action as its Test trigger does (isTest true). Its arguments, if it ran.
    pub fn test(self: &Rc<Self>, action: &str) -> Option<Obj> {
        let mut args = Obj::new();
        args.insert("isTest".into(), true.into());
        args.insert("triggerName".into(), "Test".into());
        args.insert("triggerCategory".into(), "Core".into());
        args.insert("eventSource".into(), "streamerbot".into());
        args.insert("__source".into(), name(TRIGGERS, T_TEST).unwrap_or("Test").into());
        self.queue(action, args)
    }

    /// Queue an action (by id or name) with these arguments; it starts now unless its queue is
    /// busy (blocking) or paused. Its arguments as queued (with actionId, actionName, queueName,
    /// runningActionId and actionQueuedAt); None when there's no such enabled action.
    pub fn queue(self: &Rc<Self>, action: &str, mut args: Obj) -> Option<Obj> {
        let (id, aname, queue, concurrent) = {
            let c = self.cfg.borrow();
            let a = c
                .actions
                .actions
                .iter()
                .find(|a| a.id == action)
                .or_else(|| c.actions.actions.iter().find(|a| a.name == action))?;
            if !a.enabled {
                return None;
            }
            let q = a
                .queue
                .clone()
                .filter(|q| !q.is_empty())
                .unwrap_or(DEFAULT_QUEUE.into());
            (a.id.clone(), a.name.clone(), q, a.concurrent)
        };
        let queue_name = if queue == DEFAULT_QUEUE {
            "Default".to_string()
        } else {
            let c = self.cfg.borrow();
            c.actions
                .queues
                .iter()
                .find(|q| s(q, "id") == queue)
                .map(|q| s(q, "name").to_string())
                .unwrap_or_default()
        };
        let rid = uuid();
        let queued_at = globals::iso_now();
        // A run queued by another action keeps that one's (Streamer.bot does the same).
        for (k, v) in [
            ("actionId", &id),
            ("actionName", &aname),
            ("queueName", &queue_name),
            ("runningActionId", &rid),
            ("actionQueuedAt", &queued_at),
        ] {
            args.entry(k).or_insert_with(|| v.as_str().into());
        }
        let out = args.clone();
        let run = Run {
            id: rid,
            action: id,
            name: aname,
            args,
            queued_at,
        };
        if concurrent {
            // Outside any queue.
            let me = self.clone();
            tokio::task::spawn_local(async move { me.execute(run).await });
            return Some(out);
        }
        self.st
            .borrow_mut()
            .queues
            .entry(queue.clone())
            .or_default()
            .waiting
            .push_back(run);
        self.pump(&queue);
        crate::ui::changed();
        Some(out)
    }

    fn blocking(&self, queue: &str) -> bool {
        let c = self.cfg.borrow();
        if queue == DEFAULT_QUEUE {
            return c.actions.blocking;
        }
        c.actions
            .queues
            .iter()
            .find(|q| s(q, "id") == queue)
            .is_some_and(|q| b(q, "blocking", false))
    }

    /// Start what a queue can run now.
    fn pump(self: &Rc<Self>, queue: &str) {
        let blocking = self.blocking(queue);
        loop {
            let run = {
                let mut st = self.st.borrow_mut();
                let q = st.queues.entry(queue.into()).or_default();
                if q.paused || (blocking && !q.running.is_empty()) {
                    return;
                }
                let Some(run) = q.waiting.pop_front() else { return };
                q.running.push((run.id.clone(), run.name.clone()));
                run
            };
            let (me, queue) = (self.clone(), queue.to_string());
            tokio::task::spawn_local(async move {
                let id = run.id.clone();
                me.execute(run).await;
                if let Some(q) = me.st.borrow_mut().queues.get_mut(&queue) {
                    q.running.retain(|(r, _)| *r != id);
                }
                me.pump(&queue);
            });
        }
    }

    /// Run an action's sub-actions, with the run's arguments plus the action's own.
    async fn execute(self: &Rc<Self>, run: Run) {
        let Some((subs, random, group)) = self.action(&run.action, |a| {
            (a.sub_actions.clone(), a.random_action, a.group.clone())
        }) else {
            return;
        };
        let mut ctx = Ctx { args: run.args, group };
        let start = ctx.args.clone();
        let started = (std::time::Instant::now(), globals::iso_now());
        self.emit(
            "Raw",
            "Action",
            || json!({"id": run.id, "actionId": run.action, "name": run.name, "arguments": start}),
        );
        let subs: Vec<Value> = subs.into_iter().map(Value::Object).collect();
        let outcome = RUN_OUTCOME
            .scope(RefCell::new(RunOutcome::default()), async {
                self.list(&mut ctx, &subs, random).await;
                RUN_OUTCOME.with(|state| state.borrow().clone())
            })
            .await;
        self.emit("Raw", "ActionCompleted", || {
            json!({"duration": started.0.elapsed().as_secs_f64() * 1000.0, "queuedAt": run.queued_at,
                "startedAt": started.1, "completedAt": globals::iso_now(), "success": outcome.errors == 0,
                "outcome": outcome, "executionStatus": outcome.status(),
                "excludeFromHistory": self.action(&run.action, |a| a.exclude_from_history).unwrap_or(false),
                "id": run.id, "actionId": run.action, "name": run.name, "arguments": ctx.args})
        });
        if self.action(&run.action, |a| a.exclude_from_history).unwrap_or(false) {
            return;
        }
        let mut st = self.st.borrow_mut();
        if st.history.len() == HISTORY {
            st.history.pop_front();
        }
        st.history.push_back(Record {
            outcome,
            action_id: run.action,
            name: run.name,
            args: start,
            after: ctx.args,
            queued_at: run.queued_at,
            ms: started.0.elapsed().as_secs_f64() * 1000.0,
        });
        drop(st);
        crate::ui::changed();
    }

    /// Run a sub-action list: in order, or one of the enabled ones at random (by weight).
    fn list<'a>(self: &'a Rc<Self>, ctx: &'a mut Ctx, subs: &'a [Value], random: bool) -> Fut<'a, Flow> {
        Box::pin(async move {
            let on: Vec<&Obj> = subs
                .iter()
                .filter_map(Value::as_object)
                .filter(|o| b(o, "enabled", true))
                .collect();
            if random {
                let total: f64 = on.iter().map(|o| n(o, "weight").max(0.0)).sum();
                let pick = if total > 0.0 {
                    let mut r = crate::math::random() * total;
                    on.iter().position(|o| {
                        r -= n(o, "weight").max(0.0);
                        r < 0.0
                    })
                } else if on.is_empty() {
                    None
                } else {
                    Some((crate::math::random() * on.len() as f64) as usize % on.len())
                };
                return match pick.and_then(|i| on.get(i)) {
                    Some(o) => self.one(ctx, o).await,
                    None => Flow::Continue,
                };
            }
            for o in on {
                let f = self.one(ctx, o).await;
                if f != Flow::Continue {
                    return f;
                }
            }
            Flow::Continue
        })
    }

    /// A container's children (If/Else's true/false branches, Switch cases, groups).
    fn branch<'a>(self: &'a Rc<Self>, ctx: &'a mut Ctx, o: &'a Obj) -> Fut<'a, Flow> {
        self.list(ctx, subs(o), b(o, "random", false))
    }

    fn one<'a>(self: &'a Rc<Self>, ctx: &'a mut Ctx, o: &'a Obj) -> Fut<'a, Flow> {
        Box::pin(async move {
            let kind = o.get("type").and_then(Value::as_u64).unwrap_or(0);
            let _ = RUN_OUTCOME.try_with(|state| state.borrow_mut().step = kind);
            // Containers (If/Else branches, Switch cases, groups) aren't sub-actions of their own.
            if !(99900..99999).contains(&kind) && self.listening("Raw", "SubAction") {
                let what = crate::types::SUB_ACTION_INFO
                    .binary_search_by_key(&(kind as u32), |x| x.0)
                    .map_or("", |i| crate::types::SUB_ACTION_INFO[i].1);
                let a = &ctx.args;
                let data = json!({"parentId": a.get("actionId"), "parentName": a.get("actionName"), "type": kind,
                    "id": a.get("runningActionId"), "actionId": o.get("id"), "name": what, "arguments": a});
                self.emit("Raw", "SubAction", || data);
            }
            match kind {
                // Run Action
                4 => {
                    let id = self.simple(ctx, s(o, "actionId"));
                    if b(o, "runImmedately", true) {
                        if let Some((subs, random, group)) =
                            self.action(&id, |a| (a.sub_actions.clone(), a.random_action, a.group.clone()))
                        {
                            let subs: Vec<Value> = subs.into_iter().map(Value::Object).collect();
                            let previous = std::mem::replace(&mut ctx.group, group);
                            let flow = self.list(ctx, &subs, random).await;
                            ctx.group = previous;
                            if flow == Flow::BreakToRoot {
                                return Flow::BreakToRoot;
                            }
                        }
                    } else {
                        self.queue(&id, ctx.args.clone());
                    }
                }
                2
                | 6
                | 8
                | 9
                | 10..=29
                | 30..=49
                | 50..=52
                | 200..=202
                | 204..=208
                | 213..=215
                | 251..=254
                | 321..=329
                | 500..=513
                | 550..=573
                | 575..=580
                | 1007
                | 3
                | 5
                | 1006
                | 1010
                | 1012..=1015
                | 1021
                | 1030
                | 1039
                | 1040
                | 1018
                | 1019
                | 1024
                | 1025
                | 9001..=9003
                | 11001
                | 11002
                | 11004..=11008
                | 4001..=4012
                | 7001
                | 8001
                | 8002
                | 10001
                | 15001
                | 20001..=20003
                | 35001
                | 35002
                | 35010..=35012
                | 35020..=35022
                | 35030
                | 35031
                | 35040..=35043
                | 35101..=35103
                | 35105..=35108
                | 35113
                | 43001..=43003 => self.rest(ctx, o, kind).await,
                1000 | 1026 | 1027 | 1035..=1037 => self.community_sub(ctx, o, kind),
                // Streamlabs Desktop
                401..=415 | 417 => {
                    let mut x = o.clone();
                    for k in ["sceneName", "sourceName", "groupName", "filterName", "url", "text"] {
                        if let Some(t) = o.get(k).and_then(Value::as_str) {
                            x.insert(k.into(), self.expand(ctx, t).into());
                        }
                    }
                    match self.slobs_sub(kind, &x).await {
                        Ok(Some(scene)) => drop(ctx.args.insert("currentScene".into(), scene.into())),
                        Ok(None) => {}
                        Err(e) => crate::error(format!("Streamlabs Desktop: {e}")),
                    }
                }
                // Set Argument
                123 => {
                    let k = self.simple(ctx, s(o, "variableName"));
                    let v = self.expand(ctx, s(o, "value"));
                    let v = if b(o, "autoType", false) {
                        auto_type(&v)
                    } else {
                        v.into()
                    };
                    if let Some(name) = k
                        .strip_prefix("group.")
                        .filter(|n| !n.is_empty() && !ctx.group.is_empty())
                    {
                        self.change(|c| {
                            let group = c
                                .action_group_variables
                                .entry(ctx.group.clone())
                                .or_insert_with(|| Value::Object(Obj::new()));
                            if let Some(group) = group.as_object_mut() {
                                group.insert(name.into(), v);
                            }
                        });
                    } else {
                        ctx.args.insert(k, v);
                    }
                }
                // If/Else
                120 => {
                    let ok = self.condition(ctx, o);
                    let want = if ok { 99901 } else { 99902 };
                    let branch = subs(o)
                        .iter()
                        .filter_map(Value::as_object)
                        .find(|c| c.get("type").and_then(Value::as_u64) == Some(want));
                    if let Some(c) = branch {
                        return self.branch(ctx, c).await;
                    }
                }
                // Global (Get) / (Set)
                121 => self.global_get(ctx, o),
                122 => self.global_set(ctx, o),
                124 => return Flow::BreakToRoot,
                // Clear Non-Persisted Globals / User Globals
                125 => self.st.borrow_mut().temp.clear(),
                126 => self.st.borrow_mut().user_temp.clear(),
                // Switch
                127 => {
                    let input = self.expand(ctx, s(o, "input"));
                    let auto = b(o, "autoType", false);
                    let mut default = None;
                    for c in subs(o).iter().filter_map(Value::as_object) {
                        match c.get("type").and_then(Value::as_u64) {
                            Some(99903) => {
                                let cs = b(c, "caseSensitive", false);
                                let hit = c.get("values").and_then(Value::as_array).is_some_and(|vs| {
                                    vs.iter().any(|v| {
                                        let v = self.expand(ctx, &text(v));
                                        if auto && compare(&input, &v, true).is_eq() && auto_type(&v).is_number() {
                                            return true;
                                        }
                                        if cs {
                                            v == input
                                        } else {
                                            v.to_lowercase() == input.to_lowercase()
                                        }
                                    })
                                });
                                if hit {
                                    return self.branch(ctx, c).await;
                                }
                            }
                            Some(99904) => default = Some(c),
                            _ => {}
                        }
                    }
                    if let Some(c) = default {
                        return self.branch(ctx, c).await;
                    }
                }
                // While
                128 => loop {
                    if !self.while_condition(ctx, o) {
                        break;
                    }
                    match self.list(ctx, subs(o), false).await {
                        Flow::BreakToRoot => return Flow::BreakToRoot,
                        Flow::LoopBreak => break,
                        _ => {}
                    }
                    // Let the rest run, even in an endless loop.
                    tokio::task::yield_now().await;
                },
                129 => return Flow::LoopBreak,
                130 => return Flow::LoopContinue,
                // Set Action Queue Pause State: 0 pause, 1 resume; no queue: all of them.
                1001 => {
                    let q = self.simple(ctx, s(o, "actionQueueId"));
                    self.pause(&q, n(o, "state") as i64 == 0, b(o, "clear", false));
                }
                // Delay, in ms; Random: between value and maxValue.
                1002 => {
                    let number = |key| {
                        o.get(key)
                            .and_then(Value::as_str)
                            .map(|text| self.expand(ctx, text).parse::<f64>().unwrap_or(0.0))
                            .unwrap_or_else(|| n(o, key))
                    };
                    let (lo, hi) = (number("value").max(0.0), number("maxValue").max(0.0));
                    let ms = if b(o, "random", false) && hi > lo {
                        lo + crate::math::random() * (hi - lo)
                    } else {
                        lo
                    };
                    tokio::time::sleep(Duration::from_millis(ms as u64)).await;
                }
                // Get Random Number: 0 between min and max (inclusive), 1 a float.
                1003 => {
                    if n(o, "randomType") as i64 == 1 {
                        let f = crate::math::random();
                        ctx.args.insert("randomFloat".into(), vars::number(f));
                        ctx.args.insert("randomPercent".into(), ((f * 100.0) as i64).into());
                    } else {
                        let (lo, hi) = (n(o, "min") as i64, n(o, "max") as i64);
                        let (lo, hi) = (lo.min(hi), lo.max(hi));
                        let r = lo + (crate::math::random() * (hi - lo + 1) as f64) as i64;
                        ctx.args.insert("randomNumber".into(), r.min(hi).into());
                    }
                }
                // Set Action State
                1004 => {
                    let id = self.simple(ctx, s(o, "actionId"));
                    let st = n(o, "state");
                    self.change(|c| {
                        if let Some(a) = c.actions.actions.iter_mut().find(|a| a.id == id) {
                            a.enabled = new_state(st, a.enabled);
                        }
                    });
                }
                // Clear Action Queue
                1005 => {
                    self.clear_queue(&self.simple(ctx, s(o, "actionQueueId")));
                }
                // Comment
                1009 => {}
                // Set Action Group State
                1011 => {
                    let g = self.simple(ctx, s(o, "groupName"));
                    let st = n(o, "state");
                    self.change(|c| {
                        for a in c.actions.actions.iter_mut().filter(|a| a.group == g) {
                            a.enabled = new_state(st, a.enabled);
                        }
                    });
                }
                // Get Action State
                1016 => {
                    let id = self.simple(ctx, s(o, "actionId"));
                    let on = self.action(&id, |a| a.enabled).unwrap_or(false);
                    ctx.args.insert("actionState".into(), on.into());
                }
                // Get Action Group State: 0 all disabled, 1 all enabled, 2 mixed.
                1017 => {
                    let g = self.simple(ctx, s(o, "groupName"));
                    let (on, off): (Vec<_>, Vec<_>) = {
                        let c = self.cfg.borrow();
                        c.actions
                            .actions
                            .iter()
                            .filter(|a| a.group == g)
                            .map(|a| (a.id.clone(), a.enabled))
                            .partition(|(_, e)| *e)
                    };
                    let state = match (on.is_empty(), off.is_empty()) {
                        (false, true) => 1,
                        (true, _) => 0,
                        _ => 2,
                    };
                    let ids = |v: Vec<(String, bool)>| Value::from(v.into_iter().map(|(i, _)| i).collect::<Vec<_>>());
                    ctx.args.insert("actionGroupState".into(), state.into());
                    ctx.args.insert("actionsEnabled".into(), ids(on));
                    ctx.args.insert("actionsDisabled".into(), ids(off));
                }
                // Log
                1020 => crate::say(self.expand(ctx, s(o, "message"))),
                // Set Action Queue Blocking State: 0 blocking, 1 not.
                1022 => {
                    let q = self.simple(ctx, s(o, "actionQueueId"));
                    self.set_blocking(&q, n(o, "state") as i64 == 0);
                }
                // Get Input: a dialog in the window.
                1023 => {
                    let rx = crate::ui::input(
                        self.expand(ctx, s(o, "title")),
                        self.expand(ctx, s(o, "prompt")),
                        self.expand(ctx, s(o, "originalText")),
                    );
                    let mut var = self.simple(ctx, s(o, "outputVariable"));
                    if var.is_empty() {
                        var = "inputResult".into();
                    }
                    match rx.await.ok().flatten() {
                        Some(v) => {
                            let v = if b(o, "autoTypeOutput", false) {
                                auto_type(&v)
                            } else {
                                v.into()
                            };
                            ctx.args.insert(var, v);
                            ctx.args.insert("success".into(), true.into());
                        }
                        None => {
                            ctx.args.insert("success".into(), false.into());
                        }
                    }
                }
                // Set Sub-Action State / Weight
                1028 | 1038 => {
                    let id = self.simple(ctx, s(o, "actionId"));
                    let sub = self.simple(ctx, s(o, "subActionId"));
                    let (st, w) = (n(o, "state"), n(o, "weight"));
                    self.change(|c| {
                        if let Some(a) = c.actions.actions.iter_mut().find(|a| a.id == id) {
                            with_sub(&mut a.sub_actions, &sub, &mut |x| {
                                if kind == 1028 {
                                    let on = b(x, "enabled", true);
                                    x.insert("enabled".into(), new_state(st, on).into());
                                } else {
                                    x.insert("weight".into(), json!(w));
                                }
                            });
                        }
                    });
                }
                // Send Local Signal
                1031 => {
                    let name = self.simple(ctx, s(o, "signalName"));
                    let mut payload = if b(o, "useArge", false) {
                        ctx.args.clone()
                    } else {
                        Obj::new()
                    };
                    if let Some(custom) = o.get("customArgs").and_then(Value::as_object) {
                        for (k, v) in custom {
                            payload.insert(k.clone(), auto_type(&self.expand(ctx, &text(v))));
                        }
                    }
                    let mut st = self.st.borrow_mut();
                    let waiting = st.waiters.remove(&name).unwrap_or_default();
                    let mut delivered = false;
                    for w in waiting {
                        delivered |= w.send(payload.clone()).is_ok();
                    }
                    if !delivered && b(o, "queueSignal", false) {
                        st.signals.entry(name).or_default().push_back(payload);
                    }
                }
                // Wait For Local Signal: up to `timeout` ms (blank or over 30 s: 30 s).
                1032 => {
                    let name = self.simple(ctx, s(o, "signalName"));
                    let ms = match self.duration_number(ctx, o, "timeout") as u64 {
                        0 => 30_000,
                        t => t.min(30_000),
                    };
                    let queued = self
                        .st
                        .borrow_mut()
                        .signals
                        .get_mut(&name)
                        .and_then(VecDeque::pop_front);
                    let got = match queued {
                        Some(p) => Some(p),
                        None => {
                            let (tx, rx) = oneshot::channel();
                            self.st.borrow_mut().waiters.entry(name.clone()).or_default().push(tx);
                            tokio::time::timeout(Duration::from_millis(ms), rx)
                                .await
                                .ok()
                                .and_then(Result::ok)
                        }
                    };
                    ctx.args.insert("signalName".into(), name.as_str().into());
                    match got {
                        Some(p) => {
                            let over = b(o, "overwrite", false);
                            for (k, v) in p {
                                if over || !ctx.args.contains_key(&k) {
                                    ctx.args.insert(k, v);
                                }
                            }
                            ctx.args.insert("signalTimeout".into(), false.into());
                            ctx.args.insert("hasResult".into(), true.into());
                        }
                        None => {
                            crate::warn(format!("Wait For Local Signal: {name} timed out"));
                            ctx.args.insert("signalTimeout".into(), true.into());
                        }
                    }
                }
                // Set Trigger State
                1033 => {
                    let id = self.simple(ctx, s(o, "actionId"));
                    let trig = self.simple(ctx, s(o, "triggerId"));
                    let st = n(o, "state");
                    self.change(|c| {
                        let a = c.actions.actions.iter_mut().find(|a| a.id == id);
                        if let Some(t) = a.and_then(|a| a.triggers.iter_mut().find(|t| s(t, "id") == trig)) {
                            let on = b(t, "enabled", true);
                            t.insert("enabled".into(), new_state(st, on).into());
                        }
                    });
                }
                // Custom Event Trigger: fires the Custom trigger with this event name.
                1034 => {
                    let ev = self.expand(ctx, s(o, "eventName"));
                    let args = if b(o, "useArgs", false) {
                        ctx.args.clone()
                    } else {
                        Obj::new()
                    };
                    self.fire(T_CUSTOM, "Custom", "Custom", "custom", &args, |t| {
                        let want = s(t, "eventName");
                        want.is_empty() || want == ev
                    });
                }
                // Group
                99900 => return self.branch(ctx, o).await,
                // Streamerz-only: the serialized operation is preserved by Streamer.bot but
                // only Streamerz can execute it.
                99802 => self.song_op(ctx, o).await,
                99827..=99831 => {
                    let operation = match kind {
                        99827 => "remove",
                        99828 => "skip",
                        99829 => "voteskip",
                        99830 => "bansong",
                        _ => "like",
                    };
                    let mut o = o.clone();
                    o.insert("operation".into(), operation.into());
                    self.song_op(ctx, &o).await
                }
                99832 => self.song_clear(ctx),
                99833 => self.song_list(ctx),
                99810..=99824 | 99840..=99875 => {
                    let value = self.expand(ctx, s(o, "value"));
                    let value = if kind >= 99840 {
                        crate::spotify::api::pack(&value, &self.expand(ctx, s(o, "extra")))
                    } else {
                        value
                    };
                    let result = if kind == 99810 {
                        self.music()
                            .current(self)
                            .await
                            .and_then(|track| serde_json::to_value(track).map_err(|error| error.to_string()))
                    } else if kind == 99819 {
                        self.music().queue(self, &value).await.map(Value::String)
                    } else {
                        self.music().player_action(self, kind as u32, &value).await
                    };
                    match result {
                        Ok(result) => {
                            self.spotify_done(ctx, kind as u32, &value, &result);
                            // A true or false reads "true" or "false" in a text, not "True".
                            let result = match result {
                                Value::Bool(b) => Value::String(b.to_string()),
                                other => other,
                            };
                            ctx.args.insert("spotifyResult".into(), result);
                            ctx.args.insert("spotifyError".into(), "".into());
                        }
                        Err(error) => {
                            let error = crate::spotify::friendly(&error);
                            record_error(&error);
                            ctx.args.insert("spotifyResult".into(), Value::Null);
                            ctx.args.insert("spotifyText".into(), "".into());
                            ctx.args.insert("spotifyCount".into(), 0.into());
                            ctx.args.insert("spotifyError".into(), error.clone().into());
                            crate::spotify::event(self, 99806, &error);
                        }
                    }
                }
                // Streamerz-only countdown operations, used by configurable templates.
                99803 => {
                    let id = self.expand(ctx, s(o, "clockId"));
                    let expected = self.expand(ctx, s(o, "expectedCurrency"));
                    let actual = self.expand(ctx, "%donationCurrency%");
                    if !expected.is_empty() && !expected.eq_ignore_ascii_case(actual.trim()) {
                        return Flow::Continue;
                    }
                    let now = crate::templates::clock::now_ms();
                    let ms =
                        |key: &str| crate::templates::clock::field_ms(o.get(key), |t| self.expand(ctx, t)).unwrap_or(0);
                    let (start, max) = (ms("startMs"), ms("maxMs"));
                    let reset = match s(o, "operation") {
                        "reset" => true,
                        "add" => false,
                        _ => {
                            crate::warn("Countdown: invalid operation".to_string());
                            return Flow::Continue;
                        }
                    };
                    let units = self.expand(ctx, s(o, "units")).parse::<f64>().unwrap_or(-1.0);
                    let per_unit = ms("perUnitMs");
                    if !reset && (!units.is_finite() || units < 0.0) {
                        crate::warn("Countdown: invalid amount".to_string());
                        return Flow::Continue;
                    }
                    let amount = ((per_unit as f64) * units).round() as u64;
                    let mut changed = false;
                    self.change(|config| {
                        use crate::templates::clock;
                        changed = if reset {
                            clock::reset(config, &id, start, max, now)
                        } else {
                            clock::set_max(config, &id, max, now) && clock::add(config, &id, amount, now)
                        };
                    });
                    if changed {
                        self.countdown_changed(&id);
                    } else {
                        crate::warn("Countdown: timer not found".to_string());
                    }
                }
                // Streamerz-only: set an overlay topic, which the pages listening show at once.
                99880 => {
                    let topic = self.expand(ctx, s(o, "topic"));
                    let text = self.expand(ctx, s(o, "value"));
                    let value = serde_json::from_str::<Value>(&text).unwrap_or(Value::String(text));
                    self.overlay_publish(topic.trim(), value);
                }
                plugins::SUB_PLUGIN => return self.plugin_sub(ctx, o),
                _ => self.unsupported(kind),
            }
            Flow::Continue
        })
    }

    /// Log once per type that a sub-action can't run here (yet), and skip it.
    fn unsupported(&self, kind: u64) {
        let _ = RUN_OUTCOME.try_with(|state| {
            state.borrow_mut().skipped += 1;
        });
        if self.st.borrow_mut().warned.insert(kind) {
            let what = name(SUB_ACTIONS, kind as u32).unwrap_or("unknown");
            let why = if kind >= 99998 {
                "C# isn't supported"
            } else {
                "not supported yet"
            };
            crate::warn(format!("sub-action {what} (type {kind}): {why}, skipped"));
        }
    }

    fn all_queues(&self) -> Vec<String> {
        let c = self.cfg.borrow();
        std::iter::once(DEFAULT_QUEUE.to_string())
            .chain(c.actions.queues.iter().map(|q| s(q, "id").to_string()))
            .collect()
    }

    /// If/Else: Equals, Not Equals, Contains (any case), Regex Match, Less Than, Greater Than, Does
    /// Not Exist, Equals / Not Equals (Ignore Case), Is Null or Empty.
    fn condition(&self, ctx: &mut Ctx, o: &Obj) -> bool {
        let raw = s(o, "input");
        let op = n(o, "operation") as i64;
        if op == 6 {
            return !ctx.args.contains_key(raw.trim_matches('%'));
        }
        let input = self.expand(ctx, raw);
        let value = self.expand(ctx, s(o, "value"));
        let auto = b(o, "autoType", false);
        match op {
            0 => compare(&input, &value, auto).is_eq(),
            1 => !compare(&input, &value, auto).is_eq(),
            2 => input.to_lowercase().contains(&value.to_lowercase()),
            3 => regex(&value).is_some_and(|r| match_args(&r, &input, &mut ctx.args)),
            4 => compare(&input, &value, auto).is_lt(),
            5 => compare(&input, &value, auto).is_gt(),
            7 => input.to_lowercase() == value.to_lowercase(),
            8 => input.to_lowercase() != value.to_lowercase(),
            9 => input.is_empty(),
            _ => false,
        }
    }

    /// While's own operators: Equals, Not Equals, Less Than, Greater Than.
    fn while_condition(&self, ctx: &Ctx, o: &Obj) -> bool {
        let mut input = self.expand(ctx, s(o, "input"));
        let mut value = self.expand(ctx, s(o, "value"));
        if b(o, "ignoreCase", false) {
            (input, value) = (input.to_lowercase(), value.to_lowercase());
        }
        let c = compare(&input, &value, b(o, "autoType", false));
        match n(o, "operation") as i64 {
            0 => c.is_eq(),
            1 => !c.is_eq(),
            2 => c.is_lt(),
            3 => c.is_gt(),
            _ => false,
        }
    }

    /// Past runs, oldest first.
    pub fn history(&self) -> std::cell::Ref<'_, VecDeque<Record>> {
        std::cell::Ref::map(self.st.borrow(), |s| &s.history)
    }

    pub fn clear_history(&self) {
        self.st.borrow_mut().history.clear();
    }

    /// What's running and waiting, per queue: (queue id, action name, running), leaving out
    /// actions set to Exclude from Pending.
    pub fn pending(&self) -> Vec<(String, String, bool)> {
        let c = self.cfg.borrow();
        let shown = |id: &str| !c.actions.actions.iter().any(|a| a.id == id && a.exclude_from_pending);
        let st = self.st.borrow();
        let mut out = Vec::new();
        for (q, x) in &st.queues {
            // ponytail: running runs only keep their name; Exclude from Pending hides waiting ones.
            out.extend(x.running.iter().map(|(_, nm)| (q.clone(), nm.clone(), true)));
            out.extend(
                x.waiting
                    .iter()
                    .filter(|r| shown(&r.action))
                    .map(|r| (q.clone(), r.name.clone(), false)),
            );
        }
        out
    }

    /// The paused queues' ids.
    pub fn paused(&self) -> Vec<String> {
        let st = self.st.borrow();
        st.queues
            .iter()
            .filter(|(_, q)| q.paused)
            .map(|(id, _)| id.clone())
            .collect()
    }

    /// Pause or resume a queue (all of them for ""), clearing its waiting runs if `clear`.
    pub fn pause(self: &Rc<Self>, queue: &str, pause: bool, clear: bool) {
        let ids = if queue.is_empty() {
            self.all_queues()
        } else {
            vec![queue.to_string()]
        };
        for q in ids {
            {
                let mut st = self.st.borrow_mut();
                let e = st.queues.entry(q.clone()).or_default();
                e.paused = pause;
                if clear {
                    e.waiting.clear();
                }
            }
            self.pump(&q);
        }
        crate::ui::changed();
    }

    /// Drop a queue's waiting runs.
    pub fn clear_queue(&self, queue: &str) {
        if let Some(q) = self.st.borrow_mut().queues.get_mut(queue) {
            q.waiting.clear();
        }
        crate::ui::changed();
    }

    /// Set a queue's Blocking (and start what it can run now).
    pub fn set_blocking(self: &Rc<Self>, queue: &str, blocking: bool) {
        self.change(|c| {
            if queue == DEFAULT_QUEUE {
                c.actions.blocking = blocking;
            } else if let Some(x) = c.actions.queues.iter_mut().find(|x| s(x, "id") == queue) {
                x.insert("blocking".into(), blocking.into());
            }
        });
        self.pump(queue);
    }
}

/// Find a sub-action by id anywhere in a tree.
fn with_sub(list: &mut [Obj], id: &str, f: &mut dyn FnMut(&mut Obj)) {
    for x in list {
        if s(x, "id") == id {
            f(x);
            return;
        }
        if let Some(children) = x.get_mut("subActions").and_then(Value::as_array_mut) {
            let mut objs: Vec<Obj> = children.iter().filter_map(|c| c.as_object().cloned()).collect();
            with_sub(&mut objs, id, f);
            *children = objs.into_iter().map(Value::Object).collect();
        }
    }
}

/// Background self-healing while saved keyring logins are locked: checks silently without popping
/// repeated password dialogs, and exits immediately once unlocked.
pub fn spawn_secret_recovery(e: Rc<Engine>) {
    tokio::task::spawn_local(async move {
        let mut step = 0usize;
        const DELAYS: [u64; 5] = [2, 4, 8, 15, 30];
        while crate::secrets::is_locked() {
            let secs = DELAYS[step.min(DELAYS.len() - 1)];
            step += 1;
            tokio::time::sleep(Duration::from_secs(secs)).await;
            if !crate::secrets::is_locked() {
                break;
            }
            if matches!(e.unlock_saved_logins(false).await, Ok(true)) {
                break;
            }
        }
    });
}

/// Shared trigger filter evaluation for injected events (`FireTrigger`, scenarios) and tests.
pub fn trigger_matches(code: u32, trigger: &Obj, args: &Obj) -> bool {
    match code {
        41002 => crate::tiktok::gifts::matches_args(trigger, args),
        112..=115 | 171 => {
            let title = match s(args, "rewardTitle") {
                "" => s(args, "rewardName"),
                t => t,
            };
            crate::catalog::matches("twitch.rewards", s(trigger, "rewardId"), s(args, "rewardId"), title)
        }
        35024 => {
            let title = match s(args, "reward.title") {
                "" => args
                    .get("reward")
                    .and_then(|r| r.get("title"))
                    .and_then(Value::as_str)
                    .unwrap_or(""),
                t => t,
            };
            crate::catalog::matches("kick.rewards", s(trigger, "rewardId"), s(args, "rewardId"), title)
        }
        18001 => {
            let want = s(trigger, "eventName");
            want.is_empty() || want == s(args, "eventName")
        }
        18002 => {
            let want = trigger
                .get("triggerName")
                .or_else(|| trigger.get("eventName"))
                .and_then(Value::as_str)
                .unwrap_or("");
            want.is_empty() || want == s(args, "triggerName") || want == s(args, "eventName")
        }
        _ => true,
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    #[test]
    fn action_results_capture_failures_and_skips_without_leaking_between_runs() {
        let _guard = crate::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        run(async {
            let e = engine(json!({"actions":[
                action("failed", "Failed", json!([{"type":1007,"url":"invalid URL","variableName":"body"}])),
                action("skipped", "Skipped", json!([{"type":99999}])),
                action("good", "Good", json!([{"type":123,"variableName":"value","value":"ok"}]))
            ]}));
            for id in ["failed", "skipped", "good"] {
                e.execute(Run {
                    id: uuid(),
                    action: id.into(),
                    name: id.into(),
                    args: Obj::new(),
                    queued_at: globals::iso_now(),
                })
                .await;
            }
            let history = e.history();
            assert_eq!(history[0].outcome.status(), "failed");
            assert_eq!(history[0].outcome.failed_step, Some(1007));
            assert!(history[0].outcome.error.is_some());
            assert_eq!(history[1].outcome.status(), "completed_with_skips");
            assert_eq!(history[1].outcome.skipped, 1);
            assert_eq!(history[2].outcome.status(), "completed");
            assert_eq!(history[2].outcome.errors, 0);
            assert_eq!(history[2].after["value"], "ok");
        });
    }

    #[test]
    fn duration_inputs_expand_event_values_and_keep_wire_units() {
        run(async {
            let engine = engine(json!({"actions":[]}));
            let ctx = Ctx {
                group: String::new(),
                args: json!({"seconds":30,"milliseconds":1500}).as_object().unwrap().clone(),
            };
            let object = json!({"duration":"%seconds%","timeout":"%milliseconds%","literal":12,"bad":"NaN"})
                .as_object()
                .unwrap()
                .clone();
            assert_eq!(engine.duration_number(&ctx, &object, "duration"), 30.0);
            assert_eq!(engine.duration_number(&ctx, &object, "timeout"), 1500.0);
            assert_eq!(engine.duration_number(&ctx, &object, "literal"), 12.0);
            assert_eq!(engine.duration_number(&ctx, &object, "bad"), 0.0);
        });
    }

    #[test]
    fn trigger_matches_filters_gifts_rewards_and_custom_events() {
        let obj = |v: Value| v.as_object().unwrap().clone();
        let gift = obj(json!({"type": 41002, "giftId": "5655", "giftDelivery": 1, "minCoins": 10, "maxCoins": 50}));
        assert!(!trigger_matches(
            41002,
            &gift,
            &obj(json!({"giftId": "1", "diamondCount": 20, "repeatCount": 1, "giftType": 0}))
        ));
        assert!(!trigger_matches(
            41002,
            &gift,
            &obj(json!({"giftId": "5655", "diamondCount": 10, "repeatCount": 2, "giftType": 1, "repeatEnd": false}))
        ));
        assert!(trigger_matches(
            41002,
            &gift,
            &obj(json!({"giftId": "5655", "diamondCount": 10, "repeatCount": 2, "giftType": 1, "repeatEnd": true}))
        ));
        assert!(!trigger_matches(
            41002,
            &gift,
            &obj(json!({"giftId": "5655", "diamondCount": 30, "repeatCount": 2, "giftType": 0}))
        ));

        let reward = obj(json!({"type": 112, "rewardId": "rw-1"}));
        assert!(trigger_matches(112, &reward, &obj(json!({"rewardId": "rw-1"}))));
        assert!(!trigger_matches(112, &reward, &obj(json!({"rewardId": "rw-2"}))));

        let custom = obj(json!({"type": 18001, "eventName": "goal"}));
        assert!(trigger_matches(18001, &custom, &obj(json!({"eventName": "goal"}))));
        assert!(!trigger_matches(18001, &custom, &obj(json!({"eventName": "other"}))));
    }

    #[test]
    fn group_variables_are_shared_persistent_and_isolated() {
        let _g = crate::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        run(async {
            let e = engine(json!({"actions": []}));
            let mut first = Ctx {
                args: Obj::new(),
                group: "one".into(),
            };
            let operation = json!({"type": 123, "variableName": "group.count", "value": "12", "autoType": true});
            e.list(&mut first, &[operation], false).await;
            let second = Ctx {
                args: Obj::new(),
                group: "one".into(),
            };
            let other = Ctx {
                args: Obj::new(),
                group: "two".into(),
            };
            assert_eq!(e.expand(&second, "%group.count%"), "12");
            assert_eq!(e.expand(&other, "%group.count%"), "%group.count%");
            let saved = serde_json::to_value(&*e.config()).unwrap();
            let restored: crate::Config = serde_json::from_value(saved).unwrap();
            assert_eq!(restored.action_group_variables["one"]["count"], 12);
        });
    }
    use crate::model::ActionsFile;

    pub(crate) fn engine(actions: Value) -> Rc<Engine> {
        let c = Config {
            actions: serde_json::from_value::<ActionsFile>(actions).unwrap(),
            ..Default::default()
        };
        Engine::new(Arc::new(watch::channel(c).0), |_| {})
    }

    pub(crate) fn action(id: &str, name: &str, subs: Value) -> Value {
        json!({"id": id, "name": name, "enabled": true, "queue": DEFAULT_QUEUE, "subActions": subs, "triggers": []})
    }

    pub(crate) fn log(msg: &str) -> Value {
        json!({"type": 1020, "logLevel": 1, "message": msg})
    }

    /// Run a test body on a LocalSet, then return the log lines that start with "T ".
    pub(crate) fn run(f: impl std::future::Future<Output = ()>) -> Vec<String> {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        crate::capture(true);
        tokio::task::LocalSet::new().block_on(&rt, f);
        crate::capture(false)
            .into_iter()
            .filter(|line| line.starts_with("T "))
            .collect()
    }

    #[test]
    fn a_tests_log_survives_shared_history_being_cleared() {
        let _guard = crate::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let out = run(async {
            crate::say("T this test".into());
            crate::status(|status| status.log.clear());
        });
        assert_eq!(out, ["T this test"]);
    }

    pub(crate) async fn settle() {
        tokio::time::sleep(Duration::from_millis(50)).await;
    }

    #[test]
    fn countdown_action_checks_currency_and_adds_fractional_units() {
        let _g = crate::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        run(async {
            let e = engine(json!({"actions": []}));
            let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
            let mut subscriptions = events::Subs::new();
            events::update(
                &mut subscriptions,
                &json!({"Misc": ["StreamerzCountdownChanged"]}),
                true,
            );
            e.listen(1, &subscriptions, &tx);
            let now = crate::templates::clock::now_ms();
            e.change(|c| {
                crate::templates::clock::create(c, "clock", 10_000, 30_000, now);
            });
            let mut ctx = Ctx {
                group: String::new(),
                args: json!({"donationCurrency": "eur", "donationAmount": 2.5})
                    .as_object()
                    .unwrap()
                    .clone(),
            };
            let mut operation = json!({"type": 99803, "clockId": "clock", "operation": "add", "maxMs": 30000,
                "perUnitMs": 1000, "units": "%donationAmount%", "expectedCurrency": "USD"})
            .as_object()
            .unwrap()
            .clone();
            e.one(&mut ctx, &operation).await;
            let before = crate::templates::clock::get(&e.config(), "clock", now)
                .unwrap()
                .remaining_ms;
            assert_eq!(before, 10_000);
            assert!(rx.try_recv().is_err());
            operation.insert("expectedCurrency".into(), "EUR".into());
            e.one(&mut ctx, &operation).await;
            let after = crate::templates::clock::get(&e.config(), "clock", now)
                .unwrap()
                .remaining_ms;
            assert!((12_490..=12_500).contains(&after));
            let event: Value = serde_json::from_str(&rx.try_recv().unwrap()).unwrap();
            assert_eq!(event["event"]["type"], "StreamerzCountdownChanged");
            assert_eq!(event["data"]["clockId"], "clock");
            assert_eq!(event["data"]["snapshot"]["maxMs"], 30_000);
            operation.insert("operation".into(), "reset".into());
            operation.insert("expectedCurrency".into(), "".into());
            operation.insert("startMs".into(), 5_000.into());
            e.one(&mut ctx, &operation).await;
            assert!(
                crate::templates::clock::get(&e.config(), "clock", crate::templates::clock::now_ms())
                    .unwrap()
                    .remaining_ms
                    <= 5_000
            );
        });
    }

    #[test]
    fn spotify_connection_events_reach_actions_and_listeners() {
        let _g = crate::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        run(async {
            let e = engine(json!({"actions": []}));
            let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
            let mut subscriptions = events::Subs::new();
            events::update(&mut subscriptions, &json!({"Spotify": ["Connected", "Error"]}), true);
            e.listen(1, &subscriptions, &tx);
            crate::spotify::event(&e, 99804, "");
            let connected: Value = serde_json::from_str(&rx.try_recv().unwrap()).unwrap();
            assert_eq!(connected["event"]["type"], "Connected");
            crate::spotify::event(&e, 99806, "request failed");
            let error: Value = serde_json::from_str(&rx.try_recv().unwrap()).unwrap();
            assert_eq!(error["data"]["spotifyError"], "request failed");
        });
    }

    #[test]
    fn spotify_action_reports_disabled_and_clears_stale_results() {
        let _g = crate::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        run(async {
            let e = engine(json!({"actions": []}));
            let mut ctx = Ctx {
                group: String::new(),
                args: json!({"rawInput": "a song", "spotifyQueued": "true", "spotifyUri": "old"})
                    .as_object()
                    .unwrap()
                    .clone(),
            };
            let operation = json!({"type": 99802, "query": "%rawInput%"})
                .as_object()
                .unwrap()
                .clone();
            assert_eq!(e.one(&mut ctx, &operation).await, Flow::Continue);
            assert_eq!(ctx.args["spotifyQueued"], "false");
            assert_eq!(ctx.args["spotifyUri"], "");
            assert!(!ctx.args["spotifyError"].as_str().unwrap().is_empty());
        });
    }

    #[test]
    fn logic_and_flow() {
        let _g = crate::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let ifelse = |op: i64, input: &str, value: &str, yes: &str, no: &str| {
            json!({"type": 120, "input": input, "operation": op, "value": value, "autoType": true, "subActions": [
                {"type": 99901, "random": false, "subActions": [log(yes)]},
                {"type": 99902, "random": false, "subActions": [log(no)]}]})
        };
        let main = action(
            "a1",
            "main",
            json!([
                {"type": 123, "variableName": "n", "value": "15", "autoType": true},
                {"type": 123, "variableName": "s", "value": "Hello World", "autoType": false},
                ifelse(4, "%n%", "2", "T lt yes", "T lt no"),
                ifelse(2, "%s%", "world", "T contains yes", "T contains no"),
                {"type": 120, "input": "%s%", "operation": 3, "value": "(?<first>\\w+) (\\w+)", "autoType": true, "subActions": [
                    {"type": 99901, "random": false, "subActions": [log("T re %match.count%|%match[0]%|%match[1]%|%match[2]%|%first%")]},
                    {"type": 99902, "random": false, "subActions": []}]},
                ifelse(6, "nothere", "", "T dne yes", "T dne no"),
                {"type": 123, "variableName": "i", "value": "0", "autoType": true},
                {"type": 128, "input": "%i%", "operation": 2, "value": "5", "autoType": true, "subActions": [
                    {"type": 123, "variableName": "i", "value": "$math(%i% + 1)$", "autoType": true},
                    {"type": 120, "input": "%i%", "operation": 0, "value": "2", "autoType": true, "subActions": [
                        {"type": 99901, "subActions": [{"type": 130}]}, {"type": 99902, "subActions": []}]},
                    {"type": 120, "input": "%i%", "operation": 0, "value": "4", "autoType": true, "subActions": [
                        {"type": 99901, "subActions": [{"type": 129}]}, {"type": 99902, "subActions": []}]},
                    log("T loop %i%")]},
                {"type": 127, "input": "%i%", "autoType": true, "subActions": [
                    {"type": 99903, "values": ["1", "4"], "caseSensitive": false, "subActions": [log("T case 1or4")]},
                    {"type": 99904, "subActions": [log("T default")]}]},
                {"type": 4, "actionId": "a2", "runImmedately": true},
                log("T parent sees %c% (not reached: the child breaks)")
            ]),
        );
        let child = action(
            "a2",
            "child",
            json!([
                log("T child sees %n%"),
                {"type": 123, "variableName": "c", "value": "fromchild"},
                {"type": 124},
                log("T after break")
            ]),
        );
        let e = engine(json!({"actions": [main, child]}));
        let out = run(async move {
            e.queue("main", Obj::new()).unwrap();
            settle().await;
            assert_eq!(e.history().len(), 1);
        });
        assert_eq!(
            out,
            [
                "T lt no",
                "T contains yes",
                "T re 3|Hello World|World|Hello|Hello",
                "T dne yes",
                "T loop 1",
                "T loop 3",
                "T case 1or4",
                "T child sees 15"
            ]
        );
    }

    #[test]
    fn pause_clear_pending_history() {
        let _g = crate::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let mut hidden = action("h", "hidden", json!([log("T hidden")]));
        hidden["excludeFromHistory"] = json!(true);
        let quick = action("k", "quick", json!([log("T quick")]));
        let e = engine(json!({"blocking": true, "actions": [hidden, quick]}));
        let out = run(async move {
            let waiting = |name: &str| vec![(DEFAULT_QUEUE.to_string(), name.to_string(), false)];
            e.pause(DEFAULT_QUEUE, true, false);
            e.queue("k", Obj::new());
            assert_eq!(e.pending(), waiting("quick"));
            assert_eq!(e.paused(), [DEFAULT_QUEUE]);
            e.clear_queue(DEFAULT_QUEUE);
            assert!(e.pending().is_empty());
            e.queue("h", Obj::new());
            e.queue("k", Obj::new());
            e.pause("", false, false);
            tokio::time::sleep(Duration::from_millis(50)).await;
            assert!(e.pending().is_empty() && e.paused().is_empty());
            let h = e.history();
            assert_eq!(h.iter().map(|r| r.name.as_str()).collect::<Vec<_>>(), ["quick"]);
            assert_eq!(h[0].after["actionName"], "quick");
        });
        assert_eq!(out, ["T hidden", "T quick"]);
    }

    #[test]
    fn queues_globals_signals_triggers() {
        let _g = crate::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let q = "11111111-0000-0000-0000-000000000000";
        let slow = json!({"id": "q1", "name": "slow", "enabled": true, "queue": q, "triggers": [], "subActions": [
            log("T start %n%"), {"type": 1002, "value": 30}, log("T end %n%")]});
        let driver = action(
            "d",
            "driver",
            json!([
                {"type": 123, "variableName": "n", "value": "a"},
                {"type": 4, "actionId": "q1", "runImmedately": false},
                {"type": 123, "variableName": "n", "value": "b"},
                {"type": 4, "actionId": "q1", "runImmedately": false},
                {"type": 122, "destination": 0, "variableName": "g", "persisted": true, "source": 0, "value": "5", "autoType": true},
                {"type": 122, "destination": 0, "variableName": "g", "persisted": true, "source": 1, "value": "3", "autoType": true},
                {"type": 122, "destination": 0, "variableName": "g", "persisted": true, "source": 2, "value": "x", "autoType": true},
                {"type": 121, "source": 0, "variableName": "g", "persisted": true, "destinationVariable": "gv"},
                {"type": 121, "source": 0, "variableName": "missing", "persisted": false, "destinationVariable": "m", "defaultValue": "dflt"},
                log("T globals %gv% ~g~ %m%"),
                {"type": 1031, "signalName": "sig", "useArge": false, "customArgs": {"k": "v"}, "queueSignal": true},
                {"type": 1034, "eventName": "ev", "useArgs": true},
                {"type": 1004, "actionId": "q1", "state": 2},
                {"type": 1016, "actionId": "q1"},
                log("T state %actionState%")
            ]),
        );
        let waiter = action(
            "w",
            "waiter",
            json!([
                {"type": 1032, "signalName": "sig", "overwrite": false, "timeout": 200},
                log("T got %k% %signalTimeout%"),
                {"type": 1032, "signalName": "none", "timeout": 20},
                log("T timeout %signalTimeout%")
            ]),
        );
        let mut custom = action("c", "custom", json!([log("T custom %n% %triggerName% %__source%")]));
        custom["triggers"] = json!([{"eventName": "ev", "id": "t1", "type": 18001, "enabled": true, "exclusions": []}]);
        let mut created = action("gc", "created", json!([log("T created %global.name%=%global.value%")]));
        created["triggers"] = json!([{"variableName": "", "persisted": 0, "id": "t2", "type": 711, "enabled": true}]);
        let e = engine(
            json!({"blocking": false, "queues": [{"id": q, "name": "Q", "blocking": true}],
            "actions": [slow, driver, waiter, custom, created]}),
        );
        let e2 = e.clone();
        let out = run(async move {
            e.queue("d", Obj::new()).unwrap();
            settle().await;
            e.queue("w", Obj::new()).unwrap();
            tokio::time::sleep(Duration::from_millis(200)).await;
            assert_eq!(e.cfg.borrow().globals["g"], json!(8));
        });
        let _ = e2;
        let pos = |s: &str| {
            out.iter()
                .position(|x| x == s)
                .unwrap_or_else(|| panic!("{s} missing: {out:?}"))
        };
        // Blocking queue: one after the other.
        assert!(pos("T start a") < pos("T end a") && pos("T end a") < pos("T start b"));
        assert!(out.contains(&"T globals 8 8 dflt".to_string()));
        assert!(out.contains(&"T created g=5".to_string()) && out.contains(&"T created missing=dflt".to_string()));
        assert!(out.contains(&"T custom b Custom CustomEvent".to_string()));
        assert!(out.contains(&"T state False".to_string()));
        assert!(out.contains(&"T got v False".to_string()));
        assert!(out.contains(&"T timeout True".to_string()));
    }

    #[test]
    fn remote_instances_triggers_signals_and_wait() {
        let _g = crate::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let act = action("r1", "remote_action", json!([log("T remote_trig %eventName% %val%")]));
        let mut act_with_trig = act.clone();
        act_with_trig["triggers"] = json!([{
            "type": crate::remote::T_REMOTE_TRIGGER,
            "id": "rt1",
            "enabled": true,
            "eventName": "hello_remote",
            "instanceId": ""
        }]);
        let e = engine(json!({"blocking": false, "queues": [], "actions": [act_with_trig]}));

        let out = run(async {
            let mut args = Obj::new();
            args.insert("val".into(), "42".into());
            e.fire_remote_trigger("inst-1", "hello_remote", args);
            settle().await;

            let e2 = e.clone();
            tokio::task::spawn_local(async move {
                tokio::time::sleep(Duration::from_millis(10)).await;
                let mut sig_args = Obj::new();
                sig_args.insert("sig_data".into(), "remote_ok".into());
                e2.deliver_remote_signal("inst-2", "sig1", sig_args);
            });

            let res = e.wait_remote_signal("inst-2", "sig1", 500).await;
            assert!(res.is_some());
            let res = res.unwrap();
            assert_eq!(res.get("sig_data").and_then(Value::as_str), Some("remote_ok"));
        });
        assert!(out.contains(&"T remote_trig hello_remote 42".to_string()));
    }

    #[test]
    fn inputs_hotkeys_and_midi() {
        let _g = crate::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());

        let a_midi = json!({
            "id": "a_midi",
            "name": "MIDI Action",
            "enabled": true,
            "group": "",
            "queue": crate::engine::DEFAULT_QUEUE,
            "triggers": [{
                "id": "t_midi",
                "type": crate::inputs::T_MIDI_MESSAGE,
                "enabled": true,
                "channel": 1,
                "commandCode": "NoteOn",
                "device": ""
            }],
            "subActions": [
                log("T midi %deviceName% %channel% %noteName% %velocity%"),
                // MIDI subactions and keyboard press
                json!({"type": crate::inputs::S_MIDI_NOTE_ON, "device": "Launchpad", "name": "L1", "channel": 1, "note": 60, "velocity": 127, "sendNoteOff": true, "duration": 100}),
                json!({"type": crate::inputs::S_MIDI_CONTROL_CHANGE, "device": "Launchpad", "name": "L1", "channel": 1, "controller": 7, "value": 100}),
                json!({"type": crate::inputs::S_MIDI_GENERIC, "device": "Launchpad", "name": "L1", "command": "PitchBend", "channel": 1, "data1": 0, "data2": 64}),
                json!({"type": crate::inputs::S_KEYBOARD_PRESS, "key": "F13", "modifiers": 2})
            ]
        });

        let a_input_key = json!({
            "id": "a_key",
            "name": "Key Action",
            "enabled": true,
            "group": "",
            "queue": crate::engine::DEFAULT_QUEUE,
            "triggers": [{
                "id": "t_key",
                "type": crate::hotkeys::T_KEY_PRESS,
                "enabled": true,
                "keyCode": -4095, // F14
                "modifierMask": 3 // Shift + Ctrl
            }],
            "subActions": [log("T key %key% %modifier% %hasCtrl% %hasAlt% %hasShift%")]
        });

        let a_input_mouse = json!({
            "id": "a_mouse",
            "name": "Mouse Action",
            "enabled": true,
            "group": "",
            "queue": crate::engine::DEFAULT_QUEUE,
            "triggers": [{
                "id": "t_mouse",
                "type": crate::hotkeys::T_MOUSE_CLICK,
                "enabled": true
            }],
            "subActions": [log("T mouse %mouse.button% %mouse.clicks% %mouse.x% %mouse.y%")]
        });

        let e = engine(json!({
            "blocking": false,
            "queues": [],
            "actions": [a_midi, a_input_key, a_input_mouse]
        }));

        let out = run(async {
            // MIDI note on (0x90 = NoteOn, channel 1)
            e.midi_fire(
                "Launchpad",
                "Launchpad",
                &crate::midi::Msg {
                    status: 0x90,
                    data1: 60,
                    data2: 100,
                },
                0,
            );
            // Fire key press
            // Right-hand Shift counts as Shift; another combination fires nothing.
            e.fire_hotkey(-4095, 2 | 16);
            e.fire_hotkey(-4095, 2);
            // Fire mouse click
            e.fire_mouse(2, 1, 300, 450);

            settle().await;
        });

        assert!(out.contains(&"T midi Launchpad 1 C5 100".to_string()), "{out:?}");
        assert_eq!(out.iter().filter(|l| l.starts_with("T key")).count(), 1, "{out:?}");
        assert!(out.contains(&"T key -4095 18 True False True".to_string()), "{out:?}");
        assert!(out.contains(&"T mouse 2 1 300 450".to_string()), "{out:?}");
    }

    #[test]
    fn decks_stream_deck_and_chat_window_actions() {
        let _g = crate::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());

        let a_chat = json!({
            "id": "a_chat",
            "name": "Chat Action",
            "enabled": true,
            "group": "",
            "queue": crate::engine::DEFAULT_QUEUE,
            "triggers": [{
                "id": "t_chat",
                "type": crate::decks::T_CHAT_WINDOW_ACTION,
                "enabled": true,
                "actionName": "shoutout"
            }],
            "subActions": [log("T chat %platform% %user% %actionName%")]
        });

        let a_sd = json!({
            "id": "a_sd",
            "name": "Stream Deck Action",
            "enabled": true,
            "group": "",
            "queue": crate::engine::DEFAULT_QUEUE,
            "triggers": [],
            "subActions": [
                log("T sd ran"),
                json!({"type": crate::decks::S_STREAM_DECK_SET_TITLE, "buttonId": "btn_1", "title": "Active", "state": 0}),
                json!({"type": crate::decks::S_STREAM_DECK_SET_BACKGROUND, "buttonId": "btn_1", "image": "bg.png", "color": "#ff0000", "state": 0}),
                json!({"type": crate::decks::S_STREAM_DECK_SET_STATE, "buttonId": "btn_1", "state": 0}),
                json!({"type": crate::decks::S_STREAM_DECK_SHOW_ALERT, "buttonId": "btn_1"}),
                json!({"type": crate::decks::S_STREAM_DECK_SHOW_OK, "buttonId": "btn_1"}),
                json!({"type": crate::decks::S_STREAM_DECK_TOGGLE_STATE, "buttonId": "btn_1"}),
                json!({"type": crate::decks::S_STREAM_DECK_SET_VALUE, "buttonId": "btn_1", "value": "100"})
            ]
        });

        let e = engine(json!({
            "blocking": false,
            "queues": [],
            "actions": [a_chat, a_sd]
        }));

        let out = run(async {
            e.fire_chat_window_action("twitch", "alice", "123", "hello", "m1", "shoutout");
            // The Stream Deck sub-actions push to the plugins (none here).
            e.queue("a_sd", Obj::new());

            settle().await;
        });

        assert!(out.contains(&"T chat twitch alice shoutout".to_string()));
        assert!(out.contains(&"T sd ran".to_string()));
    }

    #[test]
    fn file_io_file_tail_and_process_triggers() {
        let _g = crate::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());

        let dir = std::env::temp_dir().join(format!("streamerz_engine_file_test_{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let test_file = dir.join("test_io.txt");
        let test_file_str = test_file.to_string_lossy().to_string();
        let dir_str = dir.to_string_lossy().to_string();

        let a_file_sub = json!({
            "id": "a_fsub",
            "name": "File Subactions",
            "enabled": true,
            "group": "",
            "queue": crate::engine::DEFAULT_QUEUE,
            "triggers": [],
            "subActions": [
                // Write file
                json!({"type": crate::fileio::S_WRITE_TO_FILE, "file": test_file_str, "text": "hello
world
rust
", "append": false}),
                // File Exists
                json!({"type": crate::fileio::S_FILE_EXISTS, "file": test_file_str, "variableName": "f_exists"}),
                // Folder Exists
                json!({"type": crate::fileio::S_FOLDER_EXISTS, "folder": dir_str, "variableName": "d_exists"}),
                // Read Specific Line
                json!({"type": crate::fileio::S_READ_SPECIFIC_LINE, "file": test_file_str, "count": 2, "variableName": "line_two", "autoType": false, "parseVariables": false}),
                // Read Lines
                json!({"type": crate::fileio::S_READ_LINES, "file": test_file_str, "variableName": "all_lines", "autoType": false, "parseVariables": false}),
                // Read Random Line
                json!({"type": crate::fileio::S_READ_RANDOM_LINE, "file": test_file_str, "variableName": "rand_line", "autoType": false, "parseVariables": false}),
                log("T file f_exists=%f_exists% d_exists=%d_exists% l2=%line_two% count=%all_lines.count%")
            ]
        });

        let a_proc_start = json!({
            "id": "a_pstart",
            "name": "Process Start Action",
            "enabled": true,
            "group": "",
            "queue": crate::engine::DEFAULT_QUEUE,
            "triggers": [{
                "id": "t_pstart",
                "type": crate::fileio::T_PROCESS_STARTED,
                "enabled": true,
                "processName": "obs64.exe"
            }],
            "subActions": [log("T pstart %name% %executable% %processId% %path%")]
        });

        let a_proc_stop = json!({
            "id": "a_pstop",
            "name": "Process Stop Action",
            "enabled": true,
            "group": "",
            "queue": crate::engine::DEFAULT_QUEUE,
            "triggers": [{
                "id": "t_pstop",
                "type": crate::fileio::T_PROCESS_STOPPED,
                "enabled": true,
                "processName": "obs64.exe"
            }],
            "subActions": [log("T pstop %executable% %parentProcessId%")]
        });

        let e = engine(json!({
            "blocking": false,
            "queues": [],
            "actions": [a_file_sub, a_proc_start, a_proc_stop]
        }));

        let out = run(async {
            e.queue("a_fsub", Obj::new()).unwrap();
            settle().await;

            let obs = processes::Proc {
                name: "obs64".into(),
                executable: "obs64.exe".into(),
                full_path: "C:/obs/obs64.exe".into(),
                parent: 1,
            };
            e.process_fire(processes::T_STARTED, 4321, &obs);
            e.process_fire(processes::T_STOPPED, 4321, &obs);

            settle().await;
        });

        assert!(out.contains(&"T file f_exists=True d_exists=True l2=world count=3".to_string()));
        assert!(out.contains(&"T pstart obs64 obs64.exe 4321 C:/obs".to_string()));
        assert!(out.contains(&"T pstop obs64.exe 1".to_string()));

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn uuids() {
        let u = uuid();
        assert_eq!(u.len(), 36);
        assert_eq!(&u[14..15], "4");
        assert_ne!(uuid(), u);
    }
}
