# OBS Studio / Filters (sub-actions)

Sub-actions that turn OBS filters on or off without naming the filter.

## Set State of a Random Filter

- Kind: sub-action
- Code: 40
- Type: ObsSetRandomFilterState
- Category: OBS Studio/Filters
- Needs: an OBS connection (the one picked, else the first connected one)

Picks one filter of a source at random, then enables, disables or toggles it.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| Connection | `connectionId` | The OBS connection, by id or name. Empty uses the first connected one. |
| Scene | `sceneName` | Stored with the action, but not read. |
| Source | `sourceName` | The source whose filters are chosen from. Variables are filled in first. |
| State | `state` | One of Enable, Disable, Toggle, stored as 0, 1, 2. |

### Arguments

None set.

### Notes

- If the source has no filters, nothing changes.
