# Make automation easier to build

Investigation and proposed design, 2026-11-05. These are organized implementation ideas, not a claim
that the features below have been implemented or tested against live services.

## The experience to aim for

A trigger answers: **When should this run?** A sub-action answers: **What should happen?**
The editor should ask the fewest understandable questions needed to answer those two things.

For example: When someone sends a Rose on TikTok, after their combo finishes, reply to them.
The user picks Rose, picks Completed combo, and writes a reply using the offered Sender name value.
They should not need a gift ID, a repeat-end expression, an argument dictionary or external docs.

Keep precise control available. Simple defaults, named choices, optional conditions, and Help should
make the same underlying features easier to understand. Preserve existing exports and saved behavior.

## What was examined

The [complete inventory](AUTOMATION-INVENTORY.csv) contains one row for every registered trigger and
sub-action: code, category, title, form state, fields, reference, engine support declaration and review
ideas. It was regenerated from the compiled field definitions and compared with reference sections.
It covers 497 triggers and 454 sub-actions. The earlier audit's 455 sub-actions counted the reference
README's example as a real entry; the generator excludes that example.

Every reference section and field schema was inventoried. Shared authoring, matching, dispatch,
result handling, templates and representative connector paths were traced in source. This is not
an item-by-item live-service certification or proof that every documented field works.

The initial audit below describes the state before implementation. The CSV is regenerated as work
lands, so its current rows can differ from these initial counts and findings.
The implemented shared controls, gift matching, results and recipes are explained in
[Build an automation](AUTOMATION.md). Remaining live-service checks are tracked in PLAN.md.
The field tables distinguish three states: a form, an explicit no-settings form, and JSON fallback.
There are 171 trigger and 130 sub-action JSON fallbacks. All sub-actions declared supported by
`engine::supported` have a form or explicit no-settings route. The declaration includes 292 registered
sub-actions; it does not prove successful execution. Unsupported types and internal branch types
are also in the registry. A missing form is not automatically a request to implement an excluded app.

Reproduce the inventory:

```sh
cargo test --profile fast docs::tests::dump_types -- --ignored --exact
python3 -P dev/automation-audit.py
```

The generator reads form states and availability from the compiled Rust exporter, including computed
no-settings cases. The dump must be regenerated after changing field definitions. CSV idea tags are review candidates,
not confirmed defects. Missing section headings do not prove the information is absent elsewhere.

## Findings that should shape the design

| Finding | Evidence | Consequence for the user |
| --- | --- | --- |
| The trigger editor sends everyone to Streamer.bot docs. | `actions.trigger_hint`, `ui/actions.rs::edit_trigger` | The dialog gives neither instructions nor a truthful source for Streamerz extensions. |
| Help, picker descriptions, form fields and runtime code are separate sources. | `docs.rs`, `ui/picker.rs`, `ui/forms.rs`, `mcp.rs::describe` | The same item can be explained differently in different places. |
| The picker enumerates all types without checking runtime support. | `ui/picker.rs::sub_action_pick_items` | Users can configure an operation the engine will skip. The existing action list does mark unsupported entries, but only after selection. |
| Some unsupported types even have complete forms. | Compiled inventory and `engine::supported` | A polished form can imply a feature is ready when it is not. There are 27 such registered sub-actions. |
| No-settings and no-form are conflated in MCP. | `mcp.rs::describe`, empty fields and its note | A client cannot tell whether configuration is unnecessary or missing. |
| Gift and most other platform events have little filtering. | `tiktok.rs::handle_tiktok_event`, `youtube.rs::yt`, `kick.rs::kick` | Users must build extra conditions inside actions, or cannot express the rule in the trigger editor. Kick reward matching is an existing exception to reuse. |
| Amount bounds have a surprising legacy meaning. | `donations.rs::in_range` | A maximum alone means exactly that amount, rather than at most that amount. New controls need explicit operators. |
| Catalog suggestions display and save names. | `ui/forms.rs`, `Suggest` and `Widget::Name`; `catalog.rs` | Name lookup helps, but an exact selector needs to retain identity when names change or collide. |
| Numeric inputs and absent values need care. | `ui/forms.rs::form_validated`, `collect_draft` | Inputs show missing numbers as zero and missing booleans as false, then write them back. This can differ from runtime defaults or destroy expression values. This is a schema review risk to test per field, not proof every form changes behavior. |
| Some visible fields are documented as unused. | `reference/subactions/core-file-io.md`, `twitch-channel.md` | Users can set options that have no effect. Trace the current engine before removing controls or changing these claims. |
| Completing an action reports success unconditionally. | `engine.rs::execute`, Raw ActionCompleted event | Completion does not establish that an external request succeeded. A useful result view must distinguish these outcomes. |
| Generic trigger testing skips connector matching. | `servers.rs`, FireTrigger calls `fire` with an always-true matcher | A passing test can hide a broken gift, reward or scene filter. |
| Previous completeness reports are stale. | `FEATURE-AUDIT.md` versus current `engine/midi_in.rs`, `engine/watchers.rs`, `engine/processes.rs` | Old missing-feature claims must be rechecked. Current code has those input tasks; real hardware and platform limits remain separate questions. |

