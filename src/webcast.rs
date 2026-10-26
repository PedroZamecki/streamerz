//! TikTok Live's own web protocol, read without logging in: an anonymous `ttwid` cookie, the
//! channel's room ID, then a WebSocket of protobuf frames. Ported from piratetok-live-rs (0BSD,
//! https://github.com/PirateTok/live-rs): only the frames and fields streamerz uses, on the HTTP and
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
            let v = match key & 7 {
                0 => V::I(varint(&mut b)?),
                1 => V::I(u64::from_le_bytes(b.get(..8)?.try_into().ok()?)),
                5 => V::I(u64::from(u32::from_le_bytes(b.get(..4)?.try_into().ok()?))),
                2 => {
                    let n = usize::try_from(varint(&mut b)?).ok()?;
                    V::B(b.get(..n)?)
                }
                _ => return None, // groups: unused by TikTok
            };
            match v {
                V::B(x) => b = &b[x.len()..],
                V::I(_) if key & 7 == 1 => b = &b[8..],
                V::I(_) if key & 7 == 5 => b = &b[4..],
                V::I(_) => {}
            }
            Some(((key >> 3) as u32, v))
        })
    }
    pub fn int(self, n: u32) -> u64 {
        self.fields()
            .filter(|f| f.0 == n)
            .find_map(|f| match f.1 {
                V::I(i) => Some(i),
                V::B(_) => None,
            })
            .unwrap_or(0)
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

/// One binary frame from TikTok: its events, and the ack to send back when it asks for one.
pub fn read(data: &[u8]) -> (Vec<(&'static str, Value)>, Option<Vec<u8>>) {
    let f = Msg(data);
    if f.str(7) != "msg" {
        return (Vec::new(), None);
    }
    let raw = f.bytes(8);
    let mut unzipped = Vec::new();
    let payload = if raw.starts_with(&[0x1f, 0x8b]) {
        if flate2::read::GzDecoder::new(raw).read_to_end(&mut unzipped).is_err() {
            return (Vec::new(), None);
        }
        &unzipped[..]
    } else {
        raw
    };
    let r = Msg(payload);
    let ack = (r.int(9) != 0 && !r.bytes(5).is_empty()).then(|| frame(f.int(2), "ack", r.bytes(5)));
    let events = r
        .all(1)
        .filter_map(|m| {
            let m = Msg(m);
            event(&m.str(1), m.msg(2))
        })
        .collect();
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
            let idx = ew.int(1) as usize;
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
        let mut out = String::new();
        let mut it = indexed.iter();
        for ch in raw_comment.chars() {
            if ch == '\u{FFFC}' {
                if let Some((_, tok, _)) = it.next() {
                    if !out.is_empty() && !out.ends_with(' ') {
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
            if !out.is_empty() && !out.ends_with(' ') {
                out.push(' ');
            }
            out.push_str(tok);
        }
        return (out.trim().to_string(), emotes_json);
    }
    let mut chars: Vec<char> = raw_comment.chars().collect();
    indexed.sort_by_key(|a| std::cmp::Reverse(a.0));
    for (idx, tok, _) in &indexed {
        let pos = (*idx).min(chars.len());
        let need_leading = pos > 0 && !chars[pos - 1].is_whitespace();
        let need_trailing = pos < chars.len() && !chars[pos].is_whitespace();
        let mut insert = String::new();
        if need_leading {
            insert.push(' ');
        }
        insert.push_str(tok);
        if need_trailing {
            insert.push(' ');
        }
        chars.splice(pos..pos, insert.chars());
    }
    (chars.into_iter().collect::<String>().trim().to_string(), emotes_json)
}

/// A Webcast message as streamerz's TikTok event name and data.
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
                        "giftType": g.int(11),
                        "giftPictureUrl": g.msg(1).str(1),
                        "diamondCount": g.int(12),
                        "repeatCount": m.int(5),
                        "repeatEnd": m.int(9) == 1,
                    }),
                ),
            )
        }
        "WebcastControlMessage" if m.int(2) == 3 => ("streamEnd", json!({})),
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

async fn probe_embed(user: &str) -> (EmbedProbe, Option<String>) {
    let url = format!("https://www.tiktok.com/embed/@{}", crate::commands::url_encode(user));
    let mut saw_not_found = false;
    for attempt in 0..3 {
        match crate::http::request("GET", &url, &[("User-Agent".into(), UA.into())], b"").await {
            Ok(r) if r.status == 200 => {
                let page = r.text();
                if let Some(raw) = page
                    .split("\"avatarThumbUrl\":\"")
                    .nth(1)
                    .and_then(|s| s.split('"').next())
                {
                    let avatar = raw.replace("\\u0026", "&").replace("\\/", "/");
                    if avatar.starts_with("https://") {
                        remember_existing_user(user);
                        return (EmbedProbe::Exists, Some(avatar));
                    }
                }
                if page.contains("\"uniqueId\":\"") {
                    remember_existing_user(user);
                    return (EmbedProbe::Exists, None);
                }
                if page.contains("Couldn't find this account") || page.contains("user-not-found") {
                    saw_not_found = true;
                    break;
                }
                // Likely a bot-check / challenge HTML page rather than a missing user.
                return (EmbedProbe::Transient, None);
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
        (EmbedProbe::NotFound, None)
    } else if has_existed(user) {
        (EmbedProbe::Exists, None)
    } else {
        (EmbedProbe::Transient, None)
    }
}

/// A TikTok user's profile picture, from their public creator embed (no cookie, not rate limited
/// like `user/room`, and it works when they're not live).
pub async fn avatar(user: &str) -> Option<String> {
    probe_embed(user).await.1
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

async fn fetch_user_room_raw(channel: &str, ttwid: &str) -> Result<Value, String> {
    let url = format!(
        "https://www.tiktok.com/api-live/user/room?aid=1988&app_name=tiktok_web&device_platform=web_pc\
         &app_language=en&browser_language=en-US&region=US&user_is_login=false&uniqueId={}\
         &sourceType=54&staleTime=600000",
        crate::commands::url_encode(channel)
    );
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

/// The channel's `user/room` answer (user, live room). TikTok rate limits this lookup much less with a
/// `ttwid`.
pub async fn user_room(channel: &str, ttwid: &str) -> Result<Value, String> {
    let v = fetch_user_room_raw(channel, ttwid).await?;
    match v["statusCode"].as_i64() {
        Some(0) => {
            remember_existing_user(channel);
            Ok(v)
        }
        Some(19881007) => match probe_embed(channel).await.0 {
            EmbedProbe::Exists => Err(format!("@{channel} has never gone live yet")),
            EmbedProbe::Transient => Err(format!("@{channel} is offline")),
            EmbedProbe::NotFound => Err(format!("@{channel} not found")),
        },
        c => Err(format!("TikTok error {}", c.unwrap_or(-1))),
    }
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
        .unwrap_or(0);
    let status = v["data"]["liveRoom"]["status"]
        .as_i64()
        .or(v["data"]["user"]["status"].as_i64());
    if room == 0 || status != Some(2) {
        return Err(RoomLookupError::Offline);
    }
    Ok(room)
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
}
