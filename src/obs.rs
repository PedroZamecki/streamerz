//! OBS Studio integration via obs-websocket v5 protocol.
//! Supports multiple OBS connections, SHA-256 challenge-response authentication,
//! automatic reconnection, full event dispatch to triggers (14001..=14009),
//! and execution of all 29 OBS sub-actions.

use crate::engine::Engine;
use crate::model::Obj;
use crate::Config;
use futures_util::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{mpsc, oneshot, watch};
use tokio_tungstenite::tungstenite::Message;

pub const T_OBS_CONNECTED: u32 = 14001;
pub const T_OBS_DISCONNECTED: u32 = 14002;
pub const T_OBS_EVENT: u32 = 14003;
pub const T_OBS_SCENE_CHANGED: u32 = 14004;
pub const T_OBS_STREAMING_STARTED: u32 = 14005;
pub const T_OBS_STREAMING_STOPPED: u32 = 14006;
pub const T_OBS_RECORDING_STARTED: u32 = 14007;
pub const T_OBS_RECORDING_STOPPED: u32 = 14008;
pub const T_OBS_VENDOR_EVENT: u32 = 14009;

// OBS WebSocket v5 OpCodes
pub const OP_HELLO: u64 = 0;
pub const OP_IDENTIFY: u64 = 1;
pub const OP_IDENTIFIED: u64 = 2;
pub const OP_REIDENTIFY: u64 = 3;
pub const OP_EVENT: u64 = 5;
pub const OP_REQUEST: u64 = 6;
pub const OP_REQUEST_RESPONSE: u64 = 7;
pub const OP_REQUEST_BATCH: u64 = 8;
pub const OP_REQUEST_BATCH_RESPONSE: u64 = 9;

// All non-high-volume event subscriptions: General (1), Config (2), Scenes (4), Inputs (8),
// Transitions (16), Filters (32), Outputs (64), SceneItems (128), MediaInputs (256), Vendors (512), Ui (1024).
pub const EVENT_SUBSCRIPTIONS_ALL: u64 = 2047;

#[derive(Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct ObsConfig {
    /// Configured OBS connections ({id, name, host, port, password, autoConnect, reconnect, retryInterval, version}).
    pub connections: Vec<Obj>,
}

/// Parsed connection parameters with sensible defaults.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ObsConnInfo {
    pub id: String,
    pub name: String,
    pub host: String,
    pub port: u16,
    pub password: String,
    pub auto_connect: bool,
    pub reconnect: bool,
    pub retry_interval: u64,
    pub version: u8,
}

/// WebSocket URL for an OBS host and port, bracketing bare IPv6 addresses.
pub(crate) fn ws_url(host: &str, port: u16) -> String {
    let h = host.trim();
    let h = if h.is_empty() { "127.0.0.1" } else { h };
    if h.contains(':') && !h.starts_with('[') {
        format!("ws://[{h}]:{port}")
    } else {
        format!("ws://{h}:{port}")
    }
}

/// Exponential reconnect delay starting at `base_secs` (at least 1s), capped at `max(base_secs, 60)`s.
pub(crate) fn reconnect_delay(base_secs: u64, attempt: u32) -> Duration {
    let base = base_secs.max(1);
    let cap = base.max(60);
    let factor = 1u64 << attempt.saturating_sub(1).min(4);
    Duration::from_secs(base.saturating_mul(factor).min(cap))
}

impl ObsConnInfo {
    pub fn from_obj(o: &Obj) -> Self {
        let str_val = |key: &str| -> &str { o.get(key).and_then(Value::as_str).unwrap_or("").trim() };
        let bool_val = |key: &str, def: bool| -> bool { o.get(key).and_then(Value::as_bool).unwrap_or(def) };
        let u64_val = |key: &str, def: u64| -> u64 {
            o.get(key)
                .and_then(Value::as_u64)
                .or_else(|| o.get(key).and_then(Value::as_i64).map(|n| n.max(0) as u64))
                .unwrap_or(def)
        };

        let id = str_val("id").to_string();
        let name = match str_val("name") {
            "" => "OBS".to_string(),
            n => n.to_string(),
        };
        let host = match str_val("host") {
            "" => "127.0.0.1".to_string(),
            h => h.to_string(),
        };
        let port = u16::try_from(u64_val("port", 4455))
            .ok()
            .filter(|&p| p > 0)
            .unwrap_or(4455);
        // Preserve leading/trailing spaces in passwords.
        let password = o.get("password").and_then(Value::as_str).unwrap_or("").to_string();
        let auto_connect = bool_val("autoConnect", true);
        let reconnect = bool_val("reconnect", bool_val("reconnectOnDisconnect", true));
        let retry_interval = u64_val("retryInterval", 5).max(1);
        let version = u64_val("version", 5).min(u8::MAX as u64) as u8;

        Self {
            id,
            name,
            host,
            port,
            password,
            auto_connect,
            reconnect,
            retry_interval,
            version,
        }
    }
}

pub struct ObsRequest {
    pub request_type: String,
    pub request_id: String,
    pub request_data: Value,
    pub resp: oneshot::Sender<Result<Value, String>>,
}

pub struct ObsBatchRequest {
    pub request_id: String,
    pub halt_on_failure: bool,
    pub execution_type: u8,
    pub requests: Vec<Value>,
    pub resp: oneshot::Sender<Result<Vec<Value>, String>>,
}

pub enum ConnCmd {
    Request(ObsRequest),
    Batch(ObsBatchRequest),
    Disconnect,
}

pub enum ObsCmd {
    Request {
        connection_id: Option<String>,
        request_type: String,
        request_data: Value,
        resp: oneshot::Sender<Result<Value, String>>,
    },
    BatchRequest {
        connection_id: Option<String>,
        halt_on_failure: bool,
        execution_type: u8,
        requests: Vec<Value>,
        resp: oneshot::Sender<Result<Vec<Value>, String>>,
    },
    Connect {
        connection_id: Option<String>,
    },
    Disconnect {
        connection_id: Option<String>,
    },
    Reload,
}

#[derive(Clone)]
pub struct ObsHandle {
    tx: mpsc::UnboundedSender<ObsCmd>,
    connections: Arc<std::sync::RwLock<HashMap<String, Arc<AtomicBool>>>>,
}

impl crate::ports::ObsPort for ObsHandle {
    fn send_request<'a>(
        &'a self,
        connection_id: Option<&'a str>,
        request_type: &'a str,
        request_data: Value,
    ) -> crate::ports::Fut<'a, Result<Value, String>> {
        Box::pin(async move {
            crate::runner::ask(
                &self.tx,
                crate::runner::ReplyErrors {
                    down: "OBS runner is down",
                    cancelled: "OBS request cancelled",
                    timeout: Some((Duration::from_secs(30), "OBS request timed out")),
                },
                |resp| ObsCmd::Request {
                    connection_id: connection_id.map(String::from),
                    request_type: request_type.to_string(),
                    request_data,
                    resp,
                },
            )
            .await
        })
    }

    fn send_batch<'a>(
        &'a self,
        connection_id: Option<&'a str>,
        halt_on_failure: bool,
        requests: Vec<Value>,
    ) -> crate::ports::Fut<'a, Result<Vec<Value>, String>> {
        Box::pin(async move {
            crate::runner::ask(
                &self.tx,
                crate::runner::ReplyErrors {
                    down: "OBS runner is down",
                    cancelled: "OBS batch request cancelled",
                    timeout: Some((Duration::from_secs(30), "OBS batch request timed out")),
                },
                |resp| ObsCmd::BatchRequest {
                    connection_id: connection_id.map(String::from),
                    halt_on_failure,
                    execution_type: 0,
                    requests,
                    resp,
                },
            )
            .await
        })
    }

    fn is_connected(&self, connection_id: Option<&str>) -> bool {
        let map = match self.connections.read() {
            Ok(m) => m,
            Err(_) => return false,
        };
        if let Some(id) = connection_id.filter(|s| !s.is_empty()) {
            map.get(id).is_some_and(|st| st.load(Ordering::Relaxed))
        } else {
            map.values().any(|st| st.load(Ordering::Relaxed))
        }
    }

    fn connect(&self, connection_id: Option<&str>) {
        let _ = self.tx.send(ObsCmd::Connect {
            connection_id: connection_id.map(String::from),
        });
    }

    fn disconnect(&self, connection_id: Option<&str>) {
        let _ = self.tx.send(ObsCmd::Disconnect {
            connection_id: connection_id.map(String::from),
        });
    }

    fn reload(&self) {
        let _ = self.tx.send(ObsCmd::Reload);
    }
}

