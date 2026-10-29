# Contributing to Streamerz

Thank you for helping. Streamerz is small on purpose: it has to stay light (under 25 MB of RAM hidden,
no CPU idle), so a change that costs memory or a timer needs a good reason.

Not a programmer? You can still help a lot: [report a bug](https://github.com/PedroZamecki/streamerz/issues/new/choose),
[translate it](LOCALIZATION.md), or tell us what is confusing.

## Build and run

You need [Rust](https://rustup.rs) (stable) and, on Linux, the libraries FLTK builds against
(`libx11-dev libxft-dev libxinerama-dev libxcursor-dev libpango1.0-dev libwayland-dev libdbus-1-dev
libxkbcommon-dev` and friends: [lint.yml](.github/workflows/lint.yml) has the full list).

```sh
cargo fbuild        # a quick build, in target/fast
cargo ftest         # the tests
STREAMERZ_DEV=1 target/fast/Streamerz
```

[DEVELOPING.md](DEVELOPING.md) explains the code, the dev mode and how to test against the real services.
`dev/smoke.py` opens the app on a hidden display and visits every page.

## Send a change

1. Fork, then branch from `main`: `type/short-description`, e.g. `fix/obs-reconnect`.
2. Make the change. Follow the style of the code around it. Text for people (UI, docs, commits) is short,
   plain and in English; the UI strings live in `assets/locales` (no hard-coded text).
3. Before you commit, these must pass (`git config core.hooksPath .githooks` runs them for you):

   ```sh
   cargo fmt --check
   cargo clippy --all-targets -- -D warnings
   cargo test
   ```

4. **Commits** are [conventional](https://www.conventionalcommits.org), one line, 72 characters at most:
   `feat(chat): pinned messages show their time left`. The types decide the next version: `feat` is a
   minor release, `fix` and `perf` a patch, `type!:` a major one.
5. Open a pull request. Every commit stays in the history (it is merged, never squashed), so keep each
   commit clean. CI builds Windows and Linux, runs the tests and the security checks.
6. A bug fix comes with a test (a unit test, or a step in `dev/smoke.py` for what only shows on screen).

## Rules worth knowing

- **Light first.** No new dependency, thread, timer or polling when the OS or the existing code does it.
- **Opt-in.** An integration with nothing set up opens no socket, timer or task.
- **No secrets.** Logins live in the keyring. Never commit tokens, `streamerz.json` or Streamer.bot's data:
  `dev/scan_secrets.py` checks, and [SECURITY.md](SECURITY.md) says how to report a leak.
- **One look.** Colours, icons and widgets come from the helpers in `src/ui.rs` (see DEVELOPING.md); icons
  are [Lucide](https://lucide.dev) in `src/ui/icons.rs`.
- **Streamer.bot is the reference** for names, protocols and formats (https://docs.streamer.bot).
- [PLAN.md](PLAN.md) is the spec: say so in the issue before starting something it doesn't cover.

## Questions

Open an issue or a discussion. Small questions are welcome.
