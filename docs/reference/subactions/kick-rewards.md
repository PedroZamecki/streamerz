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
- Needs: nothing (not run yet)

Disables the rewards that are not configured. Streamerz doesn't run it yet.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| Disable not configured | `disableNotConfigured` | On disables the rewards that are not configured. |

### Arguments

None set.

### Notes

- Not done in Streamerz yet: the type is known and kept, but nothing runs it.

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
- Needs: nothing (not run yet)

Resets the redemption counter of a reward. Streamerz doesn't run it yet.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| Reward ID | `rewardId` | The Kick reward. |

### Arguments

None set.

### Notes

- Not done in Streamerz yet: the type is known and kept, but nothing runs it.

## Reset Reward User Counters

- Kind: sub-action
- Code: 35110
- Type: KickChannelRewardResetUserCounters
- Category: Kick/Rewards
- Needs: nothing (not run yet)

Resets the per-user redemption counters of a reward. Streamerz doesn't run it yet.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| Reward ID | `rewardId` | The Kick reward. |

### Arguments

None set.

### Notes

- Not done in Streamerz yet: the type is known and kept, but nothing runs it.

## Set Group Enabled State

- Kind: sub-action
- Code: 35111
- Type: KickChannelRewardSetGroupEnabledState
- Category: Kick/Rewards
- Needs: nothing (not run yet)

Turns on or off every reward in a group. Streamerz doesn't run it yet.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| Group name | `groupName` | The name of the reward group. |
| State | `state` | On enables the group, off disables it. |

### Arguments

None set.

### Notes

- Not done in Streamerz yet: the type is known and kept, but nothing runs it.

## Set Group Paused State

- Kind: sub-action
- Code: 35112
- Type: KickChannelRewardSetGroupPausedState
- Category: Kick/Rewards
- Needs: nothing (not run yet)

Pauses or unpauses every reward in a group. Streamerz doesn't run it yet.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| Group name | `groupName` | The name of the reward group. |
| State | `state` | On pauses the group, off unpauses it. |

### Arguments

None set.

### Notes

- Not done in Streamerz yet: the type is known and kept, but nothing runs it.

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
