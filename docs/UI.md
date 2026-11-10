# UI pattern

Use this pattern for every new or changed Streamerz screen. It describes the intended design; existing
screens may still need to be brought into line when they are changed. Follow `PLAN.md` for behavior and
the project rules in `AGENTS.md` for implementation and verification.

## GPUI migration

GPUI is the frontend for the current rewrite. Shared controls live in `frontend/src/controls/` and
use GPUI Component, the shadcn family and Lucide. The production adapter below remains the visual and
behavior reference while pages are migrated. Do not introduce new screens into that adapter.

| Need | GPUI shared block |
|---|---|
| Editable entity lists, singleton settings, selection, filtering and actions | `controls/list.rs` |
| Virtualized tables, cell rendering and matching context menus | `controls/rows.rs` |
| Schema forms and masked secrets | `controls/form.rs` |
| Input, confirmation and choice prompts | `controls/prompt.rs` |
| Validation messages | `controls/notice.rs` |
| Connection and file health | `controls/health.rs` |
| Zinc colors and component tokens | `theme.rs` |
| Lucide bodies and brand assets | `assets/lucide.rs`, `frontend/src/assets.rs` |

All dialogs stay inside the GPUI window. Pending core prompts belong to `src/core/prompts.rs` and are
answered through the same dispatcher as MCP. Closing the renderer must preserve pending prompts.
Forms send changed field values to the core; validation, wire types and persistence belong to
`src/automation/`. Keep unknown imported fields and unchanged values intact.
Singleton server schemas and their atomic saves live in `src/core/settings.rs`. Settings rows reuse
the same forms and expose Edit in both their buttons and context menus. A displayed settings section
does not allocate a table or search input.

The frontend process is disposable. Pages query core models through inherited pipes, keep selection
by stable IDs and discard responses from a previous page generation. Updates follow core events.
No page may add a refresh timer or start an integration merely to display its state.

The sections below describe the shared production patterns that the GPUI blocks must preserve.

## Page

Give each page one clear purpose. Put its main content first, with a short title or column headings where
needed. Keep related controls together and use the same order and names for the same actions across pages.
Use the existing sidebar for navigation. Avoid adding another navigation system for a single feature.

Use the shared zinc theme (`frontend/src/theme.rs`) and the controls in `frontend/src/controls/`. Use the muted
text colour for secondary text and `Health` for connection state. Use Lucide icons from `assets/lucide.rs` and the
existing brand assets. Don't introduce
local colors, decorative symbols or custom widget styling for one screen.

## Building blocks

The UI is modular: a look or behaviour used in more than one place lives in one shared block, and every
screen uses that block instead of styling a widget by hand. The table below was written for the retired FLTK
window (the module names are its); in GPUI the same blocks are the controls in `frontend/src/controls/`
(rows, lists, trees, forms, prompts for dialogs and questions, notices, tabs, health) and the shared theme.
The rules and what each block is for still hold. To change how something looks
everywhere, change its module. When a screen needs something new that others could use, add it to the
right module (or a new one) first, then use it.

