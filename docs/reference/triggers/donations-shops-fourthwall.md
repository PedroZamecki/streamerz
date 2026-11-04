# Donations & Shops / Fourthwall (triggers)

Triggers for Fourthwall events: products, orders, gifts, donations, subscriptions, thank you notes, newsletter sign ups and gift draws.

## Fourthwall Product Created

- Kind: trigger
- Code: 24001
- Type: FourthwallProductCreated
- Category: Donations & Shops/Fourthwall
- Needs: the HTTP server running, with Fourthwall sending its webhooks to `/api/fourthwall`

Fires when Fourthwall reports a new product.

### Fields

None.

### Arguments

Streamer.bot's names: the event's data flattened under `fw.` (`fw.username`, `fw.offer.name`,
`fw.variants[0].quantity`), with `fw.shopId` and `fw.createdAt`; a donation also has `fw.donationId`.
The body's own top-level keys are kept too.

### When

When Fourthwall sends a `product:created` or `ProductCreated` event.

### Notes

- Fourthwall posts to the HTTP server, so it has to be able to reach this computer.

## Fourthwall Product Updated

- Kind: trigger
- Code: 24002
- Type: FourthwallProductUpdated
- Category: Donations & Shops/Fourthwall
- Needs: the HTTP server running, with Fourthwall sending its webhooks to `/api/fourthwall`

Fires when Fourthwall reports a changed product.

### Fields

None.

### Arguments

Streamer.bot's names: the event's data flattened under `fw.` (`fw.username`, `fw.offer.name`,
`fw.variants[0].quantity`), with `fw.shopId` and `fw.createdAt`; a donation also has `fw.donationId`.
The body's own top-level keys are kept too.

### When

When Fourthwall sends a `product:updated` or `ProductUpdated` event.

### Notes

- Fourthwall posts to the HTTP server, so it has to be able to reach this computer.

## Fourthwall Gift Purchase

- Kind: trigger
- Code: 24003
- Type: FourthwallGiftPurchase
- Category: Donations & Shops/Fourthwall
- Needs: the HTTP server running, with Fourthwall sending its webhooks to `/api/fourthwall`

Fires when someone buys a gift on Fourthwall.

### Fields

None.

### Arguments

Streamer.bot's names: the event's data flattened under `fw.` (`fw.username`, `fw.offer.name`,
`fw.variants[0].quantity`), with `fw.shopId` and `fw.createdAt`; a donation also has `fw.donationId`.
The body's own top-level keys are kept too.

### When

When Fourthwall sends a `gift:purchase` or `GiftPurchase` event.

### Notes

- Fourthwall posts to the HTTP server, so it has to be able to reach this computer.

## Fourthwall Order Placed

- Kind: trigger
- Code: 24004
- Type: FourthwallOrderPlaced
- Category: Donations & Shops/Fourthwall
- Needs: the HTTP server running, with Fourthwall sending its webhooks to `/api/fourthwall`

Fires when a new order is placed on Fourthwall.

### Fields

None.

### Arguments

Streamer.bot's names: the event's data flattened under `fw.` (`fw.username`, `fw.offer.name`,
`fw.variants[0].quantity`), with `fw.shopId` and `fw.createdAt`; a donation also has `fw.donationId`.
The body's own top-level keys are kept too.

### When

When Fourthwall sends an `order:placed` or `OrderPlaced` event.

### Notes

- Fourthwall posts to the HTTP server, so it has to be able to reach this computer.

## Fourthwall Order Updated

- Kind: trigger
- Code: 24005
- Type: FourthwallOrderUpdated
- Category: Donations & Shops/Fourthwall
- Needs: the HTTP server running, with Fourthwall sending its webhooks to `/api/fourthwall`

Fires when an order on Fourthwall changes.

### Fields

None.

### Arguments

Streamer.bot's names: the event's data flattened under `fw.` (`fw.username`, `fw.offer.name`,
`fw.variants[0].quantity`), with `fw.shopId` and `fw.createdAt`; a donation also has `fw.donationId`.
The body's own top-level keys are kept too.

### When

When Fourthwall sends an `order:updated` or `OrderUpdated` event.

### Notes

- Fourthwall posts to the HTTP server, so it has to be able to reach this computer.

## Fourthwall Donation

- Kind: trigger
- Code: 24006
- Type: FourthwallDonation
- Category: Donations & Shops/Fourthwall
- Needs: the HTTP server running, with Fourthwall sending its webhooks to `/api/fourthwall`

Fires when someone sends a donation through Fourthwall.

### Fields

None.

### Arguments

Streamer.bot's names: the event's data flattened under `fw.` (`fw.username`, `fw.offer.name`,
`fw.variants[0].quantity`), with `fw.shopId` and `fw.createdAt`; a donation also has `fw.donationId`.
The body's own top-level keys are kept too.

### When

When Fourthwall sends a `donation` or `Donation` event. A webhook with no event type at all is read as a donation.

### Notes

- Fourthwall posts to the HTTP server, so it has to be able to reach this computer.

## Fourthwall Subscription Purchased

- Kind: trigger
- Code: 24007
- Type: FourthwallSubscriptionPurchased
- Category: Donations & Shops/Fourthwall
- Needs: the HTTP server running, with Fourthwall sending its webhooks to `/api/fourthwall`

Fires when someone buys a subscription on Fourthwall.

