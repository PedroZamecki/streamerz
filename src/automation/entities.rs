//! Entity fields and defaults shared by all frontend adapters and MCP.

use super::schema::{F, W::*};
use serde_json::{json, Value};

pub const SOURCES: &[(&str, u64)] = &[
    ("Twitch message", 1),
    ("Twitch whisper", 2),
    ("Twitch re-sub message", 8),
    ("YouTube message", 1024),
    ("Kick message", 2_097_152),
    ("TikTok message", 4_194_304),
];

pub const COMMANDS_FIELDS: &[F] = &[
    ("command", "automation.command.commands", Multi),
    ("response", "automation.command.reply", Multi),
    ("replyAll", "automation.command.reply_all", Bool),
    ("sources", "automation.command.twitch_message", Bit(SOURCES[0].1)),
    ("sources", "automation.command.twitch_whisper", Bit(SOURCES[1].1)),
    ("sources", "automation.command.twitch_resub", Bit(SOURCES[2].1)),
    ("sources", "automation.command.youtube_message", Bit(SOURCES[3].1)),
    ("sources", "automation.command.kick_message", Bit(SOURCES[4].1)),
    ("sources", "automation.command.tiktok_message", Bit(SOURCES[5].1)),
    ("globalCooldown", "automation.command.global_cooldown", Duration(1000)),
    ("userCooldown", "automation.command.user_cooldown", Duration(1000)),
    (
        "grantType",
        "automation.command.grant",
        Pick(&["automation.command.allow", "automation.command.deny"]),
    ),
    ("permittedGroups", "automation.command.user_groups", Lines),
    (
        "mode",
        "automation.command.mode",
        Pick(&["automation.command.basic", "automation.command.regex"]),
    ),
    (
        "location",
        "automation.command.location",
        Pick(&[
            "automation.command.start",
            "automation.command.exact",
            "automation.command.anywhere",
        ]),
    ),
    ("caseSensitive", "automation.command.case_sensitive", Bool),
    ("ignoreBotAccount", "automation.command.ignore_bot", Bool),
    ("ignoreInternal", "automation.command.ignore_internal", Bool),
    ("name", "automation.command.name", Text),
    ("enabled", "automation.command.enabled", Bool),
    ("include", "automation.command.include", Bool),
    ("group", "automation.command.category", CommandGroup),
    ("persistCounter", "automation.command.counter", Bool),
    ("persistUserCounter", "automation.command.user_counter", Bool),
];

pub fn command_default() -> Value {
    json!({"permittedUsers": [], "permittedGroups": [], "id": "", "name": "", "enabled": true,
            "include": true, "mode": 0, "command": "", "regexExplicitCapture": false, "location": 0,
            "ignoreBotAccount": true, "ignoreInternal": true, "sources": 6292481, "persistCounter": false,
            "persistUserCounter": false, "caseSensitive": false, "globalCooldown": 0, "userCooldown": 0,
            "group": "", "grantType": 0, "response": "", "replyAll": false})
}

pub const TIMERS_FIELDS: &[F] = &[
    ("name", "automation.timer_name", Text),
    ("enabled", "automation.timer_enable", Bool),
    ("repeat", "automation.timer_repeat", Bool),
    ("interval", "automation.timer_interval", Duration(1000)),
    ("randomInterval", "automation.timer_random", Bool),
    ("upperInterval", "automation.timer_upper", Duration(1000)),
    ("lines", "automation.timer_lines", Int),
];

pub fn timer_default() -> Value {
    json!({"id": "", "name": crate::i18n::tr("automation.timer_new_name"), "enabled": false, "repeat": true, "interval": 60,
            "randomInterval": false, "upperInterval": 0, "lines": 0, "counter": 0})
}

pub const FILE_WATCHERS_FIELDS: &[F] = &[
    ("name", "Name", Text),
    ("enabled", "Enabled", Bool),
    ("folderPath", "Folder (or a file)", Text),
    ("filter", "Filter (*.txt, *.*)", Text),
    ("includeSubdirectories", "Include subfolders", Bool),
    ("addChangedData", "Add the file's lines to Changed", Bool),
    ("asJson", "Read the file as JSON on Changed", Bool),
];

