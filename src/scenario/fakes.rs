//! Stand-ins for the platforms in a scenario: each keeps every request it gets and answers from the
//! scenario's `fakes` table, so a sub-action is tested down to the request it makes, with nothing sent.
//!
//! ```json
//! "fakes": {"twitch": {"GET users": {"data": [{"id": "9"}]}, "resolve": {"ana": "77"}},
//!           "obs": {"GetCurrentProgramScene": {"sceneName": "Main"}},
//!           "kick": {}, "youtube": {}, "streamlabsDesktop": {}, "spotify": {"current": {"name": "Song"}}}
//! ```
//!
//! An answer `{"$seq": [a, b]}` is `a` the first time and `b` after. A platform listed is connected; one left out is not. `fakes.spotify.api` set to `{http}` sends
//! player actions through the real Spotify adapter to the local web server instead. An answer `{"error": "..."}` fails the call.
//! Every request is kept as `{platform, call, ...}` for `expect.calls`.

use crate::engine::Engine;
use serde_json::{json, Value};
use std::cell::RefCell;
use std::rc::Rc;
use tokio::sync::mpsc::UnboundedReceiver;

/// The requests made, and the answers to give.
pub struct Fakes {
    answers: Value,
    pub calls: RefCell<Vec<Value>>,
    /// How many times each `{"$seq": [...]}` answer was given.
    turns: RefCell<std::collections::HashMap<String, usize>>,
}

thread_local! {
    /// The running scenario's fakes, for the music port (the engine wants it `'static`).
    static CURRENT: RefCell<Option<Rc<Fakes>>> = const { RefCell::new(None) };
}

impl Fakes {
    fn has(&self, platform: &str) -> bool {
        self.answers.get(platform).is_some()
    }

    /// Keeps a request and gives its answer: the table's, else `default`.
    fn call(&self, platform: &str, call: &str, detail: Value, default: Value) -> Result<Value, String> {
        let mut rec = json!({"platform": platform, "call": call});
        if let (Value::Object(r), Value::Object(d)) = (&mut rec, detail) {
            r.extend(d);
        }
        self.calls.borrow_mut().push(rec);
        let mut answer = self.answers[platform].get(call).cloned();
        // `{"$seq": [a, b]}`: a the first time, b after (the last one stays).
        if let Some(list) = answer
            .as_ref()
            .and_then(|a| a.get("$seq"))
            .and_then(Value::as_array)
            .cloned()
        {
            let mut turns = self.turns.borrow_mut();
            let n = turns.entry(format!("{platform} {call}")).or_insert(0);
            answer = list.get((*n).min(list.len().saturating_sub(1))).cloned();
            *n += 1;
        }
        match answer {
            Some(Value::Object(o)) if o.len() == 1 && o.get("error").is_some_and(Value::is_string) => {
                Err(o["error"].as_str().unwrap_or_default().to_string())
            }
            Some(v) => Ok(v),
            None => Ok(default),
        }
    }

    fn unit(&self, platform: &str, call: &str, detail: Value) -> Result<(), String> {
        self.call(platform, call, detail, Value::Null).map(|_| ())
    }
}

/// Plugs the scenario's fakes into `e`. Call inside a LocalSet.
pub fn install(e: &Rc<Engine>, spec: &Value, http: Option<std::net::TcpListener>) -> Rc<Fakes> {
    let f = Rc::new(Fakes {
        answers: spec.clone(),
        calls: RefCell::default(),
        turns: RefCell::default(),
    });
    CURRENT.with(|c| *c.borrow_mut() = Some(f.clone()));
    if f.has("twitch") {
        let (h, rx) = crate::twitch::TwitchHandle::detached();
        e.set_twitch(h);
        tokio::task::spawn_local(twitch(f.clone(), rx));
    }
    if f.has("kick") {
        let (h, rx) = crate::kick::KickHandle::detached();
        e.set_kick(h);
        tokio::task::spawn_local(kick(f.clone(), rx));
    }
    if f.has("youtube") {
        let (h, rx) = crate::youtube::YouTubeHandle::detached();
        e.set_youtube(h);
        tokio::task::spawn_local(youtube(f.clone(), rx));
    }
    if let Some(l) = http.and_then(|l| l.set_nonblocking(true).ok().map(|_| l)) {
        if let Ok(l) = tokio::net::TcpListener::from_std(l) {
            tokio::task::spawn_local(web(f.clone(), l));
        }
    }
    if f.has("obs") {
        e.set_obs(Rc::new(Obs(f.clone())));
    }
    if f.has("streamlabsDesktop") {
        for o in e.config().slobs.connections.clone() {
            let rx = e.slobs_detached(o);
            tokio::task::spawn_local(slobs(f.clone(), rx));
        }
    }
    f
}

