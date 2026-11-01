# Donations & Shops / Ko-fi (triggers)

Ko-fi payments that Ko-fi sends to Streamerz's HTTP server: donations, first and later subscription payments, shop orders and commissions.

## Kofi Donation

- Kind: trigger
- Code: 7001
- Type: KofiDonation
- Category: Donations & Shops/Ko-fi
- Needs: the HTTP server running (off by default, port 7474), and Ko-fi's webhook sent to it

Fires for a Ko-fi donation. Any Ko-fi payment that isn't a subscription, a shop order or a commission counts as a donation.

### Fields

None.

### Arguments

- `user`: the sender's name, `Anonymous` when Ko-fi sends none.
- `fromName`: the same as `user`.
- `amount`: the amount as text, such as `5.00`. `0` when missing.
- `currency`: the currency code. `USD` when missing.
- `message`: the message the sender wrote.
- `type`: the Ko-fi type, such as `Donation`.

### When

When Ko-fi posts to `/kofi` (or `/api/kofi`) and its `type` is not `Subscription`, `Shop Order` or `Commission`. A payload with no `type` counts as a donation.

### Notes

- Ko-fi sends its data as a `data` form field. Streamerz reads that, and also a plain JSON body.
- Streamerz doesn't check a Ko-fi token yet. The webhook takes any POST to the route.

## Kofi Subscription

- Kind: trigger
- Code: 7002
- Type: KofiSubscription
- Category: Donations & Shops/Ko-fi
- Needs: the HTTP server running (off by default, port 7474), and Ko-fi's webhook sent to it

Fires for the first payment of a Ko-fi subscription.

### Fields

None.

### Arguments

Same as Kofi Donation.

### When

When Ko-fi sends a `Subscription` payload with `is_first_subscription_payment` set to true.

### Notes

- A `Subscription` payload without that flag counts as a resubscription. See Kofi Resubscription.

## Kofi Resubscription

- Kind: trigger
- Code: 7003
- Type: KofiResubscription
- Category: Donations & Shops/Ko-fi
- Needs: the HTTP server running (off by default, port 7474), and Ko-fi's webhook sent to it

Fires for a Ko-fi subscription payment after the first one.

### Fields

None.

### Arguments

Same as Kofi Donation.

### When

When Ko-fi sends a `Subscription` payload and `is_first_subscription_payment` is not true.

### Notes

- Streamerz doesn't check a Ko-fi token yet. The webhook takes any POST to the route.

## Kofi Shop Order

- Kind: trigger
- Code: 7004
- Type: KofiShopOrder
- Category: Donations & Shops/Ko-fi
- Needs: the HTTP server running (off by default, port 7474), and Ko-fi's webhook sent to it

Fires for a Ko-fi shop order.

### Fields

None.

### Arguments

Same as Kofi Donation.

### When

When Ko-fi sends a payload with the type `Shop Order`.

### Notes

- Streamerz doesn't check a Ko-fi token yet. The webhook takes any POST to the route.

## Kofi Commission

- Kind: trigger
- Code: 7005
- Type: KofiCommission
- Category: Donations & Shops/Ko-fi
- Needs: the HTTP server running (off by default, port 7474), and Ko-fi's webhook sent to it

Fires for a Ko-fi commission.

### Fields

None.

### Arguments

Same as Kofi Donation.

### When

When Ko-fi sends a payload with the type `Commission`.

### Notes

- Streamerz doesn't check a Ko-fi token yet. The webhook takes any POST to the route.
