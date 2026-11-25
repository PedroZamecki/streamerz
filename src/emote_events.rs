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

/// The next text frame, parsed; None once the socket closes, fails or (with `quiet`) says nothing that
/// long. Pings and other frames are skipped (the socket answers pings itself).
async fn next_json(ws: &mut crate::http::Ws, quiet: Option<Duration>) -> Option<Value> {
    loop {
        let msg = match quiet {
            Some(q) => tokio::time::timeout(q, ws.next()).await.ok()??,
            None => ws.next().await?,
        };
        match msg.ok()? {
            Message::Text(t) => return Some(serde_json::from_str(&t).unwrap_or_default()),
            Message::Close(_) => return None,
            _ => {}
        }
    }
}

/// The wait before looking again, after `fails` tries in a row that got nowhere: a minute, doubling, an
/// hour at most (a channel without a 7TV or BTTV account isn't asked about every minute).
fn retry(fails: u32) -> Duration {
    Duration::from_secs(60u64 << fails.saturating_sub(1).min(6)).min(Duration::from_secs(3600))
}

/// How long 7TV's socket may say nothing: three of the heartbeats its hello announces (in ms).
fn seventv_quiet(hello: &Value) -> Option<Duration> {
    let ms = hello.pointer("/d/heartbeat_interval")?.as_u64()?;
    Some(Duration::from_millis(ms.saturating_mul(3)).max(Duration::from_secs(30)))
}

/// Watch the channel `id` on `c.platform` ("twitch", "youtube" or "kick"), reconnecting after a minute.
pub async fn watch(e: Rc<Engine>, c: Codes, id: String) {
    tokio::join!(seventv(&e, &c, &id), bttv(&e, &c, &id));
}

async fn seventv(e: &Rc<Engine>, c: &Codes, id: &str) {
    let mut fails = 0;
    loop {
        fails += 1;
        let user = get(&format!("https://7tv.io/v3/users/{}/{id}", c.platform)).await;
        if let Some(set) = user.pointer("/emote_set/id").and_then(Value::as_str) {
            if let Ok((mut ws, _)) = crate::http::ws_connect("wss://events.7tv.io/v3").await {
                fails = 0;
                let sub = json!({"op": 35, "d": {"type": "emote_set.update", "condition": {"object_id": set}}});
                let _ = ws.send(Message::Text(sub.to_string().into())).await;
                // It says hello at once, then beats: a socket that stops is dead and opened again.
                let mut quiet = Duration::from_secs(90);
                while let Some(v) = next_json(&mut ws, Some(quiet)).await {
                    if v["op"] == 1 {
                        quiet = seventv_quiet(&v).unwrap_or(quiet);
                    }
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
        tokio::time::sleep(retry(fails.max(1))).await;
    }
}

async fn bttv(e: &Rc<Engine>, c: &Codes, id: &str) {
    if c.platform == "kick" {
        return;
    }
    let url = |id: &str| format!("https://cdn.betterttv.net/emote/{id}/3x");
    let mut fails = 0;
    loop {
        fails += 1;
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
            fails = 0;
            let join = json!({"name": "join_channel", "data": {"name": format!("{}:{id}", c.platform)}});
            let _ = ws.send(Message::Text(join.to_string().into())).await;
            while let Some(v) = next_json(&mut ws, None).await {
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
        tokio::time::sleep(retry(fails.max(1))).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;

    #[test]
    fn retries_wait_longer_up_to_an_hour() {
        let mins: Vec<u64> = (0..10).map(|n| retry(n).as_secs() / 60).collect();
        assert_eq!(mins, [1, 1, 2, 4, 8, 16, 32, 60, 60, 60]);
        assert_eq!(retry(u32::MAX).as_secs(), 3600);
    }

    #[test]
    fn seventv_is_dead_after_three_missed_heartbeats() {
        let hello = json!({"op": 1, "d": {"heartbeat_interval": 25000}});
        assert_eq!(seventv_quiet(&hello), Some(Duration::from_secs(75)));
        let fast = json!({"op": 1, "d": {"heartbeat_interval": 10}});
        assert_eq!(seventv_quiet(&fast), Some(Duration::from_secs(30)), "never too eager");
        assert_eq!(seventv_quiet(&json!({"op": 1, "d": {}})), None);
    }

    /// A socket served on loopback: it sends `frames`, then waits for the client to go.
    async fn serve(frames: Vec<Message>) -> crate::http::Ws {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            let (tcp, _) = listener.accept().await.unwrap();
            let mut ws = tokio_tungstenite::accept_async(tcp).await.unwrap();
            for f in frames {
                ws.send(f).await.unwrap();
            }
            while ws.next().await.is_some() {}
        });
        crate::http::ws_connect(format!("ws://{addr}")).await.unwrap().0
    }

    #[test]
    fn pings_do_not_end_a_session_and_silence_does() {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        rt.block_on(async {
            let mut ws = serve(vec![
                Message::Ping(Default::default()),
                Message::Text(r#"{"op":1}"#.into()),
                Message::Pong(Default::default()),
                Message::Text("not json".into()),
            ])
            .await;
            let quiet = Some(Duration::from_millis(300));
            assert_eq!(next_json(&mut ws, quiet).await, Some(json!({"op": 1})));
            assert_eq!(next_json(&mut ws, quiet).await, Some(Value::Null), "kept going");
            assert_eq!(next_json(&mut ws, quiet).await, None, "nothing more: dead");

            let mut ws = serve(vec![Message::Close(None)]).await;
            assert_eq!(next_json(&mut ws, None).await, None);
        });
    }

    #[test]
    fn a_guard_stops_its_task_also_when_its_owner_is_aborted() {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        tokio::task::LocalSet::new().block_on(&rt, async {
            struct Gone(Rc<Cell<u32>>);
            impl Drop for Gone {
                fn drop(&mut self) {
                    self.0.set(self.0.get() + 1);
                }
            }
            let gone = Rc::new(Cell::new(0));
            let mark = Gone(gone.clone());
            let owner = tokio::task::spawn_local(async move {
                let _child = Abort(tokio::task::spawn_local(async move {
                    let _mark = mark;
                    std::future::pending::<()>().await
                }));
                std::future::pending::<()>().await
            });
            for _ in 0..3 {
                tokio::task::yield_now().await;
            }
            assert_eq!(gone.get(), 0, "both running");
            owner.abort();
            for _ in 0..3 {
                tokio::task::yield_now().await;
            }
            assert_eq!(gone.get(), 1, "the child stopped with its owner");
        });
    }
}
