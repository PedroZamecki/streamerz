//! Webhooks and remote integrations (Phase 14):
//! - Discord Webhook (sub-action 7001): POST text/embeds to Discord webhooks.
//! - IFTTT (sub-actions 8001, 8002): Basic and Advanced notification triggers to IFTTT webhooks.
//! - Pronouns (sub-action 15001): alejo.io Twitch Chat Pronouns API lookup, cached and mapped to variables.
//! - Streamer.bot Remote instances (sub-actions 20001..=20003, triggers 29001..=29004):
//!   inter-instance triggers, signals and signal waiting over WebSocket.

use crate::model::Obj;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

// Sub-action IDs
pub const S_DISCORD_WEBHOOK: u64 = 7001;
pub const S_IFTTT_BASIC: u64 = 8001;
pub const S_IFTTT_ADVANCED: u64 = 8002;
pub const S_PRONOUNS_ADD_FOR_USER: u64 = 15001;
pub const S_REMOTE_SEND_TRIGGER: u64 = 20001;
pub const S_REMOTE_SEND_SIGNAL: u64 = 20002;
pub const S_REMOTE_WAIT_FOR_SIGNAL: u64 = 20003;

// Trigger IDs
pub const T_REMOTE_CONNECTED: u32 = 29001;
pub const T_REMOTE_DISCONNECTED: u32 = 29002;
pub const T_REMOTE_TRIGGER: u32 = 29003;
pub const T_REMOTE_SIGNAL: u32 = 29004;

// ---------------------------------------------------------------------------
// 1. Discord Webhooks (7001)
// ---------------------------------------------------------------------------

pub fn build_discord_payload(content: &str, username: &str, avatar: &str, image: &str, tts: bool) -> Value {
    let mut payload = json!({
        "content": content,
        "tts": tts,
    });
    let obj = payload.as_object_mut().unwrap();
    if !username.is_empty() {
        obj.insert("username".into(), username.into());
    }
    if !avatar.is_empty() {
        obj.insert("avatar_url".into(), avatar.into());
    }
    if !image.is_empty() {
        obj.insert("embeds".into(), json!([{"image": {"url": image}}]));
    }
    payload
}

pub async fn execute_discord_webhook(
    url: &str,
    content: &str,
    username: &str,
    avatar: &str,
    image: &str,
    tts: bool,
) -> Result<String, String> {
    if url.is_empty() {
        return Err("missing webhook URL".into());
    }
    // wait=true: Discord answers with the message (its id), not 204.
    let url = if url.contains('?') {
        format!("{url}&wait=true")
    } else {
        format!("{url}?wait=true")
    };
    let url = url.as_str();
    let payload = build_discord_payload(content, username, avatar, image, tts);
    let body = serde_json::to_vec(&payload).map_err(|e| e.to_string())?;
    let headers = vec![("Content-Type".into(), "application/json".into())];
    let resp = crate::http::request("POST", url, &headers, &body).await?;
    if (200..300).contains(&resp.status) {
        let v: Value = serde_json::from_slice(&resp.body).unwrap_or_default();
        Ok(v["id"].as_str().unwrap_or("").to_string())
    } else {
        Err(format!("HTTP {}", resp.status))
    }
}

/// `text` cut to `max` characters (Discord refuses what is longer).
fn clip(text: &str, max: usize) -> String {
    text.chars().take(max).collect()
}

/// A colour as Discord wants it: the number of `#rrggbb`. None when it isn't one.
pub fn discord_color(text: &str) -> Option<u32> {
    let hex = text.trim().trim_start_matches('#');
    if !matches!(hex.len(), 6 | 8) || !hex.bytes().all(|c| c.is_ascii_hexdigit()) {
        return None;
    }
    u32::from_str_radix(&hex[hex.len() - 6..], 16).ok()
}

/// What an embed is made of, as typed (variables already filled in).
#[derive(Default)]
pub struct Embed {
    pub title: String,
    pub description: String,
    pub color: String,
    pub url: String,
    pub author: String,
    pub footer: String,
    pub image: String,
    pub thumbnail: String,
    /// `name=value` lines.
    pub fields: Vec<(String, String)>,
    pub inline: bool,
    pub timestamp: Option<String>,
}

impl Embed {
    /// Whether it has anything to show.
    pub fn is_empty(&self) -> bool {
        self.title.is_empty()
            && self.description.is_empty()
            && self.author.is_empty()
            && self.footer.is_empty()
            && self.image.is_empty()
            && self.thumbnail.is_empty()
            && self.fields.is_empty()
    }

