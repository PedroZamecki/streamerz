//! Forms for the core sub-actions and triggers: one row per field, from a table. Fields not in the
//! table are kept as they are; "Edit JSON" opens the whole object instead.

use super::dialogs::{ok_cancel, row, run, window};
use super::{button, ROW};
use crate::model::Obj;
use crate::Config;
use fltk::button::CheckButton;
use fltk::frame::Frame;
use fltk::group::Flex;
use fltk::input::{FloatInput, Input, IntInput, MultilineInput};
use fltk::menu::Choice;
use fltk::misc::InputChoice;
use fltk::prelude::*;
use serde_json::{json, Value};
use std::cell::{Cell, RefCell};
use std::rc::Rc;

/// What a field is, and so how it's edited.
pub enum W {
    Text,
    /// Suggested values, with expressions still accepted.
    Expression(&'static [&'static str]),
    /// Several lines.
    Multi,
    Bool,
    /// One bit of a number of flags, as a check box.
    Bit(u64),
    Int,
    Num,
    /// One of these, stored as its index.
    Pick(&'static [&'static str]),
    /// An id, picked by name.
    Action,
    Queue,
    Timer,
    FileWatcher,
    FileTail,
    VoiceCommand,
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
            Expression(values) => format!("text or expression (suggested: {values:?})"),
            Multi => "text (several lines)".into(),
            Bool => "true/false".into(),
            Bit(b) => format!("bit {b} of a number of flags"),
            Int => "whole number".into(),
            Num => "number".into(),
            Pick(o) => format!("index into {o:?}"),
            Action => "action id".into(),
            Queue => "queue id".into(),
            Timer => "timer id".into(),
            FileWatcher => "file watcher id".into(),
            FileTail => "file tail id".into(),
            VoiceCommand => "voice command id".into(),
            Command => "command id".into(),
            UserGroup => "user group name".into(),
            Client => "WebSocket client id".into(),
            Server => "WebSocket server id".into(),
            ObsConnection => "OBS connection id".into(),
            SlobsConnection => "Streamlabs Desktop connection id".into(),
            Sub => "sub-action id of the action in actionId".into(),
            Trigger => "trigger id of the action in actionId".into(),
            Suggest(k) => format!("a name ({k} detected on the connection, or typed)"),
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

