//! The live preview on an overlay's own page: a Chromium browser with no window opens the page and
//! sends a picture only when the page repaints (`Page.startScreencast`), at most 30 a second, each one
//! acknowledged. The window asks for the newest picture when told one came, and draws it.
//!
//! Nothing runs until the window shows an overlay's page. The browser is closed when the page is left,
//! when another overlay is opened, and when the window closes (it is gone while Streamerz is hidden).

use futures_util::{SinkExt, StreamExt};
use serde_json::{json, Value};
use std::cell::RefCell;
use std::sync::Mutex;
use std::time::Duration;
use tokio::sync::{mpsc, oneshot};
use tokio_tungstenite::tungstenite::Message;

/// The fastest the pictures come: one each 33 ms (30 a second).
const FRAME_GAP: Duration = Duration::from_millis(33);

/// What is shown, and how to reach the browser that shows it.
struct Session {
    /// The page and its size, so asking again for the same one keeps the browser.
    key: String,
    /// Clicks, the wheel and keys for the page.
    input: mpsc::UnboundedSender<Value>,
    _stop: oneshot::Sender<()>,
}

thread_local! {
    static SESSION: RefCell<Option<Session>> = const { RefCell::new(None) };
}

/// The newest picture: its number, its format (`jpeg` or `png`) and its bytes.
struct Frame {
    seq: u64,
    format: &'static str,
    bytes: Vec<u8>,
    width: u32,
    height: u32,
}

static FRAME: Mutex<Frame> = Mutex::new(Frame {
    seq: 0,
    format: "jpeg",
    bytes: Vec::new(),
    width: 0,
    height: 0,
});

/// What the preview is asked to show.
pub struct Ask {
    pub url: String,
    pub width: u32,
    pub height: u32,
    /// `transparent` (drawn on a checkerboard by the window), `#rrggbb`, or empty for dark grey.
    pub background: String,
}

/// Shows `ask` in the preview: the browser starts, or keeps going when it already shows the same.
pub fn start(ask: Ask) {
    let key = format!("{}|{}x{}|{}", ask.url, ask.width, ask.height, ask.background);
    if SESSION.with_borrow(|s| s.as_ref().is_some_and(|s| s.key == key && !s.input.is_closed())) {
        return;
    }
    let (stop_tx, stop_rx) = oneshot::channel();
    let (input_tx, input_rx) = mpsc::unbounded_channel();
    // The one before goes with its Session: its stop sender is dropped, and its task ends.
    SESSION.with_borrow_mut(|s| {
        *s = Some(Session {
            key,
            input: input_tx,
            _stop: stop_tx,
        })
    });
    if let Ok(mut frame) = FRAME.lock() {
        frame.bytes = Vec::new();
    }
    tokio::task::spawn_local(async move {
        if let Err(error) = run(ask, stop_rx, input_rx).await {
            crate::warn(format!("Overlay preview: {error}"));
            set_error(error);
        }
    });
}

/// Stops the preview and closes its browser.
pub fn stop() {
    SESSION.with_borrow_mut(|s| s.take());
    if let Ok(mut frame) = FRAME.lock() {
        frame.bytes = Vec::new();
    }
    ERROR.with_borrow_mut(String::clear);
}

/// Whether a preview is running.
pub fn running() -> bool {
    SESSION.with_borrow(|s| s.as_ref().is_some_and(|s| !s.input.is_closed()))
}

/// A click, the wheel or a key, at the page's own coordinates: `{"type": "mouse", "event":
/// "mousePressed", "x", "y", "button"}`, `{"type": "wheel", "x", "y", "deltaY"}`, `{"type": "key",
/// "event": "keyDown", "key", "text"}`.
pub fn input(event: Value) {
    SESSION.with_borrow(|s| {
        if let Some(s) = s {
            let _ = s.input.send(event);
        }
    });
}

thread_local! {
    static ERROR: RefCell<String> = const { RefCell::new(String::new()) };
}

