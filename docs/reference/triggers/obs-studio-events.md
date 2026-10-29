# OBS Studio / Events (triggers)

Triggers for the events OBS Studio sends over obs-websocket: every event, and the vendor events that plugins send.

## Event

- Kind: trigger
- Code: 14003
- Type: ObsEvent
- Category: OBS Studio/Events
- Needs: an OBS Studio connection (obs-websocket) set up in Streamerz

Fires for every OBS event that matches its filters. The scene, streaming and recording events also fire their own triggers.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| Connection (empty: any) | `connectionId` | The id of the OBS connection. Empty: any connection. |
| Event type (empty: any) | `eventType` | The OBS event type, such as `CurrentProgramSceneChanged`. Compared without case. Empty: any event. |

### Arguments

- `obs.id`, `obs.name`, `obs.host`, `obs.studioVersion`, `obs.websocketVersion`, `connectionId`, `connectionName`: as in Connected.
- `eventType` and `obsEvent.eventType`: the event type.
- Each key of the event's data, as is and with an `obsEvent.` prefix. For example `sceneName` and `obsEvent.sceneName`.

### When

When OBS sends an event on one of the subscriptions Streamerz asks for, and the connection and event type match the filters.

### Notes

- Each event fires once per matching trigger, so an action with two matching triggers runs twice.

## Vendor Event

- Kind: trigger
- Code: 14009
- Type: ObsVendorEvent
- Category: OBS Studio/Events
- Needs: an OBS Studio connection (obs-websocket) set up in Streamerz, and a plugin that sends vendor events

Fires when OBS gets a vendor event from a plugin.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| Connection (empty: any) | `connectionId` | The id of the OBS connection. Empty: any connection. |
| Vendor (empty: any) | `vendorName` | The name of the vendor that sent the event. Compared without case. |
| Event type (empty: any) | `eventType` | The vendor's event type. Compared without case. |

### Arguments

- `obs.id`, `obs.name`, `obs.host`, `obs.studioVersion`, `obs.websocketVersion`, `connectionId`, `connectionName`: as in Connected.
- `eventType` and `obsEvent.eventType`: `VendorEvent`.
- `obsVendorEvent.vendor`: the vendor name.
- `obsVendorEvent.event`: the vendor's event type.
- The event's data keys, as in Event.

### When

When OBS sends a `VendorEvent` and the connection, vendor and event type match the filters.

### Notes

- Each event fires once per matching trigger, so an action with two matching triggers runs twice.
