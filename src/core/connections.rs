//! Connection commands and state shared by MCP and presentation adapters.
use super::view::{column, flag, note, text};
use crate::{
    automation::entities,
    engine::Engine,
    i18n::{tr, trf},
};
use serde_json::{json, Value};
use std::rc::Rc;

pub fn apply(engine: &Rc<Engine>, kind: &str, id: &str, action: &str) -> Result<Value, String> {
    if !matches!(kind, "obs_connection" | "slobs_connection") {
        return Err("kind: obs_connection or slobs_connection".into());
    }
    if !matches!(action, "connect" | "disconnect" | "refresh") {
        return Err("action: connect, disconnect or refresh".into());
    }
    let config = engine.config();
    let items = entities::items(&config, kind).ok_or("unknown connection kind")?;
    if !id.is_empty()
        && !items
            .iter()
            .any(|item| item.get("id").and_then(Value::as_str) == Some(id))
    {
        return Err(format!("no {kind} {id}"));
    }
    let empty = items.is_empty();
    drop(config);
    if empty || action == "refresh" {
        return Ok(json!({"accepted": true}));
    }
    let one = (!id.is_empty()).then_some(id);
    match (kind, action) {
        ("obs_connection", "connect") => engine.obs_start().connect(one),
        ("obs_connection", "disconnect") => {
            if let Some(handle) = engine.obs() {
                handle.disconnect(one);
            }
        }
        ("slobs_connection", "connect") => engine.slobs_connect(id),
        ("slobs_connection", "disconnect") => engine.slobs_disconnect(id),
        _ => {}
    }
    crate::host::changed();
    Ok(json!({"accepted": true}))
}

