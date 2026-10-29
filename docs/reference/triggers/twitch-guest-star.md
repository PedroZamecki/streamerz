# Twitch / Guest Star (triggers)

Twitch's Guest Star: the sessions where guests join the stream, the guests and their slots, and the settings of a session.

## Guest Star Session Begin

- Kind: trigger
- Code: 165
- Type: TwitchGuestStarSessionBegin
- Category: Twitch/Guest Star
- Needs: a Twitch login as the broadcaster

Starts its actions when a Guest Star session starts on this channel.

### Fields

None.

### Arguments

- `broadcastUser`, `broadcastUserName`, `broadcastUserId`: the broadcaster.
- `broadcastIsAffiliate`, `broadcastIsPartner`: always false.
- `sessionId`: the id of the session.

### When

When Twitch sends the session begin event (`channel.guest_star_session.begin`).

### Notes

- Only the broadcaster's own login gets Guest Star events. A moderator's login does not.

## Guest Star Session End

- Kind: trigger
- Code: 166
- Type: TwitchGuestStarSessionEnd
- Category: Twitch/Guest Star
- Needs: a Twitch login as the broadcaster

Starts its actions when a Guest Star session ends on this channel.

### Fields

None.

### Arguments

- `broadcastUser`, `broadcastUserName`, `broadcastUserId`: the broadcaster.
- `broadcastIsAffiliate`, `broadcastIsPartner`: always false.
- `sessionId`: the id of the session.

### When

When Twitch sends the session end event (`channel.guest_star_session.end`).

### Notes

- Only the broadcaster's own login gets Guest Star events. A moderator's login does not.

## Guest Star Guest Update

- Kind: trigger
- Code: 167
- Type: TwitchGuestStarGuestUpdate
- Category: Twitch/Guest Star
- Needs: a Twitch login as the broadcaster

Starts its actions when a guest of a Guest Star session changes, with the guest, the slot and the host's audio and video.

### Fields

None.

### Arguments

- `broadcastUser`, `broadcastUserName`, `broadcastUserId`: the broadcaster.
- `broadcastIsAffiliate`, `broadcastIsPartner`: always false.
- `sessionId`: the id of the session.
- `moderatorUserName`, `moderatorUserLogin`, `moderatorUserId`: the moderator who handled the guest.
- `guestUserName`, `guestUserLogin`, `guestUserId`: the guest.
- `slotId`: the slot the guest is in.
- `state`: the guest's state, as Twitch sends it.
- `hostVideoEnabled`, `hostAudioEnabled`, `hostVolume`: the host's video, audio and volume for the guest.

### When

When Twitch sends the guest update event (`channel.guest_star_guest.update`).

### Notes

- Only the broadcaster's own login gets Guest Star events. A moderator's login does not.

## Guest Star Slot Update

- Kind: trigger
- Code: 168
- Type: TwitchGuestStarSlotUpdate
- Category: Twitch/Guest Star
- Needs: a Twitch login as the broadcaster

Meant to start its actions when a slot of a Guest Star session changes.

### Fields

None.

### Arguments

None set.

### When

It doesn't fire yet (see Notes).

### Notes

- Streamerz doesn't fire this yet.

## Guest Star Settings Update

- Kind: trigger
- Code: 169
- Type: TwitchGuestStarSettingsUpdate
- Category: Twitch/Guest Star
- Needs: a Twitch login as the broadcaster

Starts its actions when the Guest Star settings of this channel change.

### Fields

None.

### Arguments

- `broadcastUser`, `broadcastUserName`, `broadcastUserId`: the broadcaster.
- `broadcastIsAffiliate`, `broadcastIsPartner`: always false.
- `isModeratorSendLiveEnabled`: whether moderators can send guests live.
- `slotCount`: how many guest slots the session has.
- `isBrowserSourceAudioEnabled`: whether the guests' browser source sends audio.
- `groupLayout`: the layout of the guests in the group.

### When

When Twitch sends the settings update event (`channel.guest_star_settings.update`).

### Notes

- Only the broadcaster's own login gets Guest Star events. A moderator's login does not.
