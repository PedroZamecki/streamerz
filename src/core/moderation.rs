//! Moderation operations shared by the window and MCP.
use super::pages::{button, button_if};
use super::view::column;
use crate::{
    automation::schema::{F, W},
    engine::Engine,
    i18n::tr,
    model::Obj,
};
use serde_json::{json, Value};
use std::{cell::RefCell, rc::Rc};

pub const DOMAIN: &str = "moderation";

#[derive(Clone, serde::Serialize, serde::Deserialize)]
pub struct YouTubeBan {
    pub id: String,
    pub user_id: String,
    pub channel_id: String,
    pub live_chat_id: String,
    pub expires_at: Option<u64>,
}

pub const T_CROSS_MODERATION_APPLIED: u32 = 99700;
pub const T_CROSS_LINKED_USER_BANNED: u32 = 99701;
pub const T_CROSS_LINKED_USER_TIMED_OUT: u32 = 99702;
pub const T_CROSS_LINKED_USER_UNBANNED: u32 = 99703;

#[derive(Copy, Clone, Eq, PartialEq)]
pub enum LinkedModerationEvent {
    Banned,
    TimedOut,
    Unbanned,
}

#[derive(Clone, Default, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct LinkedPerson {
    pub id: String,
    pub name: String,
    pub twitch: String,
    pub kick: String,
    pub youtube: String,
    pub tiktok: String,
}

pub fn find_linked_person(engine: &Engine, query: &str, platform_hint: &str) -> Option<LinkedPerson> {
    let q = query.trim();
    if q.is_empty() {
        return None;
    }
    let clean = q.trim_start_matches('@');
    let hint = platform_hint.trim().to_ascii_lowercase();
    let config = engine.config();
    let field_eq = |stored: &str, candidate: &str| {
        !stored.is_empty()
            && (stored.eq_ignore_ascii_case(candidate)
                || stored
                    .trim_start_matches('@')
                    .eq_ignore_ascii_case(candidate.trim_start_matches('@')))
    };
    // A platform's ID names a person only on that platform: Twitch and Kick IDs are both numbers.
    if !hint.is_empty() {
        return config
            .moderation_links
            .iter()
            .find(|link| match hint.as_str() {
                "twitch" => field_eq(&link.twitch, clean),
                "kick" => field_eq(&link.kick, clean),
                "youtube" => field_eq(&link.youtube, q),
                "tiktok" => field_eq(&link.tiktok, clean),
                _ => false,
            })
            .cloned();
    }
    if let Some(link) = config
        .moderation_links
        .iter()
        .find(|link| link.id == q || link.name.eq_ignore_ascii_case(q))
    {
        return Some(link.clone());
    }
    config
        .moderation_links
        .iter()
        .find(|link| {
            field_eq(&link.twitch, clean)
                || field_eq(&link.kick, clean)
                || field_eq(&link.youtube, q)
                || field_eq(&link.tiktok, clean)
        })
        .cloned()
}

pub fn find_linked_person_from_args(engine: &Engine, args: &Obj) -> Option<LinkedPerson> {
    let text_arg = |key: &str| -> String {
        match args.get(key) {
            Some(Value::String(s)) => s.trim().to_string(),
            Some(Value::Number(n)) => n.to_string(),
            _ => String::new(),
        }
    };
    let hint = {
        let t = text_arg("userType");
        if t.is_empty() {
            text_arg("platform")
        } else {
            t
        }
    };
    for key in ["userId", "uniqueId", "userSlug", "userName", "user"] {
        let val = text_arg(key);
        if !val.is_empty() {
            if let Some(link) = find_linked_person(engine, &val, &hint) {
                return Some(link);
            }
        }
    }
    None
}

pub fn populate_linked_person_args(args: &mut Obj, link: &LinkedPerson) {
    args.insert("linkedPersonId".into(), link.id.clone().into());
    args.insert("linkedPersonName".into(), link.name.clone().into());
    args.insert("linkedTwitchId".into(), link.twitch.clone().into());
    args.insert("linkedKickId".into(), link.kick.clone().into());
    args.insert("linkedYouTubeId".into(), link.youtube.clone().into());
    args.insert("linkedTikTokId".into(), link.tiktok.clone().into());
}

pub fn emit_cross_moderation_applied(engine: &Rc<Engine>, args: &Obj) {
    engine.fire(
        T_CROSS_MODERATION_APPLIED,
        "CrossPlatformModerationApplied",
        "Core/Moderation",
        "core",
        args,
        |_| true,
    );
    engine.emit("Moderation", "CrossPlatformModerationApplied", || json!(args));
}

pub fn emit_linked_moderation_event(engine: &Rc<Engine>, platform: &str, kind: LinkedModerationEvent, args: &Obj) {
    let text_arg = |key: &str| -> String {
        match args.get(key) {
            Some(Value::String(s)) => s.trim().to_string(),
            Some(Value::Number(n)) => n.to_string(),
            _ => String::new(),
        }
    };
    let mut found = None;
    for key in ["userId", "uniqueId", "userSlug", "userName", "user"] {
        let val = text_arg(key);
        if !val.is_empty() {
            if let Some(link) = find_linked_person(engine, &val, platform) {
                found = Some(link);
                break;
            }
        }
    }
    let Some(link) = found else {
        return;
    };
    let mut out = args.clone();
    out.insert("sourcePlatform".into(), platform.into());
    populate_linked_person_args(&mut out, &link);
    let (code, name, ev) = match kind {
        LinkedModerationEvent::Banned => (
            T_CROSS_LINKED_USER_BANNED,
            "CrossPlatformLinkedUserBanned",
            "LinkedUserBanned",
        ),
        LinkedModerationEvent::TimedOut => (
            T_CROSS_LINKED_USER_TIMED_OUT,
            "CrossPlatformLinkedUserTimedOut",
            "LinkedUserTimedOut",
        ),
        LinkedModerationEvent::Unbanned => (
            T_CROSS_LINKED_USER_UNBANNED,
            "CrossPlatformLinkedUserUnbanned",
            "LinkedUserUnbanned",
        ),
    };
    engine.fire(code, name, "Core/Moderation", "core", &out, |_| true);
    engine.emit("Moderation", ev, || json!(out));
}

pub fn link_identity_from_action(
    engine: &Rc<Engine>,
    name: &str,
    twitch: &str,
    kick: &str,
    youtube: &str,
    tiktok: &str,
) -> Result<LinkedPerson, String> {
    let mut twitch = twitch.trim().to_string();
    let mut kick = kick.trim().to_string();
    let youtube = youtube.trim().to_string();
    let tiktok = tiktok.trim().trim_start_matches('@').to_string();
    let name_trimmed = name.trim().to_string();
    for id in [&mut twitch, &mut kick] {
        if !id.is_empty() {
            let number = id
                .parse::<u64>()
                .ok()
                .filter(|n| *n != 0)
                .ok_or_else(|| tr("moderation.link_invalid").to_string())?;
            *id = number.to_string();
        }
    }
    let existing = find_linked_person(engine, &name_trimmed, "")
        .or_else(|| find_linked_person(engine, &twitch, "twitch"))
        .or_else(|| find_linked_person(engine, &kick, "kick"))
        .or_else(|| find_linked_person(engine, &youtube, "youtube"))
        .or_else(|| find_linked_person(engine, &tiktok, "tiktok"));
    let mut link = existing.unwrap_or_else(|| LinkedPerson {
        id: crate::engine::uuid(),
        name: name_trimmed.clone(),
        ..LinkedPerson::default()
    });
    if !name_trimmed.is_empty() && link.id != name_trimmed {
        link.name = name_trimmed;
    }
    if !twitch.is_empty() {
        link.twitch = twitch;
    }
    if !kick.is_empty() {
        link.kick = kick;
    }
    if !youtube.is_empty() {
        link.youtube = youtube;
    }
    if !tiktok.is_empty() {
        link.tiktok = tiktok;
    }
    let has_any = [
        !link.twitch.is_empty(),
        !link.kick.is_empty(),
        !link.youtube.is_empty(),
        !link.tiktok.is_empty(),
    ]
    .into_iter()
    .any(|x| x);
    if link.name.is_empty() || !has_any {
        return Err(tr("moderation.link_invalid").into());
    }
    let config = engine.config();
    if config
        .moderation_links
        .iter()
        .filter(|other| other.id != link.id)
        .any(|other| {
            [
                (&link.twitch, &other.twitch),
                (&link.kick, &other.kick),
                (&link.youtube, &other.youtube),
                (&link.tiktok, &other.tiktok),
            ]
            .into_iter()
            .any(|(a, b)| !a.is_empty() && a.eq_ignore_ascii_case(b))
        })
    {
        return Err(tr("moderation.link_duplicate").into());
    }
    drop(config);
    let saved = link.clone();
    engine.change(|config| {
        config.moderation_links.retain(|other| other.id != link.id);
        config.moderation_links.push(link);
    });
    Ok(saved)
}

pub fn unlink_identity_from_action(
    engine: &Rc<Engine>,
    person: &str,
    platform: &str,
) -> Result<Option<LinkedPerson>, String> {
    let mut link = find_linked_person(engine, person, "").ok_or_else(|| tr("moderation.link_missing").to_string())?;
    let plat = platform.trim().to_ascii_lowercase();
    if plat.is_empty() || plat.starts_with("all") || plat == "0" {
        engine.change(|config| config.moderation_links.retain(|other| other.id != link.id));
        return Ok(None);
    }
    match plat.as_str() {
        "twitch" | "1" => link.twitch.clear(),
        "kick" | "2" => link.kick.clear(),
        "youtube" | "3" => link.youtube.clear(),
        "tiktok" | "4" => link.tiktok.clear(),
        _ => return Err(tr("moderation.link_invalid").into()),
    }
    let empty = link.twitch.is_empty() && link.kick.is_empty() && link.youtube.is_empty() && link.tiktok.is_empty();
    if empty {
        engine.change(|config| config.moderation_links.retain(|other| other.id != link.id));
        Ok(None)
    } else {
        let saved = link.clone();
        engine.change(|config| {
            config.moderation_links.retain(|other| other.id != link.id);
            config.moderation_links.push(link);
        });
        Ok(Some(saved))
    }
}

fn link_fingerprint(engine: &Engine, link: &LinkedPerson) -> String {
    use sha2::{Digest, Sha256};
    let owner = youtube_account(engine);
    let bans = youtube_ban_ids(engine, &link.youtube);
    let twitch = account(engine);
    let kick = kick_account(engine);
    let config = engine.config();
    let bytes = serde_json::to_vec(&(
        &link.twitch,
        &link.kick,
        &link.youtube,
        &link.tiktok,
        twitch,
        kick,
        owner,
        &config.youtube.channel_id,
        &config.youtube.video_id,
        bans,
    ))
    .unwrap_or_default();
    Sha256::digest(bytes).iter().map(|b| format!("{b:02x}")).collect()
}

fn form_fingerprint(engine: &Engine, kind: &str) -> String {
    use sha2::{Digest, Sha256};
    let key = if kind.starts_with("moderation_kick_") {
        kick_account(engine)
    } else if kind == "moderation_youtube_ban" {
        youtube_target(engine)
    } else {
        account(engine)
    };
    Sha256::digest(key.as_bytes())
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

pub fn youtube_ban_ids(engine: &Engine, user: &str) -> Vec<String> {
    let owner = youtube_account(engine);
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let mut bans: Vec<String> = engine
        .config()
        .youtube_moderation_bans
        .iter()
        .filter(|ban| {
            !owner.is_empty()
                && ban.channel_id == owner
                && ban.user_id == user
                && ban.expires_at.is_none_or(|end| end > now)
        })
        .map(|ban| ban.id.clone())
        .collect();
    bans.sort();
    bans
}

const LINK_FIELDS: &[F] = &[
    ("name", "Name", W::Text),
    ("twitch", "Twitch", W::Text),
    ("kick", "Kick", W::Text),
    ("youtube", "YouTube", W::Text),
    ("tiktok", "TikTok", W::Text),
];
const APPLY_FIELDS: &[F] = &[
    (
        "operation",
        "Action",
        W::Pick(&["Ban or timeout", "Remove ban or timeout"]),
    ),
    ("twitch", "Twitch", W::Bool),
    ("kick", "Kick", W::Bool),
    ("youtube", "YouTube", W::Bool),
    ("duration", "Duration", W::Int),
    ("reason", "Reason", W::Text),
];
const RESOLVE_FIELDS: &[F] = &[("response", "Response", W::Text)];
const TERM_FIELDS: &[F] = &[("text", "Term", W::Text)];

pub fn remember_youtube(engine: &Rc<Engine>, id: &str, user_id: &str, live_chat_id: &str, duration: Option<u64>) {
    let channel_id = engine
        .config()
        .youtube
        .broadcaster
        .as_ref()
        .map_or_else(String::new, |a| a.channel_id.clone());
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let expires_at = duration.map(|seconds| now.saturating_add(seconds));
    engine.change(|config| {
        config
            .youtube_moderation_bans
            .retain(|ban| ban.id != id && ban.expires_at.is_none_or(|end| end > now));
        config.youtube_moderation_bans.push(YouTubeBan {
            id: id.into(),
            user_id: user_id.into(),
            channel_id,
            live_chat_id: live_chat_id.into(),
            expires_at,
        });
    });
}
const BAN_FIELDS: &[F] = &[
    ("user", "User", W::Text),
    ("duration", "Duration", W::Int),
    ("reason", "Reason", W::Text),
];

pub fn external_fields(kind: &str) -> Option<&'static [F]> {
    if kind == "moderation_term" || kind.starts_with("moderation_term:") {
        return Some(TERM_FIELDS);
    }
    if kind.starts_with("moderation_resolve:") {
        return Some(RESOLVE_FIELDS);
    }
    if kind.starts_with("moderation_apply:") {
        return Some(APPLY_FIELDS);
    }
    if kind.starts_with("moderation_link:") {
        return Some(LINK_FIELDS);
    }
    let kind = kind.split_once(':').map_or(kind, |(base, _)| base);
    match kind {
        "moderation_twitch_ban" | "moderation_kick_ban" => Some(BAN_FIELDS),
        "moderation_kick_unban" => Some(&BAN_FIELDS[..1]),
        "moderation_youtube_ban" => Some(&BAN_FIELDS[..2]),
        _ => None,
    }
}

