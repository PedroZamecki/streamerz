# Scenarios

A scenario tests actions without the app's window, a login or a stream: it describes some actions (and
commands, globals or any other setting), then steps that fire triggers, send chat or run actions, then
what must come of it. It runs on Streamerz's real engine in a fraction of a second. Nothing is sent
anywhere: chat that would go out is kept and compared instead.

Run them:

- `cargo test scenarios`: every file in `tests/scenarios` (CI runs this on every pull request).
- `Streamerz --scenario my-tests.json` (or a folder): from the app itself, on your own actions. It prints
  `ok` or `FAIL` and why for each one, and exits with the number that failed. Your settings are never
  touched: it runs on a scratch copy.
- The MCP server's `run_scenario` tool: the same, given inline, for an assistant building actions.

## The file

One scenario, or a list of them:

```json
{
  "name": "a follow is thanked",
  "config": {"commands": [{"id": "c-hi", "command": "!hi"}]},
  "actions": [
    {"name": "thanks", "triggers": [{"type": 101}],
     "subActions": [{"type": 35001, "text": "thanks %user%"}]}
  ],
  "steps": [
    {"fire": 101, "args": {"user": "ana"}, "ran": 1},
    {"chat": {"platform": "kick", "user": "bo", "text": "!hi", "role": "moderator"}},
    {"run": "thanks", "args": {"user": "cy"}},
    {"wait": 500}
  ],
  "expect": {
    "chat": ["thanks ana", {"platform": "kick", "text": "thanks cy"}],
    "log": ["first this", "then this"],
    "notLog": ["never this"],
    "globals": {"follows": 3}
  }
}
```

- `actions`: as Streamer.bot and Streamerz store them. Ids, `enabled`, queue and the ids of triggers and
  sub-actions can be left out. The reference (`docs/reference`, or the MCP `docs` tool) lists each
  trigger's and sub-action's code and fields.
- `config`: any setting, on top of the defaults: `commands`, `globals`, `timers`, `userGroups`...
  A command listens to every platform unless it says `sources`.
- Steps:
  - `fire`: a trigger's code or name, with `args` as its platform would send them.
  - `chat`: a chat message (`platform`, `user`, `text`, `role`: viewer, subscriber, vip, moderator or
    broadcaster), run through the commands.
  - `run`: an action by name or id, with `args`.
  - `wait`: how long to let the actions run before the next step, in milliseconds (default 120).
  - `ran`: on a `fire` or `chat` step, how many actions must have started.
- `expect`:
  - `chat`: every message sent, in order: the text, or the fields to compare (`platform`, `text`, `bot`).
  - `log`: lines that must appear in the log, in this order (each is a part of a line). A Log sub-action
    (type 1020) is the easy way to say something from an action.
  - `notLog`: what must never appear.
  - `globals`: values of globals at the end (persisted or not); numbers compare as numbers.
