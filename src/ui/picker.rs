//! Hierarchical grouped picker dialog with multi-level submenus, live search,
//! breadcrumb navigation, and real-time detail/description panel.

use super::{button, ROW};
use crate::types::{SUB_ACTION_INFO, TRIGGERS};
use fltk::app;
use fltk::enums::{Align, Color, Event, Font, Key, Shortcut};
use fltk::frame::Frame;
use fltk::group::Flex;
use fltk::input::Input;
use fltk::prelude::*;
use fltk::text::{TextBuffer, TextDisplay, WrapMode};
use fltk::tree::{Tree, TreeItem, TreeReason, TreeSelect};
use fltk::window::Window;
use std::cell::{Cell, RefCell};
use std::collections::BTreeMap;
use std::rc::Rc;

#[derive(Clone, Debug)]
pub struct PickItem {
    pub id: usize,
    pub title: String,
    pub category: String,
    pub description: String,
}

/// A dialog window: centred, modal, closing it cancels.
fn window(w: i32, h: i32, title: &str) -> (Window, Rc<Cell<Option<bool>>>) {
    let mut win = Window::default().with_size(w, h).with_label(title);
    win.set_xclass("streamerz");
    win.make_modal(true);
    let answer = Rc::new(Cell::new(None));
    win.set_callback({
        let a = answer.clone();
        move |_| a.set(Some(false))
    });
    (win, answer)
}

/// Split PascalCase into words.
pub fn split_pascal(s: &str) -> String {
    let mut out = String::new();
    let mut prev_is_lower = false;
    for c in s.chars() {
        if c.is_uppercase() && prev_is_lower {
            out.push(' ');
        }
        out.push(c);
        prev_is_lower = c.is_lowercase();
    }
    out
}

/// Clean human-readable title for a trigger.
pub fn trigger_clean_title(name: &str) -> String {
    let stripped = name
        .strip_prefix("ElgatoCameraHubWebcam")
        .or_else(|| name.strip_prefix("ElgatoCameraHubPrompter"))
        .or_else(|| name.strip_prefix("ElgatoCameraHub"))
        .or_else(|| name.strip_prefix("ElgatoWaveLink"))
        .or_else(|| name.strip_prefix("ElgatoStreamDeck"))
        .or_else(|| name.strip_prefix("StreamDeck"))
        .or_else(|| name.strip_prefix("MeldStudio"))
        .or_else(|| name.strip_prefix("VoiceMod"))
        .or_else(|| name.strip_prefix("SpeakerBot"))
        .or_else(|| name.strip_prefix("TwitchSharedChat"))
        .or_else(|| name.strip_prefix("Twitch"))
        .or_else(|| name.strip_prefix("YouTube"))
        .or_else(|| name.strip_prefix("Youtube"))
        .or_else(|| name.strip_prefix("Kick"))
        .or_else(|| name.strip_prefix("TikTok"))
        .or_else(|| name.strip_prefix("Obs"))
        .or_else(|| name.strip_prefix("Slobs"))
        .or_else(|| name.strip_prefix("StreamlabsDesktop"))
        .or_else(|| name.strip_prefix("Streamlabs"))
        .or_else(|| name.strip_prefix("StreamElements"))
        .or_else(|| name.strip_prefix("StreamerBotRemoteInstance"))
        .or_else(|| name.strip_prefix("RemoteInstance"))
        .or_else(|| name.strip_prefix("VTubeStudio"))
        .or_else(|| name.strip_prefix("CrowdControl"))
        .or_else(|| name.strip_prefix("Throwing"))
        .or_else(|| name.strip_prefix("Pulsoid"))
        .or_else(|| name.strip_prefix("HypeRate"))
        .or_else(|| name.strip_prefix("FileWatcher"))
        .or_else(|| name.strip_prefix("FileTail"))
        .or_else(|| name.strip_prefix("Process"))
        .or_else(|| name.strip_prefix("WebsocketClient"))
        .or_else(|| name.strip_prefix("WebsocketCustom"))
        .or_else(|| name.strip_prefix("System"))
        .unwrap_or(name);

    let clean = split_pascal(if stripped.is_empty() { name } else { stripped });
    if clean.is_empty() {
        split_pascal(name)
    } else {
        clean
    }
}