pub fn external_form(engine: &Rc<Engine>, kind: &str) -> Option<Result<Value, String>> {
    let (base, expected) = kind
        .split_once(':')
        .map_or((kind, None), |(base, hash)| (base, Some(hash)));
    let guarded = matches!(
        base,
        "moderation_term"
            | "moderation_twitch_ban"
            | "moderation_kick_ban"
            | "moderation_kick_unban"
            | "moderation_youtube_ban"
    );
    let form_kind = if guarded {
        let hash = form_fingerprint(engine, base);
        if expected.is_some_and(|expected| expected != hash) {
            return Some(Err(tr("moderation.account_changed").into()));
        }
        format!("{base}:{hash}")
    } else {
        kind.to_string()
    };
    if base == "moderation_term" {
        let item = json!({"text": ""});
        let widget = &TERM_FIELDS[0].2;
        return Some(Ok(
            json!({"kind": form_kind, "title": tr("moderation.term_add"), "okLabel": tr("moderation.term_add"), "json": false,
            "notes": [tr("moderation.term_note")], "new": item, "item": item,
            "fields": [{"key": "text", "label": tr("moderation.term"), "widget": widget, "holds": widget.about(), "value": ""}]}),
        ));
    }
    if let Some(rest) = kind.strip_prefix("moderation_resolve:") {
        let (status, tail) = rest.split_once(':')?;
        let (id, expected) = tail
            .split_once(':')
            .map_or((tail, None), |(id, generation)| (id, Some(generation)));
        if !matches!(status, "approved" | "denied") {
            return Some(Err(tr("moderation.request_missing").into()));
        }
        let row = REQUESTS.with_borrow(|s| {
            if s.account == account(engine) && s.loaded && !s.busy {
                s.rows
                    .iter()
                    .find(|row| row["id"].as_str() == Some(id))
                    .cloned()
                    .map(|row| (s.generation, row))
            } else {
                None
            }
        });
        let Some((generation, row)) = row else {
            return Some(Err(tr("moderation.request_missing").into()));
        };
        if expected.is_some_and(|value| value.parse::<u64>().ok() != Some(generation)) {
            return Some(Err(tr("moderation.request_missing").into()));
        }
        let item = json!({"response": ""});
        let widget = &RESOLVE_FIELDS[0].2;
        return Some(Ok(
            json!({"kind": format!("moderation_resolve:{status}:{id}:{generation}"), "title": if status == "approved" { tr("moderation.approve") } else { tr("moderation.deny") },
            "okLabel": if status == "approved" { tr("moderation.approve") } else { tr("moderation.deny") }, "json": false,
            "notes": [format!("{}: {}", row["user_name"].as_str().unwrap_or_default(), row["text"].as_str().unwrap_or_default())],
            "new": item, "item": item, "fields": [{"key": "response", "label": tr("moderation.response"), "widget": widget, "holds": widget.about(), "value": ""}]}),
        ));
    }
    if let Some(rest) = kind.strip_prefix("moderation_apply:") {
        let (id, expected) = rest.split_once(':').map_or((rest, None), |(id, hash)| (id, Some(hash)));
        let link = match engine
            .config()
            .moderation_links
            .iter()
            .find(|link| link.id == id)
            .cloned()
        {
            Some(link) => link,
            None => return Some(Err(tr("moderation.link_missing").into())),
        };
        let fingerprint = link_fingerprint(engine, &link);
        if expected.is_some_and(|hash| hash != fingerprint) {
            return Some(Err(tr("moderation.link_changed").into()));
        }
        let form_kind = format!("moderation_apply:{id}:{fingerprint}");
        let youtube_bans = youtube_ban_ids(engine, &link.youtube);
        let youtube_note = if youtube_bans.len() == 1 {
            crate::i18n::trf("moderation.cross_youtube_id", &[("id", &youtube_bans[0])])
        } else {
            tr("moderation.cross_youtube_unban").to_string()
        };
        let item = json!({"operation": 0, "twitch": false, "kick": false, "youtube": false, "duration": 0, "reason": "",
            "twitch_id": link.twitch, "kick_id": link.kick, "youtube_id": link.youtube});
        let labels = [
            tr("moderation.cross_action").to_string(),
            format!("Twitch: {}", link.twitch),
            format!("Kick: {}", link.kick),
            format!("YouTube: {}", link.youtube),
            tr("moderation.duration").into(),
            tr("moderation.reason").into(),
        ];
        let disabled = [
            false,
            link.twitch.is_empty(),
            link.kick.is_empty(),
            link.youtube.is_empty(),
            false,
            false,
        ];
        let fields: Vec<_> = APPLY_FIELDS.iter().zip(labels).zip(disabled).map(|(((key, _, widget), label), disabled)| {
            let mut field = json!({"key": key, "label": label, "widget": widget, "holds": widget.about(), "disabled": disabled, "value": crate::automation::edits::shown(widget, item.get(*key))});
            if *key == "operation" { field["widget"]["options"] = json!([tr("moderation.ban_title"), tr("moderation.unban")]); }
            field
        }).collect();
        return Some(Ok(
            json!({"kind": form_kind, "title": tr("moderation.cross_title"), "okLabel": tr("moderation.apply"), "json": false,
            "notes": [tr("moderation.cross_note"), tr("moderation.cross_tiktok"), youtube_note], "new": item, "item": item, "fields": fields}),
        ));
    }
    if let Some(id) = kind.strip_prefix("moderation_link:") {
        let item = if id.is_empty() {
            json!(LinkedPerson::default())
        } else {
            match engine.config().moderation_links.iter().find(|link| link.id == id) {
                Some(link) => json!(link),
                None => return Some(Err(tr("moderation.link_missing").into())),
            }
        };
        let labels = [
            tr("moderation.link_name"),
            tr("moderation.twitch_id"),
            tr("moderation.kick_id"),
            tr("moderation.youtube_user"),
            tr("moderation.tiktok_id"),
        ];
        let fields: Vec<_> = LINK_FIELDS.iter().zip(labels).map(|((key, _, widget), label)| json!({"key": key, "label": label, "widget": widget, "holds": widget.about(), "value": crate::automation::edits::shown(widget, item.get(*key))})).collect();
        return Some(Ok(
            json!({"kind": kind, "title": tr("moderation.link_title"), "okLabel": tr("btn.save"), "json": false,
            "notes": [tr("moderation.link_note")], "new": item, "item": item, "fields": fields}),
        ));
    }
    let schema = external_fields(kind)?;
    let kick = base.starts_with("moderation_kick_");
    let youtube = base == "moderation_youtube_ban";
    let unban = base == "moderation_kick_unban";
    let item = json!({"user": "", "duration": 0, "reason": ""});
    let labels = [
        if youtube {
            tr("moderation.youtube_user")
        } else if kick {
            tr("moderation.kick_login")
        } else {
            tr("moderation.login")
        },
        tr("moderation.duration"),
        tr("moderation.reason"),
    ];
    let fields: Vec<_> = schema
        .iter()
        .zip(labels)
        .map(|((key, _, widget), label)| {
            json!({"key": key, "label": label, "widget": widget, "holds": widget.about(),
            "value": crate::automation::edits::shown(widget, item.get(*key))})
        })
        .collect();
    Some(Ok(
        json!({"kind": form_kind, "title": if unban { tr("moderation.unban") } else { tr("moderation.ban_title") }, "okLabel": tr("moderation.apply"),
        "json": false, "notes": if unban { vec![] } else if youtube { vec![tr("moderation.youtube_duration_note")] } else if kick { vec![tr("moderation.kick_duration_note")] } else { vec![tr("moderation.duration_note")] }, "new": item, "item": item, "fields": fields}),
    ))
}

pub fn save_external(engine: &Rc<Engine>, kind: &str, item: Obj) -> Option<Result<Value, String>> {
    let (base, hash) = kind
        .split_once(':')
        .map_or((kind, None), |(base, hash)| (base, Some(hash)));
    if matches!(
        base,
        "moderation_term"
            | "moderation_twitch_ban"
            | "moderation_kick_ban"
            | "moderation_kick_unban"
            | "moderation_youtube_ban"
    ) {
        if hash != Some(form_fingerprint(engine, base).as_str()) {
            return Some(Err(tr("moderation.account_changed").into()));
        }
        let kind = base;
        return save_direct(engine, kind, item);
    }
    if let Some(rest) = kind.strip_prefix("moderation_resolve:") {
        let Some((status, tail)) = rest.split_once(':') else {
            return Some(Err(tr("moderation.request_missing").into()));
        };
        let Some((id, generation)) = tail.split_once(':') else {
            return Some(Err(tr("moderation.request_missing").into()));
        };
        if !REQUESTS.with_borrow(|s| generation.parse::<u64>().ok() == Some(s.generation)) {
            return Some(Err(tr("moderation.request_missing").into()));
        }
        return Some(resolve_request(engine, status, id, &item));
    }
    if let Some(rest) = kind.strip_prefix("moderation_apply:") {
        let Some((id, hash)) = rest.split_once(':') else {
            return Some(Err(tr("moderation.link_changed").into()));
        };
        let link = engine
            .config()
            .moderation_links
            .iter()
            .find(|link| link.id == id)
            .cloned();
        let current = link.as_ref().map(|link| link_fingerprint(engine, link));
        if current.as_deref() != Some(hash) {
            return Some(Err(tr("moderation.link_changed").into()));
        }
        return Some(apply_link(engine, id, &item));
    }
    if let Some(id) = kind.strip_prefix("moderation_link:") {
        return Some(save_link(engine, id, item));
    }
    None
}

fn save_direct(engine: &Rc<Engine>, kind: &str, item: Obj) -> Option<Result<Value, String>> {
    if kind == "moderation_term" {
        return Some(add_term(engine, &item));
    }
    if kind == "moderation_youtube_ban" {
        return Some(youtube_ban(engine, &item));
    }
    if kind == "moderation_kick_ban" || kind == "moderation_kick_unban" {
        return Some(kick_action(engine, kind == "moderation_kick_unban", &item));
    }
    if kind != "moderation_twitch_ban" {
        return None;
    }
    let user = item
        .get("user")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .trim()
        .trim_start_matches('@')
        .to_string();
    let duration = item.get("duration").and_then(Value::as_u64);
    let reason = item
        .get("reason")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .trim()
        .to_string();
    if user.is_empty() || duration.is_none_or(|d| d > 1_209_600) || reason.chars().count() > 500 {
        return Some(Err(tr("moderation.invalid_ban").into()));
    }
    let duration = duration.unwrap_or_default();
    Some((|| {
        let key = account(engine);
        let (generation, twitch) = begin(engine)?;
        let engine = engine.clone();
        tokio::task::spawn_local(async move {
            let result = async {
                let user_id = twitch.resolve_user_id(&user).await?;
                if account(&engine) != key {
                    return Err(tr("moderation.account_changed").into());
                }
                let mut data = json!({"user_id": user_id});
                if duration != 0 {
                    data["duration"] = duration.into();
                }
                if !reason.is_empty() {
                    data["reason"] = reason.into();
                }
                twitch
                    .helix(
                        "POST",
                        "moderation/bans",
                        vec![
                            ("broadcaster_id", "{broadcaster_id}"),
                            ("moderator_id", "{moderator_id}"),
                        ],
                        Some(json!({"data": data})),
                        false,
                        true,
                    )
                    .await?;
                fetch(&twitch)
                    .await
                    .map_err(|error| crate::i18n::trf("moderation.done_refresh_failed", &[("error", &error)]))
            }
            .await;
            finish(&engine, generation, result);
        });
        Ok(json!({"accepted": true}))
    })())
}

fn save_link(engine: &Rc<Engine>, id: &str, item: Obj) -> Result<Value, String> {
    let get = |key: &str| {
        item.get(key)
            .and_then(Value::as_str)
            .unwrap_or_default()
            .trim()
            .to_string()
    };
    let mut link = LinkedPerson {
        id: id.to_string(),
        name: get("name"),
        twitch: get("twitch"),
        kick: get("kick"),
        youtube: get("youtube"),
        tiktok: get("tiktok"),
    };
    let count = [&link.twitch, &link.kick, &link.youtube, &link.tiktok]
        .into_iter()
        .filter(|s| !s.is_empty())
        .count();
    if link.name.is_empty()
        || count < 2
        || [&link.twitch, &link.kick]
            .into_iter()
            .any(|s| !s.is_empty() && s.parse::<u64>().ok().is_none_or(|id| id == 0))
    {
        return Err(tr("moderation.link_invalid").into());
    }
    for id in [&mut link.twitch, &mut link.kick] {
        if let Ok(number) = id.parse::<u64>() {
            *id = number.to_string();
        }
    }
    let config = engine.config();
    if !id.is_empty() && !config.moderation_links.iter().any(|existing| existing.id == id) {
        return Err(tr("moderation.link_missing").into());
    }
    if config
        .moderation_links
        .iter()
        .filter(|existing| existing.id != id)
        .any(|existing| {
            [
                (&link.twitch, &existing.twitch),
                (&link.kick, &existing.kick),
                (&link.youtube, &existing.youtube),
                (&link.tiktok, &existing.tiktok),
            ]
            .into_iter()
            .any(|(a, b)| !a.is_empty() && a == b)
        })
    {
        return Err(tr("moderation.link_duplicate").into());
    }
    drop(config);
    if link.id.is_empty() {
        link.id = crate::engine::uuid();
    }
    let saved_id = link.id.clone();
    engine.change(|config| {
        config.moderation_links.retain(|existing| existing.id != link.id);
        config.moderation_links.push(link);
    });
    Ok(json!({"saved": true, "id": saved_id}))
}