pub(super) fn table(engine: &Rc<Engine>, kind: &str) -> Result<Value, String> {
    let config = engine.config();
    let obs = kind == "obs_connection";
    let columns = if obs {
        vec![
            column("autoConnect", "lists.on_start", 72),
            column("name", "lists.obs.name", 170),
            column("state", "lists.state", 140),
            column("address", "lists.obs.address", 170),
            column("reconnect", "lists.reconnect", 140),
            column("secret", "lists.password", 120),
        ]
    } else {
        vec![
            column("autoConnect", "lists.on_start", 72),
            column("name", "lists.slobs.name", 190),
            column("state", "lists.state", 140),
            column("address", "lists.slobs.address", 170),
            column("secret", "lists.slobs.token", 120),
            column("default", "", 100),
        ]
    };
    let items = entities::items(&config, kind).ok_or("unknown connection kind")?;
    let rows = items.iter().map(|item| {
        let connected = if obs { engine.obs_connected(Some(text(item, "id"))) } else { engine.slobs_up(text(item, "id")) };
        let address = format!("{}:{}", text(item, "host"), item.get("port").and_then(Value::as_u64).unwrap_or(if obs { 4455 } else { 59650 }));
        let auto_connect = flag(item, "autoConnect");
        let reconnect = obs && flag(item, "reconnect");
        let retry_interval = item.get("retryInterval").and_then(Value::as_u64).unwrap_or(5);
        let secret = !text(item, if obs { "password" } else { "token" }).is_empty();
        let is_default = !obs && flag(item, "default");
        let brand = if obs { "obsstudio" } else { "streamlabs" };
        let mut cells = vec![json!(auto_connect), json!({"text": text(item, "name"), "showTitle": true, "brand": brand}), json!({"health": if connected { "up" } else { "off" }, "text": tr(if connected { "lists.live.connected" } else { "lists.live.disconnected" })}), json!(address)];
        if obs { cells.push(if reconnect { note("refresh-cw", trf("lists.retry_every", &[("seconds", &retry_interval.to_string())])) } else { note("circle-off", tr("lists.no_retry")) }); }
        cells.push(note(if secret { "lock" } else { "lock-open" }, tr(if secret { "lists.secret_set" } else { "lists.secret_none" })));
        if !obs { cells.push(json!(if is_default { tr("lists.default_badge") } else { "" })); }
        json!({
            "id": text(item, "id"),
            "name": text(item, "name"),
            "brand": brand,
            "connected": connected,
            "autoConnect": auto_connect,
            "address": address,
            "reconnect": reconnect,
            "retryInterval": retry_interval,
            "secret": secret,
            "default": is_default,
            "enabled": true,
            "cells": cells
        })
    }).collect::<Vec<_>>();
    let configured = rows.len();
    let connected = rows.iter().filter(|row| row["connected"] == true).count();
    let status = if configured == 0 {
        tr("lists.stream_apps.not_configured").to_owned()
    } else if connected == 0 {
        tr("lists.stream_apps.disconnected").to_owned()
    } else {
        trf(
            "lists.stream_apps.connected_count",
            &[
                ("connected", &connected.to_string()),
                ("total", &configured.to_string()),
            ],
        )
    };
    Ok(json!({
        "kind": kind,
        "presentation": "connections",
        "brand": if obs { "obsstudio" } else { "streamlabs" },
        "title": if obs { "OBS Studio" } else { "Streamlabs Desktop" },
        "what": entities::title(kind),
        "guide": tr(if obs { "lists.stream_apps.obs_guide" } else { "lists.stream_apps.slobs_guide" }),
        "columns": columns,
        "rows": rows,
        "empty": tr(if obs { "lists.obs.empty" } else { "lists.slobs.empty" }),
        "actions": ["add", "connect", "edit", "delete"],
        "headerAdd": true,
        "switchToggle": true,
        "connection": {
            "health": if connected == 0 { "off" } else if connected == configured { "up" } else { "standby" },
            "text": status,
            "canConnect": connected < configured,
            "canDisconnect": configured > 0
        }
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn connection_reads_and_invalid_commands_do_not_start_integrations_or_leak_secrets() {
        let mut config = crate::Config::default();
        config.obs.connections.push(json!({"id":"local","name":"Local","host":"localhost","port":4455,"password":"private-password","autoConnect":false}).as_object().unwrap().clone());
        let engine = Engine::new(std::sync::Arc::new(tokio::sync::watch::channel(config).0), |_| {});
        let model = table(&engine, "obs_connection").unwrap();
        assert!(!model.to_string().contains("private-password"));
        assert_eq!(model["presentation"], "connections");
        assert_eq!(model["actions"], json!(["add", "connect", "edit", "delete"]));
        assert_eq!(model["headerAdd"], true);
        assert_eq!(model["switchToggle"], true);
        assert_eq!(model["connection"]["canConnect"], true);
        assert_eq!(model["connection"]["canDisconnect"], true);
        assert_eq!(model["rows"][0]["connected"], false);
        assert_eq!(model["rows"][0]["autoConnect"], false);
        assert_eq!(model["rows"][0]["address"], "localhost:4455");
        assert_eq!(model["rows"][0]["secret"], true);
        let slobs = table(&engine, "slobs_connection").unwrap();
        assert_eq!(slobs["presentation"], "connections");
        assert_eq!(slobs["actions"], json!(["add", "connect", "edit", "delete"]));
        assert_eq!(slobs["headerAdd"], true);
        assert_eq!(slobs["switchToggle"], true);
        assert_eq!(slobs["connection"]["canConnect"], false);
        assert_eq!(slobs["connection"]["canDisconnect"], false);
        assert!(apply(&engine, "obs_connection", "missing", "connect").is_err());
        assert!(apply(&engine, "obs_connection", "local", "unknown").is_err());
        assert!(apply(&engine, "invalid", "", "connect").is_err());
        assert!(apply(&engine, "obs_connection", "local", "refresh").is_ok());
        assert!(apply(&engine, "slobs_connection", "", "connect").is_ok());
        assert!(engine.obs().is_none());
    }
}
