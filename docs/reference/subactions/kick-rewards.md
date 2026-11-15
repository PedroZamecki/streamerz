# Kick / Rewards (sub-actions)

Channel point rewards on Kick: turn them on and off, pause them, change their cost, title, description and color, and read them.

## Set Enabled State

- Kind: sub-action
- Code: 35101
- Type: KickChannelRewardSetEnabledState
- Category: Kick/Rewards
- Needs: a Kick login as the broadcaster

Turns a reward on or off.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| Reward ID | `rewardId` | The Kick reward to change. Variables like `%user%` are filled in first. |
| State | `state` | On enables the reward, off disables it. See Notes. |

### Arguments

- `rewardId`: the reward ID, after variables are filled in.

### Notes

- Kick only lets the app that made a reward change it.
- Ticked enables the reward, unticked disables it. A number also works, as Streamer.bot stores it: 0 enables, 1 disables, any other value flips it.

## Set Paused State

- Kind: sub-action
- Code: 35102
- Type: KickChannelRewardSetPausedState
- Category: Kick/Rewards
- Needs: a Kick login as the broadcaster

Pauses or unpauses a reward.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| Reward ID | `rewardId` | The Kick reward to change. Variables like `%user%` are filled in first. |
| State | `state` | On pauses the reward, off unpauses it. See Notes. |

### Arguments

- `rewardId`: the reward ID, after variables are filled in.

### Notes

- Ticked pauses the reward, unticked unpauses it. A number also works: 0 pauses, 1 unpauses, any other value flips it.

## Set Cost

- Kind: sub-action
- Code: 35103
- Type: KickChannelRewardSetCost
- Category: Kick/Rewards
- Needs: a Kick login as the broadcaster

Sets the cost of a reward in points, or adds to it or takes from it.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| Reward ID | `rewardId` | The Kick reward to change. Variables like `%user%` are filled in first. |
| Cost | `cost` | The number of points. |
| Operator | `operator` | 0 sets the cost, 1 adds to the current cost, 2 takes from it. |

### Arguments

- `rewardId`: the reward ID, after variables are filled in.

### Notes

- The cost is never set below 1.

## Set Title

- Kind: sub-action
- Code: 35105
- Type: KickChannelRewardSetTitle
- Category: Kick/Rewards
- Needs: a Kick login as the broadcaster

Changes the title of a reward.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| Reward ID | `rewardId` | The Kick reward to change. Variables like `%user%` are filled in first. |
| Title | `title` | The new title. Variables are filled in first. |

### Arguments

- `rewardId`: the reward ID, after variables are filled in.

## Set Description

- Kind: sub-action
- Code: 35106
- Type: KickChannelRewardSetDescription
- Category: Kick/Rewards
- Needs: a Kick login as the broadcaster

Changes the description of a reward.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| Reward ID | `rewardId` | The Kick reward to change. Variables like `%user%` are filled in first. |
| Description | `description` | The new description. Variables are filled in first. |

### Arguments

- `rewardId`: the reward ID, after variables are filled in.

## Configure Rewards

- Kind: sub-action
- Code: 35104
- Type: KickChannelRewardConfigure
- Category: Kick/Rewards
- Needs: the broadcaster's Kick login

Disables the rewards that are not configured.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| Turn on | `enable` | The rewards to turn on (and unpause), one per line, by name or id. |
| Pause | `pause` | The rewards to turn on and pause, one per line. |
| Turn off | `disable` | The rewards to turn off, one per line. |
| Turn off the rest | `disableNotConfigured` | On: every other reward Streamerz may change is turned off. |

### Arguments

None set.

### Notes

- Twitch and Kick let an app change only the rewards it made, so only those change; the others stay as they are.
- Streamer.bot keeps these lists in its own editor, which an export doesn't carry: an imported Configure Rewards starts with empty lists.

## Update

- Kind: sub-action
- Code: 35107
- Type: KickChannelRewardUpdate
- Category: Kick/Rewards
- Needs: a Kick login as the broadcaster

Changes the cost, title and description of a reward in one go.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| Reward ID | `rewardId` | The Kick reward to change. Variables like `%user%` are filled in first. |
| Title | `title` | The new title. Left empty, the title stays as it is. |
| Description | `description` | The new description. Left empty, the description stays as it is. |
| Cost | `cost` | The number of points. |
| Operator | `operator` | 0 sets the cost, 1 adds to the current cost, 2 takes from it. |

### Arguments

- `rewardId`: the reward ID, after variables are filled in.

### Notes

- The cost is always sent. To keep the current cost while changing only the title or description, use operator 1 with cost 0.
- The cost is never set below 1.

## Get Reward Info

- Kind: sub-action
- Code: 35108
- Type: KickChannelRewardGetInfo
- Category: Kick/Rewards
- Needs: a Kick login as the broadcaster

Reads a reward and puts its details into the arguments, for the sub-actions after it.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| Reward ID | `rewardId` | The Kick reward to read. Variables like `%user%` are filled in first. |

### Arguments

- `rewardId`: the reward ID, after variables are filled in.
- `rewardTitle`: the title.
- `rewardPrompt`: the description.
- `rewardCost`: the cost in points.
- `rewardBackgroundColor`: the background color.
- `rewardEnabled`: whether the reward is enabled.
- `rewardPaused`: whether the reward is paused.
- `rewardSkipRequestQueue`: whether redemptions skip the request queue.
- `rewardIsOurs`: always true for Kick.