#[derive(Default)]
struct CrossRun {
    busy: bool,
    rows: Vec<Value>,
}
thread_local! { static CROSS: RefCell<CrossRun> = RefCell::default(); }

fn apply_link(engine: &Rc<Engine>, id: &str, item: &Obj) -> Result<Value, String> {
    let link = engine
        .config()
        .moderation_links
        .iter()
        .find(|link| link.id == id)
        .cloned()
        .ok_or_else(|| tr("moderation.link_missing").to_string())?;
    for (key, current) in [
        ("twitch_id", &link.twitch),
        ("kick_id", &link.kick),
        ("youtube_id", &link.youtube),
    ] {
        if item.get(key).and_then(Value::as_str) != Some(current.as_str()) {
            return Err(tr("moderation.link_changed").into());
        }
    }
    let selected = |key: &str| item.get(key).and_then(Value::as_bool).unwrap_or(false);
    let on = [selected("twitch"), selected("kick"), selected("youtube")];
    let operation = item
        .get("operation")
        .and_then(Value::as_u64)
        .filter(|op| *op <= 1)
        .ok_or_else(|| tr("moderation.cross_invalid").to_string())?;
    let unban = operation == 1;
    let duration = item
        .get("duration")
        .and_then(Value::as_u64)
        .ok_or_else(|| tr("moderation.cross_invalid").to_string())?;
    let reason = item
        .get("reason")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .trim()
        .to_string();
    if !on.iter().any(|v| *v)
        || (on[0] && (link.twitch.is_empty() || (!unban && (duration > 1_209_600 || reason.chars().count() > 500))))
        || (on[1]
            && (link.kick.parse::<u64>().ok().is_none_or(|id| id == 0)
                || (!unban && (duration > 604_800 || reason.chars().count() > 100))))
        || (on[2] && link.youtube.is_empty())
    {
        return Err(tr("moderation.cross_invalid").into());
    }
    if CROSS.with_borrow(|run| run.busy) {
        return Err(tr("moderation.working").into());
    }
    CROSS.with_borrow_mut(|run| {
        run.busy = true;
        run.rows.clear();
    });
    if on[0] {
        SNAPSHOT.with_borrow_mut(|snapshot| {
            snapshot.generation = snapshot.generation.wrapping_add(1);
            snapshot.loaded = false;
            snapshot.busy = false;
            snapshot.error.clear();
        });
    }
    let twitch = engine.twitch();
    let kick = engine.kick();
    let youtube = engine.youtube();
    let accounts = [account(engine), kick_account(engine), youtube_target(engine)];
    let youtube_bans = youtube_ban_ids(engine, &link.youtube);
    let engine = engine.clone();
    crate::host::changed();
    tokio::task::spawn_local(async move {
        let mut rows = Vec::new();
        let mut succeeded: Vec<String> = Vec::new();
        let mut failed: Vec<String> = Vec::new();
        if on[0] {
            let result = if account(&engine) != accounts[0] {
                Err(tr("moderation.account_changed").into())
            } else if let Some(twitch) = twitch {
                if unban {
                    twitch
                        .helix(
                            "DELETE",
                            "moderation/bans",
                            vec![
                                ("broadcaster_id", "{broadcaster_id}"),
                                ("moderator_id", "{moderator_id}"),
                                ("user_id", &link.twitch),
                            ],
                            None,
                            false,
                            true,
                        )
                        .await
                        .map(drop)
                } else {
                    let mut data = json!({"user_id": link.twitch});
                    if duration > 0 {
                        data["duration"] = duration.into();
                    }
                    if !reason.is_empty() {
                        data["reason"] = reason.clone().into();
                    }
                    twitch
                        .helix(
                            "POST",
                            "moderation/bans",
                            vec![
                                ("broadcaster_id", "{broadcaster_id}"),
                                ("moderator_id", "{moderator_id}"),
                            ],
                            Some(json!({"data": data})),
                            false,
                            true,
                        )
                        .await
                        .map(drop)
                }
            } else {
                Err(tr("moderation.twitch_disconnected").into())
            };
            if result.is_ok() {
                succeeded.push("twitch".into());
            } else {
                failed.push("twitch".into());
            }
            rows.push(cross_result("twitch", &link.twitch, result));
        }
        if on[1] {
            let result = if kick_account(&engine) != accounts[1] {
                Err(tr("moderation.account_changed").into())
            } else if let Some(kick) = kick {
                if unban {
                    kick.unban_user_id(link.kick.parse().unwrap_or_default()).await
                } else {
                    kick.ban_user_id(
                        link.kick.parse().unwrap_or_default(),
                        (duration != 0).then_some(duration),
                        &reason,
                    )
                    .await
                }
            } else {
                Err(tr("moderation.kick_disconnected").into())
            };
            if result.is_ok() {
                succeeded.push("kick".into());
            } else {
                failed.push("kick".into());
            }
            rows.push(cross_result("kick", &link.kick, result));
        }
        if on[2] {
            let result = if youtube_target(&engine) != accounts[2] {
                Err(tr("moderation.account_changed").into())
            } else if let Some(youtube) = youtube {
                if unban {
                    if youtube_bans.len() == 1 {
                        youtube.remove_ban(&youtube_bans[0]).await
                    } else {
                        Err(tr("moderation.cross_youtube_unban").into())
                    }
                } else {
                    youtube
                        .ban_user_tracked("", &link.youtube, (duration != 0).then_some(duration))
                        .await
                        .map(drop)
                }
            } else {
                Err(tr("moderation.youtube_disconnected").into())
            };
            if result.is_ok() {
                succeeded.push("youtube".into());
            } else {
                failed.push("youtube".into());
            }
            rows.push(cross_result("youtube", &link.youtube, result));
        }
        let mut ev_args = Obj::new();
        let op_name = if unban {
            "unban"
        } else if duration > 0 {
            "timeout"
        } else {
            "ban"
        };
        ev_args.insert("operation".into(), op_name.into());
        populate_linked_person_args(&mut ev_args, &link);
        ev_args.insert("duration".into(), (if unban { 0 } else { duration }).into());
        ev_args.insert("reason".into(), reason.clone().into());
        ev_args.insert("succeededCount".into(), (succeeded.len() as u64).into());
        ev_args.insert("failedCount".into(), (failed.len() as u64).into());
        ev_args.insert("succeededPlatforms".into(), json!(succeeded));
        ev_args.insert("failedPlatforms".into(), json!(failed));
        ev_args.insert("results".into(), json!(rows));
        emit_cross_moderation_applied(&engine, &ev_args);
        CROSS.with_borrow_mut(|run| {
            run.busy = false;
            run.rows = rows;
        });
        crate::host::changed();
    });
    Ok(json!({"accepted": true}))
}

fn cross_result(platform: &str, id: &str, result: Result<(), String>) -> Value {
    let text = match result {
        Ok(()) => tr("moderation.action_done").to_string(),
        Err(error) => error,
    };
    json!({"id": platform, "name": id, "enabled": true, "cells": [{"brand": platform, "text": platform}, id, text]})
}

#[derive(Default)]
struct KickState {
    account: String,
    generation: u64,
    busy: bool,
    message: String,
    error: bool,
}
thread_local! { static KICK: RefCell<KickState> = RefCell::default(); }
thread_local! { static YOUTUBE: RefCell<KickState> = RefCell::default(); }

#[derive(Default)]
struct KickObserved {
    account: String,
    generation: u64,
    rows: Vec<Value>,
}
thread_local! { static KICK_OBSERVED: RefCell<KickObserved> = RefCell::default(); }

/// Kick's chat feed can report new bans, but cannot establish the complete current list.
pub fn kick_ban_event(engine: &Rc<Engine>, channel: &str, data: &Value) {
    let configured = crate::kick::clean_kick_channel(&engine.config().kick.channel);
    if configured.is_empty() || !configured.eq_ignore_ascii_case(channel) {
        return;
    }
    let id = data["user"]["id"]
        .as_u64()
        .or_else(|| data["user"]["id"].as_str().and_then(|s| s.parse().ok()))
        .filter(|id| *id != 0);
    let Some(id) = id else {
        return;
    };
    let key = kick_account(engine);
    KICK_OBSERVED.with_borrow_mut(|s| {
        if s.account != key {
            *s = KickObserved {
                account: key,
                ..KickObserved::default()
            };
        }
        s.generation = s.generation.wrapping_add(1);
        s.rows.retain(|row| row["id"].as_u64() != Some(id));
        if s.rows.len() >= 100 {
            s.rows.remove(0);
        }
        s.rows.push(
            json!({"id": id, "name": data["user"]["username"], "permanent": data["permanent"],
            "reason": data["reason"], "moderator": data["banned_by"]["username"], "expires": data["expires_at"]}),
        );
    });
    crate::host::changed();
}

fn kick_unban_observed(engine: &Rc<Engine>, id: &str) -> Result<Value, String> {
    let user_id = id
        .parse::<u64>()
        .ok()
        .filter(|id| *id != 0)
        .ok_or_else(|| tr("moderation.kick_select").to_string())?;
    let key = kick_account(engine);
    let observed_generation = KICK_OBSERVED.with_borrow(|s| {
        (s.account == key && s.rows.iter().any(|row| row["id"].as_u64() == Some(user_id))).then_some(s.generation)
    });
    let Some(observed_generation) = observed_generation else {
        return Err(tr("moderation.kick_select").into());
    };
    let kick = engine
        .kick()
        .ok_or_else(|| tr("moderation.kick_disconnected").to_string())?;
    if KICK.with_borrow(|s| s.busy && s.account == key) {
        return Err(tr("moderation.working").into());
    }
    let generation = KICK.with_borrow_mut(|s| {
        s.generation = s.generation.wrapping_add(1);
        s.account = key.clone();
        s.busy = true;
        s.error = false;
        s.message.clear();
        s.generation
    });
    let engine = engine.clone();
    crate::host::changed();
    tokio::task::spawn_local(async move {
        let result = kick.unban_user_id(user_id).await;
        if kick_account(&engine) != key {
            return;
        }
        let ok = result.is_ok();
        KICK.with_borrow_mut(|s| {
            if s.account != key || s.generation != generation {
                return;
            }
            s.busy = false;
            match result {
                Ok(()) => s.message = tr("moderation.action_done").into(),
                Err(error) => {
                    s.error = true;
                    s.message = error;
                }
            }
        });
        if ok && KICK.with_borrow(|s| s.account == key && s.generation == generation) {
            KICK_OBSERVED.with_borrow_mut(|s| {
                if s.account == key && s.generation == observed_generation {
                    s.rows.retain(|row| row["id"].as_u64() != Some(user_id));
                }
            });
        }
        crate::host::changed();
    });
    Ok(json!({"accepted": true}))
}

fn youtube_account(engine: &Engine) -> String {
    engine
        .config()
        .youtube
        .broadcaster
        .as_ref()
        .map_or_else(String::new, |a| a.channel_id.clone())
}

fn youtube_target(engine: &Engine) -> String {
    let owner = youtube_account(engine);
    let config = engine.config();
    format!("{}|{}|{}", owner, config.youtube.channel_id, config.youtube.video_id)
}

fn youtube_ban(engine: &Rc<Engine>, item: &Obj) -> Result<Value, String> {
    let user = item
        .get("user")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .trim()
        .to_string();
    let duration = item
        .get("duration")
        .and_then(Value::as_u64)
        .ok_or_else(|| tr("moderation.youtube_invalid").to_string())?;
    if user.is_empty() {
        return Err(tr("moderation.youtube_invalid").into());
    }
    let youtube = engine
        .youtube()
        .ok_or_else(|| tr("moderation.youtube_disconnected").to_string())?;
    let account = youtube_account(engine);
    let target = youtube_target(engine);
    if YOUTUBE.with_borrow(|s| s.busy && s.account == account) {
        return Err(tr("moderation.working").into());
    }
    let generation = YOUTUBE.with_borrow_mut(|s| {
        s.generation = s.generation.wrapping_add(1);
        s.account = account.clone();
        s.busy = true;
        s.error = false;
        s.message.clear();
        s.generation
    });
    crate::host::changed();
    let engine = engine.clone();
    tokio::task::spawn_local(async move {
        let result = if youtube_target(&engine) != target {
            Err(tr("moderation.account_changed").into())
        } else {
            youtube
                .ban_user_tracked("", &user, (duration != 0).then_some(duration))
                .await
        };
        finish_youtube(&account, generation, result.map(drop));
    });
    Ok(json!({"accepted": true}))
}

fn finish_youtube(account: &str, generation: u64, result: Result<(), String>) {
    YOUTUBE.with_borrow_mut(|s| {
        if s.account != account || s.generation != generation {
            return;
        }
        s.busy = false;
        match result {
            Ok(()) => s.message = tr("moderation.action_done").into(),
            Err(error) => {
                s.error = true;
                s.message = error;
            }
        }
    });
    crate::host::changed();
}

fn kick_account(engine: &Engine) -> String {
    let config = engine.config();
    format!(
        "{}|{}",
        config.kick.channel,
        config.kick.broadcaster.as_ref().map_or(0, |a| a.user_id)
    )
}