/// Categorize triggers into Category / Submenu / Group hierarchy.
pub fn trigger_categorize(name: &str) -> String {
    // Elgato
    if let Some(rest) = name
        .strip_prefix("ElgatoCameraHub")
        .or_else(|| name.strip_prefix("CameraHub"))
    {
        if rest.contains("Prompter") {
            if rest.contains("Bright")
                || rest.contains("Font")
                || rest.contains("Color")
                || rest.contains("Margin")
                || rest.contains("LineSpacing")
                || rest.contains("Opacity")
                || rest.contains("Background")
            {
                "Elgato/Camera Hub/Prompter - Display".into()
            } else {
                "Elgato/Camera Hub/Prompter - Script & Controls".into()
            }
        } else if rest.contains("Exposure")
            || rest.contains("Shutter")
            || rest.contains("ISO")
            || rest.contains("Focus")
        {
            "Elgato/Camera Hub/Webcam - Exposure & Focus".into()
        } else if rest.contains("Zoom")
            || rest.contains("White")
            || rest.contains("Bright")
            || rest.contains("Contrast")
            || rest.contains("Saturation")
            || rest.contains("Sharpness")
            || rest.contains("Noise")
            || rest.contains("AntiFlicker")
        {
            "Elgato/Camera Hub/Webcam - Image & Color".into()
        } else if rest.contains("Webcam")
            || rest.contains("Tilt")
            || rest.contains("Lens")
            || rest.contains("Pan")
            || rest.contains("Flash")
            || rest.contains("Mirror")
            || rest.contains("Flip")
            || rest.contains("Orientation")
        {
            "Elgato/Camera Hub/Webcam - Device & Lens".into()
        } else {
            "Elgato/Camera Hub/Connection & Status".into()
        }
    } else if let Some(rest) = name
        .strip_prefix("ElgatoWaveLink")
        .or_else(|| name.strip_prefix("WaveLink"))
    {
        if rest.contains("Microphone") {
            "Elgato/Wave Link/Microphone".into()
        } else if rest.contains("Filter") {
            "Elgato/Wave Link/Filters".into()
        } else if rest.contains("Output") {
            "Elgato/Wave Link/Outputs".into()
        } else if rest.contains("Input") {
            "Elgato/Wave Link/Inputs".into()
        } else {
            "Elgato/Wave Link/Connection".into()
        }
    } else if name.starts_with("StreamDeck") || name.starts_with("ElgatoStreamDeck") {
        "Elgato/Stream Deck".into()
    }
    // Twitch
    else if let Some(rest) = name.strip_prefix("Twitch") {
        if rest.contains("Shared") {
            if rest.contains("Session") {
                "Twitch/Shared Chat/Sessions".into()
            } else if rest.contains("Sub") || rest.contains("PayItForward") || rest.contains("Upgrade") {
                "Twitch/Shared Chat/Subscriptions".into()
            } else if rest.contains("Banned") || rest.contains("Timedout") || rest.contains("Deleted") {
                "Twitch/Shared Chat/Moderation".into()
            } else {
                "Twitch/Shared Chat/Messages & Announcements".into()
            }
        } else if rest.contains("Cheer") || rest.contains("Bits") || rest.contains("Coin") || rest.contains("HypeChat")
        {
            "Twitch/Chat/Bits & Cheers".into()
        } else if rest.contains("Whisper") || rest.contains("Announcement") || rest.contains("FirstWord") {
            "Twitch/Chat/Special Messages".into()
        } else if rest.contains("ChatMessage")
            || rest.contains("Deleted")
            || rest.contains("Clear")
            || rest.contains("EmoteOnly")
            || rest.contains("FollowersOnly")
            || rest.contains("Slow")
            || rest.contains("SubOnly")
            || rest.contains("SubscriberOnly")
        {
            "Twitch/Chat/Messages & Modes".into()
        } else if rest.contains("SubGift")
            || rest.contains("Gift")
            || rest.contains("Community")
            || rest.contains("PayItForward")
            || rest.contains("PrimePaidUpgrade")
        {
            "Twitch/Subscriptions/Gift Subscriptions".into()
        } else if rest.contains("Sub") {
            "Twitch/Subscriptions/Channel Subscriptions".into()
        } else if rest.contains("Ban")
            || rest.contains("TimeOut")
            || rest.contains("TimedOut")
            || rest.contains("Untimed")
            || rest.contains("Unban")
        {
            "Twitch/Moderation/Bans & Timeouts".into()
        } else if rest.contains("AutoMod")
            || rest.contains("Shield")
            || rest.contains("Warned")
            || rest.contains("Suspicious")
        {
            "Twitch/Moderation/Automod & Safety".into()
        } else if rest.contains("Mod") || rest.contains("Vip") || rest.contains("Shoutout") {
            "Twitch/Moderation/Roles & Shoutouts".into()
        } else if rest.contains("Reward") || rest.contains("PowerUp") || rest.contains("Point") {
            "Twitch/Rewards & Points".into()
        } else if rest.contains("HypeTrain") || rest.contains("Raid") {
            "Twitch/Community/Hype Train & Raids".into()
        } else if rest.contains("Charity") || rest.contains("Goal") {
            "Twitch/Community/Charity & Goals".into()
        } else if rest.contains("Ad") || rest.contains("Pyramid") || rest.contains("Streak") {
            "Twitch/Community/Ads & Milestones".into()
        } else if rest.contains("Poll") || rest.contains("Prediction") {
            "Twitch/Polls & Predictions".into()
        } else if rest.contains("GuestStar") {
            "Twitch/Guest Star".into()
        } else if rest.contains("Emote") {
            "Twitch/Channel/Emotes & Badges".into()
        } else {
            "Twitch/Channel/Follow & Stream".into()
        }
    }
    // OBS Studio
    else if let Some(rest) = name.strip_prefix("Obs") {
        if rest.contains("Scene") {
            "OBS Studio/Scenes & Sources".into()
        } else if rest.contains("Streaming") || rest.contains("Recording") {
            "OBS Studio/Streaming & Recording".into()
        } else if rest.contains("Connected") || rest.contains("Disconnected") {
            "OBS Studio/Connection".into()
        } else {
            "OBS Studio/Events".into()
        }
    }
    // Streamlabs Desktop
    else if let Some(rest) = name
        .strip_prefix("Slobs")
        .or_else(|| name.strip_prefix("StreamlabsDesktop"))
    {
        if rest.contains("Scene") {
            "Streamlabs Desktop/Scenes & Sources".into()
        } else if rest.contains("Streaming") || rest.contains("Recording") {
            "Streamlabs Desktop/Streaming & Recording".into()
        } else {
            "Streamlabs Desktop/Connection".into()
        }
    }
    // Kick
    else if let Some(rest) = name.strip_prefix("Kick") {
        if rest.contains("Chat") || rest.contains("FirstWord") {
            "Kick/Chat".into()
        } else if rest.contains("Sub") {
            "Kick/Subscriptions".into()
        } else if rest.contains("Ban") || rest.contains("TimeOut") {
            "Kick/Moderation".into()
        } else if rest.contains("Reward") {
            "Kick/Rewards".into()
        } else if rest.contains("Emote") {
            "Kick/Emotes".into()
        } else {
            "Kick/Channel & Stream".into()
        }
    }
    // TikTok
    else if let Some(rest) = name.strip_prefix("TikTok") {
        if rest.contains("Chat") {
            "TikTok/Chat".into()
        } else if rest.contains("Gift") {
            "TikTok/Gifts & Coins".into()
        } else if rest.contains("Connected") || rest.contains("Disconnected") {
            "TikTok/Connection".into()
        } else if rest.contains("Stream") || rest.contains("Viewer") {
            "TikTok/Stream".into()
        } else {
            "TikTok/Community".into()
        }
    }
    // YouTube
    else if let Some(rest) = name.strip_prefix("YouTube").or_else(|| name.strip_prefix("Youtube")) {
        if rest.contains("Message") || rest.contains("Super") || rest.contains("Jewel") {
            "YouTube/Chat & Super Chats".into()
        } else if rest.contains("Broadcast") {
            "YouTube/Broadcast".into()
        } else if rest.contains("Member") || rest.contains("Sponsor") {
            "YouTube/Memberships".into()
        } else if rest.contains("Ban") || rest.contains("TimeOut") {
            "YouTube/Moderation".into()
        } else if rest.contains("Emote") || rest.contains("Poll") {
            "YouTube/Community".into()
        } else {
            "YouTube/Channel & Statistics".into()
        }
    }
    // File & Process
    else if name.starts_with("FileWatcher") {
        "File & Process/File Watcher".into()
    } else if name.starts_with("FileTail") {
        "File & Process/File Tail".into()
    } else if name.starts_with("Process") {
        "File & Process/Process Monitor".into()
    }
    // Inputs & MIDI
    else if name.starts_with("Midi") {
        "Inputs & MIDI/MIDI Controller".into()
    } else if name.starts_with("SpeechToText") {
        "Inputs & MIDI/Voice Control".into()
    } else if name.starts_with("Key") || name.starts_with("Mouse") || name.starts_with("Input") {
        "Inputs & MIDI/Hotkeys & Mouse".into()
    }
    // Integrations
    else if name.starts_with("VoiceMod") {
        "Integrations/VoiceMod".into()
    } else if name.starts_with("SpeakerBot") {
        "Integrations/Speaker.bot".into()
    } else if let Some(rest) = name.strip_prefix("MeldStudio") {
        if rest.contains("Stream") || rest.contains("Record") {
            "Integrations/Meld Studio/Streaming & Recording".into()
        } else {
            "Integrations/Meld Studio/Scenes & Audio".into()
        }
    } else if name.starts_with("Pulsoid") || name.starts_with("HypeRate") {
        "Integrations/Heart Rate Monitors".into()
    } else if name.starts_with("VTubeStudio") {
        "Integrations/VTube Studio".into()
    } else if name.starts_with("CrowdControl") {
        "Integrations/Crowd Control".into()
    } else if name.starts_with("Throwing") {
        "Integrations/Throwing System".into()
    }
    // Donations & Shops
    else if name.starts_with("Kofi") || name.starts_with("KoFi") {
        "Donations & Shops/Ko-fi".into()
    } else if name.starts_with("StreamElements") {
        "Donations & Shops/StreamElements".into()
    } else if name.starts_with("Streamlabs") {
        "Donations & Shops/Streamlabs".into()
    } else if name.starts_with("Patreon") {
        "Donations & Shops/Patreon".into()
    } else if name.starts_with("Fourthwall") {
        "Donations & Shops/Fourthwall".into()
    } else if name.starts_with("Throne") {
        "Donations & Shops/Throne".into()
    } else if name.starts_with("Shopify") {
        "Donations & Shops/Shopify".into()
    } else if name.starts_with("StreamLoots") || name.starts_with("Streamloots") {
        "Donations & Shops/Streamloots".into()
    } else if name.starts_with("Tipeee")
        || name.starts_with("Treat")
        || name.starts_with("Donor")
        || name.starts_with("Pally")
    {
        "Donations & Shops/Other Services".into()
    }
    // Servers & Remote
    else if name.starts_with("WebsocketClient") {
        "Servers & Remote/WebSocket Client".into()
    } else if name.starts_with("WebsocketCustom") {
        "Servers & Remote/Custom WebSocket Server".into()
    } else if name.starts_with("RemoteInstance") || name.starts_with("StreamerBotRemoteInstance") {
        "Servers & Remote/Remote Instances".into()
    } else if name.contains("Webook") || name.contains("Webhook") {
        "Servers & Remote/Custom Webhooks".into()
    }
    // Core & System
    else if name.starts_with("ChatWindow") {
        "Core/Chat Window".into()
    } else if name.starts_with("Command") {
        "Core/Commands".into()
    } else if name.starts_with("Timed") || name.contains("Timer") {
        "Core/Timers".into()
    } else if name.starts_with("Global") || name.starts_with("UserGlobal") {
        "Core/Globals".into()
    } else if name.starts_with("Group") {
        "Core/User Groups".into()
    } else if name.starts_with("Quote") {
        "Core/Quotes".into()
    } else if name.contains("Custom")
        || name.starts_with("RawAction")
        || name.starts_with("RawSubAction")
        || name == "Test"
    {
        "Core/Custom & Scripting".into()
    } else if name.contains("Clipboard") {
        "Core/System & Clipboard".into()
    } else if name.starts_with("Application") || name.starts_with("Streamerbot") || name.contains("Toast") {
        "Core/Application Lifecycle".into()
    } else {
        "Core/Other".into()
    }
}

