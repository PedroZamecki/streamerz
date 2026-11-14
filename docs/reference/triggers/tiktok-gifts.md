# TikTok / Gifts & Coins (triggers)

Gifts sent to a TikTok Live. They need a TikTok channel set: Streamerz connects to its live by itself.

## Gift

- Kind: trigger
- Code: 41002
- Type: TikTokGift
- Category: TikTok/Gifts & Coins
- Needs: a TikTok channel set (with none, nothing is opened)

Fires when a gift is received on TikTok Live, optionally for one selected gift and a total coin range.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| Gift | `giftId` | Choose a gift by its picture, name and price. Streamerz saves its exact ID (and its name as `giftName`). Any gift matches every gift. An ID can still be set in Edit JSON. |
| Run when | `giftDelivery` | 1: the gift or combo completes. 0: every gift update. New triggers use 1. Older triggers without this setting keep 0. Ordinary gifts match both choices. |
| Minimum total coins | `minCoins` | The gift's unit value times its cumulative quantity must be at least this number. Zero sets no minimum. |
| Maximum total coins | `maxCoins` | The same total must be at most this number. Zero sets no maximum. Bounds are inclusive. |

### Arguments

- `user`: the viewer's nickname, or their unique id when there is no nickname.
- `channel`: the channel as set.
- `coins`: the value of this gift message: the gift's diamonds times its repeat count.
- `userTotalCoins`: the coins this viewer has sent in this live.
- `giftId`, `giftName`, `giftType`, `diamondCount`, `repeatCount`, `repeatEnd` and `giftPictureUrl`: the gift as TikTok sent it.
- The rest of the event's user fields, such as `userId` and `uniqueId`.

### When

With Gift or combo completes, a combo matches only its final message (`repeatEnd`); an ordinary gift
matches as soon as it arrives. Every gift update also matches intermediate combo messages. The chosen
gift ID and all coin bounds must match before the action is queued.

### Example

Choose Rose in Gift and Gift or combo completes in Run when. Add Send Message to Live Chat with
`Thanks, %user%, for %repeatCount% %giftName%!`. A completed combo of three Roses runs once with
`repeatCount` 3. A different gift does not match. Test gift matching previews these rules without
running an action or sending a message.

### Notes

- A combo gift (`giftType` 1) adds to `userTotalCoins` only when its streak ends (`repeatEnd`). The total doesn't grow with each repeat.
- The totals start over with each new live.
- The gift list loads when the Gift form opens without one: TikTok's public list (no login or live
  needed, in the app's language), or, while live with the paired TikTok browser account, the live's
  own list. The refresh icon beside the gift search loads it again. New gifts do not need an app
  update. Public reception works without an account or catalog.
- The trigger saves the gift's name (`giftName`) beside its ID, so it reads well before a list loads.
- A failed refresh keeps known selections usable. A room or browser-account change clears the list;
  late replies from the old context are discarded. Received gifts can add missing entries.
- If a gift message omits its name, picture, unit value or combo type, Streamerz fills the missing
  details from the current room's catalog. Explicit values in the message take precedence.
- Every gift update has a cumulative combo count. Adding `coins` on each update can overcount.
  Use Gift or combo completes for donation-style totals.
- Duplicate suppression is not promised if TikTok repeats an event. Protocol or signing changes
  may still require a connector update.