/// Forgets the fakes (the scenario ended).
pub fn uninstall() {
    CURRENT.with(|c| c.borrow_mut().take());
}

async fn twitch(f: Rc<Fakes>, mut rx: UnboundedReceiver<crate::twitch::TwitchCmd>) {
    use crate::twitch::TwitchCmd as C;
    const P: &str = "twitch";
    while let Some(cmd) = rx.recv().await {
        match cmd {
            C::SendMessage {
                text,
                reply_id,
                use_bot,
                resp,
                ..
            } => drop(resp.send(f.unit(P, "send", json!({"text": text, "replyId": reply_id, "bot": use_bot})))),
            C::SendAnnouncement {
                text,
                color,
                use_bot,
                resp,
                ..
            } => drop(resp.send(f.unit(P, "announce", json!({"text": text, "color": color, "bot": use_bot})))),
            C::StartRaid { target_login, resp } => drop(
                resp.send(
                    f.call(P, "raid", json!({"login": target_login}), true.into())
                        .map(|v| v.as_bool().unwrap_or(true)),
                ),
            ),
            C::CancelRaid { resp } => drop(resp.send(f.unit(P, "cancelRaid", json!({})))),
            C::Helix {
                method,
                path,
                query,
                body,
                use_bot,
                resp,
                ..
            } => {
                let q: serde_json::Map<String, Value> = query.into_iter().map(|(k, v)| (k, v.into())).collect();
                let detail = json!({"query": q, "body": body, "bot": use_bot});
                drop(resp.send(f.call(P, &format!("{method} {path}"), detail, json!({"data": []}))));
            }
            C::ResolveUserId { login, resp } => {
                let id = f.answers[P]["resolve"][&login]
                    .as_str()
                    .map(String::from)
                    .unwrap_or_else(|| format!("id-{login}"));
                f.calls
                    .borrow_mut()
                    .push(json!({"platform": P, "call": "resolve", "login": login}));
                drop(resp.send(Ok(id)));
            }
            C::Connect => drop(f.unit(P, "connect", json!({}))),
            C::Disconnect => drop(f.unit(P, "disconnect", json!({}))),
            C::GetStatus(resp) => drop(resp.send(crate::twitch::TwitchStatus {
                connected: true,
                ..Default::default()
            })),
        }
    }
}

