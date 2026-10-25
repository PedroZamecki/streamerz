//! What chat shows as pictures besides the platforms' own emotes: Unicode emoji (as Twemoji's
//! pictures, CC-BY 4.0) and the channels' third-party emotes (BTTV, FFZ and 7TV for Twitch, 7TV for
//! Kick), loaded once a run when the chat tab first shows, and kept in the cache for a day.

use super::{tell, Msg};
use crate::engine::UiCmd;
use serde_json::Value;
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, OnceLock};
use tokio::sync::mpsc::UnboundedSender;

/// Every fully-qualified emoji (Unicode's emoji-test.txt), one group per line: name, tab, emoji.
const TABLE: &str = include_str!("emoji.txt");
/// The longest emoji, in chars.
const LONGEST: usize = 10;

fn skin_tone(e: &str) -> bool {
    e.chars().any(|c| ('\u{1F3FB}'..='\u{1F3FF}').contains(&c))
}

/// The emoji groups for the picker (without the skin tone variants).
pub fn groups() -> impl Iterator<Item = (&'static str, Vec<&'static str>)> {
    TABLE
        .lines()
        .filter_map(|l| l.split_once('\t'))
        .map(|(g, es)| (g, es.split(' ').filter(|e| !skin_tone(e)).collect()))
}

/// Emoji by their text without U+FE0F (chat often leaves it out), to the fully-qualified one.
fn known() -> &'static HashMap<String, &'static str> {
    static K: OnceLock<HashMap<String, &'static str>> = OnceLock::new();
    K.get_or_init(|| {
        TABLE
            .lines()
            .filter_map(|l| l.split_once('\t'))
            .flat_map(|(_, es)| es.split(' '))
            .map(|e| (e.replace('\u{FE0F}', ""), e))
            .collect()
    })
}

/// `text` cut into emoji (with their picture's URL) and the text between them.
pub fn split(text: &str) -> Vec<(&str, Option<String>)> {
    let at: Vec<usize> = text.char_indices().map(|(i, _)| i).chain([text.len()]).collect();
    let (mut out, mut plain, mut k) = (Vec::new(), 0, 0);
    while k + 1 < at.len() {
        let hit = (1..=LONGEST.min(at.len() - 1 - k)).rev().find_map(|n| {
            let key = text[at[k]..at[k + n]].replace('\u{FE0F}', "");
            known().get(&key).map(|e| (n, *e))
        });
        match hit {
            Some((n, e)) => {
                if plain < at[k] {
                    out.push((&text[plain..at[k]], None));
                }
                out.push((&text[at[k]..at[k + n]], Some(twemoji(e))));
                k += n;
                plain = at[k];
            }
            None => k += 1,
        }
    }
    if plain < text.len() {
        out.push((&text[plain..], None));
    }
    out
}

/// Twemoji's picture of an emoji: its code points in hex, U+FE0F left out unless it's a ZWJ sequence.
pub fn twemoji(e: &str) -> String {
    let zwj = e.contains('\u{200D}');
    let name: Vec<String> = e
        .chars()
        .filter(|&c| zwj || c != '\u{FE0F}')
        .map(|c| format!("{:x}", c as u32))
        .collect();
    format!(
        "https://cdn.jsdelivr.net/gh/jdecked/twemoji@17.0.3/assets/72x72/{}.png",
        name.join("-")
    )
}

/// Third-party emotes by name, to their picture: Twitch's channel, then Kick's.
static THIRD: Mutex<[Option<HashMap<String, String>>; 2]> = Mutex::new([None, None]);
static LOADING: AtomicBool = AtomicBool::new(false);

/// The third-party emote called `word` on `platform` ("twitch" or "kick"), if loaded.
pub fn third(platform: &str, word: &str) -> Option<String> {
    let i = match platform {
        "twitch" => 0,
        "kick" => 1,
        _ => return None,
    };
    THIRD.lock().ok()?[i].as_ref()?.get(word).cloned()
}

/// Every third-party emote of `platform` (for the picker): name and picture.
pub fn third_all(platform: &str) -> Vec<(String, String)> {
    let i = if platform == "kick" { 1 } else { 0 };
    let mut v: Vec<_> = THIRD
        .lock()
        .ok()
        .and_then(|t| t[i].clone())
        .unwrap_or_default()
        .into_iter()
        .collect();
    v.sort_unstable_by_key(|a| a.0.to_lowercase());
    v
}

async fn get(url: &str) -> Value {
    match crate::http::request("GET", url, &[], b"").await {
        Ok(r) if r.status == 200 => serde_json::from_slice(&r.body).unwrap_or_default(),
        _ => Value::Null,
    }
}

fn bttv(list: &Value, x: u8, into: &mut HashMap<String, String>) {
    for e in list.as_array().into_iter().flatten() {
        if let (Some(code), Some(id)) = (e["code"].as_str(), e["id"].as_str()) {
            into.insert(code.into(), format!("https://cdn.betterttv.net/emote/{id}/{x}x.webp"));
        }
    }
}