pub fn file_watcher_default() -> Value {
    json!({"id": "", "name": "New watcher", "enabled": true, "folderPath": "", "filter": "*.*",
            "includeSubdirectories": false, "addChangedData": false, "asJson": false})
}

pub const FILE_TAILS_FIELDS: &[F] = &[
    ("name", "Name", Text),
    ("enabled", "Enabled", Bool),
    ("filePath", "File", Text),
];

pub fn file_tail_default() -> Value {
    json!({"id": "", "name": "New file tail", "enabled": true, "filePath": ""})
}

pub const CLIENTS_FIELDS: &[F] = &[
    ("name", "Name", Text),
    ("endpoint", "Endpoint (ws://...)", Text),
    ("autoConnect", "Connect on start", Bool),
    ("reconnectOnDisconnect", "Reconnect", Bool),
    ("retryInterval", "Retry interval (s)", Int),
];

pub fn websocket_client_default() -> Value {
    json!({"id": "", "name": "New client", "endpoint": "ws://127.0.0.1:8080/", "autoConnect": false,
            "reconnectOnDisconnect": true, "retryInterval": 5})
}

pub const OBS_CONNECTIONS_FIELDS: &[F] = &[
    ("name", "Name", Text),
    ("host", "Host", Text),
    ("port", "Port", Int),
    ("password", "Password", Secret),
    ("autoConnect", "Connect on start", Bool),
    ("reconnect", "Reconnect", Bool),
    ("retryInterval", "Retry interval (s)", Int),
];

pub fn obs_connection_default() -> Value {
    json!({
        "id": "",
        "name": "Local",
        "host": "127.0.0.1",
        "port": 4455,
        "password": "",
        "autoConnect": true,
        "reconnect": true,
        "retryInterval": 5,
        "version": 5,
    })
}

pub const SLOBS_CONNECTIONS_FIELDS: &[F] = &[
    ("name", "Name", Text),
    ("host", "Host", Text),
    ("port", "Port", Int),
    ("token", "Token", Secret),
    ("autoConnect", "Connect on start", Bool),
    ("reconnectOnDisconnect", "Reconnect", Bool),
    ("retryInterval", "Retry interval (s)", Int),
    ("default", "Default connection", Bool),
];

pub fn slobs_connection_default() -> Value {
    json!({"id": "", "name": "Local", "host": "127.0.0.1", "port": 59650, "autoConnect": true,
            "reconnectOnDisconnect": true, "token": "", "retryInterval": 30, "default": false})
}

pub const SERVERS_FIELDS: &[F] = &[
    ("name", "Name", Text),
    ("address", "Address", Text),
    ("port", "Port", Int),
    ("endpoint", "Endpoint", Text),
    ("autoStart", "Start on launch", Bool),
];

pub fn websocket_server_default() -> Value {
    json!({"id": "", "name": "New server", "address": "127.0.0.1", "port": 9090, "endpoint": "/",
            "autoStart": false})
}

pub const QUEUE_FIELDS: &[F] = &[("name", "Name", Text), ("blocking", "Blocking", Bool)];

pub fn fields(kind: &str) -> Option<&'static [F]> {
    Some(match kind {
        "queue" => QUEUE_FIELDS,
        "user_group" => crate::core::pages::USER_GROUP_FIELDS,
        "quote" => crate::core::pages::QUOTE_FIELDS,
        "global" => crate::core::pages::GLOBAL_FIELDS,
        "command" => COMMANDS_FIELDS,
        "timer" => TIMERS_FIELDS,
        "file_watcher" => FILE_WATCHERS_FIELDS,
        "file_tail" => FILE_TAILS_FIELDS,
        "websocket_client" => CLIENTS_FIELDS,
        "obs_connection" => OBS_CONNECTIONS_FIELDS,
        "slobs_connection" => SLOBS_CONNECTIONS_FIELDS,
        "websocket_server" => SERVERS_FIELDS,
        _ => return crate::core::settings::fields(kind),
    })
}

pub fn title(kind: &str) -> &'static str {
    crate::i18n::tr(match kind {
        "queue" => "lists.queues.queue_title",
        "user_group" => "lists.groups.new_group",
        "quote" => "lists.community.quote",
        "global" => "lists.globals.global_title",
        "command" => "lists.commands.what",
        "timer" => "lists.timers.what",
        "file_watcher" => "lists.watchers.what",
        "file_tail" => "lists.tails.what",
        "obs_connection" => "lists.obs.what",
        "slobs_connection" => "lists.slobs.what",
        "websocket_client" => "lists.clients.what",
        "websocket_server" => "lists.custom_servers.what",
        _ => return crate::core::settings::title(kind),
    })
}

