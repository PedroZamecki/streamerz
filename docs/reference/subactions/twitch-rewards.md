# Twitch / Rewards (sub-actions)

Sub-actions that change a Twitch channel's channel points rewards and their redemptions. Only the channel's owner can use them.

## Update Redemption Status

- Kind: sub-action
- Code: 11
- Type: RedemptionStatus
- Category: Twitch/Rewards
- Needs: the broadcaster's Twitch login (only the channel's owner can do this)

Marks a channel points redemption as fulfilled or canceled.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| Status | `status` | A choice: 0 Fulfilled, 1 Canceled. |

### Arguments

None set.

### Notes

- When `rewardId` or `redemptionId` is empty, the sub-action uses the argument with the same name. For the redemption, `id` also works. So it can act on the redemption that started the action.

## Set Enabled State

- Kind: sub-action
- Code: 200
- Type: RewardSetEnabledState
- Category: Twitch/Rewards
- Needs: the broadcaster's Twitch login (only the channel's owner can do this)

Turns a reward on or off.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| Reward ID | `rewardId` | Text: the ID of the reward. Variables are filled in first. |
| State | `state` | A choice: 0 Enable, 1 Disable, 2 Toggle. |

### Arguments

None set.

### Notes

- Enable turns the reward on and Disable turns it off.
- Toggle is not a real toggle. It turns the reward off, the same as Disable.

## Set Paused State

- Kind: sub-action
- Code: 201
- Type: RewardSetPausedState
- Category: Twitch/Rewards
- Needs: the broadcaster's Twitch login (only the channel's owner can do this)

Pauses or resumes a reward.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| Reward ID | `rewardId` | Text: the ID of the reward. Variables are filled in first. |
| State | `state` | A choice: 0 Enable, 1 Disable, 2 Toggle. |

### Arguments

None set.

### Notes

- Disable pauses the reward and Enable resumes it.
- Toggle resumes the reward, the same as Enable. It is not a real toggle.

## Set Cost

- Kind: sub-action
- Code: 202
- Type: RewardSetCost
- Category: Twitch/Rewards
- Needs: the broadcaster's Twitch login (only the channel's owner can do this)

Sets the cost of a reward, in channel points.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| Reward ID | `rewardId` | Text: the ID of the reward. Variables are filled in first. |
| Cost | `cost` | A whole number: the cost in channel points. |
| Operation | `operator` | A choice: 0 Set, 1 Add, 2 Subtract. |

### Arguments

None set.

### Notes

- Add and Subtract change the cost the reward has now; a cost never goes under 1.

## Configure Rewards

- Kind: sub-action
- Code: 203
- Type: RewardConfigure
- Category: Twitch/Rewards
- Needs: the broadcaster's Twitch login

Turns several rewards on, off or paused at once.

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

## Set Global Cooldown

- Kind: sub-action
- Code: 204
- Type: RewardSetCooldown
- Category: Twitch/Rewards
- Needs: the broadcaster's Twitch login (only the channel's owner can do this)

Sets the global cooldown of a reward: the time before anyone can redeem it again.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| Reward ID | `rewardId` | Text: the ID of the reward. Variables are filled in first. |
| Cooldown (seconds) | `cooldown` | A whole number: the cooldown in seconds. |
| Additive | `additive` | On or off. Stored, but not used yet. |

### Arguments

None set.

### Notes

- A cooldown above 0 turns the global cooldown on. 0 turns it off.

## Set Title

- Kind: sub-action
- Code: 205
- Type: RewardSetTitle
- Category: Twitch/Rewards
- Needs: the broadcaster's Twitch login (only the channel's owner can do this)

Sets the title of a reward.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| Reward ID | `rewardId` | Text: the ID of the reward. Variables are filled in first. |
| Title | `title` | Text: the new title. Variables are filled in first. |

### Arguments

None set.

## Set Prompt

- Kind: sub-action
- Code: 206
- Type: RewardSetPrompt
- Category: Twitch/Rewards
- Needs: the broadcaster's Twitch login (only the channel's owner can do this)

Sets the prompt that viewers see when they redeem a reward.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| Reward ID | `rewardId` | Text: the ID of the reward. Variables are filled in first. |
| Prompt | `prompt` | Text, several lines: the new prompt. Variables are filled in first. |

### Arguments

None set.

## Update

- Kind: sub-action
- Code: 207
- Type: RewardUpdate
- Category: Twitch/Rewards
- Needs: the broadcaster's Twitch login (only the channel's owner can do this)

Updates a reward's title, prompt and cost in one call.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| Reward ID | `rewardId` | Text: the ID of the reward. Variables are filled in first. |
| Title | `title` | Text: the new title. |
| Prompt | `prompt` | Text, several lines: the new prompt. |
| Cost | `cost` | A whole number: the new cost in channel points. |
| Operation | `operator` | A choice: 0 Set, 1 Add, 2 Subtract. Stored, but not used yet. |

### Arguments

None set.

### Notes

- The title, prompt and cost are all sent each time, even when they are empty.
- The operation isn't used yet: the cost is always set.

## Get Reward Info

- Kind: sub-action
- Code: 208
- Type: RewardGetInfo
- Category: Twitch/Rewards
- Needs: the broadcaster's Twitch login (only the channel's owner can do this)

Reads a reward from Twitch and puts its settings in the arguments.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| Reward ID | `rewardId` | Text: the ID of the reward. Variables are filled in first. |

### Arguments

