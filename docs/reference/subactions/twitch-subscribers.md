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