fn ffz(sets: &Value, x: u8, into: &mut HashMap<String, String>) {
    for set in sets.as_object().into_iter().flat_map(|o| o.values()) {
        for e in set["emoticons"].as_array().into_iter().flatten() {
            // The animated ones as WebP (their GIFs would show still).
            let urls = if e["animated"].is_object() {
                &e["animated"]
            } else {
                &e["urls"]
            };
            let url = urls[x.to_string()].as_str().or(urls["1"].as_str());
            if let (Some(name), Some(url)) = (e["name"].as_str(), url) {
                into.insert(name.into(), url.into());
            }
        }
    }
}

fn seventv(set: &Value, x: u8, into: &mut HashMap<String, String>) {
    for e in set["emotes"].as_array().into_iter().flatten() {
        if let (Some(name), Some(host)) = (e["name"].as_str(), e.pointer("/data/host/url").and_then(Value::as_str)) {
            into.insert(name.into(), format!("https:{host}/{x}x.webp"));
        }
    }
}

/// How long saved emote lists are used before they're fetched again (in the background).
const FRESH: u64 = 24 * 60 * 60;

/// Put the lists in, and lay the lines shown out again (like when a picture arrives).
fn set(tw: HashMap<String, String>, kick: HashMap<String, String>) {
    if let Ok(mut t) = THIRD.lock() {
        *t = [Some(tw), Some(kick)];
    }
    super::wake(Msg::Image);
}

/// Load the third-party emotes (once a run) for the Twitch channel `twitch_id` and the Kick user
/// `kick_id` (either may be empty): the saved lists at once, fetched again when older than a day.
pub fn load(cmds: &UnboundedSender<UiCmd>, twitch_id: String, kick_id: String) {
    if LOADING.swap(true, Ordering::Relaxed) {
        return;
    }
    // 2x on scaled screens (sharp), else 1x (an animated 2x can be megabytes).
    let x: u8 = if fltk::app::screen_scale(0) > 1.25 { 2 } else { 1 };
    let name = format!("emotes-{twitch_id}-{kick_id}-{x}x");
    let map = |v: &Value| -> HashMap<String, String> {
        v.as_object()
            .into_iter()
            .flatten()
            .filter_map(|(k, u)| Some((k.clone(), u.as_str()?.to_string())))
            .collect()
    };
    let fresh = match super::img::saved(&name) {
        Some((age, v)) => {
            set(map(&v["twitch"]), map(&v["kick"]));
            age < FRESH
        }
        None => false,
    };
    if fresh {
        return;
    }
    tell(cmds, move |_| {
        tokio::task::spawn_local(async move {
            let (tw, kick) = fetch(&twitch_id, &kick_id, x).await;
            // Nothing came (offline): keep what was saved.
            if tw.is_empty() && kick.is_empty() {
                return;
            }
            crate::say(format!(
                "Chat: {} Twitch and {} Kick third-party emotes",
                tw.len(),
                kick.len()
            ));
            super::img::save(&name, &serde_json::json!({"twitch": tw, "kick": kick}));
            set(tw, kick);
        });
    });
}

/// The channels' third-party emotes, from BTTV, FFZ and 7TV.
async fn fetch(twitch_id: &str, kick_id: &str, x: u8) -> (HashMap<String, String>, HashMap<String, String>) {
    let mut global = HashMap::new();
    let f = get("https://api.frankerfacez.com/v1/set/global").await;
    ffz(&f["sets"], x, &mut global);
    bttv(
        &get("https://api.betterttv.net/3/cached/emotes/global").await,
        x,
        &mut global,
    );
    let seven = get("https://7tv.io/v3/emote-sets/global").await;
    seventv(&seven, x, &mut global);
    let mut tw = global.clone();
    if !twitch_id.is_empty() {
        ffz(
            &get(&format!("https://api.frankerfacez.com/v1/room/id/{twitch_id}")).await["sets"],
            x,
            &mut tw,
        );
        let b = get(&format!("https://api.betterttv.net/3/cached/users/twitch/{twitch_id}")).await;
        bttv(&b["channelEmotes"], x, &mut tw);
        bttv(&b["sharedEmotes"], x, &mut tw);
        seventv(
            &get(&format!("https://7tv.io/v3/users/twitch/{twitch_id}")).await["emote_set"],
            x,
            &mut tw,
        );
    }
    // BTTV and FFZ are Twitch only; 7TV's global set works on Kick too.
    let mut kick = HashMap::new();
    seventv(&seven, x, &mut kick);
    if !kick_id.is_empty() {
        seventv(
            &get(&format!("https://7tv.io/v3/users/kick/{kick_id}")).await["emote_set"],
            x,
            &mut kick,
        );
    }
    (tw, kick)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splits_emoji_out_of_text() {
        let s = split("hi😀 ❤ ok👍🏽!");
        let shown: Vec<(&str, bool)> = s.iter().map(|(t, u)| (*t, u.is_some())).collect();
        assert_eq!(
            shown,
            [
                ("hi", false),
                ("😀", true),
                (" ", false),
                ("❤", true),
                (" ok", false),
                ("👍🏽", true),
                ("!", false)
            ]
        );
        assert!(s[3].1.as_deref().unwrap().ends_with("/2764.png"));
        assert!(twemoji("👨‍👩‍👧").ends_with("/1f468-200d-1f469-200d-1f467.png"));
        assert!(split("plain text").iter().all(|x| x.1.is_none()));
        assert!(groups().any(|(g, es)| g == "Smileys & Emotion" && es.contains(&"😀")));
    }
}
