//! Translations: every text the user reads goes through `tr("some.key")`. The words live in
//! `assets/locales/<code>.json` (flat, key to text), built into the binary; a key a language lacks falls
//! back to `en_US`, and a key nobody has shows as itself. To add a language, make its file with
//! `dev/locale.py new <code> "<name>"`: it's found at build time (build.rs), with nothing to register.

use std::collections::HashMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::OnceLock;

/// The language the keys are written in, and the fallback of all others (the first of `LOCALES`).
#[cfg_attr(not(test), allow(dead_code))]
pub const DEFAULT: &str = "en_US";

// `LOCALES`: every `assets/locales/<code>.json` as (code, its text), en_US first (made by build.rs).
include!(concat!(env!("OUT_DIR"), "/locales.rs"));

/// A language's name in itself, the `locale.name` of its file.
fn name(i: usize) -> &'static str {
    catalog(i).get("locale.name").map_or(LOCALES[i].0, String::as_str)
}

/// The language in use (an index of `LOCALES`).
static ACTIVE: AtomicUsize = AtomicUsize::new(0);
/// Each language's texts, read the first time they're needed.
static CATALOGS: [OnceLock<HashMap<String, String>>; LOCALES.len()] = [const { OnceLock::new() }; LOCALES.len()];

fn catalog(i: usize) -> &'static HashMap<String, String> {
    CATALOGS[i].get_or_init(|| serde_json::from_str(LOCALES[i].1).unwrap_or_default())
}

/// Picks the language by its code; one this build doesn't have means `en_US`.
/// An empty `code` means the system's language (`detect`).
pub fn set(code: &str) {
    let i = if code.is_empty() {
        pick(&system_languages())
    } else {
        LOCALES.iter().position(|l| l.0 == code).unwrap_or(0)
    };
    ACTIVE.store(i, Ordering::Relaxed);
}

/// The language the system asks for, as the one of this build (en_US when it has none).
pub fn detect() -> &'static str {
    LOCALES[pick(&system_languages())].0
}

/// The system's languages, best first, as "pt_BR" or "pt".
fn system_languages() -> Vec<String> {
    #[cfg(windows)]
    {
        use windows_sys::Win32::Globalization::GetUserDefaultLocaleName;
        let mut buf = [0u16; 85];
        // SAFETY: the buffer is 85 wide, the length LOCALE_NAME_MAX_LENGTH.
        let n = unsafe { GetUserDefaultLocaleName(buf.as_mut_ptr(), 85) };
        if n > 1 {
            return vec![String::from_utf16_lossy(&buf[..n as usize - 1]).replace('-', "_")];
        }
        Vec::new()
    }
    #[cfg(not(windows))]
    {
        // The order glibc looks in: LANGUAGE (a list), then LC_ALL, LC_MESSAGES, LANG.
        let mut out = Vec::new();
        for var in ["LANGUAGE", "LC_ALL", "LC_MESSAGES", "LANG"] {
            for l in std::env::var(var).unwrap_or_default().split(':') {
                // "pt_BR.UTF-8@euro" is pt_BR; "C" and "POSIX" say nothing.
                let l = l.split(['.', '@']).next().unwrap_or("");
                if !l.is_empty() && l != "C" && l != "POSIX" {
                    out.push(l.to_string());
                }
            }
        }
        out
    }
}

/// The index of the locale that best answers `wanted` ("pt_BR", "pt", "es_AR"...): the exact one, else one
/// of the same language, for each in turn; English when none.
fn pick(wanted: &[String]) -> usize {
    for w in wanted {
        if let Some(i) = LOCALES.iter().position(|l| l.0 == w) {
            return i;
        }
        let lang = w.split('_').next().unwrap_or("");
        if let Some(i) = LOCALES.iter().position(|l| l.0.split('_').next() == Some(lang)) {
            return i;
        }
    }
    0
}

/// The languages to choose from: code and name.
pub fn all() -> impl Iterator<Item = (&'static str, &'static str)> {
    (0..LOCALES.len()).map(|i| (LOCALES[i].0, name(i)))
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
        assert_eq!(LOCALES[0].0, DEFAULT);
        let keys = |i| catalog(i).keys().cloned().collect::<BTreeSet<_>>();
        for (i, l) in LOCALES.iter().enumerate() {
            let parsed = serde_json::from_str::<HashMap<String, String>>(l.1);
            assert!(parsed.is_ok(), "{}.json is not a flat key to text object", l.0);
            assert_eq!(keys(i), keys(0), "{}.json and en_US.json differ in keys", l.0);
            assert!(!name(i).is_empty() && name(i) != l.0, "{}.json has no locale.name", l.0);
            // The same {placeholders} as the English text, or a text fills in wrong.
            fn holes(t: &str) -> Vec<&str> {
                let mut h: Vec<&str> = t.split('{').skip(1).filter_map(|r| r.split('}').next()).collect();
                h.sort_unstable();
                h
            }
            for (k, v) in catalog(i) {
                assert_eq!(
                    holes(v),
                    holes(&catalog(0)[k]),
                    "{}.json: {k} has other {{placeholders}}",
                    l.0
                );
            }
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
            if !key.starts_with("btn.") && key != "locale.name" {
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
                l.0
            );
        }
        assert_eq!(button_id("Add"), Some("add"));
        assert_eq!(button_id("Edit…"), Some("edit"));
        assert_eq!(button_id("Connect All"), Some("connect_all"));
        assert_eq!(button_id("Nothing like it"), None);
    }

    #[test]
    fn the_system_language_picks_a_locale_or_english() {
        let w = |l: &[&str]| pick(&l.iter().map(|s| s.to_string()).collect::<Vec<_>>());
        let code = |i: usize| LOCALES[i].0;
        assert_eq!(code(w(&["pt_BR"])), "pt_BR");
        // Another country of a language we have: that language.
        assert_eq!(code(w(&["pt_PT"])), "pt_BR");
        assert_eq!(code(w(&["pt"])), "pt_BR");
        // The first the system asks for that we have.
        assert_eq!(code(w(&["xx_XX", "pt_BR", "en_US"])), "pt_BR");
        // Nothing we have, or nothing asked: English.
        assert_eq!(code(w(&["xx_XX"])), DEFAULT);
        assert_eq!(code(w(&[])), DEFAULT);
    }

    #[test]
    fn placeholders_are_filled() {
        // Any key works for the mechanics: a missing one shows as itself.
        assert_eq!(trf("{n} of {total}", &[("n", "2"), ("total", "5")]), "2 of 5");
    }

    #[test]
    fn unknown_things_fall_back() {
        set("pt_BR");
        assert_eq!(LOCALES[ACTIVE.load(Ordering::Relaxed)].0, "pt_BR");
        assert_eq!(tr("tray.quit"), "Encerrar");
        assert_eq!(button_id("Adicionar"), Some("add"));
        assert_eq!(button_id("Editar…"), Some("edit"));
        assert_eq!(button_id("Conectar todos"), Some("connect_all"));

        set("xx_XX");
        assert_eq!(ACTIVE.load(Ordering::Relaxed), 0);
        assert_eq!(tr("tray.quit"), "Quit");
        assert_eq!(tr("no.such.key"), "no.such.key");
        assert_eq!(all().next(), Some((DEFAULT, "English (United States)")));
        set(DEFAULT);
    }
}
