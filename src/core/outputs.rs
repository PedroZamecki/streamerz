//! On-demand stream output discovery and control through an already connected OBS port.
//! Vendor acknowledgements are requests, not evidence that a stream is live.

use crate::{engine::Engine, ports::ObsPort};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{
    cell::RefCell,
    collections::{HashMap, HashSet},
    rc::Rc,
    time::Duration,
};

const SUITE: &str = "aitum-stream-suite";
const MULTI: &str = "aitum-multistream";
const VERTICAL: &str = "aitum-vertical-canvas";
const RTMP: &str = "sorayuki.multi_rtmp";

// Multistream removes stopped runtime objects. Keep only states confirmed by its existing events,
// scoped to a connection and discarded on disconnect/profile changes. There is no watcher or timer.
thread_local! {
    static MULTI_STATE: RefCell<HashMap<String, HashMap<String, bool>>> = RefCell::default();
}

pub(crate) fn reset_connection(connection: &str) {
    MULTI_STATE.with_borrow_mut(|states| {
        states.remove(connection);
    });
}

pub(crate) fn observe(connection: &str, event: &str, data: &Value) {
    if matches!(event, "CurrentProfileChanged" | "CurrentSceneCollectionChanged") {
        reset_connection(connection);
    } else if event == "VendorEvent" && data["vendorName"] == MULTI {
        let active = match text(data, "eventType") {
            "output_started" => true,
            "output_stopped" => false,
            _ => return,
        };
        let name = text(&data["eventData"], "name");
        let name = name
            .strip_prefix("aitum_multi_output_")
            .or_else(|| name.strip_prefix("vertical_canvas_stream_"));
        if let Some(name) = name.filter(|name| !name.is_empty()) {
            MULTI_STATE.with_borrow_mut(|states| {
                states.entry(connection.into()).or_default().insert(name.into(), active);
            });
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Output {
    /// Opaque, connection-scoped identity. Never resolve a selection by display name alone.
    pub id: String,
    pub source: String,
    pub name: String,
    pub platform: Option<String>,
    pub platform_source: Option<String>,
    /// None means the API cannot tell us, not that the output is stopped.
    pub active: Option<bool>,
    pub reconnecting: Option<bool>,
    pub vertical: Option<bool>,
    pub controllable: bool,
    pub limitation: Option<String>,
    #[serde(skip)]
    key: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Inventory {
    pub connection_id: String,
    pub outputs: Vec<Output>,
    pub canvases: Vec<Value>,
    pub capabilities: Vec<Value>,
}

fn text<'a>(v: &'a Value, key: &str) -> &'a str {
    v[key].as_str().unwrap_or("")
}

fn platform(value: &str) -> Option<String> {
    let s = value.to_ascii_lowercase();
    ["twitch", "youtube", "tiktok", "kick"]
        .into_iter()
        .find(|p| s.split(|c: char| !c.is_ascii_alphanumeric()).any(|word| word == *p))
        .map(str::to_owned)
}

fn output(connection: &str, source: &str, key: &str, name: &str) -> Output {
    let platform = platform(name);
    Output {
        id: json!([connection, source, key]).to_string(),
        source: source.into(),
        name: name.into(),
        platform_source: platform.as_ref().map(|_| "name_hint".into()),
        platform,
        active: None,
        reconnecting: None,
        vertical: None,
        controllable: true,
        limitation: None,
        key: key.into(),
    }
}

async fn ask(port: &dyn ObsPort, conn: &str, kind: &str, data: Value) -> Result<Value, String> {
    tokio::time::timeout(Duration::from_secs(5), port.send_request(Some(conn), kind, data))
        .await
        .map_err(|_| format!("OBS {kind} timed out"))?
}

async fn vendor(port: &dyn ObsPort, conn: &str, source: &str, kind: &str, data: Value) -> Result<Value, String> {
    let response = ask(
        port,
        conn,
        "CallVendorRequest",
        json!({"vendorName": source, "requestType": kind, "requestData": data}),
    )
    .await?;
    let data = response.get("responseData").ok_or("Missing vendor responseData")?;
    if data["success"] == false || data.get("error").is_some_and(|e| !e.is_null() && e != "") {
        return Err(data["error"].as_str().unwrap_or("Plugin rejected the request").into());
    }
    Ok(data.clone())
}

/// Nothing is connected or started as a side effect of discovery.
pub async fn discover(port: &dyn ObsPort, conn: &str) -> Result<Inventory, String> {
    if !port.is_connected(Some(conn)) {
        return Err("OBS is not connected".into());
    }
    let mut inventory = Inventory {
        connection_id: conn.into(),
        outputs: Vec::new(),
        canvases: Vec::new(),
        capabilities: Vec::new(),
    };
    let mut main = output(conn, "obs", "main", "Main stream");
    match ask(port, conn, "GetStreamStatus", json!({})).await {
        Ok(s) => {
            main.active = s["outputActive"].as_bool();
            main.reconnecting = s["outputReconnecting"].as_bool();
        }
        Err(e) => main.limitation = Some(e),
    }
    // Service settings can contain a stream key. Only keep an allowlisted platform identifier.
    if let Ok(s) = ask(port, conn, "GetStreamServiceSettings", json!({})).await {
        let settings = &s["streamServiceSettings"];
        main.platform = platform(text(settings, "service"));
        main.platform_source = main.platform.as_ref().map(|_| "service".into());
    }
    inventory.outputs.push(main);
    if let Ok(c) = ask(port, conn, "GetCanvasList", json!({})).await {
        inventory.canvases = c["canvases"].as_array().cloned().unwrap_or_default();
    }
    for (source, request, array) in [
        (SUITE, "get_outputs", "outputs"),
        (MULTI, "get_outputs", "outputs"),
        (RTMP, "list_targets", "targets"),
        (VERTICAL, "status", ""),
    ] {
        let data = match vendor(port, conn, source, request, json!({})).await {
            Ok(data) => data,
            Err(e) => {
                inventory
                    .capabilities
                    .push(json!({"source": source, "available": false, "reason": e}));
                continue;
            }
        };
        inventory
            .capabilities
            .push(json!({"source": source, "available": true}));
        if source == VERTICAL {
            if !data["streaming"].is_boolean() {
                let capability = inventory.capabilities.last_mut().unwrap();
                capability["available"] = false.into();
                capability["reason"] = "Missing streaming status".into();
                continue;
            }
            let mut o = output(conn, source, "default", "Vertical canvas streams");
            o.active = data["streaming"].as_bool();
            o.vertical = Some(true);
            o.limitation = Some("Controls all enabled stream destinations on the first matching Vertical canvas. Individual dormant destinations are not exposed by this API.".into());
            // Multistream already lists Vertical's individual destinations. Avoid a second control
            // that could start several destinations outside the user's selection.
            if !inventory
                .outputs
                .iter()
                .any(|o| o.source == MULTI && o.vertical == Some(true))
            {
                inventory.outputs.push(o);
            }
            continue;
        }
        let Some(items) = data[array].as_array() else {
            inventory.capabilities.last_mut().unwrap()["reason"] = "Malformed output list".into();
            inventory.capabilities.last_mut().unwrap()["available"] = false.into();
            continue;
        };
        for item in items {
            if source == SUITE && text(item, "type") != "stream" {
                continue;
            }
            let name = text(item, "name");
            let key = if source == RTMP { text(item, "id") } else { name };
            if key.is_empty() || name.is_empty() {
                continue;
            }
            let mut o = output(conn, source, key, name);
            o.active = if source == RTMP {
                // Released Multi-RTMP builds use isRunning; its API guide uses streaming.
                item["isRunning"].as_bool().or_else(|| item["streaming"].as_bool())
            } else {
                item["active"].as_bool()
            };
            o.reconnecting = item["reconnecting"].as_bool();
            o.vertical = item["vertical"].as_bool();
            if source == MULTI {
                o.active =
                    MULTI_STATE.with_borrow(|states| states.get(conn).and_then(|outputs| outputs.get(name)).copied());
                o.limitation = Some("State is unknown until OBS creates this output. Shared encoders may require the main stream first.".into());
            }
            inventory.outputs.push(o);
        }
        if source == SUITE {
            if let Ok(c) = vendor(port, conn, source, "get_canvas", json!({})).await {
                for canvas in c["canvas"].as_array().into_iter().flatten() {
                    let id = text(canvas, "uuid");
                    if id.is_empty()
                        || !inventory
                            .canvases
                            .iter()
                            .any(|known| text(known, "canvasUuid") == id || text(known, "uuid") == id)
                    {
                        inventory.canvases.push(canvas.clone());
                    }
                }
            }
        }
    }
    // OBS also exposes instantiated outputs from plugins without a vendor API. Service outputs
    // exclude recordings and virtual cameras. Do not duplicate outputs owned by the adapters above.
    if let Ok(data) = ask(port, conn, "GetOutputList", json!({})).await {
        for item in data["outputs"].as_array().into_iter().flatten() {
            if item["outputFlags"]["OBS_OUTPUT_SERVICE"] != true {
                continue;
            }
            let name = text(item, "outputName");
            if name.is_empty() || name == "simple_stream" || name == "adv_stream" {
                continue;
            }
            // Multi-RTMP uses the same internal name for every target. Its ID-based vendor list
            // already represents these outputs; the shared runtime name cannot identify a target.
            if name == "multi-output"
                && inventory
                    .capabilities
                    .iter()
                    .any(|c| c["source"] == RTMP && c["available"] == true)
            {
                continue;
            }
            if let Some(o) = inventory.outputs.iter_mut().find(|o| {
                (o.source == MULTI
                    && (name == format!("aitum_multi_output_{}", o.name)
                        || (o.vertical == Some(true) && name == format!("vertical_canvas_stream_{}", o.name))))
                    || (o.source == SUITE && name == format!("Aitum Stream Suite Output {}", o.name))
            }) {
                o.active = item["outputActive"].as_bool();
                continue;
            }
            // Some plugins use internal names unrelated to their configured destination names.
            // Avoid ambiguous duplicate controls when any supported vendor owns the instance.
            let ambiguous_owner = inventory.capabilities.iter().any(|c| c["available"] == true);
            let mut o = output(conn, "obs-output", name, name);
            o.active = item["outputActive"].as_bool();
            o.controllable = !ambiguous_owner;
            o.limitation = Some(if ambiguous_owner {
                "This runtime output may belong to a listed plugin destination. Use the plugin control to avoid starting it twice."
            } else {
                "Only instantiated outputs are visible through the generic OBS API."
            }.into());
            inventory.outputs.push(o);
        }
    }
    MULTI_STATE.with_borrow_mut(|states| {
        if let Some(outputs) = states.get_mut(conn) {
            outputs.retain(|name, _| inventory.outputs.iter().any(|o| o.source == MULTI && o.name == *name));
            if outputs.is_empty() {
                states.remove(conn);
            }
        }
    });
    // Name-addressed APIs cannot safely distinguish duplicate names (including across canvases).
    let mut duplicates = HashSet::new();
    let mut ids = HashSet::new();
    for o in &inventory.outputs {
        if !ids.insert(o.id.clone()) {
            duplicates.insert(o.id.clone());
        }
    }
    for o in &mut inventory.outputs {
        if duplicates.contains(&o.id) {
            o.controllable = false;
            o.limitation = Some("Rename duplicate outputs in OBS before controlling them.".into());
        }
    }
    Ok(inventory)
}

async fn control(port: &dyn ObsPort, conn: &str, o: &Output, start: bool) -> Result<Value, String> {
    if !o.controllable {
        return Err(o.limitation.clone().unwrap_or_default());
    }
    if (start && o.reconnecting == Some(true)) || (o.active == Some(start) && o.reconnecting != Some(true)) {
        return Ok(json!({"id": o.id, "state": "unchanged", "active": o.active, "reconnecting": o.reconnecting}));
    }
    let verb = if start { "start" } else { "stop" };
    match o.source.as_str() {
        "obs" => {
            ask(port, conn, if start { "StartStream" } else { "StopStream" }, json!({})).await?;
        }
        "obs-output" => {
            ask(
                port,
                conn,
                if start { "StartOutput" } else { "StopOutput" },
                json!({"outputName": o.key}),
            )
            .await?;
        }
        SUITE | MULTI => {
            let data = if o.source == SUITE {
                json!({"output": o.key})
            } else {
                json!({"name": o.key})
            };
            vendor(port, conn, &o.source, &format!("{verb}_output"), data).await?;
        }
        VERTICAL => {
            vendor(port, conn, VERTICAL, &format!("{verb}_streaming"), json!({})).await?;
        }
        RTMP => {
            vendor(port, conn, RTMP, &format!("{verb}_target"), json!({"id": o.key})).await?;
        }
        _ => return Err("Unsupported output source".into()),
    }
    Ok(json!({"id": o.id, "state": "requested", "requestedActive": start}))
}

/// Validate the complete selection before mutating anything. A failure in one destination does not
/// discard the result for another. Never implicitly start an unselected main stream.
pub async fn change(
    engine: &Engine,
    port: &dyn ObsPort,
    inventory: &Inventory,
    ids: &[String],
    start: bool,
) -> Result<Value, String> {
    if ids.is_empty() {
        return Err("Select at least one output".into());
    }
    for id in ids {
        let matches: Vec<_> = inventory.outputs.iter().filter(|o| &o.id == id).collect();
        if matches.len() != 1 || !matches[0].controllable {
            return Err(format!("Output is missing or ambiguous: {id}"));
        }
    }
    let mut selected: Vec<_> = inventory.outputs.iter().filter(|o| ids.contains(&o.id)).collect();
    // Confirm the selected main stream before starting destinations that may share its encoder.
    // Stop the selected main stream last.
    selected.sort_by_key(|o| if start { o.source != "obs" } else { o.source == "obs" });
    let mut results = Vec::new();
    let mut main_failed = false;
    for o in selected {
        if main_failed {
            results.push(json!({"id": o.id, "state": "error", "error": "Main stream did not start; additional outputs were not started"}));
            continue;
        }
        // Subscribe before sending StartStream, so a fast start cannot race the observer.
        let mut events = (start && o.source == "obs").then(|| OutputEvents::new(engine));

        match control(port, &inventory.connection_id, o, start).await {
            Ok(mut result) => {
                if start && o.source == "obs" && (result["state"] == "requested" || o.reconnecting == Some(true)) {
                    match events
                        .as_mut()
                        .unwrap()
                        .main_started(port, &inventory.connection_id)
                        .await
                    {
                        Ok(()) => result = json!({"id": o.id, "state": "confirmed", "active": true}),
                        Err(e) => {
                            main_failed = true;
                            result = json!({"id": o.id, "state": "error", "error": e});
                        }
                    }
                }
                results.push(result);
            }
            Err(e) => {
                main_failed = start && o.source == "obs";
                results.push(json!({"id": o.id, "state": "error", "error": e}));
            }
        }
    }
    Ok(json!({"results": results}))
}

/// A temporary subscription on the existing event hub, removed even if the request is cancelled.
struct OutputEvents<'a> {
    engine: &'a Engine,
    id: u64,
    rx: tokio::sync::mpsc::UnboundedReceiver<std::sync::Arc<str>>,
}

impl<'a> OutputEvents<'a> {
    fn new(engine: &'a Engine) -> Self {
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(u64::MAX);
        let id = NEXT.fetch_sub(1, std::sync::atomic::Ordering::Relaxed);
        let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
        let mut subs = crate::engine::events::Subs::new();
        crate::engine::events::update(
            &mut subs,
            &json!({"ObsStudio": ["StreamingStarted", "StreamingStopped", "Disconnected", "Event"]}),
            true,
        );
        engine.listen(id, &subs, &tx);
        Self { engine, id, rx }
    }

    async fn main_started(&mut self, port: &dyn ObsPort, conn: &str) -> Result<(), String> {
        let status = ask(port, conn, "GetStreamStatus", json!({})).await?;
        if status["outputActive"] == true && status["outputReconnecting"] != true {
            return Ok(());
        }
        tokio::time::timeout(Duration::from_secs(15), async {
            while let Some(event) = self.rx.recv().await {
                let event: Value = serde_json::from_str(&event).map_err(|e| e.to_string())?;
                if event["data"]["connectionId"] != conn {
                    continue;
                }
                let kind = text(&event["event"], "type");
                if kind == "Event" {
                    if event["data"]["eventType"] != "StreamStateChanged" {
                        continue;
                    }
                    match text(&event["data"], "outputState") {
                        "OBS_WEBSOCKET_OUTPUT_STARTED" | "OBS_WEBSOCKET_OUTPUT_RECONNECTED" => {
                            let status = ask(port, conn, "GetStreamStatus", json!({})).await?;
                            if status["outputActive"] == true && status["outputReconnecting"] != true {
                                return Ok(());
                            }
                            continue;
                        }
                        "OBS_WEBSOCKET_OUTPUT_STOPPED" => return Err("Main stream stopped while starting".into()),
                        _ => continue,
                    }
                }
                return if kind == "StreamingStarted" {
                    Ok(())
                } else {
                    Err("Main stream stopped or OBS disconnected while starting".into())
                };
            }
            Err("OBS event subscription closed".into())
        })
        .await
        .map_err(|_| "Main stream did not confirm starting within 15 seconds".to_string())?
    }
}

impl Drop for OutputEvents<'_> {
    fn drop(&mut self) {
        let (tx, _) = tokio::sync::mpsc::unbounded_channel();
        self.engine.listen(self.id, &Default::default(), &tx);
    }
}

