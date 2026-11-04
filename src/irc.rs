//! Twitch chat without a login: an anonymous IRC reader (`justinfan`), read only. Each line becomes the
//! EventSub event it stands for (`channel.chat.message`, `channel.chat.message_delete`...), so the chat,
//! the triggers and the user card take it exactly as they take EventSub's. Used when a Twitch channel is
//! set but no account is logged in.

use crate::engine::Engine;
use crate::twitch::TwitchAccount;
use crate::Config;
use futures_util::{SinkExt, StreamExt};
use serde_json::{json, Value};
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::watch;
use tokio_tungstenite::tungstenite::Message;

const URL: &str = "wss://irc-ws.chat.twitch.tv:443";

/// One IRC line, split: its tags (unescaped), the sender's login, the command, its first parameter and
/// the trailing text.
#[derive(Default, Debug, PartialEq)]
pub struct Irc {
    pub tags: Vec<(String, String)>,
    pub login: String,
    pub command: String,
    pub target: String,
    pub text: String,
}

impl Irc {
    pub fn tag(&self, k: &str) -> &str {
        self.tags.iter().find(|t| t.0 == k).map_or("", |t| t.1.as_str())
    }
}

fn unescape(v: &str) -> String {
    let mut out = String::with_capacity(v.len());
    let mut chars = v.chars();
    while let Some(c) = chars.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        match chars.next() {
            Some(':') => out.push(';'),
            Some('s') => out.push(' '),
            Some('r') => out.push('\r'),
            Some('n') => out.push('\n'),
            Some(o) => out.push(o),
            None => {}
        }
    }
    out
}

/// Splits a raw IRC line.
pub fn parse(raw: &str) -> Option<Irc> {
    let mut rest = raw.trim_end_matches(['\r', '\n']);
    let mut m = Irc::default();
    if let Some(r) = rest.strip_prefix('@') {
        let (tags, r) = r.split_once(' ')?;
        m.tags = tags
            .split(';')
            .map(|kv| {
                let (k, v) = kv.split_once('=').unwrap_or((kv, ""));
                (k.to_string(), unescape(v))
            })
            .collect();
        rest = r.trim_start();
    }
    if let Some(r) = rest.strip_prefix(':') {
        let (prefix, r) = r.split_once(' ')?;
        m.login = prefix.split('!').next().unwrap_or("").to_string();
        rest = r.trim_start();
    }
    let (head, text) = match rest.split_once(" :") {
        Some((h, t)) => (h, t.to_string()),
        None => (rest, String::new()),
    };
    let mut words = head.split(' ').filter(|w| !w.is_empty());
    m.command = words.next()?.to_string();
    m.target = words.next().unwrap_or("").to_string();
    m.text = text;
    Some(m)
}

/// The emote fragments of `text` from an IRC `emotes` tag (`25:0-4,12-16/1902:6-10`, in characters).
fn fragments(text: &str, emotes: &str) -> Vec<Value> {
    let chars: Vec<char> = text.chars().collect();
    let mut spans: Vec<(usize, usize, &str)> = emotes
        .split('/')
        .filter_map(|e| e.split_once(':'))
        .flat_map(|(id, ranges)| {
            ranges.split(',').filter_map(move |r| {
                let (a, b) = r.split_once('-')?;
                Some((a.parse().ok()?, b.parse().ok()?, id))
            })
        })
        .filter(|&(a, b, _)| a <= b && b < chars.len())
        .collect();
    spans.sort_unstable();
    let mut out = Vec::new();
    let mut at = 0;
    let text_of = |a: usize, b: usize| chars[a..b].iter().collect::<String>();
    for (a, b, id) in spans {
        if a < at {
            continue;
        }
        if a > at {
            out.push(json!({"type": "text", "text": text_of(at, a)}));
        }
        out.push(json!({"type": "emote", "text": text_of(a, b + 1), "emote": {"id": id}}));
        at = b + 1;
    }
    if at < chars.len() {
        out.push(json!({"type": "text", "text": text_of(at, chars.len())}));
    }
    out
}

/// Decode a standalone GIF message, preserving Twitch's full asset URL.
pub(crate) fn message_fragments(text: &str, emotes: &str, gifs: &str) -> Vec<Value> {
    if let Some((range, rest)) = gifs.split_once('|') {
        if let Some((id, url)) = rest.split_once('|') {
            if let Some((start, end)) = range.split_once('-') {
                if start == "0"
                    && end.parse::<usize>().ok() == text.chars().count().checked_sub(1)
                    && !id.is_empty()
                    && url.starts_with("https://")
                    && !url.contains(',')
                {
                    return vec![json!({"type": "gif", "text": text, "gif": {"id": id, "url": url}})];
                }
            }
        }
    }
    fragments(text, emotes)
}