async fn kick(f: Rc<Fakes>, mut rx: UnboundedReceiver<crate::kick::KickCmd>) {
    use crate::kick::KickCmd as C;
    const P: &str = "kick";
    while let Some(cmd) = rx.recv().await {
        match cmd {
            C::SendMessage {
                text, use_bot, resp, ..
            } => drop(resp.send(f.unit(P, "send", json!({"text": text, "bot": use_bot})))),
            C::ReplyToMessage {
                text,
                reply_id,
                use_bot,
                resp,
                ..
            } => drop(resp.send(f.unit(P, "reply", json!({"text": text, "replyId": reply_id, "bot": use_bot})))),
            C::SetChannelTitle { title, resp } => drop(resp.send(f.unit(P, "title", json!({"title": title})))),
            C::Api {
                method,
                path,
                body,
                resp,
            } => drop(resp.send(f.call(
                P,
                &format!("{method} {path}"),
                json!({"body": body}),
                json!({"data": []}),
            ))),
            C::SetChannelCategory {
                category_id,
                category_name,
                resp,
            } => drop(resp.send(f.unit(P, "category", json!({"id": category_id, "name": category_name})))),
            C::SetChannelTags { tags, resp } => drop(resp.send(f.unit(P, "tags", json!({"tags": tags})))),
            C::StartAdBreak {
                duration_seconds,
                resp,
            } => drop(resp.send(f.call(
                P,
                "adBreak",
                json!({"duration": duration_seconds}),
                json!({"data": {"id": "fake-ad-id", "break_duration_seconds": duration_seconds}}),
            ))),
            C::GetAdStatus { resp } => drop(resp.send(f.call(
                P,
                "adStatus",
                json!({}),
                json!({"data": {"opted_in": true, "ads_blocked": false, "auto_ads_active": false, "remaining_ad_breaks": 5}}),
            ))),
            C::BanUser {
                user_login,
                reason,
                resp,
            } => drop(resp.send(f.unit(P, "ban", json!({"login": user_login, "reason": reason})))),
            C::UnbanUser { user_login, resp } => drop(resp.send(f.unit(P, "unban", json!({"login": user_login})))),
            C::TimeoutUser {
                user_login,
                duration,
                reason,
                resp,
            } => drop(resp.send(f.unit(
                P,
                "timeout",
                json!({"login": user_login, "duration": duration, "reason": reason}),
            ))),
            C::UnTimeoutUser { user_login, resp } => {
                drop(resp.send(f.unit(P, "untimeout", json!({"login": user_login}))))
            }
            C::Connect => drop(f.unit(P, "connect", json!({}))),
            C::Disconnect => drop(f.unit(P, "disconnect", json!({}))),
            C::GetStatus(resp) => drop(resp.send(crate::kick::KickStatus {
                connected: true,
                ..Default::default()
            })),
        }
    }
}

async fn youtube(f: Rc<Fakes>, mut rx: UnboundedReceiver<crate::youtube::YouTubeCmd>) {
    use crate::youtube::YouTubeCmd as C;
    const P: &str = "youtube";
    let text = |v: Result<Value, String>, default: &str| v.map(|v| v.as_str().unwrap_or(default).to_string());
    while let Some(cmd) = rx.recv().await {
        match cmd {
            C::Connect => drop(f.unit(P, "connect", json!({}))),
            C::Disconnect => drop(f.unit(P, "disconnect", json!({}))),
            C::LiveVideo(resp) => {
                let v = f.answers[P]["liveVideo"].as_str().unwrap_or("video1").to_string();
                drop(resp.send(v));
            }
            C::SendMessage {
                broadcast,
                text,
                use_bot,
                resp,
                ..
            } => drop(resp.send(f.unit(P, "send", json!({"broadcast": broadcast, "text": text, "bot": use_bot})))),
            C::SetTitle { broadcast, title, resp } => {
                drop(resp.send(f.unit(P, "title", json!({"broadcast": broadcast, "title": title}))))
            }
            C::SetDescription {
                broadcast,
                description,
                resp,
            } => drop(resp.send(f.unit(
                P,
                "description",
                json!({"broadcast": broadcast, "description": description}),
            ))),
            C::SetTitleAndDescription {
                broadcast,
                title,
                description,
                resp,
            } => drop(resp.send(f.unit(
                P,
                "titleAndDescription",
                json!({"broadcast": broadcast, "title": title, "description": description}),
            ))),
            C::BanUser {
                broadcast,
                user_id,
                resp,
            } => drop(resp.send(f.unit(P, "ban", json!({"broadcast": broadcast, "userId": user_id})))),
            C::TimeoutUser {
                broadcast,
                user_id,
                duration,
                resp,
            } => drop(resp.send(f.unit(
                P,
                "timeout",
                json!({"broadcast": broadcast, "userId": user_id, "duration": duration}),
            ))),
            C::CreatePoll {
                broadcast,
                question,
                options,
                resp,
            } => {
                let detail = json!({"broadcast": broadcast, "question": question, "options": options});
                drop(resp.send(text(f.call(P, "createPoll", detail, "poll1".into()), "poll1")));
            }
            C::EndPoll { broadcast, resp } => drop(resp.send(f.unit(P, "endPoll", json!({"broadcast": broadcast})))),
            C::SetThumbnail {
                broadcast,
                file_path,
                resp,
            } => drop(resp.send(f.unit(P, "setThumbnail", json!({"broadcast": broadcast, "file": file_path})))),
            C::RunCommercial {
                broadcast,
                duration_secs,
                resp,
            } => drop(resp.send(f.unit(
                P,
                "runCommercial",
                json!({"broadcast": broadcast, "duration": duration_secs}),
            ))),
        }
    }
}

