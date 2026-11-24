//! Sounds Streamerz plays itself (Play Sound and the rest of Core > Sounds).
//!
//! Nothing is open until a sound plays. The first sound on a device starts that device's thread,
//! which mixes its sounds and writes them to the system (PulseAudio or PipeWire on Linux, WASAPI on
//! Windows); a write blocks until the system takes it, so the thread needs no timer. When its last
//! sound ends, the thread drains the device, closes it and ends. Files are decoded as they play.

mod decode;
#[cfg(target_os = "linux")]
mod helper;
mod mixer;
#[cfg(target_os = "linux")]
mod pulse;
#[cfg(windows)]
mod wasapi;

#[cfg(test)]
pub use decode::sine_wav;
pub use decode::{info, EXTENSIONS};
pub use mixer::{End, Ended};

use mixer::{Mixer, Ramp, Voice};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, OnceLock};
use tokio::sync::oneshot;

/// The mix's rate and channels: every sound becomes this, the system turns it into the device's.
pub const RATE: u32 = 48_000;
pub const CHANNELS: usize = 2;
/// Frames written at a time: 20 ms.
const CHUNK: usize = 960;
/// Sounds at once, on every device together, when the Output setting says nothing else.
pub const MAX_AT_ONCE: usize = 16;

/// The Output settings of the Sounds page (`sounds` in the settings file).
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Settings {
    /// What Default means for Streamerz (empty: the system's default).
    pub device: String,
    /// Where previews play (Play on the Media tab, the Library, a form): the streamer hears them, OBS
    /// doesn't capture them when it is another device. Empty: the same as Default.
    pub monitor: String,
    /// Every sound's volume, 0 to 100.
    pub master: f64,
    /// Sounds at once, on every device together.
    pub max_at_once: usize,
    /// The Board's pads, in order.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub board: Vec<Pad>,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            device: String::new(),
            monitor: String::new(),
            master: 100.0,
            max_at_once: MAX_AT_ONCE,
            board: Vec::new(),
        }
    }
}

fn settings() -> MutexGuard<'static, Settings> {
    static SETTINGS: OnceLock<Mutex<Settings>> = OnceLock::new();
    SETTINGS
        .get_or_init(|| Mutex::new(Settings::default()))
        .lock()
        .unwrap_or_else(|e| e.into_inner())
}

/// A pad of the Sounds page's Board: a sound one press away.
#[derive(Clone, Debug, Default, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Pad {
    pub id: String,
    pub label: String,
    pub file: String,
    /// 0 to 100 (0 is read as 100: a pad nobody set plays at full volume).
    pub volume: f64,
    /// Empty: Default.
    pub device: String,
}

/// Whether the Sounds page is open: then a sound that starts or ends says so to the window.
static WATCHED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

pub fn watch(on: bool) {
    WATCHED.store(on, Ordering::Relaxed);
}

/// What plays changed: the Sounds page reads it again (nothing is told while it is closed).
pub(crate) fn changed() {
    if WATCHED.load(Ordering::Relaxed) {
        crate::host::changed();
    }
}

/// Use these Output settings from now on (the settings file's, at start and when they change).
pub fn configure(s: &Settings) {
    if (s.master - settings().master).abs() > f64::EPSILON || master() != volume(s.master) {
        set_master(volume(s.master), 0);
    }
    *settings() = Settings {
        max_at_once: s.max_at_once.clamp(1, 64),
        ..s.clone()
    };
}

/// Where the mix goes. `write` blocks until the frames are taken.
pub trait Output {
    fn write(&mut self, frames: &[f32]) -> Result<(), String>;
    /// Wait for what was written to be heard.
    fn drain(&mut self);
}

/// Opens a device by name ("" for the system's default).
pub type Opener = fn(&str) -> Result<Box<dyn Output>, String>;

fn system(device: &str) -> Result<Box<dyn Output>, String> {
    #[cfg(target_os = "linux")]
    return helper::open(device);
    #[cfg(windows)]
    return wasapi::open(device);
    #[allow(unreachable_code)]
    {
        let _ = device;
        Err("sounds aren't played on this system".into())
    }
}

/// An output that takes the frames as fast as they would be heard and keeps nothing: tests and
/// offline scenarios.
struct Silent;

/// Frames the silent output took, for tests.
static SILENT_FRAMES: AtomicU64 = AtomicU64::new(0);

