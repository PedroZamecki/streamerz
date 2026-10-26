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

/// (name, description, input schema properties, required)
const TOOLS: &[(&str, &str, &str, &[&str])] = &[
    ("status", "What's connected (Twitch, YouTube, Kick, TikTok, OBS, Streamlabs Desktop, StreamElements, Streamlabs), the version, and how many actions, commands and timers are set up.", "{}", &[]),
    ("log", "The last lines of streamerz's log.", r#"{"lines": {"type": "integer", "description": "How many (default 50)"}}"#, &[]),
    ("list_actions", "Every action: id, name, group, enabled, trigger and sub-action counts.", "{}", &[]),
    ("run_action", "Run an action by name or id, with arguments, as its Test does.", r#"{"action": {"type": "string", "description": "Name or id"}, "args": {"type": "object"}}"#, &["action"]),
    ("list_triggers", "Trigger type codes and names (e.g. 101 TwitchFollow), filtered by a word.", r#"{"filter": {"type": "string"}}"#, &[]),
    ("fire_trigger", "Fire a trigger (code or name) with arguments as its platform would: every action using it runs. Returns how many ran. Needs STREAMERZ_DEV=1.", r#"{"trigger": {"type": ["integer", "string"]}, "args": {"type": "object", "description": "e.g. {\"user\": \"ana\", \"viewers\": 10}"}}"#, &["trigger"]),
    ("inject_chat", "A fake chat message from a platform, run through the commands; returns how many ran and what streamerz would have sent back (nothing is sent). Needs STREAMERZ_DEV=1.", r#"{"platform": {"type": "string", "enum": ["twitch", "youtube", "kick", "tiktok"]}, "user": {"type": "string"}, "role": {"type": "string", "enum": ["viewer", "subscriber", "vip", "moderator", "broadcaster"]}, "text": {"type": "string"}, "badges": {"type": "array", "description": "As Streamer.bot's badges: [{\"name\": \"broadcaster\", \"version\": 1, \"imageUrl\": \"https://...\"}]"}}"#, &["platform", "text"]),
    ("chat_out", "The chat messages streamerz would have sent since the last call (dev mode keeps them instead of sending). Needs STREAMERZ_DEV=1.", "{}", &[]),
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
        match tokio::time::timeout(Duration::from_secs(10), wait).await {
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
                    .ask(json!({"request": "InjectChat", "platform": s("platform"), "user": user, "role": role, "text": s("text"), "badges": a["badges"]}))
                    .await?;
                tokio::time::sleep(Duration::from_millis(300)).await;
                let out = self.ask(json!({"request": "GetChatOut"})).await?;
                Ok(json!({"ran": r["ran"], "chatOut": out["messages"]}))
            }
            "chat_out" => self.ask(json!({"request": "GetChatOut"})).await,
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
            "serverInfo": {"name": "streamerz", "version": option_env!("STREAMERZ_VERSION").unwrap_or("dev")},
            "instructions": "Drives a running streamerz (Streamer.bot clone). Start it with STREAMERZ_DEV=1 so \
                fire_trigger, inject_chat and chat_out work and nothing is sent to real chats."
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
    fn lists_tools_with_valid_schemas() {
        let t = tools();
        assert_eq!(t["tools"].as_array().unwrap().len(), TOOLS.len());
        for tool in t["tools"].as_array().unwrap() {
            assert!(tool["inputSchema"]["properties"].is_object(), "{}", tool["name"]);
        }
    }
}
