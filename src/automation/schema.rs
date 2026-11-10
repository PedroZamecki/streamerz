//! Automation field definitions shared by the engine, MCP and frontends.

use serde_json::Value;

/// What a field is, and so how it's edited.
#[derive(serde::Serialize)]
#[serde(tag = "kind", content = "options", rename_all = "camelCase")]
pub enum W {
    Text,
    /// A password or a token: hidden until its eye is clicked.
    Secret,
    /// Suggested values, with expressions still accepted.
    Expression(&'static [&'static str]),
    /// Several lines.
    Multi,
    Bool,
    /// One bit of a number of flags, as a check box.
    Bit(u64),
    Int,
    Num,
    /// Human durations; the stored number is in units of this many milliseconds.
    Duration(u64),
    /// One of these, stored as its index.
    Pick(&'static [&'static str]),
    /// An id, picked by name.
    Action,
    Queue,
    Timer,
    FileWatcher,
    FileTail,
    Command,
    UserGroup,
    Client,
    Server,
    ObsConnection,
    SlobsConnection,
    /// A sub-action or trigger of the action picked in the `actionId` field.
    Sub,
    Trigger,
    /// A name found on a connection (the catalog's list for this key: a channel's rewards, OBS's scenes),
    /// picked, or typed when nothing was found.
    Suggest(&'static str),
    /// Display a detected object's name, store its ID and retain unavailable selections.
    Entity(&'static str),
    /// A name, typed or picked.
    ActionGroup,
    CommandGroup,
    /// An object of texts, as `name=value` lines.
    Map,
    /// A list of texts, one per line.
    Lines,
    /// A key combination, captured: SharpHook's key code here, its modifier mask in `modifierMask`.
    Keys,
    /// A mouse button, caught by clicking it on the field (SharpHook's numbers; 0: any).
    MouseButton,
    /// Not a value (its key is empty): how this computer catches the Key Press (false) or Mouse Click
    /// (true) trigger, what it needs and why, with the button that gives or takes back the access.
    Access(bool),
}

use W::*;

impl W {
    /// What it holds, for the MCP server's descriptions.
    pub fn about(&self) -> String {
        match self {
            Text => "text".into(),
            Secret => "text (a password or token)".into(),
            Expression(values) => format!("text or expression (suggested: {values:?})"),
            Multi => "text (several lines)".into(),
            Bool => "true/false".into(),
            Bit(b) => format!("bit {b} of a number of flags"),
            Int => "whole number".into(),
            Num => "number".into(),
            Duration(unit) => format!(
                "duration, stored as a whole number of {}",
                if *unit == 1 { "milliseconds" } else { "seconds" }
            ),
            Pick(o) => format!(
                "index into {:?}",
                o.iter().map(|label| field_label(label)).collect::<Vec<_>>()
            ),
            Action => "action id".into(),
            Queue => "queue id".into(),
            Timer => "timer id".into(),
            FileWatcher => "file watcher id".into(),
            FileTail => "file tail id".into(),
            Command => "command id".into(),
            UserGroup => "user group name".into(),
            Client => "WebSocket client id".into(),
            Server => "WebSocket server id".into(),
            ObsConnection => "OBS connection id".into(),
            SlobsConnection => "Streamlabs Desktop connection id".into(),
            Sub => "sub-action id of the action in actionId".into(),
            Trigger => "trigger id of the action in actionId".into(),
            Suggest(k) => format!("a name ({k} detected on the connection, or typed)"),
            Entity(k) => format!("an ID ({k}, selected by name, or entered manually)"),
            ActionGroup => "action group name".into(),
            CommandGroup => "command group name".into(),
            Map => "object of texts".into(),
            Lines => "list of texts".into(),
            Keys => "key code (modifiers in modifierMask)".into(),
            MouseButton => "mouse button: 0 any, 1 left, 2 right, 3 middle, 4 back, 5 forward".into(),
            Access(_) => "nothing (a note)".into(),
        }
    }
}

/// (JSON field, label, kind).
pub type F = (&'static str, &'static str, W);

/// Imported options that are preserved but have no runtime effect. Never offer them as controls.
pub fn preserved_fields(kind: u64) -> &'static [&'static str] {
    match kind {
        5 => &["overwrite", "count"],
        18 => &["title", "duration"],
        _ => &[],
    }
}