/// Helpful description of what a trigger does and when it fires.
pub fn trigger_description(code: u32, _raw_name: &str, title: &str, category: &str) -> String {
    match code {
        1 => "Fires when triggered programmatically via ExecuteCodeTrigger or external API calls.".into(),
        101 => "Fires when a new viewer follows the channel on Twitch.".into(),
        102 => "Fires when Bits are cheered in Twitch chat.".into(),
        103 => "Fires when a user purchases a new channel subscription on Twitch.".into(),
        104 => "Fires when a user renews their subscription (resubscribes) on Twitch.".into(),
        105 => "Fires when a subscription is gifted to another viewer on Twitch.".into(),
        106 => "Fires when multiple gift subscriptions (a gift bomb) are given at once on Twitch.".into(),
        107 => "Fires when the channel receives an incoming Twitch raid.".into(),
        108 => "Fires when a Twitch Hype Train begins.".into(),
        109 => "Fires when a Twitch Hype Train progresses with new contributions.".into(),
        110 => "Fires when a Twitch Hype Train reaches a new level.".into(),
        111 => "Fires when a Twitch Hype Train concludes.".into(),
        112 => "Fires when a viewer redeems a custom Channel Points reward on Twitch.".into(),
        118 => "Fires when channel title, game category, or stream details are updated on Twitch.".into(),
        119 => "Fires when a private whisper message is received on Twitch.".into(),
        120 => "Fires the very first time a user chats in the current stream session.".into(),
        125 => "Fires when a new Twitch poll is created.".into(),
        127 => "Fires when a Twitch poll completes and final results are calculated.".into(),
        128 => "Fires when a new Twitch prediction is created.".into(),
        130 => "Fires when a Twitch prediction completes and payouts are determined.".into(),
        133 => "Fires on every standard chat message posted in Twitch chat.".into(),
        136 => "Fires when a user is timed out in Twitch chat.".into(),
        137 => "Fires when a user is permanently banned from Twitch chat.".into(),
        138 => "Fires when a chat announcement is sent in Twitch chat.".into(),
        139 => "Fires when an ad break runs on Twitch.".into(),
        144 => "Fires when a shoutout is created for another streamer on Twitch.".into(),
        154 => "Fires when the Twitch channel broadcast goes live.".into(),
        155 => "Fires when the Twitch channel broadcast ends.".into(),
        164 => "Fires periodically when Twitch live viewer count is updated.".into(),
        170 => "Fires when a viewer sends a highlighted Hype Chat message.".into(),
        401 => "Fires when a configured chat command is triggered by a user.".into(),
        402 => "Fires when a command execution is prevented due to an active cooldown.".into(),
        501 => "Fires when a watched file on disk is modified.".into(),
        502 => "Fires when a new file is created in a watched directory folder.".into(),
        503 => "Fires when an existing file is deleted from a watched directory folder.".into(),
        504 => "Fires when a file in a watched directory folder is renamed.".into(),
        511 => "Fires when new lines are appended to a monitored tail log file.".into(),
        601 => "Fires when a new quote is added to the channel quote database.".into(),
        602 => "Fires when an existing quote is retrieved or displayed.".into(),
        701 => "Fires periodically based on an interval configured in the Timers tab.".into(),
        703 => "Fires when an external application or process starts running.".into(),
        704 => "Fires when an external application or process terminates.".into(),
        705 => "Fires on multi-platform chat window actions.".into(),
        706 => "Fires when Streamer.bot starts up and completes initialization.".into(),
        707 => "Fires when Streamer.bot is shutting down.".into(),
        5001 => "Fires when a new heart rate pulse measurement is received from Pulsoid.".into(),
        6001 => "Fires when a new heart rate pulse measurement is received from HypeRate.".into(),
        14001 => "Fires when obs-websocket connection to OBS Studio is established.".into(),
        14002 => "Fires when obs-websocket connection to OBS Studio is disconnected.".into(),
        14004 => "Fires when the active program scene in OBS Studio changes.".into(),
        14005 => "Fires when OBS Studio begins live streaming.".into(),
        14006 => "Fires when OBS Studio stops live streaming.".into(),
        14007 => "Fires when OBS Studio begins video recording.".into(),
        14008 => "Fires when OBS Studio stops video recording.".into(),
        15001 => "Fires when an incoming MIDI message (note or CC) is received from a device.".into(),
        16001 => "Fires when its key combination is pressed, whichever program has focus. Set it by \
            pressing the keys; on Linux the desktop asks you to allow it (Hyprland: a line for hyprland.conf)."
            .into(),
        16002 => "Fires on a mouse click anywhere (mouse.button: 1 left, 2 right, 3 middle). Windows only: \
            on Linux only reading every input device could see clicks, which streamerz doesn't do."
            .into(),
        17001 => "Fires when a Stream Deck key or dial action is triggered.".into(),
        21001..=21020 => format!("Fires on Elgato Wave Link audio event: {title}."),
        21301..=21390 => format!("Fires on Elgato Camera Hub webcam event: {title}."),
        21401..=21426 => format!("Fires on Elgato Camera Hub prompter event: {title}."),
        21501..=21502 => "Fires when Stream Deck MCP server connects or disconnects.".into(),
        23001..=23002 => "Fires on Speaker.bot text-to-speech connection status changes.".into(),
        29001..=29004 => format!("Fires on Streamer.bot remote instance event: {title}."),
        30001..=30004 => format!("Fires on VoiceMod voice changer event: {title}."),
        33001..=33012 => format!("Fires on Meld Studio streaming and broadcast event: {title}."),
        35010 => "Fires on every chat message in the Kick chat.".into(),
        35011 => "Fires when someone follows the channel on Kick.".into(),
        35014 => "Fires when someone subscribes on Kick.".into(),
        4003 => "Fires on every message in the YouTube live chat.".into(),
        4006 => "Fires when a Super Chat comes in on YouTube.".into(),
        4007 => "Fires when a Super Sticker comes in on YouTube.".into(),
        41001 => "Fires on every standard chat message posted in TikTok Live chat.".into(),
        41002 => "Fires when a gift (or gift combo streak) is received on TikTok Live.".into(),
        41003 => "Fires when likes / screen taps are received on TikTok Live.".into(),
        41004 => "Fires when a viewer follows the streamer on TikTok.".into(),
        41005 => "Fires when a viewer shares the TikTok live stream.".into(),
        41006 => "Fires when a viewer subscribes to the TikTok channel.".into(),
        41007 => "Fires when a viewer joins the TikTok live stream.".into(),
        41008 => "Fires when connected to the TikTok channel's live.".into(),
        41009 => "Fires when the TikTok live ends or the connection drops.".into(),
        41010 => "Fires when the TikTok live's viewer count changes (with the top viewers by coins).".into(),
        41011 => "Fires when a new TikTok live is found (the first connection to its room).".into(),
        41012 => "Fires when the TikTok live ends, with its viewers, likes and coins.".into(),
        42001 => "Fires when the system clipboard content changes.".into(),
        _ => format!("Fires on the {title} event in {}.", category.replace('/', " > ")),
    }
}

