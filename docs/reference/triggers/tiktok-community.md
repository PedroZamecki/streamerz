# TikTok / Community (triggers)

Likes, follows, shares, subscriptions and joins on a TikTok Live. They need a TikTok channel set: Streamerz connects to its live by itself.

## Like

- Kind: trigger
- Code: 41003
- Type: TikTokLike
- Category: TikTok/Community
- Needs: a TikTok channel set (with none, nothing is opened)

Fires when likes or screen taps are received on TikTok Live.

### Fields

None.

### Arguments

- `user`: the viewer's nickname, or their unique id when there is no nickname.
- `channel`: the channel as set.
- `likeCount`: the likes in this message.
- `totalLikes`: the total number of likes TikTok reports for the live.
- The rest of the event's user fields, such as `userId` and `uniqueId`.

### When

Fires for each like message TikTok sends. TikTok batches likes, so one message can stand for several likes (see `likeCount`).

### Notes

None.

## Follow

- Kind: trigger
- Code: 41004
- Type: TikTokFollow
- Category: TikTok/Community
- Needs: a TikTok channel set (with none, nothing is opened)

Fires when a viewer follows the streamer on TikTok.

### Fields

None.

### Arguments

- `user`: the viewer's nickname, or their unique id when there is no nickname.
- `channel`: the channel as set.
- The rest of the event's user fields, such as `userId` and `uniqueId`.

### When

Fires when TikTok reports a new follow during the live.

### Notes

None.

## Share

- Kind: trigger
- Code: 41005
- Type: TikTokShare
- Category: TikTok/Community
- Needs: a TikTok channel set (with none, nothing is opened)

Fires when a viewer shares the TikTok live stream.

### Fields

None.

### Arguments

- `user`: the viewer's nickname, or their unique id when there is no nickname.
- `channel`: the channel as set.
- The rest of the event's user fields, such as `userId` and `uniqueId`.

### When

Fires when TikTok reports a share during the live.

### Notes

None.

## Subscribe

- Kind: trigger
- Code: 41006
- Type: TikTokSubscribe
- Category: TikTok/Community
- Needs: a TikTok channel set (with none, nothing is opened)

Fires when a viewer subscribes to the TikTok channel.

### Fields

None.

### Arguments

- `user`: the viewer's nickname, or their unique id when there is no nickname.
- `channel`: the channel as set.
- `subMonth`: the subscription month as TikTok reports it.
- The rest of the event's user fields, such as `userId` and `uniqueId`.

### When

Fires when TikTok reports a subscription during the live.

### Notes

None.

## Join

- Kind: trigger
- Code: 41007
- Type: TikTokJoin
- Category: TikTok/Community
- Needs: a TikTok channel set (with none, nothing is opened)

Fires when a viewer joins the TikTok live stream.

### Fields

None.

### Arguments

- `user`: the viewer's nickname, or their unique id when there is no nickname.
- `channel`: the channel as set.
- The rest of the event's user fields, such as `userId` and `uniqueId`.

### When

Fires when TikTok reports that a viewer joined the live.

### Notes

None.