pub fn default(kind: &str) -> Option<Value> {
    Some(match kind {
        "queue" => json!({"id": "", "name": crate::i18n::tr("lists.queues.queue_title"), "blocking": false}),
        "user_group" => json!({"id": "", "name": crate::i18n::tr("lists.groups.new_group")}),
        "quote" => json!({"id": "", "quote": "", "user": "", "gameName": ""}),
        "global" => json!({"variableName": "", "value": "", "autoType": false, "persisted": true, "id": ""}),
        "command" => command_default(),
        "timer" => timer_default(),
        "file_watcher" => file_watcher_default(),
        "file_tail" => file_tail_default(),
        "websocket_client" => websocket_client_default(),
        "obs_connection" => obs_connection_default(),
        "slobs_connection" => slobs_connection_default(),
        "websocket_server" => websocket_server_default(),
        _ => return crate::core::settings::read(&crate::Config::default(), kind),
    })
}

pub fn describe(kind: &str) -> Result<Value, String> {
    let fields = fields(kind).ok_or_else(|| format!("unknown entity kind {kind}"))?;
    Ok(
        json!({"kind": kind, "title": title(kind), "new": default(kind), "fields": fields.iter().map(|(key, label, widget)| {
        let mut value = json!({"key": key, "label": super::schema::field_label(label), "widget": widget, "holds": widget.about()});
        if let Pick(options) = widget { value["widget"]["options"] = json!(options.iter().map(|option| super::schema::field_label(option)).collect::<Vec<_>>()); }
        value
    }).collect::<Vec<_>>()}),
    )
}

/// Prepare an unsaved draft or a JSON replacement using the same validation as ordinary editing.
pub fn draft(config: &crate::Config, kind: &str, item_id: &str, request: &Value) -> Result<crate::model::Obj, String> {
    draft_of(form(config, kind, item_id)?, kind, request)
}

/// The same, for an entity whose current values don't live in the config (a global).
pub fn draft_of(form: Value, kind: &str, request: &Value) -> Result<crate::model::Obj, String> {
    if kind == "timer" {
        return crate::core::timers::draft(form, request);
    }
    let new = form["item"]["id"].as_str().unwrap_or_default().is_empty();
    let mut draft = draft_with(form, fields(kind).ok_or("unknown entity kind")?, request)?;
    if kind == "command" && new {
        let command = draft.get("command").and_then(Value::as_str).unwrap_or_default().trim();
        if command.is_empty() {
            return Err(crate::i18n::tr("automation.command.required").into());
        }
        if draft
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .trim()
            .is_empty()
        {
            let name = command.lines().next().unwrap_or_default().trim().to_owned();
            draft.insert("name".into(), name.into());
        }
    }
    Ok(draft)
}

/// The same, with the fields given (a plugin's settings are not a fixed kind).
pub fn draft_with(form: Value, fields: &'static [F], request: &Value) -> Result<crate::model::Obj, String> {
    let original = form["item"].as_object().ok_or("entity must be an object")?;
    if let Some(text) = request["json"].as_str() {
        let value: Value = serde_json::from_str(text)
            .map_err(|error| crate::i18n::trf("dialogs.not_valid_json", &[("error", &error.to_string())]))?;
        let object = value
            .as_object()
            .ok_or_else(|| crate::i18n::tr("dialogs.object_required").to_owned())?;
        if object.get("id").and_then(Value::as_str).unwrap_or_default()
            != original.get("id").and_then(Value::as_str).unwrap_or_default()
        {
            return Err(crate::i18n::tr("forms.keep_id").into());
        }
        return Ok(object.clone());
    }
    super::edits::apply(
        fields,
        original,
        request["changes"].as_array().ok_or("changes: an array")?,
    )
}

