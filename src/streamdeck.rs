//! The server the Streamer.bot Elgato Stream Deck plugin (`bot.streamer.streamdeck`) connects to, as
//! Streamer.bot runs it on port 8059: the plugin talks the usual WebSocket API (GetActions, DoAction,
//! Subscribe), with no Hello first; the Stream Deck sub-actions push `{"request": "SetTitle", "data":
//! {...}}` to it; Connected and Disconnected fire as it comes and goes. All of it checked against
//! Streamer.bot 1.0.7's own server and the plugin's code. Off unless turned on (Servers page).

use crate::engine::Engine;
use crate::model::Obj;
use futures_util::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::rc::Rc;
use std::sync::Arc;
use tokio::net::TcpListener;
use tokio_tungstenite::tungstenite::Message;

pub const T_CONNECTED: u32 = 17002;
pub const T_DISCONNECTED: u32 = 17003;
/// What a button press is, in its arguments (`__source`).
const T_ACTION: u32 = 17001;
/// Its connections listen to events with ids from here, apart from the WebSocket server's.
const IDS: u64 = 1 << 40;

/// Streamer.bot's streamDeckSettings.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Settings {
    pub auto_start: bool,
    pub address: String,
    pub port: u16,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            auto_start: false,
            address: "127.0.0.1".into(),
            port: 8059,
        }
    }
}

/// The server, until dropped (its task aborted).
pub async fn run(e: Rc<Engine>, s: Settings) {
    let l = match TcpListener::bind((s.address.as_str(), s.port)).await {
        Ok(l) => l,
        Err(err) => {
            return crate::error(format!(
                "Stream Deck server: can't listen on {}:{} ({err})",
                s.address, s.port
            ))
        }
    };
    crate::say(format!("Stream Deck server on ws://{}:{}/", s.address, s.port));
    let mut next = IDS;
    loop {
        let Ok((tcp, _)) = l.accept().await else { continue };
        next += 1;
        let (e, id) = (e.clone(), next);
        tokio::task::spawn_local(async move {
            if let Err(err) = connection(&e, tcp, id).await {
                crate::warn(format!("Stream Deck: {err}"));
            }
        });
    }
}

/// Leaves the plugins (and fires Disconnected) however the connection ends.
struct Gone(Rc<Engine>, u64);

impl Drop for Gone {
    fn drop(&mut self) {
        self.0.stream_deck_remove(self.1);
        self.0.listen(
            self.1,
            &crate::engine::events::Subs::new(),
            &tokio::sync::mpsc::unbounded_channel().0,
        );
        self.0.fire(
            T_DISCONNECTED,
            "Disconnected",
            "Elgato/Stream Deck",
            "streamdeck",
            &Obj::new(),
            |_| true,
        );
        self.0.emit("StreamDeck", "Disconnected", || json!({}));
    }
}

async fn connection(e: &Rc<Engine>, tcp: tokio::net::TcpStream, id: u64) -> Result<(), String> {
    let ws = tokio_tungstenite::accept_async(tcp).await.map_err(|x| x.to_string())?;
    let (mut sink, mut stream) = ws.split();
    // What the sub-actions push, and the events it subscribed to.
    let (push_tx, mut push_rx) = tokio::sync::mpsc::unbounded_channel::<String>();
    let (ev_tx, mut ev_rx) = tokio::sync::mpsc::unbounded_channel::<Arc<str>>();
    e.stream_deck_add(id, push_tx);
    let _gone = Gone(e.clone(), id);
    e.fire(
        T_CONNECTED,
        "Connected",
        "Elgato/Stream Deck",
        "streamdeck",
        &Obj::new(),
        |_| true,
    );
    e.emit("StreamDeck", "Connected", || json!({}));
    let mut subs = crate::engine::events::Subs::new();
    loop {
        let out: String = tokio::select! {
            m = stream.next() => {
                let t = match m {
                    Some(Ok(Message::Text(t))) => t,
                    Some(Ok(Message::Close(_))) | None | Some(Err(_)) => return Ok(()),
                    _ => continue,
                };
                let Ok(mut req) = serde_json::from_str::<Value>(&t) else { continue };
                let rid = req.get("id").cloned().unwrap_or(Value::Null);
                match req["request"].as_str().unwrap_or("") {
                    "Subscribe" | "UnSubscribe" => {
                        let ev = req.get("events").cloned().unwrap_or(json!({}));
                        crate::engine::events::update(&mut subs, &ev, req["request"] == "Subscribe");
                        e.listen(id, &subs, &ev_tx);
                        json!({"id": rid, "events": ev, "status": "ok"}).to_string()
                    }
                    // A button: the action runs with where it came from, as Streamer.bot's does.
                    "DoAction" => {
                        if let Some(a) = req["args"].as_object_mut() {
                            a.insert("__source".into(), T_ACTION.into());
                            a.insert("eventSource".into(), "streamdeck".into());
                        } else {
                            req["args"] = json!({"__source": T_ACTION, "eventSource": "streamdeck"});
                        }
                        crate::servers::answer(e, &req, true).unwrap_or(json!({"id": rid})).to_string()
                    }
                    _ => crate::servers::answer(e, &req, true).unwrap_or(json!({"id": rid})).to_string(),
                }
            }
            p = push_rx.recv() => match p { Some(p) => p, None => return Ok(()) },
            v = ev_rx.recv() => match v { Some(v) => v.to_string(), None => continue },
        };
        sink.send(Message::Text(out.into())).await.map_err(|x| x.to_string())?;
    }
}

/// What a Stream Deck sub-action pushes to the plugin, by its code and fields (already expanded).
pub fn push(kind: u64, o: &Obj) -> Option<Value> {
    let s = |k: &str| o.get(k).map(crate::vars::text).unwrap_or_default();
    let state = o.get("state").and_then(Value::as_f64).unwrap_or(0.0) as u64;
    let button = s("buttonId");
    let (request, data) = match kind {
        11001 => (
            "SetTitle",
            json!({"buttonId": button, "text": s("title"), "state": state}),
        ),
        11002 => {
            let mut d = json!({"buttonId": button, "state": state});
            let image = s("image");
            if !s("color").is_empty() {
                d["color"] = s("color").into();
            }
            if image.starts_with("http://") || image.starts_with("https://") {
                d["imageUrl"] = image.into();
            } else if !image.is_empty() {
                use base64::Engine as _;
                match std::fs::read(&image) {
                    Ok(b) => d["imageBase64"] = base64::engine::general_purpose::STANDARD.encode(b).into(),
                    Err(err) => crate::warn(format!("Stream Deck Set Background: {image}: {err}")),
                }
            }
            ("SetImage", d)
        }
        11004 => ("SetState", json!({"buttonId": button, "state": state})),
        11005 => ("ShowAlert", json!({"buttonId": button})),
        11006 => ("ShowOk", json!({"buttonId": button})),
        11007 => ("ToggleState", json!({"buttonId": button})),
        11008 => ("SetValue", json!({"buttonId": button, "value": s("value")})),
        _ => return None,
    };
    Some(json!({"request": request, "data": data}))
}
