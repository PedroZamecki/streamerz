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
type PronounCache = HashMap<String, (Instant, Option<u8>)>;

fn cached_pronoun(id: Option<u8>) -> Option<PronounInfo> {
    id.and_then(|id| PRONOUN_IDS.get(id as usize))
        .and_then(|id| lookup_pronoun(id))
}
static PRONOUN_CACHE: Mutex<Option<PronounCache>> = Mutex::new(None);
const PRONOUN_TTL: Duration = Duration::from_secs(15 * 60);

pub fn get_cached_pronoun(login: &str) -> Option<PronounInfo> {
    let login = login.trim().to_ascii_lowercase();
    let guard = PRONOUN_CACHE.lock().unwrap();
    guard.as_ref().and_then(|m| m.get(&login)).and_then(|(ts, info)| {
        if ts.elapsed() < PRONOUN_TTL {
            cached_pronoun(*info)
        } else {
            None
        }
    })
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
                return cached_pronoun(*info);
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
        let id = found.as_ref().and_then(|info| {
            PRONOUN_IDS
                .iter()
                .position(|id| lookup_pronoun(id).as_ref() == Some(info))
                .map(|i| i as u8)
        });
        map.insert(login, (Instant::now(), id));
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