- `rewardId`: the ID of the reward.
- `rewardTitle`: the title.
- `rewardCost`: the cost in channel points.
- `rewardPrompt`: the prompt.
- `rewardBackgroundColor`: the background color.
- `rewardEnabled`: true when the reward is on.
- `rewardPaused`: true when the reward is paused.
- `rewardSkipRequestQueue`: true when redemptions skip the request queue.
- `rewardGlobalCooldown`: the global cooldown, in seconds.
- `rewardMaxPerStream`: the maximum redemptions per stream.
- `rewardMaxPerUserPerStream`: the maximum redemptions per user per stream.

### Notes

- When Twitch doesn't return the reward, no arguments are set.

## Reset Reward Counter

- Kind: sub-action
- Code: 209
- Type: RewardResetCounter
- Category: Twitch/Rewards
- Needs: nothing: the counts are Streamerz's

Resets the redemption count of a reward.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| Reward ID | `rewardId` | Text: the ID of the reward. Variables are filled in first. |

### Arguments

None set.

### Notes

- The count is the one Streamerz keeps: each Reward Redemption counts, and gives `counter` and `userCounter`.

## Reset Reward User Counters

- Kind: sub-action
- Code: 210
- Type: RewardResetUserCounters
- Category: Twitch/Rewards
- Needs: nothing: the counts are Streamerz's

Resets the redemption counts of every user for a reward.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| Reward ID | `rewardId` | Text: the ID of the reward. Variables are filled in first. |

### Arguments

None set.

### Notes

- Every user's count of the reward starts again; the reward's own count stays. Each Reward Redemption gives `userCounter`.

## Set Group Enabled State

- Kind: sub-action
- Code: 211
- Type: RewardSetGroupEnabledState
- Category: Twitch/Rewards
- Needs: the broadcaster's Twitch login

Turns the rewards in a group on or off.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| Group name | `groupName` | Text: the name of the group. |
| State | `state` | A choice: 0 Enable, 1 Disable, 2 Toggle. |

### Arguments

None set.

### Notes

- A reward's group is set on the Rewards page. Only the rewards Streamerz made can be changed: the others in the group are left as they are, with a log line.

## Set Group Paused State

- Kind: sub-action
- Code: 212
- Type: RewardSetGroupPausedState
- Category: Twitch/Rewards
- Needs: the broadcaster's Twitch login

Pauses or resumes the rewards in a group.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| Group name | `groupName` | Text: the name of the group. |
| State | `state` | A choice: 0 Enable, 1 Disable, 2 Toggle. |

### Arguments

None set.

### Notes

- A reward's group is set on the Rewards page. Only the rewards Streamerz made can be changed: the others in the group are left as they are, with a log line.

## Set Background Color

- Kind: sub-action
- Code: 213
- Type: RewardSetBackgroundColor
- Category: Twitch/Rewards
- Needs: the broadcaster's Twitch login (only the channel's owner can do this)

Sets the background color of a reward.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| Reward ID | `rewardId` | Text: the ID of the reward. Variables are filled in first. |
| Background color | `backgroundColor` | Text: the color. Variables are filled in first. |
| Random | `random` | On or off. Stored, but not used yet. |

### Arguments

None set.

### Notes

- The color is sent as it is written.
- Random isn't used yet.

## Set Max per User per Stream

- Kind: sub-action
- Code: 214
- Type: RewardSetMaxPerUserPerStream
- Category: Twitch/Rewards
- Needs: the broadcaster's Twitch login (only the channel's owner can do this)

Sets how many times one user can redeem a reward in one stream.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| Reward ID | `rewardId` | Text: the ID of the reward. Variables are filled in first. |
| Max redeems | `redeems` | A whole number: the limit per user. |
| Additive | `additive` | On or off. Stored, but not used yet. |

### Arguments

None set.

### Notes

- A number above 0 turns the limit on. 0 turns it off.

## Set Max Per Stream

- Kind: sub-action
- Code: 215
- Type: RewardSetMaxPerStream
- Category: Twitch/Rewards
- Needs: the broadcaster's Twitch login (only the channel's owner can do this)

Sets how many times a reward can be redeemed in one stream, by everyone together.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| Reward ID | `rewardId` | Text: the ID of the reward. Variables are filled in first. |
| Max redeems | `redeems` | A whole number: the limit for the stream. |
| Additive | `additive` | On or off. Stored, but not used yet. |

### Arguments

None set.

### Notes

- A number above 0 turns the limit on. 0 turns it off.

## Create Reward

- Kind: sub-action
- Code: 99522
- Type: TwitchCreateReward
- Category: Twitch/Rewards
- Needs: a Twitch login as the channel's owner (the broadcaster)

Creates a channel points reward. Streamerz can change and delete the rewards it made. This sub-action is Streamerz only: Streamer.bot does not have it.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| Title | `title` | The title. Variables are filled in first. |
| Cost | `cost` | What it costs, in points. |
| Description | `prompt` | What the reward says to viewers. Optional. |
| Viewer types a message | `inputRequired` | On: the viewer types a message to redeem it. |
| Colour | `backgroundColor` | The reward's colour, picked. Empty: the platform's own. |
| On | `enabled` | On: viewers can redeem it at once. |

### Arguments

- `rewardId`: the new reward's id.

## Delete Reward

- Kind: sub-action
- Code: 99523
- Type: TwitchDeleteReward
- Category: Twitch/Rewards
- Needs: a Twitch login as the channel's owner (the broadcaster)

Deletes a channel points reward. Twitch only lets an app delete the rewards it made. This sub-action is Streamerz only: Streamer.bot does not have it.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| Reward | `rewardId` | The reward, picked from the channel's rewards (or its id typed). |

### Arguments

None set.
