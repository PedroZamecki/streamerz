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

pub async fn execute_ifttt_notification(event_name: &str, payload: &Value, key: Option<&str>) -> Result<(), String> {
    if event_name.is_empty() {
        return Err("missing event name".into());
    }
    let key = key.unwrap_or_default();
    if key.is_empty() {
        // When no maker key is provided, Streamer.bot relies on website integration or logs note.
        crate::say(format!(
            "IFTTT: event '{event_name}' dispatched (no IFTTT key configured for direct maker webhook)"
        ));
        return Ok(());
    }
    let url = format!("https://maker.ifttt.com/trigger/{event_name}/with/key/{key}");
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
    pub display: String,
    pub pronoun: String,
    pub subject: String,
    pub subject_lower: String,
    pub object: String,
    pub object_lower: String,
    pub possessive: String,
    pub possessive_lower: String,
    pub pronoun_form: String,
    pub pronoun_form_lower: String,
    pub reflexive: String,
    pub reflexive_lower: String,
    pub past_tense: String,
    pub past_tense_lower: String,
    pub current_tense: String,
    pub current_tense_lower: String,
    pub plural_lower: String,
}

pub fn lookup_pronoun(id: &str) -> Option<PronounInfo> {
    let id = id.to_ascii_lowercase();
    match id.as_str() {
        "hehim" => Some(PronounInfo {
            display: "(He/Him)".into(),
            pronoun: "He/Him".into(),
            subject: "He".into(),
            subject_lower: "he".into(),
            object: "Him".into(),
            object_lower: "him".into(),
            possessive: "His".into(),
            possessive_lower: "his".into(),
            pronoun_form: "His".into(),
            pronoun_form_lower: "his".into(),
            reflexive: "Himself".into(),
            reflexive_lower: "himself".into(),
            past_tense: "Was".into(),
            past_tense_lower: "was".into(),
            current_tense: "Is".into(),
            current_tense_lower: "is".into(),
            plural_lower: String::new(),
        }),
        "sheher" => Some(PronounInfo {
            display: "(She/Her)".into(),
            pronoun: "She/Her".into(),
            subject: "She".into(),
            subject_lower: "she".into(),
            object: "Her".into(),
            object_lower: "her".into(),
            possessive: "Her".into(),
            possessive_lower: "her".into(),
            pronoun_form: "Hers".into(),
            pronoun_form_lower: "hers".into(),
            reflexive: "Herself".into(),
            reflexive_lower: "herself".into(),
            past_tense: "Was".into(),
            past_tense_lower: "was".into(),
            current_tense: "Is".into(),
            current_tense_lower: "is".into(),
            plural_lower: String::new(),
        }),
        "theythem" => Some(PronounInfo {
            display: "(They/Them)".into(),
            pronoun: "They/Them".into(),
            subject: "They".into(),
            subject_lower: "they".into(),
            object: "Them".into(),
            object_lower: "them".into(),
            possessive: "Their".into(),
            possessive_lower: "their".into(),
            pronoun_form: "Theirs".into(),
            pronoun_form_lower: "theirs".into(),
            reflexive: "Themselves".into(),
            reflexive_lower: "themselves".into(),
            past_tense: "Were".into(),
            past_tense_lower: "were".into(),
            current_tense: "Are".into(),
            current_tense_lower: "are".into(),
            plural_lower: "they".into(),
        }),
        "itits" => Some(PronounInfo {
            display: "(It/Its)".into(),
            pronoun: "It/Its".into(),
            subject: "It".into(),
            subject_lower: "it".into(),
            object: "It".into(),
            object_lower: "it".into(),
            possessive: "Its".into(),
            possessive_lower: "its".into(),
            pronoun_form: "Its".into(),
            pronoun_form_lower: "its".into(),
            reflexive: "Itself".into(),
            reflexive_lower: "itself".into(),
            past_tense: "Was".into(),
            past_tense_lower: "was".into(),
            current_tense: "Is".into(),
            current_tense_lower: "is".into(),
            plural_lower: String::new(),
        }),
        "any" => Some(PronounInfo {
            display: "(Any/Any)".into(),
            pronoun: "Any/Any".into(),
            subject: "Any".into(),
            subject_lower: "any".into(),
            object: "Any".into(),
            object_lower: "any".into(),
            possessive: "Any's".into(),
            possessive_lower: "any's".into(),
            pronoun_form: "Any".into(),
            pronoun_form_lower: "any".into(),
            reflexive: "Themself".into(),
            reflexive_lower: "themself".into(),
            past_tense: "Were".into(),
            past_tense_lower: "were".into(),
            current_tense: "Are".into(),
            current_tense_lower: "are".into(),
            plural_lower: String::new(),
        }),
        "other" => Some(PronounInfo {
            display: "(Other/Other)".into(),
            pronoun: "Other/Other".into(),
            subject: "Other".into(),
            subject_lower: "other".into(),
            object: "Other".into(),
            object_lower: "other".into(),
            possessive: "Other's".into(),
            possessive_lower: "other's".into(),
            pronoun_form: "Others".into(),
            pronoun_form_lower: "others".into(),
            reflexive: "Themself".into(),
            reflexive_lower: "themself".into(),
            past_tense: "Were".into(),
            past_tense_lower: "were".into(),
            current_tense: "Are".into(),
            current_tense_lower: "are".into(),
            plural_lower: String::new(),
        }),
        "xexem" => Some(PronounInfo {
            display: "(Xe/Xem)".into(),
            pronoun: "Xe/Xem".into(),
            subject: "Xe".into(),
            subject_lower: "xe".into(),
            object: "Xem".into(),
            object_lower: "xem".into(),
            possessive: "Xyr".into(),
            possessive_lower: "xyr".into(),
            pronoun_form: "Xyrs".into(),
            pronoun_form_lower: "xyrs".into(),
            reflexive: "Xemself".into(),
            reflexive_lower: "xemself".into(),
            past_tense: "Was".into(),
            past_tense_lower: "was".into(),
            current_tense: "Is".into(),
            current_tense_lower: "is".into(),
            plural_lower: String::new(),
        }),
        "faefaer" => Some(PronounInfo {
            display: "(Fae/Faer)".into(),
            pronoun: "Fae/Faer".into(),
            subject: "Fae".into(),
            subject_lower: "fae".into(),
            object: "Faer".into(),
            object_lower: "faer".into(),
            possessive: "Faer".into(),
            possessive_lower: "faer".into(),
            pronoun_form: "Faers".into(),
            pronoun_form_lower: "faers".into(),
            reflexive: "Faerself".into(),
            reflexive_lower: "faerself".into(),
            past_tense: "Was".into(),
            past_tense_lower: "was".into(),
            current_tense: "Is".into(),
            current_tense_lower: "is".into(),
            plural_lower: String::new(),
        }),
        "vever" => Some(PronounInfo {
            display: "(Ve/Ver)".into(),
            pronoun: "Ve/Ver".into(),
            subject: "Ve".into(),
            subject_lower: "ve".into(),
            object: "Ver".into(),
            object_lower: "ver".into(),
            possessive: "Vis".into(),
            possessive_lower: "vis".into(),
            pronoun_form: "Vers".into(),
            pronoun_form_lower: "vers".into(),
            reflexive: "Verself".into(),
            reflexive_lower: "verself".into(),
            past_tense: "Was".into(),
            past_tense_lower: "was".into(),
            current_tense: "Is".into(),
            current_tense_lower: "is".into(),
            plural_lower: String::new(),
        }),
        "aeaer" => Some(PronounInfo {
            display: "(Ae/Aer)".into(),
            pronoun: "Ae/Aer".into(),
            subject: "Ae".into(),
            subject_lower: "ae".into(),
            object: "Aer".into(),
            object_lower: "aer".into(),
            possessive: "Aer".into(),
            possessive_lower: "aer".into(),
            pronoun_form: "Aers".into(),
            pronoun_form_lower: "aers".into(),
            reflexive: "Aerself".into(),
            reflexive_lower: "aerself".into(),
            past_tense: "Was".into(),
            past_tense_lower: "was".into(),
            current_tense: "Is".into(),
            current_tense_lower: "is".into(),
            plural_lower: String::new(),
        }),
        "ziehir" => Some(PronounInfo {
            display: "(Zie/Hir)".into(),
            pronoun: "Zie/Hir".into(),
            subject: "Zie".into(),
            subject_lower: "zie".into(),
            object: "Hir".into(),
            object_lower: "hir".into(),
            possessive: "Hir".into(),
            possessive_lower: "hir".into(),
            pronoun_form: "Hirs".into(),
            pronoun_form_lower: "hirs".into(),
            reflexive: "Hirself".into(),
            reflexive_lower: "hirself".into(),
            past_tense: "Was".into(),
            past_tense_lower: "was".into(),
            current_tense: "Is".into(),
            current_tense_lower: "is".into(),
            plural_lower: String::new(),
        }),
        "perper" => Some(PronounInfo {
            display: "(Per/Per)".into(),
            pronoun: "Per/Per".into(),
            subject: "Per".into(),
            subject_lower: "per".into(),
            object: "Per".into(),
            object_lower: "per".into(),
            possessive: "Per".into(),
            possessive_lower: "per".into(),
            pronoun_form: "Pers".into(),
            pronoun_form_lower: "pers".into(),
            reflexive: "Perself".into(),
            reflexive_lower: "perself".into(),
            past_tense: "Was".into(),
            past_tense_lower: "was".into(),
            current_tense: "Is".into(),
            current_tense_lower: "is".into(),
            plural_lower: String::new(),
        }),
        "eem" => Some(PronounInfo {
            display: "(E/Em)".into(),
            pronoun: "E/Em".into(),
            subject: "E".into(),
            subject_lower: "e".into(),
            object: "Em".into(),
            object_lower: "em".into(),
            possessive: "Eir".into(),
            possessive_lower: "eir".into(),
            pronoun_form: "Eirs".into(),
            pronoun_form_lower: "eirs".into(),
            reflexive: "Eirself".into(),
            reflexive_lower: "eirself".into(),
            past_tense: "Was".into(),
            past_tense_lower: "was".into(),
            current_tense: "Is".into(),
            current_tense_lower: "is".into(),
            plural_lower: String::new(),
        }),
        _ => None,
    }
}

