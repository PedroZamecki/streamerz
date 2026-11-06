//! Streamerz: actions, triggers and the servers that tools already use, for Twitch, Kick, YouTube,
//! OBS and Streamlabs Desktop, running on ~nothing.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod agents;
mod backup;
mod brand;
mod browsers;
mod catalog;
mod commands;
mod crash;
pub mod decks;
#[cfg(not(windows))]
mod devinput;
mod docs;
pub mod donations;
mod edit;
mod emote_events;
pub mod engine;
pub mod fileio;
mod fmt;
mod fswatch;
mod group_icons;
mod groups;
pub mod hotkeys;
pub mod http;
mod i18n;
pub mod inputs;
mod instance;
mod irc;
pub mod kick;
mod math;
mod mcp;
mod midi;
pub mod model;
mod notify;
mod oauth;
pub mod obs;
mod overlays;
pub mod ports;
pub mod remote;
pub mod roles;
mod scenario;
mod search;
mod secrets;
pub mod servers;
mod slash;
mod slobs;
mod songrequests;
mod speech;
mod spotify;
mod storage;
mod streamdeck;
mod templates;
pub mod tiktok;
mod tiktok_bridge;
mod tray;
pub mod twitch;
pub mod types;
mod ui;
mod update;
mod usergroups;
mod vars;
mod webcast;
#[cfg(not(windows))]
mod x11input;
pub mod youtube;

/// Trims heap memory and returns unused virtual memory pages back to the operating system.
#[cfg(all(target_os = "linux", not(target_env = "musl")))]
pub fn trim_memory() {
    unsafe {
        extern "C" {
            fn malloc_trim(pad: usize) -> std::ffi::c_int;
        }
        malloc_trim(0);
    }
}

#[cfg(not(all(target_os = "linux", not(target_env = "musl"))))]
pub fn trim_memory() {}

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::collections::VecDeque;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};
use tokio::sync::watch;

/// Startup policy shared with presentation code. Changing the saved setting requires restart.
static MEMORY_SAVER: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Session-only speaker files can also be left by an interrupted process. The config directory is
/// protected by the existing instance lock before startup reaches this cleanup.
pub(crate) fn cleanup_first_words() {
    let path = config_path().with_file_name("first-words");
    if let Err(e) = std::fs::remove_dir_all(path) {
        if e.kind() != std::io::ErrorKind::NotFound {
            error(format!("Can't clear session data: {e}"));
        }
    }
}

pub fn memory_saver() -> bool {
    MEMORY_SAVER.load(std::sync::atomic::Ordering::Relaxed)
}

#[derive(Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    /// Whether the window is dark now (kept for GetInfo); `theme` says how that's decided.
    dark: bool,
    /// Follow the system, or always dark or light. None: an older file, which only has `dark`.
    #[serde(default)]
    theme: Option<ui::Theme>,
    /// The language of the app (a code from `i18n`); empty follows the system's.
    locale: String,
    start_hidden: bool,
    /// Aggressive background mode, applied on restart. Existing Community files stay untouched.
    memory_saver: bool,
    /// Dialogs, menus and floating panels move as they come and go (Settings, Appearance).
    animations: bool,
    autostart: bool,
    /// Keep [DEBUG] lines in the log.
    debug_log: bool,
    /// Actions, groups and queues, in Streamer.bot's actions.json shape.
    actions: model::ActionsFile,
    /// Persisted globals, and persisted user globals by user ("platform:id", then name).
    globals: model::Obj,
    /// Persistent variables scoped to action group names.
    action_group_variables: model::Obj,
    /// Embedded icons keyed by action group name.
    action_group_icons: model::Obj,
    /// What each action group declares besides its values: settings, files, commands and timers.
    action_groups: model::Obj,
    user_globals: model::Obj,
    /// When each persisted global was last written (ISO 8601).
    global_writes: model::Obj,
    /// Streamer.bot's server settings (its settings.json websockets/http/udp), and this instance.
    websockets: servers::WsSettings,
    http: servers::HttpSettings,
    udp: servers::UdpSettings,
    instance_id: String,
    instance_name: String,
    /// Commands, in Streamer.bot's commands.json shape.
    commands: Vec<model::Obj>,
    /// Timed actions (Streamer.bot's settings.json timedActions.timers) and user groups (its
    /// users.dat groups: id, name, bots, userIds).
    timers: Vec<model::Obj>,
    groups: Vec<model::Obj>,
    /// File and folder watchers (Streamer.bot's settings.json fileWatcherSettings.watchers: id, name,
    /// enabled, folderPath, filter, includeSubdirectories, addChangedData, asJson) and file tails (its
    /// fileTails.fileTails: id, name, enabled, filePath).
    file_watchers: Vec<model::Obj>,
    /// The server for the Stream Deck plugin (Streamer.bot's streamDeckSettings): off unless turned on.
    stream_deck: streamdeck::Settings,
    /// Voice commands (Streamer.bot's speechToText.commands: id, name, enabled, command (one phrase per
    /// line), location (0 exact, 1 start, 2 anywhere), overrideGlobalConfidence, confidenceThreshold),
    /// the confidence they need (0 to 1) and whether the log says what was heard.
    voice_commands: Vec<model::Obj>,
    speech_confidence: f64,
    speech_log_spoken: bool,
    file_tails: Vec<model::Obj>,
    /// Quotes, in Streamer.bot's quotes.dat shape (id, timestamp, userId, user, platform, gameId,
    /// gameName, quote).
    quotes: Vec<model::Obj>,
    /// Persisted command counters: by command id, and by "command id:user id".
    counters: model::Obj,
    /// Twitch configuration (broadcaster, bot, auto-connect, settings).
    #[serde(default)]
    pub twitch: twitch::TwitchConfig,
    /// OBS Studio configuration.
    #[serde(default)]
    pub obs: obs::ObsConfig,
    /// Streamlabs Desktop connections (Streamer.bot's slobsSettings).
    pub slobs: slobs::Settings,
    /// Kick configuration.
    #[serde(default)]
    pub kick: kick::KickConfig,
    /// YouTube configuration.
    #[serde(default)]
    pub youtube: youtube::YouTubeConfig,
    /// Spotify is opt-in and makes requests only when used.
    #[serde(default)]
    pub spotify: spotify::SpotifyConfig,
    /// TikTok configuration.
    #[serde(default)]
    pub tiktok: tiktok::TikTokConfig,
    /// Donations and shops configuration.
    #[serde(default)]
    pub donations: donations::DonationsConfig,
    /// The built-in chat commands were added (once: deleting one keeps it deleted).
    #[serde(default)]
    builtins: bool,
    /// The window as it was left.
    #[serde(default)]
    pub window: WindowState,
    /// What a new release does: ask first, install in the background, or nothing.
    pub updates: update::Updates,
    pub notifications: notify::Settings,
    /// Plugins by id (their file's name): on or off, and their settings.
    #[serde(skip_serializing_if = "std::collections::BTreeMap::is_empty")]
    pub plugins: std::collections::BTreeMap<String, engine::plugins::PluginConfig>,
    /// How the window shows dates and times (.NET patterns); empty follows the system.
    #[serde(skip_serializing_if = "String::is_empty")]
    date_format: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    time_format: String,
    /// Fields this build doesn't know (e.g. written by a newer one): kept, so a save loses nothing.
    #[serde(flatten)]
    rest: Map<String, Value>,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            dark: true,
            theme: Some(ui::Theme::System),
            locale: String::new(),
            start_hidden: true,
            memory_saver: false,
            animations: true,
            autostart: false,
            debug_log: false,
            actions: model::ActionsFile::default(),
            globals: Map::new(),
            action_group_variables: Map::new(),
            action_group_icons: Map::new(),
            action_groups: Map::new(),
            user_globals: Map::new(),
            global_writes: Map::new(),
            websockets: servers::WsSettings::default(),
            http: servers::HttpSettings::default(),
            udp: servers::UdpSettings::default(),
            instance_id: String::new(),
            instance_name: "Streamerz".into(),
            commands: Vec::new(),
            timers: Vec::new(),
            groups: Vec::new(),
            file_watchers: Vec::new(),
            stream_deck: Default::default(),
            voice_commands: Vec::new(),
            speech_confidence: 0.0,
            speech_log_spoken: false,
            file_tails: Vec::new(),
            counters: Map::new(),
            quotes: Vec::new(),
            twitch: twitch::TwitchConfig::default(),
            obs: obs::ObsConfig::default(),
            slobs: slobs::Settings::default(),
            kick: kick::KickConfig::default(),
            youtube: youtube::YouTubeConfig::default(),
            spotify: spotify::SpotifyConfig::default(),
            tiktok: tiktok::TikTokConfig::default(),
            donations: donations::DonationsConfig::default(),
            builtins: false,
            window: WindowState::default(),
            updates: update::Updates::default(),
            notifications: notify::Settings::default(),
            plugins: Default::default(),
            date_format: String::new(),
            time_format: String::new(),
            rest: Map::new(),
        }
    }
}

