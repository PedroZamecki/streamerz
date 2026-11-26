//! The kinds of setting that hold more than a word or a number: days and times, places and sizes on
//! the page, animations, text styles, sounds and the things a channel has (events, rewards, gifts).
//! `Kind` (in `groups.rs`) asks here how such a value is checked, typed and shown.
use super::{is_color, parse_duration, Kind};
use serde_json::{json, Value};

/// Where a `position` is measured from: a corner, the middle of a side, or the center.
pub const ANCHORS: [&str; 9] = [
    "top-left",
    "top",
    "top-right",
    "left",
    "center",
    "right",
    "bottom-left",
    "bottom",
    "bottom-right",
];

/// The animations `streamerz.animate` knows.
pub const ANIMATIONS: [&str; 11] = [
    "none",
    "fade",
    "slide-up",
    "slide-down",
    "slide-left",
    "slide-right",
    "zoom",
    "pop",
    "bounce",
    "flip",
    "shake",
];

/// The easings offered in a list, by name, with their CSS. Any other CSS easing is kept as it is.
pub const EASINGS: [(&str, &str); 8] = [
    ("Linear", "linear"),
    ("Ease", "ease"),
    ("Ease in", "ease-in"),
    ("Ease out", "ease-out"),
    ("Ease in and out", "ease-in-out"),
    ("Back", "cubic-bezier(0.34, 1.56, 0.64, 1)"),
    ("Snappy", "cubic-bezier(0.2, 0.9, 0.1, 1)"),
    ("Steps", "steps(4, end)"),
];

/// The kinds of events an `event` setting picks from (what `streamerz.alerts` calls `kind`), with
/// their icon and the platforms that have them.
pub const EVENTS: [(&str, &str, &[&str]); 15] = [
    ("follow", "user-plus", &["twitch", "kick", "tiktok"]),
    ("sub", "star", &["twitch", "youtube", "kick", "tiktok"]),
    ("resub", "repeat", &["twitch", "youtube", "kick"]),
    ("giftsub", "gift", &["twitch", "kick"]),
    ("giftbomb", "party-popper", &["twitch", "youtube", "kick"]),
    ("cheer", "gem", &["twitch", "kick"]),
    ("raid", "swords", &["twitch"]),
    ("superchat", "badge-dollar-sign", &["youtube"]),
    ("supersticker", "sticker", &["youtube"]),
    ("member", "heart", &["youtube"]),
    ("gift", "gift", &["tiktok"]),
    ("share", "share-2", &["tiktok"]),
    ("reward", "coins", &["twitch", "kick"]),
    ("powerup", "zap", &["twitch"]),
    ("announcement", "megaphone", &["twitch"]),
];

pub fn is_event(kind: &str) -> bool {
    EVENTS.iter().any(|(name, ..)| *name == kind)
}

/// `YYYY-MM-DD`, a real day.
pub fn date(text: &str) -> Option<(i64, u32, u32)> {
    let mut parts = text.trim().splitn(3, '-');
    let (y, m, d) = (parts.next()?, parts.next()?, parts.next()?);
    if y.len() != 4
        || m.len() != 2
        || d.len() != 2
        || !y.bytes().all(|b| b.is_ascii_digit())
        || !m.bytes().all(|b| b.is_ascii_digit())
        || !d.bytes().all(|b| b.is_ascii_digit())
    {
        return None;
    }
    let (y, m, d): (i64, u32, u32) = (y.parse().ok()?, m.parse().ok()?, d.parse().ok()?);
    let leap = (y % 4 == 0 && y % 100 != 0) || y % 400 == 0;
    let days = [31, if leap { 29 } else { 28 }, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31];
    ((1..=12).contains(&m) && d >= 1 && d <= days[m as usize - 1]).then_some((y, m, d))
}

/// `H:MM` or `HH:MM`.
pub fn time(text: &str) -> Option<(u32, u32)> {
    let (h, m) = text.trim().split_once(':')?;
    if !(1..=2).contains(&h.len())
        || m.len() != 2
        || !h.bytes().all(|b| b.is_ascii_digit())
        || !m.bytes().all(|b| b.is_ascii_digit())
    {
        return None;
    }
    let (h, m): (u32, u32) = (h.parse().ok()?, m.parse().ok()?);
    (h < 24 && m < 60).then_some((h, m))
}