fn set_error(error: String) {
    ERROR.with_borrow_mut(|e| *e = error);
    crate::host::preview();
}

/// A picture of the OBS scene, drawn behind a see-through page: its number and its data (base64 JPEG).
static SCENE: Mutex<(u64, String)> = Mutex::new((0, String::new()));

/// Keeps a picture of the scene (`data:image/jpg;base64,...` as OBS gives it) and tells the window.
pub fn set_scene(data_uri: &str) {
    let data = data_uri.split_once("base64,").map_or(data_uri, |(_, d)| d);
    if let Ok(mut scene) = SCENE.lock() {
        scene.0 += 1;
        scene.1 = data.to_owned();
    }
    crate::host::preview();
}

/// The scene's picture when it is newer than `have`.
pub fn scene(have: u64) -> Value {
    match SCENE.lock() {
        Ok(scene) if scene.0 > have && !scene.1.is_empty() => json!({"sceneSeq": scene.0, "scene": scene.1}),
        Ok(scene) => json!({"sceneSeq": scene.0}),
        Err(_) => json!({}),
    }
}

/// The newest picture when it is newer than `have`, base64 encoded; else just where it is.
pub fn frame(have: u64) -> Value {
    let error = ERROR.with_borrow(Clone::clone);
    let Ok(frame) = FRAME.lock() else {
        return json!({"seq": have});
    };
    if frame.seq <= have || frame.bytes.is_empty() {
        return json!({"seq": frame.seq, "running": running(), "error": error});
    }
    use base64::Engine as _;
    json!({"seq": frame.seq, "format": frame.format, "width": frame.width, "height": frame.height,
        "data": base64::engine::general_purpose::STANDARD.encode(&frame.bytes), "running": true, "error": error})
}

fn keep(format: &'static str, data: &str, width: u32, height: u32) {
    use base64::Engine as _;
    let Ok(bytes) = base64::engine::general_purpose::STANDARD.decode(data) else {
        return;
    };
    if let Ok(mut frame) = FRAME.lock() {
        frame.seq += 1;
        frame.format = format;
        frame.bytes = bytes;
        frame.width = width;
        frame.height = height;
    }
    crate::host::preview();
}

/// The DevTools request for one input event, or none when it isn't one.
fn input_request(event: &Value) -> Option<(&'static str, Value)> {
    let number = |k: &str| event[k].as_f64().unwrap_or(0.0);
    match event["type"].as_str()? {
        "mouse" => Some((
            "Input.dispatchMouseEvent",
            json!({"type": event["event"].as_str().unwrap_or("mouseMoved"), "x": number("x"), "y": number("y"),
                "button": event["button"].as_str().unwrap_or("left"), "clickCount": event["clickCount"].as_u64().unwrap_or(1)}),
        )),
        "wheel" => Some((
            "Input.dispatchMouseEvent",
            json!({"type": "mouseWheel", "x": number("x"), "y": number("y"), "deltaX": number("deltaX"), "deltaY": number("deltaY")}),
        )),
        "key" => {
            let mut params = json!({"type": event["event"].as_str().unwrap_or("keyDown"), "key": event["key"]});
            if let Some(text) = event["text"].as_str().filter(|t| !t.is_empty()) {
                params["text"] = text.into();
            }
            Some(("Input.dispatchKeyEvent", params))
        }
        _ => None,
    }
}

