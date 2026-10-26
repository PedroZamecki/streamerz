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
use std::future::Future;
use std::pin::Pin;
use std::rc::Rc;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{oneshot, watch};

pub mod builtins;
pub mod chatlog;
pub mod community;
pub mod events;
pub mod globals;
mod rest;

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
    pub action_id: String,
    pub name: String,
    /// The arguments it started with, and those it ended with.
    pub args: Obj,
    pub after: Obj,
    pub queued_at: String,
    pub ms: f64,
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
    voice_commands: HashMap<String, (String, bool)>,
    voice_input: (String, String),
    decks: HashMap<String, crate::decks::DeckState>,
    stream_deck_items: HashMap<String, crate::decks::DeckItem>,
    file_tails: HashMap<String, bool>,
    /// Dev mode (`STREAMERZ_DEV=1`): chat lines kept here instead of sent, for `GetChatOut`.
    dev_out: Option<Vec<Value>>,
    /// Every platform's chat, for the chat window.
    chat_log: chatlog::Log,
}

pub struct Engine {
    cfg: Arc<watch::Sender<Config>>,
    /// Writes the config (a no-op in tests).
    save: fn(&Config),
    st: RefCell<State>,
    /// WebSocket connections listening to events.
    hub: RefCell<Vec<events::Listener>>,
    twitch: RefCell<Option<crate::twitch::TwitchHandle>>,
    obs: RefCell<Option<crate::obs::ObsHandle>>,
    kick: RefCell<Option<crate::kick::KickHandle>>,
    youtube: RefCell<Option<crate::youtube::YouTubeHandle>>,
    tiktok: RefCell<Option<crate::tiktok::TikTokHandle>>,
    pub(crate) slobs: crate::slobs::Slobs,
    pub(crate) donations: RefCell<crate::donations::Live>,
    pub(crate) hotkeys: RefCell<crate::hotkeys::Live>,
    pub(crate) live_wake: tokio::sync::Notify,
    streaming_since: std::cell::Cell<Option<std::time::Instant>>,
}

/// A running action's context.
struct Ctx {
    args: Obj,
}

type Fut<'a, T> = Pin<Box<dyn Future<Output = T> + 'a>>;

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

