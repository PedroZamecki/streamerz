# Kick / Rewards (triggers)

Triggers for channel rewards on Kick.

## Reward Redemption

- Kind: trigger
- Code: 35024
- Type: KickRewardRedemption
- Category: Kick/Rewards
- Needs: a Kick channel and Kick connected

Fires when someone redeems a channel reward on Kick.

### Fields

None.

### Arguments

- `user`, `userName`, `userId`, `userType`: the user who redeemed the reward.
- `reward.id`, `rewardId`: the reward's id.
- `reward.title`: the reward's title.
- `reward.backgroundColor`: the reward's colour.
- `userInput`: the text the user typed for the reward, if any.
- `rawInput`, `rawInputEscaped`, `rawInputUrlEncoded`: the same text, raw, escaped and URL encoded.
- `broadcastUser`, `broadcastUsername`, `broadcastUserId`: the broadcaster.

### When

When Kick reports a reward redemption.
