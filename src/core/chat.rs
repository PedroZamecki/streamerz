//! The Chat page: every platform's chat as one list. The core decides what a line is made of (its
//! words and pictures, badges, colours, which actions we may do to it) and a frontend only draws it.
//!
//! The list streams: a frontend keeps what it was sent and asks again with the generation and the
//! `seq` it has reached, and gets only the newer lines. A change to older lines (a deletion, a trim)
//! bumps the generation and the next answer is the whole list. `StreamerzChat` carries the operations
//! (`send`, `act`, `card`, `clear`, `show`, `images`), the same for any frontend and for MCP.

use crate::{
    engine::{
        chatlog::{Line, Moderate, PLATFORMS},
        Engine,
    },
    i18n::{tr, trf},
};
use serde_json::{json, Value};
use std::cell::Cell;
use std::rc::Rc;

/// The most lines one answer carries (the log keeps at most `ChatLimits::MOST` anyway).
const MOST: usize = 1000;
/// The longest message a frontend may send in one go.
const LONGEST_MESSAGE: usize = 2000;
/// The most emotes one section of the picker carries.
const MOST_EMOTES: usize = 500;

thread_local! {
    /// Lines up to this `seq` were cleared from the view (Clear): the engine keeps them for the rest.
    static CLEARED: Cell<u64> = const { Cell::new(0) };
}

pub fn supports(domain: &str) -> bool {
    domain == "chat"
}

pub fn name(platform: &str) -> &'static str {
    match platform {
        "twitch" => "Twitch",
        "youtube" => "YouTube",
        "kick" => "Kick",
        _ => "TikTok",
    }
}

/// 1200 as "1.2k": what the viewer counter shows.
pub fn compact_viewers(n: u64) -> String {
    for (unit, suffix) in [(1_000_000_000, "b"), (1_000_000, "m"), (1_000, "k")] {
        if n >= unit {
            let whole = n / unit;
            let tenth = (n % unit) * 10 / unit;
            return if whole < 100 && tenth != 0 {
                format!("{whole}.{tenth}{suffix}")
            } else {
                format!("{whole}{suffix}")
            };
        }
    }
    n.to_string()
}

/// The colour of a name as "#rrggbb": the platform's own, lifted when too dark to read on a dark
/// background, or each platform's default palette for a line that came without one.
pub fn name_color(line: &Line) -> String {
    let hex = line.color.trim_start_matches('#');
    let (r, g, b) = match u32::from_str_radix(hex, 16) {
        Ok(value) if hex.len() == 6 => ((value >> 16) as u8, (value >> 8) as u8, value as u8),
        _ => match line.platform.as_str() {
            "youtube" => {
                if line.broadcaster {
                    (0xff, 0xd6, 0x00)
                } else if line.moderator {
                    (0x5e, 0x84, 0xf1)
                } else if line.subscriber {
                    (0x2b, 0xa6, 0x40)
                } else {
                    (0xaa, 0xaa, 0xaa)
                }
            }
            "tiktok" => {
                if line.broadcaster {
                    (0xfe, 0x2c, 0x55)
                } else if line.moderator {
                    (0x25, 0xf4, 0xee)
                } else if line.subscriber {
                    (0xff, 0x9f, 0x2e)
                } else {
                    (0x8e, 0xc5, 0xff)
                }
            }
            "kick" => {
                // kick.com's name colours.
                const PALETTE: [(u8, u8, u8); 16] = [
                    (0x00, 0xf1, 0xff),
                    (0x4c, 0xff, 0x75),
                    (0x55, 0xff, 0xc7),
                    (0x6f, 0x87, 0xff),
                    (0xaa, 0xa9, 0xff),
                    (0xbd, 0xff, 0x28),
                    (0xe2, 0x6e, 0xff),
                    (0xe4, 0xd8, 0x8f),
                    (0xe5, 0xff, 0xab),
                    (0xfe, 0xa0, 0xcf),
                    (0xff, 0x2c, 0x56),
                    (0xff, 0x41, 0x17),
                    (0xff, 0x55, 0xb3),
                    (0xff, 0xa6, 0x00),
                    (0xff, 0xae, 0x76),
                    (0xff, 0xff, 0xff),
                ];
                let hash = line
                    .user
                    .bytes()
                    .fold(0u32, |hash, byte| hash.wrapping_mul(31).wrapping_add(u32::from(byte)));
                PALETTE[hash as usize % PALETTE.len()]
            }
            _ => {
                let default = crate::twitch::default_user_color(if line.user.is_empty() {
                    &line.user_id
                } else {
                    &line.user
                });
                let value = u32::from_str_radix(default.trim_start_matches('#'), 16).unwrap_or(0x9acd32);
                ((value >> 16) as u8, (value >> 8) as u8, value as u8)
            }
        },
    };
    let luminance = 0.299 * f32::from(r) + 0.587 * f32::from(g) + 0.114 * f32::from(b);
    let (r, g, b) = if luminance < 75.0 {
        (r / 2 + 115, g / 2 + 115, b / 2 + 115)
    } else {
        (r, g, b)
    };
    format!("#{r:02x}{g:02x}{b:02x}")
}

/// The name as the line shows it (YouTube handles with their `@` when that option is on).
fn shown_name(line: &Line, youtube_tag: bool) -> String {
    if line.platform != "youtube" {
        return line.user.clone();
    }
    if youtube_tag {
        if line.user.starts_with('@') || line.user.contains(' ') {
            line.user.clone()
        } else {
            format!("@{}", line.user)
        }
    } else {
        line.user.trim_start_matches('@').to_owned()
    }
}

/// Control characters never reach the page; tabs and newlines are spaces.
fn flat(text: &str) -> String {
    text.chars().map(|c| if c.is_control() { ' ' } else { c }).collect()
}

/// What a line's words are made of: text and pictures (emotes, emoji), in order. A part is
/// `{"t": text}` or `{"e": name, "u": picture URL}`.
fn parts(engine: &Rc<Engine>, line: &Line) -> Vec<Value> {
    let text = flat(&line.text);
    let mut out: Vec<Value> = Vec::new();
    let mut plain = String::new();
    let flush = |plain: &mut String, out: &mut Vec<Value>| {
        if !plain.is_empty() {
            out.push(json!({"t": std::mem::take(plain)}));
        }
    };
    if line.deleted {
        return vec![json!({"t": text})];
    }
    let _ = engine;
    crate::emotes::want_personal(&line.platform, &line.user_id);
    // A gigantified message draws its last emote big.
    let last = if line.huge {
        text.split(' ')
            .enumerate()
            .filter(|(_, word)| line.emotes.iter().any(|emote| emote.0 == *word))
            .map(|(at, _)| at)
            .last()
    } else {
        None
    };
    for (at, word) in text.split(' ').enumerate() {
        if at > 0 {
            plain.push(' ');
        }
        let known = line
            .emotes
            .iter()
            .find(|emote| emote.0 == word)
            .map(|emote| emote.1.clone())
            .or_else(|| crate::emotes::personal(&line.platform, &line.user_id, word))
            .or_else(|| crate::emotes::third(&line.platform, word));
        if let Some(url) = known.filter(|url| !url.is_empty()) {
            flush(&mut plain, &mut out);
            out.push(json!({"e": word, "u": url, "big": last == Some(at)}));
            continue;
        }
        // Emoji inside the word become pictures, the rest stays text.
        for (piece, url) in crate::emotes::split(word) {
            match url {
                Some(url) => {
                    flush(&mut plain, &mut out);
                    out.push(json!({"e": piece, "u": url}));
                }
                None => plain.push_str(piece),
            }
        }
    }
    flush(&mut plain, &mut out);
    out
}

/// The badges a chatter wears: key, the name to show, the picture (empty: draw an icon instead).
fn badges(engine: &Rc<Engine>, line: &Line) -> Vec<Value> {
    let roles: Vec<(String, String)> = [
        ("broadcaster", line.broadcaster),
        ("moderator", line.moderator && !line.broadcaster),
        ("vip", line.vip),
        ("subscriber", line.subscriber),
    ]
    .into_iter()
    .filter(|role| role.1)
    .map(|role| (role.0.to_owned(), String::new()))
    .collect();
    let wearing = if line.badges.is_empty() { &roles } else { &line.badges };
    wearing
        .iter()
        .take(6)
        .map(|(key, url)| {
            let url = if url.is_empty() && line.platform == "twitch" {
                crate::twitch::badge_url(key)
                    .or_else(|| crate::twitch::badge_url(&format!("{key}/1")))
                    .unwrap_or_default()
            } else {
                url.clone()
            };
            let icon = match key.split('/').next().unwrap_or_default() {
                "broadcaster" | "owner" => "video",
                "moderator" | "lead_moderator" | "mod" => "shield",
                "vip" => "gem",
                "subscriber" | "founder" | "member" | "sponsor" => "star",
                "verified" => "badge-check",
                "staff" | "admin" | "global_mod" => "shield-check",
                "premium" | "prime" => "crown",
                _ => "award",
            };
            let mut badge =
                json!({"key": key, "label": engine.badge_label(&line.platform, key), "url": url, "icon": icon});
            if let Some(art) = badge_art(&line.platform, key) {
                badge["art"] = art.into();
            }
            badge
        })
        .collect()
}

