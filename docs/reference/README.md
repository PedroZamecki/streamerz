# Streamerz reference

A page for every trigger and sub-action Streamerz knows, written from what Streamerz itself does. It is built
into the app: the MCP server's `docs`, `docs_search` and `describe` tools answer from it, and a test fails
when a trigger or sub-action has no page.

- `subactions/`: what an action does, one file per category (`twitch-chat.md`, `obs-studio-scenes.md`...).
- `triggers/`: what starts an action, one file per category.

## How a page is written

Each file starts with `# Category (sub-actions)` or `# Category (triggers)` and a line on what the category
is about. Then one section per item, in this shape:

```markdown
## Send Message

- Kind: sub-action
- Code: 10
- Type: Message
- Category: Twitch/Chat
- Needs: a Twitch login (the bot, else the broadcaster)

Sends a message to the Twitch chat. Variables like `%user%` in the text are filled in first.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| Message | `message` | The text to send. |
| Send as bot | `useBot` | On: the bot account sends it. |

### Arguments

None set.

### Notes

- A message over 500 characters is cut by Twitch.
```

- `Kind` is `sub-action` or `trigger`; `Code` is the type code in `src/types.rs`; `Type` its name there.
- `Needs`: the login, role, connection or app it needs (`nothing` when it needs nothing). A moderator can do
  some things on someone else's channel, only the owner others: say which (see `src/roles.rs`).
- For a trigger, `### Arguments` lists what the actions it starts get (`user`, `message`...), and a `### When`
  section says exactly when it fires.
- For a sub-action, `### Arguments` lists what it adds for the sub-actions after it, or "None set.".
- Plain English, short sentences, as a person would write. No em or en dashes. Nothing that isn't true of
  Streamerz: when something isn't done yet, say so plainly under Notes.
