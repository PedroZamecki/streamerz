//! Hierarchical grouped picker dialog with search, breadcrumb navigation,
//! and real-time detail/hover description card.

use super::{button, ROW};
use crate::types::{SUB_ACTION_INFO, TRIGGERS};
use fltk::enums::{Align, Color, Event, Font, Key, Shortcut};
use fltk::frame::Frame;
use fltk::group::Flex;
use fltk::input::Input;
use fltk::prelude::*;
use fltk::text::{TextBuffer, TextDisplay, WrapMode};
use fltk::tree::{Tree, TreeItem, TreeReason, TreeSelect};
use fltk::window::Window;
use fltk::{app, dialog};
use std::cell::{Cell, RefCell};
use std::collections::HashMap;
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
    let raw = split_pascal(name);
    for prefix in &[
        "Twitch ",
        "Obs ",
        "Slobs ",
        "Kick ",
        "You Tube ",
        "YouTube ",
        "File Watcher ",
        "File Tail ",
        "Stream Deck ",
        "Elgato Stream Deck ",
        "Remote Instance ",
        "Ko Fi ",
        "Kofi ",
        "Stream Elements ",
        "Streamlabs Desktop ",
        "Streamlabs ",
        "Fourthwall ",
        "Patreon ",
        "Throne ",
        "Shopify ",
        "Stream Loots ",
    ] {
        if let Some(rest) = raw.strip_prefix(prefix) {
            return rest.to_string();
        }
    }
    raw
}

