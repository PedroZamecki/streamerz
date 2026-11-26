# UI pattern

Use this pattern for every new or changed Streamerz screen. It describes the intended design; existing
screens may still need to be brought into line when they are changed. Follow `PLAN.md` for behavior and
the project rules in `AGENTS.md` for implementation and verification.

## The window

The window is GPUI. Shared controls live in `frontend/src/controls/` and use GPUI Component, the shadcn
family and Lucide.

| Need | Shared block |
|---|---|
| Editable entity lists, singleton settings, selection, filtering and actions | `controls/list.rs` |
| Virtualized tables, cell rendering and matching context menus | `controls/rows.rs` |
| Schema forms and masked secrets | `controls/form.rs` |
| Input, confirmation and choice prompts | `controls/prompt.rs` |
| Validation messages | `controls/notice.rs` |
| Connection and file health | `controls/health.rs` |
| Zinc colors and component tokens | `theme.rs` |
| Lucide bodies and brand assets | `assets/lucide.rs`, `frontend/src/assets.rs` |

All dialogs stay inside the window. Pending core prompts belong to `src/core/prompts.rs` and are
answered through the same dispatcher as MCP. Closing the renderer must preserve pending prompts.
Forms send changed field values to the core; validation, wire types and persistence belong to
`src/automation/`. Keep unknown imported fields and unchanged values intact.
Singleton server schemas and their atomic saves live in `src/core/settings.rs`. Settings rows reuse
the same forms and expose Edit in both their buttons and context menus. A displayed settings section
does not allocate a table or search input.

The frontend process is disposable. Pages query core models through inherited pipes, keep selection
by stable IDs and discard responses from a previous page generation. Updates follow core events.
No page may add a refresh timer or start an integration merely to display its state.

## Page

Give each page one clear purpose. Put its main content first, with a short title or column headings where
needed. Keep related controls together and use the same order and names for the same actions across pages.
Use the existing sidebar for navigation. Avoid adding another navigation system for a single feature.

Use the shared zinc theme (`frontend/src/theme.rs`) and the controls in `frontend/src/controls/`. Use the muted
text colour for secondary text and `controls/health.rs` for connection state. Use Lucide icons from `assets/lucide.rs` and the
existing brand assets. Don't introduce
local colors, decorative symbols or custom widget styling for one screen.

### An item's own page