/// The window as it was left: its tab, the chat's view and its size (0: the default).
#[derive(Clone, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct WindowState {
    pub tab: String,
    pub chat_view: String,
    pub w: i32,
    pub h: i32,
    /// Chat: hide the time, hide the badges.
    pub chat_no_time: bool,
    pub chat_no_badges: bool,
    /// Emotes keep still (their first frame) in the chat and the user card.
    pub chat_still_emotes: bool,
    /// YouTube chat: show user tag (@handle) instead of plain display name.
    #[serde(default = "chat_youtube_tag_default")]
    pub chat_youtube_tag: bool,
    /// Platforms where the individual chat box sends as broadcaster instead of bot-with-fallback.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub chat_as_broadcaster: Vec<String>,
    /// Platforms whose chat is taken out of the Chat page's views (put back from Options).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub chat_hidden: Vec<String>,
    /// Selected target platform in "All together" chat view's input bar.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub chat_all_platform: String,
    /// Starred favorite emotes and emoji in the picker: (name, image URL).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub favorite_emotes: Vec<(String, String)>,
    /// The sidebar shows only its icons.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub nav_collapsed: bool,
    /// How many chat lines are kept, for how long, and how they come and go.
    #[serde(skip_serializing_if = "ChatLimits::is_default")]
    pub chat: ChatLimits,
}

fn chat_youtube_tag_default() -> bool {
    true
}

impl Default for WindowState {
    fn default() -> Self {
        Self {
            tab: String::new(),
            chat_view: String::new(),
            w: 0,
            h: 0,
            chat_no_time: false,
            chat_no_badges: false,
            chat_still_emotes: false,
            chat_youtube_tag: true,
            chat_as_broadcaster: Vec::new(),
            chat_hidden: Vec::new(),
            chat_all_platform: String::new(),
            favorite_emotes: Vec::new(),
            nav_collapsed: false,
            chat: ChatLimits::default(),
        }
    }
}

/// What the chat keeps in memory: the oldest lines go first, and a line can go by its age. Applies to
/// the engine's log and to every pane that shows it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct ChatLimits {
    /// Keep chat messages in memory. When off, messages are read and processed (commands, triggers,
    /// facts) but not saved in memory.
    pub save_history: bool,
    /// Lines kept of each platform's chat.
    pub per_platform: usize,
    /// Lines kept of all of them together.
    pub total: usize,
    /// A line is taken out of the chat, and the memory, after this many seconds (0: it stays).
    pub lifetime: u64,
    /// How long a new line takes to appear, and an old one to go, in ms (0: at once).
    pub fade_in: u32,
    pub fade_out: u32,
}

impl Default for ChatLimits {
    fn default() -> Self {
        Self {
            // Off unless the user turns it on: Streamerz reads chat to run actions, not to keep it.
            // (Tests read what the chat said, so they keep it.)
            save_history: cfg!(test),
            per_platform: 400,
            total: 1000,
            lifetime: 0,
            fade_in: 0,
            fade_out: 400,
        }
    }
}

impl ChatLimits {
    /// The most lines a chat can keep.
    pub const MOST: usize = 1000;
    pub const LEAST: usize = 0;

    fn is_default(&self) -> bool {
        *self == Self::default()
    }

    /// The limits as they apply: in range, whatever the file says.
    pub fn clamped(self) -> Self {
        Self {
            save_history: self.save_history,
            per_platform: if self.save_history {
                self.per_platform.clamp(Self::LEAST, Self::MOST)
            } else {
                0
            },
            total: if self.save_history {
                self.total.clamp(Self::LEAST, Self::MOST)
            } else {
                0
            },
            lifetime: self.lifetime.min(30 * 24 * 3600),
            fade_in: self.fade_in.min(3000),
            fade_out: self.fade_out.min(10_000),
        }
    }

    /// Which of the lines, oldest first (each is its platform's index, none for a line of no platform),
    /// are over the limits and go: the oldest of a platform past its own, then the oldest of all past the
    /// total. The positions, in order.
    pub fn excess(&self, lines: impl Iterator<Item = Option<usize>>) -> Vec<usize> {
        let l = self.clamped();
        let lines: Vec<Option<usize>> = lines.collect();
        if !l.save_history {
            return (0..lines.len()).collect();
        }
        let mut each = [0usize; 4];
        let mut all = lines.len();
        for p in lines.iter().flatten() {
            each[*p] += 1;
        }
        let mut gone = vec![false; lines.len()];
        // Each platform's own limit first, then the total over what is left.
        for (i, p) in lines.iter().enumerate() {
            if let Some(p) = p.filter(|p| each[*p] > l.per_platform) {
                gone[i] = true;
                each[p] -= 1;
                all -= 1;
            }
        }
        for g in gone.iter_mut().filter(|g| !**g) {
            if all > l.total {
                *g = true;
                all -= 1;
            }
        }
        (0..lines.len()).filter(|i| gone[*i]).collect()
    }
}