    /// The embed as Discord's JSON, within its limits (a title of 256, a description of 4096, 25 fields).
    pub fn json(&self) -> Value {
        let mut e = json!({});
        let mut set = |k: &str, v: Value| drop(e.as_object_mut().map(|o| o.insert(k.into(), v)));
        if !self.title.is_empty() {
            set("title", clip(&self.title, 256).into());
        }
        if !self.description.is_empty() {
            set("description", clip(&self.description, 4096).into());
        }
        if !self.url.is_empty() {
            set("url", self.url.clone().into());
        }
        if let Some(color) = discord_color(&self.color) {
            set("color", color.into());
        }
        if !self.author.is_empty() {
            set("author", json!({"name": clip(&self.author, 256)}));
        }
        if !self.footer.is_empty() {
            set("footer", json!({"text": clip(&self.footer, 2048)}));
        }
        if !self.image.is_empty() {
            set("image", json!({"url": self.image}));
        }
        if !self.thumbnail.is_empty() {
            set("thumbnail", json!({"url": self.thumbnail}));
        }
        let fields: Vec<Value> = self
            .fields
            .iter()
            .filter(|(name, value)| !name.is_empty() && !value.is_empty())
            .take(25)
            .map(|(name, value)| json!({"name": clip(name, 256), "value": clip(value, 1024), "inline": self.inline}))
            .collect();
        if !fields.is_empty() {
            set("fields", fields.into());
        }
        if let Some(when) = &self.timestamp {
            set("timestamp", when.clone().into());
        }
        e
    }
}

/// The message to post: text, an embed if it has one, and who it shows as.
pub fn discord_message(content: &str, username: &str, avatar: &str, embed: &Embed, thread_name: &str) -> Value {
    let mut m = json!({});
    let mut set = |k: &str, v: Value| drop(m.as_object_mut().map(|o| o.insert(k.into(), v)));
    if !content.is_empty() {
        set("content", clip(content, 2000).into());
    }
    if !username.is_empty() {
        set("username", clip(username, 80).into());
    }
    if !avatar.is_empty() {
        set("avatar_url", avatar.into());
    }
    if !embed.is_empty() {
        set("embeds", json!([embed.json()]));
    }
    // A forum channel starts a post with this name.
    if !thread_name.is_empty() {
        set("thread_name", clip(thread_name, 100).into());
    }
    m
}

/// A webhook's address with what a call needs: no query, and `/messages/{id}` when it has a message.
fn discord_address(url: &str, message: &str, thread: &str, wait: bool) -> Result<String, String> {
    let base = url.split('?').next().unwrap_or_default().trim_end_matches('/');
    if base.is_empty() {
        return Err("missing webhook URL".into());
    }
    let mut out = base.to_string();
    if !message.is_empty() {
        out += &format!("/messages/{}", crate::commands::url_encode(message));
    }
    let mut query = Vec::new();
    if wait {
        query.push("wait=true".to_string());
    }
    if !thread.is_empty() {
        query.push(format!("thread_id={}", crate::commands::url_encode(thread)));
    }
    if !query.is_empty() {
        out += &format!("?{}", query.join("&"));
    }
    Ok(out)
}

/// One call to a webhook: post (`message` empty), edit or delete a message; the answer's message.
pub async fn discord_call(
    method: &str,
    url: &str,
    message: &str,
    thread: &str,
    body: Option<&Value>,
) -> Result<Value, String> {
    let address = discord_address(url, message, thread, method == "POST")?;
    let headers = vec![("Content-Type".into(), "application/json".into())];
    let bytes = body
        .map(|b| serde_json::to_vec(b).unwrap_or_default())
        .unwrap_or_default();
    let resp = crate::http::request(method, &address, &headers, &bytes).await?;
    if (200..300).contains(&resp.status) {
        Ok(serde_json::from_slice(&resp.body).unwrap_or_default())
    } else {
        let said = serde_json::from_slice::<Value>(&resp.body)
            .ok()
            .and_then(|v| v["message"].as_str().map(str::to_string))
            .unwrap_or_default();
        Err(format!("HTTP {} {said}", resp.status).trim().to_string())
    }
}