pub(crate) fn field_label(label: &'static str) -> &'static str {
    if label.starts_with("spotify.") || label.starts_with("automation.") {
        return crate::i18n::tr(label);
    }
    match label {
        "countdown.id" => crate::i18n::tr("countdown.id"),
        "countdown.operation" => crate::i18n::tr("countdown.operation"),
        "countdown.start" => crate::i18n::tr("countdown.start"),
        "countdown.max" => crate::i18n::tr("countdown.max"),
        "countdown.per_unit" => crate::i18n::tr("countdown.per_unit"),
        "countdown.units" => crate::i18n::tr("countdown.units"),
        "countdown.currency" => crate::i18n::tr("countdown.currency"),
        _ => label,
    }
}

// In the order the engine reads them (Streamer.bot's stored values): 0 shows, starts or enables.
pub const STATE_HIDE_SHOW_TOGGLE: &[&str] = &["Show", "Hide", "Toggle"];
pub const STREAM_STATE: &[&str] = &["Start", "Stop", "Toggle"];
pub const RECORD_STATE: &[&str] = &["Start", "Stop", "Pause", "Resume", "Toggle"];
pub const REPLAY_STATE: &[&str] = &["Start", "Stop", "Save", "Toggle"];
// Streamlabs Desktop (checked against Streamer.bot): visibility/filters/mute 0 on, 1 off, 2 toggle;
// Streaming 0 stop, 1 start; Recording 0 start, 1 stop (Pause/Resume do nothing in Streamlabs).
const SD_SHOW: &[&str] = &["Visible", "Hidden", "Toggle"];
const SD_MUTE: &[&str] = &["Muted", "Not muted", "Toggle"];
const SD_STREAM: &[&str] = &["Stop", "Start"];
const SD_RECORD: &[&str] = &["Start", "Stop", "Pause", "Resume"];
const SD_DISPLAY: &[&str] = &["Both", "Horizontal", "Vertical"];
pub const FLIP_MODE: &[&str] = &["Horizontal", "Vertical", "Both"];
pub const FILTER_STATE: &[&str] = &["Enable", "Disable", "Toggle"];
pub const MUTE_STATE: &[&str] = &["Mute", "Unmute", "Toggle"];
pub const TRACK_STATE: &[&str] = &["Enable", "Disable", "Toggle"];
pub const MEDIA_STATE: &[&str] = &["Play", "Pause", "Restart", "Stop", "Next", "Previous"];

pub const OPS: &[&str] = &[
    "Equals",
    "Not Equals",
    "Contains",
    "Regex Match",
    "Less Than",
    "Greater Than",
    "Does Not Exist",
    "Equals (Ignore Case)",
    "Not Equals (Ignore Case)",
    "Is Null or Empty",
];
pub const WHILE_OPS: &[&str] = &["Equals", "Not Equals", "Less Than", "Greater Than"];
const STATE: &[&str] = &["Enable", "Disable", "Toggle"];
const WHO: &[&str] = &["Global", "User", "Target user"];
const PERSISTED: &[&str] = &["Any", "Persisted", "Temporary"];
// Serilog's levels.
const LEVELS: &[&str] = &["Verbose", "Debug", "Information", "Warning"];

/// A plugin's fields (a sub-action's, or its settings) as a form's; made once for each `key`.
pub fn plugin_fields(key: &str, fields: &[crate::engine::plugins::Field]) -> &'static [F] {
    use std::collections::HashMap;
    thread_local!(static MADE: std::cell::RefCell<HashMap<String, &'static [F]>> = Default::default());
    MADE.with_borrow_mut(|m| {
        *m.entry(key.to_string()).or_insert_with(|| {
            let leak = |s: &str| -> &'static str { Box::leak(s.to_string().into_boxed_str()) };
            let out: Vec<F> = fields
                .iter()
                .map(|f| {
                    let w = match f.kind.as_str() {
                        "lines" => Multi,
                        "number" => Num,
                        "bool" => Bool,
                        "choice" => {
                            let c: Vec<&'static str> = f.choices.iter().map(|c| leak(c)).collect();
                            Pick(Box::leak(c.into_boxed_slice()))
                        }
                        _ => Text,
                    };
                    let label = if f.label.is_empty() { &f.key } else { &f.label };
                    (leak(&f.key), leak(label), w)
                })
                .collect();
            Box::leak(out.into_boxed_slice())
        })
    })
}

/// A core sub-action's fields; None when it has no form (it opens as JSON).
pub fn sub_fields(kind: u64) -> Option<&'static [F]> {
    Some(match kind {
        99810..=99816 | 99830..=99833 => &[],
        99827 => &[("query", "spotify.query", Text)],
        99828 => &[
            ("who", "spotify.who", Text),
            ("skipOwn", "spotify.skip_own", Expression(&["true", "false"])),
            ("skipCooldownMs", "spotify.skip_cooldown", Text),
        ],
        99829 => &[
            ("votesNeeded", "spotify.votes_needed", Text),
            ("skipCooldownMs", "spotify.skip_cooldown", Text),
        ],
        99817 => &[("value", "spotify.position_ms", Text)],
        99818 => &[("value", "spotify.volume_percent", Text)],
        99819 => &[("value", "spotify.query", Text)],
        99820 => &[("value", "spotify.device_id", Text)],
        99821 => &[("value", "spotify.shuffle_state", Text)],
        99822 => &[("value", "spotify.repeat_state", Text)],
        99823..=99824 => &[],
        99841..=99842 | 99850..=99855 | 99860 | 99870 | 99874..=99875 => &[],
        99840 => &[("value", "spotify.play_from", Text)],
        99843 => &[("value", "spotify.volume_delta", Text)],
        99844 => &[("value", "spotify.seek_delta", Text)],
        99861 => &[
            ("value", "spotify.playlist_name", Text),
            ("extra", "spotify.playlist_description", Text),
        ],
        99862 | 99864 | 99865 => &[("value", "spotify.playlist", Text)],
        99863 => &[("value", "spotify.playlist", Text), ("extra", "spotify.query", Text)],
        99866 => &[("value", "spotify.playlist", Text), ("extra", "spotify.new_name", Text)],
        99871..=99872 => &[("value", "spotify.period", Text)],
        99873 => &[("value", "spotify.query", Text), ("extra", "spotify.search_type", Text)],
        99802 => &[
            (
                "operation",
                "spotify.operation",
                Expression(&[
                    "request", "remove", "current", "next", "queue", "pos", "skip", "voteskip", "play", "pause",
                    "volume", "bansong", "like",
                ]),
            ),
            ("query", "spotify.query", Text),
            ("who", "spotify.who", Text),
            ("reply", "spotify.reply", Text),
            ("replyDone", "spotify.reply_done", Text),
            ("maxPerUser", "spotify.max_per_user", Text),
            ("maxPerSubscriber", "spotify.max_per_subscriber", Text),
            ("unlimitedWho", "spotify.unlimited_who", Text),
            ("maxLengthMs", "spotify.max_length", Text),
            (
                "blockExplicit",
                "spotify.block_explicit",
                Expression(&["true", "false"]),
            ),
            ("explicitWho", "spotify.explicit_who", Text),
            ("blockedUsers", "spotify.blocked_users", Text),
            ("blockedArtists", "spotify.blocked_artists", Text),
            ("blockedSongs", "spotify.blocked_songs", Text),
            ("skipOwn", "spotify.skip_own", Expression(&["true", "false"])),
            ("skipCooldownMs", "spotify.skip_cooldown", Text),
            ("votesNeeded", "spotify.votes_needed", Text),
            ("msgNotAllowed", "spotify.msg_not_allowed", Text),
            ("msgNoQuery", "spotify.msg_no_query", Text),
            ("msgNotFound", "spotify.msg_not_found", Text),
            ("msgUnsupported", "spotify.msg_unsupported", Text),
            ("msgBlockedArtist", "spotify.msg_blocked_artist", Text),
            ("msgBlockedSong", "spotify.msg_blocked_song", Text),
            ("msgExplicit", "spotify.msg_explicit", Text),
            ("msgUnavailable", "spotify.msg_unavailable", Text),
            ("msgTooLong", "spotify.msg_too_long", Text),
            ("msgDuplicate", "spotify.msg_duplicate", Text),
            ("msgMaxRequests", "spotify.msg_max_requests", Text),
            ("msgSkipCooldown", "spotify.msg_skip_cooldown", Text),
            ("msgError", "spotify.msg_error", Text),
            ("msgEmpty", "spotify.msg_empty", Text),
            ("msgNoPos", "spotify.msg_no_pos", Text),
            ("msgNoRemove", "spotify.msg_no_remove", Text),
            ("msgNoVolume", "spotify.msg_no_volume", Text),
        ],
        99880 => &[("topic", "overlay.topic", Text), ("value", "overlay.value", Multi)],
        99803 => &[
            ("clockId", "countdown.id", Text),
            ("operation", "countdown.operation", Text),
            ("startMs", "countdown.start", Text),
            ("maxMs", "countdown.max", Text),
            ("perUnitMs", "countdown.per_unit", Text),
            ("units", "countdown.units", Text),
            ("expectedCurrency", "countdown.currency", Text),
        ],
        2 => &[("port", "Port", Int), ("payload", "Payload", Multi)],
        4 => &[
            ("actionId", "Action", Action),
            ("runImmedately", "Run immediately", Bool),
        ],
        6 => &[
            ("command", "Command", Text),
            ("arguments", "Arguments", Text),
            ("workingDir", "Working directory", Text),
            ("waitForExit", "automation.duration_wait_for_exit", Duration(1000)),
            ("envVars", "Environment (NAME=value)", Map),
        ],
        8 => &[("timerId", "Timer", Timer), ("state", "State", Pick(STATE))],
        9 => &[("commandId", "Command", Command), ("state", "State", Pick(STATE))],
        10 => &[
            ("text", "Message", Multi),
            ("pin", "Pin message", Bool),
            ("pinToStreamEnd", "Pin to stream end", Bool),
            ("useBot", "Send from bot account", Bool),
            ("fallback", "Fallback to broadcaster", Bool),
        ],
        11 => &[("status", "Status", Pick(&["Fulfilled", "Canceled"]))],
        12 => &[("status", "Subscriber only", Bool)],
        13 => &[("status", "Emote only", Bool)],
        14 => &[
            ("status", "Slow mode", Bool),
            ("duration", "automation.duration_duration", Duration(1000)),
        ],
        15 => &[("title", "Title", Text)],
        16 => &[
            ("source", "Source", Pick(&["Game name", "Game ID"])),
            ("game", "Game", Text),
            ("gameId", "Game ID", Text),
        ],
        17 => &[("description", "Description", Text)],
        18 => &[("hasDelay", "automation.clip_delay", Bool)],
        19 => &[("duration", "automation.duration_duration", Duration(1000))],
        20 => &[
            ("userLogin", "User login", Text),
            ("duration", "automation.duration_duration", Duration(1000)),
            ("reason", "Reason", Multi),
        ],
        21
        | 24
        | 26
        | 251
        | 501
        | 506..=510
        | 520
        | 522..=525
        | 551
        | 555..=557
        | 562
        | 563
        | 566
        | 567
        | 569
        | 570
        | 575
        | 578 => &[],
        22 => &[
            ("present", "Present users only", Bool),
            ("count", "Count", Int),
            ("includeFrom", "Include from", Text),
            ("groups", "Groups", Lines),
        ],
        23 => &[
            ("text", "Announcement", Multi),
            ("color", "Color (Primary/Blue/Green/Orange/Purple)", Text),
            ("useBot", "Send from bot account", Bool),
            ("fallback", "Fallback to broadcaster", Bool),
        ],
        25 => &[
            ("text", "Text", Multi),
            ("preferredAccount", "Preferred account", Text),
            ("color", "Color", Text),
        ],
        27 => &[("status", "Shield mode", Bool)],
        28 => &[
            ("status", "Follower mode", Bool),
            ("duration", "Duration (minutes)", Int),
        ],
        29 | 50 | 51 | 52 | 500 | 502..=505 | 512 | 513 | 526 | 527 | 580 => &[("userLogin", "User login", Text)],
        200 => &[
            ("rewardId", "Reward", Entity("twitch.rewards")),
            ("state", "State", Pick(STATE)),
        ],
        201 => &[
            ("rewardId", "Reward", Entity("twitch.rewards")),
            ("state", "State", Pick(STATE)),
        ],
        202 => &[
            ("rewardId", "Reward", Entity("twitch.rewards")),
            ("cost", "Cost", Int),
            ("operator", "Operation", Pick(&["Set", "Add", "Subtract"])),
        ],
        203 => &[("disableNotConfigured", "Disable not configured", Bool)],
        204 => &[
            ("rewardId", "Reward", Entity("twitch.rewards")),
            ("cooldown", "automation.duration_cooldown", Duration(1000)),
            ("additive", "Additive", Bool),
        ],
        205 => &[
            ("rewardId", "Reward", Entity("twitch.rewards")),
            ("title", "Title", Text),
        ],
        206 => &[
            ("rewardId", "Reward", Entity("twitch.rewards")),
            ("prompt", "Prompt", Multi),
        ],
        207 => &[
            ("rewardId", "Reward", Entity("twitch.rewards")),
            ("title", "Title", Text),
            ("prompt", "Prompt", Multi),
            ("cost", "Cost", Int),
            ("operator", "Operation", Pick(&["Set", "Add", "Subtract"])),
        ],
        208..=210 => &[("rewardId", "Reward", Entity("twitch.rewards"))],
        211 => &[("groupName", "Group name", Text), ("state", "State", Pick(STATE))],
        212 => &[("groupName", "Group name", Text), ("state", "State", Pick(STATE))],
        213 => &[
            ("rewardId", "Reward", Entity("twitch.rewards")),
            ("backgroundColor", "Background color", Text),
            ("random", "Random", Bool),
        ],
        214 | 215 => &[
            ("rewardId", "Reward", Entity("twitch.rewards")),
            ("redeems", "Max redeems", Int),
            ("additive", "Additive", Bool),
        ],
        252 => &[("tags", "Tags", Lines)],
        253 | 254 => &[("tag", "Tag", Text)],
        511 => &[("userLogin", "User login", Text), ("reason", "Reason", Multi)],
        521 => &[
            ("moderatorSendLiveEnabled", "Mod send live", Bool),
            ("slotCount", "Slot count", Int),
            ("browserSourceAudioEnabled", "Audio enabled", Bool),
            ("groupLayout", "Group layout", Text),
            ("regenerateBrowserSources", "Regenerate sources", Bool),
        ],
        528 => &[("userLogin", "User login", Text), ("slot", "Slot", Int)],
        529 => &[
            ("sourceSlot", "Source slot", Int),
            ("destinationSlot", "Destination slot", Int),
        ],
        530 => &[("userLogin", "User login", Text), ("slot", "Slot", Int)],
        531 => &[
            ("slot", "Slot", Int),
            ("audioEnabled", "Audio enabled", Bool),
            ("videoEnabled", "Video enabled", Bool),
            ("live", "Live", Bool),
            ("volume", "Volume", Num),
        ],
        550 => &[("userIndex", "User index", Int)],
        552 => &[("value", "Value", Int)],
        553 | 554 | 558 => &[("userLogin", "User login", Text), ("groupName", "Group", UserGroup)],
        560 => &[
            ("text", "Reply", Multi),
            ("replyId", "Reply message ID", Text),
            ("useBot", "Send from bot account", Bool),
            ("fallback", "Fallback to broadcaster", Bool),
        ],
        561 => &[
            ("title", "Title", Text),
            ("duration", "automation.duration_duration", Duration(1000)),
            ("channelPointsPerVote", "Points per vote", Int),
            ("choices", "Choices (one per line)", Lines),
        ],
        564 => &[("status", "Branded content", Bool)],
        565 => &[("userLogin", "User login", Text), ("reason", "Reason", Multi)],
        568 => &[
            ("title", "Title", Text),
            (
                "predictionWindow",
                "automation.duration_prediction_window",
                Duration(1000),
            ),
            ("outcomes", "Outcomes (one per line)", Lines),
        ],
        571 => &[("winningIndex", "Winning outcome index", Int)],
        572 => &[("outcomeTitle", "Winning outcome title", Text)],
        573 => &[("clipId", "Clip ID", Text)],
        576 => &[
            ("messageId", "Message ID", Text),
            ("duration", "automation.duration_duration", Duration(1000)),
        ],
        577 => &[("duration", "automation.duration_duration", Duration(1000))],
        579 => &[
            ("userLogin", "User login", Text),
            ("status", "Status", Pick(&["Monitored", "Restricted"])),
        ],

        120 => &[
            ("input", "Input", Text),
            ("operation", "Operation", Pick(OPS)),
            ("value", "Value", Text),
            ("autoType", "Auto type", Bool),
        ],
        121 => &[
            ("source", "Source", Pick(WHO)),
            ("variableName", "Variable name", Text),
            ("persisted", "Persisted", Bool),
            ("destinationVariable", "Destination variable", Text),
            ("defaultValue", "Default value", Text),
        ],
        122 => &[
            ("destination", "Destination", Pick(WHO)),
            ("variableName", "Variable name", Text),
            ("persisted", "Persisted", Bool),
            ("source", "Set", Pick(&["Value", "Increment", "Decrement", "Unset"])),
            ("value", "Value", Text),
            ("autoType", "Auto type", Bool),
        ],
        123 => &[
            ("variableName", "Variable name", Text),
            ("value", "Value", Multi),
            ("autoType", "Auto type", Bool),
        ],
        127 => &[("input", "Input", Text), ("autoType", "Auto type", Bool)],
        128 => &[
            ("input", "Input", Text),
            ("operation", "Operation", Pick(WHILE_OPS)),
            ("value", "Value", Text),
            ("autoType", "Auto type", Bool),
            ("ignoreCase", "Ignore case", Bool),
        ],
        124 | 125 | 126 | 129 | 130 => &[],
        1001 => &[
            ("actionQueueId", "Queue (none: all)", Queue),
            ("state", "State", Pick(&["Pause", "Resume"])),
            ("clear", "Clear the queue", Bool),
        ],
        1002 => &[
            ("value", "automation.duration_delay", Duration(1)),
            ("random", "Random", Bool),
            ("maxValue", "automation.duration_up_to", Duration(1)),
        ],
        1003 => &[
            ("randomType", "Type", Pick(&["Between", "Next Float"])),
            ("min", "Min", Int),
            ("max", "Max", Int),
        ],
        1004 => &[("actionId", "Action", Action), ("state", "State", Pick(STATE))],
        1005 => &[("actionQueueId", "Queue", Queue)],
        1007 => &[
            ("url", "URL", Text),
            ("variableName", "Variable name", Text),
            ("headers", "Headers (Name=value)", Map),
            ("parseAsJson", "Parse as JSON", Bool),
            ("autoType", "Auto type", Bool),
        ],
        1009 => &[("value", "Comment", Multi), ("color", "Color (#RRGGBB)", Text)],
        1010 => &[("groupName", "Group", CommandGroup), ("state", "State", Pick(STATE))],
        1011 => &[("groupName", "Group", ActionGroup), ("state", "State", Pick(STATE))],
        1013 => &[
            ("groupName", "Group", CommandGroup),
            ("variableName", "Variable name", Text),
            ("includeAll", "Include all commands", Bool),
            ("ignoreAliases", "First alias only", Bool),
            ("hasPermission", "Only permitted ones", Bool),
        ],
        1014 => &[("commandId", "Command", Command)],
        1015 => &[("groupName", "Group", CommandGroup)],
        1016 => &[("actionId", "Action", Action)],
        1017 => &[("groupName", "Group", ActionGroup)],
        1020 => &[("logLevel", "Level", Pick(LEVELS)), ("message", "Message", Multi)],
        1022 => &[
            ("actionQueueId", "Queue", Queue),
            ("state", "State", Pick(&["Blocking", "Non-blocking"])),
        ],
        1023 => &[
            ("title", "Title", Text),
            ("prompt", "Prompt", Text),
            ("originalText", "Starting text", Text),
            ("outputVariable", "Output variable", Text),
            ("autoTypeOutput", "Auto type", Bool),
        ],
        1028 => &[
            ("actionId", "Action", Action),
            ("subActionId", "Sub-action", Sub),
            ("state", "State", Pick(STATE)),
        ],
        1031 => &[
            ("signalName", "Signal", Text),
            ("useArge", "Send the arguments", Bool),
            ("customArgs", "Arguments (name=value)", Map),
            ("queueSignal", "Queue if nobody waits", Bool),
        ],
        1032 => &[
            ("signalName", "Signal", Text),
            ("overwrite", "Overwrite arguments", Bool),
            ("timeout", "automation.duration_timeout", Duration(1)),
        ],
        1033 => &[
            ("actionId", "Action", Action),
            ("triggerId", "Trigger", Trigger),
            ("state", "State", Pick(STATE)),
        ],
        1034 => &[
            ("eventName", "Event name", Text),
            ("useArgs", "Send the arguments", Bool),
        ],
        1038 => &[
            ("actionId", "Action", Action),
            ("subActionId", "Sub-action", Sub),
            ("weight", "Weight", Num),
        ],
        10001 => &[("groupId", "Group", UserGroup)],
        401 => &[
            ("connectionId", "Connection", SlobsConnection),
            ("sceneName", "Scene (empty: active)", Text),
            ("sourceName", "Source", Suggest("obs.sources")),
            ("state", "State", Pick(SD_SHOW)),
            ("display", "Display", Pick(SD_DISPLAY)),
        ],
        402 | 403 => &[
            ("connectionId", "Connection", SlobsConnection),
            ("sceneName", "Scene (empty: active)", Text),
            ("groupName", "Group", Text),
            ("display", "Display", Pick(SD_DISPLAY)),
        ],
        404 => &[
            ("connectionId", "Connection", SlobsConnection),
            ("state", "State", Pick(SD_STREAM)),
        ],
        405 => &[
            ("connectionId", "Connection", SlobsConnection),
            ("sceneName", "Scene", Suggest("obs.scenes")),
        ],
        406 => &[
            ("connectionId", "Connection", SlobsConnection),
            ("sceneName", "Scene (empty: active)", Text),
            ("sourceName", "Source", Suggest("obs.sources")),
            ("filterName", "Filter", Text),
            ("state", "State", Pick(SD_SHOW)),
        ],
        407 => &[
            ("connectionId", "Connection", SlobsConnection),
            ("flipMode", "Flip", Pick(FLIP_MODE)),
            ("sceneName", "Scene (empty: active)", Text),
            ("sourceName", "Source", Suggest("obs.sources")),
            ("display", "Display", Pick(SD_DISPLAY)),
        ],
        408 => &[
            ("connectionId", "Connection", SlobsConnection),
            ("degrees", "Degrees", Num),
            ("additive", "Additive (rotate by, not to)", Bool),
            ("sceneName", "Scene (empty: active)", Text),
            ("sourceName", "Source", Suggest("obs.sources")),
            ("display", "Display", Pick(SD_DISPLAY)),
        ],
        409 => &[
            ("connectionId", "Connection", SlobsConnection),
            ("sceneName", "Scene (empty: active)", Text),
            ("sourceName", "Source", Suggest("obs.sources")),
            ("url", "URL", Text),
        ],
        410 => &[
            ("connectionId", "Connection", SlobsConnection),
            ("sceneName", "Scene (empty: active)", Text),
            ("sourceName", "Source", Suggest("obs.sources")),
            ("text", "Text", Multi),
        ],
        411 => &[
            ("connectionId", "Connection", SlobsConnection),
            ("sceneName", "Scene (empty: active)", Text),
            ("sourceName", "Source", Suggest("obs.sources")),
            ("state", "State", Pick(SD_SHOW)),
        ],
        413 => &[
            ("connectionId", "Connection", SlobsConnection),
            ("sceneName", "Scene (empty: active)", Text),
            ("sourceName", "Source", Suggest("obs.sources")),
            ("state", "State", Pick(SD_MUTE)),
        ],
        412 => &[
            ("connectionId", "Connection", SlobsConnection),
            ("sceneName", "Scene (empty: active)", Text),
            ("sourceName", "Source", Suggest("obs.sources")),
        ],
        414 => &[("connectionId", "Connection", SlobsConnection)],
        415 => &[
            ("connectionId", "Connection", SlobsConnection),
            ("state", "State", Pick(SD_RECORD)),
        ],
        417 => &[
            ("connectionId", "Connection", SlobsConnection),
            ("sceneName", "Scene (empty: active)", Text),
            ("filterName", "Filter", Text),
            ("state", "State", Pick(SD_SHOW)),
        ],
        99900 => &[
            ("name", "Name", Text),
            ("random", "Random", Bool),
            ("color", "Color (#RRGGBB)", Text),
        ],
        30 => &[
            ("connectionId", "Connection", ObsConnection),
            ("sceneName", "Scene", Suggest("obs.scenes")),
            ("sourceName", "Source", Suggest("obs.sources")),
            ("state", "State", Pick(STATE_HIDE_SHOW_TOGGLE)),
        ],
        31 => &[
            ("connectionId", "Connection", ObsConnection),
            ("sceneName", "Scene", Suggest("obs.scenes")),
            ("groupName", "Group", Text),
        ],
        32 => &[
            ("connectionId", "Connection", ObsConnection),
            ("sceneName", "Scene", Suggest("obs.scenes")),
            ("groupName", "Group", Text),
        ],
        33 => &[
            ("connectionId", "Connection", ObsConnection),
            ("state", "State", Pick(STREAM_STATE)),
        ],
        34 => &[
            ("connectionId", "Connection", ObsConnection),
            ("sceneName", "Scene", Suggest("obs.scenes")),
        ],
        35 => &[
            ("connectionId", "Connection", ObsConnection),
            ("sceneName", "Scene", Suggest("obs.scenes")),
            ("sourceName", "Source", Suggest("obs.sources")),
            ("filterName", "Filter", Text),
            ("state", "State", Pick(FILTER_STATE)),
        ],
        36 => &[
            ("connectionId", "Connection", ObsConnection),
            ("flipMode", "Flip mode", Pick(FLIP_MODE)),
            ("sceneName", "Scene", Suggest("obs.scenes")),
            ("sourceName", "Source", Suggest("obs.sources")),
        ],
        37 => &[
            ("connectionId", "Connection", ObsConnection),
            ("degrees", "Degrees", Num),
            ("additive", "Additive", Bool),
            ("sceneName", "Scene", Suggest("obs.scenes")),
            ("sourceName", "Source", Suggest("obs.sources")),
        ],
        38 => &[
            ("connectionId", "Connection", ObsConnection),
            ("sceneName", "Scene", Suggest("obs.scenes")),
            ("sourceName", "Source", Suggest("obs.sources")),
            ("url", "URL", Text),
        ],
        39 => &[
            ("connectionId", "Connection", ObsConnection),
            ("sceneName", "Scene", Suggest("obs.scenes")),
            ("sourceName", "Source", Suggest("obs.sources")),
            ("text", "Text", Text),
        ],
        40 => &[
            ("connectionId", "Connection", ObsConnection),
            ("sceneName", "Scene", Suggest("obs.scenes")),
            ("sourceName", "Source", Suggest("obs.sources")),
            ("state", "State", Pick(FILTER_STATE)),
        ],
        41 => &[
            ("connectionId", "Connection", ObsConnection),
            ("sceneName", "Scene", Suggest("obs.scenes")),
            ("sourceName", "Source", Suggest("obs.sources")),
        ],
        42 => &[
            ("connectionId", "Connection", ObsConnection),
            ("sceneName", "Scene", Suggest("obs.scenes")),
            ("sourceName", "Source", Suggest("obs.sources")),
            ("state", "State", Pick(MUTE_STATE)),
        ],
        43 => &[("connectionId", "Connection", ObsConnection)],
        44 => &[
            ("connectionId", "Connection", ObsConnection),
            ("sceneName", "Scene", Suggest("obs.scenes")),
            ("sourceName", "Source", Suggest("obs.sources")),
            (
                "track",
                "Track",
                Pick(&["Track 1", "Track 2", "Track 3", "Track 4", "Track 5", "Track 6"]),
            ),
            ("state", "State", Pick(TRACK_STATE)),
        ],
        45 => &[
            ("connectionId", "Connection", ObsConnection),
            ("state", "State", Pick(RECORD_STATE)),
        ],
        46 => &[
            ("connectionId", "Connection", ObsConnection),
            ("name", "Request type", Text),
            ("prefix", "Prefix", Text),
            ("raw", "Raw JSON", Text),
            ("resultsToArgs", "Results to args", Bool),
        ],
        47 => &[
            ("connectionId", "Connection", ObsConnection),
            ("sceneName", "Scene", Suggest("obs.scenes")),
            ("filterName", "Filter", Text),
            ("state", "State", Pick(FILTER_STATE)),
        ],
        48 => &[
            ("connectionId", "Connection", ObsConnection),
            ("sceneName", "Scene", Suggest("obs.scenes")),
            ("sourceName", "Source", Suggest("obs.sources")),
            ("path", "File path", Text),
            ("quality", "Quality", Int),
        ],
        49 => &[
            ("connectionId", "Connection", ObsConnection),
            ("sceneName", "Scene", Suggest("obs.scenes")),
            ("sourceName", "Source", Suggest("obs.sources")),
            ("state", "State", Pick(MEDIA_STATE)),
        ],
        321 => &[
            ("connectionId", "Connection", ObsConnection),
            ("state", "State", Pick(REPLAY_STATE)),
        ],
        322 => &[
            ("connectionId", "Connection", ObsConnection),
            ("sceneName", "Scene", Suggest("obs.scenes")),
            ("sourceName", "Source", Suggest("obs.sources")),
            ("fileName", "File name", Text),
        ],
        323 => &[
            ("connectionId", "Connection", ObsConnection),
            ("sceneName", "Scene", Suggest("obs.scenes")),
            ("sourceName", "Source", Suggest("obs.sources")),
            ("fileName", "File name", Text),
        ],
        324 => &[
            ("connectionId", "Connection", ObsConnection),
            ("sceneName", "Scene", Suggest("obs.scenes")),
            ("sourceName", "Source", Suggest("obs.sources")),
        ],
        325 => &[
            ("connectionId", "Connection", ObsConnection),
            ("sceneName", "Scene", Suggest("obs.scenes")),
        ],
        326 => &[
            ("connectionId", "Connection", ObsConnection),
            ("sceneName", "Scene", Suggest("obs.scenes")),
        ],
        327 => &[
            ("connectionId", "Connection", ObsConnection),
            ("sceneName", "Scene", Suggest("obs.scenes")),
            ("sourceName", "Source", Suggest("obs.sources")),
            ("color", "Color", Text),
            ("random", "Random", Bool),
        ],
        328 => &[("connectionId", "Connection", ObsConnection)],
        329 => &[
            ("connectionId", "Connection", ObsConnection),
            ("chapterName", "Chapter name", Text),
        ],
        4001 => &[
            ("broadcast", "Broadcast", Text),
            ("text", "Message", Text),
            ("useBot", "Use Bot Account", Bool),
            ("fallback", "Fallback to Broadcaster", Bool),
        ],
        4002 => &[("broadcast", "Broadcast", Text), ("title", "Title", Text)],
        4003 => &[("broadcast", "Broadcast", Text), ("description", "Description", Text)],
        4004 => &[
            ("broadcast", "Broadcast", Text),
            ("title", "Title", Text),
            ("description", "Description", Text),
        ],
        4005..=4007 => &[("userLogin", "User login", Text), ("groupName", "Group name", Text)],
        4008 => &[("broadcast", "Broadcast", Text), ("userId", "User ID", Text)],
        4009 => &[
            ("broadcast", "Broadcast", Text),
            ("userId", "User ID", Text),
            ("duration", "automation.duration_duration", Duration(1000)),
        ],
        4010 => &[],
        4011 => &[
            ("broadcast", "Broadcast", Text),
            ("question", "Question", Text),
            ("options", "Options (one per line)", Lines),
        ],
        4012 => &[("broadcast", "Broadcast", Text)],
        35001 => &[
            ("text", "Message", Text),
            ("useBot", "Use Bot Account", Bool),
            ("fallback", "Fallback to Broadcaster", Bool),
        ],
        35002 => &[
            ("text", "Message", Text),
            ("useBot", "Use Bot Account", Bool),
            ("fallback", "Fallback to Broadcaster", Bool),
            ("replyId", "Reply Message ID", Text),
        ],
        35010 => &[],
        35011 => &[("userLogin", "User login", Text)],
        35012 => &[
            ("present", "Present users only", Bool),
            ("count", "Count", Int),
            ("includeFrom", "Include from", Int),
            ("groups", "Groups (one per line)", Lines),
        ],
        35020..=35022 => &[("userLogin", "User login", Text), ("groupName", "Group name", Text)],
        35030 => &[("title", "Title", Text)],
        35031 => &[
            ("source", "Source", Int),
            ("categoryName", "Category name", Text),
            ("categoryId", "Category ID", Text),
        ],
        35040 => &[("userLogin", "User login", Text), ("reason", "Reason", Text)],
        35041 | 35043 => &[("userLogin", "User login", Text)],
        35042 => &[
            ("userLogin", "User login", Text),
            ("duration", "automation.duration_duration", Duration(1000)),
            ("reason", "Reason", Text),
        ],
        35101 | 35102 => &[("rewardId", "Reward", Entity("kick.rewards")), ("state", "State", Bool)],
        35103 => &[
            ("rewardId", "Reward", Entity("kick.rewards")),
            ("cost", "Cost", Int),
            ("operator", "Operator", Int),
        ],
        35104 => &[("disableNotConfigured", "Disable not configured", Bool)],
        35105 => &[("rewardId", "Reward", Entity("kick.rewards")), ("title", "Title", Text)],
        35106 => &[
            ("rewardId", "Reward", Entity("kick.rewards")),
            ("description", "Description", Text),
        ],
        35107 => &[
            ("rewardId", "Reward", Entity("kick.rewards")),
            ("title", "Title", Text),
            ("description", "Description", Text),
            ("cost", "Cost", Int),
            ("operator", "Operator", Int),
        ],
        35108..=35110 => &[("rewardId", "Reward", Entity("kick.rewards"))],
        35111 | 35112 => &[("groupName", "Group name", Text), ("state", "State", Bool)],
        35113 => &[
            ("rewardId", "Reward", Entity("kick.rewards")),
            ("backgroundColor", "Background color", Text),
            ("random", "Random", Bool),
        ],
        43002 => &[("count", "How many", Int)],
        43003 | 1026 | 1027 | 1035 => &[],
        1000 => &[
            ("quoteType", "Type", Pick(&["Random", "Specific"])),
            ("quoteId", "Quote ID (Specific)", Int),
        ],
        1036 => &[("quoteId", "Quote ID", Int)],
        1037 => &[("quoteVariable", "Quote", Text)],
        43001 => &[("text", "Message", Text)],
        7001 => &[
            ("name", "Name", Text),
            ("username", "Username", Text),
            ("webhookUrl", "Webhook URL", Text),
            ("content", "Content", Multi),
            ("image", "Image URL", Text),
            ("avatar", "Avatar URL", Text),
            ("textToSpeech", "Text-to-Speech", Bool),
        ],
        8001 => &[("eventName", "Event Name", Text), ("text", "Text", Multi)],
        8002 => &[("eventName", "Event Name", Text), ("args", "Arguments", Map)],
        15001 => &[("userLogin", "User Login", Text), ("prefix", "Prefix", Text)],
        20001 => &[
            ("instanceId", "Instance", Text),
            ("eventName", "Event Name", Text),
            ("useArge", "Use current arguments", Bool),
            ("customArgs", "Custom arguments", Map),
        ],
        20002 => &[
            ("instanceId", "Instance", Text),
            ("signalName", "Signal Name", Text),
            ("useArge", "Use current arguments", Bool),
            ("customArgs", "Custom arguments", Map),
        ],
        20003 => &[
            ("instanceId", "Instance", Text),
            ("signalName", "Signal Name", Text),
            ("overwrite", "Overwrite", Bool),
            ("timeout", "automation.duration_timeout", Duration(1)),
        ],
        1012 => &[("key", "Key", Text), ("modifiers", "Modifiers", Int)],
        9001 => &[
            ("device", "Device (none: the first)", Suggest("midi.outputs")),
            ("name", "Name", Text),
            ("sendNoteOff", "Send Note Off", Bool),
            ("channel", "Channel", Int),
            ("note", "Note", Int),
            ("velocity", "Velocity", Int),
            ("duration", "automation.duration_duration", Duration(1)),
        ],
        9002 => &[
            ("device", "Device (none: the first)", Suggest("midi.outputs")),
            ("name", "Name", Text),
            ("channel", "Channel", Int),
            ("controller", "Controller", Int),
            ("value", "Value", Int),
        ],
        9003 => &[
            ("device", "Device (none: the first)", Suggest("midi.outputs")),
            ("name", "Name", Text),
            ("command", "Command", Text),
            ("channel", "Channel", Int),
            ("data1", "Data 1", Int),
            ("data2", "Data 2", Int),
        ],
        11001 => &[
            ("buttonId", "Button ID", Text),
            ("title", "Title", Text),
            ("state", "State", Pick(STATE)),
        ],
        11002 => &[
            ("buttonId", "Button ID", Text),
            ("image", "Image", Text),
            ("color", "Color", Text),
            ("state", "State", Pick(STATE)),
        ],
        11004 => &[("buttonId", "Button ID", Text), ("state", "State", Pick(STATE))],
        11005 => &[("buttonId", "Button ID", Text)],
        11006 => &[("buttonId", "Button ID", Text)],
        11007 => &[("buttonId", "Button ID", Text)],
        11008 => &[("buttonId", "Button ID", Text), ("value", "Value", Text)],
        21001 => &[
            ("deckId", "Deck ID", Text),
            ("deckName", "Deck Name", Text),
            ("itemId", "Item ID", Text),
            ("itemName", "Item Name", Text),
            ("state", "State", Pick(STATE)),
        ],
        21002 => &[
            ("deckId", "Deck ID", Text),
            ("deckName", "Deck Name", Text),
            ("itemId", "Item ID", Text),
            ("itemName", "Item Name", Text),
        ],
        21003 => &[
            ("deckId", "Deck ID", Text),
            ("deckName", "Deck Name", Text),
            ("itemId", "Item ID", Text),
            ("itemName", "Item Name", Text),
            ("color", "Color", Text),
            ("title", "Title", Text),
            ("state", "State", Pick(STATE)),
        ],
        21004 => &[
            ("deckId", "Deck ID", Text),
            ("deckName", "Deck Name", Text),
            ("itemId", "Item ID", Text),
            ("itemName", "Item Name", Text),
            ("color", "Color", Text),
            ("url", "URL", Text),
            ("fileId", "File ID", Text),
            ("state", "State", Pick(STATE)),
        ],
        21005 => &[
            ("deckId", "Deck ID", Text),
            ("deckName", "Deck Name", Text),
            ("itemId", "Item ID", Text),
            ("itemName", "Item Name", Text),
            ("color", "Color", Text),
            ("name", "Name", Text),
            ("url", "URL", Text),
            ("fileId", "File ID", Text),
            ("state", "State", Pick(STATE)),
        ],
        21006 => &[
            ("deckId", "Deck ID", Text),
            ("deckName", "Deck Name", Text),
            ("itemId", "Item ID", Text),
            ("itemName", "Item Name", Text),
            ("value", "Value", Text),
            ("state", "State", Pick(STATE)),
        ],
        21007 => &[
            ("deckId", "Deck ID", Text),
            ("deckName", "Deck Name", Text),
            ("title", "Title", Text),
            ("description", "Description", Multi),
            ("color", "Color", Text),
            ("soundId", "Sound ID", Text),
        ],
        21008 => &[
            ("deckId", "Deck ID", Text),
            ("deckName", "Deck Name", Text),
            ("page", "Page", Int),
        ],
        21009 => &[("deckId", "Deck ID", Text), ("deckName", "Deck Name", Text)],
        21010 => &[("deckId", "Deck ID", Text), ("deckName", "Deck Name", Text)],
        3 => &[
            ("file", "File", Text),
            ("text", "Text", Multi),
            ("append", "Append", Bool),
        ],
        5 => &[
            ("file", "File", Text),
            ("parseVariables", "Parse Variables", Bool),
            ("autoType", "Auto Type", Bool),
            ("variableName", "Variable Name", Text),
        ],
        1006 => &[
            ("file", "File", Text),
            ("parseVariables", "Parse Variables", Bool),
            ("autoType", "Auto Type", Bool),
            ("variableName", "Variable Name", Text),
        ],
        1021 => &[
            ("file", "File", Text),
            ("parseVariables", "Parse Variables", Bool),
            ("autoType", "Auto Type", Bool),
            ("count", "Line Number", Int),
            ("variableName", "Variable Name", Text),
        ],
        1030 => &[("fileTailId", "File tail", FileTail), ("state", "State", Pick(STATE))],
        1024 => &[
            ("toastId", "Toast id", Text),
            ("title", "Title", Text),
            ("text", "Text", Multi),
            ("attribution", "Attribution (empty: Streamerz)", Text),
            ("iconPath", "Icon file (empty: Streamerz's)", Text),
        ],
        1039 => &[("file", "File", Text), ("variableName", "Variable Name", Text)],
        1040 => &[("folder", "Folder", Text), ("variableName", "Variable Name", Text)],
        99901 | 99902 | 99904 => &[("random", "Random", Bool)],
        99903 => &[
            ("values", "Values (one per line)", Lines),
            ("caseSensitive", "Case sensitive", Bool),
            ("random", "Random", Bool),
        ],
        _ => return None,
    })
}

