# Build an automation

An action is a list of steps. A trigger decides when that action runs; a sub-action is one of its steps.

Start on Actions. Choose Templates if the task is already there, or create an action and add a
trigger. Pick the event you want, then add the steps that should follow it. Select any trigger or
step and choose Help for Streamerz's built-in explanation. The same Help is in its editor and
right-click menu. A trigger that needs no settings shows what it does and the values it supplies.

Unavailable compatibility types are kept when you import an action, but are not offered as working
operations in the add picker. Their Help explains the limitation. Editing JSON preserves advanced
or imported data; adding a setting there does not make an unsupported operation work.

## React to a specific TikTok gift

1. Choose a TikTok channel and connect to its live. Public gift reception does not require a login.
2. To discover available gifts, set up the TikTok browser account in Accounts. Streamerz asks before
   changing browser setup. Once connected, it reads the room's current gift list.
3. Add the TikTok Gift trigger. Choose a gift by name, or leave Gift blank to accept any gift.
   Refresh gifts asks TikTok for the list again. A new gift can appear without updating Streamerz.
4. Choose Gift or combo completes for one response when the gift finishes. Every gift update also
   matches the growing updates during a combo. Older triggers retain their original update behavior.
5. Optionally set Min coins and Max coins. These are the gift's unit value times its cumulative
   quantity, with inclusive bounds. Zero means no minimum or no maximum, respectively.
6. Choose Test gift matching to try an example. It reports whether the rule matches, without running
   the action or sending anything. Test both the selected gift and a different gift.
7. Add Send Message to Live Chat, or another step. Insert value offers Sender name, Gift name,
   Gift quantity and Total gift coins so you do not need to memorize their variable names.

Streamerz saves the selected gift's ID, so a name change does not select a different gift. You can
enter a numeric ID if the catalog cannot be loaded. A saved selection remains intact when its gift
is unavailable. The list is refreshed for a new live or browser account; an old reply cannot replace
the new list. A failed refresh does not prevent gift reception or existing filters from working.

Combo updates have cumulative counts. Adding the total on every update can overcount the same gifts;
use completion delivery for donation-style totals. TikTok can repeat messages, and duplicate delivery
is not suppressed. Changes to TikTok's protocol or signing may still require a connector update.

## Use values without copying IDs or variable names

Detected selectors offer named objects and store their identities. Duplicate names are distinguished.
You can keep an unavailable saved object or enter its identifier manually.

Insert value offers values from the action's triggers, preceding steps and group settings. Some values
are only present for a particular event or branch. Help lists them with examples. In a reply, put the
cursor where the value belongs and select it from Insert value. The underlying `%variable%` remains
editable for advanced uses.

Durations accept `10s`, `5m`, `1:30` or `1h 20m`. A plain number entered in a duration control means
seconds. Streamerz converts it to the operation's stored units. Expressions use those stored units:
for example, Delay uses milliseconds, while a timeout duration may use seconds. Help states the units.
An unchanged imported value retains its original type and representation.

## Start with an editable recipe

Thank TikTok gifts creates an action and a group setting for its reply. Choose the gift in its trigger,
edit Reply in the group's settings, and enable the action when ready. The recipe starts disabled.

Reward shows an OBS source creates a Twitch reward action with Show, Delay and Hide steps. Choose the
reward in the trigger, set Scene, Source and Visible for in the group's settings, then enable the action.
It uses the first connected OBS connection. Repeated runs follow the action queue's normal ordering.
You can edit the individual steps or choose a particular connection in the OBS steps.

The existing Subathon timer, Song requests (Spotify) and overlay templates use the same editable
group tools. Their settings belong to the group; no hidden recipe engine runs behind the action.

## Understand a test or result

Matching previews only check conditions. The action's Test button actually runs its steps using the
sample values you enter. Development chat is captured, but other configured operations can affect
real applications. Use matching preview when you only want to check the gift rule.

Separate triggers are alternatives. Current dispatch queues the action for each matching trigger, so
overlapping triggers can run it twice. All conditions inside one gift trigger must match together.

History shows Completed, Failed or Completed with skipped steps. Select a run and inspect its values
after execution to see the first reported error. MCP's action_history also identifies the failed step
type. Completed means no execution error was reported; it is not a delivery receipt from a remote
service. A missing response can be ambiguous, so check the service before repeating a real operation.
