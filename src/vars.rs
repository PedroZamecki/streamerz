//! Variables in sub-action text, as in Streamer.bot: `%argument%` and `%argument:format%` (.NET
//! format strings, see fmt.rs), `~persistedGlobal~`, the generic ones (`%date%`, `%time%`, `%lf%`, ...)
//! and the inline functions `$math()$`, `$length()$` and `$parse()$`. Unknown names stay as they are.

use crate::fmt::{self, Num};
use crate::model::Obj;
use serde_json::Value;

/// A value as .NET's ToString() writes it (True/False, 15-digit doubles, nothing for null).
pub fn text(v: &Value) -> String {
    match v {
        Value::Null => String::new(),
        Value::Bool(b) => if *b { "True" } else { "False" }.into(),
        Value::Number(n) => match num(v) {
            Some(Num::Int(i)) => i.to_string(),
            Some(Num::Float(f)) => fmt::general(f),
            None => n.to_string(),
        },
        Value::String(s) => s.clone(),
        v => v.to_string(),
    }
}

/// A JSON number as an integer or a double.
pub fn num(v: &Value) -> Option<Num> {
    let n = v.as_number()?;
    Some(match n.as_i64() {
        Some(i) => Num::Int(i),
        None => Num::Float(n.as_f64()?),
    })
}

/// Auto Type: "true"/"false" become booleans, numbers numbers ("007" is 7), the rest stays text.
pub fn auto_type(s: &str) -> Value {
    if s.eq_ignore_ascii_case("true") || s.eq_ignore_ascii_case("false") {
        return Value::Bool(s.eq_ignore_ascii_case("true"));
    }
    if let Ok(i) = s.parse::<i64>() {
        return i.into();
    }
    let numeric = !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit() || b"+-.eE".contains(&b));
    match s.parse::<f64>() {
        Ok(f) if numeric && f.is_finite() => f.into(),
        _ => Value::String(s.into()),
    }
}

/// A float or int as a JSON value.
pub fn number(f: f64) -> Value {
    if f.fract() == 0.0 && f.abs() < 9e15 {
        (f as i64).into()
    } else {
        serde_json::Number::from_f64(f).map_or(Value::Null, Value::Number)
    }
}

/// The generic variables, available everywhere.
fn generic(name: &str, format: Option<&str>) -> Option<String> {
    let t = || fmt::now();
    Some(match name {
        "date" => fmt::date(&t(), format.unwrap_or("yyyy/MM/dd")),
        "time" => fmt::date(&t(), format.unwrap_or("hh:mm")),
        "longtime" => fmt::date(&t(), format.unwrap_or("hh:mm:ss")),
        "filedatetime" => fmt::date(&t(), format.unwrap_or("yyyyMMdd.hhmmss")),
        "unixtime" => fmt::unix_now().as_secs().to_string(),
        "lf" => "\r\n".into(),
        _ => return None,
    })
}

/// One `%name%` or `%name:format%`, or None when there's no such variable.
fn arg(inner: &str, args: &Obj) -> Option<String> {
    let (name, format) = match inner.split_once(':') {
        Some((n, f)) if !f.is_empty() => (n, Some(f)),
        _ => (inner, None),
    };
    if let Some(v) = args.get(name) {
        return Some(match (format, num(v)) {
            (Some(f), Some(n)) => fmt::number(n, f).unwrap_or_else(|| text(v)),
            _ => text(v),
        });
    }
    generic(name, format)
}

/// Replace `open name close` everywhere `look` finds the name; unknown names stay, and their closing
/// mark can open the next one ("50% off %user%" works).
fn replace(s: &str, mark: char, look: impl Fn(&str) -> Option<String>) -> String {
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(start) = rest.find(mark) {
        out += &rest[..start];
        let after = &rest[start + 1..];
        match after.find(mark) {
            Some(end) if end > 0 => match look(&after[..end]) {
                Some(v) => {
                    out += &v;
                    rest = &after[end + 1..];
                }
                None => {
                    out.push(mark);
                    rest = after;
                }
            },
            _ => {
                out.push(mark);
                rest = after;
            }
        }
    }
    out + rest
}

