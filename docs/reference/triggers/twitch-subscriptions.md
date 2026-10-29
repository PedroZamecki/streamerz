# Twitch / Subscriptions (triggers)

Subscriptions, resubscriptions, gifts and gift bombs on Twitch, the sub counter, the subscriber chat modes, and the connections Streamerz makes to Twitch.

## Sub

- Kind: trigger
- Code: 103
- Type: TwitchSub
- Category: Twitch/Subscriptions/Channel Subscriptions
- Needs: a Twitch login (any role in the channel)

Fires when a viewer subscribes to the channel for the first time.

### Fields

None.

### Arguments

- The channel: `broadcastUser`, `broadcastUserName`, `broadcastUserId`.
- The subscriber: `user`, `userName`, `userId`, `isSubscribed`, `isModerator`, `isVip`, `badges`, `badgeCount`.
- `tier`: `tier 1`, `tier 2`, `tier 3` or `prime`.
- `isMultiMonth` and `multiMonthDuration` (months bought at once, when more than one).
- `multiMonthTenure`: always 0, Twitch doesn't say how far into a multi-month sub this is.
- `message`, `systemMessage` and `anonymous`.

### When

When Twitch sends a subscription notice in the chat. A subscription from another channel in a shared chat session fires the shared chat trigger instead.

### Notes

- Each sub adds one to the sub counter. When the counter reaches its rollover, Sub Counter Rollover fires.

## Re Sub

- Kind: trigger
- Code: 104
- Type: TwitchReSub
- Category: Twitch/Subscriptions/Channel Subscriptions
- Needs: a Twitch login (any role in the channel)

Fires when a subscriber renews their subscription.

### Fields

None.

### Arguments

- The channel, the subscriber and the notice, as for Sub.
- `tier`, `isMultiMonth`, `multiMonthDuration` and `multiMonthTenure` (always 0), as for Sub.
- `cumulative` (total months subscribed).
- `monthStreak` (months in a row) and `streakShared` (true when Twitch shared the streak).
- `isGift`. When true: `gifterUser`, `gifterUserName`, `gifterUserId`, and `anonymous` is the gifter's.

### When

When Twitch sends a resubscription notice in the chat. A resub from another channel in a shared chat session fires the shared chat trigger instead.

### Notes

- Each resub adds one to the sub counter, as for Sub.

## Gift Sub

- Kind: trigger
- Code: 105
- Type: TwitchGiftSub
- Category: Twitch/Subscriptions/Gift Subscriptions
- Needs: a Twitch login (any role in the channel)

Fires when a viewer gifts a subscription to another viewer.

### Fields

None.

### Arguments

