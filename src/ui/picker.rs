//! Hierarchical grouped picker dialog with multi-level submenus, live search,
//! breadcrumb navigation, and real-time detail/description panel.

use super::fit::Ellipsis;
use super::picker_icons;
use super::{button, icons, ROW};
use crate::types::{SUB_ACTION_INFO, TRIGGERS};
use fltk::enums::FrameType;
use fltk::enums::{Align, Color, Event, Font, Key, Shortcut};
use fltk::frame::Frame;
use fltk::group::Flex;
use fltk::image::SharedImage;
use fltk::input::Input;
use fltk::prelude::*;
use fltk::text::{TextBuffer, TextDisplay, WrapMode};
use fltk::tree::{Tree, TreeConnectorStyle, TreeItem, TreeReason, TreeSelect};
use fltk::{app, draw};
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

use super::dialogs::window;

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
        .or_else(|| name.strip_prefix("Spotify"))
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
    else if let Some(rest) = name.strip_prefix("Spotify") {
        if rest.contains("Connected")
            || rest.contains("Disconnected")
            || rest.contains("Error")
            || rest == "LoginExpired"
        {
            "Integrations/Spotify/Connection".into()
        } else if rest == "SongLiked" || rest == "PlaylistChanged" {
            "Integrations/Spotify/Library".into()
        } else if rest.starts_with("Song") || rest.starts_with("Vote") {
            "Integrations/Spotify/Song Requests".into()
        } else {
            "Integrations/Spotify/Playback".into()
        }
    } else if name.starts_with("VoiceMod") {
        "Integrations/VoiceMod".into()
    } else if name.starts_with("SpeakerBot") {
        "Integrations/Speaker.bot".into()
    } else if let Some(rest) = name.strip_prefix("MeldStudio") {
        if rest.contains("Stream") || rest.contains("Record") {
            "Integrations/Meld Studio/Streaming & Recording".into()
        } else {
            "Integrations/Meld Studio/Scenes & Audio".into()
        }
    } else if name.starts_with("Pulsoid") {
        "Integrations/Pulsoid".into()
    } else if name.starts_with("HypeRate") {
        "Integrations/HypeRate".into()
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
        5001 => "Fires on each new heart rate reading from Pulsoid. Set heartRate for the BPM.".into(),
        6001 => "Fires on each new heart rate reading from HypeRate (about once a second while it broadcasts). \
Set heartRate for the BPM."
            .into(),
        6002 => "Fires when HypeRate creates a clip on your Twitch channel by itself (its own clip feature, \
not one you made). clipUrl holds the clip's address."
            .into(),
        14001 => "Fires when obs-websocket connection to OBS Studio is established.".into(),
        14002 => "Fires when obs-websocket connection to OBS Studio is disconnected.".into(),
        14004 => "Fires when the active program scene in OBS Studio changes.".into(),
        14005 => "Fires when OBS Studio begins live streaming.".into(),
        14006 => "Fires when OBS Studio stops live streaming.".into(),
        14007 => "Fires when OBS Studio begins video recording.".into(),
        14008 => "Fires when OBS Studio stops video recording.".into(),
        15001 => "Fires when an incoming MIDI message (note or CC) is received from a device.".into(),
        16001 => "Fires when its key combination is pressed, whichever program has focus. Set it by \
            pressing the keys; on Wayland the desktop may ask you to allow it."
            .into(),
        16002 => "Fires on a mouse click anywhere (mouse.button: 1 left, 2 right, 3 middle, 4 back, \
            5 forward). On Wayland, allow mouse access once in the trigger."
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
        _ => {
            // The service: "HypeRate" in "Integrations/HypeRate", "Twitch" in "Twitch/Channel/Emotes".
            let parts: Vec<&str> = category.split('/').collect();
            let service = match parts[0] {
                "Integrations" | "Donations & Shops" | "Servers & Remote" => parts.get(1).copied().unwrap_or(parts[0]),
                top => top,
            };
            let path = category.replace('/', " > ");
            match title {
                "Connected" => format!("Fires when Streamerz connects to {service}."),
                "Disconnected" => format!("Fires when the connection to {service} is closed or lost."),
                _ => format!("Fires when {service} reports \"{title}\" ({path})."),
            }
        }
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
        43001 => "Sends a message to the TikTok LIVE chat, as the account logged in to the browser with the Streamerz for TikTok extension.",
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
            let title = match code {
                99804 => crate::i18n::tr("spotify.trigger_connected").to_string(),
                99805 => crate::i18n::tr("spotify.trigger_disconnected").to_string(),
                99806 => crate::i18n::tr("spotify.trigger_error").to_string(),
                99807 => crate::i18n::tr("spotify.trigger_local_connected").to_string(),
                99808 => crate::i18n::tr("spotify.trigger_local_disconnected").to_string(),
                99809 => crate::i18n::tr("spotify.trigger_track_changed").to_string(),
                99825 => crate::i18n::tr("spotify.trigger_playback_started").to_string(),
                99826 => crate::i18n::tr("spotify.trigger_playback_paused").to_string(),
                99836 => crate::i18n::tr("spotify.trigger_volume_changed").to_string(),
                99837 => crate::i18n::tr("spotify.trigger_shuffle_changed").to_string(),
                99838 => crate::i18n::tr("spotify.trigger_repeat_changed").to_string(),
                99839 => crate::i18n::tr("spotify.trigger_device_changed").to_string(),
                99840 => crate::i18n::tr("spotify.trigger_login_expired").to_string(),
                99841 => crate::i18n::tr("spotify.trigger_song_liked").to_string(),
                99842 => crate::i18n::tr("spotify.trigger_playlist_changed").to_string(),
                _ => trigger_clean_title(raw_name),
            };
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

pub(super) fn spotify_player_title(code: u32) -> Option<&'static str> {
    Some(match code {
        99810 => crate::i18n::tr("spotify.player_current"),
        99811 => crate::i18n::tr("spotify.player_devices"),
        99812 => crate::i18n::tr("spotify.player_queue"),
        99813 => crate::i18n::tr("spotify.player_play"),
        99814 => crate::i18n::tr("spotify.player_pause"),
        99815 => crate::i18n::tr("spotify.player_next"),
        99816 => crate::i18n::tr("spotify.player_previous"),
        99817 => crate::i18n::tr("spotify.player_seek"),
        99818 => crate::i18n::tr("spotify.player_volume"),
        99819 => crate::i18n::tr("spotify.player_add"),
        99820 => crate::i18n::tr("spotify.player_transfer"),
        99821 => crate::i18n::tr("spotify.player_shuffle"),
        99822 => crate::i18n::tr("spotify.player_repeat"),
        99823 => crate::i18n::tr("spotify.player_state"),
        99824 => crate::i18n::tr("spotify.player_recent"),
        _ => return None,
    })
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
            let desc = if (99810..=99824).contains(&code) {
                crate::i18n::tr("spotify.player_about").to_string()
            } else if (99840..=99875).contains(&code) {
                crate::i18n::tr("spotify.api_about").to_string()
            } else if code == 99802 {
                crate::i18n::tr("spotify.action_about").to_string()
            } else if code == 99803 {
                crate::i18n::tr("countdown.action_about").to_string()
            } else if code == 99880 {
                crate::i18n::tr("overlay.action_about").to_string()
            } else {
                sub_action_description(code, name, &cat)
            };
            PickItem {
                id: code as usize,
                title: if let Some(title) = spotify_player_title(code) {
                    title.to_string()
                } else if code == 99802 {
                    crate::i18n::tr("spotify.action_name").to_string()
                } else if code == 99803 {
                    crate::i18n::tr("countdown.action_name").to_string()
                } else if code == 99880 {
                    crate::i18n::tr("overlay.action_name").to_string()
                } else {
                    name.to_string()
                },
                category: cat,
                description: desc,
            }
        })
        .collect()
}

