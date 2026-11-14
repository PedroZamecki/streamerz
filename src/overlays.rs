//! Overlays: an HTML file kept in an action group (so it exports and imports with the group's actions),
//! served on the HTTP port at `/overlays/<group>/<file>`. Streamerz adds a small script to the page
//! (`window.streamerz`: the group's settings, and `on(topic)` / `event(name)` for live data over
//! server-sent events on the same port). Nothing listens until an overlay page asks.

use crate::engine::events::Subs;
use crate::engine::Engine;
use crate::groups;
use crate::Config;
use serde_json::{json, Value};
use std::rc::Rc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

/// What a group's pages may ask for, each granted in the group's `permissions`: the name and what
/// it lets a page do, as the person who imports it reads it. Reading what is on screen anyway
/// (viewers, ads, published topics, the group's settings) needs none.
pub const PERMISSIONS: &[(&str, &str)] = &[
    ("chat.read", "Read the chat of your channels"),
    ("alerts.read", "See follows, subscriptions, gifts, donations and raids"),
    ("chat.send", "Send chat messages as you"),
    ("actions.run", "Run the actions of its own group"),
    ("values.write", "Store values in its own group"),
];

/// The token a group's pages call back with: only a page Streamerz served (or saved) has it, and it
/// names the group, so a call can do what that group may and nothing else.
pub fn token(c: &Config, group: &str) -> String {
    use sha2::{Digest, Sha256};
    let mut h = Sha256::new();
    h.update(c.overlay_key.as_bytes());
    h.update([0]);
    h.update(group.as_bytes());
    h.finalize()[..16].iter().map(|b| format!("{b:02x}")).collect()
}

fn allowed(c: &Config, group: &str, permission: &str) -> Result<(), String> {
    if groups::meta(c, group).permissions.iter().any(|p| p == permission) {
        Ok(())
    } else {
        Err(format!("this overlay may not do that (it needs {permission})"))
    }
}

/// `POST /overlays/call`: what a page asks Streamerz to do, as `{group, token, op, ...}`. The answer
/// is `{ok: true, ...}` or `{ok: false, error}`.
///
/// | op | does | needs |
/// |---|---|---|
/// | `run` `{action, args?}` | queues an action of the page's group, by name or id | `actions.run` |
/// | `say` `{text, platform?, bot?}` | sends to chat (every connected platform by default) | `chat.send` |
/// | `set` `{name, value}` | stores a value of the group (`%group.name%` in actions) | `values.write` |
/// | `get` `{name}` | a value of the group | |
/// | `publish` `{topic, value}` | sets a topic for the pages listening | |
/// | `log` `{text, error?}` | a line in the log (a warning with `error`) | |
/// | `chat` `{count, lines?, alerts?}` | the newest chat lines and events, oldest first | `chat.read`, `alerts.read` |
/// | `countdown` `{id}` | a countdown's state now | |
pub fn call(e: &Rc<Engine>, body: &[u8]) -> (u16, Value) {
    let no = |status: u16, error: &str| (status, json!({"ok": false, "error": error}));
    let Ok(req) = serde_json::from_slice::<Value>(body) else {
        return no(400, "the request is not JSON");
    };
    let text = |k: &str| req.get(k).and_then(Value::as_str).unwrap_or("");
    let group = text("group");
    let c = e.config();
    if group.is_empty() || !groups::exists(&c, group) || text("token") != token(&c, group) {
        return no(403, "this page is not one of Streamerz's overlays");
    }
    let done = |r: Result<Value, String>| match r {
        Ok(mut v) => {
            v["ok"] = true.into();
            (200, v)
        }
        Err(error) => (403, json!({"ok": false, "error": error})),
    };
    match text("op") {
        "run" => done(allowed(&c, group, "actions.run").and_then(|_| {
            let wanted = text("action");
            let id = c
                .actions
                .actions
                .iter()
                .find(|a| a.group == group && !wanted.is_empty() && (a.id == wanted || a.name == wanted))
                .map(|a| a.id.clone())
                .ok_or_else(|| format!("the group has no action '{wanted}'"))?;
            let args = req.get("args").and_then(Value::as_object).cloned().unwrap_or_default();
            drop(c);
            e.queue(&id, args)
                .map(|_| json!({}))
                .ok_or_else(|| format!("the action '{wanted}' is off"))
        })),
        // The actions connected to the overlay, for a page that offers them (a dock of buttons).
        "actions" => done(allowed(&c, group, "actions.run").map(|_| {
            let list: Vec<Value> = c
                .actions
                .actions
                .iter()
                .filter(|a| a.group == group)
                .map(|a| json!({"id": a.id, "name": a.name, "enabled": a.enabled}))
                .collect();
            json!({"actions": list})
        })),
        "say" => done(allowed(&c, group, "chat.send").and_then(|_| {
            let (message, platform) = (text("text").trim(), text("platform").to_ascii_lowercase());
            let platform = if platform.is_empty() { "all" } else { platform.as_str() };
            if message.is_empty() || message.len() > 2000 {
                return Err("the message is empty or too long".into());
            }
            if platform != "all" && !crate::engine::chatlog::PLATFORMS.contains(&platform) {
                return Err(format!("unknown platform '{platform}'"));
            }
            if platform != "all" && !e.chat_connected(platform) && !e.dev() {
                return Err(format!("{platform} is not connected"));
            }
            drop(c);
            let sent = e.send_chat(platform, message, req["bot"].as_bool().unwrap_or(false));
            Ok(json!({"platforms": sent}))
        })),
        "set" => done(allowed(&c, group, "values.write").and_then(|_| {
            let name = text("name");
            let value = req.get("value").cloned().unwrap_or(Value::Null);
            if !groups::valid_variable_name(name) {
                return Err("a value's name is letters, digits and _".into());
            }
            if value.to_string().len() > 64 << 10 {
                return Err("the value is too large".into());
            }
            let mut values = groups::values(&c, group);
            values.insert(name.into(), value);
            drop(c);
            let mut result = Ok(());
            e.change(|c| result = groups::set_values(c, group, values));
            result.map(|_| json!({}))
        })),
        "get" => done(Ok(
            json!({"value": groups::values(&c, group).get(text("name")).cloned().unwrap_or(Value::Null)}),
        )),
        "publish" => {
            let topic = text("topic");
            if topic.is_empty() || topic.len() > 100 {
                return no(400, "a topic's name is 1 to 100 characters");
            }
            drop(c);
            e.overlay_publish(topic, req.get("value").cloned().unwrap_or(Value::Null));
            done(Ok(json!({})))
        }
        "chat" => {
            let may = |p| allowed(&c, group, p).is_ok();
            let wants = |k: &str| req[k].as_bool().unwrap_or(true);
            let (lines, alerts) = (
                wants("lines") && may("chat.read"),
                wants("alerts") && may("alerts.read"),
            );
            if !lines && !alerts {
                return done(Err(
                    "this overlay may not read the chat (it needs chat.read or alerts.read)".into(),
                ));
            }
            let count = req["count"].as_u64().unwrap_or(50) as usize;
            done(Ok(json!({"lines": e.chat_recent(count, lines, alerts)})))
        }
        "countdown" => {
            let snapshot = crate::templates::clock::get(&c, text("id"), crate::templates::clock::now_ms());
            done(Ok(json!({"snapshot": snapshot})))
        }
        "log" => {
            let line: String = text("text").chars().take(500).collect();
            // What the page says, or an error of the page itself (the script reports those).
            let error = req["error"].as_bool() == Some(true);
            said(group, error, &line);
            if error {
                crate::warn(format!("Overlay {group}: {line}"));
            } else {
                crate::say(format!("Overlay {group}: {line}"));
            }
            done(Ok(json!({})))
        }
        _ => no(400, "unknown op"),
    }
}

/// How many lines the script Streamerz adds puts above a page's first line.
pub fn script_lines() -> u64 {
    CLIENT.matches('\n').count() as u64
}

/// The page a new overlay starts as: small, commented, and already using the settings and a feed.
pub const STARTER: &str = include_str!("../assets/overlays/starter.html");

