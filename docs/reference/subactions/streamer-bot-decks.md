# Streamer.bot / Decks (sub-actions)

Changes the items, pages and notifications of a deck. Decks are kept in memory by Streamerz.

## Item Set State

- Kind: sub-action
- Code: 21001
- Type: StreamerBotDeckItemSetState
- Category: Streamer.bot/Decks
- Needs: nothing

Turns a deck item on, off or toggles it.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| Deck ID | `deckId` | The deck ID. |
| Deck Name | `deckName` | The deck name, used when the deck ID is empty. |
| Item ID | `itemId` | The item ID. |
| Item Name | `itemName` | The item name. |
| State | `state` | 0 Enable, 1 Disable, 2 Toggle. Any value other than 0 or 1 toggles. |

### Arguments

None set.

### Notes

- A deck is found by its deck ID, or by its deck name when the ID is empty. An item is created the first time it's used.
- The deck state is kept in memory only. No screen or feature reads it yet.

## Item Toggle State

- Kind: sub-action
- Code: 21002
- Type: StreamerBotDeckItemToggleState
- Category: Streamer.bot/Decks
- Needs: nothing

Flips the on or off state of a deck item.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| Deck ID | `deckId` | The deck ID. |
| Deck Name | `deckName` | The deck name, used when the deck ID is empty. |
| Item ID | `itemId` | The item ID. |
| Item Name | `itemName` | The item name. |

### Arguments

None set.

### Notes

- An item is created the first time it's used.
- The deck state is kept in memory only. No screen or feature reads it yet.

## Item Set Title

- Kind: sub-action
- Code: 21003
- Type: StreamerBotDeckItemSetTitle
- Category: Streamer.bot/Decks
- Needs: nothing

Sets the title and color of a deck item, and its on or off state.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| Deck ID | `deckId` | The deck ID. |
| Deck Name | `deckName` | The deck name, used when the deck ID is empty. |
| Item ID | `itemId` | The item ID. |
| Item Name | `itemName` | The item name. |
| Color | `color` | The color of the item. |
| Title | `title` | The title shown on the item. |
| State | `state` | 0 Enable, 1 Disable, 2 Toggle. Any value other than 0 or 1 toggles. |

### Arguments

None set.

### Notes

- The deck state is kept in memory only. No screen or feature reads it yet.

## Item Set Background

- Kind: sub-action
- Code: 21004
- Type: StreamerBotDeckItemSetBackground
- Category: Streamer.bot/Decks
- Needs: nothing

Sets the background of a deck item from a URL, and its color and on or off state.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| Deck ID | `deckId` | The deck ID. |
| Deck Name | `deckName` | The deck name, used when the deck ID is empty. |
| Item ID | `itemId` | The item ID. |
| Item Name | `itemName` | The item name. |
| Color | `color` | The color of the item. |
| URL | `url` | The address of the background image. |
| File ID | `fileId` | An ID of a file. |
| State | `state` | 0 Enable, 1 Disable, 2 Toggle. Any value other than 0 or 1 toggles. |

### Arguments

None set.

### Notes

- `fileId` is saved but not used yet.
- The deck state is kept in memory only. No screen or feature reads it yet.

## Item Set Icon

- Kind: sub-action
- Code: 21005
- Type: StreamerBotDeckItemSetIcon
- Category: Streamer.bot/Decks
- Needs: nothing

Sets the icon of a deck item, and its color and on or off state. The URL is used when it's set, else the name.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| Deck ID | `deckId` | The deck ID. |
| Deck Name | `deckName` | The deck name, used when the deck ID is empty. |
| Item ID | `itemId` | The item ID. |
| Item Name | `itemName` | The item name. |
| Color | `color` | The color of the item. |
| Name | `name` | The name of the icon. |
| URL | `url` | The address of the icon image. |
| File ID | `fileId` | An ID of a file. |
| State | `state` | 0 Enable, 1 Disable, 2 Toggle. Any value other than 0 or 1 toggles. |

### Arguments

None set.

### Notes

- `fileId` is saved but not used yet.
- The deck state is kept in memory only. No screen or feature reads it yet.

## Item Set Value

- Kind: sub-action
- Code: 21006
- Type: StreamerBotDeckItemSetValue
- Category: Streamer.bot/Decks
- Needs: nothing

Sets the value of a deck item, and its on or off state.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| Deck ID | `deckId` | The deck ID. |
| Deck Name | `deckName` | The deck name, used when the deck ID is empty. |
| Item ID | `itemId` | The item ID. |
| Item Name | `itemName` | The item name. |
| Value | `value` | The value shown on the item. |
| State | `state` | 0 Enable, 1 Disable, 2 Toggle. Any value other than 0 or 1 toggles. |

### Arguments

None set.

### Notes

- The deck state is kept in memory only. No screen or feature reads it yet.

## Deck Notify

- Kind: sub-action
- Code: 21007
- Type: StreamerBotDeckNotify
- Category: Streamer.bot/Decks
- Needs: nothing

Writes a notification for a deck to the debug log: the title, the description and the color.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| Deck ID | `deckId` | The deck ID. |
| Deck Name | `deckName` | The deck name, used when the deck ID is empty. |
| Title | `title` | The title of the notification. |
| Description | `description` | The text of the notification. Several lines are allowed. |
| Color | `color` | The color of the notification. |
| Sound ID | `soundId` | The ID of a sound. |

### Arguments

None set.

### Notes

- Not done in Streamerz yet: nothing shows the notification or plays the sound. The sound ID is saved but not used.

## Page Set

- Kind: sub-action
- Code: 21008
- Type: StreamerBotDeckPageSet
- Category: Streamer.bot/Decks
- Needs: nothing

Sets the page of a deck to a number.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| Deck ID | `deckId` | The deck ID. |
| Deck Name | `deckName` | The deck name, used when the deck ID is empty. |
| Page | `page` | The page number, a whole number. |

### Arguments

None set.

### Notes

- The deck state is kept in memory only. No screen or feature reads it yet.

## Page Next

- Kind: sub-action
- Code: 21009
- Type: StreamerBotDeckPageNext
- Category: Streamer.bot/Decks
- Needs: nothing

Moves a deck to the next page, one more than the current page.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| Deck ID | `deckId` | The deck ID. |
| Deck Name | `deckName` | The deck name, used when the deck ID is empty. |

### Arguments

None set.

### Notes

- There is no last page: the number keeps going up.
- The deck state is kept in memory only. No screen or feature reads it yet.

## Page Previous

- Kind: sub-action
- Code: 21010
- Type: StreamerBotDeckPagePrev
- Category: Streamer.bot/Decks
- Needs: nothing

Moves a deck to the previous page, one less than the current page.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| Deck ID | `deckId` | The deck ID. |
| Deck Name | `deckName` | The deck name, used when the deck ID is empty. |

### Arguments

None set.

### Notes

- The page never goes below 0.
- The deck state is kept in memory only. No screen or feature reads it yet.