/// How every tree looks: chevrons to open a group, no connecting lines, a soft rounded selection.
pub fn style_tree(tree: &mut Tree) {
    tree.set_connector_style(TreeConnectorStyle::None);
    // FLTK calls the icon of a closed item the "open" one: it is what opens it.
    tree.set_open_icon(icons::icon("chevron-right", super::muted(), 12));
    tree.set_close_icon(icons::icon("chevron-down", super::muted(), 12));
    tree.set_item_label_size(13);
    tree.set_line_spacing(5);
    tree.set_margin_left(6);
    tree.set_connector_width(18);
    tree.set_user_icon_margin_left(2);
    tree.set_label_margin_left(4);
    tree.set_select_frame(FrameType::RFlatBox);
    tree.set_selection_color(super::rows::selected(tree.color()));
}

/// Over FLTK's dotted focus box on a tree's focused item: the rounded ring the lists draw. Call it
/// last in the tree's draw.
pub fn focus_ring(t: &Tree) {
    if !t.has_focus() {
        return;
    }
    let Some(item) = t.get_item_focus() else { return };
    // Its place is only fresh when it was drawn: every group above it is open.
    let mut up = item.parent();
    while let Some(p) = up {
        if !p.is_open() {
            return;
        }
        up = p.parent();
    }
    // Where FLTK put the dots: the label box inset by one, one pixel short on the right and bottom.
    let (x, y, w, h) = (
        item.label_x() + 1,
        item.label_y() + 1,
        item.label_w() - 1,
        item.label_h() - 1,
    );
    if w <= 4 || h <= 4 {
        return;
    }
    draw::push_clip(t.x() + 1, t.y() + 1, t.w() - 2, t.h() - 2);
    draw::set_draw_color(t.color());
    draw::draw_rect(x, y, w, h);
    draw::draw_rect(x, y, w - 1, h - 1);
    draw::set_draw_color(Color::color_average(Color::Foreground, t.color(), 0.5));
    draw::draw_rounded_rect(x, y, w, h, 5);
    draw::pop_clip();
}