/// Description of what a sub-action does.
pub fn sub_action_description(code: u32, name: &str, category: &str) -> String {
    let said = match code {
        1 => "Plays a sound file.",
        3 => "Writes or appends text to a file.",
        5 => "Puts a random line of a text file in a variable.",
        6 => "Runs a program with arguments.",
        7 => "Plays a random sound from a folder.",
        8 => "Turns a timer on, off, or toggles it.",
        10 | 35001 => "Sends a message to the chat, as the bot or the broadcaster.",
        43001 => "Sends a message to the TikTok live chat.",
        15 => "Changes the stream title.",
        18 => "Makes a clip of the stream.",
        20 => "Times a user out for some seconds.",
        120 => "Runs one branch or the other depending on a comparison.",
        122 => "Sets a global variable.",
        127 => "Runs the case that matches a value.",
        99903 => "One case of a Switch.",
        1002 => "Waits some milliseconds before the next sub-action.",
        1004 => "Turns an action on, off, or toggles it.",
        43002 => "Sets topGifterCount and topGifter0.. (name, UserId, Coins): who sent the most coins this live.",
        43003 => "Sets isLive, roomId, viewerCount, totalLikes and totalCoins for the TikTok live.",
        _ => "",
    };
    let at = category.replace('/', " > ");
    let fields = SUB_ACTION_INFO
        .iter()
        .find(|x| x.0 == code)
        .map(|x| {
            x.3.iter()
                .filter(|f| **f != "?")
                .copied()
                .collect::<Vec<_>>()
                .join(", ")
        })
        .unwrap_or_default();
    let mut out = if said.is_empty() {
        format!("{name}, in {at}.")
    } else {
        said.to_string()
    };
    if !fields.is_empty() {
        out += &format!(" Settings: {fields}.");
    }
    out
}