- The channel: `broadcastUser`, `broadcastUserName`, `broadcastUserId`.
- The gifter: `user`, `userName`, `userId`, `isSubscribed`, `isModerator`, `isVip`, `badges`, `badgeCount`.
- `tier`.
- The recipient: `recipientId`, `recipientUser` (display name), `recipientUserName` (login).
- `monthsGifted`, `totalSubsGifted` and `totalSubsGiftedShared` (the gifter's total, as Twitch counts it).
- `fromGiftBomb` (true when it's one of a gift bomb) and `subBombCount` (how many gifts that bomb had, 0 when it isn't from one).
- `cumulativeMonths`: always 0.
- `anonymous`, `systemMessage`.

### When

When Twitch sends a gift notice in the chat. A gift bomb sends one of these for each of its gifts, then Gift Bomb fires.

### Notes

- Each gift adds one to the sub counter, including each gift in a gift bomb.

## Gift Bomb

- Kind: trigger
- Code: 106
- Type: TwitchGiftBomb
- Category: Twitch/Subscriptions/Gift Subscriptions
- Needs: a Twitch login (any role in the channel)

Fires when several gift subscriptions are given at once (a gift bomb), with the recipients.

### Fields

None.

### Arguments

- The channel: `broadcastUser`, `broadcastUserName`, `broadcastUserId`.
- The gifter: `user`, `userName`, `userId`, `isSubscribed`, `isModerator`, `isVip`, `badges`, `badgeCount`.
- `tier`, `gifts` (how many in this bomb), `totalGifts` and `totalGiftsShared` (the gifter's total, as Twitch counts it), and `bonusGifts` (always false).
- For each recipient, from 0 on: `gift.recipientUser0`, `gift.recipientUserName0`, `gift.recipientId0`, then the same with 1, 2... The recipients are the ones Streamerz got in time.

### When

When all of the bomb's gifts have come in (each one is a Gift Sub notice). If they don't all come, it fires 5 seconds after the bomb notice, with the recipients that came by then.

### Notes

- The bomb doesn't add to the sub counter itself. Each gift in it does, through Gift Sub.

## Community Goal Contribution

- Kind: trigger
- Code: 116
- Type: TwitchCommunityGoalContribution
- Category: Twitch/Subscriptions/Gift Subscriptions
- Needs: nothing yet (it never fires)

Streamerz doesn't fire this yet.

### Fields

None.

### Arguments

None.

### When

Never. Streamerz doesn't fire this yet.

## Community Goal Ended

- Kind: trigger
- Code: 117
- Type: TwitchCommunityGoalEnded
- Category: Twitch/Subscriptions/Gift Subscriptions
- Needs: nothing yet (it never fires)

Streamerz doesn't fire this yet.

### Fields

None.

### Arguments

None.

### When

Never. Streamerz doesn't fire this yet.

## Sub Counter Rollover

- Kind: trigger
- Code: 121
- Type: TwitchSubCounterRollover
- Category: Twitch/Subscriptions/Channel Subscriptions
- Needs: a Twitch login (any role in the channel)

Fires when the sub counter reaches the rollover number set for the Twitch account.

### Fields

None.

### Arguments

- The channel: `broadcastUser`, `broadcastUserName`, `broadcastUserId`.
- `subCounter`: the count left after the rollover (the counter starts again from there).
- `rollover`: the rollover number.
- `rolloverCount`: how many times the counter has rolled over.

### When

When a sub, a resub or a single gift takes the counter to the rollover number. Nothing fires when the rollover number is 0.

### Notes

- The counter is kept by Streamerz. Gift bombs count through their single gifts, not as one.

## Broadcaster Event Sub Connected

- Kind: trigger
- Code: 178
- Type: TwitchBroadcasterEventSubConnected
- Category: Twitch/Subscriptions/Channel Subscriptions
- Needs: the broadcaster's Twitch login

Fires when Streamerz's connection to Twitch's events for the broadcaster is up.

### Fields

None.

### Arguments

- The channel: `broadcastUser`, `broadcastUserName`, `broadcastUserId`.

### When

When the EventSub session for the broadcaster starts.

## Broadcaster Event Sub Disconnected

- Kind: trigger
- Code: 179
- Type: TwitchBroadcasterEventSubDisconnected
- Category: Twitch/Subscriptions/Channel Subscriptions
- Needs: the broadcaster's Twitch login

Fires when the connection to Twitch's events for the broadcaster drops, or when Streamerz closes it.

### Fields

None.

### Arguments

- The channel: `broadcastUser`, `broadcastUserName`, `broadcastUserId`.

### When

When the connection is lost, or when Streamerz disconnects it itself.

## Bot Event Sub Connected

- Kind: trigger
- Code: 184
- Type: TwitchBotEventSubConnected
- Category: Twitch/Subscriptions/Channel Subscriptions
- Needs: the bot account's Twitch login

Fires when Streamerz's connection to Twitch's events for the bot account is up.

### Fields

None.

### Arguments

- The channel: `broadcastUser`, `broadcastUserName`, `broadcastUserId`.
- The bot account: `user`, `userName`, `userId`.

### When

When the EventSub session for the bot account starts.

## Bot Event Sub Disconnected

- Kind: trigger
- Code: 185
- Type: TwitchBotEventSubDisconnected
- Category: Twitch/Subscriptions/Channel Subscriptions
- Needs: the bot account's Twitch login

Fires when the connection to Twitch's events for the bot account drops, or when Streamerz closes it.

### Fields

None.

### Arguments

- The channel: `broadcastUser`, `broadcastUserName`, `broadcastUserId`.
- The bot account: `user`, `userName`, `userId`.

### When

When the bot's connection is lost, or when Streamerz disconnects it itself. Only when a bot account is set.

## Chat Subscriber Mode On

- Kind: trigger
- Code: 459
- Type: TwitchChatSubscriberModeOn
- Category: Twitch/Subscriptions/Channel Subscriptions
- Needs: a Twitch login (any role in the channel)

Fires when subscriber only mode is turned on in the chat.

### Fields

None.

### Arguments

- The channel: `broadcastUser`, `broadcastUserName`, `broadcastUserId`.
- The chat settings after the change: `emoteMode`, `followerMode`, `followerModeDuration` (minutes), `slowMode`, `slowModeWaitTime` (seconds), `subscriberOnly`, `uniqueMode`.

### When

When a chat settings update from Twitch turns subscriber only mode on. Streamerz compares each update with the one before it. The first update after connecting is compared with all modes being off.

## Chat Subscriber Mode Off

- Kind: trigger
- Code: 460
- Type: TwitchChatSubscriberModeOff
- Category: Twitch/Subscriptions/Channel Subscriptions
- Needs: a Twitch login (any role in the channel)

Fires when subscriber only mode is turned off in the chat.

### Fields

None.

### Arguments

The same as Chat Subscriber Mode On.

### When

When a chat settings update turns subscriber only mode off. The first update after connecting is compared with all modes being off, as for Chat Subscriber Mode On.

## Prime Paid Upgrade

- Kind: trigger
- Code: 477
- Type: TwitchPrimePaidUpgrade
- Category: Twitch/Subscriptions/Gift Subscriptions
- Needs: a Twitch login (any role in the channel)

Fires when a Prime subscription is upgraded to a paid one.

### Fields

None.

### Arguments

- The channel, the subscriber and the notice, as for Sub.
- `upgradeTier` (Twitch's tier value) and `upgradeTierString` (`tier 1`, `tier 2` or `tier 3`).

### When

When Twitch sends a Prime upgrade notice in the chat.

## Pay It Forward

- Kind: trigger
- Code: 478
- Type: TwitchPayItForward
- Category: Twitch/Subscriptions/Gift Subscriptions
- Needs: a Twitch login (any role in the channel)

Fires when a gifted subscription is paid forward.

### Fields

None.

### Arguments

- The channel, the chatter and the notice, as for Sub.
- `gifterUser`, `gifterUserName`, `gifterUserId` and `anonymous`, as Twitch gave them for the gifter.

### When

When Twitch sends a Pay It Forward notice in the chat.

## Gift Paid Upgrade

- Kind: trigger
- Code: 479
- Type: TwitchGiftPaidUpgrade
- Category: Twitch/Subscriptions/Gift Subscriptions
- Needs: a Twitch login (any role in the channel)

Fires when a gifted subscription is upgraded to a paid one.

### Fields

None.

### Arguments

- The channel, the chatter and the notice, as for Sub.
- `gifterUser`, `gifterUserName`, `gifterUserId` and `anonymous`, as Twitch gave them for the gifter.

### When

When Twitch sends a Gift Paid Upgrade notice in the chat.