## Shared ideas, in implementation order

These IDs are the legend for `review_ideas` in the inventory. Existing behavior always comes first;
adding new operations is a separate scope decision.

### A1. Tell the user whether the item can work

In the picker, distinguish an available operation, a supported operation needing setup, an operation
unavailable on this OS, and a type preserved only for compatibility. Keep compatibility-only entries
out of the ordinary add flow, with an explicit way to inspect them. Imported ones remain visible,
preserved and honestly explained. A disconnected supported operation remains configurable offline.

Show a specific setup action: Connect OBS, Choose a TikTok channel, or Log in to Spotify.
Do not guess permissions from a local role table. Show known requirements, then report the service's
actual answer. A form is never evidence that the connector implements an operation.

### H1. Put useful Help inside every editor

Start with one sentence: what this does. Show the common controls, a short example, and a Help action
for details. Help explains prerequisites, each field, defaults, units, output values and limitations.
No-settings triggers show When and Available values instead of an empty JSON object.

Use one reference for the picker, selected-item details, form Help, MCP and documentation search.
Search should recognize ordinary tasks such as reply, gift, scene, song and wait, including aliases
for API terms. Keep canonical names and numeric codes for compatibility, not as required user input.

### H2. Check explanations against behavior

Require documented fields, accepted values, defaults and examples for supported entries. Check
configuration keys against the actual form schema, and verify the behavior with fixtures. Distinguish
unsupported entries and internal branch types from ordinary executable operations.

Keep concise localized summaries separate from longer reference text only where needed, with a
stable type identity linking them. Do not silently mix untranslated Markdown into translated controls.
Document the language fallback if long-form translations are incomplete.

### E1. Give every supported type an intentional editing experience

Each type declares Form, No settings, or Unavailable with a reason. JSON is an advanced option.
No settings is a useful explanation, not an implementation fallback. Unsupported imports may expose
their preserved JSON with an explicit explanation. Shared form and dialog blocks own this behavior.

### E2. Show fields only when they matter

Common fields first. Optional conditions are added with Add condition. Replies and advanced settings
follow the existing task/rules/replies pattern. An operation choice controls which fields appear.
Preserve hidden imported values, and explain preserved options that Streamerz does not use.

For Spotify Song Request, the default task is Request a song, using command input and group settings.
Do not ask the user to fill all 34 fields. Existing tabs help; conditional fields and inherited-setting
indicators would make it clearer which values they actually need to change.

### E3. Edit values in the units people use

Durations accept values such as 30 seconds, 5 minutes or 1:30 where unambiguous; the UI identifies
the unit and stores the existing wire representation. Volume is a percentage with its valid range.
Colors have a swatch and picker. Files and folders have Browse. Numeric bounds are validated before
saving. Blank, zero, disabled and unlimited are distinct when the runtime treats them differently.

An unchanged field must keep its original JSON type and missing-value behavior. Expression-capable
values remain editable expressions; opening a form must not turn `%duration%` into zero.

### E4. Offer meaningful choices

