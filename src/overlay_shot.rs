//! A picture of an overlay page as a browser draws it, for whoever builds one without looking at OBS (an
//! assistant over MCP, a script): a Chromium browser found on this computer is started with no window
//! and a profile of its own, opens the page, and is asked for a PNG over its debugging socket. What the
//! page wrote to its console and the errors of its script come back with the picture.
//!
//! Nothing here runs unless a picture is asked for, and the browser is gone when it is taken.

use futures_util::{SinkExt, StreamExt};
use serde_json::{json, Value};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::Duration;
use tokio_tungstenite::tungstenite::Message;

/// What to take a picture of.
pub struct Ask {
    pub url: String,
    pub width: u32,
    pub height: u32,
    /// How long the page is left to draw (and to animate) before the picture.
    pub wait_ms: u64,
    /// What is behind the page: `#rrggbb`, or `transparent` for none (as OBS shows it). Empty: dark grey,
    /// so white text shows in a picture viewer.
    pub background: String,
    /// Where the PNG goes (empty: the temporary folder).
    pub path: String,
    /// How many lines Streamerz's own script adds above the page's first: taken off an error's line,
    /// so it is the line of the file.
    pub script_lines: u64,
}

static NEXT: AtomicU32 = AtomicU32::new(0);

/// The browser's own folder for this one picture, removed with it.
struct Scratch(PathBuf);

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// The colour behind the page, as the browser takes it.
fn backdrop(text: &str) -> Value {
    let hex = text.trim().trim_start_matches('#');
    if text.trim().eq_ignore_ascii_case("transparent") {
        return json!({"r": 0, "g": 0, "b": 0, "a": 0});
    }
    let part = |at: usize| u8::from_str_radix(hex.get(at..at + 2).unwrap_or(""), 16).ok();
    match (hex.len(), part(0), part(2), part(4)) {
        (6, Some(r), Some(g), Some(b)) => json!({"r": r, "g": g, "b": b, "a": 1}),
        _ => json!({"r": 31, "g": 31, "b": 35, "a": 1}),
    }
}