/// Matches trigger filter criteria: empty/unspecified matches all, otherwise case-insensitive equals.
pub fn field_matches(t: &Obj, field: &str, expected: &str) -> bool {
    match t.get(field).and_then(Value::as_str) {
        None => true,
        Some(s) if s.trim().is_empty() => true,
        Some(s) => s.trim().eq_ignore_ascii_case(expected.trim()) || s.trim() == expected.trim(),
    }
}

struct ConnHandle {
    info: ObsConnInfo,
    connected: Arc<AtomicBool>,
    cmd_tx: mpsc::UnboundedSender<ConnCmd>,
    task: Option<tokio::task::AbortHandle>,
}

impl ConnHandle {
    fn is_running(&self) -> bool {
        self.task.as_ref().is_some_and(|t| !t.is_finished())
    }
}

pub fn start(engine: Rc<Engine>, cfg: &Arc<watch::Sender<Config>>) -> ObsHandle {
    let (tx, rx) = mpsc::unbounded_channel();
    let connections = Arc::new(std::sync::RwLock::new(HashMap::new()));
    let handle = ObsHandle {
        tx,
        connections: connections.clone(),
    };

    let runner = Runner {
        engine,
        cfg: cfg.clone(),
        rx,
        connections,
        conn_handles: HashMap::new(),
        conn_order: Vec::new(),
    };
    tokio::task::spawn_local(runner.run());
    handle
}

struct Runner {
    engine: Rc<Engine>,
    cfg: Arc<watch::Sender<Config>>,
    rx: mpsc::UnboundedReceiver<ObsCmd>,
    connections: Arc<std::sync::RwLock<HashMap<String, Arc<AtomicBool>>>>,
    conn_handles: HashMap<String, ConnHandle>,
    conn_order: Vec<String>,
}

impl Runner {
    async fn run(mut self) {
        self.sync_connections(true);

        while let Some(cmd) = self.rx.recv().await {
            match cmd {
                ObsCmd::Request {
                    connection_id,
                    request_type,
                    request_data,
                    resp,
                } => {
                    self.dispatch_request(connection_id.as_deref(), request_type, request_data, resp);
                }
                ObsCmd::BatchRequest {
                    connection_id,
                    halt_on_failure,
                    execution_type,
                    requests,
                    resp,
                } => {
                    self.dispatch_batch(
                        connection_id.as_deref(),
                        halt_on_failure,
                        execution_type,
                        requests,
                        resp,
                    );
                }
                ObsCmd::Connect { connection_id } => {
                    self.sync_connections(false);
                    if let Some(id) = connection_id.as_deref() {
                        self.start_connection(id);
                    } else {
                        let all_ids = self.conn_order.clone();
                        for id in all_ids {
                            self.start_connection(&id);
                        }
                    }
                }
                ObsCmd::Disconnect { connection_id } => {
                    if let Some(id) = connection_id.as_deref() {
                        self.stop_connection(id);
                    } else {
                        let all_ids = self.conn_order.clone();
                        for id in all_ids {
                            self.stop_connection(&id);
                        }
                    }
                }
                ObsCmd::Reload => {
                    self.sync_connections(true);
                }
            }
        }
    }

    fn stop_connection(&mut self, id: &str) {
        if let Some(h) = self.conn_handles.get_mut(id) {
            let _ = h.cmd_tx.send(ConnCmd::Disconnect);
            if !h.connected.load(Ordering::Relaxed) {
                if let Some(task) = h.task.take() {
                    task.abort();
                }
            }
        }
    }

    fn sync_connections(&mut self, auto_start: bool) {
        let obs_cfg = self.cfg.borrow().obs.clone();
        let mut active_ids = Vec::new();
        let mut restart_ids = Vec::new();

        for obj in &obs_cfg.connections {
            let mut info = ObsConnInfo::from_obj(obj);
            if info.id.is_empty() {
                info.id = crate::engine::uuid();
            }
            active_ids.push(info.id.clone());

            if !self.conn_handles.contains_key(&info.id) {
                let connected = Arc::new(AtomicBool::new(false));
                let (cmd_tx, _) = mpsc::unbounded_channel();
                if let Ok(mut map) = self.connections.write() {
                    map.insert(info.id.clone(), connected.clone());
                }

                let handle = ConnHandle {
                    info: info.clone(),
                    connected: connected.clone(),
                    cmd_tx,
                    task: None,
                };
                self.conn_handles.insert(info.id.clone(), handle);
            } else if let Some(h) = self.conn_handles.get_mut(&info.id) {
                if h.info != info {
                    let was_running = h.is_running();
                    let was_connected = h.connected.swap(false, Ordering::Relaxed);
                    let old_info = h.info.clone();
                    if let Some(task) = h.task.take() {
                        task.abort();
                    }
                    if was_connected {
                        fire_disconnected(&self.engine, &old_info);
                    }
                    h.info = info.clone();
                    if was_running {
                        restart_ids.push(info.id.clone());
                    }
                }
            }
        }

        // Remove deleted connections
        let mut removed_connected = Vec::new();
        self.conn_handles.retain(|id, h| {
            if !active_ids.contains(id) {
                let _ = h.cmd_tx.send(ConnCmd::Disconnect);
                if let Some(task) = h.task.take() {
                    task.abort();
                }
                if h.connected.swap(false, Ordering::Relaxed) {
                    removed_connected.push(h.info.clone());
                }
                SEEN.with_borrow_mut(|m| m.remove(id));
                if let Ok(mut map) = self.connections.write() {
                    map.remove(id);
                }
                false
            } else {
                true
            }
        });
        for old_info in removed_connected {
            fire_disconnected(&self.engine, &old_info);
        }
        self.conn_order = active_ids;

        for id in restart_ids {
            self.start_connection(&id);
        }

        if auto_start {
            let ids_to_start: Vec<String> = self
                .conn_order
                .iter()
                .filter(|id| {
                    self.conn_handles
                        .get(*id)
                        .is_some_and(|h| h.info.auto_connect && !h.is_running())
                })
                .cloned()
                .collect();
            for id in ids_to_start {
                self.start_connection(&id);
            }
        }
    }

    fn start_connection(&mut self, id: &str) {
        if let Some(h) = self.conn_handles.get_mut(id) {
            if h.connected.load(Ordering::Relaxed) {
                return;
            }
            if let Some(task) = h.task.take() {
                task.abort();
            }
            let (cmd_tx, mut cmd_rx) = mpsc::unbounded_channel();
            h.cmd_tx = cmd_tx;
            let engine = self.engine.clone();
            let info = h.info.clone();
            let connected = h.connected.clone();
            let task = tokio::task::spawn_local(async move {
                run_conn_loop(engine, info, connected, &mut cmd_rx).await;
            })
            .abort_handle();
            h.task = Some(task);
        }
    }

    fn select_connection(&self, connection_id: Option<&str>) -> Option<&ConnHandle> {
        if let Some(id) = connection_id.filter(|s| !s.is_empty()) {
            if let Some(h) = self.conn_handles.get(id) {
                return Some(h);
            }
            // Check by name or deterministic config index
            if let Some(h) = self
                .conn_order
                .iter()
                .filter_map(|k| self.conn_handles.get(k))
                .find(|h| h.info.name == id)
            {
                return Some(h);
            }
            if let Ok(idx) = id.parse::<usize>() {
                if let Some(h) = self.conn_order.get(idx).and_then(|k| self.conn_handles.get(k)) {
                    return Some(h);
                }
            }
        }
        // Fall back to first connected connection, or first connection in config order
        self.conn_order
            .iter()
            .filter_map(|k| self.conn_handles.get(k))
            .find(|h| h.connected.load(Ordering::Relaxed))
            .or_else(|| self.conn_order.first().and_then(|k| self.conn_handles.get(k)))
    }

