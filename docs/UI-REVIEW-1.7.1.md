# Streamerz 1.7.1 UI and UX proposal

Status: approved for implementation on 2026-11-14. This consolidates the two rounds of
UI, UX and newcomer review. The goal is a minimal interface that explains what is happening and why,
and completes each task with the fewest clicks. All work stays on `release/1.7.1`.

## Findings checked against the current source

- Command replies already exist. `src/automation/entities.rs` places Reply text near the bottom of
  24 fields. The fix is to expose the existing feature, without generating hidden reply actions.
- `frontend/src/controls/chat.rs` checks history-off before disconnected-empty, hiding setup guidance
  on a fresh install. Both facts must be explained together.
- `src/core/accounts.rs` constructs Connect even when setup is missing. Reading a channel and signing
  in to send are distinct capabilities; anonymous reading must remain available where supported.
- Timer defaults are disabled and their form has no action link. The Timers table's Runs value is an
  execution counter, not a linked action. Enabling by default alone would not complete the task.
- The Actions tree width and toolbar folding threshold hide labels even in a wide window. Fix the
  shared layout policy rather than adding a narrow special case or widening one pane.
- Form buttons put the commit action before Cancel, and Cancel closes directly after emitting its
  event. Check the existing subscribers before changing dismissal so cleanup happens exactly once.
- Schema translation handles only selected key families. Plain English entity labels bypass it.

Earlier commits already address dialog titles and long table-header truncation. They are not new work
in this proposal. Their existing fixes do not establish that every dialog or table is correct.

The reports used different C-number mappings and some incomparable click totals. The R identifiers
below are the canonical implementation references. Estimates are targets, not verified results.

## Proposed implementation order

### Phase A: shared controls and immediate dead ends

| ID | Change | Expected task effect | Source findings |
|---|---|---|---|
| R-01 | Give Chat one state region that explains missing connections and disabled history together. Offer Turn on history and the existing account/channel setup routes. Add a contextual Accounts route to blocked Polls. | Enable history from open Chat: 3 clicks to 1. Accounts remains directly accessible. | UI-22, UX-01, NEW-01/11 |
| R-02 | Put Command and Reply first, focus Command, derive a new command's name unless overridden. Group matching, counters and permissions, summarizing nondefault values on edit. Keep sending scope visible. | Basic reply creation: target 5 clicks to 4, with no scrolling to find Reply. | UI-02, UX-05, NEW-04/15 |
| R-03 | Use the same Cancel-then-commit footer everywhere, name the main action, and unify dismissal with exactly-once cleanup. Keep Edit JSON one button away at the left. | Same click count, consistent operation and reliable reopen. | UI-01/13, UX focus finding |
| R-04 | Replace truly empty table/editor furniture with a short explanation and direct creation actions. Keep New, Templates and Import visible on empty Actions; preserve filtered-empty lists and useful unavailable-service explanations. | Creation remains one click, with no hover search through disabled icons. | UI-04/09/21, NEW-07 |
| R-05 | Fix shared toolbar priorities and folding. Keep New, Templates and Test direct; label setup actions. Add an action-row enable switch and retain matching context menus. | Toggle an action: 2 clicks to 1. Common operations stay direct. | UI-09/11/17/23/24, UX-10, NEW-07/14 |

### Phase B: complete setup and automation tasks

| ID | Change | Expected task effect | Source findings |
|---|---|---|---|
| R-06 | Compact unset Accounts cards, distinguish channel values from Change actions, and replace unavailable Connect with a relevant setup action and reason. Explain TikTok browser capabilities without blocking exact-handle native reading. Offer a named browser shortcut only when its profile scope is known. | Setup entry stays direct; remove enabled actions that cannot work. Browser/profile consent remains explicit. | UI-05/06/13/24, UX-08, NEW-02/03 |
| R-07 | Add Turn on overlay server at the prerequisite notice. Let Examples finish with Add to OBS, showing server effects, current bind address, OBS connection and scene before commitment. Retain Add only, preserve existing server settings, select the created overlay and report partial failures. Fix the wrong Stream Apps guidance. | Enable-only detour: 3 clicks to 1. Example to OBS: target 7 to 4 with one ready OBS destination. | UX-04, UI-23, NEW-06/13 |
| R-08 | Combine type selection and fields in one authoring dialog; add no-settings types without an empty second confirmation. Keep descriptions and insertable event variables beside the fields. | Target follow action with two steps and Test: 22 clicks to 15, subject to a comparable trace. | UX-03, UI-10 |
| R-09 | Show the selected action's last test outcome beside Test and link Details to that exact run. Use actual execution evidence, distinguish skips/failure, and never equate local completion with remote delivery. | Outcome: no navigation after Test. Full details: target 3 clicks to 1. | UX-02 |
| R-10 | Add distinct unset and attention health states. Retain a bounded, redacted last connection failure and locale-formatted time beside Retry, using existing events. | Read the cause on the current card without searching Log. | UI-07, UX-06 |
| R-11 | Give timers an action-link control, conditional random bounds and human durations. Offer explicit Create and enable and a disabled draft path. Preserve existing multiple trigger links and imported fields; save linkage atomically through core and MCP. | Complete timer setup on its own form. Measure the current full trace before claiming a saving. | UI-03, NEW-05 |
| R-12 | Make template platform choices visible, initially based on configured accounts. Include TikTok only for verified supported types. Expand and select the result, collect required settings, then offer Save draft or Save and enable. | Remove post-creation hunting and deleting unwanted platform steps; no silent activation. | NEW-09/10 |

