//! Which Lucide icon each node of the trigger and sub-action picker wears: a platform's or app's logo where there
//! is one, else a Lucide icon chosen by the group's name (groups) or by the words of the title (items).

/// The logo (a brand asset's path) of a platform or app, by its group's name: what 1.6.0's trees showed
/// in place of a Lucide icon. None when it has no logo.
pub fn logo(name: &str) -> Option<&'static str> {
    Some(match name.trim() {
        "Twitch" => "brands/twitch.svg",
        "Kick" => "brands/kick.svg",
        "YouTube" => "brands/youtube.svg",
        "TikTok" => "brands/tiktok.svg",
        "OBS Studio" | "OBS" => "brands/obsstudio.svg",
        "Streamlabs Desktop" | "Streamlabs" => "brands/streamlabs.svg",
        "Spotify" => "brands/spotify.svg",
        "Discord" => "brands/discord.svg",
        "StreamElements" => "brands/streamelements.svg",
        "DonorDrive" => "brands/donordrive.svg",
        "Streamer.bot" => "brands/streamerbot.svg",
        "Crowd Control" => "brands/crowdcontrol.svg",
        "Elgato" => "brands/elgato.svg",
        "Patreon" => "brands/patreon.svg",
        "Shopify" => "brands/shopify.svg",
        "Pally.gg" => "brands/pallygg.svg",
        "Ko-fi" => "brands/ko-fi.png",
        "Fourthwall" => "brands/fourthwall.png",
        "HypeRate" => "brands/hyperate.png",
        "Pulsoid" => "brands/pulsoid.png",
        "Meld Studio" => "brands/meld-studio.png",
        "PolyPop" => "brands/polypop.png",
        "Speaker.bot" => "brands/speakerbot.png",
        "Voicemod" => "brands/voicemod.png",
        "VTube Studio" => "brands/vtube-studio.png",
        _ => return None,
    })
}

/// The logo of what a category path (`Twitch/Chat`, `Integrations/Ko-fi`) belongs to: its deepest part
/// that has one.
pub fn logo_of_category(category: &str) -> Option<&'static str> {
    category.split('/').rev().find_map(logo)
}