/// A multipart body: the message as JSON and one file (Discord's `payload_json` and `files[0]`).
pub fn discord_upload_body(boundary: &str, payload: &Value, filename: &str, file: &[u8]) -> Vec<u8> {
    let safe = filename.replace(['"', '\r', '\n'], "_");
    let head = format!(
        "--{boundary}\r\nContent-Disposition: form-data; name=\"payload_json\"\r\nContent-Type: application/json\r\n\r\n{payload}\r\n\
         --{boundary}\r\nContent-Disposition: form-data; name=\"files[0]\"; filename=\"{safe}\"\r\nContent-Type: application/octet-stream\r\n\r\n"
    );
    let tail = format!("\r\n--{boundary}--\r\n");
    let mut out = Vec::with_capacity(head.len() + file.len() + tail.len());
    out.extend_from_slice(head.as_bytes());
    out.extend_from_slice(file);
    out.extend_from_slice(tail.as_bytes());
    out
}

/// A file to upload, at most 25 MB. Only a regular file: a pipe or a device (`/dev/zero`) would never
/// end, and the read stops past the limit even when the file grows while it's read.
fn read_upload(path: &str) -> Result<Vec<u8>, String> {
    use std::io::Read;
    const MOST: u64 = 25 * 1024 * 1024;
    let too_big = || format!("{path} is over 25 MB, which Discord won't take");
    let file = std::fs::File::open(path).map_err(|e| format!("{path}: {e}"))?;
    let meta = file.metadata().map_err(|e| format!("{path}: {e}"))?;
    if !meta.is_file() {
        return Err(format!("{path} is not a file"));
    }
    if meta.len() > MOST {
        return Err(too_big());
    }
    let mut out = Vec::with_capacity(meta.len() as usize);
    file.take(MOST + 1)
        .read_to_end(&mut out)
        .map_err(|e| format!("{path}: {e}"))?;
    if out.len() as u64 > MOST {
        return Err(too_big());
    }
    Ok(out)
}

/// Posts a file (read now, at most 25 MB) with a message to a webhook; the message's id.
pub async fn discord_upload(
    url: &str,
    thread: &str,
    payload: &Value,
    path: &str,
    name: &str,
) -> Result<String, String> {
    let file = read_upload(path)?;
    let shown = if name.is_empty() {
        std::path::Path::new(path)
            .file_name()
            .map_or("file".into(), |n| n.to_string_lossy().into_owned())
    } else {
        name.to_string()
    };
    let boundary = format!("streamerz{:016x}", crate::math::secure_u64());
    let body = discord_upload_body(&boundary, payload, &shown, &file);
    drop(file);
    let address = discord_address(url, "", thread, true)?;
    let headers = vec![(
        "Content-Type".into(),
        format!("multipart/form-data; boundary={boundary}"),
    )];
    let resp = crate::http::request("POST", &address, &headers, &body).await?;
    if (200..300).contains(&resp.status) {
        Ok(serde_json::from_slice::<Value>(&resp.body).unwrap_or_default()["id"]
            .as_str()
            .unwrap_or("")
            .to_string())
    } else {
        Err(format!("HTTP {}", resp.status))
    }
}

// ---------------------------------------------------------------------------
// 2. IFTTT Notifications (8001, 8002)
// ---------------------------------------------------------------------------

pub fn build_ifttt_basic_payload(text: &str) -> Value {
    json!({ "value1": text })
}

pub fn build_ifttt_advanced_payload(args: &Obj) -> Value {
    let mut obj = json!({});
    let target = obj.as_object_mut().unwrap();
    for (i, (k, v)) in args.iter().enumerate() {
        if i < 5 {
            target.insert(format!("Argument{}Name", i + 1), json!(k));
            target.insert(format!("Argument{}Value", i + 1), v.clone());
        }
    }
    target.insert(
        "ArgumentsJson".into(),
        json!(serde_json::to_string(args).unwrap_or_default()),
    );
    // Also include standard value1..value3 if present in args
    for (src, dst) in [("value1", "value1"), ("value2", "value2"), ("value3", "value3")] {
        if let Some(val) = args.get(src) {
            target.insert(dst.into(), val.clone());
        }
    }
    obj
}

/// The Maker webhook address for an event. Without a key nothing can be sent, so that's an error the
/// user sees, not a quiet success.
fn ifttt_address(event: &str, key: Option<&str>) -> Result<String, String> {
    if event.is_empty() {
        return Err("missing event name".into());
    }
    let key = key.filter(|k| !k.is_empty()).ok_or("no IFTTT webhook key set")?;
    // Each is one path segment: a "/" or "?" in an event name can't change the address.
    let segment = |s: &str| crate::commands::url_encode(s).replace('+', "%20");
    Ok(format!(
        "https://maker.ifttt.com/trigger/{}/with/key/{}",
        segment(event),
        segment(key)
    ))
}