/// Generate structured PickItem list for all triggers.
pub fn trigger_pick_items() -> Vec<PickItem> {
    TRIGGERS
        .iter()
        .map(|&(code, raw_name)| {
            let cat = trigger_categorize(raw_name);
            let title = trigger_clean_title(raw_name);
            let desc = trigger_description(code, raw_name, &title, &cat);
            PickItem {
                id: code as usize,
                title,
                category: cat,
                description: desc,
            }
        })
        .collect()
}

/// Generate structured PickItem list for all sub-actions.
pub fn sub_action_pick_items() -> Vec<PickItem> {
    SUB_ACTION_INFO
        .iter()
        .filter(|x| !(99901..=99904).contains(&x.0) || x.0 == 99903)
        .map(|&(code, name, category, _)| {
            let cat = if category.is_empty() {
                "Core/General".to_string()
            } else {
                category.to_string()
            };
            let desc = sub_action_description(code, name, &cat);
            PickItem {
                id: code as usize,
                title: name.to_string(),
                category: cat,
                description: desc,
            }
        })
        .collect()
}

#[derive(Default)]
struct CategoryNode<'a> {
    subgroups: BTreeMap<String, CategoryNode<'a>>,
    items: Vec<&'a PickItem>,
}

fn insert_category_node<'a>(
    tree: &mut Tree,
    parent: &TreeItem,
    node: &CategoryNode<'a>,
    expanded: bool,
    node_map: &Rc<RefCell<Vec<(TreeItem, usize)>>>,
    first_leaf: &mut Option<(TreeItem, usize)>,
) {
    for (name, child) in &node.subgroups {
        if let Some(mut item) = tree.insert(parent, name, parent.children()) {
            item.set_label_font(Font::HelveticaBold);
            if expanded {
                item.open();
            } else {
                item.close();
            }
            insert_category_node(tree, &item, child, expanded, node_map, first_leaf);
        }
    }
    let mut sorted = node.items.clone();
    sorted.sort_by_key(|a| a.title.to_lowercase());
    for it in sorted {
        if let Some(item_node) = tree.insert(parent, &it.title, parent.children()) {
            if first_leaf.is_none() {
                *first_leaf = Some((item_node.clone(), it.id));
            }
            node_map.borrow_mut().push((item_node, it.id));
        }
    }
}

