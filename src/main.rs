//! Streamerz: actions, triggers and the servers that tools already use, for Twitch, Kick, YouTube,
//! OBS and Streamlabs Desktop, running on ~nothing.

mod backup;
mod brand;
mod cdp;
mod commands;
mod crash;
pub mod decks;
pub mod donations;
mod edit;
mod emote_events;
pub mod engine;
pub mod fileio;
mod fmt;
pub mod hotkeys;
pub mod http;
mod i18n;
pub mod inputs;
pub mod kick;
mod math;
mod mcp;
pub mod model;
mod notify;
mod oauth;
pub mod obs;
pub mod remote;
mod secrets;
pub mod servers;
mod slobs;
mod templates;
pub mod tiktok;
mod tray;
pub mod twitch;
pub mod types;
mod ui;
mod update;
mod vars;
mod webcast;
pub mod youtube;

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::collections::VecDeque;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};
use tokio::sync::watch;

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
    autostart: bool,
    /// Keep [DEBUG] lines in the log.
    debug_log: bool,
    /// Actions, groups and queues, in Streamer.bot's actions.json shape.
    actions: model::ActionsFile,
    /// Persisted globals, and persisted user globals by user ("platform:id", then name).
    globals: model::Obj,
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
            autostart: false,
            debug_log: false,
            actions: model::ActionsFile::default(),
            globals: Map::new(),
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
            counters: Map::new(),
            quotes: Vec::new(),
            twitch: twitch::TwitchConfig::default(),
            obs: obs::ObsConfig::default(),
            slobs: slobs::Settings::default(),
            kick: kick::KickConfig::default(),
            youtube: youtube::YouTubeConfig::default(),
            tiktok: tiktok::TikTokConfig::default(),
            donations: donations::DonationsConfig::default(),
            builtins: false,
            window: WindowState::default(),
            updates: update::Updates::default(),
            plugins: Default::default(),
            date_format: String::new(),
            time_format: String::new(),
            rest: Map::new(),
        }
    }
}

/// The window as it was left: its tab, the chat's view and its size (0: the default).
#[derive(Clone, Default, Serialize, Deserialize)]
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
    /// Platforms where the individual chat box sends as broadcaster instead of bot-with-fallback.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub chat_as_broadcaster: Vec<String>,
    /// Selected target platform in "All together" chat view's input bar.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub chat_all_platform: String,
    /// Starred favorite emotes and emoji in the picker: (name, image URL).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub favorite_emotes: Vec<(String, String)>,
    /// The sidebar shows only its icons.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub nav_collapsed: bool,
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
        if self.tiktok.channel.trim().is_empty() {
            if let Some(b) = &self.tiktok.broadcaster {
                self.tiktok.channel = b.username.trim_start_matches('@').to_string();
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
    let _ = writeln!(std::io::stdout(), "[{}] {line}", level.tag());
    crash::line(level, &line);
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
    e.save_all();
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
    if std::env::var_os("APPIMAGE").is_some() || exe.starts_with("/tmp") || exe.starts_with("/usr") {
        return None;
    }
    Some(exe.parent()?.join("streamerz.json")).filter(|p| p.is_file())
}

/// Nothing says where the settings are and there are none yet: the user should choose.
pub(crate) fn first_run() -> bool {
    explicit_config().is_none()
        && !location_file().is_file()
        && portable_config().is_none()
        && !default_config_path().exists()
}

/// Where the user chose to keep the settings (the usual folder needs no note).
pub(crate) fn remember_location(file: &Path) -> std::io::Result<()> {
    let usual = default_config_path();
    if file == usual {
        return Ok(());
    }
    if let Some(dir) = file.parent() {
        std::fs::create_dir_all(dir)?;
    }
    std::fs::create_dir_all(usual.parent().unwrap_or(Path::new(".")))?;
    std::fs::write(location_file(), file.display().to_string())
}

pub(crate) fn config_path() -> PathBuf {
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
    // The window and the engine both save: one at a time.
    static SAVING: Mutex<()> = Mutex::new(());
    let _one = SAVING.lock();
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let tmp = with(path, ".tmp");
    let mut v = serde_json::to_value(c).unwrap_or_default();
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
    let r = save_to(&config_path(), c);
    if let Err(e) = &r {
        error(format!("can't save streamerz.json ({e})"));
    }
    r
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
    let _ = save_to(path, &c);
    c
}

/// Start with Windows: a value under the user's Run registry key.
#[cfg(windows)]
fn autostart(on: bool) -> std::io::Result<()> {
    use windows_sys::Win32::System::Registry::{RegDeleteKeyValueW, RegSetKeyValueW, HKEY_CURRENT_USER, REG_SZ};
    let w = |s: &str| s.encode_utf16().chain([0]).collect::<Vec<u16>>();
    let (key, name) = (w(r"Software\Microsoft\Windows\CurrentVersion\Run"), w("streamerz"));
    let exe = w(&format!(
        "\"{}\"",
        update::exe().ok_or(std::io::ErrorKind::NotFound)?.display()
    ));
    // SAFETY: NUL-terminated UTF-16 strings that outlive the calls; size is in bytes.
    let err = unsafe {
        if on {
            let size = (exe.len() * 2) as u32;
            RegSetKeyValueW(
                HKEY_CURRENT_USER,
                key.as_ptr(),
                name.as_ptr(),
                REG_SZ,
                exe.as_ptr().cast(),
                size,
            )
        } else {
            RegDeleteKeyValueW(HKEY_CURRENT_USER, key.as_ptr(), name.as_ptr())
        }
    };
    // 2 = not found: already off.
    match err {
        0 | 2 => Ok(()),
        e => Err(std::io::Error::from_raw_os_error(e as i32)),
    }
}

/// Start at login: an XDG autostart entry.
#[cfg(not(windows))]
fn autostart(on: bool) -> std::io::Result<()> {
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

fn main() {
    // An MCP server for AI assistants, driving the running instance (mcp.rs); no window.
    if std::env::args_os().any(|a| a == "--mcp") {
        mcp::run();
        return;
    }
    let app = ui::app();
    if first_run() {
        let usual = default_config_path();
        let file = ui::dialogs::choose_location(&usual);
        if let Err(e) = remember_location(&file) {
            eprintln!("can't remember where the settings are: {e}");
        }
    }
    update::tidy();
    secrets::init(&config_path());
    crash::start();
    say(version());
    // To try the crash report by hand (see DEVELOPING).
    if std::env::var_os("STREAMERZ_CRASH_TEST").is_some() {
        panic!("crash test");
    }
    backup::on_start();
    let cfg = load_from(&config_path());
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
                let _ = save(c);
            });
            engine::set_current(&e, crash_tx);
            engine::Engine::forget_saved_chat_history();
            e.load_community();
            e.add_builtins();
            e.plugins_start();
            e.auto_connect();
            if secrets::is_locked() {
                engine::spawn_secret_recovery(e.clone());
            }
            tokio::task::spawn_local(servers::run(e.clone()));
            e.started();
            tokio::task::spawn_local(crash::offer());
            tokio::task::spawn_local(update::watch(engine_cfg.clone()));
            e.timers();
            #[cfg(unix)]
            tokio::task::spawn_local(ended(e.clone()));
            while let Some(f) = cmd_rx.recv().await {
                f(&e);
            }
            std::future::pending::<()>().await
        })
    });
    ui::run(app, cfg, cmd_tx, ui_tx, ui_rx);
}

/// Tests that use the shared log run one at a time.
#[cfg(test)]
static TEST_LOCK: Mutex<()> = Mutex::new(());

#[cfg(test)]
mod tests {
    use super::*;

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