pub async fn execute_ifttt_notification(event_name: &str, payload: &Value, key: Option<&str>) -> Result<(), String> {
    let url = ifttt_address(event_name, key)?;
    let body = serde_json::to_vec(payload).map_err(|e| e.to_string())?;
    let headers = vec![("Content-Type".into(), "application/json".into())];
    let resp = crate::http::request("POST", &url, &headers, &body).await?;
    if (200..300).contains(&resp.status) {
        Ok(())
    } else {
        Err(format!("HTTP {}", resp.status))
    }
}

// ---------------------------------------------------------------------------
// 3. Pronouns (alejo.io) (15001)
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, PartialEq)]
pub struct PronounInfo {
    pub display: &'static str,
    pub pronoun: &'static str,
    pub subject: &'static str,
    pub subject_lower: &'static str,
    pub object: &'static str,
    pub object_lower: &'static str,
    pub possessive: &'static str,
    pub possessive_lower: &'static str,
    pub pronoun_form: &'static str,
    pub pronoun_form_lower: &'static str,
    pub reflexive: &'static str,
    pub reflexive_lower: &'static str,
    pub past_tense: &'static str,
    pub past_tense_lower: &'static str,
    pub current_tense: &'static str,
    pub current_tense_lower: &'static str,
    pub plural_lower: &'static str,
}

