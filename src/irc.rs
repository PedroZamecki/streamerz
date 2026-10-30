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
                "message": {"text": text, "fragments": fragments(&text, m.tag("emotes"))},
                "color": m.tag("color"),
                "badges": badges,
                "is_action": is_action,
                "message_type": if m.tag("msg-id") == "highlighted-message" { "channel_points_highlighted" } else { "text" },
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
