# Core / Quotes (triggers)

Quotes saved to the channel's quote list, and quotes shown on screen.

## Quote Added

- Kind: trigger
- Code: 601
- Type: QuoteAdded
- Category: Core/Quotes
- Needs: nothing

Starts its actions when a quote is added to the quote list.

### Fields

None.

### Arguments

- `quote`: the text of the quote.
- `quoteId`: the id of the quote.
- `quoteUser`: who the quote is from.
- `quoteGame`: the game being played when the quote was added.
- `quoteTime`: the day the quote was added.

### When

When a quote is added, by the Add Quote sub-action or by hand in the Quotes list.

## Quote Show

- Kind: trigger
- Code: 602
- Type: QuoteShow
- Category: Core/Quotes
- Needs: nothing

Meant to start its actions when a quote is shown.

### Fields

None.

### Arguments

None set.

### When

It doesn't fire yet (see Notes).

### Notes

- Streamerz doesn't fire this yet.