async fn run(
    ask: Ask,
    mut stop: oneshot::Receiver<()>,
    mut input: mpsc::UnboundedReceiver<Value>,
) -> Result<(), String> {
    // The preview has sound: the page plays it as OBS would.
    let (mut child, scratch, _name) = crate::overlay_shot::launch(ask.width, ask.height, false)?;
    let address = crate::overlay_shot::devtools_address(&scratch.0).await?;
    let (mut ws, _) = tokio_tungstenite::connect_async(address)
        .await
        .map_err(|e| format!("cannot talk to the browser: {e}"))?;
    let mut next = 0u64;
    // Requests are sent in order and their answers waited for, until the screencast starts.
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
                let Message::Text(text) = message.map_err(|e| format!("the browser went away: {e}"))? else {
                    continue;
                };
                let message: Value = serde_json::from_str(&text).unwrap_or_default();
                if message["id"] == next {
                    if let Some(error) = message["error"]["message"].as_str() {
                        break Err(format!("{}: {error}", $method));
                    }
                    break Ok(message["result"].clone());
                }
            }
        }};
    }
    let none: Option<&String> = None;
    // The tab the browser opened with: a page made by Target.createTarget gets no screencast.
    let targets = call!(none, "Target.getTargets", json!({}))?;
    let first = targets["targetInfos"]
        .as_array()
        .into_iter()
        .flatten()
        .find(|t| t["type"] == "page")
        .cloned();
    let target = match first {
        Some(t) => t,
        None => call!(none, "Target.createTarget", json!({"url": "about:blank"}))?,
    };
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
    call!(
        page,
        "Emulation.setDeviceMetricsOverride",
        json!({"width": ask.width, "height": ask.height, "deviceScaleFactor": 1, "mobile": false})
    )?;
    let see_through = ask.background.trim().eq_ignore_ascii_case("transparent");
    call!(
        page,
        "Emulation.setDefaultBackgroundColorOverride",
        json!({"color": crate::overlay_shot::backdrop(&ask.background)})
    )?;
    let went = call!(page, "Page.navigate", json!({"url": ask.url}))?;
    if let Some(error) = went["errorText"].as_str().filter(|e| !e.is_empty()) {
        return Err(format!("the page did not open: {error}"));
    }
    // The screencast only runs for the page in front.
    call!(none, "Target.activateTarget", json!({"targetId": target["targetId"]}))?;
    call!(page, "Page.bringToFront", json!({}))?;
    // A see-through page needs PNG (JPEG has no transparency); else JPEG, much smaller.
    let format = if see_through { "png" } else { "jpeg" };
    // While the page is still being put in place the browser says it has none: asked again a moment later.
    let mut tries = 0;
    loop {
        let started = call!(
            page,
            "Page.startScreencast",
            json!({"format": format, "quality": 80, "maxWidth": ask.width, "maxHeight": ask.height, "everyNthFrame": 1})
        );
        match started {
            Ok(_) => break,
            Err(error) if tries < 40 && error.contains("Not attached") => {
                tries += 1;
                tokio::time::sleep(Duration::from_millis(250)).await;
            }
            Err(error) => return Err(error),
        }
    }
    let mut last = tokio::time::Instant::now() - FRAME_GAP;
    loop {
        tokio::select! {
            _ = &mut stop => break,
            event = input.recv() => {
                let Some(event) = event else { break };
                if let Some((method, params)) = input_request(&event) {
                    next += 1;
                    let request = json!({"id": next, "method": method, "params": params, "sessionId": session});
                    ws.send(Message::text(request.to_string())).await.map_err(|e| format!("the browser went away: {e}"))?;
                }
            }
            message = ws.next() => {
                let Some(Ok(message)) = message else { return Err("the browser went away".into()) };
                let Message::Text(text) = message else { continue };
                let message: Value = serde_json::from_str(&text).unwrap_or_default();
                if message["method"] != "Page.screencastFrame" {
                    continue;
                }
                let params = &message["params"];
                let meta = &params["metadata"];
                let (width, height) = (
                    meta["deviceWidth"].as_f64().map_or(ask.width, |w| w as u32),
                    meta["deviceHeight"].as_f64().map_or(ask.height, |h| h as u32),
                );
                keep(format, params["data"].as_str().unwrap_or_default(), width, height);
                // The next picture only once this one is acknowledged: at most 30 a second.
                tokio::time::sleep_until(last + FRAME_GAP).await;
                last = tokio::time::Instant::now();
                next += 1;
                let ack = json!({"id": next, "method": "Page.screencastFrameAck",
                    "params": {"sessionId": params["sessionId"]}, "sessionId": session});
                ws.send(Message::text(ack.to_string())).await.map_err(|e| format!("the browser went away: {e}"))?;
            }
        }
    }
    next += 1;
    let _ = ws
        .send(Message::text(
            json!({"id": next, "method": "Browser.close"}).to_string(),
        ))
        .await;
    let _ = tokio::time::timeout(Duration::from_secs(2), child.wait()).await;
    let _ = child.kill().await;
    drop(scratch);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clicks_wheel_and_keys_become_devtools_input() {
        let (method, params) =
            input_request(&json!({"type": "mouse", "event": "mousePressed", "x": 10, "y": 20})).unwrap();
        assert_eq!(method, "Input.dispatchMouseEvent");
        assert_eq!(params["type"], "mousePressed");
        assert_eq!(params["button"], "left");
        let (_, wheel) = input_request(&json!({"type": "wheel", "x": 1, "y": 2, "deltaY": -120})).unwrap();
        assert_eq!(wheel["type"], "mouseWheel");
        assert_eq!(wheel["deltaY"], -120.0);
        let (method, key) =
            input_request(&json!({"type": "key", "event": "keyDown", "key": "a", "text": "a"})).unwrap();
        assert_eq!(method, "Input.dispatchKeyEvent");
        assert_eq!(key["text"], "a");
        assert!(input_request(&json!({"type": "nothing"})).is_none());
    }

    /// Needs a Chromium browser: run by hand.
    #[test]
    #[ignore]
    fn a_real_page_streams_pictures() {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        tokio::task::LocalSet::new().block_on(&runtime, async {
            // A page from a server, as the overlays are (a data: page loads without changing process).
            let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
            let port = listener.local_addr().unwrap().port();
            tokio::task::spawn_local(async move {
                use tokio::io::{AsyncReadExt, AsyncWriteExt};
                while let Ok((mut socket, _)) = listener.accept().await {
                    let mut buffer = [0u8; 2048];
                    let _ = socket.read(&mut buffer).await;
                    let body = "<h1 style='color:white'>hi</h1><script>setInterval(()=>document.body.append('.'),200)</script>";
                    let reply = format!("HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len());
                    let _ = socket.write_all(reply.as_bytes()).await;
                }
            });
            start(Ask {
                url: std::env::var("PREVIEW_URL").unwrap_or(format!("http://127.0.0.1:{port}/")),
                width: 420,
                height: 160,
                background: String::new(),
            });
            let mut got = Value::Null;
            for _ in 0..100 {
                tokio::time::sleep(Duration::from_millis(100)).await;
                got = frame(0);
                if got["data"].is_string() {
                    break;
                }
            }
            let error = ERROR.with_borrow(Clone::clone);
            stop();
            assert!(got["data"].is_string(), "no picture: {got} {error}");
            assert_eq!(got["width"], 420);
        });
    }

    #[test]
    fn the_scene_picture_comes_once_per_shot() {
        set_scene("data:image/jpg;base64,AAEC");
        let got = scene(0);
        let seq = got["sceneSeq"].as_u64().unwrap();
        assert_eq!(got["scene"], "AAEC", "the data without its prefix");
        assert!(scene(seq)["scene"].is_null());
    }

    #[test]
    fn a_frame_is_given_only_when_newer() {
        use base64::Engine as _;
        let data = base64::engine::general_purpose::STANDARD.encode([1u8, 2, 3]);
        keep("jpeg", &data, 4, 3);
        let seq = frame(0)["seq"].as_u64().unwrap();
        assert!(seq > 0);
        assert_eq!(frame(seq - 1)["data"], data);
        assert!(frame(seq)["data"].is_null(), "nothing newer than what the window has");
    }
}
