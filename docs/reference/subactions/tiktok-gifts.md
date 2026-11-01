# TikTok / Gifts (sub-actions)

The gifts sent on the TikTok Live that Streamerz reads.

## Get Top Gifters

- Kind: sub-action
- Code: 43002
- Type: TikTokGetTopGifters
- Category: TikTok/Gifts
- Needs: nothing to log in. Gift counts come from the TikTok Live that Streamerz reads.

Reads the viewers who sent the most coins on the live, and puts them into the arguments for the sub-actions after it.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| How many | `count` | How many gifters to read, at least 1. |

### Arguments

- `topGifterCount`: how many gifters were found. This can be less than `count`.
- `topGifter0`, `topGifter1`...: the name of each gifter, from the most coins down.
- `topGifter0UserId`, `topGifter1UserId`...: the user ID of each gifter.
- `topGifter0Coins`, `topGifter1Coins`...: the coins each gifter sent.

### Notes

- Gifters with the same number of coins are ordered by name.
- The list is what Streamerz has read from the TikTok Live so far.
