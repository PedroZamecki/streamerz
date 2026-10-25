//! BTTV and 7TV emote changes on a channel (each through its own WebSocket), for the platforms'
//! "BetterTTV/SevenTV Emote Added/Removed" triggers. Runs only while a platform is connected and an
//! action uses one of those triggers; the caller aborts it.

use crate::engine::Engine;
use crate::model::Obj;
use futures_util::{SinkExt, StreamExt};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::rc::Rc;
use std::time::Duration;
use tokio_tungstenite::tungstenite::Message;

/// A task that stops when this is dropped.
pub struct Abort(pub tokio::task::JoinHandle<()>);

impl Drop for Abort {
    fn drop(&mut self) {
        self.0.abort();
    }
}

/// The four triggers for one platform: 7TV added, 7TV removed, BTTV added, BTTV removed.
pub struct Codes {
    pub platform: &'static str,
    /// "Twitch", "YouTube" or "Kick" (WebSocket event source).
    pub source: &'static str,
    pub seventv: (u32, u32),
    pub bttv: (u32, u32),
}

impl Codes {
    pub fn wanted(&self, e: &Engine) -> bool {
        [self.seventv.0, self.seventv.1, self.bttv.0, self.bttv.1]
            .iter()
            .any(|c| e.uses_trigger(*c))
    }
}

fn fire(e: &Rc<Engine>, c: &Codes, code: u32, name: &str, ev: &str, emote: &str, url: String) {
    let mut args = Obj::new();
    args.insert("emote".into(), emote.into());
    args.insert("imageUrl".into(), url.into());
    e.fire(code, name, c.source, c.platform, &args, |_| true);
    e.emit(c.source, ev, || json!(args));
}

async fn get(url: &str) -> Value {
    match crate::http::request("GET", url, &[], b"").await {
        Ok(r) if r.status == 200 => serde_json::from_slice(&r.body).unwrap_or_default(),
        _ => Value::Null,
    }
}

/// Watch the channel `id` on `c.platform` ("twitch", "youtube" or "kick"), reconnecting after a minute.
pub async fn watch(e: Rc<Engine>, c: Codes, id: String) {
    tokio::join!(seventv(&e, &c, &id), bttv(&e, &c, &id));
}

async fn seventv(e: &Rc<Engine>, c: &Codes, id: &str) {
    loop {
        let user = get(&format!("https://7tv.io/v3/users/{}/{id}", c.platform)).await;
        if let Some(set) = user.pointer("/emote_set/id").and_then(Value::as_str) {
            if let Ok((mut ws, _)) = crate::http::ws_connect("wss://events.7tv.io/v3").await {
                let sub = json!({"op": 35, "d": {"type": "emote_set.update", "condition": {"object_id": set}}});
                let _ = ws.send(Message::Text(sub.to_string().into())).await;
                while let Some(Ok(Message::Text(t))) = ws.next().await {
                    let v: Value = serde_json::from_str(&t).unwrap_or_default();
                    if v["op"] != 0 {
                        continue;
                    }
                    let body = &v["d"]["body"];
                    let url =
                        |x: &Value| format!("https://cdn.7tv.app/emote/{}/4x.webp", x["id"].as_str().unwrap_or(""));
                    for p in body["pushed"].as_array().into_iter().flatten() {
                        let x = &p["value"];
                        let name = x["name"].as_str().unwrap_or("");
                        fire(
                            e,
                            c,
                            c.seventv.0,
                            "SevenTV Emote Added",
                            "SevenTvEmoteAdded",
                            name,
                            url(x),
                        );
                    }
                    for p in body["pulled"].as_array().into_iter().flatten() {
                        let x = &p["old_value"];
                        let name = x["name"].as_str().unwrap_or("");
                        fire(
                            e,
                            c,
                            c.seventv.1,
                            "SevenTV Emote Removed",
                            "SevenTvEmoteRemoved",
                            name,
                            url(x),
                        );
                    }
                }
            }
        }
        tokio::time::sleep(Duration::from_secs(60)).await;
    }
}

async fn bttv(e: &Rc<Engine>, c: &Codes, id: &str) {
    if c.platform == "kick" {
        return;
    }
    let url = |id: &str| format!("https://cdn.betterttv.net/emote/{id}/3x");
    loop {
        // Removals only say the emote's id: its name from the channel's list.
        let mut names: HashMap<String, String> = HashMap::new();
        let list = get(&format!("https://api.betterttv.net/3/cached/users/{}/{id}", c.platform)).await;
        for e in ["channelEmotes", "sharedEmotes"]
            .iter()
            .flat_map(|k| list[k].as_array().into_iter().flatten())
        {
            names.insert(
                e["id"].as_str().unwrap_or("").into(),
                e["code"].as_str().unwrap_or("").into(),
            );
        }
        if let Ok((mut ws, _)) = crate::http::ws_connect("wss://sockets.betterttv.net/ws").await {
            let join = json!({"name": "join_channel", "data": {"name": format!("{}:{id}", c.platform)}});
            let _ = ws.send(Message::Text(join.to_string().into())).await;
            while let Some(Ok(Message::Text(t))) = ws.next().await {
                let v: Value = serde_json::from_str(&t).unwrap_or_default();
                let d = &v["data"];
                match v["name"].as_str() {
                    Some("emote_create") => {
                        let (eid, code) = (
                            d["emote"]["id"].as_str().unwrap_or(""),
                            d["emote"]["code"].as_str().unwrap_or(""),
                        );
                        names.insert(eid.into(), code.into());
                        fire(
                            e,
                            c,
                            c.bttv.0,
                            "BetterTTV Emote Added",
                            "BetterTtvEmoteAdded",
                            code,
                            url(eid),
                        );
                    }
                    Some("emote_delete") => {
                        let eid = d["emoteId"].as_str().unwrap_or("");
                        let code = names.remove(eid).unwrap_or_default();
                        fire(
                            e,
                            c,
                            c.bttv.1,
                            "BetterTTV Emote Removed",
                            "BetterTtvEmoteRemoved",
                            &code,
                            url(eid),
                        );
                    }
                    _ => {}
                }
            }
        }
        tokio::time::sleep(Duration::from_secs(60)).await;
    }
}