fn kick_action(engine: &Rc<Engine>, unban: bool, item: &Obj) -> Result<Value, String> {
    let user = item
        .get("user")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .trim()
        .trim_start_matches('@')
        .to_string();
    let duration = item.get("duration").and_then(Value::as_u64);
    let reason = item
        .get("reason")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .trim()
        .to_string();
    if user.is_empty() || (!unban && (duration.is_none_or(|d| d > 604_800) || reason.chars().count() > 100)) {
        return Err(tr("moderation.kick_invalid").into());
    }
    let duration = duration.unwrap_or_default();
    let kick = engine
        .kick()
        .ok_or_else(|| tr("moderation.kick_disconnected").to_string())?;
    let account = kick_account(engine);
    if KICK.with_borrow(|s| s.busy && s.account == account) {
        return Err(tr("moderation.working").into());
    }
    let generation = KICK.with_borrow_mut(|s| {
        s.generation = s.generation.wrapping_add(1);
        s.account = account.clone();
        s.busy = true;
        s.message.clear();
        s.error = false;
        s.generation
    });
    crate::host::changed();
    tokio::task::spawn_local(async move {
        let result = if unban {
            kick.unban_user(&user).await
        } else if duration == 0 {
            kick.ban_user(&user, &reason).await
        } else {
            kick.timeout_user(&user, duration, &reason).await
        };
        KICK.with_borrow_mut(|s| {
            if s.account != account || s.generation != generation {
                return;
            }
            s.busy = false;
            match result {
                Ok(()) => s.message = tr("moderation.action_done").into(),
                Err(error) => {
                    s.error = true;
                    s.message = error;
                }
            }
        });
        crate::host::changed();
    });
    Ok(json!({"accepted": true}))
}

#[derive(Default)]
struct Snapshot {
    account: String,
    generation: u64,
    busy: bool,
    loaded: bool,
    rows: Vec<Value>,
    error: String,
}

thread_local! { static SNAPSHOT: RefCell<Snapshot> = RefCell::default(); }
thread_local! { static REQUESTS: RefCell<Snapshot> = RefCell::default(); }
thread_local! { static TERMS: RefCell<Snapshot> = RefCell::default(); }

#[derive(Default)]
struct Shield {
    account: String,
    generation: u64,
    busy: bool,
    active: Option<bool>,
    error: String,
}
thread_local! { static SHIELD: RefCell<Shield> = RefCell::default(); }

pub fn shield_event(engine: &Rc<Engine>, event: &Value, active: bool) {
    if !is_current_twitch_channel(engine, event) {
        return;
    }
    let key = account(engine);
    SHIELD.with_borrow_mut(|s| {
        if s.account != key {
            let generation = s.generation;
            *s = Shield {
                account: key,
                generation,
                ..Shield::default()
            };
        }
        s.generation = s.generation.wrapping_add(1);
        s.busy = false;
        s.active = Some(active);
        s.error.clear();
    });
    crate::host::changed();
}

fn shield(engine: &Rc<Engine>, update: Option<bool>) -> Result<Value, String> {
    let twitch = engine
        .twitch()
        .ok_or_else(|| tr("moderation.twitch_disconnected").to_string())?;
    let key = account(engine);
    if SHIELD.with_borrow(|s| s.account == key && s.busy) {
        return Err(tr("moderation.working").into());
    }
    let generation = SHIELD.with_borrow_mut(|s| {
        if s.account != key {
            let generation = s.generation;
            *s = Shield {
                account: key.clone(),
                generation,
                ..Shield::default()
            };
        }
        s.generation = s.generation.wrapping_add(1);
        s.busy = true;
        s.error.clear();
        s.generation
    });
    crate::host::changed();
    let engine = engine.clone();
    tokio::task::spawn_local(async move {
        let method = if update.is_some() { "PUT" } else { "GET" };
        let body = update.map(|active| json!({"is_active": active}));
        let result = twitch
            .helix(
                method,
                "moderation/shield_mode",
                vec![
                    ("broadcaster_id", "{broadcaster_id}"),
                    ("moderator_id", "{moderator_id}"),
                ],
                body,
                false,
                true,
            )
            .await
            .and_then(|value| {
                value["data"][0]["is_active"]
                    .as_bool()
                    .ok_or_else(|| tr("moderation.shield_invalid").to_string())
            });
        SHIELD.with_borrow_mut(|s| {
            if s.account != key || s.generation != generation || account(&engine) != key {
                return;
            }
            s.busy = false;
            match result {
                Ok(active) => {
                    s.active = Some(active);
                    s.error.clear();
                }
                Err(error) => {
                    s.active = None;
                    s.error = error;
                }
            }
        });
        crate::host::changed();
    });
    Ok(json!({"accepted": true}))
}

fn begin_terms(engine: &Rc<Engine>) -> Result<(u64, crate::twitch::TwitchHandle), String> {
    let twitch = engine
        .twitch()
        .ok_or_else(|| tr("moderation.twitch_disconnected").to_string())?;
    let key = account(engine);
    let generation = TERMS.with_borrow_mut(|s| {
        if s.account != key {
            s.account = key;
            s.rows.clear();
            s.loaded = false;
        }
        s.generation = s.generation.wrapping_add(1);
        s.busy = true;
        s.error.clear();
        s.generation
    });
    crate::host::changed();
    Ok((generation, twitch))
}

fn finish_terms(engine: &Engine, generation: u64, result: Result<Vec<Value>, String>) {
    let key = account(engine);
    TERMS.with_borrow_mut(|s| {
        if s.generation != generation || s.account != key {
            return;
        }
        s.busy = false;
        match result {
            Ok(rows) => {
                s.rows = rows;
                s.loaded = true;
                s.error.clear();
            }
            Err(error) => {
                s.loaded = false;
                s.error = error;
            }
        }
    });
    crate::host::changed();
}

async fn fetch_terms(twitch: &crate::twitch::TwitchHandle) -> Result<Vec<Value>, String> {
    let mut rows = Vec::new();
    let mut cursor = String::new();
    let mut cursors = std::collections::HashSet::new();
    loop {
        let mut query = vec![
            ("broadcaster_id", "{broadcaster_id}"),
            ("moderator_id", "{moderator_id}"),
            ("first", "100"),
        ];
        if !cursor.is_empty() {
            query.push(("after", &cursor));
        }
        let response = twitch
            .helix("GET", "moderation/blocked_terms", query, None, false, true)
            .await?;
        rows.extend(
            response["data"]
                .as_array()
                .ok_or_else(|| tr("moderation.invalid_terms").to_string())?
                .iter()
                .cloned(),
        );
        let next = response["pagination"]["cursor"].as_str().unwrap_or_default();
        if next.is_empty() {
            return Ok(rows);
        }
        if !cursors.insert(next.to_string()) {
            return Err(tr("moderation.invalid_cursor").into());
        }
        cursor = next.to_string();
    }
}

fn refresh_terms(engine: &Rc<Engine>) -> Result<Value, String> {
    let (generation, twitch) = begin_terms(engine)?;
    let engine = engine.clone();
    tokio::task::spawn_local(async move {
        finish_terms(&engine, generation, fetch_terms(&twitch).await);
    });
    Ok(json!({"accepted": true}))
}

fn add_term(engine: &Rc<Engine>, item: &Obj) -> Result<Value, String> {
    let text = item
        .get("text")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .trim()
        .to_string();
    let count = text.chars().filter(|c| !c.is_whitespace() && *c != '*').count();
    if text.chars().count() > 500 || count < if text.contains('*') { 3 } else { 2 } {
        return Err(tr("moderation.term_invalid").into());
    }
    let (generation, twitch) = begin_terms(engine)?;
    let engine = engine.clone();
    tokio::task::spawn_local(async move {
        let result = twitch
            .helix(
                "POST",
                "moderation/blocked_terms",
                vec![
                    ("broadcaster_id", "{broadcaster_id}"),
                    ("moderator_id", "{moderator_id}"),
                ],
                Some(json!({"text": text})),
                false,
                true,
            )
            .await;
        let result = match result {
            Ok(_) => fetch_terms(&twitch)
                .await
                .map_err(|error| crate::i18n::trf("moderation.done_refresh_failed", &[("error", &error)])),
            Err(error) => Err(error),
        };
        finish_terms(&engine, generation, result);
    });
    Ok(json!({"accepted": true}))
}

fn remove_term(engine: &Rc<Engine>, id: &str) -> Result<Value, String> {
    let valid = TERMS.with_borrow(|s| {
        s.loaded && !s.busy && s.account == account(engine) && s.rows.iter().any(|row| row["id"].as_str() == Some(id))
    });
    if !valid {
        return Err(tr("moderation.term_missing").into());
    }
    let (generation, twitch) = begin_terms(engine)?;
    let engine = engine.clone();
    let id = id.to_string();
    tokio::task::spawn_local(async move {
        let result = twitch
            .helix(
                "DELETE",
                "moderation/blocked_terms",
                vec![
                    ("broadcaster_id", "{broadcaster_id}"),
                    ("moderator_id", "{moderator_id}"),
                    ("id", &id),
                ],
                None,
                false,
                true,
            )
            .await;
        let result = match result {
            Ok(_) => fetch_terms(&twitch)
                .await
                .map_err(|error| crate::i18n::trf("moderation.done_refresh_failed", &[("error", &error)])),
            Err(error) => Err(error),
        };
        finish_terms(&engine, generation, result);
    });
    Ok(json!({"accepted": true}))
}

#[derive(Default)]
struct AutoMod {
    account: String,
    generation: u64,
    busy: bool,
    error: String,
    rows: Vec<Value>,
}
thread_local! { static AUTOMOD: RefCell<AutoMod> = RefCell::default(); }

/// EventSub holds are live observations. Updates also arrive when another moderator acts.
pub fn automod_event(engine: &Rc<Engine>, event: &Value, held: bool) {
    let Some(id) = event["message_id"].as_str().filter(|id| !id.is_empty()) else {
        return;
    };
    let key = account(engine);
    if !is_current_twitch_channel(engine, event) {
        return;
    }
    AUTOMOD.with_borrow_mut(|queue| {
        if queue.account != key {
            let generation = queue.generation.wrapping_add(1);
            *queue = AutoMod {
                account: key,
                generation,
                ..AutoMod::default()
            };
        }
        queue.rows.retain(|row| row["message_id"].as_str() != Some(id));
        if held {
            if queue.rows.len() >= 100 {
                queue.rows.remove(0);
            }
            queue.rows.push(event.clone());
        }
    });
    crate::host::changed();
}

fn resolve_automod(engine: &Rc<Engine>, id: &str, allow: bool) -> Result<Value, String> {
    let key = account(engine);
    let valid = AUTOMOD.with_borrow(|queue| {
        queue.account == key && !queue.busy && queue.rows.iter().any(|row| row["message_id"].as_str() == Some(id))
    });
    if !valid {
        return Err(tr("moderation.automod_missing").into());
    }
    let twitch = engine
        .twitch()
        .ok_or_else(|| tr("moderation.twitch_disconnected").to_string())?;
    let generation = AUTOMOD.with_borrow_mut(|queue| {
        queue.generation = queue.generation.wrapping_add(1);
        queue.busy = true;
        queue.error.clear();
        queue.generation
    });
    let id = id.to_string();
    let engine = engine.clone();
    crate::host::changed();
    tokio::task::spawn_local(async move {
        let result = twitch
            .helix(
                "POST",
                "moderation/automod/message",
                vec![],
                Some(
                    json!({"user_id": "{moderator_id}", "msg_id": id, "action": if allow { "ALLOW" } else { "DENY" }}),
                ),
                false,
                true,
            )
            .await;
        AUTOMOD.with_borrow_mut(|queue| {
            if queue.account != key || queue.generation != generation || account(&engine) != key {
                return;
            }
            queue.busy = false;
            match result {
                Ok(_) => queue.rows.retain(|row| row["message_id"].as_str() != Some(id.as_str())),
                Err(error) => queue.error = error,
            }
        });
        crate::host::changed();
    });
    Ok(json!({"accepted": true}))
}

fn begin_requests(engine: &Rc<Engine>) -> Result<(u64, crate::twitch::TwitchHandle), String> {
    let twitch = engine
        .twitch()
        .ok_or_else(|| tr("moderation.twitch_disconnected").to_string())?;
    let key = account(engine);
    let generation = REQUESTS.with_borrow_mut(|state| {
        if state.account != key {
            state.account = key;
            state.rows.clear();
            state.loaded = false;
        }
        state.generation = state.generation.wrapping_add(1);
        state.busy = true;
        state.error.clear();
        state.generation
    });
    crate::host::changed();
    Ok((generation, twitch))
}

fn finish_requests(engine: &Engine, generation: u64, result: Result<Vec<Value>, String>) {
    let key = account(engine);
    REQUESTS.with_borrow_mut(|state| {
        if state.generation != generation || state.account != key {
            return;
        }
        state.busy = false;
        match result {
            Ok(rows) => {
                state.rows = rows;
                state.loaded = true;
                state.error.clear();
            }
            Err(error) => {
                state.loaded = false;
                state.error = error;
            }
        }
    });
    crate::host::changed();
}

async fn fetch_requests(twitch: &crate::twitch::TwitchHandle) -> Result<Vec<Value>, String> {
    let mut rows = Vec::new();
    let mut cursor = String::new();
    let mut cursors = std::collections::HashSet::new();
    loop {
        let mut query = vec![
            ("broadcaster_id", "{broadcaster_id}"),
            ("moderator_id", "{moderator_id}"),
            ("status", "pending"),
            ("first", "100"),
        ];
        if !cursor.is_empty() {
            query.push(("after", &cursor));
        }
        let response = twitch
            .helix("GET", "moderation/unban_requests", query, None, false, true)
            .await?;
        rows.extend(
            response["data"]
                .as_array()
                .ok_or_else(|| tr("moderation.invalid_requests").to_string())?
                .iter()
                .cloned(),
        );
        let next = response["pagination"]["cursor"].as_str().unwrap_or_default();
        if next.is_empty() {
            return Ok(rows);
        }
        if !cursors.insert(next.to_string()) {
            return Err(tr("moderation.invalid_cursor").into());
        }
        cursor = next.to_owned();
    }
}

fn refresh_requests(engine: &Rc<Engine>) -> Result<Value, String> {
    let (generation, twitch) = begin_requests(engine)?;
    let engine = engine.clone();
    tokio::task::spawn_local(async move {
        finish_requests(&engine, generation, fetch_requests(&twitch).await);
    });
    Ok(json!({"accepted": true}))
}