/// What sets a message apart, as EventSub says it: from IRC's `msg-id` tag ("text" for a plain one).
fn message_type(msg_id: &str) -> &'static str {
    match msg_id {
        "highlighted-message" => "channel_points_highlighted",
        "skip-subs-mode-message" => "channel_points_sub_only",
        "gigantified-emote-message" => "power_ups_gigantified_emote",
        "animated-message" => "power_ups_message_effect",
        _ => "text",
    }
}

/// The EventSub event an IRC line stands for: its type and its payload. None for what isn't one.
pub fn event(m: &Irc, room_id: &str) -> Option<(&'static str, Value)> {
    let channel = m.target.trim_start_matches('#');
    let who = json!({
        "broadcaster_user_id": room_id,
        "broadcaster_user_login": channel,
        "broadcaster_user_name": channel,
    });
    let with = |extra: Value| {
        let mut e = who.clone();
        if let (Some(e), Some(x)) = (e.as_object_mut(), extra.as_object()) {
            e.extend(x.clone());
        }
        e
    };
    match m.command.as_str() {
        "PRIVMSG" => {
            let mut text = m.text.clone();
            let action = text
                .strip_prefix("\x01ACTION ")
                .and_then(|s| s.strip_suffix('\x01'))
                .map(str::to_string);
            let is_action = action.is_some();
            if let Some(act) = action {
                text = act;
            }
            let name = m.tag("display-name");
            let badges: Vec<Value> = m
                .tag("badges")
                .split(',')
                .filter_map(|b| b.split_once('/'))
                .map(|(set, id)| json!({"set_id": set, "id": id, "info": ""}))
                .collect();
            let mut e = with(json!({
                "chatter_user_id": m.tag("user-id"),
                "chatter_user_login": m.login,
                "chatter_user_name": if name.is_empty() { m.login.as_str() } else { name },
                "message_id": m.tag("id"),
                "message": {"text": text, "fragments": message_fragments(&text, m.tag("emotes"), m.tag("gifs"))},
                "color": m.tag("color"),
                "badges": badges,
                "is_action": is_action,
                "message_type": message_type(m.tag("msg-id")),
                "channel_points_custom_reward_id": m.tag("custom-reward-id"),
            }));
            if let Ok(bits) = m.tag("bits").parse::<u64>() {
                e["cheer"] = json!({ "bits": bits });
            }
            if !m.tag("reply-parent-msg-id").is_empty() {
                e["reply"] = json!({
                    "parent_message_id": m.tag("reply-parent-msg-id"),
                    "parent_user_id": m.tag("reply-parent-user-id"),
                    "parent_user_login": m.tag("reply-parent-user-login"),
                    "parent_user_name": m.tag("reply-parent-display-name"),
                    "parent_message_body": m.tag("reply-parent-msg-body"),
                });
            }
            Some(("channel.chat.message", e))
        }
        // A viewer's watch streak (the one notice that is a reward: it gives channel points).
        "USERNOTICE" if m.tag("msg-id") == "viewermilestone" && m.tag("msg-param-category") == "watch-streak" => {
            let name = m.tag("display-name");
            let name = if name.is_empty() { m.login.as_str() } else { name };
            let count = |k: &str| m.tag(k).parse::<u64>().unwrap_or(0);
            Some((
                "channel.chat.notification",
                with(json!({
                    "chatter_user_id": m.tag("user-id"),
                    "chatter_user_login": m.login,
                    "chatter_user_name": name,
                    "chatter_is_anonymous": false,
                    "color": m.tag("color"),
                    "badges": [],
                    "message_id": m.tag("id"),
                    "system_message": m.tag("system-msg"),
                    "notice_type": "watch_streak",
                    "message": {"text": m.text, "fragments": fragments(&m.text, m.tag("emotes"))},
                    "watch_streak": {
                        "streak_count": count("msg-param-value"),
                        "channel_points_awarded": count("msg-param-copoReward"),
                    },
                })),
            ))
        }
        "CLEARMSG" => Some((
            "channel.chat.message_delete",
            with(json!({
                "target_user_login": m.tag("login"),
                "target_user_name": m.tag("login"),
                "target_user_id": "",
                "message_id": m.tag("target-msg-id"),
            })),
        )),
        "CLEARCHAT" if !m.tag("target-user-id").is_empty() => Some((
            "channel.chat.clear_user_messages",
            with(json!({
                "target_user_id": m.tag("target-user-id"),
                "target_user_login": m.text,
                "target_user_name": m.text,
            })),
        )),
        "CLEARCHAT" => Some(("channel.chat.clear", who)),
        _ => None,
    }
}

