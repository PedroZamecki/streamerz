//! Forms for the core sub-actions and triggers: one row per field, from a table. Fields not in the
//! table are kept as they are; "Edit JSON" opens the whole object instead.

use super::dialogs::{ok_cancel, row, run, window};
use super::{button, ROW};
use crate::model::Obj;
use crate::Config;
use fltk::button::CheckButton;
use fltk::dialog;
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
    Command,
    UserGroup,
    Client,
    Server,
    ObsConnection,
    SlobsConnection,
    /// A sub-action or trigger of the action picked in the `actionId` field.
    Sub,
    Trigger,
    /// A name, typed or picked.
    ActionGroup,
    CommandGroup,
    /// An object of texts, as `name=value` lines.
    Map,
    /// A list of texts, one per line.
    Lines,
}

use W::*;

/// (JSON field, label, kind).
pub type F = (&'static str, &'static str, W);

pub const STATE_HIDE_SHOW_TOGGLE: &[&str] = &["Hide", "Show", "Toggle"];
pub const STREAM_STATE: &[&str] = &["Stop", "Start", "Toggle"];
pub const RECORD_STATE: &[&str] = &["Stop", "Start", "Toggle", "Pause", "Resume"];
pub const REPLAY_STATE: &[&str] = &["Stop", "Start", "Toggle", "Save"];
// Streamlabs Desktop (checked against Streamer.bot): visibility/filters/mute 0 on, 1 off, 2 toggle;
// Streaming 0 stop, 1 start; Recording 0 start, 1 stop (Pause/Resume do nothing in Streamlabs).
const SD_SHOW: &[&str] = &["Visible", "Hidden", "Toggle"];
const SD_MUTE: &[&str] = &["Muted", "Not muted", "Toggle"];
const SD_STREAM: &[&str] = &["Stop", "Start"];
const SD_RECORD: &[&str] = &["Start", "Stop", "Pause", "Resume"];
const SD_DISPLAY: &[&str] = &["Both", "Horizontal", "Vertical"];
pub const FLIP_MODE: &[&str] = &["Horizontal", "Vertical", "Both"];
pub const FILTER_STATE: &[&str] = &["Disable", "Enable", "Toggle"];
pub const MUTE_STATE: &[&str] = &["Unmute", "Mute", "Toggle"];
pub const TRACK_STATE: &[&str] = &["Disable", "Enable", "Toggle"];
pub const MEDIA_STATE: &[&str] = &["Play", "Pause", "Stop", "Restart", "Next", "Previous"];

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