fn resolve_request(engine: &Rc<Engine>, status: &str, id: &str, item: &Obj) -> Result<Value, String> {
    let response = item
        .get("response")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .trim()
        .to_string();
    if !matches!(status, "approved" | "denied") || response.chars().count() > 500 {
        return Err(tr("moderation.request_invalid").into());
    }
    let valid = REQUESTS.with_borrow(|s| {
        s.loaded && !s.busy && s.account == account(engine) && s.rows.iter().any(|row| row["id"].as_str() == Some(id))
    });
    if !valid {
        return Err(tr("moderation.request_missing").into());
    }
    let (generation, twitch) = begin_requests(engine)?;
    let engine = engine.clone();
    let id = id.to_string();
    let status = status.to_string();
    tokio::task::spawn_local(async move {
        let mut query = vec![
            ("broadcaster_id", "{broadcaster_id}"),
            ("moderator_id", "{moderator_id}"),
            ("unban_request_id", id.as_str()),
            ("status", status.as_str()),
        ];
        if !response.is_empty() {
            query.push(("resolution_text", &response));
        }
        let result = twitch
            .helix("PATCH", "moderation/unban_requests", query, None, false, true)
            .await;
        let result = match result {
            Ok(_) => fetch_requests(&twitch)
                .await
                .map_err(|error| crate::i18n::trf("moderation.request_refresh_failed", &[("error", &error)])),
            Err(error) => Err(error),
        };
        finish_requests(&engine, generation, result);
    });
    Ok(json!({"accepted": true}))
}

pub fn supports(domain: &str) -> bool {
    domain == DOMAIN
}

pub(super) fn account(engine: &Engine) -> String {
    let config = engine.config();
    format!(
        "{}|{}|{}",
        config.twitch.channel,
        config.twitch.channel_id,
        config.twitch.broadcaster.as_ref().map_or("", |a| &a.user_id)
    )
}

pub(super) fn is_current_twitch_channel(engine: &Engine, event: &Value) -> bool {
    let config = engine.config();
    let expected = if config.twitch.channel_id.is_empty() {
        config.twitch.broadcaster.as_ref().map_or("", |a| a.user_id.as_str())
    } else {
        config.twitch.channel_id.as_str()
    };
    !expected.is_empty() && event["broadcaster_user_id"].as_str() == Some(expected)
}

/// A snapshot belongs to a particular account and channel, and old replies must not replace it.
fn begin(engine: &Rc<Engine>) -> Result<(u64, crate::twitch::TwitchHandle), String> {
    let twitch = engine
        .twitch()
        .ok_or_else(|| tr("moderation.twitch_disconnected").to_string())?;
    let key = account(engine);
    let generation = SNAPSHOT.with_borrow_mut(|state| {
        if state.account != key {
            state.account = key;
            state.rows.clear();
            state.loaded = false;
        }
        state.generation = state.generation.wrapping_add(1);
        state.busy = true;
        state.error.clear();
        state.generation
    });
    crate::host::changed();
    Ok((generation, twitch))
}

fn finish(engine: &Engine, generation: u64, result: Result<Vec<Value>, String>) {
    let key = account(engine);
    SNAPSHOT.with_borrow_mut(|state| {
        if state.generation != generation || state.account != key {
            return;
        }
        state.busy = false;
        match result {
            Ok(rows) => {
                state.rows = rows;
                state.loaded = true;
                state.error.clear();
            }
            Err(error) => {
                state.loaded = false;
                state.error = error;
            }
        }
        crate::host::changed();
    });
}

async fn fetch(twitch: &crate::twitch::TwitchHandle) -> Result<Vec<Value>, String> {
    let mut rows = Vec::new();
    let mut cursor = String::new();
    let mut cursors = std::collections::HashSet::new();
    loop {
        let mut query = vec![("broadcaster_id", "{broadcaster_id}"), ("first", "100")];
        if !cursor.is_empty() {
            query.push(("after", cursor.as_str()));
        }
        let response = twitch
            .helix("GET", "moderation/banned", query, None, false, true)
            .await?;
        let data = response["data"]
            .as_array()
            .ok_or_else(|| tr("moderation.invalid_list").to_string())?;
        rows.extend(data.iter().cloned());
        let next = response["pagination"]["cursor"].as_str().unwrap_or_default();
        if next.is_empty() {
            return Ok(rows);
        }
        if !cursors.insert(next.to_string()) {
            return Err(tr("moderation.invalid_cursor").into());
        }
        cursor = next.to_owned();
    }
}

fn refresh(engine: &Rc<Engine>) -> Result<Value, String> {
    let (generation, twitch) = begin(engine)?;
    let engine = engine.clone();
    tokio::task::spawn_local(async move {
        let result = fetch(&twitch).await;
        finish(&engine, generation, result);
    });
    Ok(json!({"accepted": true}))
}

pub fn act(engine: &Rc<Engine>, action: &str, item_id: &str) -> Result<Value, String> {
    match action {
        "refresh" => refresh(engine),
        "kick_open" | "tiktok_open" => {
            let channel = if action == "kick_open" {
                crate::kick::clean_kick_channel(&engine.config().kick.channel)
            } else {
                crate::tiktok::clean_tiktok_channel(&engine.config().tiktok.channel)
            };
            if channel.is_empty() {
                return Err(tr("moderation.channel_missing").into());
            }
            let channel = crate::commands::url_encode(&channel);
            let url = if action == "kick_open" {
                format!("https://kick.com/{channel}")
            } else {
                format!("https://www.tiktok.com/@{channel}/live")
            };
            crate::oauth::open_browser(&url);
            Ok(json!({"opened": true}))
        }
        "kick_unban_observed" => kick_unban_observed(engine, item_id),
        "requests_refresh" => refresh_requests(engine),
        "terms_refresh" => refresh_terms(engine),
        "term_remove" => remove_term(engine, item_id),
        "shield_refresh" => shield(engine, None),
        "settings_refresh" => super::moderation_settings::refresh(engine),
        "chat_refresh" => super::moderation_chat::refresh(engine),
        "suspicious_monitor" => super::moderation_suspicious::act(engine, item_id, "ACTIVE_MONITORING"),
        "suspicious_restrict" => super::moderation_suspicious::act(engine, item_id, "RESTRICTED"),
        "suspicious_remove" => super::moderation_suspicious::act(engine, item_id, "none"),
        "shield_on" => shield(engine, Some(true)),
        "shield_off" => shield(engine, Some(false)),
        "automod_allow" => resolve_automod(engine, item_id, true),
        "automod_deny" => resolve_automod(engine, item_id, false),
        "link_delete" => {
            if !engine.config().moderation_links.iter().any(|link| link.id == item_id) {
                return Err(tr("moderation.link_missing").into());
            }
            engine.change(|config| config.moderation_links.retain(|link| link.id != item_id));
            Ok(json!({"removed": true}))
        }
        "youtube_unban" => {
            let account = youtube_account(engine);
            let found = !account.is_empty()
                && engine
                    .config()
                    .youtube_moderation_bans
                    .iter()
                    .any(|ban| ban.id == item_id && ban.channel_id == account);
            if !found {
                return Err(tr("moderation.youtube_unknown").into());
            }
            let youtube = engine
                .youtube()
                .ok_or_else(|| tr("moderation.youtube_disconnected").to_string())?;
            if YOUTUBE.with_borrow(|s| s.busy && s.account == account) {
                return Err(tr("moderation.working").into());
            }
            let generation = YOUTUBE.with_borrow_mut(|s| {
                s.generation = s.generation.wrapping_add(1);
                s.account = account.clone();
                s.busy = true;
                s.error = false;
                s.message.clear();
                s.generation
            });
            let id = item_id.to_string();
            crate::host::changed();
            tokio::task::spawn_local(async move {
                finish_youtube(&account, generation, youtube.remove_ban(&id).await);
            });
            Ok(json!({"accepted": true}))
        }
        "unban" => {
            let found = SNAPSHOT.with_borrow(|s| {
                s.loaded
                    && !s.busy
                    && s.account == account(engine)
                    && s.rows.iter().any(|row| row["user_id"].as_str() == Some(item_id))
            });
            if !found {
                return Err(tr("moderation.refresh_first").into());
            }
            let user = item_id.to_string();
            let (generation, twitch) = begin(engine)?;
            let engine = engine.clone();
            tokio::task::spawn_local(async move {
                let result = twitch
                    .helix(
                        "DELETE",
                        "moderation/bans",
                        vec![
                            ("broadcaster_id", "{broadcaster_id}"),
                            ("moderator_id", "{moderator_id}"),
                            ("user_id", &user),
                        ],
                        None,
                        false,
                        true,
                    )
                    .await;
                let result = match result {
                    Ok(_) => fetch(&twitch)
                        .await
                        .map_err(|error| crate::i18n::trf("moderation.done_refresh_failed", &[("error", &error)])),
                    Err(error) => Err(error),
                };
                finish(&engine, generation, result);
            });
            Ok(json!({"accepted": true}))
        }
        _ => Err("Unknown moderation action".into()),
    }
}