/// Days since 1970-01-01 for a civil date (Howard Hinnant's algorithm).
fn days_from_civil(y: i64, m: u32, d: u32) -> i64 {
    let (m, d) = (i64::from(m), i64::from(d));
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let doy = (153 * (m + if m > 2 { -3 } else { 9 }) + 2) / 5 + d - 1;
    era * 146_097 + yoe * 365 + yoe / 4 - yoe / 100 + doy - 719_468
}

fn now_ms() -> u64 {
    crate::fmt::unix_now().as_millis() as u64
}

/// A local day and time as milliseconds since 1970.
fn local_ms(day: (i64, u32, u32), hour: u32, minute: u32) -> Option<u64> {
    let naive = days_from_civil(day.0, day.1, day.2) * 86_400 + i64::from(hour * 3600 + minute * 60);
    let secs = crate::fmt::from_local(naive);
    u64::try_from(secs).ok().map(|s| s * 1000)
}

/// Today in the local time zone, `days` from now, as `YYYY-MM-DD`.
pub fn day_from_today(days: i64) -> String {
    let t = crate::fmt::local_time((now_ms() / 1000) as i64 + days * 86_400);
    format!("{:04}-{:02}-{:02}", t.year, t.month, t.day)
}

/// A moment as the form shows it: the local `YYYY-MM-DD HH:MM` (empty for none).
pub fn show_moment(ms: u64) -> String {
    if ms == 0 {
        return String::new();
    }
    let t = crate::fmt::local_time((ms / 1000) as i64);
    let day = format!("{:04}-{:02}-{:02} {:02}:{:02}", t.year, t.month, t.day, t.hour, t.min);
    // The seconds only when it has some (a moment set as `+15m`), so it reads back the same.
    if t.sec == 0 {
        day
    } else {
        format!("{day}:{:02}", t.sec)
    }
}

/// A moment typed: `YYYY-MM-DD HH:MM` (local), `+15m` (from now), `20:00` (the next one), a number of
/// milliseconds, or nothing (none).
pub fn moment(text: &str) -> Option<u64> {
    let t = text.trim();
    if t.is_empty() {
        return Some(0);
    }
    if let Some(after) = t.strip_prefix('+') {
        return parse_duration(after)
            .and_then(|ms| now_ms().checked_add(ms))
            .map(|ms| ms / 1000 * 1000);
    }
    if let Some((h, m)) = time(t) {
        let today = date(&day_from_today(0))?;
        let at = local_ms(today, h, m)?;
        return Some(if at <= now_ms() { at + 86_400_000 } else { at });
    }
    if let Some((day, clock)) = t.split_once([' ', 'T']) {
        let clock = clock.trim();
        let parts: Vec<&str> = clock.split(':').collect();
        let (hm, sec) = match parts[..] {
            [_, _] => (clock, 0u64),
            [h, m, s] => {
                if s.len() != 2 || !s.bytes().all(|b| b.is_ascii_digit()) {
                    return None;
                }
                let sec = s.parse::<u64>().ok().filter(|&s| s < 60)?;
                (&clock[..h.len() + 1 + m.len()], sec)
            }
            _ => return None,
        };
        let (h, m) = time(hm)?;
        return local_ms(date(day)?, h, m).and_then(|ms| ms.checked_add(sec * 1000));
    }
    if let Some(day) = date(t) {
        return local_ms(day, 0, 0);
    }
    t.parse::<u64>().ok()
}