pub(super) fn field_label(label: &'static str) -> &'static str {
    if label.starts_with("spotify.") {
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
            ("waitForExit", "Wait for exit (s)", Int),
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
        14 => &[("status", "Slow mode", Bool), ("duration", "Duration (seconds)", Int)],
        15 => &[("title", "Title", Text)],
        16 => &[
            ("source", "Source", Pick(&["Game name", "Game ID"])),
            ("game", "Game", Text),
            ("gameId", "Game ID", Text),
        ],
        17 => &[("description", "Description", Text)],
        18 => &[("title", "Title", Text), ("duration", "Duration (seconds)", Int)],
        19 => &[("duration", "Duration (seconds)", Int)],
        20 => &[
            ("userLogin", "User login", Text),
            ("duration", "Duration (seconds)", Int),
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
            ("rewardId", "Reward", Suggest("twitch.rewards")),
            ("state", "State", Pick(STATE)),
        ],
        201 => &[
            ("rewardId", "Reward", Suggest("twitch.rewards")),
            ("state", "State", Pick(STATE)),
        ],
        202 => &[
            ("rewardId", "Reward", Suggest("twitch.rewards")),
            ("cost", "Cost", Int),
            ("operator", "Operation", Pick(&["Set", "Add", "Subtract"])),
        ],
        203 => &[("disableNotConfigured", "Disable not configured", Bool)],
        204 => &[
            ("rewardId", "Reward", Suggest("twitch.rewards")),
            ("cooldown", "Cooldown (seconds)", Int),
            ("additive", "Additive", Bool),
        ],
        205 => &[
            ("rewardId", "Reward", Suggest("twitch.rewards")),
            ("title", "Title", Text),
        ],
        206 => &[
            ("rewardId", "Reward", Suggest("twitch.rewards")),
            ("prompt", "Prompt", Multi),
        ],
        207 => &[
            ("rewardId", "Reward", Suggest("twitch.rewards")),
            ("title", "Title", Text),
            ("prompt", "Prompt", Multi),
            ("cost", "Cost", Int),
            ("operator", "Operation", Pick(&["Set", "Add", "Subtract"])),
        ],
        208..=210 => &[("rewardId", "Reward", Suggest("twitch.rewards"))],
        211 => &[("groupName", "Group name", Text), ("state", "State", Pick(STATE))],
        212 => &[("groupName", "Group name", Text), ("state", "State", Pick(STATE))],
        213 => &[
            ("rewardId", "Reward", Suggest("twitch.rewards")),
            ("backgroundColor", "Background color", Text),
            ("random", "Random", Bool),
        ],
        214 | 215 => &[
            ("rewardId", "Reward", Suggest("twitch.rewards")),
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
            ("duration", "Duration (seconds)", Int),
            ("channelPointsPerVote", "Points per vote", Int),
            ("choices", "Choices (one per line)", Lines),
        ],
        564 => &[("status", "Branded content", Bool)],
        565 => &[("userLogin", "User login", Text), ("reason", "Reason", Multi)],
        568 => &[
            ("title", "Title", Text),
            ("predictionWindow", "Prediction window (seconds)", Int),
            ("outcomes", "Outcomes (one per line)", Lines),
        ],
        571 => &[("winningIndex", "Winning outcome index", Int)],
        572 => &[("outcomeTitle", "Winning outcome title", Text)],
        573 => &[("clipId", "Clip ID", Text)],
        576 => &[
            ("messageId", "Message ID", Text),
            ("duration", "Duration (seconds)", Int),
        ],
        577 => &[("duration", "Duration (seconds)", Int)],
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
            ("value", "Delay (ms)", Int),
            ("random", "Random", Bool),
            ("maxValue", "Up to (ms)", Int),
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
            ("timeout", "Timeout (ms)", Int),
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
            ("duration", "Duration (seconds)", Int),
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
            ("duration", "Duration (seconds)", Int),
            ("reason", "Reason", Text),
        ],
        35101 | 35102 => &[
            ("rewardId", "Reward", Suggest("kick.rewards")),
            ("state", "State", Bool),
        ],
        35103 => &[
            ("rewardId", "Reward", Suggest("kick.rewards")),
            ("cost", "Cost", Int),
            ("operator", "Operator", Int),
        ],
        35104 => &[("disableNotConfigured", "Disable not configured", Bool)],
        35105 => &[
            ("rewardId", "Reward", Suggest("kick.rewards")),
            ("title", "Title", Text),
        ],
        35106 => &[
            ("rewardId", "Reward", Suggest("kick.rewards")),
            ("description", "Description", Text),
        ],
        35107 => &[
            ("rewardId", "Reward", Suggest("kick.rewards")),
            ("title", "Title", Text),
            ("description", "Description", Text),
            ("cost", "Cost", Int),
            ("operator", "Operator", Int),
        ],
        35108..=35110 => &[("rewardId", "Reward", Suggest("kick.rewards"))],
        35111 | 35112 => &[("groupName", "Group name", Text), ("state", "State", Bool)],
        35113 => &[
            ("rewardId", "Reward", Suggest("kick.rewards")),
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
            ("timeout", "Timeout (ms)", Int),
        ],
        1012 => &[("key", "Key", Text), ("modifiers", "Modifiers", Int)],
        1018 => &[
            ("commandId", "Voice command", VoiceCommand),
            ("command", "Phrases (one per line)", Multi),
        ],
        1019 => &[
            ("commandId", "Voice command", VoiceCommand),
            ("state", "State", Pick(STATE)),
        ],
        1025 => &[
            ("audioDeviceId", "Audio Device ID", Text),
            ("audioDeviceName", "Audio Device Name", Text),
        ],
        9001 => &[
            ("device", "Device (none: the first)", Suggest("midi.outputs")),
            ("name", "Name", Text),
            ("sendNoteOff", "Send Note Off", Bool),
            ("channel", "Channel", Int),
            ("note", "Note", Int),
            ("velocity", "Velocity", Int),
            ("duration", "Duration (ms)", Int),
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
            ("overwrite", "Overwrite", Bool),
            ("count", "Count", Int),
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
        112..=115 | 171 => &[("rewardId", "Reward (none: any)", Suggest("twitch.rewards"))],
        35024 => &[("rewardId", "Reward (none: any)", Suggest("kick.rewards"))],
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
        301 => &[],
        302 => &[("commandId", "Voice command (none: any)", VoiceCommand)],
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
        _ => return None,
    })
}

type Ids = Vec<(String, String)>;

/// What the pickers offer, copied out of the config so it isn't held while the form is open.
#[derive(Default)]
pub struct Lists {
    /// (id, name, its sub-actions, its triggers).
    actions: Vec<(String, String, Ids, Ids)>,
    queues: Ids,
    timers: Ids,
    file_watchers: Ids,
    file_tails: Ids,
    voice_commands: Ids,
    commands: Ids,
    user_groups: Ids,
    clients: Ids,
    servers: Ids,
    obs_connections: Ids,
    slobs_connections: Ids,
    action_groups: Vec<String>,
    command_groups: Vec<String>,
}