/// Categorize triggers into Category / Subgroup hierarchy.
pub fn trigger_categorize(name: &str) -> String {
    if let Some(rest) = name.strip_prefix("Twitch") {
        let sub = if rest.contains("Chat")
            || rest.contains("Whisper")
            || rest.contains("Cheer")
            || rest.contains("FirstWord")
            || rest.contains("Announcement")
        {
            "Chat"
        } else if rest.contains("Sub") || rest.contains("Gift") {
            "Subscriptions"
        } else if rest.contains("Follow") || rest.contains("Stream") || rest.contains("Viewer") {
            "Channel"
        } else if rest.contains("Reward") || rest.contains("PowerUp") {
            "Rewards & Points"
        } else if rest.contains("HypeTrain") {
            "Hype Train"
        } else if rest.contains("Poll") {
            "Polls"
        } else if rest.contains("Prediction") {
            "Predictions"
        } else if rest.contains("Ban")
            || rest.contains("TimeOut")
            || rest.contains("Mod")
            || rest.contains("Vip")
            || rest.contains("Shield")
            || rest.contains("AutoMod")
            || rest.contains("Warning")
            || rest.contains("Terms")
            || rest.contains("Shoutout")
        {
            "Moderation"
        } else if rest.contains("Charity") || rest.contains("Goal") {
            "Charity & Goals"
        } else if rest.contains("Shared") {
            "Shared Chat"
        } else if rest.contains("GuestStar") {
            "Guest Star"
        } else if rest.contains("Ad") {
            "Ads"
        } else if rest.contains("Emote") {
            "Emotes"
        } else if rest.contains("Raid") {
            "Raid"
        } else {
            "General"
        };
        format!("Twitch/{sub}")
    } else if let Some(rest) = name.strip_prefix("Obs") {
        let sub = if rest.contains("Scene") {
            "Scenes"
        } else if rest.contains("Streaming") || rest.contains("Recording") {
            "Streaming & Recording"
        } else if rest.contains("Connected") || rest.contains("Disconnected") {
            "Connection"
        } else {
            "Events"
        };
        format!("OBS Studio/{sub}")
    } else if let Some(rest) = name
        .strip_prefix("Slobs")
        .or_else(|| name.strip_prefix("StreamlabsDesktop"))
    {
        let sub = if rest.contains("Scene") {
            "Scenes"
        } else if rest.contains("Streaming") || rest.contains("Recording") {
            "Streaming & Recording"
        } else {
            "Connection"
        };
        format!("Streamlabs Desktop/{sub}")
    } else if let Some(rest) = name.strip_prefix("Kick") {
        let sub = if rest.contains("Chat") || rest.contains("FirstWord") {
            "Chat"
        } else if rest.contains("Sub") {
            "Subscriptions"
        } else if rest.contains("Ban") || rest.contains("TimeOut") {
            "Moderation"
        } else if rest.contains("Reward") {
            "Rewards"
        } else if rest.contains("Emote") {
            "Emotes"
        } else {
            "Channel"
        };
        format!("Kick/{sub}")
    } else if let Some(rest) = name.strip_prefix("YouTube").or_else(|| name.strip_prefix("Youtube")) {
        let sub = if rest.contains("Message") || rest.contains("Super") || rest.contains("Jewel") {
            "Chat & Super Chats"
        } else if rest.contains("Broadcast") {
            "Broadcast"
        } else if rest.contains("Member") || rest.contains("Sponsor") || rest.contains("Subscriber") {
            "Memberships"
        } else if rest.contains("Ban") || rest.contains("TimeOut") {
            "Moderation"
        } else {
            "General"
        };
        format!("YouTube/{sub}")
    } else if name.starts_with("FileWatcher") {
        "File & Process/File Watcher".into()
    } else if name.starts_with("FileTail") {
        "File & Process/File Tail".into()
    } else if name.starts_with("Process") {
        "File & Process/Process".into()
    } else if name.starts_with("Command") {
        "Core/Commands".into()
    } else if name.starts_with("Timed") || name.contains("Timer") {
        "Core/Timers".into()
    } else if name.starts_with("Global") || name.starts_with("UserGlobal") {
        "Core/Globals".into()
    } else if name.starts_with("Group") {
        "Core/User Groups".into()
    } else if name.starts_with("Midi") {
        "Inputs & MIDI/MIDI".into()
    } else if name.starts_with("SpeechToText") {
        "Inputs & MIDI/Voice Control".into()
    } else if name.starts_with("Key") || name.starts_with("Mouse") || name.starts_with("Input") {
        "Inputs & MIDI/Hotkeys".into()
    } else if name.starts_with("StreamDeck") || name.starts_with("ElgatoStreamDeck") {
        "Decks & Chat/Stream Deck".into()
    } else if name.starts_with("ChatWindow") {
        "Decks & Chat/Chat Window".into()
    } else if name.starts_with("RemoteInstance") {
        "Webhooks & Remote/Remote Instances".into()
    } else if name.starts_with("Kofi") || name.starts_with("KoFi") {
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
    } else if name.starts_with("WebsocketClient") {
        "Servers/WebSocket Client".into()
    } else if name.starts_with("WebsocketCustom") {
        "Servers/Custom Server".into()
    } else if name.starts_with("ElgatoWaveLink") || name.starts_with("WaveLink") {
        "Integrations/Elgato Wave Link".into()
    } else if name.starts_with("VTubeStudio") {
        "Integrations/VTube Studio".into()
    } else if name.starts_with("CrowdControl") {
        "Integrations/Crowd Control".into()
    } else if name.starts_with("Throwing") {
        "Integrations/Throwing System".into()
    } else if name.starts_with("Application") {
        "Core/System".into()
    } else {
        "Core/General".into()
    }
}

