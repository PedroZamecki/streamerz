# Donations & Shops / StreamElements (triggers)

Tips, merch and the connection to StreamElements. Connect with a StreamElements token on the Donations page, or send the events to the HTTP server.

## Tip

- Kind: trigger
- Code: 1201
- Type: StreamElementsTip
- Category: Donations & Shops/StreamElements
- Needs: a StreamElements token on the Donations page, or the HTTP server for webhooks

Starts an action for each tip. Any StreamElements event that isn't merch or a connection event counts as a tip.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| Min amount (0: none) | `min` | The lowest amount to match. 0 means no lower limit. |
| Max amount (0: none) | `max` | The highest amount to match. 0 means no upper limit. |

### Arguments

- `tipUsername`: the name of the tipper.
- `tipAvatar`: the tipper's avatar.
- `tipAmount`: the amount tipped.
- `tipCurrency`: the currency.
- `tipMessage`: the message with the tip.
- Plus `triggerId`, `triggerName`, `triggerCategory` and `eventSource`.

### When

Each tip that comes in, when its amount is in the range set.

### Notes

- With only Max set, the amount must be exactly that. With only Min set, the amount must be at least that.

## Merch

- Kind: trigger
- Code: 1202
- Type: StreamElementsMerch
- Category: Donations & Shops/StreamElements
- Needs: a StreamElements token on the Donations page, or the HTTP server for webhooks

Starts an action for each merch purchase.

### Fields

None.

### Arguments

- `merchUsername`: the name of the buyer.
- `merchAvatar`: the buyer's avatar.
- `merchAmount`: the amount paid.
- `merchCurrency`: the currency.
- `merchMessage`: the message with the purchase.
- `merchQuantity`: the quantity from the event, or the total of the items' quantities when the event has none.
- `merchItems`: the number of items.
- `merchItem0.name`, `merchItem0.price` and `merchItem0.quantity`, then the same for `merchItem1` and up.
- Plus `triggerId`, `triggerName`, `triggerCategory` and `eventSource`.

### When

Each merch purchase that comes in.

## Connected

- Kind: trigger
- Code: 1203
- Type: StreamElementsConnected
- Category: Donations & Shops/StreamElements
- Needs: a StreamElements token on the Donations page, or the HTTP server for webhooks

Starts an action when Streamerz connects to StreamElements.

### Fields

None.

### Arguments

None beyond the ones every trigger gets: `triggerId`, `triggerName`, `triggerCategory` and `eventSource`.

### When

When the StreamElements connection comes up, or a `connected` event arrives on the webhook.

### Notes

- Connected and Authenticated fire together when the connection signs in with the token.

## Disconnected

- Kind: trigger
- Code: 1204
- Type: StreamElementsDisconnected
- Category: Donations & Shops/StreamElements
- Needs: a StreamElements token on the Donations page, or the HTTP server for webhooks

Starts an action when the StreamElements connection drops.

### Fields

None.

### Arguments

None beyond the ones every trigger gets: `triggerId`, `triggerName`, `triggerCategory` and `eventSource`.

### When

When a connection that was up drops, or a `disconnected` event arrives on the webhook.

### Notes

- Streamerz tries to connect again a minute after a drop.

## Authenticated

- Kind: trigger
- Code: 1205
- Type: StreamElementsAuthenticated
- Category: Donations & Shops/StreamElements
- Needs: a StreamElements token on the Donations page, or the HTTP server for webhooks

Starts an action when the StreamElements connection signs in with the token.

### Fields

None.

### Arguments

None beyond the ones every trigger gets: `triggerId`, `triggerName`, `triggerCategory` and `eventSource`.

### When

When the connection signs in, or an `authenticated` event arrives on the webhook. It fires together with Connected.
