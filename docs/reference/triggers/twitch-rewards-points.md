# Twitch / Rewards & Points (triggers)

Twitch's Channel Points events: custom rewards being made, changed and redeemed, automatic rewards, and Bits
power-ups.

## Reward Redemption

- Kind: trigger
- Code: 112
- Type: TwitchRewardRedemption
- Category: Twitch/Rewards & Points
- Needs: a Twitch login with the `channel:read:redemptions` scope (Twitch says whether the account may listen)

Fires when a viewer redeems a custom Channel Points reward. A redemption with no text shows in the chat as a line
(the reward's picture, name and price); one with text comes as a chat message, which shows the reward over it.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| Reward (none: any) | `rewardId` | The reward this is for: picked from the channel's rewards (found when connected), or typed, by name or id. Empty: any reward. |

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
- `rewardImage`, `rewardColor`: the reward's picture and colour, when known.

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

| Field | Key | What it holds |
| --- | --- | --- |
| Reward (none: any) | `rewardId` | The reward this is for: picked from the channel's rewards (found when connected), or typed, by name or id. Empty: any reward. |

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

| Field | Key | What it holds |
| --- | --- | --- |
| Reward (none: any) | `rewardId` | The reward this is for: picked from the channel's rewards (found when connected), or typed, by name or id. Empty: any reward. |

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

| Field | Key | What it holds |
| --- | --- | --- |
| Reward (none: any) | `rewardId` | The reward this is for: picked from the channel's rewards (found when connected), or typed, by name or id. Empty: any reward. |

### Arguments

The same as Reward Redemption.

### When

When Twitch sends a redemption updated event for the channel.

## Automatic Reward Redemption

- Kind: trigger
- Code: 194
- Type: TwitchAutomaticRewardRedemption
- Category: Twitch/Rewards & Points
- Needs: a Twitch login with the `channel:read:redemptions` scope (Twitch says whether the account may listen)

Fires when a viewer redeems one of Twitch's automatic rewards, such as highlighting a message, unlocking an emote
or gigantifying one. The chat shows it as a line with the reward's name, its price and, for an emote, its picture
(a gigantified one is drawn big).

### Fields

None.

### Arguments

- `broadcastUser`, `broadcastUserName`, `broadcastUserId`: the channel.
- `user`, `userName`, `userId`, `userType`: the viewer who redeemed it.
- `redemptionId`: the redemption's id.
- `rewardType`: the kind of automatic reward.
- `rewardCost`: the cost, in Channel Points.
- `userInput`: the text the viewer sent with it.
- `unlockedEmoteId`, `unlockedEmoteName`, `unlockedEmoteUrl`: the emote unlocked, when the reward unlocks one.
- `gigantifiedEmoteId`, `gigantifiedEmoteName`, `gigantifiedEmoteUrl`: the emote, when the reward gigantifies
  one. The URL is the emote's large image.

### When

When Twitch sends an automatic reward redemption added event for the channel.

## Power Up Redemption

- Kind: trigger
- Code: 196
- Type: TwitchPowerUpRedemption
- Category: Twitch/Rewards & Points
- Needs: a Twitch login with the `bits:read` scope (Twitch says whether the account may listen)

Fires when a viewer uses a Bits power-up (a message effect, a gigantified emote or an on-screen celebration). The
chat shows it as a line with what was used and the Bits it cost.

### Fields

None.

### Arguments

- `broadcastUser`, `broadcastUserName`, `broadcastUserId`: the channel.
- `user`, `userName`, `userId`, `userType`: the viewer who used the power-up.
- `bits`: the Bits spent.
- `powerUpType`: the kind of power-up.
- `emoteId`, `emoteName`, `emoteUrl`: the emote, when the power-up is an emote.
- `messageEffectId`: the message effect, when the power-up is one.
- `message`: the message the viewer sent with it.

### When

When Twitch sends a Bits used event that is a power-up. Cheers sent in a chat message don't fire it.

## Custom Power Up Redemption

- Kind: trigger
- Code: 496
- Type: TwitchCustomPowerUpRedemption
- Category: Twitch/Rewards & Points
- Needs: a Twitch login with the `bits:read` scope (Twitch says whether the account may listen)

Fires when a viewer uses one of the channel's own Bits power-ups. The chat shows it as a line with its title and the
Bits it cost.

### Fields

None.

### Arguments

- `broadcastUser`, `broadcastUserName`, `broadcastUserId`: the channel.
- `user`, `userName`, `userId`, `userType`: the viewer who used the power-up.
- `bits`: the Bits spent.
- `powerUpTitle`: the power-up's title.
- `rewardId`: the power-up's reward id.
- `message`: the message the viewer sent with it.

### When

When Twitch sends a Bits used event that is a custom power-up.