/// Whether the engine runs this sub-action type (the rest are kept, logged once and skipped).
pub fn supported(kind: u32) -> bool {
    matches!(
        kind,
        2 | 4 | 6 | 8 | 9 | 10..=29 | 30..=49 | 50..=52 | 120..=130 | 200..=202 | 204..=208 | 213..=215 | 251..=254 | 321..=329 | 401..=415 | 417 | 500..=513 | 550..=573 | 575..=580
            | 3 | 5 | 1001..=1007 | 1009..=1023 | 1025 | 1028 | 1030..=1034 | 1038..=1040 | 4001..=4012 | 7001 | 8001 | 8002 | 9001..=9003 | 10001 | 11001 | 11002 | 11004..=11008 | 15001 | 20001..=20003 | 21001..=21010
            | 35001 | 35002 | 35010..=35012 | 35020..=35022 | 35030 | 35031
            | 35040..=35043 | 35101..=35103 | 35105..=35108 | 35113 | 43001..=43003 | 99900..=99904
            | 1000 | 1026 | 1027 | 1035..=1037
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
            crate::say(format!("regex {p}: {e}"));
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
    pub fn new(cfg: Arc<watch::Sender<Config>>, save: fn(&Config)) -> Rc<Self> {
        Rc::new(Engine {
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
            slobs: Default::default(),
            donations: RefCell::default(),
            hotkeys: RefCell::default(),
            live_wake: tokio::sync::Notify::new(),
            streaming_since: std::cell::Cell::new(None),
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
        let (tw, obs_on, yt, kick, tt) = {
            let c = self.cfg.borrow();
            (
                c.twitch.auto_connect && c.twitch.broadcaster.is_some(),
                c.obs
                    .connections
                    .iter()
                    .any(|o| o.get("autoConnect").and_then(Value::as_bool).unwrap_or(false)),
                c.youtube.auto_connect
                    && (!c.youtube.video_id.trim().is_empty() || !c.youtube.channel_id.trim().is_empty()),
                c.kick.auto_connect && !c.kick.channel.trim().is_empty(),
                c.tiktok.auto_connect,
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

    pub fn set_obs(&self, handle: crate::obs::ObsHandle) {
        *self.obs.borrow_mut() = Some(handle);
    }

    pub fn obs(&self) -> Option<crate::obs::ObsHandle> {
        self.obs.borrow().clone()
    }

    pub fn obs_start(self: &Rc<Self>) -> crate::obs::ObsHandle {
        if let Some(o) = self.obs() {
            return o;
        }
        let o = crate::obs::start(self.clone(), &self.cfg);
        self.set_obs(o.clone());
        o
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
                crate::say(e);
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

    pub fn set_voice_command(self: &Rc<Self>, id: &str, phrase: &str) {
        let mut st = self.st.borrow_mut();
        let entry = st
            .voice_commands
            .entry(id.to_string())
            .or_insert_with(|| (phrase.to_string(), true));
        entry.0 = phrase.to_string();
    }

    pub fn set_voice_command_state(self: &Rc<Self>, id: &str, state: f64) {
        let mut st = self.st.borrow_mut();
        let entry = st
            .voice_commands
            .entry(id.to_string())
            .or_insert_with(|| (String::new(), true));
        entry.1 = new_state(state, entry.1);
    }

    pub fn set_voice_input(self: &Rc<Self>, device_id: &str, device_name: &str) {
        self.st.borrow_mut().voice_input = (device_id.to_string(), device_name.to_string());
    }

    pub fn fire_speech_dictation(self: &Rc<Self>, text: &str, confidence: f64, culture: &str) {
        let mut args = Obj::new();
        args.insert("dictationText".into(), text.into());
        args.insert("text".into(), text.into());
        args.insert("confidence".into(), confidence.into());
        args.insert("culture".into(), culture.into());
        let culture_str = culture.to_string();
        self.fire(
            crate::inputs::T_SPEECH_DICTATION,
            "Speech to Text Dictation",
            "Core/Voice Control",
            "speechtotext",
            &args,
            |t| {
                let want = s(t, "culture");
                want.is_empty() || want == culture_str
            },
        );
        self.emit("SpeechToText", "Dictation", || {
            json!({
                "text": text,
                "confidence": confidence,
                "culture": culture
            })
        });
    }

    pub fn fire_speech_command(
        self: &Rc<Self>,
        command_id: &str,
        command_name: &str,
        phrase: &str,
        confidence: f64,
        culture: &str,
    ) {
        let mut args = Obj::new();
        args.insert("commandId".into(), command_id.into());
        args.insert("commandName".into(), command_name.into());
        args.insert("command".into(), phrase.into());
        args.insert("phrase".into(), phrase.into());
        args.insert("confidence".into(), confidence.into());
        args.insert("culture".into(), culture.into());
        let id_str = command_id.to_string();
        self.fire(
            crate::inputs::T_SPEECH_COMMAND,
            "Speech to Text Command",
            "Core/Voice Control",
            "speechtotext",
            &args,
            |t| {
                let want = s(t, "commandId");
                want.is_empty() || want == id_str
            },
        );
        self.emit("SpeechToText", "Command", || {
            json!({
                "commandId": command_id,
                "commandName": command_name,
                "command": phrase,
                "confidence": confidence,
                "culture": culture
            })
        });
    }

    pub fn fire_midi_message(self: &Rc<Self>, device: &str, channel: u8, status: u8, data1: u8, data2: u8) {
        let mut args = Obj::new();
        args.insert("midiDevice".into(), device.into());
        args.insert("channel".into(), (channel as u64).into());
        args.insert("status".into(), (status as u64).into());
        let type_name = crate::inputs::midi_type_name(status);
        args.insert("type".into(), type_name.into());
        args.insert("eventType".into(), type_name.into());
        args.insert("data1".into(), (data1 as u64).into());
        args.insert("data2".into(), (data2 as u64).into());
        if type_name == "NoteOn" || type_name == "NoteOff" {
            args.insert("note".into(), (data1 as u64).into());
            args.insert("noteName".into(), crate::inputs::note_name(data1).into());
            args.insert("velocity".into(), (data2 as u64).into());
        } else if type_name == "ControlChange" {
            args.insert("controller".into(), (data1 as u64).into());
            args.insert("value".into(), (data2 as u64).into());
        }
        let dev_str = device.to_string();
        let type_str = type_name.to_string();
        self.fire(
            crate::inputs::T_MIDI_MESSAGE,
            "Midi Message",
            "Core/MIDI",
            "midi",
            &args,
            |t| {
                let want_dev = s(t, "device");
                let want_ch = n(t, "channel") as u8;
                let want_status = s(t, "status");
                (want_dev.is_empty() || want_dev == dev_str)
                    && (want_ch == 0 || want_ch == channel)
                    && (want_status.is_empty() || want_status == type_str)
            },
        );
        self.emit("Midi", "Message", || {
            json!({
                "device": device,
                "channel": channel,
                "status": status,
                "data1": data1,
                "data2": data2,
                "type": type_name
            })
        });
    }

    pub fn fire_file_watcher(self: &Rc<Self>, kind: u32, watcher_id: &str, path: &str, old_path: &str) {
        let (name, event_type) = match kind {
            crate::fileio::T_FILE_WATCHER_CREATED => ("File Watcher Created", "Created"),
            crate::fileio::T_FILE_WATCHER_DELETED => ("File Watcher Deleted", "Deleted"),
            crate::fileio::T_FILE_WATCHER_RENAMED => ("File Watcher Renamed", "Renamed"),
            _ => ("File Watcher Changed", "Changed"),
        };
        let mut args = Obj::new();
        args.insert("watcherId".into(), watcher_id.into());
        args.insert("path".into(), path.into());
        args.insert("fullPath".into(), path.into());
        let file_name = std::path::Path::new(path)
            .file_name()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_default();
        args.insert("fileName".into(), file_name.into());
        if !old_path.is_empty() {
            args.insert("oldPath".into(), old_path.into());
        }
        let w_str = watcher_id.to_string();
        let p_str = path.to_string();
        self.fire(kind, name, "FileWatcher", "filewatcher", &args, |t| {
            let want_w = s(t, "watcherId");
            let want_p = s(t, "path");
            (want_w.is_empty() || want_w == w_str) && (want_p.is_empty() || want_p == p_str)
        });
        self.emit("FileWatcher", event_type, || {
            json!({
                "watcherId": watcher_id,
                "path": path,
                "oldPath": old_path
            })
        });
    }

    pub fn fire_file_tail(self: &Rc<Self>, tail_id: &str, path: &str, line: &str, line_number: u64) {
        let mut args = Obj::new();
        args.insert("tailId".into(), tail_id.into());
        args.insert("path".into(), path.into());
        args.insert("line".into(), line.into());
        args.insert("lineNumber".into(), line_number.into());
        let t_str = tail_id.to_string();
        let p_str = path.to_string();
        self.fire(
            crate::fileio::T_FILE_TAIL_CHANGED,
            "File Tail Changed",
            "FileTail",
            "filetail",
            &args,
            |t| {
                let want_t = s(t, "tailId");
                let want_p = s(t, "path");
                (want_t.is_empty() || want_t == t_str) && (want_p.is_empty() || want_p == p_str)
            },
        );
        self.emit("FileTail", "Changed", || {
            json!({
                "tailId": tail_id,
                "path": path,
                "line": line,
                "lineNumber": line_number
            })
        });
    }

    pub fn fire_process_started(self: &Rc<Self>, process_name: &str, pid: u32) {
        let mut args = Obj::new();
        args.insert("processName".into(), process_name.into());
        args.insert("processId".into(), (pid as u64).into());
        let p_str = process_name.to_string();
        self.fire(
            crate::fileio::T_PROCESS_STARTED,
            "Process Started",
            "Misc",
            "misc",
            &args,
            |t| {
                let want = s(t, "processName");
                want.is_empty() || want.eq_ignore_ascii_case(&p_str)
            },
        );
        self.emit("Misc", "ProcessStarted", || {
            json!({
                "processName": process_name,
                "processId": pid
            })
        });
    }

    pub fn fire_process_stopped(self: &Rc<Self>, process_name: &str, pid: u32, exit_code: i32) {
        let mut args = Obj::new();
        args.insert("processName".into(), process_name.into());
        args.insert("processId".into(), (pid as u64).into());
        args.insert("exitCode".into(), (exit_code as i64).into());
        let p_str = process_name.to_string();
        self.fire(
            crate::fileio::T_PROCESS_STOPPED,
            "Process Stopped",
            "Misc",
            "misc",
            &args,
            |t| {
                let want = s(t, "processName");
                want.is_empty() || want.eq_ignore_ascii_case(&p_str)
            },
        );
        self.emit("Misc", "ProcessStopped", || {
            json!({
                "processName": process_name,
                "processId": pid,
                "exitCode": exit_code
            })
        });
    }

    pub fn set_file_tail_state(self: &Rc<Self>, tail_id: &str, state: f64) {
        let mut st = self.st.borrow_mut();
        let entry = st.file_tails.entry(tail_id.to_string()).or_insert(true);
        *entry = new_state(state, *entry);
        crate::say(format!("File Tail [{}]: Set State {}", tail_id, *entry));
    }

    pub fn fire_stream_deck_action(
        self: &Rc<Self>,
        button_id: &str,
        action: &str,
        state: f64,
        title: &str,
        value: &str,
    ) {
        let mut args = Obj::new();
        args.insert("buttonId".into(), button_id.into());
        args.insert("action".into(), action.into());
        args.insert("state".into(), state.into());
        args.insert("title".into(), title.into());
        args.insert("value".into(), value.into());
        let btn_str = button_id.to_string();
        self.fire(
            crate::decks::T_STREAM_DECK_ACTION,
            "Stream Deck Action",
            "StreamDeck",
            "streamdeck",
            &args,
            |t| {
                let want = s(t, "buttonId");
                want.is_empty() || want == btn_str
            },
        );
        self.emit("StreamDeck", "Action", || {
            json!({
                "buttonId": button_id,
                "action": action,
                "state": state,
                "title": title,
                "value": value
            })
        });
    }

    pub fn fire_stream_deck_connected(self: &Rc<Self>, device: &str, version: &str) {
        let mut args = Obj::new();
        args.insert("device".into(), device.into());
        args.insert("version".into(), version.into());
        let dev_str = device.to_string();
        self.fire(
            crate::decks::T_STREAM_DECK_CONNECTED,
            "Stream Deck Connected",
            "StreamDeck",
            "streamdeck",
            &args,
            |t| {
                let want = s(t, "device");
                want.is_empty() || want == dev_str
            },
        );
        self.emit(
            "StreamDeck",
            "Connected",
            || json!({"device": device, "version": version}),
        );
    }

    pub fn fire_stream_deck_disconnected(self: &Rc<Self>, device: &str) {
        let mut args = Obj::new();
        args.insert("device".into(), device.into());
        let dev_str = device.to_string();
        self.fire(
            crate::decks::T_STREAM_DECK_DISCONNECTED,
            "Stream Deck Disconnected",
            "StreamDeck",
            "streamdeck",
            &args,
            |t| {
                let want = s(t, "device");
                want.is_empty() || want == dev_str
            },
        );
        self.emit("StreamDeck", "Disconnected", || json!({"device": device}));
    }

    pub fn fire_stream_deck_info(self: &Rc<Self>, device: &str, rows: u32, cols: u32) {
        let mut args = Obj::new();
        args.insert("device".into(), device.into());
        args.insert("rows".into(), (rows as u64).into());
        args.insert("columns".into(), (cols as u64).into());
        let dev_str = device.to_string();
        self.fire(
            crate::decks::T_STREAM_DECK_INFO,
            "Stream Deck Info",
            "StreamDeck",
            "streamdeck",
            &args,
            |t| {
                let want = s(t, "device");
                want.is_empty() || want == dev_str
            },
        );
        self.emit(
            "StreamDeck",
            "Info",
            || json!({"device": device, "rows": rows, "columns": cols}),
        );
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

    pub fn set_stream_deck_title(self: &Rc<Self>, button_id: &str, title: &str, state: f64) {
        let mut st = self.st.borrow_mut();
        let item = st.stream_deck_items.entry(button_id.to_string()).or_default();
        item.id = button_id.to_string();
        item.title = title.to_string();
        item.state = new_state(state, item.state);
        crate::say(format!("StreamDeck [{}]: Set Title '{}'", button_id, title));
    }

    pub fn set_stream_deck_background(self: &Rc<Self>, button_id: &str, image: &str, color: &str, state: f64) {
        let mut st = self.st.borrow_mut();
        let item = st.stream_deck_items.entry(button_id.to_string()).or_default();
        item.id = button_id.to_string();
        item.background = image.to_string();
        item.color = color.to_string();
        item.state = new_state(state, item.state);
        crate::say(format!(
            "StreamDeck [{}]: Set Background img='{}' col='{}'",
            button_id, image, color
        ));
    }

    pub fn set_stream_deck_state(self: &Rc<Self>, button_id: &str, state: f64) {
        let mut st = self.st.borrow_mut();
        let item = st.stream_deck_items.entry(button_id.to_string()).or_default();
        item.id = button_id.to_string();
        item.state = new_state(state, item.state);
        crate::say(format!("StreamDeck [{}]: Set State {}", button_id, item.state));
    }

    pub fn toggle_stream_deck_state(self: &Rc<Self>, button_id: &str) {
        let mut st = self.st.borrow_mut();
        let item = st.stream_deck_items.entry(button_id.to_string()).or_default();
        item.id = button_id.to_string();
        item.state = !item.state;
        crate::say(format!("StreamDeck [{}]: Toggle State -> {}", button_id, item.state));
    }

    pub fn show_stream_deck_alert(self: &Rc<Self>, button_id: &str) {
        crate::say(format!("StreamDeck [{}]: Show Alert", button_id));
    }

    pub fn show_stream_deck_ok(self: &Rc<Self>, button_id: &str) {
        crate::say(format!("StreamDeck [{}]: Show Ok", button_id));
    }

    pub fn set_stream_deck_value(self: &Rc<Self>, button_id: &str, value: &str) {
        let mut st = self.st.borrow_mut();
        let item = st.stream_deck_items.entry(button_id.to_string()).or_default();
        item.id = button_id.to_string();
        item.value = value.to_string();
        crate::say(format!("StreamDeck [{}]: Set Value '{}'", button_id, value));
    }

    fn deck_key<'a>(deck_id: &'a str, deck_name: &'a str) -> String {
        if !deck_id.is_empty() {
            deck_id.to_string()
        } else {
            deck_name.to_string()
        }
    }

    pub fn set_deck_item_state(
        self: &Rc<Self>,
        deck_id: &str,
        deck_name: &str,
        item_id: &str,
        item_name: &str,
        state: f64,
    ) {
        let key = Self::deck_key(deck_id, deck_name);
        let mut st = self.st.borrow_mut();
        let deck = st.decks.entry(key.clone()).or_default();
        let item = deck.get_or_create(item_id, item_name);
        item.state = new_state(state, item.state);
        crate::say(format!("Deck [{}]: Item [{}]: Set State {}", key, item_id, item.state));
    }

    pub fn toggle_deck_item_state(self: &Rc<Self>, deck_id: &str, deck_name: &str, item_id: &str, item_name: &str) {
        let key = Self::deck_key(deck_id, deck_name);
        let mut st = self.st.borrow_mut();
        let deck = st.decks.entry(key.clone()).or_default();
        let item = deck.get_or_create(item_id, item_name);
        item.state = !item.state;
        crate::say(format!(
            "Deck [{}]: Item [{}]: Toggle State -> {}",
            key, item_id, item.state
        ));
    }

    #[allow(clippy::too_many_arguments)]
    pub fn set_deck_item_title(
        self: &Rc<Self>,
        deck_id: &str,
        deck_name: &str,
        item_id: &str,
        item_name: &str,
        color: &str,
        title: &str,
        state: f64,
    ) {
        let key = Self::deck_key(deck_id, deck_name);
        let mut st = self.st.borrow_mut();
        let deck = st.decks.entry(key.clone()).or_default();
        let item = deck.get_or_create(item_id, item_name);
        item.title = title.to_string();
        item.color = color.to_string();
        item.state = new_state(state, item.state);
        crate::say(format!("Deck [{}]: Item [{}]: Set Title '{}'", key, item_id, title));
    }

    #[allow(clippy::too_many_arguments)]
    pub fn set_deck_item_background(
        self: &Rc<Self>,
        deck_id: &str,
        deck_name: &str,
        item_id: &str,
        item_name: &str,
        color: &str,
        url: &str,
        _file_id: &str,
        state: f64,
    ) {
        let key = Self::deck_key(deck_id, deck_name);
        let mut st = self.st.borrow_mut();
        let deck = st.decks.entry(key.clone()).or_default();
        let item = deck.get_or_create(item_id, item_name);
        item.background = url.to_string();
        item.color = color.to_string();
        item.state = new_state(state, item.state);
        crate::say(format!("Deck [{}]: Item [{}]: Set Background '{}'", key, item_id, url));
    }

    #[allow(clippy::too_many_arguments)]
    pub fn set_deck_item_icon(
        self: &Rc<Self>,
        deck_id: &str,
        deck_name: &str,
        item_id: &str,
        item_name: &str,
        color: &str,
        name: &str,
        url: &str,
        _file_id: &str,
        state: f64,
    ) {
        let key = Self::deck_key(deck_id, deck_name);
        let mut st = self.st.borrow_mut();
        let deck = st.decks.entry(key.clone()).or_default();
        let item = deck.get_or_create(item_id, item_name);
        item.icon = if !url.is_empty() {
            url.to_string()
        } else {
            name.to_string()
        };
        item.color = color.to_string();
        item.state = new_state(state, item.state);
        crate::say(format!("Deck [{}]: Item [{}]: Set Icon '{}'", key, item_id, item.icon));
    }

    pub fn set_deck_item_value(
        self: &Rc<Self>,
        deck_id: &str,
        deck_name: &str,
        item_id: &str,
        item_name: &str,
        value: &str,
        state: f64,
    ) {
        let key = Self::deck_key(deck_id, deck_name);
        let mut st = self.st.borrow_mut();
        let deck = st.decks.entry(key.clone()).or_default();
        let item = deck.get_or_create(item_id, item_name);
        item.value = value.to_string();
        item.state = new_state(state, item.state);
        crate::say(format!("Deck [{}]: Item [{}]: Set Value '{}'", key, item_id, value));
    }

    pub fn deck_notify(
        self: &Rc<Self>,
        deck_id: &str,
        deck_name: &str,
        title: &str,
        description: &str,
        color: &str,
        _sound_id: &str,
    ) {
        let key = Self::deck_key(deck_id, deck_name);
        crate::say(format!(
            "Deck [{}]: Notification: '{}' - '{}' (color: {})",
            key, title, description, color
        ));
    }

    pub fn set_deck_page(self: &Rc<Self>, deck_id: &str, deck_name: &str, page: i64) {
        let key = Self::deck_key(deck_id, deck_name);
        let mut st = self.st.borrow_mut();
        let deck = st.decks.entry(key.clone()).or_default();
        deck.page = page;
        crate::say(format!("Deck [{}]: Set Page {}", key, page));
    }

    pub fn next_deck_page(self: &Rc<Self>, deck_id: &str, deck_name: &str) {
        let key = Self::deck_key(deck_id, deck_name);
        let mut st = self.st.borrow_mut();
        let deck = st.decks.entry(key.clone()).or_default();
        deck.page += 1;
        crate::say(format!("Deck [{}]: Next Page -> {}", key, deck.page));
    }

    pub fn prev_deck_page(self: &Rc<Self>, deck_id: &str, deck_name: &str) {
        let key = Self::deck_key(deck_id, deck_name);
        let mut st = self.st.borrow_mut();
        let deck = st.decks.entry(key.clone()).or_default();
        deck.page = (deck.page - 1).max(0);
        crate::say(format!("Deck [{}]: Prev Page -> {}", key, deck.page));
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
        vars::expand(t, &ctx.args, &self.cfg.borrow().globals)
    }

    fn simple(&self, ctx: &Ctx, t: &str) -> String {
        vars::simple(t, &ctx.args, &self.cfg.borrow().globals)
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

    /// Streamer.bot Started.
    pub fn started(self: &Rc<Self>) {
        self.fire(
            T_STARTED,
            "Streamer.bot Started",
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
        let Some((subs, random)) = self.action(&run.action, |a| (a.sub_actions.clone(), a.random_action)) else {
            return;
        };
        let mut ctx = Ctx { args: run.args };
        let start = ctx.args.clone();
        let started = (std::time::Instant::now(), globals::iso_now());
        self.emit(
            "Raw",
            "Action",
            || json!({"id": run.id, "actionId": run.action, "name": run.name, "arguments": start}),
        );
        let subs: Vec<Value> = subs.into_iter().map(Value::Object).collect();
        self.list(&mut ctx, &subs, random).await;
        self.emit("Raw", "ActionCompleted", || {
            json!({"duration": started.0.elapsed().as_secs_f64() * 1000.0, "queuedAt": run.queued_at,
                "startedAt": started.1, "completedAt": globals::iso_now(), "success": true,
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
                        if let Some((subs, random)) = self.action(&id, |a| (a.sub_actions.clone(), a.random_action)) {
                            let subs: Vec<Value> = subs.into_iter().map(Value::Object).collect();
                            if self.list(ctx, &subs, random).await == Flow::BreakToRoot {
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
                | 1025
                | 9001..=9003
                | 11001
                | 11002
                | 11004..=11008
                | 21001..=21010
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
                        Err(e) => crate::say(format!("Streamlabs Desktop: {e}")),
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
                    ctx.args.insert(k, v);
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
                    let (lo, hi) = (n(o, "value").max(0.0), n(o, "maxValue").max(0.0));
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
                    let ms = match n(o, "timeout") as u64 {
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
                            crate::say(format!("Wait For Local Signal: {name} timed out"));
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
                _ => self.unsupported(kind),
            }
            Flow::Continue
        })
    }

    /// Log once per type that a sub-action can't run here (yet), and skip it.
    fn unsupported(&self, kind: u64) {
        if self.st.borrow_mut().warned.insert(kind) {
            let what = name(SUB_ACTIONS, kind as u32).unwrap_or("unknown");
            let why = if kind >= 99998 {
                "C# isn't supported"
            } else {
                "not supported yet"
            };
            crate::say(format!("sub-action {what} (type {kind}): {why}, skipped"));
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

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
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
    pub(crate) fn run(f: impl Future<Output = ()>) -> Vec<String> {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        tokio::task::LocalSet::new().block_on(&rt, f);
        let mut out = Vec::new();
        crate::status(|s| {
            out = s.log.iter().filter(|x| x.starts_with("T ")).cloned().collect();
            s.log.clear();
        });
        out
    }

    async fn settle() {
        tokio::time::sleep(Duration::from_millis(50)).await;
    }

    #[test]
    fn logic_and_flow() {
        let _g = crate::TEST_LOCK.lock().unwrap();
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
        let _g = crate::TEST_LOCK.lock().unwrap();
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
        let _g = crate::TEST_LOCK.lock().unwrap();
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
        let _g = crate::TEST_LOCK.lock().unwrap();
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
    fn inputs_hotkeys_midi_and_voice_control() {
        let _g = crate::TEST_LOCK.lock().unwrap();

        // 1. Actions with triggers
        let a_voice_dict = json!({
            "id": "a_dict",
            "name": "Speech Dictation",
            "enabled": true,
            "group": "",
            "queue": crate::engine::DEFAULT_QUEUE,
            "triggers": [{
                "id": "t_dict",
                "type": crate::inputs::T_SPEECH_DICTATION,
                "enabled": true,
                "culture": "en-US"
            }],
            "subActions": [log("T dictation %dictationText% %culture%")]
        });

        let a_voice_cmd = json!({
            "id": "a_cmd",
            "name": "Speech Command",
            "enabled": true,
            "group": "",
            "queue": crate::engine::DEFAULT_QUEUE,
            "triggers": [{
                "id": "t_cmd",
                "type": crate::inputs::T_SPEECH_COMMAND,
                "enabled": true,
                "commandId": "vcmd1"
            }],
            "subActions": [
                log("T speech_cmd %commandId% %phrase%"),
                // Change voice command state and audio device
                json!({"type": crate::inputs::S_SET_VOICE_COMMAND, "commandId": "vcmd1", "command": "turn off the lights"}),
                json!({"type": crate::inputs::S_SET_VOICE_COMMAND_STATE, "commandId": "vcmd1", "state": 1}),
                json!({"type": crate::inputs::S_SET_VOICE_INPUT, "audioDeviceId": "mic1", "audioDeviceName": "Blue Yeti"}),
                // Test keyboard press subaction
                json!({"type": crate::inputs::S_KEYBOARD_PRESS, "key": "F13", "modifiers": 2})
            ]
        });

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
                "status": "NoteOn",
                "device": ""
            }],
            "subActions": [
                log("T midi %midiDevice% %channel% %noteName% %velocity%"),
                // MIDI subactions
                json!({"type": crate::inputs::S_MIDI_NOTE_ON, "device": "Launchpad", "name": "L1", "channel": 1, "note": 60, "velocity": 127, "sendNoteOff": true, "duration": 100}),
                json!({"type": crate::inputs::S_MIDI_CONTROL_CHANGE, "device": "Launchpad", "name": "L1", "channel": 1, "controller": 7, "value": 100}),
                json!({"type": crate::inputs::S_MIDI_GENERIC, "device": "Launchpad", "name": "L1", "command": "PitchBend", "channel": 1, "data1": 0, "data2": 64})
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
            "actions": [a_voice_dict, a_voice_cmd, a_midi, a_input_key, a_input_mouse]
        }));

        let out = run(async {
            // Fire speech dictation
            e.fire_speech_dictation("hello world", 0.95, "en-US");
            // Fire speech command
            e.fire_speech_command("vcmd1", "Lights On", "turn on the lights", 0.98, "en-US");
            // Fire MIDI note on (0x90 = NoteOn ch 1)
            e.fire_midi_message("Launchpad", 1, 0x90, 60, 100);
            // Fire key press
            // Right-hand Shift counts as Shift; another combination fires nothing.
            e.fire_hotkey(-4095, 2 | 16);
            e.fire_hotkey(-4095, 2);
            // Fire mouse click
            e.fire_mouse(2, 1, 300, 450);

            settle().await;
        });

        assert!(out.contains(&"T dictation hello world en-US".to_string()));
        assert!(out.contains(&"T speech_cmd vcmd1 turn on the lights".to_string()));
        assert!(out.contains(&"T midi Launchpad 1 C4 100".to_string()));
        assert_eq!(out.iter().filter(|l| l.starts_with("T key")).count(), 1, "{out:?}");
        assert!(out.contains(&"T key -4095 18 True False True".to_string()), "{out:?}");
        assert!(out.contains(&"T mouse 2 1 300 450".to_string()), "{out:?}");

        // Check voice command state and voice input changes
        let st = e.st.borrow();
        assert_eq!(st.voice_commands.get("vcmd1").unwrap().0, "turn off the lights");
        assert!(!st.voice_commands.get("vcmd1").unwrap().1);
        assert_eq!(st.voice_input.0, "mic1");
        assert_eq!(st.voice_input.1, "Blue Yeti");
    }

    #[test]
    fn decks_stream_deck_and_chat_window_actions() {
        let _g = crate::TEST_LOCK.lock().unwrap();

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
            "triggers": [{
                "id": "t_sd",
                "type": crate::decks::T_STREAM_DECK_ACTION,
                "enabled": true,
                "buttonId": "btn_1"
            }],
            "subActions": [
                log("T sd %buttonId% %action% %title%"),
                json!({"type": crate::decks::S_STREAM_DECK_SET_TITLE, "buttonId": "btn_1", "title": "Active", "state": 0}),
                json!({"type": crate::decks::S_STREAM_DECK_SET_BACKGROUND, "buttonId": "btn_1", "image": "bg.png", "color": "#ff0000", "state": 0}),
                json!({"type": crate::decks::S_STREAM_DECK_SET_STATE, "buttonId": "btn_1", "state": 0}),
                json!({"type": crate::decks::S_STREAM_DECK_SHOW_ALERT, "buttonId": "btn_1"}),
                json!({"type": crate::decks::S_STREAM_DECK_SHOW_OK, "buttonId": "btn_1"}),
                json!({"type": crate::decks::S_STREAM_DECK_TOGGLE_STATE, "buttonId": "btn_1"}),
                json!({"type": crate::decks::S_STREAM_DECK_SET_VALUE, "buttonId": "btn_1", "value": "100"}),
                // Streamer.bot Decks subactions
                json!({"type": crate::decks::S_DECK_SET_ITEM_STATE, "deckId": "d1", "deckName": "MainDeck", "itemId": "i1", "itemName": "Play", "state": 0}),
                json!({"type": crate::decks::S_DECK_TOGGLE_ITEM_STATE, "deckId": "d1", "deckName": "MainDeck", "itemId": "i1", "itemName": "Play"}),
                json!({"type": crate::decks::S_DECK_SET_ITEM_TITLE, "deckId": "d1", "deckName": "MainDeck", "itemId": "i1", "itemName": "Play", "color": "#00ff00", "title": "Pause", "state": 0}),
                json!({"type": crate::decks::S_DECK_SET_ITEM_BACKGROUND, "deckId": "d1", "deckName": "MainDeck", "itemId": "i1", "itemName": "Play", "color": "#0000ff", "url": "http://img", "fileId": "", "state": 0}),
                json!({"type": crate::decks::S_DECK_SET_ITEM_ICON, "deckId": "d1", "deckName": "MainDeck", "itemId": "i1", "itemName": "Play", "color": "#ffff00", "name": "icon1", "url": "", "fileId": "", "state": 0}),
                json!({"type": crate::decks::S_DECK_SET_ITEM_VALUE, "deckId": "d1", "deckName": "MainDeck", "itemId": "i1", "itemName": "Play", "value": "50", "state": 0}),
                json!({"type": crate::decks::S_DECK_NOTIFY, "deckId": "d1", "deckName": "MainDeck", "title": "Notice", "description": "Deck alert", "color": "#ffffff", "soundId": ""}),
                json!({"type": crate::decks::S_DECK_PAGE_SET, "deckId": "d1", "deckName": "MainDeck", "page": 2}),
                json!({"type": crate::decks::S_DECK_PAGE_NEXT, "deckId": "d1", "deckName": "MainDeck"}),
                json!({"type": crate::decks::S_DECK_PAGE_PREVIOUS, "deckId": "d1", "deckName": "MainDeck"})
            ]
        });

        let e = engine(json!({
            "blocking": false,
            "queues": [],
            "actions": [a_chat, a_sd]
        }));

        let out = run(async {
            e.fire_chat_window_action("twitch", "alice", "123", "hello", "m1", "shoutout");
            e.fire_stream_deck_action("btn_1", "Press", 1.0, "Ready", "0");
            e.fire_stream_deck_connected("Elgato-XL", "1.0");
            e.fire_stream_deck_info("Elgato-XL", 4, 8);
            e.fire_stream_deck_disconnected("Elgato-XL");

            settle().await;
        });

        assert!(out.contains(&"T chat twitch alice shoutout".to_string()));
        assert!(out.contains(&"T sd btn_1 Press Ready".to_string()));

        let st = e.st.borrow();
        let sd_item = st.stream_deck_items.get("btn_1").unwrap();
        assert_eq!(sd_item.title, "Active");
        assert_eq!(sd_item.value, "100");
        assert_eq!(sd_item.background, "bg.png");

        let deck = st.decks.get("d1").unwrap();
        assert_eq!(deck.page, 2);
        let deck_item = deck.items.get("i1").unwrap();
        assert_eq!(deck_item.title, "Pause");
        assert_eq!(deck_item.value, "50");
        assert_eq!(deck_item.icon, "icon1");
    }

    #[test]
    fn file_io_file_tail_and_process_triggers() {
        let _g = crate::TEST_LOCK.lock().unwrap();

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
                // Set File Tail State
                json!({"type": crate::fileio::S_SET_FILE_TAIL_STATE, "fileTailId": "tail1", "state": 0}),
                log("T file f_exists=%f_exists% d_exists=%d_exists% l2=%line_two% count=%all_lines.count%")
            ]
        });

        let a_watch = json!({
            "id": "a_watch",
            "name": "Watcher Action",
            "enabled": true,
            "group": "",
            "queue": crate::engine::DEFAULT_QUEUE,
            "triggers": [{
                "id": "t_watch",
                "type": crate::fileio::T_FILE_WATCHER_CHANGED,
                "enabled": true,
                "watcherId": "w1",
                "path": ""
            }],
            "subActions": [log("T watcher %watcherId% %fileName%")]
        });

        let a_tail = json!({
            "id": "a_tail",
            "name": "Tail Action",
            "enabled": true,
            "group": "",
            "queue": crate::engine::DEFAULT_QUEUE,
            "triggers": [{
                "id": "t_tail",
                "type": crate::fileio::T_FILE_TAIL_CHANGED,
                "enabled": true,
                "tailId": "t1",
                "path": ""
            }],
            "subActions": [log("T tail %tailId% %line% %lineNumber%")]
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
            "subActions": [log("T pstart %processName% %processId%")]
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
            "subActions": [log("T pstop %processName% %processId% %exitCode%")]
        });

        let e = engine(json!({
            "blocking": false,
            "queues": [],
            "actions": [a_file_sub, a_watch, a_tail, a_proc_start, a_proc_stop]
        }));

        let out = run(async {
            e.queue("a_fsub", Obj::new()).unwrap();
            settle().await;

            e.fire_file_watcher(crate::fileio::T_FILE_WATCHER_CHANGED, "w1", &test_file_str, "");
            e.fire_file_tail("t1", &test_file_str, "new appended line", 4);
            e.fire_process_started("obs64.exe", 4321);
            e.fire_process_stopped("obs64.exe", 4321, 0);

            settle().await;
        });

        assert!(out.contains(&"T file f_exists=True d_exists=True l2=world count=3".to_string()));
        assert!(out.contains(&"T watcher w1 test_io.txt".to_string()));
        assert!(out.contains(&"T tail t1 new appended line 4".to_string()));
        assert!(out.contains(&"T pstart obs64.exe 4321".to_string()));
        assert!(out.contains(&"T pstop obs64.exe 4321 0".to_string()));

        let st = e.st.borrow();
        assert_eq!(st.file_tails.get("tail1"), Some(&true));

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
