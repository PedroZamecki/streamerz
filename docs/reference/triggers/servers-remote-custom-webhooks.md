# Servers & Remote / Custom Webhooks (triggers)

Triggers for custom webhooks received from outside Streamerz.

## Streamer Bot Custom Webook

- Kind: trigger
- Code: 34001
- Type: StreamerBotCustomWebook
- Category: Servers/Custom Webhooks
- Needs: the HTTP server on (Servers page)

Starts its actions when something POSTs to Streamerz's HTTP server at `/webhooks/<name>` (any site that
sends webhooks, through your own tunnel when it comes from the internet).

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| Webhook name | `webhookName` | Only this webhook (any case). Empty: any. |

### Arguments

- `webhook.name`: the name in the address.
- `webhook.body`: the body as text.
- A JSON body's values, flattened under `data` (`data.user.name`, `data.items[0]`).
- Plus `triggerId`, `triggerName`, `triggerCategory` and `eventSource` (`webhook`).

### When

On each POST to `/webhooks/<name>`; it answers 200.

### Notes

- A Streamerz extension: Streamer.bot's own Custom Webhooks are made on its website (a supporter perk)
  and forwarded to the app; the website and cloud are not part of Streamerz. Its trigger code and name
  are reused so the trigger reads the same.
- The HTTP server is off by default and refuses browser origins that aren't allowed (Servers page).
