# Twitch / Guest Star (sub-actions)

Twitch's Guest Star: guest sessions, invites and the slots guests sit in. Streamerz doesn't run these yet.

## Get Channel Guest Star Settings

- Kind: sub-action
- Code: 520
- Type: TwitchGetChannelGuestStarSettings
- Category: Twitch/Guest Star
- Needs: a Twitch login as a moderator of the channel (some settings: the broadcaster)

Would read the channel's Guest Star settings.

### Fields

None.

### Arguments

- `moderatorSendLiveEnabled`, `slotCount`, `browserSourceAudioEnabled`, `groupLayout`, `browserSourceToken`: the channel's settings.

### Notes

- Twitch still calls its Guest Star API beta, and it hasn't been tried on a live channel with Streamerz yet.

## Update Channel Guest Star Settings

- Kind: sub-action
- Code: 521
- Type: TwitchUpdateChannelGuestStarSettings
- Category: Twitch/Guest Star
- Needs: a Twitch login as a moderator of the channel (some settings: the broadcaster)

Would change the channel's Guest Star settings.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| Mod send live | `moderatorSendLiveEnabled` | True or false. |
| Slot count | `slotCount` | A whole number. |
| Audio enabled | `browserSourceAudioEnabled` | True or false. |
| Group layout | `groupLayout` | Text. |
| Regenerate sources | `regenerateBrowserSources` | True or false. |

### Arguments

None set.

### Notes

- Twitch still calls its Guest Star API beta, and it hasn't been tried on a live channel with Streamerz yet.

## Get Guest Star Session

- Kind: sub-action
- Code: 522
- Type: TwitchGetGuestStarSession
- Category: Twitch/Guest Star
- Needs: a Twitch login as a moderator of the channel (some settings: the broadcaster)

Would read the channel's current Guest Star session.

### Fields

None.

### Arguments

- `guestStarSessionId`: the session.
- `guestCount`: how many guests.
- `guest0UserId`, `guest0User`, `guest0UserName`, `guest0Slot`, `guest0IsLive`, `guest0Volume`, and so on for each one.

### Notes

- Twitch still calls its Guest Star API beta, and it hasn't been tried on a live channel with Streamerz yet.

## Create Guest Star Session

- Kind: sub-action
- Code: 523
- Type: TwitchCreateGuestStarSession
- Category: Twitch/Guest Star
- Needs: a Twitch login as a moderator of the channel (some settings: the broadcaster)

Would start a Guest Star session.

### Fields

None.

### Arguments

- `guestStarSessionId`: the new session.
- `guestCount`, `guest0UserId`... as Get Guest Star Session.

### Notes

- Twitch still calls its Guest Star API beta, and it hasn't been tried on a live channel with Streamerz yet.

## End Guest Star Session

- Kind: sub-action
- Code: 524
- Type: TwitchEndGuestStarSession
- Category: Twitch/Guest Star
- Needs: a Twitch login as a moderator of the channel (some settings: the broadcaster)

Would end the current Guest Star session.

### Fields

None.

### Arguments

None set.

### Notes

- It acts on the session running now; without one it fails with a log line.
- Twitch still calls its Guest Star API beta, and it hasn't been tried on a live channel with Streamerz yet.

## Get Guest Star Invites

- Kind: sub-action
- Code: 525
- Type: TwitchGetGuestStarInvites
- Category: Twitch/Guest Star
- Needs: a Twitch login as a moderator of the channel (some settings: the broadcaster)

Would read the pending Guest Star invites.

### Fields

None.

### Arguments

- `inviteCount`: how many invites.
- `invite0UserId`, `invite0Status`, `invite0InvitedAt`, and so on for each one.

### Notes

- It acts on the session running now; without one it fails with a log line.
- Twitch still calls its Guest Star API beta, and it hasn't been tried on a live channel with Streamerz yet.

## Send Guest Star Invite

- Kind: sub-action
- Code: 526
- Type: TwitchSendGuestStarInvite
- Category: Twitch/Guest Star
- Needs: a Twitch login as a moderator of the channel (some settings: the broadcaster)

Would invite a user to the Guest Star session.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| User login | `userLogin` | A user's login name. |

### Arguments

None set.

### Notes

- It acts on the session running now; without one it fails with a log line.
- Twitch still calls its Guest Star API beta, and it hasn't been tried on a live channel with Streamerz yet.

## Delete Guest Star Invite

- Kind: sub-action
- Code: 527
- Type: TwitchDeleteGuestStarInvite
- Category: Twitch/Guest Star
- Needs: a Twitch login as a moderator of the channel (some settings: the broadcaster)

Would remove a user's Guest Star invite.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| User login | `userLogin` | A user's login name. |

### Arguments

None set.

### Notes

- It acts on the session running now; without one it fails with a log line.
- Twitch still calls its Guest Star API beta, and it hasn't been tried on a live channel with Streamerz yet.

## Assign Guest Star Slot

- Kind: sub-action
- Code: 528
- Type: TwitchAssignGuestStarSlot
- Category: Twitch/Guest Star
- Needs: a Twitch login as a moderator of the channel (some settings: the broadcaster)

Would put a user into a Guest Star slot.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| User login | `userLogin` | A user's login name. |
| Slot | `slot` | A whole number. |

### Arguments

None set.

### Notes

- It acts on the session running now; without one it fails with a log line.
- Twitch still calls its Guest Star API beta, and it hasn't been tried on a live channel with Streamerz yet.

## Update Guest Star Slot

- Kind: sub-action
- Code: 529
- Type: TwitchUpdateGuestStarSlot
- Category: Twitch/Guest Star
- Needs: a Twitch login as a moderator of the channel (some settings: the broadcaster)

Would move a guest from one slot to another.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| Source slot | `sourceSlot` | A whole number. |
| Destination slot | `destinationSlot` | A whole number. |

### Arguments

None set.

### Notes

- It acts on the session running now; without one it fails with a log line.
- Twitch still calls its Guest Star API beta, and it hasn't been tried on a live channel with Streamerz yet.

## Delete Guest Star Slot

- Kind: sub-action
- Code: 530
- Type: TwitchDeleteGuestStarSlot
- Category: Twitch/Guest Star
- Needs: a Twitch login as a moderator of the channel (some settings: the broadcaster)

Would remove a user from a Guest Star slot.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| User login | `userLogin` | A user's login name. |
| Slot | `slot` | A whole number. |

### Arguments

None set.

### Notes

- It acts on the session running now; without one it fails with a log line.
- Twitch still calls its Guest Star API beta, and it hasn't been tried on a live channel with Streamerz yet.

## Update Guest Star Slot Settings

- Kind: sub-action
- Code: 531
- Type: TwitchUpdateGuestStarSlotSettings
- Category: Twitch/Guest Star
- Needs: a Twitch login as a moderator of the channel (some settings: the broadcaster)

Would change one slot's audio, video, live state and volume.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| Slot | `slot` | A whole number. |
| Audio enabled | `audioEnabled` | True or false. |
| Video enabled | `videoEnabled` | True or false. |
| Live | `live` | True or false. |
| Volume | `volume` | A number. |

### Arguments

None set.

### Notes

- It acts on the session running now; without one it fails with a log line.
- Twitch still calls its Guest Star API beta, and it hasn't been tried on a live channel with Streamerz yet.
