//! Chat commands, as Streamer.bot matches them (checked against the real one): Basic mode with
//! aliases one per line, matched at the Start (followed by a space or the end), Exact, or Anywhere,
//! ignoring case unless Case Sensitive; or Regex mode. Then permissions and cooldowns (the
//! broadcaster skips both), and the Command Triggered arguments.

use crate::model::Obj;
use serde_json::Value;

/// A chat message, from any platform.
#[derive(Clone, Default)]
pub struct Chat {
    /// "twitch", "youtube", "kick" or "tiktok".
    pub platform: String,
    /// Streamer.bot's CommandSource bit: 1 Twitch message, 2 whisper, 8 resub, 1024 YouTube,
    /// 2097152 Kick, 4194304 TikTok.
    pub source: u64,
    pub user_id: String,
    pub user: String,
    pub broadcaster: bool,
    pub moderator: bool,
    pub vip: bool,
    pub subscriber: bool,
    /// Sent by the bot account / by Streamerz itself.
    pub bot: bool,
    pub internal: bool,
    pub msg_id: String,
    pub text: String,
    /// The platform emotes in `text`, by the name shown there (Twitch "Kappa", YouTube ":yt:", Kick
    /// "[emote:37226:KEKW]"), with their picture's URL.
    pub emotes: Vec<(String, String)>,
    /// The platform's own arguments (user, userName, userType, ...), passed on to the action.
    pub args: Obj,
}

fn s<'a>(o: &'a Obj, k: &str) -> &'a str {
    o.get(k).and_then(Value::as_str).unwrap_or("")
}

fn n(o: &Obj, k: &str) -> f64 {
    o.get(k).and_then(Value::as_f64).unwrap_or(0.0)
}

/// A command's aliases: its `command` text, one per line.
pub fn aliases(c: &Obj) -> Vec<&str> {
    s(c, "command")
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .collect()
}

/// Whether `text` runs command `c`: the alias that matched, rawInput, and the regex match
/// arguments (Regex mode).
pub fn matches(c: &Obj, text: &str) -> Option<(String, String, Obj)> {
    let text = text.trim();
    let cs = c.get("caseSensitive").and_then(Value::as_bool).unwrap_or(false);
    if n(c, "mode") as i64 == 1 {
        let mut m = Obj::new();
        for pattern in aliases(c) {
            let compiled = if cs {
                crate::engine::regex(pattern)
            } else {
                crate::engine::regex(&format!("(?i){pattern}"))
            };
            let Some(r) = compiled else {
                continue;
            };
            if crate::engine::match_args(&r, text, &mut m) {
                return Some((pattern.to_string(), text.to_string(), m));
            }
        }
        return None;
    }
    let fold = |x: &str| if cs { x.to_string() } else { x.to_lowercase() };
    let t = fold(text);
    for alias in aliases(c) {
        let a = fold(alias);
        let hit = match n(c, "location") as i64 {
            // Start: the alias, then a space or the end.
            0 => t.starts_with(&a) && t[a.len()..].chars().next().is_none_or(char::is_whitespace),
            1 => t == a,
            _ => t.contains(&a),
        };
        if hit {
            let raw = match n(c, "location") as i64 {
                0 => {
                    let end = if cs {
                        a.len()
                    } else {
                        let mut folded_len = 0;
                        text.char_indices()
                            .find_map(|(i, ch)| {
                                if folded_len >= a.len() {
                                    return Some(i);
                                }
                                folded_len += ch.to_lowercase().map(char::len_utf8).sum::<usize>();
                                None
                            })
                            .unwrap_or(text.len())
                    };
                    text[end..].trim().to_string()
                }
                1 => String::new(),
                _ => text.to_string(),
            };
            return Some((alias.to_string(), raw, Obj::new()));
        }
    }
    None
}

/// Pushes `s` to `out` percent-encoded: the ASCII bytes `keep` accepts as they are, every other byte
/// of its UTF-8 as %XX (uppercase hex).
pub fn percent_into(out: &mut String, s: &str, keep: impl Fn(u8) -> bool) {
    const HEX: &[u8; 16] = b"0123456789ABCDEF";
    for b in s.bytes() {
        if b.is_ascii() && keep(b) {
            out.push(char::from(b));
        } else {
            out.push('%');
            out.push(char::from(HEX[usize::from(b >> 4)]));
            out.push(char::from(HEX[usize::from(b & 15)]));
        }
    }
}

/// `s` percent-encoded, keeping the bytes `keep` accepts (see `percent_into`).
pub fn percent(s: &str, keep: impl Fn(u8) -> bool) -> String {
    let mut out = String::with_capacity(s.len());
    percent_into(&mut out, s, keep);
    out
}

