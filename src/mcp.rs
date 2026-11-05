//! `streamerz --mcp`: a Model Context Protocol server on stdin/stdout, so an AI assistant can drive a
//! running streamerz: its status and log, actions, firing triggers, fake chat in and what would have
//! gone out. It talks to that instance's WebSocket server (the address in `streamerz.json` next to the
//! exe, or `STREAMERZ_URL`); the testing tools need that instance started with `STREAMERZ_DEV=1`.

use futures_util::{SinkExt, StreamExt};
use serde_json::{json, Value};
use std::io::{BufRead, Write};
use std::time::Duration;
use tokio_tungstenite::tungstenite::Message;

type Ws = tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>;

/// The running instance's WebSocket address.
fn url() -> String {
    if let Ok(u) = std::env::var("STREAMERZ_URL") {
        return u;
    }
    let c: Value = std::fs::read(crate::config_path())
        .ok()
        .and_then(|b| serde_json::from_slice(&b).ok())
        .unwrap_or_default();
    let w = &c["websockets"];
    let host = match w["address"].as_str().unwrap_or("127.0.0.1") {
        "" | "0.0.0.0" | "*" => "127.0.0.1",
        h => h,
    };
    let port = w["port"].as_u64().unwrap_or(8080);
    let path = w["endpoint"].as_str().unwrap_or("/");
    format!("ws://{host}:{port}{path}")
}

/// Streamerz's MCP server as an agent's config starts it: this exe, for the instance this one drives.
fn server() -> crate::agents::Server {
    crate::agents::Server::this(url())
}

