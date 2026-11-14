//! Semantic list models. Presentation adapters choose how to draw cells, not what the values mean.

use crate::engine::Engine;
use crate::i18n::{tr, trf};
use crate::model::Obj;
use serde_json::{json, Value};
use std::rc::Rc;

fn number(item: &Obj, key: &str) -> u64 {
    item.get(key).and_then(Value::as_u64).unwrap_or(0)
}
fn signed(item: &Obj, key: &str) -> i64 {
    item.get(key).and_then(Value::as_i64).unwrap_or(0)
}
pub(super) fn text<'a>(item: &'a Obj, key: &str) -> &'a str {
    item.get(key).and_then(Value::as_str).unwrap_or_default()
}
pub(super) fn flag(item: &Obj, key: &str) -> bool {
    item.get(key).and_then(Value::as_bool).unwrap_or(false)
}
pub(super) fn note(icon: &str, text: impl Into<String>) -> Value {
    json!({"icon": icon, "text": text.into()})
}
pub(super) fn column(key: &str, label: &'static str, width: u32) -> Value {
    json!({"key": key, "label": tr(label), "width": width})
}

fn audience(item: &Obj) -> Value {
    let groups = item
        .get("permittedGroups")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .collect::<Vec<_>>()
        .join(", ");
    let users = item.get("permittedUsers").and_then(Value::as_array).map_or(0, Vec::len);
    let mut who = groups;
    if users > 0 {
        let people = trf("lists.commands.people", &[("count", &users.to_string())]);
        who = if who.is_empty() {
            people
        } else {
            format!("{who}, {people}")
        };
    }
    if who.is_empty() {
        note("users", tr("lists.commands.everyone"))
    } else if number(item, "grantType") == 1 {
        note("shield-off", trf("lists.commands.all_but", &[("who", &who)]))
    } else {
        note("shield", who)
    }
}

/// Whether the core has a list model for this page; others are read as raw domains.
pub fn supports(domain: &str) -> bool {
    matches!(
        domain,
        "commands" | "timers" | "files" | "stream_apps" | "servers" | "log"
    ) || super::accounts::supports(domain)
        || super::actions::supports(domain)
        || super::chat::supports(domain)
        || super::pages::supports(domain)
        || super::overlays::supports(domain)
        || super::polls::supports(domain)
        || super::stream::supports(domain)
        || super::sections::supports(domain)
}

/// The crash report's trail: the page the frontend opened last, noted when it changes.
fn note_page(domain: &str) {
    // Only the sidebar's pages count: a dialog's or a popup's read says nothing about the page behind it.
    let is_page = super::navigation::NAV
        .iter()
        .flat_map(|(_, _, pages)| pages.iter())
        .any(|(id, _, _)| id.to_lowercase().replace(' ', "_") == domain);
    if !is_page {
        return;
    }
    crate::host::set_viewers_visible(domain == "chat");
    thread_local! {
        static SHOWN: std::cell::RefCell<String> = const { std::cell::RefCell::new(String::new()) };
    }
    // Opening the Stream page reads the platforms again (re-reads while it stays open don't).
    if SHOWN.with_borrow_mut(|shown| std::mem::replace(shown, domain.to_owned()) != domain) && domain == "stream" {
        super::stream::opened();
    }
    #[cfg(not(test))]
    {
        static LAST: std::sync::Mutex<String> = std::sync::Mutex::new(String::new());
        let Ok(mut last) = LAST.lock() else { return };
        if *last != domain {
            *last = domain.to_owned();
            crate::crash::step(&format!("opened page {domain}"));
            crate::crash::fact("Page", domain);
        }
    }
}