/// .NET's WebUtility.UrlEncode: unreserved `A-Z a-z 0-9 - _ . ! * ( )` stay, space is `+`, the
/// rest is %XX (uppercase) of the UTF-8 bytes.
pub fn url_encode(s: &str) -> String {
    let keep = |b: u8| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.' | b'!' | b'*' | b'(' | b')');
    let mut out = String::with_capacity(s.len());
    for (i, part) in s.split(' ').enumerate() {
        if i > 0 {
            out.push('+');
        }
        percent_into(&mut out, part, keep);
    }
    out
}

/// RFC 3986's encoding of a path segment or a query value: unreserved `A-Z a-z 0-9 - _ . ~` stay,
/// the rest (a space too) is %XX of the UTF-8 bytes.
pub fn uri_component(s: &str) -> String {
    percent(s, |b| {
        b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.' | b'~')
    })
}

/// A form or query value decoded: `+` is a space, %XX a byte (two hex digits, else kept as typed),
/// the bytes read as UTF-8.
pub fn url_decode(s: &str) -> String {
    let hex = |b: u8| (b as char).to_digit(16);
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'%' if i + 2 < bytes.len() => match (hex(bytes[i + 1]), hex(bytes[i + 2])) {
                (Some(hi), Some(lo)) => {
                    out.push((hi * 16 + lo) as u8);
                    i += 2;
                }
                _ => out.push(b'%'),
            },
            b'+' => out.push(b' '),
            b => out.push(b),
        }
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// Escaped for a JSON/C# string literal.
pub fn escape(s: &str) -> String {
    s.replace('\\', "\\\\").replace('"', "\\\"")
}

/// rawInput and its words (split on single spaces, like Streamer.bot: two spaces make an empty
/// word, and no input still gives an empty input0), each also escaped and URL-encoded.
pub fn input_args(raw: &str, words_from: &str, args: &mut Obj) {
    args.insert("rawInput".into(), raw.into());
    args.insert("rawInputEscaped".into(), escape(raw).into());
    args.insert("rawInputUrlEncoded".into(), url_encode(raw).into());
    for (i, w) in words_from.split(' ').enumerate() {
        args.insert(format!("input{i}"), w.into());
        args.insert(format!("inputEscaped{i}"), escape(w).into());
        args.insert(format!("inputUrlEncoded{i}"), url_encode(w).into());
    }
}

const USER_KEYS: [&str; 8] = [
    "id",
    "userId",
    "login",
    "user",
    "userName",
    "name",
    "display",
    "displayName",
];

fn matches_user(entry: &Value, chat: &Chat) -> bool {
    let matches_str = |x: &str| {
        !x.is_empty()
            && ((!chat.user.is_empty() && x.eq_ignore_ascii_case(&chat.user))
                || (!chat.user_id.is_empty() && x == chat.user_id))
    };
    match entry {
        Value::String(x) => matches_str(x),
        Value::Object(o) => {
            if !chat.platform.is_empty() {
                for key in ["type", "platform"] {
                    if let Some(p) = o.get(key).and_then(Value::as_str).filter(|p| !p.is_empty()) {
                        if !p.eq_ignore_ascii_case(&chat.platform) {
                            return false;
                        }
                    }
                }
            }
            USER_KEYS
                .iter()
                .filter_map(|k| o.get(*k).and_then(Value::as_str))
                .any(matches_str)
        }
        _ => false,
    }
}

/// Grant type 0 Allow (only the listed users and groups; nobody listed: everyone), 1 Deny (all but
/// the listed; nobody listed: no one). Users are listed by login or id; groups by name, the built-in
/// ones being Moderators, VIPs and Subscribers. `in_group` answers for user groups.
pub fn permitted(c: &Obj, chat: &Chat, in_group: &dyn Fn(&str) -> bool) -> bool {
    if chat.broadcaster {
        return true;
    }
    let users = c
        .get("permittedUsers")
        .and_then(Value::as_array)
        .map_or(&[][..], Vec::as_slice);
    let groups = c
        .get("permittedGroups")
        .and_then(Value::as_array)
        .map_or(&[][..], Vec::as_slice);
    let user_hit = users.iter().any(|u| matches_user(u, chat));
    let group_hit = groups
        .iter()
        .filter_map(Value::as_str)
        .any(|g| match g.to_lowercase().as_str() {
            "moderators" => chat.moderator,
            "vips" => chat.vip,
            "subscribers" => chat.subscriber,
            _ => in_group(g),
        });
    let listed = user_hit || group_hit;
    let empty = users.is_empty() && groups.is_empty();
    if n(c, "grantType") as i64 == 1 {
        !empty && !listed
    } else {
        empty || listed
    }
}