/// The size of every icon in the tree.
const ICON: i32 = 16;

/// The icon of a node: the logo of a platform or app by its name, else the Lucide icon `lucide`. With
/// `gap`, room after it to sit before a label. Kept for good: tree items only point at their icon.
fn node_icon(name: &str, lucide: &'static str, gap: bool) -> Option<SharedImage> {
    thread_local!(static ICONS: RefCell<BTreeMap<(String, bool), Option<SharedImage>>> = RefCell::default());
    ICONS.with_borrow_mut(|m| {
        m.entry((name.to_string(), gap))
            .or_insert_with(|| {
                super::img::any_logo(name, ICON).or_else(|| {
                    let l = if gap {
                        icons::before_label(lucide, super::muted(), ICON)
                    } else {
                        icons::icon(lucide, super::muted(), ICON)
                    };
                    SharedImage::from_image(&l?).ok()
                })
            })
            .clone()
    })
}

/// The Lucide icon of the group `category` ends with.
fn category_icon(category: &str) -> &'static str {
    picker_icons::group(category.rsplit('/').next().unwrap_or(category).trim())
}

/// The icon of an item by its category and title: the service's logo, else the Lucide icon of the title
/// or of the category. Kept for good, like the picker's.
fn icon_of(category: &str, title: &str) -> Option<SharedImage> {
    let mut parts = category.split('/');
    let top = parts.next().unwrap_or("");
    let service = match top {
        "Integrations" | "Donations & Shops" | "Servers & Remote" => parts.next().unwrap_or(top),
        _ => top,
    };
    node_icon(service, picker_icons::item(title, category_icon(category)), false)
}

/// The icon of a trigger (a plugin's, or an unknown one, wears the puzzle).
pub fn trigger_icon(code: u32) -> Option<SharedImage> {
    match crate::types::name(crate::types::TRIGGERS, code) {
        Some(raw) => icon_of(&trigger_categorize(raw), &trigger_clean_title(raw)),
        None => node_icon("", "puzzle", false),
    }
}

/// The icon of a sub-action (a plugin's, or an unknown one, wears the puzzle).
pub fn sub_icon(code: u32) -> Option<SharedImage> {
    let Ok(i) = SUB_ACTION_INFO.binary_search_by_key(&code, |x| x.0) else {
        return node_icon("", "puzzle", false);
    };
    let (_, name, category, _) = SUB_ACTION_INFO[i];
    let title = spotify_player_title(code).unwrap_or(name);
    icon_of(if category.is_empty() { "Core/General" } else { category }, title)
}