/// A request's own filters (the log's level and search) on top of the domain.
pub fn read_request(engine: &Rc<Engine>, request: &Value) -> Result<Value, String> {
    let domain = request["domain"].as_str().unwrap_or_default();
    note_page(domain);
    if let Some((page_id, _, _)) = super::navigation::NAV
        .iter()
        .flat_map(|(_, _, pages)| pages.iter())
        .find(|(id, _, _)| id.to_lowercase().replace(' ', "_") == domain)
    {
        if engine.config().window.tab != *page_id {
            engine.change(|config| config.window.tab = page_id.to_string());
        }
    }
    if domain == "log" {
        return Ok(super::log::read(
            request["level"].as_u64().unwrap_or(1) as usize,
            request["search"].as_str().unwrap_or_default(),
        ));
    }
    if domain == "chat" {
        return super::chat::read(engine, request);
    }
    if super::actions::supports(domain) {
        return super::actions::read(engine, request);
    }
    if domain == "group_members" {
        return super::pages::group_members(engine, request["itemId"].as_str().unwrap_or_default());
    }
    if domain == "user_card" {
        return super::pages::user_card(engine, request["itemId"].as_str().unwrap_or_default());
    }
    if super::pages::supports(domain) {
        return super::pages::read_with(engine, domain, &request["view"]);
    }
    read(engine, domain)
}

pub fn read(engine: &Rc<Engine>, domain: &str) -> Result<Value, String> {
    if super::overlays::supports(domain) {
        return super::overlays::read(engine, domain);
    }
    if super::polls::supports(domain) {
        return super::polls::read(engine);
    }
    if super::stream::supports(domain) {
        return super::stream::read(engine);
    }
    if super::chat::supports(domain) {
        return super::chat::read(engine, &serde_json::json!({}));
    }
    if super::accounts::supports(domain) {
        return super::accounts::read(engine, domain);
    }
    if super::pages::supports(domain) {
        return super::pages::read(engine, domain);
    }
    if super::sections::supports(domain) {
        return super::sections::read(engine, domain);
    }
    let kinds: &[&str] = match domain {
        "commands" => &["command"],
        "timers" => &["timer"],
        "files" => &["file_watcher", "file_tail"],
        "stream_apps" => &["obs_connection", "slobs_connection"],
        "servers" => &["server_settings", "websocket_client", "websocket_server"],
        _ => return Err(format!("no list model for {domain}")),
    };
    let tables = kinds
        .iter()
        .map(|kind| table(engine, kind))
        .collect::<Result<Vec<_>, _>>()?;
    Ok(json!({"domain": domain, "tables": tables}))
}

fn found(path: &str) -> Value {
    let (health, key) = if path.trim().is_empty() {
        ("off", "lists.not_set")
    } else if std::path::Path::new(path.trim()).exists() {
        ("up", "lists.found")
    } else {
        ("down", "lists.missing")
    };
    json!({"health": health, "text": tr(key)})
}