When an item has more to it than a dialog holds (an overlay: its preview, settings, tests, files and code),
opening its row (double-click, Enter, the Open button and its menu item) shows the item's page in place of
the list, under the same sidebar entry. A Back button above the title (and Escape, and the mouse's back
button) returns to the list. The title row keeps the item's buttons; tabs under it switch what is shown,
and the core remembers the tab per item, so Back and a new window find it again. The core gives the page
as a model (the overlay's is `overlay` in the Overlays read, `controls/overlay.rs` draws it), so MCP reads
the same page.

Its tabs show rows (a file, a rule, a made-up event) with a picture or an icon, a line of detail and
their buttons; a row's lesser buttons are their icon only, named on hover (`iconOnly`), and every row has
a right-click menu with all its buttons. A tab whose changes are settings (Settings, Alerts) keeps them
as a draft, with Revert and Save at the top; a tab of live controls (Live) sets each at once.

## Page frame, spacing and typography

One frame for every page. The numbers live as tokens in `frontend/src/theme.rs`; a screen uses the token,
never a literal. `dev/ui-spacing.py DIR` measures screenshots from `dev/ui-shot.py` against this table.

**Spacing scale** (4px grid, nothing else): `SPACE_XS` 4, `SPACE_SM` 8, `SPACE_MD` 12, `SPACE_LG` 16.
No 1, 2, 3, 5, 6, 10, 11 or 14px gaps between blocks. Icon-to-label gaps inside one control may be 4 or 8.

**Frame** (drawn once, in `app.rs`; pages never add their own outer padding): 16px left and right
(`PAGE_PAD_X`), 16px top (`PAGE_PAD_TOP`), 12px bottom (`PAGE_PAD_BOTTOM`). The scrollbar sits inside the
right padding, so the right gutter is 16px with or without a scrollbar.

**Anatomy**, top to bottom, the same order on every page:

| Part | Look | Gap after it |
|---|---|---|
| Header: icon and title (`controls/page.rs` `header`) | H1, 18/24 bold, strong colour, line under it; page-wide actions (Add, Pause all) at its right | 12 |
| Guide: what the page is for (`page::guide`) | 13/18, quiet; at most two lines | 12 |
| Toolbar: search, filter, "Show" tabs, top buttons | one row, 28px high, 8px between controls | 8 |
| Content: table, tree, cards, labelled rows, chat | fills the rest | 8 |
| Action row under the content (edit, delete, restore...) | one row, 28px high, 8px between buttons | 8 |
| Note under the content (a hint, a scope warning): `notice::note` | 12/16, quiet | none |

Chat is a full-bleed workspace: it has the frame's side and bottom padding but no header.

**Sections** inside a page: heading H2 (15/20 bold, optional icon), 8px under it to its content, and
`SECTION_GAP` (16) between one section and the next. A page with one list uses the page header and has no H2.
A section's own note or hint goes straight under its heading (12px caption, 4px gap), not at the bottom.
Cards in a grid have 12px between them, 16px inside.

**Typography**, five sizes only (`FONT_*`, `LINE_*`):

| Role | Size / line | Weight | Colour |
|---|---|---|---|
| Page title (H1) | 18 / 24 | bold | strong |
| Section title (H2), card title | 15 / 20 | bold | strong |
| Body: labels, cells, buttons, inputs, menu and sidebar items (a sidebar item is 24px high) | 13 / 18 | normal (bold for a name) | default |
| Caption: column heads, hints, notes, chips, status text | 12 / 16 | normal (column heads bold) | quiet |
| Micro: sidebar group labels, tiny badges | 11 / 14 | bold | quiet |

No half sizes (12.5, 13.4, 13.5, 11.5), and no `text_xs`/`text_sm`/`text_lg`. A card's large headline
number is the one other size (22 / 28 bold). Weights are normal and bold only.

**Rows and controls**: a table row is 28px, a header row 24px, a button 28px (26px in a dense row), an input
28px. Padding inside a row or cell is 12px left and right. Label then control rows (forms, settings) are 8px apart,
a labelled group is 16px from the next.
The bordered rows of the connection and settings lists (Servers, Stream Apps) are 28px high with 12px
at the sides, 8px between one row and the next, and buttons of `DENSE_CONTROL_HEIGHT` (26px) so they fit inside the border.

## Building blocks

The UI is modular: a look or behaviour used in more than one place lives in one shared block, and every
screen uses that block instead of styling a widget by hand: the controls in `frontend/src/controls/`
(rows, lists, trees, forms, prompts for dialogs and questions, notices, tabs, health, cards, gallery) and
the shared theme. To change how something looks everywhere, change its module. When a screen needs something
new that others could use, add it to the right module (or a new one) first, then use it.

| Need | Use | Never |
|---|---|---|
| A list of things to add, edit and delete | `list.rs` (`List`: selection, filter, buttons, right-click menu, form dialogs) | a table and buttons wired by hand |
| A table | `rows.rs` (`Rows`: virtualised, one action list for buttons and the right-click menu) | a table of its own |
| A tree (actions, groups, files) | `tree.rs` (`TreePane`: search, button bar, right-click menu, logos) | nested rows drawn by hand |
| A page of settings or platforms | `sections.rs` (labelled rows, `"card": true` cards) from a core model | a page laid out by hand |
| A form | `form.rs` (schema fields, placeholders, Browse, secrets with an eye), validated by the core | fields placed by hand, checks in the window |
| Picking one item of a tree with its description | `picker.rs` (`Picker`) | a list with a separate description |
| Picking one item by its picture | `gallery.rs` (`Gallery`) | a dropdown of names |
| A dialog | `dialog.rs` (`shell`, `body`, `footer`: header, body, buttons in the footer) | a window of its own |
| A message or a question from the core | `prompt.rs` (`Prompt`), asked through `src/core/prompts.rs` | a dialog the window owns |
| Why something is empty or can't be done | `notice::info`; a validation error: `notice::error` | a coloured label |
| A hint or a scope warning under a heading, a toolbar or a button row | `notice::note` (12/16, quiet); a page's own guide: `page::guide` | a text styled by hand |
| A small tag in a row (a lock, a retry time, the default star) | `pill` in `list.rs` (11/14, quiet, 4px padding) | a box drawn by hand |
| A page's "Show" selector | `tabs.rs` (`bar`) | buttons of its own |
| The inside of a button | `body.rs` (`button_body`: icon and label) | an icon and text placed by hand |
| Loading remote pictures in batches | `images.rs` (`Pictures`: bounded cache, queue or computed set, `"images"` requests) | a per-control image map and request loop |
| A connection's state | `health.rs` (`appearance`) | a coloured label |

Cards: a section with `"card": true` draws as a rounded, bordered panel, two to a row when the window is
wide (one when narrow): its logo and title with a short state at the right (`state`: health, text, and a
`tip` with the reason), then a `headline` (large, quiet with `headlineMuted`), `chips` (small rounded tags, an
optional icon) and `lines` (an icon and a quiet sentence), and its `buttons` under a line. A card with
`rows` instead draws them as an ordinary section inside the frame (Accounts: a platform's state, account,
channel and bot); those take the page's width, two to a row only on a very wide window. Use cards for
things that stand side by side (platforms); keep plain labelled rows for settings.

Scrolling: the wheel scrolls what is under the pointer when it can scroll (a list, a text view), else the
page. A list that has nothing to scroll passes the wheel on. A page never has blank room to scroll into.

Dialogs: always inside the app's window, in its middle, over the window dimmed (no window of their own:
those land anywhere on Wayland and tiling desktops, and could be opened twice). Only one can be in front:
nothing behind it takes clicks, the wheel or keys until it closes. Every dialog has the same three parts,
from `controls/dialog.rs`:

