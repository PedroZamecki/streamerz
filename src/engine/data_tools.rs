//! Text and data sub-actions Streamer.bot only has as C# or not at all: a regex match or replace, a
//! change to a text, splitting a list, reading a JSON value, a counter, a cooldown gate and a duration
//! as text. Streamerz-only codes 99780-99799, category Core/Text & Data.

use super::{b, n, s, Ctx, Engine};
use crate::model::Obj;
use serde_json::Value;
use std::rc::Rc;

/// The most text one of these works on (a regex or a split over more is cut here).
const MOST: usize = 100_000;

/// `path` (`a.b[0].c`) in `value`; None when something on the way is missing.
pub fn json_path<'a>(value: &'a Value, path: &str) -> Option<&'a Value> {
    let mut at = value;
    for part in path.split('.').filter(|p| !p.is_empty()) {
        // `name[0][1]`: a key, then indexes.
        let (key, mut rest) = match part.find('[') {
            Some(i) => (&part[..i], &part[i..]),
            None => (part, ""),
        };
        if !key.is_empty() {
            at = at.get(key)?;
        }
        while let Some(open) = rest.strip_prefix('[') {
            let close = open.find(']')?;
            at = at.get(open[..close].trim().parse::<usize>().ok()?)?;
            rest = &open[close + 1..];
        }
    }
    Some(at)
}

/// Seconds as text in a style: 0 `1:02:03`, 1 `1h 2m 3s`, 2 `1 hour, 2 minutes, 3 seconds`.
pub fn duration_text(secs: u64, style: u64) -> String {
    let (d, h, m, s) = (secs / 86_400, secs / 3600 % 24, secs / 60 % 60, secs % 60);
    match style {
        1 => {
            let parts: Vec<String> = [(d, "d"), (h, "h"), (m, "m"), (s, "s")]
                .iter()
                .filter(|(v, _)| *v > 0)
                .map(|(v, u)| format!("{v}{u}"))
                .collect();
            if parts.is_empty() {
                "0s".into()
            } else {
                parts.join(" ")
            }
        }
        2 => {
            let plural = |v: u64, one: &str| format!("{v} {one}{}", if v == 1 { "" } else { "s" });
            let parts: Vec<String> = [(d, "day"), (h, "hour"), (m, "minute"), (s, "second")]
                .iter()
                .filter(|(v, _)| *v > 0)
                .map(|(v, u)| plural(*v, u))
                .collect();
            if parts.is_empty() {
                "0 seconds".into()
            } else {
                parts.join(", ")
            }
        }
        _ => {
            let hours = secs / 3600;
            if hours > 0 {
                format!("{hours}:{m:02}:{s:02}")
            } else {
                format!("{m}:{s:02}")
            }
        }
    }
}

/// A text changed: upper, lower, title case, trimmed, reversed, extra spaces taken out, its first or
/// last word, URL encoded, or its length.
pub fn transform(text: &str, how: u64) -> String {
    match how {
        0 => text.to_uppercase(),
        1 => text.to_lowercase(),
        2 => text
            .split(' ')
            .map(|w| {
                let mut c = w.chars();
                c.next().map_or(String::new(), |f| {
                    f.to_uppercase().collect::<String>() + &c.as_str().to_lowercase()
                })
            })
            .collect::<Vec<_>>()
            .join(" "),
        3 => text.trim().to_string(),
        4 => text.chars().rev().collect(),
        5 => text.split_whitespace().collect::<Vec<_>>().join(" "),
        6 => text.split_whitespace().next().unwrap_or("").to_string(),
        7 => text.split_whitespace().last().unwrap_or("").to_string(),
        8 => crate::commands::url_encode(text),
        _ => text.chars().count().to_string(),
    }
}

thread_local! {
    /// When each cooldown gate opens again (ms since the epoch), by key. Only the ones still closed stay.
    static GATES: std::cell::RefCell<std::collections::HashMap<String, u64>> = std::cell::RefCell::default();
}

