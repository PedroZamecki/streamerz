# Core / Text & Data (sub-actions)

Small tools for text and numbers: regex, changing a text, splitting, JSON, counters, a cooldown gate and durations.

## Regex Match

- Kind: sub-action
- Code: 99780
- Type: TextRegexMatch
- Category: Core/Text & Data
- Needs: nothing

Looks for a pattern in a text and gives what it found: the groups, numbered and by name. This sub-action is Streamerz only: Streamer.bot does not have it, and skips it in an export.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| Text | `input` | The text to work on, up to 100000 characters. Variables are filled in first. |
| Pattern (regex) | `pattern` | The pattern, as in the If/Else Regex Match (.NET style, without look-behind or back-references). |
| Ignore upper and lower case | `ignoreCase` | On: capitals and small letters count the same. |
| Save the result in (variable name) | `variableName` | The name of the variable that gets the result. Empty: `match`. |

### Arguments

- `match[0]` is the whole match (the variable's name replaces `match`), `match[1]`... the groups, then a variable for each named group.
- `matchFound`: true when it matched (the variable's name plus `Found`).
- `match.count`: how many groups.

### Notes

- The same matching as the If/Else sub-action's Regex Match.

## Regex Replace

- Kind: sub-action
- Code: 99781
- Type: TextRegexReplace
- Category: Core/Text & Data
- Needs: nothing

Replaces what a pattern matches in a text and gives the new text. This sub-action is Streamerz only: Streamer.bot does not have it, and skips it in an export.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| Text | `input` | The text to work on, up to 100000 characters. Variables are filled in first. |
| Pattern (regex) | `pattern` | The pattern, as in the If/Else Regex Match (.NET style, without look-behind or back-references). |
| Replace with ($1 is the first group) | `replacement` | What takes the match's place. `$1` is the first group, `${name}` a named one. |
| Ignore upper and lower case | `ignoreCase` | On: capitals and small letters count the same. |
| Every match (off: the first) | `all` | On: every match is replaced. Off: only the first. |
| Save the result in (variable name) | `variableName` | The name of the variable that gets the result. Empty: `replaced`. |

### Arguments

- The variable named (default `replaced`) holds the new text.

## Change Text

- Kind: sub-action
- Code: 99782
- Type: TextTransform
- Category: Core/Text & Data
- Needs: nothing

Changes a text: its case, its spaces, its order, its words, or measures it. This sub-action is Streamerz only: Streamer.bot does not have it, and skips it in an export.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| Text | `input` | The text to work on, up to 100000 characters. Variables are filled in first. |
| how | `how` | Upper case, lower case, Title Case, trimmed, reversed, extra spaces taken out, the first or last word, URL encoded, or its length in characters. |
| Save the result in (variable name) | `variableName` | The name of the variable that gets the result. Empty: `text`. |

### Arguments

- The variable named (default `text`) holds the result.

## Split Text

- Kind: sub-action
- Code: 99783
- Type: TextSplit
- Category: Core/Text & Data
- Needs: nothing

Splits a text into its parts (a list typed with commas, a line each) and picks one at random. This sub-action is Streamerz only: Streamer.bot does not have it, and skips it in an export.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| Text | `input` | The text to work on, up to 100000 characters. Variables are filled in first. |
| Split at | `separator` | What to split at. Empty: a comma. `\n` is a new line. |
| Save the result in (variable name) | `variableName` | The name of the variable that gets the result. Empty: `part`. |

### Arguments

- `partCount`: how many parts (up to 200, empty ones left out).
- `part0`, `part1`...: each part.
- `partRandom`: one of them at random. The variable's name replaces `part`.

## Read JSON Value

- Kind: sub-action
- Code: 99784
- Type: JsonQuery
- Category: Core/Text & Data
- Needs: nothing

Reads one value out of a JSON text, by its path. This sub-action is Streamerz only: Streamer.bot does not have it, and skips it in an export.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| Text | `input` | The JSON text, such as what Fetch URL read. |
| Path (a.b[0].c) | `path` | Where the value is: `data.items[0].name`. Empty: the whole thing. |
| Save the result in (variable name) | `variableName` | The name of the variable that gets the result. Empty: `json`. |

### Arguments

- The variable named (default `json`) holds the value: text as text, a number or true or false as typed, an object or list as JSON.
- `jsonFound`: true when the path is there (the variable's name plus `Found`).

## Counter

- Kind: sub-action
- Code: 99785
- Type: CounterChange
- Category: Core/Text & Data
- Needs: nothing

Adds to, subtracts from, sets or clears a number kept as a global, in one step. This sub-action is Streamerz only: Streamer.bot does not have it, and skips it in an export.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| Counter name (a global) | `name` | The global that holds the number. |
| operation | `operation` | Add, subtract, set to the amount, or back to 0. |
| Amount | `amount` | How much. Empty: 1. |
| Keep it after a restart | `persisted` | On: the global stays after a restart. |
| Save the result in (variable name) | `variableName` | The name of the variable that gets the result. Empty: `counter`. |

### Arguments

- The variable named (default `counter`) holds the new number.

### Notes

- The global's own triggers (Global Variable Updated) fire, as when Set Global Variable changes it.

## Cooldown Gate

- Kind: sub-action
- Code: 99786
- Type: CooldownGate
- Category: Core/Text & Data
- Needs: nothing

Lets an action through once in a while: the first time it passes, and the rest of the time it stops the action. This sub-action is Streamerz only: Streamer.bot does not have it, and skips it in an export.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| Gate name | `key` | The gate's name. Two gates with the same name are the same gate. |
| Duration | `duration` | How long it stays closed after one passes. |
| Each person has their own | `perUser` | On: each person has a gate of their own (by `%userId%`). |

### Arguments

- `cooldownLeft`: the seconds until it opens (0 when it let this one through).

### Notes

- It stops the whole action, like Break, so put it first.
- The gates are kept in memory: a restart opens them all.

## Format Duration

- Kind: sub-action
- Code: 99787
- Type: FormatDuration
- Category: Core/Text & Data
- Needs: nothing

Writes a number of seconds as time: a clock, short, or in words. This sub-action is Streamerz only: Streamer.bot does not have it, and skips it in an export.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| Seconds | `seconds` | A number of seconds. |
| style | `style` | `1:02:03`, `1h 2m 3s`, or `1 hour, 2 minutes, 3 seconds`. |
| Save the result in (variable name) | `variableName` | The name of the variable that gets the result. Empty: `duration`. |

### Arguments

- The variable named (default `duration`) holds the text.

## List

- Kind: sub-action
- Code: 99788
- Type: ListChange
- Category: Core/Text & Data
- Needs: nothing

Keeps a list in a global and works on it in one step: add, remove, check, pick one or join them all. This sub-action is Streamerz only: Streamer.bot does not have it, and skips it in an export.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| List name (a global) | `name` | The global that holds the list. |
| operation | `operation` | Add, remove, check it has, get the item at a position, get one at random, count, empty it, take the first out, or get them all. |
| Item (a position from 0, or a separator for all) | `value` | The item to add, remove or look for; the position to get (from 0, negative from the end); or what goes between them all (empty: a comma). |
| Keep it after a restart | `persisted` | On: the global stays after a restart. |
| Save the result in (variable name) | `variableName` | The name of the variable that gets the result. Empty: `list`. |

### Arguments

- The variable named (default `list`) holds the item found, removed or taken, the count, or them all.
- `listFound`: true when there was something to give (the variable's name plus `Found`).
- `listCount`: how many items the list has now (the variable's name plus `Count`).

### Notes

- Remove and check compare without case, so `Ana` finds `ana`.
- A list keeps up to 1000 items: the oldest go first.