/// The Lucide icon of a group, by its name, when it has no logo.
pub fn group(name: &str) -> &'static str {
    let n = name.to_lowercase();
    let has = |words: &[&str]| words.iter().any(|w| n.contains(w));
    match n.as_str() {
        "core" => return "cpu",
        "donations & shops" => return "badge-dollar-sign",
        "elgato" => return "video",
        "file & process" => return "file-text",
        "inputs & midi" => return "keyboard",
        "integrations" => return "puzzle",
        "servers & remote" => return "server",
        "settings" => return "settings",
        "meld studio" | "polypop" => return "sparkles",
        "kick" | "twitch" | "youtube" | "tiktok" | "obs studio" | "streamlabs desktop" => return "radio",
        _ => {}
    }
    if has(&["camera hub", "webcam", "epoccam", "lenses"]) {
        "camera"
    } else if has(&["wave link", "sounds", "outputs", "speaker.bot"]) {
        "volume-2"
    } else if has(&["prompter - display"]) {
        "type"
    } else if has(&["prompter"]) {
        "scroll-text"
    } else if has(&["stream deck", "hotkeys", "decks"]) {
        "keyboard"
    } else if has(&["microphone", "voice", "inputs"]) {
        "mic"
    } else if has(&["midi", "music", "tracks", "song", "playback", "player"]) {
        "music"
    } else if has(&["action queues"]) {
        "list-ordered"
    } else if has(&["arguments", "globals", "variable"]) {
        "braces"
    } else if has(&["c#", "scripting"]) {
        "code"
    } else if has(&["commands"]) {
        "terminal"
    } else if has(&["file tail"]) {
        "scroll-text"
    } else if has(&["file watcher"]) {
        "eye"
    } else if has(&["file io", "files"]) {
        "file-text"
    } else if has(&["throwing"]) {
        "swords"
    } else if has(&["process", "system", "lifecycle"]) {
        "cpu"
    } else if has(&["overlay"]) {
        "layers"
    } else if has(&["logic"]) {
        "git-branch"
    } else if has(&["misc", "other"]) {
        "ellipsis"
    } else if has(&["network", "websocket", "webhook", "connection", "status"]) {
        if has(&["webhook"]) {
            "share-2"
        } else if has(&["network"]) {
            "globe"
        } else {
            "plug"
        }
    } else if has(&["quote"]) {
        "quote"
    } else if has(&["signals", "broadcast", "stream", "channel"]) {
        "radio"
    } else if has(&["timers"]) {
        "timer"
    } else if has(&["user groups", "users", "community", "roles", "guest", "crowd"]) {
        "users"
    } else if has(&["user", "pronouns"]) {
        "user-round"
    } else if has(&["bits", "cheers", "rewards", "points", "balance", "coins"]) {
        "coins"
    } else if has(&["gift", "loots", "throne"]) {
        "gift"
    } else if has(&["subscription", "membership", "subscribers", "sub counter"]) {
        "star"
    } else if has(&["moderation", "automod", "shield", "safety", "bans"]) {
        if has(&["bans"]) {
            "ban"
        } else {
            "shield"
        }
    } else if has(&["shoutouts", "ads", "special messages", "announcements"]) {
        "megaphone"
    } else if has(&["chat", "messages"]) {
        "message-square"
    } else if has(&["hype", "raid"]) {
        "flame"
    } else if has(&["charity", "goals", "patreon", "heart rate"]) {
        if has(&["heart rate"]) {
            "heart-pulse"
        } else {
            "heart"
        }
    } else if has(&["poll"]) {
        "vote"
    } else if has(&["prediction"]) {
        "trending-up"
    } else if has(&["emotes"]) {
        "smile"
    } else if has(&["followers"]) {
        "user-plus"
    } else if has(&["clips", "mtion", "recording", "replay", "layers"]) {
        "video"
    } else if has(&["scenes", "groups"]) {
        "layers"
    } else if has(&["sessions"]) {
        "clock"
    } else if has(&["sources", "backgrounds", "image"]) {
        "image"
    } else if has(&["filters"]) {
        "funnel"
    } else if has(&["effects", "lumia", "outfits"]) {
        "sparkles"
    } else if has(&["tags"]) {
        "tag"
    } else if has(&["shopify", "fourthwall"]) {
        "shopping-bag"
    } else if has(&["ko-fi"]) {
        "coffee"
    } else if has(&["services", "donor"]) {
        "coins"
    } else if has(&["library"]) {
        "archive"
    } else if has(&["playlists", "browse"]) {
        "list"
    } else if has(&["ifttt", "events", "triggers", "actions", "general", "throwing"]) {
        "zap"
    } else if has(&["vtube"]) {
        "smile"
    } else if has(&["remote", "instances"]) {
        "server"
    } else if has(&["statistics"]) {
        "trending-up"
    } else {
        "folder"
    }
}

/// Words of a title, lowercased: a rule's word matches the start of one of them, or all of one when it
/// starts with `=`.
fn words(title: &str) -> Vec<String> {
    title
        .to_lowercase()
        .replace("sub-action", "subaction")
        .replace("sub action", "subaction")
        .replace("event sub", "eventsub")
        .split(|c: char| !c.is_alphanumeric() && c != '#')
        .filter(|w| !w.is_empty())
        .map(str::to_string)
        .collect()
}