/// Helpful description of what a trigger does and when it fires.
pub fn trigger_description(code: u32, raw_name: &str, title: &str, category: &str) -> String {
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
        401 => "Fires when a configured chat command is triggered by a user.".into(),
        402 => "Fires when a command execution is prevented due to an active cooldown.".into(),
        501 => "Fires when a watched file on disk is modified.".into(),
        502 => "Fires when a new file is created in a watched directory folder.".into(),
        503 => "Fires when an existing file is deleted from a watched directory folder.".into(),
        504 => "Fires when a file in a watched directory folder is renamed.".into(),
        511 => "Fires when new lines are appended to a monitored tail log file.".into(),
        701 => "Fires periodically based on a timer configured in the Timers tab.".into(),
        703 => "Fires when an external application or process starts running.".into(),
        704 => "Fires when an external application or process terminates.".into(),
        705 => "Fires on multi-platform chat window actions.".into(),
        14001 => "Fires when obs-websocket connection to OBS Studio is established.".into(),
        14002 => "Fires when obs-websocket connection to OBS Studio is disconnected.".into(),
        14004 => "Fires when the active program scene in OBS Studio changes.".into(),
        14005 => "Fires when OBS Studio begins live streaming.".into(),
        14006 => "Fires when OBS Studio stops live streaming.".into(),
        14007 => "Fires when OBS Studio begins video recording.".into(),
        14008 => "Fires when OBS Studio stops video recording.".into(),
        15001 => "Fires when an incoming MIDI message (note or CC) is received from a device.".into(),
        16001 => "Fires when a configured keyboard shortcut is pressed.".into(),
        16002 => "Fires when a configured mouse button click is detected.".into(),
        17001 => "Fires when a Stream Deck key or dial action is triggered.".into(),
        35001 => "Fires on every standard chat message received from Kick.".into(),
        35003 => "Fires when a user follows the channel on Kick.".into(),
        35010 => "Fires when a subscription is purchased on Kick.".into(),
        4001 => "Fires when a chat message is received from YouTube Live chat.".into(),
        4003 => "Fires when a Super Chat is received in YouTube Live chat.".into(),
        4004 => "Fires when a Super Sticker is received in YouTube Live chat.".into(),
        _ => format!("Fires on the {title} event in {category} ({raw_name})."),
    }
}

