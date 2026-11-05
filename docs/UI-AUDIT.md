# UI and UX fixes

The agreed scope is to fix the issues cited in the independent reviews, verify the affected screens,
and report the result. The earlier 9/10 target is retired. There is no further scoring loop.

Streamerz keeps its native zinc theme, Lucide icons, brand assets and shared controls. The checklist
below consolidates the code audit and both visual reviews. Completed implementation still needs the
verification recorded at the end; unchecked items remain work to do.

## Shared controls and navigation

- [x] Visible keyboard focus, keyboard sidebar navigation, and focus on empty lists.
- [x] Shared responsive columns, matching headers and pointer targets, readable overflow tooltips.
- [x] Wrapped help without overlapping following controls.
- [x] Form edits retained on JSON transfer and failed validation; unchanged types and fields preserved.
- [x] Compact button labels retained, inactive icons dimmed, and related action icons distinguished.
- [x] One fold decision per grid (every button labelled or every one an icon), with readable wide rows
  and preserved explanation tooltips.
- [x] Responsive sidebar folding below 760 px and selected-page visibility at both review heights.
- [x] Consistent zinc selection in lists and trees (rounded ring for keyboard focus, no dotted box), and
  muted disabled items.
- [x] Connection states use Health, with darker light-theme inks; warning labels use Health colours.
- [x] Right-click menus match their buttons and also open with the Menu key or Shift+F10; credits have a
  menu; emote favorites have an Insert / Add to favorites menu and arrow-key navigation.
- [x] Narrow footer leaves room for connection state and save feedback.

## Pages

- [x] Chat: fitting toolbar, centered Accounts action, unavailable Message all explained.
- [x] Accounts: shorter status that fits narrow rows, and "Log in" wording.
- [x] Community: fitting search; quote selection states and validation; credits menu.
- [x] Polls: fitting platform controls, a checking state before the first read, ordered reads and
  mutations, locked predictions, account readiness, unavailable targets unticked, YouTube's four-answer
  limit explained.
- [x] Actions: responsive panes, plain status text ("Triggers: 1. Sub-actions: 1."), theme colours.
- [x] Commands and Timers: clear headers and a search field.
- [x] Files and Servers: usable sections in short and narrow windows; server states explained.
- [x] Queues: distinct controls, selection-aware actions, a Status column in the pending pane.
- [x] Groups: guidance on what groups are for, selection actions and a Members view.
- [x] Plugins: selection actions, a wrapped description and no gap above the buttons.
- [x] Stream Apps: readable guides, Connect all states and Health.
- [x] Integrations and Donations: an intro and a description per integration, Health states.
- [x] Globals: read-only user globals marked; eligible actions and validation keeping the form open.
- [x] History: eligible actions, distinct variable views and search.
- [x] Settings: "Keep chat history", consistent switches, wrapping descriptions, sentence case.
- [x] MCP: fitting fields, an inline port error (an alert only on Enter), the correct executable.
- [x] Log: usable search and filter when narrow.
- [x] Crashes and Backups: selection actions, empty detail hint, Unknown version, visible failures.

## Dialogs and text

- [x] Picker: search focused on open, the intro shown once, OK disabled until something is picked.
- [x] Property forms: translated labels, Spotify Operation as a choice, Delay's "Up to (ms)" disabled
  until Random is on.
- [x] Group settings: the description opens at its start; typed validation and cancellation.
- [x] Message all: wider layout, a reason on each unavailable platform, and Send disabled with a visible
  note when no account can send.
- [x] Countdown shows its operation instead of a clock ID.
- [x] Shared name/value and Globals validation explain blank names while keeping the form open.
- [x] Every text key the code uses exists in English and Portuguese (several were missing and showed
  raw keys such as `lists.search`).
- [x] AppImage identity checks also cover portable configuration and crash reports.

## Verification

- [x] Unit tests: 458 passed, 13 ignored (new: emote grid arrow keys, tooltip kept while folded).
- [x] All 22 pages captured at 540x700 and 1100x800 in both themes, plus populated Actions, and
  checked by eye.
- [x] Smoke suite passes, including a Commands list menu from a right click and from Shift+F10, compared
  with its buttons. Harness tests (3), formatting, clippy and locale checks pass.
- [x] Hidden RSS and idle CPU (release builds, nothing connected, every page visited, 15 s settle, 30 s
  sampled): main 35.4 MiB RSS / 20.2 MiB PSS, this branch 35.9 / 20.7, idle CPU 0% for both, 4 threads.
- [ ] Not checked: live Twitch/OBS (the 25 MB budget is about a connected release build), Windows, and
  hand walkthroughs of the changed dialogs (picker, Message all, Delay form, emote menu) beyond build,
  tests and smoke.