- **Header** (`dialog::shell`): the title, 13px bold, with the close button (X, and Escape, cancel) at its
  right, and a line under it. `shell` also centres the dialog, sets its width and gives it its shadow.
- **Body** (`dialog::body`): the contents, inset from the edges (16px at the sides). It has only 4px of
  vertical padding because the toolkit already adds 8px above and below it, which makes 12px between the body
  and the lines of the header and footer. A long one scrolls; the header and footer stay put.
- **Footer** (`dialog::footer`): the buttons, under a line. The main ones (OK, Cancel, Delete) sit at the
  right, the main action last; one that stands apart (Edit JSON) sits at the left. A dialog that only shows
  something gets a Close button there (`dialog::close_footer`). Buttons never go in the body.

Every Cancel or Close button closes through `dialog::dismiss`, as the X and Escape do: the dialog's
`on_cancel` runs, so the page that opened it knows it closed and can open it again. Never call
`window.close_dialog` from a Cancel or Close button.

A form or a prompt hands its buttons to the footer (`Form::buttons_view`, `Prompt::buttons_view`). The user
card is the one dialog with a header of its own: its banner (picture, name, account age, open and close).

Forms focus their primary text field, or their first empty visible text field. Commands and Reply lead
command creation. Related advanced fields use shared collapsible sections; nondefault permissions and
matching start expanded. Collapsing a section never discards its values. New timers and templates offer
Save draft and Create and enable explicitly, using core-provided validated field changes. Edit JSON
remains one button away and saves its explicit values. Long translated footer groups wrap together.

Motion: dialogs, menus and popovers move as the toolkit's components do (short, eased out, events still
handled), and nothing moves when Animations is off in Settings or with the memory saver (the window asks
the toolkit to reduce motion). Never add a timer of your own for it. A focus ring shows only
when the keyboard moved the focus, never after a click, and always fits inside its control. A menu marks
only the row under the mouse; off every row, none is marked.