/// `window.streamerz` in TypeScript's words, for an editor to offer names and types: put beside a new
/// overlay's page, and served at `/overlays/-/streamerz.d.ts`.
pub const TYPES: &str = include_str!("../assets/overlays/streamerz.d.ts");

/// The name the types file has in an overlay's folder.
pub const TYPES_FILE: &str = "streamerz.d.ts";

/// A page that is open and listening, as the inspector shows it.
struct OpenPage {
    id: u64,
    file: String,
    /// When it opened (ms since 1970).
    since: u64,
    /// The topics and events it asked for.
    topics: Vec<String>,
    events: Vec<String>,
}

/// What lately went to an overlay's open pages and what they said: kept only while one is open.
#[derive(Default)]
struct Trail {
    sent: std::collections::VecDeque<(u64, Arc<str>)>,
    said: std::collections::VecDeque<(u64, bool, String)>,
}

/// How much of each the inspector keeps.
const TRAIL: usize = 30;

thread_local! {
    /// The pages of each group that are open and listening now.
    static OPEN: std::cell::RefCell<std::collections::HashMap<String, Vec<OpenPage>>> = Default::default();
    /// The groups whose folder is watched, so their open pages load again when a file is saved.
    static WATCHED: std::cell::RefCell<std::collections::HashMap<String, tokio::task::AbortHandle>> = Default::default();
    static TRAILS: std::cell::RefCell<std::collections::HashMap<String, Trail>> = Default::default();
}

fn now_ms() -> u64 {
    crate::fmt::unix_now().as_millis() as u64
}

/// How many pages of the group are open now (the ones that listen to something).
pub fn open_pages(group: &str) -> usize {
    OPEN.with_borrow(|open| open.get(group).map_or(0, Vec::len))
}

/// A message went to a page of the group. Several pages get the same one: it is kept once.
fn sent(group: &str, message: &Arc<str>) {
    TRAILS.with_borrow_mut(|trails| {
        let trail = trails.entry(group.to_string()).or_default();
        if trail.sent.iter().rev().take(4).any(|(_, m)| Arc::ptr_eq(m, message)) {
            return;
        }
        if trail.sent.len() == TRAIL {
            trail.sent.pop_front();
        }
        trail.sent.push_back((now_ms(), message.clone()));
    });
}

/// A page of the group said something (`streamerz.log`), or its script failed.
fn said(group: &str, error: bool, line: &str) {
    if open_pages(group) == 0 {
        return;
    }
    TRAILS.with_borrow_mut(|trails| {
        let trail = trails.entry(group.to_string()).or_default();
        if trail.said.len() == TRAIL {
            trail.said.pop_front();
        }
        trail.said.push_back((now_ms(), error, line.to_string()));
    });
}

/// What an overlay is doing now: its open pages with what each listens to, the last messages sent
/// to them (newest last, long ones cut), what they logged and the errors of their scripts, and the
/// topics that hold a value. Empty lists when no page is open.
pub fn inspect(e: &Rc<Engine>, group: &str) -> Value {
    let pages: Vec<Value> = OPEN.with_borrow(|open| {
        open.get(group)
            .into_iter()
            .flatten()
            .map(|p| json!({"file": p.file, "openedMs": p.since, "topics": p.topics, "events": p.events}))
            .collect()
    });
    let (sent, said) = TRAILS.with_borrow(|trails| {
        let Some(trail) = trails.get(group) else {
            return (Vec::new(), Vec::new());
        };
        let sent = trail
            .sent
            .iter()
            .map(|(at, m)| {
                let v: Value = serde_json::from_str(m).unwrap_or_default();
                let name = format!(
                    "{}.{}",
                    v["event"]["source"].as_str().unwrap_or_default(),
                    v["event"]["type"].as_str().unwrap_or_default()
                );
                let mut data = v["data"].to_string();
                if data.len() > 400 {
                    let cut = (0..=400).rev().find(|i| data.is_char_boundary(*i)).unwrap_or(0);
                    data.truncate(cut);
                    data.push_str("...");
                }
                json!({"atMs": at, "event": name, "about": about(&v["event"], &v["data"]), "data": data})
            })
            .collect();
        let said = trail
            .said
            .iter()
            .map(|(at, error, line)| json!({"atMs": at, "error": error, "text": line}))
            .collect();
        (sent, said)
    });
    let topics: serde_json::Map<String, Value> = e
        .overlay_topics()
        .into_iter()
        .filter_map(|t| Some((t.clone(), e.overlay_value(&t)?)))
        .collect();
    json!({"pages": pages, "sent": sent, "log": said, "topics": topics})
}

/// A message sent to a page, in a few words: who said what, what happened, which topic.
fn about(event: &Value, data: &Value) -> String {
    let text = |v: &Value| v.as_str().unwrap_or_default().to_owned();
    let short = |v: &Value| {
        let mut s = match v {
            Value::String(s) => s.clone(),
            v => v.to_string(),
        };
        if s.chars().count() > 120 {
            s = s.chars().take(117).collect::<String>() + "...";
        }
        s
    };
    match (
        event["source"].as_str().unwrap_or_default(),
        event["type"].as_str().unwrap_or_default(),
    ) {
        ("Chat", "Line") => format!("{}: {}", text(&data["user"]), short(&data["text"])),
        ("Chat", "Alert") => format!("{} {}", text(&data["user"]), text(&data["event"])),
        ("Chat", "Removed") => "A message was deleted".into(),
        ("Test", _) => {
            let mut parts: Vec<String> = data["events"]
                .as_array()
                .into_iter()
                .flatten()
                .map(|e| match e[0].as_str().unwrap_or_default() {
                    "Chat.Line" => format!("{}: {}", text(&e[1]["user"]), short(&e[1]["text"])),
                    "Chat.Alert" => format!("{} {}", text(&e[1]["user"]), text(&e[1]["event"])),
                    name => name.to_owned(),
                })
                .collect();
            parts.extend(
                data["topics"]
                    .as_object()
                    .into_iter()
                    .flatten()
                    .map(|(k, _)| format!("topic {k}")),
            );
            format!("Test: {}", parts.join("; "))
        }
        ("Settings", _) => "Its settings changed".into(),
        ("Reload", _) => "A file was saved: the page loads again".into(),
        ("Overlay", topic) => format!("{topic} = {}", short(data)),
        _ => short(data),
    }
}

/// Counts a listening page of a group for as long as it lives.
struct Open(String, u64);

impl Open {
    fn new(group: String, file: String, subs: &Subs) -> Self {
        let id = NEXT.fetch_add(1, Ordering::Relaxed);
        let list = |source: &str| {
            let mut names: Vec<String> = subs.get(source).into_iter().flatten().cloned().collect();
            names.sort();
            names
        };
        let mut events: Vec<String> = subs
            .iter()
            .filter(|(source, _)| !matches!(source.as_str(), "Overlay" | "Test" | "Reload" | "Settings"))
            .flat_map(|(source, kinds)| kinds.iter().map(move |kind| format!("{source}.{kind}")))
            .collect();
        events.sort();
        let page = OpenPage {
            id,
            file,
            since: now_ms(),
            topics: list("Overlay"),
            events,
        };
        OPEN.with_borrow_mut(|open| open.entry(group.clone()).or_default().push(page));
        crate::host::changed();
        Open(group, id)
    }
}

impl Drop for Open {
    fn drop(&mut self) {
        let last = OPEN.with_borrow_mut(|open| {
            let Some(pages) = open.get_mut(&self.0) else {
                return false;
            };
            pages.retain(|p| p.id != self.1);
            if pages.is_empty() {
                open.remove(&self.0);
            }
            !open.contains_key(&self.0)
        });
        // The last page of the group closed: its folder is watched no longer, and what the inspector
        // kept about it goes.
        if last {
            if let Some(watch) = WATCHED.with_borrow_mut(|w| w.remove(&self.0)) {
                watch.abort();
            }
            TRAILS.with_borrow_mut(|trails| trails.remove(&self.0));
        }
        crate::host::changed();
    }
}

/// How long after the last write a saved file counts as saved (an editor writes in several steps).
const SETTLE: std::time::Duration = std::time::Duration::from_millis(300);