/// A typed value of the kinds kept here; `None` for the kinds `Kind::parse` reads itself.
pub fn parse(kind: Kind, t: &str) -> Option<Result<Value, String>> {
    let json = |what: &str| {
        serde_json::from_str::<Value>(t)
            .map_err(|_| format!("not {what} (JSON)"))
            .and_then(|v| {
                if kind.accepts(&v) {
                    Ok(fill(kind, &v))
                } else {
                    Err(format!("not {what}"))
                }
            })
    };
    Some(match kind {
        Kind::Date => match t.to_ascii_lowercase().as_str() {
            "" => Ok(json!("")),
            "today" => Ok(day_from_today(0).into()),
            "tomorrow" => Ok(day_from_today(1).into()),
            _ => date(t)
                .map(|_| Value::from(t))
                .ok_or_else(|| "not a day (YYYY-MM-DD)".into()),
        },
        Kind::Time => match time(t) {
            Some((h, m)) => Ok(format!("{h:02}:{m:02}").into()),
            None if t.is_empty() => Ok(json!("")),
            None => Err("not a time of day (HH:MM)".into()),
        },
        Kind::DateTime => moment(t)
            .map(Value::from)
            .ok_or_else(|| "not a day and time (YYYY-MM-DD HH:MM)".into()),
        Kind::Percent => crate::fmt::parse_number(t.trim_end_matches('%').trim())
            .filter(|n| (0.0..=100.0).contains(n))
            .map(tidy)
            .ok_or_else(|| "a number from 0 to 100".into()),
        Kind::Range => {
            if t.starts_with('[') {
                return Some(json("two numbers"));
            }
            let cut = t.find("..").map(|at| (at, 2)).or_else(|| {
                t.char_indices()
                    .skip(1)
                    .find(|(_, c)| matches!(c, '-' | ','))
                    .map(|(at, _)| (at, 1))
            });
            let Some((at, len)) = cut else {
                return Some(Err("two numbers: from - to".into()));
            };
            let number = |s: &str| crate::fmt::parse_number(s.trim()).filter(|n| n.is_finite());
            match (number(&t[..at]), number(&t[at + len..])) {
                (Some(a), Some(b)) => Ok(json!([tidy(a.min(b)), tidy(a.max(b))])),
                _ => Err("two numbers: from - to".into()),
            }
        }
        Kind::Size if !t.starts_with('{') => {
            let (w, h) = t.split_once(['x', 'X', '*', ',']).unwrap_or((t, ""));
            match (w.trim().parse::<u64>(), h.trim().parse::<u64>()) {
                (Ok(width), Ok(height)) => Ok(json!({"width": width, "height": height})),
                _ => Err("a size: width x height".into()),
            }
        }
        Kind::Event => {
            let named: Vec<String> = t
                .split(|c: char| c == ',' || c.is_whitespace())
                .filter(|k| !k.is_empty())
                .map(str::to_ascii_lowercase)
                .collect();
            match named.iter().find(|k| !is_event(k)) {
                Some(unknown) => Err(format!("{unknown} is not a kind of event")),
                None => Ok(json!(named)),
            }
        }
        Kind::Position => json("a position"),
        Kind::Size => json("a size"),
        Kind::Animation => json("an animation"),
        Kind::TextStyle => json("a text style"),
        Kind::Sound => json("a sound"),
        Kind::Reward => json("a reward"),
        Kind::Gift => json("a gift"),
        Kind::Json | Kind::Hidden => serde_json::from_str(t).map_err(|e| format!("not JSON: {e}")),
        Kind::Button => Ok(Value::Null),
        Kind::Easing | Kind::Gradient | Kind::UserGroup | Kind::Scene | Kind::Source => Ok(t.into()),
        _ => return None,
    })
}

/// A whole number when the number is one.
pub fn tidy(n: f64) -> Value {
    if n.fract() == 0.0 && n.abs() < 1e15 {
        json!(n as i64)
    } else {
        json!((n * 1e6).round() / 1e6)
    }
}

/// What an object of each kind holds when nothing is set: a value missing some of these gets them.
pub fn blank(kind: Kind) -> Value {
    match kind {
        Kind::Position => json!({"anchor": "center", "x": 0, "y": 0}),
        Kind::Size => json!({"width": 400, "height": 300}),
        Kind::Animation => json!({"name": "fade", "durationMs": 400, "delayMs": 0, "easing": "ease-out"}),
        Kind::TextStyle => json!({"font": "", "size": 32, "weight": 700, "color": "#ffffff", "outline": 0,
            "outlineColor": "#000000", "shadow": true}),
        Kind::Sound => json!({"file": "", "volume": 100}),
        Kind::Reward | Kind::Gift => json!({}),
        _ => Value::Null,
    }
}