Show Start, Stop and Toggle, not an integer or a generic State. Offer operation-specific choices
such as Muted/Unmuted, Completed/Progress, or Once/Repeat. Explain Toggle in a reusable template:
it depends on the current state; Set visible is more predictable when that is what the user wants.
Keep stored indexes unchanged and test each mapping against its engine branch.

### C1. Pick known objects by name while storing their identity

Reuse catalog suggestions for rewards, scenes, sources and MIDI devices. Add room gifts, OBS filters,
Spotify devices/playlists and other catalogs only when the connector can retrieve them reliably.
Scope catalogs by connection, channel, room or account as appropriate. A source list follows the
selected scene; a filter list follows the selected source. Distinguish duplicate names.

Retain a saved selection when it is unavailable. Offer typed IDs and expressions as advanced input.
Show loaded-empty, failed, loading and stale states separately. Refresh on relevant events or user
request, using existing connection tasks. Do not download every image merely to populate a list.

### V1. Insert event values without memorizing variables

An Insert value action offers Sender name, Message, Gift, Amount, Reward, Current song and the outputs
of preceding steps, with examples and the underlying variable name available. Include group settings
and saved variables with clear scope. A user can still type expressions directly.

Multiple possible triggers provide different values. Mark values that are only present for some
triggers. Branches and nested actions need explicit availability rules; do not promise data-flow
inference before it is implemented. Preview with sample values and identify it as a preview.

### T1. Let the trigger express the ordinary rule

Offer only meaningful filters for that event: a particular gift or reward, subscription type, sender,
amount/currency, scene, source or connection. Use named operators: Any, Exactly, At least, At most,
Between. Do not expose a large generic condition builder by default.

All chosen conditions on one trigger must pass. Explain that separate triggers are alternatives.
Current dispatch queues once per matching trigger, so overlapping triggers can run an action twice;
show that in the rule summary. Any future deduplication would be a deliberate behavior change.
Advanced arbitrary-argument conditions can come later, sharing existing comparison semantics.

### T2. Explain frequency and let users test matching

Describe whether an event occurs per message, per recipient, per batch, per combo, on state change,
or after reconnect. Offer appropriate choices where supported. Test a matching and nonmatching
sample through the same matcher as live events. Explain why a sample did not match.

Separate a matcher preview from running the action. A preview sends nothing; running an action can
perform real effects and must be labeled accordingly. Keep live observation deliberate and bounded,
with values redacted where needed. Do not retain every raw platform payload for this feature.

### R1. Show useful outputs and failures

A step explains what values it adds, with examples. Execution history distinguishes Completed,
Skipped, Failed and outcome unknown where a remote response was lost. Tie errors to the failed step
and show the actual service response in details. Clearing stale outputs after a failed read prevents
later steps from presenting an old value as a new result.

Preserve established argument names. Add consistent result metadata internally before choosing new
public result fields, because a generic `success` argument can collide with existing actions.
Do not infer success merely because a task finished. Never retry an ambiguous chat send automatically.

### R2. Offer a useful recipe before adding another operation

Use the existing action groups and templates for multi-step tasks: thank a gifter, answer a command,
change an OBS source temporarily, publish song information, or update a countdown. Ask only for the
unknowns and show the ordinary actions created. Do not add a new serialized type for every recipe.

Add a new trigger only for a distinct observable event users need. Add a new sub-action only for a
distinct operation or calculation that existing steps cannot express simply. A new operation must
have a concrete source, clear inputs/outputs, failure behavior, MCP access and a test.

## Concrete ideas by family

Every inventory category belongs to one of these work areas. The CSV retains every individual type
so small or compatibility-only families do not disappear from the review.