pub fn read(engine: &Rc<Engine>) -> Result<Value, String> {
    let shield_key = account(engine);
    SHIELD.with_borrow_mut(|s| {
        if s.account != shield_key {
            *s = Shield {
                account: shield_key,
                generation: s.generation.wrapping_add(1),
                ..Shield::default()
            };
        }
    });
    let terms_key = account(engine);
    TERMS.with_borrow_mut(|s| {
        if s.account != terms_key {
            s.account = terms_key;
            s.generation = s.generation.wrapping_add(1);
            s.rows.clear();
            s.loaded = false;
            s.busy = false;
            s.error.clear();
        }
    });
    let automod_key = account(engine);
    AUTOMOD.with_borrow_mut(|queue| {
        if queue.account != automod_key {
            let generation = queue.generation.wrapping_add(1);
            *queue = AutoMod {
                account: automod_key,
                generation,
                ..AutoMod::default()
            };
        }
    });
    let request_key = account(engine);
    REQUESTS.with_borrow_mut(|s| {
        if s.account != request_key {
            s.account = request_key;
            s.generation = s.generation.wrapping_add(1);
            s.rows.clear();
            s.loaded = false;
            s.busy = false;
            s.error.clear();
        }
    });
    let links: Vec<Value> = engine
        .config()
        .moderation_links
        .iter()
        .map(|link| {
            json!({"id": link.id, "name": link.name,
        "enabled": true, "cells": [link.name, link.twitch, link.kick, link.youtube, link.tiktok]})
        })
        .collect();
    let youtube_key = youtube_account(engine);
    YOUTUBE.with_borrow_mut(|s| {
        if s.account != youtube_key {
            let generation = s.generation.wrapping_add(1);
            *s = KickState {
                account: youtube_key.clone(),
                generation,
                ..KickState::default()
            };
        }
    });
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let youtube_rows: Vec<Value> = engine.config().youtube_moderation_bans.iter().filter(|ban| !youtube_key.is_empty() && ban.channel_id == youtube_key && ban.expires_at.is_none_or(|end| end > now)).map(|ban| {
        json!({"id": ban.id, "name": ban.user_id, "enabled": true, "cells": [ban.user_id, ban.live_chat_id,
            ban.expires_at.map_or_else(|| tr("moderation.banned").to_string(), |expires| crate::fmt::shown_when(expires.min(i64::MAX as u64) as i64))]})
    }).collect();
    let kick_key = kick_account(engine);
    KICK_OBSERVED.with_borrow_mut(|s| {
        if s.account != kick_key {
            *s = KickObserved {
                account: kick_key.clone(),
                ..KickObserved::default()
            };
        }
    });
    KICK.with_borrow_mut(|s| {
        if s.account != kick_key {
            let generation = s.generation.wrapping_add(1);
            *s = KickState {
                account: kick_key,
                generation,
                ..KickState::default()
            };
        }
    });
    let key = account(engine);
    let needs_reset = SNAPSHOT.with_borrow(|s| s.account != key);
    if needs_reset {
        SNAPSHOT.with_borrow_mut(|s| {
            s.account = key;
            s.generation = s.generation.wrapping_add(1);
            s.rows.clear();
            s.loaded = false;
            s.busy = false;
            s.error.clear();
        });
    }
    let mut model = SNAPSHOT.with_borrow(|s| {
        let rows: Vec<_> = s.rows.iter().filter_map(|row| {
            let id = row["user_id"].as_str()?;
            let expiry = row["expires_at"].as_str().unwrap_or_default();
            Some(json!({"id": id, "name": row["user_name"], "enabled": true, "cells": [
                row["user_name"], if expiry.is_empty() { tr("moderation.banned").to_string() } else { tr("moderation.timeout").to_string() },
                if expiry.is_empty() { String::new() } else { crate::fmt::shown_iso(expiry, true) }, row["reason"], row["moderator_name"]
            ]}))
        }).collect();
        let status = if s.busy { tr("moderation.loading").to_string() } else if !s.error.is_empty() { s.error.clone() }
            else if s.loaded { tr("moderation.loaded").to_string() } else { tr("moderation.refresh_prompt").to_string() };
        let mut refresh_button = button_if(button("refresh", "btn.refresh", "refresh-cw", false), !s.busy);
        refresh_button["top"] = true.into();
        let kick_rows = KICK_OBSERVED.with_borrow(|seen| seen.rows.iter().rev().map(|row| json!({"id": row["id"].as_u64().unwrap_or_default().to_string(), "name": row["name"], "enabled": true,
            "cells": [row["name"], match row["permanent"].as_bool() { Some(true) => tr("moderation.banned"), Some(false) => tr("moderation.timeout"), None => tr("actions.unknown") },
                crate::fmt::shown_iso(row["expires"].as_str().unwrap_or_default(), true), row["reason"], row["moderator"]]})).collect::<Vec<_>>());
        let kick = KICK.with_borrow(|kick| json!({"kind": "moderation_kick", "domain": DOMAIN,
            "title": "Kick", "showTitle": true, "icon": "shield", "intro": tr("moderation.kick_partial"),
            "status": if kick.busy || !kick.message.is_empty() { json!({"health": if kick.busy { "busy" } else if kick.error { "down" } else { "up" },
                "text": if kick.busy { tr("moderation.working") } else { &kick.message }}) } else { Value::Null },
            "columns": [column("user", "moderation.user", 180), column("type", "moderation.type", 110), column("expires", "moderation.expires", 160), column("reason", "moderation.reason", 220), column("moderator", "moderation.moderator", 140)], "rows": kick_rows, "empty": tr("moderation.kick_empty"), "search": true,
            "actions": [{"entity": "moderation_kick_ban", "id": "kick_ban", "label": tr("moderation.ban_title"), "icon": "plus", "needs": "none", "top": true, "enabled": !kick.busy},
                {"entity": "moderation_kick_unban", "id": "kick_unban", "label": tr("moderation.unban"), "icon": "user", "needs": "none", "top": true, "enabled": !kick.busy},
                {"id": "kick_open", "label": tr("moderation.kick_open"), "icon": "external-link", "needs": "none", "top": true, "enabled": !engine.config().kick.channel.trim().is_empty(), "tip": engine.config().kick.channel.trim().is_empty().then(|| tr("moderation.channel_missing"))},
                button_if(button("kick_unban_observed", "moderation.unban", "user", true), !kick.busy)]
        }));
        let youtube = YOUTUBE.with_borrow(|state| json!({"kind": "moderation_youtube", "domain": DOMAIN,
            "title": "YouTube", "showTitle": true, "icon": "shield", "intro": tr("moderation.youtube_partial"),
            "status": if state.busy || !state.message.is_empty() { json!({"health": if state.busy { "busy" } else if state.error { "down" } else { "up" },
                "text": if state.busy { tr("moderation.working") } else { &state.message }}) } else { Value::Null },
            "columns": [column("user", "moderation.youtube_user", 180), column("chat", "moderation.youtube_chat", 180), column("expires", "moderation.expires", 160)],
            "rows": youtube_rows, "empty": tr("moderation.youtube_empty"), "search": true,
            "actions": [{"entity": "moderation_youtube_ban", "id": "youtube_ban", "label": tr("moderation.ban_title"), "icon": "plus", "needs": "none", "top": true, "enabled": !state.busy},
                button_if(button("youtube_unban", "moderation.unban", "user", true), !state.busy)]
        }));
        let linked = json!({"kind": "moderation_links", "domain": DOMAIN, "title": tr("moderation.link_title"),
            "showTitle": true, "icon": "link", "intro": tr("moderation.link_note"), "empty": tr("moderation.link_empty"), "search": true,
            "columns": [column("name", "moderation.link_name", 140), column("twitch", "moderation.twitch_id", 130), column("kick", "moderation.kick_id", 130), column("youtube", "moderation.youtube_user", 180), column("tiktok", "moderation.tiktok_id", 150)],
            "rows": links, "actions": [{"id": "link_new", "entity": "moderation_link:", "label": tr("moderation.link_add"), "icon": "plus", "needs": "none", "top": true},
                {"id": "link_edit", "form": "moderation_link:", "label": tr("btn.edit"), "icon": "pencil", "needs": "row"},
                {"id": "link_apply", "form": "moderation_apply:", "label": tr("moderation.cross_title"), "icon": "shield", "needs": "row"},
                {"id": "link_delete", "label": tr("btn.delete"), "icon": "trash-2", "needs": "row", "confirm": tr("moderation.link_delete_confirm"), "confirmLabel": tr("btn.delete"), "danger": true}]});
        let results = CROSS.with_borrow(|run| json!({"kind": "moderation_results", "domain": DOMAIN,
            "title": tr("moderation.cross_results"), "showTitle": true, "icon": "list", "rows": run.rows,
            "status": if run.busy { json!({"health": "busy", "text": tr("moderation.working")}) } else { Value::Null },
            "columns": [column("platform", "moderation.platform", 120), column("user", "moderation.user", 180), column("result", "moderation.result", 300)],
            "empty": tr("moderation.cross_empty"), "search": false, "actions": []}));
        let shield = SHIELD.with_borrow(|s| {
            let status = if s.busy { tr("moderation.working") } else if !s.error.is_empty() { &s.error } else if s.active == Some(true) { tr("moderation.shield_on") } else if s.active == Some(false) { tr("moderation.shield_off") } else { tr("moderation.shield_unknown") };
            json!({"kind": "moderation_shield", "domain": DOMAIN, "title": tr("moderation.shield"), "showTitle": true, "icon": "shield", "intro": tr("moderation.shield_note"),
                "status": {"health": if s.busy { "busy" } else if !s.error.is_empty() { "down" } else if s.active.is_some() { "up" } else { "off" }, "text": status},
                "columns": [column("state", "moderation.type", 160)], "rows": [{"id": "shield", "name": status, "cells": [status], "enabled": true}],
                "search": false, "actions": [{"id": "shield_refresh", "label": tr("btn.refresh"), "icon": "refresh-cw", "needs": "none", "top": true, "enabled": !s.busy},
                    {"id": "shield_on", "label": tr("moderation.shield_enable"), "icon": "shield", "needs": "none", "top": true, "enabled": !s.busy && s.active != Some(true)},
                    {"id": "shield_off", "label": tr("moderation.shield_disable"), "icon": "shield-off", "needs": "none", "top": true, "enabled": !s.busy && s.active != Some(false)}]})
        });
        let terms = TERMS.with_borrow(|terms| {
            let rows: Vec<Value> = terms.rows.iter().filter_map(|row| {
                let id = row["id"].as_str()?;
                Some(json!({"id": id, "name": row["text"], "enabled": true, "cells": [row["text"], crate::fmt::shown_iso(row["created_at"].as_str().unwrap_or_default(), true), crate::fmt::shown_iso(row["expires_at"].as_str().unwrap_or_default(), true)]}))
            }).collect();
            json!({"kind": "moderation_terms", "domain": DOMAIN, "title": tr("moderation.terms"), "showTitle": true, "icon": "shield", "intro": tr("moderation.terms_note"),
                "status": {"health": if terms.busy { "busy" } else if !terms.error.is_empty() { "down" } else if terms.loaded { "up" } else { "off" },
                    "text": if terms.busy { tr("moderation.loading") } else if !terms.error.is_empty() { &terms.error } else if terms.loaded { tr("moderation.terms_loaded") } else { tr("moderation.terms_prompt") }},
                "columns": [column("text", "moderation.term", 300), column("created", "moderation.request_created", 160), column("expires", "moderation.expires", 160)],
                "rows": rows, "empty": tr("moderation.terms_empty"), "search": true,
                "actions": [{"id": "term_add", "entity": "moderation_term", "label": tr("moderation.term_add"), "icon": "plus", "needs": "none", "top": true, "enabled": !terms.busy},
                    {"id": "terms_refresh", "label": tr("btn.refresh"), "icon": "refresh-cw", "needs": "none", "top": true, "enabled": !terms.busy},
                    button_if(button("term_remove", "btn.delete", "trash-2", true), terms.loaded && !terms.busy)]
            })
        });
        let automod = AUTOMOD.with_borrow(|queue| {
            let rows: Vec<Value> = queue.rows.iter().filter_map(|row| {
                let id = row["message_id"].as_str()?;
                let reason = row["automod"]["category"].as_str().or_else(|| row["category"].as_str()).or_else(|| row["reason"].as_str()).unwrap_or_default();
                Some(json!({"id": id, "name": row["user_name"], "enabled": true, "cells": [row["user_name"], row["message"]["text"], reason, crate::fmt::shown_iso(row["held_at"].as_str().unwrap_or_default(), true)]}))
            }).collect();
            json!({"kind": "moderation_automod", "domain": DOMAIN, "title": tr("moderation.automod"), "showTitle": true, "icon": "shield", "intro": tr("moderation.automod_note"),
                "status": if queue.busy || !queue.error.is_empty() { json!({"health": if queue.busy { "busy" } else { "down" },
                    "text": if queue.busy { tr("moderation.working") } else { &queue.error }}) } else { Value::Null },
                "columns": [column("user", "moderation.user", 160), column("message", "moderation.message", 300), column("reason", "moderation.reason", 160), column("held", "moderation.held_at", 160)],
                "rows": rows, "empty": tr("moderation.automod_empty"), "search": true,
                "actions": [button_if(button("automod_allow", "moderation.allow", "check", true), !queue.busy), button_if(button("automod_deny", "moderation.deny", "x", true), !queue.busy)]
            })
        });
        let requests = REQUESTS.with_borrow(|requests| {
            let rows: Vec<Value> = requests.rows.iter().filter_map(|row| {
                let id = row["id"].as_str()?;
                Some(json!({"id": id, "name": row["user_name"], "enabled": true, "cells": [row["user_name"], row["text"], crate::fmt::shown_iso(row["created_at"].as_str().unwrap_or_default(), true)]}))
            }).collect();
            json!({"kind": "moderation_requests", "domain": DOMAIN, "title": tr("moderation.requests"), "showTitle": true, "icon": "message-square", "intro": tr("moderation.requests_note"),
                "status": {"health": if requests.busy { "busy" } else if !requests.error.is_empty() { "down" } else if requests.loaded { "up" } else { "off" },
                    "text": if requests.busy { tr("moderation.loading") } else if !requests.error.is_empty() { &requests.error } else if requests.loaded { tr("moderation.requests_loaded") } else { tr("moderation.requests_prompt") }},
                "columns": [column("user", "moderation.user", 180), column("text", "moderation.request_text", 300), column("created", "moderation.request_created", 160)],
                "rows": rows, "empty": tr("moderation.requests_empty"), "search": true,
                "actions": [{"id": "requests_refresh", "label": tr("btn.refresh"), "icon": "refresh-cw", "needs": "none", "top": true, "enabled": !requests.busy},
                    {"id": "request_approve", "form": "moderation_resolve:approved:", "label": tr("moderation.approve"), "icon": "check", "needs": "row", "enabled": requests.loaded && !requests.busy},
                    {"id": "request_deny", "form": "moderation_resolve:denied:", "label": tr("moderation.deny"), "icon": "x", "needs": "row", "enabled": requests.loaded && !requests.busy}]
            })
        });
        let tiktok = json!({"kind": "moderation_tiktok", "domain": DOMAIN, "title": "TikTok", "showTitle": true, "icon": "shield",
            "intro": tr("moderation.tiktok_note"), "columns": [column("user", "moderation.user", 180)], "rows": [],
            "empty": tr("moderation.tiktok_empty"), "search": false,
            "actions": [{"id": "tiktok_open", "label": tr("moderation.tiktok_open"), "icon": "external-link", "needs": "none", "top": true, "enabled": !engine.config().tiktok.channel.trim().is_empty(), "tip": engine.config().tiktok.channel.trim().is_empty().then(|| tr("moderation.channel_missing"))} ]});
        json!({"domain": DOMAIN, "tables": [{"kind": "moderation_twitch", "domain": DOMAIN,
            "title": "Twitch", "showTitle": true, "icon": "shield", "intro": tr("moderation.intro"),
            "status": {"health": if s.busy { "busy" } else if !s.error.is_empty() { "down" } else if s.loaded { "up" } else { "off" }, "text": status},
            "columns": [column("user", "moderation.user", 180), column("type", "moderation.type", 110), column("expires", "moderation.expires", 160), column("reason", "moderation.reason", 220), column("moderator", "moderation.moderator", 140)],
            "rows": rows, "empty": tr("moderation.empty"), "search": true,
            "actions": [{"entity": "moderation_twitch_ban", "id": "ban", "label": tr("moderation.ban_title"), "icon": "plus", "needs": "none", "top": true, "enabled": !s.busy}, refresh_button, button_if(button("unban", "moderation.unban", "user", true), s.loaded && !s.busy)]
        }, kick, youtube, linked, results, requests, automod, terms, shield, super::moderation_settings::table(engine), super::moderation_chat::table(engine), super::moderation_suspicious::table(engine), super::moderation_activity::table(engine), tiktok]})
    });
    let options: Vec<_> = model["tables"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|table| json!({"id": table["kind"], "label": table["title"]}))
        .collect();
    model["tabs"] = json!({"label": tr("lists.community.show"), "selected": "moderation_twitch", "options": options});
    Ok(model)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_platform_id_matches_only_that_platforms_link() {
        let _lock = crate::TEST_LOCK.lock().unwrap_or_else(|error| error.into_inner());
        crate::engine::tests::run(async {
            let engine = crate::engine::tests::engine(json!({"actions": []}));
            engine.change(|c| {
                c.moderation_links = vec![
                    LinkedPerson {
                        id: "a".into(),
                        name: "Alice".into(),
                        kick: "42".into(),
                        ..Default::default()
                    },
                    LinkedPerson {
                        id: "b".into(),
                        name: "42".into(),
                        twitch: "7".into(),
                        ..Default::default()
                    },
                ];
            });
            assert!(find_linked_person(&engine, "42", "twitch").is_none());
            assert_eq!(find_linked_person(&engine, "42", "kick").unwrap().id, "a");
            assert_eq!(find_linked_person(&engine, "alice", "").unwrap().id, "a");
            let args = json!({"userType": "Twitch", "userId": "42", "userName": "Alice"});
            assert!(find_linked_person_from_args(&engine, args.as_object().unwrap()).is_none());
            // Linking a Twitch ID makes a new person, not a merge into Alice's Kick ID.
            let bob = link_identity_from_action(&engine, "Bob", "42", "", "", "").unwrap();
            assert!(bob.id != "a" && bob.kick.is_empty());
            assert_eq!(find_linked_person(&engine, "Alice", "").unwrap().kick, "42");
        });
    }

    #[test]
    fn kick_observed_bans_are_partial_and_unban_by_id() {
        let _lock = crate::TEST_LOCK.lock().unwrap_or_else(|error| error.into_inner());
        crate::engine::tests::run(async {
            let engine = crate::engine::tests::engine(json!({"actions": []}));
            engine.change(|c| c.kick.channel = "owner".into());
            let fakes = crate::scenario::fakes::install(&engine, &json!({"kick": {}}), None);
            KICK.with_borrow_mut(|s| *s = KickState::default());
            KICK_OBSERVED.with_borrow_mut(|s| *s = KickObserved::default());
            let ban = json!({"user": {"id": 42, "username": "Viewer"}, "banned_by": {"username": "Mod"}, "reason": "Spam", "permanent": true});
            kick_ban_event(&engine, "different", &ban);
            assert!(KICK_OBSERVED.with_borrow(|s| s.rows.is_empty()));
            kick_ban_event(&engine, "owner", &ban);
            let view = read(&engine).unwrap();
            assert_eq!(view["tables"][1]["rows"][0]["id"], "42");
            kick_unban_observed(&engine, "42").unwrap();
            for _ in 0..10 {
                tokio::task::yield_now().await;
            }
            assert!(fakes
                .calls
                .borrow()
                .iter()
                .any(|c| c["call"] == "unbanId" && c["userId"] == 42));
            assert!(KICK_OBSERVED.with_borrow(|s| s.rows.is_empty()));
            crate::scenario::fakes::uninstall();
            KICK.with_borrow_mut(|s| *s = KickState::default());
            KICK_OBSERVED.with_borrow_mut(|s| *s = KickObserved::default());
        });
    }

    #[test]
    fn every_moderation_section_is_available_in_show() {
        let _lock = crate::TEST_LOCK.lock().unwrap_or_else(|error| error.into_inner());
        crate::engine::tests::run(async {
            let engine = crate::engine::tests::engine(json!({"actions": []}));
            let model = read(&engine).unwrap();
            let tables = model["tables"].as_array().unwrap();
            let options = model["tabs"]["options"].as_array().unwrap();
            assert_eq!(options.len(), tables.len());
            assert_eq!(model["tabs"]["selected"], tables[0]["kind"]);
            for (table, option) in tables.iter().zip(options) {
                assert_eq!(option["id"], table["kind"]);
                assert_eq!(option["label"], table["title"]);
                assert!(!option["label"].as_str().unwrap().is_empty());
            }
        });
    }

    #[test]
    fn external_channel_links_require_a_configured_channel() {
        let _lock = crate::TEST_LOCK.lock().unwrap_or_else(|error| error.into_inner());
        crate::engine::tests::run(async {
            let engine = crate::engine::tests::engine(json!({"actions": []}));
            assert!(act(&engine, "kick_open", "").is_err());
            assert!(act(&engine, "tiktok_open", "").is_err());
        });
    }

    #[test]
    fn linked_unban_uses_each_platforms_stable_identifier() {
        let _lock = crate::TEST_LOCK.lock().unwrap_or_else(|error| error.into_inner());
        crate::engine::tests::run(async {
            let engine = crate::engine::tests::engine(json!({"actions": []}));
            engine.change(|c| {
                c.youtube.broadcaster = Some(crate::youtube::YouTubeAccount {
                    channel_id: "owner".into(),
                    ..Default::default()
                })
            });
            let fakes =
                crate::scenario::fakes::install(&engine, &json!({"twitch": {}, "kick": {}, "youtube": {}}), None);
            remember_youtube(&engine, "ban-id", "yt-user", "chat-id", None);
            let link = json!({"name": "Person", "twitch": "123", "kick": "456", "youtube": "yt-user"})
                .as_object()
                .unwrap()
                .clone();
            let id = save_link(&engine, "", link).unwrap()["id"]
                .as_str()
                .unwrap()
                .to_string();
            let mut item = external_form(&engine, &format!("moderation_apply:{id}"))
                .unwrap()
                .unwrap()["item"]
                .as_object()
                .unwrap()
                .clone();
            item.insert("operation".into(), 1.into());
            for platform in ["twitch", "kick", "youtube"] {
                item.insert(platform.into(), true.into());
            }
            CROSS.with_borrow_mut(|run| *run = CrossRun::default());
            apply_link(&engine, &id, &item).unwrap();
            for _ in 0..20 {
                tokio::task::yield_now().await;
            }
            let calls = fakes.calls.borrow();
            assert!(calls
                .iter()
                .any(|c| c["call"] == "DELETE moderation/bans" && c["query"]["user_id"] == "123"));
            assert!(calls.iter().any(|c| c["call"] == "unbanId" && c["userId"] == 456));
            assert!(calls.iter().any(|c| c["call"] == "removeBan" && c["id"] == "ban-id"));
            assert_eq!(CROSS.with_borrow(|run| run.rows.len()), 3);
            drop(calls);
            CROSS.with_borrow_mut(|run| *run = CrossRun::default());
            crate::scenario::fakes::uninstall();
        });
    }

    #[test]
    fn shield_mode_fetch_update_and_event_are_scoped_to_channel() {
        let _lock = crate::TEST_LOCK.lock().unwrap_or_else(|error| error.into_inner());
        crate::engine::tests::run(async {
            let engine = crate::engine::tests::engine(json!({"actions": []}));
            engine.change(|c| c.twitch.channel_id = "owner".into());
            let fakes = crate::scenario::fakes::install(
                &engine,
                &json!({"twitch": {
                    "GET moderation/shield_mode": {"data": [{"is_active": false}]},
                    "PUT moderation/shield_mode": {"data": [{"is_active": true}]}
                }}),
                None,
            );
            SHIELD.with_borrow_mut(|s| *s = Shield::default());
            shield(&engine, None).unwrap();
            for _ in 0..10 {
                tokio::task::yield_now().await;
            }
            assert_eq!(SHIELD.with_borrow(|s| s.active), Some(false));
            shield(&engine, Some(true)).unwrap();
            for _ in 0..10 {
                tokio::task::yield_now().await;
            }
            assert_eq!(SHIELD.with_borrow(|s| s.active), Some(true));
            assert!(fakes
                .calls
                .borrow()
                .iter()
                .any(|c| c["call"] == "PUT moderation/shield_mode" && c["body"]["is_active"] == true));
            shield_event(&engine, &json!({"broadcaster_user_id": "other"}), false);
            assert_eq!(SHIELD.with_borrow(|s| s.active), Some(true));
            shield_event(&engine, &json!({"broadcaster_user_id": "owner"}), false);
            assert_eq!(SHIELD.with_borrow(|s| s.active), Some(false));
            crate::scenario::fakes::uninstall();
            SHIELD.with_borrow_mut(|s| *s = Shield::default());
        });
    }

    #[test]
    fn blocked_terms_are_loaded_and_removed_by_id() {
        let _lock = crate::TEST_LOCK.lock().unwrap_or_else(|error| error.into_inner());
        crate::engine::tests::run(async {
            let engine = crate::engine::tests::engine(json!({"actions": []}));
            let fakes = crate::scenario::fakes::install(
                &engine,
                &json!({"twitch": {
                    "GET moderation/blocked_terms": {"$seq": [{"data": [{"id": "term-id", "text": "spam"}], "pagination": {}}, {"data": [], "pagination": {}}]}
                }}),
                None,
            );
            TERMS.with_borrow_mut(|s| *s = Snapshot::default());
            assert!(add_term(&engine, json!({"text": "*x"}).as_object().unwrap()).is_err());
            refresh_terms(&engine).unwrap();
            for _ in 0..10 {
                tokio::task::yield_now().await;
            }
            assert_eq!(read(&engine).unwrap()["tables"][7]["rows"][0]["id"], "term-id");
            assert!(remove_term(&engine, "other").is_err());
            remove_term(&engine, "term-id").unwrap();
            for _ in 0..10 {
                tokio::task::yield_now().await;
            }
            assert!(fakes
                .calls
                .borrow()
                .iter()
                .any(|call| call["call"] == "DELETE moderation/blocked_terms" && call["query"]["id"] == "term-id"));
            assert!(TERMS.with_borrow(|s| s.loaded && s.rows.is_empty()));
            crate::scenario::fakes::uninstall();
            TERMS.with_borrow_mut(|s| *s = Snapshot::default());
        });
    }

    #[test]
    fn automod_keeps_failed_reviews_and_rejects_other_channels() {
        let _lock = crate::TEST_LOCK.lock().unwrap_or_else(|error| error.into_inner());
        crate::engine::tests::run(async {
            let engine = crate::engine::tests::engine(json!({"actions": []}));
            engine.change(|config| config.twitch.channel_id = "owner".into());
            let fakes = crate::scenario::fakes::install(
                &engine,
                &json!({"twitch": {"POST moderation/automod/message": {"error": "Permission denied"}}}),
                None,
            );
            AUTOMOD.with_borrow_mut(|queue| *queue = AutoMod::default());
            let mut event = json!({"broadcaster_user_id": "other", "message_id": "msg", "user_name": "Person",
                "message": {"text": "hello"}, "automod": {"category": "swearing", "level": 2}});
            automod_event(&engine, &event, true);
            assert!(AUTOMOD.with_borrow(|queue| queue.rows.is_empty()));
            event["broadcaster_user_id"] = "owner".into();
            automod_event(&engine, &event, true);
            let view = read(&engine).unwrap();
            assert_eq!(view["tables"][6]["rows"][0]["cells"][2], "swearing");
            resolve_automod(&engine, "msg", false).unwrap();
            assert!(resolve_automod(&engine, "msg", true).is_err());
            for _ in 0..10 {
                tokio::task::yield_now().await;
            }
            assert!(
                AUTOMOD.with_borrow(|queue| !queue.busy && queue.rows.len() == 1 && queue.error == "Permission denied")
            );
            assert_eq!(fakes.calls.borrow().len(), 1);
            event["automod"] = Value::Null;
            event["reason"] = "blocked_term".into();
            automod_event(&engine, &event, true);
            assert_eq!(
                read(&engine).unwrap()["tables"][6]["rows"][0]["cells"][2],
                "blocked_term"
            );
            engine.change(|config| config.twitch.channel_id = "another".into());
            assert!(resolve_automod(&engine, "msg", true).is_err());
            assert!(read(&engine).unwrap()["tables"][6]["rows"]
                .as_array()
                .unwrap()
                .is_empty());
            crate::scenario::fakes::uninstall();
            AUTOMOD.with_borrow_mut(|queue| *queue = AutoMod::default());
        });
    }

    #[test]
    fn automod_queue_follows_updates_and_sends_moderator_id() {
        let _lock = crate::TEST_LOCK.lock().unwrap_or_else(|error| error.into_inner());
        crate::engine::tests::run(async {
            let engine = crate::engine::tests::engine(json!({"actions": []}));
            engine.change(|config| config.twitch.channel_id = "owner".into());
            let fakes = crate::scenario::fakes::install(&engine, &json!({"twitch": {}}), None);
            AUTOMOD.with_borrow_mut(|queue| *queue = AutoMod::default());
            let event = json!({"broadcaster_user_id": "owner", "message_id": "msg", "user_name": "Person", "message": {"text": "hello"}});
            automod_event(&engine, &event, true);
            assert_eq!(read(&engine).unwrap()["tables"][6]["rows"][0]["id"], "msg");
            resolve_automod(&engine, "msg", true).unwrap();
            for _ in 0..10 {
                tokio::task::yield_now().await;
            }
            let calls = fakes.calls.borrow();
            let call = calls
                .iter()
                .find(|call| call["call"] == "POST moderation/automod/message")
                .unwrap();
            assert_eq!(
                call["body"],
                json!({"user_id": "{moderator_id}", "msg_id": "msg", "action": "ALLOW"})
            );
            assert!(AUTOMOD.with_borrow(|queue| queue.rows.is_empty()));
            drop(calls);
            automod_event(&engine, &event, true);
            automod_event(&engine, &event, false);
            assert!(AUTOMOD.with_borrow(|queue| queue.rows.is_empty()));
            crate::scenario::fakes::uninstall();
            AUTOMOD.with_borrow_mut(|queue| *queue = AutoMod::default());
        });
    }

    #[test]
    fn pending_requests_can_be_reviewed_and_resolved() {
        let _lock = crate::TEST_LOCK.lock().unwrap_or_else(|error| error.into_inner());
        crate::engine::tests::run(async {
            let engine = crate::engine::tests::engine(json!({"actions": []}));
            let fakes = crate::scenario::fakes::install(
                &engine,
                &json!({"twitch": {
                    "GET moderation/unban_requests": {"$seq": [
                        {"data": [{"id": "req", "user_name": "Person", "text": "Sorry"}], "pagination": {}},
                        {"data": [], "pagination": {}}
                    ]}
                }}),
                None,
            );
            REQUESTS.with_borrow_mut(|s| *s = Snapshot::default());
            refresh_requests(&engine).unwrap();
            for _ in 0..10 {
                tokio::task::yield_now().await;
            }
            let form = external_form(&engine, "moderation_resolve:denied:req")
                .unwrap()
                .unwrap();
            assert_eq!(form["notes"][0], "Person: Sorry");
            let item = json!({"response": "Please wait"}).as_object().unwrap().clone();
            resolve_request(&engine, "denied", "req", &item).unwrap();
            for _ in 0..10 {
                tokio::task::yield_now().await;
            }
            let calls = fakes.calls.borrow();
            let resolved = calls
                .iter()
                .find(|c| c["call"] == "PATCH moderation/unban_requests")
                .unwrap();
            assert_eq!(resolved["query"]["unban_request_id"], "req");
            assert_eq!(resolved["query"]["status"], "denied");
            assert_eq!(resolved["query"]["resolution_text"], "Please wait");
            assert!(REQUESTS.with_borrow(|s| s.loaded && s.rows.is_empty()));
            drop(calls);
            crate::scenario::fakes::uninstall();
            REQUESTS.with_borrow_mut(|s| *s = Snapshot::default());
        });
    }

    #[test]
    fn linked_action_checks_identity_and_reports_each_platform() {
        let _lock = crate::TEST_LOCK.lock().unwrap_or_else(|error| error.into_inner());
        crate::engine::tests::run(async {
            let engine = crate::engine::tests::engine(json!({"actions": []}));
            let fakes = crate::scenario::fakes::install(
                &engine,
                &json!({"twitch": {}, "kick": {"banId": {"error": "refused"}}}),
                None,
            );
            let link = json!({"name": "Person", "twitch": "123", "kick": "456"})
                .as_object()
                .unwrap()
                .clone();
            let id = save_link(&engine, "", link).unwrap()["id"]
                .as_str()
                .unwrap()
                .to_string();
            let mut item = external_form(&engine, &format!("moderation_apply:{id}"))
                .unwrap()
                .unwrap()["item"]
                .as_object()
                .unwrap()
                .clone();
            let form = external_form(&engine, &format!("moderation_apply:{id}"))
                .unwrap()
                .unwrap();
            let pinned_kind = form["kind"].as_str().unwrap().to_string();
            engine.change(|config| config.moderation_links[0].kick = "789".into());
            let stale_save = crate::servers::answer(&engine, &json!({"request": "StreamerzEditEntity", "kind": pinned_kind, "changes": [{"field": 1, "value": true}]}), true).unwrap();
            assert_eq!(stale_save["status"], "error");
            engine.change(|config| config.moderation_links[0].kick = "456".into());
            let form = external_form(&engine, &format!("moderation_apply:{id}"))
                .unwrap()
                .unwrap();
            let pinned_kind = form["kind"].as_str().unwrap().to_string();
            engine.change(|config| config.twitch.channel = "different-channel".into());
            let stale_save = crate::servers::answer(&engine, &json!({"request": "StreamerzEditEntity", "kind": pinned_kind, "changes": [{"field": 1, "value": true}]}), true).unwrap();
            assert_eq!(stale_save["status"], "error");
            engine.change(|config| config.twitch.channel.clear());
            item.insert("twitch".into(), true.into());
            item.insert("kick".into(), true.into());
            item.insert("duration".into(), 60.into());
            let mut stale = item.clone();
            stale.insert("kick_id".into(), "999".into());
            assert!(apply_link(&engine, &id, &stale).is_err());
            CROSS.with_borrow_mut(|run| *run = CrossRun::default());
            apply_link(&engine, &id, &item).unwrap();
            for _ in 0..15 {
                tokio::task::yield_now().await;
            }
            let rows = CROSS.with_borrow(|run| run.rows.clone());
            assert_eq!(rows.len(), 2);
            assert_eq!(rows[0]["cells"][2], tr("moderation.action_done"));
            assert_eq!(rows[1]["cells"][2], "refused");
            assert!(fakes
                .calls
                .borrow()
                .iter()
                .any(|c| c["call"] == "banId" && c["userId"] == 456));
            assert!(fakes
                .calls
                .borrow()
                .iter()
                .any(|c| c["call"] == "POST moderation/bans" && c["body"]["data"]["user_id"] == "123"));
            CROSS.with_borrow_mut(|run| *run = CrossRun::default());
            crate::scenario::fakes::uninstall();
        });
    }

    #[test]
    fn links_are_explicit_unique_editable_and_removable() {
        let _lock = crate::TEST_LOCK.lock().unwrap_or_else(|error| error.into_inner());
        crate::engine::tests::run(async {
            let engine = crate::engine::tests::engine(json!({"actions": []}));
            let item = json!({"name": "Person", "twitch": "123", "kick": "456", "youtube": "", "tiktok": ""})
                .as_object()
                .unwrap()
                .clone();
            let result = save_link(&engine, "", item.clone()).unwrap();
            let id = result["id"].as_str().unwrap();
            assert!(save_link(&engine, "", item.clone()).is_err());
            let mut changed = item;
            changed.insert("name".into(), "Updated".into());
            save_link(&engine, id, changed).unwrap();
            assert_eq!(
                external_form(&engine, &format!("moderation_link:{id}"))
                    .unwrap()
                    .unwrap()["item"]["name"],
                "Updated"
            );
            assert_eq!(read(&engine).unwrap()["tables"][3]["rows"].as_array().unwrap().len(), 1);
            act(&engine, "link_delete", id).unwrap();
            assert!(engine.config().moderation_links.is_empty());
        });
    }

    #[test]
    fn youtube_records_are_scoped_to_account_and_unban_uses_ban_id() {
        let _lock = crate::TEST_LOCK.lock().unwrap_or_else(|error| error.into_inner());
        crate::engine::tests::run(async {
            let engine = crate::engine::tests::engine(json!({"actions": []}));
            engine.change(|config| {
                config.youtube.broadcaster = Some(crate::youtube::YouTubeAccount {
                    channel_id: "owner".into(),
                    ..Default::default()
                })
            });
            let fakes = crate::scenario::fakes::install(&engine, &json!({"youtube": {}}), None);
            remember_youtube(&engine, "ban-id", "user-id", "chat-id", None);
            let view = read(&engine).unwrap();
            assert_eq!(view["tables"][2]["rows"][0]["id"], "ban-id");
            assert_eq!(view["tables"][2]["rows"][0]["cells"][0], "user-id");
            assert!(act(&engine, "youtube_unban", "unknown").is_err());
            act(&engine, "youtube_unban", "ban-id").unwrap();
            for _ in 0..10 {
                tokio::task::yield_now().await;
            }
            assert!(fakes
                .calls
                .borrow()
                .iter()
                .any(|call| call["call"] == "removeBan" && call["id"] == "ban-id"));
            engine.change(|config| config.youtube.broadcaster.as_mut().unwrap().channel_id = "other".into());
            assert!(read(&engine).unwrap()["tables"][2]["rows"]
                .as_array()
                .unwrap()
                .is_empty());
            assert!(act(&engine, "youtube_unban", "ban-id").is_err());
            crate::scenario::fakes::uninstall();
            YOUTUBE.with_borrow_mut(|s| *s = KickState::default());
        });
    }

    #[test]
    fn fetches_all_pages_and_sends_selected_unban() {
        let _lock = crate::TEST_LOCK.lock().unwrap_or_else(|error| error.into_inner());
        crate::engine::tests::run(async {
            let engine = crate::engine::tests::engine(json!({"actions": []}));
            let fakes = crate::scenario::fakes::install(
                &engine,
                &json!({"twitch": {
                    "GET moderation/banned": {"$seq": [
                        {"data": [{"user_id": "1", "user_name": "First"}], "pagination": {"cursor": "next"}},
                        {"data": [{"user_id": "2", "user_name": "Second"}], "pagination": {}},
                        {"data": [], "pagination": {}}
                    ]}
                }}),
                None,
            );
            SNAPSHOT.with_borrow_mut(|s| *s = Snapshot::default());
            refresh(&engine).unwrap();
            for _ in 0..10 {
                tokio::task::yield_now().await;
            }
            let view = read(&engine).unwrap();
            assert_eq!(view["tables"][0]["rows"].as_array().unwrap().len(), 2);
            assert_eq!(fakes.calls.borrow()[1]["query"]["after"], "next");
            act(&engine, "unban", "2").unwrap();
            for _ in 0..10 {
                tokio::task::yield_now().await;
            }
            assert!(fakes
                .calls
                .borrow()
                .iter()
                .any(|c| c["call"] == "DELETE moderation/bans" && c["query"]["user_id"] == "2"));
            assert!(read(&engine).unwrap()["tables"][0]["rows"]
                .as_array()
                .unwrap()
                .is_empty());
            crate::scenario::fakes::uninstall();
            SNAPSHOT.with_borrow_mut(|s| *s = Snapshot::default());
        });
    }

    #[test]
    fn direct_forms_reject_changed_accounts_and_channels() {
        let _lock = crate::TEST_LOCK.lock().unwrap_or_else(|error| error.into_inner());
        crate::engine::tests::run(async {
            let engine = crate::engine::tests::engine(json!({"actions": []}));
            for (kind, platform) in [
                ("moderation_twitch_ban", "twitch"),
                ("moderation_term", "twitch"),
                ("moderation_kick_ban", "kick"),
                ("moderation_youtube_ban", "youtube"),
            ] {
                let form = external_form(&engine, kind).unwrap().unwrap();
                let saved_kind = form["kind"].as_str().unwrap().to_string();
                engine.change(|c| match platform {
                    "twitch" => c.twitch.channel.push('x'),
                    "kick" => c.kick.channel.push('x'),
                    _ => c.youtube.channel_id.push('x'),
                });
                assert!(external_form(&engine, &saved_kind).unwrap().is_err());
                assert!(save_external(&engine, &saved_kind, Obj::new()).unwrap().is_err());
            }
        });
    }

    #[test]
    fn ban_form_validates_and_sends_duration_and_reason() {
        let _lock = crate::TEST_LOCK.lock().unwrap_or_else(|error| error.into_inner());
        crate::engine::tests::run(async {
            let engine = crate::engine::tests::engine(json!({"actions": []}));
            let fakes = crate::scenario::fakes::install(&engine, &json!({"twitch": {}}), None);
            SNAPSHOT.with_borrow_mut(|s| *s = Snapshot::default());
            let kind = external_form(&engine, "moderation_twitch_ban").unwrap().unwrap()["kind"]
                .as_str()
                .unwrap()
                .to_string();
            let invalid = json!({"user": "test", "duration": 1209601, "reason": ""})
                .as_object()
                .unwrap()
                .clone();
            assert!(save_external(&engine, &kind, invalid).unwrap().is_err());
            let valid = json!({"user": "@test", "duration": 600, "reason": "Spam"})
                .as_object()
                .unwrap()
                .clone();
            save_external(&engine, &kind, valid).unwrap().unwrap();
            for _ in 0..10 {
                tokio::task::yield_now().await;
            }
            let calls = fakes.calls.borrow();
            let ban = calls.iter().find(|c| c["call"] == "POST moderation/bans").unwrap();
            assert_eq!(
                ban["body"]["data"],
                json!({"user_id": "id-test", "duration": 600, "reason": "Spam"})
            );
            drop(calls);
            crate::scenario::fakes::uninstall();
            SNAPSHOT.with_borrow_mut(|s| *s = Snapshot::default());
        });
    }

    #[test]
    fn changing_channel_during_twitch_user_lookup_cancels_the_ban() {
        let _lock = crate::TEST_LOCK.lock().unwrap_or_else(|error| error.into_inner());
        crate::engine::tests::run(async {
            let engine = crate::engine::tests::engine(json!({"actions": []}));
            engine.change(|c| c.twitch.channel_id = "first".into());
            let (twitch, mut commands) = crate::twitch::TwitchHandle::detached();
            engine.set_twitch(twitch);
            save_direct(
                &engine,
                "moderation_twitch_ban",
                json!({"user":"person","duration":0}).as_object().unwrap().clone(),
            )
            .unwrap()
            .unwrap();
            let Some(crate::twitch::TwitchCmd::ResolveUserId { resp, .. }) = commands.recv().await else {
                panic!("expected a user lookup");
            };
            engine.change(|c| c.twitch.channel_id = "second".into());
            resp.send(Ok("42".into())).unwrap();
            for _ in 0..10 {
                tokio::task::yield_now().await;
            }
            assert!(commands.try_recv().is_err());
        });
    }

    #[test]
    fn kick_actions_report_results_without_claiming_a_complete_list() {
        let _lock = crate::TEST_LOCK.lock().unwrap_or_else(|error| error.into_inner());
        crate::engine::tests::run(async {
            let engine = crate::engine::tests::engine(json!({"actions": []}));
            let fakes = crate::scenario::fakes::install(&engine, &json!({"kick": {}}), None);
            KICK.with_borrow_mut(|s| *s = KickState::default());
            let kind = external_form(&engine, "moderation_kick_ban").unwrap().unwrap()["kind"]
                .as_str()
                .unwrap()
                .to_string();
            let item = json!({"user": "@person", "duration": 70, "reason": "Spam"})
                .as_object()
                .unwrap()
                .clone();
            save_external(&engine, &kind, item).unwrap().unwrap();
            for _ in 0..10 {
                tokio::task::yield_now().await;
            }
            assert!(fakes
                .calls
                .borrow()
                .iter()
                .any(|c| c["call"] == "timeout" && c["login"] == "person" && c["duration"] == 70));
            let view = read(&engine).unwrap();
            assert_eq!(view["tables"][1]["intro"], tr("moderation.kick_partial"));
            assert_eq!(view["tables"][1]["status"]["text"], tr("moderation.action_done"));
            crate::scenario::fakes::uninstall();
            KICK.with_borrow_mut(|s| *s = KickState::default());
        });
    }

    #[test]
    fn snapshot_rejects_old_responses_and_unlisted_users() {
        let _lock = crate::TEST_LOCK.lock().unwrap_or_else(|error| error.into_inner());
        crate::engine::tests::run(async {
            let engine = crate::engine::tests::engine(json!({"actions": []}));
            let key = account(&engine);
            SNAPSHOT.with_borrow_mut(|s| {
                *s = Snapshot {
                    account: key,
                    generation: 2,
                    busy: true,
                    ..Snapshot::default()
                };
            });
            finish(&engine, 1, Ok(vec![json!({"user_id": "old", "user_name": "Old"})]));
            assert!(SNAPSHOT.with_borrow(|s| s.busy && s.rows.is_empty()));
            finish(
                &engine,
                2,
                Ok(vec![json!({"user_id": "123", "user_name": "User", "expires_at": ""})]),
            );
            let view = crate::servers::answer(
                &engine,
                &json!({"request": "StreamerzCoreView", "domain": "moderation"}),
                true,
            )
            .unwrap();
            assert_eq!(view["tables"][0]["rows"][0]["id"], "123");
            let action = crate::servers::answer(
            &engine,
            &json!({"request": "StreamerzCoreAction", "domain": "moderation", "action": "unban", "itemId": "other"}),
            true,
        )
        .unwrap();
            assert_eq!(action["status"], "error");
            SNAPSHOT.with_borrow_mut(|s| *s = Snapshot::default());
        });
    }
}