pub(super) const RULES: &[(&str, &[&str])] = &[
    ("clapperboard", &["clip", "clips"]),
    (
        "circle-check",
        &["unban", "untime", "untimeout", "approved", "permitted", "success"],
    ),
    ("ban", &["ban", "block", "denied", "refused", "censor"]),
    (
        "timer",
        &["timeout", "timedout", "timed", "timer", "countdown", "snooze", "=time"],
    ),
    (
        "hourglass",
        &["delay", "wait", "cooldown", "duration", "pending", "upcoming"],
    ),
    ("volume-x", &["mute", "musted"]),
    (
        "volume-2",
        &[
            "volume",
            "gain",
            "audio",
            "sound",
            "=hear",
            "noise",
            "monitoring",
            "meter",
        ],
    ),
    ("mic", &["microphone", "voice", "speech", "speak", "dictation", "say"]),
    ("swords", &["raid", "throw", "hit"]),
    ("flame", &["hype", "streak", "fire"]),
    (
        "coins",
        &["cheer", "bits", "coin", "balance", "points", "credits", "currency"],
    ),
    ("gift", &["gift", "present", "loots", "pack"]),
    (
        "trophy",
        &[
            "goal",
            "level",
            "milestone",
            "mile",
            "completed",
            "achievement",
            "winner",
        ],
    ),
    ("heart-pulse", &["pulse", "heartrate"]),
    (
        "heart",
        &[
            "heart", "pledge", "donation", "donor", "tip", "charity", "thank", "like", "liked",
        ],
    ),
    (
        "shopping-bag",
        &[
            "order",
            "purchase",
            "product",
            "shop",
            "merch",
            "cost",
            "commission",
            "vendor",
        ],
    ),
    (
        "star",
        &[
            "sub",
            "subscri",
            "resub",
            "member",
            "sponsor",
            "prime",
            "tier",
            "=star",
            "modiversary",
        ],
    ),
    ("vote", &["poll", "vote"]),
    ("trending-up", &["prediction", "statistics", "rate", "progress"]),
    ("user-plus", &["follow", "invite", "join", "welcome"]),
    (
        "shield",
        &[
            "mod",
            "vip",
            "moderator",
            "suspicious",
            "shield",
            "warn",
            "warned",
            "automod",
            "safe",
        ],
    ),
    (
        "megaphone",
        &[
            "announcement",
            "shoutout",
            "=ad",
            "ads",
            "commercial",
            "alert",
            "notify",
        ],
    ),
    ("bell", &["notification", "toast"]),
    (
        "smile",
        &[
            "emote",
            "tvemote",
            "ttvemote",
            "reaction",
            "sticker",
            "expression",
            "animation",
        ],
    ),
    (
        "message-square",
        &["chat", "message", "whisper", "reply", "comment", "note", "mention"],
    ),
    ("quote", &["quote"]),
    ("globe", &["webhook", "webook"]),
    (
        "video",
        &[
            "clip", "video", "record", "replay", "webcam", "marker", "chapter", "pre-roll",
        ],
    ),
    (
        "camera",
        &[
            "camera",
            "lens",
            "zoom",
            "pan",
            "tilt",
            "focus",
            "shutter",
            "flip",
            "mirror",
            "overscan",
            "orientation",
            "flicker",
            "exposure",
            "snapshot",
            "screenshot",
            "iso",
        ],
    ),
    (
        "image",
        &[
            "image",
            "icon",
            "background",
            "banner",
            "overlay",
            "gdi",
            "badge",
            "profile",
        ],
    ),
    (
        "sun",
        &[
            "color",
            "tint",
            "white",
            "saturation",
            "contrast",
            "brightness",
            "opacity",
            "sharpness",
            "bright",
        ],
    ),
    (
        "type",
        &[
            "font",
            "text",
            "title",
            "description",
            "spacing",
            "line",
            "name",
            "word",
            "caption",
            "margin",
        ],
    ),
    ("layers", &["scene", "layer", "slot", "deck"]),
    ("funnel", &["filter"]),
    (
        "music",
        &[
            "track", "song", "album", "artist", "playlist", "music", "media", "midi", "spotify", "queue",
        ],
    ),
    ("shuffle", &["shuffle", "random"]),
    ("repeat", &["repeat", "loop"]),
    (
        "skip-forward",
        &["next", "skip", "forward", "previous", "back", "rewind", "seek"],
    ),
    ("pause", &["pause"]),
    (
        "square",
        &["stop", "end", "terminate", "close", "cancel", "exiting", "exit"],
    ),
    (
        "play",
        &[
            "play", "start", "begin", "resume", "activate", "run", "execute", "launch", "open",
        ],
    ),
    ("radio", &["stream", "broadcast", "live", "online", "offline", "viewer"]),
    ("circle-off", &["disabled", "deactivate", "deactivated"]),
    ("unplug", &["disconnect", "broken"]),
    (
        "plug",
        &[
            "connect",
            "enabled",
            "server",
            "websocket",
            "udp",
            "mcp",
            "remote",
            "device",
        ],
    ),
    ("log-in", &["login", "authenticated", "auth", "logged"]),
    ("lock", &["lock", "locked", "expired", "token"]),
    ("calendar", &["schedule", "date", "age", "day"]),
    ("clock", &["last", "latest", "recent", "recently", "history"]),
    ("file-text", &["file", "folder", "log", "tail", "directory", "path"]),
    ("clipboard", &["clipboard", "paste"]),
    ("keyboard", &["key", "hotkey", "keyboard", "press", "held", "macro"]),
    (
        "mouse",
        &[
            "mouse",
            "click",
            "clicked",
            "crosshair",
            "wheel",
            "scroll",
            "cursor",
            "drag",
        ],
    ),
    (
        "globe",
        &["web", "url", "browser", "http", "page", "site", "link", "api", "fetch"],
    ),
    (
        "code",
        &[
            "code",
            "c#",
            "script",
            "raw",
            "expression",
            "regex",
            "javascript",
            "python",
        ],
    ),
    ("terminal", &["command", "commands", "if/else", "program", "process"]),
    (
        "braces",
        &[
            "variable", "global", "argument", "value", "json", "counter", "setting", "config", "arlens",
        ],
    ),
    ("git-branch", &["if", "switch", "case", "else", "condition", "compare"]),
    ("send-horizontal", &["send", "post", "push", "share", "export"]),
    ("download", &["download", "import", "load", "loaded"]),
    ("upload", &["upload", "save", "write"]),
    ("tag", &["tag", "category", "game", "label"]),
    (
        "users",
        &["user", "users", "group", "team", "community", "guest", "member"],
    ),
    (
        "trash-2",
        &[
            "delete", "deleted", "remove", "removed", "clear", "cleared", "reset", "purge",
        ],
    ),
    (
        "plus",
        &["add", "added", "create", "created", "new", "append", "assign"],
    ),
    ("pencil", &["rename", "renamed", "edit", "=change", "modify"]),
    (
        "refresh-cw",
        &[
            "changed",
            "update",
            "updated",
            "refresh",
            "sync",
            "ischanged",
            "isochanged",
            "switched",
        ],
    ),
    (
        "toggle-left",
        &[
            "state",
            "toggle",
            "enabled",
            "on",
            "off",
            "active",
            "status",
            "mode",
            "visible",
            "visibility",
            "hide",
            "show",
        ],
    ),
    ("info", &["info", "information", "current", "details"]),
    (
        "zap",
        &["trigger", "triggered", "action", "event", "events", "signal", "fired"],
    ),
];