impl Output for Silent {
    fn write(&mut self, frames: &[f32]) -> Result<(), String> {
        let n = (frames.len() / CHANNELS) as u64;
        SILENT_FRAMES.fetch_add(n, Ordering::Relaxed);
        std::thread::sleep(std::time::Duration::from_micros(n * 1_000_000 / RATE as u64));
        Ok(())
    }
    fn drain(&mut self) {}
}

fn silent(_: &str) -> Result<Box<dyn Output>, String> {
    Ok(Box::new(Silent))
}

struct Player {
    devices: HashMap<String, Arc<Mutex<Mixer>>>,
    opener: Opener,
}

fn player() -> MutexGuard<'static, Player> {
    static PLAYER: OnceLock<Mutex<Player>> = OnceLock::new();
    PLAYER
        .get_or_init(|| {
            Mutex::new(Player {
                devices: HashMap::new(),
                opener: if cfg!(test) { silent } else { system },
            })
        })
        .lock()
        .unwrap_or_else(|e| e.into_inner())
}

fn lock(m: &Mutex<Mixer>) -> MutexGuard<'_, Mixer> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

/// The audio process (`Streamerz --sound-output <device>`): plays what Streamerz pipes to it, so the
/// system's audio library is loaded there and not in Streamerz. Its exit code.
pub fn output_main(device: &str) -> i32 {
    #[cfg(target_os = "linux")]
    return helper::run(device);
    #[allow(unreachable_code)]
    {
        let _ = device;
        eprintln!("the sound output runs in Streamerz itself on this system");
        1
    }
}

/// Sounds are played silently from now on (offline scenarios, tests).
pub fn silence() {
    player().opener = silent;
}

static NEXT: AtomicU64 = AtomicU64::new(0);
/// The master volume, as f32 bits.
static MASTER: AtomicU32 = AtomicU32::new(0x3f80_0000);

/// The device a sub-action names, as the player keys it: "" is the default.
pub fn device_key(device: &str) -> String {
    let d = device.trim();
    if d.is_empty() || d.eq_ignore_ascii_case("Application Default") || d.eq_ignore_ascii_case("Default") {
        // Default is the device the Output settings chose (Set Sound Device), else the system's.
        settings().device.clone()
    } else if d.eq_ignore_ascii_case("Monitor") {
        let m = settings().monitor.clone();
        if m.is_empty() {
            settings().device.clone()
        } else {
            m
        }
    } else {
        d.to_string()
    }
}

/// A volume as a sub-action holds it: 0 to 100 (Streamer.bot's slider), or 0 to 1. Kept in 0..=1.
pub fn volume(value: f64) -> f32 {
    let v = if value > 1.0 { value / 100.0 } else { value };
    if v.is_finite() {
        v.clamp(0.0, 1.0) as f32
    } else {
        1.0
    }
}

/// A sound that started.
pub struct Started {
    pub id: u64,
    pub duration_ms: Option<u64>,
    pub ended: oneshot::Receiver<Ended>,
}

/// Start a sound. It plays on its own; `ended` says when and how it ended.
pub fn play(file: &Path, name: &str, device: &str, volume: f32) -> Result<Started, String> {
    let source = decode::Source::open(file)?;
    let duration_ms = source.info.duration_ms;
    let (done, ended) = oneshot::channel();
    let id = NEXT.fetch_add(1, Ordering::Relaxed) + 1;
    let voice = Voice {
        id,
        name: name.to_string(),
        file: file.display().to_string(),
        source,
        volume: Ramp::new(volume),
        paused: false,
        stopping: false,
        played: 0,
        done: Some(done),
    };
    let key = device_key(device);
    let mut p = player();
    let busy: usize = p.devices.values().map(|m| lock(m).voices.len()).sum();
    let most = settings().max_at_once;
    if busy >= most {
        return Err(format!("{most} sounds are already playing"));
    }
    let opener = p.opener;
    let mixer = p
        .devices
        .entry(key.clone())
        .or_insert_with(|| {
            let mixer = Arc::new(Mutex::new(Mixer::new(master())));
            let (k, m) = (key.clone(), mixer.clone());
            let spawned = std::thread::Builder::new()
                .name("sound".into())
                .stack_size(512 * 1024)
                .spawn(move || run(k, m, opener));
            if let Err(e) = spawned {
                lock(&mixer).fail_all(&format!("cannot start the sound thread: {e}"));
            }
            mixer
        })
        .clone();
    lock(&mixer).voices.push(voice);
    drop(p);
    changed();
    Ok(Started { id, duration_ms, ended })
}

