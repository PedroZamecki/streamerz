# UI pattern

Use this pattern for every new or changed Streamerz screen. It describes the intended design; existing
screens may still need to be brought into line when they are changed. Follow `PLAN.md` for behavior and
the project rules in `AGENTS.md` for implementation and verification.

## Page

Give each page one clear purpose. Put its main content first, with a short title or column headings where
needed. Keep related controls together and use the same order and names for the same actions across pages.
Use the existing sidebar for navigation. Avoid adding another navigation system for a single feature.

Use the shared zinc theme and controls from `src/ui.rs`. Use `muted()` for secondary text and `Health` for
connection state. Use Lucide icons from `src/ui/icons.rs` and the existing brand assets. Don't introduce
local colors, decorative symbols or custom widget styling for one screen.

## Lists and actions

Use `Rows` for lists and `header()` for column titles when columns need names. Make each row easy to scan:
put its identifying value first and keep secondary details brief. Give an empty list a useful hint that
explains what belongs there or how to add it. Use search when it helps users find items in a long list.

Put related actions together using `grid()`, which fills the available width and folds to icons when narrow.
An action on a selection must operate on that selection; make unavailable actions clearly unavailable.
Every actionable list or tree must offer the same actions in its right-click menu using `context_menu()`.
Keep action names and icons consistent between the buttons and menu.

Show a switched off item (a trigger or sub-action) with dimmed text and the Lucide `power-off` icon, never
a text tag and never red (red is for errors). In `Rows` use `add_state(line, true)`; in a tree set the
`power-off` icon on the item. The icon's tooltip says "disabled".

Triggers and sub-actions wear an icon: the logo of their service, else a Lucide icon chosen by their title.
The action list itself has no icons.

## Trees and pickers

A tree looks like shadcn's file tree: a chevron on groups (right when closed, down when open), no connector
lines, a rounded zinc selection, and an icon on every node. A group wears the logo of its platform or app
when `img::logo` has one, else a Lucide icon chosen by name in `ui/picker_icons.rs`; an item wears the Lucide
icon its title's words name, else its group's. Icons are drawn in `muted()`. Clicking a group opens or closes
it. The trigger and sub-action picker is the model: search with an icon, collapse and expand all, the tree
on the left and the selection's icon, path (muted) and description on the right. A new group or item needs
no mapping unless the tests say so: `no_group_is_left_with_the_plain_folder` fails for a group with no icon.

## Action groups

Keep actions, group controls, triggers, and sub-actions together on the Actions page. The group tree
shows optional user-supplied images next to group names. Group actions are available from the button
grid and the tree's context menu. The grid folds to icons when space is limited, with the action name
available as a tooltip. Deleting a group confirms that actions become ungrouped and its settings, files and icon are removed.

Group settings is one dialog: a table of settings (label, value, type) with the standard button grid
(Change value, Add, Edit and Delete setting, Files, Commands and timers). A value is typed by its
kind: a time takes `h:mm:ss`, an on or off setting asks for yes or no.

## Forms and dialogs

Use `dialogs::window`, `dialogs::row`, `dialogs::ok_cancel` and the existing form helpers where they fit.
Group fields by task, label them clearly and put common fields before advanced options. Use placeholders as
examples or hints, not as the only label. Keep the user's entries when validation fails and explain what
needs to be fixed. Cancel or closing a dialog must leave the original values unchanged.

For a form with many optional fields, use tabs to separate the main task, rules and replies. Keep the
buttons visible and let each tab scroll when needed. When a value accepts expressions, offer common
values in an editable choice so selecting a value is easy and expressions remain available. Preserve an
unchanged value and its stored type.

Prefer forms and tables for ordinary editing. When raw data is useful, provide JSON one button away with
the existing editor. Ask for confirmation before an irreversible destructive action, with text that names
what will be affected. Use the existing date and time formatting helpers for displayed values.

## Text and states

Write short, natural English and translate all UI text through `i18n::tr` and the locale files. Name the
object or action instead of relying on an icon or color alone. Show useful empty, loading, error and
connection states when they apply; an error should say what happened and, when possible, what to do next.
Keep disabled integrations inactive, as required by the project rules.

A stream ending and a chat disconnecting are separate states. A stream end gets a desktop notification
and leaves an available chat usable. For a disconnected chat, hide its messages and replace the composer
with a compact disconnected state and reconnect and hide actions; keep its history in memory. In a combined chat, keep other chats
usable and identify disconnected platforms in the notice.

Active chat restrictions appear above the composer. Name the platform when more than one chat is shown,
and include the slow mode interval when it is known. Hide the indicator when the mode is off or its chat
is disconnected.

## Browser accounts

When an integration uses a browser extension as its account, show one Browser account row with the
shared Health state and Set up action. Do not also show independent account or bot logins. Distinguish
extension unavailable, page loading, login required and account ready. Disable sending while the account
is unavailable. Setup explains installation and offers Log in for the one explicit sign-in window;
ordinary requests must not open helper tabs or windows. A channel picker shows search failures plainly
and still accepts a pasted handle or link.

## Check before finishing

- Reuse the shared helpers before adding a new control or pattern. If a reusable helper is missing, extend
  the shared UI instead of copying a one-off style.
- Check the changed screen on the hidden display at narrow and wide sizes with the MCP `ui` tool. Inspect
  the screenshots for clipping, readable labels, alignment and usable actions; check relevant empty,
  populated, error and disabled states, including the right-click menu.
- Exercise the main task, cancellation and validation by hand. Add a regression test for each bug fixed,
  or a smoke step when a unit test is not suitable. Run `dev/smoke.py` before closing the version.