    fn dispatch_request(
        &self,
        connection_id: Option<&str>,
        request_type: String,
        request_data: Value,
        resp: oneshot::Sender<Result<Value, String>>,
    ) {
        let req_id = crate::engine::uuid();
        if let Some(h) = self.select_connection(connection_id) {
            let req = ObsRequest {
                request_type,
                request_id: req_id,
                request_data,
                resp,
            };
            if let Err(mpsc::error::SendError(ConnCmd::Request(req))) = h.cmd_tx.send(ConnCmd::Request(req)) {
                let _ = req.resp.send(Err("OBS is not connected".to_string()));
            }
        } else {
            let _ = resp.send(Err("No configured OBS connection".to_string()));
        }
    }

    fn dispatch_batch(
        &self,
        connection_id: Option<&str>,
        halt_on_failure: bool,
        execution_type: u8,
        requests: Vec<Value>,
        resp: oneshot::Sender<Result<Vec<Value>, String>>,
    ) {
        let req_id = crate::engine::uuid();
        if let Some(h) = self.select_connection(connection_id) {
            let req = ObsBatchRequest {
                request_id: req_id,
                halt_on_failure,
                execution_type,
                requests,
                resp,
            };
            if let Err(mpsc::error::SendError(ConnCmd::Batch(req))) = h.cmd_tx.send(ConnCmd::Batch(req)) {
                let _ = req.resp.send(Err("OBS is not connected".to_string()));
            }
        } else {
            let _ = resp.send(Err("No configured OBS connection".to_string()));
        }
    }
}

/// Replies immediately to a request received while OBS is not ready; returns true if the task should exit.
fn handle_offline_cmd(cmd: Option<ConnCmd>) -> bool {
    match cmd {
        Some(ConnCmd::Request(req)) => {
            let _ = req.resp.send(Err("OBS is not connected".to_string()));
            false
        }
        Some(ConnCmd::Batch(req)) => {
            let _ = req.resp.send(Err("OBS is not connected".to_string()));
            false
        }
        Some(ConnCmd::Disconnect) | None => true,
    }
}

async fn run_conn_loop(
    engine: Rc<Engine>,
    info: ObsConnInfo,
    is_connected: Arc<AtomicBool>,
    cmd_rx: &mut mpsc::UnboundedReceiver<ConnCmd>,
) {
    let url = ws_url(&info.host, info.port);
    crate::say(format!("OBS '{}': connecting to {url}...", info.name));
    let mut attempt: u32 = 0;
    let mut last_err: Option<String> = None;

    loop {
        let connect_fut = tokio::time::timeout(Duration::from_secs(15), crate::http::ws_connect(&url));
        tokio::pin!(connect_fut);
        let connect_res = loop {
            tokio::select! {
                res = &mut connect_fut => break res,
                cmd = cmd_rx.recv() => {
                    if handle_offline_cmd(cmd) {
                        return;
                    }
                }
            }
        };

        match connect_res {
            Ok(Ok((ws, _))) => {
                let (mut sink, mut stream) = ws.split();
                let mut identified = false;
                let mut pending: HashMap<String, oneshot::Sender<Result<Value, String>>> = HashMap::new();
                let mut batch_pending: HashMap<String, oneshot::Sender<Result<Vec<Value>, String>>> = HashMap::new();

                // Step 1: Wait for Op 0 (Hello) and Op 2 (Identified) with a handshake timeout.
                let handshake_deadline = tokio::time::sleep(Duration::from_secs(15));
                tokio::pin!(handshake_deadline);
                let mut close_note: Option<String> = None;
                loop {
                    tokio::select! {
                        _ = &mut handshake_deadline => {
                            crate::warn(format!("OBS '{}': handshake timed out", info.name));
                            break;
                        }
                        cmd = cmd_rx.recv() => {
                            if handle_offline_cmd(cmd) {
                                let _ = tokio::time::timeout(Duration::from_secs(1), sink.close()).await;
                                return;
                            }
                        }
                        msg_opt = stream.next() => {
                            let Some(msg_res) = msg_opt else { break };
                            let msg = match msg_res {
                                Ok(m) => m,
                                Err(e) => {
                                    crate::warn(format!("OBS '{}': handshake error: {e}", info.name));
                                    break;
                                }
                            };
                            match msg {
                                Message::Ping(data) => {
                                    let _ = sink.send(Message::Pong(data)).await;
                                }
                                Message::Close(frame) => {
                                    if let Some(f) = frame {
                                        close_note = Some(format!("code {} ({})", u16::from(f.code), f.reason));
                                    }
                                    break;
                                }
                                Message::Text(txt) => {
                                    if let Ok(v) = serde_json::from_str::<Value>(&txt) {
                                        let op = v.get("op").and_then(Value::as_u64).unwrap_or(99);
                                        if op == OP_HELLO {
                                            let d = v.get("d").cloned().unwrap_or(json!({}));
                                            let ws_ver = d["obsWebSocketVersion"].as_str().unwrap_or("").to_string();
                                            let studio = d["obsStudioVersion"].as_str().unwrap_or("").to_string();
                                            SEEN.with_borrow_mut(|m| m.entry(info.id.clone()).or_default().0 = (studio, ws_ver));
                                            let auth_obj = d.get("authentication").filter(|a| a.is_object());
                                            let identify_data = if let Some(auth) = auth_obj {
                                                let salt = auth.get("salt").and_then(Value::as_str).unwrap_or("");
                                                let challenge = auth.get("challenge").and_then(Value::as_str).unwrap_or("");
                                                let auth_response = crate::servers::auth_response(&info.password, salt, challenge);
                                                json!({
                                                    "rpcVersion": 1,
                                                    "authentication": auth_response,
                                                    "eventSubscriptions": EVENT_SUBSCRIPTIONS_ALL,
                                                })
                                            } else {
                                                json!({
                                                    "rpcVersion": 1,
                                                    "eventSubscriptions": EVENT_SUBSCRIPTIONS_ALL,
                                                })
                                            };

                                            let identify_msg = json!({
                                                "op": OP_IDENTIFY,
                                                "d": identify_data,
                                            });
                                            if sink.send(Message::Text(identify_msg.to_string().into())).await.is_err() {
                                                break;
                                            }
                                        } else if op == OP_IDENTIFIED {
                                            identified = true;
                                            attempt = 0;
                                            last_err = None;
                                            is_connected.store(true, Ordering::Relaxed);
                                            crate::core::status::clear_failure(&format!("obs:{}", info.id));
                                            crate::say(format!(
                                                "OBS '{}' ({}): connected and identified at {url}",
                                                info.name, info.id
                                            ));

                                            let args = obs_args(&info);
                                            engine.fire(T_OBS_CONNECTED, "ObsConnected", "OBS Studio", "obs", &args, |t| {
                                                field_matches(t, "connectionId", &info.id)
                                            });
                                            engine.emit("ObsStudio", "Connected", || json!(args));
                                            crate::core::preflight::on_obs_connected(&engine, &info.id);
                                            tokio::task::spawn_local(publish_catalog(engine.clone(), info.id.clone()));
                                            break;
                                        }
                                    }
                                }
                                _ => {}
                            }
                        }
                    }
                }

                if !identified {
                    crate::core::status::record_failure(
                        &format!("obs:{}", info.id),
                        crate::core::status::Failure::Handshake,
                    );
                    if let Some(note) = close_note {
                        crate::error(format!("OBS '{}': identification failed: {note}", info.name));
                    } else {
                        crate::error(format!("OBS '{}': identification failed", info.name));
                    }
                } else {
                    // Step 2: Active communication loop
                    loop {
                        tokio::select! {
                            cmd_opt = cmd_rx.recv() => {
                                match cmd_opt {
                                    Some(ConnCmd::Request(req)) => {
                                        let mut d = json!({
                                            "requestType": req.request_type,
                                            "requestId": req.request_id,
                                        });
                                        if !req.request_data.is_null() {
                                            d["requestData"] = req.request_data;
                                        }
                                        let req_msg = json!({
                                            "op": OP_REQUEST,
                                            "d": d,
                                        });
                                        pending.insert(req.request_id, req.resp);
                                        if sink.send(Message::Text(req_msg.to_string().into())).await.is_err() {
                                            break;
                                        }
                                    }
                                    Some(ConnCmd::Batch(req)) => {
                                        let batch_msg = json!({
                                            "op": OP_REQUEST_BATCH,
                                            "d": {
                                                "requestId": req.request_id,
                                                "haltOnFailure": req.halt_on_failure,
                                                "executionType": req.execution_type,
                                                "requests": req.requests,
                                            }
                                        });
                                        batch_pending.insert(req.request_id, req.resp);
                                        if sink.send(Message::Text(batch_msg.to_string().into())).await.is_err() {
                                            break;
                                        }
                                    }
                                    Some(ConnCmd::Disconnect) | None => {
                                        let _ = tokio::time::timeout(Duration::from_secs(1), sink.close()).await;
                                        for (_, tx) in pending.drain() {
                                            let _ = tx.send(Err("OBS connection closed".to_string()));
                                        }
                                        for (_, tx) in batch_pending.drain() {
                                            let _ = tx.send(Err("OBS connection closed".to_string()));
                                        }
                                        crate::say(format!("OBS '{}': disconnected by user request", info.name));
                                        is_connected.store(false, Ordering::Relaxed);
                                        fire_disconnected(&engine, &info);
                                        return;
                                    }
                                }
                            }
                            msg_opt = stream.next() => {
                                match msg_opt {
                                    Some(Ok(Message::Text(txt))) => {
                                        handle_obs_message(
                                            &engine,
                                            &info,
                                            &txt,
                                            &mut pending,
                                            &mut batch_pending,
                                        );
                                    }
                                    Some(Ok(Message::Ping(data))) => {
                                        let _ = sink.send(Message::Pong(data)).await;
                                    }
                                    Some(Ok(Message::Close(_))) | Some(Err(_)) | None => {
                                        crate::core::status::record_failure(&format!("obs:{}", info.id), crate::core::status::Failure::Closed);
                                        crate::warn(format!("OBS '{}': connection closed", info.name));
                                        break;
                                    }
                                    _ => {}
                                }
                            }
                        }
                    }
                }

                // Cleanup on disconnect
                is_connected.store(false, Ordering::Relaxed);
                for (_, tx) in pending.drain() {
                    let _ = tx.send(Err("OBS connection closed".to_string()));
                }
                for (_, tx) in batch_pending.drain() {
                    let _ = tx.send(Err("OBS connection closed".to_string()));
                }
                if identified {
                    fire_disconnected(&engine, &info);
                }
            }
            Ok(Err(e)) => {
                crate::core::status::record_failure(&format!("obs:{}", info.id), crate::core::status::Failure::Connect);
                let msg = e.to_string();
                if last_err.as_deref() != Some(msg.as_str()) {
                    crate::warn(format!("OBS '{}': failed to connect to {url}: {msg}", info.name));
                    last_err = Some(msg);
                }
            }
            Err(_) => {
                crate::core::status::record_failure(&format!("obs:{}", info.id), crate::core::status::Failure::Connect);
                let msg = "connection timed out".to_string();
                if last_err.as_deref() != Some(msg.as_str()) {
                    crate::warn(format!("OBS '{}': failed to connect to {url}: {msg}", info.name));
                    last_err = Some(msg);
                }
            }
        }

        if !info.reconnect {
            break;
        }

        attempt = attempt.saturating_add(1);
        let wait = tokio::time::sleep(reconnect_delay(info.retry_interval, attempt));
        tokio::pin!(wait);
        loop {
            tokio::select! {
                _ = &mut wait => break,
                cmd = cmd_rx.recv() => {
                    if handle_offline_cmd(cmd) {
                        return;
                    }
                }
            }
        }
    }
}