/// The value with what it leaves out taken from [`blank`].
pub fn fill(kind: Kind, v: &Value) -> Value {
    match (blank(kind), v) {
        (Value::Object(mut base), Value::Object(given)) => {
            base.extend(given.clone());
            Value::Object(base)
        }
        _ => v.clone(),
    }
}

/// Whether an object's fields, those it has, are of the right type (missing ones take [`blank`]'s).
pub fn object_fits(kind: Kind, v: &Value) -> bool {
    let Some(o) = v.as_object() else { return false };
    let number = |k: &str| o.get(k).is_none_or(Value::is_number);
    let whole = |k: &str| o.get(k).is_none_or(|n| n.as_u64().is_some());
    let text = |k: &str| o.get(k).is_none_or(Value::is_string);
    let color = |k: &str| o.get(k).is_none_or(|c| c.as_str().is_some_and(is_color));
    match kind {
        Kind::Position => {
            o.get("anchor")
                .is_none_or(|a| a.as_str().is_some_and(|a| ANCHORS.contains(&a)))
                && number("x")
                && number("y")
        }
        Kind::Size => whole("width") && whole("height"),
        Kind::Animation => {
            o.get("name")
                .is_none_or(|n| n.as_str().is_some_and(|n| ANIMATIONS.contains(&n)))
                && whole("durationMs")
                && whole("delayMs")
                && text("easing")
        }
        Kind::TextStyle => {
            text("font")
                && number("size")
                && whole("weight")
                && color("color")
                && number("outline")
                && color("outlineColor")
                && o.get("shadow").is_none_or(Value::is_boolean)
        }
        Kind::Sound => {
            text("file")
                && o.get("volume")
                    .is_none_or(|n| n.as_f64().is_some_and(|n| (0.0..=100.0).contains(&n)))
        }
        _ => false,
    }
}

/// A gradient's angle and colors, when it is one this form wrote: `linear-gradient(90deg, #a, #b)`.
pub fn gradient_parts(css: &str) -> Option<(f64, Vec<String>)> {
    let inner = css.trim().strip_prefix("linear-gradient(")?.strip_suffix(')')?;
    let mut parts = inner.split(',').map(str::trim);
    let angle = parts
        .next()?
        .strip_suffix("deg")?
        .trim()
        .parse::<f64>()
        .ok()
        .filter(|a| a.is_finite())?;
    let colors: Vec<String> = parts.map(str::to_owned).collect();
    (colors.len() >= 2 && colors.iter().all(|c| is_color(c))).then_some((angle, colors))
}