| Work area | Simplest trigger choice | Simplest sub-action or workflow | Boundary to preserve |
| --- | --- | --- | --- |
| Chat, all four platforms | A message arrives, optionally containing text or from a chosen sender. | Reply to this message, or send a message to a chosen channel; offer Sender name and Command input. | Show the sending account and platform. Cross-platform sending is explicit; absent event context needs a destination. |
| Commands and voice commands | Choose an existing command, with Create command alongside it. | Set up `!hello` and its reply together; edit shared permissions/cooldowns in the command. | Reuse command matching rather than introducing another regex or cooldown implementation. |
| First words, present users and groups | First message this session, or a membership change in a selected group. | Welcome a user, add/remove a user, choose a random eligible participant. | A chatter is not every live viewer. State retention and reset rules. |
| Twitch follows, subscriptions and raids | Subscription kind, gifted or purchased, optional tier/count; incoming or outgoing raid. | Thank the sender or give a shoutout with the triggering user already offered. | Gift-batch and recipient events can overlap. Never imply one event equals one financial contribution. |
| Twitch Bits, power-ups, charity and hype trains | Named event, optional amount or milestone where data exists. | Update an overlay or countdown using the offered event amount. | Separate paid amount, progress and final totals; document unavailable legacy events. |
| Twitch and Kick rewards | A selected reward is redeemed. | Complete/refund or update that reward using event context. | Name is display data, ID is identity. Only show operations supported by the relevant platform. |
| TikTok gifts and coins | Gift: Any or selected gift; Deliver: Completed combo or Every update. | Thank this gifter; offer gift name, unit value, final quantity and combo total. | Dynamic room catalog, unknown gifts, legacy behavior and combo overcounting, see the dedicated audit. |
| TikTok follows, likes, joins, shares and subscriptions | Event plus the few applicable sender/count filters. | Reply or update an overlay using offered values. | Likes and viewer updates are frequent; a threshold crossing is an optional distinct rule, not repeated `>=` matches. |
| Kick subscriptions and channel events | Named event with count/subscription filters where available. | Thank, reply, update title/category or choose a reward. | Public reading and authorized writing have different requirements. |
| YouTube Super Chats, stickers, memberships and gifts | A named event, optionally amount plus its reported currency or membership kind. | Thank the actual sender or update an overlay. | Paid currency, Jewels and membership counts are different units. Do not imply automatic conversion. |
| Stream online/offline, viewers and statistics | Stream starts/ends or a value changes; choose channel. | Publish current statistics or a stream-end summary. | Disconnect is not stream ended. If adding thresholds, define crossing, first sample and reset behavior. |
| Chat moderation, restrictions, warnings and shared chat | Named state/event, optionally selected user or restriction. | Timeout the triggering user for 5 minutes with an optional reason; explicitly choose other targets. | Clear target/platform/account. Shared-chat source and broadcaster context must be preserved. |
| Polls, predictions, ads and clips | A poll/prediction changes phase, an ad starts, or a clip becomes available. | Build options as rows, enter a duration, choose an outcome by title and preview the result. | Do not claim fields work if the connector ignores them. Destructive/live operations stay explicit. |
| OBS scenes, sources, filters, audio, media and replay | A selected connection changes scene/state, optionally a specific scene. | Choose connection, scene and source, then Show/Hide, Mute/Unmute or Play/Pause. | Scope names to the connection; distinguish scene items from global inputs. Keep Raw request advanced. |
| Streamlabs Desktop | Connection or selected scene changes. | The same task vocabulary as OBS where supported, with Desktop-specific choices. | Reuse UI, not assumed OBS protocol parity. Unsupported state choices must be explained. |
| Spotify player, library, playlists and browsing | Playback changes or song requests change state; select source if needed. | Pick song/device/playlist by name or link; offer Current song; use percent and duration controls. | Explain local Windows playback observation versus API operations. Only expose changes that can actually be observed. |
| Spotify song requests | Request accepted/refused, queue changed or song changed. | Request a song from command input using group rules; show only overridden settings. | Reuse general commands, groups and cooldowns. Existing dedicated operations should share the same rules. |
| Donations, memberships, shops and charity integrations | Named event; optional amount, reported currency, product or membership where supplied. | Thank the sender or update an overlay/countdown. | An order is not always a payment. Show which event is paid, refunded, test or recurring when known. Do not invent absent fields. |
| Files, folders and file tail | Browse to a file/folder; choose event and optional filename filter. | Browse to file, then Read, Append or Replace; choose where the result goes. | Explain existing content, rotation, recursive scope and repeated filesystem notifications. Report failures. |
| Processes, shortcuts, mouse and clipboard | Choose an application, record a shortcut, choose a mouse button, or clipboard changes. | Run a program with arguments and a working folder; insert its returned values. | OS permissions and support are visible. Do not read clipboard or open input devices just to fill an editor. |
| MIDI and speech | Choose device and Learn from the next input; use note/controller names. | Choose device, note and duration; offer the spoken phrase as a value. | Learning is temporary. Close devices/listeners when no enabled automation needs them. Current hardware limits are explicit. |
| Timers and countdowns | Every chosen duration, or the existing timer fires. | Start/stop a selected timer; add time/reset a chosen countdown using human durations. | Countdown lifecycle events are new scope unless already emitted. Preserve downtime and maximum-time behavior. |
| Variables, arguments and logic | A selected saved value changes. | Save/read a named value; If Gift equals Rose; choose a comparison and typed value. | Show event-only, saved, per-user and group scope. Preserve false, zero, empty, null and missing distinctly. |
| Actions, groups, queues and execution | Existing event, command or timer runs a named action. | Run action, enable/disable group, pause queue; select objects by name. | Branch containers stay structural, not ordinary addable operations. Explain nested execution and queue ordering. |
| Quotes, credits, users and pronouns | A supported record/group event, clearly named. | Pick/add a quote, reset credits, enrich the triggering user. | Do not present a stored record or credit log as an existing points economy or a complete viewer list. |
| Overlays and group files | An existing platform event or chosen custom event. | Choose an overlay and named value to publish; copy its OBS URL. | Keep custom JSON available. Reuse existing topics and group files; no background connection without a listener. |
| Webhooks, HTTP, UDP, sockets, remote instances and signals | Choose connection/custom event by name, with optional event filter. | GET an address into a named result, send a webhook, or send/wait for a named signal. | Form fields explain payloads, timeout and outputs. Arbitrary bodies and raw protocols stay advanced. |
| Stream Deck and compatibility Decks | A supported plugin button action, with a detected/learned button if feasible. | Change a known button's title/state. | Stream Deck plugin protocol and Streamer.bot website Decks are different features. Do not confuse their support. |
| Startup, shutdown, notifications and settings | Supported lifecycle or notification click, labeled Streamerz. | Show a notification; reset the selected session statistic. | Some registered lifecycle types never fire. Confirm wiring before offering them. |
| Sound, C#, Speaker.bot, Elgato extras, Meld, PolyPop and excluded integrations | Explain preserved type and current limitation. | Preserve imported data; suggest a real supported alternative only when equivalent. | Do not implement excluded integrations or promise audio playback as an incidental UI fix. A new scope decision is needed. |