/// A timeout or a ban as EventSub's `channel.ban` tells it (who, for how long), from Twitch's CLEARCHAT.
/// Twitch tells moderators and the owner that way, and everybody else only through the chat's own lines,
/// without who did it.
pub fn ban_event(m: &Irc, broadcaster_id: &str, now: i64) -> Option<Value> {
    let user = m.tag("target-user-id");
    if m.command != "CLEARCHAT" || user.is_empty() {
        return None;
    }
    // A ban has no duration; a timeout is for so many seconds.
    let secs = m.tag("ban-duration").parse::<i64>().ok();
    let at = |t: i64| crate::fmt::date(&crate::engine::civil_time(t), "o");
    Some(json!({
        "broadcaster_user_id": broadcaster_id,
        "user_id": user,
        "user_login": m.text,
        "user_name": m.text,
        "moderator_user_id": "",
        "moderator_user_login": "",
        "moderator_user_name": "",
        "reason": "",
        "banned_at": at(now),
        "ends_at": secs.map_or(String::new(), |d| at(now + d)),
        "duration": secs,
        "is_permanent": secs.is_none(),
    }))
}

/// Lets a login that can't see moderation (a viewer's, not a moderator's) hear timeouts and bans: the
/// chat's own lines carry them, and anyone may read those. Only while `wanted`; no messages are taken from
/// here (the login's own connection brings those).
pub async fn watch_bans(engine: Rc<Engine>, cfg: Arc<watch::Sender<Config>>, channel: TwitchAccount) {
    let mut backoff = 2u64;
    loop {
        match bans_session(&engine, &cfg, &channel).await {
            Ok(()) => backoff = 2,
            Err(e) => crate::debug(format!("Twitch: timeouts and bans: {e}")),
        }
        tokio::time::sleep(Duration::from_secs(backoff)).await;
        backoff = (backoff * 2).min(300);
    }
}

async fn bans_session(
    engine: &Rc<Engine>,
    cfg: &Arc<watch::Sender<Config>>,
    channel: &TwitchAccount,
) -> Result<(), String> {
    let (mut ws, _) = crate::http::ws_connect(URL).await.map_err(|e| e.to_string())?;
    let nick = format!("justinfan{}", 10_000 + std::process::id() % 80_000);
    for line in [
        "CAP REQ :twitch.tv/tags twitch.tv/commands".to_string(),
        "PASS SCHMOOPIIE".into(),
        format!("NICK {nick}"),
        format!("JOIN #{}", channel.login),
    ] {
        ws.send(Message::Text(line.into())).await.map_err(|e| e.to_string())?;
    }
    let mut probed = false;
    loop {
        let wait = if probed { PONG_WAIT } else { IDLE_PROBE };
        let msg = match tokio::time::timeout(wait, ws.next()).await {
            Ok(Some(m)) => m,
            Ok(None) => return Ok(()),
            Err(_) if !probed => {
                probed = true;
                ws.send(Message::Text("PING :tmi.twitch.tv".into()))
                    .await
                    .map_err(|e| e.to_string())?;
                continue;
            }
            Err(_) => return Err("no answer from Twitch's chat".into()),
        };
        probed = false;
        let text = match msg.map_err(|e| e.to_string())? {
            Message::Text(t) => t,
            Message::Close(_) => return Ok(()),
            _ => continue,
        };
        for raw in text.lines() {
            let Some(m) = parse(raw) else { continue };
            match m.command.as_str() {
                "PING" => ws
                    .send(Message::Text(format!("PONG :{}", m.text).into()))
                    .await
                    .map_err(|e| e.to_string())?,
                "RECONNECT" => return Ok(()),
                _ => {
                    let now = crate::fmt::unix_now().as_secs() as i64;
                    if let Some(e) = ban_event(&m, &channel.user_id, now) {
                        crate::twitch::notify(engine, channel, cfg, "channel.ban", &e);
                    }
                }
            }
        }
    }
}