/// A connection's (OBS Studio version, obs-websocket version), and its program scene.
type Seen = ((String, String), String);

thread_local! {
    static SEEN: std::cell::RefCell<HashMap<String, Seen>> = Default::default();
}

/// Last program scene received from this explicit connection, without a request or a poll.
pub fn current_scene(connection: &str) -> Option<String> {
    SEEN.with_borrow(|seen| {
        seen.get(connection)
            .map(|(_, scene)| scene.clone())
            .filter(|scene| !scene.is_empty())
    })
}

/// Streamer.bot's obs.* variables for a connection (and the old names Streamerz used).
fn obs_args(info: &ObsConnInfo) -> Obj {
    let ((studio, ws), _) = SEEN.with_borrow(|m| m.get(&info.id).cloned().unwrap_or_default());
    let mut a = Obj::new();
    for (k, v) in [
        ("obs.id", info.id.clone()),
        ("obs.name", info.name.clone()),
        ("obs.host", format!("{}:{}", info.host, info.port)),
        ("obs.studioVersion", studio),
        ("obs.websocketVersion", ws),
        ("connectionId", info.id.clone()),
        ("connectionName", info.name.clone()),
    ] {
        a.insert(k.into(), v.into());
    }
    a
}

/// Reads OBS's scenes and inputs into the catalog (the editors' pickers).
async fn publish_catalog(engine: Rc<Engine>, connection: String) {
    let Some(obs) = engine.obs() else { return };
    let names = |v: &Value, list: &str, key: &str| -> Vec<(String, String)> {
        v[list]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|x| x[key].as_str())
            .map(|n| (n.to_string(), n.to_string()))
            .collect()
    };
    if let Ok(v) = obs.send_request(Some(&connection), "GetSceneList", json!({})).await {
        if let Some(scene) = v["currentProgramSceneName"].as_str() {
            SEEN.with_borrow_mut(|seen| seen.entry(connection.clone()).or_default().1 = scene.to_owned());
        }
        crate::catalog::set("obs.scenes", names(&v, "scenes", "sceneName"));
    }
    if let Ok(v) = obs.send_request(Some(&connection), "GetInputList", json!({})).await {
        crate::catalog::set("obs.sources", names(&v, "inputs", "inputName"));
    }
    // Outputs by name, for the Output sub-action (multistream and vertical canvas plugins add theirs).
    if let Ok(v) = obs.send_request(Some(&connection), "GetOutputList", json!({})).await {
        crate::catalog::set("obs.outputs", names(&v, "outputs", "outputName"));
    }
}

fn fire_disconnected(engine: &Rc<Engine>, info: &ObsConnInfo) {
    crate::core::outputs::reset_connection(&info.id);
    crate::core::stream::obs_changed();
    let args = obs_args(info);
    SEEN.with_borrow_mut(|m| {
        if let Some(entry) = m.get_mut(&info.id) {
            entry.1.clear();
        }
    });
    engine.fire(T_OBS_DISCONNECTED, "ObsDisconnected", "OBS Studio", "obs", &args, |t| {
        field_matches(t, "connectionId", &info.id)
    });
    engine.emit("ObsStudio", "Disconnected", || json!(args));
}