fn s<'a>(o: &'a Obj, k: &str) -> &'a str {
    o.get(k).and_then(Value::as_str).unwrap_or("")
}

fn ids(list: &[Obj]) -> Ids {
    list.iter().map(|x| (s(x, "id").into(), s(x, "name").into())).collect()
}

fn sorted<'a>(it: impl Iterator<Item = &'a str>) -> Vec<String> {
    let mut v: Vec<String> = it.filter(|g| !g.is_empty()).map(String::from).collect();
    v.sort_by_key(|g| g.to_lowercase());
    v.dedup();
    v
}

impl Lists {
    pub fn new(c: &Config) -> Self {
        let names = super::actions::names(c);
        let mut actions: Vec<_> = c
            .actions
            .actions
            .iter()
            .map(|a| {
                let mut subs = Vec::new();
                crate::edit::walk(&a.sub_actions, &mut |o| {
                    subs.push((s(o, "id").into(), super::actions::sub_label(o, &names)));
                });
                let trigs = a
                    .triggers
                    .iter()
                    .map(|t| (s(t, "id").into(), super::actions::trigger_label(t, &names)))
                    .collect();
                let display = if a.group.is_empty() {
                    a.name.clone()
                } else {
                    format!("[{}] {}", a.group, a.name)
                };
                (a.id.clone(), display, subs, trigs, a.group.clone(), a.name.clone())
            })
            .collect();
        actions.sort_by(|x, y| {
            x.4.to_lowercase()
                .cmp(&y.4.to_lowercase())
                .then_with(|| x.5.to_lowercase().cmp(&y.5.to_lowercase()))
        });
        let actions = actions
            .into_iter()
            .map(|(id, name, subs, trigs, _, _)| (id, name, subs, trigs))
            .collect();
        let mut queues = vec![(crate::engine::DEFAULT_QUEUE.to_string(), "Default".to_string())];
        queues.extend(ids(&c.actions.queues));
        Lists {
            actions,
            queues,
            timers: ids(&c.timers),
            file_watchers: ids(&c.file_watchers),
            file_tails: ids(&c.file_tails),
            voice_commands: ids(&c.voice_commands),
            commands: ids(&c.commands),
            user_groups: ids(&c.groups),
            clients: ids(&c.websockets.connections),
            servers: ids(&c.websockets.servers),
            obs_connections: ids(&c.obs.connections),
            slobs_connections: ids(&c.slobs.connections),
            action_groups: sorted(c.actions.actions.iter().map(|a| a.group.as_str())),
            command_groups: sorted(c.commands.iter().map(|x| s(x, "group"))),
        }
    }

    fn of(&self, action: &str, sub: bool) -> Ids {
        self.actions
            .iter()
            .find(|a| a.0 == action)
            .map(|a| if sub { a.2.clone() } else { a.3.clone() })
            .unwrap_or_default()
    }
}

/// Menu text: FLTK reads "/" as a submenu, "&" as a shortcut and "@" as a symbol.
fn item(t: &str) -> String {
    t.replace('/', "\\/").replace('&', "&&").replace('@', "@@")
}

/// Fill an id picker: "(none)" first, then the names; an id not in the list stays, as itself.
fn fill(c: &mut Choice, list: &Ids, cur: &str) -> Vec<String> {
    c.clear();
    c.add_choice("(none)");
    let mut out = vec![String::new()];
    for (id, name) in list {
        c.add_choice(&item(if name.is_empty() { id } else { name }));
        out.push(id.clone());
    }
    if !cur.is_empty() && !out.iter().any(|x| x == cur) {
        c.add_choice(&item(cur));
        out.push(cur.into());
    }
    c.set_value(out.iter().position(|x| x == cur).unwrap_or(0) as i32);
    out
}

enum Widget {
    Text(Input),
    Multi(MultilineInput),
    Bool(CheckButton),
    Bit(CheckButton, u64),
    Int(IntInput),
    Num(FloatInput),
    Pick(Choice),
    Id(Choice, Rc<RefCell<Vec<String>>>),
    Name(InputChoice),
    Expression(InputChoice, String),
    Map(MultilineInput),
    Lines(MultilineInput),
    Keys(Rc<Cell<(i64, i64)>>),
    MouseButton(Rc<Cell<i64>>),
    /// Shown, not saved.
    Note,
}

