//! Singleton settings use the same core schemas and validation as editable list entities.
use crate::{
    automation::schema::{F, W::*},
    engine::Engine,
    i18n::tr,
    Config,
};
use serde_json::{json, Value};
use std::rc::Rc;

pub const WEBSOCKET: &[F] = &[
    ("autoStart", "On", Bool),
    ("address", "Address", Text),
    ("port", "Port", Int),
    ("endpoint", "Endpoint", Text),
    ("enableAuth", "Password required", Bool),
    ("authEnforce", "Password for every request", Bool),
    ("authPassword", "Password", Secret),
    ("allowedOrigins", "Websites allowed (also HTTP), one per line", Multi),
];
pub const HTTP: &[F] = &[
    ("autoStart", "On", Bool),
    ("address", "Address", Text),
    ("port", "Port", Int),
];
pub const UDP: &[F] = &[("autoStart", "On", Bool), ("port", "Port", Int)];
pub const STREAMELEMENTS: &[F] = &[("streamelementsToken", "JWT token", Secret)];
pub const STREAMLABS: &[F] = &[("streamlabsToken", "Socket API token", Secret)];
pub const DONORDRIVE: &[F] = &[
    ("donordriveInstance", "Site", Text),
    (
        "donordriveParticipantId",
        "Participant ID (the number in your page's address)",
        Text,
    ),
];
pub const KINDS: &[&str] = &["server_websocket", "server_http", "server_udp", "server_stream_deck"];
pub fn fields(kind: &str) -> Option<&'static [F]> {
    match kind {
        "mcp_connection" => Some(crate::core::pages::MCP_FIELDS),
        "donations_streamelements" => Some(STREAMELEMENTS),
        "donations_streamlabs" => Some(STREAMLABS),
        "donations_donordrive" => Some(DONORDRIVE),
        "server_websocket" => Some(WEBSOCKET),
        "server_http" | "server_stream_deck" => Some(HTTP),
        "server_udp" => Some(UDP),
        _ => None,
    }
}
pub fn title(kind: &str) -> &'static str {
    tr(match kind {
        "server_websocket" => "lists.servers.websocket_server",
        "server_http" => "lists.servers.http_server",
        "server_udp" => "lists.servers.udp_server",
        "server_stream_deck" => "lists.servers.stream_deck_server",
        "mcp_connection" => "agents.cfg.title",
        "donations_streamelements" => return "StreamElements",
        "donations_streamlabs" => return "Streamlabs",
        "donations_donordrive" => return "DonorDrive",
        _ => "forms.empty",
    })
}
pub fn read(config: &Config, kind: &str) -> Option<Value> {
    let value = match kind {
        "server_websocket" => serde_json::to_value(&config.websockets),
        "server_http" => serde_json::to_value(&config.http),
        "server_udp" => serde_json::to_value(&config.udp),
        "server_stream_deck" => serde_json::to_value(&config.stream_deck),
        "mcp_connection" => serde_json::to_value(&config.websockets),
        "donations_streamelements" | "donations_streamlabs" | "donations_donordrive" => {
            serde_json::to_value(&config.donations)
        }
        _ => return None,
    };
    value.ok()
}
pub fn save(engine: &Rc<Engine>, kind: &str, item: crate::model::Obj) -> Result<Value, String> {
    if kind == "mcp_connection" {
        return super::pages::save_mcp_connection(engine, item);
    }
    if kind.starts_with("donations_") {
        let donations = serde_json::from_value(Value::Object(item)).map_err(|error| format!("{error}"))?;
        engine.change(|config| config.donations = donations);
        engine.donations_sync();
        return Ok(json!({"saved": kind}));
    }
    let port = item.get("port").and_then(Value::as_u64).unwrap_or(0);
    if !(1..=65535).contains(&port) {
        return Err(tr("lists.servers.port_range_warning").into());
    }
    // Decode before publishing the config, so an invalid draft never notifies or restarts a server.
    let value = Value::Object(item);
    match kind {
        "server_websocket" => {
            let settings = serde_json::from_value(value).map_err(|error| format!("{error}"))?;
            engine.change(|config| config.websockets = settings);
        }
        "server_http" => {
            let settings = serde_json::from_value(value).map_err(|error| format!("{error}"))?;
            engine.change(|config| config.http = settings);
        }
        "server_udp" => {
            let settings = serde_json::from_value(value).map_err(|error| format!("{error}"))?;
            engine.change(|config| config.udp = settings);
        }
        "server_stream_deck" => {
            let settings = serde_json::from_value(value).map_err(|error| format!("{error}"))?;
            engine.change(|config| config.stream_deck = settings);
        }
        _ => return Err(format!("unknown settings kind {kind}")),
    }
    Ok(json!({"saved": kind}))
}
pub fn model(config: &Config) -> Value {
    let rows = KINDS.iter().filter_map(|kind| {
        let settings = read(config, kind)?;
        let on = settings["autoStart"] == true;
        let address = settings["address"].as_str().unwrap_or_default();
        let port = settings["port"].as_u64().unwrap_or_default();
        let endpoint = settings["endpoint"].as_str().unwrap_or("/");
        let location = match *kind {
            "server_udp" => tr("lists.servers.port_display").replace("{port}", &port.to_string()),
            "server_http" => format!("http://{address}:{port}/"),
            _ => format!("ws://{address}:{port}{endpoint}"),
        };
        let password = if settings["enableAuth"] == true { tr("lists.servers.password_tag") } else { "" };
        let icon = match *kind {
            "server_websocket" => "radio",
            "server_http" => "globe",
            "server_udp" => "zap",
            "server_stream_deck" => "layers",
            _ => "server",
        };
        Some(json!({
            "id": kind,
            "kind": kind,
            "singleton": true,
            "name": title(kind),
            "enabled": on,
            "icon": icon,
            "health": if on { "up" } else { "off" },
            "location": location,
            "auth": settings["enableAuth"] == true,
            "text": format!("{}: {}, {location}{password}", title(kind), tr(if on { "lists.servers.enabled" } else { "lists.servers.disabled" }))
        }))
    }).collect::<Vec<_>>();
    json!({"kind":"server_settings", "presentation":"settings", "brand":"streamerbot", "title":tr("lists.servers.title"), "guide":tr("lists.servers.state_hint"), "rows":rows, "columns":[], "actions":["toggle", "edit"]})
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn server_edits_are_atomic_keep_unedited_connections_and_hide_passwords_in_models() {
        let _lock = crate::TEST_LOCK.lock().unwrap_or_else(|error| error.into_inner());
        crate::engine::tests::run(async {
            let engine = crate::engine::tests::engine(json!({"actions":[]}));
            engine.change(|config| {
                config.websockets.connections.push(
                    json!({"id":"client","name":"Keep me","autoConnect":false})
                        .as_object()
                        .unwrap()
                        .clone(),
                );
                config.websockets.auth_password = "private-fixture".into();
            });
            let original = engine.config().websockets.clone();
            let form = crate::automation::entities::form(&engine.config(), "server_websocket", "").unwrap();
            let index = |key| {
                form["fields"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .position(|field| field["key"] == key)
                    .unwrap()
            };
            for port in ["0", "65536", "-1", "invalid"] {
                let reply = crate::servers::answer(&engine,&json!({"request":"StreamerzEditEntity","kind":"server_websocket","changes":[{"field":index("port"),"value":port}]}),true).unwrap();
                assert_eq!(reply["status"], "error");
                assert!(engine.config().websockets == original);
            }
            let result = crate::servers::answer(&engine,&json!({"request":"StreamerzEditEntity","kind":"server_websocket","changes":[{"field":index("address"),"value":"localhost"}]}),true).unwrap();
            assert_eq!(result["status"], "ok");
            assert_eq!(engine.config().websockets.connections, original.connections);
            assert_eq!(engine.config().websockets.auth_password, original.auth_password);
            assert!(!model(&engine.config()).to_string().contains("private-fixture"));
            assert_eq!(
                crate::servers::answer(
                    &engine,
                    &json!({"request":"StreamerzDelete","what":"server_websocket","itemId":""}),
                    true
                )
                .unwrap()["status"],
                "error"
            );
            engine.change(|config| config.websockets.enable_auth = true);
            assert_eq!(
                crate::servers::answer(
                    &engine,
                    &json!({"request":"StreamerzEntityForm","kind":"server_websocket"}),
                    false
                )
                .unwrap()["status"],
                "error"
            );
        });
    }

    #[test]
    fn server_model_states_use_online_health_and_clear_icons() {
        let _lock = crate::TEST_LOCK.lock().unwrap_or_else(|error| error.into_inner());
        let mut config = Config::default();
        config.websockets.auto_start = true;
        config.http.auto_start = false;
        config.udp.auto_start = false;
        config.stream_deck.auto_start = true;

        let m = model(&config);
        assert_eq!(m["actions"], json!(["toggle", "edit"]));
        let rows = m["rows"].as_array().expect("rows array");
        assert_eq!(rows.len(), 4);

        // WebSocket server: on -> up, radio icon
        assert_eq!(rows[0]["id"], "server_websocket");
        assert_eq!(rows[0]["health"], "up");
        assert_eq!(rows[0]["enabled"], true);
        assert_eq!(rows[0]["icon"], "radio");

        // HTTP server: off -> off, globe icon
        assert_eq!(rows[1]["id"], "server_http");
        assert_eq!(rows[1]["health"], "off");
        assert_eq!(rows[1]["enabled"], false);
        assert_eq!(rows[1]["icon"], "globe");

        // UDP server: off -> off, zap icon
        assert_eq!(rows[2]["id"], "server_udp");
        assert_eq!(rows[2]["health"], "off");
        assert_eq!(rows[2]["enabled"], false);
        assert_eq!(rows[2]["icon"], "zap");

        // Stream Deck server: on -> up, layers icon
        assert_eq!(rows[3]["id"], "server_stream_deck");
        assert_eq!(rows[3]["health"], "up");
        assert_eq!(rows[3]["enabled"], true);
        assert_eq!(rows[3]["icon"], "layers");
    }
}