pub async fn request(engine: &Rc<Engine>, req: &Value) -> Result<Value, String> {
    let port = engine.obs().ok_or("OBS is not connected")?;
    let connection = text(req, "connectionId");
    let connection = if connection.is_empty() {
        let config = engine.config();
        let connected: Vec<_> = config
            .obs
            .connections
            .iter()
            .filter_map(|c| c["id"].as_str())
            .filter(|id| port.is_connected(Some(id)))
            .collect();
        if connected.len() != 1 {
            return Err("Choose connectionId when there is not exactly one connected OBS".into());
        }
        connected[0].to_owned()
    } else {
        connection.to_owned()
    };
    let op = req["op"].as_str().unwrap_or("list");
    if !["list", "start", "stop"].contains(&op) {
        return Err("Unknown output operation".into());
    }
    let inventory = discover(port.as_ref(), &connection).await?;
    if op == "list" {
        return serde_json::to_value(inventory).map_err(|e| e.to_string());
    }
    let ids = req["outputs"]
        .as_array()
        .ok_or("outputs must be an array of output IDs")?
        .iter()
        .map(|id| id.as_str().map(str::to_owned).ok_or("Each output ID must be a string"))
        .collect::<Result<Vec<_>, _>>()?;
    change(engine, port.as_ref(), &inventory, &ids, op == "start").await
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;

    #[derive(Default)]
    struct Obs {
        replies: RefCell<std::collections::HashMap<String, Value>>,
        calls: RefCell<Vec<(String, Value)>>,
    }
    impl Obs {
        fn reply(&self, key: &str, value: Value) {
            self.replies.borrow_mut().insert(key.into(), value);
        }
    }
    impl ObsPort for Obs {
        fn send_request<'a>(
            &'a self,
            _: Option<&'a str>,
            kind: &'a str,
            data: Value,
        ) -> crate::ports::Fut<'a, Result<Value, String>> {
            self.calls.borrow_mut().push((kind.into(), data.clone()));
            let key = if kind == "CallVendorRequest" {
                format!("{}/{}", text(&data, "vendorName"), text(&data, "requestType"))
            } else {
                kind.into()
            };
            let result = self
                .replies
                .borrow()
                .get(&key)
                .cloned()
                .ok_or_else(|| "Unavailable".to_string());
            Box::pin(async move {
                let value = result?;
                if let Some(e) = value["testError"].as_str() {
                    return Err(e.into());
                }
                Ok(if kind == "CallVendorRequest" {
                    json!({"responseData": value})
                } else {
                    value
                })
            })
        }
        fn send_batch<'a>(
            &'a self,
            _: Option<&'a str>,
            _: bool,
            _: Vec<Value>,
        ) -> crate::ports::Fut<'a, Result<Vec<Value>, String>> {
            unreachable!()
        }
        fn is_connected(&self, _: Option<&str>) -> bool {
            true
        }
        fn connect(&self, _: Option<&str>) {
            panic!("must not connect")
        }
        fn disconnect(&self, _: Option<&str>) {
            unreachable!()
        }
        fn reload(&self) {
            unreachable!()
        }
    }
    fn engine() -> Rc<Engine> {
        Engine::new(
            std::sync::Arc::new(tokio::sync::watch::channel(crate::Config::default()).0),
            |_| {},
        )
    }

    #[test]
    fn discovery_adapts_plugins_and_keeps_secrets_out() {
        crate::engine::tests::run(async {
            let obs = Obs::default();
            obs.reply("GetStreamStatus", json!({"outputActive": false}));
            obs.reply("GetStreamServiceSettings", json!({"streamServiceSettings": {"service": "Twitch", "key": "private-key", "server": "rtmp://private-host"}}));
            obs.reply(
                "aitum-stream-suite/get_outputs",
                json!({"success": true, "outputs": [
                {"name": "YouTube", "type": "stream", "active": false},
                {"name": "Recording", "type": "record", "active": true}]}),
            );
            obs.reply(
                "aitum-multistream/get_outputs",
                json!({"success": true, "outputs": [{"name": "Kick", "vertical": false}]}),
            );
            obs.reply(
                "aitum-vertical-canvas/status",
                json!({"success": true, "streaming": true}),
            );
            obs.reply(
                "sorayuki.multi_rtmp/list_targets",
                json!({"targets": [{"id": "target-1", "name": "TikTok", "isRunning": true}]}),
            );
            obs.reply("GetOutputList", json!({"outputs": [{"outputName": "aitum_multi_output_Kick", "outputActive": true, "outputFlags": {"OBS_OUTPUT_SERVICE": true}}]}));
            let inventory = discover(&obs, "a").await.unwrap();
            assert_eq!(inventory.outputs.len(), 5);
            assert_eq!(inventory.outputs[0].platform.as_deref(), Some("twitch"));
            assert_eq!(inventory.outputs[2].active, Some(true));
            assert_eq!(
                inventory.outputs[3].active,
                Some(true),
                "released Multi-RTMP isRunning field"
            );
            assert_eq!(inventory.outputs[4].active, Some(true));
            let serialized = serde_json::to_string(&inventory).unwrap();
            assert!(!serialized.contains("private"));
            assert!(!serialized.contains("Recording"));
            assert!(inventory.outputs.iter().all(|o| o.id.starts_with("[\"a\",")));
        });
    }

    #[test]
    fn duplicate_names_and_foreign_ids_cannot_mutate_outputs() {
        crate::engine::tests::run(async {
            let obs = Obs::default();
            obs.reply(
                "aitum-multistream/get_outputs",
                json!({"success": true, "outputs": [
                {"name": "Same", "vertical": false}, {"name": "Same", "vertical": true}]}),
            );
            let inventory = discover(&obs, "a").await.unwrap();
            assert!(!inventory.outputs[1].controllable);
            assert!(!inventory.outputs.iter().any(|o| o.source == VERTICAL));
            obs.calls.borrow_mut().clear();
            let e = engine();
            assert!(change(&e, &obs, &inventory, &[inventory.outputs[1].id.clone()], true)
                .await
                .is_err());
            assert!(
                change(&e, &obs, &inventory, &[output("b", "obs", "main", "Main").id], true)
                    .await
                    .is_err()
            );
            assert!(change(&e, &obs, &inventory, &[], true).await.is_err());
            assert!(obs.calls.borrow().is_empty());
        });
    }

    #[test]
    fn selected_outputs_use_vendor_keys_and_report_partial_errors() {
        crate::engine::tests::run(async {
            let obs = Obs::default();
            let e = engine();
            let inventory = Inventory {
                connection_id: "a".into(),
                canvases: vec![],
                capabilities: vec![],
                outputs: vec![
                    output("a", "obs", "main", "Main"),
                    output("a", SUITE, "YouTube", "YouTube"),
                    output("a", MULTI, "Kick", "Kick"),
                    output("a", RTMP, "target-1", "TikTok"),
                ],
            };
            obs.reply(
                "aitum-stream-suite/start_output",
                json!({"success": false, "error": "Encoder failed"}),
            );
            obs.reply("aitum-multistream/start_output", json!({"success": true}));
            obs.reply("sorayuki.multi_rtmp/start_target", json!({"status": "start_requested"}));
            let ids: Vec<_> = inventory.outputs[1..].iter().map(|o| o.id.clone()).collect();
            let result = change(&e, &obs, &inventory, &ids, true).await.unwrap();
            assert_eq!(result["results"][0]["state"], "error");
            assert_eq!(result["results"][1]["state"], "requested");
            assert_eq!(result["results"][2]["state"], "requested");
            let calls = obs.calls.borrow();
            assert_eq!(calls.len(), 3, "unselected main stream is never started");
            assert_eq!(calls[0].1["requestData"], json!({"output": "YouTube"}));
            assert_eq!(calls[1].1["requestData"], json!({"name": "Kick"}));
            assert_eq!(calls[2].1["requestData"], json!({"id": "target-1"}));
        });
    }

    #[test]
    fn selected_main_must_be_confirmed_before_other_starts_and_stops_last() {
        crate::engine::tests::run(async {
            let obs = Obs::default();
            let e = engine();
            let inventory = Inventory {
                connection_id: "a".into(),
                canvases: vec![],
                capabilities: vec![],
                outputs: vec![output("a", MULTI, "Kick", "Kick"), output("a", "obs", "main", "Main")],
            };
            let ids: Vec<_> = inventory.outputs.iter().map(|o| o.id.clone()).collect();
            obs.reply("StartStream", json!({}));
            obs.reply("GetStreamStatus", json!({"outputActive": true}));
            obs.reply("aitum-multistream/start_output", json!({"success": true}));
            let result = change(&e, &obs, &inventory, &ids, true).await.unwrap();
            assert_eq!(result["results"][0]["state"], "confirmed");
            assert_eq!(obs.calls.borrow()[0].0, "StartStream");
            assert_eq!(obs.calls.borrow()[1].0, "GetStreamStatus");
            obs.calls.borrow_mut().clear();
            obs.reply("StopStream", json!({}));
            obs.reply("aitum-multistream/stop_output", json!({"success": true}));
            change(&e, &obs, &inventory, &ids, false).await.unwrap();
            assert_eq!(obs.calls.borrow()[1].0, "StopStream");
            obs.calls.borrow_mut().clear();
            obs.reply("StartStream", json!({"testError": "No service configured"}));
            let result = change(&e, &obs, &inventory, &ids, true).await.unwrap();
            assert_eq!(result["results"][0]["state"], "error");
            assert_eq!(result["results"][1]["state"], "error");
            assert_eq!(obs.calls.borrow().len(), 1);
        });
    }

    #[test]
    fn generic_fallback_filters_recordings_and_preserves_unknown_plugin_state() {
        crate::engine::tests::run(async {
            let obs = Obs::default();
            obs.reply(
                "GetOutputList",
                json!({"outputs": [
                {"outputName": "Plugin stream", "outputActive": false, "outputFlags": {"OBS_OUTPUT_SERVICE": true}},
                {"outputName": "Recording", "outputActive": true, "outputFlags": {"OBS_OUTPUT_SERVICE": false}}]}),
            );
            let inventory = discover(&obs, "a").await.unwrap();
            assert_eq!(inventory.outputs.len(), 2);
            assert!(inventory.outputs[1].controllable);
            obs.reply(
                "aitum-multistream/get_outputs",
                json!({"success": true, "outputs": [{"name": "Offline destination"}]}),
            );
            let inventory = discover(&obs, "a").await.unwrap();
            assert_eq!(inventory.outputs[1].active, None);
            assert_eq!(inventory.outputs.len(), 3);
            assert!(
                !inventory.outputs[2].controllable,
                "ambiguous runtime duplicates are read-only"
            );
        });
    }
    #[test]
    fn event_confirmation_is_connection_scoped_and_subscription_is_removed() {
        crate::engine::tests::run(async {
            let obs = Obs::default();
            obs.reply("GetStreamStatus", json!({"outputActive": false}));
            let e = engine();
            let mut events = OutputEvents::new(&e);
            assert!(e.listening("ObsStudio", "StreamingStarted"));
            e.emit("ObsStudio", "StreamingStopped", || json!({"connectionId": "other"}));
            e.emit("ObsStudio", "StreamingStarted", || json!({"connectionId": "a"}));
            events.main_started(&obs, "a").await.unwrap();
            drop(events);
            assert!(!e.listening("ObsStudio", "StreamingStarted"));
        });
    }

    #[test]
    fn multistream_states_follow_events_runtime_names_and_connection_lifetime() {
        crate::engine::tests::run(async {
            reset_connection("a");
            let obs = Obs::default();
            obs.reply(
                "aitum-multistream/get_outputs",
                json!({"success":true,"outputs":[{"name":"Kick","vertical":false},{"name":"TikTok","vertical":true}]}),
            );
            obs.reply("GetOutputList", json!({"outputs":[{"outputName":"vertical_canvas_stream_TikTok","outputActive":true,"outputFlags":{"OBS_OUTPUT_SERVICE":true}}]}));
            let initial = discover(&obs, "a").await.unwrap();
            assert_eq!(
                initial.outputs.len(),
                3,
                "runtime vertical output must not be duplicated"
            );
            assert_eq!(initial.outputs[1].active, None);
            assert_eq!(initial.outputs[2].active, Some(true));
            observe(
                "a",
                "VendorEvent",
                &json!({"vendorName":MULTI,"eventType":"output_stopped","eventData":{"name":"aitum_multi_output_Kick"}}),
            );
            assert_eq!(
                discover(&obs, "a").await.unwrap().outputs[1].active,
                Some(false),
                "a confirmed stop survives runtime object removal"
            );
            assert_eq!(
                discover(&obs, "b").await.unwrap().outputs[1].active,
                None,
                "connection state must not leak"
            );
            observe("a", "CurrentProfileChanged", &json!({}));
            assert_eq!(discover(&obs, "a").await.unwrap().outputs[1].active, None);
            observe(
                "a",
                "VendorEvent",
                &json!({"vendorName":MULTI,"eventType":"output_started","eventData":{"name":"aitum_multi_output_Kick"}}),
            );
            assert_eq!(discover(&obs, "a").await.unwrap().outputs[1].active, Some(true));
            reset_connection("a");
            assert_eq!(discover(&obs, "a").await.unwrap().outputs[1].active, None);
        });
    }

    #[test]
    fn a_runtime_label_does_not_override_another_plugins_state() {
        crate::engine::tests::run(async {
            let obs = Obs::default();
            obs.reply(
                "aitum-stream-suite/get_outputs",
                json!({"success":true,"outputs":[{"name":"Same","type":"stream","active":false}]}),
            );
            obs.reply("GetOutputList",json!({"outputs":[{"outputName":"Same","outputActive":true,"outputFlags":{"OBS_OUTPUT_SERVICE":true}}]}));
            let inventory = discover(&obs, "a").await.unwrap();
            assert_eq!(inventory.outputs[1].active, Some(false));
            assert_eq!(inventory.outputs[2].active, Some(true));
            assert!(!inventory.outputs[2].controllable);
        });
    }

    #[test]
    fn reconnecting_main_waits_for_reconnection_without_another_start_request() {
        crate::engine::tests::run(async {
            let obs = Rc::new(Obs::default());
            obs.reply(
                "GetStreamStatus",
                json!({"outputActive":true,"outputReconnecting":true}),
            );
            let e = engine();
            let mut main = output("a", "obs", "main", "Main");
            main.active = Some(true);
            main.reconnecting = Some(true);
            let inventory = Inventory {
                connection_id: "a".into(),
                outputs: vec![main.clone()],
                canvases: vec![],
                capabilities: vec![],
            };
            let obs_clone = obs.clone();
            let engine_clone = e.clone();
            tokio::task::spawn_local(async move {
                tokio::task::yield_now().await;
                obs_clone.reply(
                    "GetStreamStatus",
                    json!({"outputActive":true,"outputReconnecting":false}),
                );
                engine_clone.emit("ObsStudio","Event",||json!({"connectionId":"a","eventType":"StreamStateChanged","outputState":"OBS_WEBSOCKET_OUTPUT_RECONNECTED"}));
            });
            let result = change(&e, obs.as_ref(), &inventory, &[main.id], true).await.unwrap();
            assert_eq!(result["results"][0]["state"], "confirmed");
            assert!(obs.calls.borrow().iter().all(|(kind, _)| kind == "GetStreamStatus"));
            assert!(!e.listening("ObsStudio", "Event"));
            obs.reply("StopStream", json!({}));
            let mut main = output("a", "obs", "main", "Main");
            main.active = Some(false);
            main.reconnecting = Some(true);
            let stopped = control(obs.as_ref(), "a", &main, false).await.unwrap();
            assert_eq!(
                stopped["state"], "requested",
                "stopping must cancel reconnection even if outputActive is false"
            );
            assert_eq!(obs.calls.borrow().last().unwrap().0, "StopStream");
        });
    }

    #[test]
    fn native_and_suite_canvas_reports_are_joined_by_uuid() {
        crate::engine::tests::run(async {
            let obs = Obs::default();
            obs.reply("GetCanvasList",json!({"canvases":[{"canvasUuid":"main","canvasName":"Main"},{"canvasUuid":"vertical","canvasName":"Vertical"}]}));
            obs.reply("aitum-stream-suite/get_outputs", json!({"success":true,"outputs":[]}));
            obs.reply("aitum-stream-suite/get_canvas",json!({"success":true,"canvas":[{"uuid":"vertical","name":"Vertical"},{"uuid":"extra","name":"Extra"}]}));
            let inventory = discover(&obs, "a").await.unwrap();
            assert_eq!(inventory.canvases.len(), 3);
            assert_eq!(
                inventory.canvases[1]["canvasUuid"], "vertical",
                "keep the native report when both describe one canvas"
            );
            assert_eq!(inventory.canvases[2]["uuid"], "extra");
        });
    }

    #[test]
    fn rtmp_targets_are_not_duplicated_or_mapped_by_the_shared_runtime_name() {
        crate::engine::tests::run(async {
            let obs = Obs::default();
            obs.reply("sorayuki.multi_rtmp/list_targets",json!({"targets":[{"id":"one","name":"First","isRunning":false},{"id":"two","name":"Second","isRunning":true}]}));
            obs.reply("GetOutputList",json!({"outputs":[{"outputName":"multi-output","outputActive":true,"outputFlags":{"OBS_OUTPUT_SERVICE":true}},{"outputName":"multi-output","outputActive":false,"outputFlags":{"OBS_OUTPUT_SERVICE":true}}]}));
            let inventory = discover(&obs, "a").await.unwrap();
            assert_eq!(inventory.outputs.len(), 3);
            assert_eq!(inventory.outputs[1].active, Some(false));
            assert_eq!(inventory.outputs[2].active, Some(true));
        });
    }

    /// Start an isolated OBS with its websocket on this port before running this test.
    #[test]
    #[ignore = "needs an isolated OBS, STREAMERZ_TEST_OBS_PORT"]
    fn real_obs_discovery_through_shared_dispatcher() {
        crate::engine::tests::run(async {
            let port: u16 = std::env::var("STREAMERZ_TEST_OBS_PORT")
                .expect("isolated OBS port")
                .parse()
                .unwrap();
            let config: crate::Config = serde_json::from_value(json!({"obs": {"connections": [{
                "id": "outputs-test", "name": "Outputs test", "host": "127.0.0.1", "port": port,
                "autoConnect": true, "reconnect": false}]}}))
            .unwrap();
            let e = Engine::new(std::sync::Arc::new(tokio::sync::watch::channel(config).0), |_| {});
            let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
            let mut subs = crate::engine::events::Subs::new();
            crate::engine::events::update(&mut subs, &json!({"ObsStudio": ["Connected"]}), true);
            e.listen(123, &subs, &tx);
            e.obs_start();
            tokio::time::timeout(Duration::from_secs(15), rx.recv())
                .await
                .unwrap()
                .unwrap();
            let req = json!({"request": "StreamerzOutputs", "op": "list", "connectionId": "outputs-test"});
            let denied = crate::servers::request(&e, &req, true, false).await;
            assert_eq!(denied["status"], "error");
            let result = crate::servers::request(&e, &req, true, true).await;
            assert_eq!(result["status"], "ok", "{result}");
            assert_eq!(result["outputs"][0]["source"], "obs");
            assert!(result["outputs"][0]["active"].is_boolean());
            if let Ok(source) = std::env::var("STREAMERZ_TEST_OBS_VENDOR") {
                assert!(
                    result["capabilities"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .any(|c| c["source"] == source && c["available"] == true),
                    "{result}"
                );
            }
            println!("{result}");
            // Opt in only for the private profile pointing at a loopback RTMP receiver.
            if std::env::var("STREAMERZ_TEST_OBS_LOCAL_STREAM").as_deref() == Ok("1") {
                let main_id = result["outputs"][0]["id"].clone();
                let started = crate::servers::request(&e, &json!({"request": "StreamerzOutputs", "op": "start", "connectionId": "outputs-test", "outputs": [main_id]}), true, true).await;
                assert_eq!(started["results"][0]["state"], "confirmed", "{started}");
                let live = crate::servers::request(&e, &req, true, true).await;
                assert_eq!(live["outputs"][0]["active"], true);
                let stopped = crate::servers::request(&e, &json!({"request": "StreamerzOutputs", "op": "stop", "connectionId": "outputs-test", "outputs": [main_id]}), true, true).await;
                assert_eq!(stopped["results"][0]["state"], "requested", "{stopped}");
                println!("Local RTMP main stream: start confirmed, live state read, stop requested");
            }
            e.obs().unwrap().disconnect(Some("outputs-test"));
            e.listen(123, &Default::default(), &tx);
        });
    }
}

#[cfg(test)]
mod live_tests;