### Notes

- If the reward is not found, nothing is added and the error is logged.

## Reset Reward Counter

- Kind: sub-action
- Code: 35109
- Type: KickChannelRewardResetCounter
- Category: Kick/Rewards
- Needs: nothing: the counts are Streamerz's

Resets the redemption counter of a reward.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| Reward ID | `rewardId` | The Kick reward. |

### Arguments

None set.

### Notes

- The count is the one Streamerz keeps: each Reward Redemption counts, and gives `counter` and `userCounter`.

## Reset Reward User Counters

- Kind: sub-action
- Code: 35110
- Type: KickChannelRewardResetUserCounters
- Category: Kick/Rewards
- Needs: nothing: the counts are Streamerz's

Resets the per-user redemption counters of a reward.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| Reward ID | `rewardId` | The Kick reward. |

### Arguments

None set.

### Notes

- Every user's count of the reward starts again; the reward's own count stays. Each Reward Redemption gives `userCounter`.

## Set Group Enabled State

- Kind: sub-action
- Code: 35111
- Type: KickChannelRewardSetGroupEnabledState
- Category: Kick/Rewards
- Needs: the broadcaster's Kick login

Turns on or off every reward in a group.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| Group name | `groupName` | The name of the reward group. |
| State | `state` | On enables the group, off disables it. |

### Arguments

None set.

### Notes

- A reward's group is set on the Rewards page. Only the rewards Streamerz made can be changed: the others in the group are left as they are, with a log line.

## Set Group Paused State

- Kind: sub-action
- Code: 35112
- Type: KickChannelRewardSetGroupPausedState
- Category: Kick/Rewards
- Needs: the broadcaster's Kick login

Pauses or unpauses every reward in a group.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| Group name | `groupName` | The name of the reward group. |
| State | `state` | On pauses the group, off unpauses it. |

### Arguments

None set.

### Notes

- A reward's group is set on the Rewards page. Only the rewards Streamerz made can be changed: the others in the group are left as they are, with a log line.

## Set Background Color

- Kind: sub-action
- Code: 35113
- Type: KickChannelRewardSetBackgroundColor
- Category: Kick/Rewards
- Needs: a Kick login as the broadcaster

Changes the background color of a reward.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| Reward ID | `rewardId` | The Kick reward to change. Variables like `%user%` are filled in first. |
| Background color | `backgroundColor` | A color from the picker, such as `#AARRGGBB`. Variables are filled in first. |
| Random | `random` | On picks a random color, and the background color field is not used. |

### Arguments

- `rewardId`: the reward ID, after variables are filled in.

### Notes

- Kick takes `#RRGGBB`, so the alpha digits of a picked color are dropped. A `#` is optional.

## Accept Redemption

- Kind: sub-action
- Code: 99601
- Type: KickAcceptRedemption
- Category: Kick/Rewards
- Needs: the broadcaster's Kick login

Accepts a Kick reward redemption. This sub-action is Streamerz only: Streamer.bot does not have it.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| Redemption | `redemptionId` | The redemption's id. Empty: the newest waiting redemption of the reward that started the action, by the same viewer (Kick's chat doesn't say which one it was). |

### Arguments

- `redemptionResult`: true when Kick took it.
- `redemptionId`: the redemption.

### Notes

- Kick only lets an app accept the redemptions of the rewards it made.

## Reject Redemption

- Kind: sub-action
- Code: 99602
- Type: KickRejectRedemption
- Category: Kick/Rewards
- Needs: the broadcaster's Kick login

Rejects a Kick reward redemption, giving the points back. This sub-action is Streamerz only: Streamer.bot does not have it.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| Redemption | `redemptionId` | The redemption's id. Empty: the newest waiting redemption of the reward that started the action, by the same viewer. |

### Arguments

- `redemptionResult`: true when Kick took it.
- `redemptionId`: the redemption.

### Notes

- Kick only lets an app reject the redemptions of the rewards it made.

## Create Reward

- Kind: sub-action
- Code: 99603
- Type: KickCreateReward
- Category: Kick/Rewards
- Needs: the broadcaster's Kick login

Creates a Kick channel reward (up to 50 per channel). This sub-action is Streamerz only: Streamer.bot does not have it.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| Title | `title` | The title. Variables are filled in first. |
| Cost | `cost` | What it costs, in points. |
| Description | `description` | What the reward says to viewers. Optional. |
| Viewer types a message | `inputRequired` | On: the viewer types a message to redeem it. |
| Colour | `backgroundColor` | The reward's colour, picked. Empty: the platform's own. |
| On | `enabled` | On: viewers can redeem it at once. |

### Arguments

- `rewardId`: the new reward's id.

## Delete Reward

- Kind: sub-action
- Code: 99604
- Type: KickDeleteReward
- Category: Kick/Rewards
- Needs: the broadcaster's Kick login

Deletes a Kick channel reward. Kick only lets an app delete the rewards it made. This sub-action is Streamerz only: Streamer.bot does not have it.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| Reward | `rewardId` | The reward, picked from the channel's rewards (or its id typed). |

### Arguments

None set.