/// The local web server: each request is kept as `{platform: "http", call: "METHOD /path", body}` and
/// answered from `fakes.http["METHOD /path"]` (`{"status": 404, "body": ...}`, or the body itself), else
/// 200 with `{}`. One request per connection.
async fn web(f: Rc<Fakes>, l: tokio::net::TcpListener) {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    while let Ok((mut sock, _)) = l.accept().await {
        let f = f.clone();
        tokio::task::spawn_local(async move {
            let mut buf = Vec::new();
            let mut chunk = [0u8; 4096];
            let (head, body) = loop {
                let Ok(n) = sock.read(&mut chunk).await else { return };
                if n == 0 {
                    return;
                }
                buf.extend_from_slice(&chunk[..n]);
                let Some(end) = buf.windows(4).position(|w| w == b"\r\n\r\n") else {
                    continue;
                };
                let head = String::from_utf8_lossy(&buf[..end]).to_string();
                let len = head
                    .lines()
                    .find_map(|l| {
                        let (k, v) = l.split_once(':')?;
                        k.eq_ignore_ascii_case("content-length")
                            .then(|| v.trim().parse::<usize>().ok())?
                    })
                    .unwrap_or(0);
                while buf.len() < end + 4 + len {
                    match sock.read(&mut chunk).await {
                        Ok(0) | Err(_) => break,
                        Ok(n) => buf.extend_from_slice(&chunk[..n]),
                    }
                }
                let body = buf[end + 4..].to_vec();
                break (head, body);
            };
            let line = head.lines().next().unwrap_or("");
            let mut parts = line.split_whitespace();
            let call = format!("{} {}", parts.next().unwrap_or(""), parts.next().unwrap_or(""));
            let text = String::from_utf8_lossy(&body).to_string();
            let body = serde_json::from_str::<Value>(&text).unwrap_or(Value::String(text));
            let answer = f.call("http", &call, json!({"body": body}), json!({}));
            let (status, out) = match answer {
                Ok(Value::Object(o)) if o.get("status").is_some_and(Value::is_u64) => (
                    o["status"].as_u64().unwrap_or(200),
                    o.get("body").cloned().unwrap_or(Value::Null),
                ),
                Ok(v) => (200, v),
                Err(e) => (500, Value::String(e)),
            };
            let out = match out {
                Value::String(t) => t,
                v => v.to_string(),
            };
            let reply = format!(
                "HTTP/1.1 {status} X\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{out}",
                out.len()
            );
            let _ = sock.write_all(reply.as_bytes()).await;
            let _ = sock.shutdown().await;
        });
    }
}

async fn slobs(f: Rc<Fakes>, mut rx: UnboundedReceiver<crate::slobs::Call>) {
    while let Some((resource, method, args, reply)) = rx.recv().await {
        let call = format!("{resource}.{method}");
        drop(reply.send(f.call("streamlabsDesktop", &call, json!({"args": args}), Value::Null)));
    }
}

/// OBS: a request's answer is the table's for its type (`{}` when none).
struct Obs(Rc<Fakes>);

