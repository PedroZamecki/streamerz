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
    /// A user group's name, typed or picked.
    UserGroup,
    /// A user group's id, picked by name.
    UserGroupId,
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
    /// An overlay's name, typed or picked.
    Overlay,
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
    /// Who a sub-action acts on: "Who started this" (`%userName%`), "The user named in the command"
    /// (`%input0%`), or typed.
    Who,
    /// Which message (or request): the one that started this (that value), or typed.
    Message(&'static str),
    /// A few on or off choices, saved as the list of the ticked ones (platforms wear their logos).
    Checks(&'static [&'static str]),
    /// A colour, `#rrggbb`.
    Color,
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
            UserGroupId => "user group id".into(),
            Client => "WebSocket client id".into(),
            Server => "WebSocket server id".into(),
            ObsConnection => "OBS connection id".into(),
            SlobsConnection => "Streamlabs Desktop connection id".into(),
            Sub => "sub-action id of the action in actionId".into(),
            Trigger => "trigger id of the action in actionId".into(),
            Suggest(k) => format!("a name ({k} detected on the connection, or typed)"),
            Entity(k) => format!("an ID ({k}, selected by name, or entered manually)"),
            ActionGroup => "action group name".into(),
            Overlay => "overlay name".into(),
            CommandGroup => "command group name".into(),
            Map => "object of texts".into(),
            Lines => "list of texts".into(),
            Keys => "key code (modifiers in modifierMask)".into(),
            MouseButton => "mouse button: 0 any, 1 left, 2 right, 3 middle, 4 back, 5 forward".into(),
            Access(_) => "nothing (a note)".into(),
            Who => "text: a user name (%userName% is who started the action, %input0% a name typed after the command)"
                .into(),
            Message(from) => format!("text: an id ({from} is the one that started the action)"),
            Checks(options) => format!("a list of some of {options:?}"),
            Color => "a colour, #rrggbb".into(),
        }
    }
}

/// (JSON field, label, kind).
pub type F = (&'static str, &'static str, W);

/// Imported options that are preserved but have no runtime effect. Never offer them as controls.
pub fn preserved_fields(kind: u64) -> &'static [&'static str] {
    match kind {
        5 => &["overwrite", "count"],
        10 => &["pin", "pinToStreamEnd"],
        22 | 35012 => &["present", "includeFrom", "groups"],
        25 => &["text", "preferredAccount", "color"],
        1020 => &["logLevel"],
        _ => &[],
    }
}