/// Whether a change in a group's folder is a file of the overlay being saved: not an editor's
/// scratch file (`.page.html.swp`, `page.html~`, `4913`).
fn saved(change: &crate::fswatch::Change) -> bool {
    use crate::fswatch::Change;
    let path = match change {
        Change::Created(p) | Change::Modified(p) | Change::Deleted(p) | Change::Renamed(_, p) => p,
    };
    let name = crate::fswatch::name_of(path);
    !(name.starts_with('.') || name.ends_with('~') || name.ends_with(".tmp") || !name.contains('.'))
}

/// While a page of the group is open, its folder is watched (by the system, no timer): a file saved
/// there tells the group's open pages to load again, so an edit shows without touching OBS.
fn watch_folder(e: &Rc<Engine>, group: &str) {
    if WATCHED.with_borrow(|w| w.contains_key(group)) {
        return;
    }
    let Some(dir) = groups::folder(&e.config(), group).filter(|dir| dir.is_dir()) else {
        return;
    };
    let (engine, name) = (e.clone(), group.to_string());
    let task = tokio::task::spawn_local(async move {
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
        let watch = crate::fswatch::watch(dir, true, tx);
        tokio::pin!(watch);
        let mut due: Option<tokio::time::Instant> = None;
        loop {
            tokio::select! {
                _ = &mut watch => break,
                change = rx.recv() => match change {
                    None => break,
                    Some(change) if saved(&change) => due = Some(tokio::time::Instant::now() + SETTLE),
                    Some(_) => {}
                },
                _ = async { tokio::time::sleep_until(due.unwrap_or_else(tokio::time::Instant::now)).await }, if due.is_some() => {
                    due = None;
                    // A file added or deleted by hand is now one of the overlay's, or no longer is.
                    if groups::synced(&engine.config(), &name).is_some() {
                        engine.change(|c| {
                            groups::sync_files(c, &name);
                        });
                    }
                    engine.emit("Reload", &name, || json!({}));
                }
            }
        }
    });
    WATCHED.with_borrow_mut(|w| w.insert(group.to_string(), task.abort_handle()));
}

/// What a page says about itself (`Meta.template.pages`), or a plain page of that file.
pub fn page_of(c: &Config, group: &str, file: &str) -> groups::Page {
    groups::meta(c, group)
        .template
        .pages
        .into_iter()
        .find(|p| p.file == file)
        .unwrap_or_else(|| groups::Page {
            file: file.into(),
            ..Default::default()
        })
}

/// The name a page goes by: its title, else its file without the ending.
pub fn page_name(page: &groups::Page) -> String {
    if !page.title.trim().is_empty() {
        return page.title.trim().to_string();
    }
    let file = page.file.rsplit('/').next().unwrap_or(&page.file);
    file.rsplit_once('.').map_or(file, |x| x.0).to_string()
}

/// The OBS browser source of a page: its name there and its settings. The size is the page's own,
/// else a full 1920x1080 canvas (an overlay is drawn over the whole picture).
pub fn obs_source(c: &Config, group: &str, file: &str) -> (String, Value) {
    let page = page_of(c, group, file);
    let name = if page_name(&page).eq_ignore_ascii_case(group) {
        group.to_string()
    } else {
        format!("{group}: {}", page_name(&page))
    };
    let (width, height) = if page.width > 0 && page.height > 0 {
        (page.width, page.height)
    } else {
        (1920, 1080)
    };
    // Kept loaded while hidden: an overlay that reloads each time its scene shows loses its state.
    let settings = json!({"url": url(c, group, file), "width": width, "height": height, "shutdown": false,
        "restart_when_active": false});
    (name, settings)
}

/// Put a page into OBS as a browser source of the scene on air. One made before (the same name) is
/// pointed at the page again and not doubled. Says what happened: "added" or "updated", and the name.
pub async fn add_to_obs(e: &Rc<Engine>, group: &str, file: &str) -> Result<(&'static str, String), String> {
    let (name, settings) = {
        let c = e.config();
        if groups::file_path(&c, group, file).is_none() {
            return Err("that page is gone".into());
        }
        if e.obs().is_none() && c.obs.connections.is_empty() {
            return Err("No OBS connection is set up".into());
        }
        obs_source(&c, group, file)
    };
    let obs = e.obs_start();
    let inputs = obs
        .send_request(None, "GetInputList", json!({"inputKind": "browser_source"}))
        .await?;
    let exists = inputs["inputs"]
        .as_array()
        .is_some_and(|list| list.iter().any(|i| i["inputName"].as_str() == Some(&name)));
    if exists {
        obs.send_request(
            None,
            "SetInputSettings",
            json!({"inputName": name, "inputSettings": settings, "overlay": true}),
        )
        .await?;
        return Ok(("updated", name));
    }
    let scene = obs.send_request(None, "GetCurrentProgramScene", json!({})).await?;
    let scene = scene["sceneName"]
        .as_str()
        .or(scene["currentProgramSceneName"].as_str())
        .ok_or("OBS did not say which scene is on")?
        .to_string();
    obs.send_request(
        None,
        "CreateInput",
        json!({"sceneName": scene, "inputName": name, "inputKind": "browser_source", "inputSettings": settings,
            "sceneItemEnabled": true}),
    )
    .await?;
    Ok(("added", name))
}

const CLIENT: &str = include_str!("../assets/overlays/client.js");

/// Listener ids above the WebSocket connections' (they count up from 1).
static NEXT: AtomicU64 = AtomicU64::new(1 << 62);

fn encode(s: &str) -> String {
    s.bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => (b as char).to_string(),
            _ => format!("%{b:02X}"),
        })
        .collect()
}

fn escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

/// The overlay files of every group: (group, file name).
pub fn list(c: &Config) -> Vec<(String, String)> {
    let mut out = Vec::new();
    for g in groups::all(c) {
        for f in groups::meta(c, &g).files {
            if groups::is_page(&f.name) {
                out.push((g.clone(), f.name));
            }
        }
    }
    out
}

/// A group's settings as its pages get them: a `file` setting is the address the file is served at
/// (a web address stays as it is, none stays empty), so a page uses it in `src` or `url()` as it comes.
pub fn settings(c: &Config, group: &str) -> Value {
    let mut values = groups::values(c, group);
    for d in groups::meta(c, group).variables {
        if d.kind != groups::Kind::File {
            continue;
        }
        if let Some(Value::String(file)) = values.get(&d.name) {
            if !file.is_empty() && !file.starts_with("https://") && !file.starts_with("http://") {
                let address = url(c, group, file);
                values.insert(d.name.clone(), address.into());
            }
        }
    }
    Value::Object(values.into_iter().collect())
}

/// Where overlays are served, as a browser opens it.
pub fn base(c: &Config) -> String {
    let h = &c.http;
    let host = if h.address == "0.0.0.0" || h.address.is_empty() {
        "127.0.0.1"
    } else {
        &h.address
    };
    format!("http://{host}:{}", h.port)
}

pub fn url(c: &Config, group: &str, file: &str) -> String {
    url_at(&base(c), group, file)
}

pub fn url_at(base: &str, group: &str, file: &str) -> String {
    // A file's folders stay as they are, so the page's relative links work.
    let file: Vec<String> = file.split('/').map(encode).collect();
    format!("{base}/overlays/{}/{}", encode(group), file.join("/"))
}

/// The page with Streamerz's script added at the top of its head. `server` is empty when the page is
/// served by Streamerz, and its address in a copy saved to disk.
pub fn page(html: &str, settings: &Value, group: &str, server: &str, token: &str) -> String {
    let config = json!({"server": server, "settings": settings, "group": group, "token": token})
        .to_string()
        .replace("</", "<\\/");
    let script = format!("<script>{}</script>", CLIENT.replace("__STREAMERZ__", &config));
    format!("{}{script}{}", &html[..head_end(html)], &html[head_end(html)..])
}

/// Where the script goes: just inside `<head>`, else after the doctype (before it, a browser would
/// fall into quirks mode), else at the start.
fn head_end(html: &str) -> usize {
    let lower = html.to_ascii_lowercase();
    let after = |tag: &str| {
        lower.match_indices(tag).find_map(|(i, _)| {
            let next = lower.as_bytes().get(i + tag.len()).copied();
            if matches!(next, Some(b'>' | b' ' | b'\t' | b'\r' | b'\n')) {
                lower[i..].find('>').map(|j| i + j + 1)
            } else {
                None
            }
        })
    };
    after("<head").or_else(|| after("<!doctype")).unwrap_or(0)
}