impl Config {
    /// Each platform reads the broadcaster's own channel unless another is set (in Advanced).
    pub fn own_channels(&mut self) {
        if self.youtube.channel_id.trim().is_empty() {
            if let Some(b) = &self.youtube.broadcaster {
                self.youtube.channel_id.clone_from(&b.channel_id);
            }
        }
        if self.kick.channel.trim().is_empty() {
            if let Some(b) = &self.kick.broadcaster {
                self.kick.channel = b.username.to_lowercase();
            }
        }
    }
}

/// How bad a log line is. Each has its tag in the log file and its colour on the Log page.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
pub enum Level {
    Debug,
    Info,
    Warn,
    Error,
    Fatal,
}

impl Level {
    pub const ALL: [Level; 5] = [Level::Debug, Level::Info, Level::Warn, Level::Error, Level::Fatal];

    pub fn tag(self) -> &'static str {
        match self {
            Level::Debug => "DEBUG",
            Level::Info => "INFO",
            Level::Warn => "WARN",
            Level::Error => "ERROR",
            Level::Fatal => "FATAL",
        }
    }
}

/// Debug lines are dropped unless Settings turns them on.
static DEBUG_LOG: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
/// Log lines aren't printed (the scenario runner prints its own results).
static QUIET: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

thread_local! {
    /// A scenario's own copy of the log lines logged on its thread (scenario.rs), so tests running
    /// side by side don't mix their lines.
    static CAPTURE: std::cell::RefCell<Option<Vec<String>>> = const { std::cell::RefCell::new(None) };
}

/// Starts (true) or ends this thread's capture of log lines; what was caught.
fn capture(on: bool) -> Vec<String> {
    CAPTURE.with_borrow_mut(|c| std::mem::replace(c, on.then(Vec::new)).unwrap_or_default())
}

fn set_quiet(on: bool) {
    QUIET.store(on, std::sync::atomic::Ordering::Relaxed);
}

pub fn set_debug_log(on: bool) {
    DEBUG_LOG.store(on, std::sync::atomic::Ordering::Relaxed);
}

/// What the window and tray show. Shared by the tasks.
struct Status {
    /// The last LOG_LINES log lines (level, when in unix seconds, text), and how many were ever logged
    /// (so the window knows what's new).
    log: VecDeque<(Level, i64, String)>,
    logged: u64,
}

const LOG_LINES: usize = 200;

/// How many lines were ever logged (to tell which lines came after a moment).
pub fn log_count() -> u64 {
    let mut n = 0;
    status(|s| n = s.logged);
    n
}

/// The log's last `n` lines, without their levels.
pub fn log_tail(n: usize) -> Vec<String> {
    let mut out = Vec::new();
    status(|s| out = s.log.iter().rev().take(n).rev().map(|(_, _, l)| l.clone()).collect());
    out
}

/// The log's last `n` lines with their levels: "[WARN] text" (the dev WebSocket's GetLog).
pub fn log_tail_tagged(n: usize) -> Vec<String> {
    let mut out = Vec::new();
    status(|s| {
        out = s
            .log
            .iter()
            .rev()
            .take(n)
            .rev()
            .map(|(lv, _, l)| format!("[{}] {l}", lv.tag()))
            .collect()
    });
    out
}

static STATUS: Mutex<Status> = Mutex::new(Status {
    log: VecDeque::new(),
    logged: 0,
});

fn status(f: impl FnOnce(&mut Status)) {
    if let Ok(mut s) = STATUS.lock() {
        f(&mut s);
    }
}

/// The log's last `n` lines for a crash report: never waits (the panic may have come with the lock).
fn log_tail_try(n: usize) -> Vec<String> {
    STATUS
        .try_lock()
        .map(|s| {
            s.log
                .iter()
                .rev()
                .take(n)
                .rev()
                .map(|(lv, _, l)| format!("[{}] {l}", lv.tag()))
                .collect()
        })
        .unwrap_or_default()
}

/// Print and keep for the window's log.
/// Hide credentials a log line may carry (error bodies, URLs): the value after each marker.
fn redact(line: &str) -> String {
    const MARKERS: &[&str] = &[
        "access_token",
        "refresh_token",
        "accesstoken",
        "refreshtoken",
        "client_secret",
        "id_token",
        "sessionid",
        "token=",
        "Bearer ",
        "password",
    ];
    let lower = line.to_ascii_lowercase();
    let mut cuts: Vec<(usize, usize)> = Vec::new();
    for m in MARKERS {
        let m = m.to_ascii_lowercase();
        let mut from = 0;
        while let Some(i) = lower[from..].find(&m) {
            let mut j = from + i + m.len();
            let b = line.as_bytes();
            while j < b.len() && matches!(b[j], b'"' | b'\'' | b':' | b'=' | b' ') {
                j += 1;
            }
            let start = j;
            while j < b.len() && (b[j].is_ascii_alphanumeric() || b"._~+/-%=".contains(&b[j])) {
                j += 1;
            }
            if j > start {
                cuts.push((start, j));
            }
            from = j.max(from + i + 1);
        }
    }
    cuts.sort_unstable();
    let mut out = String::with_capacity(line.len());
    let mut at = 0;
    for (a, b) in cuts {
        if a >= at {
            out.push_str(&line[at..a]);
            out.push_str("***");
            at = b;
        }
    }
    out.push_str(&line[at..]);
    out
}

/// Logs `line` at `level`: printed, written to the log file, and kept for the Log page.
fn log(level: Level, line: String) {
    if level == Level::Debug && !DEBUG_LOG.load(std::sync::atomic::Ordering::Relaxed) {
        return;
    }
    let line = redact(&line);
    if level == Level::Error {
        engine::record_error(&line);
    }
    if !QUIET.load(std::sync::atomic::Ordering::Relaxed) {
        let _ = writeln!(std::io::stdout(), "[{}] {line}", level.tag());
    }
    crash::line(level, &line);
    CAPTURE.with_borrow_mut(|c| {
        if let Some(c) = c {
            c.push(line.clone());
        }
    });
    status(|s| {
        if s.log.len() == LOG_LINES {
            s.log.pop_front();
        }
        s.log.push_back((level, fmt::unix_now().as_secs() as i64, line));
        s.logged += 1;
    });
    ui::logged();
}

/// Something that happened, worth knowing.
fn say(line: String) {
    log(Level::Info, line);
}

/// Detail for finding a problem; only kept when Settings turns debug lines on.
fn debug(line: String) {
    log(Level::Debug, line);
}

