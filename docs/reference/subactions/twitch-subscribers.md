# Twitch / Subscribers (sub-actions)

The subscriber count of your Twitch channel. Both need the channel's owner to be logged in.

## Get Subscriber Count

- Kind: sub-action
- Code: 508
- Type: Get Subscriber Count
- Category: Twitch/Subscribers
- Needs: a Twitch login as the channel's owner (the broadcaster)

Reads the total number of subscribers of the channel.

### Fields

None.

### Arguments

Adds `totalSubscribers`, the total number of subscribers.

## Get Latest Subscriber

- Kind: sub-action
- Code: 509
- Type: Get Latest Subscriber
- Category: Twitch/Subscribers
- Needs: a Twitch login as the channel's owner (the broadcaster)

Runs the same call as Get Subscriber Count.

### Fields

None.

### Arguments

Adds `totalSubscribers`, the total number of subscribers.

### Notes

- The latest subscriber isn't read yet. Only the total is set, the same as Get Subscriber Count.

## Is Subscribed

- Kind: sub-action
- Code: 99513
- Type: TwitchIsSubscribed
- Category: Twitch/Subscribers
- Needs: a Twitch login as the channel's owner (the broadcaster)

Says whether someone subscribes to the channel, and at which tier. This sub-action is Streamerz only: Streamer.bot does not have it.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| Who | `userLogin` | Who: the person who started the action (`%userName%`, the default), the name typed after a command (`%input0%`), or any name typed. Variables are filled in first. |

### Arguments

- `isSubscribed`: true when they subscribe.
- `subscriptionTier`: 1, 2 or 3 (0: not subscribed).
- `isGift`, `gifterUser`: whether it was a gift, and from whom.
