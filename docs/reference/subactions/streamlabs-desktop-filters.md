# Streamlabs Desktop / Filters (sub-actions)

Filters on sources in Streamlabs Desktop, picked at random. These run on a Streamlabs Desktop connection.

## Set State of a Random Filter

- Kind: sub-action
- Code: 411
- Type: Set State of a Random Filter
- Category: Streamlabs Desktop/Filters
- Needs: a Streamlabs Desktop connection

Picks one of a source's filters at random, and shows, hides or toggles it.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| Connection | `connectionId` | The Streamlabs Desktop connection id. Empty: the default connection. |
| Scene (empty: active) | `sceneName` | The scene's name. Empty: the active scene. |
| Source | `sourceName` | The source's name. |
| State | `state` | 0 Visible, 1 Hidden, 2 Toggle. |

### Arguments

None set.

### Notes

- The filter is picked again each time the action runs. A source with no filters does nothing.