/// One device's thread: open it, write the mix until no sound is left, drain, close. A sound that
/// comes while it drains opens the device again.
fn run(key: String, mixer: Arc<Mutex<Mixer>>, opener: Opener) {
    let mut buf = vec![0f32; CHUNK * CHANNELS];
    loop {
        let opened = opener(&key).or_else(|e| if key.is_empty() { Err(e) } else { opener("") });
        let mut out = match opened {
            Ok(out) => out,
            Err(e) => return give_up(&key, &mixer, &e),
        };
        loop {
            {
                let mut m = lock(&mixer);
                if m.is_empty() {
                    break;
                }
                m.render(&mut buf);
            }
            if let Err(e) = out.write(&buf) {
                return give_up(&key, &mixer, &e);
            }
        }
        out.drain();
        drop(out);
        let mut p = player();
        if lock(&mixer).is_empty() {
            if p.devices.get(&key).is_some_and(|m| Arc::ptr_eq(m, &mixer)) {
                p.devices.remove(&key);
            }
            let last = p.devices.is_empty();
            drop(p);
            drop(buf);
            if last {
                give_back();
            }
            return;
        }
    }
}

/// The memory the decoders used goes back to the system once nothing plays: glibc keeps a thread's
/// freed memory in its arena otherwise, for the life of the process.
fn give_back() {
    #[cfg(all(target_os = "linux", target_env = "gnu"))]
    // SAFETY: malloc_trim only releases memory the allocator holds free.
    unsafe {
        libc::malloc_trim(0);
    }
}

fn give_up(key: &str, mixer: &Arc<Mutex<Mixer>>, error: &str) {
    let mut p = player();
    if p.devices.get(key).is_some_and(|m| Arc::ptr_eq(m, mixer)) {
        p.devices.remove(key);
    }
    lock(mixer).fail_all(error);
}

fn matches(voice: &Voice, name: Option<&str>) -> bool {
    name.is_none_or(|n| voice.name.eq_ignore_ascii_case(n))
}

/// Stop the sounds of a name (every sound for None), at once or over `fade_ms`. How many.
pub fn stop(name: Option<&str>, fade_ms: u64) -> usize {
    let p = player();
    p.devices
        .values()
        .map(|m| lock(m).stop(|v| matches(v, name), fade_ms))
        .sum()
}

/// Stop one sound by its id (the Sounds page), at once or over `fade_ms`.
pub fn stop_id(id: u64, fade_ms: u64) -> usize {
    let p = player();
    p.devices.values().map(|m| lock(m).stop(|v| v.id == id, fade_ms)).sum()
}

/// Pause or resume one sound by its id.
pub fn pause_id(id: u64, paused: bool) {
    for m in player().devices.values() {
        for v in lock(m).voices.iter_mut().filter(|v| v.id == id) {
            v.paused = paused;
        }
    }
    changed();
}

/// Change one sound's volume by its id, over `fade_ms`.
pub fn set_volume_id(id: u64, volume: f32, fade_ms: u64) {
    for m in player().devices.values() {
        for v in lock(m).voices.iter_mut().filter(|v| v.id == id && !v.stopping) {
            v.volume.to(volume, fade_ms);
        }
    }
    changed();
}

/// Pause or resume the sounds of a name (every sound for None). How many.
pub fn pause(name: Option<&str>, paused: bool) -> usize {
    let p = player();
    let mut n = 0;
    for m in p.devices.values() {
        for v in lock(m).voices.iter_mut().filter(|v| matches(v, name)) {
            v.paused = paused;
            n += 1;
        }
    }
    n
}

/// Change the volume of the sounds of a name (every sound for None), over `fade_ms`. How many.
pub fn set_volume(name: Option<&str>, volume: f32, fade_ms: u64) -> usize {
    let p = player();
    let mut n = 0;
    for m in p.devices.values() {
        for v in lock(m).voices.iter_mut().filter(|v| matches(v, name) && !v.stopping) {
            v.volume.to(volume, fade_ms);
            n += 1;
        }
    }
    n
}

pub fn master() -> f32 {
    f32::from_bits(MASTER.load(Ordering::Relaxed))
}

