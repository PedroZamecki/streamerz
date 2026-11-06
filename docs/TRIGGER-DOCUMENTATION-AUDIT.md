# Trigger configuration and documentation audit

Inspected on 2026-11-05. This is a code and reference audit, not a live TikTok test.
The implementation proposals below are not completed work.

## Confirmed problems

1. Every trigger's JSON editor uses `actions.trigger_hint`, which claims its fields are in
   Streamer.bot's docs (`src/ui/actions.rs`, `edit_trigger`; `assets/locales/en_US.json`). This is
   misleading for Streamerz extensions and does not explain any configuration.
2. TikTok Gift (41002) has no form in `src/ui/forms.rs::trigger_fields`. Editing falls back to JSON.
   All twelve TikTok triggers currently fall through to that fallback, including events with no
   configurable fields. A no-settings event should show its explanation and arguments, with JSON
   available separately.
3. TikTok events call `engine.fire` with `|_| true` (`src/tiktok.rs::handle_tiktok_event`). A gift ID
   put into a trigger's JSON does not filter gifts. Adding a form alone would not fix this.
4. The native decoder already supplies `giftId`, `giftName`, `giftPictureUrl`, `giftType`,
   `diamondCount`, `repeatCount` and `repeatEnd` (`src/webcast.rs`, `WebcastGiftMessage`). Exact gift
   identity is available today. What is missing is configuration, matching and a catalog.
5. Gift events fire for every combo update. `coins` is unit value times the cumulative repeat count.
   Streamerz's own totals count a type-1 gift only at its end, but an action adding `coins` on every
   event can count the same gifts repeatedly. The reference describes this behavior but offers no
   configuration to avoid it (`docs/reference/triggers/tiktok-gifts.md`).
6. No room gift-list request was found in the TikTok reader, browser bridge or extension. Received
   gifts supply metadata about themselves; they do not enumerate gifts that nobody has sent yet.
7. Reference pages are embedded and exposed through MCP, but the Actions details and editor do not
   use them. Descriptions in `src/ui/picker.rs`, fields in `src/ui/forms.rs`, editor hints and Markdown
   are separate sources that can disagree. Picker descriptions for 706 and 707 still name
   Streamer.bot as the application starting or stopping.
8. The reference coverage test in `src/docs.rs` checks existence and duplicates, not completeness or
   agreement with runtime behavior. Excluding the README example, a heading inventory found 454
   sub-action and 497 trigger entries.
   Of these, 67 lack a Fields heading, 68 lack Arguments, and 22 triggers lack When. Some describe
   unsupported operations or use another structure; these counts are review candidates, not proof
   that every such entry lacks the information. No entry has a dedicated Example heading.
9. Most form inputs fall back to their label as their tooltip (`src/ui/forms.rs`). A label such as
   Count does not explain units, defaults, boundaries or how variables are evaluated.

## Dynamic gifts: proposed implementation

