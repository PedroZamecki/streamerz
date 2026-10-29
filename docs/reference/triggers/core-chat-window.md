# Core / Chat Window (triggers)

Triggers for the actions used in the multi-platform chat window.

## Chat Window Action

- Kind: trigger
- Code: 705
- Type: ChatWindowAction
- Category: Core/Chat Window
- Needs: nothing

Starts an action when a chat window action is used. The action's name filters which ones match. An empty name matches any action.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| Action Name | `actionName` | The action's name. Empty matches any action. |

### Arguments

None yet (see Notes).

### When

Not yet. Streamerz doesn't fire this trigger.

### Notes

- Streamerz doesn't fire this yet.
- The code that will fire it sets `platform`, `user`, `userId`, `text`, `messageId` and `actionName`.