impl crate::ports::ObsPort for Obs {
    fn send_request<'a>(
        &'a self,
        connection_id: Option<&'a str>,
        request_type: &'a str,
        request_data: Value,
    ) -> crate::ports::Fut<'a, Result<Value, String>> {
        let detail = json!({"connection": connection_id, "data": request_data});
        let r = self.0.call("obs", request_type, detail, json!({}));
        Box::pin(async move { r })
    }
    fn send_batch<'a>(
        &'a self,
        connection_id: Option<&'a str>,
        _halt_on_failure: bool,
        requests: Vec<Value>,
    ) -> crate::ports::Fut<'a, Result<Vec<Value>, String>> {
        let out = requests
            .iter()
            .map(|r| {
                let t = r["requestType"].as_str().unwrap_or("");
                let detail = json!({"connection": connection_id, "data": r["requestData"], "batch": true});
                let (ok, data) = match self.0.call("obs", t, detail, json!({})) {
                    Ok(v) => (true, v),
                    Err(e) => (false, json!({"comment": e})),
                };
                json!({"requestType": t, "requestStatus": {"result": ok, "code": if ok { 100 } else { 600 }},
                       "responseData": data})
            })
            .collect();
        Box::pin(async move { Ok(out) })
    }
    fn is_connected(&self, _: Option<&str>) -> bool {
        true
    }
    fn connect(&self, c: Option<&str>) {
        drop(self.0.unit("obs", "connect", json!({"connection": c})));
    }
    fn disconnect(&self, c: Option<&str>) {
        drop(self.0.unit("obs", "disconnect", json!({"connection": c})));
    }
    fn reload(&self) {}
}

/// Spotify: the music port the scenario engine is built with. Without `fakes.spotify` it isn't
/// connected.
pub struct Music;
pub static MUSIC: Music = Music;

fn current() -> Option<Rc<Fakes>> {
    CURRENT.with(|c| c.borrow().clone()).filter(|f| f.has("spotify"))
}

fn track(v: &Value) -> crate::spotify::Track {
    let name = v["name"].as_str().unwrap_or("Song").to_string();
    crate::spotify::Track {
        uri: v["uri"]
            .as_str()
            .map(String::from)
            .unwrap_or_else(|| format!("spotify:track:{}", name.replace(' ', ""))),
        artists: v["artists"]
            .as_array()
            .map(|a| a.iter().filter_map(Value::as_str).map(String::from).collect())
            .unwrap_or_else(|| vec!["Artist".into()]),
        playing: v["playing"].as_bool().unwrap_or(true),
        duration_ms: v["durationMs"].as_u64().unwrap_or(180_000),
        explicit: v["explicit"].as_bool().unwrap_or(false),
        popularity: v["popularity"].as_u64().unwrap_or(50) as u32,
        restricted: v["restricted"].as_bool().unwrap_or(false),
        name,
    }
}

fn music<T: 'static>(
    call: &str,
    detail: Value,
    default: Value,
    map: impl FnOnce(Value) -> Result<T, String> + 'static,
) -> crate::ports::Fut<'static, Result<T, String>> {
    let r = match current() {
        None => Err("Spotify is not connected".to_string()),
        Some(f) => f.call("spotify", call, detail, default).and_then(map),
    };
    Box::pin(async move { r })
}

