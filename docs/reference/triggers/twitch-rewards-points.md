# Twitch / Rewards & Points (triggers)

Twitch's Channel Points events: custom rewards being made, changed and redeemed, automatic rewards, and Bits
power-ups.

## Reward Redemption

- Kind: trigger
- Code: 112
- Type: TwitchRewardRedemption
- Category: Twitch/Rewards & Points
- Needs: a Twitch login that is the channel's owner or a moderator of it

Fires when a viewer redeems a custom Channel Points reward.

### Fields

None.

### Arguments

- `broadcastUser`, `broadcastUserName`, `broadcastUserId`: the channel.
- `user`, `userName`, `userId`, `userType`: the viewer who redeemed it.
- `id`, `redemptionId`: the redemption's id.
- `rewardId`, `rewardTitle`, `rewardName`, `rewardPrompt`, `rewardCost`: the reward.
- `userInput`, `rawInput`, `rawInputEscaped`, `rawInputUrlEncoded`: what the viewer typed, if the reward asks
  for it.
- `status`, `rewardStatus`, `redeemedAt`: the redemption's status and time.
- `skipsQueue`, `requiresUserInput`, `maxPerStream`, `maxPerUserPerStream`, `globalCooldown`,
  `backgroundColor`: the reward's settings.

### When

When Twitch sends a redemption added event for the channel.

### Notes

- The reward's settings come from the last reward created or updated event that Streamerz saw in this session.
  Until one is seen, they are missing.

## Reward Created

- Kind: trigger
- Code: 113
- Type: TwitchRewardCreated
- Category: Twitch/Rewards & Points
- Needs: a Twitch login that is the channel's owner or a moderator of it

Fires when a custom Channel Points reward is created.

### Fields

None.

### Arguments

- `broadcastUser`, `broadcastUserName`, `broadcastUserId`: the channel.
- `rewardId`: the reward's id.
- `rewardTitle`: the reward's title.
- `rewardCost`: the cost, in Channel Points.
- `rewardPrompt`: the reward's prompt.

### When

When Twitch sends a custom reward added event for the channel.

## Reward Updated

- Kind: trigger
- Code: 114
- Type: TwitchRewardUpdated
- Category: Twitch/Rewards & Points
- Needs: a Twitch login that is the channel's owner or a moderator of it

Fires when a custom Channel Points reward is changed.

### Fields

None.

### Arguments

The same as Reward Created.

### When

When Twitch sends a custom reward updated event for the channel.

## Reward Redemption Updated

- Kind: trigger
- Code: 171
- Type: TwitchRewardRedemptionUpdated
- Category: Twitch/Rewards & Points
- Needs: a Twitch login that is the channel's owner or a moderator of it

Fires when the status of a redemption changes on Twitch.

### Fields

None.

### Arguments

The same as Reward Redemption.

### When

When Twitch sends a redemption updated event for the channel.

## Automatic Reward Redemption

- Kind: trigger
- Code: 194
- Type: TwitchAutomaticRewardRedemption
- Category: Twitch/Rewards & Points
- Needs: a Twitch login that is the channel's owner or a moderator of it

Fires when a viewer redeems one of Twitch's automatic rewards, such as unlocking an emote or gigantifying one.

### Fields

None.

### Arguments

- `broadcastUser`, `broadcastUserName`, `broadcastUserId`: the channel.
- `user`, `userName`, `userId`, `userType`: the viewer who redeemed it.
- `redemptionId`: the redemption's id.
- `rewardType`: the kind of automatic reward.
- `rewardCost`: the cost, in Channel Points.
- `userInput`: the text the viewer sent with it.
- `unlockedEmoteId`, `unlockedEmoteName`: the emote unlocked, when the reward unlocks one.
- `gigantifiedEmoteId`, `gigantifiedEmoteName`, `gigantifiedEmoteUrl`: the emote, when the reward gigantifies
  one. The URL is the emote's large image.

### When

When Twitch sends an automatic reward redemption added event for the channel.

## Power Up Redemption

- Kind: trigger
- Code: 196
- Type: TwitchPowerUpRedemption
- Category: Twitch/Rewards & Points
- Needs: a Twitch login that is the channel's owner or a moderator of it

Fires when a viewer uses a Bits power-up.

### Fields

None.

### Arguments

- `broadcastUser`, `broadcastUserName`, `broadcastUserId`: the channel.
- `user`, `userName`, `userId`, `userType`: the viewer who used the power-up.
- `bits`: the Bits spent.
- `powerUpType`: the kind of power-up.
- `emoteId`, `emoteName`: the emote, when the power-up is an emote.
- `messageEffectId`: the message effect, when the power-up is one.
- `message`: the message the viewer sent with it.

### When

When Twitch sends a Bits used event that is a power-up. Cheers sent in a chat message don't fire it.

## Custom Power Up Redemption

- Kind: trigger
- Code: 496
- Type: TwitchCustomPowerUpRedemption
- Category: Twitch/Rewards & Points
- Needs: nothing yet (Streamerz doesn't fire it)

Streamerz doesn't fire this yet.

### Fields

None.

### Arguments

None.

### When

Never, for now.

### Notes

- Streamerz doesn't fire this yet.