/// The platform's own drawing of a badge (a file in `assets/badges`), for the ones that have no picture to fetch.
pub fn badge_art(platform: &str, key: &str) -> Option<&'static str> {
    let base = key.split('/').next().unwrap_or(key).to_lowercase();
    let base = base.strip_prefix(&format!("{platform}:")).unwrap_or(&base);
    Some(match (platform, base) {
        ("kick", "broadcaster") => "kick-broadcaster",
        ("kick", "moderator") => "kick-moderator",
        ("kick", "vip") => "kick-vip",
        ("kick", "og") => "kick-og",
        ("kick", "founder") => "kick-founder",
        ("kick", "verified") => "kick-verified",
        ("kick", "staff") => "kick-staff",
        ("kick", "sidekick") => "kick-sidekick",
        ("kick", "subscriber") => "kick-subscriber",
        ("kick", base) if base.starts_with("sub_gifter") => "kick-sub_gifter",
        ("youtube", "owner" | "broadcaster") => "youtube-owner",
        ("youtube", "moderator") => "youtube-moderator",
        ("youtube", "verified") => "youtube-verified",
        ("youtube", "member" | "subscriber") => "youtube-member",
        ("tiktok", "host" | "broadcaster") => "tiktok-host",
        ("tiktok", "moderator") => "tiktok-moderator",
        ("tiktok", "subscriber") => "tiktok-subscriber",
        ("tiktok", "friend") => "tiktok-friend",
        _ => return None,
    })
}

/// What we may do to a line, by our role in that chat: ids a frontend maps to its menu.
fn actions(line: &Line, can_reply: bool) -> Vec<&'static str> {
    if line.note || line.welcome || line.deleted {
        return Vec::new();
    }
    let (me, my_id) = crate::roles::get(&line.platform);
    let may = crate::roles::allowed(
        &line.platform,
        me,
        &my_id,
        crate::roles::Target {
            user_id: &line.user_id,
            broadcaster: line.broadcaster,
            moderator: line.moderator,
        },
    );
    let mut list = vec!["copy"];
    if can_reply && may.reply && line.event.is_empty() {
        list.insert(0, "reply");
    }
    if may.pin && !line.msg_id.is_empty() {
        list.push(if line.pinned.is_some() { "unpin" } else { "pin" });
    }
    if may.delete && !line.msg_id.is_empty() {
        list.push("delete");
    }
    if may.timeout {
        list.push("timeout");
    }
    if may.ban {
        list.push("ban");
    }
    list
}

/// One line for a frontend.
fn line_model(engine: &Rc<Engine>, line: &Line, youtube_tag: bool, can_reply: bool, show: &Show) -> Value {
    if line.welcome {
        return json!({"seq": line.seq, "platform": line.platform, "rule": line.text});
    }
    let mut model = serde_json::Map::new();
    let mut put = |key: &str, value: Value| {
        model.insert(key.to_owned(), value);
    };
    put("seq", line.seq.into());
    put("platform", line.platform.clone().into());
    if show.time && !line.time.is_empty() {
        put("time", line.time.clone().into());
    }
    if line.note {
        put("note", true.into());
        put("text", flat(&line.text).into());
        return Value::Object(model);
    }
    put("user", shown_name(line, youtube_tag).into());
    put("userId", line.user_id.clone().into());
    put("color", name_color(line).into());
    if !line.msg_id.is_empty() {
        put("msgId", line.msg_id.clone().into());
    }
    if show.badges {
        let list = badges(engine, line);
        if !list.is_empty() {
            put("badges", list.into());
        }
    }
    put("parts", parts(engine, line).into());
    if !line.gif.is_empty() {
        put("gif", line.gif.clone().into());
    }
    if line.deleted {
        put("deleted", true.into());
    }
    if line.action {
        put("action", true.into());
    }
    if let Some(reply) = &line.reply {
        put("reply", json!({"user": reply.user, "text": flat(&reply.text)}));
    }
    if !line.event.is_empty() {
        put("event", flat(&line.event).into());
        put("icon", line.icon.clone().into());
    }
    if line.cost > 0 {
        put("cost", line.cost.into());
    }
    for (key, value) in [
        ("picture", &line.picture),
        ("accent", &line.accent),
        ("tag", &line.tag),
        ("costIcon", &line.cost_icon),
    ] {
        if !value.is_empty() {
            put(key, value.clone().into());
        }
    }
    if line.huge {
        put("huge", true.into());
    }
    let acts = actions(line, can_reply);
    if !acts.is_empty() {
        put("acts", json!(acts));
    }
    Value::Object(model)
}

/// What the page shows of each line (the chat options).
struct Show {
    time: bool,
    badges: bool,
}

/// What the chat shows: "all" (every chat together), "side" (a column each) or one platform's id. The
/// saved choice may be a word of 1.6.0 (its own language); a platform that is not there falls back to all.
fn view(config: &crate::Config, present: &[&str]) -> String {
    let saved = config.window.chat_view.as_str();
    let id = if saved.is_empty() || saved == tr("chat.view_all") {
        "all"
    } else if saved == "side" || saved == tr("chat.view_side_by_side") {
        "side"
    } else if let Some(id) = PLATFORMS.iter().find(|id| **id == saved || name(id) == saved) {
        id
    } else {
        "all"
    };
    if PLATFORMS.contains(&id) && !present.contains(&id) {
        return "all".into();
    }
    id.into()
}

fn hidden(engine: &Rc<Engine>) -> Vec<String> {
    engine.config().window.chat_hidden.clone()
}

/// The platforms' bar: who is connected, who we can send to, their viewers.
fn platforms(engine: &Rc<Engine>) -> Vec<Value> {
    let config = engine.config();
    let viewers = engine.chat_viewers();
    let hidden = &config.window.chat_hidden;
    PLATFORMS
        .iter()
        .enumerate()
        .map(|(at, &platform)| {
            let connected = engine.chat_connected(platform);
            let sends = crate::engine::can_send(&config, platform);
            let (sender, senders) = sender(engine, &config, platform);
            let mut sender = sender;
            if platform == "tiktok" || sender["name"].is_string() {
                sender["picture"] = super::accounts::account_picture(engine, platform, sender["role"] == "bot").into();
            }
            json!({
                "sender": sender, "senders": senders, "room": super::accounts::room(engine, platform),
                "id": platform, "name": name(platform), "connected": connected, "send": sends && connected,
                "present": (connected || engine.chat_has(platform)) && !hidden.iter().any(|shown| shown == platform),
                "shown": !hidden.iter().any(|shown| shown == platform),
                "viewers": viewers[at].map(compact_viewers),
                "ended": engine.chat_stream_ended(platform),
                "asBroadcaster": engine.sends_as_broadcaster(platform),
                "tip": if sends { String::new() } else if platform == "tiktok" { tr("chat.tiktok_read_only_tip").to_owned() } else { trf("chat.nobody_logged_in_tip", &[("platform", name(platform))]) },
            })
        })
        .collect()
}

/// Who the chat sends as on `platform`: the current account (name and role) and the ones to pick from.
/// A platform with both a broadcaster and a bot sends as the one picked in the chat; with one, as it.
/// TikTok sends as whoever is logged in to the browser, with nothing to pick.
fn sender(engine: &Rc<Engine>, config: &crate::Config, platform: &str) -> (Value, Vec<Value>) {
    if platform == "tiktok" {
        let user = engine.tiktok_bridge.user();
        let ready = engine.tiktok_bridge.ready() && !user.is_empty();
        let tip = if ready {
            trf("chat.tiktok_sending_as_tip", &[("name", &user)])
        } else {
            String::new()
        };
        return (
            json!({"name": ready.then_some(user), "role": "", "tip": tip}),
            Vec::new(),
        );
    }
    let [broadcaster, bot] = super::accounts::account_names(config, platform);
    let want_broadcaster = engine.sends_as_broadcaster(platform);
    let use_bot = if want_broadcaster {
        broadcaster.is_none() && bot.is_some()
    } else {
        bot.is_some() || broadcaster.is_none()
    };
    let role = if use_bot { "bot" } else { "broadcaster" };
    let account = if use_bot { bot.clone() } else { broadcaster.clone() };
    let choices = [
        ("broadcaster", tr("chat.role_broadcaster"), broadcaster),
        ("bot", tr("chat.role_bot"), bot),
    ]
    .into_iter()
    .map(|(id, label, name)| json!({"role": id, "label": label, "name": name, "current": id == role && name.is_some()}))
    .collect();
    let tip = match &account {
        Some(who) => trf(
            "chat.sending_as_tip",
            &[
                ("platform", name(platform)),
                ("name", who),
                (
                    "role",
                    if use_bot {
                        tr("chat.role_bot")
                    } else {
                        tr("chat.role_broadcaster")
                    },
                ),
            ],
        ),
        None => trf("chat.nobody_logged_in_tip", &[("platform", name(platform))]),
    };
    (
        json!({"name": account, "role": if account.is_some() { role } else { "" }, "tip": tip}),
        choices,
    )
}