/// The form's answer.
pub enum Done {
    Ok(Obj),
    Cancel,
    /// Edit it as JSON instead.
    Json(Obj),
}

fn text(v: Option<&Value>) -> String {
    v.map(crate::vars::text).unwrap_or_default()
}

/// Edit `o`'s fields `fields`. The result keeps every other field of `o`, in place.
/// The access note's height: its text and its button.
const ACCESS_H: i32 = 36 + ROW;

/// How this computer catches a Key Press or Mouse Click trigger (see `hotkeys::access_note`), with the
/// button that allows reading the devices or takes it back.
fn access_note(col: &mut Flex, mouse: bool) {
    let mut block = Flex::default().column();
    col.fixed(&block, ACCESS_H);
    block.set_pad(4);
    let mut text = Frame::default().with_align(
        fltk::enums::Align::Left | fltk::enums::Align::Inside | fltk::enums::Align::Wrap | fltk::enums::Align::Top,
    );
    text.set_label_size(12);
    text.set_label_color(super::muted());
    let mut bar = Flex::default().row();
    let mut b = fltk::button::Button::default();
    bar.fixed(&b, 170);
    Frame::default();
    bar.end();
    block.fixed(&bar, ROW);
    block.end();
    let mut show = {
        let (mut text, mut b, mut bar, mut block) = (text.clone(), b.clone(), bar.clone(), block.clone());
        move || {
            let (note, fix) = crate::hotkeys::access_note(mouse);
            text.set_label(&note.replace('@', "@@"));
            match fix {
                Some(f) => {
                    b.set_label(crate::i18n::tr(if f == crate::hotkeys::Fix::Allow {
                        "btn.allow_access"
                    } else {
                        "btn.remove_access"
                    }));
                    super::with_icon(&mut b);
                    let file = if mouse {
                        "70-streamerz-mice.rules"
                    } else {
                        "70-streamerz-keyboards.rules"
                    };
                    b.set_tooltip(&crate::i18n::trf(
                        "input.allow_tip",
                        &[("file", &format!("/etc/udev/rules.d/{file}"))],
                    ));
                    bar.show();
                }
                None => bar.hide(),
            }
            block.layout();
            block.redraw();
        }
    };
    show();
    #[cfg(not(windows))]
    b.set_callback(move |b| {
        let kind = if mouse {
            crate::devinput::Kind::Mice
        } else {
            crate::devinput::Kind::Keyboards
        };
        let install = crate::hotkeys::access_note(mouse).1 == Some(crate::hotkeys::Fix::Allow);
        b.deactivate();
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || drop(tx.send(crate::devinput::grant(kind, install))));
        let got = loop {
            if let Ok(r) = rx.try_recv() {
                break r;
            }
            fltk::app::wait_for(0.1).ok();
        };
        b.activate();
        match got {
            Ok(()) if !install => super::dialogs::alert(crate::i18n::tr("input.removed")),
            Ok(()) => {}
            Err(e) => super::dialogs::alert(&e),
        }
        show();
    });
    #[cfg(windows)]
    let _ = &mut b;
}