fn mime(name: &str) -> &'static str {
    match name.rsplit('.').next().unwrap_or("").to_ascii_lowercase().as_str() {
        "html" | "htm" => "text/html; charset=utf-8",
        "js" | "mjs" => "text/javascript; charset=utf-8",
        "css" => "text/css; charset=utf-8",
        "json" => "application/json",
        "svg" => "image/svg+xml",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "mp3" => "audio/mpeg",
        "ogg" => "audio/ogg",
        "wav" => "audio/wav",
        "mp4" => "video/mp4",
        "webm" => "video/webm",
        "woff2" => "font/woff2",
        "woff" => "font/woff",
        "ttf" => "font/ttf",
        "txt" => "text/plain; charset=utf-8",
        _ => "application/octet-stream",
    }
}

fn index(c: &Config) -> String {
    let mut rows = String::new();
    for (g, f) in list(c) {
        rows += &format!(
            "<li><a href=\"/overlays/{}/{}\">{} / {}</a></li>",
            encode(&g),
            encode(&f),
            escape(&g),
            escape(&f)
        );
    }
    if rows.is_empty() {
        rows = "<li>No overlays yet. Add an .html file to an action group.</li>".into();
    }
    format!(
        "<!doctype html><meta charset=utf-8><title>Streamerz overlays</title>\
         <style>body{{font:15px sans-serif;background:#18181b;color:#e4e4e7;margin:2em}}a{{color:#a78bfa}}</style>\
         <h1>Streamerz overlays</h1><ul>{rows}</ul>"
    )
}

