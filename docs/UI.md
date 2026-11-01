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

## Forms and dialogs

Use `dialogs::window`, `dialogs::row`, `dialogs::ok_cancel` and the existing form helpers where they fit.
Group fields by task, label them clearly and put common fields before advanced options. Use placeholders as
examples or hints, not as the only label. Keep the user's entries when validation fails and explain what
needs to be fixed. Cancel or closing a dialog must leave the original values unchanged.

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

## Check before finishing

- Reuse the shared helpers before adding a new control or pattern. If a reusable helper is missing, extend
  the shared UI instead of copying a one-off style.
- Check the changed screen on the hidden display at narrow and wide sizes with the MCP `ui` tool. Inspect
  the screenshots for clipping, readable labels, alignment and usable actions; check relevant empty,
  populated, error and disabled states, including the right-click menu.
- Exercise the main task, cancellation and validation by hand. Add a regression test for each bug fixed,
  or a smoke step when a unit test is not suitable. Run `dev/smoke.py` before closing the version.