/// Hierarchical tree picker dialog with multi-level submenus, collapsed by default,
/// live search with auto-expanding paths, and detailed right-pane description.
pub fn pick_tree(win_title: &str, items: &[PickItem]) -> Option<usize> {
    let (mut win, answer) = window(840, 580, win_title);
    let mut root_col = Flex::new(12, 10, 816, 560, None).column();
    root_col.set_pad(6);

    // Filter row with Search input and Collapse/Expand all controls
    let mut filter_row = Flex::default().row();
    filter_row.set_pad(4);
    let f_label = Frame::default()
        .with_label("Search:")
        .with_align(Align::Left | Align::Inside);
    filter_row.fixed(&f_label, 50);
    let mut filter = Input::default();
    filter.set_tooltip("Filter triggers by title, category, or description...");
    super::placeholder(&mut filter, "Search…");
    let mut btn_collapse = button(&mut filter_row, "Collapse All", 95);
    let mut btn_expand = button(&mut filter_row, "Expand All", 85);
    filter_row.fixed(&btn_collapse, 95);
    filter_row.fixed(&btn_expand, 85);
    filter_row.end();
    root_col.fixed(&filter_row, ROW);

    // Main split view: Submenu Tree (left) + Detail card (right)
    let mut split = Flex::default().row();
    split.set_pad(8);

    let mut tree = Tree::default();
    tree.set_show_root(false);
    tree.set_select_mode(TreeSelect::Single);

    let mut detail_col = Flex::default().column();
    detail_col.set_pad(4);
    let mut d_title = Frame::default()
        .with_label("Select an option")
        .with_align(Align::Left | Align::Inside);
    d_title.set_label_font(Font::HelveticaBold);
    d_title.set_label_size(14);
    detail_col.fixed(&d_title, 24);

    let mut d_cat = Frame::default()
        .with_label("Browse submenus on the left")
        .with_align(Align::Left | Align::Inside);
    d_cat.set_label_font(Font::HelveticaItalic);
    d_cat.set_label_color(Color::from_rgb(100, 160, 240));
    detail_col.fixed(&d_cat, 18);

    let divider = Frame::default();
    detail_col.fixed(&divider, 4);

    let mut desc_buf = TextBuffer::default();
    desc_buf.set_text(
        "Browse the submenus on the left or type in the Search box above to quickly find any trigger or sub-action.",
    );
    let mut desc_disp = TextDisplay::default();
    desc_disp.set_buffer(desc_buf.clone());
    desc_disp.wrap_mode(WrapMode::AtBounds, 0);
    desc_disp.set_text_font(Font::Helvetica);
    desc_disp.set_text_size(12);

    detail_col.end();
    split.fixed(&detail_col, 330);
    split.end();

    // Bottom action bar
    let mut bot_row = Flex::default().row();
    bot_row.set_pad(6);
    let mut count_label = Frame::default().with_align(Align::Left | Align::Inside);
    count_label.set_label_color(Color::from_rgb(140, 140, 140));
    Frame::default();

    let mut ok_btn = button(&mut bot_row, "OK", 90);
    ok_btn.set_shortcut(Shortcut::from_key(Key::Enter));
    let mut cancel_btn = button(&mut bot_row, "Cancel", 90);
    cancel_btn.set_shortcut(Shortcut::from_key(Key::Escape));
    bot_row.end();
    root_col.fixed(&bot_row, ROW + 4);

    root_col.end();
    win.end();
    win.resizable(&root_col);

    let selected_id: Rc<Cell<Option<usize>>> = Rc::new(Cell::new(None));
    let node_map: Rc<RefCell<Vec<(TreeItem, usize)>>> = Rc::default();

    // Rebuild tree contents based on query and expand flag
    let items_vec = items.to_vec();
    let mut populate = {
        let mut tree = tree.clone();
        let items = items_vec.clone();
        let node_map = node_map.clone();
        let selected_id = selected_id.clone();
        let mut count_label = count_label.clone();
        let mut d_title = d_title.clone();
        let mut d_cat = d_cat.clone();
        let mut desc_buf = desc_buf.clone();
        move |q: &str, force_expand: Option<bool>| {
            let q = q.trim().to_lowercase();
            let Some(root) = tree.root() else { return };
            tree.clear_children(&root);
            node_map.borrow_mut().clear();

            let mut matched = 0;
            let mut root_node = CategoryNode::default();

            for it in &items {
                let m = q.is_empty()
                    || it.title.to_lowercase().contains(&q)
                    || it.category.to_lowercase().contains(&q)
                    || it.description.to_lowercase().contains(&q);
                if !m {
                    continue;
                }
                matched += 1;
                let parts: Vec<&str> = it
                    .category
                    .split('/')
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                    .collect();
                let mut cur = &mut root_node;
                for part in parts {
                    cur = cur.subgroups.entry(part.to_string()).or_default();
                }
                cur.items.push(it);
            }

            count_label.set_label(&format!("Showing {} of {} items", matched, items.len()));

            let mut first_leaf: Option<(TreeItem, usize)> = None;
            let expand_nodes = force_expand.unwrap_or(!q.is_empty());
            insert_category_node(&mut tree, &root, &root_node, expand_nodes, &node_map, &mut first_leaf);

            if let Some((first_item, id)) = first_leaf {
                if !q.is_empty() {
                    let _ = tree.select_only(&first_item, false);
                    selected_id.set(Some(id));
                    if let Some(it) = items.iter().find(|x| x.id == id) {
                        d_title.set_label(&it.title);
                        d_cat.set_label(&it.category.replace('/', " > "));
                        desc_buf.set_text(&it.description);
                    }
                }
            } else if matched == 0 {
                d_title.set_label("No matches found");
                d_cat.set_label("");
                desc_buf.set_text("Try a different search query.");
                selected_id.set(None);
            }
            tree.redraw();
        }
    };

    // Initially populate with EVERYTHING COLLAPSED
    populate("", Some(false));

    // Wire filter callback
    filter.set_trigger(fltk::enums::CallbackTrigger::Changed);
    filter.set_callback({
        let mut populate = populate.clone();
        move |inp| populate(&inp.value(), None)
    });

    // Wire Collapse All / Expand All buttons
    btn_collapse.set_callback({
        let mut populate = populate.clone();
        let filter = filter.clone();
        move |_| populate(&filter.value(), Some(false))
    });
    btn_expand.set_callback({
        let mut populate = populate.clone();
        let filter = filter.clone();
        move |_| populate(&filter.value(), Some(true))
    });

    // Wire selection callback
    tree.set_callback({
        let node_map = node_map.clone();
        let selected_id = selected_id.clone();
        let items = items_vec.clone();
        let mut d_title = d_title.clone();
        let mut d_cat = d_cat.clone();
        let mut desc_buf = desc_buf.clone();
        move |t| {
            let Some(item) = t.callback_item() else { return };
            if t.callback_reason() != TreeReason::Selected {
                return;
            }
            if let Some((_, id)) = node_map.borrow().iter().find(|(node, _)| node == &item) {
                selected_id.set(Some(*id));
                if let Some(it) = items.iter().find(|x| x.id == *id) {
                    d_title.set_label(&it.title);
                    d_cat.set_label(&it.category.replace('/', " > "));
                    desc_buf.set_text(&it.description);
                }
            } else if let Some(lbl) = item.label() {
                // Clicked a submenu folder node
                selected_id.set(None);
                d_title.set_label(&lbl);
                d_cat.set_label("Submenu");
                desc_buf.set_text("Click to expand or collapse this submenu to view and select options.");
            }
        }
    });

    // Double-click on leaf item confirms; Enter confirms
    tree.handle({
        let answer = answer.clone();
        let selected_id = selected_id.clone();
        let node_map = node_map.clone();
        move |t, ev| match ev {
            Event::Push => {
                if app::event_clicks() {
                    if let Some(item) = t.first_selected_item() {
                        if node_map.borrow().iter().any(|(node, _)| node == &item) && selected_id.get().is_some() {
                            answer.set(Some(true));
                            return true;
                        }
                    }
                }
                false
            }
            Event::KeyDown => {
                let key = app::event_key();
                if key == Key::Enter && selected_id.get().is_some() {
                    answer.set(Some(true));
                    return true;
                }
                false
            }
            _ => false,
        }
    });

    // OK / Cancel buttons
    ok_btn.set_callback({
        let answer = answer.clone();
        let selected_id = selected_id.clone();
        move |_| {
            if selected_id.get().is_some() {
                answer.set(Some(true));
            } else {
                super::dialogs::alert("Please select an item from the tree.");
            }
        }
    });
    cancel_btn.set_callback({
        let answer = answer.clone();
        move |_| answer.set(Some(false))
    });

    let ok = super::dialogs::run(&mut win, &answer, || selected_id.get().is_some());
    if ok {
        selected_id.get()
    } else {
        None
    }
}
