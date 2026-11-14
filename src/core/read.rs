//! Core state queries. Values cross adapters without exposing account credentials.

use crate::engine::Engine;
use serde_json::{json, Value};
use std::rc::Rc;

pub const DOMAINS: &[&str] = &[
    "actions",
    "commands",
    "timers",
    "files",
    "queues",
    "groups",
    "globals",
    "community",
    "history",
    "chat",
    "polls",
    "accounts",
    "servers",
    "stream_apps",
    "integrations",
    "donations",
    "plugins",
    "settings",
    "log",
    "crashes",
    "backups",
    "mcp",
];

pub fn read(engine: &Rc<Engine>, domain: &str) -> Result<Value, String> {
    let config = engine.config();
    let data = match domain {
        "actions" => json!(config.actions.actions),
        "commands" => json!(config.commands),
        "timers" => json!(config.timers),
        "files" => json!({"watchers": config.file_watchers, "tails": config.file_tails}),
        "queues" => json!({"queues": config.actions.queues, "pending": engine.pending()}),
        "groups" => json!(config.groups),
        "globals" => json!({"persisted": engine.globals_list(true), "temporary": engine.globals_list(false)}),
        "community" => json!(engine.user_rows()),
        "history" => json!(engine.history().iter().rev().map(|record| json!({
            "actionId": record.action_id, "name": record.name, "queuedAt": record.queued_at,
            "durationMs": record.ms, "args": record.args, "after": record.after, "outcome": record.outcome,
        })).collect::<Vec<_>>()),
        "chat" => {
            let (generation, reset, lines) = engine.chat_sync(0, 0);
            json!({"generation": generation, "reset": reset, "lines": lines})
        }
        "polls" => json!(engine.chat_polls().iter().map(|poll| json!({"platform": poll.platform, "id": poll.id, "title": poll.title, "prediction": poll.prediction, "choices": poll.choices, "ends": poll.ends, "over": poll.over})).collect::<Vec<_>>()),
        "accounts" => json!(crate::engine::chatlog::PLATFORMS.iter().map(|platform| json!({
            "platform": platform, "channel": engine.broadcaster(platform), "connected": engine.chat_connected(platform),
        })).collect::<Vec<_>>()),
        "servers" => json!({
            "websocket": {"address": config.websockets.address, "port": config.websockets.port, "enabled": config.websockets.auto_start, "authentication": config.websockets.enable_auth},
            "http": {"address": config.http.address, "port": config.http.port, "enabled": config.http.auto_start},
            "udp": {"port": config.udp.port, "enabled": config.udp.auto_start},
        }),
        "stream_apps" => json!({"obs": public_connections(&config.obs.connections), "streamlabs": public_connections(&config.slobs.connections)}),
        "integrations" => json!({"spotify": {"enabled": config.spotify.enabled, "localMonitor": config.spotify.local_monitor}}),
        "donations" => json!({"streamElements": engine.donations.borrow().streamelements.get(), "streamlabs": engine.donations.borrow().streamlabs.get()}),
        "plugins" => json!(engine.plugins_on().iter().map(|(id, manifest)| json!({"id": id, "manifest": manifest})).collect::<Vec<_>>()),
        "settings" => json!({"theme": config.theme, "locale": config.locale, "animations": config.animations, "startHidden": config.start_hidden,
            "memorySaver": config.memory_saver, "autostart": config.autostart, "notifications": config.notifications,
            "dateFormat": config.date_format, "timeFormat": config.time_format, "debugLog": config.debug_log}),
        "log" => json!(crate::log_tail_tagged(500)),
        "crashes" => json!(crate::crash::reports().iter().map(|report| json!({
            "path": report.path, "made": report.made, "version": report.version, "what": report.what, "seen": report.seen,
        })).collect::<Vec<_>>()),
        "backups" => json!(crate::backup::list().iter().map(|backup| json!({"name": backup.name(), "info": backup.info, "size": backup.size})).collect::<Vec<_>>()),
        "mcp" => {
            let websocket = &config.websockets;
            let server = crate::agents::Server::this(format!("ws://{}:{}{}", websocket.address, websocket.port, websocket.endpoint));
            json!(crate::agents::found(&crate::agents::Places::here(), &server).iter().map(|agent| json!({
                "id": agent.kind.id, "name": agent.kind.name, "state": format!("{:?}", agent.state).to_lowercase(), "config": agent.file,
                "terminal": agent.cli.is_some(),
            })).collect::<Vec<_>>())
        }
        _ => return Err(format!("unknown core domain {domain}")),
    };
    Ok(json!({"domain": domain, "data": data}))
}

fn public_connections(connections: &[crate::model::Obj]) -> Vec<Value> {
    connections
        .iter()
        .map(|connection| {
            let mut result = serde_json::Map::new();
            for key in [
                "id",
                "name",
                "host",
                "port",
                "autoConnect",
                "reconnect",
                "retryInterval",
                "version",
            ] {
                if let Some(value) = connection.get(key) {
                    result.insert(key.into(), value.clone());
                }
            }
            Value::Object(result)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn connection_reads_use_an_allowlist_and_do_not_return_credentials_or_unknown_fields() {
        let connections = vec![json!({"id": "obs", "host": "localhost", "port": 4455, "password": "private", "token": "private", "futureSecret": "private"}).as_object().unwrap().clone()];
        assert_eq!(
            public_connections(&connections),
            vec![json!({"id": "obs", "host": "localhost", "port": 4455})]
        );
    }
}
