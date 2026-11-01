# YouTube / User (sub-actions)

Sub-actions about the YouTube account that Streamerz is set up with.

## Add Broadcaster Information

- Kind: sub-action
- Code: 4010
- Type: YouTubeAddBroadcasterInformation
- Category: YouTube/User
- Needs: nothing (it reads what is set up in Streamerz)

Adds the broadcaster's channel details to the arguments. It calls nothing on YouTube.

### Fields

None.

### Arguments

- `broadcasterUserId`: the YouTube channel id set up in Streamerz.
- `broadcasterUser` and `broadcasterUserName`: the broadcaster's name. Set only when a broadcaster account is logged in.

### Notes

- The name arguments are left out when no broadcaster account is logged in.