### Fields

None.

### Arguments

Streamer.bot's names: the event's data flattened under `fw.` (`fw.username`, `fw.offer.name`,
`fw.variants[0].quantity`), with `fw.shopId` and `fw.createdAt`; a donation also has `fw.donationId`.
The body's own top-level keys are kept too.

### When

When Fourthwall sends a `subscription:purchased` or `SubscriptionPurchased` event.

### Notes

- Fourthwall posts to the HTTP server, so it has to be able to reach this computer.

## Fourthwall Subscription Expired

- Kind: trigger
- Code: 24008
- Type: FourthwallSubscriptionExpired
- Category: Donations & Shops/Fourthwall
- Needs: the HTTP server running, with Fourthwall sending its webhooks to `/api/fourthwall`

Fires when a subscription on Fourthwall ends.

### Fields

None.

### Arguments

Streamer.bot's names: the event's data flattened under `fw.` (`fw.username`, `fw.offer.name`,
`fw.variants[0].quantity`), with `fw.shopId` and `fw.createdAt`; a donation also has `fw.donationId`.
The body's own top-level keys are kept too.

### When

When Fourthwall sends a `subscription:expired` or `SubscriptionExpired` event.

### Notes

- Fourthwall posts to the HTTP server, so it has to be able to reach this computer.

## Fourthwall Subscription Changed

- Kind: trigger
- Code: 24009
- Type: FourthwallSubscriptionChanged
- Category: Donations & Shops/Fourthwall
- Needs: the HTTP server running, with Fourthwall sending its webhooks to `/api/fourthwall`

Fires when a subscription on Fourthwall changes.

### Fields

None.

### Arguments

Streamer.bot's names: the event's data flattened under `fw.` (`fw.username`, `fw.offer.name`,
`fw.variants[0].quantity`), with `fw.shopId` and `fw.createdAt`; a donation also has `fw.donationId`.
The body's own top-level keys are kept too.

### When

When Fourthwall sends a `subscription:changed` or `SubscriptionChanged` event.

### Notes

- Fourthwall posts to the HTTP server, so it has to be able to reach this computer.

## Fourthwall Thank You Sent

- Kind: trigger
- Code: 24010
- Type: FourthwallThankYouSent
- Category: Donations & Shops/Fourthwall
- Needs: the HTTP server running, with Fourthwall sending its webhooks to `/api/fourthwall`

Fires when Fourthwall sends a thank you note.

### Fields

None.

### Arguments

Streamer.bot's names: the event's data flattened under `fw.` (`fw.username`, `fw.offer.name`,
`fw.variants[0].quantity`), with `fw.shopId` and `fw.createdAt`; a donation also has `fw.donationId`.
The body's own top-level keys are kept too.

### When

When Fourthwall sends a `thank_you:sent` or `ThankYouSent` event.

### Notes

- Fourthwall posts to the HTTP server, so it has to be able to reach this computer.

## Fourthwall Newsletter Subscribed

- Kind: trigger
- Code: 24011
- Type: FourthwallNewsletterSubscribed
- Category: Donations & Shops/Fourthwall
- Needs: the HTTP server running, with Fourthwall sending its webhooks to `/api/fourthwall`

Fires when someone subscribes to the newsletter on Fourthwall.

### Fields

None.

### Arguments

Streamer.bot's names: the event's data flattened under `fw.` (`fw.username`, `fw.offer.name`,
`fw.variants[0].quantity`), with `fw.shopId` and `fw.createdAt`; a donation also has `fw.donationId`.
The body's own top-level keys are kept too.

### When

When Fourthwall sends a `newsletter:subscribed` or `NewsletterSubscribed` event.

### Notes

- Fourthwall posts to the HTTP server, so it has to be able to reach this computer.

## Fourthwall Gift Draw Started

- Kind: trigger
- Code: 24012
- Type: FourthwallGiftDrawStarted
- Category: Donations & Shops/Fourthwall
- Needs: the HTTP server running, with Fourthwall sending its webhooks to `/api/fourthwall`

Fires when a gift draw starts on Fourthwall.

### Fields

None.

### Arguments

Streamer.bot's names: the event's data flattened under `fw.` (`fw.username`, `fw.offer.name`,
`fw.variants[0].quantity`), with `fw.shopId` and `fw.createdAt`; a donation also has `fw.donationId`.
The body's own top-level keys are kept too.

### When

When Fourthwall sends a `gift_draw:started` or `GiftDrawStarted` event.

### Notes

- Fourthwall posts to the HTTP server, so it has to be able to reach this computer.

## Fourthwall Gift Draw Ended

- Kind: trigger
- Code: 24013
- Type: FourthwallGiftDrawEnded
- Category: Donations & Shops/Fourthwall
- Needs: the HTTP server running, with Fourthwall sending its webhooks to `/api/fourthwall`

Fires when a gift draw ends on Fourthwall.

### Fields

None.

### Arguments

Streamer.bot's names: the event's data flattened under `fw.` (`fw.username`, `fw.offer.name`,
`fw.variants[0].quantity`), with `fw.shopId` and `fw.createdAt`; a donation also has `fw.donationId`.
The body's own top-level keys are kept too.

### When

When Fourthwall sends a `gift_draw:ended` or `GiftDrawEnded` event.

### Notes

- Fourthwall posts to the HTTP server, so it has to be able to reach this computer.
