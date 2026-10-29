# Core / Quotes (sub-actions)

The quote database: quotes are kept by Streamerz, so these don't need a platform login.

## Get Quote

- Kind: sub-action
- Code: 1000
- Type: GetQuote
- Category: Core/Quotes
- Needs: nothing

Picks a quote from the database, at random or by its id.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| Type | `quoteType` | `0` for a random quote (the default), `1` for a specific quote. |
| Quote ID (Specific) | `quoteId` | A whole number. Used when the type is `1`. |

### Arguments

When a quote is found:

- `quoteId`, `quote` (the text), `quoteUser`, `quoteUserId`, `quotePlatform` and `quoteGame`: the quote's details.
- `quoteTimestamp`: the time it was added.
- `quoteTime`: the day it was added.

### Notes

- With no quotes in the database, or when the id isn't found, no quote arguments are added.

## Get Quote Count

- Kind: sub-action
- Code: 1035
- Type: GetQuoteCount
- Category: Core/Quotes
- Needs: nothing

Counts the quotes in the database.

### Fields

None.

### Arguments

- `quoteCount`: the number of quotes.

## Delete Quote

- Kind: sub-action
- Code: 1036
- Type: DeleteQuote
- Category: Core/Quotes
- Needs: nothing

Deletes the quote with the given id.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| Quote ID | `quoteId` | A whole number. |

### Arguments

- `success`: true when the quote was deleted, false when no quote has that id.
- `quoteId`: the deleted quote's id (only when it was deleted).
- `error`: why it failed (only when no quote has that id).

## Add Quote

- Kind: sub-action
- Code: 1037
- Type: AddQuote
- Category: Core/Quotes
- Needs: nothing

Adds a quote to the database.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| Quote | `quoteVariable` | The quote's text. Variables are filled in first. |

### Arguments

- `success`: true.
- The quote's details, the same as Get Quote (`quoteId`, `quote`, `quoteUser` and so on).

### Notes

- The quote's author is the user that caused the action, when there is one. Otherwise it is the broadcaster.
- Adding a quote fires the Quote Added trigger.