/// Something went wrong but Streamerz carries on (it retries, or uses a fallback).
fn warn(line: String) {
    log(Level::Warn, line);
}

/// Something failed: what was asked wasn't done.
fn error(line: String) {
    log(Level::Error, line);
}

/// Asked to end (logout, shutdown, `kill`, Ctrl+C): saves and quits like Quit does, so the next start
/// doesn't take it for a crash.
#[cfg(unix)]
async fn ended(e: std::rc::Rc<engine::Engine>) {
    use tokio::signal::unix::{signal, SignalKind};
    let (Ok(mut term), Ok(mut hup), Ok(mut int)) = (
        signal(SignalKind::terminate()),
        signal(SignalKind::hangup()),
        signal(SignalKind::interrupt()),
    ) else {
        return;
    };
    tokio::select! {
        _ = term.recv() => {}
        _ = hup.recv() => {}
        _ = int.recv() => {}
    }
    say("Asked to close: saving and quitting".into());
    e.save_shutdown();
    crash::quit(0)
}

fn version() -> String {
    format!("Streamerz {}", brand::VERSION)
}

fn default_config_path() -> PathBuf {
    #[cfg(target_os = "windows")]
    {
        if let Some(appdata) = std::env::var_os("APPDATA") {
            return PathBuf::from(appdata).join("streamerz").join("streamerz.json");
        }
        if let Some(userprofile) = std::env::var_os("USERPROFILE") {
            return PathBuf::from(userprofile)
                .join("AppData")
                .join("Roaming")
                .join("streamerz")
                .join("streamerz.json");
        }
    }
    #[cfg(target_os = "macos")]
    {
        if let Some(home) = std::env::var_os("HOME") {
            return PathBuf::from(home)
                .join("Library")
                .join("Application Support")
                .join("streamerz")
                .join("streamerz.json");
        }
    }
    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    {
        if let Some(xdg) = std::env::var_os("XDG_CONFIG_HOME").filter(|s| !s.is_empty()) {
            return PathBuf::from(xdg).join("streamerz").join("streamerz.json");
        }
        if let Some(home) = std::env::var_os("HOME").filter(|s| !s.is_empty()) {
            return PathBuf::from(home)
                .join(".config")
                .join("streamerz")
                .join("streamerz.json");
        }
    }
    PathBuf::from("streamerz.json")
}

fn resolve_config_path(raw: PathBuf) -> PathBuf {
    if let Ok(canon) = std::fs::canonicalize(&raw) {
        return canon;
    }
    let abs = if raw.is_absolute() {
        raw
    } else {
        std::env::current_dir().unwrap_or_default().join(raw)
    };
    if let (Some(parent), Some(name)) = (abs.parent(), abs.file_name()) {
        if let Ok(p) = std::fs::canonicalize(parent) {
            return p.join(name);
        }
    }
    abs
}

/// A settings file named on the command line (`--config`) or in `STREAMERZ_CONFIG`.
fn explicit_config() -> Option<PathBuf> {
    let mut args = std::env::args_os().skip(1);
    while let Some(a) = args.next() {
        if a == "--config" {
            if let Some(p) = args.next() {
                return Some(resolve_config_path(PathBuf::from(p)));
            }
        } else if let Some(s) = a.to_str().and_then(|s| s.strip_prefix("--config=")) {
            return Some(resolve_config_path(PathBuf::from(s)));
        }
    }
    std::env::var_os("STREAMERZ_CONFIG")
        .filter(|p| !p.is_empty())
        .map(|p| resolve_config_path(PathBuf::from(p)))
}

/// The file in the usual folder that says where the settings are, when the user put them elsewhere.
fn location_file() -> PathBuf {
    default_config_path().with_file_name("location")
}

/// A `streamerz.json` next to the program (a portable copy), unless it runs from an AppImage or a
/// system folder.
fn portable_config() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    if update::appimage().is_some() || exe.starts_with("/tmp") || exe.starts_with("/usr") {
        return None;
    }
    Some(exe.parent()?.join("streamerz.json")).filter(|p| p.is_file())
}

pub(crate) fn config_path() -> PathBuf {
    if let Some(dir) = DEV_DIR.get() {
        return dir.join("streamerz.json");
    }
    if !brand::IS_DEV && !std::env::args_os().any(|a| a == "--scenario") {
        return resolve_config_path(default_config_path());
    }
    static CFG_PATH: OnceLock<PathBuf> = OnceLock::new();
    CFG_PATH
        .get_or_init(|| {
            let chosen = std::fs::read_to_string(location_file())
                .ok()
                .map(|t| PathBuf::from(t.trim()))
                .filter(|p| !p.as_os_str().is_empty());
            let path = explicit_config()
                .or(chosen)
                .or_else(portable_config)
                .unwrap_or_else(default_config_path);
            resolve_config_path(path)
        })
        .clone()
}

/// `path` with `ext` added: streamerz.json -> streamerz.json.bak.
fn with(path: &Path, ext: &str) -> PathBuf {
    let mut p = path.as_os_str().to_owned();
    p.push(ext);
    p.into()
}

/// Written to a temp file and renamed over the old one, so a crash never leaves half a file; the
/// previous version is kept as `.bak`.
fn save_to(path: &Path, c: &Config) -> std::io::Result<()> {
    write_value(path, serde_json::to_value(c).unwrap_or_default())
}

/// `save_to`, from the config already as JSON (sealing its secrets happens here).
fn write_value(path: &Path, mut v: serde_json::Value) -> std::io::Result<()> {
    let _dev_guard = dev_write_guard()?;
    // The window and the engine both save: one at a time.
    static SAVING: Mutex<()> = Mutex::new(());
    let _one = SAVING.lock();
    // Development cleanup removes the directory while shutdown saves may still be queued.
    // Never recreate it from a background save.
    if DEV_DIR.get().is_none() {
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
    }
    let tmp = with(path, ".tmp");
    secrets::seal(&mut v);
    let mut o = std::fs::OpenOptions::new();
    o.write(true).create(true).truncate(true);
    // Only the owner can read it: it may hold logins (see secrets.rs).
    #[cfg(unix)]
    std::os::unix::fs::OpenOptionsExt::mode(&mut o, 0o600);
    let mut f = o.open(&tmp)?;
    #[cfg(unix)]
    f.set_permissions(std::os::unix::fs::PermissionsExt::from_mode(0o600))?;
    f.write_all(serde_json::to_string_pretty(&v).unwrap_or_default().as_bytes())?;
    f.sync_all()?;
    match std::fs::copy(path, with(path, ".bak")) {
        Err(e) if e.kind() != std::io::ErrorKind::NotFound => return Err(e),
        _ => {}
    }
    std::fs::rename(&tmp, path)
}

fn save(c: &Config) -> std::io::Result<()> {
    save_value(serde_json::to_value(c).unwrap_or_default())
}