/// The Lucide icon of an item.
fn item_icon(it: &PickItem) -> &'static str {
    picker_icons::item(&it.title, category_icon(&it.category))
}

#[derive(Default)]
struct CategoryNode<'a> {
    subgroups: BTreeMap<String, CategoryNode<'a>>,
    items: Vec<&'a PickItem>,
}

/// Where a tree is being filled: the items' map to ids and the first one to select.
struct Fill<'a> {
    expanded: bool,
    node_map: &'a Rc<RefCell<Vec<(TreeItem, usize)>>>,
    first_leaf: Option<(TreeItem, usize)>,
}

fn insert_category_node(tree: &mut Tree, parent: &TreeItem, node: &CategoryNode, depth: usize, fill: &mut Fill) {
    for (name, child) in &node.subgroups {
        if let Some(mut item) = tree.insert(parent, name, parent.children()) {
            item.set_label_font(if depth == 0 {
                Font::HelveticaBold
            } else {
                Font::Helvetica
            });
            item.set_user_icon(node_icon(name, picker_icons::group(name), false));
            if fill.expanded {
                item.open();
            } else {
                item.close();
            }
            insert_category_node(tree, &item, child, depth + 1, fill);
        }
    }
    let mut sorted = node.items.clone();
    sorted.sort_by_key(|a| a.title.to_lowercase());
    for it in sorted {
        if let Some(mut item_node) = tree.insert(parent, &it.title, parent.children()) {
            item_node.set_label_font(Font::Helvetica);
            item_node.set_user_icon(icons::icon(item_icon(it), super::muted(), ICON));
            if fill.first_leaf.is_none() {
                fill.first_leaf = Some((item_node.clone(), it.id));
            }
            fill.node_map.borrow_mut().push((item_node, it.id));
        }
    }
}

/// The right side: what is selected, where it is, and what it does.
#[derive(Clone)]
struct Detail {
    title: Frame,
    path: Frame,
    text: TextBuffer,
}

impl Detail {
    fn show(&mut self, title: &str, path: &str, text: &str, icon: Option<SharedImage>) {
        self.title.set_label(&title.replace('@', "@@"));
        self.title.set_image(icon);
        self.path.set_label(&path.replace('@', "@@"));
        self.text.set_text(text);
        self.title.redraw();
        self.path.redraw();
    }

    fn item(&mut self, it: &PickItem) {
        let icon = node_icon("", item_icon(it), true);
        self.show(&it.title, &it.category.replace('/', " > "), &it.description, icon);
    }

    fn group(&mut self, name: &str) {
        let icon = node_icon(name, picker_icons::group(name), true);
        let title = if super::img::has_logo(name) {
            format!(" {name}")
        } else {
            name.to_string()
        };
        self.show(
            &title,
            crate::i18n::tr("picker.group"),
            crate::i18n::tr("picker.group_hint"),
            icon,
        );
    }
}