/// The master volume (every sound, every device), over `fade_ms`.
pub fn set_master(volume: f32, fade_ms: u64) {
    MASTER.store(volume.to_bits(), Ordering::Relaxed);
    for m in player().devices.values() {
        lock(m).master.to(volume, fade_ms);
    }
}

/// A sound playing now.
#[derive(Debug, Clone, PartialEq)]
pub struct Playing {
    pub id: u64,
    pub name: String,
    pub file: String,
    pub device: String,
    pub position_ms: u64,
    pub duration_ms: Option<u64>,
    pub volume: f32,
    pub paused: bool,
}

/// What plays now, on every device.
pub fn playing() -> Vec<Playing> {
    let p = player();
    let mut out = Vec::new();
    for (device, m) in &p.devices {
        for v in &lock(m).voices {
            out.push(Playing {
                id: v.id,
                name: v.name.clone(),
                file: v.file.clone(),
                device: device.clone(),
                position_ms: v.position_ms(),
                duration_ms: v.source.info.duration_ms,
                volume: v.volume.target,
                paused: v.paused,
            });
        }
    }
    out.sort_by_key(|p| p.id);
    out
}

/// Devices with a thread now (one is open while a sound plays on it).
pub fn open_devices() -> usize {
    player().devices.len()
}

/// A path as a sub-action or a request holds it: a relative one is beside `streamerz.json`.
pub fn resolve(path: &str) -> PathBuf {
    let p = PathBuf::from(path.trim());
    if p.is_absolute() {
        return p;
    }
    crate::config_path().parent().map_or(p.clone(), |dir| dir.join(&p))
}

/// The names of the outputs a sound can be played on.
pub fn devices() -> Vec<String> {
    outputs().into_iter().map(|(name, _)| name).collect()
}

/// The outputs a sound can be played on: the name a sub-action keeps and the one shown. Windows
/// lists its endpoints; on Linux PulseAudio (or PipeWire, which answers it) lists its sinks through
/// `pactl`, asked only when a list is wanted.
pub fn outputs() -> Vec<(String, String)> {
    #[cfg(windows)]
    return wasapi::devices().into_iter().map(|d| (d.clone(), d)).collect();
    #[cfg(target_os = "linux")]
    return pactl_sinks();
    #[allow(unreachable_code)]
    Vec::new()
}

/// Waits until the system says an output came or went (PulseAudio's `pactl subscribe` on Linux, the
/// endpoint notifications on Windows), then returns true; false when it can't listen. Nothing is
/// asked in between: no poll.
pub struct DeviceEvents {
    #[cfg(target_os = "linux")]
    lines: tokio::io::Lines<tokio::io::BufReader<tokio::process::ChildStdout>>,
    #[cfg(target_os = "linux")]
    _child: tokio::process::Child,
    #[cfg(windows)]
    rx: tokio::sync::mpsc::UnboundedReceiver<()>,
    #[cfg(windows)]
    _watch: wasapi::DeviceWatch,
}

impl DeviceEvents {
    pub fn start() -> Result<Self, String> {
        #[cfg(target_os = "linux")]
        {
            let mut cmd = std::process::Command::new("pactl");
            cmd.arg("subscribe");
            let mut child = tokio::process::Command::from(crate::sandbox::on_host(cmd))
                .stdout(std::process::Stdio::piped())
                .stderr(std::process::Stdio::null())
                .kill_on_drop(true)
                .spawn()
                .map_err(|e| format!("pactl: {e}"))?;
            let out = child.stdout.take().ok_or("pactl gave no output")?;
            use tokio::io::AsyncBufReadExt;
            Ok(Self {
                lines: tokio::io::BufReader::new(out).lines(),
                _child: child,
            })
        }
        #[cfg(windows)]
        {
            let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
            let watch = wasapi::watch(tx)?;
            Ok(Self { rx, _watch: watch })
        }
        #[cfg(not(any(target_os = "linux", windows)))]
        Err("sound outputs aren't followed on this system".into())
    }

    /// The next time an output came or went.
    pub async fn next(&mut self) -> bool {
        #[cfg(target_os = "linux")]
        loop {
            match self.lines.next_line().await {
                Ok(Some(line)) if is_sink_event(&line) => return true,
                Ok(Some(_)) => continue,
                _ => return false,
            }
        }
        #[cfg(windows)]
        return self.rx.recv().await.is_some();
        #[allow(unreachable_code)]
        false
    }
}