fn save_value(v: serde_json::Value) -> std::io::Result<()> {
    let r = write_value(&config_path(), v);
    if let Err(e) = &r {
        error(format!("can't save streamerz.json ({e})"));
    }
    r
}

/// Saves `c` without making the caller wait: the window's thread must never sit on the disk, the keyring
/// or the save lock (the engine may hold it). One short thread writes the latest config, and the next
/// one asked for meanwhile replaces a pending one, so a burst of changes is one write.
#[derive(Default)]
struct SaveQueue {
    pending: Option<serde_json::Value>,
    busy: bool,
}

static SAVE_QUEUE: Mutex<SaveQueue> = Mutex::new(SaveQueue {
    pending: None,
    busy: false,
});
static SAVE_DONE: std::sync::Condvar = std::sync::Condvar::new();

fn save_later(c: &Config) {
    let mut queue = SAVE_QUEUE.lock().unwrap_or_else(|e| e.into_inner());
    queue.pending = Some(serde_json::to_value(c).unwrap_or_default());
    if queue.busy {
        return;
    }
    queue.busy = true;
    drop(queue);
    if std::thread::Builder::new()
        .name("save".into())
        .spawn(drain_saves)
        .is_err()
    {
        drain_saves();
    }
}

fn drain_saves() {
    loop {
        let mut queue = SAVE_QUEUE.lock().unwrap_or_else(|e| e.into_inner());
        let Some(value) = queue.pending.take() else {
            queue.busy = false;
            SAVE_DONE.notify_all();
            return;
        };
        drop(queue);
        drop(save_value(value));
    }
}

/// Wait for the last requested save before deleting development storage or exiting.
fn wait_for_saves() {
    wait_on_saves(&SAVE_QUEUE, &SAVE_DONE);
}

fn wait_on_saves(queue: &Mutex<SaveQueue>, done: &std::sync::Condvar) {
    let state = queue.lock().unwrap_or_else(|e| e.into_inner());
    drop(done.wait_while(state, |q| q.busy).unwrap_or_else(|e| e.into_inner()));
}

/// The config, else its `.bak`, else the defaults. Written back, so new settings show up in the
/// file; an invalid file is left alone.
fn load_from(path: &Path) -> Config {
    let read = |p: &Path| {
        std::fs::read_to_string(p).map(|s| {
            serde_json::from_str::<serde_json::Value>(&s).and_then(|mut v| {
                secrets::open(&mut v);
                serde_json::from_value::<Config>(v)
            })
        })
    };
    let mut c = match read(path) {
        Ok(Ok(c)) => c,
        Err(_) => Config::default(),
        Ok(Err(e)) => {
            error(format!("{} is invalid ({e})", path.display()));
            return match read(&with(path, ".bak")) {
                Ok(Ok(c)) => {
                    warn("using its .bak, the previous version".into());
                    c
                }
                _ => {
                    warn("using the defaults".into());
                    Config::default()
                }
            };
        }
    };
    c.own_channels();
    usergroups::migrate(&mut c.groups);
    let _ = save_to(path, &c);
    c
}

/// Start with Windows: a value under the user's Run registry key.
#[cfg(windows)]
fn autostart(on: bool) -> std::io::Result<()> {
    if DEV_DIR.get().is_some() {
        return Err(std::io::Error::other("Autostart is unavailable in development"));
    }
    let exe = format!("\"{}\"", update::exe().ok_or(std::io::ErrorKind::NotFound)?.display());
    run_at_login("streamerz", on.then_some(exe.as_str()))
}

/// Sets (or, with None, removes) the value `name` of the user's Run key: what Windows starts at login.
#[cfg(windows)]
fn run_at_login(name: &str, command: Option<&str>) -> std::io::Result<()> {
    use windows_sys::Win32::System::Registry::{RegDeleteKeyValueW, RegSetKeyValueW, HKEY_CURRENT_USER, REG_SZ};
    let w = |s: &str| s.encode_utf16().chain([0]).collect::<Vec<u16>>();
    let (key, name) = (w(RUN_KEY), w(name));
    // SAFETY: NUL-terminated UTF-16 strings that outlive the calls; size is in bytes.
    let err = unsafe {
        match command.map(w) {
            Some(exe) => RegSetKeyValueW(
                HKEY_CURRENT_USER,
                key.as_ptr(),
                name.as_ptr(),
                REG_SZ,
                exe.as_ptr().cast(),
                (exe.len() * 2) as u32,
            ),
            None => RegDeleteKeyValueW(HKEY_CURRENT_USER, key.as_ptr(), name.as_ptr()),
        }
    };
    // 2 = not found: already off.
    match err {
        0 | 2 => Ok(()),
        e => Err(std::io::Error::from_raw_os_error(e as i32)),
    }
}

#[cfg(windows)]
const RUN_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";

/// Start at login: an XDG autostart entry.
#[cfg(not(windows))]
fn autostart(on: bool) -> std::io::Result<()> {
    if DEV_DIR.get().is_some() {
        return Err(std::io::Error::other("Autostart is unavailable in development"));
    }
    use std::io::ErrorKind::NotFound;
    let config = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))
        .ok_or(NotFound)?;
    let file = config.join("autostart/streamerz.desktop");
    if !on {
        return match std::fs::remove_file(file) {
            Err(e) if e.kind() == NotFound => Ok(()),
            r => r,
        };
    }
    std::fs::create_dir_all(config.join("autostart"))?;
    let exe = update::exe().ok_or(NotFound)?;
    std::fs::write(
        file,
        format!(
            "[Desktop Entry]\nType=Application\nName=Streamerz\nExec=\"{}\"\n",
            exe.display()
        ),
    )
}

static DEV_DIR: OnceLock<PathBuf> = OnceLock::new();
static DEV_SOURCE: OnceLock<PathBuf> = OnceLock::new();

/// Where the TikTok extension's files go: next to streamerz.json. A browser keeps loading them after a
/// development process (and its disposable settings) is gone, so a development build's go to the user's
/// cache instead, one folder per settings it cloned: stable across its restarts, apart from a test's
/// scratch settings, and never next to the production settings.
pub(crate) fn extension_folder() -> PathBuf {
    extension_folder_for(
        &config_path(),
        DEV_SOURCE.get().map(PathBuf::as_path),
        dev_cache_dir().as_deref(),
    )
}

fn extension_folder_for(config: &Path, source: Option<&Path>, cache: Option<&Path>) -> PathBuf {
    match (source, cache) {
        (Some(source), Some(cache)) => {
            use sha2::Digest;
            let id = sha2::Sha256::digest(source.to_string_lossy().as_bytes());
            let id: String = id[..6].iter().map(|b| format!("{b:02x}")).collect();
            cache.join(id).join("tiktok-extension")
        }
        _ => config.with_file_name("tiktok-extension"),
    }
}