/// The Lucide icon of an item, by the words of its title; `group` (the icon of its group) when no word
/// says more.
pub fn item(title: &str, group: &'static str) -> &'static str {
    let ws = words(title);
    for (icon, keys) in RULES {
        for w in &ws {
            for k in *keys {
                let hit = match k.strip_prefix('=') {
                    Some(exact) => w == exact,
                    None => w == k || (k.len() > 3 && w.starts_with(k)),
                };
                if hit {
                    return icon;
                }
            }
        }
    }
    group
}

#[cfg(test)]
mod tests {
    #[test]
    fn platforms_and_apps_wear_their_logos() {
        assert_eq!(super::logo("TikTok"), Some("brands/tiktok.svg"));
        assert_eq!(super::logo_of_category("Twitch/Chat"), Some("brands/twitch.svg"));
        assert_eq!(
            super::logo_of_category("Donations & Shops/Ko-fi"),
            Some("brands/ko-fi.png")
        );
        assert_eq!(super::logo_of_category("Core/General"), None);
        // Every logo named is an asset the window can draw.
        let root = concat!(env!("CARGO_MANIFEST_DIR"), "/assets/icons/");
        for name in [
            "Twitch",
            "Kick",
            "YouTube",
            "TikTok",
            "OBS Studio",
            "Streamlabs",
            "Spotify",
            "Discord",
            "StreamElements",
            "DonorDrive",
            "Streamer.bot",
            "Crowd Control",
            "Elgato",
            "Patreon",
            "Shopify",
            "Pally.gg",
            "Ko-fi",
            "Fourthwall",
            "HypeRate",
            "Pulsoid",
            "Meld Studio",
            "PolyPop",
            "Speaker.bot",
            "Voicemod",
            "VTube Studio",
        ] {
            let file = super::logo(name).unwrap().trim_start_matches("brands/");
            assert!(
                std::path::Path::new(&format!("{root}{file}")).is_file(),
                "{name}: {file}"
            );
        }
    }
}
