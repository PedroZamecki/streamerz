//! Speaking: text to speech played by Streamerz's player. One queue per output (and per overlay), so
//! speeches never talk over each other there; filters that make chat fit to be read aloud; a voice per
//! viewer (the user variable `ttsVoice`); the Speak sub-actions (99320 to 99327) and the triggers
//! Speech Started and Speech Ended (99320, 99321). Nothing runs until something speaks: a queue's
//! task ends with its last speech.
use super::{b, s, Ctx, Engine};
use crate::model::Obj;
use crate::sound;
use serde_json::{json, Value};
use std::cell::{Cell, RefCell};
use std::collections::{HashMap, VecDeque};
use std::rc::Rc;
use tokio::sync::oneshot;

pub const T_SPEECH_STARTED: u32 = 99320;
pub const T_SPEECH_ENDED: u32 = 99321;

/// The sub-actions this module runs.
pub fn handles(kind: u64) -> bool {
    (99320..=99327).contains(&kind)
}

/// What makes a message fit to be read aloud.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Filters {
    pub skip_links: bool,
    pub skip_emotes: bool,
    /// A letter said at most this many times in a row (0: as written).
    pub max_repeat: usize,
    /// At most this many characters, cut at a word (0: all).
    pub max_length: usize,
    /// Words never read, a word or a phrase a line.
    pub blocked: Vec<String>,
    /// A message that starts with `!` is not read.
    pub skip_commands: bool,
    /// Said before the message (`%user% says`), filled in already.
    pub prefix: String,
}

fn is_link(word: &str) -> bool {
    let w = word.to_ascii_lowercase();
    w.starts_with("http://") || w.starts_with("https://") || w.starts_with("www.")
}

/// The text as it will be said; None when nothing is left to say (a command, only links...).
pub fn clean(text: &str, f: &Filters, emotes: &[String]) -> Option<String> {
    let text = text.trim();
    if f.skip_commands && text.starts_with('!') {
        return None;
    }
    let mut words: Vec<String> = text
        .split_whitespace()
        .filter(|w| !(f.skip_links && is_link(w)))
        .filter(|w| !(f.skip_emotes && emotes.iter().any(|e| e == w)))
        .map(str::to_owned)
        .collect();
    if f.max_repeat > 0 {
        for w in &mut words {
            let mut out = String::with_capacity(w.len());
            let (mut last, mut run) = ('\0', 0);
            for c in w.chars() {
                run = if c.to_lowercase().eq(last.to_lowercase()) {
                    run + 1
                } else {
                    1
                };
                last = c;
                if run <= f.max_repeat {
                    out.push(c);
                }
            }
            *w = out;
        }
    }
    let mut said = words.join(" ");
    for blocked in f.blocked.iter().map(|b| b.trim()).filter(|b| !b.is_empty()) {
        said = remove_word(&said, blocked);
    }
    let said = said.split_whitespace().collect::<Vec<_>>().join(" ");
    let mut said = if f.max_length > 0 && said.chars().count() > f.max_length {
        let cut: String = said.chars().take(f.max_length).collect();
        match cut.rfind(' ') {
            Some(at) if cut[..at].chars().count() > f.max_length / 2 => cut[..at].to_string(),
            _ => cut,
        }
    } else {
        said
    };
    if said.chars().all(|c| !c.is_alphanumeric()) {
        return None;
    }
    if !f.prefix.trim().is_empty() {
        said = format!("{} {said}", f.prefix.trim());
    }
    Some(said)
}

/// `text` without `word` wherever it stands as a word (any case).
fn remove_word(text: &str, word: &str) -> String {
    let wanted = word.to_lowercase();
    if wanted.is_empty() {
        return text.to_string();
    }
    // Map each byte boundary in `lower` back to its byte offset in `text`, so Unicode case-folding
    // length changes (such as 'İ' -> "i\u{307}" or 'ẞ' -> "ß") never shift slices.
    let mut lower = String::with_capacity(text.len());
    let mut map: Vec<Option<usize>> = Vec::with_capacity(text.len() + 1);
    for (orig_at, ch) in text.char_indices() {
        let mut first = true;
        for lc in ch.to_lowercase() {
            let start = lower.len();
            lower.push(lc);
            let added = lower.len() - start;
            map.push(first.then_some(orig_at));
            for _ in 1..added {
                map.push(None);
            }
            first = false;
        }
    }
    map.push(Some(text.len()));

    let mut out = String::with_capacity(text.len());
    let mut orig_cursor = 0;
    let mut lower_cursor = 0;
    while let Some(rel) = lower[lower_cursor..].find(&wanted) {
        let found = lower_cursor + rel;
        let end = found + wanted.len();
        let before = lower[..found].chars().next_back().is_none_or(|c| !c.is_alphanumeric());
        let after = lower[end..].chars().next().is_none_or(|c| !c.is_alphanumeric());
        if before && after {
            if let (Some(orig_start), Some(orig_end)) = (map[found], map[end]) {
                if orig_start >= orig_cursor {
                    out.push_str(&text[orig_cursor..orig_start]);
                    orig_cursor = orig_end;
                    lower_cursor = end;
                    continue;
                }
            }
        }
        // Step past one character so overlapping phrases after a non-word match are still checked.
        let step = lower[found..].chars().next().map_or(1, char::len_utf8);
        lower_cursor = found + step;
    }
    out.push_str(&text[orig_cursor..]);
    out
}