pub fn lookup_pronoun(id: &str) -> Option<PronounInfo> {
    let id = id.to_ascii_lowercase();
    match id.as_str() {
        "hehim" => Some(PronounInfo {
            display: "(He/Him)",
            pronoun: "He/Him",
            subject: "He",
            subject_lower: "he",
            object: "Him",
            object_lower: "him",
            possessive: "His",
            possessive_lower: "his",
            pronoun_form: "His",
            pronoun_form_lower: "his",
            reflexive: "Himself",
            reflexive_lower: "himself",
            past_tense: "Was",
            past_tense_lower: "was",
            current_tense: "Is",
            current_tense_lower: "is",
            plural_lower: "",
        }),
        "sheher" => Some(PronounInfo {
            display: "(She/Her)",
            pronoun: "She/Her",
            subject: "She",
            subject_lower: "she",
            object: "Her",
            object_lower: "her",
            possessive: "Her",
            possessive_lower: "her",
            pronoun_form: "Hers",
            pronoun_form_lower: "hers",
            reflexive: "Herself",
            reflexive_lower: "herself",
            past_tense: "Was",
            past_tense_lower: "was",
            current_tense: "Is",
            current_tense_lower: "is",
            plural_lower: "",
        }),
        "theythem" => Some(PronounInfo {
            display: "(They/Them)",
            pronoun: "They/Them",
            subject: "They",
            subject_lower: "they",
            object: "Them",
            object_lower: "them",
            possessive: "Their",
            possessive_lower: "their",
            pronoun_form: "Theirs",
            pronoun_form_lower: "theirs",
            reflexive: "Themselves",
            reflexive_lower: "themselves",
            past_tense: "Were",
            past_tense_lower: "were",
            current_tense: "Are",
            current_tense_lower: "are",
            plural_lower: "they",
        }),
        "itits" => Some(PronounInfo {
            display: "(It/Its)",
            pronoun: "It/Its",
            subject: "It",
            subject_lower: "it",
            object: "It",
            object_lower: "it",
            possessive: "Its",
            possessive_lower: "its",
            pronoun_form: "Its",
            pronoun_form_lower: "its",
            reflexive: "Itself",
            reflexive_lower: "itself",
            past_tense: "Was",
            past_tense_lower: "was",
            current_tense: "Is",
            current_tense_lower: "is",
            plural_lower: "",
        }),
        "any" => Some(PronounInfo {
            display: "(Any/Any)",
            pronoun: "Any/Any",
            subject: "Any",
            subject_lower: "any",
            object: "Any",
            object_lower: "any",
            possessive: "Any's",
            possessive_lower: "any's",
            pronoun_form: "Any",
            pronoun_form_lower: "any",
            reflexive: "Themself",
            reflexive_lower: "themself",
            past_tense: "Were",
            past_tense_lower: "were",
            current_tense: "Are",
            current_tense_lower: "are",
            plural_lower: "",
        }),
        "other" => Some(PronounInfo {
            display: "(Other/Other)",
            pronoun: "Other/Other",
            subject: "Other",
            subject_lower: "other",
            object: "Other",
            object_lower: "other",
            possessive: "Other's",
            possessive_lower: "other's",
            pronoun_form: "Others",
            pronoun_form_lower: "others",
            reflexive: "Themself",
            reflexive_lower: "themself",
            past_tense: "Were",
            past_tense_lower: "were",
            current_tense: "Are",
            current_tense_lower: "are",
            plural_lower: "",
        }),
        "xexem" => Some(PronounInfo {
            display: "(Xe/Xem)",
            pronoun: "Xe/Xem",
            subject: "Xe",
            subject_lower: "xe",
            object: "Xem",
            object_lower: "xem",
            possessive: "Xyr",
            possessive_lower: "xyr",
            pronoun_form: "Xyrs",
            pronoun_form_lower: "xyrs",
            reflexive: "Xemself",
            reflexive_lower: "xemself",
            past_tense: "Was",
            past_tense_lower: "was",
            current_tense: "Is",
            current_tense_lower: "is",
            plural_lower: "",
        }),
        "faefaer" => Some(PronounInfo {
            display: "(Fae/Faer)",
            pronoun: "Fae/Faer",
            subject: "Fae",
            subject_lower: "fae",
            object: "Faer",
            object_lower: "faer",
            possessive: "Faer",
            possessive_lower: "faer",
            pronoun_form: "Faers",
            pronoun_form_lower: "faers",
            reflexive: "Faerself",
            reflexive_lower: "faerself",
            past_tense: "Was",
            past_tense_lower: "was",
            current_tense: "Is",
            current_tense_lower: "is",
            plural_lower: "",
        }),
        "vever" => Some(PronounInfo {
            display: "(Ve/Ver)",
            pronoun: "Ve/Ver",
            subject: "Ve",
            subject_lower: "ve",
            object: "Ver",
            object_lower: "ver",
            possessive: "Vis",
            possessive_lower: "vis",
            pronoun_form: "Vers",
            pronoun_form_lower: "vers",
            reflexive: "Verself",
            reflexive_lower: "verself",
            past_tense: "Was",
            past_tense_lower: "was",
            current_tense: "Is",
            current_tense_lower: "is",
            plural_lower: "",
        }),
        "aeaer" => Some(PronounInfo {
            display: "(Ae/Aer)",
            pronoun: "Ae/Aer",
            subject: "Ae",
            subject_lower: "ae",
            object: "Aer",
            object_lower: "aer",
            possessive: "Aer",
            possessive_lower: "aer",
            pronoun_form: "Aers",
            pronoun_form_lower: "aers",
            reflexive: "Aerself",
            reflexive_lower: "aerself",
            past_tense: "Was",
            past_tense_lower: "was",
            current_tense: "Is",
            current_tense_lower: "is",
            plural_lower: "",
        }),
        "ziehir" => Some(PronounInfo {
            display: "(Zie/Hir)",
            pronoun: "Zie/Hir",
            subject: "Zie",
            subject_lower: "zie",
            object: "Hir",
            object_lower: "hir",
            possessive: "Hir",
            possessive_lower: "hir",
            pronoun_form: "Hirs",
            pronoun_form_lower: "hirs",
            reflexive: "Hirself",
            reflexive_lower: "hirself",
            past_tense: "Was",
            past_tense_lower: "was",
            current_tense: "Is",
            current_tense_lower: "is",
            plural_lower: "",
        }),
        "perper" => Some(PronounInfo {
            display: "(Per/Per)",
            pronoun: "Per/Per",
            subject: "Per",
            subject_lower: "per",
            object: "Per",
            object_lower: "per",
            possessive: "Per",
            possessive_lower: "per",
            pronoun_form: "Pers",
            pronoun_form_lower: "pers",
            reflexive: "Perself",
            reflexive_lower: "perself",
            past_tense: "Was",
            past_tense_lower: "was",
            current_tense: "Is",
            current_tense_lower: "is",
            plural_lower: "",
        }),
        "eem" => Some(PronounInfo {
            display: "(E/Em)",
            pronoun: "E/Em",
            subject: "E",
            subject_lower: "e",
            object: "Em",
            object_lower: "em",
            possessive: "Eir",
            possessive_lower: "eir",
            pronoun_form: "Eirs",
            pronoun_form_lower: "eirs",
            reflexive: "Eirself",
            reflexive_lower: "eirself",
            past_tense: "Was",
            past_tense_lower: "was",
            current_tense: "Is",
            current_tense_lower: "is",
            plural_lower: "",
        }),
        _ => None,
    }
}

