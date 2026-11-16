# Core / Moderation (triggers)

Triggers for cross-platform linked-identity moderation events.

## Moderation Applied

- Kind: trigger
- Code: 99700
- Type: CrossPlatformModerationApplied
- Category: Core/Moderation

Fires whenever cross-platform moderation runs for a linked person, either from the Moderation page or from the Cross-Platform Ban, Timeout, or Unban sub-actions.

### Fields

None.

### Arguments

- `operation`: `ban`, `timeout`, or `unban`.
- `linkedPersonId`, `linkedPersonName`, `linkedTwitchId`, `linkedKickId`, `linkedYouTubeId`, `linkedTikTokId`: details of the linked person.
- `duration`: timeout duration in seconds (`0` for ban or unban).
- `reason`: moderation reason string.
- `succeededCount`, `failedCount`: number of platforms that succeeded or failed.
- `succeededPlatforms`, `failedPlatforms`: lists of platform names.

### When

When cross-platform moderation finishes executing across the selected platforms.

## Linked User Banned

- Kind: trigger
- Code: 99701
- Type: CrossPlatformLinkedUserBanned
- Category: Core/Moderation

Fires when a user who belongs to a linked person record is banned on Twitch, Kick, or YouTube.

### Fields

None.

### Arguments

All arguments from the originating platform ban event, plus:

- `sourcePlatform`: `twitch`, `kick`, or `youtube`.
- `linkedPersonId`, `linkedPersonName`, `linkedTwitchId`, `linkedKickId`, `linkedYouTubeId`, `linkedTikTokId`: details of the matched linked person.

### When

When an incoming platform ban event matches an existing linked person record.

## Linked User Timed Out

- Kind: trigger
- Code: 99702
- Type: CrossPlatformLinkedUserTimedOut
- Category: Core/Moderation

Fires when a user who belongs to a linked person record is timed out on Twitch or Kick.

### Fields

None.

### Arguments

All arguments from the originating platform timeout event, plus:

- `sourcePlatform`: `twitch` or `kick`.
- `linkedPersonId`, `linkedPersonName`, `linkedTwitchId`, `linkedKickId`, `linkedYouTubeId`, `linkedTikTokId`: details of the matched linked person.

### When

When an incoming platform timeout event matches an existing linked person record.

## Linked User Unbanned

- Kind: trigger
- Code: 99703
- Type: CrossPlatformLinkedUserUnbanned
- Category: Core/Moderation

Fires when a user who belongs to a linked person record is unbanned or untimed out on Twitch.

### Fields

None.

### Arguments

All arguments from the originating Twitch unban or shared-chat unban event, plus:

- `sourcePlatform`: `twitch`.
- `linkedPersonId`, `linkedPersonName`, `linkedTwitchId`, `linkedKickId`, `linkedYouTubeId`, `linkedTikTokId`: details of the matched linked person.

### When

When an incoming unban event matches an existing linked person record.