/// The chat modes worth telling the reader about, one line per platform (`Twitch: Slow mode (5s)`).
fn modes(engine: &Rc<Engine>) -> Vec<String> {
    let hidden = hidden(engine);
    engine
        .chat_modes()
        .into_iter()
        .zip(["twitch", "kick"])
        .filter_map(|(modes, platform)| {
            let modes =
                modes.filter(|_| engine.chat_connected(platform) && !hidden.iter().any(|shown| shown == platform))?;
            let mut labels = Vec::new();
            if modes.emote_only {
                labels.push(tr("chat.emote_only").to_string());
            }
            if let Some(seconds) = modes.slow_seconds {
                labels.push(trf("chat.slow_mode_seconds", &[("seconds", &seconds.to_string())]));
            }
            (!labels.is_empty()).then(|| format!("{}: {}", name(platform), labels.join(", ")))
        })
        .collect()
}

fn texts() -> Value {
    let mut texts = json!({
        "show": tr("chat.show"), "hint": tr("chat.hint"), "messageAll": tr("chat.message_all_btn").trim(),
        "messageAllTip": tr("chat.message_all_tooltip"), "options": tr("chat.options"), "clear": tr("btn.clear"),
        "empty": tr("chat.no_connection"), "accounts": tr("nav.page.accounts"),
        "sendPlaceholder": tr("chat.send_placeholder"), "send": tr("chat.send_btn_tooltip"),
        "sendTo": tr("chat.send_to"), "all": tr("chat.view_all"), "side": tr("chat.view_side_by_side"), "viewLabel": tr("chat.view_label"), "viewers": tr("chat.viewers"),
        "unavailable": tr("chat.viewers_unavailable"), "off": tr("chat.sending_disabled"),
        "reply": tr("chat.act_reply"), "copy": tr("chat.act_copy"), "pin": tr("chat.act_pin"), "unpin": tr("chat.act_unpin"),
        "delete": tr("chat.act_delete_message"), "timeout": tr("chat.act_timeout"), "ban": tr("chat.act_ban"),
        "userInfo": tr("chat.user_info"), "newMessages": tr("chat.new_message"), "newMany": tr("chat.new_messages"),
        "dontReply": tr("chat.dont_reply_tooltip"), "live": tr("chat.live_below"),
        "timeouts": [[60, tr("chat.timeout_1m")], [600, tr("chat.timeout_10m")], [3600, tr("chat.timeout_1h")], [86_400, tr("chat.timeout_1d")], [604_800, tr("chat.timeout_1w")]],
        "pins": [[30, tr("chat.pin_30s")], [60, tr("chat.pin_1m")], [120, tr("chat.pin_2m")], [300, tr("chat.pin_5m")], [600, tr("chat.pin_10m")], [1800, tr("chat.pin_30m")], [0, tr("chat.pin_stream_ends")]],
        "emoteTip": tr("chat.emote_btn_tooltip"), "emoteSearch": tr("chat.search_emotes"), "emotes": tr("emotes.emoji"),
        "senderTitle": tr("chat.acct_win_title"), "senderNote": tr("chat.acct_note"), "manageAccounts": tr("chat.manage_accounts"),
        "notLoggedIn": tr("accounts.not_logged_in"),
        "historyOff": tr("chat.history_disabled_title"), "historyOffAbout": tr("chat.history_disabled_desc"),
    });
    // Kept out of the macro above, which is at its size limit.
    for (key, text) in [
        ("disconnected", tr("chat.disconnected")),
        ("reconnect", tr("chat.reconnect")),
        ("hideChat", tr("chat.hide_chat")),
    ] {
        texts[key] = text.into();
    }
    texts
}

/// The page: the platforms and the lines newer than the frontend's `seq` (or all of them).
///
/// A request carries `gen` and `seq` (where the frontend is), `have` (how many lines it holds, to
/// notice a line that left the log without a reset) and nothing else.
pub fn read(engine: &Rc<Engine>, request: &Value) -> Result<Value, String> {
    let config = engine.config();
    let hidden = hidden(engine);
    let hidden_ids: Vec<&str> = hidden.iter().map(String::as_str).collect();
    let cleared = CLEARED.get();
    let asked = request.get("seq").and_then(Value::as_u64);
    let client_gen = request.get("gen").and_then(Value::as_u64).unwrap_or(0);
    // What lines look like changes with the options: a view drawn with other ones starts over.
    let look = u64::from(!config.window.chat_no_time)
        | u64::from(!config.window.chat_no_badges) << 1
        | u64::from(config.window.chat_youtube_tag) << 2;
    let (generation, mut reset, mut lines) = engine.chat_sync(client_gen, asked.unwrap_or(0).max(cleared));
    if !reset && asked.is_some() {
        let have = request.get("have").and_then(Value::as_u64).unwrap_or(0) as usize;
        let upto = asked.unwrap_or(0);
        let held = engine
            .chat_count(upto, &hidden_ids)
            .saturating_sub(engine.chat_count(cleared, &hidden_ids));
        // A line left the log without a reset, or a platform came back: fill the view again.
        reset = have != held || request.get("look").and_then(Value::as_u64) != Some(look);
    }
    if asked.is_none() {
        reset = true;
    }
    if reset && engine.chat_last_seq() > 0 {
        lines = engine.chat_sync(u64::MAX, 0).2;
    }
    let first_kept = lines
        .iter()
        .filter(|line| line.seq > cleared && !hidden_ids.contains(&line.platform.as_str()))
        .count()
        .saturating_sub(MOST);
    let show = Show {
        time: !config.window.chat_no_time,
        badges: !config.window.chat_no_badges,
    };
    let can_reply = config.window.chat.save_history && !crate::memory_saver();
    let youtube_tag = config.window.chat_youtube_tag;
    let models: Vec<Value> = lines
        .iter()
        .filter(|line| line.seq > cleared && !hidden_ids.contains(&line.platform.as_str()))
        .skip(first_kept)
        .map(|line| line_model(engine, line, youtube_tag, can_reply, &show))
        .collect();
    let last = models
        .last()
        .and_then(|line| line["seq"].as_u64())
        .unwrap_or(asked.unwrap_or(0));
    let list = platforms(engine);
    let connected = list
        .iter()
        .any(|platform| platform["present"] == true && platform["connected"] == true);
    let pins: Vec<Value> = engine
        .chat_pins()
        .iter()
        .filter(|pin| pin.live(crate::fmt::unix_now().as_millis() as u64))
        .map(|pin| json!({"platform": pin.platform, "user": pin.user, "text": flat(&pin.text), "by": if pin.by.is_empty() { String::new() } else { trf("chat.pinned_by", &[("user", &pin.by)]) }}))
        .collect();
    Ok(json!({"domain": "chat", "chat": {
        "gen": generation, "reset": reset, "look": look, "seq": last.max(asked.unwrap_or(0)), "lines": models,
        "platforms": list, "connected": connected, "history": config.window.chat.save_history && !crate::memory_saver(),
        "pins": pins, "modes": modes(engine), "texts": texts(), "fadeIn": config.window.chat.fade_in,
        "view": view(&config, &list.iter().filter(|platform| platform["present"] == true).filter_map(|platform| platform["id"].as_str()).collect::<Vec<_>>()),
    }}))
}

/// Which line a request means: by its `seq`.
/// The line a request is about: one in the chat (`seq`), or a person of the Community (`platform` and
/// `userId`), who stands for their latest line, or for what the Community knows of them.
fn line_of(engine: &Rc<Engine>, request: &Value) -> Result<Line, String> {
    if let Some(seq) = request["seq"].as_u64() {
        return engine
            .chat_line(seq)
            .ok_or_else(|| "that line has left the chat".to_owned());
    }
    let platform = request["platform"]
        .as_str()
        .filter(|platform| PLATFORMS.contains(platform))
        .ok_or("seq: the line's number, or platform and userId")?;
    let user_id = request["userId"]
        .as_str()
        .filter(|id| !id.is_empty())
        .ok_or("userId: the chatter")?;
    let found = engine.community_user(platform, user_id).ok_or("that person is gone")?;
    let text = |key: &str| found.get(key).and_then(Value::as_str).unwrap_or_default().to_owned();
    let role = found.get("role").and_then(Value::as_u64).unwrap_or(1);
    let (_, _, lines) = engine.chatter_sync(platform, user_id, u64::MAX, 0);
    let mut line = lines.last().cloned().unwrap_or_default();
    line.platform = platform.to_owned();
    line.user_id = user_id.to_owned();
    line.user = text("display");
    line.color = text("color");
    line.broadcaster = role >= 4;
    line.moderator = role == 3;
    line.vip = role == 2;
    // Nobody acts on a message from here: the card's person is the one.
    line.msg_id.clear();
    Ok(line)
}