pub fn form(title: &str, fields: &[F], o: &Obj, lists: &Lists) -> Done {
    let tall = |w: &W| matches!(w, Multi | Map | Lines);
    let check = |w: &W| matches!(w, Bool | Bit(_));
    // Check boxes go two to a row: a check box right after one that opened a row shares it.
    let mut shares = vec![false; fields.len()];
    for i in 1..fields.len() {
        shares[i] = check(&fields[i].2) && check(&fields[i - 1].2) && !shares[i - 1];
    }
    let h = fields
        .iter()
        .zip(&shares)
        .map(|(f, &shared)| match () {
            _ if shared => 0,
            _ if tall(&f.2) => 84,
            _ if matches!(f.2, Access(_)) => ACCESS_H + 4,
            _ => ROW + 4,
        })
        .sum::<i32>()
        + ROW
        + 34;
    let visible_h = h.clamp(120, 600);
    let (mut win, answer) = window(560, visible_h, title);
    // The song request action has many optional rules and replies. Keep them in separate
    // tabs so the common operation is visible without scrolling through advanced options.
    let song = fields.iter().any(|f| f.1 == "spotify.operation");
    let tab_y = if song { 35 } else { 10 };
    let area_h = visible_h - ROW - 34 - (tab_y - 10);
    let mut tabs = song.then(|| fltk::group::Tabs::new(10, 10, 540, visible_h - ROW - 34, None));
    let mut page = song.then(|| fltk::group::Group::new(10, tab_y, 540, area_h, crate::i18n::tr("spotify.tab_action")));
    let mut scroll = fltk::group::Scroll::new(10, tab_y, 540, area_h, None);
    scroll.set_type(fltk::group::ScrollType::Vertical);
    let content_h = if song {
        (5 * (ROW + 4)).max(area_h)
    } else {
        (h - ROW - 34).max(area_h)
    };
    let mut col = Flex::new(10, tab_y, 526, content_h, None).column();
    col.set_pad(4);
    let mut ws = Vec::new();
    let mut half: Option<Flex> = None;
    for (index, ((key, label, w), &shared)) in fields.iter().zip(&shares).enumerate() {
        if song && (index == 5 || index == 17) {
            Frame::default();
            col.end();
            scroll.end();
            if let Some(p) = &mut page {
                p.end();
            }
            let label = crate::i18n::tr(if index == 5 {
                "spotify.tab_rules"
            } else {
                "spotify.tab_replies"
            });
            page = Some(fltk::group::Group::new(10, tab_y, 540, area_h, label));
            scroll = fltk::group::Scroll::new(10, tab_y, 540, area_h, None);
            scroll.set_type(fltk::group::ScrollType::Vertical);
            let count = if index == 5 { 12 } else { fields.len() - 17 };
            col = Flex::new(10, tab_y, 526, (count as i32 * (ROW + 4)).max(area_h), None).column();
            col.set_pad(4);
        }
        let v = o.get(*key);
        if let Bool | Bit(_) = w {
            let r = match half.take() {
                Some(r) if shared => r,
                _ => {
                    let r = Flex::default().row();
                    col.fixed(&r, ROW);
                    r
                }
            };
            r.begin();
            let c = CheckButton::default().with_label(label);
            c.set_checked(match w {
                Bit(bit) => v.and_then(Value::as_u64).unwrap_or(0) & bit != 0,
                _ => v.and_then(Value::as_bool).unwrap_or(false),
            });
            r.end();
            half = (!shared).then_some(r);
            ws.push(match w {
                Bit(bit) => Widget::Bit(c, *bit),
                _ => Widget::Bool(c),
            });
            continue;
        }
        half = None;
        if let Access(mouse) = w {
            access_note(&mut col, *mouse);
            ws.push(Widget::Note);
            continue;
        }
        let label = field_label(label);
        let mut r = row(label);
        if song {
            if let Some(label) = r.child(0) {
                r.fixed(&label, 200);
            }
        }
        let widget = match w {
            Text => {
                let mut i = Input::default();
                i.set_value(&text(v));
                if *key == "operation" && fields.iter().any(|f| f.1 == "spotify.operation") {
                    i.set_tooltip(crate::i18n::tr("spotify.operation_help"));
                }
                Widget::Text(i)
            }
            Expression(values) => {
                let mut c = InputChoice::default();
                for value in *values {
                    c.add(&item(value));
                }
                let original = text(v);
                c.set_value(&original);
                Widget::Expression(c, original)
            }
            Multi => {
                let mut i = MultilineInput::default();
                i.set_value(&text(v).replace("\r\n", "\n"));
                Widget::Multi(i)
            }
            Int => {
                let mut i = IntInput::default();
                i.set_value(&v.and_then(Value::as_f64).unwrap_or(0.0).to_string());
                Widget::Int(i)
            }
            Num => {
                let mut i = FloatInput::default();
                i.set_value(&v.and_then(Value::as_f64).unwrap_or(0.0).to_string());
                Widget::Num(i)
            }
            Pick(opts) => {
                let mut c = super::fit::choice();
                for x in *opts {
                    c.add_choice(x);
                }
                c.set_value(v.and_then(Value::as_i64).unwrap_or(0) as i32);
                Widget::Pick(c)
            }
            Action | Queue | Timer | FileWatcher | FileTail | VoiceCommand | Command | UserGroup | Client | Server
            | ObsConnection | SlobsConnection | Sub | Trigger => {
                let mut c = super::fit::choice();
                let list = match w {
                    Action => lists.actions.iter().map(|a| (a.0.clone(), a.1.clone())).collect(),
                    Queue => lists.queues.clone(),
                    Timer => lists.timers.clone(),
                    FileWatcher => lists.file_watchers.clone(),
                    FileTail => lists.file_tails.clone(),
                    VoiceCommand => lists.voice_commands.clone(),
                    Command => lists.commands.clone(),
                    UserGroup => lists.user_groups.clone(),
                    Client => lists.clients.clone(),
                    Server => lists.servers.clone(),
                    ObsConnection => lists.obs_connections.clone(),
                    SlobsConnection => lists.slobs_connections.clone(),
                    _ => lists.of(s(o, "actionId"), matches!(w, Sub)),
                };
                let got = fill(&mut c, &list, &text(v));
                Widget::Id(c, Rc::new(RefCell::new(got)))
            }
            Suggest(key) => {
                let mut c = InputChoice::default();
                for (_, name) in crate::catalog::get(key) {
                    c.add(&item(&name));
                }
                // An id saved before is shown as the name it has now.
                let saved = text(v);
                c.set_value(&crate::catalog::name_of(key, &saved).unwrap_or(saved));
                Widget::Name(c)
            }
            ActionGroup | CommandGroup => {
                let mut c = InputChoice::default();
                let list = if let ActionGroup = w {
                    &lists.action_groups
                } else {
                    &lists.command_groups
                };
                for g in list {
                    c.add(&item(g));
                }
                c.set_value(&text(v));
                Widget::Name(c)
            }
            Map => {
                let mut i = MultilineInput::default();
                let lines: Vec<String> = v
                    .and_then(Value::as_object)
                    .map(|m| m.iter().map(|(k, x)| format!("{k}={}", text(Some(x)))).collect())
                    .unwrap_or_default();
                i.set_value(&lines.join("\n"));
                Widget::Map(i)
            }
            Lines => {
                let mut i = MultilineInput::default();
                let lines: Vec<String> = v
                    .and_then(Value::as_array)
                    .map(|a| a.iter().map(|x| text(Some(x))).collect())
                    .unwrap_or_default();
                i.set_value(&lines.join("\n"));
                Widget::Lines(i)
            }
            Keys => {
                let n = |k: &str| o.get(k).and_then(Value::as_i64).unwrap_or(0);
                let combo = Rc::new(Cell::new((n("keyCode"), n("modifierMask"))));
                let mut b = fltk::button::Button::default();
                let show = |b: &mut fltk::button::Button, (k, m): (i64, i64)| {
                    b.set_label(&if k == 0 {
                        "Press to set the keys…".to_string()
                    } else {
                        crate::hotkeys::label(k, m).replace('&', "&&").replace('@', "@@")
                    });
                };
                show(&mut b, combo.get());
                b.set_tooltip("Click, then press the key combination");
                b.set_callback({
                    let combo = combo.clone();
                    move |b| {
                        if let Some(c) = super::dialogs::capture_keys() {
                            combo.set(c);
                            show(b, c);
                        }
                    }
                });
                Widget::Keys(combo)
            }
            MouseButton => {
                let held = Rc::new(Cell::new(o.get(*key).and_then(Value::as_i64).unwrap_or(0)));
                let mut b = fltk::button::Button::default().with_label(crate::hotkeys::button_name(held.get()));
                b.set_tooltip(crate::i18n::tr("dialogs.mouse.tip"));
                b.set_callback({
                    let held = held.clone();
                    move |b| {
                        if let Some(n) = super::dialogs::capture_button(held.get()) {
                            held.set(n);
                            b.set_label(crate::hotkeys::button_name(n));
                        }
                    }
                });
                Widget::MouseButton(held)
            }
            Bool | Bit(_) | Access(_) => unreachable!(),
        };
        r.end();
        col.fixed(&r, if tall(w) { 80 } else { ROW });
        ws.push(widget);
    }
    // The sub-action and trigger pickers follow the action picked above them.
    let at = |k: &str| fields.iter().position(|f| f.0 == k);
    if let (Some(a), Some(d)) = (at("actionId"), at("subActionId").or(at("triggerId"))) {
        if let (Widget::Id(ac, aids), Widget::Id(dc, dids)) = (&ws[a], &ws[d]) {
            let (aids, dids, mut dc) = (aids.clone(), dids.clone(), dc.clone());
            let sub = at("subActionId").is_some();
            let lists: Vec<(String, Ids)> = lists
                .actions
                .iter()
                .map(|x| (x.0.clone(), if sub { x.2.clone() } else { x.3.clone() }))
                .collect();
            ac.clone().set_callback(move |c| {
                let id = aids
                    .borrow()
                    .get(c.value().max(0) as usize)
                    .cloned()
                    .unwrap_or_default();
                let list = lists
                    .iter()
                    .find(|x| x.0 == id)
                    .map(|x| x.1.clone())
                    .unwrap_or_default();
                *dids.borrow_mut() = fill(&mut dc, &list, "");
            });
        }
    }
    if fields.is_empty() {
        let f = Frame::default().with_label(crate::i18n::tr("forms.empty"));
        col.fixed(&f, ROW);
    }
    Frame::default();
    col.end();
    scroll.end();
    if let Some(p) = &mut page {
        p.end();
    }
    if let Some(t) = &mut tabs {
        t.end();
    }
    let mut bar = Flex::new(10, visible_h - ROW - 14, 540, ROW + 4, None).row();
    bar.set_pad(8);
    let json = Rc::new(Cell::new(false));
    let mut j = button(&mut bar, crate::i18n::tr("btn.edit_json"), 100);
    j.set_callback({
        let (json, a) = (json.clone(), answer.clone());
        move |_| {
            json.set(true);
            a.set(Some(true));
        }
    });
    ok_cancel(&mut bar, &answer, !fields.iter().any(|f| tall(&f.2)));
    bar.end();
    win.end();
    let mut out = o.clone();
    let mut show_json = false;
    let ok = run(&mut win, &answer, || {
        // Consume the intent before validation, so a later normal OK cannot inherit it.
        let requested_json = json.replace(false);
        match collect_draft(fields, &ws, o) {
            Ok(draft) => {
                out = draft;
                show_json = requested_json;
                true
            }
            Err(error) => {
                super::dialogs::alert(&error);
                false
            }
        }
    });
    if ok && show_json {
        Done::Json(out)
    } else if ok {
        Done::Ok(out)
    } else {
        Done::Cancel
    }
}

