# streamerz project rules

- Commits: conventional commits, one line, max 72 chars: `type(scope)?: subject`.
  Scope is optional. No body, no footer, no `Co-Authored-By` or other trailers. Enforced by
  commitlint (`.githooks/commit-msg`, `commitlint.config.mjs`) and in CI.
- `main` is protected: no direct pushes. Work on a branch, open a pull request; it's squash-merged,
  so the PR title becomes the commit on `main` and must follow the same commit rule.
  No approvals required.
- One PR at a time, never stacked: merge the open PR before branching the next one from `main`.
  One `PLAN.md` phase per branch/PR; when it works, tick it (and its `COVERAGE.md` items), commit,
  open the PR, and stop.
- Branch names (suggested, not enforced): `type/short-description`, e.g. `feat/twitch-chat`,
  `fix/obs-reconnect`, using the same types as commits.
- Before committing, `cargo fmt --check` and `cargo clippy --all-targets -- -D warnings` must pass
  (`.githooks/pre-commit`, and the `lint` workflow on every PR).
- Releases are automatic: each merge to `main` bumps the version from the commit types
  (`type!:` major, `feat` minor, `fix`/`perf` patch) and publishes the Windows exe and the Linux binary.
  Don't tag by hand, and don't bump the version in `Cargo.toml` (it's unused).
- Language: English everywhere (code, comments, commits, PRs, docs, UI).
- Push as rarely as possible: every push runs CI. Commit locally, test locally, and push once when the
  work is done (not after every commit or fix).
- No mention of Claude or any other AI agent anywhere: commits, PR titles and descriptions, comments
  (no "Generated with", no "Co-Authored-By").
- Performance is a hard rule: least RAM and CPU possible. If a change can make it even 1% lighter or
  faster, do it; never add a dependency, thread, timer or poll when the OS or existing code covers it.
  Budget: hidden window with Twitch + OBS connected under 25 MB RSS and ~0% idle CPU. Measure hidden RSS
  and idle CPU before and after every phase and put the numbers in the PR description.
- Integrations are opt-in: off by default, and a disabled one opens no socket, timer or task.
- Windows-only code can be type-checked locally before pushing (official rustc with the Windows std,
  e.g. installed to /tmp/rt): `RUSTC=/tmp/rt/bin/rustc cargo check --target x86_64-pc-windows-msvc
  --features fltk/fltk-bundled --target-dir /tmp/wincheck`.
- `PLAN.md` is the spec: follow it, tick its phases off, and update it when a decision changes.
  Ask before guessing on anything it doesn't cover. Streamer.bot (https://docs.streamer.bot, source of the
  docs at https://github.com/Streamerbot/docs) is the reference for behaviour, names, protocols and
  formats; songrequestz (`../songrequestz`) and tikstream (`../tikstream`) are the reference for code style,
  structure and CI.
- Test against the real services where possible: the real Streamer.bot (ws://127.0.0.1:8080/, under Wine in
  `~/Games/streamerbot`), OBS, and live Twitch/Kick/YouTube channels. During development streamerz runs on
  port 8090 next to the real Streamer.bot; the shipped default is 8080.
- Never commit secrets or user data: OAuth tokens, Kick client secret, passwords and anything read from
  the real Streamer.bot's `data/` live only in `streamerz.json` (gitignored) or stay outside the repo.
