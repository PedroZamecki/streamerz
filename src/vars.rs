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
        "unixtime" => fmt::now_secs().to_string(),
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

/// Replace `%name%` and `~global~` in a single pass so values substituted from one cannot trigger
/// substitution of the other. Unknown names stay, and their closing mark can open the next one
/// ("50% off %user%" works).
pub fn simple(s: &str, args: &Obj, globals: &Obj) -> String {
    if !s.contains(['%', '~']) {
        return s.into();
    }
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(start) = rest.find(['%', '~']) {
        out += &rest[..start];
        let mark = rest.as_bytes()[start] as char;
        let after = &rest[start + 1..];
        match after.find(mark) {
            Some(end) if end > 0 => {
                let inner = &after[..end];
                let found = if mark == '%' {
                    arg(inner, args)
                } else {
                    globals.get(inner).map(text)
                };
                match found {
                    Some(v) => {
                        out += &v;
                        rest = &after[end + 1..];
                    }
                    None => {
                        out.push(mark);
                        rest = after;
                    }
                }
            }
            _ => {
                out.push(mark);
                rest = after;
            }
        }
    }
    out + rest
}

const INLINE_FUNCS: [&str; 3] = ["math(", "length(", "parse("];
const MAX_FUNC_DEPTH: usize = 16;

/// Finds the `)$` that closes the inline function starting right after `$name(`, skipping over
/// nested `$math(`, `$length(` and `$parse(` calls.
fn find_func_end(after_open: &str) -> Option<usize> {
    let bytes = after_open.as_bytes();
    let mut depth = 0usize;
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'$' {
            let tail = &after_open[i + 1..];
            if let Some(f) = INLINE_FUNCS.iter().find(|f| tail.starts_with(*f)) {
                depth += 1;
                i += 1 + f.len();
                continue;
            }
        } else if bytes[i] == b')' && bytes.get(i + 1) == Some(&b'$') {
            if depth == 0 {
                return Some(i);
            }
            depth -= 1;
            i += 2;
            continue;
        }
        i += 1;
    }
    None
}

fn expand_funcs(s: &str, args: &Obj, depth: usize) -> String {
    if depth >= MAX_FUNC_DEPTH || !s.contains('$') {
        return s.into();
    }
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(start) = rest.find('$') {
        out += &rest[..start];
        let after = &rest[start + 1..];
        let call = INLINE_FUNCS
            .into_iter()
            .find(|f| after.starts_with(f))
            .and_then(|f| Some((f, find_func_end(&after[f.len()..])?)));
        match call {
            Some((f, end)) => {
                let raw_inner = &after[f.len()..f.len() + end];
                let inner = expand_funcs(raw_inner, args, depth + 1);
                out += &match f {
                    "math(" => {
                        let v = crate::math::eval(&inner);
                        if v.is_nan() {
                            crate::warn(format!("$math({inner})$: not a valid expression"));
                        }
                        fmt::general(v)
                    }
                    "length(" => inner.encode_utf16().count().to_string(),
                    _ => args.get(inner.as_str()).map(text).unwrap_or_default(),
                };
                rest = &after[f.len() + end + 2..];
            }
            None => {
                out.push('$');
                rest = after;
            }
        }
    }
    out + rest
}

/// Everything: arguments, persisted globals, then inline functions.
pub fn expand(s: &str, args: &Obj, globals: &Obj) -> String {
    let s = simple(s, args, globals);
    expand_funcs(&s, args, 0)
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
        assert_eq!(auto_type("false"), json!(false));
        assert_eq!(auto_type("hello"), json!("hello"));
        assert_eq!(auto_type("1e5"), json!(100000.0));
        assert_eq!(auto_type("NaN"), json!("NaN"));
        assert_eq!(auto_type("inf"), json!("inf"));
        assert_eq!(auto_type(""), json!(""));
        assert_eq!(text(&json!(1e20)), "1E+20");
        assert_eq!(number(42.0), json!(42));
        assert_eq!(number(-0.0), json!(0));
        assert_eq!(number(1.5), json!(1.5));
        assert_eq!(number(f64::NAN), Value::Null);
        assert_eq!(number(f64::INFINITY), Value::Null);
    }

    #[test]
    fn single_pass_prevents_global_leak_and_supports_nested_functions() {
        let args = obj(json!({
            "rawInput": "look ~secret~",
            "user": "Ana",
            "expr": "$math(2+2)$",
            "emoji": "😀"
        }));
        let globals = obj(json!({"secret": "top-secret", "other": "%user%"}));
        assert_eq!(simple("said: %rawInput%", &args, &globals), "said: look ~secret~");
        assert_eq!(simple("~other~ %user%", &args, &globals), "%user% Ana");
        assert_eq!(simple("50% off ~secret~", &args, &globals), "50% off top-secret");
        assert_eq!(simple("%% %user% ~~ ~secret~", &args, &globals), "%% Ana ~~ top-secret");
        assert_eq!(expand("$math($length(hello)$ * 2)$", &args, &globals), "10");
        assert_eq!(expand("$length($math(10+10)$)$", &args, &globals), "2");
        assert_eq!(expand("$parse(expr)$", &args, &globals), "$math(2+2)$");
        assert_eq!(expand("$parse(missing)$", &args, &globals), "");
        // UTF-16 code units, as .NET String.Length counts them.
        assert_eq!(expand("$length(%emoji%)$", &args, &globals), "2");
        assert_eq!(expand("$math(1 + 2", &args, &globals), "$math(1 + 2");
        assert_eq!(expand("$$math(1+1)$", &args, &globals), "$2");
        assert_eq!(expand("%date:%", &args, &globals), "%date:%");
    }
}
