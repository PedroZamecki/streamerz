# Security

## Report a problem

Please don't open a public issue for a security problem. Use
[Report a vulnerability](https://github.com/PedroZamecki/streamerz/security/advisories/new) (a private
report only the maintainer sees). Say what you found, how to repeat it and what it could do. You will
get an answer within a few days, and credit in the fix if you want it.

Only the latest release is supported: Streamerz updates itself, so fixes reach everyone.

## What Streamerz keeps, and where

- **Logins** (tokens, cookies) and the secrets you type are never plain text in `streamerz.json`. On
  Windows they are encrypted for your Windows user (DPAPI). On Linux they go to your keyring (GNOME
  Keyring, KWallet, KeePassXC). Without a keyring they stay in the file, which only you can read.
- **Logout** revokes the login at Twitch, Kick and Google.
- **Servers**: the WebSocket and HTTP servers listen on `127.0.0.1` unless you change the address. A web
  page from another site can't reach them (its `Origin` is refused), unless you list it in Servers >
  WebSocket > Websites allowed. The UDP server (off by default) listens on every interface, like
  Streamer.bot's, because devices on your network send to it: turn it on only on a network you trust.
- **Crash reports** hide your home folder and never contain logins. You read a report before it is sent.
- **TikTok** uses your own browser (Chrome, Chromium, Edge, Brave or Vivaldi) with a profile of Streamerz's
  own: your everyday browser's cookies are never read.
- Importing an action that runs a program shows it and asks first. Plugins run in a WebAssembly sandbox.

## What is in this repository

Nothing private: no logins, no `streamerz.json`, no Streamer.bot data. The only client id in the code is
Twitch's, which is public by design (the device login needs no secret). Kick and Google apps are yours,
made once and kept in your settings.

This is checked, not assumed:

- `dev/scan_secrets.py` reads what you stage before every commit (`.githooks/pre-commit`) and, in the
  `security` workflow, every line of every commit on every branch. It looks for tokens, keys, login
  cookies, settings files and a person's home folder.
- The same workflow runs `cargo audit` on `Cargo.lock` every week and on every pull request.
- `streamerz.json*`, `community.json*`, backups and logs are in `.gitignore`.

If you find something private in the history, say so through the link above.