/// A speech waiting or being said.
struct Item {
    text: String,
    voice: String,
    speed: f64,
    pitch: f64,
    volume: f64,
    /// The output ("" Default, "Monitor") or, with `overlay`, nothing.
    device: String,
    /// Played by an overlay's pages instead of this computer.
    overlay: String,
    user: String,
    action: String,
    /// Asked by an action Speech Started or Speech Ended ran: it fires neither (no endless speech).
    quiet: bool,
    /// When it ended: its length in ms and why (`finished`, `stopped`, `skipped`, `failed`).
    done: Option<oneshot::Sender<(Option<u64>, &'static str)>>,
}

#[derive(Default)]
struct Queue {
    waiting: VecDeque<Item>,
    running: bool,
}

struct ActiveSpeech {
    id: u64,
    overlay: Option<String>,
    local_playing: bool,
    stop: Option<oneshot::Sender<()>>,
}

struct ActiveGuard(u64);

impl Drop for ActiveGuard {
    fn drop(&mut self) {
        ACTIVE.with_borrow_mut(|active| active.retain(|entry| entry.id != self.0));
        crate::host::changed();
    }
}

thread_local! {
    static QUEUES: RefCell<HashMap<String, Queue>> = RefCell::new(HashMap::new());
    static ACTIVE: RefCell<Vec<ActiveSpeech>> = const { RefCell::new(Vec::new()) };
    static ACTIVE_NEXT: Cell<u64> = const { Cell::new(0) };
    static PAUSED: Cell<bool> = const { Cell::new(false) };
    static DISABLED: Cell<bool> = const { Cell::new(false) };
    /// Wakes the queues when speech is resumed.
    static RESUMED: Rc<tokio::sync::Notify> = Rc::new(tokio::sync::Notify::new());
}

fn is_speaking() -> bool {
    ACTIVE.with_borrow(|active| !active.is_empty()) || sound::playing().iter().any(|p| p.name == SPEECH)
}

/// The player's name for speeches, so Stop Speaking finds them.
const SPEECH: &str = "speech";

fn queue_key(item: &Item) -> String {
    if item.overlay.is_empty() {
        sound::device_key(&item.device)
    } else {
        format!("overlay:{}", item.overlay)
    }
}

/// Speeches waiting, on every queue.
pub fn waiting() -> usize {
    QUEUES.with_borrow(|q| q.values().map(|q| q.waiting.len()).sum())
}

impl Engine {
    /// Says `text` now (in its output's queue) with `voice` (empty: the default), on `device` ("" Default,
    /// "Monitor"). Runs on its own; a failure goes to the log.
    pub fn say_now(self: &Rc<Self>, text: &str, voice: &str, speed: f64, pitch: f64, volume: f64, device: &str) {
        let voice = if voice.is_empty() {
            self.config().voices.voice.clone()
        } else {
            voice.to_string()
        };
        self.enqueue(Item {
            text: text.to_string(),
            voice,
            speed,
            pitch,
            volume,
            device: device.to_string(),
            overlay: String::new(),
            user: String::new(),
            action: String::new(),
            quiet: false,
            done: None,
        });
    }

    fn enqueue(self: &Rc<Self>, mut item: Item) {
        if DISABLED.get() {
            if let Some(done) = item.done.take() {
                let _ = done.send((None, "skipped"));
            }
            return;
        }
        let key = queue_key(&item);
        let start = QUEUES.with_borrow_mut(|q| {
            let queue = q.entry(key.clone()).or_default();
            queue.waiting.push_back(item);
            !std::mem::replace(&mut queue.running, true)
        });
        crate::host::changed();
        if start {
            tokio::task::spawn_local(self.clone().run_queue(key));
        }
    }

    /// One queue: each speech in turn, until none is left.
    async fn run_queue(self: Rc<Self>, key: String) {
        loop {
            while PAUSED.get() {
                let resumed = RESUMED.with(|r| r.clone());
                resumed.notified().await;
            }
            let next = QUEUES.with_borrow_mut(|q| {
                let queue = q.get_mut(&key)?;
                let next = queue.waiting.pop_front();
                if next.is_none() {
                    q.remove(&key);
                }
                next
            });
            let Some(mut item) = next else {
                if QUEUES.with_borrow(HashMap::is_empty) && !is_speaking() {
                    crate::voice::neural::maybe_unload_idle(self.config().voices.keep_loaded).await;
                }
                return;
            };
            let (length, why) = self.say_item(&item).await;
            if let Some(done) = item.done.take() {
                let _ = done.send((length, why));
            }
        }
    }

    fn speech_args(item: &Item, length: Option<u64>) -> Obj {
        let mut args = Obj::new();
        args.insert("text".into(), item.text.clone().into());
        args.insert("voice".into(), item.voice.clone().into());
        args.insert("user".into(), item.user.clone().into());
        args.insert("actionName".into(), item.action.clone().into());
        if let Some(ms) = length {
            args.insert("speechDurationMs".into(), ms.into());
        }
        args
    }

    async fn say_item(self: &Rc<Self>, item: &Item) -> (Option<u64>, &'static str) {
        let active_id = ACTIVE_NEXT.with(|n| {
            let next = n.get().wrapping_add(1);
            n.set(next);
            next
        });
        let (stop_tx, mut stop_rx) = oneshot::channel();
        ACTIVE.with_borrow_mut(|active| {
            active.push(ActiveSpeech {
                id: active_id,
                overlay: (!item.overlay.is_empty()).then(|| item.overlay.clone()),
                local_playing: false,
                stop: Some(stop_tx),
            });
        });
        let _active_guard = ActiveGuard(active_id);
        crate::host::changed();

        let voices_cfg = self.config().voices.clone();
        let wav = tokio::select! {
            biased;
            _ = &mut stop_rx => {
                self.speech_ended(item, None, "stopped");
                return (None, "stopped");
            }
            res = crate::voice::speak_with(&item.text, &item.voice, item.speed, item.pitch, &voices_cfg) => match res {
                Ok(wav) => wav,
                Err(e) => {
                    crate::warn(format!("Speak: {e}"));
                    self.speech_ended(item, None, "failed");
                    return (None, "failed");
                }
            }
        };
        let length = wav_length_ms(&wav);
        while PAUSED.get() {
            let resumed = RESUMED.with(|r| r.clone());
            tokio::select! {
                biased;
                _ = &mut stop_rx => {
                    self.speech_ended(item, length, "stopped");
                    return (length, "stopped");
                }
                _ = resumed.notified() => {}
            }
        }
        if DISABLED.get() || stop_rx.try_recv().is_ok() {
            self.speech_ended(item, length, "stopped");
            return (length, "stopped");
        }
        if !item.quiet {
            let args = Self::speech_args(item, length);
            self.fire(
                T_SPEECH_STARTED,
                "Speech Started",
                "Core/Voices",
                "voices",
                &args,
                |_| true,
            );
        }
        if !item.overlay.is_empty() {
            let url = crate::overlays::keep_speech(&self.config(), wav);
            let data = json!({"url": url, "volume": sound::volume(item.volume), "name": SPEECH});
            self.emit("Sound", &item.overlay, || data.clone());
            let why = tokio::select! {
                biased;
                _ = &mut stop_rx => "stopped",
                _ = tokio::time::sleep(std::time::Duration::from_millis(length.unwrap_or(0))) => "finished",
            };
            self.speech_ended(item, length, why);
            return (length, why);
        }
        let file = match crate::voice::to_file(&wav) {
            Ok(file) => file,
            Err(e) => {
                crate::warn(format!("Speak: {e}"));
                self.speech_ended(item, None, "failed");
                return (None, "failed");
            }
        };
        let why = match sound::play(&file, SPEECH, &item.device, sound::volume(item.volume)) {
            Ok(started) => {
                ACTIVE.with_borrow_mut(|active| {
                    if let Some(entry) = active.iter_mut().find(|entry| entry.id == active_id) {
                        entry.local_playing = true;
                    }
                });
                match started.ended.await {
                    Ok(ended) => match ended.why {
                        sound::End::Finished => "finished",
                        sound::End::Stopped => "stopped",
                        sound::End::Failed => "failed",
                    },
                    Err(_) => "failed",
                }
            }
            Err(e) => {
                crate::warn(format!("Speak: {e}"));
                "failed"
            }
        };
        let _ = std::fs::remove_file(&file);
        self.speech_ended(item, length, why);
        (length, why)
    }

