//! Streamer.bot Decks, Elgato Stream Deck, HTML overlays and Chat Window actions (Phase 16).
//!
//! Sub-actions:
//! - 11001: StreamDeck Set Title (buttonId, title, state)
//! - 11002: StreamDeck Set Background (buttonId, image, color, state)
//! - 11004: StreamDeck Set State (buttonId, state)
//! - 11005: StreamDeck Show Alert (buttonId)
//! - 11006: StreamDeck Show Ok (buttonId)
//! - 11007: StreamDeck Toggle State (buttonId)
//! - 11008: StreamDeck Set Value (buttonId, value)
//! - 21001: Deck Set Item State (deckId, deckName, itemId, itemName, state)
//! - 21002: Deck Toggle Item State (deckId, deckName, itemId, itemName)
//! - 21003: Deck Set Item Title (deckId, deckName, itemId, itemName, color, title, state)
//! - 21004: Deck Set Item Background (deckId, deckName, itemId, itemName, color, url, fileId, state)
//! - 21005: Deck Set Item Icon (deckId, deckName, itemId, itemName, color, name, url, fileId, state)
//! - 21006: Deck Set Item Value (deckId, deckName, itemId, itemName, value, state)
//! - 21007: Deck Notify (deckId, deckName, title, description, color, soundId)
//! - 21008: Deck Page Set (deckId, deckName, page)
//! - 21009: Deck Page Next (deckId, deckName)
//! - 21010: Deck Page Previous (deckId, deckName)
//!
//! Triggers:
//! - 705: ChatWindowAction
//! - 17001: StreamDeckAction
//! - 17002: StreamDeckConnected
//! - 17003: StreamDeckDisconnected
//! - 17004: StreamDeckInfo

use std::collections::HashMap;

pub const S_STREAM_DECK_SET_TITLE: u32 = 11001;
pub const S_STREAM_DECK_SET_BACKGROUND: u32 = 11002;
pub const S_STREAM_DECK_SET_STATE: u32 = 11004;
pub const S_STREAM_DECK_SHOW_ALERT: u32 = 11005;
pub const S_STREAM_DECK_SHOW_OK: u32 = 11006;
pub const S_STREAM_DECK_TOGGLE_STATE: u32 = 11007;
pub const S_STREAM_DECK_SET_VALUE: u32 = 11008;

pub const S_DECK_SET_ITEM_STATE: u32 = 21001;
pub const S_DECK_TOGGLE_ITEM_STATE: u32 = 21002;
pub const S_DECK_SET_ITEM_TITLE: u32 = 21003;
pub const S_DECK_SET_ITEM_BACKGROUND: u32 = 21004;
pub const S_DECK_SET_ITEM_ICON: u32 = 21005;
pub const S_DECK_SET_ITEM_VALUE: u32 = 21006;
pub const S_DECK_NOTIFY: u32 = 21007;
pub const S_DECK_PAGE_SET: u32 = 21008;
pub const S_DECK_PAGE_NEXT: u32 = 21009;
pub const S_DECK_PAGE_PREVIOUS: u32 = 21010;

pub const T_CHAT_WINDOW_ACTION: u32 = 705;
pub const T_STREAM_DECK_ACTION: u32 = 17001;
pub const T_STREAM_DECK_CONNECTED: u32 = 17002;
pub const T_STREAM_DECK_DISCONNECTED: u32 = 17003;
pub const T_STREAM_DECK_INFO: u32 = 17004;

#[derive(Clone, Default, Debug, PartialEq)]
pub struct DeckItem {
    pub id: String,
    pub name: String,
    pub title: String,
    pub background: String,
    pub icon: String,
    pub value: String,
    pub state: bool,
    pub color: String,
}

#[derive(Clone, Default, Debug)]
pub struct DeckState {
    pub page: i64,
    pub items: HashMap<String, DeckItem>,
}

impl DeckState {
    pub fn get_or_create<'a>(&'a mut self, id: &str, name: &str) -> &'a mut DeckItem {
        let key = if !id.is_empty() {
            id.to_string()
        } else {
            name.to_string()
        };
        self.items.entry(key.clone()).or_insert_with(|| DeckItem {
            id: id.to_string(),
            name: name.to_string(),
            state: true,
            ..Default::default()
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deck_item_lifecycle() {
        let mut d = DeckState::default();
        let item = d.get_or_create("btn1", "Button 1");
        assert_eq!(item.id, "btn1");
        assert_eq!(item.name, "Button 1");
        assert!(item.state);

        item.title = "Start".into();
        item.state = false;

        let item2 = d.get_or_create("btn1", "");
        assert_eq!(item2.title, "Start");
        assert!(!item2.state);
    }
}