/// Description of what a sub-action does.
pub fn sub_action_description(code: u32, name: &str, category: &str) -> String {
    match code {
        1 => "Plays a sound file through the selected audio playback device.".into(),
        3 => "Writes or appends specified text to a local file path on disk, auto-creating parent folders.".into(),
        5 => "Selects a random line from a text file using a cryptographic PRNG and sets a variable.".into(),
        6 => "Runs an external program or script, passing arguments and parsing stdout into variables.".into(),
        10 => "Sends a chat message to the Twitch channel as the broadcaster or configured bot account.".into(),
        11 => "Replies to a specific chat message in Twitch chat.".into(),
        12 => "Toggles subscriber-only chat mode on Twitch.".into(),
        13 => "Toggles emote-only chat mode on Twitch.".into(),
        14 => "Sets slow mode delay for messages in Twitch chat.".into(),
        15 => "Updates the stream broadcast title on Twitch.".into(),
        16 => "Updates the stream game / category on Twitch.".into(),
        18 => "Creates a Twitch poll with options and duration.".into(),
        21 => "Times out a user in Twitch chat for a specified duration in seconds.".into(),
        22 => "Permanently bans a user from Twitch chat.".into(),
        23 => "Unbans a user in Twitch chat.".into(),
        24 => "Deletes a specific chat message from Twitch chat.".into(),
        25 => "Clears all recent messages from Twitch chat.".into(),
        27 => "Toggles Shield Mode on Twitch.".into(),
        28 => "Sends a shoutout to another streamer in Twitch chat.".into(),
        30 => "Connects or disconnects the OBS Studio websocket connection.".into(),
        41 => "Switches the active program scene in OBS Studio.".into(),
        42 => "Sets the visibility (show/hide) of a source in OBS Studio.".into(),
        43 => "Sets the mute state of an audio input source in OBS Studio.".into(),
        44 => "Enables or disables a filter on an OBS Studio source.".into(),
        48 => "Starts, stops, or toggles live streaming in OBS Studio.".into(),
        49 => "Starts, stops, pauses, resumes, or toggles video recording in OBS Studio.".into(),
        328 => "Executes a raw JSON-RPC request to obs-websocket.".into(),
        1001 => "Pauses action execution for the specified number of milliseconds.".into(),
        1002 => "Generates a random integer within a specified minimum and maximum range.".into(),
        1004 => "Enables, disables, or toggles the enabled state of another action.".into(),
        1005 => "Clears all waiting actions from a specific action queue.".into(),
        1006 => "Reads all lines from a text file into an array variable for processing in actions.".into(),
        1007 => "Conditionally branches execution (If/Else) by comparing variable values.".into(),
        1008 => "Executes another action, optionally waiting for it to finish before continuing.".into(),
        1009 => "Adds a non-executing documentation comment or label in the sub-actions list.".into(),
        1010 => "Enables, disables, or toggles the state of a group of commands.".into(),
        1011 => "Enables, disables, or toggles the state of an action group.".into(),
        1012 => "Simulates keyboard keystrokes and modifier combinations.".into(),
        1013 => "Sends an HTTP GET or POST request to a remote URL and parses the response.".into(),
        1020 => "Writes a log message to the streamerz application log and console.".into(),
        1021 => "Reads a specific line number from a text file into a variable.".into(),
        1022 => "Sets the value of a global variable (persisted or temporary).".into(),
        1030 => "Enables or disables background tail monitoring for a log or text file.".into(),
        1039 => "Checks if a file exists on disk and sets a boolean variable in action arguments.".into(),
        1040 => "Checks if a directory folder exists on disk and sets a boolean variable in action arguments.".into(),
        10001 => "Clears all users from a specified user group.".into(),
        11001 => "Sets the title of an Elgato Stream Deck key.".into(),
        11004 => "Sets the state of an Elgato Stream Deck key.".into(),
        21001 => "Sets the state of a Streamer.bot Deck item.".into(),
        21007 => "Sends a push notification to a connected Streamer.bot Deck.".into(),
        21008 => "Switches the active page on a Streamer.bot Deck.".into(),
        35001 => "Sends a chat message to the Kick channel.".into(),
        35030 => "Updates the broadcast title on Kick.".into(),
        35040 => "Bans a user from chat on Kick.".into(),
        4001 => "Sends a chat message to YouTube Live chat.".into(),
        4002 => "Updates the stream title on YouTube.".into(),
        _ => format!("Performs the {name} operation in {category}."),
    }
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
                "General".to_string()
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

/// Hierarchical tree picker dialog with category/subgroup tree and description panel.
pub fn pick_tree(win_title: &str, items: &[PickItem]) -> Option<usize> {
    let (mut win, answer) = window(800, 560, win_title);
    let mut root_col = Flex::new(12, 10, 776, 540, None).column();
    root_col.set_pad(6);

    // Filter input
    let mut filter_row = Flex::default().row();
    filter_row.set_pad(4);
    let f_label = Frame::default()
        .with_label("Search:")
        .with_align(Align::Left | Align::Inside);
    filter_row.fixed(&f_label, 50);
    let mut filter = Input::default();
    filter.set_tooltip("Filter by title, category, or description...");
    filter_row.end();
    root_col.fixed(&filter_row, ROW);

    // Main split view: Tree (left) + Detail card (right)
    let mut split = Flex::default().row();
    split.set_pad(8);

    let mut tree = Tree::default();
    tree.set_show_root(false);
    tree.set_select_mode(TreeSelect::Single);

    let mut detail_col = Flex::default().column();
    detail_col.set_pad(4);
    let mut d_title = Frame::default()
        .with_label("Select an item")
        .with_align(Align::Left | Align::Inside);
    d_title.set_label_font(Font::HelveticaBold);
    d_title.set_label_size(14);
    detail_col.fixed(&d_title, 24);

    let mut d_cat = Frame::default()
        .with_label("Category")
        .with_align(Align::Left | Align::Inside);
    d_cat.set_label_font(Font::HelveticaItalic);
    d_cat.set_label_color(Color::from_rgb(100, 160, 240));
    detail_col.fixed(&d_cat, 18);

    let divider = Frame::default();
    detail_col.fixed(&divider, 4);

    let mut desc_buf = TextBuffer::default();
    desc_buf.set_text("Browse the categories or search above to view details.");
    let mut desc_disp = TextDisplay::default();
    desc_disp.set_buffer(desc_buf.clone());
    desc_disp.wrap_mode(WrapMode::AtBounds, 0);
    desc_disp.set_text_font(Font::Helvetica);
    desc_disp.set_text_size(12);

    detail_col.end();
    split.fixed(&detail_col, 310);
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

    // Rebuild tree contents based on query
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
        move |q: &str| {
            let q = q.trim().to_lowercase();
            let Some(root) = tree.root() else { return };
            tree.clear_children(&root);
            node_map.borrow_mut().clear();

            let mut matched = 0;
            // Group matching items by (top_cat, sub_cat)
            let mut top_groups: HashMap<String, HashMap<String, Vec<&PickItem>>> = HashMap::new();
            for it in &items {
                let m = q.is_empty()
                    || it.title.to_lowercase().contains(&q)
                    || it.category.to_lowercase().contains(&q)
                    || it.description.to_lowercase().contains(&q);
                if !m {
                    continue;
                }
                matched += 1;
                let parts: Vec<&str> = it.category.split('/').collect();
                let top = parts.first().copied().unwrap_or("General").to_string();
                let sub = if parts.len() > 1 {
                    parts[1..].join(" / ")
                } else {
                    String::new()
                };
                top_groups.entry(top).or_default().entry(sub).or_default().push(it);
            }

            count_label.set_label(&format!("Showing {} of {} items", matched, items.len()));

            // Sort categories for consistent order
            let mut top_keys: Vec<_> = top_groups.keys().cloned().collect();
            top_keys.sort();

            let mut first_leaf: Option<(TreeItem, usize)> = None;

            for top in top_keys {
                let Some(mut top_node) = tree.insert(&root, &top, root.children()) else {
                    continue;
                };
                top_node.set_label_font(Font::HelveticaBold);

                let sub_map = &top_groups[&top];
                let mut sub_keys: Vec<_> = sub_map.keys().cloned().collect();
                sub_keys.sort();

                for sub in sub_keys {
                    let items_in_group = &sub_map[&sub];
                    let parent_for_items = if sub.is_empty() {
                        top_node.clone()
                    } else {
                        let Some(mut sub_node) = tree.insert(&top_node, &sub, top_node.children()) else {
                            continue;
                        };
                        sub_node.set_label_font(Font::HelveticaBold);
                        if !q.is_empty() {
                            sub_node.open();
                        }
                        sub_node
                    };

                    let mut sorted_items = items_in_group.clone();
                    sorted_items.sort_by_key(|a| a.title.to_lowercase());

                    for it in sorted_items {
                        if let Some(item_node) = tree.insert(&parent_for_items, &it.title, parent_for_items.children())
                        {
                            if first_leaf.is_none() {
                                first_leaf = Some((item_node.clone(), it.id));
                            }
                            node_map.borrow_mut().push((item_node, it.id));
                        }
                    }
                }

                top_node.open();
            }

            // Select first matching item if filtering
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

    populate("");

    // Wire filter callback
    filter.set_trigger(fltk::enums::CallbackTrigger::Changed);
    filter.set_callback({
        let mut populate = populate.clone();
        move |inp| populate(&inp.value())
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
                // Clicked a group node
                selected_id.set(None);
                d_title.set_label(&lbl);
                d_cat.set_label("Category Folder");
                desc_buf.set_text("Expand this category to select a specific item.");
            }
        }
    });

    // Double-click on leaf item confirms
    tree.handle({
        let answer = answer.clone();
        let selected_id = selected_id.clone();
        let node_map = node_map.clone();
        move |t, ev| match ev {
            Event::Push => {
                if app::event_clicks() {
                    if let Some(item) = t.first_selected_item() {
                        if node_map.borrow().iter().any(|(node, _)| node == &item) {
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
        move |_| answer.set(Some(true))
    });
    cancel_btn.set_callback({
        let answer = answer.clone();
        move |_| answer.set(Some(false))
    });

    let ok = super::dialogs::run(&mut win, &answer, || {
        let sel = selected_id.get().is_some();
        if !sel {
            dialog::alert_default("Please select an item from the list.");
        }
        sel
    });

    if ok {
        selected_id.get()
    } else {
        None
    }
}
