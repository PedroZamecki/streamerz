# TikTok / Gifts & Coins (triggers)

Gifts sent to a TikTok Live. They need a TikTok channel set: Streamerz connects to its live by itself.

## Gift

- Kind: trigger
- Code: 41002
- Type: TikTokGift
- Category: TikTok/Gifts & Coins
- Needs: a TikTok channel set (with none, nothing is opened)

Fires when a gift is received on TikTok Live.

### Fields

None.

### Arguments

- `user`: the viewer's nickname, or their unique id when there is no nickname.
- `channel`: the channel as set.
- `coins`: the value of this gift message: the gift's diamonds times its repeat count.
- `userTotalCoins`: the coins this viewer has sent in this live.
- `giftId`, `giftName`, `giftType`, `diamondCount`, `repeatCount`, `repeatEnd` and `giftPictureUrl`: the gift as TikTok sent it.
- The rest of the event's user fields, such as `userId` and `uniqueId`.

### When

Fires for each gift message TikTok sends. A combo gift (a streak of the same gift) sends several messages while it lasts, so the trigger fires for each one.

### Notes

- A combo gift (`giftType` 1) adds to `userTotalCoins` only when its streak ends (`repeatEnd`). The total doesn't grow with each repeat.
- The totals start over with each new live.
