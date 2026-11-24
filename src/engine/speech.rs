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
            Some(at) if at > f.max_length / 2 => cut[..at].to_string(),
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
    let lower = text.to_lowercase();
    let wanted = word.to_lowercase();
    let mut out = String::new();
    let mut at = 0;
    while let Some(found) = lower[at..].find(&wanted).map(|f| f + at) {
        let end = found + wanted.len();
        let before = lower[..found].chars().next_back().is_none_or(|c| !c.is_alphanumeric());
        let after = lower[end..].chars().next().is_none_or(|c| !c.is_alphanumeric());
        if before && after && text.is_char_boundary(found) && text.is_char_boundary(end) {
            out.push_str(&text[at..found]);
        } else if text.is_char_boundary(end) {
            out.push_str(&text[at..end]);
        } else {
            break;
        }
        at = end;
    }
    out.push_str(text.get(at..).unwrap_or_default());
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

thread_local! {
    static QUEUES: RefCell<HashMap<String, Queue>> = RefCell::new(HashMap::new());
    static PAUSED: Cell<bool> = const { Cell::new(false) };
    static DISABLED: Cell<bool> = const { Cell::new(false) };
    /// Wakes the queues when speech is resumed.
    static RESUMED: Rc<tokio::sync::Notify> = Rc::new(tokio::sync::Notify::new());
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
            let Some(mut item) = next else { return };
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
        let wav = match crate::voice::speak(&item.text, &item.voice, item.speed, item.pitch).await {
            Ok(wav) => wav,
            Err(e) => {
                crate::warn(format!("Speak: {e}"));
                self.speech_ended(item, None, "failed");
                return (None, "failed");
            }
        };
        let length = wav_length_ms(&wav);
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
            tokio::time::sleep(std::time::Duration::from_millis(length.unwrap_or(0))).await;
            self.speech_ended(item, length, "finished");
            return (length, "finished");
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
            Ok(started) => match started.ended.await {
                Ok(ended) => match ended.why {
                    sound::End::Finished => "finished",
                    sound::End::Stopped => "stopped",
                    sound::End::Failed => "failed",
                },
                Err(_) => "failed",
            },
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
        sound::stop(Some(SPEECH), 0)
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
            "voices" => crate::voice::voices().map(|v| json!({"voices": v, "default": self.config().voices.voice})),
            other => Err(format!(
                "unknown op {other}: speak, stop, clear, pause, enable, state or voices"
            )),
        }
    }

    pub fn speech_state(&self) -> Value {
        json!({"paused": PAUSED.get(), "enabled": !DISABLED.get(), "waiting": waiting()})
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
            99327 => match crate::voice::voices() {
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
        // Bots and the users named are never read.
        let skip_users = words("skipUsers");
        if !user.is_empty() && skip_users.iter().any(|u| u.eq_ignore_ascii_case(&user)) {
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
        let wav = match crate::voice::speak(&item.text, &item.voice, item.speed, item.pitch).await {
            Ok(wav) => wav,
            Err(e) => return crate::warn(format!("Speak To File: {e}")),
        };
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
    if wav.len() < 44 || &wav[0..4] != b"RIFF" {
        return None;
    }
    let mut at = 12;
    let mut rate = None;
    while at + 8 <= wav.len() {
        let size = u32::from_le_bytes(wav[at + 4..at + 8].try_into().ok()?) as usize;
        match &wav[at..at + 4] {
            b"fmt " if at + 16 <= wav.len() => {
                rate = Some(u32::from_le_bytes(wav[at + 16..at + 20].try_into().ok()?) as u64);
            }
            b"data" => {
                let bytes = size.min(wav.len() - at - 8) as u64;
                return rate.filter(|r| *r > 0).map(|r| bytes * 1000 / r);
            }
            _ => {}
        }
        at += 8 + size + (size & 1);
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
}
