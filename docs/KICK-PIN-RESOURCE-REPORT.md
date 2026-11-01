# Kick pinning resource report

Measured on 2026-10-31 on this Linux machine. The proposed Kick pinning implementation was removed at the user's request. These measurements document the earlier investigation, not current functionality.

A fresh Chromium profile opened the Kick login page on DISPLAY=:99. After 12 seconds, its process group had 18 processes. The sum of resident set sizes was 2,315,672 KiB (2,261 MiB). The sum of proportional set sizes was 792,740 KiB (774 MiB). RSS double-counts shared pages; PSS apportions shared pages among processes. This is one snapshot, not a peak or a repeatable benchmark. Browser extensions, the page load state, machine configuration, and shared pages affect it. The test did not include signing in.

The temporary browser and its profile were removed after the measurement. This measures a candidate login method, not Streamerz's steady-state cost. A browser used only for login should exit afterward; the saved session and network requests would remain, but their memory and CPU costs have not been measured.

The project records an earlier empty hidden release baseline of 10.1 MB RSS and zero CPU ticks over 30 seconds in PLAN.md. This is historical context, not a current measurement and not the required Twitch plus OBS baseline. No current controlled baseline, feature-on measurement, idle CPU measurement, or peak login measurement is available. A live unpin succeeded in the later test below.

## Message storage experiment (removed)

A debug-build allocation probe on 2026-10-31 retained 10,000 copies of one representative Kick pin payload. RSS increased by 5,244 KiB (about 0.51 KiB per payload). The same probe with the earlier JSON object representation increased by 29,628 KiB. These are single-process, single-run RSS deltas and include allocator overhead; they are not measurements of a hidden release build or of CPU. Payload size varies with the message and badges. The experimental implementation kept compact fields only in memory and shared each payload between chat views; it has been removed.

The login browser cost above is temporary. The experimental pin operation used Streamerz's existing asynchronous HTTP client; no persistent browser or timer was started by pinning. A controlled before-and-after hidden RSS and idle CPU measurement with Twitch and OBS connected has not yet been performed.

The pin and unpin HTTP requests were exercised against the live Kick channel using Streamerz's Rust HTTP client: both returned HTTP 200 and the subsequent history request showed the pin cleared. The experimental browser login was not exercised end to end and has been removed.

## Reusing an existing Kick login

Investigation on 2026-10-31 found a Kick session cookie in the existing Chromium profile on this machine. Its stored expiry was in the future; that does not establish that Kick still accepts the session. Only the cookie's metadata was inspected, not its value. The browser did not expose a DevTools port for attaching to its existing session.

A small extension can request `cookies` permission limited to Kick and send the session to a local native messaging host. An isolated Chromium profile was tested with a dummy cookie: the extension read it and passed it to a temporary native host successfully. No real browser cookies or account credentials were used for that test. The host manifest had to be in the selected user-data directory's `NativeMessagingHosts` folder. Chrome starts a separate host process for a one-shot native message; the process can exit after the transfer. The idle memory and startup cost of a production extension and host have not been measured. Windows registration, distribution, session expiration, account matching and the full Streamerz integration still need testing.

Chrome cookies API: https://developer.chrome.com/docs/extensions/reference/api/cookies
Chrome native messaging: https://developer.chrome.com/docs/extensions/develop/concepts/native-messaging

An undocumented mobile login flow may offer a browserless alternative using account credentials and, when required, a one-time code. A GET to its token provider returned HTTP 200. A synthetic login with a non-existent example.invalid account returned HTTP 429, so successful login, 2FA and session compatibility have not been verified. No real credentials were submitted. The flow is not Kick's documented public OAuth API and may change.

Unofficial mobile login description: https://github.com/fb-sean/kick-website-endpoints/blob/main/APP_LOGIN_FLOW.md
Kick's open request for a public pin API: https://github.com/KickEngineering/KickDevDocs/issues/417

## Previously considered direction

Reuse the existing browser login with a small, opt-in extension and a one-shot native messaging host. This avoids a second Kick login when the user is already signed in and avoids starting another browser. The extension needs a one-time installation, access limited to Kick cookies, and a registered host for the user's browser and operating system. The host should exit after the handoff. Verify that the browser session belongs to the account the user intended before saving it. The extension and host were not implemented or measured in Streamerz; the temporary-browser login has also been removed.

The direct mobile login is less attractive until its private protocol and rate limiting can be verified: it would ask users for their Kick password and possibly a one-time code, even when they are already signed in in their browser. The supported public API would be simpler still if Kick adds pinning.
