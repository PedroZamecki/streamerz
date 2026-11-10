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
    ("prompts", "Read pending core input, confirmation and choice prompts. Reading does not consume them; they survive closing the frontend.", "{}", &[]),
    ("answer_prompt", "Answer a pending prompt exactly once: a string for input, a boolean for confirmation, an option ID for a choice. Null cancels input or a choice.", r#"{"promptId":{"type":"integer"},"value":{}}"#, &["promptId","value"]),
    ("connection", "Connect, disconnect or refresh OBS Studio or Streamlabs Desktop. Omit itemId for all configured connections. Refresh only reads state.", r#"{"kind":{"type":"string","enum":["obs_connection","slobs_connection"]},"action":{"type":"string","enum":["connect","disconnect","refresh"]},"itemId":{"type":"string"}}"#, &["kind","action"]),
    ("core_view", "Read a core list model, including semantic cells and current connection state, without a frontend.", r#"{"domain":{"type":"string","enum":["commands","timers","files","stream_apps","servers","queues","globals","history","backups","crashes","settings","notifications","donations","groups","plugins","mcp","community","integrations","polls","user_card","accounts","tiktok_sending","actions","actions_picker"]},"itemId":{"type":"string","description":"The row id for user_card (platform|user id)"},"actionId":{"type":"string","description":"actions: the action (id, or the a:<id> row id) whose triggers and sub-actions to include"},"kind":{"type":"string","description":"actions_picker: trigger, sub or template"},"view":{"type":"object","description":"A page's view state by table kind: {\"tab\":\"quote\",\"community_user\":{\"sort\":{\"column\":\"name\",\"desc\":false},\"hidden\":[\"tiktok\"]}}"}}"#, &["domain"]),
    ("core_status", "How many of the connections that are set up are up, and each one's state.", r#"{}"#, &[]),
    ("core_save", "Write the settings to disk now.", r#"{}"#, &[]),
    ("core_action", "Run a page operation from a core_view model's actions (for example queues: pause, pause_all; history: run_again, clear; backups: backup_now, restore, delete; crashes: delete; community: card, card_group (itemId platform|user id|group id), open_profile, quote_delete, credits_reset, credits_test; polls: end, lock, resolve, cancel, refresh, started with entity_form/edit_entity kinds poll and prediction). accounts: <platform>.connect, .disconnect, .login, .logout, .bot_login, .bot_logout, .channel (twitch.login, kick.channel; logins and the channel search open prompts to answer with answer_prompt), unlock; tiktok_sending: setup:N, retry, remove, copy_path; connections: retry with itemId the connection key from core_status; Advanced settings are entity_form/edit_entity kinds account_twitch, account_youtube, account_kick, account_tiktok. itemId is the row id from the model. actions (the Actions page; core_view actions with actionId lists groups, actions and the open action's triggers and sub-actions; ids are a:<action id>, g:<group>, or the trigger or sub-action id): action_toggle, action_delete, action_run, action_move (to=group), group_delete, group_icon_clear, template_add (code=template name), and with actionId and itemId: trigger_toggle, trigger_delete, trigger_help, sub_toggle, sub_delete, sub_help, sub_up, sub_down, sub_in, sub_out, sub_move (to=target). Forms are entity_form kinds act:action_add, act:action_edit, act:action_duplicate, act:group_add, act:group_rename, act:group_icon, act:group_settings, act:import, act:export, act:trigger_add (actionId, code), act:trigger_edit, act:trigger_test, act:sub_add (actionId, code, anchor), act:sub_edit; use the kind in the reply with edit_entity.", r#"{"domain":{"type":"string","enum":["queues","history","backups","crashes","settings","groups","community","plugins","mcp","polls","accounts","tiktok_sending","connections","actions"]},"action":{"type":"string"},"itemId":{"type":"string"},"actionId":{"type":"string","description":"actions: the open action, for trigger_* and sub_* operations"},"code":{"type":"string","description":"actions: the picked template name (template_add)"},"to":{"type":"string","description":"actions: the group (action_move) or the target sub-action id (sub_move)"}}"#, &["domain", "action"]),
    ("core_setting", "Change one setting of a core_view sections page (settings, notifications) by its row key. Choices are their index, switches true/false, text a string. mcp: address, port and endpoint (the MCP page's connection).", r#"{"domain":{"type":"string","enum":["settings","notifications","mcp"]},"key":{"type":"string"},"value":{}}"#, &["domain", "key", "value"]),
    ("entity_form", "Read a core entity form, including current values and field types. Omit itemId for a new item. Action editor forms (kinds act:...) also take actionId, code (the trigger or sub-action type, from core_view actions_picker) and anchor (the sub-action to add beside); use the kind in the reply when saving.", r#"{"kind":{"type":"string"},"itemId":{"type":"string"},"actionId":{"type":"string"},"code":{"type":"string"},"anchor":{"type":"string"}}"#, &["kind"]),
    ("edit_entity", "Save changed fields or JSON through core validation. Changes use field indexes from entity_form. JSON must keep the item ID.", r#"{"kind":{"type":"string"},"itemId":{"type":"string"},"changes":{"type":"array","items":{"type":"object"}},"json":{"type":"string"}}"#, &["kind"]),
    ("entity_draft", "Validate changed fields or JSON and return an unsaved draft. This does not write settings.", r#"{"kind":{"type":"string"},"itemId":{"type":"string"},"changes":{"type":"array","items":{"type":"object"}},"json":{"type":"string"}}"#, &["kind"]),

    ("run_subaction", "Execute a sub-action directly on the core and return its resulting arguments. This performs real operations. Use describe first.", r#"{"item":{"type":"object"}}"#, &["item"]),
    ("describe_entity", "Shared fields and a default item for a configurable core entity. Works without a frontend or a running instance.", r#"{"kind":{"type":"string","enum":["command","timer","file_watcher","file_tail","websocket_client","websocket_server","obs_connection","slobs_connection","server_websocket","server_http","server_udp","server_stream_deck"]}}"#, &["kind"]),
    ("core_read", "Read a core domain without a frontend. Account credentials are omitted.", r#"{"domain":{"type":"string","enum":["actions","commands","timers","files","queues","groups","globals","community","history","chat","polls","accounts","servers","stream_apps","integrations","donations","plugins","settings","log","crashes","backups","mcp"]}}"#, &["domain"]),
    ("action_history", "Recent action runs, reported errors, failed step type and skipped steps. Completed means no error was reported, not a remote delivery receipt.", "{}", &[]),
    ("tiktok_gifts", "Read the current live's dynamic gift catalog (IDs, names, coin values, combo support and pictures). Refresh uses the paired browser account.", r#"{"refresh":{"type":"boolean"}}"#, &[]),
    ("preview_gift_trigger", "Test a TikTok gift against an exact ID, combo phase and total coin bounds. Uses the live matcher. Offline and sends nothing.", r#"{"trigger":{"type":"object"},"gift":{"type":"object"}}"#, &["trigger","gift"]),
    ("status", "What's connected (Twitch, YouTube, Kick, TikTok, OBS, Streamlabs Desktop, StreamElements, Streamlabs), the version, and how many actions, commands and timers are set up.", "{}", &[]),
    ("log", "The last lines of Streamerz's log. Needs STREAMERZ_DEV=1.", r#"{"lines": {"type": "integer", "description": "How many (default 50)", "minimum": 0}}"#, &[]),
    ("search_channels", "Channels by name on Twitch, Kick, YouTube or TikTok. TikTok needs its browser extension. Returns handle, name, followers and live state.", r#"{"platform": {"type": "string", "enum": ["twitch", "kick", "youtube", "tiktok"]}, "query": {"type": "string"}}"#, &["platform", "query"]),
    ("list_actions", "Every action: id, name, group, enabled, trigger and sub-action counts.", "{}", &[]),
    ("run_action", "Run an action by name or id, with arguments, as its Test does.", r#"{"action": {"type": "string", "description": "Name or id"}, "args": {"type": "object"}}"#, &["action"]),
    ("list_triggers", "Trigger type codes and names (e.g. 101 TwitchFollow), filtered by a word.", r#"{"filter": {"type": "string"}}"#, &[]),
    ("fire_trigger", "Fire a trigger (code or name) with arguments as its platform would: every action using it runs. Returns how many ran. Needs STREAMERZ_DEV=1.", r#"{"trigger": {"type": ["integer", "string"]}, "args": {"type": "object", "description": "e.g. {\"user\": \"ana\", \"viewers\": 10}"}}"#, &["trigger"]),
    ("inject_chat", "A fake chat message from a platform, run through the commands; returns how many ran and what streamerz would have sent back (nothing is sent). Needs STREAMERZ_DEV=1.", r#"{"platform": {"type": "string", "enum": ["twitch", "youtube", "kick", "tiktok"]}, "user": {"type": "string"}, "userId": {"type": "string", "description": "A real account's id, so its user card can be tried; the name otherwise"}, "role": {"type": "string", "enum": ["viewer", "subscriber", "vip", "moderator", "broadcaster"]}, "text": {"type": "string"}, "badges": {"type": "array", "description": "As Streamer.bot's badges: [{\"name\": \"broadcaster\", \"version\": 1, \"imageUrl\": \"https://...\"}]"}, "args": {"type": "object", "description": "More of what the message carries, as the trigger's arguments (messageType, messageRewardId, messageRewardTitle, messageRewardCost, messageRewardImage...)"}}"#, &["platform", "text"]),
    ("chat", "The Chat page's model and operations, as the app's Chat page uses them. op read: the lines newer than seq (omit seq for all) with the platforms bar; send: text to platform (or all connected) with an optional replySeq; act: delete, timeout (secs), ban (confirmed true skips the prompt), pin (secs 30 to 1800, 0 until the stream ends) or unpin on the line seq; card: a chatter's card from the line seq (lookup true asks the platform and answers pending until done; prefetch true only starts the asking); clear: empty the view; show: hide or show a platform's chat; view: what the Chat page shows (all, side, or a platform id); sender: send as broadcaster or bot; emotes: the picker tabs and sections for platform (provider tab id, query searches every tab); favorite: toggle an emote (name, url) in the favorites; images: files for picture URLs. Sending in dev mode (STREAMERZ_DEV=1) keeps the message instead of sending.", r#"{"op":{"type":"string","enum":["read","send","act","card","clear","show","view","sender","emotes","favorite","images"]},"seq":{"type":"integer"},"gen":{"type":"integer"},"have":{"type":"integer"},"platform":{"type":"string"},"text":{"type":"string"},"replySeq":{"type":"integer"},"act":{"type":"string","enum":["reply","copy","pin","unpin","delete","timeout","ban"]},"secs":{"type":"integer"},"confirmed":{"type":"boolean"},"shown":{"type":"boolean"},"view":{"type":"string"},"lookup":{"type":"boolean"},"prefetch":{"type":"boolean"},"broadcaster":{"type":"boolean"},"urls":{"type":"array","items":{"type":"string"}},"provider":{"type":"string","enum":["favorites","platform","7tv","bttv","ffz","emoji"]},"query":{"type":"string"},"name":{"type":"string"},"url":{"type":"string"}}"#, &["op"]),
    ("chat_out", "The chat messages streamerz would have sent since the last call (dev mode keeps them instead of sending). Needs STREAMERZ_DEV=1.", "{}", &[]),
    ("list_subactions", "Sub-action type codes and names (e.g. 10 TwitchSendMessage), filtered by a word.", r#"{"filter": {"type": "string"}}"#, &[]),
    ("describe", "What a sub-action or trigger type does and its fields (JSON key, label, what it holds), plus a new one ready to fill in. Use before writing one.", r#"{"kind": {"type": "string", "enum": ["subaction", "trigger"]}, "code": {"type": "integer"}}"#, &["kind", "code"]),
    ("get", "What's set up: all actions, commands, timers, user groups or queues, or one by id or name, as stored (Streamer.bot's shapes).", r#"{"what": {"type": "string", "enum": ["action", "command", "timer", "group", "queue", "file_watcher", "file_tail", "websocket_client", "websocket_server", "obs_connection", "slobs_connection"]}, "id": {"type": "string"}, "name": {"type": "string"}}"#, &["what"]),
    ("save", "Add or replace an action, command, timer, user group or queue (same id replaces; no id adds one). For an action: {name, group, enabled, triggers: [...], subActions: [...]}, items from describe. Applies at once.", r#"{"what": {"type": "string", "enum": ["action", "command", "timer", "group", "queue", "file_watcher", "file_tail", "websocket_client", "websocket_server", "obs_connection", "slobs_connection"]}, "item": {"type": "object"}}"#, &["what", "item"]),
    ("delete", "Delete an action, command, timer, user group or queue by id.", r#"{"what": {"type": "string", "enum": ["action", "command", "timer", "group", "queue", "file_watcher", "file_tail", "websocket_client", "websocket_server", "obs_connection", "slobs_connection"]}, "id": {"type": "string"}}"#, &["what", "id"]),
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
    ("spotify_current", "Read the current Spotify track, or null when unavailable. API playback needs Spotify enabled and signed in.", r#"{}"#, &[]),
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
    ("docs_search", "Search the reference (every trigger and sub-action) by words, e.g. \"obs scene\" or \"first words\": kind, code, title and the first line of each match.", r#"{"query": {"type": "string"}, "limit": {"type": "integer", "description": "Default 20", "minimum": 0}}"#, &["query"]),
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
                "properties": serde_json::from_str::<Value>(props).expect("tool schema"), "required": required, "additionalProperties": false}})
        })
        .collect();
    json!({ "tools": list })
}

/// Reject malformed tool arguments before they can reach a running instance.
fn validate_arguments(name: &str, args: &Value) -> Result<(), String> {
    let (_, _, props, required) = TOOLS
        .iter()
        .find(|t| t.0 == name)
        .ok_or_else(|| format!("no tool {name}"))?;
    let object = args.as_object().ok_or("arguments must be an object")?;
    let properties: Value = serde_json::from_str(props).expect("tool schema");
    for key in *required {
        if !object.contains_key(*key) {
            return Err(format!("missing argument {key}"));
        }
    }
    for (key, value) in object {
        let Some(schema) = properties.get(key) else {
            return Err(format!("unknown argument {key}"));
        };
        let matches_type = |kind: &str| match kind {
            "string" => value.is_string(),
            "integer" => value.is_i64() || value.is_u64(),
            "object" => value.is_object(),
            "array" => value.is_array(),
            "boolean" => value.is_boolean(),
            "null" => value.is_null(),
            _ => false,
        };
        let valid = match &schema["type"] {
            Value::String(kind) => matches_type(kind),
            Value::Array(kinds) => kinds.iter().any(|k| k.as_str().is_some_and(matches_type)),
            Value::Null => true,
            _ => false,
        };
        if !valid
            || schema["enum"]
                .as_array()
                .is_some_and(|choices| !choices.contains(value))
        {
            return Err(format!("invalid argument {key}"));
        }
        if let Some(minimum) = schema["minimum"].as_i64() {
            if value.as_i64().is_some_and(|n| n < minimum) {
                return Err(format!("{key} must be at least {minimum}"));
            }
        }
        if schema["items"]["type"] == "string" && value.as_array().is_some_and(|a| a.iter().any(|v| !v.is_string())) {
            return Err(format!("{key} must contain strings"));
        }
    }
    if name == "request" && !args["json"]["request"].is_string() {
        return Err("json.request must be a string".into());
    }
    if matches!(name, "describe" | "docs") && args["code"].as_u64().and_then(|c| u32::try_from(c).ok()).is_none() {
        return Err("code must be an unsigned 32-bit integer".into());
    }
    Ok(())
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
            let (ws, _) = tokio::time::timeout(Duration::from_secs(10), crate::http::ws_connect(u.as_str()))
                .await
                .map_err(|_| format!("connection to Streamerz at {u} timed out after 10 s"))?
                .map_err(|e| {
                    format!(
                        "streamerz isn't running at {u} ({e}); start it, with STREAMERZ_DEV=1 for the testing tools"
                    )
                })?;
            self.ws = Some(ws);
        }
        self.next += 1;
        let id = format!("mcp{}", self.next);
        req["id"] = id.clone().into();
        let ws = self.ws.as_mut().ok_or("no connection")?;
        let sent = tokio::time::timeout(Duration::from_secs(10), ws.send(Message::Text(req.to_string().into()))).await;
        if !matches!(sent, Ok(Ok(()))) {
            self.ws = None;
            return Err("the connection to streamerz dropped; try again".into());
        }
        let wait = async {
            while let Some(Ok(m)) = ws.next().await {
                if let Message::Text(t) = m {
                    let v: Value = serde_json::from_str(&t).unwrap_or_default();
                    if v.get("authentication").is_some() && v["request"] == "Hello" {
                        return Some(
                            json!({"status": "error", "error": "This instance requires WebSocket authentication. Use a separate local development instance with authentication off."}),
                        );
                    }
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
                Err("Streamerz closed the connection; check its WebSocket settings".into())
            }
            Err(_) => {
                self.ws = None;
                Err(format!(
                    "no answer in {seconds} s (a dev-only request without STREAMERZ_DEV=1?)"
                ))
            }
        }
    }

    async fn call(&mut self, name: &str, a: &Value) -> Result<Value, String> {
        validate_arguments(name, a)?;
        let s = |k: &str| a[k].as_str().unwrap_or("").to_string();
        match name {
            "action_history" => self.ask(json!({"request":"StreamerzActionHistory"})).await,
            "tiktok_gifts" => self.ask(json!({"request":"StreamerzTikTokGifts", "refresh":a["refresh"]})).await,
            "preview_gift_trigger" => crate::tiktok::gifts::preview(
                a["trigger"].as_object().ok_or("trigger must be an object")?, &a["gift"]),
            "entity_form" => self.ask(json!({"request": "StreamerzEntityForm", "kind": s("kind"), "itemId": s("itemId"), "actionId": s("actionId"), "code": s("code"), "anchor": s("anchor")})).await,
            "prompts" => self.ask(json!({"request": "StreamerzPrompts"})).await,
            "answer_prompt" => self.ask(json!({"request": "StreamerzAnswerPrompt", "promptId": a["promptId"], "value": a["value"]})).await,
            "connection" => self.ask(json!({"request": "StreamerzConnection", "kind": s("kind"), "itemId": s("itemId"), "action": s("action")})).await,
            "core_view" => self.ask(json!({"request": "StreamerzCoreView", "domain": s("domain"), "itemId": s("itemId"), "actionId": s("actionId"), "kind": s("kind"), "view": a["view"]})).await,
            "core_status" => self.ask(json!({"request": "StreamerzCoreStatus"})).await,
            "core_save" => self.ask(json!({"request": "StreamerzCoreSave"})).await,
            "core_setting" => self.ask(json!({"request": "StreamerzCoreSetting", "domain": s("domain"), "key": s("key"), "value": a["value"]})).await,
            "core_action" => self.ask(json!({"request": "StreamerzCoreAction", "domain": s("domain"), "action": s("action"), "itemId": s("itemId"), "actionId": s("actionId"), "code": s("code"), "to": s("to")})).await,
            "edit_entity" => self.ask(json!({"request": "StreamerzEditEntity", "kind": s("kind"), "itemId": s("itemId"), "changes": a["changes"], "json": a["json"]})).await,
            "entity_draft" => self.ask(json!({"request": "StreamerzEntityDraft", "kind": s("kind"), "itemId": s("itemId"), "changes": a["changes"], "json": a["json"]})).await,
            "describe_entity" => crate::automation::entities::describe(&s("kind")),
            "run_subaction" => self.ask(json!({"request": "StreamerzRunSubaction", "item": a["item"]})).await,
            "core_read" => self.ask(json!({"request": "StreamerzCoreRead", "domain": s("domain")})).await,
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
                self.ask(json!({"request": "DoAction", "action": {"id": s("action"), "name": s("action")}, "args": a["args"]}))
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
            "chat" if s("op") == "read" => {
                let mut request = json!({"request": "StreamerzCoreView", "domain": "chat"});
                for key in ["seq", "gen", "have"] {
                    request[key] = a[key].clone();
                }
                self.ask(request).await
            }
            "chat" => {
                let mut request = json!({"request": "StreamerzChat", "op": s("op")});
                for key in ["seq", "platform", "text", "replySeq", "act", "secs", "confirmed", "shown", "view", "lookup", "prefetch", "broadcaster", "urls", "provider", "query", "name", "url"] {
                    request[key] = a[key].clone();
                }
                self.ask(request).await
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
                let code = a["code"].as_u64().and_then(|c| u32::try_from(c).ok()).ok_or("code: a 32-bit number")?;
                crate::automation::describe(&s("kind"), code)
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
                self.ask(json!({"request": "StreamerzGet", "what": s("what"), "itemId": s("id"), "name": s("name")}))
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
                self.ask(json!({"request": "StreamerzDelete", "what": s("what"), "itemId": s("id")}))
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
            "connectors" => self.ask(json!({"request": "StreamerzConnectors"})).await,
            "obs_request" => self.ask(json!({"request": "StreamerzObsRequest", "requestType": s("requestType"), "requestData": a["requestData"], "connectionId": s("connectionId")})).await,
            "spotify_current" => self.ask(json!({"request": "StreamerzSpotifyCurrent"})).await,
            "spotify_local" => self.ask(json!({"request": "StreamerzSpotifyLocal"})).await,
            "spotify_find" => self.ask(json!({"request": "StreamerzSpotifyFind", "query": s("query")})).await,
            "spotify_player" => self.ask(json!({"request": "StreamerzSpotifyPlayer"})).await,
            "spotify_queue" => self.ask(json!({"request": "StreamerzSpotifyQueue", "query": s("query")})).await,
            "spotify_playback" => self.ask(json!({"request": "StreamerzSpotifyPlayback", "command": s("command")})).await,
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
            "list_templates" => Ok(json!({"templates": crate::templates::TEMPLATES.iter().map(|t| json!({"name": t.name, "title":t.title(), "about": t.description()})).collect::<Vec<_>>()})),
            "add_template" => self.ask(json!({"request": "StreamerzTemplateAdd", "name": s("name")})).await,
            "ui" => self.ask(json!({"request": "StreamerzUi", "do": s("do")})).await,
            "request" => self.ask(a["json"].clone()).await,
            _ => Err(format!("no tool {name}")),
        }
    }
}

/// Answer one JSON-RPC message; None for notifications.
async fn handle(c: &mut Client, m: &Value) -> Option<Value> {
    if !m.is_object()
        || m["jsonrpc"] != "2.0"
        || !m["method"].is_string()
        || m.get("id")
            .is_some_and(|id| !id.is_null() && !id.is_string() && !id.is_i64() && !id.is_u64())
    {
        return Some(rpc_error(Value::Null, -32600, "Invalid Request"));
    }
    let id = m.get("id")?.clone();
    if m.get("params").is_some_and(|p| !p.is_object()) {
        return Some(rpc_error(id, -32602, "params must be an object"));
    }
    let result = match m["method"].as_str().unwrap_or("") {
        "initialize" => json!({
            "protocolVersion": m["params"]["protocolVersion"].as_str().unwrap_or("2025-06-18"),
            "capabilities": {"tools": {}},
            "serverInfo": {"name": "Streamerz", "version": crate::brand::VERSION},
            "instructions": "Use guide MCP for setup, tool workflows and limitations. Local tools (guide, docs, \
                docs_search, list_triggers, list_subactions, describe, new_action, list_templates, \
                song_request_template, run_scenario) work without a running app. Build actions with describe \
                and new_action, then test offline with run_scenario before save. Other tools drive the instance \
                selected by STREAMERZ_URL. Live tests (fire_trigger, inject_chat, chat_out, status, log, ui, \
                obs_request) require STREAMERZ_DEV=1. Dev mode captures chat; OBS and Spotify controls still \
                affect real applications. Read get before save: save replaces the whole item."
        }),
        "ping" => json!({}),
        "tools/list" => tools(),
        "tools/call" => {
            let p = &m["params"];
            let empty = json!({});
            let args = p.get("arguments").unwrap_or(&empty);
            let (text, failed) = match c.call(p["name"].as_str().unwrap_or(""), args).await {
                Ok(v) => (serde_json::to_string(&v).unwrap_or_default(), false),
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

fn rpc_error(id: Value, code: i32, message: &str) -> Value {
    json!({"jsonrpc": "2.0", "id": id, "error": {"code": code, "message": message}})
}

/// Serve MCP on stdin/stdout until stdin closes.
pub fn run() {
    crate::set_quiet(true);
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("runtime");
    let mut c = Client { ws: None, next: 0 };
    let stdout = std::io::stdout();
    for line in std::io::stdin().lock().lines() {
        let Ok(line) = line else { break };
        if line.trim().is_empty() {
            continue;
        }
        let reply = match serde_json::from_str::<Value>(&line) {
            Ok(m) => rt.block_on(handle(&mut c, &m)),
            Err(_) => Some(rpc_error(Value::Null, -32700, "Parse error")),
        };
        if let Some(reply) = reply {
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
    fn protocol_errors_and_notifications_do_not_dispatch() {
        let rt = tokio::runtime::Builder::new_current_thread().build().unwrap();
        rt.block_on(async {
            let mut client = Client { ws: None, next: 0 };
            for message in [
                json!(null),
                json!([]),
                json!({"id": 1, "method": "ping"}),
                json!({"jsonrpc": "2.0", "id": {}, "method": "ping"}),
            ] {
                assert_eq!(handle(&mut client, &message).await.unwrap()["error"]["code"], -32600);
            }
            assert!(handle(
                &mut client,
                &json!({"jsonrpc": "2.0", "method": "notifications/initialized"})
            )
            .await
            .is_none());
            let reply = handle(
                &mut client,
                &json!({"jsonrpc": "2.0", "id": 7, "method": "ping", "params": []}),
            )
            .await
            .unwrap();
            assert_eq!(reply["error"]["code"], -32602);
            assert_eq!(reply["id"], 7);
            let reply = handle(
                &mut client,
                &json!({"jsonrpc": "2.0", "id": 8, "method": "tools/call", "params": {"name": "list_templates"}}),
            )
            .await
            .unwrap();
            assert_eq!(reply["result"]["isError"], false);
            let reply = handle(&mut client, &json!({"jsonrpc": "2.0", "id": 9, "method": "missing"}))
                .await
                .unwrap();
            assert_eq!(reply["error"]["code"], -32601);
            assert!(client.ws.is_none());
        });
        for name in ["log", "docs_search"] {
            let args = if name == "log" {
                json!({"lines": -1})
            } else {
                json!({"query": "test", "limit": -1})
            };
            assert!(validate_arguments(name, &args).is_err());
        }
    }

    #[test]
    fn websocket_ignores_events_and_reports_errors_and_disconnects() {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        rt.block_on(async {
            let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
            let address = listener.local_addr().unwrap();
            let server = tokio::spawn(async move {
                let (stream, _) = listener.accept().await.unwrap();
                let mut ws = tokio_tungstenite::accept_async(stream).await.unwrap();
                for failure in [false, true] {
                    let Message::Text(text) = ws.next().await.unwrap().unwrap() else { panic!("request") };
                    let request: Value = serde_json::from_str(&text).unwrap();
                    ws.send(Message::Text(json!({"event": {"source": "Test"}}).to_string().into())).await.unwrap();
                    ws.send(Message::Text(json!({"id": "unrelated", "status": "ok"}).to_string().into())).await.unwrap();
                    ws.send(Message::Text(json!({"id": request["id"], "status": if failure { "error" } else { "ok" }, "error": "service unavailable"}).to_string().into())).await.unwrap();
                }
                ws.close(None).await.unwrap();
            });
            let (ws, _) = tokio_tungstenite::connect_async(format!("ws://{address}")).await.unwrap();
            let mut client = Client { ws: Some(ws), next: 0 };
            assert!(client.call("status", &json!({})).await.is_ok());
            assert_eq!(client.call("status", &json!({})).await.unwrap_err(), "service unavailable");
            assert!(client.call("status", &json!({})).await.is_err());
            assert!(client.ws.is_none());
            server.await.unwrap();
        });
    }

    fn sample(schema: &Value) -> Value {
        if let Some(values) = schema["enum"].as_array() {
            return values[0].clone();
        }
        let kind = schema["type"].as_str().or_else(|| schema["type"][0].as_str());
        match kind {
            Some("string") => json!("test"),
            Some("integer") => json!(1),
            Some("boolean") => json!(true),
            Some("array") => json!([]),
            Some("object") => json!({}),
            _ => Value::Null,
        }
    }

    #[test]
    fn every_tool_rejects_missing_and_mistyped_arguments() {
        let mut names = std::collections::HashSet::new();
        for (name, _, props, required) in TOOLS {
            assert!(names.insert(name), "duplicate tool {name}");
            let props: Value = serde_json::from_str(props).unwrap();
            let mut args = json!({});
            for key in *required {
                args[key] = sample(&props[key]);
            }
            if *name == "request" {
                args["json"] = json!({"request": "GetStatus"});
            }
            assert!(validate_arguments(name, &args).is_ok(), "{name}: {args}");
            for key in *required {
                let mut missing = args.clone();
                missing.as_object_mut().unwrap().remove(*key);
                assert!(validate_arguments(name, &missing).is_err(), "{name}.{key}");
            }
            for (key, schema) in props.as_object().unwrap() {
                if schema.get("type").is_none() {
                    continue;
                }
                let mut wrong = args.clone();
                wrong[key] = json!(1.5);
                assert!(validate_arguments(name, &wrong).is_err(), "{name}.{key}");
            }
            assert!(validate_arguments(name, &Value::Null).is_err(), "{name}");
            args["typo"] = json!(true);
            assert!(validate_arguments(name, &args).is_err(), "{name}");
        }
    }

    #[test]
    fn every_tool_dispatches_without_touching_live_services() {
        // run_scenario uses the shared log the engine tests read.
        let _g = crate::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        rt.block_on(async {
            let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
            let address = listener.local_addr().unwrap();
            let server = tokio::spawn(async move {
                let (stream, _) = listener.accept().await.unwrap();
                let mut ws = tokio_tungstenite::accept_async(stream).await.unwrap();
                while let Some(Ok(Message::Text(text))) = ws.next().await {
                    let req: Value = serde_json::from_str(&text).unwrap();
                    assert!(req["request"].is_string(), "{req}");
                    let reply = json!({"id": req["id"], "status": "ok", "messages": [], "ran": 0});
                    ws.send(Message::Text(reply.to_string().into())).await.unwrap();
                }
            });
            let (ws, _) = tokio_tungstenite::connect_async(format!("ws://{address}"))
                .await
                .unwrap();
            let mut client = Client { ws: Some(ws), next: 0 };
            for (name, _, props, required) in TOOLS {
                let props: Value = serde_json::from_str(props).unwrap();
                let mut args = json!({});
                for key in *required {
                    args[key] = sample(&props[key]);
                }
                match *name {
                    "request" => args["json"] = json!({"request": "GetStatus"}),
                    "docs" | "describe" => {
                        args["kind"] = json!("subaction");
                        args["code"] = json!(10);
                    }
                    "run_scenario" => args["scenario"] = json!({"name": "MCP dispatch", "actions": [], "steps": []}),
                    _ => {}
                }
                let result = client.call(name, &args).await;
                if *name == "agent_setup" {
                    // A nonexistent agent exercises dispatch without editing any configuration.
                    assert!(result.unwrap_err().contains("no agent"));
                } else {
                    assert!(result.is_ok(), "{name}: {result:?}");
                }
            }
            server.abort();
        });
    }

    #[test]
    fn rejects_requests_that_would_panic_or_truncate_codes() {
        for request in [json!(null), json!([]), json!("GetStatus"), json!({})] {
            assert!(validate_arguments("request", &json!({"json": request})).is_err());
        }
        for name in ["docs", "describe"] {
            for code in [json!(-1), json!(4294967296_u64)] {
                assert!(validate_arguments(name, &json!({"kind": "trigger", "code": code})).is_err());
            }
        }
        assert!(validate_arguments("export_actions", &json!({"ids": [42]})).is_err());
        assert!(validate_arguments("action_group_icon", &json!({"group": "test", "icon": null})).is_ok());
        assert!(validate_arguments("overlay_publish", &json!({"topic": "test", "value": [1, true]})).is_ok());
    }

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

    #[test]
    fn the_action_editor_is_reachable_through_the_core_tools() {
        let tool = |name: &str| TOOLS.iter().find(|tool| tool.0 == name).expect("tool");
        for (name, keys) in [
            ("core_view", &["actionId", "kind"][..]),
            ("core_action", &["actionId", "code", "to"][..]),
            ("entity_form", &["actionId", "code", "anchor"][..]),
        ] {
            let props: Value = serde_json::from_str(tool(name).2).unwrap();
            for key in keys {
                assert!(props.get(*key).is_some(), "{name} lacks {key}");
            }
        }
        for (name, word) in [
            ("core_view", "actions_picker"),
            ("core_action", "\"actions\""),
            ("core_action", "act:sub_add"),
        ] {
            let tool = tool(name);
            assert!(tool.1.contains(word) || tool.2.contains(word), "{name} omits {word}");
        }
    }
}
