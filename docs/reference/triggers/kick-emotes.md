# Kick / Emotes (triggers)

Triggers for 7TV emotes added to or removed from the channel's emote set on Kick.

## Seven TVEmote Added

- Kind: trigger
- Code: 35021
- Type: KickSevenTVEmoteAdded
- Category: Kick/Emotes
- Needs: a Kick login (the broadcaster account) and Kick connected

Fires when a 7TV emote is added to the channel's emote set.

### Fields

None.

### Arguments

- `emote`: the emote's name.
- `imageUrl`: the emote's image address on the 7TV CDN.

### When

When 7TV reports an emote added to the channel's emote set. Streamerz watches the emote set only while Kick is connected and an action uses one of the 7TV emote triggers.

### Notes

- The watch uses the broadcaster account's Kick user id, so it needs a Kick login.

## Seven TVEmote Removed

- Kind: trigger
- Code: 35022
- Type: KickSevenTVEmoteRemoved
- Category: Kick/Emotes
- Needs: a Kick login (the broadcaster account) and Kick connected

Fires when a 7TV emote is removed from the channel's emote set.

### Fields

None.

### Arguments

- `emote`: the emote's name.
- `imageUrl`: the emote's image address on the 7TV CDN.

### When

When 7TV reports an emote removed from the channel's emote set. The same watch as Seven TVEmote Added serves both.

### Notes

- The watch uses the broadcaster account's Kick user id, so it needs a Kick login.