/// The cache folder development builds keep across runs.
fn dev_cache_dir() -> Option<PathBuf> {
    #[cfg(windows)]
    let base = std::env::var_os("LOCALAPPDATA").map(PathBuf::from);
    #[cfg(not(windows))]
    let base = std::env::var_os("XDG_CACHE_HOME")
        .filter(|p| !p.is_empty())
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".cache")));
    base.map(|b| b.join("streamerz-dev"))
}
static DEV_LOCK: Mutex<Option<std::fs::File>> = Mutex::new(None);
static DEV_LISTENER: Mutex<Option<std::net::TcpListener>> = Mutex::new(None);

pub(crate) fn take_dev_listener(port: u16) -> Option<std::net::TcpListener> {
    let mut listener = DEV_LISTENER.lock().ok()?;
    if listener.as_ref()?.local_addr().ok()?.port() == port {
        listener.take()
    } else {
        None
    }
}
// Serializes development writes against deleting the temporary directory on shutdown.
pub(crate) static DEV_CACHE_CLOSED: Mutex<bool> = Mutex::new(false);

pub(crate) fn dev_write_guard() -> std::io::Result<Option<std::sync::MutexGuard<'static, bool>>> {
    if DEV_DIR.get().is_none() {
        return Ok(None);
    }
    let guard = DEV_CACHE_CLOSED
        .lock()
        .map_err(|_| std::io::Error::other("development storage lock poisoned"))?;
    if *guard {
        return Err(std::io::Error::other("development storage is closing"));
    }
    Ok(Some(guard))
}

/// Reclaim old abandoned directories only after acquiring their lock.
fn cleanup_stale_dev(base: &Path) {
    let Ok(entries) = std::fs::read_dir(base) else { return };
    for entry in entries.flatten() {
        let name = entry.file_name();
        if !name.to_string_lossy().starts_with("streamerz-dev-") {
            continue;
        }
        let Ok(kind) = entry.file_type() else { continue };
        if !kind.is_dir() {
            continue;
        }
        let Ok(meta) = entry.metadata() else { continue };
        let old = meta
            .modified()
            .ok()
            .and_then(|t| t.elapsed().ok())
            .is_some_and(|age| age.as_secs() > 24 * 60 * 60);
        if !old {
            continue;
        }
        // Only directories with an existing lock are ours. Never create files in a foreign directory.
        let Ok(lock) = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .open(entry.path().join("instance.lock"))
        else {
            continue;
        };
        if lock.try_lock().is_ok() {
            // Windows cannot remove an open lock file. The directory is already stale and no
            // instance can reuse its unique name while its owner is alive.
            #[cfg(windows)]
            drop(lock);
            let _ = std::fs::remove_dir_all(entry.path());
        }
    }
}

/// Give each development process a private copy before any settings are loaded or saved.
fn prepare_dev(source: &Path) -> std::io::Result<PathBuf> {
    let base = std::env::temp_dir();
    cleanup_stale_dev(&base);
    for n in 0..1000 {
        let dir = base.join(format!("streamerz-dev-{}-{n}", std::process::id()));
        #[cfg_attr(not(unix), allow(unused_mut))]
        let mut builder = std::fs::DirBuilder::new();
        #[cfg(unix)]
        std::os::unix::fs::DirBuilderExt::mode(&mut builder, 0o700);
        match builder.create(&dir) {
            Ok(()) => {
                for name in ["streamerz.json", "streamerz.json.bak", "community.json"] {
                    let from = if name == "streamerz.json" {
                        source.to_path_buf()
                    } else {
                        source.with_file_name(name)
                    };
                    match std::fs::copy(from, dir.join(name)) {
                        Ok(_) => {}
                        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                        Err(e) => {
                            let _ = std::fs::remove_dir_all(&dir);
                            return Err(e);
                        }
                    }
                }
                {
                    let name = "plugins";
                    let from = source.with_file_name(name);
                    if from.exists() {
                        if let Err(e) = storage::copy_tree(&from, &dir.join(name)) {
                            let _ = std::fs::remove_dir_all(&dir);
                            return Err(e);
                        }
                    }
                }
                return Ok(dir);
            }
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(e) => return Err(e),
        }
    }
    Err(std::io::Error::new(
        std::io::ErrorKind::AlreadyExists,
        "no temporary directory available",
    ))
}

pub(crate) fn cleanup_dev() {
    if let Some(dir) = DEV_DIR.get() {
        let Ok(mut closed) = DEV_CACHE_CLOSED.lock() else {
            return;
        };
        *closed = true;
        if let Ok(mut lock) = DEV_LOCK.lock() {
            drop(lock.take());
        }
        let _ = std::fs::remove_dir_all(dir);
    }
}

#[cfg(windows)]
fn attach_parent_console() {
    unsafe {
        extern "system" {
            fn AttachConsole(dw_process_id: u32) -> i32;
        }
        AttachConsole(u32::MAX);
    }
}

