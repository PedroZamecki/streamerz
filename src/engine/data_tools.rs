//! Text and data sub-actions Streamer.bot only has as C# or not at all: a regex match or replace, a
//! change to a text, splitting a list, reading a JSON value, a counter, a cooldown gate and a duration
//! as text, a list kept as a global, and the system clipboard. Streamerz-only codes 99780-99799,
//! category Core/Text & Data (the clipboard's in Core/System).

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
        if !rest.is_empty() {
            return None;
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
                    gates.insert(key.to_string(), now.saturating_add(wait));
                }
                0
            }
        }
    })
}

/// The most items a list keeps (the oldest go first).
const MOST_ITEMS: usize = 1000;

/// A list operation on `items`; what it gives back, if anything. 0 add, 1 remove, 2 has, 3 item at
/// (from 0, negative from the end), 4 random, 5 count, 6 clear, 7 take the first, 8 all joined.
pub fn list_op(items: &mut Vec<String>, op: u64, value: &str) -> Option<String> {
    match op {
        0 => {
            items.push(value.to_string());
            let over = items.len().saturating_sub(MOST_ITEMS);
            items.drain(..over);
            None
        }
        1 => {
            let needle = value.to_lowercase();
            let at = items.iter().position(|item| item.to_lowercase() == needle)?;
            Some(items.remove(at))
        }
        2 => {
            let needle = value.to_lowercase();
            items.iter().find(|item| item.to_lowercase() == needle).cloned()
        }
        3 => {
            let at = value.trim().parse::<i64>().ok()?;
            let at = if at < 0 {
                (items.len() as i64).checked_add(at)?
            } else {
                at
            };
            items.get(usize::try_from(at).ok()?).cloned()
        }
        4 => (!items.is_empty()).then(|| items[crate::math::secure_u64() as usize % items.len()].clone()),
        5 => Some(items.len().to_string()),
        6 => {
            items.clear();
            None
        }
        7 => (!items.is_empty()).then(|| items.remove(0)),
        _ => Some(items.join(if value.is_empty() { ", " } else { value })),
    }
}

/// The system clipboard's text. Linux asks wl-paste (Wayland) or xclip / xsel (X11).
#[cfg(not(windows))]
async fn clipboard_read() -> Result<String, String> {
    let mut last = String::new();
    for (program, args) in clipboard_programs(false) {
        let fut = tokio::process::Command::new(program)
            .args(args)
            .kill_on_drop(true)
            .output();
        match tokio::time::timeout(std::time::Duration::from_secs(2), fut).await {
            Ok(Ok(out)) if out.status.success() => return Ok(String::from_utf8_lossy(&out.stdout).into_owned()),
            // wl-paste says so when the clipboard is empty.
            Ok(Ok(out)) if String::from_utf8_lossy(&out.stderr).contains("Nothing is copied") => {
                return Ok(String::new())
            }
            Ok(Ok(out)) => last = String::from_utf8_lossy(&out.stderr).trim().to_string(),
            Ok(Err(_)) => {}
            Err(_) => last = format!("{program} timed out"),
        }
    }
    Err(if last.is_empty() {
        NO_CLIPBOARD.to_string()
    } else {
        last
    })
}

/// Puts text on the system clipboard. The program forks and keeps the text there after it returns.
#[cfg(not(windows))]
async fn clipboard_write(text: &str) -> Result<(), String> {
    use std::process::Stdio;
    use tokio::io::AsyncWriteExt;
    let mut last = String::new();
    for (program, args) in clipboard_programs(true) {
        let Ok(mut child) = tokio::process::Command::new(program)
            .args(args)
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .kill_on_drop(true)
            .spawn()
        else {
            continue;
        };
        if let Some(mut stdin) = child.stdin.take() {
            if let Err(e) = stdin.write_all(text.as_bytes()).await {
                let _ = child.kill().await;
                last = format!("{program}: {e}");
                continue;
            }
        }
        match tokio::time::timeout(std::time::Duration::from_secs(2), child.wait()).await {
            Ok(Ok(status)) if status.success() => return Ok(()),
            Ok(_) => last = format!("{program} could not set the clipboard"),
            Err(_) => {
                let _ = child.kill().await;
                last = format!("{program} timed out");
            }
        }
    }
    Err(if last.is_empty() { NO_CLIPBOARD.into() } else { last })
}

#[cfg(not(windows))]
const NO_CLIPBOARD: &str = "no clipboard program found: install wl-clipboard (Wayland) or xclip (X11)";