/// One line of what a page's console got: its words, whatever they were.
fn console_text(args: &Value) -> String {
    args.as_array()
        .into_iter()
        .flatten()
        .map(|a| match (&a["value"], a["description"].as_str()) {
            (Value::String(s), _) => s.clone(),
            (Value::Null, Some(d)) => d.to_owned(),
            (v, _) if !v.is_null() => v.to_string(),
            _ => a["type"].as_str().unwrap_or_default().to_owned(),
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// Open the page with no window, call `ready` once it is loaded and listening (to send it made-up
/// events), wait, and take the picture. Gives `{path, width, height, console, errors}`.
pub async fn take(ask: Ask, ready: impl FnOnce()) -> Result<Value, String> {
    use crate::browsers::BrowserEngine;
    let browser = crate::browsers::installed()
        .into_iter()
        .find(|b| matches!(b.engine, BrowserEngine::Chromium | BrowserEngine::Edge))
        .ok_or("A picture needs Chrome, Chromium, Brave or Edge on this computer, and none was found.")?;
    let scratch = Scratch(std::env::temp_dir().join(format!(
        "streamerz-shot-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    )));
    std::fs::create_dir_all(&scratch.0).map_err(|e| format!("cannot make a folder for the browser: {e}"))?;
    let mut child = tokio::process::Command::new(&browser.exe)
        .arg("--headless=new")
        .arg("--remote-debugging-port=0")
        .arg(format!("--user-data-dir={}", scratch.0.display()))
        .arg(format!("--window-size={},{}", ask.width, ask.height))
        .args([
            "--no-first-run",
            "--no-default-browser-check",
            "--disable-extensions",
            "--hide-scrollbars",
            "--mute-audio",
            "--autoplay-policy=no-user-gesture-required",
            "about:blank",
        ])
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .kill_on_drop(true)
        .spawn()
        .map_err(|e| format!("cannot start {}: {e}", browser.name))?;
    let result = tokio::time::timeout(
        Duration::from_millis(40_000 + ask.wait_ms),
        drive(&scratch.0, &ask, ready),
    )
    .await
    .unwrap_or_else(|_| Err(format!("{} took too long to draw the page", browser.name)));
    let _ = child.kill().await;
    result
}

async fn drive(profile: &std::path::Path, ask: &Ask, ready: impl FnOnce()) -> Result<Value, String> {
    // The browser writes where it listens once it is up: the port, then the socket's path.
    let port_file = profile.join("DevToolsActivePort");
    let mut address = None;
    for _ in 0..150 {
        if let Ok(text) = tokio::fs::read_to_string(&port_file).await {
            let mut lines = text.lines();
            if let (Some(port), Some(path)) = (lines.next(), lines.next()) {
                address = Some(format!("ws://127.0.0.1:{port}{path}"));
                break;
            }
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    let address = address.ok_or("the browser did not start")?;
    let (mut ws, _) = tokio_tungstenite::connect_async(address)
        .await
        .map_err(|e| format!("cannot talk to the browser: {e}"))?;
    let mut next = 0u64;
    let mut console: Vec<Value> = Vec::new();
    let mut errors: Vec<String> = Vec::new();
    let mut loaded = false;
    // What the page says arrives between the answers: kept as it comes.
    let (page_url, script_lines) = (ask.url.clone(), ask.script_lines);
    let mut note = |message: &Value, loaded: &mut bool| match message["method"].as_str().unwrap_or_default() {
        "Runtime.consoleAPICalled" if console.len() < 50 => {
            let p = &message["params"];
            console.push(json!({"type": p["type"], "text": console_text(&p["args"])}));
        }
        "Runtime.exceptionThrown" if errors.len() < 50 => {
            let d = &message["params"]["exceptionDetails"];
            // What went wrong, without the browser's trail of addresses, and the line of the file.
            let what = d["exception"]["description"]
                .as_str()
                .or(d["text"].as_str())
                .unwrap_or("error")
                .lines()
                .next()
                .unwrap_or("error");
            let line = d["lineNumber"].as_u64().unwrap_or(0) + 1;
            let own = d["url"].as_str().is_none_or(|url| url.is_empty() || url == page_url);
            errors.push(if own {
                format!("{what} (line {})", line.saturating_sub(script_lines).max(1))
            } else {
                let file = d["url"]
                    .as_str()
                    .unwrap_or_default()
                    .rsplit('/')
                    .next()
                    .unwrap_or_default();
                format!("{what} ({file}:{line})")
            });
        }
        "Page.loadEventFired" => *loaded = true,
        _ => {}
    };
    macro_rules! call {
        ($session:expr, $method:expr, $params:expr) => {{
            next += 1;
            let mut request = json!({"id": next, "method": $method, "params": $params});
            if let Some(session) = $session {
                request["sessionId"] = Value::String(String::clone(session));
            }
            ws.send(Message::text(request.to_string()))
                .await
                .map_err(|e| format!("the browser went away: {e}"))?;
            loop {
                let message = ws.next().await.ok_or("the browser went away")?;
                let message = message.map_err(|e| format!("the browser went away: {e}"))?;
                let Message::Text(text) = message else { continue };
                let message: Value = serde_json::from_str(&text).unwrap_or_default();
                if message["id"] == next {
                    if let Some(error) = message["error"]["message"].as_str() {
                        break Err(format!("{}: {error}", $method));
                    }
                    break Ok(message["result"].clone());
                }
                note(&message, &mut loaded);
            }
        }};
    }
    let none: Option<&String> = None;
    let target = call!(none, "Target.createTarget", json!({"url": "about:blank"}))?;
    let attached = call!(
        none,
        "Target.attachToTarget",
        json!({"targetId": target["targetId"], "flatten": true})
    )?;
    let session = attached["sessionId"]
        .as_str()
        .ok_or("the browser opened no page")?
        .to_owned();
    let page = Some(&session);
    call!(page, "Page.enable", json!({}))?;
    call!(page, "Runtime.enable", json!({}))?;
    call!(
        page,
        "Emulation.setDeviceMetricsOverride",
        json!({"width": ask.width, "height": ask.height, "deviceScaleFactor": 1, "mobile": false})
    )?;
    call!(
        page,
        "Emulation.setDefaultBackgroundColorOverride",
        json!({"color": backdrop(&ask.background)})
    )?;
    let went = call!(page, "Page.navigate", json!({"url": ask.url}))?;
    if let Some(error) = went["errorText"].as_str().filter(|e| !e.is_empty()) {
        return Err(format!("the page did not open: {error}"));
    }
    // Until the page has loaded, then a moment for it to connect to its feed; `ready` sends it the
    // made-up events, and the wait lets it draw them.
    let mut waits = vec![Duration::from_millis(600), Duration::from_millis(ask.wait_ms)];
    let mut ready = Some(ready);
    let started = tokio::time::Instant::now();
    while !loaded && started.elapsed() < Duration::from_secs(20) {
        match tokio::time::timeout(Duration::from_millis(250), ws.next()).await {
            Ok(Some(Ok(Message::Text(text)))) => note(&serde_json::from_str(&text).unwrap_or_default(), &mut loaded),
            Ok(None) | Ok(Some(Err(_))) => return Err("the browser went away".into()),
            _ => {}
        }
    }
    for wait in waits.drain(..) {
        let until = tokio::time::Instant::now() + wait;
        while let Ok(message) = tokio::time::timeout_at(until, ws.next()).await {
            match message {
                Some(Ok(Message::Text(text))) => note(&serde_json::from_str(&text).unwrap_or_default(), &mut loaded),
                Some(Ok(_)) => {}
                _ => return Err("the browser went away".into()),
            }
        }
        if let Some(ready) = ready.take() {
            ready();
        }
    }
    let shot = call!(
        page,
        "Page.captureScreenshot",
        json!({"format": "png", "captureBeyondViewport": false})
    )?;
    use base64::Engine as _;
    let png = base64::engine::general_purpose::STANDARD
        .decode(shot["data"].as_str().unwrap_or_default())
        .map_err(|_| "the browser gave no picture".to_string())?;
    let path = if ask.path.trim().is_empty() {
        std::env::temp_dir().join(format!(
            "streamerz-overlay-{}-{}.png",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ))
    } else {
        let mut path = PathBuf::from(ask.path.trim());
        if path.extension().is_none() {
            path.set_extension("png");
        }
        path
    };
    tokio::fs::write(&path, &png)
        .await
        .map_err(|e| format!("cannot write {}: {e}", path.display()))?;
    let _ = call!(none, "Browser.close", json!({}));
    Ok(
        json!({"path": path.display().to_string(), "width": ask.width, "height": ask.height, "bytes": png.len(),
        "console": console, "errors": errors}),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_backdrop_is_a_colour_or_nothing() {
        assert_eq!(backdrop("transparent")["a"], 0);
        assert_eq!(backdrop("#ff0080"), json!({"r": 255, "g": 0, "b": 128, "a": 1}));
        assert_eq!(backdrop(""), json!({"r": 31, "g": 31, "b": 35, "a": 1}));
        assert_eq!(backdrop("#zzz")["r"], 31);
    }

    #[test]
    fn console_lines_read_as_text() {
        let args = json!([{"type": "string", "value": "spun"}, {"type": "number", "value": 3},
            {"type": "object", "description": "Object"}]);
        assert_eq!(console_text(&args), "spun 3 Object");
    }
}