pub fn items<'a>(config: &'a crate::Config, kind: &str) -> Option<&'a Vec<crate::model::Obj>> {
    Some(match kind {
        "queue" => &config.actions.queues,
        "command" => &config.commands,
        "timer" => &config.timers,
        "file_watcher" => &config.file_watchers,
        "file_tail" => &config.file_tails,
        "websocket_client" => &config.websockets.connections,
        "websocket_server" => &config.websockets.servers,
        "obs_connection" => &config.obs.connections,
        "slobs_connection" => &config.slobs.connections,
        _ => return None,
    })
}

pub fn items_mut<'a>(config: &'a mut crate::Config, kind: &str) -> Option<&'a mut Vec<crate::model::Obj>> {
    Some(match kind {
        "queue" => &mut config.actions.queues,
        "command" => &mut config.commands,
        "timer" => &mut config.timers,
        "file_watcher" => &mut config.file_watchers,
        "file_tail" => &mut config.file_tails,
        "websocket_client" => &mut config.websockets.connections,
        "websocket_server" => &mut config.websockets.servers,
        "obs_connection" => &mut config.obs.connections,
        "slobs_connection" => &mut config.slobs.connections,
        _ => return None,
    })
}

/// A form's current values are read from the core, not reconstructed by its frontend.
pub fn form(config: &crate::Config, kind: &str, item_id: &str) -> Result<Value, String> {
    let item = if crate::core::settings::fields(kind).is_some() {
        crate::core::settings::read(config, kind).ok_or("unknown settings kind")?
    } else if item_id.is_empty() {
        default(kind).ok_or_else(|| format!("unknown entity kind {kind}"))?
    } else {
        items(config, kind)
            .and_then(|items| {
                items
                    .iter()
                    .find(|item| item.get("id").and_then(Value::as_str) == Some(item_id))
            })
            .cloned()
            .map(Value::Object)
            .ok_or_else(|| format!("no {kind} {item_id}"))?
    };
    let mut form = form_of(kind, item)?;
    if kind == "command" {
        let index = COMMANDS_FIELDS
            .iter()
            .position(|(key, _, _)| *key == "permittedGroups")
            .unwrap();
        let mut names: Vec<(String, String)> = [
            ("Moderators", "chat.card_moderator"),
            ("VIPs", "chat.card_vip"),
            ("Subscribers", "chat.card_subscriber"),
        ]
        .into_iter()
        .map(|(id, label)| (id.into(), crate::i18n::tr(label).into()))
        .collect();
        for name in config
            .groups
            .iter()
            .filter_map(|group| group.get("name").and_then(Value::as_str))
            .chain(
                form["item"]["permittedGroups"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(Value::as_str),
            )
        {
            if !names.iter().any(|(id, _)| id == name) {
                names.push((name.into(), name.into()));
            }
        }
        form["fields"][index]["widget"] = json!({"kind":"checks","options":names.iter().map(|(id,label)| json!({"id":id,"label":label})).collect::<Vec<_>>()});
        form["fields"][index]["value"] = form["item"]["permittedGroups"]
            .as_array()
            .cloned()
            .unwrap_or_default()
            .into();
        form["fields"][index]["holds"] = "user group names".into();
    }
    Ok(if kind == "timer" {
        crate::core::timers::decorate(config, form)
    } else {
        form
    })
}

/// A form for `kind` showing `item`'s values.
pub fn form_of(kind: &str, item: Value) -> Result<Value, String> {
    let mut form = describe(kind)?;
    let object = item.as_object().ok_or("entity must be an object")?;
    for (index, (key, _, widget)) in fields(kind).unwrap_or_default().iter().enumerate() {
        form["fields"][index]["value"] = super::edits::shown(widget, object.get(*key));
    }
    form["item"] = item;
    if kind == "command" {
        decorate_command(&mut form);
    }
    Ok(form)
}

/// Shared presentation metadata; field indices always refer to the schema.
fn decorate_command(form: &mut Value) {
    use crate::i18n::tr;
    let new = form["item"]["id"].as_str().unwrap_or_default().is_empty();
    form["okLabel"] = tr(if new { "automation.command.add" } else { "btn.save" }).into();
    form["focusField"] = json!(0);
    let defaults = command_default();
    for (key, end_key, title, collapsed) in [
        ("sources", "globalCooldown", "automation.command.platforms", false),
        ("globalCooldown", "mode", "automation.command.permissions", true),
        ("mode", "name", "automation.command.matching", true),
        ("name", "", "automation.command.more", true),
    ] {
        let start = COMMANDS_FIELDS.iter().position(|(field, _, _)| *field == key).unwrap();
        let end = COMMANDS_FIELDS
            .iter()
            .position(|(field, _, _)| *field == end_key)
            .unwrap_or(COMMANDS_FIELDS.len());
        let customized = COMMANDS_FIELDS[start..end].iter().any(|(field, _, _)| {
            form["item"]
                .get(*field)
                .is_some_and(|value| defaults.get(*field) != Some(value))
        });
        form["fields"][start]["section"] = tr(title).into();
        form["fields"][start]["collapsed"] = json!(collapsed && !customized);
    }
    form["fields"][0]["placeholder"] = "!hello".into();
    let mode = COMMANDS_FIELDS.iter().position(|(key, _, _)| *key == "mode").unwrap();
    let location = COMMANDS_FIELDS
        .iter()
        .position(|(key, _, _)| *key == "location")
        .unwrap();
    form["fields"][location]["showIf"] = json!({"field": mode, "equals": 0});
    for (index, (_, _, widget)) in COMMANDS_FIELDS.iter().enumerate() {
        if let Bit(bit) = widget {
            form["fields"][index]["brand"] = match bit {
                1 | 2 | 8 => "twitch",
                1024 => "youtube",
                2_097_152 => "kick",
                _ => "tiktok",
            }
            .into();
        }
    }
}

/// Explicit draft and activation actions use the same validated entity changes as MCP.
pub fn activation_buttons(form: &mut Value) {
    let Some(index) = form["fields"].as_array().and_then(|fields| {
        fields
            .iter()
            .position(|field| matches!(field["key"].as_str(), Some("enabled" | "enable")))
    }) else {
        return;
    };
    form["fields"][index]["hidden"] = true.into();
    form["okLabel"] = crate::i18n::tr("automation.create_enable").into();
    form["commitChanges"] = json!([{"field":index,"value":true}]);
    form["alternateSubmit"] =
        json!({"label":crate::i18n::tr("automation.save_draft"),"changes":[{"field":index,"value":false}]});
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn json_drafts_preserve_unknown_data_and_cannot_replace_another_identity() {
        let original = json!({"id":"one","name":"Original","unknown":{"nested":[1,2]}});
        let config = crate::Config {
            commands: vec![original.as_object().unwrap().clone()],
            ..Default::default()
        };
        let draft = draft(
            &config,
            "command",
            "one",
            &json!({"changes":[{"field":0,"value":"Edited"}]}),
        )
        .unwrap();
        assert_eq!(draft["unknown"], original["unknown"]);
        assert_eq!(config.commands[0]["name"], "Original");
        let text = serde_json::to_string(&draft).unwrap();
        assert_eq!(
            super::draft(&config, "command", "one", &json!({"json":text})).unwrap(),
            draft
        );
        for text in ["[]", "broken", r#"{"id":"other"}"#] {
            assert!(super::draft(&config, "command", "one", &json!({"json":text})).is_err());
        }
    }

    #[test]
    fn command_creation_leads_with_reply_and_derives_only_new_names() {
        let config = crate::Config::default();
        let form = form(&config, "command", "").unwrap();
        assert_eq!(form["fields"][0]["key"], "command");
        assert_eq!(form["fields"][1]["key"], "response");
        assert_eq!(form["focusField"], 0);
        assert!(draft(&config, "command", "", &json!({"changes":[]})).is_err());
        let command = draft(
            &config,
            "command",
            "",
            &json!({"changes":[
                {"field":0,"value":"!hello\n!hi"}, {"field":1,"value":"Hello!"}
            ]}),
        )
        .unwrap();
        assert_eq!(command["name"], "!hello");
        assert_eq!(command["response"], "Hello!");
        let original = json!({"id":"existing","name":"Custom name","command":"!old","mode":1,"unknown":true});
        let edit = form_of("command", original.clone()).unwrap();
        let mode = COMMANDS_FIELDS.iter().position(|(key, _, _)| *key == "mode").unwrap();
        assert_eq!(edit["fields"][mode]["collapsed"], false);
        let draft = draft_of(edit, "command", &json!({"changes":[{"field":0,"value":"!new"}]})).unwrap();
        assert_eq!(draft["name"], "Custom name");
        assert_eq!(draft["unknown"], true);
    }
}
