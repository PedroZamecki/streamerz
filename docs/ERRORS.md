# Errors and crashes

Every failure a person can meet is a `StreamerzError` (`src/error.rs`, mirrored for the renderer in
`frontend/src/error.rs`). Nothing shows a bare string or a silent failure.

## The error

| Field | Meaning |
|---|---|
| `code` | Stable id (`config.corrupted`, `server.http.bind`, `frontend.crash`, `unexpected`...) |
| `title` | What happened, in a few words ("Corrupted configuration", "HTTP server start error") |
| `description` | The detail, with credentials redacted (`redact`) |
| `recommendation` | The first thing to try ("Restart the HTTP server") |
| `recommendations` | More steps, when there are any |
| `unexpected` | True when nothing classified it. The detail is kept, never dropped |
| `detail` | The raw cause, redacted |
| `action` | An optional button: open a page (`navigate`) or retry a server (`retry_server`) |

Titles, descriptions and recommendations are translated (`error.*` in `assets/locales`).

## Rules

- A new failure gets a constructor on `StreamerzError` with its own code and text, not a loose string.
- A message nobody classified goes through `StreamerzError::from_message` (it picks a class, else
  `unexpected`) or `StreamerzError::unexpected`: the original text is always kept.
- Anything that can reach a log, a reply, an MCP result or the screen goes through `redact`.
- Backend failures are reported with `report_error` (one log line in the shape `[LEVEL] [code] title:
  description (recommendation)`). A failure that lasts (a server that did not bind, a corrupt config) is
  also kept with `record_active` and removed with `clear_active` when it is fixed. The active list is sent
  to the renderer, which shows it as a dismissible banner above the page.
- Replies to requests, the HTTP API and MCP tools carry the same structure (`to_json_response`,
  `to_http_response`, `format_mcp`). The renderer reads it with `StreamerzError::from_response`; an old
  plain-string reply is upgraded, never lost.
- The renderer shows an error with `controls::notice::error` (title, description, recommendations, action).
  Do not style an error by hand.

## One log, one crash list

The renderer is a separate process (`Streamerz --frontend`), so it cannot write the backend's log itself.
`frontend/src/diag.rs` sends its log lines, steps ("opened page X") and facts to the core over the control
pipe, and the core writes them in the unified log as `[UI]`. The backend and the renderer read as one log.

## When the renderer panics

1. The panic hook writes `crash-ui-*.txt` (message, place, last steps, backtrace, home folder hidden) and
   sends the panic over the pipe before the process ends. If the pipe is gone, the file is still written.
2. The core logs it as `[FATAL] [UI]`. Integrations, MCP and the servers keep running.
3. The core starts the renderer again on its own, at most twice in 60 seconds (`core/parent.rs`).
4. The new window shows a banner that says the window recovered, with "View crashes" and "View log".
5. A third crash inside the window stops the auto-restore (a loop would not help). The tray and a manual
   start still open it, and the crash is on the Crashes page.
6. If the renderer cannot start at all, the core reports `frontend.launch` with what failed.

A renderer that ends without a panic (killed, the display closed) is logged too, with the end of its log.

## Tests

- Unit: `error::tests`, `crash::tests`, `core::parent::tests`, `core::frontend::tests`, `mcp::tests`,
  `servers::tests`, `tests::corrupted_config...` (backend); `error::tests`, `diag::tests`, `app::tests`
  (renderer).
- Smoke (`dev/ui-smoke.py`): a real renderer panic is triggered through the dev pipe. It checks the
  backend answers, the renderer comes back, the banner and its buttons work, the crash is on the Crashes page,
  `[FATAL] [UI]` is in the unified log, the second crash restores again and the third pauses the restore.
