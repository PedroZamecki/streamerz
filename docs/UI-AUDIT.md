# UI and UX review

The target is at least 9 out of 10 for both UI and UX on every page. No page has passed yet.
Scores below are an independent code review of the initial implementation. They are provisional,
not proof of the current implementation's quality. Changes require fresh visual and task reviews.

The existing native design has a useful foundation: zinc colors, rounded controls, Lucide SVG icons,
brand assets and searchable trees. Improve its consistency and usability without a new framework.

## Initial scores

| Page | UI | UX | Main obstacle |
|---|---:|---:|---|
| Chat | 8 | 8 | Narrow toolbar and keyboard access |
| Accounts | 7 | 7 | Fixed widths and repeated tall sections |
| Community | 7 | 7 | Fixed search row and columns; credits menu parity |
| Polls | 7 | 6 | Long form and unclear unavailable operations |
| Actions | 7 | 6 | Fixed split hides editor panes when narrow |
| Commands | 7 | 6 | Columns, long form and selection action availability |
| Timers | 7 | 7 | Columns, selection actions and finding items |
| Files | 6 | 6 | Competing lists and guides in short windows |
| Queues | 6 | 6 | Crowded action bar and fixed pending pane |
| Groups | 7 | 6 | Membership discovery and selection availability |
| Plugins | 7 | 7 | Fixed description and columns; selection actions |
| Servers | 6 | 6 | Server controls and two lists compete for height |
| Stream Apps | 6 | 6 | Repeated lists and weak connection state hierarchy |
| Integrations | 7 | 6 | Little context or next-step guidance |
| Donations | 6 | 6 | Connection states bypass shared Health presentation |
| Globals | 6 | 5 | Read-only distinction and validation after form closes |
| History | 7 | 7 | Fixed columns, finding records and selection actions |
| Settings | 8 | 7 | Fixed controls and hover-only path actions |
| MCP | 7 | 6 | Fixed address, port and route row |
| Log | 7 | 7 | Fixed filter row when narrow |
| Crashes | 7 | 7 | Fixed list geometry and operation failure feedback |
| Backups | 7 | 7 | Selection availability and operation failure feedback |

## Shared work

The initial visual review independently scored every page at both target sizes. Its scores are
lower in several places because screenshots show failures that code inspection alone cannot prove.

| Page | UI | UX |
|---|---:|---:|
| Chat | 6 | 4 |
| Accounts | 7 | 7 |
| Community | 7 | 6 |
| Polls | 6 | 5 |
| Actions | 7 | 5 |
| Commands | 7 | 7 |
| Timers | 5 | 6 |
| Files | 7 | 7 |
| Queues | 5 | 5 |
| Groups | 6 | 5 |
| Plugins | 7 | 6 |
| Servers | 6 | 6 |
| Stream Apps | 5 | 6 |
| Integrations | 6 | 5 |
| Donations | 5 | 6 |
| Globals | 7 | 7 |
| History | 6 | 6 |
| Settings | 7 | 6 |
| MCP | 6 | 5 |
| Log | 8 | 7 |
| Crashes | 7 | 7 |
| Backups | 7 | 7 |

Initial captures and the detailed visual report are in `/tmp/streamerz-ui-baseline/`. The baseline
uses the fast-profile binary built before these changes, with isolated English settings and no live
connections. After visiting every page and hiding the window, it used 44.3 MB RSS (9.1 MB anonymous),
25.1 MB PSS and zero CPU ticks over 30 seconds. This is only a comparable disconnected development
baseline, not proof of the release budget with Twitch and OBS connected.

## Implementation and verification

First implementation pass:

- Restored keyboard focus and sidebar arrow/Home/End navigation with automatic reveal.
- Disabled generic list selection actions until a row is selected, including right-click menus.
- Shared responsive column geometry across headers, row drawing and pointer targets. Values truncate
  with full display text on hover; disabled icons keep reserved space.
- Stacked Actions panes at narrow widths without replacing the widgets or losing selection.
- Preserved form drafts on transfer to JSON and through failed validation.
- Measured wrapped help on resize and recalculated remaining list space after the layout pass.
- Added missing compact action icons and replaced the timer header's raw interpolation template.

The current capture matrix in `/tmp/streamerz-ui-current/` covers all 22 pages in both themes at
both sizes, plus a populated Actions editor selected with the keyboard. The full smoke suite passes,
including sidebar keyboard navigation and generic list selection-state checks. Shared UI tests,
the hidden-display draft collection test and the wrapped-help layout test pass. Formatting and
clippy pass. Updated independent scores are still required.

Disconnected development samples after this pass: 51,744 KiB RSS in dark mode and 52,024 KiB in
light mode, each with zero CPU ticks over five seconds. These samples are not a matched performance
comparison: the earlier binary predates the current base commit, and retained chat also differs.
Establish a matched baseline and investigate RSS before concluding that the performance gate passes.

- [ ] Visible keyboard focus, sidebar keyboard navigation and automatic reveal of selected pages.
- [ ] Selection actions disabled when no eligible item is selected, including context menus.
- [ ] Shared column geometry for headers, values and hit testing, with readable overflow access.
- [ ] Responsive Actions panes and page-level controls.
- [ ] Preserve unsaved fields when switching to JSON; keep invalid entries in the open form.
- [ ] Consistent translated text, Health states, error feedback and right-click actions.

## Evidence required for a passing score

UI needs readable hierarchy, consistent shared tokens and icons, alignment, contrast and usable
content at narrow and wide sizes. UX needs discoverable primary tasks, visible keyboard focus,
appropriate selection actions, useful empty/loading/error states, reversible cancellation,
clear validation and safe destructive operations. Simplicity alone is not a defect.

Review each page in dark and light themes, at 540 by 700 and 1100 by 800, with empty and populated
content where relevant. Inspect app-window captures from the development `ui` tool and exercise
the main task, keyboard navigation, context menus, cancellation and invalid input. Record each
remaining issue before assigning updated scores. Unverified pages remain open.

Major secondary surfaces also need review: action editor, trigger/sub-action picker, property form,
JSON editor, import/export, group settings/files/membership, arguments, channel search, account setup,
connection popup, user card, chat account picker, message-all composer, emote picker, templates,
hotkey capture and notices. Passing page screenshots do not establish that these workflows pass.

Run meaningful regression tests for fixed bugs, the full smoke test and the required Rust checks.
Record comparable hidden RSS and idle CPU before and after. A disconnected test instance cannot
establish the Twitch and OBS performance budget.
