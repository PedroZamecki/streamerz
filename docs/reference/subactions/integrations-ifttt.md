# Integrations / IFTTT (sub-actions)

Sends events to IFTTT through the Maker Webhooks service.

## IFTTT Notification (Basic)

- Kind: sub-action
- Code: 8001
- Type: IFTTTNotificationBasic
- Category: Integrations/IFTTT
- Needs: an IFTTT Maker key, in the `iftttKey` or `key` argument

Sends an event to IFTTT with the text as `value1`. The event name and text are filled in with variables first.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| Event Name | `eventName` | Text. The IFTTT event to trigger. |
| Text | `text` | Text, several lines. Sent as `value1`. |

### Arguments

None set.

### Notes

- The key is not a setting. Streamerz reads it from the `iftttKey` argument, else from the `key` argument.
- With no key, nothing is sent. Streamerz writes a log line that the event was dispatched and stops there.
- An empty event name is an error, and nothing is sent.

## IFTTT Notification (Advanced)

- Kind: sub-action
- Code: 8002
- Type: IFTTTNotificationAdvanced
- Category: Integrations/IFTTT
- Needs: an IFTTT Maker key, in the `iftttKey` or `key` argument

Sends an event to IFTTT with named arguments. The event name and each argument value are filled in with variables first.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| Event Name | `eventName` | Text. The IFTTT event to trigger. |
| Arguments | `args` | Named texts. |

### Arguments

None set.

### Notes

- The first five arguments are sent as `Argument1Name` and `Argument1Value` up to `Argument5Name` and `Argument5Value`.
- All arguments are also sent together as `ArgumentsJson`.
- `value1`, `value2` and `value3` are sent as they are, when they are in the arguments.
- The key works the same as for the Basic notification.