fn main() {
    #[cfg(windows)]
    if std::env::args_os().len() > 1 {
        attach_parent_console();
    }
    // An MCP server for AI assistants, driving the running instance (mcp.rs); no window.
    if std::env::args_os().any(|a| a == "--mcp") {
        mcp::run();
        return;
    }
    // Scenarios (scenario.rs): the engine alone, on settings of their own in a scratch folder.
    let args: Vec<String> = std::env::args().collect();
    if let Some(i) = args.iter().position(|a| a == "--scenario") {
        let scratch = std::env::temp_dir().join(format!("streamerz-scenario-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&scratch);
        std::env::set_var("STREAMERZ_CONFIG", scratch.join("streamerz.json"));
        let code = scenario::cli(&args[i + 1..]);
        let _ = std::fs::remove_dir_all(&scratch);
        std::process::exit(code);
    }
    // Fast and debug builds can run side by side. Release builds share one OS lock.
    let _instance = if brand::IS_DEV {
        None
    } else {
        let path = default_config_path();
        let dir = path.parent().unwrap_or(Path::new("."));
        match instance::acquire(dir) {
            Ok(lock) => Some(lock),
            Err(e) => {
                eprintln!("Streamerz could not acquire its instance lock: {e}");
                return;
            }
        }
    };
    if !brand::IS_DEV {
        let target = resolve_config_path(default_config_path());
        let chosen = std::fs::read_to_string(location_file())
            .ok()
            .map(|s| resolve_config_path(PathBuf::from(s.trim())))
            .filter(|p| p.is_file());
        if let Some(source) = chosen {
            if let Err(e) = storage::migrate_selected(&source, &target) {
                eprintln!("Streamerz could not migrate settings: {e}");
                return;
            }
        } else if let Some(source) = portable_config() {
            if let Err(e) = storage::migrate(&source, &target) {
                eprintln!("Streamerz could not migrate settings: {e}");
                return;
            }
        }
    }
    if brand::IS_DEV {
        // Resolve the source before selecting the private path. The source is only read.
        let source = default_config_path();
        let _ = DEV_SOURCE.set(source.clone());
        let dir = match prepare_dev(&source) {
            Ok(dir) => dir,
            Err(e) => {
                eprintln!("Streamerz could not create development settings: {e}");
                return;
            }
        };
        let lock = match instance::acquire(&dir) {
            Ok(lock) => lock,
            Err(e) => {
                let _ = std::fs::remove_dir_all(&dir);
                eprintln!("Streamerz could not lock development settings: {e}");
                return;
            }
        };
        if let Ok(mut held) = DEV_LOCK.lock() {
            *held = Some(lock);
        } else {
            eprintln!("Streamerz could not hold development settings lock");
            return;
        }
        secrets::init(&storage::key_source(&source));
        std::env::set_var("STREAMERZ_CONFIG", dir.join("streamerz.json"));
        let _ = DEV_DIR.set(dir);
    }
    let app = ui::app();
    if !brand::IS_DEV {
        update::tidy();
    }
    secrets::init(&storage::key_source(&config_path()));
    crash::start();
    say(version());
    // To try the crash report by hand (see DEVELOPING).
    if std::env::var_os("STREAMERZ_CRASH_TEST").is_some() {
        panic!("crash test");
    }
    backup::on_start();
    let mut cfg = load_from(&config_path());
    tiktok_bridge::restore_pairing(&mut cfg);
    MEMORY_SAVER.store(cfg.memory_saver, std::sync::atomic::Ordering::Relaxed);
    cleanup_first_words();
    if brand::IS_DEV {
        // A local server gives each development instance its own endpoint for the MCP tools.
        let requested = std::env::var("STREAMERZ_DEV_PORT")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(8090);
        let listener = match std::net::TcpListener::bind(("127.0.0.1", requested))
            .or_else(|_| std::net::TcpListener::bind(("127.0.0.1", 0)))
        {
            Ok(listener) => listener,
            Err(e) => {
                eprintln!("Streamerz could not choose a development port: {e}");
                cleanup_dev();
                return;
            }
        };
        cfg.websockets.port = match listener.local_addr() {
            Ok(addr) => addr.port(),
            Err(e) => {
                eprintln!("Streamerz could not read its development port: {e}");
                cleanup_dev();
                return;
            }
        };
        if let Err(e) = listener.set_nonblocking(true) {
            eprintln!("Streamerz could not prepare the development server: {e}");
            cleanup_dev();
            return;
        }
        if let Ok(mut reserved) = DEV_LISTENER.lock() {
            *reserved = Some(listener);
        } else {
            eprintln!("Streamerz could not reserve the development port");
            cleanup_dev();
            return;
        }
        cfg.websockets.address = "127.0.0.1".into();
        cfg.websockets.auto_start = true;
        cfg.spotify.enabled = false;
        cfg.spotify.local_monitor = false;
        cfg.http.auto_start = false;
        cfg.udp.auto_start = false;
        for connection in &mut cfg.websockets.connections {
            connection.insert("autoConnect".into(), false.into());
        }
        for server in &mut cfg.websockets.servers {
            server.insert("autoStart".into(), false.into());
        }
        cfg.instance_id.clear();
        if let Err(e) = save_to(&config_path(), &cfg) {
            eprintln!("Streamerz could not save development settings: {e}");
            cleanup_dev();
            return;
        }
    }
    notify::configure(cfg.notifications);
    i18n::set(&cfg.locale);
    set_debug_log(cfg.debug_log);
    fmt::set_shown_as(&cfg.date_format, &cfg.time_format);
    say(format!("config: {}", config_path().display()));
    let (ui_tx, ui_rx) = fltk::app::channel();
    // ponytail: one runtime thread is plenty for a few sockets, and lighter than one per core.
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    rt.spawn(tray::run(ui_tx));
    let cfg = Arc::new(watch::channel(cfg).0);
    let engine_cfg = cfg.clone();
    let (cmd_tx, mut cmd_rx) = tokio::sync::mpsc::unbounded_channel::<engine::UiCmd>();
    let crash_tx = cmd_tx.clone();
    // The engine isn't Send (one thread, no locks): it lives in a LocalSet on the runtime thread.
    std::thread::spawn(move || {
        tokio::task::LocalSet::new().block_on(&rt, async move {
            let e = engine::Engine::new(engine_cfg.clone(), |c| {
                save_later(c);
            });
            engine::set_current(&e, crash_tx);
            engine::Engine::forget_saved_chat_history();
            e.load_community();
            e.add_builtins();
            if !brand::IS_DEV {
                e.plugins_start();
                e.auto_connect();
            }
            if !brand::IS_DEV && secrets::is_locked() {
                engine::spawn_secret_recovery(e.clone());
            }
            tokio::task::spawn_local(servers::run(e.clone()));
            if !brand::IS_DEV {
                e.started();
            }
            tokio::task::spawn_local(crash::offer());
            e.watchers();
            if !brand::IS_DEV {
                tokio::task::spawn_local(update::watch(engine_cfg.clone()));
                e.timers();
            }
            #[cfg(unix)]
            tokio::task::spawn_local(ended(e.clone()));
            while let Some(f) = cmd_rx.recv().await {
                f(&e);
            }
            std::future::pending::<()>().await
        })
    });
    ui::run(app, cfg, cmd_tx, ui_tx, ui_rx);
    cleanup_dev();
}

/// Tests that use the shared log run one at a time.
#[cfg(test)]
static TEST_LOCK: Mutex<()> = Mutex::new(());

/// Whether the tests run under Wine (its ntdll has `wine_get_version`), for the few things Wine does
/// differently from Windows.
#[cfg(all(test, windows))]
fn under_wine() -> bool {
    use windows_sys::Win32::System::LibraryLoader::{GetModuleHandleW, GetProcAddress};
    let ntdll: Vec<u16> = "ntdll.dll".encode_utf16().chain([0]).collect();
    // SAFETY: a NUL-terminated module name, and a name looked up in a module that is always loaded.
    unsafe { GetProcAddress(GetModuleHandleW(ntdll.as_ptr()), c"wine_get_version".as_ptr().cast()).is_some() }
}

#[cfg(test)]
mod tests {
    #[test]
    fn development_extensions_outlive_disposable_settings_and_keep_release_separate() {
        use std::path::Path;
        let (source, cache) = (
            Path::new("/settings/streamerz.json"),
            Some(Path::new("/cache/streamerz-dev")),
        );
        let a = super::extension_folder_for(Path::new("/scratch/run-a/streamerz.json"), Some(source), cache);
        let b = super::extension_folder_for(Path::new("/scratch/run-b/streamerz.json"), Some(source), cache);
        // Every run of the same settings: one folder, in the cache, never beside the production settings.
        assert_eq!(a, b);
        assert!(a.starts_with("/cache/streamerz-dev") && a.ends_with("tiktok-extension"));
        assert!(!a.starts_with("/settings"));
        // A test's scratch settings get another.
        let scratch = Path::new("/tmp/scratch/streamerz.json");
        assert_ne!(a, super::extension_folder_for(scratch, Some(scratch), cache));
        // A release build: beside its settings.
        assert_eq!(
            super::extension_folder_for(source, None, cache),
            Path::new("/settings/tiktok-extension")
        );
    }

    #[test]
    fn shutdown_waits_for_the_pending_save() {
        let queue = std::sync::Arc::new(std::sync::Mutex::new(super::SaveQueue {
            busy: true,
            pending: None,
        }));
        let done = std::sync::Arc::new(std::sync::Condvar::new());
        let (tx, rx) = std::sync::mpsc::channel();
        let worker_queue = queue.clone();
        let worker_done = done.clone();
        let worker = std::thread::spawn(move || {
            super::wait_on_saves(&worker_queue, &worker_done);
            tx.send(()).unwrap();
        });
        assert!(rx.recv_timeout(std::time::Duration::from_millis(50)).is_err());
        queue.lock().unwrap().busy = false;
        done.notify_all();
        rx.recv_timeout(std::time::Duration::from_secs(2)).unwrap();
        worker.join().unwrap();
    }
    use super::*;

    /// Start with Windows writes the Run value Windows reads, and turning it off removes it (twice is fine).
    #[cfg(windows)]
    #[test]
    fn run_at_login_sets_and_removes_the_value() {
        use windows_sys::Win32::System::Registry::{RegGetValueW, HKEY_CURRENT_USER, RRF_RT_REG_SZ};
        let w = |s: &str| s.encode_utf16().chain([0]).collect::<Vec<u16>>();
        let read = || {
            let mut buf = [0u16; 512];
            let mut size = (buf.len() * 2) as u32;
            // SAFETY: NUL-terminated names; the buffer and its size in bytes.
            let r = unsafe {
                RegGetValueW(
                    HKEY_CURRENT_USER,
                    w(RUN_KEY).as_ptr(),
                    w("streamerz-test").as_ptr(),
                    RRF_RT_REG_SZ,
                    std::ptr::null_mut(),
                    buf.as_mut_ptr().cast(),
                    &mut size,
                )
            };
            (r == 0).then(|| String::from_utf16_lossy(&buf[..(size as usize / 2).saturating_sub(1)]))
        };
        run_at_login("streamerz-test", Some(r#""C:\Program Files\Streamerz\Streamerz.exe""#)).unwrap();
        assert_eq!(read().as_deref(), Some(r#""C:\Program Files\Streamerz\Streamerz.exe""#));
        run_at_login("streamerz-test", None).unwrap();
        assert_eq!(read(), None);
        run_at_login("streamerz-test", None).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn stale_cleanup_keeps_locked_and_foreign_directories() {
        let root = std::env::temp_dir().join(format!("streamerz-clean-test-{}", std::process::id()));
        std::fs::create_dir_all(&root).unwrap();
        let active = root.join("streamerz-dev-active");
        let stale = root.join("streamerz-dev-stale");
        let foreign = root.join("streamerz-dev-foreign");
        std::fs::create_dir_all(&foreign).unwrap();
        let held = instance::acquire(&active).unwrap();
        drop(instance::acquire(&stale).unwrap());
        let old = std::time::SystemTime::now() - std::time::Duration::from_secs(2 * 24 * 60 * 60);
        for dir in [&active, &stale, &foreign] {
            std::fs::File::open(dir)
                .unwrap()
                .set_times(std::fs::FileTimes::new().set_modified(old))
                .unwrap();
        }
        cleanup_stale_dev(&root);
        assert!(active.is_dir());
        assert!(!stale.exists());
        assert!(foreign.is_dir());
        drop(held);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn development_copy_is_independent() {
        let root = std::env::temp_dir().join(format!("streamerz-copy-test-{}", std::process::id()));
        std::fs::create_dir_all(&root).unwrap();
        let source = root.join("streamerz.json");
        std::fs::write(&source, b"original").unwrap();
        std::fs::write(root.join("community.json"), b"community").unwrap();
        let first = prepare_dev(&source).unwrap();
        let second = prepare_dev(&source).unwrap();
        assert_ne!(first, second);
        std::fs::write(first.join("streamerz.json"), b"changed").unwrap();
        assert_eq!(std::fs::read(&source).unwrap(), b"original");
        assert_eq!(std::fs::read(second.join("streamerz.json")).unwrap(), b"original");
        assert_eq!(std::fs::read(first.join("community.json")).unwrap(), b"community");
        std::fs::remove_dir_all(first).unwrap();
        std::fs::remove_dir_all(second).unwrap();
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn redacts_credentials() {
        assert_eq!(
            redact(r#"token request failed (HTTP 400): {"access_token":"abc.DEF-1","refresh_token": "x/y=", "ok":1}"#),
            r#"token request failed (HTTP 400): {"access_token":"***","refresh_token": "***", "ok":1}"#
        );
        assert_eq!(
            redact("GET /revoke?client_id=c&token=t%2B1&x=2"),
            "GET /revoke?client_id=c&token=***&x=2"
        );
        assert_eq!(redact("Authorization: Bearer abc123"), "Authorization: Bearer ***");
        assert_eq!(
            redact(r#"{"accessToken":"private","refreshToken":"secret"}"#),
            r#"{"accessToken":"***","refreshToken":"***"}"#
        );
        assert_eq!(
            redact("Twitch: refreshed the bot token"),
            "Twitch: refreshed the bot token"
        );
        assert_eq!(redact("cookie sessionid=zz; tt=1"), "cookie sessionid=***; tt=1");
    }

    #[test]
    fn save_keeps_bak_and_unknown_fields() {
        let dir = std::env::temp_dir().join(format!("streamerz-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("streamerz.json");
        // Missing: defaults, written out.
        assert!(load_from(&path).dark);
        assert!(path.exists() && !with(&path, ".bak").exists());
        // Unknown fields survive a save; the previous version becomes the .bak.
        std::fs::write(&path, r#"{"dark":false,"future":[1,2]}"#).unwrap();
        let c = load_from(&path);
        assert!(!c.dark && c.rest["future"] == serde_json::json!([1, 2]));
        assert!(std::fs::read_to_string(with(&path, ".bak")).unwrap().contains("future"));
        assert!(std::fs::read_to_string(&path).unwrap().contains("future"));
        assert!(!with(&path, ".tmp").exists());
        // Broken: the .bak is used and the broken file left alone.
        std::fs::write(&path, "{broken").unwrap();
        assert!(!load_from(&path).dark);
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "{broken");
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