/// A core trigger's fields; None when it has no form.
pub fn trigger_fields(kind: u64) -> Option<&'static [F]> {
    Some(match kind {
        401 | 402 => &[("commandId", "Command", Command)],
        701 => &[("timerId", "Timer", Timer)],
        702 | 706 | 99804..=99809 | 99825..=99826 | 99830..=99842 => &[],
        709 | 711 | 712 => &[
            ("variableName", "Variable (none: any)", Text),
            ("persisted", "Persisted", Pick(PERSISTED)),
        ],
        710 => &[
            ("variableName", "Variable (none: any)", Text),
            ("persisted", "Persisted", Pick(PERSISTED)),
            ("userLogon", "User login (none: any)", Text),
            ("platform", "Platform (none: any)", Text),
        ],
        803..=805 => &[("clientId", "Client", Client)],
        2001..=2003 => &[("serverId", "Server", Server)],
        18001 => &[("eventName", "Event name", Text)],
        18002 => &[("triggerName", "Trigger name", Text)],
        32001..=32005 => &[("groupId", "Group (none: any)", UserGroup)],
        14001 | 14002 | 14005 | 14006 | 14007 | 14008 => &[("connectionId", "Connection (empty: any)", ObsConnection)],
        22001 | 22002 | 22004..=22007 => &[("sdId", "Connection (none: any)", SlobsConnection)],
        22003 => &[
            ("sdId", "Connection (none: any)", SlobsConnection),
            ("sceneName", "Scene (empty: any)", Text),
        ],
        14003 => &[
            ("connectionId", "Connection (empty: any)", ObsConnection),
            ("eventType", "Event type (empty: any)", Text),
        ],
        14004 => &[
            ("connectionId", "Connection (empty: any)", ObsConnection),
            ("sceneName", "Scene (empty: any)", Text),
        ],
        14009 => &[
            ("connectionId", "Connection (empty: any)", ObsConnection),
            ("vendorName", "Vendor (empty: any)", Text),
            ("eventType", "Event type (empty: any)", Text),
        ],
        112..=115 | 171 => &[("rewardId", "Reward (none: any)", Entity("twitch.rewards"))],
        35024 => &[("rewardId", "Reward (none: any)", Entity("kick.rewards"))],
        41002 => &[
            ("giftId", "automation.gift", Entity("tiktok.gifts")),
            (
                "giftDelivery",
                "automation.delivery",
                Pick(&["automation.every_update", "automation.completed_combo"]),
            ),
            ("minCoins", "automation.min_coins", Int),
            ("maxCoins", "automation.max_coins", Int),
        ],
        41001..=41012 => &[],
        101..=196
        | 451..=498
        | 4001..=4031
        | 35001..=35003
        | 35008..=35025
        | 202..=204
        | 206
        | 1202..=1205
        | 3001..=3003
        | 7001..=7005
        | 8001..=8005
        | 10001
        | 11001..=11004
        | 13001..=13002
        | 24001..=24013
        | 28001..=28003
        | 36001..=36004
        | 37001..=37003 => &[],
        // Range: neither (any amount), only Max (exactly it), only Min (at least it), or both.
        201 | 205 | 1201 => &[
            ("min", "Min amount (0: none)", Num),
            ("max", "Max amount (0: none)", Num),
        ],
        29001 | 29002 => &[("instanceId", "Instance", Text)],
        29003 => &[("instanceId", "Instance", Text), ("eventName", "Event Name", Text)],
        29004 => &[("instanceId", "Instance", Text), ("signalName", "Signal Name", Text)],
        15001 => &[
            ("device", "Device (none: any)", Suggest("midi.inputs")),
            ("commandCode", "Command (none: any)", Pick(crate::midi::COMMANDS)),
            ("channel", "Channel (0: any)", Int),
            ("data1", "Note or controller (none: any)", Text),
        ],
        16001 => &[("keyCode", "Keys", Keys), ("", "", Access(false))],
        16002 => &[("button", "Button", MouseButton), ("", "", Access(true))],
        705 => &[("actionName", "Action Name", Text)],
        17001 => &[("buttonId", "Button ID", Text)],
        17002 => &[("device", "Device", Text)],
        17003 => &[("device", "Device", Text)],
        17004 => &[("device", "Device", Text)],
        34001 => &[("webhookName", "Webhook name (none: any)", Text)],
        501..=504 => &[("watcherId", "Watcher (none: any)", FileWatcher)],
        511 => &[("tailId", "File tail (none: any)", FileTail)],
        703 | 704 => &[("processName", "Process Name", Text)],
        code if crate::docs::available("trigger", code as u32) => &[],
        _ => return None,
    })
}

pub(crate) fn parse_duration_field(value: &str, unit: u64) -> Result<Value, String> {
    let trimmed = value.trim();
    if trimmed.contains(['%', '~', '$']) {
        return Ok(Value::from(trimmed));
    }
    let ms = if trimmed.is_empty() {
        Some(0)
    } else {
        crate::groups::parse_duration(trimmed)
    }
    .filter(|ms| ms.is_multiple_of(unit))
    .ok_or_else(|| crate::i18n::tr("automation.duration_error").to_string())?;
    Ok(Value::from(ms / unit))
}