    fn speech_ended(self: &Rc<Self>, item: &Item, length: Option<u64>, why: &'static str) {
        if item.quiet {
            return;
        }
        let mut args = Self::speech_args(item, length);
        args.insert("reason".into(), why.into());
        self.fire(T_SPEECH_ENDED, "Speech Ended", "Core/Voices", "voices", &args, |_| true);
    }

    /// Stop Speaking: the speeches being said now (each queue goes on to its next).
    pub fn speech_stop(&self) -> usize {
        let mut stopped = sound::stop(Some(SPEECH), 0);
        let mut overlays = Vec::new();
        ACTIVE.with_borrow_mut(|active| {
            for entry in active.iter_mut() {
                if let Some(tx) = entry.stop.take() {
                    let _ = tx.send(());
                    if !entry.local_playing {
                        stopped += 1;
                    }
                }
                if let Some(overlay) = &entry.overlay {
                    if !overlays.contains(overlay) {
                        overlays.push(overlay.clone());
                    }
                }
            }
        });
        for overlay in overlays {
            self.emit("SoundStop", &overlay, || json!({"name": SPEECH}));
        }
        crate::host::changed();
        stopped
    }

    /// Clear Speech Queue: what waits is dropped (its Speak sub-actions go on, `skipped`).
    pub fn speech_clear(self: &Rc<Self>) -> usize {
        let dropped: Vec<Item> = QUEUES.with_borrow_mut(|q| q.values_mut().flat_map(|q| q.waiting.drain(..)).collect());
        let n = dropped.len();
        for mut item in dropped {
            if let Some(done) = item.done.take() {
                let _ = done.send((None, "skipped"));
            }
            self.speech_ended(&item, None, "skipped");
        }
        crate::host::changed();
        n
    }

    /// Set Speech Paused: the speech being said pauses where it is, and the queues wait.
    pub fn speech_pause(&self, paused: bool) {
        PAUSED.set(paused);
        sound::pause(Some(SPEECH), paused);
        if !paused {
            RESUMED.with(|r| r.notify_waiters());
        }
        crate::host::changed();
    }

    /// Set Speech Enabled: off, nothing is said and what waits is dropped.
    pub fn speech_enable(self: &Rc<Self>, enabled: bool) {
        DISABLED.set(!enabled);
        if !enabled {
            self.speech_clear();
            self.speech_stop();
        }
        crate::host::changed();
    }

    /// The request `StreamerzSpeech` (and the MCP tool `speak`): `op` is `speak` (text, voice, device,
    /// overlay), `stop`, `clear`, `pause` (paused), `enable` (enabled), `state` or `voices`.
    pub fn speech_request(self: &Rc<Self>, req: &Value) -> Result<Value, String> {
        let text = |k: &str| req.get(k).and_then(Value::as_str).unwrap_or("").trim().to_string();
        match text("op").as_str() {
            "speak" => {
                let said = text("text");
                if said.is_empty() {
                    return Err("text is required".into());
                }
                let d = self.config().voices.clone();
                let num = |k: &str, or: f64| req[k].as_f64().filter(|n| n.is_finite()).unwrap_or(or);
                self.enqueue(Item {
                    text: said,
                    voice: Some(text("voice")).filter(|v| !v.is_empty()).unwrap_or(d.voice),
                    speed: num("speed", d.speed),
                    pitch: num("pitch", d.pitch),
                    volume: num("volume", d.volume),
                    device: text("device"),
                    overlay: text("overlay"),
                    user: String::new(),
                    action: String::new(),
                    quiet: false,
                    done: None,
                });
                Ok(self.speech_state())
            }
            "stop" => Ok(json!({"stopped": self.speech_stop()})),
            "clear" => Ok(json!({"cleared": self.speech_clear()})),
            "pause" => {
                self.speech_pause(req["paused"].as_bool().unwrap_or(true));
                Ok(self.speech_state())
            }
            "enable" => {
                self.speech_enable(req["enabled"].as_bool().unwrap_or(true));
                Ok(self.speech_state())
            }
            "" | "state" => Ok(self.speech_state()),
            "voices" => crate::voice::voices_with(&self.config().voices)
                .map(|v| json!({"voices": v, "default": self.config().voices.voice})),
            other => Err(format!(
                "unknown op {other}: speak, stop, clear, pause, enable, state or voices"
            )),
        }
    }