/// What `GET /overlays...` answers.
pub enum Served {
    /// Status, content type, body.
    Bytes(u16, &'static str, Vec<u8>),
    /// A file sent from disk as it is, piece by piece: its content type and path.
    Disk(&'static str, std::path::PathBuf),
}

impl Served {
    /// Status, content type and body, read from disk when that is where it is.
    #[cfg(test)]
    pub fn read(self) -> (u16, &'static str, Vec<u8>) {
        match self {
            Served::Bytes(status, kind, body) => (status, kind, body),
            Served::Disk(kind, path) => (200, kind, std::fs::read(path).unwrap()),
        }
    }
}

pub fn file(c: &Config, path: &str) -> Served {
    let rest = path.trim_start_matches("/overlays").trim_matches('/');
    if rest.is_empty() {
        return Served::Bytes(200, "text/html; charset=utf-8", index(c).into_bytes());
    }
    let mut parts = rest.splitn(2, '/');
    let group = crate::donations::urlencoding_decode(parts.next().unwrap_or(""));
    let name = crate::donations::urlencoding_decode(parts.next().unwrap_or(""));
    let missing = || Served::Bytes(404, "text/plain; charset=utf-8", b"No such overlay file".to_vec());
    if group == BUILT_IN {
        return built_in(&name).unwrap_or_else(missing);
    }
    let kind = mime(&name);
    if !kind.starts_with("text/html") {
        if let Some(path) = groups::file_path(c, &group, &name) {
            return Served::Disk(kind, path);
        }
    }
    let Some(bytes) = groups::read_file(c, &group, &name) else {
        return missing();
    };
    if kind.starts_with("text/html") {
        let html = String::from_utf8_lossy(&bytes);
        let settings = settings(c, &group);
        return Served::Bytes(
            200,
            kind,
            page(&html, &settings, &group, "", &token(c, &group)).into_bytes(),
        );
    }
    Served::Bytes(200, kind, bytes)
}

/// Where the pictures the window is drawn with come from (logos, badges, icons), by path. The program's
/// start hands it in, so they are in the program once and nothing here knows the window's toolkit.
pub type Pictures = fn(&str) -> Option<std::borrow::Cow<'static, [u8]>>;
static PICTURES: std::sync::OnceLock<Pictures> = std::sync::OnceLock::new();

pub fn pictures(from: Pictures) {
    let _ = PICTURES.set(from);
}

/// The folder of what every page can use, in place of a group's name: `/overlays/-/...`.
pub const BUILT_IN: &str = "-";

/// What Streamerz itself has for a page: the pictures the window is drawn with, and the cover of the
/// song when the player gave it as a picture.
///
/// | path | is |
/// |---|---|
/// | `brands/<name>.svg` | a platform's or an app's logo (twitch, youtube, kick, tiktok, spotify...) |
/// | `badges/<platform>-<role>.svg` | a chat badge the platform has no picture address for |
/// | `icons/<name>.svg` | a Lucide icon in white, `icons/<rrggbb>/<name>.svg` in that colour |
/// | `cover` | the cover of the song playing (the `song` topic gives its address) |
/// | `guide` | the guide to making overlays, as a page |
/// | `streamerz.d.ts` | `window.streamerz` as TypeScript types, for an editor |
fn built_in(name: &str) -> Option<Served> {
    // The guide (docs/OVERLAYS.md, as the MCP tool `guide` gives it), drawn as a page.
    if name == "guide" {
        let text = serde_json::to_string(crate::docs::guide("OVERLAYS")?)
            .ok()?
            .replace("</", "<\\/");
        let page = include_str!("../assets/overlays/guide.html").replace("__GUIDE__", &text);
        return Some(Served::Bytes(200, "text/html; charset=utf-8", page.into_bytes()));
    }
    if name == TYPES_FILE {
        return Some(Served::Bytes(
            200,
            "text/plain; charset=utf-8",
            TYPES.as_bytes().to_vec(),
        ));
    }
    if name == "cover" {
        let bytes = crate::spotify::local::cover()?;
        let kind = if bytes.starts_with(b"\x89PNG") {
            "image/png"
        } else {
            "image/jpeg"
        };
        return Some(Served::Bytes(200, kind, bytes));
    }
    let (path, color) = match name.strip_prefix("icons/").and_then(|rest| rest.split_once('/')) {
        Some((hex, icon)) if groups::is_color(&format!("#{hex}")) => (format!("icons/{icon}"), hex),
        _ => (name.to_string(), "ffffff"),
    };
    if !["icons/", "brands/", "badges/"]
        .iter()
        .any(|folder| path.starts_with(folder))
    {
        return None;
    }
    let bytes = PICTURES.get()?(&path)?;
    let bytes = if path.starts_with("icons/") {
        String::from_utf8_lossy(&bytes)
            .replace("stroke=\"black\"", &format!("stroke=\"#{color}\""))
            .into_bytes()
    } else {
        bytes.into_owned()
    };
    Some(Served::Bytes(200, mime(&path), bytes))
}

/// `POST /overlays/call` with `op: "avatar"` `{platform, userId}`: the picture of someone in the chat,
/// asked from the platform once and remembered (the same lookup as the Chat page's user card). Needs
/// `chat.read` or `alerts.read`. None: the request is another op, for `call`.
pub async fn avatar(e: &Rc<Engine>, body: &[u8]) -> Option<(u16, Value)> {
    let req = serde_json::from_slice::<Value>(body).ok()?;
    if req["op"] != "avatar" {
        return None;
    }
    let text = |k: &str| req.get(k).and_then(Value::as_str).unwrap_or("");
    let (group, platform, user) = (text("group"), text("platform"), text("userId"));
    {
        let c = e.config();
        if group.is_empty() || !groups::exists(&c, group) || text("token") != token(&c, group) {
            return Some((
                403,
                json!({"ok": false, "error": "this page is not one of Streamerz's overlays"}),
            ));
        }
        if allowed(&c, group, "chat.read").is_err() && allowed(&c, group, "alerts.read").is_err() {
            return Some((
                403,
                json!({"ok": false, "error": "this overlay may not ask about chatters (it needs chat.read or alerts.read)"}),
            ));
        }
    }
    if user.is_empty() || !crate::engine::chatlog::PLATFORMS.contains(&platform) {
        return Some((200, json!({"ok": true, "url": ""})));
    }
    let facts = e.chatter_lookup(platform, user, &|_| {}).await;
    let url = facts
        .iter()
        .find(|fact| fact.0 == "Picture")
        .map_or("", |fact| fact.1.as_str());
    Some((200, json!({"ok": true, "url": url})))
}

pub async fn respond(tcp: &mut TcpStream, status: u16, kind: &str, body: &[u8]) -> std::io::Result<()> {
    let reason = match status {
        200 => "OK",
        400 => "Bad Request",
        403 => "Forbidden",
        404 => "Not Found",
        _ => "Internal Server Error",
    };
    let head = format!(
        "HTTP/1.1 {status} {reason}\r\nConnection: close\r\nContent-Type: {kind}\r\nContent-Length: {}\r\n\
         Cache-Control: no-store\r\nAccess-Control-Allow-Origin: *\r\n\r\n",
        body.len()
    );
    tcp.write_all(head.as_bytes()).await?;
    tcp.write_all(body).await?;
    tcp.shutdown().await
}

/// The bytes a `Range` header asks for, of a file of `len` bytes: first and last, inclusive. None: the
/// header isn't one range of bytes (the whole file is sent); Err: it starts past the end.
pub fn range(header: &str, len: u64) -> Option<Result<(u64, u64), ()>> {
    let (from, to) = header.trim().strip_prefix("bytes=")?.split_once('-')?;
    let (from, to) = (from.trim(), to.trim());
    let found = if from.is_empty() {
        let last: u64 = to.parse().ok()?;
        (len.saturating_sub(last), len.checked_sub(1)?)
    } else {
        let first: u64 = from.parse().ok()?;
        let last = if to.is_empty() { u64::MAX } else { to.parse().ok()? };
        (first, last.min(len.saturating_sub(1)))
    };
    Some(if found.0 > found.1 || found.0 >= len {
        Err(())
    } else {
        Ok(found)
    })
}

/// A file from disk, read and sent in pieces (a long video never sits in memory), with the part a
/// player asks for (`Range`) and nothing at all when the browser already has this version (`ETag`).
pub async fn respond_disk(
    tcp: &mut TcpStream,
    kind: &str,
    path: &std::path::Path,
    wanted: Option<&str>,
    have: Option<&str>,
) -> std::io::Result<()> {
    use tokio::io::AsyncSeekExt;
    let opened = async {
        let file = tokio::fs::File::open(path).await?;
        let meta = file.metadata().await?;
        std::io::Result::Ok((file, meta))
    };
    let Ok((mut file, meta)) = opened.await else {
        return respond(tcp, 404, "text/plain; charset=utf-8", b"No such overlay file").await;
    };
    let len = meta.len();
    let changed = meta
        .modified()
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map_or(0, |d| d.as_millis());
    let tag = format!("\"{len:x}-{changed:x}\"");
    let common = format!(
        "Connection: close\r\nContent-Type: {kind}\r\nETag: {tag}\r\nAccept-Ranges: bytes\r\n\
         Cache-Control: no-cache\r\nAccess-Control-Allow-Origin: *\r\n"
    );
    if have.is_some_and(|h| h.split(',').any(|t| t.trim() == tag)) {
        tcp.write_all(format!("HTTP/1.1 304 Not Modified\r\n{common}\r\n").as_bytes())
            .await?;
        return tcp.shutdown().await;
    }
    let (first, last, head) = match wanted.and_then(|w| range(w, len)) {
        Some(Err(())) => {
            let head = format!(
                "HTTP/1.1 416 Range Not Satisfiable\r\n{common}Content-Range: bytes */{len}\r\nContent-Length: 0\r\n\r\n"
            );
            tcp.write_all(head.as_bytes()).await?;
            return tcp.shutdown().await;
        }
        Some(Ok((first, last))) => (
            first,
            last,
            format!("HTTP/1.1 206 Partial Content\r\n{common}Content-Range: bytes {first}-{last}/{len}\r\n"),
        ),
        None => (0, len.saturating_sub(1), format!("HTTP/1.1 200 OK\r\n{common}")),
    };
    let mut left = if len == 0 { 0 } else { last - first + 1 };
    tcp.write_all(format!("{head}Content-Length: {left}\r\n\r\n").as_bytes())
        .await?;
    if first > 0 {
        file.seek(std::io::SeekFrom::Start(first)).await?;
    }
    let mut buf = vec![0u8; 64 << 10];
    while left > 0 {
        let want = left.min(buf.len() as u64) as usize;
        let n = file.read(&mut buf[..want]).await?;
        if n == 0 {
            break;
        }
        tcp.write_all(&buf[..n]).await?;
        left -= n as u64;
    }
    tcp.shutdown().await
}

/// The subscriptions a page asks for: `topics=a,b` and `events=Source.Type,Source.*`.
pub fn subscriptions(query: &str) -> Subs {
    let mut subs = Subs::new();
    for pair in query.split('&') {
        let Some((k, v)) = pair.split_once('=') else { continue };
        let v = crate::donations::urlencoding_decode(&v.replace('+', " "));
        for item in v.split(',').map(str::trim).filter(|i| !i.is_empty()) {
            match k {
                "topics" => {
                    subs.entry("Overlay".into()).or_default().insert(item.into());
                }
                "events" => {
                    let (source, kind) = item.split_once('.').unwrap_or((item, "*"));
                    subs.entry(source.into()).or_default().insert(kind.into());
                }
                _ => {}
            }
        }
    }
    subs
}

/// The subscriptions left after taking away what the page's group may not read: the chat and the
/// alerts need the group's permission and the page's token (`group=` and `token=`), and a page
/// follows the settings of its own group only. Topics stay open to every page.
/// The group whose page asks, when the query carries its name and token (`group=` and `token=`).
fn caller(c: &Config, query: &str) -> Option<String> {
    let value = |key: &str| {
        query
            .split('&')
            .filter_map(|p| p.split_once('='))
            .find(|(k, _)| *k == key)
            .map(|(_, v)| crate::donations::urlencoding_decode(&v.replace('+', " ")))
            .unwrap_or_default()
    };
    let group = value("group");
    (!group.is_empty() && groups::exists(c, &group) && value("token") == token(c, &group)).then_some(group)
}

/// The file a listening page says it is (`page=` is its address's path), for the inspector.
fn page_file(query: &str, group: &str) -> String {
    let path = query
        .split('&')
        .filter_map(|p| p.split_once('='))
        .find(|(k, _)| *k == "page")
        .map(|(_, v)| crate::donations::urlencoding_decode(&v.replace('+', " ")))
        .unwrap_or_default();
    match path.split_once(&format!("/overlays/{group}/")) {
        Some((_, file)) => file.to_string(),
        None => path.rsplit('/').next().unwrap_or_default().to_string(),
    }
}

pub fn permitted(c: &Config, mut subs: Subs, query: &str) -> Subs {
    let caller = caller(c, query);
    let known = caller.is_some();
    let group = caller.unwrap_or_default();
    let may = |p: &str| known && allowed(c, &group, p).is_ok();
    if let Some(chat) = subs.get_mut("Chat") {
        let (lines, alerts) = (may("chat.read"), may("alerts.read"));
        chat.retain(|k| match k.as_str() {
            "Line" | "Removed" => lines,
            "Alert" => alerts,
            _ => false,
        });
    }
    if let Some(settings) = subs.get_mut("Settings") {
        settings.retain(|g| known && *g == group);
    }
    subs.retain(|_, kinds| !kinds.is_empty());
    // What the Overlays page's Test sends reaches the pages of that overlay alone, and so does the
    // word that one of its files was saved.
    if known && !subs.is_empty() {
        subs.insert("Test".into(), std::iter::once(group.clone()).collect());
        subs.insert("Reload".into(), std::iter::once(group).collect());
    }
    subs
}

/// Stops a page's listening however its feed ends, also when the server is stopped mid-way.
struct Listening<'a>(&'a Engine, u64, tokio::sync::mpsc::UnboundedSender<Arc<str>>);

impl Drop for Listening<'_> {
    fn drop(&mut self) {
        self.0.listen(self.1, &Subs::new(), &self.2);
        self.0.spotify_local_idle();
    }
}

