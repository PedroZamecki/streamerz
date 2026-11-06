# Core / Application Lifecycle (triggers)

Events for Streamerz itself: starting and exiting, toast clicks, and changes to its actions and commands. Only the start event fires.

## Streamerz Started

- Kind: trigger
- Code: 706
- Type: StreamerbotStarted
- Category: Core/Application Lifecycle
- Needs: nothing

Starts an action once each time Streamerz starts.

### Fields

None.

### Arguments

None beyond the ones every trigger gets: `triggerId`, `triggerName`, `triggerCategory` and `eventSource`.

### When

Once each time Streamerz starts.

## Streamerz Exiting

- Kind: trigger
- Code: 707
- Type: StreamerbotExiting
- Category: Core/Application Lifecycle
- Needs: nothing

Would start an action when Streamerz shuts down.

### Fields

None.

### Arguments

None yet.

### When

Not yet. Streamerz doesn't fire this trigger.

### Notes

- Streamerz doesn't fire this yet.

## Toast Activation

- Kind: trigger
- Code: 708
- Type: ToastActivation
- Category: Core/Application Lifecycle
- Needs: a Toast Notification sub-action

Starts its actions when someone clicks a toast that Toast Notification showed.

### Fields

None.

### Arguments

- `toast.id`, `toast.title`, `toast.message`, `toast.attribution`: what the toast showed.
- `toast.iconPath`: its icon path; `toast.localIconPath`: the same when it is a file here, else empty.
- Plus `triggerId`, `triggerName`, `triggerCategory` and `eventSource`.

### When

On a click on the toast, while Streamerz runs (a toast is listened to for an hour).

## Application Action Added

- Kind: trigger
- Code: 9001
- Type: ApplicationActionAdded
- Category: Core/Application Lifecycle
- Needs: nothing

Would start an action when an action is added to Streamerz.

### Fields

None.

### Arguments

None yet.

### When

Not yet. Streamerz doesn't fire this trigger.

### Notes

- Streamerz doesn't fire this yet.

## Application Action Updated

- Kind: trigger
- Code: 9002
- Type: ApplicationActionUpdated
- Category: Core/Application Lifecycle
- Needs: nothing

Would start an action when an action is changed.

### Fields

None.

### Arguments

None yet.

### When

Not yet. Streamerz doesn't fire this trigger.

### Notes

- Streamerz doesn't fire this yet.

## Application Action Deleted

- Kind: trigger
- Code: 9003
- Type: ApplicationActionDeleted
- Category: Core/Application Lifecycle
- Needs: nothing

Would start an action when an action is deleted.

### Fields

None.

### Arguments

None yet.

### When

Not yet. Streamerz doesn't fire this trigger.

### Notes

- Streamerz doesn't fire this yet.

## Application Import

- Kind: trigger
- Code: 9004
- Type: ApplicationImport
- Category: Core/Application Lifecycle
- Needs: nothing

Would start an action when actions are imported into Streamerz.

### Fields

None.

### Arguments

None yet.

### When

Not yet. Streamerz doesn't fire this trigger.

### Notes

- Streamerz doesn't fire this yet.

## Application Command Added

- Kind: trigger
- Code: 9005
- Type: ApplicationCommandAdded
- Category: Core/Application Lifecycle
- Needs: nothing

Would start an action when a command is added to Streamerz.

### Fields

None.

### Arguments

None yet.

### When

Not yet. Streamerz doesn't fire this trigger.

### Notes

- Streamerz doesn't fire this yet.

## Application Command Updated

- Kind: trigger
- Code: 9006
- Type: ApplicationCommandUpdated
- Category: Core/Application Lifecycle
- Needs: nothing

Would start an action when a command is changed.

### Fields

None.

### Arguments

None yet.

### When

Not yet. Streamerz doesn't fire this trigger.

### Notes

- Streamerz doesn't fire this yet.

## Application Command Deleted

- Kind: trigger
- Code: 9007
- Type: ApplicationCommandDeleted
- Category: Core/Application Lifecycle
- Needs: nothing

Would start an action when a command is deleted.

### Fields

None.

### Arguments

None yet.

### When

Not yet. Streamerz doesn't fire this trigger.

### Notes

- Streamerz doesn't fire this yet.
