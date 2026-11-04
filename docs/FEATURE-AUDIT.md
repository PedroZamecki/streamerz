# Feature audit and proposed plan

Inventory of the current working tree, 2026-10-31. This is a documentation-led audit, not proof of runtime behavior. Every trigger and sub-action section in `docs/reference` is listed below. The reference describes Streamerz, not the full current Streamer.bot or platform API. A flagged entry contains an explicit limitation matching the audit search; an unflagged entry still requires implementation and live-service verification. Other limitations may exist in the linked page. The working tree contains unreleased changes, so results can differ from the last release.

## Verification evidence, 2026-10-31

- `cargo fmt --check` and `cargo clippy --all-targets -- -D warnings` passed on this working tree.
- The Windows MSVC target check with `fltk/fltk-bundled` passed, with unused `mut` warnings in `src/storage.rs:42` and `src/main.rs:976`. This is a type check, not a Windows runtime test.
- `dev/smoke.py target/debug/Streamerz` passed on `DISPLAY=:99`, including its page and user-card checks with simulated events.
- The scenario coverage generator reports 5 of 476 triggers and 20 of 405 sub-actions exercised by scenarios. Other unit tests cover additional paths. Scenarios alone do not verify real transports.
- The full test suite did not finish: FLTK icon/image tests waited indefinitely without a display, and a serial rerun on `DISPLAY=:99` also stalled in the icon test. Both were interrupted after the localization failure was observed.
- The unused `chat.read_only_suffix` translations were removed; the localization test now passes. With the stalled icon and image tests excluded, the remaining suite passed: 269 passed, 5 ignored, 8 filtered out.
- MIDI output functions in `src/inputs.rs:98` only write debug messages; they do not send MIDI. The MIDI input, file watcher, file tail and process trigger dispatch functions in `src/engine.rs:829` have no callers outside their direct unit tests. The tests exercise dispatch, not an actual input source. `docs/INTEGRATIONS.md` and the checked phase entries therefore overstate these features.
- The real Streamer.bot installation expected at `~/Games/streamerbot` and OBS were not found in this environment. Live service behavior, actual device I/O, performance budget, Windows runtime, and parity with the real application are not verified here.
- Current [Streamer.bot API index](https://docs.streamer.bot/api) lists 457 sub-actions and 467 triggers. Its catalog must be compared item by item, including version and scope; the 881 local entries are not a parity measure.

## Findings

- The phase checkboxes in PLAN.md are not sufficient evidence of feature completeness. The reference reports missing MIDI input, file watcher, file tail and process triggers despite phases 15 and 17 being marked complete. docs/INTEGRATIONS.md also describes these as working. Reconcile each claim with code and tests before changing its status.
- Twitch, YouTube, OBS and Streamlabs Desktop each have some triggers documented as not firing. TikTok and Kick have documented limitations too. Check each flagged item against its actual dispatch or execution path and platform behavior.
- Many missing Elgato and other third-party integration events belong to integrations explicitly excluded by the agreed plan. Do not silently expand scope.
- Viewer counts and real keyring and Windows isolation checks remain open in PLAN.md. Existing checked phases also need evidence before declaring parity.
- COVERAGE.md, the reference, the plan and integration guide can disagree. A catalog entry, editor form or scenario injection alone does not demonstrate that a real event arrives or that a remote action succeeds.

## Proposed sequence for approval

1. Establish a baseline: inventory each feature against current Streamer.bot documentation and the real application, record version, event arguments, platform permissions and scope. Resolve stale or conflicting documents. Preserve unsupported imports and document intentional differences.
2. Audit the five agreed core integrations feature by feature: trace event transport to dispatch and actions to remote response, compare argument and error behavior, then test available services. Add fixtures and focused regression tests. Track unavailable live cases explicitly.
3. Reconcile and implement agreed later features already marked complete, starting with MIDI input, file tail and watchers, and process triggers. Confirm operating-system and resource constraints before selecting transports.
4. Verify TikTok and the agreed donation, shop, remote and input integrations individually. Separate unsupported provider APIs, configuration limitations and defects. Verify disabled integrations create no socket, task or timer.
5. Verify engine, storage, import/export, servers, UI and MCP paths; run scenarios, full local checks, Windows type-check and hidden-display smoke tests. Measure hidden RSS and idle CPU with Twitch and OBS connected.
6. Review remaining gaps with the owner, prioritize versions and update PLAN.md after decisions. Any excluded integration or new dependency requires an explicit decision. Do not mark a phase complete until its stated acceptance checks pass.

## Feature inventory

Each row links to the source page. `Flagged` means the reference contains a matching explicit missing or unsupported statement; `Review` means no such statement was detected, not that the feature was verified. Codes are Streamerz type codes.

### Triggers

| Category | Feature | Code | Reference status |
| --- | --- | --- | --- |
| [core-application-lifecycle](reference/triggers/core-application-lifecycle.md) | Streamerbot Started | 706 | Review |
| [core-application-lifecycle](reference/triggers/core-application-lifecycle.md) | Streamerbot Exiting | 707 | Flagged: Not yet. Streamerz doesn't fire this trigger. |
| [core-application-lifecycle](reference/triggers/core-application-lifecycle.md) | Toast Activation | 708 | Flagged: Not yet. Streamerz doesn't fire this trigger. |
| [core-application-lifecycle](reference/triggers/core-application-lifecycle.md) | Application Action Added | 9001 | Flagged: Not yet. Streamerz doesn't fire this trigger. |
| [core-application-lifecycle](reference/triggers/core-application-lifecycle.md) | Application Action Updated | 9002 | Flagged: Not yet. Streamerz doesn't fire this trigger. |
| [core-application-lifecycle](reference/triggers/core-application-lifecycle.md) | Application Action Deleted | 9003 | Flagged: Not yet. Streamerz doesn't fire this trigger. |
| [core-application-lifecycle](reference/triggers/core-application-lifecycle.md) | Application Import | 9004 | Flagged: Not yet. Streamerz doesn't fire this trigger. |
| [core-application-lifecycle](reference/triggers/core-application-lifecycle.md) | Application Command Added | 9005 | Flagged: Not yet. Streamerz doesn't fire this trigger. |
| [core-application-lifecycle](reference/triggers/core-application-lifecycle.md) | Application Command Updated | 9006 | Flagged: Not yet. Streamerz doesn't fire this trigger. |
| [core-application-lifecycle](reference/triggers/core-application-lifecycle.md) | Application Command Deleted | 9007 | Flagged: Not yet. Streamerz doesn't fire this trigger. |
| [core-chat-window](reference/triggers/core-chat-window.md) | Chat Window Action | 705 | Flagged: Not yet. Streamerz doesn't fire this trigger. |
| [core-commands](reference/triggers/core-commands.md) | Command Triggered | 401 | Review |
| [core-commands](reference/triggers/core-commands.md) | Command Cooldown | 402 | Flagged: Starts its actions when someone uses a command that is still on cooldown, so it doesn't run. |
| [core-custom-scripting](reference/triggers/core-custom-scripting.md) | Custom | 1 | Flagged: Meant for custom events. Streamerz doesn't fire it yet. |
| [core-custom-scripting](reference/triggers/core-custom-scripting.md) | Test | 702 | Review |
| [core-custom-scripting](reference/triggers/core-custom-scripting.md) | Raw Action | 801 | Flagged: Streamerz doesn't fire this yet. |
| [core-custom-scripting](reference/triggers/core-custom-scripting.md) | Raw Sub Action | 802 | Flagged: Streamerz doesn't fire this yet. |
| [core-custom-scripting](reference/triggers/core-custom-scripting.md) | Raw Action Completed | 806 | Flagged: Streamerz doesn't fire this yet. |
| [core-custom-scripting](reference/triggers/core-custom-scripting.md) | Custom Event | 18001 | Review |
| [core-custom-scripting](reference/triggers/core-custom-scripting.md) | Custom Code Event | 18002 | Review |
| [core-globals](reference/triggers/core-globals.md) | Global Variable Updated | 709 | Review |
| [core-globals](reference/triggers/core-globals.md) | User Global Variable Updated | 710 | Review |
| [core-globals](reference/triggers/core-globals.md) | Global Variable Created | 711 | Review |
| [core-globals](reference/triggers/core-globals.md) | Global Variable Deleted | 712 | Review |
| [core-quotes](reference/triggers/core-quotes.md) | Quote Added | 601 | Review |
| [core-quotes](reference/triggers/core-quotes.md) | Quote Show | 602 | Flagged: It doesn't fire yet (see Notes). |
| [core-system-clipboard](reference/triggers/core-system-clipboard.md) | Clipboard Changed | 42001 | Flagged: It never fires yet. |
| [core-timers](reference/triggers/core-timers.md) | Timed Action | 701 | Review |
| [core-user-groups](reference/triggers/core-user-groups.md) | Group Added | 32001 | Review |
| [core-user-groups](reference/triggers/core-user-groups.md) | Group Removed | 32002 | Review |
| [core-user-groups](reference/triggers/core-user-groups.md) | Group Cleared | 32003 | Review |
| [core-user-groups](reference/triggers/core-user-groups.md) | Group Users Added | 32004 | Review |
| [core-user-groups](reference/triggers/core-user-groups.md) | Group Users Removed | 32005 | Review |
| [donations-shops-fourthwall](reference/triggers/donations-shops-fourthwall.md) | Fourthwall Product Created | 24001 | Review |
| [donations-shops-fourthwall](reference/triggers/donations-shops-fourthwall.md) | Fourthwall Product Updated | 24002 | Review |
| [donations-shops-fourthwall](reference/triggers/donations-shops-fourthwall.md) | Fourthwall Gift Purchase | 24003 | Review |
| [donations-shops-fourthwall](reference/triggers/donations-shops-fourthwall.md) | Fourthwall Order Placed | 24004 | Review |
| [donations-shops-fourthwall](reference/triggers/donations-shops-fourthwall.md) | Fourthwall Order Updated | 24005 | Review |
| [donations-shops-fourthwall](reference/triggers/donations-shops-fourthwall.md) | Fourthwall Donation | 24006 | Review |
| [donations-shops-fourthwall](reference/triggers/donations-shops-fourthwall.md) | Fourthwall Subscription Purchased | 24007 | Review |
| [donations-shops-fourthwall](reference/triggers/donations-shops-fourthwall.md) | Fourthwall Subscription Expired | 24008 | Review |
| [donations-shops-fourthwall](reference/triggers/donations-shops-fourthwall.md) | Fourthwall Subscription Changed | 24009 | Review |
| [donations-shops-fourthwall](reference/triggers/donations-shops-fourthwall.md) | Fourthwall Thank You Sent | 24010 | Review |
| [donations-shops-fourthwall](reference/triggers/donations-shops-fourthwall.md) | Fourthwall Newsletter Subscribed | 24011 | Review |
| [donations-shops-fourthwall](reference/triggers/donations-shops-fourthwall.md) | Fourthwall Gift Draw Started | 24012 | Review |
| [donations-shops-fourthwall](reference/triggers/donations-shops-fourthwall.md) | Fourthwall Gift Draw Ended | 24013 | Review |
| [donations-shops-ko-fi](reference/triggers/donations-shops-ko-fi.md) | Kofi Donation | 7001 | Review |
| [donations-shops-ko-fi](reference/triggers/donations-shops-ko-fi.md) | Kofi Subscription | 7002 | Review |
| [donations-shops-ko-fi](reference/triggers/donations-shops-ko-fi.md) | Kofi Resubscription | 7003 | Review |
| [donations-shops-ko-fi](reference/triggers/donations-shops-ko-fi.md) | Kofi Shop Order | 7004 | Review |
| [donations-shops-ko-fi](reference/triggers/donations-shops-ko-fi.md) | Kofi Commission | 7005 | Review |
| [donations-shops-other-services](reference/triggers/donations-shops-other-services.md) | Donor Drive Donation | 3001 | Review |
| [donations-shops-other-services](reference/triggers/donations-shops-other-services.md) | Donor Drive Profile Updated | 3002 | Flagged: The first check after start doesn't fire this trigger. |
| [donations-shops-other-services](reference/triggers/donations-shops-other-services.md) | Donor Drive Incentive | 3003 | Review |
| [donations-shops-other-services](reference/triggers/donations-shops-other-services.md) | Tipeee Stream Donation | 10001 | Review |
| [donations-shops-other-services](reference/triggers/donations-shops-other-services.md) | Treat Stream Treat | 11001 | Review |
| [donations-shops-other-services](reference/triggers/donations-shops-other-services.md) | Treat Stream Authenticated | 11002 | Flagged: Not yet. Streamerz doesn't fire this trigger. |
| [donations-shops-other-services](reference/triggers/donations-shops-other-services.md) | Treat Stream Connected | 11003 | Flagged: Not yet. Streamerz doesn't fire this trigger. |
| [donations-shops-other-services](reference/triggers/donations-shops-other-services.md) | Treat Stream Disconnected | 11004 | Flagged: Not yet. Streamerz doesn't fire this trigger. |
| [donations-shops-other-services](reference/triggers/donations-shops-other-services.md) | Pallygg Connected | 28001 | Review |
| [donations-shops-other-services](reference/triggers/donations-shops-other-services.md) | Pallygg Disconnected | 28002 | Review |
| [donations-shops-other-services](reference/triggers/donations-shops-other-services.md) | Pallygg Campaign Tip | 28003 | Review |
| [donations-shops-patreon](reference/triggers/donations-shops-patreon.md) | Patreon Follow Created | 8001 | Review |
| [donations-shops-patreon](reference/triggers/donations-shops-patreon.md) | Patreon Follow Deleted | 8002 | Review |
| [donations-shops-patreon](reference/triggers/donations-shops-patreon.md) | Patreon Pledge Created | 8003 | Review |
| [donations-shops-patreon](reference/triggers/donations-shops-patreon.md) | Patreon Pledge Updated | 8004 | Review |
| [donations-shops-patreon](reference/triggers/donations-shops-patreon.md) | Patreon Pledge Deleted | 8005 | Review |
| [donations-shops-shopify](reference/triggers/donations-shops-shopify.md) | Shopify Order Created | 13001 | Review |
| [donations-shops-shopify](reference/triggers/donations-shops-shopify.md) | Shopify Order Paid | 13002 | Review |
| [donations-shops-streamelements](reference/triggers/donations-shops-streamelements.md) | Tip | 1201 | Review |
| [donations-shops-streamelements](reference/triggers/donations-shops-streamelements.md) | Merch | 1202 | Review |
| [donations-shops-streamelements](reference/triggers/donations-shops-streamelements.md) | Connected | 1203 | Review |
| [donations-shops-streamelements](reference/triggers/donations-shops-streamelements.md) | Disconnected | 1204 | Review |
| [donations-shops-streamelements](reference/triggers/donations-shops-streamelements.md) | Authenticated | 1205 | Review |
| [donations-shops-streamlabs](reference/triggers/donations-shops-streamlabs.md) | Donation | 201 | Review |
| [donations-shops-streamlabs](reference/triggers/donations-shops-streamlabs.md) | Merchandise | 202 | Review |
| [donations-shops-streamlabs](reference/triggers/donations-shops-streamlabs.md) | Connected | 203 | Review |
| [donations-shops-streamlabs](reference/triggers/donations-shops-streamlabs.md) | Disconnected | 204 | Review |
| [donations-shops-streamlabs](reference/triggers/donations-shops-streamlabs.md) | Charity Donation | 205 | Review |
| [donations-shops-streamlabs](reference/triggers/donations-shops-streamlabs.md) | Authenticated | 206 | Review |
| [donations-shops-streamloots](reference/triggers/donations-shops-streamloots.md) | Stream Loots Pack Purchased | 36001 | Review |
| [donations-shops-streamloots](reference/triggers/donations-shops-streamloots.md) | Stream Loots Pack Gifted | 36002 | Review |
| [donations-shops-streamloots](reference/triggers/donations-shops-streamloots.md) | Stream Loots Card Redeemed | 36003 | Review |
| [donations-shops-streamloots](reference/triggers/donations-shops-streamloots.md) | Stream Loots Reaction | 36004 | Review |
| [donations-shops-throne](reference/triggers/donations-shops-throne.md) | Throne Gift Purchased | 37001 | Review |
| [donations-shops-throne](reference/triggers/donations-shops-throne.md) | Throne Contribution Purchased | 37002 | Review |
| [donations-shops-throne](reference/triggers/donations-shops-throne.md) | Throne Gift Crowdfunded | 37003 | Review |
| [elgato-camera-hub](reference/triggers/elgato-camera-hub.md) | Camera Hub Connected | 21301 | Flagged: Never for now. Streamerz doesn't fire this trigger, so it never starts an action. |
| [elgato-camera-hub](reference/triggers/elgato-camera-hub.md) | Camera Hub Disconnected | 21302 | Flagged: Never for now. Streamerz doesn't fire this trigger, so it never starts an action. |
| [elgato-camera-hub](reference/triggers/elgato-camera-hub.md) | Webcam Connected | 21341 | Flagged: Never for now. Streamerz doesn't fire this trigger, so it never starts an action. |
| [elgato-camera-hub](reference/triggers/elgato-camera-hub.md) | Webcam Disconnected | 21342 | Flagged: Never for now. Streamerz doesn't fire this trigger, so it never starts an action. |
| [elgato-camera-hub](reference/triggers/elgato-camera-hub.md) | Activated | 21351 | Flagged: Never for now. Streamerz doesn't fire this trigger, so it never starts an action. |
| [elgato-camera-hub](reference/triggers/elgato-camera-hub.md) | Deactivated | 21352 | Flagged: Never for now. Streamerz doesn't fire this trigger, so it never starts an action. |
| [elgato-camera-hub](reference/triggers/elgato-camera-hub.md) | Selected Webcam Changed | 21353 | Flagged: Never for now. Streamerz doesn't fire this trigger, so it never starts an action. |
| [elgato-camera-hub](reference/triggers/elgato-camera-hub.md) | Mirrored | 21354 | Flagged: Never for now. Streamerz doesn't fire this trigger, so it never starts an action. |
| [elgato-camera-hub](reference/triggers/elgato-camera-hub.md) | Flipped | 21355 | Flagged: Never for now. Streamerz doesn't fire this trigger, so it never starts an action. |
| [elgato-camera-hub](reference/triggers/elgato-camera-hub.md) | Device Orientation Changed | 21356 | Flagged: Never for now. Streamerz doesn't fire this trigger, so it never starts an action. |
| [elgato-camera-hub](reference/triggers/elgato-camera-hub.md) | Exposure Auto Lock Enabled | 21357 | Flagged: Never for now. Streamerz doesn't fire this trigger, so it never starts an action. |
| [elgato-camera-hub](reference/triggers/elgato-camera-hub.md) | Exposure Auto Lock Disabled | 21358 | Flagged: Never for now. Streamerz doesn't fire this trigger, so it never starts an action. |
| [elgato-camera-hub](reference/triggers/elgato-camera-hub.md) | Snapshot Taken | 21359 | Flagged: Never for now. Streamerz doesn't fire this trigger, so it never starts an action. |
| [elgato-camera-hub](reference/triggers/elgato-camera-hub.md) | Zoom Changed | 21360 | Flagged: Never for now. Streamerz doesn't fire this trigger, so it never starts an action. |
| [elgato-camera-hub](reference/triggers/elgato-camera-hub.md) | Webcam Contrast Changed | 21361 | Flagged: Never for now. Streamerz doesn't fire this trigger, so it never starts an action. |
| [elgato-camera-hub](reference/triggers/elgato-camera-hub.md) | White Balance Changed | 21362 | Flagged: Never for now. Streamerz doesn't fire this trigger, so it never starts an action. |
| [elgato-camera-hub](reference/triggers/elgato-camera-hub.md) | Auto Exposure Enabled | 21363 | Flagged: Never for now. Streamerz doesn't fire this trigger, so it never starts an action. |
| [elgato-camera-hub](reference/triggers/elgato-camera-hub.md) | Auto Exposure Disabled | 21364 | Flagged: Never for now. Streamerz doesn't fire this trigger, so it never starts an action. |
| [elgato-camera-hub](reference/triggers/elgato-camera-hub.md) | Auto White Balance Enabled | 21365 | Flagged: Never for now. Streamerz doesn't fire this trigger, so it never starts an action. |
| [elgato-camera-hub](reference/triggers/elgato-camera-hub.md) | Auto White Balance Disabled | 21366 | Flagged: Never for now. Streamerz doesn't fire this trigger, so it never starts an action. |
| [elgato-camera-hub](reference/triggers/elgato-camera-hub.md) | Noise Reduction Enabled | 21367 | Flagged: Never for now. Streamerz doesn't fire this trigger, so it never starts an action. |
| [elgato-camera-hub](reference/triggers/elgato-camera-hub.md) | Noise Reduction Disabled | 21368 | Flagged: Never for now. Streamerz doesn't fire this trigger, so it never starts an action. |
| [elgato-camera-hub](reference/triggers/elgato-camera-hub.md) | ISOChanged | 21369 | Flagged: Never for now. Streamerz doesn't fire this trigger, so it never starts an action. |
| [elgato-camera-hub](reference/triggers/elgato-camera-hub.md) | Shutter Speed Changed | 21370 | Flagged: Never for now. Streamerz doesn't fire this trigger, so it never starts an action. |
| [elgato-camera-hub](reference/triggers/elgato-camera-hub.md) | Sharpness Changed | 21371 | Flagged: Never for now. Streamerz doesn't fire this trigger, so it never starts an action. |
| [elgato-camera-hub](reference/triggers/elgato-camera-hub.md) | Anti Flicker Changed | 21372 | Flagged: Never for now. Streamerz doesn't fire this trigger, so it never starts an action. |
| [elgato-camera-hub](reference/triggers/elgato-camera-hub.md) | Lens Changed | 21373 | Flagged: Never for now. Streamerz doesn't fire this trigger, so it never starts an action. |
| [elgato-camera-hub](reference/triggers/elgato-camera-hub.md) | ARLens Changed | 21374 | Flagged: Never for now. Streamerz doesn't fire this trigger, so it never starts an action. |
| [elgato-camera-hub](reference/triggers/elgato-camera-hub.md) | Bitrate Changed | 21375 | Flagged: Never for now. Streamerz doesn't fire this trigger, so it never starts an action. |
| [elgato-camera-hub](reference/triggers/elgato-camera-hub.md) | Flash Enabled | 21376 | Flagged: Never for now. Streamerz doesn't fire this trigger, so it never starts an action. |
| [elgato-camera-hub](reference/triggers/elgato-camera-hub.md) | Flash Disabled | 21377 | Flagged: Never for now. Streamerz doesn't fire this trigger, so it never starts an action. |
| [elgato-camera-hub](reference/triggers/elgato-camera-hub.md) | Pan Changed | 21378 | Flagged: Never for now. Streamerz doesn't fire this trigger, so it never starts an action. |
| [elgato-camera-hub](reference/triggers/elgato-camera-hub.md) | Tilt Changed | 21379 | Flagged: Never for now. Streamerz doesn't fire this trigger, so it never starts an action. |
| [elgato-camera-hub](reference/triggers/elgato-camera-hub.md) | Overscan Changed | 21380 | Flagged: Never for now. Streamerz doesn't fire this trigger, so it never starts an action. |
| [elgato-camera-hub](reference/triggers/elgato-camera-hub.md) | Auto Focus Enabled | 21381 | Flagged: Never for now. Streamerz doesn't fire this trigger, so it never starts an action. |
| [elgato-camera-hub](reference/triggers/elgato-camera-hub.md) | Auto Focus Disabled | 21382 | Flagged: Never for now. Streamerz doesn't fire this trigger, so it never starts an action. |
| [elgato-camera-hub](reference/triggers/elgato-camera-hub.md) | Focus Changed | 21383 | Flagged: Never for now. Streamerz doesn't fire this trigger, so it never starts an action. |
| [elgato-camera-hub](reference/triggers/elgato-camera-hub.md) | White Balance Tint Changed | 21384 | Flagged: Never for now. Streamerz doesn't fire this trigger, so it never starts an action. |
| [elgato-camera-hub](reference/triggers/elgato-camera-hub.md) | Webcam Brightness Changed | 21385 | Flagged: Never for now. Streamerz doesn't fire this trigger, so it never starts an action. |
| [elgato-camera-hub](reference/triggers/elgato-camera-hub.md) | Saturation Changed | 21386 | Flagged: Never for now. Streamerz doesn't fire this trigger, so it never starts an action. |
| [elgato-camera-hub](reference/triggers/elgato-camera-hub.md) | Live ISOChanged | 21387 | Flagged: Never for now. Streamerz doesn't fire this trigger, so it never starts an action. |
| [elgato-camera-hub](reference/triggers/elgato-camera-hub.md) | Live Shutter Speed Changed | 21388 | Flagged: Never for now. Streamerz doesn't fire this trigger, so it never starts an action. |
| [elgato-camera-hub](reference/triggers/elgato-camera-hub.md) | Live White Balance Changed | 21389 | Flagged: Never for now. Streamerz doesn't fire this trigger, so it never starts an action. |
| [elgato-camera-hub](reference/triggers/elgato-camera-hub.md) | Live White Balance Tint Changed | 21390 | Flagged: Never for now. Streamerz doesn't fire this trigger, so it never starts an action. |
| [elgato-camera-hub](reference/triggers/elgato-camera-hub.md) | Prompter Connected | 21401 | Flagged: Never for now. Streamerz doesn't fire this trigger, so it never starts an action. |
| [elgato-camera-hub](reference/triggers/elgato-camera-hub.md) | Prompter Disconnected | 21402 | Flagged: Never for now. Streamerz doesn't fire this trigger, so it never starts an action. |
| [elgato-camera-hub](reference/triggers/elgato-camera-hub.md) | Mode Changed | 21403 | Flagged: Never for now. Streamerz doesn't fire this trigger, so it never starts an action. |
| [elgato-camera-hub](reference/triggers/elgato-camera-hub.md) | Prompter Brightness Changed | 21404 | Flagged: Never for now. Streamerz doesn't fire this trigger, so it never starts an action. |
| [elgato-camera-hub](reference/triggers/elgato-camera-hub.md) | Font Changed | 21405 | Flagged: Never for now. Streamerz doesn't fire this trigger, so it never starts an action. |
| [elgato-camera-hub](reference/triggers/elgato-camera-hub.md) | Font Size Changed | 21406 | Flagged: Never for now. Streamerz doesn't fire this trigger, so it never starts an action. |
| [elgato-camera-hub](reference/triggers/elgato-camera-hub.md) | Auto Scroll Enabled | 21407 | Flagged: Never for now. Streamerz doesn't fire this trigger, so it never starts an action. |
| [elgato-camera-hub](reference/triggers/elgato-camera-hub.md) | Auto Scroll Disabled | 21408 | Flagged: Never for now. Streamerz doesn't fire this trigger, so it never starts an action. |
| [elgato-camera-hub](reference/triggers/elgato-camera-hub.md) | Auto Scroll Chapter Enabled | 21409 | Flagged: Never for now. Streamerz doesn't fire this trigger, so it never starts an action. |
| [elgato-camera-hub](reference/triggers/elgato-camera-hub.md) | Auto Scroll Chapter Disabled | 21410 | Flagged: Never for now. Streamerz doesn't fire this trigger, so it never starts an action. |
| [elgato-camera-hub](reference/triggers/elgato-camera-hub.md) | Scroll Speed Changed | 21411 | Flagged: Never for now. Streamerz doesn't fire this trigger, so it never starts an action. |
| [elgato-camera-hub](reference/triggers/elgato-camera-hub.md) | Opacity Changed | 21412 | Flagged: Never for now. Streamerz doesn't fire this trigger, so it never starts an action. |
| [elgato-camera-hub](reference/triggers/elgato-camera-hub.md) | Horizontal Margin Changed | 21413 | Flagged: Never for now. Streamerz doesn't fire this trigger, so it never starts an action. |
| [elgato-camera-hub](reference/triggers/elgato-camera-hub.md) | Vertical Margin Changed | 21414 | Flagged: Never for now. Streamerz doesn't fire this trigger, so it never starts an action. |
| [elgato-camera-hub](reference/triggers/elgato-camera-hub.md) | Line Spacing Changed | 21415 | Flagged: Never for now. Streamerz doesn't fire this trigger, so it never starts an action. |
| [elgato-camera-hub](reference/triggers/elgato-camera-hub.md) | Font Color Changed | 21416 | Flagged: Never for now. Streamerz doesn't fire this trigger, so it never starts an action. |
| [elgato-camera-hub](reference/triggers/elgato-camera-hub.md) | Background Color Changed | 21417 | Flagged: Never for now. Streamerz doesn't fire this trigger, so it never starts an action. |
| [elgato-camera-hub](reference/triggers/elgato-camera-hub.md) | Selected Chapter Changed | 21418 | Flagged: Never for now. Streamerz doesn't fire this trigger, so it never starts an action. |
| [elgato-camera-hub](reference/triggers/elgato-camera-hub.md) | Channels Changed | 21419 | Flagged: Never for now. Streamerz doesn't fire this trigger, so it never starts an action. |
| [elgato-camera-hub](reference/triggers/elgato-camera-hub.md) | Selected Channel Changed | 21420 | Flagged: Never for now. Streamerz doesn't fire this trigger, so it never starts an action. |
| [elgato-camera-hub](reference/triggers/elgato-camera-hub.md) | Prompter Contrast Changed | 21421 | Flagged: Never for now. Streamerz doesn't fire this trigger, so it never starts an action. |
| [elgato-camera-hub](reference/triggers/elgato-camera-hub.md) | Crosshair Enabled | 21422 | Flagged: Never for now. Streamerz doesn't fire this trigger, so it never starts an action. |
| [elgato-camera-hub](reference/triggers/elgato-camera-hub.md) | Crosshair Disabled | 21423 | Flagged: Never for now. Streamerz doesn't fire this trigger, so it never starts an action. |
| [elgato-camera-hub](reference/triggers/elgato-camera-hub.md) | Crosshair Image Changed | 21424 | Flagged: Never for now. Streamerz doesn't fire this trigger, so it never starts an action. |
| [elgato-camera-hub](reference/triggers/elgato-camera-hub.md) | Crosshair Color Changed | 21425 | Flagged: Never for now. Streamerz doesn't fire this trigger, so it never starts an action. |
| [elgato-camera-hub](reference/triggers/elgato-camera-hub.md) | Selected Script Changed | 21426 | Flagged: Never for now. Streamerz doesn't fire this trigger, so it never starts an action. |
| [elgato-stream-deck](reference/triggers/elgato-stream-deck.md) | Action | 17001 | Flagged: Never, for now. Streamerz doesn't fire this yet. |
| [elgato-stream-deck](reference/triggers/elgato-stream-deck.md) | Connected | 17002 | Flagged: Never, for now. Streamerz doesn't fire this yet. |
| [elgato-stream-deck](reference/triggers/elgato-stream-deck.md) | Disconnected | 17003 | Flagged: Never, for now. Streamerz doesn't fire this yet. |
| [elgato-stream-deck](reference/triggers/elgato-stream-deck.md) | Info | 17004 | Flagged: Never, for now. Streamerz doesn't fire this yet. |
| [elgato-stream-deck](reference/triggers/elgato-stream-deck.md) | Mcp Connected | 21501 | Flagged: Never, for now. Streamerz doesn't fire this yet. |
| [elgato-stream-deck](reference/triggers/elgato-stream-deck.md) | Mcp Disconnected | 21502 | Flagged: Never, for now. Streamerz doesn't fire this yet. |
| [elgato-wave-link](reference/triggers/elgato-wave-link.md) | Output Switched | 21001 | Flagged: Never, for now. Streamerz doesn't fire this yet. |
| [elgato-wave-link](reference/triggers/elgato-wave-link.md) | Output Volume Changed | 21002 | Flagged: Never, for now. Streamerz doesn't fire this yet. |
| [elgato-wave-link](reference/triggers/elgato-wave-link.md) | Output Mute Changed | 21003 | Flagged: Never, for now. Streamerz doesn't fire this yet. |
| [elgato-wave-link](reference/triggers/elgato-wave-link.md) | Selected Output Changed | 21004 | Flagged: Never, for now. Streamerz doesn't fire this yet. |
| [elgato-wave-link](reference/triggers/elgato-wave-link.md) | Input Volume Changed | 21005 | Flagged: Never, for now. Streamerz doesn't fire this yet. |
| [elgato-wave-link](reference/triggers/elgato-wave-link.md) | Input Mute Changed | 21006 | Flagged: Never, for now. Streamerz doesn't fire this yet. |
| [elgato-wave-link](reference/triggers/elgato-wave-link.md) | Input Name Changed | 21007 | Flagged: Never, for now. Streamerz doesn't fire this yet. |
| [elgato-wave-link](reference/triggers/elgato-wave-link.md) | Microphone Gain Changed | 21008 | Flagged: Never, for now. Streamerz doesn't fire this yet. |
| [elgato-wave-link](reference/triggers/elgato-wave-link.md) | Microphone Output Volume Changed | 21009 | Flagged: Never, for now. Streamerz doesn't fire this yet. |
| [elgato-wave-link](reference/triggers/elgato-wave-link.md) | Microphone Balance Changed | 21010 | Flagged: Never, for now. Streamerz doesn't fire this yet. |
| [elgato-wave-link](reference/triggers/elgato-wave-link.md) | Microphone Mute Changed | 21011 | Flagged: Never, for now. Streamerz doesn't fire this yet. |
| [elgato-wave-link](reference/triggers/elgato-wave-link.md) | Microphone Setting Changed | 21012 | Flagged: Never, for now. Streamerz doesn't fire this yet. |
| [elgato-wave-link](reference/triggers/elgato-wave-link.md) | Filter Added | 21013 | Flagged: Never, for now. Streamerz doesn't fire this yet. |
| [elgato-wave-link](reference/triggers/elgato-wave-link.md) | Filter Changed | 21014 | Flagged: Never, for now. Streamerz doesn't fire this yet. |
| [elgato-wave-link](reference/triggers/elgato-wave-link.md) | Filter Deleted | 21015 | Flagged: Never, for now. Streamerz doesn't fire this yet. |
| [elgato-wave-link](reference/triggers/elgato-wave-link.md) | Filter Bypass State Changed | 21016 | Flagged: Never, for now. Streamerz doesn't fire this yet. |
| [elgato-wave-link](reference/triggers/elgato-wave-link.md) | Connected | 21017 | Flagged: Never, for now. Streamerz doesn't fire this yet. |
| [elgato-wave-link](reference/triggers/elgato-wave-link.md) | Disconnected | 21018 | Flagged: Never, for now. Streamerz doesn't fire this yet. |
| [elgato-wave-link](reference/triggers/elgato-wave-link.md) | Input Level Meter Changed | 21019 | Flagged: Never, for now. Streamerz doesn't fire this yet. |
| [elgato-wave-link](reference/triggers/elgato-wave-link.md) | Output Level Meter Changed | 21020 | Flagged: Never, for now. Streamerz doesn't fire this yet. |
| [file-process-file-tail](reference/triggers/file-process-file-tail.md) | Changed | 511 | Flagged: It doesn't fire yet (see Notes). |
| [file-process-file-watcher](reference/triggers/file-process-file-watcher.md) | Changed | 501 | Flagged: It doesn't fire yet (see Notes). |
| [file-process-file-watcher](reference/triggers/file-process-file-watcher.md) | Created | 502 | Flagged: It doesn't fire yet (see Notes). |
| [file-process-file-watcher](reference/triggers/file-process-file-watcher.md) | Deleted | 503 | Flagged: It doesn't fire yet (see Notes). |
| [file-process-file-watcher](reference/triggers/file-process-file-watcher.md) | Renamed | 504 | Flagged: It doesn't fire yet (see Notes). |
| [file-process-process-monitor](reference/triggers/file-process-process-monitor.md) | Started | 703 | Flagged: Not yet. Streamerz doesn't fire this trigger. |
| [file-process-process-monitor](reference/triggers/file-process-process-monitor.md) | Stopped | 704 | Flagged: Not yet. Streamerz doesn't fire this trigger. |
| [inputs-midi-hotkeys-mouse](reference/triggers/inputs-midi-hotkeys-mouse.md) | Input Key Press | 16001 | Review |
| [inputs-midi-hotkeys-mouse](reference/triggers/inputs-midi-hotkeys-mouse.md) | Input Mouse Click | 16002 | Review |
| [inputs-midi-midi-controller](reference/triggers/inputs-midi-midi-controller.md) | Midi Message | 15001 | Flagged: Never, for now. Streamerz doesn't fire this yet. |
| [inputs-midi-voice-control](reference/triggers/inputs-midi-voice-control.md) | Speech To Text Dictation | 301 | Flagged: It doesn't fire yet (see Notes). |
| [inputs-midi-voice-control](reference/triggers/inputs-midi-voice-control.md) | Speech To Text Command | 302 | Flagged: It doesn't fire yet (see Notes). |
| [integrations-crowd-control](reference/triggers/integrations-crowd-control.md) | Game Session Start | 20001 | Flagged: Never, for now. Streamerz doesn't fire this yet. |
| [integrations-crowd-control](reference/triggers/integrations-crowd-control.md) | Game Session End | 20002 | Flagged: Never, for now. Streamerz doesn't fire this yet. |
| [integrations-crowd-control](reference/triggers/integrations-crowd-control.md) | Effect Request | 20003 | Flagged: Never, for now. Streamerz doesn't fire this yet. |
| [integrations-crowd-control](reference/triggers/integrations-crowd-control.md) | Effect Success | 20004 | Flagged: Never, for now. Streamerz doesn't fire this yet. |
| [integrations-crowd-control](reference/triggers/integrations-crowd-control.md) | Effect Failure | 20005 | Flagged: Never, for now. Streamerz doesn't fire this yet. |
| [integrations-crowd-control](reference/triggers/integrations-crowd-control.md) | Timed Effect Started | 20006 | Flagged: Never, for now. Streamerz doesn't fire this yet. |
| [integrations-crowd-control](reference/triggers/integrations-crowd-control.md) | Timed Effect Ended | 20007 | Flagged: Never, for now. Streamerz doesn't fire this yet. |
| [integrations-crowd-control](reference/triggers/integrations-crowd-control.md) | Timed Effect Updated | 20008 | Flagged: Never, for now. Streamerz doesn't fire this yet. |
| [integrations-crowd-control](reference/triggers/integrations-crowd-control.md) | Coin Exchange | 20009 | Flagged: Never, for now. Streamerz doesn't fire this yet. |
| [integrations-pulsoid](reference/triggers/integrations-pulsoid.md) | Heart Rate Pulse | 5001 | Flagged: Meant to fire on each new heart rate reading from Pulsoid. Streamerz doesn't fire this yet. |
| [integrations-hyperate](reference/triggers/integrations-hyperate.md) | Heart Rate Pulse | 6001 | Flagged: Meant to fire on each new heart rate reading from HypeRate. Streamerz doesn't fire this yet. |
| [integrations-hyperate](reference/triggers/integrations-hyperate.md) | Twitch Clip Created | 6002 | Flagged: Meant to fire when HypeRate itself creates a clip on Twitch. Streamerz doesn't fire this yet. |
| [integrations-hyperate](reference/triggers/integrations-hyperate.md) | Connected | 6003 | Flagged: Meant to fire when HypeRate is connected. Streamerz doesn't fire this yet. |
| [integrations-hyperate](reference/triggers/integrations-hyperate.md) | Disconnected | 6004 | Flagged: Meant to fire when HypeRate is disconnected. Streamerz doesn't fire this yet. |
| [integrations-meld-studio](reference/triggers/integrations-meld-studio.md) | Connected | 33001 | Flagged: Streamerz doesn't fire this yet. |
| [integrations-meld-studio](reference/triggers/integrations-meld-studio.md) | Disconnected | 33002 | Flagged: Streamerz doesn't fire this yet. |
| [integrations-meld-studio](reference/triggers/integrations-meld-studio.md) | Streaming Started | 33003 | Flagged: Streamerz doesn't fire this yet. |
| [integrations-meld-studio](reference/triggers/integrations-meld-studio.md) | Streaming Stopped | 33004 | Flagged: Streamerz doesn't fire this yet. |
| [integrations-meld-studio](reference/triggers/integrations-meld-studio.md) | Recording Started | 33005 | Flagged: Streamerz doesn't fire this yet. |
| [integrations-meld-studio](reference/triggers/integrations-meld-studio.md) | Recording Stopped | 33006 | Flagged: Streamerz doesn't fire this yet. |
| [integrations-meld-studio](reference/triggers/integrations-meld-studio.md) | Scene Changed | 33007 | Flagged: Streamerz doesn't fire this yet. |
| [integrations-meld-studio](reference/triggers/integrations-meld-studio.md) | Layer Visbility Changed | 33008 | Flagged: Streamerz doesn't fire this yet. |
| [integrations-meld-studio](reference/triggers/integrations-meld-studio.md) | LEffect Enabled State Changed | 33009 | Flagged: Streamerz doesn't fire this yet. |
| [integrations-meld-studio](reference/triggers/integrations-meld-studio.md) | Track Monitoring State Changed | 33010 | Flagged: Streamerz doesn't fire this yet. |
| [integrations-meld-studio](reference/triggers/integrations-meld-studio.md) | Track Musted State Changed | 33011 | Flagged: Streamerz doesn't fire this yet. |
| [integrations-meld-studio](reference/triggers/integrations-meld-studio.md) | Event | 33012 | Flagged: Streamerz doesn't fire this yet. |
| [integrations-speaker-bot](reference/triggers/integrations-speaker-bot.md) | Connected | 23001 | Flagged: Never, for now. Streamerz doesn't fire this yet. |
| [integrations-speaker-bot](reference/triggers/integrations-speaker-bot.md) | Disconnected | 23002 | Flagged: Never, for now. Streamerz doesn't fire this yet. |
| [integrations-throwing-system](reference/triggers/integrations-throwing-system.md) | System Connected | 27001 | Flagged: Never, for now. Streamerz doesn't fire this yet. |
| [integrations-throwing-system](reference/triggers/integrations-throwing-system.md) | System Websocket Connected | 27002 | Flagged: Never, for now. Streamerz doesn't fire this yet. |
| [integrations-throwing-system](reference/triggers/integrations-throwing-system.md) | System Websocket Disconnected | 27003 | Flagged: Never, for now. Streamerz doesn't fire this yet. |
| [integrations-throwing-system](reference/triggers/integrations-throwing-system.md) | System Events Connected | 27004 | Flagged: Never, for now. Streamerz doesn't fire this yet. |
| [integrations-throwing-system](reference/triggers/integrations-throwing-system.md) | System Events Disconnected | 27005 | Flagged: Never, for now. Streamerz doesn't fire this yet. |
| [integrations-throwing-system](reference/triggers/integrations-throwing-system.md) | System Item Hit | 27010 | Flagged: Never, for now. Streamerz doesn't fire this yet. |
| [integrations-throwing-system](reference/triggers/integrations-throwing-system.md) | System Trigger Activated | 27011 | Flagged: Never, for now. Streamerz doesn't fire this yet. |
| [integrations-throwing-system](reference/triggers/integrations-throwing-system.md) | System Trigger Ended | 27012 | Flagged: Never, for now. Streamerz doesn't fire this yet. |
| [integrations-voicemod](reference/triggers/integrations-voicemod.md) | Connected | 30001 | Flagged: Never, for now. Streamerz doesn't fire this yet. |
| [integrations-voicemod](reference/triggers/integrations-voicemod.md) | Disconnected | 30002 | Flagged: Never, for now. Streamerz doesn't fire this yet. |
| [integrations-voicemod](reference/triggers/integrations-voicemod.md) | Voice Loaded | 30003 | Flagged: Never, for now. Streamerz doesn't fire this yet. |
| [integrations-voicemod](reference/triggers/integrations-voicemod.md) | Soundboard Changed | 30004 | Flagged: Never, for now. Streamerz doesn't fire this yet. |
| [integrations-vtube-studio](reference/triggers/integrations-vtube-studio.md) | Model Loaded | 19001 | Flagged: Never, for now. Streamerz doesn't fire this yet. |
| [integrations-vtube-studio](reference/triggers/integrations-vtube-studio.md) | Model Unloaded | 19002 | Flagged: Never, for now. Streamerz doesn't fire this yet. |
| [integrations-vtube-studio](reference/triggers/integrations-vtube-studio.md) | Background Changed | 19003 | Flagged: Never, for now. Streamerz doesn't fire this yet. |
| [integrations-vtube-studio](reference/triggers/integrations-vtube-studio.md) | Model Config Changed | 19004 | Flagged: Never, for now. Streamerz doesn't fire this yet. |
| [integrations-vtube-studio](reference/triggers/integrations-vtube-studio.md) | Hotkey Triggered | 19005 | Flagged: Never, for now. Streamerz doesn't fire this yet. |
| [integrations-vtube-studio](reference/triggers/integrations-vtube-studio.md) | Model Animation | 19006 | Flagged: Never, for now. Streamerz doesn't fire this yet. |
| [integrations-vtube-studio](reference/triggers/integrations-vtube-studio.md) | Connected | 19007 | Flagged: Never, for now. Streamerz doesn't fire this yet. |
| [integrations-vtube-studio](reference/triggers/integrations-vtube-studio.md) | Disconnected | 19008 | Flagged: Never, for now. Streamerz doesn't fire this yet. |
| [integrations-vtube-studio](reference/triggers/integrations-vtube-studio.md) | Tracking Status Changed | 19009 | Flagged: Never, for now. Streamerz doesn't fire this yet. |
| [integrations-vtube-studio](reference/triggers/integrations-vtube-studio.md) | Item Event | 19010 | Flagged: Never, for now. Streamerz doesn't fire this yet. |
| [integrations-vtube-studio](reference/triggers/integrations-vtube-studio.md) | Model Clicked | 19011 | Flagged: Never, for now. Streamerz doesn't fire this yet. |
| [kick-channel-stream](reference/triggers/kick-channel-stream.md) | Broadcaster Authenticated | 35001 | Review |
| [kick-channel-stream](reference/triggers/kick-channel-stream.md) | Present Viewers | 35009 | Review |
| [kick-channel-stream](reference/triggers/kick-channel-stream.md) | Follow | 35011 | Review |
| [kick-channel-stream](reference/triggers/kick-channel-stream.md) | Stream Online | 35012 | Review |
| [kick-channel-stream](reference/triggers/kick-channel-stream.md) | Stream Offline | 35013 | Review |
| [kick-channel-stream](reference/triggers/kick-channel-stream.md) | Resubscription | 35015 | Review |
| [kick-channel-stream](reference/triggers/kick-channel-stream.md) | User Timed Out | 35018 | Review |
| [kick-channel-stream](reference/triggers/kick-channel-stream.md) | Channel Update | 35020 | Review |
| [kick-channel-stream](reference/triggers/kick-channel-stream.md) | Viewer Count Update | 35023 | Review |
| [kick-channel-stream](reference/triggers/kick-channel-stream.md) | Kicks Gifted | 35025 | Review |
| [kick-chat](reference/triggers/kick-chat.md) | Broadcaster Chat Connected | 35002 | Review |
| [kick-chat](reference/triggers/kick-chat.md) | Broadcaster Chat Disconnected | 35003 | Review |
| [kick-chat](reference/triggers/kick-chat.md) | First Words | 35008 | Review |
| [kick-chat](reference/triggers/kick-chat.md) | Chat Message | 35010 | Review |
| [kick-emotes](reference/triggers/kick-emotes.md) | Seven TVEmote Added | 35021 | Review |
| [kick-emotes](reference/triggers/kick-emotes.md) | Seven TVEmote Removed | 35022 | Review |
| [kick-moderation](reference/triggers/kick-moderation.md) | User Banned | 35019 | Review |
| [kick-rewards](reference/triggers/kick-rewards.md) | Reward Redemption | 35024 | Review |
| [kick-subscriptions](reference/triggers/kick-subscriptions.md) | Subscription | 35014 | Review |
| [kick-subscriptions](reference/triggers/kick-subscriptions.md) | Gift Subscription | 35016 | Review |
| [kick-subscriptions](reference/triggers/kick-subscriptions.md) | Mass Gift Subscription | 35017 | Review |
| [obs-studio-connection](reference/triggers/obs-studio-connection.md) | Connected | 14001 | Flagged: A connection that never gets in (a wrong password, for example) doesn't fire this trigger. |
| [obs-studio-connection](reference/triggers/obs-studio-connection.md) | Disconnected | 14002 | Flagged: A connection that failed before it was identified doesn't fire this trigger. |
| [obs-studio-events](reference/triggers/obs-studio-events.md) | Event | 14003 | Review |
| [obs-studio-events](reference/triggers/obs-studio-events.md) | Vendor Event | 14009 | Review |
| [obs-studio-scenes-sources](reference/triggers/obs-studio-scenes-sources.md) | Scene Changed | 14004 | Review |
| [obs-studio-streaming-recording](reference/triggers/obs-studio-streaming-recording.md) | Streaming Started | 14005 | Flagged: The "starting" state OBS sends first doesn't fire anything. Only the started and stopped states do. |
| [obs-studio-streaming-recording](reference/triggers/obs-studio-streaming-recording.md) | Streaming Stopped | 14006 | Review |
| [obs-studio-streaming-recording](reference/triggers/obs-studio-streaming-recording.md) | Recording Started | 14007 | Review |
| [obs-studio-streaming-recording](reference/triggers/obs-studio-streaming-recording.md) | Recording Stopped | 14008 | Review |
| [servers-remote-custom-webhooks](reference/triggers/servers-remote-custom-webhooks.md) | Streamer Bot Custom Webook | 34001 | Flagged: Streamerz doesn't fire this yet. |
| [servers-remote-custom-websocket-server](reference/triggers/servers-remote-custom-websocket-server.md) | Server Open | 2001 | Review |
| [servers-remote-custom-websocket-server](reference/triggers/servers-remote-custom-websocket-server.md) | Server Close | 2002 | Review |
| [servers-remote-custom-websocket-server](reference/triggers/servers-remote-custom-websocket-server.md) | Server Message | 2003 | Review |
| [servers-remote-remote-instances](reference/triggers/servers-remote-remote-instances.md) | Connected | 29001 | Review |
| [servers-remote-remote-instances](reference/triggers/servers-remote-remote-instances.md) | Disconnected | 29002 | Review |
| [servers-remote-remote-instances](reference/triggers/servers-remote-remote-instances.md) | Trigger | 29003 | Review |
| [servers-remote-remote-instances](reference/triggers/servers-remote-remote-instances.md) | Signal | 29004 | Review |
| [servers-remote-websocket-client](reference/triggers/servers-remote-websocket-client.md) | Open | 803 | Review |
| [servers-remote-websocket-client](reference/triggers/servers-remote-websocket-client.md) | Close | 804 | Review |
| [servers-remote-websocket-client](reference/triggers/servers-remote-websocket-client.md) | Message | 805 | Review |
| [streamlabs-desktop-connection](reference/triggers/streamlabs-desktop-connection.md) | Connected | 22001 | Review |
| [streamlabs-desktop-connection](reference/triggers/streamlabs-desktop-connection.md) | Disconnected | 22002 | Flagged: When a connection that was up drops, or when you disconnect it in Streamerz. A connection that never came up doesn't fire it. |
| [streamlabs-desktop-scenes-sources](reference/triggers/streamlabs-desktop-scenes-sources.md) | Scene Changed | 22003 | Review |
| [streamlabs-desktop-streaming-recording](reference/triggers/streamlabs-desktop-streaming-recording.md) | Streaming Started | 22004 | Review |
| [streamlabs-desktop-streaming-recording](reference/triggers/streamlabs-desktop-streaming-recording.md) | Streaming Stopped | 22005 | Review |
| [streamlabs-desktop-streaming-recording](reference/triggers/streamlabs-desktop-streaming-recording.md) | Recording Started | 22006 | Review |
| [streamlabs-desktop-streaming-recording](reference/triggers/streamlabs-desktop-streaming-recording.md) | Recording Stopped | 22007 | Review |
| [tiktok-chat](reference/triggers/tiktok-chat.md) | Chat Message | 41001 | Review |
| [tiktok-community](reference/triggers/tiktok-community.md) | Like | 41003 | Review |
| [tiktok-community](reference/triggers/tiktok-community.md) | Follow | 41004 | Review |
| [tiktok-community](reference/triggers/tiktok-community.md) | Share | 41005 | Review |
| [tiktok-community](reference/triggers/tiktok-community.md) | Subscribe | 41006 | Review |
| [tiktok-community](reference/triggers/tiktok-community.md) | Join | 41007 | Review |
| [tiktok-connection](reference/triggers/tiktok-connection.md) | Connected | 41008 | Review |
| [tiktok-connection](reference/triggers/tiktok-connection.md) | Disconnected | 41009 | Flagged: Fires when Streamerz was connected and the connection stops. That happens when the live ends, the connection fails, the channel isn't found, or the connection is closed by hand. It doesn't fire when Streamerz was not connected. |
| [tiktok-gifts](reference/triggers/tiktok-gifts.md) | Gift | 41002 | Review |
| [tiktok-stream](reference/triggers/tiktok-stream.md) | Viewer Count Update | 41010 | Flagged: Fires when TikTok's viewer count differs from the last count seen in this live. An unchanged count doesn't fire. |
| [tiktok-stream](reference/triggers/tiktok-stream.md) | Stream Online | 41011 | Flagged: Coming back to the same live after a drop doesn't fire this again. |
| [tiktok-stream](reference/triggers/tiktok-stream.md) | Stream Offline | 41012 | Review |
| [twitch-channel](reference/triggers/twitch-channel.md) | Follow | 101 | Review |
| [twitch-channel](reference/triggers/twitch-channel.md) | Stream Update | 118 | Review |
| [twitch-channel](reference/triggers/twitch-channel.md) | Stream Update Game On Connect | 123 | Flagged: Streamerz doesn't fire this yet. |
| [twitch-channel](reference/triggers/twitch-channel.md) | Present Viewers | 124 | Review |
| [twitch-channel](reference/triggers/twitch-channel.md) | Stream Online | 154 | Review |
| [twitch-channel](reference/triggers/twitch-channel.md) | Stream Offline | 155 | Review |
| [twitch-channel](reference/triggers/twitch-channel.md) | Viewer Count Update | 164 | Review |
| [twitch-channel](reference/triggers/twitch-channel.md) | Broadcaster Authenticated | 173 | Flagged: When the broadcaster's Twitch connection comes up. It doesn't fire on automatic reconnects. |
| [twitch-channel](reference/triggers/twitch-channel.md) | Broadcaster Chat Connected | 174 | Flagged: When the broadcaster's Twitch connection comes up, at the same time as Broadcaster Authenticated. It doesn't fire on automatic reconnects. |
| [twitch-channel](reference/triggers/twitch-channel.md) | Broadcaster Chat Disconnected | 175 | Review |
| [twitch-channel](reference/triggers/twitch-channel.md) | SevenTV Emote Removed | 181 | Review |
| [twitch-channel](reference/triggers/twitch-channel.md) | BetterTTV Emote Removed | 183 | Review |
| [twitch-channel](reference/triggers/twitch-channel.md) | Warning Acknowledged | 471 | Review |
| [twitch-chat](reference/triggers/twitch-chat.md) | Cheer | 102 | Review |
| [twitch-chat](reference/triggers/twitch-chat.md) | Reward Deleted | 115 | Review |
| [twitch-chat](reference/triggers/twitch-chat.md) | Whisper | 119 | Review |
| [twitch-chat](reference/triggers/twitch-chat.md) | First Words | 120 | Review |
| [twitch-chat](reference/triggers/twitch-chat.md) | Chat Message | 133 | Review |
| [twitch-chat](reference/triggers/twitch-chat.md) | Chat Message Deleted | 135 | Review |
| [twitch-chat](reference/triggers/twitch-chat.md) | Announcement | 138 | Review |
| [twitch-chat](reference/triggers/twitch-chat.md) | Bot Whisper | 140 | Review |
| [twitch-chat](reference/triggers/twitch-chat.md) | Coin Cheer | 143 | Flagged: Needs: nothing yet (it never fires) |
| [twitch-chat](reference/triggers/twitch-chat.md) | Chat Cleared | 157 | Review |
| [twitch-chat](reference/triggers/twitch-chat.md) | Hype Chat | 170 | Flagged: Needs: nothing yet (it never fires) |
| [twitch-chat](reference/triggers/twitch-chat.md) | Hype Chat Level | 172 | Flagged: Needs: nothing yet (it never fires) |
| [twitch-chat](reference/triggers/twitch-chat.md) | Chat Slow Mode On | 456 | Review |
| [twitch-chat](reference/triggers/twitch-chat.md) | Chat Slow Mode Off | 457 | Review |
| [twitch-chat](reference/triggers/twitch-chat.md) | Chat Slow Mode Changed | 458 | Review |
| [twitch-chat](reference/triggers/twitch-chat.md) | Blocked Terms Deleted | 466 | Review |
| [twitch-chat](reference/triggers/twitch-chat.md) | Permitted Terms Deleted | 470 | Review |
| [twitch-chat](reference/triggers/twitch-chat.md) | Bits Badge Tier | 480 | Review |
| [twitch-community](reference/triggers/twitch-community.md) | Raid | 107 | Review |
| [twitch-community](reference/triggers/twitch-community.md) | Hype Train Start | 108 | Review |
| [twitch-community](reference/triggers/twitch-community.md) | Hype Train Update | 109 | Review |
| [twitch-community](reference/triggers/twitch-community.md) | Hype Train Level Up | 110 | Review |
| [twitch-community](reference/triggers/twitch-community.md) | Hype Train End | 111 | Review |
| [twitch-community](reference/triggers/twitch-community.md) | Ad Run | 139 | Review |
| [twitch-community](reference/triggers/twitch-community.md) | Charity Donation | 141 | Review |
| [twitch-community](reference/triggers/twitch-community.md) | Charity Started | 146 | Review |
| [twitch-community](reference/triggers/twitch-community.md) | Charity Progress | 147 | Review |
| [twitch-community](reference/triggers/twitch-community.md) | Charity Completed | 142 | Review |
| [twitch-community](reference/triggers/twitch-community.md) | Goal Begin | 148 | Review |
| [twitch-community](reference/triggers/twitch-community.md) | Goal Progress | 149 | Review |
| [twitch-community](reference/triggers/twitch-community.md) | Goal End | 150 | Review |
| [twitch-community](reference/triggers/twitch-community.md) | Raid Start | 158 | Review |
| [twitch-community](reference/triggers/twitch-community.md) | Raid Send | 159 | Review |
| [twitch-community](reference/triggers/twitch-community.md) | Raid Cancelled | 160 | Review |
| [twitch-community](reference/triggers/twitch-community.md) | Pyramid Success | 162 | Review |
| [twitch-community](reference/triggers/twitch-community.md) | Pyramid Broken | 163 | Review |
| [twitch-community](reference/triggers/twitch-community.md) | Seven TVEmote Added | 180 | Review |
| [twitch-community](reference/triggers/twitch-community.md) | Better TTVEmote Added | 182 | Review |
| [twitch-community](reference/triggers/twitch-community.md) | Upcoming Ad | 186 | Review |
| [twitch-community](reference/triggers/twitch-community.md) | Blocked Terms Added | 465 | Review |
| [twitch-community](reference/triggers/twitch-community.md) | Permitted Terms Added | 469 | Review |
| [twitch-community](reference/triggers/twitch-community.md) | Watch Streak | 472 | Review |
| [twitch-guest-star](reference/triggers/twitch-guest-star.md) | Guest Star Session Begin | 165 | Review |
| [twitch-guest-star](reference/triggers/twitch-guest-star.md) | Guest Star Session End | 166 | Review |
| [twitch-guest-star](reference/triggers/twitch-guest-star.md) | Guest Star Guest Update | 167 | Review |
| [twitch-guest-star](reference/triggers/twitch-guest-star.md) | Guest Star Slot Update | 168 | Flagged: It doesn't fire yet (see Notes). |
| [twitch-guest-star](reference/triggers/twitch-guest-star.md) | Guest Star Settings Update | 169 | Review |
| [twitch-moderation](reference/triggers/twitch-moderation.md) | User Timed Out | 136 | Review |
| [twitch-moderation](reference/triggers/twitch-moderation.md) | User Banned | 137 | Review |
| [twitch-moderation](reference/triggers/twitch-moderation.md) | User Untimed Out | 145 | Review |
| [twitch-moderation](reference/triggers/twitch-moderation.md) | User Unbanned | 191 | Review |
| [twitch-moderation](reference/triggers/twitch-moderation.md) | Shoutout Created | 144 | Review |
| [twitch-moderation](reference/triggers/twitch-moderation.md) | Shoutout Received | 156 | Review |
| [twitch-moderation](reference/triggers/twitch-moderation.md) | Shield Mode Begin | 151 | Review |
| [twitch-moderation](reference/triggers/twitch-moderation.md) | Shield Mode End | 152 | Review |
| [twitch-moderation](reference/triggers/twitch-moderation.md) | Moderator Added | 187 | Review |
| [twitch-moderation](reference/triggers/twitch-moderation.md) | Moderator Removed | 188 | Review |
| [twitch-moderation](reference/triggers/twitch-moderation.md) | Vip Added | 189 | Review |
| [twitch-moderation](reference/triggers/twitch-moderation.md) | Vip Removed | 190 | Review |
| [twitch-moderation](reference/triggers/twitch-moderation.md) | Unban Request Created | 195 | Review |
| [twitch-moderation](reference/triggers/twitch-moderation.md) | Unban Request Approved | 192 | Review |
| [twitch-moderation](reference/triggers/twitch-moderation.md) | Unban Request Denied | 193 | Review |
| [twitch-moderation](reference/triggers/twitch-moderation.md) | Chat Emote Mode On | 451 | Review |
| [twitch-moderation](reference/triggers/twitch-moderation.md) | Chat Emote Mode Off | 452 | Review |
| [twitch-moderation](reference/triggers/twitch-moderation.md) | Chat Follower Mode On | 453 | Review |
| [twitch-moderation](reference/triggers/twitch-moderation.md) | Chat Follower Mode Off | 454 | Review |
| [twitch-moderation](reference/triggers/twitch-moderation.md) | Chat Follower Mode Changed | 455 | Review |
| [twitch-moderation](reference/triggers/twitch-moderation.md) | Chat Unique Mode On | 461 | Review |
| [twitch-moderation](reference/triggers/twitch-moderation.md) | Chat Unique Mode Off | 462 | Review |
| [twitch-moderation](reference/triggers/twitch-moderation.md) | Auto Mod Message Held | 463 | Review |
| [twitch-moderation](reference/triggers/twitch-moderation.md) | Auto Mod Message Update | 464 | Review |
| [twitch-moderation](reference/triggers/twitch-moderation.md) | Warned User | 467 | Review |
| [twitch-moderation](reference/triggers/twitch-moderation.md) | Suspicious User Update | 468 | Review |
| [twitch-moderation](reference/triggers/twitch-moderation.md) | Suspicious User Message | 495 | Review |
| [twitch-moderation](reference/triggers/twitch-moderation.md) | Modiversary | 497 | Flagged: Streamerz doesn't fire this yet. |
| [twitch-polls-predictions](reference/triggers/twitch-polls-predictions.md) | Poll Created | 125 | Review |
| [twitch-polls-predictions](reference/triggers/twitch-polls-predictions.md) | Poll Updated | 126 | Review |
| [twitch-polls-predictions](reference/triggers/twitch-polls-predictions.md) | Poll Completed | 127 | Review |
| [twitch-polls-predictions](reference/triggers/twitch-polls-predictions.md) | Poll Terminated | 161 | Review |
| [twitch-polls-predictions](reference/triggers/twitch-polls-predictions.md) | Poll Archived | 473 | Review |
| [twitch-polls-predictions](reference/triggers/twitch-polls-predictions.md) | Prediction Created | 128 | Review |
| [twitch-polls-predictions](reference/triggers/twitch-polls-predictions.md) | Prediction Updated | 129 | Review |
| [twitch-polls-predictions](reference/triggers/twitch-polls-predictions.md) | Prediction Locked | 132 | Review |
| [twitch-polls-predictions](reference/triggers/twitch-polls-predictions.md) | Prediction Completed | 130 | Review |
| [twitch-polls-predictions](reference/triggers/twitch-polls-predictions.md) | Prediction Canceled | 131 | Review |
| [twitch-rewards-points](reference/triggers/twitch-rewards-points.md) | Reward Redemption | 112 | Review |
| [twitch-rewards-points](reference/triggers/twitch-rewards-points.md) | Reward Created | 113 | Review |
| [twitch-rewards-points](reference/triggers/twitch-rewards-points.md) | Reward Updated | 114 | Review |
| [twitch-rewards-points](reference/triggers/twitch-rewards-points.md) | Reward Redemption Updated | 171 | Review |
| [twitch-rewards-points](reference/triggers/twitch-rewards-points.md) | Automatic Reward Redemption | 194 | Review |
| [twitch-rewards-points](reference/triggers/twitch-rewards-points.md) | Power Up Redemption | 196 | Review |
| [twitch-rewards-points](reference/triggers/twitch-rewards-points.md) | Custom Power Up Redemption | 496 | Review |
| [twitch-shared-chat](reference/triggers/twitch-shared-chat.md) | Session Begin | 474 | Review |
| [twitch-shared-chat](reference/triggers/twitch-shared-chat.md) | Session Update | 475 | Review |
| [twitch-shared-chat](reference/triggers/twitch-shared-chat.md) | Session End | 476 | Review |
| [twitch-shared-chat](reference/triggers/twitch-shared-chat.md) | Announcement | 481 | Review |
| [twitch-shared-chat](reference/triggers/twitch-shared-chat.md) | Raid | 482 | Review |
| [twitch-shared-chat](reference/triggers/twitch-shared-chat.md) | Prime Paid Upgrade | 483 | Review |
| [twitch-shared-chat](reference/triggers/twitch-shared-chat.md) | Gift Paid Upgrade | 484 | Review |
| [twitch-shared-chat](reference/triggers/twitch-shared-chat.md) | Pay It Forward | 485 | Review |
| [twitch-shared-chat](reference/triggers/twitch-shared-chat.md) | Sub | 486 | Review |
| [twitch-shared-chat](reference/triggers/twitch-shared-chat.md) | Resub | 487 | Review |
| [twitch-shared-chat](reference/triggers/twitch-shared-chat.md) | Sub Gift | 488 | Review |
| [twitch-shared-chat](reference/triggers/twitch-shared-chat.md) | Community Sub Gift | 489 | Review |
| [twitch-shared-chat](reference/triggers/twitch-shared-chat.md) | User Banned | 490 | Review |
| [twitch-shared-chat](reference/triggers/twitch-shared-chat.md) | User Unbanned | 491 | Review |
| [twitch-shared-chat](reference/triggers/twitch-shared-chat.md) | User Timedout | 492 | Review |
| [twitch-shared-chat](reference/triggers/twitch-shared-chat.md) | User Untimedout | 493 | Review |
| [twitch-shared-chat](reference/triggers/twitch-shared-chat.md) | Message Deleted | 494 | Review |
| [twitch-shared-chat](reference/triggers/twitch-shared-chat.md) | Shared Modiversary | 498 | Flagged: It doesn't fire yet (see Notes). |
| [twitch-subscriptions](reference/triggers/twitch-subscriptions.md) | Sub | 103 | Review |
| [twitch-subscriptions](reference/triggers/twitch-subscriptions.md) | Re Sub | 104 | Review |
| [twitch-subscriptions](reference/triggers/twitch-subscriptions.md) | Gift Sub | 105 | Review |
| [twitch-subscriptions](reference/triggers/twitch-subscriptions.md) | Gift Bomb | 106 | Review |
| [twitch-subscriptions](reference/triggers/twitch-subscriptions.md) | Community Goal Contribution | 116 | Flagged: Needs: nothing yet (it never fires) |
| [twitch-subscriptions](reference/triggers/twitch-subscriptions.md) | Community Goal Ended | 117 | Flagged: Needs: nothing yet (it never fires) |
| [twitch-subscriptions](reference/triggers/twitch-subscriptions.md) | Sub Counter Rollover | 121 | Review |
| [twitch-subscriptions](reference/triggers/twitch-subscriptions.md) | Broadcaster Event Sub Connected | 178 | Review |
| [twitch-subscriptions](reference/triggers/twitch-subscriptions.md) | Broadcaster Event Sub Disconnected | 179 | Review |
| [twitch-subscriptions](reference/triggers/twitch-subscriptions.md) | Bot Event Sub Connected | 184 | Review |
| [twitch-subscriptions](reference/triggers/twitch-subscriptions.md) | Bot Event Sub Disconnected | 185 | Review |
| [twitch-subscriptions](reference/triggers/twitch-subscriptions.md) | Chat Subscriber Mode On | 459 | Review |
| [twitch-subscriptions](reference/triggers/twitch-subscriptions.md) | Chat Subscriber Mode Off | 460 | Review |
| [twitch-subscriptions](reference/triggers/twitch-subscriptions.md) | Prime Paid Upgrade | 477 | Review |
| [twitch-subscriptions](reference/triggers/twitch-subscriptions.md) | Pay It Forward | 478 | Review |
| [twitch-subscriptions](reference/triggers/twitch-subscriptions.md) | Gift Paid Upgrade | 479 | Review |
| [youtube-broadcast](reference/triggers/youtube-broadcast.md) | Broadcast Started | 4001 | Review |
| [youtube-broadcast](reference/triggers/youtube-broadcast.md) | Broadcast Ended | 4002 | Review |
| [youtube-broadcast](reference/triggers/youtube-broadcast.md) | Broadcast Updated | 4013 | Flagged: Streamerz doesn't fire this yet. |
| [youtube-broadcast](reference/triggers/youtube-broadcast.md) | Broadcast Monitoring Started | 4019 | Review |
| [youtube-broadcast](reference/triggers/youtube-broadcast.md) | Broadcast Monitoring Ended | 4020 | Review |
| [youtube-broadcast](reference/triggers/youtube-broadcast.md) | Broadcast Added | 4021 | Flagged: Streamerz doesn't fire this yet. |
| [youtube-broadcast](reference/triggers/youtube-broadcast.md) | Broadcast Removed | 4022 | Flagged: Streamerz doesn't fire this yet. |
| [youtube-channel-statistics](reference/triggers/youtube-channel-statistics.md) | Statistics Updated | 4012 | Review |
| [youtube-channel-statistics](reference/triggers/youtube-channel-statistics.md) | First Words | 4016 | Review |
| [youtube-channel-statistics](reference/triggers/youtube-channel-statistics.md) | Present Viewers | 4017 | Review |
| [youtube-channel-statistics](reference/triggers/youtube-channel-statistics.md) | New Subscriber | 4018 | Review |
| [youtube-channel-statistics](reference/triggers/youtube-channel-statistics.md) | User Timedout | 4031 | Flagged: Streamerz doesn't fire this yet. |
| [youtube-chat-super-chats](reference/triggers/youtube-chat-super-chats.md) | Message | 4003 | Review |
| [youtube-chat-super-chats](reference/triggers/youtube-chat-super-chats.md) | Message Deleted | 4004 | Review |
| [youtube-chat-super-chats](reference/triggers/youtube-chat-super-chats.md) | Super Chat | 4006 | Review |
| [youtube-chat-super-chats](reference/triggers/youtube-chat-super-chats.md) | Super Sticker | 4007 | Review |
| [youtube-chat-super-chats](reference/triggers/youtube-chat-super-chats.md) | Jewels Gifted | 4030 | Review |
| [youtube-community](reference/triggers/youtube-community.md) | Seven TVEmote Added | 4023 | Review |
| [youtube-community](reference/triggers/youtube-community.md) | Seven TVEmote Removed | 4024 | Review |
| [youtube-community](reference/triggers/youtube-community.md) | Better TTVEmote Added | 4025 | Review |
| [youtube-community](reference/triggers/youtube-community.md) | Better TTVEmote Removed | 4026 | Review |
| [youtube-community](reference/triggers/youtube-community.md) | Poll Closed | 4027 | Review |
| [youtube-community](reference/triggers/youtube-community.md) | Poll Started | 4028 | Review |
| [youtube-community](reference/triggers/youtube-community.md) | Poll Updated | 4029 | Review |
| [youtube-memberships](reference/triggers/youtube-memberships.md) | New Sponsor | 4008 | Review |
| [youtube-memberships](reference/triggers/youtube-memberships.md) | Member Mile Stone | 4009 | Review |
| [youtube-memberships](reference/triggers/youtube-memberships.md) | New Sponsor Only Started | 4010 | Review |
| [youtube-memberships](reference/triggers/youtube-memberships.md) | New Sponsor Only Ended | 4011 | Review |
| [youtube-memberships](reference/triggers/youtube-memberships.md) | Membership Gift | 4014 | Review |
| [youtube-memberships](reference/triggers/youtube-memberships.md) | Gift Membership Received | 4015 | Review |
| [youtube-moderation](reference/triggers/youtube-moderation.md) | User Banned | 4005 | Review |
### Subactions

| Category | Feature | Code | Reference status |
| --- | --- | --- | --- |
| [core-action-queues](reference/subactions/core-action-queues.md) | Set Action Queue Pause State | 1001 | Review |
| [core-action-queues](reference/subactions/core-action-queues.md) | Clear Action Queue | 1005 | Review |
| [core-action-queues](reference/subactions/core-action-queues.md) | Set Action Queue Blocking State | 1022 | Review |
| [core-actions](reference/subactions/core-actions.md) | Run Action | 4 | Review |
| [core-actions](reference/subactions/core-actions.md) | Set Action State | 1004 | Review |
| [core-actions](reference/subactions/core-actions.md) | Set Action Group State | 1011 | Review |
| [core-actions](reference/subactions/core-actions.md) | Get Action State | 1016 | Review |
| [core-actions](reference/subactions/core-actions.md) | Get Action Group State | 1017 | Review |
| [core-actions](reference/subactions/core-actions.md) | Set Sub-Action State | 1028 | Review |
| [core-actions](reference/subactions/core-actions.md) | Set Sub-Action Weight | 1038 | Review |
| [core-arguments](reference/subactions/core-arguments.md) | Set Argument | 123 | Review |
| [core-c](reference/subactions/core-c.md) | Execute C# Method | 99998 | Flagged: Needs: nothing (it doesn't run here) |
| [core-c](reference/subactions/core-c.md) | Execute C# Code | 99999 | Flagged: Needs: nothing (it doesn't run here) |
| [core-commands](reference/subactions/core-commands.md) | Set Command State | 9 | Review |
| [core-commands](reference/subactions/core-commands.md) | Set Command Group State | 1010 | Review |
| [core-commands](reference/subactions/core-commands.md) | Get Commands | 1013 | Review |
| [core-commands](reference/subactions/core-commands.md) | Get Command State | 1014 | Review |
| [core-commands](reference/subactions/core-commands.md) | Get Command Group State | 1015 | Review |
| [core-file-io](reference/subactions/core-file-io.md) | Write To File | 3 | Review |
| [core-file-io](reference/subactions/core-file-io.md) | Read Random Line From File | 5 | Review |
| [core-file-io](reference/subactions/core-file-io.md) | Read Lines From File | 1006 | Review |
| [core-file-io](reference/subactions/core-file-io.md) | Read Specific Line From File | 1021 | Review |
| [core-file-io](reference/subactions/core-file-io.md) | File Exists | 1039 | Review |
| [core-file-io](reference/subactions/core-file-io.md) | Folder Exists | 1040 | Review |
| [core-file-tail](reference/subactions/core-file-tail.md) | Set File Tail State | 1030 | Flagged: Streamerz keeps the state, but nothing reads it yet. No file tail is watched yet, so File Tail Changed doesn't fire. |
| [core-globals](reference/subactions/core-globals.md) | Global (Get) | 121 | Review |
| [core-globals](reference/subactions/core-globals.md) | Global (Set) | 122 | Review |
| [core-globals](reference/subactions/core-globals.md) | Clear Non-Persisted Globals | 125 | Review |
| [core-globals](reference/subactions/core-globals.md) | Clear Non-Persisted User Globals | 126 | Review |
| [core-logic](reference/subactions/core-logic.md) | If/Else | 120 | Review |
| [core-logic](reference/subactions/core-logic.md) | Break | 124 | Review |
| [core-logic](reference/subactions/core-logic.md) | Switch | 127 | Review |
| [core-logic](reference/subactions/core-logic.md) | While | 128 | Review |
| [core-logic](reference/subactions/core-logic.md) | Loop Break | 129 | Review |
| [core-logic](reference/subactions/core-logic.md) | Loop Continue | 130 | Review |
| [core-logic](reference/subactions/core-logic.md) | Get Random Number | 1003 | Review |
| [core-logic](reference/subactions/core-logic.md) | Get Input | 1023 | Review |
| [core-midi](reference/subactions/core-midi.md) | Note On | 9001 | Review |
| [core-midi](reference/subactions/core-midi.md) | Control Change | 9002 | Review |
| [core-midi](reference/subactions/core-midi.md) | Generic Event | 9003 | Review |
| [core-misc](reference/subactions/core-misc.md) | Comment | 1009 | Review |
| [core-misc](reference/subactions/core-misc.md) | Log | 1020 | Review |
| [core-network](reference/subactions/core-network.md) | UDP Broadcast | 2 | Review |
| [core-network](reference/subactions/core-network.md) | Fetch URL | 1007 | Review |
| [core-quotes](reference/subactions/core-quotes.md) | Get Quote | 1000 | Review |
| [core-quotes](reference/subactions/core-quotes.md) | Get Quote Count | 1035 | Review |
| [core-quotes](reference/subactions/core-quotes.md) | Delete Quote | 1036 | Review |
| [core-quotes](reference/subactions/core-quotes.md) | Add Quote | 1037 | Review |
| [core-signals](reference/subactions/core-signals.md) | Send Local Signal | 1031 | Review |
| [core-signals](reference/subactions/core-signals.md) | Wait For Local Signal | 1032 | Review |
| [core-sounds](reference/subactions/core-sounds.md) | Play Sound | 1 | Flagged: Plays a sound file. Streamerz doesn't run it yet. |
| [core-sounds](reference/subactions/core-sounds.md) | Play Sound From Folder | 7 | Flagged: Plays a random sound from a folder. Streamerz doesn't run it yet. |
| [core-sounds](reference/subactions/core-sounds.md) | Stop All Sound Playback | 30001 | Flagged: Stops all sound playback. Streamerz doesn't run it yet. |
| [core-sounds](reference/subactions/core-sounds.md) | Stop Sound Playback | 30002 | Flagged: Stops one sound. Streamerz doesn't run it yet. |
| [core-system](reference/subactions/core-system.md) | Run a Program | 6 | Review |
| [core-system](reference/subactions/core-system.md) | Keyboard Press | 1012 | Review |
| [core-system](reference/subactions/core-system.md) | Toast Notification | 1024 | Review |
| [core-timers](reference/subactions/core-timers.md) | Set Timer State | 8 | Review |
| [core-triggers](reference/subactions/core-triggers.md) | Set Trigger State | 1033 | Review |
| [core-triggers](reference/subactions/core-triggers.md) | Custom Event Trigger | 1034 | Review |
| [core-users](reference/subactions/core-users.md) | Clear Users From Group | 10001 | Review |
| [core-voice-control](reference/subactions/core-voice-control.md) | Set Voice Control Command | 1018 | Flagged: No speech recognition runs in Streamerz yet, so nothing reads these commands, and the Speech to Text Command trigger doesn't fire. |
| [core-voice-control](reference/subactions/core-voice-control.md) | Set Voice Control Command State | 1019 | Review |
| [core](reference/subactions/core.md) | Delay | 1002 | Review |
| [elgato-camera-hub](reference/subactions/elgato-camera-hub.md) | Set Contrast | 14301 | Review |
| [elgato-camera-hub](reference/subactions/elgato-camera-hub.md) | Set Brightness | 14302 | Review |
| [elgato-camera-hub](reference/subactions/elgato-camera-hub.md) | Set Saturation | 14303 | Review |
| [elgato-camera-hub](reference/subactions/elgato-camera-hub.md) | Set Zoom | 14304 | Review |
| [elgato-camera-hub](reference/subactions/elgato-camera-hub.md) | Set Pan | 14305 | Review |
| [elgato-camera-hub](reference/subactions/elgato-camera-hub.md) | Set Tilt | 14306 | Review |
| [elgato-camera-hub](reference/subactions/elgato-camera-hub.md) | Set Focus | 14307 | Review |
| [elgato-camera-hub](reference/subactions/elgato-camera-hub.md) | Set Auto White Balance | 14308 | Review |
| [elgato-camera-hub](reference/subactions/elgato-camera-hub.md) | Set White Balance | 14309 | Review |
| [elgato-camera-hub](reference/subactions/elgato-camera-hub.md) | Set White Balance Tint | 14310 | Review |
| [elgato-camera-hub](reference/subactions/elgato-camera-hub.md) | Set Noise Reduction | 14311 | Review |
| [elgato-camera-hub](reference/subactions/elgato-camera-hub.md) | Set Anti Flicker | 14312 | Review |
| [elgato-camera-hub](reference/subactions/elgato-camera-hub.md) | Set Sharpness | 14313 | Review |
| [elgato-camera-hub](reference/subactions/elgato-camera-hub.md) | Set Dynamic Range | 14314 | Review |
| [elgato-camera-hub](reference/subactions/elgato-camera-hub.md) | Set AR Lens | 14401 | Review |
| [elgato-camera-hub](reference/subactions/elgato-camera-hub.md) | Set Camera | 14402 | Review |
| [elgato-camera-hub](reference/subactions/elgato-camera-hub.md) | Set Bitrate | 14403 | Review |
| [elgato-camera-hub](reference/subactions/elgato-camera-hub.md) | Set Flash Mode | 14404 | Review |
| [elgato-camera-hub](reference/subactions/elgato-camera-hub.md) | Fast Forward Chapter | 14601 | Review |
| [elgato-camera-hub](reference/subactions/elgato-camera-hub.md) | Rewind Chapter | 14602 | Review |
| [elgato-camera-hub](reference/subactions/elgato-camera-hub.md) | Set Chapter | 14603 | Review |
| [elgato-camera-hub](reference/subactions/elgato-camera-hub.md) | Set Font | 14604 | Review |
| [elgato-camera-hub](reference/subactions/elgato-camera-hub.md) | Set Font Size | 14605 | Review |
| [elgato-camera-hub](reference/subactions/elgato-camera-hub.md) | Set Opacity | 14606 | Review |
| [elgato-camera-hub](reference/subactions/elgato-camera-hub.md) | Set Line Spacing | 14607 | Review |
| [elgato-camera-hub](reference/subactions/elgato-camera-hub.md) | Set Horizontal Margin | 14608 | Review |
| [elgato-camera-hub](reference/subactions/elgato-camera-hub.md) | Set Vertical Margin | 14609 | Review |
| [elgato-camera-hub](reference/subactions/elgato-camera-hub.md) | Set Brightness | 14610 | Review |
| [elgato-camera-hub](reference/subactions/elgato-camera-hub.md) | Set Auto Scroll State | 14611 | Review |
| [elgato-camera-hub](reference/subactions/elgato-camera-hub.md) | Set Mode | 14612 | Review |
| [elgato-camera-hub](reference/subactions/elgato-camera-hub.md) | Set Crosshair State | 14614 | Review |
| [elgato-camera-hub](reference/subactions/elgato-camera-hub.md) | Set Crosshair Image | 14615 | Review |
| [elgato-camera-hub](reference/subactions/elgato-camera-hub.md) | Get Selected Script | 14616 | Review |
| [elgato-camera-hub](reference/subactions/elgato-camera-hub.md) | Set Script | 14617 | Review |
| [elgato-stream-deck](reference/subactions/elgato-stream-deck.md) | Set Title | 11001 | Review |
| [elgato-stream-deck](reference/subactions/elgato-stream-deck.md) | Set Background | 11002 | Review |
| [elgato-stream-deck](reference/subactions/elgato-stream-deck.md) | Set State | 11004 | Review |
| [elgato-stream-deck](reference/subactions/elgato-stream-deck.md) | Show Alert | 11005 | Review |
| [elgato-stream-deck](reference/subactions/elgato-stream-deck.md) | Show Ok | 11006 | Review |
| [elgato-stream-deck](reference/subactions/elgato-stream-deck.md) | Toggle State | 11007 | Review |
| [elgato-stream-deck](reference/subactions/elgato-stream-deck.md) | Set Value | 11008 | Review |
| [elgato-stream-deck](reference/subactions/elgato-stream-deck.md) | Execute MCP Action | 14701 | Review |
| [elgato-wave-link](reference/subactions/elgato-wave-link.md) | Mute Microphone | 14001 | Review |
| [elgato-wave-link](reference/subactions/elgato-wave-link.md) | Mute Output | 14002 | Review |
| [elgato-wave-link](reference/subactions/elgato-wave-link.md) | Mute Input | 14003 | Review |
| [elgato-wave-link](reference/subactions/elgato-wave-link.md) | Set Filter Bypass State | 14004 | Review |
| [elgato-wave-link](reference/subactions/elgato-wave-link.md) | Set Filter State | 14005 | Review |
| [elgato-wave-link](reference/subactions/elgato-wave-link.md) | Set Output Monitor Device | 14006 | Review |
| [elgato-wave-link](reference/subactions/elgato-wave-link.md) | Get Selected Output | 14007 | Review |
| [elgato-wave-link](reference/subactions/elgato-wave-link.md) | Set Output Volume | 14008 | Review |
| [elgato-wave-link](reference/subactions/elgato-wave-link.md) | Get Output Volumes | 14009 | Review |
| [elgato-wave-link](reference/subactions/elgato-wave-link.md) | Set Input Volume | 14010 | Review |
| [elgato-wave-link](reference/subactions/elgato-wave-link.md) | Get Input Information | 14011 | Review |
| [elgato-wave-link](reference/subactions/elgato-wave-link.md) | Get Microphone Information | 14012 | Review |
| [elgato-wave-link](reference/subactions/elgato-wave-link.md) | Set Microphone Gain | 14013 | Review |
| [elgato-wave-link](reference/subactions/elgato-wave-link.md) | Set Microphone Output Volume | 14014 | Review |
| [elgato-wave-link](reference/subactions/elgato-wave-link.md) | Set Microphone Balance | 14015 | Review |
| [elgato-wave-link](reference/subactions/elgato-wave-link.md) | Get Filter State | 14016 | Review |
| [integrations-discord](reference/subactions/integrations-discord.md) | Basic Webhook | 7001 | Review |
| [integrations-ifttt](reference/subactions/integrations-ifttt.md) | IFTTT Notification (Basic) | 8001 | Review |
| [integrations-ifttt](reference/subactions/integrations-ifttt.md) | IFTTT Notification (Advanced) | 8002 | Review |
| [integrations-lumia-stream](reference/subactions/integrations-lumia-stream.md) | Send Command | 6001 | Review |
| [integrations-lumia-stream](reference/subactions/integrations-lumia-stream.md) | Set Color | 6002 | Review |
| [integrations-lumia-stream](reference/subactions/integrations-lumia-stream.md) | Set To Default | 6004 | Review |
| [integrations-mtion](reference/subactions/integrations-mtion.md) | Get Active Clubhouse | 40000 | Review |
| [integrations-mtion](reference/subactions/integrations-mtion.md) | Send Trigger | 40001 | Review |
| [integrations-pronouns](reference/subactions/integrations-pronouns.md) | Add Pronouns for User | 15001 | Review |
| [integrations-streamfog](reference/subactions/integrations-streamfog.md) | Activate Lens | 37001 | Review |
| [integrations-streamfog](reference/subactions/integrations-streamfog.md) | Deactivate Lens | 37002 | Review |
| [integrations-streamfog](reference/subactions/integrations-streamfog.md) | Activate Lens by Name | 37004 | Review |
| [integrations-streamfog](reference/subactions/integrations-streamfog.md) | Activate Outfit | 37011 | Review |
| [integrations-streamfog](reference/subactions/integrations-streamfog.md) | Reset Look | 37012 | Review |
| [integrations-streamfog](reference/subactions/integrations-streamfog.md) | Activate Outfit by Name | 37013 | Review |
| [integrations-streamfog](reference/subactions/integrations-streamfog.md) | Play Video Background | 37014 | Review |
| [integrations-streamfog](reference/subactions/integrations-streamfog.md) | Play Video Background by Name | 37015 | Review |
| [integrations-streamlabs](reference/subactions/integrations-streamlabs.md) | Spin Wheel | 44001 | Flagged: Spins a Streamlabs wheel. Streamerz doesn't run it yet. |
| [integrations-throwing-system](reference/subactions/integrations-throwing-system.md) | Activate Trigger | 19001 | Review |
| [integrations-throwing-system](reference/subactions/integrations-throwing-system.md) | Activate Trigger by Name | 19002 | Review |
| [integrations-throwing-system](reference/subactions/integrations-throwing-system.md) | Throw Item | 19003 | Review |
| [integrations-throwing-system](reference/subactions/integrations-throwing-system.md) | Throw Item by Name | 19004 | Review |
| [integrations-throwing-system](reference/subactions/integrations-throwing-system.md) | Throw Items | 19005 | Review |
| [integrations-throwing-system](reference/subactions/integrations-throwing-system.md) | Throw Items by Name | 19006 | Review |
| [integrations-voicemod](reference/subactions/integrations-voicemod.md) | Get Current Voice | 5001 | Review |
| [integrations-voicemod](reference/subactions/integrations-voicemod.md) | Select Voice | 5002 | Review |
| [integrations-voicemod](reference/subactions/integrations-voicemod.md) | Select Random Voice | 5003 | Review |
| [integrations-voicemod](reference/subactions/integrations-voicemod.md) | Set Voice Changer State | 5004 | Review |
| [integrations-voicemod](reference/subactions/integrations-voicemod.md) | Set Mute State | 5005 | Review |
| [integrations-voicemod](reference/subactions/integrations-voicemod.md) | Set Hear My Voice State | 5006 | Review |
| [integrations-voicemod](reference/subactions/integrations-voicemod.md) | Set Censor State | 5007 | Review |
| [integrations-voicemod](reference/subactions/integrations-voicemod.md) | Set Background Effect State | 5008 | Review |
| [integrations-voicemod](reference/subactions/integrations-voicemod.md) | Select Voice By Id | 5009 | Review |
| [integrations-voicemod](reference/subactions/integrations-voicemod.md) | Play Sound | 5010 | Review |
| [integrations-voicemod](reference/subactions/integrations-voicemod.md) | Stop All Sounds | 5011 | Review |
| [integrations-voicemod](reference/subactions/integrations-voicemod.md) | Select Voice by Name | 5012 | Review |
| [integrations-vtube-studio](reference/subactions/integrations-vtube-studio.md) | Load Model | 12001 | Review |
| [integrations-vtube-studio](reference/subactions/integrations-vtube-studio.md) | Load Model by Name | 12002 | Review |
| [integrations-vtube-studio](reference/subactions/integrations-vtube-studio.md) | Trigger Hotkey | 12003 | Review |
| [integrations-vtube-studio](reference/subactions/integrations-vtube-studio.md) | Trigger Hotkey by Name | 12004 | Review |
| [integrations-vtube-studio](reference/subactions/integrations-vtube-studio.md) | Move Model | 12005 | Review |
| [integrations-vtube-studio](reference/subactions/integrations-vtube-studio.md) | Get Current Model | 12006 | Review |
| [integrations-vtube-studio](reference/subactions/integrations-vtube-studio.md) | Color Tint | 12007 | Review |
| [integrations-vtube-studio](reference/subactions/integrations-vtube-studio.md) | Reset All Color Tints | 12008 | Review |
| [integrations-vtube-studio](reference/subactions/integrations-vtube-studio.md) | Set Expression State | 12009 | Review |
| [integrations-vtube-studio](reference/subactions/integrations-vtube-studio.md) | Send Raw Request | 12010 | Review |
| [kick-channel](reference/subactions/kick-channel.md) | Set Channel Title | 35030 | Review |
| [kick-channel](reference/subactions/kick-channel.md) | Set Channel Category | 35031 | Review |
| [kick-chat](reference/subactions/kick-chat.md) | Send Message to Channel | 35001 | Review |
| [kick-chat](reference/subactions/kick-chat.md) | Reply To Message | 35002 | Review |
| [kick-moderation](reference/subactions/kick-moderation.md) | Ban User | 35040 | Review |
| [kick-moderation](reference/subactions/kick-moderation.md) | Unban User | 35041 | Review |
| [kick-moderation](reference/subactions/kick-moderation.md) | Timeout User | 35042 | Review |
| [kick-moderation](reference/subactions/kick-moderation.md) | UnTimeout User | 35043 | Review |
| [kick-rewards](reference/subactions/kick-rewards.md) | Set Enabled State | 35101 | Review |
| [kick-rewards](reference/subactions/kick-rewards.md) | Set Paused State | 35102 | Review |
| [kick-rewards](reference/subactions/kick-rewards.md) | Set Cost | 35103 | Review |
| [kick-rewards](reference/subactions/kick-rewards.md) | Set Title | 35105 | Review |
| [kick-rewards](reference/subactions/kick-rewards.md) | Set Description | 35106 | Review |
| [kick-rewards](reference/subactions/kick-rewards.md) | Configure Rewards | 35104 | Flagged: Disables the rewards that are not configured. Streamerz doesn't run it yet. |
| [kick-rewards](reference/subactions/kick-rewards.md) | Update | 35107 | Review |
| [kick-rewards](reference/subactions/kick-rewards.md) | Get Reward Info | 35108 | Review |
| [kick-rewards](reference/subactions/kick-rewards.md) | Reset Reward Counter | 35109 | Flagged: Resets the redemption counter of a reward. Streamerz doesn't run it yet. |
| [kick-rewards](reference/subactions/kick-rewards.md) | Reset Reward User Counters | 35110 | Flagged: Resets the per-user redemption counters of a reward. Streamerz doesn't run it yet. |
| [kick-rewards](reference/subactions/kick-rewards.md) | Set Group Enabled State | 35111 | Flagged: Turns on or off every reward in a group. Streamerz doesn't run it yet. |
| [kick-rewards](reference/subactions/kick-rewards.md) | Set Group Paused State | 35112 | Flagged: Pauses or unpauses every reward in a group. Streamerz doesn't run it yet. |
| [kick-rewards](reference/subactions/kick-rewards.md) | Set Background Color | 35113 | Review |
| [kick-user-groups](reference/subactions/kick-user-groups.md) | Add User To Group | 35020 | Review |
| [kick-user-groups](reference/subactions/kick-user-groups.md) | Remove User from Group | 35021 | Review |
| [kick-user-groups](reference/subactions/kick-user-groups.md) | User In Group | 35022 | Review |
| [kick-user](reference/subactions/kick-user.md) | Add Broadcaster Information | 35010 | Review |
| [kick-user](reference/subactions/kick-user.md) | Get User Info for Target | 35011 | Review |
| [kick-user](reference/subactions/kick-user.md) | Add Random Users | 35012 | Review |
| [meld-studio-effects](reference/subactions/meld-studio-effects.md) | Set Effect Enabled State | 41007 | Review |
| [meld-studio-layers](reference/subactions/meld-studio-layers.md) | Set Layer Visibility State | 41006 | Review |
| [meld-studio-layers](reference/subactions/meld-studio-layers.md) | Set Text | 41012 | Review |
| [meld-studio-layers](reference/subactions/meld-studio-layers.md) | Set Media Layer Source | 41014 | Review |
| [meld-studio-layers](reference/subactions/meld-studio-layers.md) | Set Browser Layer Url | 41016 | Review |
| [meld-studio-tracks](reference/subactions/meld-studio-tracks.md) | Set Layer Track Muted State | 41008 | Review |
| [meld-studio-tracks](reference/subactions/meld-studio-tracks.md) | Set Track Muted State | 41009 | Review |
| [meld-studio](reference/subactions/meld-studio.md) | Get Current Scene | 41001 | Review |
| [meld-studio](reference/subactions/meld-studio.md) | Get Status | 41002 | Review |
| [meld-studio](reference/subactions/meld-studio.md) | Streaming | 41003 | Review |
| [meld-studio](reference/subactions/meld-studio.md) | Recording | 41004 | Review |
| [meld-studio](reference/subactions/meld-studio.md) | Show Scene | 41005 | Review |
| [meld-studio](reference/subactions/meld-studio.md) | Take Screenshot | 41010 | Review |
| [meld-studio](reference/subactions/meld-studio.md) | Record Clip | 41013 | Review |
| [meld-studio](reference/subactions/meld-studio.md) | Take Vertical Screenshot | 41017 | Review |
| [obs-studio-filters](reference/subactions/obs-studio-filters.md) | Set State of a Random Filter | 40 | Review |
| [obs-studio-groups](reference/subactions/obs-studio-groups.md) | Set Random Group Source Visible | 31 | Review |
| [obs-studio-replay](reference/subactions/obs-studio-replay.md) | Set Replay Buffer State | 321 | Review |
| [obs-studio-scenes](reference/subactions/obs-studio-scenes.md) | Set Scene Filter State | 47 | Review |
| [obs-studio-scenes](reference/subactions/obs-studio-scenes.md) | Get Scene Item Properties | 324 | Review |
| [obs-studio-scenes](reference/subactions/obs-studio-scenes.md) | Hide Scene's Sources | 325 | Review |
| [obs-studio-scenes](reference/subactions/obs-studio-scenes.md) | Set Random Scene Source Visible | 326 | Review |
| [obs-studio-sources](reference/subactions/obs-studio-sources.md) | Set Source Visibility State | 30 | Review |
| [obs-studio-sources](reference/subactions/obs-studio-sources.md) | Hide Group's Sources | 32 | Review |
| [obs-studio-sources](reference/subactions/obs-studio-sources.md) | Set Source Filter State | 35 | Review |
| [obs-studio-sources](reference/subactions/obs-studio-sources.md) | Flip Source | 36 | Review |
| [obs-studio-sources](reference/subactions/obs-studio-sources.md) | Rotate Source | 37 | Review |
| [obs-studio-sources](reference/subactions/obs-studio-sources.md) | Set Browser Source URL | 38 | Review |
| [obs-studio-sources](reference/subactions/obs-studio-sources.md) | Set GDI Text | 39 | Review |
| [obs-studio-sources](reference/subactions/obs-studio-sources.md) | Hide Source's Filters | 41 | Review |
| [obs-studio-sources](reference/subactions/obs-studio-sources.md) | Set Source Mute State | 42 | Review |
| [obs-studio-sources](reference/subactions/obs-studio-sources.md) | Set Source Audio Track State | 44 | Review |
| [obs-studio-sources](reference/subactions/obs-studio-sources.md) | Set Media State | 49 | Review |
| [obs-studio-sources](reference/subactions/obs-studio-sources.md) | Set Media Source File | 322 | Review |
| [obs-studio-sources](reference/subactions/obs-studio-sources.md) | Set Image Source File | 323 | Review |
| [obs-studio-sources](reference/subactions/obs-studio-sources.md) | Set Color Source Color | 327 | Review |
| [obs-studio](reference/subactions/obs-studio.md) | Streaming | 33 | Review |
| [obs-studio](reference/subactions/obs-studio.md) | Set Active Scene | 34 | Review |
| [obs-studio](reference/subactions/obs-studio.md) | Get Current Scene | 43 | Review |
| [obs-studio](reference/subactions/obs-studio.md) | Recording | 45 | Review |
| [obs-studio](reference/subactions/obs-studio.md) | Raw | 46 | Review |
| [obs-studio](reference/subactions/obs-studio.md) | Take Screenshot | 48 | Review |
| [obs-studio](reference/subactions/obs-studio.md) | Get Status | 328 | Review |
| [obs-studio](reference/subactions/obs-studio.md) | Create Record Chapter | 329 | Review |
| [other](reference/subactions/other.md) | Nothing | 0 | Flagged: Does nothing. Streamerz doesn't run it yet. |
| [other](reference/subactions/other.md) | SlobsRaw | 416 | Flagged: Streamlabs Desktop raw request. Streamerz doesn't run it yet. |
| [other](reference/subactions/other.md) | Get Sub-Action State | 1029 | Flagged: Gets the state of a sub-action. Streamerz doesn't run it yet. |
| [other](reference/subactions/other.md) | Lumia Stream Set Brightness | 6003 | Flagged: Lumia Stream brightness. Streamerz doesn't run it yet. |
| [other](reference/subactions/other.md) | Switch Profile | 11003 | Flagged: Stream Deck profile switch. Streamerz doesn't run it yet. |
| [other](reference/subactions/other.md) | Elgato Camera Hub Prompter Set Contrast | 14613 | Flagged: Elgato Camera Hub prompter contrast. Streamerz doesn't run it yet. |
| [other](reference/subactions/other.md) | Meld Studio Set Layer Rotation | 41015 | Flagged: Meld Studio layer rotation. Streamerz doesn't run it yet. |
| [other](reference/subactions/other.md) | Group | 99900 | Review |
| [other](reference/subactions/other.md) | If True Group | 99901 | Review |
| [other](reference/subactions/other.md) | If False Group | 99902 | Review |
| [other](reference/subactions/other.md) | Switch Case | 99903 | Review |
| [other](reference/subactions/other.md) | Switch Default | 99904 | Review |
| [polypop](reference/subactions/polypop.md) | Trigger Alert | 701 | Review |
| [settings](reference/subactions/settings.md) | Pick Color | 1008 | Review |
| [settings](reference/subactions/settings.md) | Set Voice Control Input | 1025 | Flagged: The choice is kept while Streamerz runs. Nothing reads it yet, because Streamerz doesn't run speech recognition yet. |
| [settings](reference/subactions/settings.md) | Reset First Words | 1026 | Review |
| [settings](reference/subactions/settings.md) | Reset Credits | 1027 | Review |
| [speaker-bot](reference/subactions/speaker-bot.md) | Speak (UDP) | 601 | Review |
| [speaker-bot](reference/subactions/speaker-bot.md) | Speak | 602 | Review |
| [speaker-bot](reference/subactions/speaker-bot.md) | Set Enabled State | 603 | Review |
| [speaker-bot](reference/subactions/speaker-bot.md) | Set Paused State | 604 | Review |
| [speaker-bot](reference/subactions/speaker-bot.md) | Stop Speaking | 605 | Review |
| [speaker-bot](reference/subactions/speaker-bot.md) | Clear Pending | 606 | Review |
| [speaker-bot](reference/subactions/speaker-bot.md) | Activate VoiceGate Profile | 607 | Review |
| [streamer-bot-decks](reference/subactions/streamer-bot-decks.md) | Item Set State | 21001 | Review |
| [streamer-bot-decks](reference/subactions/streamer-bot-decks.md) | Item Toggle State | 21002 | Review |
| [streamer-bot-decks](reference/subactions/streamer-bot-decks.md) | Item Set Title | 21003 | Review |
| [streamer-bot-decks](reference/subactions/streamer-bot-decks.md) | Item Set Background | 21004 | Review |
| [streamer-bot-decks](reference/subactions/streamer-bot-decks.md) | Item Set Icon | 21005 | Review |
| [streamer-bot-decks](reference/subactions/streamer-bot-decks.md) | Item Set Value | 21006 | Review |
| [streamer-bot-decks](reference/subactions/streamer-bot-decks.md) | Deck Notify | 21007 | Review |
| [streamer-bot-decks](reference/subactions/streamer-bot-decks.md) | Page Set | 21008 | Review |
| [streamer-bot-decks](reference/subactions/streamer-bot-decks.md) | Page Next | 21009 | Review |
| [streamer-bot-decks](reference/subactions/streamer-bot-decks.md) | Page Previous | 21010 | Review |
| [streamer-bot-remote](reference/subactions/streamer-bot-remote.md) | Send Remote Trigger | 20001 | Review |
| [streamer-bot-remote](reference/subactions/streamer-bot-remote.md) | Send Remote Signal | 20002 | Review |
| [streamer-bot-remote](reference/subactions/streamer-bot-remote.md) | Wait For Remote Signal | 20003 | Review |
| [streamlabs-desktop-filters](reference/subactions/streamlabs-desktop-filters.md) | Set State of a Random Filter | 411 | Review |
| [streamlabs-desktop-groups](reference/subactions/streamlabs-desktop-groups.md) | Set Random Group Source Visible | 402 | Review |
| [streamlabs-desktop-groups](reference/subactions/streamlabs-desktop-groups.md) | Hide Group's Sources | 403 | Review |
| [streamlabs-desktop-scenes](reference/subactions/streamlabs-desktop-scenes.md) | Set Active Scene | 405 | Review |
| [streamlabs-desktop-scenes](reference/subactions/streamlabs-desktop-scenes.md) | Get Current Scene | 414 | Review |
| [streamlabs-desktop-scenes](reference/subactions/streamlabs-desktop-scenes.md) | Set Scene Filter State | 417 | Review |
| [streamlabs-desktop-sources](reference/subactions/streamlabs-desktop-sources.md) | Set Source Visibility State | 401 | Review |
| [streamlabs-desktop-sources](reference/subactions/streamlabs-desktop-sources.md) | Set Source Filter State | 406 | Review |
| [streamlabs-desktop-sources](reference/subactions/streamlabs-desktop-sources.md) | Flip Source | 407 | Review |
| [streamlabs-desktop-sources](reference/subactions/streamlabs-desktop-sources.md) | Rotate Source | 408 | Review |
| [streamlabs-desktop-sources](reference/subactions/streamlabs-desktop-sources.md) | Set Browser Source URL | 409 | Review |
| [streamlabs-desktop-sources](reference/subactions/streamlabs-desktop-sources.md) | Set GDI Text | 410 | Review |
| [streamlabs-desktop-sources](reference/subactions/streamlabs-desktop-sources.md) | Hide Source's Filters | 412 | Review |
| [streamlabs-desktop-sources](reference/subactions/streamlabs-desktop-sources.md) | Set Source Mute State | 413 | Review |
| [streamlabs-desktop](reference/subactions/streamlabs-desktop.md) | Streaming | 404 | Review |
| [streamlabs-desktop](reference/subactions/streamlabs-desktop.md) | Recording | 415 | Review |
| [tiktok-chat](reference/subactions/tiktok-chat.md) | Send Message to Live Chat | 43001 | Review |
| [tiktok-gifts](reference/subactions/tiktok-gifts.md) | Get Top Gifters | 43002 | Review |
| [tiktok](reference/subactions/tiktok.md) | Get Stream Info | 43003 | Review |
| [twitch-ads](reference/subactions/twitch-ads.md) | Run Commercial | 19 | Review |
| [twitch-ads](reference/subactions/twitch-ads.md) | Snooze Ad | 555 | Review |
| [twitch-ads](reference/subactions/twitch-ads.md) | Get Ad Schedule | 556 | Review |
| [twitch-ads](reference/subactions/twitch-ads.md) | Get Pre-roll Free Time | 557 | Review |
| [twitch-channel](reference/subactions/twitch-channel.md) | Set Channel Title | 15 | Review |
| [twitch-channel](reference/subactions/twitch-channel.md) | Set Channel Game | 16 | Review |
| [twitch-channel](reference/subactions/twitch-channel.md) | Create Stream Marker | 17 | Review |
| [twitch-channel](reference/subactions/twitch-channel.md) | Create Clip | 18 | Flagged: Streamerz sends only the delay setting to Twitch (hasDelay, off when missing). It doesn't send title or duration yet, so the clip's title and length are not set by these fields. |
| [twitch-channel](reference/subactions/twitch-channel.md) | Add Viewer Count | 24 | Review |
| [twitch-channel](reference/subactions/twitch-channel.md) | Set Channel Branded Content | 564 | Review |
| [twitch-charity](reference/subactions/twitch-charity.md) | Get Latest Charity Campaign | 25 | Review |
| [twitch-chat](reference/subactions/twitch-chat.md) | Send Message to Channel | 10 | Review |
| [twitch-chat](reference/subactions/twitch-chat.md) | Send Announcement to Channel | 23 | Review |
| [twitch-chat](reference/subactions/twitch-chat.md) | Clear Chat Messages | 510 | Review |
| [twitch-chat](reference/subactions/twitch-chat.md) | Reply To Message | 560 | Review |
| [twitch-chat](reference/subactions/twitch-chat.md) | Get Pinned Chat Message | 575 | Review |
| [twitch-chat](reference/subactions/twitch-chat.md) | Pin Chat Message | 576 | Review |
| [twitch-chat](reference/subactions/twitch-chat.md) | Updated Pinned Chat Message Duration | 577 | Review |
| [twitch-chat](reference/subactions/twitch-chat.md) | Unpin Chat Message | 578 | Review |
| [twitch-clips](reference/subactions/twitch-clips.md) | Get Clip Download Urls | 573 | Review |
| [twitch-followers](reference/subactions/twitch-followers.md) | Get Follow Age Info for Target | 51 | Review |
| [twitch-followers](reference/subactions/twitch-followers.md) | Get Follower Count | 506 | Review |
| [twitch-followers](reference/subactions/twitch-followers.md) | Get Latest Follower | 507 | Review |
| [twitch-guest-star](reference/subactions/twitch-guest-star.md) | Get Channel Guest Star Settings | 520 | Review |
| [twitch-guest-star](reference/subactions/twitch-guest-star.md) | Update Channel Guest Star Settings | 521 | Review |
| [twitch-guest-star](reference/subactions/twitch-guest-star.md) | Get Guest Star Session | 522 | Review |
| [twitch-guest-star](reference/subactions/twitch-guest-star.md) | Create Guest Star Session | 523 | Review |
| [twitch-guest-star](reference/subactions/twitch-guest-star.md) | End Guest Star Session | 524 | Review |
| [twitch-guest-star](reference/subactions/twitch-guest-star.md) | Get Guest Star Invites | 525 | Review |
| [twitch-guest-star](reference/subactions/twitch-guest-star.md) | Send Guest Star Invite | 526 | Review |
| [twitch-guest-star](reference/subactions/twitch-guest-star.md) | Delete Guest Star Invite | 527 | Review |
| [twitch-guest-star](reference/subactions/twitch-guest-star.md) | Assign Guest Star Slot | 528 | Review |
| [twitch-guest-star](reference/subactions/twitch-guest-star.md) | Update Guest Star Slot | 529 | Review |
| [twitch-guest-star](reference/subactions/twitch-guest-star.md) | Delete Guest Star Slot | 530 | Review |
| [twitch-guest-star](reference/subactions/twitch-guest-star.md) | Update Guest Star Slot Settings | 531 | Review |
| [twitch-moderation](reference/subactions/twitch-moderation.md) | Subscriber Only | 12 | Review |
| [twitch-moderation](reference/subactions/twitch-moderation.md) | Emote Only | 13 | Review |
| [twitch-moderation](reference/subactions/twitch-moderation.md) | Slow Mode | 14 | Review |
| [twitch-moderation](reference/subactions/twitch-moderation.md) | Timeout User | 20 | Review |
| [twitch-moderation](reference/subactions/twitch-moderation.md) | Get Shield Mode Status | 26 | Review |
| [twitch-moderation](reference/subactions/twitch-moderation.md) | Update Shield Mode Status | 27 | Review |
| [twitch-moderation](reference/subactions/twitch-moderation.md) | Follow Mode | 28 | Review |
| [twitch-moderation](reference/subactions/twitch-moderation.md) | Send Shoutout | 29 | Review |
| [twitch-moderation](reference/subactions/twitch-moderation.md) | Add VIP | 502 | Review |
| [twitch-moderation](reference/subactions/twitch-moderation.md) | Remove VIP | 503 | Review |
| [twitch-moderation](reference/subactions/twitch-moderation.md) | Add Moderator | 504 | Review |
| [twitch-moderation](reference/subactions/twitch-moderation.md) | Remove Moderator | 505 | Review |
| [twitch-moderation](reference/subactions/twitch-moderation.md) | Ban User | 511 | Review |
| [twitch-moderation](reference/subactions/twitch-moderation.md) | Unban User | 512 | Review |
| [twitch-moderation](reference/subactions/twitch-moderation.md) | UnTimeout User | 513 | Review |
| [twitch-moderation](reference/subactions/twitch-moderation.md) | Warn User | 565 | Review |
| [twitch-moderation](reference/subactions/twitch-moderation.md) | Add Suspicious Status | 579 | Review |
| [twitch-moderation](reference/subactions/twitch-moderation.md) | Remove Suspicious Status | 580 | Review |
| [twitch-polls](reference/subactions/twitch-polls.md) | Create Poll | 561 | Review |
| [twitch-polls](reference/subactions/twitch-polls.md) | Terminate Active Poll | 562 | Review |
| [twitch-polls](reference/subactions/twitch-polls.md) | Archive Active Poll | 563 | Review |
| [twitch-polls](reference/subactions/twitch-polls.md) | Get Active Poll | 566 | Review |
| [twitch-predictions](reference/subactions/twitch-predictions.md) | Get Active Prediction | 567 | Review |
| [twitch-predictions](reference/subactions/twitch-predictions.md) | Create Prediction | 568 | Review |
| [twitch-predictions](reference/subactions/twitch-predictions.md) | Cancel Active Prediction | 569 | Review |
| [twitch-predictions](reference/subactions/twitch-predictions.md) | Lock Active Prediction | 570 | Review |
| [twitch-predictions](reference/subactions/twitch-predictions.md) | Resolve Last Prediction | 571 | Review |
| [twitch-predictions](reference/subactions/twitch-predictions.md) | Resolve Last Prediction By Title | 572 | Review |
| [twitch-raid](reference/subactions/twitch-raid.md) | Start Raid | 500 | Review |
| [twitch-raid](reference/subactions/twitch-raid.md) | Cancel Raid | 501 | Review |
| [twitch-rewards](reference/subactions/twitch-rewards.md) | Update Redemption Status | 11 | Review |
| [twitch-rewards](reference/subactions/twitch-rewards.md) | Set Enabled State | 200 | Review |
| [twitch-rewards](reference/subactions/twitch-rewards.md) | Set Paused State | 201 | Review |
| [twitch-rewards](reference/subactions/twitch-rewards.md) | Set Cost | 202 | Review |
| [twitch-rewards](reference/subactions/twitch-rewards.md) | Configure Rewards | 203 | Review |
| [twitch-rewards](reference/subactions/twitch-rewards.md) | Set Global Cooldown | 204 | Review |
| [twitch-rewards](reference/subactions/twitch-rewards.md) | Set Title | 205 | Review |
| [twitch-rewards](reference/subactions/twitch-rewards.md) | Set Prompt | 206 | Review |
| [twitch-rewards](reference/subactions/twitch-rewards.md) | Update | 207 | Review |
| [twitch-rewards](reference/subactions/twitch-rewards.md) | Get Reward Info | 208 | Review |
| [twitch-rewards](reference/subactions/twitch-rewards.md) | Reset Reward Counter | 209 | Review |
| [twitch-rewards](reference/subactions/twitch-rewards.md) | Reset Reward User Counters | 210 | Review |
| [twitch-rewards](reference/subactions/twitch-rewards.md) | Set Group Enabled State | 211 | Review |
| [twitch-rewards](reference/subactions/twitch-rewards.md) | Set Group Paused State | 212 | Review |
| [twitch-rewards](reference/subactions/twitch-rewards.md) | Set Background Color | 213 | Review |
| [twitch-rewards](reference/subactions/twitch-rewards.md) | Set Max per User per Stream | 214 | Review |
| [twitch-rewards](reference/subactions/twitch-rewards.md) | Set Max Per Stream | 215 | Review |
| [twitch-sub-counter](reference/subactions/twitch-sub-counter.md) | Get Twitch Sub Counter Count | 551 | Review |
| [twitch-sub-counter](reference/subactions/twitch-sub-counter.md) | Set Twitch Sub Counter Count | 552 | Review |
| [twitch-subscribers](reference/subactions/twitch-subscribers.md) | Get Subscriber Count | 508 | Review |
| [twitch-subscribers](reference/subactions/twitch-subscribers.md) | Get Latest Subscriber | 509 | Review |
| [twitch-tags](reference/subactions/twitch-tags.md) | Clear Channel Tags | 251 | Review |
| [twitch-tags](reference/subactions/twitch-tags.md) | Set Channel Tags | 252 | Review |
| [twitch-tags](reference/subactions/twitch-tags.md) | Add Channel Tag | 253 | Review |
| [twitch-tags](reference/subactions/twitch-tags.md) | Remove Channel Tag | 254 | Review |
| [twitch-user-groups](reference/subactions/twitch-user-groups.md) | Add User To Group | 553 | Review |
| [twitch-user-groups](reference/subactions/twitch-user-groups.md) | Remove User from Group | 554 | Review |
| [twitch-user-groups](reference/subactions/twitch-user-groups.md) | User In Group | 558 | Review |
| [twitch-user](reference/subactions/twitch-user.md) | Add Broadcaster Information | 21 | Review |
| [twitch-user](reference/subactions/twitch-user.md) | Add Random Users | 22 | Review |
| [twitch-user](reference/subactions/twitch-user.md) | Get User Info for Target | 50 | Review |
| [twitch-user](reference/subactions/twitch-user.md) | Get Team Info for Target | 52 | Review |
| [twitch-user](reference/subactions/twitch-user.md) | Add Present User | 550 | Review |
| [youtube-moderation](reference/subactions/youtube-moderation.md) | Ban User | 4008 | Review |
| [youtube-moderation](reference/subactions/youtube-moderation.md) | Timeout User | 4009 | Review |
| [youtube-polls](reference/subactions/youtube-polls.md) | Create Poll | 4011 | Review |
| [youtube-polls](reference/subactions/youtube-polls.md) | End Poll | 4012 | Review |
| [youtube-user-groups](reference/subactions/youtube-user-groups.md) | Add User To Group | 4005 | Review |
| [youtube-user-groups](reference/subactions/youtube-user-groups.md) | Remove User from Group | 4006 | Review |
| [youtube-user-groups](reference/subactions/youtube-user-groups.md) | User In Group | 4007 | Review |
| [youtube-user](reference/subactions/youtube-user.md) | Add Broadcaster Information | 4010 | Review |
| [youtube](reference/subactions/youtube.md) | Send Message to Channel | 4001 | Review |
| [youtube](reference/subactions/youtube.md) | Set Title | 4002 | Review |
| [youtube](reference/subactions/youtube.md) | Set Description | 4003 | Review |
| [youtube](reference/subactions/youtube.md) | Set Title and Description | 4004 | Review |
