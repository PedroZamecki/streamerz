# Donations & Shops / Streamloots (triggers)

Events from Streamloots, the card and pack shop. Streamloots sends them to Streamerz by webhook.

## Stream Loots Pack Purchased

- Kind: trigger
- Code: 36001
- Type: StreamLootsPackPurchased
- Category: Donations & Shops/Streamloots
- Needs: the local HTTP server running, and Streamloots sending its webhooks to it

Fires when a viewer buys a Streamloots pack.

### Fields

None.

### Arguments

The fields of the webhook's JSON, as Streamloots sent them. Streamerz doesn't rename them.

### When

Fires when a webhook arrives with a `type` (or `event`) that isn't one of the other Streamloots events.

### Notes

- The local HTTP server is off until it is started. It listens on `127.0.0.1` port `7474` unless its settings change.
- Streamerz doesn't check a secret on these webhooks. Requests from a browser page on another site are refused.

## Stream Loots Pack Gifted

- Kind: trigger
- Code: 36002
- Type: StreamLootsPackGifted
- Category: Donations & Shops/Streamloots
- Needs: the local HTTP server running, and Streamloots sending its webhooks to it

Fires when a viewer gifts a Streamloots pack to someone.

### Fields

None.

### Arguments

The fields of the webhook's JSON, as Streamloots sent them.

### When

Fires when the webhook's event is `pack:gifted` or `PackGifted`.

### Notes

None.

## Stream Loots Card Redeemed

- Kind: trigger
- Code: 36003
- Type: StreamLootsCardRedeemed
- Category: Donations & Shops/Streamloots
- Needs: the local HTTP server running, and Streamloots sending its webhooks to it

Fires when a viewer redeems a card.

### Fields

None.

### Arguments

The fields of the webhook's JSON, as Streamloots sent them.

### When

Fires when the webhook's event is `card:redeemed` or `CardRedeemed`.

### Notes

None.

## Stream Loots Reaction

- Kind: trigger
- Code: 36004
- Type: StreamLootsReaction
- Category: Donations & Shops/Streamloots
- Needs: the local HTTP server running, and Streamloots sending its webhooks to it

Fires when a viewer sends a reaction on Streamloots.

### Fields

None.

### Arguments

The fields of the webhook's JSON, as Streamloots sent them.

### When

Fires when the webhook's event is `reaction` or `Reaction`.

### Notes

None.