/// A core sub-action's fields; None when it has no form (it opens as JSON).
pub fn sub_fields(kind: u64) -> Option<&'static [F]> {
    Some(match kind {
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
            ("source", "Source", Pick(&["Manual", "Variable", "Twitch Game ID"])),
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
        200 => &[("rewardId", "Reward ID", Text), ("state", "State", Pick(STATE))],
        201 => &[("rewardId", "Reward ID", Text), ("state", "State", Pick(STATE))],
        202 => &[
            ("rewardId", "Reward ID", Text),
            ("cost", "Cost", Int),
            ("operator", "Operation", Pick(&["Set", "Add", "Subtract"])),
        ],
        203 => &[("disableNotConfigured", "Disable not configured", Bool)],
        204 => &[
            ("rewardId", "Reward ID", Text),
            ("cooldown", "Cooldown (seconds)", Int),
            ("additive", "Additive", Bool),
        ],
        205 => &[("rewardId", "Reward ID", Text), ("title", "Title", Text)],
        206 => &[("rewardId", "Reward ID", Text), ("prompt", "Prompt", Multi)],
        207 => &[
            ("rewardId", "Reward ID", Text),
            ("title", "Title", Text),
            ("prompt", "Prompt", Multi),
            ("cost", "Cost", Int),
            ("operator", "Operation", Pick(&["Set", "Add", "Subtract"])),
        ],
        208..=210 => &[("rewardId", "Reward ID", Text)],
        211 => &[("groupName", "Group name", Text), ("state", "State", Pick(STATE))],
        212 => &[("groupName", "Group name", Text), ("state", "State", Pick(STATE))],
        213 => &[
            ("rewardId", "Reward ID", Text),
            ("backgroundColor", "Background color", Text),
            ("random", "Random", Bool),
        ],
        214 | 215 => &[
            ("rewardId", "Reward ID", Text),
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
        579 => &[("userLogin", "User login", Text), ("status", "Status", Text)],

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
            ("sourceName", "Source", Text),
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
            ("sceneName", "Scene", Text),
        ],
        406 => &[
            ("connectionId", "Connection", SlobsConnection),
            ("sceneName", "Scene (empty: active)", Text),
            ("sourceName", "Source", Text),
            ("filterName", "Filter", Text),
            ("state", "State", Pick(SD_SHOW)),
        ],
        407 => &[
            ("connectionId", "Connection", SlobsConnection),
            ("flipMode", "Flip", Pick(FLIP_MODE)),
            ("sceneName", "Scene (empty: active)", Text),
            ("sourceName", "Source", Text),
            ("display", "Display", Pick(SD_DISPLAY)),
        ],
        408 => &[
            ("connectionId", "Connection", SlobsConnection),
            ("degrees", "Degrees", Num),
            ("additive", "Additive (rotate by, not to)", Bool),
            ("sceneName", "Scene (empty: active)", Text),
            ("sourceName", "Source", Text),
            ("display", "Display", Pick(SD_DISPLAY)),
        ],
        409 => &[
            ("connectionId", "Connection", SlobsConnection),
            ("sceneName", "Scene (empty: active)", Text),
            ("sourceName", "Source", Text),
            ("url", "URL", Text),
        ],
        410 => &[
            ("connectionId", "Connection", SlobsConnection),
            ("sceneName", "Scene (empty: active)", Text),
            ("sourceName", "Source", Text),
            ("text", "Text", Multi),
        ],
        411 => &[
            ("connectionId", "Connection", SlobsConnection),
            ("sceneName", "Scene (empty: active)", Text),
            ("sourceName", "Source", Text),
            ("state", "State", Pick(SD_SHOW)),
        ],
        413 => &[
            ("connectionId", "Connection", SlobsConnection),
            ("sceneName", "Scene (empty: active)", Text),
            ("sourceName", "Source", Text),
            ("state", "State", Pick(SD_MUTE)),
        ],
        412 => &[
            ("connectionId", "Connection", SlobsConnection),
            ("sceneName", "Scene (empty: active)", Text),
            ("sourceName", "Source", Text),
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
            ("sceneName", "Scene", Text),
            ("sourceName", "Source", Text),
            ("state", "State", Pick(STATE_HIDE_SHOW_TOGGLE)),
        ],
        31 => &[
            ("connectionId", "Connection", ObsConnection),
            ("sceneName", "Scene", Text),
            ("groupName", "Group", Text),
        ],
        32 => &[
            ("connectionId", "Connection", ObsConnection),
            ("sceneName", "Scene", Text),
            ("groupName", "Group", Text),
        ],
        33 => &[
            ("connectionId", "Connection", ObsConnection),
            ("state", "State", Pick(STREAM_STATE)),
        ],
        34 => &[
            ("connectionId", "Connection", ObsConnection),
            ("sceneName", "Scene", Text),
        ],
        35 => &[
            ("connectionId", "Connection", ObsConnection),
            ("sceneName", "Scene", Text),
            ("sourceName", "Source", Text),
            ("filterName", "Filter", Text),
            ("state", "State", Pick(FILTER_STATE)),
        ],
        36 => &[
            ("connectionId", "Connection", ObsConnection),
            ("flipMode", "Flip mode", Pick(FLIP_MODE)),
            ("sceneName", "Scene", Text),
            ("sourceName", "Source", Text),
        ],
        37 => &[
            ("connectionId", "Connection", ObsConnection),
            ("degrees", "Degrees", Num),
            ("additive", "Additive", Bool),
            ("sceneName", "Scene", Text),
            ("sourceName", "Source", Text),
        ],
        38 => &[
            ("connectionId", "Connection", ObsConnection),
            ("sceneName", "Scene", Text),
            ("sourceName", "Source", Text),
            ("url", "URL", Text),
        ],
        39 => &[
            ("connectionId", "Connection", ObsConnection),
            ("sceneName", "Scene", Text),
            ("sourceName", "Source", Text),
            ("text", "Text", Text),
        ],
        40 => &[
            ("connectionId", "Connection", ObsConnection),
            ("sceneName", "Scene", Text),
            ("sourceName", "Source", Text),
            ("state", "State", Pick(FILTER_STATE)),
        ],
        41 => &[
            ("connectionId", "Connection", ObsConnection),
            ("sceneName", "Scene", Text),
            ("sourceName", "Source", Text),
        ],
        42 => &[
            ("connectionId", "Connection", ObsConnection),
            ("sceneName", "Scene", Text),
            ("sourceName", "Source", Text),
            ("state", "State", Pick(MUTE_STATE)),
        ],
        43 => &[("connectionId", "Connection", ObsConnection)],
        44 => &[
            ("connectionId", "Connection", ObsConnection),
            ("sceneName", "Scene", Text),
            ("sourceName", "Source", Text),
            ("track", "Track (1-6)", Int),
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
            ("sceneName", "Scene", Text),
            ("filterName", "Filter", Text),
            ("state", "State", Pick(FILTER_STATE)),
        ],
        48 => &[
            ("connectionId", "Connection", ObsConnection),
            ("sceneName", "Scene", Text),
            ("sourceName", "Source", Text),
            ("path", "File path", Text),
            ("quality", "Quality", Int),
        ],
        49 => &[
            ("connectionId", "Connection", ObsConnection),
            ("sceneName", "Scene", Text),
            ("sourceName", "Source", Text),
            ("state", "State", Pick(MEDIA_STATE)),
        ],
        321 => &[
            ("connectionId", "Connection", ObsConnection),
            ("state", "State", Pick(REPLAY_STATE)),
        ],
        322 => &[
            ("connectionId", "Connection", ObsConnection),
            ("sceneName", "Scene", Text),
            ("sourceName", "Source", Text),
            ("fileName", "File name", Text),
        ],
        323 => &[
            ("connectionId", "Connection", ObsConnection),
            ("sceneName", "Scene", Text),
            ("sourceName", "Source", Text),
            ("fileName", "File name", Text),
        ],
        324 => &[
            ("connectionId", "Connection", ObsConnection),
            ("sceneName", "Scene", Text),
            ("sourceName", "Source", Text),
        ],
        325 => &[
            ("connectionId", "Connection", ObsConnection),
            ("sceneName", "Scene", Text),
        ],
        326 => &[
            ("connectionId", "Connection", ObsConnection),
            ("sceneName", "Scene", Text),
        ],
        327 => &[
            ("connectionId", "Connection", ObsConnection),
            ("sceneName", "Scene", Text),
            ("sourceName", "Source", Text),
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
        35101 | 35102 => &[("rewardId", "Reward ID", Text), ("state", "State", Bool)],
        35103 => &[
            ("rewardId", "Reward ID", Text),
            ("cost", "Cost", Int),
            ("operator", "Operator", Int),
        ],
        35104 => &[("disableNotConfigured", "Disable not configured", Bool)],
        35105 => &[("rewardId", "Reward ID", Text), ("title", "Title", Text)],
        35106 => &[("rewardId", "Reward ID", Text), ("description", "Description", Text)],
        35107 => &[
            ("rewardId", "Reward ID", Text),
            ("title", "Title", Text),
            ("description", "Description", Text),
            ("cost", "Cost", Int),
            ("operator", "Operator", Int),
        ],
        35108..=35110 => &[("rewardId", "Reward ID", Text)],
        35111 | 35112 => &[("groupName", "Group name", Text), ("state", "State", Bool)],
        35113 => &[
            ("rewardId", "Reward ID", Text),
            ("backgroundColor", "Background color", Text),
            ("random", "Random", Bool),
        ],
        43001 => &[
            ("text", "Message", Text),
            ("useBot", "Use Bot Account", Bool),
            ("fallback", "Fallback to Broadcaster", Bool),
        ],
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
        1018 => &[("commandId", "Command ID", Text), ("command", "Command Phrase", Text)],
        1019 => &[("commandId", "Command ID", Text), ("state", "State", Pick(STATE))],
        1025 => &[
            ("audioDeviceId", "Audio Device ID", Text),
            ("audioDeviceName", "Audio Device Name", Text),
        ],
        9001 => &[
            ("device", "Device", Text),
            ("name", "Name", Text),
            ("sendNoteOff", "Send Note Off", Bool),
            ("channel", "Channel", Int),
            ("note", "Note", Int),
            ("velocity", "Velocity", Int),
            ("duration", "Duration (ms)", Int),
        ],
        9002 => &[
            ("device", "Device", Text),
            ("name", "Name", Text),
            ("channel", "Channel", Int),
            ("controller", "Controller", Int),
            ("value", "Value", Int),
        ],
        9003 => &[
            ("device", "Device", Text),
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
        1030 => &[("fileTailId", "File Tail ID", Text), ("state", "State", Pick(STATE))],
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
        702 | 706 => &[],
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
        301 => &[("culture", "Culture", Text)],
        302 => &[("commandId", "Command ID", Text)],
        15001 => &[
            ("device", "Device", Text),
            ("channel", "Channel", Int),
            ("status", "Status / Type", Text),
        ],
        16001 => &[("key", "Key", Text), ("modifiers", "Modifiers", Int)],
        16002 => &[("button", "Button", Text)],
        705 => &[("actionName", "Action Name", Text)],
        17001 => &[("buttonId", "Button ID", Text)],
        17002 => &[("device", "Device", Text)],
        17003 => &[("device", "Device", Text)],
        17004 => &[("device", "Device", Text)],
        501..=504 => &[("watcherId", "Watcher ID", Text), ("path", "Path", Text)],
        511 => &[("tailId", "Tail ID", Text), ("path", "Path", Text)],
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
    Map(MultilineInput),
    Lines(MultilineInput),
}

/// The form's answer.
pub enum Done {
    Ok(Obj),
    Cancel,
    /// Edit it as JSON instead.
    Json,
}

fn text(v: Option<&Value>) -> String {
    v.map(crate::vars::text).unwrap_or_default()
}

/// Edit `o`'s fields `fields`. The result keeps every other field of `o`, in place.
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
            _ => ROW + 4,
        })
        .sum::<i32>()
        + ROW
        + 34;
    let (mut win, answer) = window(560, h.max(120), title);
    let mut col = Flex::new(10, 10, 540, h.max(120) - 20, None).column();
    col.set_pad(4);
    let mut ws = Vec::new();
    let mut half: Option<Flex> = None;
    for ((key, label, w), &shared) in fields.iter().zip(&shares) {
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
        let r = row(label);
        let widget = match w {
            Text => {
                let mut i = Input::default();
                i.set_value(&text(v));
                Widget::Text(i)
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
                let mut c = Choice::default();
                for x in *opts {
                    c.add_choice(x);
                }
                c.set_value(v.and_then(Value::as_i64).unwrap_or(0) as i32);
                Widget::Pick(c)
            }
            Action | Queue | Timer | Command | UserGroup | Client | Server | ObsConnection | SlobsConnection | Sub
            | Trigger => {
                let mut c = Choice::default();
                let list = match w {
                    Action => lists.actions.iter().map(|a| (a.0.clone(), a.1.clone())).collect(),
                    Queue => lists.queues.clone(),
                    Timer => lists.timers.clone(),
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
            Bool | Bit(_) => unreachable!(),
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
        let f = Frame::default().with_label("Nothing to set.");
        col.fixed(&f, ROW);
    }
    Frame::default();
    let mut bar = Flex::default().row();
    let json = Rc::new(Cell::new(false));
    let mut j = button(&mut bar, "Edit JSON", 100);
    j.set_callback({
        let (json, a) = (json.clone(), answer.clone());
        move |_| {
            json.set(true);
            a.set(Some(false));
        }
    });
    ok_cancel(&mut bar, &answer, !fields.iter().any(|f| tall(&f.2)));
    bar.end();
    col.fixed(&bar, ROW + 4);
    col.end();
    win.end();
    let mut out = o.clone();
    let ok = run(&mut win, &answer, || {
        for ((key, label, _), w) in fields.iter().zip(&ws) {
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
                            dialog::alert_default(&format!("{label}: a whole number, please."));
                            return false;
                        }
                    },
                },
                Widget::Num(i) => match i.value().trim() {
                    "" => json!(0.0),
                    t => match t.parse::<f64>() {
                        Ok(n) => json!(n),
                        Err(_) => {
                            dialog::alert_default(&format!("{label}: a number, please."));
                            return false;
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
                Widget::Map(i) => Value::Object(
                    i.value()
                        .lines()
                        .filter_map(|l| l.split_once('='))
                        .map(|(k, v)| (k.trim().to_string(), Value::from(v)))
                        .filter(|(k, _)| !k.is_empty())
                        .collect(),
                ),
                Widget::Lines(i) => i.value().lines().map(Value::from).collect(),
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
        true
    });
    if json.get() {
        Done::Json
    } else if ok {
        Done::Ok(out)
    } else {
        Done::Cancel
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