// In-memory pronoun cache: user_login -> (timestamp, result)
const PRONOUN_IDS: [&str; 13] = [
    "hehim", "sheher", "theythem", "itits", "any", "other", "xexem", "faefaer", "vever", "aeaer", "ziehir", "perper",
    "eem",
];
/// What a lookup found (a pronoun from [`PRONOUN_IDS`] and when it was looked up), or that the user
/// has none set.
type PronounCache = HashMap<String, (Instant, Option<(u8, String)>)>;
/// Chatters looked up in 15 minutes: past this, the expired ones go (all of them in a raid).
const MAX_PRONOUNS: usize = 2048;

fn cached_pronoun(id: Option<u8>) -> Option<PronounInfo> {
    id.and_then(|id| PRONOUN_IDS.get(id as usize))
        .and_then(|id| lookup_pronoun(id))
}
static PRONOUN_CACHE: Mutex<Option<PronounCache>> = Mutex::new(None);
const PRONOUN_TTL: Duration = Duration::from_secs(15 * 60);

/// A Twitch login as it goes in the lookup's path: letters, digits and `_` only, so a name from chat
/// can't point the request at another address.
fn twitch_login(login: &str) -> Option<String> {
    let login = login.trim().trim_start_matches('@');
    let valid = !login.is_empty() && login.len() <= 25 && login.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_');
    valid.then(|| login.to_ascii_lowercase())
}

/// A fresh entry for `login`: the pronoun and when it was cached.
fn cached_entry(cache: &PronounCache, login: &str) -> Option<Option<(PronounInfo, String)>> {
    let (_, found) = cache.get(login).filter(|(seen, _)| seen.elapsed() < PRONOUN_TTL)?;
    Some(
        found
            .as_ref()
            .and_then(|(id, at)| Some((cached_pronoun(Some(*id))?, at.clone()))),
    )
}

/// Keeps a lookup without letting the cache grow past [`MAX_PRONOUNS`].
fn remember(cache: &mut PronounCache, login: String, found: Option<(u8, String)>) {
    if cache.len() >= MAX_PRONOUNS {
        cache.retain(|_, (seen, _)| seen.elapsed() < PRONOUN_TTL);
        if cache.len() >= MAX_PRONOUNS {
            cache.clear();
        }
    }
    cache.insert(login, (Instant::now(), found));
}

pub fn get_cached_pronoun(login: &str) -> Option<PronounInfo> {
    let login = twitch_login(login)?;
    let guard = PRONOUN_CACHE.lock().unwrap();
    cached_entry(guard.as_ref()?, &login)?.map(|(info, _)| info)
}

/// The user's pronoun and when it was looked up. None when they have none set, or the lookup failed
/// (a failure isn't kept: the next call asks again).
pub async fn get_user_pronouns(login: &str) -> Option<(PronounInfo, String)> {
    let login = twitch_login(login)?;
    if let Some(found) = PRONOUN_CACHE
        .lock()
        .unwrap()
        .as_ref()
        .and_then(|c| cached_entry(c, &login))
    {
        return found;
    }
    let url = format!("https://pronouns.alejo.io/api/users/{login}");
    let resp = crate::http::request("GET", &url, &[], b"").await.ok()?;
    let id = match resp.status {
        200 => serde_json::from_slice::<Vec<Value>>(&resp.body)
            .ok()?
            .iter()
            .find_map(|u| u["pronoun_id"].as_str())
            .and_then(|id| PRONOUN_IDS.iter().position(|known| known.eq_ignore_ascii_case(id))),
        404 => None,
        _ => return None,
    };
    let found = id.map(|id| (id as u8, crate::engine::globals::iso_now()));
    let mut guard = PRONOUN_CACHE.lock().unwrap();
    remember(guard.get_or_insert_with(HashMap::new), login, found.clone());
    let (id, at) = found?;
    Some((cached_pronoun(Some(id))?, at))
}

pub fn pronoun_to_json(info: &PronounInfo, user_found: bool) -> Value {
    json!({
        "userFound": user_found,
        "pronoun": info.pronoun_form,
        "pronouns": info.display,
        "pronounSubject": info.subject,
        "pronounSubjectLower": info.subject_lower,
        "pronounObject": info.object,
        "pronounObjectLower": info.object_lower,
        "pronounPossessive": info.possessive,
        "pronounPossessiveLower": info.possessive_lower,
        "pronounPronoun": info.pronoun_form,
        "pronounPronounLower": info.pronoun_form_lower,
        "pronounReflexive": info.reflexive,
        "pronounReflexiveLower": info.reflexive_lower,
        "pronounPastTense": info.past_tense,
        "pronounPastTenseLower": info.past_tense_lower,
        "pronounCurrentTense": info.current_tense,
        "pronounCurrentTenseLower": info.current_tense_lower,
        "pronounPluralLower": if info.plural_lower.is_empty() { Value::Null } else { json!(info.plural_lower) },
    })
}

