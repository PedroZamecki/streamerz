//! Streamerz's own reference: a page for every trigger and sub-action (`docs/reference`), and the guides
//! (`docs/*.md`), built into the binary so the MCP server answers from them with nothing on disk. Each
//! entry is a `## Title` section with a `- Kind:` and a `- Code:` line under it.

// (path, text) of every file in docs/, from build.rs.
include!(concat!(env!("OUT_DIR"), "/docs.rs"));

/// One trigger's or sub-action's page.
#[derive(Debug, Clone, PartialEq)]
pub struct Entry {
    pub kind: &'static str,
    pub code: u32,
    pub title: String,
    pub file: &'static str,
    pub text: String,
}

/// `- Key: value` under a section's title.
fn field<'a>(section: &'a str, key: &str) -> Option<&'a str> {
    section
        .lines()
        .take_while(|l| !l.starts_with("### "))
        .find_map(|l| l.strip_prefix("- ")?.strip_prefix(key)?.strip_prefix(':'))
        .map(str::trim)
}

/// The entries of one file.
fn entries_in(file: &'static str, text: &str) -> Vec<Entry> {
    text.split("\n## ")
        .skip(1)
        .filter_map(|s| {
            let kind = match field(s, "Kind")? {
                "sub-action" => "subaction",
                "trigger" => "trigger",
                _ => return None,
            };
            let code = field(s, "Code")?.trim_matches('`').parse().ok()?;
            let title = s.lines().next()?.trim().to_string();
            let body = s.split("\n# ").next().unwrap_or(s).trim_end();
            Some(Entry {
                kind,
                code,
                title,
                file,
                text: format!("## {body}\n"),
            })
        })
        .collect()
}

/// Every entry of the reference.
pub fn entries() -> Vec<Entry> {
    DOCS.iter()
        .filter(|(p, _)| p.starts_with("reference/") && !p.ends_with("README.md"))
        .flat_map(|(p, t)| entries_in(p, t))
        .collect()
}

/// The page of a `kind` ("subaction" or "trigger") by its code.
pub fn entry(kind: &str, code: u32) -> Option<Entry> {
    entries().into_iter().find(|e| e.kind == kind && e.code == code)
}

/// The guides (everything outside the reference): their names.
pub fn guides() -> Vec<&'static str> {
    DOCS.iter()
        .map(|(p, _)| *p)
        .filter(|p| !p.starts_with("reference/"))
        .collect()
}

/// A guide by name (with or without `.md`, any case).
pub fn guide(name: &str) -> Option<&'static str> {
    let want = name.trim().trim_end_matches(".md").to_lowercase();
    DOCS.iter()
        .find(|(p, _)| p.trim_end_matches(".md").to_lowercase() == want)
        .map(|(_, t)| *t)
}

/// Entries whose title or text has every word of `query`, best first (title matches before text).
pub fn search(query: &str) -> Vec<Entry> {
    let words: Vec<String> = query.split_whitespace().map(str::to_lowercase).collect();
    if words.is_empty() {
        return Vec::new();
    }
    let mut found: Vec<(usize, Entry)> = entries()
        .into_iter()
        .filter_map(|e| {
            let (title, text) = (e.title.to_lowercase(), e.text.to_lowercase());
            words.iter().all(|w| text.contains(w)).then(|| {
                let in_title = words.iter().filter(|w| title.contains(w.as_str())).count();
                (words.len() - in_title, e)
            })
        })
        .collect();
    found.sort_by(|a, b| a.0.cmp(&b.0).then_with(|| a.1.title.cmp(&b.1.title)));
    found.into_iter().map(|f| f.1).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sections_are_read_by_kind_and_code() {
        let text = "# Sounds\n\nIntro.\n\n## Play Sound\n\n- Kind: sub-action\n- Code: 1\n\nPlays a file.\n\n### Fields\n\n- Kind: not this\n\n## Broken\n\nNo code.\n";
        let e = entries_in("reference/x.md", text);
        assert_eq!(e.len(), 1);
        assert_eq!(
            (e[0].kind, e[0].code, e[0].title.as_str()),
            ("subaction", 1, "Play Sound")
        );
        assert!(e[0].text.starts_with("## Play Sound") && e[0].text.contains("### Fields"));
    }

    /// Every trigger and sub-action Streamerz knows has its page, and no page is about one it doesn't.
    #[test]
    fn every_trigger_and_sub_action_is_documented() {
        let all = entries();
        let mut missing = Vec::new();
        for (kind, table) in [
            ("subaction", crate::types::SUB_ACTIONS),
            ("trigger", crate::types::TRIGGERS),
        ] {
            for (code, name) in table {
                if !all.iter().any(|e| e.kind == kind && e.code == *code) {
                    missing.push(format!("{kind} {code} {name}"));
                }
            }
        }
        assert!(
            missing.is_empty(),
            "{} undocumented, e.g. {:?}",
            missing.len(),
            &missing[..missing.len().min(10)]
        );
        let mut seen = std::collections::HashSet::new();
        for e in &all {
            assert!(
                seen.insert((e.kind, e.code)),
                "{} {} is documented twice ({})",
                e.kind,
                e.code,
                e.file
            );
        }
    }

    #[test]
    fn the_docs_follow_the_writing_rules() {
        for (path, text) in DOCS {
            assert!(
                !text.contains('\u{2014}') && !text.contains('\u{2013}'),
                "{path}: an em or en dash"
            );
            // The reference is ours, as written by a person (the README does name assistants: how to set up MCP).
            if !path.starts_with("reference/") {
                continue;
            }
            let lower = text.to_lowercase();
            for word in ["claude", "chatgpt", "generated by", "as an ai", "streamer.bot docs"] {
                assert!(!lower.contains(word), "{path}: says {word:?}");
            }
        }
    }

    /// Writes `target/types.json`: every trigger and sub-action with its category and fields, what the
    /// reference is written from. `cargo test --profile fast dump_types -- --ignored`
    #[test]
    #[ignore]
    fn dump_types() {
        use crate::ui::{forms, picker};
        let fields = |f: Option<&'static [forms::F]>| -> Vec<serde_json::Value> {
            f.unwrap_or(&[])
                .iter()
                .map(|(k, l, w)| serde_json::json!({"key": k, "label": l, "holds": w.about()}))
                .collect()
        };
        let subs: Vec<_> = crate::types::SUB_ACTIONS
            .iter()
            .map(|&(code, name)| {
                let info = crate::types::SUB_ACTION_INFO.iter().find(|x| x.0 == code);
                let category = info.map_or("", |i| i.2);
                serde_json::json!({
                    "code": code, "name": name, "title": info.map_or(name, |i| i.1), "category": category,
                    "about": picker::sub_action_description(code, name, category),
                    "fields": fields(forms::sub_fields(code as u64)),
                    "stored": info.map_or(&[][..], |i| i.3),
                })
            })
            .collect();
        let triggers: Vec<_> = crate::types::TRIGGERS
            .iter()
            .map(|&(code, name)| {
                let category = picker::trigger_categorize(name);
                let title = picker::trigger_clean_title(name);
                serde_json::json!({
                    "code": code, "name": name, "title": title, "category": category,
                    "about": picker::trigger_description(code, name, &title, &category),
                    "fields": fields(forms::trigger_fields(code as u64)),
                })
            })
            .collect();
        let out = serde_json::json!({"subactions": subs, "triggers": triggers});
        std::fs::write("target/types.json", serde_json::to_string_pretty(&out).unwrap()).unwrap();
    }
}