/// A line of `pactl subscribe` about an output that came or went.
#[cfg(any(target_os = "linux", test))]
fn is_sink_event(line: &str) -> bool {
    line.contains(" on sink #") && (line.contains("'new'") || line.contains("'remove'"))
}

#[cfg(target_os = "linux")]
fn pactl_sinks() -> Vec<(String, String)> {
    let mut cmd = std::process::Command::new("pactl");
    cmd.args(["-f", "json", "list", "sinks"]);
    let Ok(out) = crate::sandbox::on_host(cmd).output() else {
        return Vec::new();
    };
    sinks_from_json(&String::from_utf8_lossy(&out.stdout))
}

/// `pactl -f json list sinks`: each sink's name and description.
#[cfg(any(target_os = "linux", test))]
fn sinks_from_json(text: &str) -> Vec<(String, String)> {
    let list: Vec<serde_json::Value> = serde_json::from_str(text).unwrap_or_default();
    list.iter()
        .filter_map(|s| {
            let name = s["name"].as_str()?.to_string();
            let shown = s["description"]
                .as_str()
                .filter(|d| !d.is_empty())
                .unwrap_or(&name)
                .to_string();
            Some((name, shown))
        })
        .collect()
}

fn playing_json(p: &Playing) -> serde_json::Value {
    serde_json::json!({"id": p.id, "name": p.name, "file": p.file, "device": p.device,
        "positionMs": p.position_ms, "durationMs": p.duration_ms, "volume": (p.volume * 100.0).round(),
        "paused": p.paused})
}

/// The request `StreamerzSound` (and the MCP tools `sounds`, `sound_play`, `sound_stop`): `op` is
/// `list`, `play`, `stop`, `pause`, `resume`, `volume`, `master` or `info`. A sound played this way
/// fires no trigger.
pub fn request(req: &serde_json::Value) -> Result<serde_json::Value, String> {
    use serde_json::{json, Value};
    let text = |k: &str| req.get(k).and_then(Value::as_str).unwrap_or("").trim().to_string();
    let num = |k: &str, d: f64| req.get(k).and_then(Value::as_f64).unwrap_or(d);
    let name = text("name");
    let named = (!name.is_empty()).then_some(name.as_str());
    let fade = num("fadeMs", 0.0).clamp(0.0, 60_000.0) as u64;
    let count = |n: usize| Ok(json!({"count": n}));
    match text("op").as_str() {
        "" | "list" => {
            let devices = devices();
            if !devices.is_empty() {
                crate::catalog::set(
                    "sound.devices",
                    devices.iter().map(|d| (d.clone(), d.clone())).collect(),
                );
            }
            Ok(json!({
                "playing": playing().iter().map(playing_json).collect::<Vec<_>>(),
                "devices": devices,
                "master": (master() * 100.0).round(),
                "maxAtOnce": settings().max_at_once,
                "openOutputs": open_devices(),
                "formats": EXTENSIONS,
            }))
        }
        "play" => {
            let file = text("file");
            if file.is_empty() {
                return Err("file is required".into());
            }
            let started = play(&resolve(&file), &name, &text("device"), volume(num("volume", 100.0)))?;
            // Not `id`: a request's answer carries the request's own id.
            Ok(json!({"soundId": started.id, "durationMs": started.duration_ms}))
        }
        "stop" => count(stop(named, fade)),
        "pause" => count(pause(named, true)),
        "resume" => count(pause(named, false)),
        "volume" => count(set_volume(named, volume(num("volume", 100.0)), fade)),
        "master" => {
            set_master(volume(num("volume", 100.0)), fade);
            Ok(json!({"master": (master() * 100.0).round()}))
        }
        "info" => {
            let i = info(&resolve(&text("file")))?;
            Ok(json!({"durationMs": i.duration_ms, "rate": i.rate, "channels": i.channels, "format": i.format}))
        }
        other => Err(format!(
            "unknown op {other}: list, play, stop, pause, resume, volume, master or info"
        )),
    }
}