/// The operations of the Chat page.
pub async fn request(engine: &Rc<Engine>, request: &Value) -> Result<Value, String> {
    let accepted = || Ok(json!({"accepted": true}));
    match request["op"].as_str().unwrap_or_default() {
        "send" => send(engine, request),
        "act" => act(engine, request),
        "card" => card(engine, request).await,
        "clear" => {
            CLEARED.set(engine.chat_last_seq());
            crate::host::chat();
            accepted()
        }
        "reconnect" => {
            let platform = request["platform"].as_str().unwrap_or_default().to_owned();
            if !PLATFORMS.contains(&platform.as_str()) {
                return Err(format!("unknown platform {platform}"));
            }
            super::accounts::act(engine, "accounts", &format!("{platform}.connect"), "")
        }
        "view" => {
            let view = request["view"].as_str().unwrap_or("all");
            if !matches!(view, "all" | "side") && !PLATFORMS.contains(&view) {
                return Err(format!("unknown view {view}"));
            }
            engine
                .change(|config| config.window.chat_view = if view == "all" { String::new() } else { view.to_owned() });
            crate::host::chat();
            accepted()
        }
        "show" => {
            let platform = request["platform"].as_str().unwrap_or_default().to_owned();
            if !PLATFORMS.contains(&platform.as_str()) {
                return Err(format!("unknown platform {platform}"));
            }
            let shown = request["shown"].as_bool().ok_or("shown: true or false")?;
            engine.change(|config| {
                config.window.chat_hidden.retain(|hidden| *hidden != platform);
                if !shown {
                    config.window.chat_hidden.push(platform.clone());
                }
            });
            crate::host::chat();
            accepted()
        }
        "sender" => {
            let platform = request["platform"].as_str().unwrap_or_default().to_owned();
            if !PLATFORMS.contains(&platform.as_str()) {
                return Err(format!("unknown platform {platform}"));
            }
            let broadcaster = request["broadcaster"].as_bool().ok_or("broadcaster: true or false")?;
            engine.change(|config| {
                config.window.chat_as_broadcaster.retain(|name| *name != platform);
                if broadcaster {
                    config.window.chat_as_broadcaster.push(platform.clone());
                }
            });
            accepted()
        }
        "emotes" => emotes(engine, request),
        "favorite" => {
            let name = request["name"]
                .as_str()
                .filter(|name| !name.is_empty())
                .ok_or("name: the emote")?
                .to_owned();
            let url = request["url"].as_str().unwrap_or_default().to_owned();
            engine.change(|config| {
                let list = &mut config.window.favorite_emotes;
                match list.iter().position(|(have, _)| *have == name) {
                    Some(at) => drop(list.remove(at)),
                    None => list.push((name, url)),
                }
            });
            accepted()
        }
        "images" => {
            let urls: Vec<&str> = request["urls"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(Value::as_str)
                .collect();
            Ok(json!({"files": super::images::files(&urls)}))
        }
        other => Err(format!("chat has no operation {other}")),
    }
}

/// The emote picker's contents: the tabs a platform has and what the open one (or a search across
/// them) holds, as `[name, picture url]` pairs. Asking also starts loading the catalogs.
fn emotes(engine: &Rc<Engine>, request: &Value) -> Result<Value, String> {
    use crate::emotes::{providers_for, sections, EmoteSection, Provider};
    let config = engine.config();
    if crate::memory_saver() || !config.window.chat.save_history {
        return Err(tr("chat.sending_disabled").to_owned());
    }
    let platform = request["platform"].as_str().unwrap_or("all");
    if platform != "all" && !PLATFORMS.contains(&platform) {
        return Err(format!("unknown platform {platform}"));
    }
    crate::emotes::load_local(&config);
    let id = |provider: Provider| match provider {
        Provider::Favorites => "favorites",
        Provider::Platform => "platform",
        Provider::SevenTv => "7tv",
        Provider::Bttv => "bttv",
        Provider::Ffz => "ffz",
        Provider::Emoji => "emoji",
    };
    let providers = providers_for(platform);
    let wanted = request["provider"].as_str().unwrap_or_default();
    let active = providers
        .iter()
        .copied()
        .find(|provider| id(*provider) == wanted)
        .or_else(|| {
            providers
                .iter()
                .copied()
                .find(|provider| *provider == Provider::Platform)
        })
        .unwrap_or(providers[0]);
    let mut seen: Vec<(String, String)> = Vec::new();
    for shown in PLATFORMS
        .iter()
        .filter(|shown| (platform == "all" || **shown == platform) && **shown != "youtube")
    {
        for emote in engine.chat_emotes(shown) {
            if !seen.iter().any(|have| have.0 == emote.0) {
                seen.push(emote);
            }
        }
    }
    let as_broadcaster = sender(engine, &config, platform).0["role"] == "broadcaster";
    let favorites = &config.window.favorite_emotes;
    let query = request["query"].as_str().unwrap_or_default().trim().to_lowercase();
    let found: Vec<EmoteSection> = if query.is_empty() {
        sections(platform, active, as_broadcaster, &seen, favorites)
    } else {
        // Searching looks in every tab of the platform.
        let mut names = std::collections::HashSet::new();
        providers
            .iter()
            .filter(|provider| **provider != Provider::Favorites)
            .flat_map(|provider| {
                sections(platform, *provider, as_broadcaster, &seen, favorites)
                    .into_iter()
                    .map(move |section| (*provider, section))
            })
            .filter_map(|(provider, section)| {
                let emotes: Vec<(String, String)> = section
                    .emotes
                    .into_iter()
                    .filter(|(name, _)| name.to_lowercase().contains(&query) && names.insert(name.clone()))
                    .take(MOST_EMOTES)
                    .collect();
                (!emotes.is_empty()).then(|| EmoteSection {
                    title: format!("{}: {}", provider.label(platform), section.title),
                    note: None,
                    emotes,
                })
            })
            .collect()
    };
    let found: Vec<Value> = found
        .into_iter()
        .map(|section| {
            let count = section.emotes.len();
            let emotes: Vec<Value> = section
                .emotes
                .into_iter()
                .take(MOST_EMOTES)
                .map(|(name, url)| json!([name, url]))
                .collect();
            json!({"title": section.title, "note": section.note, "count": count, "emotes": emotes})
        })
        .collect();
    let tabs: Vec<Value> = providers
        .iter()
        .map(|provider| json!({"id": id(*provider), "label": provider.label(platform)}))
        .collect();
    let who = match sender(engine, &config, platform).0["name"].as_str() {
        Some(name) => trf("chat.as_sender_prefix", &[("name", name)]),
        None => String::new(),
    };
    Ok(json!({"emotes": {
        "platform": platform, "tabs": tabs, "active": id(active), "sections": found, "query": query,
        "favorites": favorites.iter().map(|(name, _)| name).collect::<Vec<_>>(),
        "help": format!("{who}{}", tr("chat.emote_picker_help")), "empty": tr("picker.no_matches"),
        "search": tr("chat.search_emotes"), "title": tr("chat.emote_btn_tooltip"),
        "foldHint": tr("emotes.favorites_hint"),
    }}))
}

/// Send a message to one platform or to every connected one. `replySeq` answers a line.
fn send(engine: &Rc<Engine>, request: &Value) -> Result<Value, String> {
    let text = request["text"].as_str().unwrap_or_default().trim().to_owned();
    if text.is_empty() {
        return Err("text: the message to send".into());
    }
    if text.chars().count() > LONGEST_MESSAGE {
        return Err(format!("text: at most {LONGEST_MESSAGE} characters"));
    }
    let text = flat(&text);
    if !engine.config().window.chat.save_history || crate::memory_saver() {
        return Err(tr("chat.sending_disabled").to_owned());
    }
    let target = request["platform"].as_str().unwrap_or("all");
    let reply = request["replySeq"].as_u64().and_then(|seq| engine.chat_line(seq));
    let config = engine.config();
    let to: Vec<&'static str> = PLATFORMS
        .into_iter()
        .filter(|platform| {
            if target == "all" {
                engine.chat_connected(platform) && crate::engine::can_send(&config, platform)
            } else {
                *platform == target
            }
        })
        .collect();
    if to.is_empty() {
        return Err(if target == "all" {
            tr("chat.msg_all_nowhere").to_owned()
        } else if PLATFORMS.contains(&target) {
            return Err(unavailable(engine, target));
        } else {
            format!("unknown platform {target}")
        });
    }
    if target != "all" && !(engine.chat_connected(target) && crate::engine::can_send(&config, target)) {
        return Err(unavailable(engine, target));
    }
    let mut sent = Vec::new();
    for platform in to {
        let bot = !engine.sends_as_broadcaster(platform);
        sent.extend(engine.send_chat_list(&[platform], &text, bot, true, reply.as_ref()));
        if platform == "tiktok" {
            sent.push("tiktok");
        }
    }
    Ok(json!({"sent": sent}))
}

/// Why nothing can be sent to `platform` now.
fn unavailable(engine: &Rc<Engine>, platform: &str) -> String {
    let config = engine.config();
    if !crate::engine::can_send(&config, platform) {
        if platform == "tiktok" {
            tr("chat.msg_all_needs_tiktok").to_owned()
        } else {
            trf("chat.msg_all_needs_login", &[("platform", name(platform))])
        }
    } else {
        trf("chat.state_down_title", &[("platform", name(platform))])
    }
}

/// A message's moderation. Banning asks first through a core prompt unless `confirmed`.
fn act(engine: &Rc<Engine>, request: &Value) -> Result<Value, String> {
    let line = line_of(engine, request)?;
    let act = request["act"].as_str().unwrap_or_default();
    if !actions(&line, true).contains(&act) {
        return Err(format!("{act} is not possible on that line"));
    }
    let moderation = match act {
        "delete" => Moderate::Delete,
        "unpin" => Moderate::Unpin,
        "pin" => {
            let seconds = request["secs"].as_u64().unwrap_or(0);
            if seconds != 0 && !(30..=1800).contains(&seconds) {
                return Err("secs: 30 to 1800, or 0 until the stream ends".into());
            }
            Moderate::Pin((seconds > 0).then_some(seconds))
        }
        "timeout" => {
            let seconds = request["secs"].as_u64().unwrap_or(0);
            if !(1..=1_209_600).contains(&seconds) {
                return Err("secs: 1 to 1209600".into());
            }
            Moderate::Timeout(seconds)
        }
        "ban" => Moderate::Ban,
        other => return Err(format!("unknown act {other}")),
    };
    if matches!(moderation, Moderate::Ban) && request["confirmed"] != true {
        let question = trf(
            "chat.ban_confirm",
            &[("user", &line.user), ("platform", name(&line.platform))],
        );
        let answer = crate::host::confirm(question, tr("dialogs.cancel"), tr("btn.ban"));
        let engine = engine.clone();
        tokio::task::spawn_local(async move {
            if answer.await == Ok(true) {
                engine.moderate(&line, Moderate::Ban);
            }
        });
        return Ok(json!({"accepted": true, "asked": true}));
    }
    engine.moderate(&line, moderation);
    Ok(json!({"accepted": true}))
}

/// What the platform has said about a chatter so far, and whether it is done.
type Run = (Vec<(String, String)>, bool);

thread_local! {
    /// Lookups of a chatter's card in flight or just done: what the platform has said so far, and whether
    /// it is done. The card asks again until it is, and shows what is there each time.
    static RUNS: std::cell::RefCell<std::collections::HashMap<(String, String), Run>> =
        std::cell::RefCell::default();
}

/// Start asking the platform about a chatter (once at a time), keeping what it says as it says it.
fn start_lookup(engine: &Rc<Engine>, platform: &str, user_id: &str) {
    let key = (platform.to_owned(), user_id.to_owned());
    if RUNS.with_borrow(|runs| runs.contains_key(&key)) {
        return;
    }
    RUNS.with_borrow_mut(|runs| runs.insert(key.clone(), (Vec::new(), false)));
    let engine = engine.clone();
    tokio::task::spawn_local(async move {
        let seen = key.clone();
        let found = engine
            .chatter_lookup(&key.0, &key.1, &move |facts| {
                RUNS.with_borrow_mut(|runs| {
                    if let Some(run) = runs.get_mut(&seen) {
                        run.0 = facts.to_vec();
                    }
                });
            })
            .await;
        RUNS.with_borrow_mut(|runs| runs.insert(key.clone(), (found, true)));
        tokio::time::sleep(std::time::Duration::from_secs(10)).await;
        RUNS.with_borrow_mut(|runs| runs.remove(&key));
    });
}

/// A chatter's card: what the chat knows at once (and what the platform said lately, if kept), then what
/// the platform says as it says it (`lookup`: asked again until `pending` is false). `prefetch` only starts
/// the asking, so a card opened a moment later has it.
async fn card(engine: &Rc<Engine>, request: &Value) -> Result<Value, String> {
    let line = line_of(engine, request)?;
    if line.note || line.user_id.is_empty() {
        return Err("that line has no chatter".into());
    }
    let platform = line.platform.clone();
    if request["prefetch"] == true {
        if engine.chatter_cached(&platform, &line.user_id).is_none() {
            start_lookup(engine, &platform, &line.user_id);
        }
        return Ok(json!({"accepted": true}));
    }
    let seen = engine.chatter(&platform, &line.user_id);
    let lookup = request["lookup"] == true;
    let key = (platform.clone(), line.user_id.clone());
    let mut pending = false;
    let facts = match (engine.chatter_cached(&platform, &line.user_id), lookup) {
        (Some(found), _) => found,
        (None, false) => Vec::new(),
        (None, true) => {
            start_lookup(engine, &platform, &line.user_id);
            // Whatever the platform has said in a moment goes out; the card asks again for the rest.
            for _ in 0..12 {
                if RUNS.with_borrow(|runs| runs.get(&key).is_none_or(|run| run.1 || !run.0.is_empty())) {
                    break;
                }
                tokio::time::sleep(std::time::Duration::from_millis(25)).await;
            }
            let (found, done) = RUNS.with_borrow(|runs| runs.get(&key).cloned()).unwrap_or_default();
            pending = !done;
            found
        }
    };
    let mut shown: Vec<Value> = Vec::new();
    let mut picture = String::new();
    let mut banner = String::new();
    let mut created = String::new();
    for (label, value) in &facts {
        match label.as_str() {
            "Picture" => picture = value.clone(),
            "Banner" => banner = value.clone(),
            "Account created" => {
                created = Some(crate::fmt::shown_iso(value, false))
                    .filter(|shown| !shown.is_empty())
                    .unwrap_or_else(|| value.clone());
            }
            "Following since" => {
                let when = crate::fmt::shown_iso(value, false);
                shown.push(json!({"label": label, "value": if when.is_empty() { value.clone() } else { when }}));
            }
            _ => shown.push(json!({"label": label, "value": value})),
        }
    }
    let community = engine.community_user(&platform, &line.user_id);
    let text_of = |key: &str| {
        community
            .as_ref()
            .and_then(|found| found.get(key))
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned()
    };
    let first = crate::fmt::shown_iso(&text_of("firstMessage"), false);
    let last = crate::fmt::shown_iso(&text_of("lastActive"), false);
    if !first.is_empty() {
        shown.push(json!({"label": tr("card.first_seen"), "value": first}));
    }
    if !last.is_empty() {
        shown.push(json!({"label": tr("card.last_seen"), "value": last}));
    }
    if let Some(seen) = &seen {
        shown.push(json!({"label": tr("chat.card_messages_session"), "value": seen.messages.to_string()}));
    }
    let (_, _, said) = engine.chatter_sync(&platform, &line.user_id, u64::MAX, 0);
    let skip = said.len().saturating_sub(40);
    let config = engine.config();
    let show = Show {
        time: !config.window.chat_no_time,
        badges: !config.window.chat_no_badges,
    };
    let messages: Vec<Value> = said
        .iter()
        .skip(skip)
        .map(|said| line_model(engine, said, true, false, &show))
        .collect();
    let role = if line.broadcaster {
        tr("chat.card_broadcaster")
    } else if line.moderator {
        tr("chat.card_moderator")
    } else if line.vip {
        tr("chat.card_vip")
    } else if line.subscriber {
        tr("chat.card_subscriber")
    } else {
        tr("chat.card_viewer")
    };
    let groups: Vec<Value> = engine
        .groups_of(&platform, &line.user_id)
        .into_iter()
        .map(|(group, name, member)| json!({"id": group, "name": name, "member": member}))
        .collect();
    let id = format!("{platform}|{}", line.user_id);
    // What the actions get for this person (%userName%...), for the curious.
    let mut details: Vec<Value> = seen
        .as_ref()
        .map(|seen| {
            seen.args
                .iter()
                .filter(|(key, _)| key.as_str() != "user")
                .map(|(key, value)| {
                    let value = value.as_str().map_or_else(|| value.to_string(), str::to_owned);
                    json!({"key": key, "value": value.replace(['\n', '\t'], " ")})
                })
                .collect()
        })
        .unwrap_or_default();
    if !details.iter().any(|row| row["key"] == "userId") {
        details.push(json!({"key": "userId", "value": line.user_id}));
    }
    Ok(json!({"card": {
        "details": details, "detailsLabel": tr("card.details"), "recentShort": tr("card.recent_short"), "copied": tr("chat.copied"),
        "id": id, "seq": line.seq, "loading": !lookup, "pending": pending, "user": shown_name(&line, true), "userId": line.user_id,
        "platform": platform, "platformName": name(&platform), "color": name_color(&line), "role": role,
        "picture": picture, "banner": banner, "created": created, "facts": shown, "messages": messages, "groups": groups,
        "badges": badges(engine, &line), "acts": actions(&line, false).into_iter().filter(|act| !matches!(*act, "copy" | "pin" | "unpin")).collect::<Vec<_>>(),
        "createdText": if created.is_empty() { String::new() } else { trf("card.created_on", &[("date", &created)]) }, "groupsLabel": tr("card.groups"), "noGroups": tr("card.no_groups"),
        "groupsTip": tr("card.groups_tip"), "recent": tr("card.recent"),
        "labels": {"timeout": tr("chat.timeout_10m"), "ban": tr("chat.act_ban"), "delete": tr("chat.act_delete_message"),
            "profile": tr("btn.open_profile"), "close": tr("dialogs.close")},
        "noMessages": tr("lists.community.card_no_messages"),
        "profile": engine_profile(&platform, &line),
    }}))
}

/// The person's page on their platform.
fn engine_profile(platform: &str, line: &Line) -> String {
    super::pages::profile_url(platform, &line.user, &line.user_id)
}

// The Chat options dialog: a sections page like Notifications.

fn row(key: &str, label: &str, about: &str, control: Value) -> Value {
    json!({"key": key, "label": label, "about": about, "control": control})
}

/// The dialog's rows: which chats show, what a line shows, and what the chat keeps.
pub fn options(engine: &Rc<Engine>) -> Vec<Value> {
    let config = engine.config();
    let window = &config.window;
    let limits = window.chat;
    let saver = crate::memory_saver();
    let mut rows = vec![
        row(
            "chat_time",
            tr("chat.opt_time"),
            "",
            json!({"type": "switch", "value": !window.chat_no_time}),
        ),
        row(
            "chat_badges",
            tr("chat.opt_badges"),
            "",
            json!({"type": "switch", "value": !window.chat_no_badges}),
        ),
        row(
            "chat_animated",
            tr("chat.opt_animated"),
            "",
            json!({"type": "switch", "value": !window.chat_still_emotes}),
        ),
        row(
            "chat_youtube_tag",
            tr("chat.opt_yt_tag"),
            "",
            json!({"type": "switch", "value": window.chat_youtube_tag}),
        ),
    ];
    for platform in PLATFORMS {
        rows.push(row(
            &format!("show_{platform}"),
            &trf("chat.opt_show_chat", &[("platform", name(platform))]),
            "",
            json!({"type": "switch", "value": !window.chat_hidden.iter().any(|hidden| hidden == platform)}),
        ));
    }
    let mut keep = vec![row(
        "keep_history",
        tr("settings.chat.keep_history"),
        &if saver {
            tr("settings.chat.keep_history.memory_saver").to_owned()
        } else {
            tr("settings.chat.keep_history.about").to_owned()
        },
        json!({"type": "switch", "value": limits.save_history && !saver}),
    )];
    if !saver {
        for (key, label, about, value) in [
            (
                "per_platform",
                "settings.chat.per_platform",
                "settings.chat.per_platform.about",
                limits.per_platform as u64,
            ),
            (
                "total",
                "settings.chat.total",
                "settings.chat.total.about",
                limits.total as u64,
            ),
            (
                "lifetime",
                "settings.chat.lifetime",
                "settings.chat.lifetime.about",
                limits.lifetime,
            ),
            (
                "fade_in",
                "settings.chat.fade_in",
                "settings.chat.fade_in.about",
                u64::from(limits.fade_in),
            ),
            (
                "fade_out",
                "settings.chat.fade_out",
                "settings.chat.fade_out.about",
                u64::from(limits.fade_out),
            ),
        ] {
            keep.push(row(
                key,
                tr(label),
                tr(about),
                json!({"type": "text", "value": value.to_string()}),
            ));
        }
    }
    vec![
        json!({"title": "", "icon": "message-square", "rows": rows}),
        json!({"title": tr("settings.chat"), "icon": "sliders-horizontal", "rows": keep}),
    ]
}

/// Change one option of the dialog by its row key.
pub fn set_option(engine: &Rc<Engine>, key: &str, value: &Value) -> Result<Value, String> {
    let on = || value.as_bool().ok_or_else(|| format!("{key}: true or false"));
    let number = || {
        value
            .as_u64()
            .or_else(|| value.as_str().and_then(|text| text.trim().parse().ok()))
            .ok_or_else(|| format!("{key}: a whole number"))
    };
    match key {
        "chat_time" => {
            let on = on()?;
            engine.change(|config| config.window.chat_no_time = !on);
        }
        "chat_badges" => {
            let on = on()?;
            engine.change(|config| config.window.chat_no_badges = !on);
        }
        "chat_animated" => {
            let on = on()?;
            engine.change(|config| config.window.chat_still_emotes = !on);
        }
        "chat_youtube_tag" => {
            let on = on()?;
            engine.change(|config| config.window.chat_youtube_tag = on);
        }
        "keep_history" | "per_platform" | "total" | "lifetime" | "fade_in" | "fade_out" => {
            if crate::memory_saver() {
                return Err(tr("settings.chat.keep_history.memory_saver").to_owned());
            }
            let mut limits = engine.config().window.chat;
            match key {
                "keep_history" => limits.save_history = on()?,
                "per_platform" => limits.per_platform = number()? as usize,
                "total" => limits.total = number()? as usize,
                "lifetime" => limits.lifetime = number()?,
                "fade_in" => limits.fade_in = number()?.min(u64::from(u32::MAX)) as u32,
                _ => limits.fade_out = number()?.min(u64::from(u32::MAX)) as u32,
            }
            let limits = limits.clamped();
            engine.change(|config| config.window.chat = limits);
            engine.chat_trim();
        }
        _ => {
            let Some(platform) = key
                .strip_prefix("show_")
                .filter(|platform| PLATFORMS.contains(platform))
            else {
                return Err(format!("chat_options has no setting {key}"));
            };
            let shown = on()?;
            engine.change(|config| {
                config.window.chat_hidden.retain(|hidden| hidden != platform);
                if !shown {
                    config.window.chat_hidden.push(platform.to_owned());
                }
            });
        }
    }
    crate::host::chat();
    Ok(json!({"accepted": true}))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn line(platform: &str, user: &str, text: &str) -> Line {
        Line {
            platform: platform.into(),
            user: user.into(),
            user_id: user.into(),
            text: text.into(),
            ..Default::default()
        }
    }

    #[test]
    fn viewer_counts_are_compact_without_rounding_up() {
        assert_eq!(compact_viewers(999), "999");
        assert_eq!(compact_viewers(1_250), "1.2k");
        assert_eq!(compact_viewers(1_999), "1.9k");
        assert_eq!(compact_viewers(120_000), "120k");
    }

    #[test]
    fn names_get_the_platforms_colour_lifted_when_dark_and_a_default_without_one() {
        let mut one = line("twitch", "ana", "hi");
        one.color = "#ff0000".into();
        assert_eq!(name_color(&one), "#ff0000");
        one.color = "#000000".into();
        assert_eq!(name_color(&one), "#737373");
        let mut bare = line("youtube", "bo", "hi");
        bare.moderator = true;
        assert_eq!(name_color(&bare), "#5e84f1");
        assert_eq!(
            name_color(&line("kick", "same", "x")),
            name_color(&line("kick", "same", "y"))
        );
        assert!(name_color(&line("twitch", "", "x")).starts_with('#'));
    }

    #[test]
    fn youtube_handles_follow_the_option_and_control_characters_never_pass() {
        let one = line("youtube", "@Ana", "hi");
        assert_eq!(shown_name(&one, false), "Ana");
        assert_eq!(shown_name(&line("youtube", "Ana", ""), true), "@Ana");
        assert_eq!(shown_name(&line("twitch", "Ana", ""), true), "Ana");
        assert_eq!(flat("a\u{7}b\nc\td"), "a b c d");
    }

    #[test]
    fn chat_reads_stream_by_cursor_and_reset_on_change() {
        let _lock = crate::TEST_LOCK.lock().unwrap_or_else(|error| error.into_inner());
        crate::engine::tests::run(async {
            let engine = crate::engine::tests::engine(json!({"actions": []}));
            engine.dev_mode();
            engine.change(|config| config.window.chat.save_history = true);
            engine
                .inject_chat("twitch", "ana", "ana", "viewer", "first Kappa", &json!({}))
                .unwrap();
            let first = read(&engine, &json!({})).unwrap();
            let page = &first["chat"];
            assert_eq!(page["reset"], true);
            let lines = page["lines"].as_array().unwrap();
            let mine = lines.iter().find(|line| line["user"] == "ana").unwrap();
            assert_eq!(mine["platform"], "twitch");
            assert!(mine["color"].as_str().unwrap().starts_with('#'));
            assert!(mine["parts"]
                .as_array()
                .unwrap()
                .iter()
                .any(|part| part["t"].as_str().is_some_and(|text| text.contains("first"))));
            let (gen, seq, look) = (page["gen"].clone(), page["seq"].clone(), page["look"].clone());
            let have = lines.len();
            // Nothing new: an empty delta.
            let again = read(&engine, &json!({"gen": gen, "seq": seq, "have": have, "look": look})).unwrap();
            assert_eq!(again["chat"]["reset"], false);
            assert!(again["chat"]["lines"].as_array().unwrap().is_empty());
            // A new line: only it comes.
            engine
                .inject_chat("kick", "bo", "bo", "viewer", "second", &json!({}))
                .unwrap();
            let next = read(&engine, &json!({"gen": gen, "seq": seq, "have": have, "look": look})).unwrap();
            let fresh = next["chat"]["lines"].as_array().unwrap();
            assert_eq!(fresh.len(), 1);
            assert_eq!(fresh[0]["user"], "bo");
            // A frontend that lost a line asks to be filled again.
            let behind = read(
                &engine,
                &json!({"gen": gen, "seq": seq, "have": have + 5, "look": look}),
            )
            .unwrap();
            assert_eq!(behind["chat"]["reset"], true);
            // Hiding a platform takes its lines out.
            request_sync(&engine, json!({"op": "show", "platform": "kick", "shown": false})).unwrap();
            let shown = read(&engine, &json!({})).unwrap();
            assert!(shown["chat"]["lines"]
                .as_array()
                .unwrap()
                .iter()
                .all(|line| line["platform"] != "kick"));
            assert_eq!(shown["chat"]["platforms"][2]["shown"], false);
            // Clear drops what is there; the engine keeps its log.
            request_sync(&engine, json!({"op": "clear"})).unwrap();
            assert!(read(&engine, &json!({})).unwrap()["chat"]["lines"]
                .as_array()
                .unwrap()
                .is_empty());
            assert!(engine.chat_has("twitch"));
            CLEARED.set(0);
        });
    }

    /// Run an operation that does not wait.
    fn request_sync(engine: &Rc<Engine>, value: Value) -> Result<Value, String> {
        futures_util::FutureExt::now_or_never(request(engine, &value)).ok_or("it waited")?
    }

    #[test]
    fn badges_without_a_picture_have_their_platforms_own_art() {
        assert_eq!(badge_art("kick", "kick:broadcaster"), Some("kick-broadcaster"));
        assert_eq!(badge_art("kick", "moderator"), Some("kick-moderator"));
        assert_eq!(badge_art("kick", "sub_gifter/25"), Some("kick-sub_gifter"));
        assert_eq!(badge_art("youtube", "owner"), Some("youtube-owner"));
        assert_eq!(badge_art("tiktok", "tiktok:host"), Some("tiktok-host"));
        // Twitch has real pictures only; another platform's art is never used for it.
        assert_eq!(badge_art("twitch", "moderator"), None);
        assert_eq!(badge_art("kick", "tiktok:host"), None);
    }

    #[test]
    fn the_view_is_set_by_an_operation_and_a_hidden_chat_leaves_it() {
        let _lock = crate::TEST_LOCK.lock().unwrap_or_else(|error| error.into_inner());
        crate::engine::tests::run(async {
            let engine = crate::engine::tests::engine(json!({"actions": []}));
            engine.dev_mode();
            engine.change(|config| config.window.chat.save_history = true);
            engine
                .inject_chat("twitch", "Ana", "ana", "viewer", "hi", &json!({}))
                .unwrap();
            let page = |engine: &Rc<Engine>| read(engine, &json!({})).unwrap()["chat"].clone();
            assert_eq!(page(&engine)["view"], "all");
            assert!(request_sync(&engine, json!({"op": "view", "view": "nope"})).is_err());
            request_sync(&engine, json!({"op": "view", "view": "twitch"})).unwrap();
            assert_eq!(page(&engine)["view"], "twitch");
            assert_eq!(page(&engine)["platforms"][0]["present"], true);
            // Hiding the chat takes it out of the choices, and the view falls back to all together.
            request_sync(&engine, json!({"op": "show", "platform": "twitch", "shown": false})).unwrap();
            assert_eq!(page(&engine)["platforms"][0]["present"], false);
            assert_eq!(page(&engine)["view"], "all");
            assert!(request_sync(&engine, json!({"op": "reconnect", "platform": "nope"})).is_err());
        });
    }

    #[test]
    fn the_view_is_the_saved_choice_or_all_together() {
        let mut config = crate::Config::default();
        let present = ["twitch", "kick"];
        assert_eq!(view(&config, &present), "all");
        config.window.chat_view = "side".into();
        assert_eq!(view(&config, &present), "side");
        // 1.6.0 kept the words it showed.
        config.window.chat_view = tr("chat.view_side_by_side").into();
        assert_eq!(view(&config, &present), "side");
        config.window.chat_view = "Kick".into();
        assert_eq!(view(&config, &present), "kick");
        // A chat that is not there shows all together.
        config.window.chat_view = "youtube".into();
        assert_eq!(view(&config, &present), "all");
    }

    #[test]
    fn sending_checks_the_text_and_where_it_goes_and_dev_mode_keeps_it() {
        let _lock = crate::TEST_LOCK.lock().unwrap_or_else(|error| error.into_inner());
        crate::engine::tests::run(async {
            let engine = crate::engine::tests::engine(json!({"actions": []}));
            engine.dev_mode();
            engine.change(|config| config.window.chat.save_history = true);
            assert!(request_sync(&engine, json!({"op": "send", "text": "   "})).is_err());
            assert!(request_sync(&engine, json!({"op": "send", "text": "x".repeat(3000)})).is_err());
            // Nobody to send to.
            assert!(request_sync(&engine, json!({"op": "send", "text": "hi"})).is_err());
            assert!(request_sync(&engine, json!({"op": "send", "platform": "twitch", "text": "hi"})).is_err());
            engine.change(|config| {
                config.twitch.broadcaster = Some(Default::default());
            });
            engine.simulate_chat_connection("twitch", Some(true));
            let sent = request_sync(
                &engine,
                json!({"op": "send", "platform": "twitch", "text": "hello\nthere"}),
            )
            .unwrap();
            assert_eq!(sent["sent"], json!(["twitch"]));
            let out = engine.dev_take().unwrap_or_default();
            assert!(
                out.iter().any(|message| message.to_string().contains("hello there")),
                "{out:?}"
            );
            let all = request_sync(&engine, json!({"op": "send", "text": "to everyone"})).unwrap();
            assert_eq!(all["sent"], json!(["twitch"]));
            assert!(request_sync(&engine, json!({"op": "nothing"})).is_err());
        });
    }

    #[test]
    fn moderation_is_limited_to_what_our_role_allows_and_bans_ask_first() {
        let _lock = crate::TEST_LOCK.lock().unwrap_or_else(|error| error.into_inner());
        crate::engine::tests::run(async {
            let engine = crate::engine::tests::engine(json!({"actions": []}));
            engine.dev_mode();
            engine.change(|config| config.window.chat.save_history = true);
            engine
                .inject_chat("twitch", "ana", "ana", "viewer", "rude", &json!({}))
                .unwrap();
            let seq = read(&engine, &json!({})).unwrap()["chat"]["lines"]
                .as_array()
                .unwrap()
                .iter()
                .find(|line| line["user"] == "ana")
                .unwrap()["seq"]
                .clone();
            // Not logged in: nothing but copy and nothing moderated.
            crate::roles::clear("twitch");
            let denied = request_sync(&engine, json!({"op": "act", "seq": seq, "act": "delete"}));
            assert!(denied.is_err());
            crate::roles::set("twitch", crate::roles::Role::Broadcaster, "owner");
            let page = read(&engine, &json!({})).unwrap();
            let acts = page["chat"]["lines"]
                .as_array()
                .unwrap()
                .iter()
                .find(|line| line["user"] == "ana")
                .unwrap()["acts"]
                .clone();
            assert!(acts.as_array().unwrap().iter().any(|act| act == "timeout"), "{acts}");
            assert!(request_sync(&engine, json!({"op": "act", "seq": seq, "act": "timeout", "secs": 0})).is_err());
            assert!(request_sync(&engine, json!({"op": "act", "seq": seq, "act": "nonsense"})).is_err());
            assert!(request_sync(&engine, json!({"op": "act", "seq": 999_999, "act": "ban"})).is_err());
            // A ban waits for the answer to its prompt.
            let asked = request_sync(&engine, json!({"op": "act", "seq": seq, "act": "ban"})).unwrap();
            assert_eq!(asked["asked"], true);
            assert!(!crate::core::prompts::list().is_empty());
            crate::roles::clear("twitch");
        });
    }

    #[test]
    fn the_sender_follows_the_pick_and_the_modes_name_their_platform() {
        let _lock = crate::TEST_LOCK.lock().unwrap_or_else(|error| error.into_inner());
        crate::engine::tests::run(async {
            let engine = crate::engine::tests::engine(json!({"actions": []}));
            engine.dev_mode();
            engine.change(|config| {
                config.window.chat.save_history = true;
                config.twitch.broadcaster = Some(crate::twitch::TwitchAccount {
                    display_name: "Owner".into(),
                    ..Default::default()
                });
                config.twitch.bot = Some(crate::twitch::TwitchAccount {
                    display_name: "Helper".into(),
                    ..Default::default()
                });
            });
            let twitch = |engine: &Rc<Engine>| read(engine, &json!({})).unwrap()["chat"]["platforms"][0].clone();
            // With both logged in, the chat sends as the bot until the broadcaster is picked.
            let platform = twitch(&engine);
            assert_eq!(
                (platform["sender"]["name"].as_str(), platform["sender"]["role"].as_str()),
                (Some("Helper"), Some("bot"))
            );
            assert!(platform["sender"]["tip"].as_str().unwrap().contains("Helper"));
            assert_eq!(platform["senders"][1]["current"], true);
            request_sync(
                &engine,
                json!({"op": "sender", "platform": "twitch", "broadcaster": true}),
            )
            .unwrap();
            let platform = twitch(&engine);
            assert_eq!(platform["sender"]["name"], "Owner");
            assert_eq!(platform["senders"][0]["current"], true);
            // Without the broadcaster, the bot is all there is, whatever was picked.
            engine.change(|config| config.twitch.broadcaster = None);
            assert_eq!(twitch(&engine)["sender"]["name"], "Helper");
            // Nobody logged in: no name, and the tip says so.
            engine.change(|config| config.twitch.bot = None);
            let platform = twitch(&engine);
            assert!(platform["sender"]["name"].is_null());
            assert!(platform["senders"]
                .as_array()
                .unwrap()
                .iter()
                .all(|choice| choice["name"].is_null()));
            // Modes show for a connected platform that has some.
            engine.set_chat_modes(
                "twitch",
                Some(crate::engine::ChatModes {
                    emote_only: true,
                    slow_seconds: Some(5),
                }),
            );
            engine.set_chat_modes(
                "kick",
                Some(crate::engine::ChatModes {
                    emote_only: false,
                    slow_seconds: None,
                }),
            );
            assert!(read(&engine, &json!({})).unwrap()["chat"]["modes"]
                .as_array()
                .unwrap()
                .is_empty());
            engine.simulate_chat_connection("twitch", Some(true));
            let modes = read(&engine, &json!({})).unwrap()["chat"]["modes"].clone();
            assert_eq!(modes.as_array().unwrap().len(), 1, "{modes}");
            assert_eq!(modes[0], "Twitch: Emote-only mode, Slow mode (5s)");
            request_sync(&engine, json!({"op": "show", "platform": "twitch", "shown": false})).unwrap();
            assert!(read(&engine, &json!({})).unwrap()["chat"]["modes"]
                .as_array()
                .unwrap()
                .is_empty());
        });
    }

    #[test]
    fn the_emote_picker_lists_tabs_searches_every_tab_and_keeps_favorites() {
        let _lock = crate::TEST_LOCK.lock().unwrap_or_else(|error| error.into_inner());
        crate::engine::tests::run(async {
            let engine = crate::engine::tests::engine(json!({"actions": []}));
            engine.dev_mode();
            engine.change(|config| config.window.chat.save_history = false);
            // The picker is off with the chat history.
            assert!(request_sync(&engine, json!({"op": "emotes", "platform": "twitch"})).is_err());
            engine.change(|config| config.window.chat.save_history = true);
            assert!(request_sync(&engine, json!({"op": "emotes", "platform": "nowhere"})).is_err());
            let page = request_sync(
                &engine,
                json!({"op": "emotes", "platform": "tiktok", "provider": "emoji"}),
            )
            .unwrap();
            let picker = &page["emotes"];
            let tabs: Vec<&str> = picker["tabs"]
                .as_array()
                .unwrap()
                .iter()
                .filter_map(|tab| tab["id"].as_str())
                .collect();
            assert_eq!(tabs, ["favorites", "emoji"]);
            assert_eq!(picker["active"], "emoji");
            let first = &picker["sections"][0]["emotes"][0];
            assert!(first[1].as_str().unwrap().starts_with("https://"), "{first}");
            assert!(picker["sections"][0]["emotes"].as_array().unwrap().len() <= 500);
            // An unknown tab falls back to the platform's own, else the first.
            let page = request_sync(
                &engine,
                json!({"op": "emotes", "platform": "twitch", "provider": "nope"}),
            )
            .unwrap();
            assert_eq!(page["emotes"]["active"], "platform");
            // A search finds by name across the tabs (an emoji's name is the emoji), once per name.
            let page = request_sync(
                &engine,
                json!({"op": "emotes", "platform": "tiktok", "query": " \u{1F600} "}),
            )
            .unwrap();
            let sections = page["emotes"]["sections"].as_array().unwrap();
            assert!(!sections.is_empty());
            assert!(sections
                .iter()
                .flat_map(|section| section["emotes"].as_array().unwrap())
                .all(|emote| emote[0].as_str().unwrap().contains('\u{1F600}')));
            // Favorites toggle by name and come back in the favorites tab.
            assert!(request_sync(&engine, json!({"op": "favorite"})).is_err());
            request_sync(
                &engine,
                json!({"op": "favorite", "name": "Kappa", "url": "https://x/k.png"}),
            )
            .unwrap();
            let page = request_sync(
                &engine,
                json!({"op": "emotes", "platform": "twitch", "provider": "favorites"}),
            )
            .unwrap();
            assert_eq!(page["emotes"]["favorites"][0], "Kappa");
            assert_eq!(page["emotes"]["sections"][0]["emotes"][0][0], "Kappa");
            request_sync(&engine, json!({"op": "favorite", "name": "Kappa"})).unwrap();
            assert!(engine.config().window.favorite_emotes.is_empty());
        });
    }

    #[test]
    fn a_community_person_gets_the_same_card_and_moderation_without_a_chat_line() {
        let _lock = crate::TEST_LOCK.lock().unwrap_or_else(|error| error.into_inner());
        crate::engine::tests::run(async {
            let engine = crate::engine::tests::engine(json!({"actions": []}));
            engine.dev_mode();
            engine.change(|config| config.window.chat.save_history = true);
            engine
                .inject_chat("twitch", "Ana", "ana", "viewer", "hello", &json!({}))
                .unwrap();
            assert!(request_sync(&engine, json!({"op": "card"})).is_err());
            assert!(request_sync(&engine, json!({"op": "card", "platform": "twitch"})).is_err());
            assert!(request_sync(&engine, json!({"op": "card", "platform": "twitch", "userId": "nobody"})).is_err());
            let card = request_sync(&engine, json!({"op": "card", "platform": "twitch", "userId": "ana"})).unwrap();
            assert_eq!(card["card"]["userId"], "ana");
            assert_eq!(card["card"]["platform"], "twitch");
            // The card carries what a frontend needs to draw it as the platforms do: an id, groups, and what
            // they said as lines (words and badges), not bare text.
            assert_eq!(card["card"]["id"], "twitch|ana");
            assert!(card["card"]["groups"].is_array());
            assert!(card["card"]["details"]
                .as_array()
                .unwrap()
                .iter()
                .any(|row| row["key"] == "userId"));
            assert_eq!(card["card"]["messages"][0]["parts"][0]["t"], "hello");
            // Asking ahead only starts the platform's lookup.
            let ahead = request_sync(
                &engine,
                json!({"op": "card", "platform": "twitch", "userId": "ana", "prefetch": true}),
            )
            .unwrap();
            assert_eq!(ahead["accepted"], true);
            // A message of theirs is never offered from their card.
            crate::roles::set("twitch", crate::roles::Role::Broadcaster, "owner");
            let card = request_sync(&engine, json!({"op": "card", "platform": "twitch", "userId": "ana"})).unwrap();
            let acts: Vec<&str> = card["card"]["acts"]
                .as_array()
                .unwrap()
                .iter()
                .filter_map(Value::as_str)
                .collect();
            assert!(
                acts.contains(&"timeout") && acts.contains(&"ban") && !acts.contains(&"delete"),
                "{acts:?}"
            );
            let asked = request_sync(
                &engine,
                json!({"op": "act", "platform": "twitch", "userId": "ana", "act": "ban"}),
            )
            .unwrap();
            assert_eq!(asked["asked"], true);
            assert!(request_sync(
                &engine,
                json!({"op": "act", "platform": "twitch", "userId": "ana", "act": "delete"})
            )
            .is_err());
            crate::roles::clear("twitch");
        });
    }

    #[test]
    fn the_options_dialog_changes_the_limits_within_range_and_the_shown_chats() {
        let _lock = crate::TEST_LOCK.lock().unwrap_or_else(|error| error.into_inner());
        crate::engine::tests::run(async {
            let engine = crate::engine::tests::engine(json!({"actions": []}));
            engine.change(|config| config.window.chat.save_history = true);
            let sections = options(&engine);
            assert_eq!(sections.len(), 2);
            set_option(&engine, "per_platform", &json!("250")).unwrap();
            assert_eq!(engine.config().window.chat.per_platform, 250);
            set_option(&engine, "total", &json!(99_999)).unwrap();
            assert_eq!(engine.config().window.chat.total, crate::ChatLimits::MOST);
            set_option(&engine, "lifetime", &json!(60)).unwrap();
            set_option(&engine, "fade_out", &json!(0)).unwrap();
            assert_eq!(engine.config().window.chat.fade_out, 0);
            assert!(set_option(&engine, "per_platform", &json!("many")).is_err());
            set_option(&engine, "chat_time", &json!(false)).unwrap();
            assert!(engine.config().window.chat_no_time);
            set_option(&engine, "show_kick", &json!(false)).unwrap();
            assert_eq!(engine.config().window.chat_hidden, vec!["kick".to_owned()]);
            set_option(&engine, "show_kick", &json!(true)).unwrap();
            assert!(engine.config().window.chat_hidden.is_empty());
            assert!(set_option(&engine, "show_nowhere", &json!(true)).is_err());
            set_option(&engine, "keep_history", &json!(false)).unwrap();
            assert!(!engine.config().window.chat.save_history);
            assert_eq!(options(&engine)[1]["rows"].as_array().unwrap().len(), 6);
        });
    }
}
