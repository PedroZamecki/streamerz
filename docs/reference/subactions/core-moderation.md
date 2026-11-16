# Core / Moderation (sub-actions)

Cross-platform moderation sub-actions that act on linked viewer identities or dispatch to the active platform.

## Ban User

- Kind: sub-action
- Code: 99700
- Type: CrossPlatformBanUser
- Category: Core/Moderation
- Needs: a linked person in Moderation, or platform connections for the linked IDs

Bans a linked person across Twitch, Kick, and YouTube.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| Person | `person` | Linked person name, link ID, or any linked platform handle or ID. Leave empty to resolve from current trigger arguments. |
| Twitch | `twitch` | Whether to also apply the ban on Twitch when linked (default: true). |
| Kick | `kick` | Whether to also apply the ban on Kick when linked (default: true). |
| YouTube | `youtube` | Whether to also apply the ban on YouTube when linked (default: true). |
| Reason | `reason` | Optional moderation reason for Twitch and Kick. |

### Arguments

- `linkedPersonFound`: true if a matching linked person was found.
- `linkedPersonId`, `linkedPersonName`, `linkedTwitchId`, `linkedKickId`, `linkedYouTubeId`, `linkedTikTokId`: details of the matched person.
- `moderationResult`: true when at least one selected platform succeeded and none failed.
- `succeededCount`, `failedCount`: counts of succeeded and failed platforms.
- `succeededPlatforms`, `failedPlatforms`: JSON lists of platform names.

## Timeout User

- Kind: sub-action
- Code: 99701
- Type: CrossPlatformTimeoutUser
- Category: Core/Moderation
- Needs: a linked person in Moderation, or platform connections for the linked IDs

Times out a linked person across Twitch, Kick, and YouTube for the configured duration.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| Person | `person` | Linked person name, link ID, or any linked platform handle or ID. Leave empty to resolve from current trigger arguments. |
| Duration | `duration` | Timeout duration in seconds. Less than 1 is sent as 1. |
| Twitch | `twitch` | Whether to apply on Twitch when linked (default: true). |
| Kick | `kick` | Whether to apply on Kick when linked (default: true). |
| YouTube | `youtube` | Whether to apply on YouTube when linked (default: true). |
| Reason | `reason` | Optional moderation reason for Twitch and Kick. |

### Arguments

- `linkedPersonFound`: true if a matching linked person was found.
- `linkedPersonId`, `linkedPersonName`, `linkedTwitchId`, `linkedKickId`, `linkedYouTubeId`, `linkedTikTokId`: details of the matched person.
- `moderationResult`: true when at least one selected platform succeeded and none failed.
- `succeededCount`, `failedCount`: counts of succeeded and failed platforms.
- `succeededPlatforms`, `failedPlatforms`: JSON lists of platform names.

## Unban User

- Kind: sub-action
- Code: 99702
- Type: CrossPlatformUnbanUser
- Category: Core/Moderation
- Needs: a linked person in Moderation, and a tracked YouTube ban ID when unbanning on YouTube

Removes bans or timeouts for a linked person across Twitch, Kick, and YouTube.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| Person | `person` | Linked person name, link ID, or any linked platform handle or ID. Leave empty to resolve from current trigger arguments. |
| Twitch | `twitch` | Whether to unban on Twitch when linked (default: true). |
| Kick | `kick` | Whether to unban on Kick when linked (default: true). |
| YouTube | `youtube` | Whether to unban on YouTube when linked (default: true). |

### Arguments

- `linkedPersonFound`: true if a matching linked person was found.
- `linkedPersonId`, `linkedPersonName`, `linkedTwitchId`, `linkedKickId`, `linkedYouTubeId`, `linkedTikTokId`: details of the matched person.
- `moderationResult`: true when at least one selected platform succeeded and none failed.
- `succeededCount`, `failedCount`: counts of succeeded and failed platforms.
- `succeededPlatforms`, `failedPlatforms`: JSON lists of platform names.

## Lookup Linked Person

- Kind: sub-action
- Code: 99703
- Type: CrossPlatformLookupLinkedPerson
- Category: Core/Moderation

Looks up a linked person record by name, link ID, or any platform identity, and exposes all linked platform IDs to the action arguments.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| Person | `person` | Name, link ID, or platform handle or ID. Leave empty to resolve from current trigger arguments (`userId`, `uniqueId`, `userSlug`, `userName`, or `user`). |

### Arguments

- `linkedPersonFound`: true when a linked record matched.
- `linkedPersonId`, `linkedPersonName`, `linkedTwitchId`, `linkedKickId`, `linkedYouTubeId`, `linkedTikTokId`: set when found.

## Link Platform Identity

- Kind: sub-action
- Code: 99704
- Type: CrossPlatformLinkIdentity
- Category: Core/Moderation

Creates or updates a linked person record connecting Twitch, Kick, YouTube, and TikTok identities.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| Person Name | `name` | Display name for the linked person. |
| Twitch User ID | `twitch` | Numeric Twitch user ID. |
| Kick User ID | `kick` | Numeric Kick user ID. |
| YouTube Channel ID | `youtube` | YouTube channel ID. |
| TikTok Username | `tiktok` | TikTok handle (with or without `@`). |

### Arguments

- `linkSuccess`: true if the link was saved.
- `linkedPersonId`, `linkedPersonName`, `linkedTwitchId`, `linkedKickId`, `linkedYouTubeId`, `linkedTikTokId`: saved values when `linkSuccess` is true.

## Unlink Platform Identity

- Kind: sub-action
- Code: 99705
- Type: CrossPlatformUnlinkIdentity
- Category: Core/Moderation

Removes one platform identity from a linked person, or deletes the entire linked person record when All is chosen.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| Person | `person` | Name, link ID, or platform ID of the linked person. |
| Platform | `platform` | Which platform to unlink: `All` (0), `Twitch` (1), `Kick` (2), `YouTube` (3), or `TikTok` (4). |

### Arguments

- `unlinkSuccess`: true if the record was updated or removed.

## Delete Message

- Kind: sub-action
- Code: 99706
- Type: CrossPlatformDeleteMessage
- Category: Core/Moderation
- Needs: connection to the target platform

Deletes a chat message on Twitch, Kick, or YouTube, either explicitly or by auto-detecting the platform from the triggering event.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| Platform | `platform` | `Auto` (0), `Twitch` (1), `Kick` (2), or `YouTube` (3). `Auto` reads `eventSource`, `userType`, or `platform` from trigger arguments. |
| Message ID | `messageId` | ID of the chat message to delete. Falls back to `msgId` or `messageId` in trigger arguments if left empty. |

### Arguments

- `deleteMessageResult`: true when the platform accepted the deletion.