/// A random sound file of a folder (and its folders, `recursive`), not `avoid` when there is another.
pub fn pick_from_folder(folder: &Path, recursive: bool, avoid: Option<&str>) -> Result<PathBuf, String> {
    let mut found = Vec::new();
    let mut dirs = vec![folder.to_path_buf()];
    while let Some(dir) = dirs.pop() {
        let entries = std::fs::read_dir(&dir).map_err(|e| format!("cannot read {}: {e}", dir.display()))?;
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                if recursive && found.len() < 10_000 {
                    dirs.push(path);
                }
            } else if path
                .extension()
                .and_then(|e| e.to_str())
                .is_some_and(|e| EXTENSIONS.iter().any(|x| x.eq_ignore_ascii_case(e)))
            {
                found.push(path);
            }
        }
    }
    if found.is_empty() {
        return Err(format!("{} has no sound files", folder.display()));
    }
    found.sort();
    if found.len() > 1 {
        if let Some(avoid) = avoid {
            found.retain(|p| p.to_string_lossy() != avoid);
        }
    }
    let i = (crate::math::random() * found.len() as f64) as usize;
    Ok(found[i.min(found.len() - 1)].clone())
}

/// The player is one for the process: tests that play take turns.
#[cfg(test)]
pub(crate) static TURN: Mutex<()> = Mutex::new(());

#[cfg(test)]
mod settings_tests {
    use super::*;

    #[test]
    fn default_and_monitor_follow_the_output_settings() {
        let _g = crate::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        configure(&Settings {
            device: "Speakers".into(),
            monitor: "Headphones".into(),
            ..Settings::default()
        });
        assert_eq!(device_key(""), "Speakers");
        assert_eq!(device_key("Default"), "Speakers");
        assert_eq!(device_key("Monitor"), "Headphones");
        assert_eq!(device_key("USB"), "USB");
        configure(&Settings {
            device: "Speakers".into(),
            ..Settings::default()
        });
        assert_eq!(device_key("Monitor"), "Speakers", "no monitor: previews go to Default");
        configure(&Settings::default());
        assert_eq!(device_key(""), "");
        let sinks = r#"[{"name":"alsa_output.hdmi","description":"HDMI Audio"},{"name":"usb"}]"#;
        assert_eq!(
            sinks_from_json(sinks),
            vec![
                ("alsa_output.hdmi".to_string(), "HDMI Audio".to_string()),
                ("usb".into(), "usb".into())
            ]
        );
        assert!(sinks_from_json("not json").is_empty());
        assert!(is_sink_event("Event 'new' on sink #61"));
        assert!(is_sink_event("Event 'remove' on sink #61"));
        assert!(!is_sink_event("Event 'change' on sink #61"));
        assert!(!is_sink_event("Event 'new' on sink-input #9"));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    use super::TURN;

    fn wav(dir: &Path, name: &str, rate: u32, channels: u16, ms: u32) -> std::path::PathBuf {
        let path = dir.join(name);
        std::fs::write(&path, sine_wav(rate, channels, 440.0, rate * ms / 1000)).unwrap();
        path
    }

    fn scratch(tag: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("streamerz-sound-{tag}-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn wait(started: Started) -> Ended {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        rt.block_on(async {
            tokio::time::timeout(Duration::from_secs(5), started.ended)
                .await
                .unwrap()
                .unwrap()
        })
    }

    #[test]
    fn a_file_is_read_with_its_length_rate_and_channels() {
        let dir = scratch("info");
        let mono = wav(&dir, "mono.wav", 22_050, 1, 500);
        let i = info(&mono).unwrap();
        assert_eq!(
            (i.duration_ms, i.rate, i.channels, i.format.as_str()),
            (Some(500), 22_050, 1, "wav")
        );
        let bad = dir.join("bad.mp3");
        std::fs::write(&bad, b"not a sound").unwrap();
        assert!(info(&bad).is_err());
        assert!(info(&dir.join("missing.wav")).is_err());
    }

    #[test]
    fn decoding_resamples_to_the_mix_and_makes_mono_stereo() {
        let dir = scratch("decode");
        let path = wav(&dir, "a.wav", 24_000, 1, 100);
        let mut source = decode::Source::open(&path).unwrap();
        let mut out = vec![0f32; 48_000 * 2];
        let frames = source.read(&mut out);
        // 100 ms at 24 kHz is about 100 ms at 48 kHz (the last source frame has nothing after it).
        assert!((4790..=4800).contains(&frames), "{frames}");
        assert_eq!(out[200], out[201], "mono is the same on both sides");
        assert!(out[..frames * 2].iter().any(|s| s.abs() > 0.3));
        assert_eq!(source.read(&mut out), 0);
    }