/// (name, description, input schema properties, required)
const TOOLS: &[(&str, &str, &str, &[&str])] = &[
    ("status", "What's connected (Twitch, YouTube, Kick, TikTok, OBS, Streamlabs Desktop, StreamElements, Streamlabs), the version, and how many actions, commands and timers are set up.", "{}", &[]),
    ("log", "The last lines of streamerz's log.", r#"{"lines": {"type": "integer", "description": "How many (default 50)"}}"#, &[]),
    ("search_channels", "Channels by name on Twitch, Kick, YouTube or TikTok. TikTok needs its browser extension. Returns handle, name, followers and live state.", r#"{"platform": {"type": "string", "enum": ["twitch", "kick", "youtube", "tiktok"]}, "query": {"type": "string"}}"#, &["platform", "query"]),
    ("list_actions", "Every action: id, name, group, enabled, trigger and sub-action counts.", "{}", &[]),
    ("run_action", "Run an action by name or id, with arguments, as its Test does.", r#"{"action": {"type": "string", "description": "Name or id"}, "args": {"type": "object"}}"#, &["action"]),
    ("list_triggers", "Trigger type codes and names (e.g. 101 TwitchFollow), filtered by a word.", r#"{"filter": {"type": "string"}}"#, &[]),
    ("fire_trigger", "Fire a trigger (code or name) with arguments as its platform would: every action using it runs. Returns how many ran. Needs STREAMERZ_DEV=1.", r#"{"trigger": {"type": ["integer", "string"]}, "args": {"type": "object", "description": "e.g. {\"user\": \"ana\", \"viewers\": 10}"}}"#, &["trigger"]),
    ("inject_chat", "A fake chat message from a platform, run through the commands; returns how many ran and what streamerz would have sent back (nothing is sent). Needs STREAMERZ_DEV=1.", r#"{"platform": {"type": "string", "enum": ["twitch", "youtube", "kick", "tiktok"]}, "user": {"type": "string"}, "userId": {"type": "string", "description": "A real account's id, so its user card can be tried; the name otherwise"}, "role": {"type": "string", "enum": ["viewer", "subscriber", "vip", "moderator", "broadcaster"]}, "text": {"type": "string"}, "badges": {"type": "array", "description": "As Streamer.bot's badges: [{\"name\": \"broadcaster\", \"version\": 1, \"imageUrl\": \"https://...\"}]"}, "args": {"type": "object", "description": "More of what the message carries, as the trigger's arguments (messageType, messageRewardId, messageRewardTitle, messageRewardCost, messageRewardImage...)"}}"#, &["platform", "text"]),
    ("chat_out", "The chat messages streamerz would have sent since the last call (dev mode keeps them instead of sending). Needs STREAMERZ_DEV=1.", "{}", &[]),
    ("list_subactions", "Sub-action type codes and names (e.g. 10 TwitchSendMessage), filtered by a word.", r#"{"filter": {"type": "string"}}"#, &[]),
    ("describe", "What a sub-action or trigger type does and its fields (JSON key, label, what it holds), plus a new one ready to fill in. Use before writing one.", r#"{"kind": {"type": "string", "enum": ["subaction", "trigger"]}, "code": {"type": "integer"}}"#, &["kind", "code"]),
    ("get", "What's set up: all actions, commands, timers, user groups or queues, or one by id or name, as stored (Streamer.bot's shapes).", r#"{"what": {"type": "string", "enum": ["action", "command", "timer", "group", "queue"]}, "id": {"type": "string"}, "name": {"type": "string"}}"#, &["what"]),
    ("save", "Add or replace an action, command, timer, user group or queue (same id replaces; no id adds one). For an action: {name, group, enabled, triggers: [...], subActions: [...]}, items from describe. Applies at once.", r#"{"what": {"type": "string", "enum": ["action", "command", "timer", "group", "queue"]}, "item": {"type": "object"}}"#, &["what", "item"]),
    ("delete", "Delete an action, command, timer, user group or queue by id.", r#"{"what": {"type": "string", "enum": ["action", "command", "timer", "group", "queue"]}, "id": {"type": "string"}}"#, &["what", "id"]),
    ("new_action", "A new, empty action (name, group) ready to fill with triggers and sub-actions from describe, then save.", r#"{"name": {"type": "string"}, "group": {"type": "string"}}"#, &["name"]),
    ("manage_action_group", "Create, rename, or delete an action group. Deleting ungroups its actions and removes its variables and icon.", r#"{"operation": {"type": "string", "enum": ["create", "rename", "delete"]}, "group": {"type": "string"}, "name": {"type": "string"}}"#, &["operation", "group"]),
    ("list_action_groups", "List action groups.", "{}", &[]),
    ("action_group_icon", "Read, set, or remove a group icon. Set icon to null to remove it; icons contain format, sha256 and base64 data.", r#"{"group": {"type": "string"}, "icon": {"type": ["object", "null"]}}"#, &["group"]),
    ("export_actions", "Export selected action IDs with group variables and icons.", r#"{"ids": {"type": "array", "items": {"type": "string"}}, "meta": {"type": "object"}}"#, &["ids"]),
    ("import_actions", "Import an export object; confirm programs before setting allowPrograms.", r#"{"export": {"type": "object"}, "allowPrograms": {"type": "boolean"}}"#, &["export"]),
    ("group_meta", "A group's description, declared settings (name, label, kind text|integer|number|boolean|duration, default; a duration is milliseconds), files (name, base64 content) and the commands and timers it lists.", r#"{"group": {"type": "string"}}"#, &["group"]),
    ("set_group_meta", "Replace a group's description, settings, files and listed command and timer ids. New settings get their default value. Actions read a setting as %group.name%.", r#"{"group": {"type": "string"}, "meta": {"type": "object"}}"#, &["group", "meta"]),
    ("export_group", "Export a whole group (actions, commands, timers, settings, files, icon) as an import string for others to import.", r#"{"group": {"type": "string"}, "meta": {"type": "object"}}"#, &["group"]),
    ("save_group_files", "Write a group's files into an existing folder, never replacing a file.", r#"{"group": {"type": "string"}, "folder": {"type": "string"}}"#, &["group", "folder"]),
    ("list_templates", "The ready-made templates (like the subathon timer) you can add.", "{}", &[]),
    ("add_template", "Add a template by name: its actions, commands, timers and group settings.", r#"{"name": {"type": "string"}}"#, &["name"]),
    ("group_variables", "Read persistent action group variables.", r#"{"group": {"type": "string"}}"#, &["group"]),
    ("set_group_variables", "Replace persistent variables for an action group.", r#"{"group": {"type": "string"}, "values": {"type": "object"}}"#, &["group", "values"]),
    ("song_request_template", "The Spotify song request template as data: its actions, commands and group settings. add_template installs it.", "{}", &[]),
    ("connectors", "Each connector behind a port (OBS connections and whether they are connected, Spotify enabled and signed in), without secrets.", "{}", &[]),
    ("obs_request", "A raw obs-websocket request through the OBS port, e.g. GetSceneList. Acts on the real OBS. Needs STREAMERZ_DEV=1.", r#"{"requestType": {"type": "string"}, "requestData": {"type": "object"}, "connectionId": {"type": "string", "description": "Default: the first connected"}}"#, &["requestType"]),
    ("spotify_current", "Read the current Spotify track. Requires Spotify to be enabled and signed in.", r#"{}"#, &[]),
    ("spotify_local", "Read Spotify's local Windows media session without an API login. Local information cannot control the Spotify API queue.", r#"{}"#, &[]),
    ("overlays", "The overlays (HTML files in action groups) with their URLs for an OBS browser source, and the topics that have a value. An overlay page reads its group's settings from streamerz.settings, a topic with streamerz.on(topic, fn) (built in: viewers, ad) and any event with streamerz.event('Source.Type', fn).", "{}", &[]),
    ("overlay_publish", "Set an overlay topic: the pages listening show it at once. The value is any JSON. The Overlay sub-action (type 99880, fields topic and value) does the same from an action.", r#"{"topic": {"type": "string"}, "value": {}}"#, &["topic"]),
    ("tiktok_bridge", "Sending to TikTok LIVE chat through the user's browser (the Streamerz for TikTok extension): status (set up, connected, the browser's TikTok account, the extension folder to load unpacked), install (sets it up and writes the extension) or remove. Messages then go out with the chat tools and the TikTok Send Message sub-action (43001).", r#"{"action": {"type": "string", "enum": ["status", "install", "login", "remove"]}}"#, &[]),
    ("countdown", "Read a template countdown by its clock ID.", r#"{"clockId": {"type": "string"}}"#, &["clockId"]),
    ("spotify_find", "The track a song name or Spotify link would be: what a viewer's !sr would queue. Adds nothing.", r#"{"query": {"type": "string"}}"#, &["query"]),
    ("spotify_player", "What Spotify plays and has queued, and the song requests (who asked, and whether each is with Spotify or still waiting).", "{}", &[]),
    ("spotify_queue", "Add a Spotify track link, URI or song name to the playback queue.", r#"{"query": {"type": "string"}}"#, &["query"]),
    ("spotify_playback", "Control playback on Spotify's active device.", r#"{"command": {"type": "string", "enum": ["play", "pause", "next", "previous"]}}"#, &["command"]),
    ("list_plugins", "The WebAssembly plugins in the plugins folder: on or off, and for those on their manifest (sub-actions, triggers, settings). Plugin sub-actions are type 99800 with plugin and name; plugin triggers type 99801 with plugin and event. See PLUGINS.md.", "{}", &[]),
    ("plugin_enable", "Turn a plugin on or off.", r#"{"plugin": {"type": "string", "description": "Its id (the .wasm file's name)"}, "on": {"type": "boolean"}}"#, &["plugin"]),
    ("ui", "Look at the window (needs STREAMERZ_DEV=1): `shot /tmp/a.png [n]` saves window n (0 = the top one) as a PNG you can read, `windows`, `tree [n]` lists every widget with its place, `find TEXT [n]` gives screen coordinates to click (xdotool), `page Settings` goes to a page, `chatview Twitch` selects a chat view, `form CODE` opens a sub-action form. Take a shot after every UI change you make.", r#"{"do": {"type": "string"}}"#, &["do"]),
    ("docs", "Streamerz's own reference page for a sub-action or trigger: what it does, its fields, the arguments it reads and sets, what login or role it needs, and examples. Read it before using one.", r#"{"kind": {"type": "string", "enum": ["subaction", "trigger"]}, "code": {"type": "integer"}}"#, &["kind", "code"]),
    ("docs_search", "Search the reference (every trigger and sub-action) by words, e.g. \"obs scene\" or \"first words\": kind, code, title and the first line of each match.", r#"{"query": {"type": "string"}, "limit": {"type": "integer", "description": "Default 20"}}"#, &["query"]),
    ("guide", "Streamerz's guides: with no name, their list (README, chat, plugins, commands, integrations, the reference's index...); with one, its text.", r#"{"name": {"type": "string"}}"#, &[]),
    ("run_scenario", "Test actions with no running app: a scenario (actions, optional config with commands or globals, steps that fire triggers, send chat or run actions, and what must come of it: chat sent, log lines, globals) runs on a fresh engine here, nothing sent anywhere. Returns what failed. Format: guide scenarios, or tests/scenarios in the repo.", r#"{"scenario": {"type": "object", "description": "{name, actions: [...], config: {...}, steps: [{fire: 101, args: {...}, ran: 1}, {chat: {platform, user, text, role}}, {run: \"name\", args: {...}}, {wait: ms}], expect: {chat: [...], log: [...], notLog: [...], globals: {...}}}"}}"#, &["scenario"]),
    ("agents", "The AI agents installed on this computer (Claude Code, Codex, OpenCode, Gemini CLI, Antigravity, Cursor...): id, name, whether Streamerz's MCP server is in its config (ready, added, outdated), its config file and whether it has a terminal command.", "{}", &[]),
    ("agent_setup", "Add Streamerz's MCP server to an installed agent's config (or remove it with on: false), as the MCP page does. Everything else in the file is kept.", r#"{"agent": {"type": "string", "description": "Its id, from agents"}, "on": {"type": "boolean", "description": "Default true"}}"#, &["agent"]),
    ("request", "Any Streamer.bot WebSocket request, as JSON (e.g. {\"request\": \"GetGlobals\"}).", r#"{"json": {"type": "object"}}"#, &["json"]),
];

fn tools() -> Value {
    let list: Vec<Value> = TOOLS
        .iter()
        .map(|(name, about, props, required)| {
            json!({"name": name, "description": about, "inputSchema": {"type": "object",
                "properties": serde_json::from_str::<Value>(props).unwrap_or_default(), "required": required}})
        })
        .collect();
    json!({ "tools": list })
}

struct Client {
    ws: Option<Ws>,
    next: u64,
}

impl Client {
    /// One request to the running instance, its answer.
    async fn ask(&mut self, mut req: Value) -> Result<Value, String> {
        if self.ws.is_none() {
            let u = url();
            let (ws, _) = crate::http::ws_connect(u.as_str()).await.map_err(|e| {
                format!("streamerz isn't running at {u} ({e}); start it, with STREAMERZ_DEV=1 for the testing tools")
            })?;
            self.ws = Some(ws);
        }
        self.next += 1;
        let id = format!("mcp{}", self.next);
        req["id"] = id.clone().into();
        let ws = self.ws.as_mut().ok_or("no connection")?;
        let sent = ws.send(Message::Text(req.to_string().into())).await;
        if sent.is_err() {
            self.ws = None;
            return Err("the connection to streamerz dropped; try again".into());
        }
        let wait = async {
            while let Some(Ok(m)) = ws.next().await {
                if let Message::Text(t) = m {
                    let v: Value = serde_json::from_str(&t).unwrap_or_default();
                    if v["id"] == id.as_str() {
                        return Some(v);
                    }
                }
            }
            None
        };
        let seconds = if req["request"] == "StreamerzSearchChannels" {
            60
        } else {
            10
        };
        match tokio::time::timeout(Duration::from_secs(seconds), wait).await {
            Ok(Some(v)) if v["status"] == "error" => Err(v["error"].as_str().unwrap_or("error").to_string()),
            // Unknown requests are answered with just their id.
            Ok(Some(v)) if v.get("status").is_none() => Err(format!(
                "streamerz doesn't know {} (the testing ones need it started with STREAMERZ_DEV=1)",
                req["request"]
            )),
            Ok(Some(v)) => Ok(v),
            Ok(None) => {
                self.ws = None;
                Err("streamerz closed the connection (WebSocket authentication on? turn it off for testing)".into())
            }
            Err(_) => Err("no answer in 10 s (a dev-only request without STREAMERZ_DEV=1?)".into()),
        }
    }

    async fn call(&mut self, name: &str, a: &Value) -> Result<Value, String> {
        let s = |k: &str| a[k].as_str().unwrap_or("").to_string();
        match name {
            "status" => self.ask(json!({"request": "GetStatus"})).await,
            "log" => {
                self.ask(json!({"request": "GetLog", "lines": a["lines"].as_u64().unwrap_or(50)}))
                    .await
            }
            "search_channels" => {
                self.ask(json!({"request": "StreamerzSearchChannels", "platform": s("platform"), "query": s("query")})).await
            }
            "list_actions" => self.ask(json!({"request": "GetActions"})).await,
            "run_action" => {
                let key = if s("action").len() == 36 && s("action").contains('-') {
                    "id"
                } else {
                    "name"
                };
                self.ask(json!({"request": "DoAction", "action": {key: s("action")}, "args": a["args"]}))
                    .await
            }
            "list_triggers" => {
                let f = s("filter").to_lowercase();
                let list: Vec<Value> = crate::types::TRIGGERS
                    .iter()
                    .filter(|(_, n)| n.to_lowercase().contains(&f))
                    .map(|(c, n)| json!([c, n]))
                    .collect();
                Ok(json!({ "triggers": list }))
            }
            "fire_trigger" => {
                let r = self
                    .ask(json!({"request": "FireTrigger", "trigger": a["trigger"], "args": a["args"]}))
                    .await?;
                // What the actions logged and sent, once they've had a moment to run.
                tokio::time::sleep(Duration::from_millis(300)).await;
                let out = self.ask(json!({"request": "GetChatOut"})).await?;
                Ok(json!({"fired": r, "chatOut": out["messages"]}))
            }
            "inject_chat" => {
                let user = if s("user").is_empty() {
                    "tester".into()
                } else {
                    s("user")
                };
                let role = if s("role").is_empty() {
                    "viewer".into()
                } else {
                    s("role")
                };
                let r = self
                    .ask(json!({"request": "InjectChat", "platform": s("platform"), "user": user, "userId": s("userId"), "role": role, "text": s("text"), "badges": a["badges"], "args": a["args"]}))
                    .await?;
                tokio::time::sleep(Duration::from_millis(300)).await;
                let out = self.ask(json!({"request": "GetChatOut"})).await?;
                Ok(json!({"ran": r["ran"], "chatOut": out["messages"]}))
            }
            "chat_out" => self.ask(json!({"request": "GetChatOut"})).await,
            "list_subactions" => {
                let f = s("filter").to_lowercase();
                let list: Vec<Value> = crate::types::SUB_ACTIONS
                    .iter()
                    .filter(|(_, n)| n.to_lowercase().contains(&f))
                    .map(|(c, n)| json!([c, n]))
                    .collect();
                Ok(json!({ "subActions": list }))
            }
            "describe" => {
                let code = a["code"].as_u64().ok_or("code: a number")?;
                let sub = s("kind") == "subaction";
                let table = if sub {
                    crate::types::SUB_ACTIONS
                } else {
                    crate::types::TRIGGERS
                };
                let name = crate::types::name(table, code as u32).ok_or(format!("no {} {code}", s("kind")))?;
                use crate::ui::{forms, picker};
                let cat = if sub {
                    String::new()
                } else {
                    picker::trigger_categorize(name)
                };
                let (about, fields, new) = if sub {
                    (
                        picker::sub_action_description(code as u32, name, &cat),
                        forms::sub_fields(code),
                        Value::Object(crate::edit::new_sub(code)),
                    )
                } else {
                    let title = picker::trigger_clean_title(name);
                    (
                        picker::trigger_description(code as u32, name, &title, &cat),
                        forms::trigger_fields(code),
                        Value::Object(crate::edit::new_trigger(code as u32)),
                    )
                };
                let fields: Vec<Value> = fields
                    .unwrap_or(&[])
                    .iter()
                    .filter(|f| !f.0.is_empty())
                    .map(|(k, l, w)| json!({"key": k, "label": l, "holds": w.about()}))
                    .collect();
                let kind = if sub { "subaction" } else { "trigger" };
                let docs = crate::docs::entry(kind, code as u32).map(|e| e.text).unwrap_or_default();
                Ok(
                    json!({"code": code, "name": name, "about": about, "fields": fields, "new": new, "docs": docs,
                    "note": if fields.is_empty() { "No form: fields as Streamer.bot stores them." } else { "" }}),
                )
            }
            "run_scenario" => {
                let sc = a["scenario"].clone();
                // Its own engine and runtime, off this one.
                let o = std::thread::spawn(move || {
                    crate::set_quiet(true);
                    crate::scenario::run(&sc)
                })
                .join()
                .map_err(|_| "the scenario crashed".to_string())?;
                Ok(json!({"name": o.name, "passed": o.failures.is_empty(), "failures": o.failures}))
            }
            "docs" => {
                let code = a["code"].as_u64().ok_or("code: a number")? as u32;
                crate::docs::entry(&s("kind"), code)
                    .map(|e| json!({"title": e.title, "file": e.file, "text": e.text}))
                    .ok_or_else(|| format!("no page for {} {code}", s("kind")))
            }
            "docs_search" => {
                let limit = a["limit"].as_u64().unwrap_or(20) as usize;
                let found: Vec<Value> = crate::docs::search(&s("query"))
                    .into_iter()
                    .take(limit)
                    .map(|e| {
                        let first = e.text.lines().skip(1).find(|l| !l.trim().is_empty() && !l.starts_with("- "));
                        json!({"kind": e.kind, "code": e.code, "title": e.title, "about": first.unwrap_or("")})
                    })
                    .collect();
                Ok(json!({ "found": found }))
            }
            "agents" => {
                let list: Vec<Value> = crate::agents::found(&crate::agents::Places::here(), &server())
                    .iter()
                    .map(|f| {
                        json!({"id": f.kind.id, "name": f.kind.name, "state": format!("{:?}", f.state).to_lowercase(),
                            "config": f.file.display().to_string(), "terminal": f.cli.is_some()})
                    })
                    .collect();
                Ok(json!({ "agents": list }))
            }
            "agent_setup" => {
                let kind = crate::agents::KINDS
                    .iter()
                    .find(|k| k.id == s("agent"))
                    .ok_or_else(|| format!("no agent {}; see agents", s("agent")))?;
                let on = a["on"].as_bool().unwrap_or(true);
                crate::agents::set(kind, &crate::agents::Places::here(), &server(), on)?;
                Ok(json!({ "ok": true }))
            }
            "guide" => match s("name").trim() {
                "" => Ok(json!({ "guides": crate::docs::guides() })),
                n => crate::docs::guide(n)
                    .map(|t| json!({ "text": t }))
                    .ok_or_else(|| format!("no guide {n}; call guide with no name for the list")),
            },
            "get" => {
                self.ask(json!({"request": "StreamerzGet", "what": s("what"), "id": s("id"), "name": s("name")}))
                    .await
            }
            "manage_action_group" => self.ask(json!({"request": "StreamerzManageActionGroup", "operation": s("operation"), "group": s("group"), "name": s("name")})).await,
            "list_action_groups" => self.ask(json!({"request": "StreamerzActionGroups"})).await,
            "action_group_icon" => {
                let mut req = json!({"request": "StreamerzGroupIcon", "group": s("group")});
                if let Some(icon) = a.get("icon") { req["icon"] = icon.clone(); }
                self.ask(req).await
            }
            "export_actions" => self.ask(json!({"request": "StreamerzActionExport", "ids": a["ids"], "meta": a["meta"]})).await,
            "import_actions" => self.ask(json!({"request": "StreamerzActionImport", "export": a["export"], "allowPrograms": a["allowPrograms"]})).await,
            "group_variables" => self.ask(json!({"request": "StreamerzGroupVariables", "group": s("group")})).await,
            "set_group_variables" => self.ask(json!({"request": "StreamerzSetGroupVariables", "group": s("group"), "values": a["values"]})).await,
            "save" => {
                self.ask(json!({"request": "StreamerzSave", "what": s("what"), "item": a["item"]}))
                    .await
            }
            "delete" => {
                self.ask(json!({"request": "StreamerzDelete", "what": s("what"), "id": s("id")}))
                    .await
            }
            "list_plugins" => self.ask(json!({"request": "StreamerzPlugins"})).await,
            "plugin_enable" => {
                self.ask(json!({"request": "StreamerzPluginEnable", "plugin": s("plugin"), "on": a["on"].as_bool().unwrap_or(true)}))
                    .await
            }
            "new_action" => Ok(
                serde_json::to_value(crate::edit::new_action(&s("name"), &s("group"))).map_err(|e| e.to_string())?
            ),
            "song_request_template" => {
                let made = crate::templates::songs::made();
                Ok(json!({"actions": made.actions, "commands": made.commands,
                    "groups": made.groups.iter().map(|(name, meta)| json!({"name": name, "meta": meta})).collect::<Vec<_>>()}))
            }
            "spotify_current" => self.ask(json!({"request": "StreamerzSpotifyCurrent"})).await,
            "spotify_local" => self.ask(json!({"request": "StreamerzSpotifyLocal"})).await,
            "overlays" => self.ask(json!({"request": "StreamerzOverlays"})).await,
            "overlay_publish" => {
                self.ask(json!({"request": "StreamerzOverlayPublish", "topic": s("topic"), "value": a["value"]}))
                    .await
            }
            "countdown" => self.ask(json!({"request": "StreamerzCountdown", "clockId": s("clockId")})).await,
            "tiktok_bridge" => self.ask(json!({"request": "StreamerzTikTokBridge", "action": s("action")})).await,
            "group_meta" => self.ask(json!({"request": "StreamerzGroupMeta", "group": s("group")})).await,
            "set_group_meta" => self.ask(json!({"request": "StreamerzSetGroupMeta", "group": s("group"), "meta": a["meta"]})).await,
            "export_group" => self.ask(json!({"request": "StreamerzGroupExport", "group": s("group"), "meta": a["meta"]})).await,
            "save_group_files" => self.ask(json!({"request": "StreamerzSaveGroupFiles", "group": s("group"), "folder": s("folder")})).await,
            "list_templates" => Ok(json!({"templates": crate::templates::TEMPLATES.iter().map(|t| json!({"name": t.name, "about": t.about})).collect::<Vec<_>>()})),
            "add_template" => self.ask(json!({"request": "StreamerzTemplateAdd", "name": s("name")})).await,
            "ui" => self.ask(json!({"request": "StreamerzUi", "do": s("do")})).await,
            "request" => self.ask(a["json"].clone()).await,
            _ => Err(format!("no tool {name}")),
        }
    }
}

/// Answer one JSON-RPC message; None for notifications.
async fn handle(c: &mut Client, m: &Value) -> Option<Value> {
    let id = m.get("id")?.clone();
    let result = match m["method"].as_str().unwrap_or("") {
        "initialize" => json!({
            "protocolVersion": m["params"]["protocolVersion"].as_str().unwrap_or("2025-06-18"),
            "capabilities": {"tools": {}},
            "serverInfo": {"name": "streamerz", "version": crate::brand::VERSION},
            "instructions": "Drives and builds a running streamerz (a Streamer.bot clone). To make something: \
                list_triggers / list_subactions to find types, describe each to get its fields and a ready item, \
                new_action, fill triggers and subActions, save; then test with fire_trigger or inject_chat and read \
                chat_out and log. Start streamerz with STREAMERZ_DEV=1 so testing sends nothing to real chats."
        }),
        "ping" => json!({}),
        "tools/list" => tools(),
        "tools/call" => {
            let p = &m["params"];
            let (text, failed) = match c.call(p["name"].as_str().unwrap_or(""), &p["arguments"]).await {
                Ok(v) => (serde_json::to_string_pretty(&v).unwrap_or_default(), false),
                Err(e) => (e, true),
            };
            json!({"content": [{"type": "text", "text": text}], "isError": failed})
        }
        other => {
            return Some(json!({"jsonrpc": "2.0", "id": id,
                "error": {"code": -32601, "message": format!("no method {other}")}}))
        }
    };
    Some(json!({"jsonrpc": "2.0", "id": id, "result": result}))
}

/// Serve MCP on stdin/stdout until stdin closes.
pub fn run() {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("runtime");
    let mut c = Client { ws: None, next: 0 };
    let stdout = std::io::stdout();
    for line in std::io::stdin().lock().lines() {
        let Ok(line) = line else { break };
        let Ok(m) = serde_json::from_str::<Value>(&line) else {
            continue;
        };
        if let Some(reply) = rt.block_on(handle(&mut c, &m)) {
            let mut o = stdout.lock();
            let _ = writeln!(o, "{reply}");
            let _ = o.flush();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn song_request_template_is_reachable() {
        let rt = tokio::runtime::Builder::new_current_thread().build().unwrap();
        rt.block_on(async {
            let mut client = Client { ws: None, next: 0 };
            let result = client.call("song_request_template", &json!({})).await.unwrap();
            assert_eq!(result["commands"].as_array().unwrap().len(), 13);
            assert_eq!(result["actions"][0]["subActions"][0]["type"], 99802);
            assert_eq!(result["groups"][0]["name"], "Song requests");
            let list = client.call("list_templates", &json!({})).await.unwrap();
            assert!(list["templates"]
                .as_array()
                .unwrap()
                .iter()
                .any(|t| t["name"] == "Song requests (Spotify)"));
        });
    }

    #[test]
    fn agents_are_reachable() {
        let rt = tokio::runtime::Builder::new_current_thread().build().unwrap();
        rt.block_on(async {
            let mut client = Client { ws: None, next: 0 };
            let list = client.call("agents", &json!({})).await.unwrap();
            assert!(list["agents"].is_array());
            let e = client
                .call("agent_setup", &json!({"agent": "nothing"}))
                .await
                .unwrap_err();
            assert!(e.contains("no agent"));
        });
    }

    #[test]
    fn lists_tools_with_valid_schemas() {
        let t = tools();
        assert_eq!(t["tools"].as_array().unwrap().len(), TOOLS.len());
        for tool in t["tools"].as_array().unwrap() {
            assert!(tool["inputSchema"]["properties"].is_object(), "{}", tool["name"]);
        }
    }
}