    /// The request `StreamerzVoice` (and MCP tools `voices`, `voice_install`, `voice_remove`,
    /// `voice_mix`): manages neural voices, downloads, unloading, samples and user-made voices.
    pub async fn voice_request(self: &Rc<Self>, req: &Value) -> Result<Value, String> {
        let text = |k: &str| req.get(k).and_then(Value::as_str).unwrap_or("").trim().to_string();
        let num = |k: &str, or: f64| req[k].as_f64().filter(|n| n.is_finite()).unwrap_or(or);
        match text("op").as_str() {
            "" | "list" => {
                let cfg = self.config().voices.clone();
                let installed = crate::voice::voices_with(&cfg).unwrap_or_default();
                let catalog: Vec<Value> = crate::voice::neural::catalog()
                    .iter()
                    .map(|e| {
                        json!({
                            "id": e.id,
                            "name": e.name,
                            "engine": e.engine,
                            "language": e.language,
                            "gender": e.gender,
                            "quality": e.quality,
                            "diskBytes": e.disk_bytes,
                            "ramMb": e.ram_mb,
                            "license": e.license,
                            "commercial": e.commercial,
                            "attribution": e.attribution,
                            "sampleUrl": e.sample_url,
                            "installed": crate::voice::neural::is_model_installed(&e.id),
                        })
                    })
                    .collect();
                Ok(json!({
                    "default": cfg.voice,
                    "keepLoaded": cfg.keep_loaded,
                    "engine": crate::voice::neural::state(),
                    "installed": installed,
                    "custom": cfg.custom,
                    "catalog": catalog,
                }))
            }
            "state" => Ok(serde_json::to_value(crate::voice::neural::state()).unwrap_or_default()),
            "install" => {
                let id = text("id");
                if id.is_empty() || id == "engine" {
                    crate::voice::neural::install_engine().await?;
                } else {
                    crate::voice::neural::install_model(&id).await?;
                }
                Ok(json!({"installed": true, "engine": crate::voice::neural::state()}))
            }
            "cancel" => Ok(json!({"cancelled": crate::voice::neural::cancel_download()})),
            "remove" => {
                let id = text("id");
                if id.is_empty() {
                    return Err("id is required".into());
                }
                if id == "engine" {
                    crate::voice::neural::remove_engine().await?;
                } else if id.starts_with("custom:") {
                    self.remove_custom_voice(&id)?;
                } else {
                    crate::voice::neural::remove_model(&id).await?;
                }
                Ok(json!({"removed": id, "engine": crate::voice::neural::state()}))
            }
            "unload" => {
                crate::voice::neural::unload().await;
                Ok(json!({"unloaded": true, "engine": crate::voice::neural::state()}))
            }
            "sample" => {
                let id = text("id");
                if id.is_empty() {
                    return Err("id is required".into());
                }
                let path = crate::voice::neural::play_sample(&id).await?;
                Ok(json!({"playing": true, "file": path.display().to_string()}))
            }
            "mix" => {
                let name = text("name");
                if name.is_empty() {
                    return Err("name is required".into());
                }
                let parts_raw = req["parts"]
                    .as_array()
                    .ok_or("parts must be an array of [speaker, share]")?;
                let mut mix = Vec::new();
                for p in parts_raw {
                    let Some(pair) = p.as_array() else { continue };
                    let sid = if let Some(n) = pair.first().and_then(Value::as_i64) {
                        i32::try_from(n)
                            .ok()
                            .filter(|&id| id >= 0)
                            .ok_or_else(|| format!("invalid Kokoro speaker id {n}"))?
                    } else if let Some(sp_name) = pair.first().and_then(Value::as_str) {
                        let key = sp_name.strip_prefix("kokoro:").unwrap_or(sp_name);
                        crate::voice::neural::kokoro_speakers()
                            .iter()
                            .find(|s| s.id.eq_ignore_ascii_case(key) || s.name.eq_ignore_ascii_case(key))
                            .map(|s| s.sid)
                            .ok_or_else(|| format!("unknown Kokoro speaker {sp_name}"))?
                    } else {
                        continue;
                    };
                    let share = pair.get(1).and_then(Value::as_f64).unwrap_or(0.0);
                    if share.is_finite() && share > 0.0 {
                        mix.push((sid, share));
                    }
                }
                if mix.len() < 2 || mix.len() > 4 {
                    return Err("A Kokoro mix needs 2 to 4 voices with positive shares.".into());
                }
                let lang = {
                    let l = text("language");
                    if l.is_empty() {
                        crate::voice::neural::kokoro_speakers()
                            .iter()
                            .find(|s| s.sid == mix[0].0)
                            .map(|s| s.language.clone())
                            .unwrap_or_else(|| "en-US".into())
                    } else {
                        l
                    }
                };
                let kokoro_lang = {
                    let kl = text("kokoroLang");
                    if kl.is_empty() {
                        kokoro_lang_for(&lang)
                    } else {
                        kl
                    }
                };
                let custom = crate::voice::CustomVoice {
                    id: custom_voice_id(&name, &text("id")),
                    name,
                    kind: crate::voice::CustomKind::Mix,
                    language: lang,
                    mix,
                    kokoro_lang,
                    recording: String::new(),
                    consent: false,
                    base_voice: String::new(),
                    speed: num("speed", 1.0).clamp(0.5, 3.0),
                    pitch: num("pitch", 1.0).clamp(0.5, 1.5),
                    effect: validate_effect(&text("effect"))?,
                };
                let id = custom.id.clone();
                self.save_custom_voice(custom.clone());
                let preview = text("previewText");
                if !preview.is_empty() {
                    let cfg = self.config().voices.clone();
                    self.say_now(&preview, &id, 1.0, 1.0, cfg.volume, "Monitor");
                }
                Ok(json!({"saved": id, "voice": custom}))
            }
            "clone" => {
                let name = text("name");
                if name.is_empty() {
                    return Err("name is required".into());
                }
                if req["consent"] != true {
                    return Err("You must confirm consent (consent: true) that this is your own voice or one you have permission to use.".into());
                }
                let file = text("recording");
                if file.is_empty() {
                    return Err("recording file path is required".into());
                }
                let id = custom_voice_id(&name, &text("id"));
                let (filename, duration_ms) =
                    crate::voice::neural::import_recording(std::path::Path::new(&file), &id)?;
                let lang = {
                    let l = text("language");
                    if l.is_empty() { "en-US".into() } else { l }
                };
                let custom = crate::voice::CustomVoice {
                    id: id.clone(),
                    name,
                    kind: crate::voice::CustomKind::Clone,
                    language: lang,
                    mix: Vec::new(),
                    kokoro_lang: String::new(),
                    recording: filename,
                    consent: true,
                    base_voice: String::new(),
                    speed: num("speed", 1.0).clamp(0.5, 3.0),
                    pitch: num("pitch", 1.0).clamp(0.5, 1.5),
                    effect: validate_effect(&text("effect"))?,
                };
                self.save_custom_voice(custom.clone());
                Ok(json!({"saved": id, "durationMs": duration_ms, "voice": custom}))
            }
            "effect" => {
                let name = text("name");
                if name.is_empty() {
                    return Err("name is required".into());
                }
                let base_voice = text("baseVoice");
                if base_voice.is_empty() {
                    return Err("baseVoice is required".into());
                }
                if base_voice.starts_with("custom:") {
                    return Err("baseVoice must be a system or neural voice, not another custom voice.".into());
                }
                let effect = validate_effect(&text("effect"))?;
                let id = custom_voice_id(&name, &text("id"));
                let lang = {
                    let l = text("language");
                    if l.is_empty() {
                        crate::voice::voices_with(&self.config().voices)
                            .ok()
                            .and_then(|all| all.into_iter().find(|v| v.id == base_voice))
                            .map(|v| v.language)
                            .unwrap_or_default()
                    } else {
                        l
                    }
                };
                let custom = crate::voice::CustomVoice {
                    id: id.clone(),
                    name,
                    kind: crate::voice::CustomKind::Effect,
                    language: lang,
                    mix: Vec::new(),
                    kokoro_lang: String::new(),
                    recording: String::new(),
                    consent: false,
                    base_voice,
                    speed: num("speed", 1.0).clamp(0.5, 3.0),
                    pitch: num("pitch", 1.0).clamp(0.5, 1.5),
                    effect,
                };
                self.save_custom_voice(custom.clone());
                Ok(json!({"saved": id, "voice": custom}))
            }
            "record_mic" => {
                let secs = req["seconds"].as_u64().unwrap_or(6).clamp(3, 12) as u32;
                let path = crate::voice::neural::record_mic(secs).await?;
                Ok(json!({"file": path.display().to_string(), "seconds": secs}))
            }
            other => Err(format!(
                "unknown op {other}: list, state, install, cancel, remove, unload, sample, mix, clone, effect or record_mic"
            )),
        }
    }

