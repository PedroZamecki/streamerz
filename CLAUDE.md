# Streamerz project rules

- Commits: conventional commits, one line, max 72 chars: `type(scope)?: subject`.
  Scope is optional. No body, no footer, no `Co-Authored-By` or other trailers. Enforced by
  commitlint (`.githooks/commit-msg`, `commitlint.config.mjs`) and in CI.
- `main` is protected: no direct pushes. Work on a branch, open a pull request; it's merged with a merge
  commit, NEVER squashed or rebased: the history of every commit is kept, so each commit follows the commit
  rule. No approvals required.
- Work goes in versions: one branch and one PR per version, never stacked, named for the version
  (`release/1.1.0`). A version holds every feature, fix and refactor the user asks for while it's open
  (PLAN.md phases included: tick them as they're done). Keep asking the user what else goes in, and before
  opening the PR ask whether the version is complete. Then open the PR, wait for its CI, check it and fix
  what fails (pushing again only for those fixes). At the end of every version ask: "Now is a great time
  to finish, want to release a new version on GitHub?". Merging the PR is the release.
- Test it here, by hand, before closing a version: CI runs only when the version's PR is pushed.
- Branch names (suggested, not enforced): `type/short-description`, e.g. `feat/twitch-chat`,
  `fix/obs-reconnect`, using the same types as commits.
- Before committing, `cargo fmt --check` and `cargo clippy --all-targets -- -D warnings` must pass
  (`.githooks/pre-commit`, and the `lint` workflow on every PR).
- Builds are never repeated: a pull request's CI builds the binaries and keeps them as artifacts named by
  version and tree hash; the merge to `main` publishes those same files when the merged tree is the same
  (`.github/workflows/build.yml`). Keep that working: don't add steps that change the tree after the PR
  is built.
- Releases are automatic: each merge to `main` bumps the version from the commit types
  (`type!:` major, `feat` minor, `fix`/`perf` patch) and publishes the Windows exe and, on Linux, the AppImage,
  the program alone, the .deb, .rpm and Arch packages and the .flatpak (`dev/packages.sh`, all from one build).
  It also signs and puts the APT, DNF/zypper, pacman and Flatpak repositories on GitHub Pages
  (`dev/publish-repos.sh`, key in the `PACKAGES_GPG_KEY` secret, public half `packaging/streamerz.asc`).
  Don't tag by hand, and don't bump the version in `Cargo.toml` (it's unused).
- The name is **Streamerz**, capitalized, everywhere a person or the system sees it: UI, docs, window and
  notification titles, desktop entries, the executable (`Streamerz`, `Streamerz.exe`) and the release files.
  Only identifiers keep lowercase (crate and repo `streamerz`, `streamerz.json`, the config folder, wire ids).
- Language: English everywhere (code, comments, commits, PRs, docs, UI).
- Push as rarely as possible: every push runs CI. Commit locally, test locally, and push once when the
  version is closed (not after every commit or fix).
- No mention of Claude or any other AI agent anywhere: commits, PR titles and descriptions, comments
  (no "Generated with", no "Co-Authored-By").
- Performance is a hard rule: least RAM and CPU possible. If a change can make it even 1% lighter or
  faster, do it; never add a dependency, thread, timer or poll when the OS or existing code covers it.
  Budget: hidden window with Twitch + OBS connected under 25 MB RSS and ~0% idle CPU. Measure hidden RSS
  and idle CPU before and after every version and put the numbers in the PR description.
- Integrations are opt-in: with no login or channel set, one opens no socket, timer or task. Once set up, it
  connects at start by itself (Advanced has the switch to turn that off), and a disabled one opens nothing.
- Windows-only code can be type-checked locally before pushing (official rustc with the Windows std,
  e.g. installed to /tmp/rt): `RUSTC=/tmp/rt/bin/rustc cargo check --features dev-ui
  --target x86_64-pc-windows-msvc --target-dir /tmp/wincheck`, or `cargo xwin check --features dev-ui
  --target x86_64-pc-windows-msvc --target-dir target/xwin`.
- `PLAN.md` is the spec: follow it, tick its phases off, and update it when a decision changes.
  Ask before guessing on anything it doesn't cover. Streamer.bot (https://docs.streamer.bot, source of the
  docs at https://github.com/Streamerbot/docs) is the reference for behaviour, names, protocols and
  formats; songrequestz (`../songrequestz`) and tikstream (`../tikstream`) are the reference for code style,
  structure and CI.
- Test against the real services where possible: the real Streamer.bot (ws://127.0.0.1:8080/, under Wine in
  `~/Games/streamerbot`), OBS, and live Twitch/Kick/YouTube channels. During development Streamerz runs on
  port 8090 next to the real Streamer.bot; the shipped default is 8080.
- Never commit secrets or user data: OAuth tokens, Kick client secret, passwords and anything read from
  the real Streamer.bot's `data/` live only in `streamerz.json` (gitignored) or stay outside the repo.

## Writing and UI

- Follow `docs/UI.md` for every UI change. Treat its page, list, form, dialog and verification patterns as
  required; update the pattern alongside an approved change to the design.
- Text (UI, log lines, messages, docs, commits, PRs): short, plain and natural, as a person would write
  it. Never use the em dash (—) or en dash (–); use a comma, colon, period or parentheses. No decorative
  Unicode symbols (●, ◐, ✓, ★, →, 🔒...) in UI text: show an icon instead.
- Icons are Lucide (`assets/lucide.rs`, ISC), drawn in the text's colour; brand logos (platforms, apps) are
  SVGs in `assets/icons`. A new icon: add its Lucide body to `ICONS`; never draw symbols with text.
- One look everywhere: the zinc colours and quiet text from `frontend/src/theme.rs`, `Health` (`controls/health.rs`)
  for a connection's state, `Rows` for lists, the shared form, prompt and notice controls in
  `frontend/src/controls/`. Reuse these; don't style a widget by hand.
- Every list and tree with actions gets a right-click menu with the same actions as its buttons.
- The UI is GPUI and modular: the application rules live in the toolkit-independent `src/core/` (a test,
  `core/boundary.rs`, fails when it imports a toolkit) and the renderer is the `frontend/` crate, run as
  `Streamerz --frontend`. Shared looks live in `frontend/src/controls/` (see "Building blocks" in
  `docs/UI.md`): change or extend the shared block so every screen gets it, never style one screen apart.
- Set it up for the user: configure everything possible by itself (find what's installed, read what's
  already set, fill in what's known, move on when a step is done). What lives in another program (a
  browser's extensions) is changed only after the user agrees, with everything it will do explained, and
  the steps only the user can take are shown and ticked off. See "Set it up for the user" in `docs/UI.md`.
- - Raw JSON is for power users: show values as forms or tables, with JSON one button away.
- Dates and times on screen go through `fmt::shown_date`, `shown_time`, `shown_iso` or `shown_clock`
  (the user's pick in Settings, else the system's). Never format a date by hand for display.
- Check every UI change on the hidden display (`DISPLAY=:99`) with a screenshot, narrow and wide. Use the
  MCP `ui` tool (dev mode): `shot` saves the app's own window as a PNG (no desktop screenshot, no blind
  coordinates), `find TEXT` says where to click. Run the dev instance only with `env -u WAYLAND_DISPLAY
  DISPLAY=:99`, and kill it by pid (`pkill -x Streamerz`). `DISPLAY=:99 python3 -P dev/ui-smoke.py` (after
  `cargo build --profile fast --features dev-ui`) starts it, visits every page, right-clicks, opens the user
  card and fails on a crash report, a missing screenshot or anything cut off: run it before closing a version,
  and again with `--locale pt_BR` (the longest words). Every bug fixed gets a test (a unit test where it can be, else a step in the smoke).

## Code

- Before adding a helper, look for one: `frontend/src/controls/` (rows, form, prompt, notice, health), `fmt.rs`,
  `edit.rs`.
- Descriptions and names of trigger and sub-action types come from `types.rs`; check a code against it
  before writing anything keyed by it.
- New features that can be driven should be reachable from the MCP server (`mcp.rs`), so an assistant can
  build and test them: add a tool or a WebSocket request, with a test.