impl crate::ports::MusicPort for Music {
    fn player_action<'a>(
        &'a self,
        _: &'a Rc<Engine>,
        kind: u32,
        value: &'a str,
    ) -> crate::ports::Fut<'a, Result<Value, String>> {
        // `fakes.spotify.api`: the real adapter's requests, to the local web server.
        let api = current().and_then(|f| f.answers["spotify"]["api"].as_str().map(String::from));
        if let Some(base) = api {
            let value = value.to_string();
            return Box::pin(async move { crate::spotify::player_action_at(&base, "scenario", kind, &value).await });
        }
        music(
            "action",
            json!({"kind": kind, "value": value}),
            json!({"kind": kind, "value": value}),
            Ok,
        )
    }
    fn current<'a>(
        &'a self,
        _: &'a Rc<Engine>,
    ) -> crate::ports::Fut<'a, Result<Option<crate::spotify::Track>, String>> {
        music("current", json!({}), Value::Null, |v| {
            Ok((!v.is_null()).then(|| track(&v)))
        })
    }
    fn find<'a>(
        &'a self,
        _: &'a Rc<Engine>,
        query: &'a str,
    ) -> crate::ports::Fut<'a, Result<crate::spotify::Track, String>> {
        let q = query.to_string();
        music(
            "find",
            json!({"query": query}),
            json!({"name": query}),
            move |v| match v {
                Value::Null => Err(format!("no track for {q}")),
                v => Ok(track(&v)),
            },
        )
    }
    fn add<'a>(
        &'a self,
        _: &'a Rc<Engine>,
        uri: &'a str,
        device: &'a str,
    ) -> crate::ports::Fut<'a, Result<(), String>> {
        music("add", json!({"uri": uri, "device": device}), Value::Null, |_| Ok(()))
    }
    fn volume<'a>(
        &'a self,
        _: &'a Rc<Engine>,
        percent: u32,
        device: &'a str,
    ) -> crate::ports::Fut<'a, Result<(), String>> {
        music(
            "volume",
            json!({"percent": percent, "device": device}),
            Value::Null,
            |_| Ok(()),
        )
    }
    fn like<'a>(&'a self, _: &'a Rc<Engine>, uri: &'a str) -> crate::ports::Fut<'a, Result<(), String>> {
        music("like", json!({"uri": uri}), Value::Null, |_| Ok(()))
    }
    fn player<'a>(&'a self, _: &'a Rc<Engine>) -> crate::ports::Fut<'a, Result<crate::spotify::Player, String>> {
        music("player", json!({}), json!({}), |v| {
            Ok(crate::spotify::Player {
                current: v.get("current").filter(|c| !c.is_null()).map(track),
                playing: v["playing"].as_bool().unwrap_or(true),
                progress_ms: v["progressMs"].as_u64().unwrap_or(0),
                queue: v["queue"].as_array().into_iter().flatten().map(track).collect(),
                device_id: v["deviceId"].as_str().unwrap_or("device1").to_string(),
                volume: v["volume"].as_u64().map(|n| n as u32),
            })
        })
    }
    fn playback<'a>(&'a self, _: &'a Rc<Engine>, command: &'a str) -> crate::ports::Fut<'a, Result<(), String>> {
        music("playback", json!({"command": command}), Value::Null, |_| Ok(()))
    }
}

/// A platform's raw event, through that platform's own handler: `{platform, type, data}`.
pub fn event(e: &Rc<Engine>, ev: &Value) -> Result<(), String> {
    let platform = ev["platform"].as_str().unwrap_or("");
    let kind = ev["type"].as_str().unwrap_or("");
    let data = &ev["data"];
    let channel = ev["channel"].as_str().unwrap_or("channel");
    match platform {
        "twitch" => {
            let b = e
                .config()
                .twitch
                .broadcaster
                .clone()
                .unwrap_or_else(|| crate::twitch::TwitchAccount {
                    user_id: "1".into(),
                    login: "streamer".into(),
                    display_name: "Streamer".into(),
                    ..Default::default()
                });
            crate::twitch::handle_notification(e, &b, e.cfg_tx(), kind, data);
        }
        "kick" => crate::kick::handle_kick_event(e, channel, kind, data),
        "youtube" => {
            let list = match data {
                Value::Array(a) => a.clone(),
                one => vec![one.clone()],
            };
            crate::youtube::handle_innertube_actions(e, &list);
        }
        "tiktok" => crate::tiktok::handle_tiktok_event(e, channel, kind, data),
        "obs" => {
            let o = e.config().obs.connections.first().cloned().unwrap_or_default();
            let info = crate::obs::ObsConnInfo::from_obj(&o);
            let (mut a, mut b) = Default::default();
            crate::obs::handle_obs_message(e, &info, &data.to_string(), &mut a, &mut b);
        }
        "streamlabsDesktop" => {
            let o = e.config().slobs.connections.first().cloned().unwrap_or_default();
            e.slobs_heard(&o, data);
        }
        "" => return Err("an event needs a platform".into()),
        // Donations, shops and webhooks: the body as their webhook would post it.
        p => {
            let body = match data {
                Value::String(s) => s.clone().into_bytes(),
                v => v.to_string().into_bytes(),
            };
            crate::donations::handle_webhook(e, p, &body, ev["signature"].as_str())?;
        }
    }
    Ok(())
}