    pub fn save_custom_voice(self: &Rc<Self>, voice: crate::voice::CustomVoice) {
        self.change(|c| match c.voices.custom.iter_mut().find(|v| v.id == voice.id) {
            Some(existing) => *existing = voice,
            None => c.voices.custom.push(voice),
        });
    }

    pub fn remove_custom_voice(self: &Rc<Self>, id: &str) -> Result<(), String> {
        let recording = self
            .config()
            .voices
            .custom
            .iter()
            .find(|v| v.id == id)
            .map(|v| v.recording.clone())
            .ok_or_else(|| format!("no custom voice {id}"))?;
        self.change(|c| {
            c.voices.custom.retain(|v| v.id != id);
            if c.voices.voice == id {
                c.voices.voice.clear();
            }
        });
        if !recording.is_empty()
            && std::path::Path::new(&recording).file_name() == Some(std::ffi::OsStr::new(&recording))
        {
            let _ = std::fs::remove_file(crate::voice::neural::recordings_dir().join(recording));
        }
        Ok(())
    }

    pub fn speech_state(&self) -> Value {
        json!({"paused": PAUSED.get(), "enabled": !DISABLED.get(), "waiting": waiting(), "speaking": is_speaking()})
    }

    /// The Speak sub-actions.
    pub(super) async fn speech_sub(self: &Rc<Self>, ctx: &mut Ctx, o: &Obj, kind: u64) {
        // 0 off, 1 on, 2 the other way round.
        let state = |now: bool| match super::n(o, "state") as i64 {
            0 => false,
            1 => true,
            _ => !now,
        };
        match kind {
            99320 | 99321 => {
                let Some(item) = self.speech_item(ctx, o) else {
                    ctx.args.insert("speechSkipped".into(), true.into());
                    return;
                };
                if kind == 99321 {
                    return self.speak_to_file(ctx, o, item).await;
                }
                let wait = b(o, "finishBeforeContinuing", true);
                let (tx, rx) = oneshot::channel();
                self.enqueue(Item { done: Some(tx), ..item });
                if wait {
                    if let Ok((length, why)) = rx.await {
                        if let Some(ms) = length {
                            ctx.args.insert("speechDurationMs".into(), ms.into());
                        }
                        ctx.args.insert("speechReason".into(), why.into());
                    }
                }
            }
            99322 => {
                self.speech_stop();
            }
            99323 => {
                self.speech_clear();
            }
            99324 => {
                let paused = state(PAUSED.get());
                self.speech_pause(paused);
            }
            99325 => {
                let enabled = state(!DISABLED.get());
                self.speech_enable(enabled);
            }
            99326 => {
                let voice = self.expand(ctx, s(o, "voice"));
                // 0 the one who ran the action, 1 the target user.
                let source = (super::n(o, "source") as i64).clamp(0, 1) + 1;
                if !self.set_user_voice(ctx, source, &voice) {
                    crate::warn("Set User Voice: there is no user to give a voice".to_string());
                }
            }
            99327 => match crate::voice::voices_with(&self.config().voices) {
                Ok(voices) => {
                    let list: Vec<Value> = voices
                        .iter()
                        .map(|v| json!({"id": v.id, "name": v.name, "language": v.language, "gender": v.gender}))
                        .collect();
                    ctx.args.insert("voiceCount".into(), list.len().into());
                    ctx.args.insert("voices".into(), Value::Array(list).to_string().into());
                }
                Err(e) => {
                    crate::warn(format!("Get Voices: {e}"));
                    ctx.args.insert("voiceCount".into(), 0.into());
                }
            },
            _ => {}
        }
    }

    /// What a Speak sub-action asks for, filtered; None when there is nothing to say.
    fn speech_item(self: &Rc<Self>, ctx: &mut Ctx, o: &Obj) -> Option<Item> {
        let defaults = self.config().voices.clone();
        let text = self.expand(ctx, s(o, "text"));
        let words = |k: &str| -> Vec<String> {
            self.expand(ctx, s(o, k))
                .lines()
                .map(|l| l.trim().to_string())
                .filter(|l| !l.is_empty())
                .collect()
        };
        let user = ctx
            .args
            .get("user")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();
        let user_login = ctx.args.get("userName").and_then(Value::as_str).unwrap_or_default();
        // Bots and the users named are never read.
        let skip_users = words("skipUsers");
        let matches_skip = |candidate: &str| {
            let candidate = candidate.trim_start_matches('@');
            !candidate.is_empty()
                && skip_users
                    .iter()
                    .any(|u| u.trim_start_matches('@').eq_ignore_ascii_case(candidate))
        };
        if matches_skip(&user) || matches_skip(user_login) {
            return None;
        }
        // A switch or a number may be a variable (`%group.skipLinks%`): filled in first.
        let flag = |ctx: &Ctx, k: &str| match o.get(k) {
            Some(Value::String(v)) => matches!(
                self.expand(ctx, v).trim().to_ascii_lowercase().as_str(),
                "true" | "1" | "yes" | "on"
            ),
            _ => b(o, k, false),
        };
        let amount = |ctx: &Ctx, k: &str, or: f64| match o.get(k) {
            Some(Value::String(v)) => crate::fmt::parse_number(self.expand(ctx, v).trim())
                .filter(|n| n.is_finite())
                .unwrap_or(or),
            _ => number(o, k, or),
        };
        let filters = Filters {
            skip_links: flag(ctx, "skipLinks"),
            skip_emotes: flag(ctx, "skipEmotes"),
            max_repeat: amount(ctx, "maxRepeat", 0.0).max(0.0) as usize,
            max_length: amount(ctx, "maxLength", 0.0).max(0.0) as usize,
            blocked: words("blockedWords"),
            skip_commands: flag(ctx, "skipCommands"),
            prefix: self.expand(ctx, s(o, "prefix")),
        };
        let emotes = emote_names(&ctx.args);
        let said = clean(&text, &filters, &emotes)?;
        let voice = Some(self.expand(ctx, s(o, "voice")).trim().to_string())
            .filter(|v| !v.is_empty())
            .or_else(|| self.user_voice(ctx, 1))
            .unwrap_or_else(|| defaults.voice.clone());
        let action = ctx
            .args
            .get("actionName")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();
        let quiet = matches!(
            ctx.args.get("__source").and_then(Value::as_str),
            Some("SpeechStarted" | "SpeechEnded")
        );
        Some(Item {
            text: said,
            voice,
            speed: amount(ctx, "speed", defaults.speed),
            pitch: amount(ctx, "pitch", defaults.pitch),
            volume: amount(ctx, "volume", defaults.volume),
            device: self.expand(ctx, s(o, "device")),
            overlay: self.expand(ctx, s(o, "overlay")).trim().to_string(),
            user,
            action,
            quiet,
            done: None,
        })
    }