#[cfg(test)]
mod tests {
    /// The encoders as they were written before `percent`: the new ones must match them byte for byte.
    fn dotnet_before(s: &str) -> String {
        const HEX: &[u8; 16] = b"0123456789ABCDEF";
        let mut out = String::with_capacity(s.len());
        for b in s.bytes() {
            match b {
                b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'!' | b'*' | b'(' | b')' => {
                    out.push(b as char)
                }
                b' ' => out.push('+'),
                _ => {
                    out.push('%');
                    out.push(HEX[(b >> 4) as usize] as char);
                    out.push(HEX[(b & 0x0f) as usize] as char);
                }
            }
        }
        out
    }

    fn rfc3986_before(s: &str) -> String {
        s.bytes()
            .map(|b| match b {
                b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => (b as char).to_string(),
                _ => format!("%{b:02X}"),
            })
            .collect()
    }

    #[test]
    fn both_percent_alphabets_match_the_old_encoders() {
        // Every ASCII character, some of the wider ones, and strings made of them (a fixed seed).
        let pool: Vec<char> = (0u8..128)
            .map(char::from)
            .chain("é ç ã 中 😀 \u{a0}\u{fffd}".chars())
            .collect();
        let mut seed: u64 = 0x2545_f491_4f6c_dd1d;
        let mut samples: Vec<String> = pool.iter().map(|c| c.to_string()).collect();
        for _ in 0..2000 {
            let mut s = String::new();
            for _ in 0..(seed % 24) {
                seed ^= seed << 13;
                seed ^= seed >> 7;
                seed ^= seed << 17;
                s.push(pool[(seed % pool.len() as u64) as usize]);
            }
            samples.push(s);
        }
        samples.extend(["", " ", "  a  ", "a+b c&d=e/f?g#h~i'j"].map(String::from));
        for s in &samples {
            assert_eq!(super::url_encode(s), dotnet_before(s), "{s:?}");
            assert_eq!(super::uri_component(s), rfc3986_before(s), "{s:?}");
        }
        assert_eq!(super::url_encode("a b~!"), "a+b%7E!");
        assert_eq!(super::uri_component("a b~!"), "a%20b~%21");
    }

    use super::*;
    use serde_json::json;

    #[test]
    fn percent_escapes_take_two_hex_digits_only() {
        assert_eq!(url_decode("a%2Fb"), "a/b");
        // A sign isn't a hex digit (the `+` is then a space, as always).
        assert_eq!(url_decode("%+1%-1"), "% 1%-1");
        assert_eq!(url_decode("%zz%4"), "%zz%4");
        assert_eq!(url_decode("100%"), "100%");
        assert_eq!(url_decode("%ZZ%20ok"), "%ZZ ok");
        // Several bytes make one character.
        assert_eq!(url_decode("Doa%C3%A7%C3%A3o+%E2%82%AC"), "Doação €");
        assert_eq!(url_decode("Jos%C3%A9+%E2%9D%A4%EF%B8%8F"), "José ❤️");
        assert_eq!(url_decode("é%"), "é%");
        // Bytes that aren't UTF-8 don't panic.
        assert_eq!(url_decode("%FF"), "\u{FFFD}");
        assert_eq!(url_decode(&url_encode("Doação ❤ a/b")), "Doação ❤ a/b");
    }

    fn cmd(v: Value) -> Obj {
        v.as_object().unwrap().clone()
    }

    #[test]
    fn unicode_case_matching_keeps_raw_input() {
        let command = cmd(json!({"command": "!İ"}));
        assert_eq!(matches(&command, "!i\u{307} hello").unwrap().1, "hello");
        let reverse = cmd(json!({"command": "!i\u{307}"}));
        assert_eq!(matches(&reverse, "!İ hello").unwrap().1, "hello");
        // Kelvin sign (3 UTF-8 bytes) lowercases to ASCII 'k' (1 byte).
        let kelvin = cmd(json!({"command": "!k"}));
        assert_eq!(matches(&kelvin, "!\u{212A} world").unwrap().1, "world");
        assert_eq!(matches(&kelvin, "!\u{212A}").unwrap().1, "");
    }