### Phase C: consistency and recovery

| ID | Change | Expected task effect | Source findings |
|---|---|---|---|
| R-13 | Inline notification switches, unify Chat settings entry points, group version with updates and show the language restart action. Expose nested settings on Settings before deciding whether search is still needed. | Notification toggle on Settings: 3 clicks to 1. | UI-14/15, UX-07 |
| R-14 | Localize every touched schema/template label and new default text. Allocate table space by content and priority, keeping hidden permissions/destinations in keyboard-accessible selected-row details. Reuse search, selection, log-spacing and theme patterns. | Readable narrow layouts and Portuguese forms; no claimed intrinsic click reduction. | UI-02/16/18/19/20/26, NEW-10/13/15 |
| R-15 | Name Restore and restart and disclose actual backup scope, exclusions and safety snapshot. Use visible shared Community tabs without relocating Groups. Remove permanent Save only after verifying flush and failure/retry behavior. | Restore keeps its click count with clear effects. Community switching: 2 clicks to 1. | UX-09, UI-08, NEW-12 |

Effort: R-03 is small but requires lifecycle verification. Most other items are medium shared-control
or core changes. R-09 can be large if current execution records cannot represent reliable step outcomes.
R-11 is core linkage work, not just adding a dropdown. Implement and verify one phase at a time.

## Decisions and rejected shortcuts

- Keep direct sidebar access to History, Log, Servers, Backups and Groups. A collapsed More menu adds
  a click. No page relocation or automatic hiding is proposed.
- Keep history retention opt-in. A clearer empty state does not justify changing its memory cost.
- Keep Streamer.bot concepts and wire identifiers. Clarify user-group permission labels according to
  Allow or Deny; do not rename configurable action packages as simple folders.
- Preserve anonymous reading and disabled drafts. Do not silently enable incomplete timers, templates,
  servers or integrations to make setup appear shorter.
- Keep primary operations visible. Overflow can organize secondary actions, but each moved operation
  costs an extra click and must be justified by actual use, not visual tidiness alone.
- Keep consent, sending scope, restart consequences and manual setup steps visible where needed.
  Tooltips supplement them rather than becoming their only location.
- OBS configuration discovery can follow R-06 if needed. Read existing local configuration on demand;
  do not add polling, change OBS settings or connect merely because it was detected.
- Do not reclassify graphics-driver errors based only on noisy headless screenshots. Establish the
  actual severity before changing logging.

## Acceptance and version gates

Implementation must update the relevant patterns in `docs/UI.md`, reuse shared controls, keep rules in
the toolkit-independent core, and expose driven operations through MCP with regression coverage.
No new dependency, thread, timer or background poll is planned.

For each phase, record comparable task traces using the same starting state and input. Count field
focus, selections and menu openings; double-clicks count twice. Separate external authorization from
in-app clicks. Verify fresh, populated, filtered-empty, disconnected, failure, imported and edit states.

Check keyboard navigation, Cancel/X/Escape, reopen, pending operations, matching right-click menus,
EN/PT-BR and narrow/wide layouts on `DISPLAY=:99`, using the app's MCP UI tools and screenshots.
Add a unit regression for each bug where possible, otherwise a smoke step. Keep review fixtures isolated;
do not log in, install browser components, send messages or start public streams during review testing.

Before a push or version close, run applicable shared checks through `python3 -P dev/check.py`, frontend
checks, the full UI smoke after the fast dev build, Windows checks and release packaging. Record exact
commands/results in `docs/TESTING.md`. Report unavailable checks explicitly, including native Windows
runtime separately from cross-compilation. Measure hidden RSS and idle CPU before and after with Twitch
and OBS connected; disconnected samples cannot establish the connected budget. Follow existing artifact
reuse and automatic release rules. This proposal neither closes the version nor authorizes a push.

## Review status

The three implementation phases are in place. Validation results and remaining release gates are
recorded in `docs/TESTING.md`. Settings search is included: inlining the options made the page long
enough to justify it. The sidebar keeps direct access to every page.
The approved scope includes related fixes found during implementation and a separate investigation of
extension installation, browser connection, pairing and version compatibility. Installation evidence
must be distinguished from an active connection; unavailable detection must not be shown as absence.
The investigation was checked against the current source: stable development extension folders,
socket-level connection state and protocol-range checks already existed. The remaining fixes cover
installation evidence, disabled copies, explicit pairing precedence, refusal feedback and IPv6 URLs.