## First workflows to make concrete

These are acceptance examples, not new automatic side effects:

1. **Thank for a specific TikTok gift.** Select the channel, Rose and Completed combo. Write a reply
   with Sender name and Gift quantity inserted. A different gift does not match. A new catalog gift
   appears after refresh without an app release.
2. **A reward controls an OBS source.** Select the detected reward, the OBS connection and source,
   then Show. Add a 10-second delay and Hide if desired, with overlap/queue behavior explained.
3. **Add time for a donation.** Choose a paid donation event, its reported currency, and 1 minute per
   currency unit. Preview an example. Other currencies do not match; there is no guessed exchange rate.
4. **A subscriber gets a thank-you.** Pick purchased or gifted subscription events deliberately.
   Preview sender, recipient and count so gift-batch handling is understandable.
5. **A command queues music.** Enter `!sr`, choose allowed users, and use command input as the song.
   Spotify setup and rejection reasons are visible; no manual argument names are needed.
6. **Change a playlist.** Select a playlist by name and Add current song. Show the output and error
   when Spotify refuses it, rather than presenting completion as success.
7. **Read an updated text file.** Browse to the file, use File changed, read it into a named value,
   and update an overlay. Explain repeated write notifications and the file-read failure state.
8. **Use a MIDI button.** Select Learn, press the button, inspect its device/channel/note, and assign
   an OBS operation. Learning stops on save or cancel.