    /// Speak To File: the speech as a WAV (the file given, else one in the sounds folder).
    async fn speak_to_file(self: &Rc<Self>, ctx: &mut Ctx, o: &Obj, item: Item) {
        let voices_cfg = self.config().voices.clone();
        let wav = match crate::voice::speak_with(&item.text, &item.voice, item.speed, item.pitch, &voices_cfg).await {
            Ok(wav) => wav,
            Err(e) => return crate::warn(format!("Speak To File: {e}")),
        };
        if waiting() == 0 {
            crate::voice::neural::maybe_unload_idle(voices_cfg.keep_loaded).await;
        }
        let given = self.expand(ctx, s(o, "file")).trim().to_string();
        let path = if given.is_empty() {
            let dir = crate::core::sounds::folder();
            let _ = std::fs::create_dir_all(&dir);
            dir.join(format!("speech-{}.wav", crate::fmt::unix_now().as_millis()))
        } else {
            sound::resolve(&given)
        };
        if let Err(e) = std::fs::write(&path, &wav) {
            return crate::warn(format!("Speak To File: {e}"));
        }
        ctx.args.insert("speechFile".into(), path.display().to_string().into());
        if let Some(ms) = wav_length_ms(&wav) {
            ctx.args.insert("speechDurationMs".into(), ms.into());
        }
    }
}

fn number(o: &Obj, key: &str, or: f64) -> f64 {
    o.get(key)
        .and_then(|v| {
            v.as_f64()
                .or_else(|| v.as_str().and_then(|t| crate::fmt::parse_number(t.trim())))
        })
        .filter(|n| n.is_finite())
        .unwrap_or(or)
}

pub fn custom_voice_id(name: &str, given: &str) -> String {
    let given = given.trim();
    if given.starts_with("custom:") && given.len() > 7 {
        return given.to_string();
    }
    let slug: String = name
        .to_ascii_lowercase()
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect::<String>()
        .trim_matches('-')
        .to_string();
    let short = crate::engine::uuid();
    let tail = &short[..short.len().min(6)];
    if slug.is_empty() {
        format!("custom:{tail}")
    } else {
        format!("custom:{slug}-{tail}")
    }
}

pub fn kokoro_lang_for(lang: &str) -> String {
    match lang.to_ascii_lowercase().as_str() {
        "en-gb" => "en-gb",
        "pt-br" | "pt" => "pt-br",
        "es" | "es-es" | "es-mx" | "es-ar" => "es",
        "fr" | "fr-fr" => "fr-fr",
        "it" | "it-it" => "it",
        "ja" | "ja-jp" => "ja",
        "hi" | "hi-in" => "hi",
        "zh" | "zh-cn" | "cmn" => "cmn",
        _ => "en-us",
    }
    .into()
}

pub fn validate_effect(effect: &str) -> Result<String, String> {
    let e = effect.trim().to_ascii_lowercase();
    if e.is_empty() || e == "none" {
        return Ok(String::new());
    }
    if crate::voice::effects::EFFECTS.contains(&e.as_str()) {
        Ok(e)
    } else {
        Err(format!("unknown voice effect {effect:?}: none, echo, radio or robot"))
    }
}

/// The names of the emotes of the chat message that started the action (Twitch's `emotes`, Kick's).
fn emote_names(args: &Obj) -> Vec<String> {
    let list: Vec<Value> = match args.get("emotes") {
        Some(Value::Array(a)) => a.clone(),
        Some(Value::String(s)) => serde_json::from_str(s).unwrap_or_default(),
        _ => Vec::new(),
    };
    list.iter()
        .filter_map(|e| {
            e.get("name")
                .or_else(|| e.get("text"))
                .and_then(Value::as_str)
                .map(str::to_owned)
        })
        .collect()
}

/// A WAV's length from its header (16-bit or any PCM: data bytes over bytes a second).
pub fn wav_length_ms(wav: &[u8]) -> Option<u64> {
    if wav.len() < 44 || &wav[0..4] != b"RIFF" || &wav[8..12] != b"WAVE" {
        return None;
    }
    let mut at = 12usize;
    let mut rate = None;
    while at + 8 <= wav.len() {
        let size = u32::from_le_bytes(wav[at + 4..at + 8].try_into().ok()?) as usize;
        match &wav[at..at + 4] {
            b"fmt " if size >= 12 && at + 20 <= wav.len() => {
                rate = Some(u32::from_le_bytes(wav[at + 16..at + 20].try_into().ok()?) as u64);
            }
            b"data" => {
                let bytes = size.min(wav.len() - at - 8) as u64;
                return rate.filter(|r| *r > 0).map(|r| bytes.saturating_mul(1000) / r);
            }
            _ => {}
        }
        let step = 8usize.checked_add(size)?.checked_add(size & 1)?;
        at = at.checked_add(step)?;
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn f() -> Filters {
        Filters::default()
    }

    #[test]
    fn chat_is_made_fit_to_be_read() {
        let links = Filters {
            skip_links: true,
            ..f()
        };
        assert_eq!(
            clean("look https://x.io/a now www.y.com", &links, &[]).unwrap(),
            "look now"
        );
        assert_eq!(clean("https://x.io", &links, &[]), None, "only a link: nothing to say");
        let commands = Filters {
            skip_commands: true,
            ..f()
        };
        assert_eq!(clean("!sr song", &commands, &[]), None);
        let repeat = Filters { max_repeat: 3, ..f() };
        assert_eq!(clean("noooooooo HAHAHAAAAA", &repeat, &[]).unwrap(), "nooo HAHAHAAA");
        let emotes = Filters {
            skip_emotes: true,
            ..f()
        };
        assert_eq!(clean("hi Kappa there", &emotes, &["Kappa".into()]).unwrap(), "hi there");
        let blocked = Filters {
            blocked: vec!["bad word".into(), "ugly".into()],
            ..f()
        };
        assert_eq!(
            clean("a Bad Word and ugly, not uglyish", &blocked, &[]).unwrap(),
            "a and , not uglyish"
        );
        let long = Filters { max_length: 20, ..f() };
        assert_eq!(
            clean("one two three four five six seven", &long, &[]).unwrap(),
            "one two three four"
        );
        let prefix = Filters {
            prefix: "ana says".into(),
            ..f()
        };
        assert_eq!(clean("hello", &prefix, &[]).unwrap(), "ana says hello");
        assert_eq!(clean("  ...  ", &f(), &[]), None);
    }

    #[test]
    fn a_wav_says_its_length() {
        let mut wav = Vec::new();
        wav.extend(b"RIFF\0\0\0\0WAVEfmt ");
        wav.extend(16u32.to_le_bytes());
        wav.extend([1, 0, 1, 0]);
        wav.extend(22050u32.to_le_bytes());
        wav.extend(44100u32.to_le_bytes());
        wav.extend([2, 0, 16, 0]);
        wav.extend(b"data");
        wav.extend(44100u32.to_le_bytes());
        wav.extend(vec![0u8; 44100]);
        assert_eq!(wav_length_ms(&wav), Some(1000));
        assert_eq!(wav_length_ms(b"nope"), None);
    }

    #[test]
    fn wav_length_ms_rejects_malformed_and_truncated_headers_without_panicking() {
        // Non-WAVE RIFF container.
        let mut avi = vec![0u8; 64];
        avi[0..4].copy_from_slice(b"RIFF");
        avi[8..12].copy_from_slice(b"AVI ");
        assert_eq!(wav_length_ms(&avi), None);

        // Truncated fmt chunk near EOF: at + 16 <= len < at + 20 previously panicked.
        let mut trunc_fmt = Vec::new();
        trunc_fmt.extend(b"RIFF\0\0\0\0WAVEJUNK");
        trunc_fmt.extend(20u32.to_le_bytes());
        trunc_fmt.extend([0u8; 20]);
        trunc_fmt.extend(b"fmt ");
        trunc_fmt.extend(16u32.to_le_bytes());
        trunc_fmt.extend([0u8; 10]); // total 40 + 18 = 58 bytes; fmt starts at 40, at+16=56<=58 but at+20=60>58
        assert_eq!(wav_length_ms(&trunc_fmt), None);

        // Chunk size u32::MAX must not overflow or loop forever.
        let mut huge = Vec::new();
        huge.extend(b"RIFF\0\0\0\0WAVEJUNK");
        huge.extend(u32::MAX.to_le_bytes());
        huge.extend([0u8; 40]);
        assert_eq!(wav_length_ms(&huge), None);
    }

    #[test]
    fn clean_handles_unicode_case_folding_overlaps_and_multibyte_max_length() {
        // Turkish 'İ' expands from 2 bytes to 3 bytes on to_lowercase().
        let blocked = Filters {
            blocked: vec!["bad".into(), "ab ab".into()],
            ..f()
        };
        assert_eq!(clean("İstanbul bad word", &blocked, &[]).unwrap(), "İstanbul word");
        assert_eq!(clean("İ bad!", &blocked, &[]).unwrap(), "İ !");
        // Overlapping phrase after a rejected partial match is still removed.
        assert_eq!(clean("xab ab ab ok", &blocked, &[]).unwrap(), "xab ok");
        // Multi-byte UTF-8 max_length counts characters, not bytes.
        let short = Filters { max_length: 8, ..f() };
        // "あいう えおかきくけ" has 3 chars before space (<= 8/2 = 4), so it must keep all 8 chars.
        assert_eq!(clean("あいう えおかきくけ", &short, &[]).unwrap(), "あいう えおかき");
    }

    #[test]
    fn overlay_speech_can_be_stopped_and_reports_speaking() {
        let _g = crate::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        // Run on a dedicated thread so thread-local voice::offline() does not affect other tests.
        std::thread::spawn(|| {
            crate::engine::tests::run(async {
                crate::voice::offline();
                let e = crate::engine::tests::engine(json!({"actions": []}));
                e.speech_enable(true);
                e.speech_pause(false);

                let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
                let mut subs = crate::engine::events::Subs::new();
                crate::engine::events::update(&mut subs, &json!({"Sound": ["ov1"], "SoundStop": ["ov1"]}), true);
                e.listen(99, &subs, &tx);

                let (done_tx, done_rx) = oneshot::channel();
                e.enqueue(Item {
                    text: "word ".repeat(20),
                    voice: "en".into(),
                    speed: 1.0,
                    pitch: 1.0,
                    volume: 100.0,
                    device: String::new(),
                    overlay: "ov1".into(),
                    user: "Ana".into(),
                    action: String::new(),
                    quiet: true,
                    done: Some(done_tx),
                });

                // Let the queue start and emit the overlay Sound event.
                tokio::time::sleep(std::time::Duration::from_millis(30)).await;
                assert_eq!(e.speech_state()["speaking"], true);

                let stopped = e.speech_stop();
                assert_eq!(stopped, 1);
                let (_, why) = tokio::time::timeout(std::time::Duration::from_secs(1), done_rx)
                    .await
                    .expect("overlay speech stopped promptly")
                    .unwrap();
                assert_eq!(why, "stopped");

                let events: Vec<String> = std::iter::from_fn(|| rx.try_recv().ok())
                    .map(|m| {
                        let v: Value = serde_json::from_str(&m).unwrap();
                        v["event"]["source"].as_str().unwrap().to_string()
                    })
                    .collect();
                assert!(events.contains(&"Sound".to_string()), "{events:?}");
                assert!(events.contains(&"SoundStop".to_string()), "{events:?}");
                assert_eq!(e.speech_state()["speaking"], false);
            });
        })
        .join()
        .unwrap();
    }

    #[test]
    fn speech_queue_pause_clear_disable_and_skip_users_by_login() {
        let _g = crate::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        crate::engine::tests::run(async {
            let e = crate::engine::tests::engine(json!({"actions": []}));
            e.speech_enable(true);
            e.speech_pause(true);

            let (done_tx, done_rx) = oneshot::channel();
            e.enqueue(Item {
                text: "queued".into(),
                voice: "en".into(),
                speed: 1.0,
                pitch: 1.0,
                volume: 100.0,
                device: String::new(),
                overlay: String::new(),
                user: String::new(),
                action: String::new(),
                quiet: true,
                done: Some(done_tx),
            });
            assert_eq!(waiting(), 1);
            assert_eq!(e.speech_clear(), 1);
            assert_eq!(waiting(), 0);
            assert_eq!(done_rx.await.unwrap(), (None, "skipped"));

            // Disabled speech immediately skips newly enqueued items.
            e.speech_enable(false);
            let (d2_tx, d2_rx) = oneshot::channel();
            e.enqueue(Item {
                text: "blocked".into(),
                voice: "en".into(),
                speed: 1.0,
                pitch: 1.0,
                volume: 100.0,
                device: String::new(),
                overlay: String::new(),
                user: String::new(),
                action: String::new(),
                quiet: true,
                done: Some(d2_tx),
            });
            assert_eq!(d2_rx.await.unwrap(), (None, "skipped"));
            e.speech_enable(true);
            e.speech_pause(false);

            // skipUsers matches both login (userName) and @-prefixed names.
            let mut ctx = Ctx {
                args: json!({"user": "Bot Display", "userName": "nightbot"})
                    .as_object()
                    .cloned()
                    .unwrap(),
                group: String::new(),
            };
            let o = json!({"text": "hello", "skipUsers": "@Nightbot\nstreamlabs"})
                .as_object()
                .cloned()
                .unwrap();
            assert!(e.speech_item(&mut ctx, &o).is_none());
        });
    }

    #[test]
    fn helper_functions_cover_languages_effects_ids_and_emotes() {
        assert_eq!(kokoro_lang_for("PT-BR"), "pt-br");
        assert_eq!(kokoro_lang_for("es-MX"), "es");
        assert_eq!(kokoro_lang_for("ja-JP"), "ja");
        assert_eq!(kokoro_lang_for("unknown"), "en-us");
        assert_eq!(validate_effect("none").unwrap(), "");
        assert_eq!(validate_effect(" Radio ").unwrap(), "radio");
        assert!(validate_effect("reverb").is_err());
        assert_eq!(custom_voice_id("Test", "custom:keep-me"), "custom:keep-me");
        assert!(custom_voice_id("My Voice!", "").starts_with("custom:my-voice-"));
        let args = json!({"emotes": [{"name": "Kappa"}, {"text": "PogChamp"}]})
            .as_object()
            .cloned()
            .unwrap();
        assert_eq!(emote_names(&args), vec!["Kappa", "PogChamp"]);
    }

    #[test]
    fn voice_request_manages_catalog_mixes_clones_and_effects() {
        let _g = crate::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let tmp = std::env::temp_dir().join(format!("streamerz-voice-req-{}", crate::engine::uuid()));
        std::fs::create_dir_all(&tmp).unwrap();
        *crate::voice::neural::TEST_DIR.lock().unwrap() = Some(tmp.join("voices"));
        crate::engine::tests::run(async {
            let e = crate::engine::tests::engine(json!({"actions": []}));
            let list = e.voice_request(&json!({"op": "list"})).await.unwrap();
            assert!(list["catalog"].as_array().unwrap().len() >= 20);

            // Kokoro mix validation and save.
            let bad_mix = e
                .voice_request(&json!({"op": "mix", "name": "Too Few", "parts": [["af_heart", 1.0]]}))
                .await;
            assert!(bad_mix.is_err());
            let mix = e
                .voice_request(&json!({
                    "op": "mix",
                    "name": "Warm Host",
                    "parts": [["af_heart", 60], ["am_adam", 40]],
                    "language": "en-US",
                    "effect": "echo",
                }))
                .await
                .unwrap();
            let mix_id = mix["saved"].as_str().unwrap().to_string();
            assert!(mix_id.starts_with("custom:"));
            assert_eq!(e.config().voices.custom.len(), 1);

            // Pocket TTS clone requires explicit consent and converts WAV into voices/recordings/.
            let sample_wav = tmp.join("sample.wav");
            std::fs::write(&sample_wav, crate::sound::sine_wav(24_000, 1, 300.0, 48_000)).unwrap();
            let no_consent = e
                .voice_request(&json!({
                    "op": "clone",
                    "name": "My Voice",
                    "recording": sample_wav.display().to_string(),
                    "consent": false,
                }))
                .await;
            assert!(no_consent.is_err(), "clone must refuse without consent");
            let cloned = e
                .voice_request(&json!({
                    "op": "clone",
                    "name": "My Voice",
                    "recording": sample_wav.display().to_string(),
                    "consent": true,
                }))
                .await
                .unwrap();
            let clone_id = cloned["saved"].as_str().unwrap().to_string();
            assert!(cloned["durationMs"].as_u64().unwrap() >= 1900);
            let rec_name = cloned["voice"]["recording"].as_str().unwrap();
            assert!(crate::voice::neural::recordings_dir().join(rec_name).is_file());

            // Effect voice creation and cleanup.
            let fx = e
                .voice_request(&json!({
                    "op": "effect",
                    "name": "Radio Host",
                    "baseVoice": "en-US",
                    "effect": "radio",
                    "pitch": 0.9,
                }))
                .await
                .unwrap();
            let fx_id = fx["saved"].as_str().unwrap().to_string();
            assert_eq!(e.config().voices.custom.len(), 3);

            // Removing custom voices cleans up their recordings too.
            for id in [mix_id, clone_id, fx_id] {
                e.voice_request(&json!({"op": "remove", "id": id})).await.unwrap();
            }
            assert!(e.config().voices.custom.is_empty());
            assert!(!crate::voice::neural::recordings_dir().join(rec_name).exists());
        });
        *crate::voice::neural::TEST_DIR.lock().unwrap() = None;
        let _ = std::fs::remove_dir_all(&tmp);
    }
}