pub(crate) fn field_label(label: &'static str) -> &'static str {
    if label.starts_with("spotify.")
        || label.starts_with("automation.")
        || label.starts_with("tools.")
        || label.starts_with("field.")
        || label.starts_with("overlay.")
        || label.starts_with("sounds.")
        || label.starts_with("voices.")
    {
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
pub const STATE_HIDE_SHOW_TOGGLE: &[&str] = &["field.show", "field.hide", "field.toggle"];
pub const STREAM_STATE: &[&str] = &["field.start", "field.stop", "field.toggle"];
pub const RECORD_STATE: &[&str] = &[
    "field.start",
    "field.stop",
    "field.pause",
    "field.resume",
    "field.toggle",
];
pub const REPLAY_STATE: &[&str] = &["field.start", "field.stop", "field.save", "field.toggle"];
// Streamlabs Desktop (checked against Streamer.bot): visibility/filters/mute 0 on, 1 off, 2 toggle;
// Streaming 0 stop, 1 start; Recording 0 start, 1 stop (Pause/Resume do nothing in Streamlabs).
const SD_SHOW: &[&str] = &["field.visible", "field.hidden", "field.toggle"];
const SD_MUTE: &[&str] = &["field.muted", "field.not_muted", "field.toggle"];
const SD_STREAM: &[&str] = &["field.stop", "field.start"];
const SD_RECORD: &[&str] = &["field.start", "field.stop", "field.pause", "field.resume"];
const SD_DISPLAY: &[&str] = &["field.both", "field.horizontal", "field.vertical"];
pub const FLIP_MODE: &[&str] = &["field.horizontal", "field.vertical", "field.both"];
pub const FILTER_STATE: &[&str] = &["field.enable", "field.disable", "field.toggle"];
pub const MUTE_STATE: &[&str] = &["field.mute", "field.unmute", "field.toggle"];
pub const TRACK_STATE: &[&str] = &["field.enable", "field.disable", "field.toggle"];
pub const MEDIA_STATE: &[&str] = &[
    "field.play",
    "field.pause",
    "field.restart",
    "field.stop",
    "field.next",
    "field.previous",
];

pub const OPS: &[&str] = &[
    "field.equals",
    "field.not_equals",
    "field.contains",
    "field.regex_match",
    "field.less_than",
    "field.greater_than",
    "field.does_not_exist",
    "field.equals_ignore_case",
    "field.not_equals_ignore_case",
    "field.is_null_or_empty",
];
pub const WHILE_OPS: &[&str] = &[
    "field.equals",
    "field.not_equals",
    "field.less_than",
    "field.greater_than",
];
const STATE: &[&str] = &["field.enable", "field.disable", "field.toggle"];
const WHO: &[&str] = &["field.global", "field.user", "field.target_user"];
const PERSISTED: &[&str] = &["field.any", "field.persisted", "field.temporary"];
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

/// The label of a check box that isn't a platform.
pub fn check_label(id: &str) -> &'static str {
    crate::i18n::tr(match id {
        "DebatedSocialIssuesAndPolitics" => "tools.ccl_politics",
        "DrugsIntoxication" => "tools.ccl_drugs",
        "SexualThemes" => "tools.ccl_sexual",
        "ViolentGraphic" => "tools.ccl_violence",
        "Gambling" => "tools.ccl_gambling",
        "ProfanityVulgarity" => "tools.ccl_profanity",
        _ => "tools.unknown",
    })
}

// Twitch's AutoMod levels as stored (0 to 4); the last, outside them, leaves a level as it is.
const AUTOMOD_LEVEL: &[&str] = &[
    "field.automod_0",
    "field.automod_1",
    "field.automod_2",
    "field.automod_3",
    "field.automod_4",
    "field.keep_as_is",
];
const AUTOMOD_OVERALL: &[&str] = &[
    "field.automod_0",
    "field.automod_1",
    "field.automod_2",
    "field.automod_3",
    "field.automod_4",
    "field.automod_each",
];
const ON_OFF_TOGGLE: &[&str] = &["tools.turn_on", "tools.turn_off", "tools.toggle"];
const PLATFORM_CHECKS: &[&str] = &crate::engine::PLATFORMS;
const WHO_FIELD: F = ("userLogin", "tools.who", Who);
const PLATFORMS_FIELD: F = ("platforms", "tools.platforms", Checks(PLATFORM_CHECKS));
const MESSAGE_FIELD: F = ("messageId", "tools.message", Message("%msgId%"));
const TEXT_FIELD: F = ("text", "tools.text", Multi);
const BOT_FIELD: F = ("useBot", "tools.use_bot", Bool);
const FALLBACK_FIELD: F = ("fallback", "tools.fallback", Bool);
const COUNT_FIELD: F = ("count", "tools.count", Int);
const REASON_FIELD: F = ("reason", "tools.reason", Text);
const PIN_FIELD: F = ("duration", "tools.pin_for", Duration(1000));
const GROUP_FIELD: F = ("groupName", "tools.group", UserGroup);
const TITLE_FIELD: F = ("title", "tools.title", Text);
const COST_FIELD: F = ("cost", "tools.cost", Int);
const INPUT_FIELD: F = ("inputRequired", "tools.input_required", Bool);
const COLOR_FIELD: F = ("backgroundColor", "tools.color", Color);
const ENABLED_FIELD: F = ("enabled", "tools.enabled", Bool);
const HOOK_FIELD: F = ("webhookUrl", "tools.dc_hook", Text);
// Typed, or picked by title from the broadcasts YouTube listed (empty: the live one).
const BROADCAST_FIELD: F = ("broadcast", "tools.broadcast", Suggest("youtube.broadcasts"));

/// The fields of the Streamerz platform tools (PLAN.md, Version 1.9.0).
fn tool_fields(kind: u64) -> Option<&'static [F]> {
    Some(match kind {
        203 => &[
            ("enable", "tools.rewards_on", Lines),
            ("pause", "tools.rewards_paused", Lines),
            ("disable", "tools.rewards_off", Lines),
            ("disableNotConfigured", "tools.rewards_rest_off", Bool),
        ],
        35104 => &[
            ("enable", "tools.rewards_on", Lines),
            ("pause", "tools.rewards_paused", Lines),
            ("disable", "tools.rewards_off", Lines),
            ("disableNotConfigured", "tools.rewards_rest_off", Bool),
        ],
        99450..=99452 => &[WHO_FIELD, GROUP_FIELD],
        99453 | 99663 => &[COUNT_FIELD],
        99501 => &[WHO_FIELD, REASON_FIELD],
        99454 | 99513 | 99534 | 99535 | 99662 => &[WHO_FIELD],
        99502 => &[TEXT_FIELD, PIN_FIELD, BOT_FIELD, FALLBACK_FIELD],
        99503 => &[WHO_FIELD, TEXT_FIELD, BOT_FIELD],
        99516 | 99517 | 99536 | 99538 | 99606 | 99724 => &[],
        99514 => &[
            ("userLogin", "tools.channel", Who),
            (
                "pick",
                "tools.clip_pick",
                Pick(&["tools.clip_random", "tools.clip_latest", "tools.clip_top"]),
            ),
            ("featured", "tools.featured", Bool),
            ("days", "tools.clip_days", Int),
        ],
        99515 => &[("userLogin", "tools.channel_ours", Text)],
        99518 => &[
            (
                "period",
                "tools.period",
                Pick(&["tools.all_time", "tools.day", "tools.week", "tools.month", "tools.year"]),
            ),
            COUNT_FIELD,
        ],
        99605 => &[
            (
                "period",
                "tools.period",
                Pick(&["tools.all_time", "tools.month", "tools.week"]),
            ),
            COUNT_FIELD,
        ],
        99519 => &[("labels", "tools.labels", Checks(&crate::engine::CONTENT_LABELS))],
        99520 => &[(
            "language",
            "tools.language",
            Expression(&["en", "pt", "es", "fr", "de", "it", "ja", "ko", "zh", "ru", "other"]),
        )],
        99521 => &[("delay", "tools.stream_delay", Duration(1000))],
        99522 => &[
            TITLE_FIELD,
            COST_FIELD,
            ("prompt", "tools.prompt", Multi),
            INPUT_FIELD,
            COLOR_FIELD,
            ENABLED_FIELD,
        ],
        99603 => &[
            TITLE_FIELD,
            COST_FIELD,
            ("description", "tools.prompt", Multi),
            INPUT_FIELD,
            COLOR_FIELD,
            ENABLED_FIELD,
        ],
        99523 => &[("rewardId", "tools.reward", Entity("twitch.rewards"))],
        99604 => &[("rewardId", "tools.reward", Entity("kick.rewards"))],
        99533 => &[(
            "color",
            "tools.chat_color",
            Expression(&[
                "blue",
                "blue_violet",
                "cadet_blue",
                "chocolate",
                "coral",
                "dodger_blue",
                "firebrick",
                "golden_rod",
                "green",
                "hot_pink",
                "orange_red",
                "red",
                "sea_green",
                "spring_green",
                "yellow_green",
            ]),
        )],
        99537 => &[
            ("start", "tools.starts", Text),
            ("duration", "tools.length", Duration(1000)),
            TITLE_FIELD,
            ("category", "tools.category", Text),
            ("recurring", "tools.weekly", Bool),
        ],
        99539 => &[
            ("state", "tools.vacation", Pick(&["tools.turn_on", "tools.turn_off"])),
            ("start", "tools.from", Text),
            ("end", "tools.until", Text),
        ],
        99601 | 99602 => &[("redemptionId", "tools.redemption", Text)],
        99607 => &[("tag", "tools.tag", Text)],
        99541 => &[],
        99542 => &[("userLogin", "tools.channel_ours", Text)],
        99543 => &[
            ("userLogin", "tools.channel_ours", Text),
            (
                "kind",
                "tools.video_kind",
                Pick(&["tools.video_past", "tools.video_upload", "tools.video_highlight"]),
            ),
        ],
        99545 => &[
            ("videoId", "tools.clip_video", Text),
            ("offset", "tools.clip_ends_at", Duration(1000)),
            ("duration", "tools.clip_length", Duration(1000)),
            ("title", "tools.clip_title_video", Text),
        ],
        99544 => &[
            (
                "mode",
                "tools.cm_mode",
                Pick(&[
                    "tools.cm_slow",
                    "tools.cm_followers",
                    "tools.cm_subs",
                    "tools.cm_emotes",
                    "tools.cm_unique",
                ]),
            ),
            ("value", "tools.cm_value", Text),
            ("duration", "tools.cm_for", Duration(1000)),
        ],
        99664 => &[
            BROADCAST_FIELD,
            ("stage", "tools.yt_stage", Pick(&["tools.yt_testing", "tools.yt_live"])),
        ],
        99665 => &[],
        99666 => &[BROADCAST_FIELD, ("playlist", "tools.yt_playlist", Text)],
        99654 => &[BROADCAST_FIELD, ("tags", "tools.tags", Lines)],
        99655 | 99656 => &[BROADCAST_FIELD, ("tag", "tools.tag", Text)],
        99657 | 99660 | 99661 => &[BROADCAST_FIELD],
        99658 => &[
            BROADCAST_FIELD,
            (
                "category",
                "tools.category",
                Pick(&[
                    "field.gaming",
                    "field.people_blogs",
                    "field.entertainment",
                    "field.music",
                    "field.comedy",
                    "field.education",
                    "field.science_technology",
                    "field.sports",
                    "field.howto_style",
                    "field.news_politics",
                    "field.film_animation",
                    "field.autos_vehicles",
                    "field.pets_animals",
                    "field.travel_events",
                ]),
            ),
        ],
        99659 => &[
            BROADCAST_FIELD,
            (
                "privacy",
                "tools.privacy",
                Pick(&["tools.public", "tools.unlisted", "tools.private"]),
            ),
        ],
        99750 => &[
            ("connectionId", "field.connection_empty_any", ObsConnection),
            ("sceneName", "field.scene", Suggest("obs.scenes")),
            ("sourceName", "field.source", Suggest("obs.sources")),
            ("x", "tools.obs_x", Text),
            ("y", "tools.obs_y", Text),
            ("scale", "tools.obs_scale", Text),
            ("rotation", "tools.obs_rotation", Text),
            ("cropLeft", "tools.obs_crop_left", Text),
            ("cropRight", "tools.obs_crop_right", Text),
            ("cropTop", "tools.obs_crop_top", Text),
            ("cropBottom", "tools.obs_crop_bottom", Text),
        ],
        99751 => &[
            ("connectionId", "field.connection_empty_any", ObsConnection),
            ("inputName", "field.source", Suggest("obs.sources")),
            ("level", "tools.obs_level", Text),
            ("unit", "tools.obs_unit", Pick(&["tools.obs_percent", "tools.obs_db"])),
            ("fade", "tools.obs_fade", Duration(1)),
        ],
        99752 => &[
            ("connectionId", "field.connection_empty_any", ObsConnection),
            ("inputName", "field.source", Suggest("obs.sources")),
            (
                "monitor",
                "tools.obs_monitor",
                Pick(&[
                    "tools.obs_monitor_off",
                    "tools.obs_monitor_only",
                    "tools.obs_monitor_and_out",
                ]),
            ),
        ],
        99753 => &[
            ("connectionId", "field.connection_empty_any", ObsConnection),
            ("state", "tools.state", Pick(ON_OFF_TOGGLE)),
        ],
        99754 => &[
            ("connectionId", "field.connection_empty_any", ObsConnection),
            ("sceneName", "field.scene", Suggest("obs.scenes")),
        ],
        99755 | 99758 | 99762 => &[("connectionId", "field.connection_empty_any", ObsConnection)],
        99756 => &[
            ("connectionId", "field.connection_empty_any", ObsConnection),
            ("transitionName", "tools.obs_transition", Text),
            ("duration", "tools.obs_transition_ms", Text),
        ],
        99757 => &[
            ("connectionId", "field.connection_empty_any", ObsConnection),
            (
                "state",
                "tools.state",
                Pick(&["tools.obs_start", "tools.obs_stop", "tools.toggle"]),
            ),
        ],
        99759 => &[
            ("connectionId", "field.connection_empty_any", ObsConnection),
            ("hotkeyName", "tools.obs_hotkey", Text),
        ],
        99760 => &[
            ("connectionId", "field.connection_empty_any", ObsConnection),
            ("profileName", "tools.obs_profile", Text),
        ],
        99761 => &[
            ("connectionId", "field.connection_empty_any", ObsConnection),
            ("collectionName", "tools.obs_collection", Text),
        ],
        99763 => &[
            ("connectionId", "field.connection_empty_any", ObsConnection),
            ("sourceName", "field.source", Suggest("obs.sources")),
            ("filterName", "field.filter", Text),
            ("settings", "tools.obs_settings", Map),
        ],
        99764 => &[
            ("connectionId", "field.connection_empty_any", ObsConnection),
            ("inputName", "field.source", Suggest("obs.sources")),
            ("settings", "tools.obs_settings", Map),
        ],
        99765 => &[
            ("connectionId", "field.connection_empty_any", ObsConnection),
            ("sceneName", "field.scene", Suggest("obs.scenes")),
            ("sourceName", "field.source", Suggest("obs.sources")),
            ("duration", "tools.obs_show_for", Duration(1)),
            ("wait", "tools.obs_wait", Bool),
        ],
        99766 => &[
            ("connectionId", "field.connection_empty_any", ObsConnection),
            ("outputName", "tools.obs_output", Suggest("obs.outputs")),
            (
                "state",
                "tools.state",
                Pick(&["tools.obs_start", "tools.obs_stop", "tools.toggle"]),
            ),
        ],
        99767 => &[
            ("connectionId", "field.connection_empty_any", ObsConnection),
            ("sceneName", "field.scene", Suggest("obs.scenes")),
            ("sourceName", "field.source", Suggest("obs.sources")),
            (
                "state",
                "tools.state",
                Pick(&["tools.obs_lock", "tools.obs_unlock", "tools.toggle"]),
            ),
        ],
        99768 => &[
            ("connectionId", "field.connection_empty_any", ObsConnection),
            ("inputName", "field.source", Suggest("obs.sources")),
            ("offset", "tools.obs_sync_offset", Text),
        ],
        99769 => &[
            ("connectionId", "field.connection_empty_any", ObsConnection),
            ("inputName", "field.source", Suggest("obs.sources")),
            ("balance", "tools.obs_balance", Text),
        ],
        99780 => &[
            ("input", "tools.td_input", Multi),
            ("pattern", "tools.td_pattern", Text),
            ("ignoreCase", "tools.td_ignore_case", Bool),
            ("variableName", "tools.td_variable", Text),
        ],
        99781 => &[
            ("input", "tools.td_input", Multi),
            ("pattern", "tools.td_pattern", Text),
            ("replacement", "tools.td_replacement", Text),
            ("ignoreCase", "tools.td_ignore_case", Bool),
            ("all", "tools.td_all", Bool),
            ("variableName", "tools.td_variable", Text),
        ],
        99782 => &[
            ("input", "tools.td_input", Multi),
            (
                "how",
                "tools.td_how",
                Pick(&[
                    "tools.td_upper",
                    "tools.td_lower",
                    "tools.td_title",
                    "tools.td_trim",
                    "tools.td_reverse",
                    "tools.td_spaces",
                    "tools.td_first",
                    "tools.td_last",
                    "tools.td_url",
                    "tools.td_length",
                ]),
            ),
            ("variableName", "tools.td_variable", Text),
        ],
        99783 => &[
            ("input", "tools.td_input", Multi),
            ("separator", "tools.td_separator", Text),
            ("variableName", "tools.td_variable", Text),
        ],
        99784 => &[
            ("input", "tools.td_json", Multi),
            ("path", "tools.td_path", Text),
            ("variableName", "tools.td_variable", Text),
        ],
        99785 => &[
            ("name", "tools.td_counter", Text),
            (
                "operation",
                "tools.td_op",
                Pick(&["tools.td_add", "tools.td_subtract", "tools.td_set", "tools.td_reset"]),
            ),
            ("amount", "tools.td_amount", Text),
            ("persisted", "tools.td_persisted", Bool),
            ("variableName", "tools.td_variable", Text),
        ],
        99786 => &[
            ("key", "tools.td_gate", Text),
            ("duration", "tools.td_wait", Duration(1000)),
            ("perUser", "tools.td_per_user", Bool),
        ],
        99787 => &[
            ("seconds", "tools.td_seconds", Text),
            (
                "style",
                "tools.td_style",
                Pick(&["tools.td_clock", "tools.td_short", "tools.td_words"]),
            ),
            ("variableName", "tools.td_variable", Text),
        ],
        99788 => &[
            ("name", "tools.td_list", Text),
            (
                "operation",
                "tools.td_op",
                Pick(&[
                    "tools.td_list_add",
                    "tools.td_list_remove",
                    "tools.td_list_has",
                    "tools.td_list_at",
                    "tools.td_list_random",
                    "tools.td_list_count",
                    "tools.td_list_clear",
                    "tools.td_list_take",
                    "tools.td_list_join",
                ]),
            ),
            ("value", "tools.td_list_value", Text),
            ("persisted", "tools.td_persisted", Bool),
            ("variableName", "tools.td_variable", Text),
        ],
        99789 => &[("variableName", "tools.td_variable", Text)],
        99790 => &[("text", "field.text", Multi)],
        99740 => &[
            HOOK_FIELD,
            ("username", "tools.dc_username", Text),
            ("avatar", "tools.dc_avatar", Text),
            ("content", "tools.text", Multi),
            ("title", "tools.title", Text),
            ("description", "tools.prompt", Multi),
            ("color", "tools.color", Color),
            ("url", "tools.dc_url", Text),
            ("author", "tools.dc_author", Text),
            ("footer", "tools.dc_footer", Text),
            ("image", "tools.dc_image", Text),
            ("thumbnail", "tools.dc_thumbnail", Text),
            ("fields", "tools.dc_fields", Map),
            ("inlineFields", "tools.dc_inline", Bool),
            ("timestamp", "tools.dc_timestamp", Bool),
            ("threadId", "tools.dc_thread", Text),
            ("threadName", "tools.dc_thread_name", Text),
        ],
        99741 => &[
            HOOK_FIELD,
            ("messageId", "tools.dc_message", Message("%messageId%")),
            ("threadId", "tools.dc_thread", Text),
            ("content", "tools.text", Multi),
            ("title", "tools.title", Text),
            ("description", "tools.prompt", Multi),
            ("color", "tools.color", Color),
            ("url", "tools.dc_url", Text),
            ("author", "tools.dc_author", Text),
            ("footer", "tools.dc_footer", Text),
            ("image", "tools.dc_image", Text),
            ("thumbnail", "tools.dc_thumbnail", Text),
            ("fields", "tools.dc_fields", Map),
            ("inlineFields", "tools.dc_inline", Bool),
        ],
        99742 => &[
            HOOK_FIELD,
            ("messageId", "tools.dc_message", Message("%messageId%")),
            ("threadId", "tools.dc_thread", Text),
        ],
        99743 => &[
            HOOK_FIELD,
            ("file", "tools.dc_file", Text),
            ("fileName", "tools.dc_file_name", Text),
            ("content", "tools.text", Multi),
            ("username", "tools.dc_username", Text),
            ("avatar", "tools.dc_avatar", Text),
            ("threadId", "tools.dc_thread", Text),
        ],
        99720 => &[PLATFORMS_FIELD, TEXT_FIELD, BOT_FIELD, FALLBACK_FIELD],
        99721 => &[MESSAGE_FIELD, TEXT_FIELD, BOT_FIELD, FALLBACK_FIELD],
        99723 => &[MESSAGE_FIELD, PIN_FIELD],
        99726 | 99733 => &[PLATFORMS_FIELD, WHO_FIELD],
        99729 => &[PLATFORMS_FIELD, TITLE_FIELD],
        99730 => &[PLATFORMS_FIELD, ("category", "tools.category", Text)],
        99731 => &[PLATFORMS_FIELD],
        99734 => &[
            PLATFORMS_FIELD,
            ("title", "tools.update_title", Text),
            ("category", "tools.update_category", Text),
            ("tags", "tools.update_tags", Lines),
        ],
        99732 => &[PLATFORMS_FIELD, WHO_FIELD, ("text", "tools.shoutout_text", Multi)],
        _ => return None,
    })
}

/// A core sub-action's fields; None when it has no form (it opens as JSON).
pub fn sub_fields(kind: u64) -> Option<&'static [F]> {
    if let Some(fields) = tool_fields(kind) {
        return Some(fields);
    }
    Some(match kind {
        99810..=99816 | 99830..=99833 => &[],
        99827 => &[("query", "spotify.query", Text)],
        99834 => &[(
            "state",
            "spotify.requests_state",
            Pick(&[
                "spotify.requests_open",
                "spotify.requests_close",
                "spotify.requests_toggle",
            ]),
        )],
        99835 => &[("userLogin", "tools.who", Who)],
        99836 => &[
            (
                "what",
                "spotify.block_what",
                Pick(&["spotify.block_viewer", "spotify.block_artist"]),
            ),
            ("value", "spotify.block_name", Text),
        ],
        99837 => &[
            (
                "rule",
                "spotify.rule",
                Pick(&[
                    "spotify.rule_who",
                    "spotify.rule_max_per_user",
                    "spotify.rule_max_per_subscriber",
                    "spotify.rule_unlimited_who",
                    "spotify.rule_max_length",
                    "spotify.rule_block_explicit",
                    "spotify.rule_explicit_who",
                    "spotify.rule_skip_who",
                    "spotify.rule_skip_own",
                    "spotify.rule_vote_skip_count",
                    "spotify.rule_skip_cooldown",
                    "spotify.rule_control_who",
                ]),
            ),
            ("value", "spotify.rule_value", Text),
        ],
        99845 => &[
            ("value", "spotify.fade_to", Text),
            ("duration", "spotify.fade_over", Duration(1000)),
        ],
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
            ("open", "spotify.open", Expression(&["true", "false"])),
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
            ("msgClosed", "spotify.msg_closed", Text),
            ("msgError", "spotify.msg_error", Text),
            ("msgEmpty", "spotify.msg_empty", Text),
            ("msgNoPos", "spotify.msg_no_pos", Text),
            ("msgNoRemove", "spotify.msg_no_remove", Text),
            ("msgNoVolume", "spotify.msg_no_volume", Text),
        ],
        99880 => &[("topic", "overlay.topic", Text), ("value", "overlay.value", Multi)],
        99881 => &[
            ("overlay", "overlay.control_overlay", Overlay),
            ("control", "overlay.control_name", Text),
            ("value", "overlay.control_value", Text),
        ],
        99803 => &[
            ("clockId", "countdown.id", Text),
            (
                "operation",
                "countdown.operation",
                Expression(&["reset", "add", "subtract", "set", "pause", "resume", "toggle", "get"]),
            ),
            ("startMs", "countdown.start", Text),
            ("maxMs", "countdown.max", Text),
            ("perUnitMs", "countdown.per_unit", Text),
            ("units", "countdown.units", Text),
            ("expectedCurrency", "countdown.currency", Text),
        ],
        2 => &[("port", "field.port", Int), ("payload", "field.payload", Multi)],
        4 => &[
            ("actionId", "field.action", Action),
            ("runImmedately", "field.run_immediately", Bool),
        ],
        6 => &[
            ("command", "field.command", Text),
            ("arguments", "field.arguments", Text),
            ("workingDir", "field.working_directory", Text),
            ("waitForExit", "automation.duration_wait_for_exit", Duration(1000)),
            ("envVars", "field.environment_name_value", Map),
        ],
        8 => &[("timerId", "field.timer", Timer), ("state", "field.state", Pick(STATE))],
        9 => &[
            ("commandId", "field.command", Command),
            ("state", "field.state", Pick(STATE)),
        ],
        10 => &[
            ("text", "field.message", Multi),
            ("useBot", "field.send_from_bot_account", Bool),
            ("fallback", "field.fallback_to_broadcaster", Bool),
        ],
        11 => &[("status", "field.status", Pick(&["field.fulfilled", "field.canceled"]))],
        12 => &[("status", "field.subscriber_only", Bool)],
        13 => &[("status", "field.emote_only", Bool)],
        14 => &[
            ("status", "field.slow_mode", Bool),
            ("duration", "automation.duration_duration", Duration(1000)),
        ],
        15 => &[("title", "field.title", Text)],
        16 => &[
            ("source", "field.source", Pick(&["field.game_name", "field.game_id"])),
            ("game", "field.game", Text),
            ("gameId", "field.game_id", Text),
        ],
        17 => &[("description", "field.description", Text)],
        18 => &[
            ("title", "tools.clip_title_stream", Text),
            ("duration", "tools.clip_length", Duration(1000)),
            ("hasDelay", "automation.clip_delay", Bool),
        ],
        19 => &[("duration", "automation.duration_duration", Duration(1000))],
        20 => &[
            ("userLogin", "tools.who", Who),
            ("duration", "automation.duration_duration", Duration(1000)),
            ("reason", "field.reason", Multi),
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
        22 | 35012 => &[("count", "field.count", Int)],
        23 => &[
            ("text", "field.announcement", Multi),
            // Twitch's own names, as it stores them.
            (
                "color",
                "field.color",
                Expression(&["Primary", "Blue", "Green", "Orange", "Purple"]),
            ),
            ("useBot", "field.send_from_bot_account", Bool),
            ("fallback", "field.fallback_to_broadcaster", Bool),
        ],
        25 => &[],
        27 => &[("status", "field.shield_mode", Bool)],
        28 => &[
            ("status", "field.follower_mode", Bool),
            ("duration", "automation.duration_duration", Duration(60_000)),
        ],
        29 | 50 | 51 | 52 | 500 | 502..=505 | 512 | 513 | 526 | 527 | 580 => &[("userLogin", "tools.who", Who)],
        200 => &[
            ("rewardId", "field.reward", Entity("twitch.rewards")),
            ("state", "field.state", Pick(STATE)),
        ],
        201 => &[
            ("rewardId", "field.reward", Entity("twitch.rewards")),
            ("state", "field.state", Pick(STATE)),
        ],
        202 => &[
            ("rewardId", "field.reward", Entity("twitch.rewards")),
            ("cost", "field.cost", Int),
            (
                "operator",
                "field.operation",
                Pick(&["field.set", "field.add", "field.subtract"]),
            ),
        ],
        204 => &[
            ("rewardId", "field.reward", Entity("twitch.rewards")),
            ("cooldown", "automation.duration_cooldown", Duration(1000)),
            ("additive", "field.additive", Bool),
        ],
        205 => &[
            ("rewardId", "field.reward", Entity("twitch.rewards")),
            ("title", "field.title", Text),
        ],
        206 => &[
            ("rewardId", "field.reward", Entity("twitch.rewards")),
            ("prompt", "field.prompt", Multi),
        ],
        207 => &[
            ("rewardId", "field.reward", Entity("twitch.rewards")),
            ("title", "field.title", Text),
            ("prompt", "field.prompt", Multi),
            ("cost", "field.cost", Int),
            (
                "operator",
                "field.operation",
                Pick(&["field.set", "field.add", "field.subtract"]),
            ),
        ],
        208..=210 => &[("rewardId", "field.reward", Entity("twitch.rewards"))],
        211 => &[
            ("groupName", "tools.reward_group", Text),
            ("state", "field.state", Pick(STATE)),
        ],
        212 => &[
            ("groupName", "tools.reward_group", Text),
            ("state", "field.state", Pick(STATE)),
        ],
        213 => &[
            ("rewardId", "field.reward", Entity("twitch.rewards")),
            ("backgroundColor", "tools.color", Color),
            ("random", "field.random", Bool),
        ],
        214 | 215 => &[
            ("rewardId", "field.reward", Entity("twitch.rewards")),
            ("redeems", "field.max_redeems", Int),
            ("additive", "field.additive", Bool),
        ],
        252 => &[("tags", "field.tags", Lines)],
        253 | 254 => &[("tag", "field.tag", Text)],
        511 => &[("userLogin", "tools.who", Who), ("reason", "field.reason", Multi)],
        521 => &[
            ("moderatorSendLiveEnabled", "field.mod_send_live", Bool),
            ("slotCount", "field.slot_count", Int),
            ("browserSourceAudioEnabled", "field.audio_enabled", Bool),
            ("groupLayout", "field.group_layout", Text),
            ("regenerateBrowserSources", "field.regenerate_sources", Bool),
        ],
        528 => &[("userLogin", "tools.who", Who), ("slot", "field.slot", Int)],
        529 => &[
            ("sourceSlot", "field.source_slot", Int),
            ("destinationSlot", "field.destination_slot", Int),
        ],
        530 => &[("userLogin", "tools.who", Who), ("slot", "field.slot", Int)],
        531 => &[
            ("slot", "field.slot", Int),
            ("audioEnabled", "field.audio_enabled", Bool),
            ("videoEnabled", "field.video_enabled", Bool),
            ("live", "field.live", Bool),
            ("volume", "field.volume", Num),
        ],
        550 => &[("userIndex", "field.user_index", Int)],
        552 => &[("value", "field.value", Int)],
        553 | 554 | 558 => &[("userLogin", "tools.who", Who), ("groupName", "field.group", UserGroup)],
        560 => &[
            ("text", "field.reply", Multi),
            ("replyId", "tools.reply_to", Message("%msgId%")),
            ("useBot", "field.send_from_bot_account", Bool),
            ("fallback", "field.fallback_to_broadcaster", Bool),
        ],
        561 => &[
            ("title", "field.title", Text),
            ("duration", "automation.duration_duration", Duration(1000)),
            ("channelPointsPerVote", "field.points_per_vote", Int),
            ("choices", "field.choices_one_per_line", Lines),
        ],
        564 => &[("status", "field.branded_content", Bool)],
        565 => &[("userLogin", "tools.who", Who), ("reason", "field.reason", Multi)],
        568 => &[
            ("title", "field.title", Text),
            (
                "predictionWindow",
                "automation.duration_prediction_window",
                Duration(1000),
            ),
            ("outcomes", "field.outcomes_one_per_line", Lines),
        ],
        571 => &[("winningIndex", "field.winning_outcome_index", Int)],
        572 => &[("outcomeTitle", "field.winning_outcome_title", Text)],
        573 => &[("clipId", "field.clip_id", Text)],
        576 => &[
            ("messageId", "tools.message", Message("%msgId%")),
            ("duration", "automation.duration_duration", Duration(1000)),
        ],
        577 => &[("duration", "automation.duration_duration", Duration(1000))],
        579 => &[
            ("userLogin", "tools.who", Who),
            ("status", "field.status", Pick(&["field.monitored", "field.restricted"])),
        ],
        581 => &[MESSAGE_FIELD],
        582 => &[
            ("messageId", "tools.held_message", Message("%messageId%")),
            ("action", "field.action", Pick(&["field.allow", "field.deny"])),
        ],
        583 => &[("text", "field.term", Text)],
        584 => &[("termId", "field.term_its_id_or_text", Text)],
        585 | 590 | 591 | 595 | 596 => &[],
        586 => &[
            ("unbanRequestId", "field.unban_request_id", Text),
            ("status", "field.decision", Pick(&["field.approve", "field.deny"])),
            ("resolutionText", "field.resolution_text", Text),
        ],
        587 => &[(
            "status",
            "field.status",
            Pick(&[
                "field.pending",
                "field.approved",
                "field.denied",
                "field.acknowledged",
                "field.canceled",
            ]),
        )],
        588 => &[("status", "field.enabled", Bool)],
        589 => &[
            ("status", "field.enabled", Bool),
            ("duration", "field.delay_2_4_or_6s", Duration(1000)),
        ],
        592 => &[
            ("overallLevel", "field.automod_overall", Pick(AUTOMOD_OVERALL)),
            ("aggression", "field.automod_aggression", Pick(AUTOMOD_LEVEL)),
            ("bullying", "field.automod_bullying", Pick(AUTOMOD_LEVEL)),
            ("disability", "field.automod_disability", Pick(AUTOMOD_LEVEL)),
            ("misogyny", "field.automod_misogyny", Pick(AUTOMOD_LEVEL)),
            ("raceEthnicityOrReligion", "field.automod_race", Pick(AUTOMOD_LEVEL)),
            ("sexBasedTerms", "field.automod_sex_terms", Pick(AUTOMOD_LEVEL)),
            ("sexualitySexOrGender", "field.automod_sexuality", Pick(AUTOMOD_LEVEL)),
            ("swearing", "field.automod_swearing", Pick(AUTOMOD_LEVEL)),
        ],
        593 => &[
            ("msgId", "field.message_id_empty_auto", Text),
            ("message", "field.message", Text),
        ],
        594 => &[("userLogin", "field.user_login_empty_all", Text)],

        120 => &[
            ("input", "field.input", Text),
            ("operation", "field.operation", Pick(OPS)),
            ("value", "field.value", Text),
            ("autoType", "field.auto_type", Bool),
        ],
        121 => &[
            ("source", "field.source", Pick(WHO)),
            ("variableName", "field.variable_name", Text),
            ("persisted", "field.persisted", Bool),
            ("destinationVariable", "field.destination_variable", Text),
            ("defaultValue", "field.default_value", Text),
        ],
        122 => &[
            ("destination", "field.destination", Pick(WHO)),
            ("variableName", "field.variable_name", Text),
            ("persisted", "field.persisted", Bool),
            (
                "source",
                "field.set",
                Pick(&["field.value", "field.increment", "field.decrement", "field.unset"]),
            ),
            ("value", "field.value", Text),
            ("autoType", "field.auto_type", Bool),
        ],
        123 => &[
            ("variableName", "field.variable_name", Text),
            ("value", "field.value", Multi),
            ("autoType", "field.auto_type", Bool),
        ],
        127 => &[("input", "field.input", Text), ("autoType", "field.auto_type", Bool)],
        128 => &[
            ("input", "field.input", Text),
            ("operation", "field.operation", Pick(WHILE_OPS)),
            ("value", "field.value", Text),
            ("autoType", "field.auto_type", Bool),
            ("ignoreCase", "field.ignore_case", Bool),
        ],
        124 | 125 | 126 | 129 | 130 => &[],
        1001 => &[
            ("actionQueueId", "field.queue_none_all", Queue),
            ("state", "field.state", Pick(&["field.pause", "field.resume"])),
            ("clear", "field.clear_the_queue", Bool),
        ],
        1002 => &[
            ("value", "automation.duration_delay", Duration(1)),
            ("random", "field.random", Bool),
            ("maxValue", "automation.duration_up_to", Duration(1)),
        ],
        1003 => &[
            ("randomType", "field.type", Pick(&["field.between", "field.next_float"])),
            ("min", "field.min", Int),
            ("max", "field.max", Int),
        ],
        1004 => &[
            ("actionId", "field.action", Action),
            ("state", "field.state", Pick(STATE)),
        ],
        1005 => &[("actionQueueId", "field.queue", Queue)],
        1007 => &[
            ("url", "field.url", Text),
            ("variableName", "field.variable_name", Text),
            ("headers", "field.headers_name_value", Map),
            ("parseAsJson", "field.parse_as_json", Bool),
            ("autoType", "field.auto_type", Bool),
            (
                "method",
                "tools.http_method",
                Pick(&["GET", "POST", "PUT", "PATCH", "DELETE"]),
            ),
            ("body", "tools.http_body", Multi),
            (
                "contentType",
                "tools.http_content_type",
                Expression(&["application/json", "application/x-www-form-urlencoded", "text/plain"]),
            ),
        ],
        1009 => &[("value", "field.comment", Multi), ("color", "field.color", Color)],
        1010 => &[
            ("groupName", "field.group", CommandGroup),
            ("state", "field.state", Pick(STATE)),
        ],
        1011 => &[
            ("groupName", "field.group", ActionGroup),
            ("state", "field.state", Pick(STATE)),
        ],
        1013 => &[
            ("groupName", "field.group", CommandGroup),
            ("variableName", "field.variable_name", Text),
            ("includeAll", "field.include_all_commands", Bool),
            ("ignoreAliases", "field.first_alias_only", Bool),
            ("hasPermission", "field.only_permitted_ones", Bool),
        ],
        1014 => &[("commandId", "field.command", Command)],
        1015 => &[("groupName", "field.group", CommandGroup)],
        1016 => &[("actionId", "field.action", Action)],
        1017 => &[("groupName", "field.group", ActionGroup)],
        1020 => &[("message", "field.message", Multi)],
        1022 => &[
            ("actionQueueId", "field.queue", Queue),
            ("state", "field.state", Pick(&["field.blocking", "field.non_blocking"])),
        ],
        1023 => &[
            ("title", "field.title", Text),
            ("prompt", "field.prompt", Text),
            ("originalText", "field.starting_text", Text),
            ("outputVariable", "field.output_variable", Text),
            ("autoTypeOutput", "field.auto_type", Bool),
        ],
        1028 => &[
            ("actionId", "field.action", Action),
            ("subActionId", "field.sub_action", Sub),
            ("state", "field.state", Pick(STATE)),
        ],
        1031 => &[
            ("signalName", "field.signal", Text),
            ("useArge", "field.send_the_arguments", Bool),
            ("customArgs", "field.arguments_name_value", Map),
            ("queueSignal", "field.queue_if_nobody_waits", Bool),
        ],
        1032 => &[
            ("signalName", "field.signal", Text),
            ("overwrite", "field.overwrite_arguments", Bool),
            ("timeout", "automation.duration_timeout", Duration(1)),
        ],
        1033 => &[
            ("actionId", "field.action", Action),
            ("triggerId", "field.trigger", Trigger),
            ("state", "field.state", Pick(STATE)),
        ],
        1034 => &[
            ("eventName", "field.event_name", Text),
            ("useArgs", "field.send_the_arguments", Bool),
        ],
        1038 => &[
            ("actionId", "field.action", Action),
            ("subActionId", "field.sub_action", Sub),
            ("weight", "field.weight", Num),
        ],
        10001 => &[("groupId", "field.group", UserGroupId)],
        401 => &[
            ("connectionId", "field.connection", SlobsConnection),
            ("sceneName", "field.scene_empty_active", Text),
            ("sourceName", "field.source", Suggest("obs.sources")),
            ("state", "field.state", Pick(SD_SHOW)),
            ("display", "field.display", Pick(SD_DISPLAY)),
        ],
        402 | 403 => &[
            ("connectionId", "field.connection", SlobsConnection),
            ("sceneName", "field.scene_empty_active", Text),
            ("groupName", "field.group", Text),
            ("display", "field.display", Pick(SD_DISPLAY)),
        ],
        404 => &[
            ("connectionId", "field.connection", SlobsConnection),
            ("state", "field.state", Pick(SD_STREAM)),
        ],
        405 => &[
            ("connectionId", "field.connection", SlobsConnection),
            ("sceneName", "field.scene", Suggest("obs.scenes")),
        ],
        406 => &[
            ("connectionId", "field.connection", SlobsConnection),
            ("sceneName", "field.scene_empty_active", Text),
            ("sourceName", "field.source", Suggest("obs.sources")),
            ("filterName", "field.filter", Text),
            ("state", "field.state", Pick(SD_SHOW)),
        ],
        407 => &[
            ("connectionId", "field.connection", SlobsConnection),
            ("flipMode", "field.flip", Pick(FLIP_MODE)),
            ("sceneName", "field.scene_empty_active", Text),
            ("sourceName", "field.source", Suggest("obs.sources")),
            ("display", "field.display", Pick(SD_DISPLAY)),
        ],
        408 => &[
            ("connectionId", "field.connection", SlobsConnection),
            ("degrees", "field.degrees", Num),
            ("additive", "field.additive_rotate_by_not_to", Bool),
            ("sceneName", "field.scene_empty_active", Text),
            ("sourceName", "field.source", Suggest("obs.sources")),
            ("display", "field.display", Pick(SD_DISPLAY)),
        ],
        409 => &[
            ("connectionId", "field.connection", SlobsConnection),
            ("sceneName", "field.scene_empty_active", Text),
            ("sourceName", "field.source", Suggest("obs.sources")),
            ("url", "field.url", Text),
        ],
        410 => &[
            ("connectionId", "field.connection", SlobsConnection),
            ("sceneName", "field.scene_empty_active", Text),
            ("sourceName", "field.source", Suggest("obs.sources")),
            ("text", "field.text", Multi),
        ],
        411 => &[
            ("connectionId", "field.connection", SlobsConnection),
            ("sceneName", "field.scene_empty_active", Text),
            ("sourceName", "field.source", Suggest("obs.sources")),
            ("state", "field.state", Pick(SD_SHOW)),
        ],
        413 => &[
            ("connectionId", "field.connection", SlobsConnection),
            ("sceneName", "field.scene_empty_active", Text),
            ("sourceName", "field.source", Suggest("obs.sources")),
            ("state", "field.state", Pick(SD_MUTE)),
        ],
        412 => &[
            ("connectionId", "field.connection", SlobsConnection),
            ("sceneName", "field.scene_empty_active", Text),
            ("sourceName", "field.source", Suggest("obs.sources")),
        ],
        414 => &[("connectionId", "field.connection", SlobsConnection)],
        415 => &[
            ("connectionId", "field.connection", SlobsConnection),
            ("state", "field.state", Pick(SD_RECORD)),
        ],
        417 => &[
            ("connectionId", "field.connection", SlobsConnection),
            ("sceneName", "field.scene_empty_active", Text),
            ("filterName", "field.filter", Text),
            ("state", "field.state", Pick(SD_SHOW)),
        ],
        99900 => &[
            ("name", "field.name", Text),
            ("random", "field.random", Bool),
            ("color", "field.color", Color),
        ],
        30 => &[
            ("connectionId", "field.connection", ObsConnection),
            ("sceneName", "field.scene", Suggest("obs.scenes")),
            ("sourceName", "field.source", Suggest("obs.sources")),
            ("state", "field.state", Pick(STATE_HIDE_SHOW_TOGGLE)),
        ],
        31 => &[
            ("connectionId", "field.connection", ObsConnection),
            ("sceneName", "field.scene", Suggest("obs.scenes")),
            ("groupName", "field.group", Text),
        ],
        32 => &[
            ("connectionId", "field.connection", ObsConnection),
            ("sceneName", "field.scene", Suggest("obs.scenes")),
            ("groupName", "field.group", Text),
        ],
        33 => &[
            ("connectionId", "field.connection", ObsConnection),
            ("state", "field.state", Pick(STREAM_STATE)),
        ],
        34 => &[
            ("connectionId", "field.connection", ObsConnection),
            ("sceneName", "field.scene", Suggest("obs.scenes")),
        ],
        35 => &[
            ("connectionId", "field.connection", ObsConnection),
            ("sceneName", "field.scene", Suggest("obs.scenes")),
            ("sourceName", "field.source", Suggest("obs.sources")),
            ("filterName", "field.filter", Text),
            ("state", "field.state", Pick(FILTER_STATE)),
        ],
        36 => &[
            ("connectionId", "field.connection", ObsConnection),
            ("flipMode", "field.flip_mode", Pick(FLIP_MODE)),
            ("sceneName", "field.scene", Suggest("obs.scenes")),
            ("sourceName", "field.source", Suggest("obs.sources")),
        ],
        37 => &[
            ("connectionId", "field.connection", ObsConnection),
            ("degrees", "field.degrees", Num),
            ("additive", "field.additive", Bool),
            ("sceneName", "field.scene", Suggest("obs.scenes")),
            ("sourceName", "field.source", Suggest("obs.sources")),
        ],
        38 => &[
            ("connectionId", "field.connection", ObsConnection),
            ("sceneName", "field.scene", Suggest("obs.scenes")),
            ("sourceName", "field.source", Suggest("obs.sources")),
            ("url", "field.url", Text),
        ],
        39 => &[
            ("connectionId", "field.connection", ObsConnection),
            ("sceneName", "field.scene", Suggest("obs.scenes")),
            ("sourceName", "field.source", Suggest("obs.sources")),
            ("text", "field.text", Text),
        ],
        40 => &[
            ("connectionId", "field.connection", ObsConnection),
            ("sceneName", "field.scene", Suggest("obs.scenes")),
            ("sourceName", "field.source", Suggest("obs.sources")),
            ("state", "field.state", Pick(FILTER_STATE)),
        ],
        41 => &[
            ("connectionId", "field.connection", ObsConnection),
            ("sceneName", "field.scene", Suggest("obs.scenes")),
            ("sourceName", "field.source", Suggest("obs.sources")),
        ],
        42 => &[
            ("connectionId", "field.connection", ObsConnection),
            ("sceneName", "field.scene", Suggest("obs.scenes")),
            ("sourceName", "field.source", Suggest("obs.sources")),
            ("state", "field.state", Pick(MUTE_STATE)),
        ],
        43 => &[("connectionId", "field.connection", ObsConnection)],
        44 => &[
            ("connectionId", "field.connection", ObsConnection),
            ("sceneName", "field.scene", Suggest("obs.scenes")),
            ("sourceName", "field.source", Suggest("obs.sources")),
            (
                "track",
                "field.track",
                Pick(&[
                    "field.track_1",
                    "field.track_2",
                    "field.track_3",
                    "field.track_4",
                    "field.track_5",
                    "field.track_6",
                ]),
            ),
            ("state", "field.state", Pick(TRACK_STATE)),
        ],
        45 => &[
            ("connectionId", "field.connection", ObsConnection),
            ("state", "field.state", Pick(RECORD_STATE)),
        ],
        46 => &[
            ("connectionId", "field.connection", ObsConnection),
            ("name", "field.request_type", Text),
            ("prefix", "field.prefix", Text),
            ("raw", "field.raw_json", Text),
            ("resultsToArgs", "field.results_to_args", Bool),
        ],
        47 => &[
            ("connectionId", "field.connection", ObsConnection),
            ("sceneName", "field.scene", Suggest("obs.scenes")),
            ("filterName", "field.filter", Text),
            ("state", "field.state", Pick(FILTER_STATE)),
        ],
        48 => &[
            ("connectionId", "field.connection", ObsConnection),
            ("sceneName", "field.scene", Suggest("obs.scenes")),
            ("sourceName", "field.source", Suggest("obs.sources")),
            ("path", "field.file_path", Text),
            ("quality", "field.quality", Int),
        ],
        49 => &[
            ("connectionId", "field.connection", ObsConnection),
            ("sceneName", "field.scene", Suggest("obs.scenes")),
            ("sourceName", "field.source", Suggest("obs.sources")),
            ("state", "field.state", Pick(MEDIA_STATE)),
        ],
        321 => &[
            ("connectionId", "field.connection", ObsConnection),
            ("state", "field.state", Pick(REPLAY_STATE)),
        ],
        322 => &[
            ("connectionId", "field.connection", ObsConnection),
            ("sceneName", "field.scene", Suggest("obs.scenes")),
            ("sourceName", "field.source", Suggest("obs.sources")),
            ("fileName", "field.file_name", Text),
        ],
        323 => &[
            ("connectionId", "field.connection", ObsConnection),
            ("sceneName", "field.scene", Suggest("obs.scenes")),
            ("sourceName", "field.source", Suggest("obs.sources")),
            ("fileName", "field.file_name", Text),
        ],
        324 => &[
            ("connectionId", "field.connection", ObsConnection),
            ("sceneName", "field.scene", Suggest("obs.scenes")),
            ("sourceName", "field.source", Suggest("obs.sources")),
        ],
        325 => &[
            ("connectionId", "field.connection", ObsConnection),
            ("sceneName", "field.scene", Suggest("obs.scenes")),
        ],
        326 => &[
            ("connectionId", "field.connection", ObsConnection),
            ("sceneName", "field.scene", Suggest("obs.scenes")),
        ],
        327 => &[
            ("connectionId", "field.connection", ObsConnection),
            ("sceneName", "field.scene", Suggest("obs.scenes")),
            ("sourceName", "field.source", Suggest("obs.sources")),
            ("color", "field.color", Text),
            ("random", "field.random", Bool),
        ],
        328 => &[("connectionId", "field.connection", ObsConnection)],
        329 => &[
            ("connectionId", "field.connection", ObsConnection),
            ("chapterName", "field.chapter_name", Text),
        ],
        4001 => &[
            BROADCAST_FIELD,
            ("text", "field.message", Text),
            ("useBot", "field.use_bot_account", Bool),
            ("fallback", "field.fallback_to_broadcaster_2", Bool),
        ],
        4002 => &[BROADCAST_FIELD, ("title", "field.title", Text)],
        4003 => &[BROADCAST_FIELD, ("description", "field.description", Text)],
        4004 => &[
            BROADCAST_FIELD,
            ("title", "field.title", Text),
            ("description", "field.description", Text),
        ],
        4005..=4007 => &[("userLogin", "tools.who", Who), ("groupName", "field.group_name", Text)],
        4008 => &[BROADCAST_FIELD, ("userId", "tools.who", Who)],
        4009 => &[
            BROADCAST_FIELD,
            ("userId", "tools.who", Who),
            ("duration", "automation.duration_duration", Duration(1000)),
        ],
        4010 => &[],
        4011 => &[
            BROADCAST_FIELD,
            ("question", "field.question", Text),
            ("options", "field.options_one_per_line", Lines),
        ],
        4012 => &[BROADCAST_FIELD],
        4013 => &[BROADCAST_FIELD, ("file", "field.image_file", Text)],
        4014 => &[
            BROADCAST_FIELD,
            ("duration", "automation.duration_duration", Duration(1000)),
        ],
        4015 => &[
            ("banId", "field.ban_id_empty_the_one_streamerz_made", Text),
            ("userId", "tools.who", Who),
        ],
        4016 => &[MESSAGE_FIELD],
        4017 => &[BROADCAST_FIELD, ("userId", "tools.who", Who)],
        4018 => &[
            BROADCAST_FIELD,
            ("moderatorId", "field.moderator_id_empty_found_from_who", Text),
            ("userId", "tools.who", Who),
        ],
        35001 => &[
            ("text", "field.message", Text),
            ("useBot", "field.use_bot_account", Bool),
            ("fallback", "field.fallback_to_broadcaster_2", Bool),
        ],
        35002 => &[
            ("text", "field.message", Text),
            ("useBot", "field.use_bot_account", Bool),
            ("fallback", "field.fallback_to_broadcaster_2", Bool),
            ("replyId", "tools.reply_to", Message("%msgId%")),
        ],
        35010 => &[],
        35011 => &[("userLogin", "tools.who", Who)],
        35020..=35022 => &[("userLogin", "tools.who", Who), ("groupName", "field.group_name", Text)],
        35030 => &[("title", "field.title", Text)],
        35031 => &[
            ("categoryName", "field.category_name", Text),
            ("categoryId", "field.category_id", Text),
        ],
        35032 => &[("tags", "field.tags_one_per_line", Lines)],
        35033 => &[("tag", "field.tag", Text)],
        35034 => &[],
        35050 => &[("duration", "automation.duration_duration", Duration(1000))],
        35051 => &[],
        35040 => &[("userLogin", "tools.who", Who), ("reason", "field.reason", Text)],
        35041 | 35043 => &[("userLogin", "tools.who", Who)],
        35042 => &[
            ("userLogin", "tools.who", Who),
            ("duration", "automation.duration_duration", Duration(1000)),
            ("reason", "field.reason", Text),
        ],
        35044 => &[MESSAGE_FIELD],
        35101 | 35102 => &[
            ("rewardId", "field.reward", Entity("kick.rewards")),
            ("state", "field.state", Bool),
        ],
        35103 => &[
            ("rewardId", "field.reward", Entity("kick.rewards")),
            ("cost", "field.cost", Int),
            (
                "operator",
                "field.operation",
                Pick(&["field.set", "field.add", "field.subtract"]),
            ),
        ],
        35105 => &[
            ("rewardId", "field.reward", Entity("kick.rewards")),
            ("title", "field.title", Text),
        ],
        35106 => &[
            ("rewardId", "field.reward", Entity("kick.rewards")),
            ("description", "field.description", Text),
        ],
        35107 => &[
            ("rewardId", "field.reward", Entity("kick.rewards")),
            ("title", "field.title", Text),
            ("description", "field.description", Text),
            ("cost", "field.cost", Int),
            (
                "operator",
                "field.operation",
                Pick(&["field.set", "field.add", "field.subtract"]),
            ),
        ],
        35108..=35110 => &[("rewardId", "field.reward", Entity("kick.rewards"))],
        35111 | 35112 => &[
            ("groupName", "tools.reward_group", Text),
            ("state", "field.state", Bool),
        ],
        35113 => &[
            ("rewardId", "field.reward", Entity("kick.rewards")),
            ("backgroundColor", "tools.color", Color),
            ("random", "field.random", Bool),
        ],
        43002 => &[("count", "field.how_many", Int)],
        43003 | 1026 | 1027 | 1035 => &[],
        1000 => &[
            ("quoteType", "field.type", Pick(&["field.random", "field.specific"])),
            ("quoteId", "field.quote_id_specific", Int),
        ],
        1036 => &[("quoteId", "field.quote_id", Int)],
        1037 => &[("quoteVariable", "field.quote", Text)],
        43001 => &[("text", "field.message", Text)],
        43010 => &[("title", "field.stream_title", Text)],
        43011 => &[("msgId", "tools.message", Message("%msgId%"))],
        43012 => &[("msgId", "tools.message", Message("%msgId%"))],
        7001 => &[
            ("name", "field.name", Text),
            ("username", "field.username", Text),
            ("webhookUrl", "field.webhook_url", Text),
            ("content", "field.content", Multi),
            ("image", "field.image_url", Text),
            ("avatar", "field.avatar_url", Text),
            ("textToSpeech", "field.text_to_speech", Bool),
        ],
        8001 => &[("eventName", "field.event_name_2", Text), ("text", "field.text", Multi)],
        8002 => &[
            ("eventName", "field.event_name_2", Text),
            ("args", "field.arguments", Map),
        ],
        15001 => &[
            ("userLogin", "field.user_login", Text),
            ("prefix", "field.prefix", Text),
        ],
        20001 => &[
            ("instanceId", "field.instance", Text),
            ("eventName", "field.event_name_2", Text),
            ("useArge", "field.use_current_arguments", Bool),
            ("customArgs", "field.custom_arguments", Map),
        ],
        20002 => &[
            ("instanceId", "field.instance", Text),
            ("signalName", "field.signal_name", Text),
            ("useArge", "field.use_current_arguments", Bool),
            ("customArgs", "field.custom_arguments", Map),
        ],
        20003 => &[
            ("instanceId", "field.instance", Text),
            ("signalName", "field.signal_name", Text),
            ("overwrite", "field.overwrite", Bool),
            ("timeout", "automation.duration_timeout", Duration(1)),
        ],
        1012 => &[("key", "field.key", Text), ("modifiers", "field.modifiers", Int)],
        9001 => &[
            ("device", "field.device_none_the_first", Suggest("midi.outputs")),
            ("name", "field.name", Text),
            ("sendNoteOff", "field.send_note_off", Bool),
            ("channel", "field.channel", Int),
            ("note", "field.note", Int),
            ("velocity", "field.velocity", Int),
            ("duration", "automation.duration_duration", Duration(1)),
        ],
        9002 => &[
            ("device", "field.device_none_the_first", Suggest("midi.outputs")),
            ("name", "field.name", Text),
            ("channel", "field.channel", Int),
            ("controller", "field.controller", Int),
            ("value", "field.value", Int),
        ],
        9003 => &[
            ("device", "field.device_none_the_first", Suggest("midi.outputs")),
            ("name", "field.name", Text),
            ("command", "field.command", Text),
            ("channel", "field.channel", Int),
            ("data1", "field.data_1", Int),
            ("data2", "field.data_2", Int),
        ],
        11001 => &[
            ("buttonId", "field.button_id", Text),
            ("title", "field.title", Text),
            ("state", "field.state", Pick(STATE)),
        ],
        11002 => &[
            ("buttonId", "field.button_id", Text),
            ("image", "field.image", Text),
            ("color", "field.color", Text),
            ("state", "field.state", Pick(STATE)),
        ],
        11004 => &[
            ("buttonId", "field.button_id", Text),
            ("state", "field.state", Pick(STATE)),
        ],
        11005 => &[("buttonId", "field.button_id", Text)],
        11006 => &[("buttonId", "field.button_id", Text)],
        11007 => &[("buttonId", "field.button_id", Text)],
        11008 => &[("buttonId", "field.button_id", Text), ("value", "field.value", Text)],
        21001 => &[
            ("deckId", "field.deck_id", Text),
            ("deckName", "field.deck_name", Text),
            ("itemId", "field.item_id", Text),
            ("itemName", "field.item_name", Text),
            ("state", "field.state", Pick(STATE)),
        ],
        21002 => &[
            ("deckId", "field.deck_id", Text),
            ("deckName", "field.deck_name", Text),
            ("itemId", "field.item_id", Text),
            ("itemName", "field.item_name", Text),
        ],
        21003 => &[
            ("deckId", "field.deck_id", Text),
            ("deckName", "field.deck_name", Text),
            ("itemId", "field.item_id", Text),
            ("itemName", "field.item_name", Text),
            ("color", "field.color", Text),
            ("title", "field.title", Text),
            ("state", "field.state", Pick(STATE)),
        ],
        21004 => &[
            ("deckId", "field.deck_id", Text),
            ("deckName", "field.deck_name", Text),
            ("itemId", "field.item_id", Text),
            ("itemName", "field.item_name", Text),
            ("color", "field.color", Text),
            ("url", "field.url", Text),
            ("fileId", "field.file_id", Text),
            ("state", "field.state", Pick(STATE)),
        ],
        21005 => &[
            ("deckId", "field.deck_id", Text),
            ("deckName", "field.deck_name", Text),
            ("itemId", "field.item_id", Text),
            ("itemName", "field.item_name", Text),
            ("color", "field.color", Text),
            ("name", "field.name", Text),
            ("url", "field.url", Text),
            ("fileId", "field.file_id", Text),
            ("state", "field.state", Pick(STATE)),
        ],
        21006 => &[
            ("deckId", "field.deck_id", Text),
            ("deckName", "field.deck_name", Text),
            ("itemId", "field.item_id", Text),
            ("itemName", "field.item_name", Text),
            ("value", "field.value", Text),
            ("state", "field.state", Pick(STATE)),
        ],
        21007 => &[
            ("deckId", "field.deck_id", Text),
            ("deckName", "field.deck_name", Text),
            ("title", "field.title", Text),
            ("description", "field.description", Multi),
            ("color", "field.color", Text),
            ("soundId", "field.sound_id", Text),
        ],
        21008 => &[
            ("deckId", "field.deck_id", Text),
            ("deckName", "field.deck_name", Text),
            ("page", "field.page", Int),
        ],
        21009 => &[("deckId", "field.deck_id", Text), ("deckName", "field.deck_name", Text)],
        21010 => &[("deckId", "field.deck_id", Text), ("deckName", "field.deck_name", Text)],
        1 => &[
            ("soundFile", "field.sound_file", Text),
            ("device", "field.audio_device", Suggest("sound.devices")),
            ("volume", "field.volume_0_to_100", Int),
            ("finishBeforeContinuing", "field.finish_playing_before_continuing", Bool),
            ("soundName", "field.sound_name", Text),
            ("useFileNameAsName", "field.use_the_file_name_as_the_name", Bool),
        ],
        7 => &[
            ("folder", "field.folder", Text),
            ("recursive", "field.include_its_folders", Bool),
            ("avoidRepeats", "field.not_the_same_sound_twice_in_a_row", Bool),
            ("device", "field.audio_device", Suggest("sound.devices")),
            ("volume", "field.volume_0_to_100", Int),
            ("finishBeforeContinuing", "field.finish_playing_before_continuing", Bool),
            ("soundName", "field.sound_name", Text),
            ("useFileNameAsName", "field.use_the_file_name_as_the_name", Bool),
        ],
        30001 => &[],
        30002 => &[("soundName", "field.sound_name", Text)],
        99300 => &[
            ("soundName", "field.sound_name_empty_every_sound", Text),
            ("volume", "field.volume_0_to_100", Int),
            ("fadeMs", "field.fade", Duration(1)),
        ],
        99301 | 99302 => &[("soundName", "field.sound_name_empty_every_sound", Text)],
        99303 => &[
            ("volume", "field.volume_0_to_100", Int),
            ("fadeMs", "field.fade", Duration(1)),
        ],
        99305 => &[
            ("overlay", "overlay.control_overlay", Overlay),
            ("soundFile", "field.sound_overlay_file", Text),
            ("volume", "field.volume_0_to_100", Int),
            ("soundName", "field.sound_name", Text),
        ],
        99306 => &[("device", "field.audio_device", Suggest("sound.devices"))],
        99320 => &[
            ("text", "field.text", Multi),
            ("voice", "field.voice_default", Suggest("voices")),
            ("volume", "field.volume_0_to_100", Int),
            ("speed", "field.speech_speed", Num),
            ("pitch", "field.speech_pitch", Num),
            ("device", "field.audio_device", Suggest("sound.devices")),
            ("overlay", "field.speech_overlay", Overlay),
            ("finishBeforeContinuing", "field.finish_playing_before_continuing", Bool),
            ("prefix", "field.speech_prefix", Text),
            ("skipLinks", "field.speech_skip_links", Bool),
            ("skipEmotes", "field.speech_skip_emotes", Bool),
            ("skipCommands", "field.speech_skip_commands", Bool),
            ("maxRepeat", "field.speech_max_repeat", Int),
            ("maxLength", "field.speech_max_length", Int),
            ("blockedWords", "field.speech_blocked_words", Lines),
            ("skipUsers", "field.speech_skip_users", Lines),
        ],
        99321 => &[
            ("text", "field.text", Multi),
            ("voice", "field.voice_default", Suggest("voices")),
            ("speed", "field.speech_speed", Num),
            ("pitch", "field.speech_pitch", Num),
            ("file", "field.speech_file", Text),
        ],
        99322 | 99323 | 99327 => &[],
        99324 => &[(
            "state",
            "field.state",
            Pick(&["field.resume", "field.pause", "field.toggle"]),
        )],
        99325 => &[(
            "state",
            "field.state",
            Pick(&["field.disable", "field.enable", "field.toggle"]),
        )],
        99326 => &[
            ("source", "field.speech_who", Pick(&["field.user", "field.target_user"])),
            ("voice", "field.voice_default", Suggest("voices")),
        ],
        99304 => &[("soundFile", "field.sound_file", Text)],
        3 => &[
            ("file", "field.file", Text),
            ("text", "field.text", Multi),
            ("append", "field.append", Bool),
        ],
        5 => &[
            ("file", "field.file", Text),
            ("parseVariables", "field.parse_variables", Bool),
            ("autoType", "field.auto_type_2", Bool),
            ("variableName", "field.variable_name_2", Text),
        ],
        1006 => &[
            ("file", "field.file", Text),
            ("parseVariables", "field.parse_variables", Bool),
            ("autoType", "field.auto_type_2", Bool),
            ("variableName", "field.variable_name_2", Text),
        ],
        1021 => &[
            ("file", "field.file", Text),
            ("parseVariables", "field.parse_variables", Bool),
            ("autoType", "field.auto_type_2", Bool),
            ("count", "field.line_number", Int),
            ("variableName", "field.variable_name_2", Text),
        ],
        1030 => &[
            ("fileTailId", "field.file_tail", FileTail),
            ("state", "field.state", Pick(STATE)),
        ],
        1024 => &[
            ("toastId", "field.toast_id", Text),
            ("title", "field.title", Text),
            ("text", "field.text", Multi),
            ("attribution", "field.attribution_empty_streamerz", Text),
            ("iconPath", "field.icon_file_empty_streamerz_s", Text),
        ],
        1039 => &[
            ("file", "field.file", Text),
            ("variableName", "field.variable_name_2", Text),
        ],
        1040 => &[
            ("folder", "field.folder", Text),
            ("variableName", "field.variable_name_2", Text),
        ],
        99700 => &[
            ("person", "field.linked_person_name_id_or_handle_empty_current", Text),
            ("twitch", "Twitch", Bool),
            ("kick", "Kick", Bool),
            ("youtube", "YouTube", Bool),
            ("reason", "field.reason", Text),
        ],
        99701 => &[
            ("person", "field.linked_person_name_id_or_handle_empty_current", Text),
            ("duration", "automation.duration_duration", Duration(1000)),
            ("twitch", "Twitch", Bool),
            ("kick", "Kick", Bool),
            ("youtube", "YouTube", Bool),
            ("reason", "field.reason", Text),
        ],
        99702 => &[
            ("person", "field.linked_person_name_id_or_handle_empty_current", Text),
            ("twitch", "Twitch", Bool),
            ("kick", "Kick", Bool),
            ("youtube", "YouTube", Bool),
        ],
        99703 => &[("person", "field.linked_person_name_id_or_handle_empty_current", Text)],
        99704 => &[
            ("name", "field.person_name", Text),
            ("twitch", "field.twitch_user_id", Text),
            ("kick", "field.kick_user_id", Text),
            ("youtube", "field.youtube_channel_id", Text),
            ("tiktok", "field.tiktok_username", Text),
        ],
        99705 => &[
            ("person", "field.linked_person", Text),
            (
                "platform",
                "field.platform",
                Pick(&["field.all", "Twitch", "Kick", "YouTube", "TikTok"]),
            ),
        ],
        99706 => &[
            (
                "platform",
                "field.platform",
                Pick(&["field.auto", "Twitch", "Kick", "YouTube"]),
            ),
            ("messageId", "field.message_id_empty_current", Text),
        ],
        99901 | 99902 | 99904 => &[("random", "field.random", Bool)],
        99903 => &[
            ("values", "field.values_one_per_line", Lines),
            ("caseSensitive", "field.case_sensitive", Bool),
            ("random", "field.random", Bool),
        ],
        _ => return None,
    })
}

