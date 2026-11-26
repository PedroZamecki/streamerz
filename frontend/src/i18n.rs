//! The core supplies the active translation catalog once at renderer startup.

use std::collections::HashMap;
use std::sync::OnceLock;

static CATALOG: OnceLock<HashMap<String, String>> = OnceLock::new();

pub fn init(value: serde_json::Value) -> Result<(), String> {
    let catalog = serde_json::from_value(value).map_err(|error| format!("invalid translation catalog: {error}"))?;
    CATALOG
        .set(catalog)
        .map_err(|_| "translation catalog already installed".into())
}

pub fn tr(key: &'static str) -> &'static str {
    CATALOG
        .get()
        .and_then(|catalog| catalog.get(key))
        .map_or(key, String::as_str)
}

pub fn trf(key: &'static str, vars: &[(&str, &str)]) -> String {
    fill(tr(key), vars)
}

/// Puts each `{name}` of `text` in place, in one pass: a value that holds a `{name}` of its own (a
/// chat line, an error from a server) stays as it is.
fn fill(text: &str, vars: &[(&str, &str)]) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(open) = rest.find('{') {
        out.push_str(&rest[..open]);
        let after = &rest[open + 1..];
        let value = after.find('}').and_then(|close| {
            let name = &after[..close];
            vars.iter()
                .find(|(key, _)| *key == name)
                .map(|(_, value)| (*value, close))
        });
        match value {
            Some((value, close)) => {
                out.push_str(value);
                rest = &after[close + 1..];
            }
            None => {
                out.push('{');
                rest = after;
            }
        }
    }
    out.push_str(rest);
    out
}

#[cfg(test)]
mod tests {
    use super::fill;

    #[test]
    fn placeholders_are_filled_once_and_values_are_kept_as_they_are() {
        assert_eq!(
            fill("{who} said {what}", &[("who", "{what}"), ("what", "hi")]),
            "{what} said hi"
        );
        assert_eq!(fill("{n} of {n}", &[("n", "2")]), "2 of 2");
    }

    #[test]
    fn unknown_or_unclosed_braces_stay_in_the_text() {
        assert_eq!(fill("{other} {name", &[("name", "x")]), "{other} {name");
        assert_eq!(fill("a {} b {{x}}", &[("x", "1")]), "a {} b {1}");
        assert_eq!(fill("café {é}", &[("é", "ü")]), "café ü");
        assert_eq!(fill("", &[]), "");
    }
}