/// A page's live feed: what it starts with (the current value of each topic it asked for, marked
/// `replay` so a page that only acts on a change tells it apart), then every event as it happens.
struct Feed<'a> {
    first: Vec<String>,
    rx: tokio::sync::mpsc::UnboundedReceiver<Arc<str>>,
    open: Option<Open>,
    _listening: Listening<'a>,
}

fn feed<'a>(e: &'a Rc<Engine>, query: &str) -> Feed<'a> {
    let subs = permitted(&e.config(), subscriptions(query), query);
    let open = caller(&e.config(), query).map(|group| {
        let file = page_file(query, &group);
        Open::new(group, file, &subs)
    });
    if let Some(open) = &open {
        watch_folder(e, &open.0);
    }
    let wanted = subs.get("Overlay").cloned().unwrap_or_default();
    let topics = if wanted.contains("*") {
        e.overlay_topics()
    } else {
        wanted.into_iter().collect()
    };
    let first = topics
        .into_iter()
        .filter_map(|t| {
            let v = e.overlay_value(&t)?;
            Some(
                json!({"replay": true, "timeStamp": crate::engine::globals::iso_now(),
                    "event": {"source": "Overlay", "type": t}, "data": v})
                .to_string(),
            )
        })
        .collect();
    let id = NEXT.fetch_add(1, Ordering::Relaxed);
    let (tx, rx) = tokio::sync::mpsc::unbounded_channel::<Arc<str>>();
    e.listen(id, &subs, &tx);
    // A page that shows the song has the player watched for as long as it is open.
    if e.listening("Overlay", "song") || e.listening("Overlay", "songrequests.queue") {
        e.spotify_local_start();
    }
    Feed {
        first,
        rx,
        open,
        _listening: Listening(e, id, tx),
    }
}

/// The live feed as server-sent events (`GET /overlays/events`), until the page closes. Pages use
/// the WebSocket (`events_ws`): a browser keeps at most six of these open to one address, so the
/// seventh page would wait forever. This one stays for tools that read a plain stream.
pub async fn events(e: &Rc<Engine>, mut tcp: TcpStream, query: &str) -> std::io::Result<()> {
    let mut feed = feed(e, query);
    tcp.write_all(
        b"HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nCache-Control: no-store\r\nConnection: close\r\n\
          Access-Control-Allow-Origin: *\r\n\r\n",
    )
    .await?;
    for m in std::mem::take(&mut feed.first) {
        tcp.write_all(format!("data: {m}\n\n").as_bytes()).await?;
    }
    let mut buf = [0u8; 256];
    loop {
        tokio::select! {
            m = feed.rx.recv() => {
                let Some(m) = m else { break Ok(()) };
                if let Some(open) = &feed.open { sent(&open.0, &m); }
                if let Err(err) = tcp.write_all(format!("data: {m}\n\n").as_bytes()).await { break Err(err); }
            }
            n = tcp.read(&mut buf) => {
                if !matches!(n, Ok(x) if x > 0) { break Ok(()) }
            }
        }
    }
}

