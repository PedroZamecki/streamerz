//! Who is in a user group. A member is kept as `{platform}-{id}` ("twitch-152674150"): the id never
//! changes when they rename themselves, and the platform keeps the same number on two platforms apart.
//! Their name as last seen is kept beside it (`names`), only to show it.

use crate::model::Obj;
use serde_json::Value;

/// The platforms a member can be from.
pub const PLATFORMS: [&str; 4] = ["twitch", "youtube", "kick", "tiktok"];

/// A member's key: `twitch-152674150`. An empty platform is Twitch (Streamer.bot's groups were).
pub fn key(platform: &str, id: &str) -> String {
    let platform = platform.trim().to_lowercase();
    let platform = if platform.is_empty() {
        "twitch"
    } else {
        platform.as_str()
    };
    format!("{platform}-{}", id.trim())
}

/// A key's platform and id, when it has one.
pub fn split(key: &str) -> Option<(&'static str, &str)> {
    PLATFORMS.iter().find_map(|p| {
        key.strip_prefix(p)
            .and_then(|rest| rest.strip_prefix('-'))
            .map(|id| (*p, id))
    })
}

/// What a group's `userIds` holds, as text.
pub fn members(g: &Obj) -> Vec<String> {
    g.get("userIds")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .map(crate::vars::text)
        .collect()
}

/// Whether `g` holds the member `key`.
pub fn has(g: &Obj, key: &str) -> bool {
    members(g).iter().any(|m| m == key)
}

/// A member's name as last seen, if known.
pub fn name<'a>(g: &'a Obj, key: &str) -> Option<&'a str> {
    g.get("names")?.get(key)?.as_str().filter(|n| !n.is_empty())
}

/// Adds `key` (named `shown`) to `g`; false when it was in already (its name is still updated).
pub fn add(g: &mut Obj, key: &str, shown: &str) -> bool {
    let fresh = !has(g, key);
    if fresh {
        let ids = g.entry("userIds").or_insert_with(|| Value::Array(vec![]));
        if let Some(ids) = ids.as_array_mut() {
            ids.push(key.into());
        }
    }
    if !shown.is_empty() {
        let names = g.entry("names").or_insert_with(|| Value::Object(Obj::new()));
        if let Some(names) = names.as_object_mut() {
            names.insert(key.into(), shown.into());
        }
    }
    fresh
}

/// Takes `key` out of `g`; false when it wasn't in.
pub fn remove(g: &mut Obj, key: &str) -> bool {
    let was = has(g, key);
    if let Some(ids) = g.get_mut("userIds").and_then(Value::as_array_mut) {
        ids.retain(|x| crate::vars::text(x) != key);
    }
    if let Some(names) = g.get_mut("names").and_then(Value::as_object_mut) {
        names.remove(key);
    }
    was
}

/// Old groups kept a bare id (Streamer.bot's, Twitch's): each becomes a key. A YouTube channel id
/// ("UC" and 22 more) is YouTube's; any other bare id was Twitch's, the only platform groups had.
pub fn migrate(groups: &mut [Obj]) {
    for g in groups {
        let Some(ids) = g.get_mut("userIds").and_then(Value::as_array_mut) else {
            continue;
        };
        for id in ids.iter_mut() {
            let text = crate::vars::text(id);
            if text.is_empty() || split(&text).is_some() {
                continue;
            }
            let youtube = text.starts_with("UC") && text.len() == 24;
            *id = key(if youtube { "youtube" } else { "twitch" }, &text).into();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn members_are_kept_by_platform_and_id() {
        assert_eq!(key("Twitch", "152674150"), "twitch-152674150");
        assert_eq!(key("", "1"), "twitch-1");
        assert_eq!(split("youtube-UCabc"), Some(("youtube", "UCabc")));
        assert_eq!(split("kick-12"), Some(("kick", "12")));
        assert_eq!(split("12"), None);
        let mut g = Obj::new();
        assert!(add(&mut g, "kick-12", "Pedro"));
        assert!(!add(&mut g, "kick-12", "PedroZ"));
        assert_eq!(name(&g, "kick-12"), Some("PedroZ"));
        // The same number on another platform is someone else.
        assert!(!has(&g, "twitch-12"));
        assert!(remove(&mut g, "kick-12"));
        assert!(members(&g).is_empty() && name(&g, "kick-12").is_none());
    }

    #[test]
    fn old_bare_ids_get_their_platform() {
        let mut groups = vec![json!({"userIds": ["123", "UCabcdefghijklmnopqrstuv", "kick-9", 456]})
            .as_object()
            .unwrap()
            .clone()];
        migrate(&mut groups);
        assert_eq!(
            groups[0]["userIds"],
            json!(["twitch-123", "youtube-UCabcdefghijklmnopqrstuv", "kick-9", "twitch-456"])
        );
    }
}
