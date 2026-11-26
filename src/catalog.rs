//! What was found on the connections (a channel's rewards, OBS's scenes and sources), kept for the editors:
//! a field that names one of them offers the list, and takes a typed name when nothing was found.

use std::collections::HashMap;
use std::sync::Mutex;

/// `(id, name)` items.
type Items = Vec<(String, String)>;

/// Each list by key (`twitch.rewards`, `kick.rewards`, `obs.scenes`, `obs.sources`).
static LISTS: Mutex<Vec<(&'static str, Items)>> = Mutex::new(Vec::new());

fn eq_case(a: &str, b: &str) -> bool {
    if a.is_ascii() && b.is_ascii() {
        a.eq_ignore_ascii_case(b)
    } else {
        a.to_lowercase() == b.to_lowercase()
    }
}

/// Replaces a list with what a connection just reported.
pub fn set(key: &'static str, mut items: Vec<(String, String)>) {
    items.sort_by(|a, b| a.1.to_lowercase().cmp(&b.1.to_lowercase()).then_with(|| a.0.cmp(&b.0)));
    let mut l = LISTS.lock().unwrap_or_else(|e| e.into_inner());
    match l.iter_mut().find(|x| x.0 == key) {
        Some(x) => x.1 = items,
        None => l.push((key, items)),
    }
}

/// The list found for `key` (empty when nothing connected yet), by name.
pub fn get(key: &str) -> Vec<(String, String)> {
    let l = LISTS.lock().unwrap_or_else(|e| e.into_inner());
    l.iter().find(|x| x.0 == key).map(|x| x.1.clone()).unwrap_or_default()
}

/// The id of what `text` names: an id of the list as it is, else the one whose name it is (any case).
/// None when nothing in the list matches (the text may be an id or name not detected).
pub fn id_of(key: &str, text: &str) -> Option<String> {
    let text = text.trim();
    if text.is_empty() {
        return None;
    }
    let l = LISTS.lock().unwrap_or_else(|e| e.into_inner());
    let items = &l.iter().find(|x| x.0 == key)?.1;
    items
        .iter()
        .find(|x| !x.0.is_empty() && x.0 == text)
        .or_else(|| items.iter().find(|x| !x.0.is_empty() && eq_case(&x.1, text)))
        .map(|x| x.0.clone())
}

/// `text` as an id: what it names when that was detected, else the text itself (an id typed in, or a
/// name nothing found: the platform will say if it is wrong).
pub fn resolve(key: &str, text: &str) -> String {
    id_of(key, text).unwrap_or_else(|| text.trim().to_string())
}

/// Whether a trigger's field `want` (empty: anything) names the item `id` / `title` an event is about.
pub fn matches(key: &str, want: &str, id: &str, title: &str) -> bool {
    let want = want.trim();
    want.is_empty()
        || (!id.is_empty() && want == id)
        || (!title.is_empty() && eq_case(want, title))
        || (!id.is_empty() && id_of(key, want).is_some_and(|w| w == id))
}

/// The name of the item with this id.
pub fn name_of(key: &str, id: &str) -> Option<String> {
    if id.is_empty() {
        return None;
    }
    let l = LISTS.lock().unwrap_or_else(|e| e.into_inner());
    l.iter()
        .find(|x| x.0 == key)?
        .1
        .iter()
        .find(|x| x.0 == id)
        .map(|x| x.1.clone())
}

/// Distinguish duplicate display names while keeping the selected identity stable.
pub fn choices(key: &str) -> Items {
    let list = get(key);
    let mut counts: HashMap<&str, usize> = HashMap::with_capacity(list.len());
    for (_, name) in &list {
        *counts.entry(name.as_str()).or_insert(0) += 1;
    }
    list.iter()
        .map(|(id, name)| {
            let label = if name.is_empty() {
                id.clone()
            } else if counts.get(name.as_str()).copied().unwrap_or(0) > 1 {
                format!("{name} ({id})")
            } else {
                name.clone()
            };
            (id.clone(), label)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_and_ids_find_each_other() {
        set(
            "test.rewards",
            vec![
                ("a-1".into(), "Pick the LED color".into()),
                ("b-2".into(), "Hydrate".into()),
            ],
        );
        assert_eq!(id_of("test.rewards", "hydrate").as_deref(), Some("b-2"));
        assert_eq!(id_of("test.rewards", "a-1").as_deref(), Some("a-1"));
        assert_eq!(id_of("test.rewards", "nothing"), None);
        assert_eq!(id_of("test.rewards", ""), None);
        assert_eq!(name_of("test.rewards", "a-1").as_deref(), Some("Pick the LED color"));
        assert_eq!(get("test.rewards")[0].1, "Hydrate");
        assert!(matches("test.rewards", "", "x", "y"));
        assert!(matches("test.rewards", "HYDRATE", "b-2", "Hydrate"));
        assert!(matches("test.rewards", "hydrate", "b-2", "Renamed since"));
        assert!(matches("test.rewards", "b-2", "b-2", ""));
        assert!(!matches("test.rewards", "Hydrate", "a-1", "Pick the LED color"));
        assert!(get("nothing").is_empty());
    }

    #[test]
    fn handles_unicode_case_duplicates_and_empty_ids() {
        set(
            "test.unicode",
            vec![
                ("id-2".into(), "Caf\u{00e9}".into()),
                ("id-1".into(), "Caf\u{00e9}".into()),
                ("id-3".into(), "".into()),
                ("".into(), "Ghost".into()),
            ],
        );
        assert_eq!(id_of("test.unicode", "CAF\u{00c9}").as_deref(), Some("id-1"));
        assert!(matches("test.unicode", "caf\u{00e9}", "id-1", "CAF\u{00c9}"));
        assert!(!matches("test.unicode", "Ghost", "", "Other"));
        assert_eq!(name_of("test.unicode", ""), None);
        assert_eq!(
            choices("test.unicode"),
            vec![
                ("id-3".into(), "id-3".into()),
                ("id-1".into(), "Caf\u{00e9} (id-1)".into()),
                ("id-2".into(), "Caf\u{00e9} (id-2)".into()),
                ("".into(), "Ghost".into()),
            ]
        );
    }
}
