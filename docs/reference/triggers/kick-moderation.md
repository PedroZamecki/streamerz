# Kick / Moderation (triggers)

Triggers for moderation actions on Kick: bans.

## User Banned

- Kind: trigger
- Code: 35019
- Type: KickUserBanned
- Category: Kick/Moderation
- Needs: a Kick channel and Kick connected

Fires when a user is banned on Kick.

### Fields

None.

### Arguments

- `user`, `userName`, `userId`, `userType`: the banned user.
- `createdById`, `createdByUsername`, `createdByDisplayName`, `bannedBy`: the moderator who banned them.
- `createdAt`: when Streamerz saw the ban.
- `reason`: the reason given, if any.
- `expiresAt`: empty for a permanent ban.
- `broadcastUser`, `broadcastUsername`, `broadcastUserId`: the broadcaster.

### When

When Kick reports a permanent ban. A timeout fires User Timed Out instead.