/// A core trigger's fields; None when it has no form.
pub fn trigger_fields(kind: u64) -> Option<&'static [F]> {
    let code = u32::try_from(kind).ok()?;
    crate::types::name(crate::types::TRIGGERS, code)?;
    Some(match kind {
        401 | 402 => &[("commandId", "field.command", Command)],
        701 => &[("timerId", "field.timer", Timer)],
        99302 => &[("device", "field.sound_device_any", Suggest("sound.devices"))],
        99320 | 99321 => &[],
        702 | 706 | 99300 | 99301 | 99700..=99703 | 99804..=99809 | 99825..=99826 | 99830..=99842 => &[],
        99881 => &[
            ("overlay", "overlay.control_overlay_any", Overlay),
            ("control", "overlay.control_name_any", Text),
        ],
        709 | 711 | 712 => &[
            ("variableName", "field.variable_none_any", Text),
            ("persisted", "field.persisted", Pick(PERSISTED)),
        ],
        710 => &[
            ("variableName", "field.variable_none_any", Text),
            ("persisted", "field.persisted", Pick(PERSISTED)),
            ("userLogon", "field.user_login_none_any", Text),
            ("platform", "field.platform_none_any", Text),
        ],
        803..=805 => &[("clientId", "field.client", Client)],
        2001..=2003 => &[("serverId", "field.server", Server)],
        18001 => &[("eventName", "field.event_name", Text)],
        18002 => &[("triggerName", "field.trigger_name", Text)],
        32001..=32005 => &[("groupId", "field.group_none_any", UserGroupId)],
        14001 | 14002 | 14005 | 14006 | 14007 | 14008 => {
            &[("connectionId", "field.connection_empty_any", ObsConnection)]
        }
        22001 | 22002 | 22004..=22007 => &[("sdId", "field.connection_none_any", SlobsConnection)],
        22003 => &[
            ("sdId", "field.connection_none_any", SlobsConnection),
            ("sceneName", "field.scene_empty_any", Text),
        ],
        14003 => &[
            ("connectionId", "field.connection_empty_any", ObsConnection),
            ("eventType", "field.event_type_empty_any", Text),
        ],
        14004 => &[
            ("connectionId", "field.connection_empty_any", ObsConnection),
            ("sceneName", "field.scene_empty_any", Text),
        ],
        14009 => &[
            ("connectionId", "field.connection_empty_any", ObsConnection),
            ("vendorName", "field.vendor_empty_any", Text),
            ("eventType", "field.event_type_empty_any", Text),
        ],
        112..=115 | 171 => &[("rewardId", "field.reward_none_any", Entity("twitch.rewards"))],
        35024 => &[("rewardId", "field.reward_none_any", Entity("kick.rewards"))],
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
        41001..=41016 => &[],
        101..=196
        | 451..=498
        | 4001..=4031
        | 35001..=35003
        | 35008..=35031
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
            ("min", "field.min_amount_0_none", Num),
            ("max", "field.max_amount_0_none", Num),
        ],
        29001 | 29002 => &[("instanceId", "field.instance", Text)],
        29003 => &[
            ("instanceId", "field.instance", Text),
            ("eventName", "field.event_name_2", Text),
        ],
        29004 => &[
            ("instanceId", "field.instance", Text),
            ("signalName", "field.signal_name", Text),
        ],
        15001 => &[
            ("device", "field.device_none_any", Suggest("midi.inputs")),
            ("commandCode", "field.command_none_any", Pick(crate::midi::COMMANDS)),
            ("channel", "field.channel_0_any", Int),
            ("data1", "field.note_or_controller_none_any", Text),
        ],
        16001 => &[("keyCode", "field.keys", Keys), ("", "", Access(false))],
        16002 => &[("button", "field.button", MouseButton), ("", "", Access(true))],
        705 => &[("actionName", "field.action_name", Text)],
        17001 => &[("buttonId", "field.button_id", Text)],
        17002 => &[("device", "field.device", Text)],
        17003 => &[("device", "field.device", Text)],
        17004 => &[("device", "field.device", Text)],
        34001 => &[("webhookName", "field.webhook_name_none_any", Text)],
        501..=504 => &[("watcherId", "field.watcher_none_any", FileWatcher)],
        511 => &[("tailId", "field.file_tail_none_any", FileTail)],
        703 | 704 => &[("processName", "field.process_name", Text)],
        _ if crate::docs::available("trigger", code) => &[],
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn available_form_keys_have_consumers_and_preserved_keys_stay_in_json() {
        // This catches orphaned keys, including when an engine reader is renamed or removed.
        // A matching literal is only a first check. Behavior tests cover individual readers.
        fn collect_rust(dir: &std::path::Path, out: &mut String) {
            for entry in std::fs::read_dir(dir).unwrap() {
                let path = entry.unwrap().path();
                let name = path.file_name().unwrap().to_str().unwrap();
                if path.is_dir() {
                    if name != "automation" {
                        collect_rust(&path, out);
                    }
                } else if path.extension().is_some_and(|ext| ext == "rs")
                    && !["types.rs", "edit.rs", "docs.rs", "mcp.rs"].contains(&name)
                {
                    let source = std::fs::read_to_string(path).unwrap();
                    let production = source
                        .split_once("\n#[cfg(test)]\nmod tests {")
                        .map_or(source.as_str(), |(production, _)| production);
                    out.push_str(production);
                }
            }
        }

        let mut source = String::new();
        collect_rust(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src").as_path(),
            &mut source,
        );
        let keys = source
            .split('"')
            .filter(|word| !word.is_empty() && word.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_'))
            .collect::<std::collections::HashSet<_>>();
        for (kind, table, schema) in [
            (
                "subaction",
                crate::types::SUB_ACTIONS,
                sub_fields as fn(u64) -> Option<&'static [F]>,
            ),
            ("trigger", crate::types::TRIGGERS, trigger_fields),
        ] {
            for &(code, name) in table {
                let code64 = u64::from(code);
                let fields = schema(code64).unwrap_or_default();
                for &(key, _, _) in fields {
                    if kind == "subaction" {
                        assert!(
                            !preserved_fields(code64).contains(&key),
                            "{name} exposes inert field {key}"
                        );
                    }
                    if !key.is_empty() && crate::docs::available(kind, code) {
                        assert!(keys.contains(key), "{kind} {code} {name} has no consumer for {key}");
                    }
                }
            }
        }
    }

    #[test]
    fn an_out_of_range_trigger_does_not_inherit_a_known_types_schema() {
        assert!(trigger_fields(101).is_some());
        assert!(trigger_fields(u64::from(u32::MAX) + 1 + 101).is_none());
    }

    /// Every field's label and every choice is a translation key, so the form reads in the user's
    /// language (names that stay as they are: platforms, HTTP methods, numbers).
    #[test]
    fn every_label_and_choice_is_translated() {
        let kept = |t: &str| {
            ["Twitch", "Kick", "YouTube", "TikTok", "GET", "POST", "PUT", "PATCH", "DELETE"].contains(&t)
                // MIDI's own message names.
                || ["NoteOn", "NoteOff", "ControlChange", "PatchChange", "PitchWheelChange"].contains(&t)
                || t.ends_with("AfterTouch")
                || t.chars().all(|c| c.is_ascii_digit())
        };
        let key = |t: &str| t.contains('.') && !t.contains(' ') && t.chars().all(|c| !c.is_uppercase());
        for code in 0..100_000u64 {
            for fields in [sub_fields(code), trigger_fields(code)].into_iter().flatten() {
                for (name, label, widget) in fields {
                    if name.is_empty() {
                        continue;
                    }
                    assert!(key(label) || kept(label), "{code} {name}: label {label:?} isn't a key");
                    assert!(
                        kept(label) || field_label(label) != *label,
                        "{code} {name}: {label} has no text"
                    );
                    if let Pick(options) = widget {
                        for option in *options {
                            assert!(key(option) || kept(option), "{code} {name}: choice {option:?}");
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn new_sub_and_trigger_fields_exist() {
        assert!(sub_fields(4013).is_some());
        assert!(sub_fields(4014).is_some());
        assert!(sub_fields(35032).is_some());
        assert!(sub_fields(35033).is_some());
        assert!(sub_fields(35034).unwrap().is_empty());
        assert!(sub_fields(35050).is_some());
        assert!(sub_fields(35051).unwrap().is_empty());
        assert!(sub_fields(43010).is_some());
        assert!(sub_fields(43011).is_some());
        assert!(sub_fields(43012).is_some());

        assert!(trigger_fields(35026).unwrap().is_empty());
        assert!(trigger_fields(35027).unwrap().is_empty());
        for code in [
            581, 582, 583, 584, 586, 587, 588, 589, 592, 593, 594, 4015, 4016, 4017, 4018, 35044, 99700, 99701, 99702,
            99703, 99704, 99705, 99706,
        ] {
            assert!(!sub_fields(code).unwrap().is_empty(), "sub_fields({code})");
        }
        for code in [585, 590, 591, 595, 596] {
            assert!(sub_fields(code).unwrap().is_empty(), "sub_fields({code})");
        }
        for code in [
            35028, 35029, 35030, 35031, 41013, 41014, 41015, 41016, 99700, 99701, 99702, 99703,
        ] {
            assert!(trigger_fields(code).unwrap().is_empty(), "trigger_fields({code})");
        }
    }
}
