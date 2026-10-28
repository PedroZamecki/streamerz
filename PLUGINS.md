# Plugins

Streamerz runs WebAssembly plugins from the `plugins` folder next to `streamerz.json` (Plugins page,
Open folder). A plugin is one `.wasm` file; its file name is its id. Plugins are off until turned on in
the Plugins page, and one that's off isn't loaded. They run in an interpreter (wasmi) on Streamerz's
engine thread, with no access to files or the network except what Streamerz offers below, and each
event may run up to 200 million instructions before it's stopped.

`plugins/dice` is a complete example in Rust:

    cd plugins/dice
    cargo build --release --target wasm32-unknown-unknown
    cp target/wasm32-unknown-unknown/release/dice.wasm <streamerz folder>/plugins/

Any language that compiles to WebAssembly works; JSON is the only format.

## What a plugin exports

- `memory`
- `alloc(len: i32) -> i32`: room for `len` bytes Streamerz writes JSON into (the plugin owns it then).
- `manifest() -> i64`: its manifest, as JSON (see below).
- `on_event(ptr: i32, len: i32) -> i64`: an event, as JSON; its answer, or 0.

An `i64` answer is `ptr << 32 | len` of JSON in the plugin's memory, read at once.

## What it imports

`streamerz.call(ptr: i32, len: i32) -> i64`: a request, as JSON `{"op": ...}`. These answer at once:

| op | answer |
|---|---|
| `settings` | the plugin's settings, `{key: value}` |
| `global_get` `{name, persisted?}` | the global's value, or null |
| `now` | `{"unix": seconds}` |

These answer 0 and run right after the event returns:

| op | does |
|---|---|
| `log` `{text}` | a line in the log |
| `say` `{platform, text, bot?}` | sends to a chat (twitch, youtube, kick, tiktok) |
| `run_action` `{action, args?}` | runs an action, by name or id |
| `global_set` `{name, value, persisted?}` | sets a global |
| `trigger` `{event, args?}` | fires one of its triggers: the actions using it run |
| `http` `{id, method?, url, headers?, body?}` | a web request; the answer comes as an `http` event |
| `timer` `{id, ms, repeat?}` | a `timer` event after `ms` (again and again with `repeat`) |
| `timer_cancel` `{id}` | stops that timer |

## The manifest

    {
      "name": "Dice", "version": "1.0.0", "about": "What it does.",
      "settings": [{"key": "sides", "label": "Sides", "kind": "number", "default": 6}],
      "subactions": [{"name": "Roll dice", "about": "...", "fields": [{"key": "count", "kind": "number"}]}],
      "triggers": [{"name": "Rolled a six", "about": "..."}],
      "events": ["TwitchChatMessage"]
    }

A field's `kind` is `text`, `lines`, `number`, `bool` or `choice` (with `choices`; the value is the
index). `events` are the Streamer.bot trigger names it hears (`"*"` for all).

## Events

| kind | with |
|---|---|
| `start` | `settings`: it was turned on (or Streamerz started) |
| `settings` | `settings`: they changed |
| `trigger` | `name`, `args`: a trigger it listens to fired |
| `subaction` | `name`, `fields`, `args`: one of its sub-actions runs; answer `{"args": {...}}` to add variables, `{"stop": true}` to stop the action |
| `http` | `id`, `status`, `body` (or `error`) |
| `timer` | `id` |

In actions, its sub-actions are stored as type 99800 (`plugin`, `name` and the fields) and its triggers
as type 99801 (`plugin`, `event`); Streamer.bot skips both on import.
