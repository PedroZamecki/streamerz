//! Translations: every text the user reads goes through `tr("some.key")`. The words live in
//! `assets/locales/<code>.json` (flat, key to text), built into the binary; a key a language lacks falls
//! back to `en_US`, and a key nobody has shows as itself. Only `en_US` for now. To add a language: drop
//! its file next to the English one and add a line to `LOCALES`; the tests check it has every key.

use std::collections::HashMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::OnceLock;

/// The language the keys are written in, and the fallback of all others (the first of `LOCALES`).
pub const DEFAULT: &str = "en_US";

/// A language: its code (Settings stores it), its name in itself, and its texts.
struct Locale {
    code: &'static str,
    name: &'static str,
    json: &'static str,
}

const LOCALES: [Locale; 1] = [Locale {
    code: DEFAULT,
    name: "English (United States)",
    json: include_str!("../assets/locales/en_US.json"),
}];

/// The language in use (an index of `LOCALES`).
static ACTIVE: AtomicUsize = AtomicUsize::new(0);
/// Each language's texts, read the first time they're needed.
static CATALOGS: [OnceLock<HashMap<String, String>>; LOCALES.len()] = [const { OnceLock::new() }; LOCALES.len()];

fn catalog(i: usize) -> &'static HashMap<String, String> {
    CATALOGS[i].get_or_init(|| serde_json::from_str(LOCALES[i].json).unwrap_or_default())
}

/// Picks the language by its code; one this build doesn't have means `en_US`.
pub fn set(code: &str) {
    let i = LOCALES.iter().position(|l| l.code == code).unwrap_or(0);
    ACTIVE.store(i, Ordering::Relaxed);
}

/// The languages to choose from: code and name.
pub fn all() -> impl Iterator<Item = (&'static str, &'static str)> {
    LOCALES.iter().map(|l| (l.code, l.name))
}

/// The text of `key` in the language in use.
pub fn tr(key: &'static str) -> &'static str {
    let i = ACTIVE.load(Ordering::Relaxed);
    catalog(i)
        .get(key)
        .or_else(|| catalog(0).get(key))
        .map_or(key, String::as_str)
}

/// The text of `key` with each `{name}` in it replaced by its value in `args`.
#[allow(dead_code)]
pub fn trf(key: &'static str, args: &[(&str, &str)]) -> String {
    args.iter().fold(tr(key).to_string(), |s, (name, value)| {
        s.replace(&format!("{{{name}}}"), value)
    })
}

/// Which common button a label is (`add` for the `btn.add` key, in any language), if it is one: the
/// icon and the behaviour of a button follow this id, never its words. A trailing "..." or "…" is ignored.
pub fn button_id(label: &str) -> Option<&'static str> {
    let label = label.trim_end_matches(['.', '…']);
    let find = |i: usize| {
        catalog(i)
            .iter()
            .find(|(k, v)| k.starts_with("btn.") && v.as_str() == label)
            .map(|(k, _)| &k["btn.".len()..])
    };
    find(ACTIVE.load(Ordering::Relaxed)).or_else(|| find(0))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;
    use std::path::Path;

    fn sources(dir: &Path, out: &mut Vec<String>) {
        for e in std::fs::read_dir(dir).unwrap().flatten() {
            let p = e.path();
            if p.is_dir() {
                sources(&p, out);
            } else if p.extension().is_some_and(|x| x == "rs") && !p.ends_with("i18n.rs") {
                out.push(std::fs::read_to_string(p).unwrap());
            }
        }
    }

    #[test]
    fn every_language_is_valid_and_has_every_key_of_the_english_one() {
        assert_eq!(LOCALES[0].code, DEFAULT);
        let keys = |i| catalog(i).keys().cloned().collect::<BTreeSet<_>>();
        for (i, l) in LOCALES.iter().enumerate() {
            let parsed = serde_json::from_str::<HashMap<String, String>>(l.json);
            assert!(parsed.is_ok(), "{}.json is not a flat key to text object", l.code);
            assert_eq!(keys(i), keys(0), "{}.json and en_US.json differ in keys", l.code);
        }
    }

    #[test]
    fn keys_used_in_the_code_exist_and_keys_in_the_files_are_used() {
        let mut files = Vec::new();
        sources(&Path::new(env!("CARGO_MANIFEST_DIR")).join("src"), &mut files);
        let code = files.join("\n");
        // Every `tr("key")` call (not `str("`, `attr("`...) names a key the English file has.
        for (at, _) in code.match_indices("tr(\"") {
            if code[..at].ends_with(|c: char| c.is_alphanumeric() || c == '_') {
                continue;
            }
            let key = code[at + 4..].split('"').next().unwrap();
            assert!(catalog(0).contains_key(key), "tr(\"{key}\") is not in en_US.json");
        }
        for key in catalog(0).keys() {
            if !key.starts_with("btn.") {
                assert!(
                    code.contains(&format!("\"{key}\"")),
                    "\"{key}\" is in en_US.json but unused"
                );
            }
        }
    }

    #[test]
    fn button_words_are_unique_so_the_id_is_certain() {
        for (i, l) in LOCALES.iter().enumerate() {
            let words: Vec<&String> = catalog(i)
                .iter()
                .filter(|(k, _)| k.starts_with("btn."))
                .map(|(_, v)| v)
                .collect();
            let unique: BTreeSet<_> = words.iter().collect();
            assert_eq!(
                words.len(),
                unique.len(),
                "{}.json has two buttons with the same words",
                l.code
            );
        }
        assert_eq!(button_id("Add"), Some("add"));
        assert_eq!(button_id("Edit…"), Some("edit"));
        assert_eq!(button_id("Connect All"), Some("connect_all"));
        assert_eq!(button_id("Nothing like it"), None);
    }

    #[test]
    fn placeholders_are_filled() {
        // Any key works for the mechanics: a missing one shows as itself.
        assert_eq!(trf("{n} of {total}", &[("n", "2"), ("total", "5")]), "2 of 5");
    }

    #[test]
    fn unknown_things_fall_back() {
        set("xx_XX");
        assert_eq!(ACTIVE.load(Ordering::Relaxed), 0);
        assert_eq!(tr("tray.quit"), "Quit");
        assert_eq!(tr("no.such.key"), "no.such.key");
        assert_eq!(all().next(), Some((DEFAULT, "English (United States)")));
    }
}
