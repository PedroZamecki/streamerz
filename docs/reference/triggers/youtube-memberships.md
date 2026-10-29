# YouTube / Memberships (triggers)

Channel memberships in YouTube: new members, milestones, gifts and members-only chat.

## New Sponsor

- Kind: trigger
- Code: 4008
- Type: YouTubeNewSponsor
- Category: YouTube/Memberships
- Needs: a YouTube channel or video set, and a live broadcast (no login).

Fires when a viewer joins the channel's memberships, or upgrades to a higher level.

### Fields

None.

### Arguments

| Argument | What it holds |
| --- | --- |
| `levelName` | The membership level. |
| `isUpgrade` | On when the membership was upgraded from a lower level. |
| The author arguments | `user`, `userName`, `userId`, and the others the chat messages have. |

### When

When YouTube shows a welcome or upgrade message in the chat.

## Member Mile Stone

- Kind: trigger
- Code: 4009
- Type: YouTubeMemberMileStone
- Category: YouTube/Memberships
- Needs: a YouTube channel or video set, and a live broadcast (no login).

Fires when a member's milestone message is shown in the chat, for the number of months they have been a member.

### Fields

None.

### Arguments

| Argument | What it holds |
| --- | --- |
| `months` | The number of months. |
| `levelName` | The membership level. |
| `message` | The text the member wrote with the milestone. |
| The author arguments | `user`, `userName`, `userId`, and the others the chat messages have. |

### When

When YouTube shows a milestone message in the chat.

## New Sponsor Only Started

- Kind: trigger
- Code: 4010
- Type: YouTubeNewSponsorOnlyStarted
- Category: YouTube/Memberships
- Needs: a YouTube channel or video set, and a live broadcast (no login).

Fires when the chat is turned to members only.

### Fields

None.

### Arguments

The author arguments from the message YouTube shows for the change, and the broadcast arguments.

### When

When the chat page shows that members-only chat was turned on.

### Notes

- Streamerz finds the change from the text of the chat notice, so a notice in another wording is not seen.

## New Sponsor Only Ended

- Kind: trigger
- Code: 4011
- Type: YouTubeNewSponsorOnlyEnded
- Category: YouTube/Memberships
- Needs: a YouTube channel or video set, and a live broadcast (no login).

Fires when members-only chat is turned off.

### Fields

None.

### Arguments

The author arguments from the message YouTube shows for the change, and the broadcast arguments.

### When

When the chat page shows that members-only chat was turned off.

### Notes

- Streamerz finds the change from the text of the chat notice, so a notice in another wording is not seen.

## Membership Gift

- Kind: trigger
- Code: 4014
- Type: YouTubeMembershipGift
- Category: YouTube/Memberships
- Needs: a YouTube channel or video set, and a live broadcast (no login).

Fires when a viewer gifts channel memberships to others.

### Fields

None.

### Arguments

| Argument | What it holds |
| --- | --- |
| `count` | The number of memberships, read from the notice. |
| `tier` | The notice's heading, from YouTube. |
| The author arguments | `user`, `userName`, `userId`, and the others the chat messages have. |

### When

When YouTube shows a gift purchase notice in the chat.

## Gift Membership Received

- Kind: trigger
- Code: 4015
- Type: YouTubeGiftMembershipReceived
- Category: YouTube/Memberships
- Needs: a YouTube channel or video set, and a live broadcast (no login).

Fires when a viewer receives a membership someone gifted to them.

### Fields

None.

### Arguments

| Argument | What it holds |
| --- | --- |
| `gifterUser` | The gifter's name, without `@`. |
| `gifterUserName` | The gifter's name, as YouTube shows it. |
| `gifterUserType` | Always `youtube`. |
| `message` | The text of the notice. |
| The author arguments | `user`, `userName`, `userId`, and the others the chat messages have. |

### When

When YouTube shows that a membership was received as a gift in the chat.