    #[test]
    fn volumes_fade_and_a_faded_stop_ends_the_sound() {
        let mut r = Ramp::new(1.0);
        r.to(0.0, 10);
        let mut m = Mixer::new(1.0);
        let dir = scratch("fade");
        let path = wav(&dir, "f.wav", 48_000, 2, 1000);
        let (done, mut ended) = oneshot::channel();
        m.voices.push(Voice {
            id: 1,
            name: "f".into(),
            file: String::new(),
            source: decode::Source::open(&path).unwrap(),
            volume: Ramp::new(1.0),
            paused: false,
            stopping: false,
            played: 0,
            done: Some(done),
        });
        assert_eq!(m.stop(|v| v.name == "f", 10), 1);
        let mut out = vec![0f32; CHUNK * CHANNELS];
        m.render(&mut out);
        // 10 ms is 480 frames: faded out within the first chunk.
        assert!(out[CHUNK * 2 - 2].abs() < 1e-6);
        assert!(m.is_empty());
        assert_eq!(ended.try_recv().unwrap().why, End::Stopped);
    }

    #[test]
    fn a_sound_plays_to_its_end_and_the_device_closes_after() {
        let _turn = TURN.lock().unwrap_or_else(|e| e.into_inner());
        let dir = scratch("play");
        let path = wav(&dir, "beep.wav", 44_100, 2, 100);
        let before = SILENT_FRAMES.load(Ordering::Relaxed);
        let started = play(&path, "beep", "", 0.5).unwrap();
        assert_eq!(started.duration_ms, Some(100));
        assert_eq!(playing().iter().filter(|p| p.name == "beep").count(), 1);
        assert_eq!(wait(started).why, End::Finished);
        assert!(SILENT_FRAMES.load(Ordering::Relaxed) - before >= 4700);
        // The thread drains and lets the device go.
        for _ in 0..100 {
            if open_devices() == 0 {
                break;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        assert_eq!(open_devices(), 0);
    }

    #[test]
    fn named_sounds_are_paused_turned_down_and_stopped() {
        let _turn = TURN.lock().unwrap_or_else(|e| e.into_inner());
        let dir = scratch("named");
        let long = wav(&dir, "long.wav", 48_000, 2, 3000);
        let a = play(&long, "Music", "", 1.0).unwrap();
        let b = play(&long, "other", "Application Default", 1.0).unwrap();
        assert_eq!(pause(Some("music"), true), 1);
        assert!(playing().iter().any(|p| p.name == "Music" && p.paused));
        assert_eq!(set_volume(Some("MUSIC"), 0.2, 0), 1);
        assert!(playing().iter().any(|p| p.name == "Music" && p.volume == 0.2));
        assert_eq!(stop(Some("music"), 0), 1);
        assert_eq!(wait(a).why, End::Stopped);
        assert_eq!(stop(None, 30), 1);
        assert_eq!(wait(b).why, End::Stopped);
    }

    #[test]
    fn a_device_that_fails_ends_its_sounds_with_the_reason() {
        let _turn = TURN.lock().unwrap_or_else(|e| e.into_inner());
        fn broken(_: &str) -> Result<Box<dyn Output>, String> {
            Err("no such device".into())
        }
        let dir = scratch("broken");
        let path = wav(&dir, "x.wav", 48_000, 2, 200);
        player().opener = broken;
        let started = play(&path, "x", "Speakers", 1.0);
        player().opener = silent;
        let ended = wait(started.unwrap());
        assert_eq!(ended.why, End::Failed);
        assert_eq!(ended.error.as_deref(), Some("no such device"));
    }

    #[test]
    fn requests_play_list_change_and_stop_sounds() {
        let _turn = TURN.lock().unwrap_or_else(|e| e.into_inner());
        let dir = scratch("request");
        let path = wav(&dir, "req.wav", 48_000, 2, 2000);
        let file = path.to_string_lossy();
        let r = request(&serde_json::json!({"op": "play", "file": file, "name": "Req", "volume": 40})).unwrap();
        assert_eq!(r["durationMs"], 2000);
        assert!(r.get("id").is_none() && r["soundId"].as_u64().is_some(), "{r}");
        let list = request(&serde_json::json!({})).unwrap();
        let mine: Vec<_> = list["playing"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|p| p["name"] == "Req")
            .collect();
        assert_eq!(mine.len(), 1);
        assert_eq!(mine[0]["volume"], 40.0);
        assert_eq!(
            request(&serde_json::json!({"op": "pause", "name": "req"})).unwrap()["count"],
            1
        );
        assert_eq!(
            request(&serde_json::json!({"op": "volume", "name": "req", "volume": 10})).unwrap()["count"],
            1
        );
        assert_eq!(
            request(&serde_json::json!({"op": "stop", "name": "req"})).unwrap()["count"],
            1
        );
        assert_eq!(
            request(&serde_json::json!({"op": "info", "file": file})).unwrap()["channels"],
            2
        );
        assert!(request(&serde_json::json!({"op": "play"})).is_err());
        assert!(request(&serde_json::json!({"op": "dance"})).is_err());
        assert_eq!(
            request(&serde_json::json!({"op": "master", "volume": 100})).unwrap()["master"],
            100.0
        );
    }

    /// Plays on this computer's real output through the audio process of a built Streamerz
    /// (`STREAMERZ_SOUND_EXE=target/fast/Streamerz cargo test real_output -- --ignored`): a stream named
    /// Streamerz shows in the mixer while it plays, and the output is closed after.
    #[test]
    #[ignore]
    fn real_output() {
        let _turn = TURN.lock().unwrap_or_else(|e| e.into_inner());
        let dir = scratch("real");
        let path = wav(&dir, "real.wav", 44_100, 1, 600);
        player().opener = system;
        let started = play(&path, "real", "", 0.2);
        let ended = wait(started.unwrap());
        player().opener = silent;
        assert_eq!(
            ended,
            Ended {
                why: End::Finished,
                error: None
            }
        );
        for _ in 0..100 {
            if open_devices() == 0 {
                break;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        assert_eq!(open_devices(), 0);
    }

    /// The audio process's side of the pipe, with a script in its place: it says "ok" once the device
    /// is open, gets every frame as it was mixed, and a device it can't open comes back with its words.
    #[cfg(target_os = "linux")]
    #[test]
    fn the_audio_process_gets_the_mix_and_says_why_it_failed() {
        let _turn = TURN.lock().unwrap_or_else(|e| e.into_inner());
        let dir = scratch("helper");
        let script = dir.join("output.sh");
        let got = dir.join("got.raw");
        std::fs::write(
            &script,
            format!(
                "#!/bin/sh\n[ \"$2\" = bad ] && {{ echo 'no output called bad' >&2; exit 1; }}\necho ok\ncat > '{}'\n",
                got.display()
            ),
        )
        .unwrap();
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
        std::env::set_var("STREAMERZ_SOUND_EXE", &script);
        let mut out = helper::open("").unwrap();
        let frames: Vec<f32> = (0..CHUNK * CHANNELS).map(|i| i as f32 / 10.0).collect();
        out.write(&frames).unwrap();
        out.write(&frames).unwrap();
        out.drain();
        drop(out);
        let bytes = std::fs::read(&got).unwrap();
        assert_eq!(bytes.len(), 2 * CHUNK * CHANNELS * 4);
        assert_eq!(f32::from_le_bytes(bytes[40..44].try_into().unwrap()), 1.0);
        let failed = helper::open("bad").err();
        std::env::remove_var("STREAMERZ_SOUND_EXE");
        assert_eq!(failed.as_deref(), Some("no output called bad"));
    }

    #[test]
    fn volumes_and_devices_read_as_streamer_bot_writes_them() {
        // Default follows the Output settings, which other tests change.
        let _g = crate::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        assert_eq!(volume(100.0), 1.0);
        assert_eq!(volume(50.0), 0.5);
        assert_eq!(volume(0.25), 0.25);
        assert_eq!(volume(-3.0), 0.0);
        assert_eq!(volume(f64::NAN), 1.0);
        assert_eq!(device_key(" Application Default "), "");
        assert_eq!(device_key("Speakers (Realtek)"), "Speakers (Realtek)");
    }

    #[test]
    fn a_folder_gives_a_sound_and_avoids_the_last_one() {
        let dir = scratch("folder");
        let a = wav(&dir, "a.wav", 8_000, 1, 10);
        let _ = wav(&dir, "b.WAV", 8_000, 1, 10);
        std::fs::write(dir.join("notes.txt"), "x").unwrap();
        for _ in 0..20 {
            let picked = pick_from_folder(&dir, false, Some(&a.to_string_lossy())).unwrap();
            assert!(picked.ends_with("b.WAV"), "{}", picked.display());
        }
        let empty = scratch("empty-folder");
        assert!(pick_from_folder(&empty, true, None).is_err());
    }
}