pub fn default_pronoun_json() -> Value {
    json!({
        "userFound": false,
        "pronoun": "Theirs",
        "pronouns": "(They/Them)",
        "pronounSubject": "They",
        "pronounSubjectLower": "they",
        "pronounObject": "Them",
        "pronounObjectLower": "them",
        "pronounPossessive": "Their",
        "pronounPossessiveLower": "their",
        "pronounPronoun": "Theirs",
        "pronounPronounLower": "theirs",
        "pronounReflexive": "Themselves",
        "pronounReflexiveLower": "themselves",
        "pastTense": "Were",
        "pronounPastTense": "Were",
        "pronounPastTenseLower": "were",
        "pronounCurrentTense": "Are",
        "pronounCurrentTenseLower": "are",
        "pronounPluralLower": Value::Null,
    })
}

/// The variables of Add Pronouns for User. A user with none set gets only `userFound: false`, as in
/// Streamer.bot; `pronounLastCached` is when the pronoun was looked up.
pub fn populate_pronoun_vars(args: &mut Obj, prefix: &str, info: Option<&(PronounInfo, String)>) {
    let key = |name: &str| -> String {
        if prefix.is_empty() {
            name.to_string()
        } else {
            format!("{prefix}.{name}")
        }
    };

    if let Some((info, cached)) = info {
        args.insert(key("userFound"), true.into());
        args.insert(key("pronoun"), info.pronoun.into());
        args.insert(key("pronouns"), info.display.into());
        args.insert(key("pronounSubject"), info.subject.into());
        args.insert(key("pronounSubjectLower"), info.subject_lower.into());
        args.insert(key("pronounObject"), info.object.into());
        args.insert(key("pronounObjectLower"), info.object_lower.into());
        args.insert(key("pronounPossessive"), info.possessive.into());
        args.insert(key("pronounPossessiveLower"), info.possessive_lower.into());
        args.insert(key("pronounPronoun"), info.pronoun_form.into());
        args.insert(key("pronounPronounLower"), info.pronoun_form_lower.into());
        args.insert(key("pronounReflexive"), info.reflexive.into());
        args.insert(key("pronounReflexiveLower"), info.reflexive_lower.into());
        args.insert(key("pronounPastTense"), info.past_tense.into());
        args.insert(key("pronounPastTenseLower"), info.past_tense_lower.into());
        args.insert(key("pronounCurrentTense"), info.current_tense.into());
        args.insert(key("pronounCurrentTenseLower"), info.current_tense_lower.into());
        args.insert(key("pronounPluralLower"), info.plural_lower.into());
        args.insert(key("pronounLastCached"), cached.as_str().into());
    } else {
        args.insert(key("userFound"), false.into());
    }
}

#[cfg(test)]
pub mod tests {
    use super::*;

    #[test]
    fn cached_pronouns_preserve_every_value() {
        for (i, id) in PRONOUN_IDS.iter().enumerate() {
            let expected = lookup_pronoun(id).expect("known pronoun");
            assert_eq!(cached_pronoun(Some(i as u8)), Some(expected));
        }
        assert_eq!(cached_pronoun(None), None);
        assert_eq!(cached_pronoun(Some(u8::MAX)), None);
    }

    #[test]
    fn test_discord_payload_building() {
        let p = build_discord_payload("hello world", "BotName", "http://avatar", "http://image", true);
        assert_eq!(p["content"], "hello world");
        assert_eq!(p["username"], "BotName");
        assert_eq!(p["avatar_url"], "http://avatar");
        assert_eq!(p["embeds"][0]["image"]["url"], "http://image");
        assert_eq!(p["tts"], true);

        let p2 = build_discord_payload("minimal", "", "", "", false);
        assert_eq!(p2["content"], "minimal");
        assert_eq!(p2["tts"], false);
        assert!(p2.get("username").is_none());
        assert!(p2.get("avatar_url").is_none());
        assert!(p2.get("embeds").is_none());
    }