/// How long the gate `key` is still closed (ms). Open: it closes for `wait` ms from `now` and says 0.
pub fn gate(key: &str, wait: u64, now: u64) -> u64 {
    GATES.with_borrow_mut(|gates| {
        if gates.len() > 500 {
            gates.retain(|_, until| *until > now);
        }
        match gates.get(key) {
            Some(until) if *until > now => *until - now,
            _ => {
                if wait > 0 {
                    gates.insert(key.to_string(), now + wait);
                }
                0
            }
        }
    })
}

fn cut(text: String) -> String {
    if text.len() > MOST {
        text.chars().take(MOST).collect()
    } else {
        text
    }
}

impl Engine {
    /// Runs a text or data sub-action; false to stop the action (a cooldown still on).
    pub(super) fn data_tool(self: &Rc<Self>, ctx: &mut Ctx, o: &Obj, kind: u64) -> bool {
        let text = |ctx: &Ctx, key: &str| self.expand(ctx, s(o, key));
        let name = |ctx: &Ctx, default: &str| {
            let v = self.simple(ctx, s(o, "variableName"));
            if v.trim().is_empty() {
                default.to_string()
            } else {
                v.trim().to_string()
            }
        };
        match kind {
            // Regex Match: the groups as `<name>[0]`, `<name>[1]`..., and `<name>Found`.
            99780 => {
                let var = name(ctx, "match");
                let (input, pattern) = (cut(text(ctx, "input")), text(ctx, "pattern"));
                let pattern = if b(o, "ignoreCase", false) {
                    format!("(?i){pattern}")
                } else {
                    pattern
                };
                let mut found = false;
                if let Some(r) = crate::engine::regex(&pattern) {
                    let mut got = Obj::new();
                    found = crate::engine::match_args(&r, &input, &mut got);
                    for (k, v) in got {
                        // `match[0]` becomes `<name>[0]`; named groups and the count keep their names.
                        let key = k.strip_prefix("match").map_or(k.clone(), |rest| format!("{var}{rest}"));
                        ctx.args.insert(key, v);
                    }
                }
                ctx.args.insert(format!("{var}Found"), found.into());
            }
            99781 => {
                let var = name(ctx, "replaced");
                let (input, pattern, with) = (cut(text(ctx, "input")), text(ctx, "pattern"), text(ctx, "replacement"));
                let pattern = if b(o, "ignoreCase", false) {
                    format!("(?i){pattern}")
                } else {
                    pattern
                };
                let out = match crate::engine::regex(&pattern) {
                    Some(r) if b(o, "all", true) => r.replace_all(&input, with.as_str()).into_owned(),
                    Some(r) => r.replace(&input, with.as_str()).into_owned(),
                    None => input,
                };
                ctx.args.insert(var, out.into());
            }
            99782 => {
                let var = name(ctx, "text");
                ctx.args
                    .insert(var, transform(&cut(text(ctx, "input")), n(o, "how") as u64).into());
            }
            // Split: `<name>Count`, `<name>0`, `<name>1`...
            99783 => {
                let var = name(ctx, "part");
                let (input, sep) = (cut(text(ctx, "input")), text(ctx, "separator"));
                let sep = if sep.is_empty() {
                    ",".to_string()
                } else {
                    sep.replace("\\n", "\n")
                };
                let parts: Vec<&str> = input
                    .split(sep.as_str())
                    .map(str::trim)
                    .filter(|p| !p.is_empty())
                    .take(200)
                    .collect();
                ctx.args.insert(format!("{var}Count"), parts.len().into());
                for (i, p) in parts.iter().enumerate() {
                    ctx.args.insert(format!("{var}{i}"), (*p).into());
                }
                if !parts.is_empty() {
                    // A pick at random, for a "say one of these" action.
                    let at = crate::math::secure_u64() as usize % parts.len();
                    ctx.args.insert(format!("{var}Random"), parts[at].into());
                }
            }
            // JSON value: `<name>`, and `<name>Found`.
            99784 => {
                let var = name(ctx, "json");
                let (input, path) = (cut(text(ctx, "input")), text(ctx, "path"));
                let found = serde_json::from_str::<Value>(&input)
                    .ok()
                    .and_then(|v| json_path(&v, path.trim()).cloned());
                ctx.args.insert(format!("{var}Found"), found.is_some().into());
                match found {
                    Some(Value::String(t)) => drop(ctx.args.insert(var, t.into())),
                    Some(Value::Null) | None => drop(ctx.args.insert(var, "".into())),
                    Some(v) => drop(ctx.args.insert(var, v)),
                }
            }
            // Counter: add, subtract, set or reset a number kept as a global.
            99785 => {
                let global = text(ctx, "name").trim().to_string();
                if global.is_empty() {
                    crate::error("Counter: no name given".to_string());
                    return true;
                }
                let persisted = b(o, "persisted", true);
                let amount = crate::fmt::parse_number(&text(ctx, "amount")).unwrap_or(1.0);
                let now = self.global_number(&global, persisted);
                let next = match n(o, "operation") as u64 {
                    1 => now - amount,
                    2 => amount,
                    3 => 0.0,
                    _ => now + amount,
                };
                self.put_global(&global, serde_json::json!(next), persisted);
                let var = name(ctx, "counter");
                ctx.args.insert(var, serde_json::json!(next));
            }
            // Cooldown gate: the action goes on once in `duration`, and stops the other times.
            99786 => {
                let who = if b(o, "perUser", false) {
                    self.expand(ctx, "%userId%")
                } else {
                    String::new()
                };
                let key = format!("{}|{who}", text(ctx, "key").trim());
                let wait = (self.duration_number(ctx, o, "duration").max(0.0) * 1000.0) as u64;
                let now = crate::templates::clock::now_ms();
                let left = gate(&key, wait, now);
                ctx.args.insert("cooldownLeft".into(), left.div_ceil(1000).into());
                if left > 0 {
                    return false;
                }
            }
            99787 => {
                let var = name(ctx, "duration");
                let secs = crate::fmt::parse_number(&text(ctx, "seconds")).unwrap_or(0.0).max(0.0) as u64;
                ctx.args.insert(var, duration_text(secs, n(o, "style") as u64).into());
            }
            _ => self.unsupported(kind),
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn a_json_path_walks_keys_and_indexes() {
        let v = json!({"a": {"b": [10, {"c": "deep"}]}, "list": [[1, 2], [3]]});
        assert_eq!(json_path(&v, "a.b[0]"), Some(&json!(10)));
        assert_eq!(json_path(&v, "a.b[1].c"), Some(&json!("deep")));
        assert_eq!(json_path(&v, "list[0][1]"), Some(&json!(2)));
        assert_eq!(json_path(&v, ""), Some(&v));
        assert_eq!(json_path(&v, "a.x"), None);
        assert_eq!(json_path(&v, "a.b[5]"), None);
        assert_eq!(json_path(&v, "a.b[x]"), None);
    }

    #[test]
    fn texts_change_and_durations_read_as_words() {
        assert_eq!(transform("heLLo wORLD", 2), "Hello World");
        assert_eq!(transform("  a   b  ", 5), "a b");
        assert_eq!(transform("one two three", 6), "one");
        assert_eq!(transform("one two three", 7), "three");
        assert_eq!(transform("héllo", 9), "5");
        assert_eq!(transform("a b&c", 8), "a+b%26c");
        assert_eq!(duration_text(3723, 0), "1:02:03");
        assert_eq!(duration_text(59, 0), "0:59");
        assert_eq!(duration_text(3723, 1), "1h 2m 3s");
        assert_eq!(duration_text(0, 1), "0s");
        assert_eq!(duration_text(90_061, 2), "1 day, 1 hour, 1 minute, 1 second");
        assert_eq!(duration_text(120, 2), "2 minutes");
    }

    #[test]
    fn a_gate_closes_for_its_time_and_opens_after() {
        assert_eq!(gate("k", 5_000, 1_000), 0, "open: it lets the first through");
        assert_eq!(gate("k", 5_000, 3_000), 3_000, "closed for the rest");
        assert_eq!(gate("k", 5_000, 6_000), 0, "open again");
        assert_eq!(gate("other", 5_000, 3_000), 0, "another key is its own");
        assert_eq!(gate("none", 0, 1_000), 0);
        assert_eq!(gate("none", 0, 1_001), 0, "no time, no gate");
    }
}
