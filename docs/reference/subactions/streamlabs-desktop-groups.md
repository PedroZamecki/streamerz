# Streamlabs Desktop / Groups (sub-actions)

Groups of sources in a Streamlabs Desktop scene. These run on a Streamlabs Desktop connection, and the scene is the active one when Scene is empty.

## Set Random Group Source Visible

- Kind: sub-action
- Code: 402
- Type: Set Random Group Source Visible
- Category: Streamlabs Desktop/Groups
- Needs: a Streamlabs Desktop connection

Shows one of the group's hidden sources, picked at random.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| Connection | `connectionId` | The Streamlabs Desktop connection id. Empty: the default connection. |
| Scene (empty: active) | `sceneName` | The scene's name. Empty: the active scene. |
| Group | `groupName` | The group's name. Variables are filled in first. |
| Display | `display` | 0 Both, 1 Horizontal, 2 Vertical. |

### Arguments

None set.

### Notes

- Nothing happens when every source in the group is already shown.

## Hide Group's Sources

- Kind: sub-action
- Code: 403
- Type: Hide Group's Sources
- Category: Streamlabs Desktop/Groups
- Needs: a Streamlabs Desktop connection

Hides every visible source in a group.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| Connection | `connectionId` | The Streamlabs Desktop connection id. Empty: the default connection. |
| Scene (empty: active) | `sceneName` | The scene's name. Empty: the active scene. |
| Group | `groupName` | The group's name. Variables are filled in first. |
| Display | `display` | 0 Both, 1 Horizontal, 2 Vertical. |

### Arguments

None set.
