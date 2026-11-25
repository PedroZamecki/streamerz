//! TikTok Live's own web protocol, read without logging in: an anonymous `ttwid` cookie, the
//! channel's room ID, then a WebSocket of protobuf frames. Ported from piratetok-live-rs (0BSD,
//! https://github.com/PirateTok/live-rs): only the frames and fields Streamerz uses, on the HTTP and
//! WebSocket code already here instead of its own HTTP and protobuf stacks.

use serde_json::{json, Value};
use std::collections::HashSet;
use std::io::Read;
use std::sync::{Mutex, OnceLock};

pub const UA: &str = "Mozilla/5.0 (X11; Linux x86_64; rv:140.0) Gecko/20100101 Firefox/140.0";

/// Accounts confirmed to exist during this session so transient 503/WAF responses on TikTok's embed
/// endpoint never misreport a valid account as "not found".
fn existed_cache() -> &'static Mutex<HashSet<String>> {
    static EXISTED: OnceLock<Mutex<HashSet<String>>> = OnceLock::new();
    EXISTED.get_or_init(|| Mutex::new(HashSet::new()))
}

pub fn remember_existing_user(user: &str) {
    let key = user.trim().trim_start_matches('@').to_lowercase();
    if !key.is_empty() {
        if let Ok(mut set) = existed_cache().lock() {
            set.insert(key);
        }
    }
}

fn has_existed(user: &str) -> bool {
    let key = user.trim().trim_start_matches('@').to_lowercase();
    existed_cache().lock().map(|s| s.contains(&key)).unwrap_or(false)
}

/// One protobuf field value: varints and fixed numbers as `I`, length-delimited as `B`.
#[derive(Clone, Copy)]
pub enum V<'a> {
    I(u64),
    B(&'a [u8]),
}

fn varint(b: &mut &[u8]) -> Option<u64> {
    let mut v = 0u64;
    for shift in (0..64).step_by(7) {
        let (&x, rest) = b.split_first()?;
        *b = rest;
        if shift == 63 && x > 1 {
            return None;
        }
        v |= u64::from(x & 0x7f) << shift;
        if x < 0x80 {
            return Some(v);
        }
    }
    None
}

/// A protobuf message, read lazily: a lookup scans its fields (they're small).
#[derive(Clone, Copy, Default)]
pub struct Msg<'a>(pub &'a [u8]);

impl<'a> Msg<'a> {
    pub fn fields(self) -> impl Iterator<Item = (u32, V<'a>)> {
        let mut b = self.0;
        std::iter::from_fn(move || {
            let key = varint(&mut b)?;
            let tag = u32::try_from(key).ok()?;
            let field = tag >> 3;
            if field == 0 {
                return None;
            }
            let v = match tag & 7 {
                0 => V::I(varint(&mut b)?),
                1 => {
                    let val = u64::from_le_bytes(b.get(..8)?.try_into().ok()?);
                    b = &b[8..];
                    V::I(val)
                }
                5 => {
                    let val = u64::from(u32::from_le_bytes(b.get(..4)?.try_into().ok()?));
                    b = &b[4..];
                    V::I(val)
                }
                2 => {
                    let n = usize::try_from(varint(&mut b)?).ok()?;
                    let slice = b.get(..n)?;
                    b = &b[n..];
                    V::B(slice)
                }
                _ => return None, // groups: unused by TikTok
            };
            Some((field, v))
        })
    }
    pub fn opt_int(self, n: u32) -> Option<u64> {
        self.fields().find_map(|(k, v)| match v {
            V::I(i) if k == n => Some(i),
            _ => None,
        })
    }
    pub fn int(self, n: u32) -> u64 {
        self.opt_int(n).unwrap_or(0)
    }

    /// Repeated integers may arrive packed or as individual varints.
    fn ints(self, n: u32) -> Vec<String> {
        let mut out = Vec::new();
        for (_, v) in self.fields().filter(|(k, _)| *k == n) {
            match v {
                V::I(i) => out.push(i.to_string()),
                V::B(mut b) => {
                    while let Some(i) = varint(&mut b) {
                        out.push(i.to_string());
                    }
                }
            }
        }
        out
    }
    pub fn all(self, n: u32) -> impl Iterator<Item = &'a [u8]> {
        self.fields().filter_map(move |(k, v)| match v {
            V::B(b) if k == n => Some(b),
            _ => None,
        })
    }
    pub fn bytes(self, n: u32) -> &'a [u8] {
        self.all(n).next().unwrap_or_default()
    }
    pub fn str(self, n: u32) -> String {
        String::from_utf8_lossy(self.bytes(n)).into_owned()
    }
    pub fn msg(self, n: u32) -> Msg<'a> {
        Msg(self.bytes(n))
    }
}

fn put_varint(out: &mut Vec<u8>, mut v: u64) {
    while v >= 0x80 {
        out.push(v as u8 | 0x80);
        v >>= 7;
    }
    out.push(v as u8);
}
fn put_int(out: &mut Vec<u8>, n: u32, v: u64) {
    put_varint(out, u64::from(n) << 3);
    put_varint(out, v);
}
fn put_bytes(out: &mut Vec<u8>, n: u32, b: &[u8]) {
    put_varint(out, u64::from(n) << 3 | 2);
    put_varint(out, b.len() as u64);
    out.extend_from_slice(b);
}