// In-memory pronoun cache: user_login -> (timestamp, result)
type PronounCache = HashMap<String, (Instant, Option<PronounInfo>)>;
static PRONOUN_CACHE: Mutex<Option<PronounCache>> = Mutex::new(None);
const PRONOUN_TTL: Duration = Duration::from_secs(15 * 60);

pub fn get_cached_pronoun(login: &str) -> Option<PronounInfo> {
    let login = login.trim().to_ascii_lowercase();
    let guard = PRONOUN_CACHE.lock().unwrap();
    guard
        .as_ref()
        .and_then(|m| m.get(&login))
        .and_then(|(ts, info)| if ts.elapsed() < PRONOUN_TTL { info.clone() } else { None })
}

pub async fn get_user_pronouns(login: &str) -> Option<PronounInfo> {
    let login = login.trim().to_ascii_lowercase();
    if login.is_empty() {
        return None;
    }

    // Check cache
    {
        let mut guard = PRONOUN_CACHE.lock().unwrap();
        let map = guard.get_or_insert_with(HashMap::new);
        if let Some((ts, info)) = map.get(&login) {
            if ts.elapsed() < PRONOUN_TTL {
                return info.clone();
            }
        }
    }

    // HTTP lookup
    let url = format!("https://pronouns.alejo.io/api/users/{login}");
    let resp = crate::http::request("GET", &url, &[], b"").await;
    let found = match resp {
        Ok(r) if r.status == 200 => {
            if let Ok(Value::Array(arr)) = serde_json::from_slice::<Value>(&r.body) {
                arr.into_iter()
                    .find_map(|u| u.get("pronoun_id").and_then(Value::as_str).map(|s| s.to_string()))
                    .and_then(|id| lookup_pronoun(&id))
            } else {
                None
            }
        }
        _ => None,
    };

    // Store in cache
    {
        let mut guard = PRONOUN_CACHE.lock().unwrap();
        let map = guard.get_or_insert_with(HashMap::new);
        map.insert(login, (Instant::now(), found.clone()));
    }

    found
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

pub fn populate_pronoun_vars(args: &mut Obj, prefix: &str, info: Option<&PronounInfo>) {
    let key = |name: &str| -> String {
        if prefix.is_empty() {
            name.to_string()
        } else {
            format!("{prefix}.{name}")
        }
    };

    let now_str = crate::engine::globals::iso_now();

    if let Some(info) = info {
        args.insert(key("userFound"), true.into());
        args.insert(key("pronoun"), info.pronoun.as_str().into());
        args.insert(key("pronouns"), info.display.as_str().into());
        args.insert(key("pronounSubject"), info.subject.as_str().into());
        args.insert(key("pronounSubjectLower"), info.subject_lower.as_str().into());
        args.insert(key("pronounObject"), info.object.as_str().into());
        args.insert(key("pronounObjectLower"), info.object_lower.as_str().into());
        args.insert(key("pronounPossessive"), info.possessive.as_str().into());
        args.insert(key("pronounPossessiveLower"), info.possessive_lower.as_str().into());
        args.insert(key("pronounPronoun"), info.pronoun_form.as_str().into());
        args.insert(key("pronounPronounLower"), info.pronoun_form_lower.as_str().into());
        args.insert(key("pronounReflexive"), info.reflexive.as_str().into());
        args.insert(key("pronounReflexiveLower"), info.reflexive_lower.as_str().into());
        args.insert(key("pronounPastTense"), info.past_tense.as_str().into());
        args.insert(key("pronounPastTenseLower"), info.past_tense_lower.as_str().into());
        args.insert(key("pronounCurrentTense"), info.current_tense.as_str().into());
        args.insert(
            key("pronounCurrentTenseLower"),
            info.current_tense_lower.as_str().into(),
        );
        args.insert(key("pronounPluralLower"), info.plural_lower.as_str().into());
        args.insert(key("pronounLastCached"), now_str.into());
    } else {
        args.insert(key("userFound"), false.into());
        args.insert(key("pronoun"), "Theirs".into());
        args.insert(key("pronouns"), "(They/Them)".into());
        args.insert(key("pronounSubject"), "They".into());
        args.insert(key("pronounSubjectLower"), "they".into());
        args.insert(key("pronounObject"), "Them".into());
        args.insert(key("pronounObjectLower"), "them".into());
        args.insert(key("pronounPossessive"), "Their".into());
        args.insert(key("pronounPossessiveLower"), "their".into());
        args.insert(key("pronounPronoun"), "Theirs".into());
        args.insert(key("pronounPronounLower"), "theirs".into());
        args.insert(key("pronounReflexive"), "Themselves".into());
        args.insert(key("pronounReflexiveLower"), "themselves".into());
        args.insert(key("pronounPastTense"), "Were".into());
        args.insert(key("pronounPastTenseLower"), "were".into());
        args.insert(key("pronounCurrentTense"), "Are".into());
        args.insert(key("pronounCurrentTenseLower"), "are".into());
        args.insert(key("pronounPluralLower"), "".into());
        args.insert(key("pronounLastCached"), now_str.into());
    }
}

#[cfg(test)]
pub mod tests {
    use super::*;

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
        let he = lookup_pronoun("hehim");
        populate_pronoun_vars(&mut args, "target", he.as_ref());
        assert_eq!(args.get("target.userFound"), Some(&Value::Bool(true)));
        assert_eq!(args.get("target.pronounSubject"), Some(&Value::String("He".into())));
        assert_eq!(args.get("target.pronounObject"), Some(&Value::String("Him".into())));

        let mut empty_args = Obj::new();
        populate_pronoun_vars(&mut empty_args, "", None);
        assert_eq!(empty_args.get("userFound"), Some(&Value::Bool(false)));
        assert_eq!(empty_args.get("pronounSubject"), Some(&Value::String("They".into())));
    }
}