/// Arguments and persisted globals only (what `$parse()$` and names of things get).
pub fn simple(s: &str, args: &Obj, globals: &Obj) -> String {
    if !s.contains(['%', '~']) {
        return s.into();
    }
    let s = replace(s, '%', |n| arg(n, args));
    replace(&s, '~', |n| globals.get(n).map(text))
}

/// Everything: arguments, persisted globals, then inline functions.
pub fn expand(s: &str, args: &Obj, globals: &Obj) -> String {
    let s = simple(s, args, globals);
    if !s.contains('$') {
        return s;
    }
    let mut out = String::with_capacity(s.len());
    let mut rest = s.as_str();
    while let Some(start) = rest.find('$') {
        out += &rest[..start];
        let after = &rest[start + 1..];
        let call = ["math(", "length(", "parse("]
            .into_iter()
            .find(|f| after.starts_with(f))
            .and_then(|f| Some((f, after.find(")$")?)));
        match call {
            Some((f, end)) if end >= f.len() => {
                let inner = &after[f.len()..end];
                out += &match f {
                    "math(" => {
                        let v = crate::math::eval(inner);
                        if v.is_nan() {
                            crate::warn(format!("$math({inner})$: not a valid expression"));
                        }
                        fmt::general(v)
                    }
                    "length(" => inner.encode_utf16().count().to_string(),
                    _ => args.get(inner).map(text).unwrap_or_default(),
                };
                rest = &after[end + 2..];
            }
            _ => {
                out.push('$');
                rest = after;
            }
        }
    }
    out + rest
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn obj(v: Value) -> Obj {
        v.as_object().unwrap().clone()
    }

    #[test]
    fn like_streamerbot() {
        let args = obj(json!({"user": "Ana", "n": 42, "x": 10, "f": 0.5, "b": true, "z": null,
            "line0": "first", "line1": "second", "i": 1, "s": "hello"}));
        let globals = obj(json!({"count": 7}));
        let e = |s: &str| expand(s, &args, &globals);
        assert_eq!(e("Hi %user%!"), "Hi Ana!");
        assert_eq!(e("%nope% and %user%"), "%nope% and Ana");
        assert_eq!(e("50% off for %user%"), "50% off for Ana");
        assert_eq!(e("100%"), "100%");
        assert_eq!(e("%n:D5%|%n:X%|%s:x%|%b%|%z%|%f%"), "00042|2A|hello|True||0.5");
        assert_eq!(e("~count~ ~nope~"), "7 ~nope~");
        assert_eq!(e("$math(%x% * 1.25)$"), "12.5");
        assert_eq!(e("$math(10+10)$ $math(1/3)$ $math(abc)$"), "20 0.333333333333333 NaN");
        assert_eq!(e("$length(hello)$ $length(%user%)$"), "5 3");
        assert_eq!(e("$parse(line%i%)$ $parse(user)$"), "second Ana");
        assert_eq!(e("costs $5 (or $math(2+3)$)"), "costs $5 (or 5)");
        assert_eq!(e("a%lf%b"), "a\r\nb");
        assert_eq!(e("%unixtime%").len(), 10);
        assert_eq!(e("%date%").len(), 10);
    }

    #[test]
    fn auto_types() {
        assert_eq!(auto_type("007"), json!(7));
        assert_eq!(auto_type("1234.5678"), json!(1234.5678));
        assert_eq!(auto_type("True"), json!(true));
        assert_eq!(auto_type("hello"), json!("hello"));
        assert_eq!(auto_type("1e5"), json!(100000.0));
        assert_eq!(auto_type("NaN"), json!("NaN"));
        assert_eq!(auto_type(""), json!(""));
        assert_eq!(text(&json!(1e20)), "1E+20");
    }
}