9. **Create a poll.** Enter the question, add answer rows and choose a duration. Preview shows the
   exact platform/account and choices. A real test launch is explicitly a live operation.
10. **Understand a failed action.** Run with sample data, see which triggers match, and inspect which
    step failed and why. A timeout does not falsely say the remote service did nothing or succeeded.

## Implementation packages

### 1. Honest availability and Help

Deliver A1, H1, H2 and E1 through shared picker/form/dialog helpers. Correct the generic external-docs
claim and stale application names. Separate form state from execution support in MCP. Audit docs
against current code rather than copying old FEATURE-AUDIT claims. Avoid allocating every reference
entry to display one Help page; use compiled text with a direct lookup.

Acceptance: each selectable supported type has an intentional editor and local Help; compatibility
types show a reason before selection; no-settings examples stay easy; UI and MCP agree. Validate
screens narrow/wide and keyboard paths. Do not broaden the supported-integration list.

### 2. Gift configuration and shared exact selectors

Implement C1 and T1/T2 first for TikTok gifts, using the signed room gift catalog. Then use the same
identity-preserving selector for rewards and connection-scoped scene/source catalogs. Keep loading
and unavailable states meaningful. See [the TikTok audit](TRIGGER-DOCUMENTATION-AUDIT.md).

Proposed default for new gift triggers: Completed combo; old triggers keep Every update behavior.
Additional filters and their unit meanings are explicit. Live request behavior and combo identity
must be verified before promising deduplication or per-repeat delivery.

Acceptance: exact ID matches, renamed/duplicate names, unknown gifts, room changes, late replies,
unavailable browser, failed catalog, non-combo gifts, combo updates/finals and legacy triggers.

### 3. Fewer fields, better values

Deliver E2/E3/E4/V1 with reusable duration, expression, optional-value, entity and value-insertion
controls. Start with the ten workflows above, then migrate every supported family using the CSV.
Do not silently change stored types, defaults or interpretation of old amount bounds.

Acceptance: no-op edit preserves values and missing keys; invalid values keep the draft; cancel
changes nothing; names resolve to the intended identities; branches and unavailable values are
explained. Every touched control is localized and uses existing shared UI patterns.

### 4. Matching previews and trustworthy results

Deliver T2 and R1. Extract reusable event matching where necessary so sample previews and connector
delivery share it. Separate preview from execution and label side effects. Represent step outcomes
without rewriting existing public wire fields. Keep histories bounded and Memory Saver behavior.

Acceptance: nonmatching samples do not run actions; a connector error or skip is not reported as
successful remote execution; ambiguous sends are not retried; stale outputs do not mislead later steps.

### 5. Useful recipes and selected new events

Deliver R2 using groups, ordinary actions and templates. Prioritize gift thanks, reward-controlled
OBS source, countdown donation and song-request recipes. Only then decide on new standalone events
such as viewer threshold crossed or countdown finished, with reset/reconnect/first-sample semantics.
Do not build a second automation engine inside a recipe.

Acceptance: exported recipes are inspectable, configurable and editable with the general tools.
Each new operation has MCP access and tests. No unnecessary background work is introduced.

## What remains a decision or a live check

- These packages propose an order; they do not close the current version or authorize a release.
- New gift defaults, amount-filter units and multi-gift selection must be settled explicitly in the
  implementation spec. Favor one clearly named quantity per filter and preserve existing behavior.
- Arbitrary cross-platform triggers, shared normalized event arguments, threshold events and generic
  success/error arguments require compatibility design. They are not prerequisites for fixing Help.
- Catalog endpoints and metadata availability must be verified for each service before UI promises.
- No hardware, authenticated API success, live gift catalog or Windows runtime behavior was verified
  in this investigation. No runtime code was changed. The inventory is structural evidence.
- During implementation, run focused engine/connector regressions, formatting, clippy and the hidden
  display smoke checks. Measure hidden RSS and idle CPU before/after the version with the required
  real connections. New selectors and Help must not add recurring polls or eager image downloads.