pub fn gradient_css(angle: f64, colors: &[String]) -> String {
    let angle = if angle.is_finite() {
        angle.rem_euclid(360.0)
    } else {
        0.0
    };
    let angle = tidy(angle);
    format!("linear-gradient({angle}deg, {})", colors.join(", "))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn days_times_and_moments_are_read_and_checked() {
        assert_eq!(date("2026-02-29"), None);
        assert_eq!(date("2028-02-29"), Some((2028, 2, 29)));
        assert_eq!(date("2026-13-01"), None);
        assert_eq!(time("9:05"), Some((9, 5)));
        assert_eq!(time("24:00"), None);
        assert_eq!(Kind::Time.parse("9:05").unwrap(), json!("09:05"));
        assert_eq!(Kind::Date.parse("today").unwrap(), json!(day_from_today(0)));
        assert!(Kind::Date.parse("soon").is_err());
        // A day and time typed in the local zone comes back as typed.
        let at = moment("2030-05-04 20:15").unwrap();
        assert_eq!(show_moment(at), "2030-05-04 20:15");
        let soon = moment("+15m").unwrap();
        assert!((soon as i64 - now_ms() as i64 - 900_000).abs() < 2_000);
        // Shown with its seconds, it reads back to the same second.
        let whole = soon / 1000 * 1000;
        assert_eq!(moment(&show_moment(whole)), Some(whole));
        assert_eq!(moment("2030-05-04 20:15:09"), Some(at + 9_000));
        // A time of day alone is the next one: never in the past.
        assert!(moment("00:00").unwrap() > now_ms());
        assert_eq!(moment(""), Some(0));
        assert!(Kind::DateTime.accepts(&json!(at)) && !Kind::DateTime.accepts(&json!("x")));
    }

    #[test]
    fn moments_keep_the_offset_of_their_own_day() {
        // Winter and summer: each uses its own offset, not today's.
        for (typed, day) in [("2030-01-15 12:00", (2030, 1, 15)), ("2030-07-15 12:00", (2030, 7, 15))] {
            let naive = days_from_civil(day.0, day.1, day.2) * 86_400 + 12 * 3600;
            let at = moment(typed).unwrap();
            assert_eq!(at as i64, crate::fmt::from_local(naive) * 1000);
            assert_eq!(show_moment(at), typed);
        }
    }

    #[test]
    fn ranges_percents_sizes_and_events_are_typed() {
        assert_eq!(Kind::Range.parse("10 - 20").unwrap(), json!([10, 20]));
        assert_eq!(Kind::Range.parse("20..10").unwrap(), json!([10, 20]));
        assert_eq!(Kind::Range.parse("[1, 2]").unwrap(), json!([1, 2]));
        assert!(Kind::Range.parse("ten").is_err());
        assert_eq!(Kind::Percent.parse("45%").unwrap(), json!(45));
        assert!(Kind::Percent.parse("140").is_err());
        assert_eq!(
            Kind::Size.parse("1920x1080").unwrap(),
            json!({"width": 1920, "height": 1080})
        );
        assert_eq!(Kind::Event.parse("follow, raid").unwrap(), json!(["follow", "raid"]));
        assert!(Kind::Event.parse("follow, hug").is_err());
        assert_eq!(Kind::Json.parse("[1, {\"a\": 2}]").unwrap(), json!([1, {"a": 2}]));
    }

    #[test]
    fn objects_take_what_they_leave_out() {
        let partial = Kind::Position.parse(r#"{"anchor": "top-left", "x": 20}"#).unwrap();
        assert_eq!(partial, json!({"anchor": "top-left", "x": 20, "y": 0}));
        assert!(Kind::Position.parse(r#"{"anchor": "middle"}"#).is_err());
        assert!(Kind::Animation.accepts(&json!({"name": "pop", "durationMs": 300})));
        assert!(!Kind::Animation.accepts(&json!({"name": "wobble"})));
        assert!(Kind::TextStyle.accepts(&blank(Kind::TextStyle)));
        assert!(!Kind::TextStyle.accepts(&json!({"color": "red"})));
        assert!(!Kind::Sound.accepts(&json!({"volume": 140})));
    }

    #[test]
    fn a_gradient_goes_to_css_and_back() {
        let css = gradient_css(450.0, &["#ff0000".into(), "#0000ff".into()]);
        assert_eq!(css, "linear-gradient(90deg, #ff0000, #0000ff)");
        assert_eq!(
            gradient_parts(&css),
            Some((90.0, vec!["#ff0000".into(), "#0000ff".into()]))
        );
        assert_eq!(gradient_parts("radial-gradient(red, blue)"), None);
        assert_eq!(gradient_parts("linear-gradient(NaNdeg, #ff0000, #0000ff)"), None);
        assert_eq!(gradient_parts("linear-gradient(infdeg, #ff0000, #0000ff)"), None);
        assert_eq!(
            gradient_css(f64::NAN, &["#ff0000".into(), "#0000ff".into()]),
            "linear-gradient(0deg, #ff0000, #0000ff)"
        );
    }

    #[test]
    fn strict_date_time_single_digit_hour_seconds_and_finite_ranges() {
        assert_eq!(date("+202-01-01"), None);
        assert_eq!(date("2026-+1-01"), None);
        assert_eq!(time("+1:05"), None);
        assert_eq!(time("09:+5"), None);
        assert_eq!(time("9:5"), None);
        // Single-digit hour with seconds parses cleanly.
        let base = moment("2030-05-04 09:15").unwrap();
        assert_eq!(moment("2030-05-04 9:15:30"), Some(base + 30_000));
        assert_eq!(moment("2030-05-04 9:15:5"), None);
        // Non-finite values in ranges are refused.
        assert!(Kind::Range.parse("inf - 10").is_err());
        assert!(Kind::Range.parse("NaN - 10").is_err());
    }
}