/// Reads `channel`'s chat without a login while `wanted`, reconnecting when dropped.
pub async fn read(
    engine: Rc<Engine>,
    cfg: Arc<watch::Sender<Config>>,
    connected: Arc<AtomicBool>,
    wanted: Arc<AtomicBool>,
) {
    let mut backoff = 2u64;
    while wanted.load(Ordering::Relaxed) {
        let channel = cfg.borrow().twitch.channel.trim().to_ascii_lowercase();
        if channel.is_empty() {
            break;
        }
        match session(&engine, &cfg, &connected, &channel).await {
            Ok(()) => backoff = 2,
            Err(e) => crate::warn(format!("Twitch: chat without login: {e}")),
        }
        engine.set_chat_viewers("twitch", None);
        if connected.swap(false, Ordering::Relaxed) {
            crate::ui::changed();
        }
        if !wanted.load(Ordering::Relaxed) {
            break;
        }
        tokio::time::sleep(Duration::from_secs(backoff)).await;
        backoff = (backoff * 2).min(60);
    }
}

/// Silence before Twitch is pinged, and how long its answer is waited for.
#[cfg(not(test))]
const IDLE_PROBE: Duration = Duration::from_secs(120);
#[cfg(not(test))]
const PONG_WAIT: Duration = Duration::from_secs(20);
#[cfg(test)]
const IDLE_PROBE: Duration = Duration::from_millis(300);
#[cfg(test)]
const PONG_WAIT: Duration = Duration::from_millis(300);

