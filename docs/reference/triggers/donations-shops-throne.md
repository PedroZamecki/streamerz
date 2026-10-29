# Donations & Shops / Throne (triggers)

Events from Throne, the wish list and gift service. Throne sends them to Streamerz by webhook.

## Throne Gift Purchased

- Kind: trigger
- Code: 37001
- Type: ThroneGiftPurchased
- Category: Donations & Shops/Throne
- Needs: the local HTTP server running, and Throne sending its webhooks to it

Fires when a viewer buys a gift from a Throne wish list.

### Fields

None.

### Arguments

The fields of the webhook's JSON, as Throne sent them. Streamerz doesn't rename them.

### When

Fires when the webhook's event is `GiftPurchased`, or when it isn't one of the other Throne events.

### Notes

- The local HTTP server is off until it is started. It listens on `127.0.0.1` port `7474` unless its settings change.
- Streamerz doesn't check a secret on these webhooks. Requests from a browser page on another site are refused.

## Throne Contribution Purchased

- Kind: trigger
- Code: 37002
- Type: ThroneContributionPurchased
- Category: Donations & Shops/Throne
- Needs: the local HTTP server running, and Throne sending its webhooks to it

Fires when a viewer makes a contribution on Throne.

### Fields

None.

### Arguments

The fields of the webhook's JSON, as Throne sent them.

### When

Fires when the webhook's event is `contribution:purchased` or `ContributionPurchased`.

### Notes

None.

## Throne Gift Crowdfunded

- Kind: trigger
- Code: 37003
- Type: ThroneGiftCrowdfunded
- Category: Donations & Shops/Throne
- Needs: the local HTTP server running, and Throne sending its webhooks to it

Fires when a gift is crowdfunded on Throne.

### Fields

None.

### Arguments

The fields of the webhook's JSON, as Throne sent them.

### When

Fires when the webhook's event is `gift:crowdfunded` or `GiftCrowdfunded`.

### Notes

None.