/// Hierarchical tree picker dialog, drawn like shadcn's file tree: chevrons, an icon on every node and
/// a rounded selection. Groups start collapsed; searching opens the paths to what it finds.
pub fn pick_tree(win_title: &str, items: &[PickItem]) -> Option<usize> {
    let tr = crate::i18n::tr;
    let (mut win, answer) = window(840, 580, win_title);
    let mut root_col = Flex::new(12, 10, 816, 560, None).column();
    root_col.set_pad(6);

    // Search with the collapse and expand controls
    let mut filter_row = Flex::default().row();
    filter_row.set_pad(4);
    let mut f_icon = Frame::default();
    f_icon.set_image(icons::icon("search", super::muted(), ICON));
    filter_row.fixed(&f_icon, 24);
    let mut filter = Input::default();
    filter.set_tooltip(tr("picker.search_tooltip"));
    super::placeholder(&mut filter, tr("picker.search_placeholder"));
    let mut btn_collapse = button(&mut filter_row, tr("btn.collapse_all"), 95);
    let mut btn_expand = button(&mut filter_row, tr("btn.expand_all"), 85);
    filter_row.end();
    root_col.fixed(&filter_row, ROW);

    // The tree (left) and the detail of what is selected (right)
    let mut split = Flex::default().row();
    split.set_pad(8);

    let mut tree = Tree::default();
    tree.set_show_root(false);
    tree.set_select_mode(TreeSelect::Single);
    style_tree(&mut tree);
    tree.draw(|t| {
        focus_ring(t);
        super::scroll::paint_bars(t);
    });

    let mut detail_col = Flex::default().column();
    detail_col.set_pad(4);
    let mut d_title = Frame::default().with_align(Align::Left | Align::Inside | Align::ImageNextToText);
    d_title.set_label_font(Font::HelveticaBold);
    d_title.set_label_size(14);
    detail_col.fixed(&d_title, 24);

    let mut d_path = Frame::default().with_align(Align::Left | Align::Inside).ellipsis();
    d_path.set_label_color(super::muted());
    detail_col.fixed(&d_path, 18);

    let divider = Frame::default();
    detail_col.fixed(&divider, 4);

    let desc_buf = TextBuffer::default();
    let mut desc_disp = TextDisplay::default();
    super::scroll::view(&mut desc_disp);
    desc_disp.set_buffer(desc_buf.clone());
    desc_disp.wrap_mode(WrapMode::AtBounds, 0);
    desc_disp.set_text_font(Font::Helvetica);
    desc_disp.set_text_size(12);

    detail_col.end();
    split.fixed(&detail_col, 330);
    split.end();

    // Bottom bar
    let mut bot_row = Flex::default().row();
    bot_row.set_pad(6);
    let mut count_label = Frame::default().with_align(Align::Left | Align::Inside).ellipsis();
    count_label.set_label_color(super::muted());
    Frame::default();

    let mut ok_btn = button(&mut bot_row, tr("dialogs.ok"), 90);
    ok_btn.set_shortcut(Shortcut::from_key(Key::Enter));
    let mut cancel_btn = button(&mut bot_row, tr("dialogs.cancel"), 90);
    cancel_btn.set_shortcut(Shortcut::from_key(Key::Escape));
    bot_row.end();
    root_col.fixed(&bot_row, ROW + 4);

    root_col.end();
    win.end();
    win.resizable(&root_col);

    let mut detail = Detail {
        title: d_title,
        path: d_path,
        text: desc_buf,
    };
    detail.show(tr("picker.title"), "", tr("picker.intro"), None);

    let selected_id: Rc<Cell<Option<usize>>> = Rc::new(Cell::new(None));
    let node_map: Rc<RefCell<Vec<(TreeItem, usize)>>> = Rc::default();

    // Rebuild the tree from the query and the expand flag
    let items_vec = items.to_vec();
    let populate = {
        let mut tree = tree.clone();
        let items = items_vec.clone();
        let node_map = node_map.clone();
        let selected_id = selected_id.clone();
        let mut count_label = count_label.clone();
        let mut detail = detail.clone();
        let mut ok_btn = ok_btn.clone();
        move |q: &str, force_expand: Option<bool>| {
            let q = q.trim().to_lowercase();
            let Some(root) = tree.root() else { return };
            tree.clear_children(&root);
            node_map.borrow_mut().clear();
            // The tree is new: nothing in it is picked until a search selects its first match.
            selected_id.set(None);
            ok_btn.deactivate();

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
                let mut cur = &mut root_node;
                for part in it.category.split('/').map(str::trim).filter(|s| !s.is_empty()) {
                    cur = cur.subgroups.entry(part.to_string()).or_default();
                }
                cur.items.push(it);
            }

            count_label.set_label(&crate::i18n::trf(
                "picker.count",
                &[("shown", &matched.to_string()), ("total", &items.len().to_string())],
            ));

            let mut fill = Fill {
                expanded: force_expand.unwrap_or(!q.is_empty()),
                node_map: &node_map,
                first_leaf: None,
            };
            insert_category_node(&mut tree, &root, &root_node, 0, &mut fill);

            if let Some((first_item, id)) = fill.first_leaf {
                if !q.is_empty() {
                    let _ = tree.select_only(&first_item, false);
                    selected_id.set(Some(id));
                    ok_btn.activate();
                    if let Some(it) = items.iter().find(|x| x.id == id) {
                        detail.item(it);
                    }
                }
            } else if matched == 0 {
                detail.show(
                    crate::i18n::tr("picker.no_matches"),
                    "",
                    crate::i18n::tr("picker.try_other"),
                    None,
                );
            }
            tree.redraw();
        }
    };

    // Everything collapsed to start with
    let mut populate_now = populate.clone();
    populate_now("", Some(false));

    filter.set_trigger(fltk::enums::CallbackTrigger::Changed);
    filter.set_callback({
        let mut populate = populate.clone();
        move |inp| populate(&inp.value(), None)
    });
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

    // Selecting an item shows it; selecting a group opens or closes it
    tree.set_callback({
        let node_map = node_map.clone();
        let selected_id = selected_id.clone();
        let items = items_vec.clone();
        let mut detail = detail.clone();
        let mut ok_btn = ok_btn.clone();
        move |t| {
            let Some(mut item) = t.callback_item() else { return };
            if t.callback_reason() != TreeReason::Selected {
                return;
            }
            let id = node_map
                .borrow()
                .iter()
                .find(|(node, _)| node == &item)
                .map(|(_, id)| *id);
            if let Some(id) = id {
                selected_id.set(Some(id));
                ok_btn.activate();
                if let Some(it) = items.iter().find(|x| x.id == id) {
                    detail.item(it);
                }
            } else if let Some(name) = item.label() {
                selected_id.set(None);
                ok_btn.deactivate();
                detail.group(&name);
                if item.is_open() {
                    item.close();
                } else {
                    item.open();
                }
                t.redraw();
            }
        }
    });

    // Double click on an item confirms; so does Enter
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
                if app::event_key() == Key::Enter && selected_id.get().is_some() {
                    answer.set(Some(true));
                    return true;
                }
                false
            }
            _ => false,
        }
    });

    ok_btn.set_callback({
        let answer = answer.clone();
        let selected_id = selected_id.clone();
        move |_| {
            if selected_id.get().is_some() {
                answer.set(Some(true));
            } else {
                super::dialogs::alert(crate::i18n::tr("picker.pick_first"));
            }
        }
    });
    cancel_btn.set_callback({
        let answer = answer.clone();
        move |_| answer.set(Some(false))
    });

    let _ = filter.take_focus();
    let ok = super::dialogs::run(&mut win, &answer, || selected_id.get().is_some());
    if ok {
        selected_id.get()
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use fltk::enums::Color;

    /// Every group and every item of both pickers shows an icon: a logo or a Lucide icon we have.
    #[test]
    fn every_node_has_an_icon() {
        for items in [trigger_pick_items(), sub_action_pick_items()] {
            for it in &items {
                assert!(
                    icons::icon(item_icon(it), Color::White, ICON).is_some(),
                    "{}: {}",
                    it.category,
                    it.title
                );
                let mut path = String::new();
                for part in it.category.split('/').map(str::trim).filter(|s| !s.is_empty()) {
                    path = format!("{path}/{part}");
                    assert!(
                        super::super::img::has_logo(part)
                            || icons::icon(picker_icons::group(part), Color::White, ICON).is_some(),
                        "group {path}"
                    );
                }
            }
        }
    }

    /// Nothing falls back to the plain folder: a group nobody mapped would show it.
    #[test]
    fn no_group_is_left_with_the_plain_folder() {
        let mut bare = std::collections::BTreeSet::new();
        for items in [trigger_pick_items(), sub_action_pick_items()] {
            for it in &items {
                for part in it.category.split('/').map(str::trim).filter(|s| !s.is_empty()) {
                    if !super::super::img::has_logo(part) && picker_icons::group(part) == "folder" {
                        bare.insert(part.to_string());
                    }
                }
            }
        }
        assert!(bare.is_empty(), "groups with no icon of their own: {bare:?}");
    }
}