| Need | Use | Never |
|---|---|---|
| A page that may not fit | `scroll::page` (as tall as its content, lists get a minimum) | `Scroll::new` with a guessed height |
| A scroll area in a dialog | `scroll::area` | FLTK's scrollbar with arrows |
| A text view, tree or browser | `scroll::view`, or `scroll::paint_bars` at the end of its own draw | its default bars |
| A list drawn by hand | `scroll::thumb`, `scroll::dragged`, `scroll::draw_thumb` | a bar of its own |
| A right-click or any popup menu | `menu::Menu` (`context_menu()` for a list's buttons, `pick`, `tick_menu`) | `MenuButton::popup`, `menu_at_pointer` |
| A dropdown | `fit::choice()`; `menu::dropdown` on a MenuButton; `menu::input_choice` on an InputChoice | FLTK's own pulldown |
| Something floating over the window | `popover::Popover` (rounded, closed by Escape or a click outside) | a borderless window |
| A dialog | `dialogs::window` (or `window_tone`), then `dialogs::run`, or `open`/`close` for a live one | a window of its own, `make_modal` |
| A message or a question | `dialogs::message`, `success`, `alert` (warning), `error`, `choice`, `danger` (can't be undone), `pick_one` | `fltk::dialog::alert_default` and the like |
| A short form in a dialog | `fields::field` (icon and hint, no label column), `fields::Growing` (grows as it's filled), `fields::chip` (a platform to pick) | a column of labels and empty fields |
| A password or a token | `fields::secret` (forms: `W::Secret`): dots, and an eye that shows what's typed | `SecretInput`, or a plain field |
| Something that comes and goes | `anim::play` (eased, short, off in Settings); popovers and dialogs already animate | a timer of its own, or a delay |
| A row of buttons | `grid()` | buttons placed by hand |
| A list | `Rows` with `header()`, each column's look set with `set_cell` (see Lists and actions) | `Browser` |
| A connection's state | `Health::show` | a coloured label |

Scrolling: the wheel scrolls what is under the pointer when it can scroll (a list, a text view), else the
page. A list that has nothing to scroll passes the wheel on. A page never has blank room to scroll into.

Dialogs: always inside the app's window, in its middle, over the window dimmed (no window of their own:
those land anywhere on Wayland and tiling desktops, and could be opened twice). Only one can be in front:
nothing behind it takes clicks, the wheel or keys until it closes. Every dialog has the same three parts,
from `controls/dialog.rs`:

- **Header** (`dialog::shell`): the title, 13px bold, with the close button (X, and Escape, cancel) at its
  right, and a line under it. `shell` also centres the dialog, sets its width and gives it its shadow.
- **Body** (`dialog::body`): the contents, inset from the edges. A long one scrolls; the header and footer
  stay put.
- **Footer** (`dialog::footer`): the buttons, under a line. The main ones (OK, Cancel, Delete) sit at the
  right, the main action last; one that stands apart (Edit JSON) sits at the left. A dialog that only shows
  something gets a Close button there (`dialog::close_footer`). Buttons never go in the body.

A form or a prompt hands its buttons to the footer (`Form::buttons_view`, `Prompt::buttons_view`). The user
card is the one dialog with a header of its own: its banner (picture, name, account age, open and close).

Motion: a dialog rises into place while the window behind it fades from sharp to blurred, and sinks as it
closes; a menu or a popover unrolls from the top. Everything moves through `anim::play` (eased out, under a
fifth of a second, events still handled), and nothing moves when Animations is off in Settings or with the
memory saver. A switch's knob slides across (`anim::start`, which doesn't wait). A focus ring shows only
when the keyboard moved the focus, never after a click, and always fits inside its control. A menu marks
only the row under the mouse; off every row, none is marked.

Menus: a rounded panel with a quiet border, roomy rows, a Lucide icon per item when any has one (a tick for
toggles and a dropdown's current choice), dividing lines between groups, greyed items that can't be picked.
They take the mouse and the keyboard: arrows, Home, End, Enter, Space, Escape, and first letters. From the
keyboard they open on the focused control; a dropdown opens under its control (above it when there is no
room) and lines up with its right edge when it would leave the window.

## Lists and actions

Use `Rows` for lists and `header()` for column titles when columns need names. Make each row easy to scan:
put its identifying value first and keep secondary details brief. Show what a row is, not only its name:
each column says how it draws (`Rows::set_cell`), and the MCP and Stream Apps pages show the pattern:

| Cell | Holds | For |
|---|---|---|
| `Switch` | `1` or `0` | the row's on/off flag, turned in place (a click or Space, see `on_switch`); first column |
| `Lead` | `logo-or-icon\|text` | what the row is (an app's logo and its name) |
| `State` | `icon\|rrggbb\|text` | a live state in its colour (Connected, Found, Not found) |
| `Status` | `icon\|rrggbb\|tip` | a state as an icon alone, explained on hover |
| `Note` | `icon\|text` | a quiet detail (every 5 s, 10 s each, Moderators) |
| `Badge` | `text` | a short tag in an outline (default, *.png) |
| `Link` | a path or address | quiet, underlined on hover |
| `Logos` | `twitch,youtube` | the platforms something applies to |

A list of things that can be turned off gives each row a switch (and dims the row when off) instead of
writing "disabled" in a column. A list of connections shows each one's live state and connects or
disconnects the selected one, not only all of them. Give an empty list a useful hint that
explains what belongs there or how to add it. Use search when it helps users find items in a long list.

Put related actions together using `grid()`, which fills the available width and folds to icons when narrow.
On a wide page, a long button row wraps to two rows before folding. A grid folds as one: every button
shows its words, or every one only its icon, never a mix. Keep each button's label on the widget when
it shows only an icon, keep any explanation tooltip while folded, and dim both its text and icon when
unavailable. Related actions in the same grid need distinct icons when their meaning differs.
An action on a selection must operate on that selection; make unavailable actions clearly unavailable.
Every actionable list or tree must offer the same actions in its right-click menu using `context_menu()`;
the Menu key and Shift+F10 open the same menu from the keyboard. Lists and trees show their selection the
same way: a soft zinc fill (`rows::selected`), ringed while the list has keyboard focus, never FLTK's
dotted box. Unavailable items use `muted()`, never a hard-coded grey.
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

Keep the Actions tree and its trigger/sub-action editor visible together. Below 860px of content
width, stack the existing panes, with about one third of the height for the action tree. Use compact,
scrollable trigger rows to keep the sub-action editor usable. Preserve selection and filters on resize.

## Forms and dialogs

Use `dialog::shell`, `dialog::body`, `dialog::footer` and the form helpers in `controls/form.rs`.
Group fields by task, label them clearly and put common fields before advanced options. Use placeholders as
examples or hints, not as the only label. Keep the user's entries when validation fails and explain what
needs to be fixed. Cancel or closing a dialog must leave the original values unchanged.

For a form with many optional fields, use tabs to separate the main task, rules and replies. Keep the
buttons visible and let each tab scroll when needed. When a value accepts expressions, offer common
values in an editable choice so selecting a value is easy and expressions remain available. Preserve an
unchanged value and its stored type.

Keep a page for what is happening and what can be done to it; put a form (starting a poll, a
prediction) in a dialog its button opens, with the action's own name on its main button (Start poll, not
OK). Ask the least: one field per thing asked, its icon and hint saying what goes in it, and more fields
only as they're needed (a list of answers shows two, then one more as the last is filled). Show choices
with their logos or icons (a platform is a chip with its logo), and grey out with a reason what can't be
picked now.

Prefer forms and tables for ordinary editing. When raw data is useful, provide JSON one button away with
the existing editor. Ask for confirmation before an irreversible destructive action, with text that names
what will be affected. Use the existing date and time formatting helpers for displayed values.

Automation editors use the built-in Streamerz reference for summaries, field explanations and Help.
An event with no settings shows its behavior and available values, rather than opening JSON. Types
kept only for compatibility are preserved on import, explained in Help, and omitted from the ordinary
add picker. Help is also available from selected-item buttons and context menus.

Offer Insert value on expression-capable inputs. Name common values such as Sender name and Command
input; explain that some depend on the event or branch. Durations accept human units and retain the
wire format. An unchanged field retains its original type, missing key and unknown imported data.
Imported options that the engine ignores are available through JSON, not ordinary controls.

Detected selectors show names and save identities, distinguishing duplicates and preserving an
unavailable saved selection. Catalog loading, empty, failure and unavailable states are separate.
Refresh and result updates use connection/UI events, with no idle poll. Matching previews run no
actions; actual test execution is labeled separately. History reports observed failures and skipped
steps, and never treats completion alone as proof of remote delivery.

## Text and states

Write short, natural English and translate all UI text through `i18n::tr` and the locale files. Name the
object or action instead of relying on an icon or color alone. Show useful empty, loading, error and
connection states when they apply; an error should say what happened and, when possible, what to do next.
Keep disabled integrations inactive, as required by the project rules.

Keep filter fields and toolbar actions inside narrow pages. Let search take the remaining width;
stack navigation and actions when one row cannot fit. A chat with no configured channel offers
Accounts in its empty state. When chat history is off, disable sending controls with an explanation.
The Polls page shows what runs (a poll, a prediction) with what can be done to it, and a status line
saying which accounts are ready; New poll and New prediction open their forms in dialogs. It keeps
locked predictions available for resolution and distinguishes an unknown result from confirmed success
or failure. A chat view with no chat set up (Side by side too) shows the same note and Accounts button.

A stream ending and a chat disconnecting are separate states. A stream end gets a desktop notification
and leaves an available chat usable. For a disconnected chat, hide its messages and replace the composer
with a compact disconnected state and reconnect and hide actions; keep its history in memory. In a combined chat, keep other chats
usable and identify disconnected platforms in the notice.

Active chat restrictions appear above the composer. Name the platform when more than one chat is shown,
and include the slow mode interval when it is known. Hide the indicator when the mode is off or its chat
is disconnected.

## Set it up for the user

Streamerz configures everything it can itself, so setting something up takes the fewest clicks: it finds
what's installed (browsers, apps), reads what's already set up, fills in what it knows, and moves on by
itself when a step is done. When the setting lives in another program (a browser's extensions, another
app's settings), it asks first: one button that says what it will do, and a note explaining all of it
(what it writes, where, what it opens). The steps only the user can take there are shown in order, and
each is ticked off as it happens. Never change another program's files or settings behind the user's back.

## Browser accounts

When an integration uses a browser extension as its account, show one Browser account row with the
shared Health state and Set up action. Do not also show independent account or bot logins. Distinguish
extension unavailable, page loading, login required and account ready. Disable sending while the account
is unavailable. Setup lists the Chromium browsers found (and whether the extension is in them), and Set up on one writes
the extension, copies its folder and opens a setup tab in that browser's existing window (closing itself
once connected); the steps then tick themselves off, and Log in opens once the extension connects. It
offers Log in for the one explicit sign-in window; ordinary requests must not open helper tabs or windows.
A channel picker shows search failures plainly and still accepts a pasted handle or link.

## Check before finishing

- Reuse the shared helpers before adding a new control or pattern. If a reusable helper is missing, extend
  the shared UI instead of copying a one-off style.
- Check the changed screen on the hidden display at narrow and wide sizes with the MCP `ui` tool. Inspect
  the screenshots for clipping, readable labels, alignment and usable actions; check relevant empty,
  populated, error and disabled states, including the right-click menu.
- Run input-driven checks serially on the shared hidden display so one test cannot send input to
  another instance. `python3 dev/gpui-shot.py PAGE...` captures pages and `python3 dev/gpui-smoke.py` visits every page with
  isolated settings (`dev/ui-review.py` captures the released 1.6.0 reference); its disconnected memory sample is separate from the live integration budget.
- Exercise the main task, cancellation and validation by hand. Add a regression test for each bug fixed,
  or a smoke step when a unit test is not suitable. Run `dev/gpui-smoke.py` before closing the version.