    #[test]
    fn test_ifttt_payloads() {
        let basic = build_ifttt_basic_payload("Alert message");
        assert_eq!(basic["value1"], "Alert message");

        let mut args = Obj::new();
        args.insert("user".into(), "Alice".into());
        args.insert("count".into(), json!(42));
        args.insert("value1".into(), "direct1".into());

        let adv = build_ifttt_advanced_payload(&args);
        assert_eq!(adv["Argument1Name"], "user");
        assert_eq!(adv["Argument1Value"], "Alice");
        assert_eq!(adv["Argument2Name"], "count");
        assert_eq!(adv["Argument2Value"], 42);
        assert_eq!(adv["value1"], "direct1");
        assert!(adv["ArgumentsJson"].as_str().unwrap().contains("Alice"));
    }

    #[test]
    fn pronoun_logins_cannot_change_the_lookup_address() {
        assert_eq!(twitch_login(" @Some_User1 ").as_deref(), Some("some_user1"));
        for bad in ["", "two words", "../admin", "a?b", "a/b", "名前", &"a".repeat(26)] {
            assert_eq!(twitch_login(bad), None, "{bad}");
        }
    }

    #[test]
    fn pronoun_cache_stays_bounded() {
        let mut cache = PronounCache::new();
        for i in 0..MAX_PRONOUNS {
            remember(&mut cache, format!("u{i}"), None);
        }
        assert_eq!(cache.len(), MAX_PRONOUNS);
        assert_eq!(cached_entry(&cache, "u1"), Some(None));
        // Full of fresh entries: they all go for the new one.
        remember(&mut cache, "late".into(), Some((0, "then".into())));
        assert_eq!(cache.len(), 1);
        assert_eq!(
            cached_entry(&cache, "late"),
            Some(Some((lookup_pronoun("hehim").unwrap(), "then".into())))
        );
        assert_eq!(cached_entry(&cache, "u1"), None);
    }

    #[test]
    fn ifttt_needs_a_key_and_keeps_each_part_in_its_segment() {
        assert_eq!(
            ifttt_address("stream up/down?", Some("k+1")).unwrap(),
            "https://maker.ifttt.com/trigger/stream%20up%2Fdown%3F/with/key/k%2B1"
        );
        assert!(ifttt_address("live", None).is_err());
        assert!(ifttt_address("live", Some("")).is_err());
        assert!(ifttt_address("", Some("k")).is_err());
    }

    #[test]
    fn uploads_are_regular_files() {
        let dir = std::env::temp_dir().join(format!("streamerz-upload-{}", crate::engine::uuid()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("a.txt");
        std::fs::write(&file, b"hello").unwrap();
        assert_eq!(read_upload(file.to_str().unwrap()).unwrap(), b"hello");
        assert!(read_upload(dir.to_str().unwrap()).is_err());
        assert!(read_upload(dir.join("missing").to_str().unwrap()).is_err());
        #[cfg(unix)]
        assert!(read_upload("/dev/zero").unwrap_err().contains("not a file"));
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn test_pronoun_definitions() {
        let they = lookup_pronoun("theythem").unwrap();
        assert_eq!(they.display, "(They/Them)");
        assert_eq!(they.subject, "They");
        assert_eq!(they.subject_lower, "they");
        assert_eq!(they.past_tense, "Were");
        assert_eq!(they.current_tense, "Are");

        let she = lookup_pronoun("sheher").unwrap();
        assert_eq!(she.display, "(She/Her)");
        assert_eq!(she.subject, "She");
        assert_eq!(she.past_tense, "Was");
        assert_eq!(she.current_tense, "Is");

        let he = lookup_pronoun("hehim").unwrap();
        assert_eq!(he.display, "(He/Him)");
        assert_eq!(he.object, "Him");

        assert!(lookup_pronoun("unknown_id").is_none());
    }

    #[test]
    fn test_populate_pronoun_vars() {
        let mut args = Obj::new();
        let he = (lookup_pronoun("hehim").unwrap(), "2024-03-02T13:00:47Z".to_string());
        populate_pronoun_vars(&mut args, "target", Some(&he));
        assert_eq!(args["target.pronounLastCached"], "2024-03-02T13:00:47Z");
        assert_eq!(args.get("target.userFound"), Some(&Value::Bool(true)));
        assert_eq!(args.get("target.pronounSubject"), Some(&Value::String("He".into())));
        assert_eq!(args.get("target.pronounObject"), Some(&Value::String("Him".into())));

        let mut empty_args = Obj::new();
        populate_pronoun_vars(&mut empty_args, "", None);
        assert_eq!(empty_args.get("userFound"), Some(&Value::Bool(false)));
        // Streamer.bot sets only userFound for a user with no pronouns.
        assert_eq!(empty_args.len(), 1);
    }
}