/// The programs to try, the session's own first.
#[cfg(not(windows))]
fn clipboard_programs(write: bool) -> Vec<(&'static str, &'static [&'static str])> {
    let wayland = (
        if write { "wl-copy" } else { "wl-paste" },
        if write { &[][..] } else { &["--no-newline"][..] },
    );
    let x11 = [
        (
            "xclip",
            if write {
                &["-selection", "clipboard", "-i"][..]
            } else {
                &["-selection", "clipboard", "-o"][..]
            },
        ),
        (
            "xsel",
            if write {
                &["--clipboard", "--input"][..]
            } else {
                &["--clipboard", "--output"][..]
            },
        ),
    ];
    let mut list = Vec::new();
    if std::env::var_os("WAYLAND_DISPLAY").is_some() {
        list.push(wayland);
    }
    if std::env::var_os("DISPLAY").is_some() {
        list.extend(x11);
    }
    list
}

#[cfg(windows)]
async fn clipboard_read() -> Result<String, String> {
    use windows_sys::Win32::System::{
        DataExchange::{CloseClipboard, GetClipboardData, OpenClipboard},
        Memory::{GlobalLock, GlobalSize, GlobalUnlock},
        Ole::CF_UNICODETEXT,
    };
    // SAFETY: the clipboard is opened and closed here; the locked memory is bounded by GlobalSize.
    unsafe {
        if OpenClipboard(std::ptr::null_mut()) == 0 {
            return Err("the clipboard is in use by another program".into());
        }
        let handle = GetClipboardData(u32::from(CF_UNICODETEXT));
        let mut text = String::new();
        if !handle.is_null() {
            let max_units = GlobalSize(handle) / 2;
            let wide = GlobalLock(handle) as *const u16;
            if !wide.is_null() {
                let mut len = 0;
                while len < max_units && *wide.add(len) != 0 {
                    len += 1;
                }
                text = String::from_utf16_lossy(std::slice::from_raw_parts(wide, len));
                GlobalUnlock(handle);
            }
        }
        CloseClipboard();
        Ok(text)
    }
}

#[cfg(windows)]
async fn clipboard_write(text: &str) -> Result<(), String> {
    use windows_sys::Win32::{
        Foundation::GlobalFree,
        System::{
            DataExchange::{CloseClipboard, EmptyClipboard, OpenClipboard, SetClipboardData},
            Memory::{GlobalAlloc, GlobalLock, GlobalUnlock, GMEM_MOVEABLE},
            Ole::CF_UNICODETEXT,
        },
    };
    let wide: Vec<u16> = text.encode_utf16().chain([0]).collect();
    // SAFETY: the memory is sized for `wide`; the clipboard owns it once SetClipboardData takes it.
    unsafe {
        if OpenClipboard(std::ptr::null_mut()) == 0 {
            return Err("the clipboard is in use by another program".into());
        }
        EmptyClipboard();
        let memory = GlobalAlloc(GMEM_MOVEABLE, wide.len() * 2);
        let mut done = false;
        if !memory.is_null() {
            let to = GlobalLock(memory) as *mut u16;
            if !to.is_null() {
                std::ptr::copy_nonoverlapping(wide.as_ptr(), to, wide.len());
                GlobalUnlock(memory);
                done = !SetClipboardData(u32::from(CF_UNICODETEXT), memory).is_null();
            }
            if !done {
                GlobalFree(memory);
            }
        }
        CloseClipboard();
        if done {
            Ok(())
        } else {
            Err("the clipboard could not be set".into())
        }
    }
}

fn cut(text: String) -> String {
    if text.len() > MOST {
        text.chars().take(MOST).collect()
    } else {
        text
    }
}

impl Engine {
    /// Get Clipboard (99789) into a variable, or Set Clipboard (99790) to a text.
    pub(super) async fn clipboard_sub(self: &Rc<Self>, ctx: &mut Ctx, o: &Obj, kind: u64) {
        let result = if kind == 99790 {
            let value = self.expand(ctx, s(o, "text"));
            match crate::scenario::fakes::clipboard(Some(&value)) {
                Some(r) => r.map(|_| ()),
                None => clipboard_write(&value).await,
            }
        } else {
            let got = match crate::scenario::fakes::clipboard(None) {
                Some(r) => r,
                None => clipboard_read().await,
            };
            got.map(|got| {
                let var = self.simple(ctx, s(o, "variableName"));
                let var = if var.trim().is_empty() { "clipboard" } else { var.trim() };
                ctx.args.insert(var.to_string(), cut(got).into());
            })
        };
        if let Err(e) = result {
            crate::error(format!("Clipboard: {e}"));
        }
    }

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
                let amount = crate::fmt::parse_number(&text(ctx, "amount"))
                    .filter(|x| x.is_finite())
                    .unwrap_or(1.0);
                let now = self.global_number(&global, persisted);
                let next = match n(o, "operation") as u64 {
                    1 => now - amount,
                    2 => amount,
                    3 => 0.0,
                    _ => now + amount,
                };
                let next = if next.is_finite() { next } else { now };
                let value = crate::vars::number(next);
                self.put_global(&global, value.clone(), persisted);
                let var = name(ctx, "counter");
                ctx.args.insert(var, value);
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
            // List: a list kept as a global; `<name>`, `<name>Found` and `<name>Count`.
            99788 => {
                let global = text(ctx, "name").trim().to_string();
                if global.is_empty() {
                    crate::error("List: no name given".to_string());
                    return true;
                }
                let persisted = b(o, "persisted", true);
                let mut items: Vec<String> = match self.global_value(&global, persisted) {
                    Some(Value::Array(list)) => list
                        .iter()
                        .map(|v| v.as_str().map_or_else(|| v.to_string(), str::to_string))
                        .collect(),
                    _ => Vec::new(),
                };
                let op = n(o, "operation") as u64;
                let got = list_op(&mut items, op, &text(ctx, "value"));
                if matches!(op, 0 | 1 | 6 | 7) {
                    self.put_global(&global, serde_json::json!(items), persisted);
                }
                let var = name(ctx, "list");
                ctx.args.insert(format!("{var}Found"), got.is_some().into());
                ctx.args.insert(format!("{var}Count"), items.len().into());
                ctx.args.insert(var, got.unwrap_or_default().into());
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
    fn a_list_keeps_its_newest_thousand_and_counts_from_the_end() {
        let mut items: Vec<String> = Vec::new();
        for i in 0..1005 {
            list_op(&mut items, 0, &i.to_string());
        }
        assert_eq!(items.len(), 1000);
        assert_eq!(list_op(&mut items, 3, "0").as_deref(), Some("5"));
        assert_eq!(list_op(&mut items, 3, "-1").as_deref(), Some("1004"));
        assert_eq!(list_op(&mut items, 3, "-2000"), None);
        assert_eq!(list_op(&mut items, 1, "nope"), None);
        assert_eq!(list_op(&mut items, 5, "").as_deref(), Some("1000"));
    }

    /// Run by hand in a desktop session: it changes the real clipboard.
    #[test]
    #[ignore]
    fn the_system_clipboard_takes_and_gives_back_a_text() {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        runtime.block_on(async {
            clipboard_write("Streamerz clipboard check").await.unwrap();
            assert_eq!(clipboard_read().await.unwrap(), "Streamerz clipboard check");
        });
    }

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
        assert_eq!(json_path(&v, "a.b[0]junk"), None);
        assert_eq!(json_path(&v, "a.b[0"), None);
    }

    #[test]
    fn texts_change_and_durations_read_as_words() {
        assert_eq!(transform("abc", 0), "ABC");
        assert_eq!(transform("ABC", 1), "abc");
        assert_eq!(transform("heLLo wORLD", 2), "Hello World");
        assert_eq!(transform("  hi  ", 3), "hi");
        assert_eq!(transform("héllo", 4), "olléh");
        assert_eq!(transform("  a   b  ", 5), "a b");
        assert_eq!(transform("one two three", 6), "one");
        assert_eq!(transform("one two three", 7), "three");
        assert_eq!(transform("a b&c", 8), "a+b%26c");
        assert_eq!(transform("héllo", 9), "5");
        assert_eq!(duration_text(3723, 0), "1:02:03");
        assert_eq!(duration_text(59, 0), "0:59");
        assert_eq!(duration_text(3723, 1), "1h 2m 3s");
        assert_eq!(duration_text(0, 1), "0s");
        assert_eq!(duration_text(0, 2), "0 seconds");
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
        assert_eq!(gate("huge", u64::MAX, 1_000), 0);
        assert!(gate("huge", u64::MAX, 2_000) > 0);
    }

    #[test]
    fn list_operations_cover_membership_pop_clear_and_join() {
        let mut items = vec!["Alpha".into(), "Beta".into(), "Gamma".into()];
        assert_eq!(list_op(&mut items, 2, "beta").as_deref(), Some("Beta"));
        assert_eq!(list_op(&mut items, 2, "missing"), None);
        assert_eq!(list_op(&mut items, 1, "BETA").as_deref(), Some("Beta"));
        assert_eq!(list_op(&mut items, 8, "").as_deref(), Some("Alpha, Gamma"));
        assert_eq!(list_op(&mut items, 8, " | ").as_deref(), Some("Alpha | Gamma"));
        assert_eq!(list_op(&mut items, 7, "").as_deref(), Some("Alpha"));
        assert_eq!(list_op(&mut items, 4, "").as_deref(), Some("Gamma"));
        assert_eq!(list_op(&mut items, 6, ""), None);
        assert_eq!(list_op(&mut items, 5, "").as_deref(), Some("0"));
        assert_eq!(list_op(&mut items, 4, ""), None);
        assert_eq!(list_op(&mut items, 7, ""), None);
        assert_eq!(list_op(&mut items, 3, &i64::MIN.to_string()), None);
    }
}