pub(crate) fn handle_obs_message(
    engine: &Rc<Engine>,
    info: &ObsConnInfo,
    txt: &str,
    pending: &mut HashMap<String, oneshot::Sender<Result<Value, String>>>,
    batch_pending: &mut HashMap<String, oneshot::Sender<Result<Vec<Value>, String>>>,
) {
    let v: Value = match serde_json::from_str(txt) {
        Ok(val) => val,
        Err(_) => return,
    };
    let op = v.get("op").and_then(Value::as_u64).unwrap_or(99);
    let d = match v.get("d") {
        Some(val) => val,
        None => return,
    };

    match op {
        OP_EVENT => {
            let event_type = d.get("eventType").and_then(Value::as_str).unwrap_or("");
            let event_data = d.get("eventData").and_then(Value::as_object);
            crate::core::outputs::observe(&info.id, event_type, &d["eventData"]);

            let mut args = obs_args(info);
            args.insert("eventType".into(), event_type.to_string().into());
            if let Some(data) = event_data {
                for (k, val) in data {
                    args.insert(k.clone(), val.clone());
                    args.insert(format!("obsEvent.{k}"), val.clone());
                }
            }
            // Keep obsEvent.eventType as the outer OBS event type even when eventData has its own eventType.
            args.insert("obsEvent.eventType".into(), event_type.to_string().into());

            // Always fire 14003: ObsEvent
            let ev_type_owned = event_type.to_string();
            let info_id = info.id.clone();
            engine.fire(T_OBS_EVENT, "ObsEvent", "OBS Studio", "obs", &args, move |t| {
                field_matches(t, "connectionId", &info_id) && field_matches(t, "eventType", &ev_type_owned)
            });
            engine.emit("ObsStudio", "Event", || json!(args));

            // Specific triggers
            match event_type {
                "CurrentProgramSceneChanged" => {
                    let scene_name = d
                        .pointer("/eventData/sceneName")
                        .and_then(Value::as_str)
                        .unwrap_or("")
                        .to_string();
                    let info_id = info.id.clone();
                    let scene_clone = scene_name.clone();
                    let mut scene_args = args.clone();
                    let old = SEEN.with_borrow_mut(|m| {
                        std::mem::replace(&mut m.entry(info.id.clone()).or_default().1, scene_name.clone())
                    });
                    scene_args.insert("obs.sceneName".into(), scene_name.clone().into());
                    scene_args.insert("obs.oldSceneName".into(), old.into());
                    scene_args.insert("sceneName".into(), scene_name.into());
                    engine.fire(
                        T_OBS_SCENE_CHANGED,
                        "ObsSceneChanged",
                        "OBS Studio",
                        "obs",
                        &scene_args,
                        move |t| {
                            field_matches(t, "connectionId", &info_id) && field_matches(t, "sceneName", &scene_clone)
                        },
                    );
                    engine.emit("ObsStudio", "SceneChanged", || json!(scene_args));
                }
                // The scenes and sources the editors offer follow what OBS has.
                "SceneListChanged" | "SceneCreated" | "SceneRemoved" | "SceneNameChanged" | "InputCreated"
                | "InputRemoved" | "InputNameChanged" => {
                    tokio::task::spawn_local(publish_catalog(engine.clone(), info.id.clone()));
                }
                "StreamStateChanged" => {
                    // Starting and stopping are their own states (outputActive false while starting).
                    let state = d
                        .pointer("/eventData/outputState")
                        .and_then(Value::as_str)
                        .unwrap_or("");
                    if state == "OBS_WEBSOCKET_OUTPUT_STARTING" {
                        crate::core::preflight::on_obs_stream_starting(engine, &info.id, None);
                        return;
                    }
                    if state != "OBS_WEBSOCKET_OUTPUT_STARTED" && state != "OBS_WEBSOCKET_OUTPUT_STOPPED" {
                        return;
                    }
                    let active = state == "OBS_WEBSOCKET_OUTPUT_STARTED";
                    let info_id = info.id.clone();
                    if active {
                        engine.notify_stream_started();
                        crate::core::stream::obs_changed();
                        engine.fire(
                            T_OBS_STREAMING_STARTED,
                            "ObsStreamingStarted",
                            "OBS Studio",
                            "obs",
                            &args,
                            move |t| field_matches(t, "connectionId", &info_id),
                        );
                        engine.emit("ObsStudio", "StreamingStarted", || json!(args));
                    } else {
                        crate::core::stream::obs_changed();
                        engine.fire(
                            T_OBS_STREAMING_STOPPED,
                            "ObsStreamingStopped",
                            "OBS Studio",
                            "obs",
                            &args,
                            move |t| field_matches(t, "connectionId", &info_id),
                        );
                        engine.emit("ObsStudio", "StreamingStopped", || json!(args));
                    }
                }
                "RecordStateChanged" => {
                    // Starting and stopping are their own states (outputActive false while starting).
                    let state = d
                        .pointer("/eventData/outputState")
                        .and_then(Value::as_str)
                        .unwrap_or("");
                    if state != "OBS_WEBSOCKET_OUTPUT_STARTED" && state != "OBS_WEBSOCKET_OUTPUT_STOPPED" {
                        return;
                    }
                    let active = state == "OBS_WEBSOCKET_OUTPUT_STARTED";
                    let info_id = info.id.clone();
                    if active {
                        engine.fire(
                            T_OBS_RECORDING_STARTED,
                            "ObsRecordingStarted",
                            "OBS Studio",
                            "obs",
                            &args,
                            move |t| field_matches(t, "connectionId", &info_id),
                        );
                        engine.emit("ObsStudio", "RecordingStarted", || json!(args));
                    } else {
                        engine.fire(
                            T_OBS_RECORDING_STOPPED,
                            "ObsRecordingStopped",
                            "OBS Studio",
                            "obs",
                            &args,
                            move |t| field_matches(t, "connectionId", &info_id),
                        );
                        engine.emit("ObsStudio", "RecordingStopped", || json!(args));
                    }
                }
                "VendorEvent" => {
                    let vendor_name = d
                        .pointer("/eventData/vendorName")
                        .and_then(Value::as_str)
                        .unwrap_or("")
                        .to_string();
                    let v_ev_type = d
                        .pointer("/eventData/eventType")
                        .and_then(Value::as_str)
                        .unwrap_or("")
                        .to_string();
                    if (vendor_name == "aitum-multistream" || vendor_name == "aitum-stream-suite")
                        && v_ev_type == "output_started"
                    {
                        let out_name = d
                            .pointer("/eventData/eventData/name")
                            .or_else(|| d.pointer("/eventData/eventData/output_name"))
                            .and_then(Value::as_str);
                        crate::core::preflight::on_obs_stream_starting(engine, &info.id, out_name);
                    }
                    let info_id = info.id.clone();
                    let mut args = args.clone();
                    args.insert("obsVendorEvent.vendor".into(), vendor_name.clone().into());
                    args.insert("obsVendorEvent.event".into(), v_ev_type.clone().into());
                    let vendor_clone = vendor_name.clone();
                    let v_type_clone = v_ev_type.clone();
                    engine.fire(
                        T_OBS_VENDOR_EVENT,
                        "ObsVendorEvent",
                        "OBS Studio",
                        "obs",
                        &args,
                        move |t| {
                            field_matches(t, "connectionId", &info_id)
                                && field_matches(t, "vendorName", &vendor_clone)
                                && field_matches(t, "eventType", &v_type_clone)
                        },
                    );
                    engine.emit("ObsStudio", "VendorEvent", || json!(args));
                }
                _ => {}
            }
        }
        OP_REQUEST_RESPONSE => {
            let req_id = d.get("requestId").and_then(Value::as_str).unwrap_or("");
            if let Some(resp_tx) = pending.remove(req_id) {
                let status = d.get("requestStatus");
                let success = status
                    .and_then(|s| s.get("result"))
                    .and_then(Value::as_bool)
                    .unwrap_or(false);
                if success {
                    let resp_data = d.get("responseData").cloned().unwrap_or(Value::Null);
                    let _ = resp_tx.send(Ok(resp_data));
                } else {
                    let comment = status
                        .and_then(|s| s.get("comment"))
                        .and_then(Value::as_str)
                        .unwrap_or("OBS request returned error");
                    let what = d.get("requestType").and_then(Value::as_str).unwrap_or("request");
                    // An absent optional vendor is a capability result, returned to the caller.
                    if what != "CallVendorRequest" || comment != "No vendor was found by that name." {
                        crate::error(format!("OBS {what}: {comment}"));
                    }
                    let _ = resp_tx.send(Err(comment.to_string()));
                }
            }
        }
        OP_REQUEST_BATCH_RESPONSE => {
            let req_id = d.get("requestId").and_then(Value::as_str).unwrap_or("");
            if let Some(resp_tx) = batch_pending.remove(req_id) {
                let results = d.get("results").and_then(Value::as_array).cloned().unwrap_or_default();
                let _ = resp_tx.send(Ok(results));
            }
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn obs_conn_info_defaults() {
        let mut obj = Obj::new();
        obj.insert("name".into(), "Main OBS".into());
        obj.insert("port".into(), 4455.into());
        let info = ObsConnInfo::from_obj(&obj);
        assert_eq!(info.name, "Main OBS");
        assert_eq!(info.host, "127.0.0.1");
        assert_eq!(info.port, 4455);
        assert!(info.auto_connect);
        assert!(info.reconnect);
        assert_eq!(info.version, 5);
    }

    #[test]
    fn obs_field_matches() {
        let mut t = Obj::new();
        assert!(field_matches(&t, "connectionId", "conn-1"));

        t.insert("connectionId".into(), "".into());
        assert!(field_matches(&t, "connectionId", "conn-1"));

        t.insert("connectionId".into(), "conn-1".into());
        assert!(field_matches(&t, "connectionId", "conn-1"));
        assert!(!field_matches(&t, "connectionId", "conn-2"));

        t.insert("sceneName".into(), "Gaming".into());
        assert!(field_matches(&t, "sceneName", "gaming"));
        assert!(!field_matches(&t, "sceneName", "Just Chatting"));
    }

    #[test]
    fn obs_auth_challenge_response() {
        // Known obs-websocket v5 example test
        let pass = "supersecretpassword";
        let salt = "lM1GncleQOaCu9lT1yeUZhFYnqhsLLP1G5lAGo3ixaI=";
        let challenge = "+IxH4CnCiqpX1rM9scsNynZzbOe4KhDeYcTNS3PDaeY=";
        let expected = "1Ct943GAT+6YQUUX47Ia/ncufilbe6+oD6lY+5kaCu4=";
        assert_eq!(crate::servers::auth_response(pass, salt, challenge), expected);
        // Empty and Unicode passwords still produce valid 44-char base64 SHA-256 digests.
        let empty = crate::servers::auth_response("", salt, challenge);
        let unicode = crate::servers::auth_response("pässwörd 🎬", salt, challenge);
        assert_eq!(empty.len(), 44);
        assert_eq!(unicode.len(), 44);
        assert_ne!(empty, unicode);
        assert_ne!(empty, expected);
    }

    #[test]
    fn obs_conn_info_and_url_edge_cases() {
        let mut obj = Obj::new();
        obj.insert("port".into(), 70000.into());
        obj.insert("password".into(), " keep spaces ".into());
        obj.insert("retryInterval".into(), (-5).into());
        obj.insert("version".into(), 999.into());
        let info = ObsConnInfo::from_obj(&obj);
        assert_eq!(info.port, 4455, "port above u16::MAX falls back to default");
        assert_eq!(info.password, " keep spaces ");
        assert_eq!(info.retry_interval, 1);
        assert_eq!(info.version, 255);

        assert_eq!(ws_url("127.0.0.1", 4455), "ws://127.0.0.1:4455");
        assert_eq!(ws_url("", 4455), "ws://127.0.0.1:4455");
        assert_eq!(ws_url("::1", 4455), "ws://[::1]:4455");
        assert_eq!(ws_url("[::1]", 4455), "ws://[::1]:4455");
    }

    #[test]
    fn reconnect_delay_backs_off_with_cap() {
        let seq: Vec<u64> = (1..=6).map(|n| reconnect_delay(5, n).as_secs()).collect();
        assert_eq!(seq, [5, 10, 20, 40, 60, 60]);
        assert_eq!(reconnect_delay(0, 1).as_secs(), 1);
        assert_eq!(reconnect_delay(120, 1).as_secs(), 120);
        assert_eq!(reconnect_delay(120, 5).as_secs(), 120);
    }

    #[test]
    fn request_and_batch_response_matching() {
        let (tx, _rx) = watch::channel(Config::default());
        let engine = Engine::new(Arc::new(tx), |_| {});
        let info = ObsConnInfo::from_obj(&Obj::new());
        let mut pending = HashMap::new();
        let mut batch_pending = HashMap::new();

        let (ok_tx, mut ok_rx) = oneshot::channel();
        let (err_tx, mut err_rx) = oneshot::channel();
        let (batch_tx, mut batch_rx) = oneshot::channel();
        pending.insert("r1".into(), ok_tx);
        pending.insert("r2".into(), err_tx);
        batch_pending.insert("b1".into(), batch_tx);

        // Unknown request id leaves real pending requests untouched.
        handle_obs_message(
            &engine,
            &info,
            &json!({"op": OP_REQUEST_RESPONSE, "d": {"requestId": "other", "requestStatus": {"result": true}}})
                .to_string(),
            &mut pending,
            &mut batch_pending,
        );
        assert_eq!(pending.len(), 2);

        handle_obs_message(
            &engine,
            &info,
            &json!({
                "op": OP_REQUEST_RESPONSE,
                "d": {
                    "requestType": "GetVersion",
                    "requestId": "r1",
                    "requestStatus": {"result": true, "code": 100},
                    "responseData": {"obsVersion": "31.0.0"}
                }
            })
            .to_string(),
            &mut pending,
            &mut batch_pending,
        );
        assert_eq!(ok_rx.try_recv().unwrap().unwrap()["obsVersion"], "31.0.0");

        handle_obs_message(
            &engine,
            &info,
            &json!({
                "op": OP_REQUEST_RESPONSE,
                "d": {
                    "requestType": "SetCurrentProgramScene",
                    "requestId": "r2",
                    "requestStatus": {"result": false, "code": 600, "comment": "No scene found"}
                }
            })
            .to_string(),
            &mut pending,
            &mut batch_pending,
        );
        assert_eq!(err_rx.try_recv().unwrap().unwrap_err(), "No scene found");

        handle_obs_message(
            &engine,
            &info,
            &json!({
                "op": OP_REQUEST_BATCH_RESPONSE,
                "d": {
                    "requestId": "b1",
                    "results": [{"requestStatus": {"result": true}}, {"requestStatus": {"result": false}}]
                }
            })
            .to_string(),
            &mut pending,
            &mut batch_pending,
        );
        assert_eq!(batch_rx.try_recv().unwrap().unwrap().len(), 2);
        assert!(pending.is_empty() && batch_pending.is_empty());
    }

    #[test]
    fn vendor_event_and_record_states_and_disconnect_clears_scene() {
        use crate::engine::tests::{action, engine, log, run};
        let _g = crate::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let mk = |id: &str, code: u32, msg: &str, extra: Value| {
            let mut a = action(id, id, json!([log(msg)]));
            let mut t = extra;
            t["id"] = "t".into();
            t["type"] = code.into();
            t["enabled"] = true.into();
            a["triggers"] = json!([t]);
            a
        };
        let e = engine(json!({
            "actions": [
                mk("rec_on", T_OBS_RECORDING_STARTED, "T R on", json!({})),
                mk("rec_off", T_OBS_RECORDING_STOPPED, "T R off", json!({})),
                mk("vend", T_OBS_VENDOR_EVENT, "T V %obsEvent.eventType% %obsVendorEvent.vendor% %obsVendorEvent.event%", json!({"vendorName": "my-plugin"})),
            ]
        }));
        let info = ObsConnInfo {
            id: "obs-v".into(),
            name: "Studio".into(),
            host: "127.0.0.1".into(),
            port: 4455,
            password: String::new(),
            auto_connect: false,
            reconnect: false,
            retry_interval: 1,
            version: 5,
        };
        let out = run(async move {
            let (mut p, mut b) = (HashMap::new(), HashMap::new());
            let ev = |t: &str, d: Value| json!({"op": OP_EVENT, "d": {"eventType": t, "eventData": d}}).to_string();
            handle_obs_message(
                &e,
                &info,
                &ev("CurrentProgramSceneChanged", json!({"sceneName": "Live"})),
                &mut p,
                &mut b,
            );
            assert_eq!(current_scene("obs-v").as_deref(), Some("Live"));
            for st in ["STARTING", "STARTED", "PAUSED", "RESUMED", "STOPPING", "STOPPED"] {
                handle_obs_message(
                    &e,
                    &info,
                    &ev(
                        "RecordStateChanged",
                        json!({"outputState": format!("OBS_WEBSOCKET_OUTPUT_{st}")}),
                    ),
                    &mut p,
                    &mut b,
                );
            }
            handle_obs_message(
                &e,
                &info,
                &ev("VendorEvent", json!({"vendorName": "my-plugin", "eventType": "cue"})),
                &mut p,
                &mut b,
            );
            fire_disconnected(&e, &info);
            assert_eq!(current_scene("obs-v"), None, "disconnect clears cached scene");
            tokio::time::sleep(Duration::from_millis(100)).await;
        });
        assert_eq!(out, ["T R on", "T R off", "T V VendorEvent my-plugin cue"]);
    }

    #[tokio::test]
    async fn mock_obs_no_auth_null_batch_and_backoff_offline_requests() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            let mut ws = tokio_tungstenite::accept_async(stream).await.unwrap();
            let hello = json!({
                "op": OP_HELLO,
                "d": { "obsWebSocketVersion": "5.5.0", "rpcVersion": 1, "authentication": null }
            });
            ws.send(Message::Text(hello.to_string().into())).await.unwrap();
            if let Some(Ok(Message::Text(txt))) = ws.next().await {
                let v: Value = serde_json::from_str(&txt).unwrap();
                assert_eq!(v["op"].as_u64(), Some(OP_IDENTIFY));
                assert!(
                    v["d"].get("authentication").is_none(),
                    "null auth must not send auth string"
                );
            }
            ws.send(Message::Text(
                json!({"op": OP_IDENTIFIED, "d": {"negotiatedRpcVersion": 1}})
                    .to_string()
                    .into(),
            ))
            .await
            .unwrap();
            if let Some(Ok(Message::Text(txt))) = ws.next().await {
                let v: Value = serde_json::from_str(&txt).unwrap();
                assert_eq!(v["op"].as_u64(), Some(OP_REQUEST_BATCH));
                let id = v["d"]["requestId"].as_str().unwrap();
                ws.send(Message::Text(
                    json!({"op": OP_REQUEST_BATCH_RESPONSE, "d": {"requestId": id, "results": [{"requestStatus": {"result": true}}]}})
                        .to_string()
                        .into(),
                ))
                .await
                .unwrap();
            }
            let _ = ws.close(None).await;
        });

        let local = tokio::task::LocalSet::new();
        local
            .run_until(async move {
                let (tx, _rx) = watch::channel(Config::default());
                let engine = Engine::new(Arc::new(tx), |_| {});
                let info = ObsConnInfo {
                    id: "mock-null-auth".into(),
                    name: "Mock".into(),
                    host: "127.0.0.1".into(),
                    port,
                    password: "ignored".into(),
                    auto_connect: true,
                    reconnect: true,
                    retry_interval: 10,
                    version: 5,
                };
                let connected = Arc::new(AtomicBool::new(false));
                let (cmd_tx, mut cmd_rx) = mpsc::unbounded_channel();
                let cc = connected.clone();
                let task = tokio::task::spawn_local(async move {
                    run_conn_loop(engine, info, cc, &mut cmd_rx).await;
                });

                let start = std::time::Instant::now();
                while !connected.load(Ordering::Relaxed) && start.elapsed() < Duration::from_secs(2) {
                    tokio::time::sleep(Duration::from_millis(15)).await;
                }
                assert!(connected.load(Ordering::Relaxed));

                let (btx, brx) = oneshot::channel();
                cmd_tx
                    .send(ConnCmd::Batch(ObsBatchRequest {
                        request_id: "batch-1".into(),
                        halt_on_failure: false,
                        execution_type: 0,
                        requests: vec![json!({"requestType": "GetVersion"})],
                        resp: btx,
                    }))
                    .unwrap();
                let res = brx.await.unwrap().unwrap();
                assert_eq!(res.len(), 1);

                // Wait for close so the loop enters its 10s reconnect wait.
                let start = std::time::Instant::now();
                while connected.load(Ordering::Relaxed) && start.elapsed() < Duration::from_secs(2) {
                    tokio::time::sleep(Duration::from_millis(15)).await;
                }
                assert!(!connected.load(Ordering::Relaxed));

                // Requests during reconnect wait fail fast without waking or aborting the sleep.
                let (rtx, rrx) = oneshot::channel();
                cmd_tx
                    .send(ConnCmd::Request(ObsRequest {
                        request_type: "GetVersion".into(),
                        request_id: "offline-1".into(),
                        request_data: json!({}),
                        resp: rtx,
                    }))
                    .unwrap();
                let err = tokio::time::timeout(Duration::from_millis(500), rrx)
                    .await
                    .unwrap()
                    .unwrap()
                    .unwrap_err();
                assert!(err.contains("not connected"), "unexpected error: {err}");

                // Disconnect exits the reconnect wait promptly.
                let _ = cmd_tx.send(ConnCmd::Disconnect);
                tokio::time::timeout(Duration::from_secs(1), task)
                    .await
                    .unwrap()
                    .unwrap();
            })
            .await;
    }

    #[tokio::test]
    #[ignore = "requires a live local OBS instance"]
    async fn live_obs_connection_test() {
        let local = tokio::task::LocalSet::new();
        local
            .run_until(async move {
                let (tx, _rx) = watch::channel(Config::default());
                let engine = Engine::new(Arc::new(tx), |_| {});
                let info = ObsConnInfo {
                    id: "test-live".into(),
                    name: "Live OBS".into(),
                    host: "127.0.0.1".into(),
                    port: 4455,
                    password: std::env::var("OBS_TEST_PASSWORD").unwrap_or_default(),
                    auto_connect: false,
                    reconnect: false,
                    retry_interval: 1,
                    version: 5,
                };
                let connected = Arc::new(AtomicBool::new(false));
                let (cmd_tx, mut cmd_rx) = mpsc::unbounded_channel();
                let connected_clone = connected.clone();
                let task = tokio::task::spawn_local(async move {
                    run_conn_loop(engine, info, connected_clone, &mut cmd_rx).await;
                });

                let start = std::time::Instant::now();
                while !connected.load(Ordering::Relaxed) && start.elapsed().as_millis() < 800 {
                    tokio::time::sleep(Duration::from_millis(50)).await;
                }

                if !connected.load(Ordering::Relaxed) {
                    // OBS not running in environment (e.g. CI), skip gracefully
                    let _ = cmd_tx.send(ConnCmd::Disconnect);
                    let _ = task.await;
                    return;
                }

                let (resp_tx, resp_rx) = oneshot::channel();
                let _ = cmd_tx.send(ConnCmd::Request(ObsRequest {
                    request_type: "GetVersion".into(),
                    request_id: crate::engine::uuid(),
                    request_data: json!({}),
                    resp: resp_tx,
                }));

                let ver = resp_rx.await.unwrap().unwrap();
                println!("OBS Live Version Response: {ver}");
                assert!(ver.get("obsVersion").is_some());

                let _ = cmd_tx.send(ConnCmd::Disconnect);
                let _ = task.await;
            })
            .await;
    }

    #[test]
    fn stream_states_and_scene_variables() {
        use crate::engine::tests::{action, engine, log, run};
        let _g = crate::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let watch: &[(u32, &str)] = &[
            (T_OBS_STREAMING_STARTED, "started %obs.name% %obs.host%"),
            (T_OBS_STREAMING_STOPPED, "stopped"),
            (T_OBS_SCENE_CHANGED, "%obs.oldSceneName%>%obs.sceneName%"),
        ];
        let actions: Vec<Value> = watch
            .iter()
            .map(|(code, vars)| {
                let mut a = action(&code.to_string(), "a", json!([log(&format!("T {vars}"))]));
                a["triggers"] = json!([{"id": "t", "type": code, "enabled": true}]);
                a
            })
            .collect();
        let e = engine(json!({ "actions": actions }));
        let info = ObsConnInfo {
            id: "c".into(),
            name: "Main".into(),
            host: "127.0.0.1".into(),
            port: 4455,
            password: String::new(),
            auto_connect: false,
            reconnect: false,
            retry_interval: 0,
            version: 5,
        };
        let out = run(async move {
            let (mut p, mut b) = (HashMap::new(), HashMap::new());
            let ev = |t: &str, d: Value| json!({"op": OP_EVENT, "d": {"eventType": t, "eventData": d}}).to_string();
            for (state, active) in [
                ("STARTING", false),
                ("STARTED", true),
                ("STOPPING", true),
                ("STOPPED", false),
            ] {
                let d = json!({"outputActive": active, "outputState": format!("OBS_WEBSOCKET_OUTPUT_{state}")});
                handle_obs_message(&e, &info, &ev("StreamStateChanged", d), &mut p, &mut b);
            }
            for scene in ["Intro", "Game"] {
                handle_obs_message(
                    &e,
                    &info,
                    &ev("CurrentProgramSceneChanged", json!({"sceneName": scene})),
                    &mut p,
                    &mut b,
                );
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        });
        assert_eq!(
            out,
            ["T started Main 127.0.0.1:4455", "T stopped", "T >Intro", "T Intro>Game"]
        );
    }

    #[tokio::test]
    async fn mock_obs_v5_protocol_and_events() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();

        // Spawn mock OBS server
        tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            let mut ws = tokio_tungstenite::accept_async(stream).await.unwrap();

            // 1. Send Op 0 (Hello)
            let hello = json!({
                "op": OP_HELLO,
                "d": {
                    "obsWebSocketVersion": "5.7.4",
                    "rpcVersion": 1,
                    "authentication": {
                        "challenge": "+IxH4CnCiqpX1rM9scsNynZzbOe4KhDeYcTNS3PDaeY=",
                        "salt": "lM1GncleQOaCu9lT1yeUZhFYnqhsLLP1G5lAGo3ixaI="
                    }
                }
            });
            ws.send(Message::Text(hello.to_string().into())).await.unwrap();

            // 2. Receive Op 1 (Identify)
            if let Some(Ok(Message::Text(txt))) = ws.next().await {
                let v: Value = serde_json::from_str(&txt).unwrap();
                assert_eq!(v["op"].as_u64(), Some(OP_IDENTIFY));
                assert_eq!(
                    v["d"]["authentication"].as_str(),
                    Some("1Ct943GAT+6YQUUX47Ia/ncufilbe6+oD6lY+5kaCu4=")
                );
            }

            // 3. Send Op 2 (Identified)
            let identified = json!({
                "op": OP_IDENTIFIED,
                "d": { "negotiatedRpcVersion": 1 }
            });
            ws.send(Message::Text(identified.to_string().into())).await.unwrap();

            // 4. Send Events
            // SceneChanged
            let ev_scene = json!({
                "op": OP_EVENT,
                "d": {
                    "eventType": "CurrentProgramSceneChanged",
                    "eventData": { "sceneName": "Gaming" }
                }
            });
            ws.send(Message::Text(ev_scene.to_string().into())).await.unwrap();

            // StreamStateChanged
            let ev_stream = json!({
                "op": OP_EVENT,
                "d": {
                    "eventType": "StreamStateChanged",
                    "eventData": { "outputActive": true }
                }
            });
            ws.send(Message::Text(ev_stream.to_string().into())).await.unwrap();

            // RecordStateChanged
            let ev_rec = json!({
                "op": OP_EVENT,
                "d": {
                    "eventType": "RecordStateChanged",
                    "eventData": { "outputActive": true }
                }
            });
            ws.send(Message::Text(ev_rec.to_string().into())).await.unwrap();

            // VendorEvent
            let ev_vendor = json!({
                "op": OP_EVENT,
                "d": {
                    "eventType": "VendorEvent",
                    "eventData": {
                        "vendorName": "streamerz",
                        "eventType": "test_vendor_event",
                        "custom": 42
                    }
                }
            });
            ws.send(Message::Text(ev_vendor.to_string().into())).await.unwrap();

            // 5. Handle a Request
            if let Some(Ok(Message::Text(txt))) = ws.next().await {
                let v: Value = serde_json::from_str(&txt).unwrap();
                if v["op"].as_u64() == Some(OP_REQUEST) {
                    let req_id = v["d"]["requestId"].as_str().unwrap();
                    let resp = json!({
                        "op": OP_REQUEST_RESPONSE,
                        "d": {
                            "requestType": v["d"]["requestType"],
                            "requestId": req_id,
                            "requestStatus": { "result": true, "code": 100 },
                            "responseData": { "currentProgramSceneName": "Gaming" }
                        }
                    });
                    ws.send(Message::Text(resp.to_string().into())).await.unwrap();
                }
            }

            // Let client process and then close
            tokio::time::sleep(Duration::from_millis(50)).await;
            let _ = ws.close(None).await;
        });

        // Run client in local set
        let local = tokio::task::LocalSet::new();
        local
            .run_until(async move {
                // Connected and Disconnected fire for this connection.
                use crate::engine::tests::{action, engine as with_actions};
                let on = |id: &str, code: u32| {
                    let mut a = action(id, id, json!([]));
                    a["triggers"] = json!([{"id": "t", "type": code, "enabled": true}]);
                    a
                };
                let engine =
                    with_actions(json!({"actions": [on("up", T_OBS_CONNECTED), on("down", T_OBS_DISCONNECTED)]}));
                let info = ObsConnInfo {
                    id: "mock-obs".into(),
                    name: "Mock OBS".into(),
                    host: "127.0.0.1".into(),
                    port,
                    password: "supersecretpassword".into(),
                    auto_connect: true,
                    reconnect: false,
                    retry_interval: 1,
                    version: 5,
                };
                let connected = Arc::new(AtomicBool::new(false));
                let (cmd_tx, mut cmd_rx) = mpsc::unbounded_channel();
                let connected_clone = connected.clone();
                let engine_clone = engine.clone();

                let task = tokio::task::spawn_local(async move {
                    run_conn_loop(engine_clone, info, connected_clone, &mut cmd_rx).await;
                });

                // Wait until connected
                let start = std::time::Instant::now();
                while !connected.load(Ordering::Relaxed) && start.elapsed().as_millis() < 1000 {
                    tokio::time::sleep(Duration::from_millis(20)).await;
                }
                assert!(connected.load(Ordering::Relaxed));

                // Test request
                let (resp_tx, resp_rx) = oneshot::channel();
                let req_id = crate::engine::uuid();
                cmd_tx
                    .send(ConnCmd::Request(ObsRequest {
                        request_type: "GetCurrentProgramScene".into(),
                        request_id: req_id,
                        request_data: json!({}),
                        resp: resp_tx,
                    }))
                    .unwrap();

                let res = resp_rx.await.unwrap().unwrap();
                assert_eq!(res["currentProgramSceneName"].as_str(), Some("Gaming"));

                // Wait for events to process
                tokio::time::sleep(Duration::from_millis(100)).await;
                let _ = task.await;
                assert!(!connected.load(Ordering::Relaxed));
                tokio::time::sleep(Duration::from_millis(50)).await;
                let ran: Vec<String> = engine.history().iter().map(|r| r.name.clone()).collect();
                assert_eq!(ran, ["up", "down"], "Connected, then Disconnected");
            })
            .await;
    }
}