async fn session(
    engine: &Rc<Engine>,
    cfg: &Arc<watch::Sender<Config>>,
    connected: &AtomicBool,
    channel: &str,
) -> Result<(), String> {
    engine.set_chat_viewers("twitch", None);
    crate::say(format!("Twitch: reading #{channel} without a login (read only)..."));
    let (mut ws, _) = crate::http::ws_connect(URL).await.map_err(|e| e.to_string())?;
    // The anonymous nick Twitch's own site uses; any number works.
    let nick = format!("justinfan{}", 10_000 + std::process::id() % 80_000);
    for line in [
        "CAP REQ :twitch.tv/tags twitch.tv/commands".to_string(),
        "PASS SCHMOOPIIE".into(),
        format!("NICK {nick}"),
        format!("JOIN #{channel}"),
    ] {
        ws.send(Message::Text(line.into())).await.map_err(|e| e.to_string())?;
    }
    let count_engine = engine.clone();
    let count_channel = channel.to_string();
    let _viewer_task = crate::emote_events::Abort(tokio::task::spawn_local(async move {
        let mut tick = tokio::time::interval(Duration::from_secs(60));
        loop {
            tick.tick().await;
            if !count_engine.viewers_wanted() {
                continue;
            }
            let headers = [
                ("Client-Id".to_string(), "kimne78kx3ncx6brgo4mv6wki5h1ko".to_string()),
                ("Content-Type".to_string(), "application/json".to_string()),
            ];
            let body = serde_json::json!({"query":"query($login: String!) { user(login: $login) { stream { viewersCount } } }", "variables":{"login":count_channel}});
            let count = match crate::http::request(
                "POST",
                "https://gql.twitch.tv/gql",
                &headers,
                body.to_string().as_bytes(),
            )
            .await
            {
                Ok(r) => serde_json::from_slice::<serde_json::Value>(&r.body)
                    .ok()
                    .and_then(|v| v["data"]["user"]["stream"]["viewersCount"].as_u64()),
                Err(_) => None,
            };
            count_engine.set_chat_viewers("twitch", count);
        }
    }));
    let mut room: Option<TwitchAccount> = None;
    // A socket that goes quiet without closing is found by asking: Twitch answers a PING with a PONG.
    let mut probed = false;
    loop {
        let wait = if probed { PONG_WAIT } else { IDLE_PROBE };
        let msg = match tokio::time::timeout(wait, ws.next()).await {
            Ok(Some(m)) => m,
            Ok(None) => break,
            Err(_) if !probed => {
                probed = true;
                ws.send(Message::Text("PING :tmi.twitch.tv".into()))
                    .await
                    .map_err(|e| e.to_string())?;
                continue;
            }
            Err(_) => return Err("no answer from Twitch's chat".into()),
        };
        probed = false;
        let text = match msg.map_err(|e| e.to_string())? {
            Message::Text(t) => t,
            Message::Close(_) => break,
            _ => continue,
        };
        for raw in text.lines() {
            let Some(m) = parse(raw) else { continue };
            match m.command.as_str() {
                "PING" => {
                    ws.send(Message::Text(format!("PONG :{}", m.text).into()))
                        .await
                        .map_err(|e| e.to_string())?;
                }
                "RECONNECT" => return Ok(()),
                "NOTICE" if room.is_none() => return Err(m.text),
                "ROOMSTATE" if room.is_none() => {
                    let id = m.tag("room-id").to_string();
                    let b = TwitchAccount {
                        user_id: id.clone(),
                        login: channel.to_string(),
                        display_name: channel.to_string(),
                        ..Default::default()
                    };
                    connected.store(true, Ordering::Relaxed);
                    crate::roles::set("twitch", crate::roles::Role::Anonymous, "");
                    crate::ui::changed();
                    crate::say(format!("Twitch: reading #{channel} (no login: read only)"));
                    if cfg.borrow().twitch.channel_id != id {
                        cfg.send_modify(|c| c.twitch.channel_id.clone_from(&id));
                    }
                    crate::twitch::load_public_badges(&id).await;
                    crate::twitch::load_public_rewards(&id).await;
                    let hist = crate::twitch::recent_messages(channel, &id).await;
                    engine.chat_welcome("twitch", hist);
                    engine.refresh_chat_badges("twitch", crate::twitch::badge_url);
                    room = Some(b);
                }
                _ => {
                    if let (Some(b), Some((kind, e))) = (&room, event(&m, room.as_ref().map_or("", |b| &b.user_id))) {
                        crate::twitch::notify(engine, b, cfg, kind, &e);
                    }
                }
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_message_becomes_the_eventsub_event() {
        let raw = "@badges=moderator/1,subscriber/12;color=#FF0000;display-name=Ana\\sB;emotes=25:0-4;id=abc;user-id=42;bits=100 :ana!ana@ana.tmi.twitch.tv PRIVMSG #chan :Kappa hi there";
        let m = parse(raw).unwrap();
        assert_eq!(
            (m.command.as_str(), m.target.as_str(), m.login.as_str()),
            ("PRIVMSG", "#chan", "ana")
        );
        let (kind, e) = event(&m, "7").unwrap();
        assert_eq!(kind, "channel.chat.message");
        assert_eq!(e["broadcaster_user_id"], "7");
        assert_eq!(e["chatter_user_name"], "Ana B");
        assert_eq!(e["chatter_user_id"], "42");
        assert_eq!(e["message"]["text"], "Kappa hi there");
        assert_eq!(
            e["message"]["fragments"][0],
            json!({"type": "emote", "text": "Kappa", "emote": {"id": "25"}})
        );
        assert_eq!(e["message"]["fragments"][1]["text"], " hi there");
        assert_eq!(e["badges"][0]["set_id"], "moderator");
        assert_eq!(e["cheer"]["bits"], 100);
    }

    #[test]
    fn rewards_and_power_ups_ride_on_the_message_tags() {
        let kind = |tags: &str| {
            let raw = format!("@{tags};id=abc;user-id=42 :ana!ana@ana.tmi.twitch.tv PRIVMSG #chan :hi");
            event(&parse(&raw).unwrap(), "7").unwrap().1
        };
        for (id, want) in [
            ("highlighted-message", "channel_points_highlighted"),
            ("skip-subs-mode-message", "channel_points_sub_only"),
            ("gigantified-emote-message", "power_ups_gigantified_emote"),
            ("animated-message", "power_ups_message_effect"),
        ] {
            assert_eq!(kind(&format!("msg-id={id}"))["message_type"], want);
        }
        assert_eq!(kind("badges=")["message_type"], "text");
        let e = kind("custom-reward-id=r-1");
        assert_eq!(e["channel_points_custom_reward_id"], "r-1");
    }

    #[test]
    fn a_watch_streak_is_a_notification() {
        let raw = "@display-name=Vlad;id=n1;login=vlad;msg-id=viewermilestone;msg-param-category=watch-streak;msg-param-copoReward=350;msg-param-value=3;system-msg=Vlad\\swatched\\s3\\sconsecutive\\sstreams;user-id=69 :tmi.twitch.tv USERNOTICE #xqc :ty";
        let (kind, e) = event(&parse(raw).unwrap(), "71").unwrap();
        assert_eq!(kind, "channel.chat.notification");
        assert_eq!(e["notice_type"], "watch_streak");
        assert_eq!(e["watch_streak"]["streak_count"], 3);
        assert_eq!(e["watch_streak"]["channel_points_awarded"], 350);
        assert_eq!(e["chatter_user_name"], "Vlad");
        // Other milestones are not this one.
        let other = raw.replace("watch-streak", "other");
        assert!(event(&parse(&other).unwrap(), "71").is_none());
    }

    #[test]
    fn an_action_and_a_one_word_message() {
        let m = parse(":a!a@a PRIVMSG #c :\x01ACTION waves\x01").unwrap();
        assert_eq!(event(&m, "1").unwrap().1["message"]["text"], "waves");
        assert_eq!(event(&m, "1").unwrap().1["is_action"], true);
        let m = parse(":a!a@a PRIVMSG #c :hi").unwrap();
        assert_eq!(event(&m, "1").unwrap().1["message"]["text"], "hi");
        assert_eq!(event(&m, "1").unwrap().1["is_action"], false);
    }

    #[test]
    fn deletions_and_clears() {
        let m = parse("@login=bob;target-msg-id=xyz :tmi.twitch.tv CLEARMSG #c :bad words").unwrap();
        let (kind, e) = event(&m, "1").unwrap();
        assert_eq!(
            (kind, e["message_id"].as_str()),
            ("channel.chat.message_delete", Some("xyz"))
        );
        let m = parse("@ban-duration=600;target-user-id=9 :tmi.twitch.tv CLEARCHAT #c :bob").unwrap();
        let (kind, e) = event(&m, "1").unwrap();
        assert_eq!(
            (kind, e["target_user_id"].as_str()),
            ("channel.chat.clear_user_messages", Some("9"))
        );
        let m = parse(":tmi.twitch.tv CLEARCHAT #c").unwrap();
        assert_eq!(event(&m, "1").unwrap().0, "channel.chat.clear");
    }

    #[test]
    fn a_timeout_and_a_ban_are_the_eventsub_ban() {
        let m = parse("@ban-duration=600;target-user-id=9 :tmi.twitch.tv CLEARCHAT #c :bob").unwrap();
        let e = ban_event(&m, "7", 1_800_000_000).unwrap();
        assert_eq!(
            (e["user_id"].as_str(), e["user_login"].as_str()),
            (Some("9"), Some("bob"))
        );
        assert_eq!(
            (e["duration"].as_i64(), e["is_permanent"].as_bool()),
            (Some(600), Some(false))
        );
        let (from, to) = (e["banned_at"].as_str().unwrap(), e["ends_at"].as_str().unwrap());
        assert_eq!(
            crate::engine::rest_parse_iso(to).unwrap() - crate::engine::rest_parse_iso(from).unwrap(),
            600
        );
        // No duration: a ban, for good.
        let m = parse("@target-user-id=9 :tmi.twitch.tv CLEARCHAT #c :bob").unwrap();
        let e = ban_event(&m, "7", 1_800_000_000).unwrap();
        assert_eq!(
            (e["is_permanent"].as_bool(), e["ends_at"].as_str()),
            (Some(true), Some(""))
        );
        // The whole chat cleared, and other lines, are none.
        assert!(ban_event(&parse(":tmi.twitch.tv CLEARCHAT #c").unwrap(), "7", 0).is_none());
        assert!(ban_event(&parse(":a!a@a PRIVMSG #c :hi").unwrap(), "7", 0).is_none());
    }

    #[test]
    fn ping_and_others_are_no_event() {
        let m = parse("PING :tmi.twitch.tv").unwrap();
        assert_eq!((m.command.as_str(), m.text.as_str()), ("PING", "tmi.twitch.tv"));
        assert!(event(&m, "1").is_none());
        assert!(event(&parse(":tmi.twitch.tv 001 justinfan1 :Welcome").unwrap(), "1").is_none());
    }

    #[test]
    fn emotes_split_by_characters_not_bytes() {
        let f = fragments("é Kappa", "25:2-6");
        assert_eq!(f[0]["text"], "é ");
        assert_eq!(f[1]["text"], "Kappa");
        assert!(fragments("hi", "25:0-9").len() == 1, "a range past the end is ignored");
    }
}

#[cfg(test)]
mod gif_tests {
    use super::*;
    #[test]
    fn standalone_gif_preserves_url_and_rejects_bad_spans() {
        let url = "https://media.giphy.com/a.gif?x=1&y=2";
        let tag = format!("0-6|id|{url}");
        let f = message_fragments("[a gif]", "", &tag);
        assert_eq!(f[0]["gif"]["url"], url);
        assert_eq!(
            message_fragments("[a gif]", "", "0-99|id|https://example.com/a")[0]["type"],
            "text"
        );
    }
}