/// A WebcastPushFrame carrying `payload`.
pub fn frame(log_id: u64, kind: &str, payload: &[u8]) -> Vec<u8> {
    let mut f = Vec::with_capacity(payload.len() + 16);
    if log_id != 0 {
        put_int(&mut f, 2, log_id);
    }
    put_bytes(&mut f, 6, b"pb");
    put_bytes(&mut f, 7, kind.as_bytes());
    put_bytes(&mut f, 8, payload);
    f
}

pub fn heartbeat(room: u64) -> Vec<u8> {
    let mut p = Vec::new();
    put_int(&mut p, 1, room);
    frame(0, "hb", &p)
}

pub fn enter_room(room: u64) -> Vec<u8> {
    let mut p = Vec::new();
    put_int(&mut p, 1, room);
    put_int(&mut p, 4, 12);
    put_bytes(&mut p, 5, b"audience");
    put_bytes(&mut p, 9, b"0");
    frame(0, "im_enter_room", &p)
}

/// Upper bound for one decompressed webcast push frame payload.
const MAX_DECOMPRESSED: usize = 16 << 20;

/// One binary frame from TikTok: its events, and the ack to send back when it asks for one.
pub fn read(data: &[u8]) -> (Vec<(&'static str, Value)>, Option<Vec<u8>>) {
    let f = Msg(data);
    if f.str(7) != "msg" {
        return (Vec::new(), None);
    }
    let raw = f.bytes(8);
    let mut unzipped = Vec::new();
    let payload = if raw.starts_with(&[0x1f, 0x8b]) {
        let limit = (MAX_DECOMPRESSED + 1) as u64;
        if flate2::read::GzDecoder::new(raw)
            .take(limit)
            .read_to_end(&mut unzipped)
            .is_err()
            || unzipped.len() > MAX_DECOMPRESSED
        {
            return (Vec::new(), None);
        }
        &unzipped[..]
    } else {
        raw
    };
    let mut need_ack = false;
    let mut internal_ext: &[u8] = &[];
    let mut events = Vec::new();
    for (k, v) in Msg(payload).fields() {
        match (k, v) {
            (1, V::B(m)) => {
                let m = Msg(m);
                if let Some(ev) = event(&m.str(1), m.msg(2)) {
                    events.push(ev);
                }
            }
            (5, V::B(ext)) if internal_ext.is_empty() => internal_ext = ext,
            (9, V::I(flag)) if flag != 0 => need_ack = true,
            _ => {}
        }
    }
    let ack = (need_ack && !internal_ext.is_empty()).then(|| frame(f.int(2), "ack", internal_ext));
    (events, ack)
}

fn with(mut a: Value, b: Value) -> Value {
    if let (Value::Object(a), Value::Object(b)) = (&mut a, b) {
        a.extend(b);
    }
    a
}

/// Extract badge image URLs directly from TikTok's `User` protobuf (`badge_image_list` 21 and `badge_list` 64).
fn user_badge_urls(u: Msg) -> Vec<String> {
    let mut urls = Vec::new();
    let mut push = |url: String| {
        if url.starts_with("https://") && !urls.contains(&url) {
            urls.push(url);
        }
    };
    for img in u.all(21) {
        push(Msg(img).str(1));
    }
    for b in u.all(64) {
        let b = Msg(b);
        push(b.msg(20).msg(2).str(1));
        push(b.msg(23).msg(2).str(1));
    }
    urls
}

/// TikTok-Live-Connector's flat user fields (what TikFinity and tikstream send).
fn user(u: Msg, common: Msg) -> Value {
    json!({
        "userId": u.int(1).to_string(),
        "uniqueId": u.str(38),
        "nickname": u.str(3),
        "profilePictureUrl": u.msg(9).str(1),
        "badgeUrls": user_badge_urls(u),
        "msgId": common.int(2).to_string(),
        "createTime": common.int(4),
    })
}

/// The sender's standing with the streamer. followRole: 0 none, 1 follower, 2 friend.
fn role(i: Msg) -> Value {
    json!({
        "isModerator": i.int(5) != 0,
        "isSubscriber": i.int(2) != 0,
        "followRole": if i.int(3) != 0 { 2 } else { u8::from(i.int(4) != 0) },
    })
}

/// Inline custom emotes on a `WebcastChatMessage` (field 13: repeated `EmoteWithIndex`).
fn chat_comment_and_emotes(raw_comment: String, m: Msg) -> (String, Vec<Value>) {
    let mut indexed: Vec<(usize, String, String)> = m
        .all(13)
        .filter_map(|raw| {
            let ew = Msg(raw);
            let idx = usize::try_from(ew.int(1)).unwrap_or(usize::MAX);
            let em = ew.msg(2);
            let id = em.str(1);
            let url = em.msg(2).str(1);
            if id.is_empty() || !url.starts_with("https://") {
                return None;
            }
            Some((idx, format!("[tiktok:{id}]"), url))
        })
        .collect();
    if indexed.is_empty() {
        return (raw_comment, Vec::new());
    }
    let emotes_json: Vec<Value> = indexed
        .iter()
        .map(|(_, tok, url)| json!({ "token": tok, "url": url }))
        .collect();
    if raw_comment.contains('\u{FFFC}') {
        let mut out = String::with_capacity(raw_comment.len() + indexed.len() * 16);
        let mut it = indexed.iter();
        for ch in raw_comment.chars() {
            if ch == '\u{FFFC}' {
                if let Some((_, tok, _)) = it.next() {
                    if !out.is_empty() && !out.ends_with(char::is_whitespace) {
                        out.push(' ');
                    }
                    out.push_str(tok);
                    out.push(' ');
                    continue;
                }
            }
            out.push(ch);
        }
        for (_, tok, _) in it {
            if !out.is_empty() && !out.ends_with(char::is_whitespace) {
                out.push(' ');
            }
            out.push_str(tok);
        }
        return (out.trim().to_string(), emotes_json);
    }
    indexed.sort_by_key(|a| a.0);
    let mut out = String::with_capacity(raw_comment.len() + indexed.len() * 16);
    let mut emotes = indexed.iter().peekable();
    let mut after_emote = false;
    for (pos, ch) in raw_comment.chars().enumerate() {
        while let Some((_, tok, _)) = emotes.next_if(|(idx, _, _)| *idx <= pos) {
            if !out.is_empty() && !out.ends_with(char::is_whitespace) {
                out.push(' ');
            }
            out.push_str(tok);
            after_emote = true;
        }
        if after_emote && !ch.is_whitespace() && !out.ends_with(char::is_whitespace) {
            out.push(' ');
        }
        after_emote = false;
        out.push(ch);
    }
    for (_, tok, _) in emotes {
        if !out.is_empty() && !out.ends_with(char::is_whitespace) {
            out.push(' ');
        }
        out.push_str(tok);
    }
    (out.trim().to_string(), emotes_json)
}

/// A Webcast message as Streamerz's TikTok event name and data.
pub fn event(kind: &str, m: Msg) -> Option<(&'static str, Value)> {
    let c = m.msg(1);
    Some(match kind {
        "WebcastChatMessage" => {
            let (comment, emotes) = chat_comment_and_emotes(m.str(3), m);
            (
                "chat",
                with(
                    with(user(m.msg(2), c), role(m.msg(18))),
                    json!({ "comment": comment, "emotes": emotes }),
                ),
            )
        }
        "WebcastEmoteChatMessage" => {
            let mut toks = Vec::new();
            let mut emotes = Vec::new();
            for raw in m.all(3) {
                let em = Msg(raw);
                let id = em.str(1);
                let url = em.msg(2).str(1);
                if !id.is_empty() && url.starts_with("https://") {
                    let tok = format!("[tiktok:{id}]");
                    toks.push(tok.clone());
                    emotes.push(json!({ "token": tok, "url": url }));
                }
            }
            if toks.is_empty() {
                return None;
            }
            (
                "chat",
                with(
                    with(user(m.msg(2), c), role(m.msg(5))),
                    json!({ "comment": toks.join(" "), "emotes": emotes }),
                ),
            )
        }
        "WebcastLikeMessage" => (
            "like",
            with(
                user(m.msg(5), c),
                json!({ "likeCount": m.int(2), "totalLikeCount": m.int(3) }),
            ),
        ),
        "WebcastSocialMessage" => match m.int(4) {
            1 => ("follow", user(m.msg(2), c)),
            2..=5 => ("share", user(m.msg(2), c)),
            _ => return None,
        },
        "WebcastMemberMessage" if m.int(10) == 1 => ("member", user(m.msg(2), c)),
        "WebcastSubNotifyMessage" => ("subscribe", with(user(m.msg(11), c), json!({ "subMonth": m.int(4) }))),
        "WebcastImDeleteMessage" => (
            "messageDelete",
            json!({"deleteMsgIds": m.ints(2), "deleteUserIds": m.ints(3)}),
        ),
        "WebcastRoomPinMessage" => {
            let chat = m.msg(2);
            let (_, data) = event("WebcastChatMessage", chat)?;
            (
                "roomPin",
                json!({"action": m.int(33), "chatMessage": data,
                "pinTime": m.int(31), "displayDuration": m.int(34), "pinMsgId": m.int(35).to_string(),
                "operator": user(m.msg(32), c)}),
            )
        }
        // ranksList (2): the top viewers by coins sent, each TopUser {coinCount 1, user 2}.
        "WebcastRoomUserSeqMessage" => {
            let top: Vec<Value> = m
                .all(2)
                .map(|t| {
                    let t = Msg(t);
                    with(user(t.msg(2), c), json!({ "coinCount": t.int(1) }))
                })
                .collect();
            ("roomUser", json!({ "viewerCount": m.int(3), "topViewers": top }))
        }
        "WebcastGiftMessage" => {
            let g = m.msg(15);
            (
                "gift",
                with(
                    with(user(m.msg(7), c), role(m.msg(32))),
                    json!({
                        "giftId": m.int(2),
                        "giftName": g.str(16),
                        "giftType": g.opt_int(11),
                        "giftPictureUrl": g.msg(1).str(1),
                        "diamondCount": g.opt_int(12),
                        "repeatCount": m.int(5),
                        "repeatEnd": m.int(9) == 1,
                    }),
                ),
            )
        }
        "WebcastControlMessage" if m.int(2) == 3 => ("streamEnd", json!({})),
        "WebcastControlMessage" if m.int(2) == 1 => ("streamPaused", json!({})),
        "WebcastControlMessage" if m.int(2) == 2 => ("streamResumed", json!({})),
        _ => return None,
    })
}

/// The anonymous device cookie TikTok's WebSocket wants (the homepage stopped setting it; /explore does).
pub async fn ttwid() -> Result<String, String> {
    let r = crate::http::request(
        "GET",
        "https://www.tiktok.com/explore",
        &[("User-Agent".into(), UA.into())],
        b"",
    )
    .await?;
    r.headers
        .iter()
        .filter(|(k, _)| k.eq_ignore_ascii_case("set-cookie"))
        .find_map(|(_, v)| v.strip_prefix("ttwid=")?.split(';').next().filter(|t| !t.is_empty()))
        .map(str::to_string)
        .ok_or_else(|| "TikTok sent no ttwid cookie".into())
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum EmbedProbe {
    Exists,
    NotFound,
    Transient,
}

/// The `"key":"value"` string in a page, JSON escapes decoded.
fn json_string(page: &str, key: &str) -> Option<String> {
    let rest = page.split(&format!("\"{key}\":\"")).nth(1)?;
    let mut end = 0;
    let mut escaped = false;
    for (i, c) in rest.char_indices() {
        if escaped {
            escaped = false;
        } else if c == '\\' {
            escaped = true;
        } else if c == '"' {
            end = i;
            break;
        }
    }
    serde_json::from_str::<String>(&format!("\"{}\"", &rest[..end]))
        .ok()
        .filter(|n| !n.trim().is_empty())
}

async fn probe_embed(user: &str) -> (EmbedProbe, Option<String>, Option<String>) {
    let url = format!("https://www.tiktok.com/embed/@{}", crate::commands::url_encode(user));
    let mut saw_not_found = false;
    for attempt in 0..3 {
        match crate::http::request("GET", &url, &[("User-Agent".into(), UA.into())], b"").await {
            Ok(r) if r.status == 200 => {
                let page = r.text();
                if let Some(avatar) = json_string(&page, "avatarThumbUrl").filter(|a| a.starts_with("https://")) {
                    remember_existing_user(user);
                    return (EmbedProbe::Exists, Some(avatar), json_string(&page, "nickname"));
                }
                if page.contains("\"uniqueId\":\"") {
                    remember_existing_user(user);
                    return (EmbedProbe::Exists, None, json_string(&page, "nickname"));
                }
                if page.contains("Couldn't find this account") || page.contains("user-not-found") {
                    saw_not_found = true;
                    break;
                }
                // Likely a bot-check / challenge HTML page rather than a missing user.
                return (EmbedProbe::Transient, None, None);
            }
            Ok(r) if r.status == 404 => {
                saw_not_found = true;
                break;
            }
            _ if attempt < 2 => tokio::time::sleep(std::time::Duration::from_secs(1)).await,
            _ => {}
        }
    }
    if saw_not_found && !has_existed(user) {
        (EmbedProbe::NotFound, None, None)
    } else if has_existed(user) {
        (EmbedProbe::Exists, None, None)
    } else {
        (EmbedProbe::Transient, None, None)
    }
}

/// Structured reason why a TikTok channel currently has no active live room.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RoomLookupError {
    /// Account exists (logged-in, cached, or verified via embed) and has never started a live stream yet.
    NeverLive,
    /// Account exists (or embed probe was rate-limited/challenged) and is currently offline.
    Offline,
    /// Embed explicitly confirmed the `@handle` does not exist on TikTok.
    NotFound,
    /// Network, HTTP 429, or unexpected upstream error.
    Transient(String),
}

pub(crate) fn user_room_url(channel: &str) -> String {
    format!(
        "https://www.tiktok.com/api-live/user/room?aid=1988&app_name=tiktok_web&device_platform=web_pc\
         &app_language=en&browser_language=en-US&region=US&user_is_login=false&uniqueId={}\
         &sourceType=54&staleTime=600000",
        crate::commands::url_encode(channel)
    )
}

async fn fetch_user_room_raw(channel: &str, ttwid: &str) -> Result<Value, String> {
    let url = user_room_url(channel);
    let h = [
        ("User-Agent".into(), UA.into()),
        ("Referer".into(), "https://www.tiktok.com/".into()),
        ("Cookie".into(), format!("ttwid={ttwid}")),
    ];
    let r = crate::http::request("GET", &url, &h, b"").await?;
    if r.status == 429 {
        return Err("TikTok rate limit reached; backing off".into());
    }
    serde_json::from_slice(&r.body).map_err(|_| format!("TikTok answered HTTP {}", r.status))
}

/// The channel's live room ID, distinguishing normal offline/standby states from missing handles or
/// transient errors. When `known_account` is true (e.g. a logged-in TikTok account), never hits the
/// public embed endpoint on `19881007`.
pub async fn room_id(channel: &str, ttwid: &str, known_account: bool) -> Result<u64, RoomLookupError> {
    if known_account {
        remember_existing_user(channel);
    }
    let v = fetch_user_room_raw(channel, ttwid)
        .await
        .map_err(RoomLookupError::Transient)?;
    room_from_response(channel, &v, known_account).await
}

pub(crate) async fn room_from_response(channel: &str, v: &Value, known_account: bool) -> Result<u64, RoomLookupError> {
    match v["statusCode"].as_i64() {
        Some(0) => remember_existing_user(channel),
        // TikTok's Live service returns 19881007 (LIVE_USER_NOT_EXIST) for valid accounts that have
        // never started a livestream.
        Some(19881007) => {
            if known_account || has_existed(channel) {
                return Err(RoomLookupError::NeverLive);
            }
            return Err(match probe_embed(channel).await.0 {
                EmbedProbe::Exists => RoomLookupError::NeverLive,
                EmbedProbe::Transient => RoomLookupError::Offline,
                EmbedProbe::NotFound => RoomLookupError::NotFound,
            });
        }
        c => {
            return Err(RoomLookupError::Transient(format!(
                "TikTok upstream code {}",
                c.unwrap_or(-1)
            )))
        }
    }
    let room = v["data"]["user"]["roomId"]
        .as_str()
        .and_then(|r| r.parse().ok())
        .or_else(|| v["data"]["user"]["roomId"].as_u64())
        .unwrap_or(0);
    let status = v["data"]["liveRoom"]["status"]
        .as_i64()
        .or(v["data"]["user"]["status"].as_i64());
    if room == 0 || status != Some(2) {
        return Err(RoomLookupError::Offline);
    }
    let live = &v["data"]["liveRoom"];
    ROOM.with_borrow_mut(|facts| {
        *facts = RoomFacts {
            title: live["title"].as_str().unwrap_or_default().trim().to_owned(),
            since: live["startTime"].as_i64().filter(|at| *at > 0),
        }
    });
    Ok(room)
}

/// What TikTok said about the live the chat last connected to.
#[derive(Default, Clone, Debug, PartialEq)]
pub struct RoomFacts {
    pub title: String,
    /// When it started (Unix seconds).
    pub since: Option<i64>,
}

thread_local! {
    static ROOM: std::cell::RefCell<RoomFacts> = std::cell::RefCell::default();
}

/// The live's title and start, as the room lookup said when the chat connected to it.
pub fn room_facts() -> RoomFacts {
    ROOM.with_borrow(Clone::clone)
}

/// A title set from here: what the live is called now.
pub fn room_titled(title: &str) {
    ROOM.with_borrow_mut(|facts| facts.title = title.to_owned());
}

pub fn ws_url(room: u64) -> String {
    let rtt = 100
        + std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.subsec_millis() % 100);
    format!(
        "wss://webcast-ws.tiktok.com/webcast/im/ws_proxy/ws_reuse_supplement/?version_code=180800\
         &device_platform=web&cookie_enabled=true&screen_width=1920&screen_height=1080\
         &browser_language=en-US&browser_platform=Linux%20x86_64&browser_name=Mozilla\
         &browser_version=5.0%20(X11)&browser_online=true&tz_name=UTC&app_name=tiktok_web\
         &sup_ws_ds_opt=1&update_version_code=2.0.0&compress=gzip&webcast_language=en&ws_direct=1\
         &aid=1988&live_id=12&app_language=en&client_enter=1&room_id={room}&identity=audience\
         &history_comment_count=30&last_rtt={rtt}.000&heartbeat_duration=10000\
         &resp_content_type=protobuf&did_rule=3"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deletion_ids_accept_packed_and_unpacked_large_integers() {
        let mut message = Vec::new();
        let large = 7_565_717_210_162_105_144;
        put_int(&mut message, 2, large);
        let mut packed = Vec::new();
        put_varint(&mut packed, large + 1);
        put_varint(&mut packed, large + 2);
        put_bytes(&mut message, 2, &packed);
        put_int(&mut message, 3, 42);
        let (kind, data) = event("WebcastImDeleteMessage", Msg(&message)).unwrap();
        assert_eq!(kind, "messageDelete");
        assert_eq!(
            data["deleteMsgIds"],
            json!([large.to_string(), (large + 1).to_string(), (large + 2).to_string()])
        );
        assert_eq!(data["deleteUserIds"], json!(["42"]));
    }

    #[test]
    fn room_pins_and_pause_controls_decode() {
        let mut common = Vec::new();
        put_int(&mut common, 2, 9001);
        let mut sender = Vec::new();
        put_bytes(&mut sender, 3, b"Display name");
        put_bytes(&mut sender, 38, b"actual.handle");
        let mut chat = Vec::new();
        put_bytes(&mut chat, 1, &common);
        put_bytes(&mut chat, 2, &sender);
        put_bytes(&mut chat, 3, b":)");
        let mut pin = Vec::new();
        put_bytes(&mut pin, 2, &chat);
        put_bytes(&mut pin, 32, &sender);
        put_int(&mut pin, 33, 1);
        put_int(&mut pin, 35, 9002);
        let (kind, data) = event("WebcastRoomPinMessage", Msg(&pin)).unwrap();
        assert_eq!(kind, "roomPin");
        assert_eq!(data["chatMessage"]["msgId"], "9001");
        assert_eq!(data["chatMessage"]["comment"], ":)");
        assert_eq!(data["operator"]["nickname"], "Display name");
        assert_eq!(data["pinMsgId"], "9002");
        for (action, expected) in [(1, "streamPaused"), (2, "streamResumed"), (3, "streamEnd")] {
            let mut control = Vec::new();
            put_int(&mut control, 2, action);
            assert_eq!(event("WebcastControlMessage", Msg(&control)).unwrap().0, expected);
        }
    }

    #[test]
    fn room_lookup_accepts_numeric_ids_and_checks_live_status() {
        let _g = crate::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        crate::engine::tests::run(async {
            for id in [json!("777"), json!(777)] {
                let mut body = json!({"statusCode": 0, "data": {"user": {"roomId": id}, "liveRoom": {"status": 2}}});
                assert_eq!(room_from_response("fixture", &body, true).await, Ok(777));
                body["data"]["liveRoom"]["status"] = json!(4);
                assert_eq!(
                    room_from_response("fixture", &body, true).await,
                    Err(RoomLookupError::Offline)
                );
            }
        });
    }

    #[test]
    fn json_string_decodes_escapes() {
        let page = r#"{"uniqueId":"x","nickname":"Nan\u00e1 \"uma\"","a":1}"#;
        assert_eq!(json_string(page, "nickname").as_deref(), Some("Naná \"uma\""));
        assert_eq!(json_string(page, "missing"), None);
        assert_eq!(json_string(r#"{"nickname":" "}"#, "nickname"), None);
    }

    fn chat_frame(gzip: bool, ack: bool) -> Vec<u8> {
        let mut badge_img = Vec::new();
        put_bytes(&mut badge_img, 1, b"https://p16-webcast.tiktokcdn.com/badge.png");
        let mut u = Vec::new();
        put_int(&mut u, 1, 42);
        put_bytes(&mut u, 3, "Ana 🎉".as_bytes());
        put_bytes(&mut u, 21, &badge_img);
        put_bytes(&mut u, 38, b"ana");
        let mut who = Vec::new();
        put_int(&mut who, 4, 1);
        put_int(&mut who, 5, 1);
        let mut em_img = Vec::new();
        put_bytes(&mut em_img, 1, b"https://p16-webcast.tiktokcdn.com/emote1.png");
        let mut em = Vec::new();
        put_bytes(&mut em, 1, b"em1");
        put_bytes(&mut em, 2, &em_img);
        let mut ewi = Vec::new();
        put_int(&mut ewi, 1, 7);
        put_bytes(&mut ewi, 2, &em);
        let mut chat = Vec::new();
        put_bytes(&mut chat, 2, &u);
        put_bytes(&mut chat, 3, b"!uptime");
        put_bytes(&mut chat, 13, &ewi);
        put_bytes(&mut chat, 18, &who);
        let mut m = Vec::new();
        put_bytes(&mut m, 1, b"WebcastChatMessage");
        put_bytes(&mut m, 2, &chat);
        let mut r = Vec::new();
        put_bytes(&mut r, 1, &m);
        if ack {
            put_bytes(&mut r, 5, b"ext");
            put_int(&mut r, 9, 1);
        }
        let payload = if gzip {
            let mut z = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::fast());
            std::io::Write::write_all(&mut z, &r).unwrap();
            z.finish().unwrap()
        } else {
            r
        };
        let mut f = frame(7, "msg", &payload);
        put_int(&mut f, 1, u64::MAX); // a 10-byte varint (negative int64) before the rest
        f
    }

    #[test]
    fn decodes_chat_and_acks() {
        for gzip in [false, true] {
            let data = chat_frame(gzip, true);
            let (ev, ack) = read(&data);
            assert_eq!(ev.len(), 1);
            let (name, d) = &ev[0];
            assert_eq!(*name, "chat");
            assert_eq!(d["comment"], "!uptime [tiktok:em1]");
            assert_eq!(d["emotes"][0]["token"], "[tiktok:em1]");
            assert_eq!(d["badgeUrls"][0], "https://p16-webcast.tiktokcdn.com/badge.png");
            assert_eq!(d["uniqueId"], "ana");
            assert_eq!(d["nickname"], "Ana 🎉");
            assert_eq!(d["userId"], "42");
            assert_eq!(d["isModerator"], true);
            assert_eq!(d["followRole"], 1);
            let ack = Msg(ack.as_deref().unwrap());
            assert_eq!(ack.int(2), 7);
            assert_eq!(ack.str(7), "ack");
            assert_eq!(ack.bytes(8), b"ext");
        }
        assert!(read(&chat_frame(false, false)).1.is_none());
        assert!(read(&frame(0, "hb", b"")).0.is_empty());
    }

    #[test]
    fn builds_frames() {
        let hb = heartbeat(7_000_000_000_000_000_001);
        let f = Msg(&hb);
        assert_eq!((f.str(6).as_str(), f.str(7).as_str()), ("pb", "hb"));
        assert_eq!(f.msg(8).int(1), 7_000_000_000_000_000_001);
        let e = enter_room(5);
        let p = Msg(&e).msg(8);
        assert_eq!((p.int(1), p.int(4), p.str(5).as_str()), (5, 12, "audience"));
        assert!(ws_url(5).contains("history_comment_count=30"));
    }

    #[test]
    fn events() {
        let mut s = Vec::new();
        put_int(&mut s, 4, 1);
        assert_eq!(event("WebcastSocialMessage", Msg(&s)).unwrap().0, "follow");
        let mut c = Vec::new();
        put_int(&mut c, 2, 3);
        assert_eq!(event("WebcastControlMessage", Msg(&c)).unwrap().0, "streamEnd");
        assert!(event("WebcastMemberMessage", Msg(&[])).is_none());
        let mut g = Vec::new();
        let mut d = Vec::new();
        put_bytes(&mut d, 16, b"Rose");
        put_int(&mut d, 12, 1);
        put_bytes(&mut g, 15, &d);
        put_int(&mut g, 5, 3);
        put_int(&mut g, 9, 1);
        let (_, v) = event("WebcastGiftMessage", Msg(&g)).unwrap();
        assert_eq!(
            (v["giftName"].as_str(), v["repeatCount"].as_u64()),
            (Some("Rose"), Some(3))
        );
        assert_eq!(v["repeatEnd"], true);
        let (_, sparse) = event("WebcastGiftMessage", Msg(&[])).unwrap();
        assert!(sparse["giftType"].is_null() && sparse["diamondCount"].is_null());
        let mut u = Vec::new();
        put_bytes(&mut u, 38, b"bigfan");
        let mut top = Vec::new();
        put_int(&mut top, 1, 500);
        put_bytes(&mut top, 2, &u);
        let mut seq = Vec::new();
        put_bytes(&mut seq, 2, &top);
        put_int(&mut seq, 3, 77);
        let (_, v) = event("WebcastRoomUserSeqMessage", Msg(&seq)).unwrap();
        assert_eq!(
            (v["viewerCount"].as_u64(), v["topViewers"][0]["uniqueId"].as_str()),
            (Some(77), Some("bigfan"))
        );
        assert_eq!(v["topViewers"][0]["coinCount"], 500);
    }

    /// Frames captured from a real live (`STREAMERZ_TIKTOK_FRAMES=dir`, *.bin, each holding a chat):
    /// TikTok's current wire format still decodes.
    #[test]
    fn real_frames() {
        let Ok(dir) = std::env::var("STREAMERZ_TIKTOK_FRAMES") else {
            return;
        };
        let mut n = 0;
        for f in std::fs::read_dir(dir).unwrap().flatten() {
            let (events, _) = read(&std::fs::read(f.path()).unwrap());
            for (name, d) in events.iter().filter(|e| e.0 == "chat") {
                println!("{name} {} {:?} {:?}", d["uniqueId"], d["nickname"], d["comment"]);
                assert!(d["userId"].as_str().is_some_and(|u| u != "0"));
                n += 1;
            }
        }
        assert!(n > 0, "no chat in the captured frames");
    }

    #[test]
    fn protobuf_varint_rejects_truncation_and_overflow() {
        let mut empty: &[u8] = &[];
        assert_eq!(varint(&mut empty), None);

        let mut truncated: &[u8] = &[0x80];
        assert_eq!(varint(&mut truncated), None);

        // Max valid 64-bit varint: 9 bytes of 0xff followed by 0x01.
        let mut max_ok: &[u8] = &[0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0x01];
        assert_eq!(varint(&mut max_ok), Some(u64::MAX));
        assert!(max_ok.is_empty());

        // 10th byte > 1 overflows 64 bits (bits 64..69 set).
        let mut tenth_overflow: &[u8] = &[0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0x02];
        assert_eq!(varint(&mut tenth_overflow), None);

        let mut tenth_high: &[u8] = &[0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x7f];
        assert_eq!(varint(&mut tenth_high), None);

        // 11-byte varint must be rejected.
        let mut eleven: &[u8] = &[0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x01];
        assert_eq!(varint(&mut eleven), None);
    }

    #[test]
    fn protobuf_msg_rejects_invalid_tags_lengths_and_wire_types() {
        // Field number 0 (tag 0x00) is illegal and must stop iteration cleanly.
        assert_eq!(Msg(&[0x00, 0x01, 0x08, 0x07]).fields().count(), 0);

        // Tag > u32::MAX whose low 32 bits would spoof field 1 (wire type 0).
        let mut spoof = Vec::new();
        put_varint(&mut spoof, (1u64 << 32) | (1u64 << 3));
        put_varint(&mut spoof, 99);
        assert_eq!(Msg(&spoof).int(1), 0);
        assert_eq!(Msg(&spoof).fields().count(), 0);

        // Length-delimited field claiming more bytes than remain in the buffer.
        assert_eq!(Msg(&[0x0a, 0x10, b'a', b'b']).fields().count(), 0);

        // Length-delimited field claiming u64::MAX bytes.
        let mut huge_len = vec![0x0a];
        put_varint(&mut huge_len, u64::MAX);
        assert_eq!(Msg(&huge_len).fields().count(), 0);

        // Truncated fixed64 (wire type 1) and fixed32 (wire type 5).
        assert_eq!(Msg(&[0x09, 1, 2, 3, 4, 5, 6, 7]).fields().count(), 0);
        assert_eq!(Msg(&[0x0d, 1, 2, 3]).fields().count(), 0);

        // Unsupported wire types (3, 4, 6, 7) stop cleanly without panicking.
        for wt in [3u8, 4, 6, 7] {
            let tag = (1u8 << 3) | wt;
            assert_eq!(Msg(&[tag, 0x01]).fields().count(), 0);
        }

        // Valid fixed64, fixed32, varint, and length-delimited fields in sequence.
        let mut mixed = Vec::new();
        mixed.push((1 << 3) | 1);
        mixed.extend_from_slice(&0x0102_0304_0506_0708u64.to_le_bytes());
        mixed.push((2 << 3) | 5);
        mixed.extend_from_slice(&0x1122_3344u32.to_le_bytes());
        put_int(&mut mixed, 3, 42);
        put_bytes(&mut mixed, 4, b"ok");
        let m = Msg(&mixed);
        assert_eq!(m.opt_int(1), Some(0x0102_0304_0506_0708));
        assert_eq!(m.opt_int(2), Some(0x1122_3344));
        assert_eq!(m.opt_int(3), Some(42));
        assert_eq!(m.opt_int(4), None);
        assert_eq!(m.str(4), "ok");
    }

    #[test]
    fn gzip_bomb_and_corrupt_gzip_frames_are_rejected_without_panic() {
        // Corrupt gzip payload.
        let bad = frame(1, "msg", &[0x1f, 0x8b, 0x08, 0x00, 0xff, 0xff]);
        assert!(read(&bad).0.is_empty());

        // Small compressed payload that expands past MAX_DECOMPRESSED.
        let mut enc = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::best());
        let zeros = vec![0u8; 64 << 10];
        let chunks = (MAX_DECOMPRESSED / zeros.len()) + 2;
        for _ in 0..chunks {
            std::io::Write::write_all(&mut enc, &zeros).unwrap();
        }
        let bomb = enc.finish().unwrap();
        assert!(bomb.len() < 64 << 10, "gzip bomb should be compact");
        let framed = frame(1, "msg", &bomb);
        let (events, ack) = read(&framed);
        assert!(events.is_empty());
        assert!(ack.is_none());
    }

    #[test]
    fn gift_message_ignores_wrong_wire_type_for_optional_numbers() {
        // Field 11 (giftType) and 12 (diamondCount) encoded as bytes instead of varint.
        let mut g = Vec::new();
        put_bytes(&mut g, 11, b"bad");
        put_bytes(&mut g, 12, b"bad");
        put_bytes(&mut g, 16, b"Rose");
        let mut msg = Vec::new();
        put_int(&mut msg, 2, 5655);
        put_bytes(&mut msg, 15, &g);
        let (kind, v) = event("WebcastGiftMessage", Msg(&msg)).unwrap();
        assert_eq!(kind, "gift");
        assert!(v["giftType"].is_null());
        assert!(v["diamondCount"].is_null());
        assert_eq!(v["giftName"], "Rose");
    }

    #[test]
    fn chat_emotes_handle_placeholders_same_index_and_bounds() {
        let make_ewi = |idx: u64, id: &str, url: &str| {
            let mut img = Vec::new();
            put_bytes(&mut img, 1, url.as_bytes());
            let mut em = Vec::new();
            put_bytes(&mut em, 1, id.as_bytes());
            put_bytes(&mut em, 2, &img);
            let mut ewi = Vec::new();
            put_int(&mut ewi, 1, idx);
            put_bytes(&mut ewi, 2, &em);
            ewi
        };
        // Two emotes at the same index keep their wire order; non-https emote is dropped.
        let mut chat = Vec::new();
        put_bytes(&mut chat, 3, b"hiworld");
        put_bytes(&mut chat, 13, &make_ewi(2, "e1", "https://cdn.example/1.png"));
        put_bytes(&mut chat, 13, &make_ewi(2, "e2", "https://cdn.example/2.png"));
        put_bytes(&mut chat, 13, &make_ewi(2, "bad", "http://insecure.example/3.png"));
        put_bytes(&mut chat, 13, &make_ewi(u64::MAX, "end", "https://cdn.example/4.png"));
        let (_, d) = event("WebcastChatMessage", Msg(&chat)).unwrap();
        assert_eq!(d["comment"], "hi [tiktok:e1] [tiktok:e2] world [tiktok:end]");
        assert_eq!(d["emotes"].as_array().unwrap().len(), 3);

        // Object replacement character placeholders with extra trailing emote.
        let mut fffc = Vec::new();
        put_bytes(&mut fffc, 3, "a\u{FFFC}b".as_bytes());
        put_bytes(&mut fffc, 13, &make_ewi(0, "x", "https://cdn.example/x.png"));
        put_bytes(&mut fffc, 13, &make_ewi(1, "y", "https://cdn.example/y.png"));
        let (_, d2) = event("WebcastChatMessage", Msg(&fffc)).unwrap();
        assert_eq!(d2["comment"], "a [tiktok:x] b [tiktok:y]");
    }

    #[test]
    fn decodes_emote_chat_like_social_member_and_subscribe_events() {
        let mut img = Vec::new();
        put_bytes(&mut img, 1, b"https://cdn.example/em.png");
        let mut em = Vec::new();
        put_bytes(&mut em, 1, b"sub1");
        put_bytes(&mut em, 2, &img);
        let mut ec = Vec::new();
        put_bytes(&mut ec, 3, &em);
        let (kind, data) = event("WebcastEmoteChatMessage", Msg(&ec)).unwrap();
        assert_eq!(kind, "chat");
        assert_eq!(data["comment"], "[tiktok:sub1]");
        assert!(event("WebcastEmoteChatMessage", Msg(&[])).is_none());

        let mut like = Vec::new();
        put_int(&mut like, 2, 15);
        put_int(&mut like, 3, 900);
        let (kind, data) = event("WebcastLikeMessage", Msg(&like)).unwrap();
        assert_eq!(kind, "like");
        assert_eq!((&data["likeCount"], &data["totalLikeCount"]), (&json!(15), &json!(900)));

        let mut share = Vec::new();
        put_int(&mut share, 4, 3);
        assert_eq!(event("WebcastSocialMessage", Msg(&share)).unwrap().0, "share");
        let mut unknown_social = Vec::new();
        put_int(&mut unknown_social, 4, 99);
        assert!(event("WebcastSocialMessage", Msg(&unknown_social)).is_none());

        let mut join = Vec::new();
        put_int(&mut join, 10, 1);
        assert_eq!(event("WebcastMemberMessage", Msg(&join)).unwrap().0, "member");

        let mut sub = Vec::new();
        put_int(&mut sub, 4, 6);
        let (kind, data) = event("WebcastSubNotifyMessage", Msg(&sub)).unwrap();
        assert_eq!(kind, "subscribe");
        assert_eq!(data["subMonth"], 6);
    }
}
