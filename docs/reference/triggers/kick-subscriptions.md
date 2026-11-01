# Kick / Subscriptions (triggers)

Triggers for Kick subscriptions: a new subscription, a gifted one, or several gifted at once.

## Subscription

- Kind: trigger
- Code: 35014
- Type: KickSubscription
- Category: Kick/Subscriptions
- Needs: a Kick channel and Kick connected

Fires when someone subscribes on Kick.

### Fields

None.

### Arguments

- `user`, `userName`, `userId`, `userType`: the subscriber.
- `months`, `monthsSubscribed`: the number of months (1 when Kick does not say).
- `duration`: `1`.
- `subscribedAt`: when Streamerz saw the event.
- `expiresAt`: empty.
- `broadcastUser`, `broadcastUsername`, `broadcastUserId`: the broadcaster.

### When

When Kick reports a subscription for one month. Renewals for more than one month fire Resubscription.

## Gift Subscription

- Kind: trigger
- Code: 35016
- Type: KickGiftSubscription
- Category: Kick/Subscriptions
- Needs: a Kick channel and Kick connected

Fires when someone gifts a subscription to one user on Kick.

### Fields

None.

### Arguments

- `user`, `userName`, `userId`, `userType`: the gifter. The name is `Anonymous` when Kick does not give one.
- `count`: `1`.
- `totalGifted`: how many subscriptions the gifter has gifted in total.
- `subscribedAt`: when Streamerz saw the event.
- `recipient.userName`: the recipient's name.
- `recipient.userLogin`: the recipient's name in lowercase.
- `recipient.platform`: `kick`.
- `broadcastUser`, `broadcastUsername`, `broadcastUserId`: the broadcaster.

### When

When Kick reports a gift subscription with one recipient.

### Notes

- When Kick reports a gift without a recipient name, this trigger still fires, with an empty `recipient.userName`.

## Mass Gift Subscription

- Kind: trigger
- Code: 35017
- Type: KickMassGiftSubscription
- Category: Kick/Subscriptions
- Needs: a Kick channel and Kick connected

Fires when someone gifts subscriptions to several users at once on Kick.

### Fields

None.

### Arguments

- `user`, `userName`, `userId`, `userType`: the gifter. The name is `Anonymous` when Kick does not give one.
- `count`: how many recipients.
- `totalGifted`: how many subscriptions the gifter has gifted in total.
- `subscribedAt`: when Streamerz saw the event.
- `recipient.0.userName`, `recipient.0.userLogin`, `recipient.0.platform`: the first recipient. The next recipients use `recipient.1.`, `recipient.2.` and so on.
- `broadcastUser`, `broadcastUsername`, `broadcastUserId`: the broadcaster.

### When

When Kick reports a gift event with two or more recipients.