The upstream TikTok-Live-Connector implementation requests signed `webcast/gift/list/` with
`room_id`, returning `data.gifts`:
[fetch-room-gifts.ts](https://github.com/zerodytrash/TikTok-Live-Connector/blob/ts-rewrite/src/lib/web/routes/base/fetch-room-gifts.ts).
This establishes a practical route, not a guarantee that it works in every account or region.
Streamer.bot's own [platform support page](https://docs.streamer.bot/faq/platform-support) describes
TikTok as a third-party integration, not a native feature documented by its trigger reference.

Use the existing browser bridge's signed fetch path with the selected live room and browser context.
Parse IDs, names, unit coin values, combo capability and image URLs. Keep the catalog scoped to the
room and account context; invalidate it on a room/account change and ignore late replies from the
old context. Fetch on room discovery when the bridge is ready, on explicit refresh and when a
previously unavailable bridge becomes ready. No new recurring poll or worker is needed.

The existing `catalog.rs` and `Suggest` field cover ID/name choices for rewards and OBS items. Reuse
that mechanism, extending the shared catalog only where gift metadata and availability need it.
Store the selected gift ID, with a display name as a convenience. Names and translated labels must
not determine exact identity. Keep an unavailable saved selection intact. Let users enter an ID if
the list cannot be loaded. Receiving an unknown gift should enrich the current catalog immediately.

Show loading, unavailable, failed and loaded-empty states separately. Catalog failure must not stop
gift reception or existing actions. Do not require extension setup merely to keep public reading
working. Preserve the current consent requirement for installing or changing browser extensions.

New gift entries can then appear without a Streamerz release. A TikTok protocol or signing change
can still require a connector update; dynamic data cannot prevent that.

## Gift form and event behavior

Proposed controls:

- Gift: any gift, a detected gift, or a manually entered ID.
- Delivery: completed gift/combo, or every combo update.
- Optional repeat-count and coin-value bounds, with explicit units and inclusive limits.
- A preview explaining the resulting rule and a realistic test event.

Keep existing triggers without new fields behaving as before. Recommend completed combos for newly
created triggers, with a visible explanation. Ordinary non-combo gifts must fire in that mode too.
Show unit value separately from combo total; retain existing argument names for compatibility.
Do not silently reinterpret `coins` as an incremental amount.

Decisions to confirm before implementation: the default for newly created triggers; whether coin
bounds apply to unit price or combo total; and whether multiple gifts belong in one trigger or are
represented by separate triggers. A selector and completed-combo option are useful independently of
those additional filters.

Review event identity before promising duplicate suppression or per-repeat delivery. The current
gift decoder does not expose an explicit combo identity in its gift-specific fields. A viewer plus
gift ID alone cannot safely distinguish successive or overlapping combos. Do not invent timers to
guess that a streak has finished. Test the available wire identifiers and final-event behavior first.

## Documentation and forms across the app

Use the embedded Streamerz reference for an in-app Help view reachable from the picker, editor and
selected item. Show a short explanation first, then configuration, arguments, examples and limits.
Keep Help inside the app window using the shared dialog and scrolling blocks. JSON remains an
advanced option. State clearly when something is unsupported or only preserved for import/export.

Each supported entry should explain:

- What it does, or exactly when it fires.
- What to connect or enable, and which account performs it.
- Every field's meaning, default, units, accepted values and blank-value behavior.
- Arguments read and produced, with types and a small realistic example.
- Repeat behavior, errors, reconnect effects and relevant platform limitations.
- A short setup example with an observable result.

Keep runtime field definitions and documentation aligned. Extend coverage checks to require an
explicit form or an intentional no-settings state for supported types, and to compare documented
configuration keys with the editor schema. Test unsupported status separately. Require meaningful
sections and examples, while manually checking claims against connectors and engine code.

Avoid building a second complete help database in RAM. `docs::entry` currently parses and allocates
every entry before finding one. Locate the requested section directly or generate a static index at
build time before using this path routinely in the UI. Retain compiled text and allocate only the
selected content. Measure the resulting memory and idle CPU changes.

## Implementation order and verification

1. Remove external-documentation claims. Connect the shared editor and picker to built-in Help,
   distinguish no-settings from missing forms, and correct application names in descriptions.
2. Implement and fixture-test room gift discovery, stale-response rejection, metadata normalization
   and catalog states. Verify the signed request against a real live before declaring it supported.
3. Add the gift form and real trigger matching, preserve legacy behavior and document combo handling.
   Expose catalog read/refresh through MCP and make test events use the connector matching path.
4. Audit all supported trigger/sub-action entries and forms, by family, with a finite inventory.
   Unsupported entries need an honest limitation and preservation behavior, not invented controls.
5. Add regression checks for gift ID mismatch, renamed/unknown gifts, no bridge, failed/empty
   catalogs, room/account switches, combo updates and final events, ordinary gifts, filter boundaries,
   and legacy triggers. Verify that generic FireTrigger testing cannot falsely imply connector
   filtering was exercised.
6. Check Help and gift forms narrow and wide on DISPLAY=:99, including keyboard access, validation,
   cancellation and unavailable data. Run the smoke suite, relevant unit/scenario/extension tests,
   formatting and clippy. Record live-service and Windows limits and comparable hidden RSS/idle CPU
   measurements before closing the version.

## Audit limits

No runtime changes were made for this audit. No live gift catalog was fetched, no real gift was sent,
and no performance improvement or delivery guarantee has been measured. Existing uncommitted work
was inspected without alteration. Implementation choices above remain proposals where noted.
