//! Elgato Stream Deck and Chat Window codes (the server the Stream Deck plugin talks to is
//! `streamdeck.rs`). Streamer.bot Decks need Streamer.bot's website, which Streamerz doesn't use: their
//! sub-actions (21001-21010) are kept on import and skipped with a log line.
//!
//! Sub-actions, pushed to the Stream Deck plugin:
//! - 11001: StreamDeck Set Title (buttonId, title, state)
//! - 11002: StreamDeck Set Background (buttonId, image, color, state)
//! - 11004: StreamDeck Set State (buttonId, state)
//! - 11005: StreamDeck Show Alert (buttonId)
//! - 11006: StreamDeck Show Ok (buttonId)
//! - 11007: StreamDeck Toggle State (buttonId)
//! - 11008: StreamDeck Set Value (buttonId, value)
//!
//! Triggers: 705 ChatWindowAction, 17001 StreamDeckAction (both dropped by Streamer.bot 1.0.7 from
//! actions), 17002 StreamDeckConnected, 17003 StreamDeckDisconnected, 17004 StreamDeckInfo.

pub const S_STREAM_DECK_SET_TITLE: u32 = 11001;
pub const S_STREAM_DECK_SET_BACKGROUND: u32 = 11002;
pub const S_STREAM_DECK_SET_STATE: u32 = 11004;
pub const S_STREAM_DECK_SHOW_ALERT: u32 = 11005;
pub const S_STREAM_DECK_SHOW_OK: u32 = 11006;
pub const S_STREAM_DECK_TOGGLE_STATE: u32 = 11007;
pub const S_STREAM_DECK_SET_VALUE: u32 = 11008;

pub const T_CHAT_WINDOW_ACTION: u32 = 705;