fn table(engine: &Rc<Engine>, kind: &str) -> Result<Value, String> {
    if kind == "server_settings" {
        return Ok(super::settings::model(&engine.config()));
    }
    if matches!(kind, "obs_connection" | "slobs_connection") {
        return super::connections::table(engine, kind);
    }
    let config = engine.config();
    let (empty, title, guide, columns, rows) = match kind {
        "command" => {
            let columns = vec![
                column("enabled", "lists.on", 52),
                column("name", "lists.commands.name", 160),
                column("command", "lists.commands.commands", 200),
                column("who", "lists.commands.who", 140),
                column("sources", "lists.commands.sources", 100),
                column("cooldown", "lists.commands.cooldown", 120),
                column("group", "lists.commands.group", 120),
            ];
            let rows = config.commands.iter().map(|item| {
                let sources = number(item, "sources");
                let mut platforms = Vec::new();
                for (source, bit) in crate::automation::entities::SOURCES {
                    let platform = source.split(' ').next().unwrap_or_default().to_lowercase();
                    if sources & bit != 0 && !platforms.contains(&platform) { platforms.push(platform); }
                }
                let cooldown = match (number(item, "globalCooldown"), number(item, "userCooldown")) {
                    (0, 0) => String::new(),
                    (all, 0) => trf("lists.commands.cooldown_all", &[("seconds", &all.to_string())]),
                    (0, each) => trf("lists.commands.cooldown_each", &[("seconds", &each.to_string())]),
                    (all, each) => trf("lists.commands.cooldown_both", &[("all", &all.to_string()), ("each", &each.to_string())]),
                };
                json!({"id": text(item, "id"), "name": text(item, "name"), "enabled": flag(item, "enabled"), "cells": [flag(item, "enabled"), text(item, "name"), crate::commands::aliases(item).join(", "), audience(item), {"platforms": platforms}, note("timer", cooldown), note("folder", text(item, "group"))]})
            }).collect::<Vec<_>>();
            ("lists.commands.empty", "lists.commands.what", "", columns, rows)
        }
        "timer" => {
            let columns = vec![
                column("enabled", "lists.on", 52),
                column("name", "lists.timers.name", 200),
                column("interval", "lists.timers.interval", 160),
                column("repeat", "lists.timers.repeat", 120),
                column("lines", "lists.timers.lines", 120),
                column("counter", "lists.timers.runs", 100),
            ];
            let rows = config.timers.iter().map(|item| {
                let random = flag(item, "randomInterval");
                let interval = signed(item, "interval").to_string();
                let every = if random { trf("lists.timers.every_random", &[("min", &interval), ("max", &signed(item, "upperInterval").to_string())]) } else { trf("lists.timers.every", &[("seconds", &interval)]) };
                let lines = signed(item, "lines");
                json!({"id": text(item, "id"), "name": text(item, "name"), "enabled": flag(item, "enabled"), "cells": [flag(item, "enabled"), text(item, "name"), note(if random { "shuffle" } else { "timer" }, every), note(if flag(item, "repeat") { "repeat" } else { "play" }, tr(if flag(item, "repeat") { "lists.timers.repeats" } else { "lists.timers.once" })), note("message-square", if lines > 0 { trf("lists.timers.lines_count", &[("count", &lines.to_string())]) } else { String::new() }), note("hash", trf("lists.timers.ran_count", &[("count", &signed(item, "counter").to_string())]))]})
            }).collect::<Vec<_>>();
            ("lists.timers.empty", "lists.timers.what", "", columns, rows)
        }
        "file_watcher" => {
            let columns = vec![
                column("enabled", "lists.on", 52),
                column("name", "lists.watchers.name", 180),
                column("state", "lists.state", 120),
                column("folderPath", "lists.watchers.folder", 300),
                column("filter", "lists.watchers.filter", 100),
                column("subfolders", "lists.watchers.subfolders", 140),
            ];
            let rows = config.file_watchers.iter().map(|item| {
                let path = text(item, "folderPath");
                let state = found(path);
                let filter = text(item, "filter");
                let subfolders = flag(item, "includeSubdirectories");
                let mut badges = Vec::new();
                if !filter.is_empty() {
                    badges.push(json!({"icon": "filter", "text": filter}));
                }
                if subfolders {
                    badges.push(json!({"icon": "folder-tree", "text": "", "tip": tr("lists.watchers.with_subfolders")}));
                }
                json!({
                    "id": text(item, "id"),
                    "name": text(item, "name"),
                    "enabled": flag(item, "enabled"),
                    "health": state["health"],
                    "healthTip": state["text"],
                    "detail": path,
                    "badges": badges,
                    "cells": [flag(item, "enabled"), text(item, "name"), state, path, filter, note("folder-tree", if subfolders { tr("lists.watchers.with_subfolders") } else { "" })]
                })
            }).collect();
            (
                "lists.watchers.empty",
                "lists.watchers.title",
                "lists.watchers.guide",
                columns,
                rows,
            )
        }
        "file_tail" => {
            let columns = vec![
                column("enabled", "lists.on", 52),
                column("name", "lists.tails.name", 200),
                column("state", "lists.state", 120),
                column("filePath", "lists.tails.file", 400),
            ];
            let rows = config
                .file_tails
                .iter()
                .map(|item| {
                    let path = text(item, "filePath");
                    let state = found(path);
                    json!({
                        "id": text(item, "id"),
                        "name": text(item, "name"),
                        "enabled": flag(item, "enabled"),
                        "health": state["health"],
                        "healthTip": state["text"],
                        "detail": path,
                        "cells": [flag(item, "enabled"), text(item, "name"), state, path]
                    })
                })
                .collect();
            (
                "lists.tails.empty",
                "lists.tails.title",
                "lists.tails.guide",
                columns,
                rows,
            )
        }
        "websocket_client" => {
            let columns = vec![
                column("autoConnect", "lists.on_start", 72),
                column("name", "lists.clients.name", 200),
                column("endpoint", "lists.clients.endpoint", 320),
                column("retry", "lists.reconnect", 140),
            ];
            let rows = config.websockets.connections.iter().map(|item| {
                let retry_on = flag(item, "reconnectOnDisconnect");
                let retry_secs = item.get("retryInterval").and_then(Value::as_u64).unwrap_or(5);
                let retry = if retry_on {
                    note("refresh-cw", trf("lists.retry_every", &[("seconds", &retry_secs.to_string())]))
                } else {
                    note("circle-off", tr("lists.no_retry"))
                };
                let retry_badge = if retry_on {
                    json!({"icon": "refresh-cw", "text": format!("{retry_secs}s"), "tip": trf("lists.retry_every", &[("seconds", &retry_secs.to_string())])})
                } else {
                    json!({"icon": "circle-off", "text": "", "tip": tr("lists.no_retry")})
                };
                json!({
                    "id": text(item, "id"),
                    "name": text(item, "name"),
                    "enabled": flag(item, "autoConnect"),
                    "switchOnly": true,
                    "icon": "network",
                    "detail": text(item, "endpoint"),
                    "badges": [retry_badge],
                    "cells": [flag(item, "autoConnect"), text(item, "name"), text(item, "endpoint"), retry]
                })
            }).collect();
            (
                "lists.clients.empty",
                "lists.servers.ws_clients_title",
                "",
                columns,
                rows,
            )
        }
        "websocket_server" => {
            let columns = vec![
                column("autoStart", "lists.on_start", 72),
                column("name", "lists.custom_servers.name", 200),
                column("address", "lists.custom_servers.address", 400),
            ];
            let rows = config
                .websockets
                .servers
                .iter()
                .map(|item| {
                    let address = format!(
                        "ws://{}:{}{}",
                        text(item, "address"),
                        number(item, "port"),
                        text(item, "endpoint")
                    );
                    json!({
                        "id": text(item, "id"),
                        "name": text(item, "name"),
                        "enabled": flag(item, "autoStart"),
                        "icon": "server",
                        "detail": address,
                        "cells": [flag(item, "autoStart"), text(item, "name"), address]
                    })
                })
                .collect();
            (
                "lists.custom_servers.empty",
                "lists.servers.ws_servers_title",
                "",
                columns,
                rows,
            )
        }
        _ => return Err(format!("no list model for {kind}")),
    };
    let what = crate::automation::entities::title(kind);
    // The icon beside a section's heading, as the production page draws it.
    let icon = match kind {
        "file_watcher" => "folder-open",
        "file_tail" => "scroll-text",
        "websocket_client" => "network",
        "websocket_server" => "server",
        _ => "",
    };
    let cards = matches!(
        kind,
        "file_watcher" | "file_tail" | "websocket_client" | "websocket_server"
    );
    let mut model = json!({
        "kind": kind,
        "title": tr(title),
        "what": what,
        "guide": if guide.is_empty() { "" } else { tr(guide) },
        "columns": columns,
        "rows": rows,
        "empty": tr(empty),
        "search": matches!(kind, "command" | "timer"),
        "showTitle": cards,
        "actions": if kind == "websocket_client" { vec!["add","edit","delete"] } else { vec!["add","edit","toggle","delete"] }
    });
    if cards {
        model["presentation"] = "cards".into();
        model["headerAdd"] = true.into();
    }
    if matches!(kind, "command" | "timer") {
        model["addTop"] = true.into();
        model["strip"] = true.into();
    }
    if !icon.is_empty() {
        model["icon"] = icon.into();
    }
    Ok(model)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn viewer_counts_are_polled_only_while_the_chat_page_is_open() {
        let _guard = crate::TEST_LOCK.lock().unwrap_or_else(|error| error.into_inner());
        note_page("chat");
        assert!(crate::host::viewers_visible());
        // A dialog's or a popup's read doesn't leave the page.
        note_page("user_card");
        assert!(crate::host::viewers_visible());
        note_page("timers");
        assert!(!crate::host::viewers_visible());
        note_page("chat");
        crate::host::detach();
        assert!(!crate::host::viewers_visible());
    }

    #[test]
    fn list_models_keep_platform_masks_signed_intervals_and_file_health() {
        let config = crate::Config {
            commands: vec![json!({"id":"c","name":"command","enabled":true,"sources":6292481})
                .as_object()
                .unwrap()
                .clone()],
            timers: vec![
                json!({"id":"t","interval":-3,"upperInterval":9,"randomInterval":true,"counter":-2})
                    .as_object()
                    .unwrap()
                    .clone(),
            ],
            file_watchers: vec![json!({"id":"w","folderPath":""}).as_object().unwrap().clone()],
            file_tails: vec![json!({"id":"f","filePath":file!()}).as_object().unwrap().clone()],
            ..Default::default()
        };
        let engine = Engine::new(std::sync::Arc::new(tokio::sync::watch::channel(config).0), |_| {});
        let commands = read(&engine, "commands").unwrap();
        assert_eq!(
            commands["tables"][0]["rows"][0]["cells"][4]["platforms"],
            json!(["twitch", "youtube", "kick", "tiktok"])
        );
        let timers = read(&engine, "timers").unwrap();
        assert!(timers["tables"][0]["rows"][0]["cells"][2]["text"]
            .as_str()
            .unwrap()
            .contains("-3"));
        let files = read(&engine, "files").unwrap();
        assert_eq!(files["tables"][0]["rows"][0]["cells"][2]["health"], "off");
        assert_eq!(files["tables"][1]["rows"][0]["cells"][2]["health"], "up");
        // Each section heading carries the icon the production page draws.
        assert_eq!(files["tables"][0]["icon"], "folder-open");
        assert_eq!(files["tables"][1]["icon"], "scroll-text");
        assert_eq!(read(&engine, "servers").unwrap()["tables"][0]["brand"], "streamerbot");
        assert_eq!(read(&engine, "servers").unwrap()["tables"][1]["icon"], "network");
        assert_eq!(read(&engine, "servers").unwrap()["tables"][2]["icon"], "server");
        assert!(!read(&engine, "servers").unwrap()["tables"][1]["actions"]
            .as_array()
            .unwrap()
            .contains(&json!("toggle")));
        assert!(read(&engine, "servers").unwrap()["tables"][2]["actions"]
            .as_array()
            .unwrap()
            .contains(&json!("toggle")));
    }

    #[test]
    fn reading_a_page_request_records_the_tab_in_window_state() {
        let _guard = crate::TEST_LOCK.lock().unwrap_or_else(|error| error.into_inner());
        crate::engine::tests::run(async {
            let engine = crate::engine::tests::engine(json!({"actions": []}));
            assert_eq!(engine.config().window.tab, "");

            let _ = read_request(&engine, &json!({"domain": "settings"})).unwrap();
            assert_eq!(engine.config().window.tab, "Settings");

            let _ = read_request(&engine, &json!({"domain": "chat"})).unwrap();
            assert_eq!(engine.config().window.tab, "Chat");

            // Non-top-level views (dialogs, member popups) do not change the active tab
            let _ = read_request(&engine, &json!({"domain": "group_members", "itemId": "test"}));
            assert_eq!(engine.config().window.tab, "Chat");
        });
    }
}