/// The live feed over a WebSocket on the HTTP port (the same address with `Upgrade: websocket`): one
/// text message of JSON per event. Unlike server-sent events, a browser sets no small limit on how
/// many pages hold one, so every source in OBS and every tab stays live.
pub async fn events_ws(e: &Rc<Engine>, mut tcp: TcpStream, query: &str, key: &str) -> std::io::Result<()> {
    use futures_util::{SinkExt, StreamExt};
    use tokio_tungstenite::tungstenite::{handshake::derive_accept_key, protocol::Role, Message};
    let accept = derive_accept_key(key.as_bytes());
    tcp.write_all(
        format!(
            "HTTP/1.1 101 Switching Protocols\r\nUpgrade: websocket\r\nConnection: Upgrade\r\n\
             Sec-WebSocket-Accept: {accept}\r\n\r\n"
        )
        .as_bytes(),
    )
    .await?;
    let mut ws = tokio_tungstenite::WebSocketStream::from_raw_socket(tcp, Role::Server, None).await;
    let mut feed = feed(e, query);
    let gone = |_| std::io::Error::from(std::io::ErrorKind::BrokenPipe);
    for m in std::mem::take(&mut feed.first) {
        ws.send(Message::text(m)).await.map_err(gone)?;
    }
    loop {
        tokio::select! {
            m = feed.rx.recv() => {
                let Some(m) = m else { break Ok(()) };
                if let Some(open) = &feed.open { sent(&open.0, &m); }
                ws.send(Message::text(m.to_string())).await.map_err(gone)?;
            }
            // Reading also answers the browser's pings; a close or an error ends the feed.
            m = ws.next() => {
                if !matches!(m, Some(Ok(m)) if !m.is_close()) { break Ok(()) }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_script_goes_into_the_head_with_the_settings() {
        let out = page(
            "<html><head><title>x</title></head><body></body></html>",
            &json!({"a": "</script>"}),
            "G",
            "",
            "t0k",
        );
        assert!(out.contains("\"token\":\"t0k\""));
        assert!(out.starts_with("<html><head><script>"));
        assert!(out.contains("<title>x</title>"));
        assert!(!out.contains("\"</script>\""), "a setting can't close the script");
        assert!(page("<p>hi</p>", &json!({}), "G", "http://h:1", "").starts_with("<script>"));
        let header = page("<!DOCTYPE html><body><header>x</header>", &json!({}), "G", "", "");
        assert!(header.starts_with("<!DOCTYPE html><script>"), "{header}");
        assert!(page("<html><HEAD lang=en><p>", &json!({}), "G", "", "").starts_with("<html><HEAD lang=en><script>"));
    }

    #[test]
    fn a_page_calls_back_only_with_its_token_and_what_its_group_may() {
        use crate::engine::tests::{action, engine, log, run};
        let _g = crate::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let lines = run(async {
            let log = json!([log("T spun %slice%")]);
            let mut mine = action("a1", "Spin", log.clone());
            mine["group"] = "Wheel".into();
            let e = engine(json!({"actions": [mine, action("a2", "Other", log)]}));
            e.dev_mode();
            e.change(|c| {
                groups::set_meta(
                    c,
                    "Wheel",
                    groups::Meta {
                        description: "w".into(),
                        ..Default::default()
                    },
                )
                .unwrap()
            });
            let token = token(&e.config(), "Wheel");
            assert_eq!(token.len(), 32);
            assert_ne!(token, super::token(&e.config(), "Other group"));
            let ask = |mut req: Value| {
                req["group"] = req.get("group").cloned().unwrap_or("Wheel".into());
                req["token"] = req.get("token").cloned().unwrap_or(token.clone().into());
                call(&e, req.to_string().as_bytes())
            };
            // Not a page of ours: no token, another group's token, a group that isn't there.
            assert_eq!(ask(json!({"op": "get", "name": "x", "token": ""})).0, 403);
            assert_eq!(ask(json!({"op": "get", "name": "x", "group": "Nope"})).0, 403);
            assert_eq!(call(&e, b"not json").0, 400);
            assert_eq!(ask(json!({"op": "fly"})).0, 400);
            // Nothing granted: reading and publishing work, the rest is refused with the permission named.
            for (op, needs) in [("run", "actions.run"), ("say", "chat.send"), ("set", "values.write")] {
                let (status, answer) = ask(json!({"op": op, "action": "Spin", "text": "hi", "name": "n", "value": 1}));
                assert_eq!(status, 403, "{op}");
                assert!(answer["error"].as_str().unwrap().contains(needs), "{answer}");
            }
            assert_eq!(
                ask(json!({"op": "get", "name": "n"})).1,
                json!({"ok": true, "value": null})
            );
            assert_eq!(
                ask(json!({"op": "publish", "topic": "score", "value": {"n": 2}})).0,
                200
            );
            assert_eq!(e.overlay_value("score"), Some(json!({"n": 2})));
            assert_eq!(ask(json!({"op": "publish", "topic": ""})).0, 400);
            assert_eq!(ask(json!({"op": "log", "text": "T page line"})).0, 200);
            // Granted: its own actions only, chat, and values actions read as %group.name%.
            e.change(|c| {
                let mut m = groups::meta(c, "Wheel");
                m.permissions = PERMISSIONS.iter().map(|(p, _)| p.to_string()).collect();
                groups::set_meta(c, "Wheel", m).unwrap();
            });
            assert_eq!(
                ask(json!({"op": "run", "action": "Spin", "args": {"slice": "red"}})).0,
                200
            );
            assert_eq!(ask(json!({"op": "run", "action": "a1"})).0, 200);
            let (status, answer) = ask(json!({"op": "run", "action": "Other"}));
            assert!(
                status == 403 && answer["error"].as_str().unwrap().contains("no action"),
                "{answer}"
            );
            assert_eq!(ask(json!({"op": "set", "name": "wins", "value": 3})).0, 200);
            assert_eq!(ask(json!({"op": "set", "name": "no good", "value": 3})).0, 403);
            assert_eq!(ask(json!({"op": "get", "name": "wins"})).1["value"], 3);
            assert_eq!(groups::resolve(&e.config(), "Wheel", "%group.wins%"), "3");
            assert_eq!(ask(json!({"op": "say", "text": " "})).0, 403);
            assert_eq!(ask(json!({"op": "say", "text": "hi", "platform": "myspace"})).0, 403);
            assert_eq!(
                ask(json!({"op": "say", "text": "hello chat", "platform": "twitch"})).0,
                200
            );
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
            let out = e.dev_take().unwrap_or_default();
            assert!(out.iter().any(|l| l.to_string().contains("hello chat")), "{out:?}");
        });
        assert_eq!(lines, ["T spun red", "T spun %slice%"]);
    }

    #[test]
    fn a_page_reads_the_chat_alerts_song_and_settings_only_as_its_group_may() {
        use crate::engine::tests::{engine, run};
        let _g = crate::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        run(async {
            let e = engine(json!({"actions": []}));
            let grant = |group: &str, permissions: &[&str]| {
                e.change(|c| {
                    groups::create(c, group);
                    let m = groups::Meta {
                        description: "d".into(),
                        permissions: permissions.iter().map(|p| p.to_string()).collect(),
                        ..Default::default()
                    };
                    groups::set_meta(c, group, m).unwrap();
                });
                format!("group={group}&token={}", token(&e.config(), group))
            };
            let wanted = "topics=song,viewers&events=Chat.Line,Chat.Alert,Chat.Removed,Chat.*,Settings.Reader,Settings.Other,Twitch.Follow";
            let subs = |who: &str| permitted(&e.config(), subscriptions(wanted), &format!("{wanted}&{who}"));
            // No token: what was open before stays open, the rest is dropped.
            let s = subs("group=Reader&token=wrong");
            assert!(!s.contains_key("Chat") && !s.contains_key("Settings"), "{s:?}");
            assert!(
                s["Overlay"].contains("viewers") && s["Overlay"].contains("song"),
                "topics are open"
            );
            assert!(s["Twitch"].contains("Follow"));
            let s = subs(&grant("Reader", &["chat.read"]));
            assert!(s["Chat"].contains("Line") && s["Chat"].contains("Removed") && !s["Chat"].contains("Alert"));
            assert!(!s["Chat"].contains("*"));
            assert_eq!(
                s["Settings"].iter().collect::<Vec<_>>(),
                ["Reader"],
                "its own settings only"
            );
            let s = subs(&grant("Alerts", &["alerts.read"]));
            assert_eq!(s["Chat"].iter().collect::<Vec<_>>(), ["Alert"]);

            // What a listening page is sent: a chat line, an event, a deleted message, a changed setting.
            let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
            let all = "events=Chat.Line,Chat.Alert,Chat.Removed,Settings.Reader";
            e.change(|c| {
                let mut m = groups::meta(c, "Reader");
                m.permissions.push("alerts.read".into());
                groups::set_meta(c, "Reader", m).unwrap();
            });
            let who = format!("group=Reader&token={}", token(&e.config(), "Reader"));
            e.listen(
                9,
                &permitted(&e.config(), subscriptions(all), &format!("{all}&{who}")),
                &tx,
            );
            let mut next = move || serde_json::from_str::<Value>(&rx.try_recv().expect("an event")).unwrap();
            e.inject_chat("twitch", "ana", "", "viewer", "hello", &Value::Null)
                .unwrap();
            let m = next();
            assert_eq!(
                (m["event"]["type"].as_str(), m["data"]["text"].as_str()),
                (Some("Line"), Some("hello"))
            );
            e.chat_remove("twitch", "m1", "");
            assert_eq!(
                next()["data"],
                json!({"platform": "twitch", "msgId": "m1", "userId": ""})
            );
            e.change(|c| {
                let mut v = groups::values(c, "Reader");
                v.insert("color".into(), "#fff".into());
                groups::set_values(c, "Reader", v).unwrap();
            });
            let m = next();
            assert_eq!(
                (m["event"]["source"].as_str(), m["data"]["color"].as_str()),
                (Some("Settings"), Some("#fff"))
            );
            let (status, answer) = call(
                &e,
                json!({"op": "chat", "group": "Reader", "token": token(&e.config(), "Reader"), "count": 5})
                    .to_string()
                    .as_bytes(),
            );
            assert_eq!(status, 200);
            assert_eq!(answer["lines"][0]["text"], "hello");
            let (status, _) = call(
                &e,
                json!({"op": "chat", "group": "Alerts", "token": token(&e.config(), "Alerts"), "alerts": false})
                    .to_string()
                    .as_bytes(),
            );
            assert_eq!(status, 403, "it may read alerts, not lines");
        });
    }

    #[test]
    fn pages_are_served_the_programs_own_pictures() {
        pictures(|path| match path {
            "icons/heart.svg" => Some(std::borrow::Cow::Borrowed(b"<svg stroke=\"black\"/>")),
            "brands/twitch.svg" => Some(std::borrow::Cow::Borrowed(b"<svg/>")),
            _ => None,
        });
        let c = Config::default();
        let get = |path: &str| {
            let (status, kind, body) = file(&c, path).read();
            (status, kind, String::from_utf8(body).unwrap())
        };
        // An icon is white unless the address names a colour.
        let (status, kind, body) = get("/overlays/-/icons/heart.svg");
        assert_eq!((status, kind), (200, "image/svg+xml"));
        assert!(body.contains("stroke=\"#ffffff\""), "{body}");
        assert!(get("/overlays/-/icons/ff5c8a/heart.svg")
            .2
            .contains("stroke=\"#ff5c8a\""));
        assert_eq!(get("/overlays/-/brands/twitch.svg").0, 200);
        // Only the folders of pictures, and only what is in them.
        for path in [
            "/overlays/-/icons/nope.svg",
            "/overlays/-/icons/zzzzzz/heart.svg",
            "/overlays/-/locales/en_US.json",
            "/overlays/-/cover",
            "/overlays/-/",
        ] {
            assert_eq!(get(path).0, 404, "{path}");
        }
    }

    #[test]
    fn a_page_that_opens_gets_the_topics_values_marked_as_old_and_asks_for_pictures_only_as_it_may() {
        use crate::engine::tests::{engine, run};
        let _g = crate::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        run(async {
            let e = engine(json!({"actions": []}));
            e.overlay_publish("score", json!({"home": 1}));
            let mut page = feed(&e, "topics=score,none");
            assert_eq!(page.first.len(), 1);
            let first: Value = serde_json::from_str(&page.first[0]).unwrap();
            assert_eq!(
                (&first["replay"], &first["event"]["type"], &first["data"]["home"]),
                (&json!(true), &json!("score"), &json!(1))
            );
            // What happens after is not marked.
            e.overlay_publish("score", json!({"home": 2}));
            let next: Value = serde_json::from_str(&page.rx.try_recv().unwrap()).unwrap();
            assert!(next.get("replay").is_none() && next["data"]["home"] == 2, "{next}");
            drop(page);
            assert!(!e.listening("Overlay", "score"), "a closed page listens no more");

            // The picture of a chatter: another op is left to `call`, and it needs a permission.
            e.change(|c| {
                groups::create(c, "Cards");
                groups::set_meta(
                    c,
                    "Cards",
                    groups::Meta {
                        description: "d".into(),
                        ..Default::default()
                    },
                )
                .unwrap();
            });
            let ask = |op: &str, platform: &str| {
                json!({"op": op, "group": "Cards", "token": token(&e.config(), "Cards"), "platform": platform, "userId": "1"})
                    .to_string()
            };
            assert!(avatar(&e, ask("get", "twitch").as_bytes()).await.is_none());
            let (status, answer) = avatar(&e, ask("avatar", "twitch").as_bytes()).await.unwrap();
            assert!(
                status == 403 && answer["error"].as_str().unwrap().contains("alerts.read"),
                "{answer}"
            );
            e.change(|c| {
                let mut m = groups::meta(c, "Cards");
                m.permissions = vec!["alerts.read".into()];
                groups::set_meta(c, "Cards", m).unwrap();
            });
            assert_eq!(
                avatar(&e, ask("avatar", "myspace").as_bytes()).await.unwrap(),
                (200, json!({"ok": true, "url": ""}))
            );
            let wrong = json!({"op": "avatar", "group": "Cards", "token": "no", "platform": "twitch", "userId": "1"});
            assert_eq!(avatar(&e, wrong.to_string().as_bytes()).await.unwrap().0, 403);
        });
    }

    #[test]
    fn a_saved_file_tells_the_groups_open_pages_to_load_again() {
        use crate::engine::tests::{engine, run};
        use crate::fswatch::Change;
        use base64::Engine as _;
        let _g = crate::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        // An editor's scratch files are not a save of the overlay.
        for (name, counts) in [
            ("index.html", true),
            ("css/look.css", true),
            (".index.html.swp", false),
            ("index.html~", false),
            ("4913", false),
            ("page.tmp", false),
        ] {
            assert_eq!(saved(&Change::Modified(name.into())), counts, "{name}");
        }
        run(async {
            let e = engine(json!({"actions": []}));
            e.change(|c| {
                groups::create(c, "Live edit");
                let meta = groups::Meta {
                    description: "d".into(),
                    files: vec![groups::File {
                        name: "index.html".into(),
                        content: base64::engine::general_purpose::STANDARD.encode("<p>one</p>"),
                        ..Default::default()
                    }],
                    ..Default::default()
                };
                groups::set_meta(c, "Live edit", meta).unwrap();
            });
            let path = groups::file_path(&e.config(), "Live edit", "index.html").expect("the file is on disk");
            let who = format!("group=Live%20edit&token={}", token(&e.config(), "Live edit"));
            let mut page = feed(&e, &format!("topics=score&{who}"));
            assert!(
                WATCHED.with_borrow(|w| w.contains_key("Live edit")),
                "its folder is watched"
            );
            // The watch needs a moment to start; then a save reaches the page once it has settled.
            tokio::time::sleep(std::time::Duration::from_millis(150)).await;
            std::fs::write(&path, "<p>two</p>").unwrap();
            let told = tokio::time::timeout(std::time::Duration::from_secs(5), page.rx.recv())
                .await
                .expect("the page is told")
                .unwrap();
            let told: Value = serde_json::from_str(&told).unwrap();
            assert_eq!(
                (&told["event"]["source"], &told["event"]["type"]),
                (&json!("Reload"), &json!("Live edit"))
            );
            // A page of another group is not told, and the watch ends with the last page.
            drop(page);
            assert!(!WATCHED.with_borrow(|w| w.contains_key("Live edit")));
            let _ = groups::set_meta(&mut crate::Config::default(), "Live edit", groups::Meta::default());
        });
    }

    #[test]
    fn the_guide_tells_of_everything_a_page_can_use() {
        let guide = crate::docs::guide("OVERLAYS").expect("docs/OVERLAYS.md is a guide");
        // Every call of `window.streamerz`: the names at the top of the object in the page script.
        let object = CLIENT.split_once("const streamerz = {").unwrap().1;
        let mut calls = Vec::new();
        for line in object.lines() {
            let Some(rest) = line.strip_prefix("    ") else {
                continue;
            };
            if rest.starts_with(' ') || rest.starts_with("//") || rest.starts_with('}') {
                continue;
            }
            let name: String = rest.chars().take_while(|c| c.is_alphanumeric()).collect();
            if !name.is_empty()
                && (rest[name.len()..].starts_with(':')
                    || rest[name.len()..].starts_with('(')
                    || rest[name.len()..].starts_with(','))
            {
                calls.push(name);
            }
        }
        for expected in [
            "settings", "on", "event", "chat", "line", "avatar", "song", "position", "say", "publish",
        ] {
            assert!(
                calls.iter().any(|c| c == expected),
                "{expected} was not read from the script: {calls:?}"
            );
        }
        for call in calls.iter().filter(|c| *c != "group" && *c != "state") {
            assert!(
                guide.contains(&format!("streamerz.{call}")),
                "the guide does not tell of streamerz.{call}"
            );
        }
        // The types file names every call too, so an editor offers them all.
        for call in &calls {
            let named = [format!("    {call}("), format!("    readonly {call}:")];
            assert!(
                named.iter().any(|n| TYPES.contains(n.as_str())),
                "streamerz.d.ts does not declare {call}"
            );
        }
        // Every permission, kind of setting, folder of pictures and example.
        for (permission, _) in PERMISSIONS {
            assert!(guide.contains(&format!("`{permission}`")), "{permission}");
        }
        for kind in groups::Kind::ALL {
            assert!(
                guide.contains(&format!("`{}`", kind.name())),
                "the kind {}",
                kind.name()
            );
        }
        for word in [
            "`section`",
            "`showIf`",
            "`min`",
            "`max`",
            "`step`",
            "`options`",
            "template.test",
        ] {
            assert!(guide.contains(word), "{word}");
        }
        for path in [
            "brands/<name>.svg",
            "badges/<platform>-<role>.svg",
            "icons/<name>.svg",
            "`cover`",
            "`guide`",
        ] {
            assert!(guide.contains(path), "{path}");
        }
        for example in crate::examples::ALL {
            let name = crate::examples::package(example.slug).unwrap().name;
            assert!(guide.contains(&format!("| {name} |")), "the example {name}");
        }
        // And the guide is served as a page that holds it.
        let (status, kind, page) = file(&Config::default(), "/overlays/-/guide").read();
        let page = String::from_utf8(page).unwrap();
        assert_eq!((status, kind), (200, "text/html; charset=utf-8"));
        assert!(page.contains("Work on a page") && !page.contains("__GUIDE__"));
    }

    #[test]
    fn unknown_permissions_are_refused() {
        let m = groups::Meta {
            permissions: vec!["chat.send".into(), "everything".into()],
            ..Default::default()
        };
        assert_eq!(m.validate(), Err("unknown permission: everything".into()));
    }

    #[test]
    fn pages_ask_for_topics_and_events() {
        let s = subscriptions("topics=viewers,ad&events=Spotify.TrackChanged%2CTwitch.*");
        assert!(s["Overlay"].contains("viewers") && s["Overlay"].contains("ad"));
        assert!(s["Spotify"].contains("TrackChanged") && s["Twitch"].contains("*"));
        assert!(subscriptions("topics=&events=").is_empty());
    }

    #[test]
    fn names_are_encoded_in_urls() {
        assert_eq!(encode("My group/1.html"), "My%20group%2F1.html");
        assert_eq!(
            url_at("http://h:1", "My/group", "img/a b.png"),
            "http://h:1/overlays/My%2Fgroup/img/a%20b.png"
        );
    }

    #[test]
    fn ranges_of_a_file() {
        assert_eq!(range("bytes=0-99", 1000), Some(Ok((0, 99))));
        assert_eq!(range("bytes=900-", 1000), Some(Ok((900, 999))));
        assert_eq!(range("bytes=-100", 1000), Some(Ok((900, 999))));
        assert_eq!(range("bytes=-5000", 1000), Some(Ok((0, 999))));
        assert_eq!(range("bytes=10-5000", 1000), Some(Ok((10, 999))));
        assert_eq!(range("bytes=1000-", 1000), Some(Err(())));
        assert_eq!(range("bytes=5-2", 1000), Some(Err(())));
        assert_eq!(range("bytes=0-", 0), Some(Err(())));
        assert_eq!(range("bytes=0-1,5-6", 1000), None);
        assert_eq!(range("items=0-1", 1000), None);
    }
}
