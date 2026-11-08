# Version 1.6.0 review

This version remains open. Nothing was pushed and no pull request was opened.

## Scope and removal evidence

The starting checkout had uncommitted work on main. Browser discovery, extension publishing and
retiring speech/voice control were confirmed as intentional. They belong to this version.

This review checked the pending changes, compiler diagnostics on Linux and Windows, embedded asset
references, asset-generation scripts, packaging lists, workflow validation and the existing test layers.
It is not a claim that every line of the application or every remote service has been verified.

| Removed or simplified | Evidence and reason |
|---|---|
| Speech recognition and voice controls | Explicitly retired. Removed the runtime modules, dispatch, type registrations, editors, reference pages and scenarios together. Unknown imported JSON remains preserved by the existing model. |
| Four Windows feature flags | Speech, COM, OLE and Variant had no remaining direct consumers after speech removal. Windows cross-target checking passed without them; WinRT, media controls and notification support remain. Transitive features may still be enabled by other Windows APIs. |
| `assets/screenshots/plugins.png` | 45,233 bytes. Added with the README screenshots in commit `1d9af6a`, but no in-repo code, documentation or script references it. Packaging explicitly selects other assets. Removing it changes neither the executable nor the release package. |
| `ensure_clean_dir` and the `os` import in the extension packager | No callers or references. The helper was never used to build an archive. The used `shutil.copyfile` remains. |
| Windows `running_profile` implementation | All callers are under non-Windows guards. The Windows cross-check reported it as dead code. The non-Windows implementation remains. |
| Field-name guessing | Removed with approval. Prefix/substring guesses could turn `userName` into a boolean and `support` into an integer. The synthesized forms leaked their labels into a permanent cache. Explicit forms and the JSON fallback remain; a regression test covers that boundary. |
| Blanket dead-code suppression | Removed obsolete module-wide allowances in model, types and Spotify, plus an unnecessary dialog allowance. Unused Spotify wrappers now live only in the test module, and the wrapper with no callers was deleted. |
| Duplicate picture-cache eviction | One `evict_to` implementation now serves the active and idle budgets. LRU ordering, byte accounting and the minimum one-picture policy remain covered by tests. |
| Unused workflow package installs | The extension publisher uses the Python standard library, including its own JWT implementation. Installing PyJWT and requests was unnecessary. |
| Pending Windows linker flag | Removed the claim of an unmeasured speedup and the indirect linker argument. The existing static CRT configuration remains. |

No integration logos were removed. They serve active UI names and imported action categories.
The Spotify PNG is embedded in the song-request template, even though the ordinary UI uses its SVG.
Brand SVGs generate the tray/window rasters and desktop icons; the development SVG generates the
development raster. Locales are enumerated by `build.rs`, so a missing literal filename reference does
not prove a locale unused. Overlays and extension resources are packaged or served explicitly.

The removal guarantee is limited to consumers in this repository and its release packaging. It cannot
establish whether an external document links to a public asset URL.

## Bugs found during validation

- The Windows unit run exposed a real file-watcher crash on a packed notification record. Replaced
  the misaligned struct reference with checked byte parsing, validated record/name bounds, and added
  a two-byte-aligned regression fixture that runs on Linux as well as Windows.

- Fixed formatting and a clippy needless-borrow error in the pending Rust work.
- Added deterministic extension timers to the test harness and an idle-context disposal regression test.
- Restored the extension-origin restriction on the frame-header rule using the runtime URL hostname.
- Failed extension publishing no longer records a successful content hash or exits successfully.
  Mock tests cover each store failure and the successful-content cache hit, without contacting stores.
- Engine test helpers now reuse thread-local log capture rather than clearing the shared UI history.
  A regression reproduces concurrent history clearing without losing the test log.
- The version-selection shell reads PR titles through an environment variable. A hostile-title test
  verifies that command substitution in a title does not execute.
- The scheduled cleanup workflow now has explicit repository context and valid, paginated artifact JSON
  output instead of an unsupported CLI flag.
- Added the missing Edge tenant secret to the publishing workflow's environment.
- Signing documentation now describes conditional support instead of promising signed binaries.

## Local verification

Run `python3 -P dev/check.py`, with actionlint and cargo-audit installed on PATH. Add `--release` for
Linux release build, scenarios and AppImage packaging; add `--windows` for the MSVC cross-check.
The command reports every failed or unavailable check and returns a failure if any check fails.
It does not upload, sign, publish, push or contact extension stores.

The project rule in `AGENTS.md` requires all applicable checks before a push and an explicit report of
unavailable checks. Local validation reduces CI failures; it cannot guarantee GitHub-hosted service,
permission, credential, operating-system or network behavior.

Final unit suites passed: 533 tests on Linux and 519 Windows tests under Wine. Both executables passed
all 149 scenarios. Detailed checks and outstanding limits are recorded in [TESTING.md](TESTING.md).

## Remaining concerns

- The full-history secret scanner reports nine personal-home-path occurrences in historical versions
  of `docs/memory-results.json`, in commits `26fd638c`, `4460aee4` and `89efc5ad`. They predate this
  version. No allowlist exception or history rewrite was made. The security workflow will fail until
  the history-policy issue is resolved. New staged changes are scanned separately by every commit hook.
- Cargo audit exits successfully, with five allowed warnings: unmaintained `ttf-parser`, `zbus_names`,
  `zvariant`, `zvariant_derive`, and an unsoundness advisory for tokio. No dependency update or warning
  suppression was added in this cleanup. See the local audit output before deciding their remediation.
- Normal AppImage packaging fails on this host because linuxdeploy's bundled strip does not recognize
  a system library's `.relr.dyn`. `NO_STRIP=1 dev/appimage.sh dev` produced the AppImage and zsync file.
  This is a workaround test, not a pass for the normal CI packaging command on Ubuntu.
  The [linuxdeploy maintainers document NO_STRIP](https://github.com/linuxdeploy/linuxdeploy/issues/72).
- A narrow community-channel question clips its first button at 540 pixels in the smoke screenshot.
  The smoke verifies survival and screenshot creation, but does not catch this visual defect. It needs
  a shared dialog-layout fix and a bounds assertion before release.
- Protected-folder elevation was retained with approval and hardened: private per-installation staging,
  ownership and symlink checks, encoded Windows commands and atomic Linux replacement through mktemp.
  UAC and polkit prompts still require native manual verification.
- Native Windows runtime, UAC, release signing, store acceptance and GitHub artifact reuse cannot be
  established by Linux/Wine tests. No store publishing or elevation was executed during this review.
- The connected Twitch + OBS resource budget has not been established. A disconnected fast build used
  39,964 KiB RSS after 50 synthetic messages and zero CPU seconds over 15 idle seconds. The older release
  binary did not complete the same isolation harness, so it is not a comparable before/after baseline.
  These numbers do not demonstrate the required 25 MB connected budget.

Large modules such as chat, platform readers and the action engine remain. Splitting them mechanically
would move code without proving it simpler. Further extraction should follow a concrete shared behavior,
with platform fixtures or smoke coverage, one small change at a time.