Menus: a rounded panel with a quiet border, roomy rows, a Lucide icon per item when any has one (a tick for
toggles and a dropdown's current choice), dividing lines between groups, greyed items that can't be picked.
They take the mouse and the keyboard: arrows, Home, End, Enter, Space, Escape, and first letters. From the
keyboard they open on the focused control; a dropdown opens under its control (above it when there is no
room) and lines up with its right edge when it would leave the window.

## Lists and actions

Use `Rows` (`controls/rows.rs`) for lists, with column titles when columns need names. Make each row easy to
scan: put its identifying value first and keep secondary details brief. Show what a row is, not only its
name: the core's model says how each cell draws, and the MCP and Stream Apps pages show the pattern:

| Cell | Holds | For |
|---|---|---|
| `true` or `false` | a switch | the row's on/off flag, turned in place (a click or Space); first column |
| `{"brand", "text"}` | a logo and a name | what the row is (an app's logo and its name) |
| `{"icon", "text"}` | a Lucide icon and text | what the row is, or a quiet detail (every 5 s, Moderators) |
| `{"health", "text"}` | a state in its colour and icon | a live state (Connected, Found, Not found) |
| `{"platforms": [...]}` | platform logos | the platforms something applies to |
| a string | text | anything else, ending in an ellipsis (whole on hover) when it doesn't fit |

A list of things that can be turned off gives each row a switch (and dims the row when off) instead of
writing "disabled" in a column. A list of connections shows each one's live state and connects or
disconnects the selected one, not only all of them. Give an empty list a useful hint that
explains what belongs there or how to add it. Use search when it helps users find items in a long list.

Put related actions together in the list's button rows (`controls/list.rs`), which fill the available width
and fold to icons when narrow.
On a wide page, a long button row wraps to two rows before folding. A grid folds as one: every button
shows its words, or every one only its icon, never a mix. Keep each button's label on the widget when
it shows only an icon, keep any explanation tooltip while folded, and dim both its text and icon when
unavailable. Related actions in the same grid need distinct icons when their meaning differs.
Every cell of a grid is as wide as the others, on every row.
A list with more actions than two rows hold (Overlays) splits them instead of stacking rows: what starts or
brings something in (New, Import, Examples) goes on the line above the list, at the right of the search,
with its words (`"top": true`; `"iconOnly": true` for the ones every page has, such as Guide and Refresh;
`"primary": true` for the one main action), and what is done with the selected row is a strip of equal
icon buttons under the list (`"strip": true`), each named on hover and in the right-click menu, with what
deletes standing apart at the right. Merge buttons that are the same step for different rows into one.
An action on a selection must operate on that selection; make unavailable actions clearly unavailable.
Every actionable list or tree must offer the same actions in its right-click menu: `Rows` and `TreePane`
build it from the same action list as their buttons. The Menu key and Shift+F10 open the same menu from the
keyboard. Lists and trees show their selection the same way: a soft zinc fill, ringed while the list has
keyboard focus. Unavailable items use the theme's muted colour (`theme::quiet`), never a hard-coded grey.

A truly empty list or tree shows its explanation beside enabled creation actions, without table headers,
an empty frame or disabled selection buttons. A filtered-empty list retains its structure and Clear
search route. Actions hides the inactive editor until an action is selected. New, Templates and Import
remain visible in the empty state. Common operations retain words by wrapping before folding to icons.

Responsive columns may declare a priority (zero always stays visible). Secondary values that would be
unreadable move into persistent selected-row details, accessible from the keyboard as well as the mouse.
Compute layout on model or width changes. A Show selector uses the shared segmented tabs; long translated
groups wrap as units. Settings inlines the same Chat and notification controls used by their other entry
points, with related version and update controls together. Save is a compact manual retry while changes
save automatically; failures retain a visible error and retry path.
Settings search matches labels and explanations, including the formerly nested Chat and notification
controls. Filtering never changes their values.
Keep action names and icons consistent between the buttons and menu.

Show a switched off item (a trigger or sub-action) with dimmed text and the Lucide `power-off` icon, never
a text tag and never red (red is for errors). In `Rows` a row with `"enabled": false` draws that way; in a
tree set the `power-off` icon on the item. The icon's tooltip says "disabled".

Triggers and sub-actions wear an icon: the logo of their service, else a Lucide icon chosen by their title.
An action's row says what starts it and what it runs: its first trigger's logo or icon (a dashed circle when
nothing starts it), its name, its count of sub-actions in quiet text at the right, and on hover the triggers
that run it. A switched off action is dimmed with `power-off` at the right, like a trigger.

## Trees and pickers

A tree looks like shadcn's file tree: a chevron on groups (right when closed, down when open), no connector
lines, a rounded zinc selection, and an icon on every node. A group wears the logo of its platform or app
when `pick_icons::logo` has one (a node's `logo`, drawn in its own colours), else a Lucide icon chosen by name
in `automation/pick_icons.rs`; an item wears the Lucide icon its title's words name, else its group's. Clicking
a group opens or closes it. The trigger and sub-action picker (`controls/picker.rs`) is the model: search
with an icon, Expand all and Collapse all beside it (`Options::fold_buttons`), the tree on the left and the
selection's icon or logo, path (muted), description and, for a trigger, the values it gives the action on
the right (under the tree on a narrow window), with a hint there while nothing is picked. Cancel and OK are
the dialog's footer. A search box in a column keeps its own height; only beside a title does it take the
rest of the row.

## Action groups

Keep actions, group controls, triggers, and sub-actions together on the Actions page. The group tree
shows optional user-supplied images next to group names. Group actions are available from the button
grid and the tree's context menu. The grid folds to icons when space is limited, with the action name
available as a tooltip. Deleting a group confirms that actions become ungrouped and its settings, files and icon are removed.

An overlay is not a group: one with no actions appears only on the Overlays page. A group of actions with
the name of an overlay is connected to it, and both pages say so with a `link` icon (a chain): at the
right of the group's row in the tree (a row's `note`), and in the Overlays table's Connected actions
column with what the group holds. Deleting a connected group asks its own question (a row's `confirm`,
by button id, replaces the button's): it names the overlay and says it stays. The Overlays page has
Connect to actions (a form: a new group of the overlay's name, or one of the groups that have no
overlay) and Disconnect.

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

A value is set with the control made for it, never typed in a code: a colour is picked (the field's widget
`color`: a swatch that opens a palette and sliders for hue, saturation, lightness and opacity, with the
`#rrggbb` beside it), a number with two ends is dragged (`slider`, with `min`, `max` and `step`, its value
beside it). A long form is split by headings: a field's `section` draws a quiet heading with a rule above
it, for the fields from there on. What a field is for (`tip`) shows when the pointer rests on its label.
A font is picked from the ones installed (`font`: a list that narrows as its name is typed, starting with
the system's). A field that only matters with another one's value is shown only then (`showIf`: the other
field's place and the value that calls for it), so a form shows what applies and nothing else.
A few on or off choices about one thing are a line of check boxes (`checks`, each with its logo: the
platforms), and their value is the list of the ticked ones. A file is picked from a list of the ones
there, with Other to type a path or an address and Browse beside it (`choices` with `other` and
`browse`). Rows of a few columns are typed a row a line, the cells between bars, with the columns named in
the empty box and on the label's tip; a mistake is said with its line and column.
A group's settings get these from what its maker declared (`color`, `slider`, `lines`, `font`, `file`,
`platforms`, `action`, `list`, `section`, `showIf`).
Values people often want are one click away under a box (`presets`: Today, In 15 minutes, `5m`), and a
value typed from now (`+15m`) goes back in the box as the day and time it became. A value made of a few
small ones (a place, a size, a text style, a sound) is one field of parts (`parts`): each part a control
with a quiet caption over it, on the lines its maker gave, its value the parts by key. A place on the page
is picked on a 3 by 3 grid (`anchor`), a range is a slider with two handles (`range`), a percent shows its
%, a gradient draws a bar with its colors, a size keeps its ratio while Keep ratio is ticked, and code is a
tall box in a fixed-width font (`code`). Many short check boxes (kinds of events, each with its icon) go in
as many columns as fit. A button in a form (`button`, with `press`) does one thing at once and says what it
did in a notification; Play of a sound sends the form's value as it is, not the saved one. A form on a
page (`tall`) takes the page's height instead of a dialog's.

A label that says what an empty value means (`Gift (blank: any gift)`, `Max coins (0: no limit)`) shows its
name only, with the meaning in the empty box (`Any gift`). A form's `notes` are quiet lines with an info icon
(`notice::info`): why a list is empty, what a trigger gives the action. A field that names something found on
a connection takes typed text too; its list shows only once something was found, and can be searched. A
thing people know by its picture (TikTok's gifts) is picked with `controls/gallery.rs`: the field shows the
chosen picture, name and a quiet detail (its price), and opens a searchable grid with "any" first, the
details on hover and a reload icon beside the search (its tip says what it does). Ids stay out of sight
(Edit JSON has them); the list loads by itself when the form opens without one, and its pictures are
asked for only as their rows scroll into view.

A form field can carry a `placeholder` (an example, shown in the empty box) and `browse` (`"file"` or
`"image"`): a Browse button beside the box opens the system's file dialog, and an image shows under the box
once the path is a file. A field that doesn't apply to what was picked is `hidden`.

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
stack navigation and actions when one row cannot fit. Nothing is ever cut off, in any language: a row of
buttons wraps to a second line instead of running past its dialog or card, a row's buttons go under its
text before squeezing the text, and a button with words is as wide as its label needs (`min_w`, never a
fixed `w`; only icon buttons are fixed). A label that can't grow truncates with an ellipsis and has a
tooltip. A chat with no configured channel offers
Accounts in its empty state. When chat history is off, disable sending controls with an explanation.
The Polls page shows what runs (a poll, a prediction) with what can be done to it, and a status line
saying which accounts are ready; New poll and New prediction open their forms in dialogs. It keeps
locked predictions available for resolution and distinguishes an unknown result from confirmed success
or failure. The Stream page shows a card per platform that is set up (a `"card": true` section, below), its
state in a word ("Ready", "Read only", "Offline") with the full reason on hover and in the card, the title
large, the category and tags as chips, then its buttons. A platform that isn't connected shows only why and
an Accounts button. Edit stream and Run ad at the top act on every platform, picked with logo check boxes
that grey out with the reason on hover; a card's Edit opens a form with only the fields that platform takes.
An empty field keeps what is there. A chat view with no chat set up (Side by side too) shows the same note and Accounts button.

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
is unavailable. Setup lists the Chromium and Firefox browsers found (and whether the extension is in them). Set up on a
Chromium one writes the extension, copies its folder and opens a setup tab in that browser's existing window
(closing itself once connected); on a Firefox one it downloads the signed add-on of the pinned version and
opens it beside the setup tab, so Firefox asks to add it. An extension refused for its version says which
side to update; the steps then tick themselves off, and Log in opens once the extension connects. It
offers Log in for the one explicit sign-in window; ordinary requests must not open helper tabs or windows.
A channel picker shows search failures plainly and still accepts a pasted handle or link.

Installation evidence and connection state are separate. List detected copies with their version,
profile, enabled/disabled state and whether a local copy uses another folder. Unreadable metadata means
verification is unavailable, not that the extension is absent. Distinguish an authenticated socket,
TikTok account loading, login needed, pairing refusal and protocol incompatibility. Show the running
extension's actual version independently of the bundled version. Explicit re-pairing overrides defaults
from an unpacked copy. Browser-scoped setup actions may sit beside the browser they affect; dialog-wide
Cancel, Close and commit actions stay in the footer. No setup or diagnostic read changes browser profiles.

## Check before finishing

- Reuse the shared helpers before adding a new control or pattern. If a reusable helper is missing, extend
  the shared UI instead of copying a one-off style.
- Check the changed screen on the hidden display at narrow and wide sizes with the MCP `ui` tool. Inspect
  the screenshots for clipping, readable labels, alignment and usable actions; check relevant empty,
  populated, error and disabled states, including the right-click menu.
- Run input-driven checks serially on the shared hidden display so one test cannot send input to
  another instance. `python3 -P dev/ui-shot.py PAGE...` captures pages (always on :99, the window of the instance it started,
  at any `--size`, narrow ones too) and
  `DISPLAY=:99 python3 -P dev/ui-smoke.py` visits every page with isolated settings; its disconnected memory
  sample is separate from the live integration budget. The smoke fails when anything on a page or in the
  TikTok setup dialog is cut off sideways (the dev `overflow` command: a painted button, field or box only
  partly inside its clip); run it in Portuguese too (`--locale pt_BR`), whose words are the longest.
- Show failures with `controls::notice::error` and a `StreamerzError` (`docs/ERRORS.md`): a title, what
  happened and what to try, never a bare string.
- Exercise the main task, cancellation and validation by hand. Add a regression test for each bug fixed,
  or a smoke step when a unit test is not suitable. Run `dev/ui-smoke.py` before closing the version.