/// Collect a fresh draft for either submit action; a failed attempt never changes the original.
fn collect_draft(fields: &[F], ws: &[Widget], original: &Obj) -> Result<Obj, String> {
    let mut out = original.clone();
    for ((key, label, _), w) in fields.iter().zip(ws) {
        let v = match w {
            Widget::Text(i) => i.value().into(),
            // Line breaks as Streamer.bot's text boxes write them (it splits aliases on \r\n only).
            Widget::Multi(i) => i.value().replace('\n', "\r\n").into(),
            Widget::Bool(c) => c.is_checked().into(),
            // Bits not on the form are kept.
            Widget::Bit(c, bit) => {
                let old = out.get(*key).and_then(Value::as_u64).unwrap_or(0) & !bit;
                (if c.is_checked() { old | bit } else { old }).into()
            }
            Widget::Int(i) => match i.value().trim() {
                "" => 0.into(),
                t => match t.parse::<i64>() {
                    Ok(n) => n.into(),
                    Err(_) => {
                        return Err(crate::i18n::trf("forms.whole_number", &[("label", field_label(label))]));
                    }
                },
            },
            Widget::Num(i) => match i.value().trim() {
                "" => json!(0.0),
                t => match t.parse::<f64>() {
                    Ok(n) => json!(n),
                    Err(_) => {
                        return Err(crate::i18n::trf("forms.number", &[("label", field_label(label))]));
                    }
                },
            },
            Widget::Pick(c) if c.value() < 0 => continue,
            Widget::Pick(c) => c.value().into(),
            Widget::Id(c, ids) => match ids.borrow().get(c.value().max(0) as usize) {
                Some(id) if !id.is_empty() => id.as_str().into(),
                // ponytail: no id, no field, rather than an empty string where Streamer.bot wants a Guid.
                _ => {
                    out.shift_remove(*key);
                    continue;
                }
            },
            Widget::Name(c) => c.value().unwrap_or_default().trim().into(),
            Widget::Expression(c, original) => {
                let value = c.value().unwrap_or_default();
                if &value == original {
                    continue;
                }
                value.trim().into()
            }
            Widget::Map(i) => Value::Object(
                i.value()
                    .lines()
                    .filter_map(|l| l.split_once('='))
                    .map(|(k, v)| (k.trim().to_string(), Value::from(v)))
                    .filter(|(k, _)| !k.is_empty())
                    .collect(),
            ),
            Widget::Lines(i) => i.value().lines().map(Value::from).collect(),
            Widget::Note => continue,
            Widget::MouseButton(c) => c.get().into(),
            Widget::Keys(c) => {
                let (k, m) = c.get();
                if k == 0 {
                    return Err(crate::i18n::tr("forms.capture_keys").into());
                }
                out.insert("modifierMask".into(), m.into());
                k.into()
            }
        };
        match out.get_mut(*key) {
            Some(x) => *x = v,
            // New fields go before the base ones, as Streamer.bot writes them.
            None => {
                let at = out
                    .keys()
                    .position(|k| ["subActions", "id"].contains(&k.as_str()))
                    .unwrap_or(out.len());
                out.shift_insert(at, (*key).into(), v);
            }
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Run separately on the hidden display; FLTK must stay on this test's thread.
    #[test]
    #[ignore]
    fn form_json_draft_keeps_edits_and_retries_validation() {
        let _app = fltk::app::App::default();
        let mut window = fltk::window::DoubleWindow::new(0, 0, 560, 300, None);
        let mut name = Input::default();
        name.set_value("Edited name");
        let mut lines = MultilineInput::default();
        lines.set_value("first\nsecond");
        let mut count = IntInput::default();
        count.set_value("invalid");
        let mut expression = InputChoice::default();
        expression.set_value("7");
        let enabled = CheckButton::default();
        enabled.set_checked(true);
        let bit = CheckButton::default();
        bit.set_checked(true);
        window.end();
        let fields: &[F] = &[
            ("name", "Name", Text),
            ("lines", "Lines", Multi),
            ("count", "Count", Int),
            ("expression", "Expression", Expression(&[])),
            ("enabled", "Enabled", Bool),
            ("sources", "Source", Bit(1)),
        ];
        let original = json!({"name": "Original", "count": 2, "expression": 7,
            "enabled": false, "sources": 8, "unknown": {"retained": true}, "type": 99, "id": "original"})
        .as_object()
        .unwrap()
        .clone();
        let untouched = original.clone();
        let widgets = vec![
            Widget::Text(name),
            Widget::Multi(lines),
            Widget::Int(count.clone()),
            Widget::Expression(expression, "7".into()),
            Widget::Bool(enabled),
            Widget::Bit(bit, 1),
        ];
        assert!(collect_draft(fields, &widgets, &original).is_err());
        assert_eq!(original, untouched);
        count.set_value("12");
        let draft = collect_draft(fields, &widgets, &original).unwrap();
        assert_eq!(draft["name"], "Edited name");
        assert_eq!(draft["lines"], "first\r\nsecond");
        assert_eq!(draft["count"], 12);
        assert_eq!(draft["expression"], 7);
        assert!(draft["expression"].is_number());
        assert_eq!(draft["enabled"], true);
        assert_eq!(draft["sources"], 9);
        assert_eq!(draft["unknown"], original["unknown"]);
        assert_eq!(draft["type"], original["type"]);
        assert_eq!(draft["id"], original["id"]);
        assert_eq!(original, untouched);
        window.hide();
        fltk::app::delete_widget(window);
    }

    #[test]
    fn spotify_song_form_groups_and_expression_suggestions() {
        let fields = sub_fields(99802).unwrap();
        assert_eq!(fields[0].0, "operation");
        assert_eq!(fields[5].0, "maxPerUser");
        assert_eq!(fields[17].0, "msgNotAllowed");
        assert!(matches!(fields[0].2, Expression(_)));
        for key in ["skipOwn", "blockExplicit"] {
            assert!(matches!(fields.iter().find(|f| f.0 == key).unwrap().2, Expression(_)));
        }
    }

    #[test]
    fn every_core_type_has_a_form() {
        for (k, name, _, fields) in crate::types::SUB_ACTION_INFO {
            if !crate::engine::supported(*k) {
                continue;
            }
            let form = sub_fields(*k as u64).unwrap_or_else(|| panic!("{name} has no form"));
            // Every field but the children is on the form.
            for f in fields.iter().filter(|f| **f != "subActions") {
                assert!(form.iter().any(|x| x.0 == *f), "{name}: {f} missing");
            }
        }
        for k in [
            401, 402, 701, 702, 706, 709, 710, 711, 712, 803, 804, 805, 2001, 2002, 2003, 18001, 18002,
        ]
        .into_iter()
        .chain(32001..=32005)
        .chain(101..=196)
        .chain(451..=498)
        .chain(14001..=14009)
        .chain(22001..=22007)
        .chain(4001..=4031)
        .chain([35001, 35002, 35003, 35008, 35009])
        .chain(35010..=35025)
        .chain(201..=206)
        .chain(1201..=1205)
        .chain(3001..=3003)
        .chain(7001..=7005)
        .chain(8001..=8005)
        .chain([10001])
        .chain(11001..=11004)
        .chain(13001..=13002)
        .chain(24001..=24013)
        .chain(28001..=28003)
        .chain(36001..=36004)
        .chain(37001..=37003)
        .chain(29001..=29004)
        .chain([301, 302, 15001, 16001, 16002])
        .chain([705, 17001, 17002, 17003, 17004])
        .chain(501..=504)
        .chain([511, 703, 704])
        {
            assert!(trigger_fields(k).is_some(), "trigger {k} has no form");
        }
    }
}