    #[test]
    fn regex_case_sensitivity_and_multiple_patterns() {
        let ci = cmd(json!({"command": "[\n^!roll (?<dice>\\d+)d(\\d+)$", "mode": 1}));
        let (pat, raw, m) = matches(&ci, "!ROLL 2d6").unwrap();
        assert_eq!(pat, "^!roll (?<dice>\\d+)d(\\d+)$");
        assert_eq!(raw, "!ROLL 2d6");
        assert_eq!(m["dice"], json!("2"));

        let cs = cmd(json!({"command": "^!roll (\\d+)$", "mode": 1, "caseSensitive": true}));
        assert!(matches(&cs, "!ROLL 5").is_none());
        assert!(matches(&cs, "!roll 5").is_some());
    }

    #[test]
    fn like_streamerbot() {
        let start = cmd(json!({"command": "!szt\r\n!sz", "mode": 0, "location": 0}));
        let (alias, raw, _) = matches(&start, "!szt hello  world & más").unwrap();
        assert_eq!((alias.as_str(), raw.as_str()), ("!szt", "hello  world & más"));
        assert_eq!(matches(&start, "!SZ").unwrap().0, "!sz");
        assert!(matches(&start, "!szthello").is_none());
        let exact = cmd(json!({"command": "!x", "location": 1}));
        assert!(matches(&exact, "!x extra").is_none() && matches(&exact, "!X").is_some());
        let any = cmd(json!({"command": "banana", "location": 2}));
        assert_eq!(matches(&any, "I like Banana bread").unwrap().1, "I like Banana bread");
        let cs = cmd(json!({"command": "!Hi", "caseSensitive": true}));
        assert!(matches(&cs, "!hi").is_none());
        let re = cmd(json!({"command": "^!roll (?<dice>\\d+)d(\\d+)$", "mode": 1}));
        let (_, raw, m) = matches(&re, "!roll 2d6").unwrap();
        assert_eq!(raw, "!roll 2d6");
        assert_eq!(
            (m["match.count"].clone(), m["match[1]"].clone(), m["dice"].clone()),
            (json!(3), json!("6"), json!("2"))
        );

        let mut a = Obj::new();
        input_args("hello  world & más", "hello  world & más", &mut a);
        assert_eq!(a["rawInputUrlEncoded"], "hello++world+%26+m%C3%A1s");
        assert_eq!(
            (a["input0"].clone(), a["input1"].clone(), a["input2"].clone()),
            (json!("hello"), json!(""), json!("world"))
        );
    }

    #[test]
    fn permissions() {
        let viewer = Chat {
            user: "ana".into(),
            user_id: "1".into(),
            ..Default::default()
        };
        let none = |_: &str| false;
        let open = cmd(json!({"grantType": 0, "permittedUsers": [], "permittedGroups": []}));
        assert!(permitted(&open, &viewer, &none));
        let mods = cmd(json!({"grantType": 0, "permittedGroups": ["Moderators"]}));
        assert!(!permitted(&mods, &viewer, &none));
        assert!(permitted(
            &mods,
            &Chat {
                moderator: true,
                ..viewer.clone()
            },
            &none
        ));
        let deny_ana = cmd(json!({"grantType": 1, "permittedUsers": ["Ana"]}));
        assert!(!permitted(&deny_ana, &viewer, &none));
        let deny_all = cmd(json!({"grantType": 1}));
        assert!(!permitted(&deny_all, &viewer, &none));
        assert!(permitted(
            &deny_all,
            &Chat {
                broadcaster: true,
                ..viewer.clone()
            },
            &none
        ));
        let group = cmd(json!({"grantType": 0, "permittedGroups": ["Regulars"]}));
        assert!(permitted(&group, &viewer, &|g| g == "Regulars"));

        // Object entries in permittedUsers match identity fields, never metadata or empty strings.
        let obj_user = cmd(json!({
            "grantType": 0,
            "permittedUsers": [{"id": "9", "name": "alice", "type": "twitch", "display": ""}]
        }));
        let spoof = Chat {
            platform: "twitch".into(),
            user: "twitch".into(),
            user_id: String::new(),
            ..Default::default()
        };
        assert!(!permitted(&obj_user, &spoof, &none));
        let wrong_platform = Chat {
            platform: "kick".into(),
            user: "alice".into(),
            user_id: "9".into(),
            ..Default::default()
        };
        assert!(!permitted(&obj_user, &wrong_platform, &none));
        let right = Chat {
            platform: "twitch".into(),
            user: "Alice".into(),
            user_id: "9".into(),
            ..Default::default()
        };
        assert!(permitted(&obj_user, &right, &none));
        let empty_entry = cmd(json!({"grantType": 0, "permittedUsers": [""]}));
        assert!(!permitted(&empty_entry, &spoof, &none));
    }
}
