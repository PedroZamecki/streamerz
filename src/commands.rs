//! Chat commands, as Streamer.bot matches them (checked against the real one): Basic mode with
//! aliases one per line, matched at the Start (followed by a space or the end), Exact, or Anywhere,
//! ignoring case unless Case Sensitive; or Regex mode. Then permissions and cooldowns (the
//! broadcaster skips both), and the Command Triggered arguments.

use crate::model::Obj;
use serde_json::Value;

/// A chat message, from any platform.
#[derive(Clone, Default)]
pub struct Chat {
    /// "twitch", "youtube" or "kick".
    pub platform: String,
    /// Streamer.bot's CommandSource bit: 1 Twitch message, 2 whisper, 8 resub, 1024 YouTube,
    /// 2097152 Kick.
    pub source: u64,
    pub user_id: String,
    pub user: String,
    pub broadcaster: bool,
    pub moderator: bool,
    pub vip: bool,
    pub subscriber: bool,
    /// Sent by the bot account / by streamerz itself.
    pub bot: bool,
    pub internal: bool,
    pub msg_id: String,
    pub text: String,
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
    if n(c, "mode") as i64 == 1 {
        let mut m = Obj::new();
        for pattern in aliases(c) {
            let r = crate::engine::regex(pattern)?;
            if crate::engine::match_args(&r, text, &mut m) {
                return Some((s(c, "command").to_string(), text.to_string(), m));
            }
        }
        return None;
    }
    let cs = c.get("caseSensitive").and_then(Value::as_bool).unwrap_or(false);
    let fold = |x: &str| if cs { x.to_string() } else { x.to_lowercase() };
    let t = fold(text);
    for alias in aliases(c) {
        let a = fold(alias);
        let hit = match n(c, "location") as i64 {
            // Start: the alias, then a space or the end. ponytail: lowercase keeps byte offsets for
            // the text that matters here (ASCII aliases); exotic case-folding may shift rawInput.
            0 => t.starts_with(&a) && t[a.len()..].chars().next().is_none_or(char::is_whitespace),
            1 => t == a,
            _ => t.contains(&a),
        };
        if hit {
            let raw = match n(c, "location") as i64 {
                0 => text.get(alias.len()..).unwrap_or("").trim().to_string(),
                1 => String::new(),
                _ => text.to_string(),
            };
            return Some((alias.to_string(), raw, Obj::new()));
        }
    }
    None
}

/// .NET's WebUtility.UrlEncode: unreserved `A-Z a-z 0-9 - _ . ! * ( )` stay, space is `+`, the
/// rest is %XX (uppercase) of the UTF-8 bytes.
pub fn url_encode(s: &str) -> String {
    let mut out = String::new();
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'!' | b'*' | b'(' | b')' => {
                out.push(b as char)
            }
            b' ' => out.push('+'),
            _ => out += &format!("%{b:02X}"),
        }
    }
    out
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

/// Grant type 0 Allow (only the listed users and groups; nobody listed: everyone), 1 Deny (all but
/// the listed; nobody listed: no one). Users are listed by login or id; groups by name, the built-in
/// ones being Moderators, VIPs and Subscribers. `in_group` answers for user groups.
pub fn permitted(c: &Obj, chat: &Chat, in_group: &dyn Fn(&str) -> bool) -> bool {
    if chat.broadcaster {
        return true;
    }
    let list = |k: &str| c.get(k).and_then(Value::as_array).cloned().unwrap_or_default();
    let (users, groups) = (list("permittedUsers"), list("permittedGroups"));
    let user_hit = users.iter().any(|u| {
        let names: Vec<&str> = match u {
            Value::String(x) => vec![x],
            Value::Object(o) => o.values().filter_map(Value::as_str).collect(),
            _ => vec![],
        };
        names
            .iter()
            .any(|x| x.eq_ignore_ascii_case(&chat.user) || *x == chat.user_id)
    });
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
    use super::*;
    use serde_json::json;

    fn cmd(v: Value) -> Obj {
        v.as_object().unwrap().clone()
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
    }
}
