//! Pages that are tables with their own operations: queues, globals, history, backups and crashes.
//!
//! A table model carries semantic cells, per-row `can` flags and an ordered `actions` list. Built-in
//! entries are plain ids (`add`, `edit`, `delete`, `refresh`); anything else is an object with its
//! label, icon and whether it needs a selected row. Frontends draw them; `act` does the work, and MCP
//! reaches both without a frontend.

use super::view::{column, flag, note, text};
use crate::{
    engine::{Engine, DEFAULT_QUEUE},
    i18n::{tr, trf},
};
use serde_json::{json, Value};
use std::rc::Rc;

pub const DOMAINS: &[&str] = &[
    "queues",
    "globals",
    "history",
    "backups",
    "crashes",
    "plugins",
    "mcp",
    "groups",
    "community",
];

pub fn supports(domain: &str) -> bool {
    DOMAINS.contains(&domain)
}

pub(super) fn button(id: &str, label: &'static str, icon: &str, needs_row: bool) -> Value {
    json!({"id": id, "label": tr(label), "icon": icon, "needs": if needs_row { "row" } else { "none" }})
}
pub(super) fn button_if(mut button: Value, enabled: bool) -> Value {
    button["enabled"] = enabled.into();
    button
}
pub(super) fn confirm(mut button: Value, question: String, label: &'static str) -> Value {
    button["confirm"] = question.into();
    button["confirmLabel"] = tr(label).into();
    button
}
pub(super) fn danger(mut button: Value) -> Value {
    button["danger"] = true.into();
    button
}

fn table(kind: &str, title: &str, empty: &str, columns: Vec<Value>, rows: Vec<Value>, actions: Vec<Value>) -> Value {
    json!({"kind": kind, "title": title, "what": "", "guide": "", "columns": columns, "rows": rows, "empty": empty,
        "search": false, "showTitle": false, "actions": actions})
}

pub fn read(engine: &Rc<Engine>, domain: &str) -> Result<Value, String> {
    read_with(engine, domain, &Value::Null)
}

/// The page model with the frontend's view state (`{"tab": kind, kind: {"sort": {"column", "desc"},
/// "hidden": [filter option ids]}}`), so sorting, filtering and the shown tab cover everything the
/// core holds and not only what a frontend has drawn.
pub fn read_with(engine: &Rc<Engine>, domain: &str, view: &Value) -> Result<Value, String> {
    let mut tabs = None;
    let mut tables = match domain {
        "queues" => queues(engine),
        "globals" => vec![globals(engine)],
        "history" => vec![history(engine)],
        "backups" => vec![backups()],
        "crashes" => vec![crashes()],
        "plugins" => vec![plugins(engine)],
        "mcp" => mcp(engine),
        "groups" => vec![groups(engine)],
        "community" => {
            let (tables, shown) = community(engine, view);
            tabs = Some(shown);
            tables
        }
        _ => return Err(format!("no page model for {domain}")),
    };
    for table in &mut tables {
        table["domain"] = domain.into();
    }
    let mut model = json!({"domain": domain, "tables": tables});
    if let Some(tabs) = tabs {
        model["tabs"] = tabs;
    }
    Ok(model)
}

// Queues

fn queues(engine: &Rc<Engine>) -> Vec<Value> {
    let (paused, pending) = (engine.paused(), engine.pending());
    let config = engine.config();
    let mut all = vec![(
        DEFAULT_QUEUE.to_string(),
        tr("lists.queues.default").to_string(),
        config.actions.blocking,
    )];
    all.extend(config.actions.queues.iter().map(|queue| {
        (
            text(queue, "id").to_owned(),
            text(queue, "name").to_owned(),
            flag(queue, "blocking"),
        )
    }));
    let name = |id: &str| {
        all.iter()
            .find(|queue| queue.0 == id)
            .map_or(id, |queue| queue.1.as_str())
            .to_owned()
    };
    let count = |id: &str, running: bool| pending.iter().filter(|item| item.0 == id && item.2 == running).count();
    let rows = all
        .iter()
        .map(|(id, label, blocking)| {
            let is_default = id == DEFAULT_QUEUE;
            let (running, waiting) = (count(id, true), count(id, false));
            json!({"id": id, "name": label, "enabled": true,
                "can": {"edit": !is_default, "delete": !is_default, "clear": waiting > 0},
                "cells": [label,
                    if *blocking { note("lock", tr("lists.queues.blocking")) } else { Value::from("") },
                    if paused.contains(id) { note("pause", tr("lists.queues.paused")) } else { Value::from("") },
                    running.to_string(), waiting.to_string()]})
        })
        .collect();
    let any_paused = all.iter().any(|queue| paused.contains(&queue.0));
    let any_running = all.iter().any(|queue| !paused.contains(&queue.0));
    let mut queue = table(
        "queue",
        "",
        "",
        vec![
            column("name", "lists.queues.queue", 220),
            column("blocking", "", 100),
            column("paused", "", 80),
            column("running", "lists.queues.running", 100),
            column("waiting", "lists.queues.waiting", 100),
        ],
        rows,
        vec![
            "add".into(),
            "edit".into(),
            button("blocking", "btn.blocking", "lock", true),
            button("pause", "btn.pause_resume", "pause", true),
            button("clear", "btn.clear", "eraser", true),
            danger(confirm(
                button("delete", "btn.delete", "trash-2", true),
                tr("lists.queues.delete_confirm").into(),
                "btn.delete",
            )),
            {
                let mut b = button_if(button("pause_all", "btn.pause_all", "circle-pause", false), any_running);
                b["top"] = true.into();
                b["hideDisabled"] = true.into();
                b
            },
            {
                let mut b = button_if(button("resume_all", "btn.resume_all", "play", false), any_paused);
                b["top"] = true.into();
                b["hideDisabled"] = true.into();
                b
            },
        ],
    );
    queue["what"] = tr("lists.queues.what").into();
    queue["empty"] = "".into();
    queue["addTop"] = true.into();
    queue["strip"] = true.into();
    let pending_rows = pending
        .iter()
        .enumerate()
        .map(|(index, (queue, action, running))| {
            json!({"id": format!("pending-{index}"), "name": action, "enabled": true, "cells": [name(queue), action,
                if *running { tr("lists.queues.running") } else { tr("lists.queues.waiting") }]})
        })
        .collect();
    let mut waiting = table(
        "queue_pending",
        tr("lists.queues.pending_title"),
        tr("lists.queues.pending_empty"),
        vec![
            column("queue", "lists.queues.queue", 220),
            column("action", "lists.queues.action", 420),
            column("status", "lists.queues.status", 100),
        ],
        pending_rows,
        Vec::new(),
    );
    waiting["showTitle"] = true.into();
    waiting["icon"] = "hourglass".into();
    vec![queue, waiting]
}

// Globals

fn global_rows(engine: &Rc<Engine>) -> Vec<(String, Value, bool, Option<String>)> {
    let mut out = Vec::new();
    for persisted in [true, false] {
        for (name, value) in engine.globals_list(persisted) {
            out.push((name, value["value"].clone(), persisted, None));
        }
    }
    for platform in ["twitch", "youtube", "kick"] {
        for persisted in [true, false] {
            for value in engine.user_globals_list(platform, None, None, persisted) {
                let user = format!("{} ({platform})", value["userName"].as_str().unwrap_or_default());
                out.push((
                    value["name"].as_str().unwrap_or_default().to_owned(),
                    value["value"].clone(),
                    persisted,
                    Some(user),
                ));
            }
        }
    }
    out
}

pub fn global_id(name: &str, persisted: bool) -> String {
    format!("{}|{name}", u8::from(persisted))
}

fn split_global_id(id: &str) -> Option<(bool, &str)> {
    let (persisted, name) = id.split_once('|')?;
    match persisted {
        "1" => Some((true, name)),
        "0" => Some((false, name)),
        _ => None,
    }
}

fn globals(engine: &Rc<Engine>) -> Value {
    let rows = global_rows(engine)
        .into_iter()
        .enumerate()
        .map(|(index, (name, value, persisted, user))| {
            let id = if user.is_some() { format!("user|{index}") } else { global_id(&name, persisted) };
            json!({"id": id, "name": name, "enabled": true, "can": {"edit": user.is_none(), "delete": user.is_none()},
                "cells": [name, crate::vars::text(&value),
                    tr(if persisted { "lists.globals.persisted" } else { "lists.globals.temporary" }),
                    user.as_ref().map(|user| trf("lists.globals.read_only_user", &[("user", user)])).unwrap_or_default()]})
        })
        .collect();
    let mut model = table(
        "global",
        tr("lists.globals.global_title"),
        tr("lists.globals.empty"),
        vec![
            column("name", "lists.globals.name", 220),
            column("value", "lists.globals.value", 420),
            column("kind", "", 100),
            column("user", "lists.globals.user", 200),
        ],
        rows,
        vec![
            "add".into(),
            "edit".into(),
            "delete".into(),
            json!({"id": "refresh", "label": tr("btn.refresh"), "icon": "refresh-cw", "top": true, "iconOnly": true}),
        ],
    );
    model["what"] = tr("lists.globals.what").into();
    model["guide"] = tr("lists.globals.read_only_hint").into();
    model["titleless"] = true.into();
    model["addTop"] = true.into();
    model["strip"] = true.into();
    model
}

/// The form values of one global, from its row id (an empty id is a new one).
pub fn global_item(engine: &Rc<Engine>, id: &str) -> Result<Value, String> {
    if id.is_empty() {
        return Ok(json!({"variableName": "", "value": "", "autoType": false, "persisted": true, "id": ""}));
    }
    let (persisted, name) = split_global_id(id).ok_or_else(|| format!("no global {id}"))?;
    let value = engine
        .globals_list(persisted)
        .get(name)
        .map(|value| value["value"].clone())
        .ok_or_else(|| format!("no global {name}"))?;
    Ok(
        json!({"variableName": name, "value": crate::vars::text(&value), "autoType": !value.is_string(), "persisted": persisted, "id": id}),
    )
}

pub const GLOBAL_FIELDS: &[crate::automation::schema::F] = &[
    ("variableName", "Name", crate::automation::schema::W::Text),
    ("value", "Value", crate::automation::schema::W::Text),
    ("autoType", "Auto type", crate::automation::schema::W::Bool),
    ("persisted", "Persisted", crate::automation::schema::W::Bool),
];

/// Save a global from a form: renaming or moving it between persisted and temporary deletes the old one.
pub fn save_global(engine: &Rc<Engine>, id: &str, item: crate::model::Obj) -> Result<Value, String> {
    let name = text(&item, "variableName").trim().to_owned();
    if name.is_empty() {
        return Err(tr("lists.globals.needs_name").into());
    }
    let value = if flag(&item, "autoType") {
        crate::vars::auto_type(text(&item, "value"))
    } else {
        text(&item, "value").into()
    };
    let persisted = flag(&item, "persisted");
    if let Some((old_persisted, old_name)) = split_global_id(id) {
        if old_name != name || old_persisted != persisted {
            engine.delete_global(old_name, old_persisted);
        }
    }
    engine.put_global(&name, value, persisted);
    Ok(json!({"saved": global_id(&name, persisted)}))
}

pub fn delete_global(engine: &Rc<Engine>, id: &str) -> Result<Value, String> {
    let (persisted, name) = split_global_id(id).ok_or_else(|| format!("no global {id}"))?;
    engine.delete_global(name, persisted);
    Ok(json!({}))
}

// History

fn run_id(record: &crate::engine::Record) -> String {
    format!(
        "{}|{}|{}",
        record.queued_at,
        record.action_id,
        record
            .args
            .get("runningActionId")
            .and_then(Value::as_str)
            .unwrap_or_default()
    )
}

fn history(engine: &Rc<Engine>) -> Value {
    let runs = engine.history().iter().rev().cloned().collect::<Vec<_>>();
    let rows = runs
        .iter()
        .map(|record| {
            let result = match record.outcome.status() {
                "failed" => "automation.failed",
                "completed_with_skips" => "automation.completed_with_skips",
                _ => "automation.completed",
            };
            json!({"id": run_id(record), "name": record.name, "enabled": true, "cells": [
                crate::fmt::shown_iso(&record.queued_at, true), record.name, format!("{:.0} ms", record.ms),
                record.after.len().to_string(), tr(result)]})
        })
        .collect();
    let mut model = table(
        "history_run",
        "",
        tr("lists.history.empty"),
        vec![
            column("when", "lists.history.when", 180),
            column("action", "lists.history.action", 340),
            column("took", "lists.history.took", 100),
            column("variables", "lists.history.variables", 90),
            column("result", "automation.result", 160),
        ],
        rows,
        vec![
            button("values_before", "btn.variables_when_queued", "list-ordered", true),
            button("values_after", "btn.variables_after_run", "braces", true),
            button("run_again", "btn.run_again", "rotate-ccw", true),
            {
                let mut b = danger(button_if(
                    button("clear", "btn.clear", "eraser", false),
                    !runs.is_empty(),
                ));
                b["top"] = true.into();
                b["iconOnly"] = true.into();
                b["hideDisabled"] = true.into();
                b
            },
            json!({"id": "refresh", "label": tr("btn.refresh"), "icon": "refresh-cw", "top": true, "iconOnly": true}),
        ],
    );
    model["search"] = true.into();
    model["searchHint"] = tr("lists.history.search").into();
    model["strip"] = true.into();
    model["perRow"] = 3.into();
    model
}

// Backups and crashes

fn backup_reason(backup: &crate::backup::Backup) -> String {
    match backup.info.kind {
        crate::backup::Kind::Update => trf("backups.kind_update", &[("version", &backup.info.to)]),
        crate::backup::Kind::Manual => tr("backups.kind_manual").into(),
        crate::backup::Kind::Restore => tr("backups.kind_restore").into(),
    }
}

pub(crate) fn size(bytes: u64) -> String {
    match bytes {
        b if b < 1024 => format!("{b} B"),
        b if b < 1024 * 1024 => format!("{:.0} KB", b as f64 / 1024.0),
        b => format!("{:.1} MB", b as f64 / (1024.0 * 1024.0)),
    }
}

fn backups() -> Value {
    let rows = crate::backup::list()
        .iter()
        .map(|backup| {
            json!({"id": backup.name(), "name": backup.name(), "enabled": true, "cells": [
                crate::fmt::shown_when(backup.info.made),
                if backup.info.version.is_empty() { tr("backups.version_unknown").to_owned() } else { backup.info.version.clone() },
                backup_reason(backup), size(backup.size)]})
        })
        .collect::<Vec<_>>();
    let mut model = table(
        "backup",
        "",
        tr("backups.empty"),
        vec![
            column("when", "backups.when", 170),
            column("version", "backups.version", 90),
            column("why", "backups.why", 340),
            column("size", "backups.size", 90),
        ],
        rows,
        vec![
            {
                let mut b = button("backup_now", "btn.backup_now", "archive", false);
                b["top"] = true.into();
                b["primary"] = true.into();
                b
            },
            button("restore", "btn.restore", "history", true),
            {
                let mut b = button("open_folder", "btn.open_folder", "folder-open", false);
                b["top"] = true.into();
                b["iconOnly"] = true.into();
                b
            },
            danger(confirm(
                button("delete", "btn.delete", "trash-2", true),
                tr("backups.delete_ask").into(),
                "btn.delete",
            )),
        ],
    );
    model["guide"] = tr("backups.hint").into();
    model["guideBelow"] = true.into();
    model["titleless"] = true.into();
    model["strip"] = true.into();
    model
}

fn crashes() -> Value {
    let reports = crate::crash::reports();
    let rows = reports
        .iter()
        .map(|report| {
            let (detail, readable) = match std::fs::read_to_string(&report.path) {
                Ok(text) => (text, true),
                Err(error) => (trf("crashes.read_failed", &[("error", &error.to_string())]), false),
            };
            json!({"id": report.path.display().to_string(), "name": report.what, "enabled": true,
                "can": {"report": readable}, "detail": detail, "detailOk": readable, "cells": [
                crate::fmt::shown_when(report.made), report.version, report.what,
                tr(if report.seen { "crashes.reported" } else { "crashes.new" })]})
        })
        .collect();
    let mut model = table(
        "crash",
        "",
        tr("crashes.empty"),
        vec![
            column("when", "crashes.when", 170),
            column("version", "crashes.version", 90),
            column("what", "crashes.what", 420),
            column("status", "crashes.status", 100),
        ],
        rows,
        vec![
            button("report", "btn.report_github", "github", true),
            {
                let mut b = button("open_folder", "btn.open_folder", "folder-open", false);
                b["top"] = true.into();
                b["iconOnly"] = true.into();
                b
            },
            danger(confirm(
                button("delete", "btn.delete", "trash-2", true),
                tr("crashes.delete_ask").into(),
                "btn.delete",
            )),
            json!({"id": "refresh", "label": tr("btn.refresh"), "icon": "refresh-cw", "top": true, "iconOnly": true}),
        ],
    );
    model["detail"] = tr("crashes.detail_empty").into();
    // A short table, and the report fills the rest of the page.
    model["tableHeight"] = 192.into();
    model["detailFill"] = true.into();
    model["strip"] = true.into();
    model
}

// User groups

fn groups(engine: &Rc<Engine>) -> Value {
    let rows = engine
        .config()
        .groups
        .iter()
        .map(|group| {
            let users = group.get("userIds").and_then(Value::as_array).map_or(0, Vec::len);
            json!({"id": text(group, "id"), "name": text(group, "name"), "enabled": true, "cells": [
                text(group, "name"), trf("lists.groups.user_count", &[("count", &users.to_string())])]})
        })
        .collect();
    let mut members = button("members", "btn.members", "users", true);
    members["dialog"] = "group_members".into();
    let mut model = table(
        "user_group",
        "",
        tr("lists.groups.empty"),
        vec![
            column("group", "lists.groups.group", 300),
            column("users", "lists.groups.users", 120),
        ],
        rows,
        vec![
            "add".into(),
            members,
            danger(confirm(
                button("delete", "btn.delete", "trash-2", true),
                tr("lists.groups.delete_confirm").into(),
                "btn.delete",
            )),
        ],
    );
    model["guide"] = tr("lists.groups.guide").into();
    model["addTop"] = true.into();
    model["strip"] = true.into();
    model["perRow"] = 3.into();
    model["what"] = tr("lists.groups.new_group").into();
    model
}

/// One group's members, for its dialog. Each row id is `group|member key`.
pub fn group_members(engine: &Rc<Engine>, group_id: &str) -> Result<Value, String> {
    let config = engine.config();
    let group = config
        .groups
        .iter()
        .find(|group| text(group, "id") == group_id)
        .ok_or("that group is gone")?;
    let rows = crate::usergroups::members(group)
        .into_iter()
        .map(|key| {
            let (platform, user) = crate::usergroups::split(&key).unwrap_or(("twitch", key.as_str()));
            let name = crate::usergroups::name(group, &key).unwrap_or(user).to_owned();
            json!({"id": format!("{group_id}|{key}"), "name": name, "enabled": true,
                "cells": [{"brand": platform, "text": ""}, name, user]})
        })
        .collect();
    let mut model = table(
        "group_member",
        text(group, "name"),
        tr("lists.groups.members_empty"),
        vec![
            column("platform", "lists.groups.platform", 60),
            column("name", "lists.groups.user_id", 260),
            column("id", "", 160),
        ],
        rows,
        vec![danger(button("remove_member", "btn.remove_streamerz", "x", true))],
    );
    model["domain"] = "groups".into();
    model["perRow"] = 1.into();
    Ok(json!({"domain": "group_members", "tables": [model]}))
}

pub fn add_group(engine: &Rc<Engine>, item: &crate::model::Obj) -> Result<Value, String> {
    let name = text(item, "name").trim().to_owned();
    if name.is_empty() {
        return Err(tr("lists.name_required").into());
    }
    Ok(json!({"saved": engine.add_group(&name, false)}))
}

pub const USER_GROUP_FIELDS: &[crate::automation::schema::F] = &[("name", "Name", crate::automation::schema::W::Text)];

// Community: the people, the quotes and the credits

/// How many people the page lists (the most recent first); the rest stay in the engine.
const USERS_SHOWN: usize = 500;

fn role_name(role: u64) -> &'static str {
    tr(match role {
        1 => "lists.community.role_viewer",
        2 => "lists.community.role_vip",
        3 => "lists.community.role_moderator",
        4 => "lists.community.role_broadcaster",
        _ => return "",
    })
}

fn platform_label(platform: &str) -> &str {
    ["Twitch", "YouTube", "Kick", "TikTok"]
        .into_iter()
        .find(|name| name.eq_ignore_ascii_case(platform))
        .unwrap_or(platform)
}

/// The platforms people can come from, in the order they are listed.
const PLATFORMS: [&str; 4] = ["twitch", "youtube", "kick", "tiktok"];

/// The Users table: sorted by the view's column (newest activity first by default), with the
/// platforms the view hides left out (one is always left showing).
fn users_table(engine: &Rc<Engine>, view: &Value) -> Value {
    let mut users = engine.user_rows();
    let mut present: Vec<String> = users.iter().map(|user| user.platform.clone()).collect();
    present.sort_by_key(|platform| {
        PLATFORMS
            .iter()
            .position(|name| name == platform)
            .unwrap_or(PLATFORMS.len())
    });
    present.dedup();
    let mut hidden: Vec<String> = view["hidden"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .filter(|id| present.iter().any(|platform| platform == id))
        .map(str::to_owned)
        .collect();
    if present.len() < 2 || hidden.len() >= present.len() {
        hidden.clear();
    }
    users.retain(|user| !hidden.contains(&user.platform));
    let by_column = view["sort"]["column"].as_str().unwrap_or("last");
    let desc = view["sort"]["desc"].as_bool().unwrap_or(true);
    users.sort_by(|a, b| {
        let by = match by_column {
            "platform" => a.platform.cmp(&b.platform),
            "name" => a.display.to_lowercase().cmp(&b.display.to_lowercase()),
            "role" => a.role.cmp(&b.role),
            "first" => a.first_message.cmp(&b.first_message),
            _ => a.last_active.cmp(&b.last_active),
        }
        .then_with(|| a.display.cmp(&b.display))
        .then_with(|| a.id.cmp(&b.id));
        if desc {
            by.reverse()
        } else {
            by
        }
    });
    let total = users.len();
    users.truncate(USERS_SHOWN);
    let rows = users
        .into_iter()
        .map(|user| {
            json!({"id": format!("{}|{}", user.platform, user.id), "name": user.display, "enabled": true, "cells": [
                {"brand": user.platform, "text": ""}, user.display, role_name(user.role),
                crate::fmt::shown_iso(&user.first_message, false), crate::fmt::shown_iso(&user.last_active, false)]})
        })
        .collect();
    let mut details = button("card", "btn.details", "user", true);
    details["dialog"] = "user_card".into();
    let mut people = table(
        "community_user",
        tr("lists.community.users"),
        tr("lists.community.users_empty"),
        vec![
            column("platform", "", 44),
            column("name", "lists.community.name", 220),
            column("role", "lists.community.role", 110),
            column("first", "lists.community.first_seen", 110),
            column("last", "lists.community.last_seen", 110),
        ],
        rows,
        vec![details],
    );
    sortable(&mut people, by_column, desc);
    if present.len() > 1 {
        people["filter"] = json!({"column": "platform", "options": present.iter().map(|platform| json!({
            "id": platform, "label": platform_label(platform), "brand": platform,
            "shown": !hidden.contains(platform)})).collect::<Vec<_>>()});
    }
    people["open"] = "card".into();
    people["search"] = true.into();
    people["searchHint"] = tr("lists.community.search_users").into();
    if total > USERS_SHOWN {
        people["guide"] = trf(
            "lists.community.users_capped",
            &[("shown", &USERS_SHOWN.to_string()), ("total", &total.to_string())],
        )
        .into();
    }
    people
}

/// Marks every column of a table sortable and says which one it is sorted by now.
fn sortable(model: &mut Value, column: &str, desc: bool) {
    for entry in model["columns"].as_array_mut().into_iter().flatten() {
        entry["sortable"] = true.into();
        // Dates start newest first when a header is first clicked, names from A.
        entry["descFirst"] = matches!(entry["key"].as_str(), Some("first" | "last")).into();
    }
    model["sort"] = json!({"column": column, "desc": desc});
}

/// The Community page: one table at a time, picked by the `tab` of the view. Only that one is built.
fn community(engine: &Rc<Engine>, view: &Value) -> (Vec<Value>, Value) {
    let kinds = [
        ("community_user", "lists.community.users"),
        ("quote", "lists.community.quotes"),
        ("credit", "lists.community.credits"),
    ];
    let selected = kinds
        .iter()
        .map(|kind| kind.0)
        .find(|kind| view["tab"] == *kind)
        .unwrap_or(kinds[0].0);
    let tabs = json!({"label": tr("lists.community.show"), "selected": selected,
        "options": kinds.iter().map(|(id, label)| json!({"id": id, "label": tr(label)})).collect::<Vec<_>>()});
    let table = match selected {
        "community_user" => users_table(engine, &view[selected]),
        "quote" => quotes_table(engine),
        _ => credits_table(engine),
    };
    (vec![table], tabs)
}

fn quotes_table(engine: &Rc<Engine>) -> Value {
    let quotes = engine
        .config()
        .quotes
        .iter()
        .map(|quote| {
            let id = quote.get("id").and_then(Value::as_u64).unwrap_or(0);
            json!({"id": id.to_string(), "name": format!("#{id}"), "enabled": true, "cells": [
                format!("#{id}"), text(quote, "quote"), text(quote, "user"), text(quote, "gameName"),
                crate::fmt::shown_iso(text(quote, "timestamp"), false)]})
        })
        .collect();
    let mut quotes = table(
        "quote",
        tr("lists.community.quotes"),
        tr("lists.community.quotes_empty"),
        vec![
            column("number", "lists.community.number", 60),
            column("quote", "lists.community.quote", 420),
            column("user", "lists.community.said_by", 140),
            column("game", "lists.community.game", 140),
            column("date", "lists.community.date", 100),
        ],
        quotes,
        vec![
            "add".into(),
            "edit".into(),
            danger(confirm(
                button("quote_delete", "btn.delete", "trash-2", true),
                tr("lists.community.delete_quote_confirm").into(),
                "btn.delete",
            )),
        ],
    );
    quotes["what"] = tr("lists.community.quote").into();
    quotes["addTop"] = true.into();
    quotes["strip"] = true.into();
    quotes["perRow"] = 3.into();
    quotes
}

fn credits_table(engine: &Rc<Engine>) -> Value {
    let credits = engine.credits();
    let mut rows = Vec::new();
    let mut put = |title: String, names: &Value| {
        let names: Vec<&str> = names
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
            .collect();
        if !names.is_empty() {
            rows.push(json!({"id": title.clone(), "name": title.clone(), "enabled": true,
                "cells": [title, names.join(", ")]}));
        }
    };
    for (key, names) in credits["events"].as_object().into_iter().flatten() {
        put(key.clone(), names);
    }
    for (key, names) in credits["users"].as_object().into_iter().flatten() {
        put(trf("lists.community.spoke", &[("list", key)]), names);
    }
    put(
        tr("lists.community.top_rewards").into(),
        &credits["top"]["channelRewards"],
    );
    let mut credits = table(
        "credit",
        tr("lists.community.credits"),
        tr("lists.community.credits_empty"),
        vec![
            column("list", "lists.community.list", 200),
            column("names", "lists.community.names", 520),
        ],
        rows,
        vec![
            {
                let mut b = button("credits_test", "btn.fill_with_test_names", "flask-conical", false);
                b["top"] = true.into();
                b["primary"] = true.into();
                b
            },
            {
                let mut b = danger(confirm(
                    button("credits_reset", "btn.reset", "rotate-ccw", false),
                    tr("lists.community.reset_confirm").into(),
                    "btn.reset",
                ));
                b["top"] = true.into();
                b["iconOnly"] = true.into();
                b
            },
        ],
    );
    credits["perRow"] = 2.into();
    credits
}

pub const QUOTE_FIELDS: &[crate::automation::schema::F] = &[
    ("quote", "Quote", crate::automation::schema::W::Multi),
    ("user", "Said by", crate::automation::schema::W::Text),
    ("gameName", "Game", crate::automation::schema::W::Text),
];

/// One quote for its form; an empty id is a new one said by the channel's owner.
pub fn quote_item(engine: &Rc<Engine>, id: &str) -> Result<Value, String> {
    if id.is_empty() {
        let (user, user_id) = engine
            .config()
            .twitch
            .broadcaster
            .as_ref()
            .map(|account| (account.display_name.clone(), account.user_id.clone()))
            .unwrap_or_default();
        return Ok(
            json!({"id": "", "quote": "", "user": user, "userId": user_id, "platform": "twitch", "gameName": ""}),
        );
    }
    let number: u64 = id.parse().map_err(|_| "that quote is gone")?;
    engine
        .config()
        .quotes
        .iter()
        .find(|quote| quote.get("id").and_then(Value::as_u64) == Some(number))
        .cloned()
        .map(Value::Object)
        .ok_or_else(|| "that quote is gone".into())
}

pub fn save_quote(engine: &Rc<Engine>, id: &str, item: crate::model::Obj) -> Result<Value, String> {
    let quote = text(&item, "quote").trim().to_owned();
    if quote.is_empty() {
        return Err(tr("lists.community.quote_needed").into());
    }
    if id.is_empty() {
        let made = engine.add_quote(&quote, text(&item, "user"), text(&item, "userId"), "twitch");
        let mut made = made;
        if !text(&item, "gameName").is_empty() {
            made.insert("gameName".into(), text(&item, "gameName").into());
            let number = made.get("id").and_then(Value::as_u64);
            engine.change(|config| {
                if let Some(saved) = config
                    .quotes
                    .iter_mut()
                    .find(|q| q.get("id").and_then(Value::as_u64) == number)
                {
                    *saved = made.clone();
                }
            });
        }
        return Ok(json!({"saved": made.get("id")}));
    }
    let number: u64 = id.parse().map_err(|_| "that quote is gone")?;
    engine.change(|config| {
        if let Some(saved) = config
            .quotes
            .iter_mut()
            .find(|q| q.get("id").and_then(Value::as_u64) == Some(number))
        {
            for key in ["quote", "user", "gameName"] {
                saved.insert(key.into(), text(&item, key).into());
            }
        }
    });
    Ok(json!({"saved": id}))
}

fn user_values(engine: &Rc<Engine>, id: &str) -> Result<Value, String> {
    let (platform, user) = id.split_once('|').ok_or("that person is gone")?;
    let found = engine.community_user(platform, user).ok_or("that person is gone")?;
    let mut shown = crate::model::Obj::new();
    let mut put = |label: &str, value: String| {
        if !value.is_empty() {
            shown.insert(label.into(), value.into());
        }
    };
    put(tr("lists.community.name"), text(&found, "display").to_owned());
    put(tr("lists.groups.platform"), platform_label(platform).to_owned());
    put(
        tr("lists.community.role"),
        role_name(found.get("role").and_then(Value::as_u64).unwrap_or(1)).to_owned(),
    );
    put(
        tr("lists.community.first_seen"),
        crate::fmt::shown_iso(text(&found, "firstMessage"), true),
    );
    put(
        tr("lists.community.last_seen"),
        crate::fmt::shown_iso(text(&found, "lastActive"), true),
    );
    let title = text(&found, "display").to_owned();
    Ok(values(&title, "", &shown))
}

/// The platform's page for a person.
pub fn profile_url(platform: &str, login: &str, id: &str) -> String {
    match platform {
        "twitch" => format!("https://twitch.tv/{}", login.to_lowercase()),
        "youtube" => format!("https://youtube.com/channel/{id}"),
        "kick" => format!("https://kick.com/{}", login.to_lowercase()),
        _ => format!("https://tiktok.com/@{login}"),
    }
}

/// "3 h 20 min", "45 min", "30 s".
fn duration(secs: u64) -> String {
    match secs {
        0..=59 => format!("{secs} s"),
        60..=3599 => format!("{} min", secs / 60),
        _ => format!("{} h {} min", secs / 3600, secs % 3600 / 60),
    }
}

/// How many of a person's latest messages their card lists.
const CARD_MESSAGES: usize = 40;

/// A person's card for its dialog: who they are, their role, their time here, the groups they are in
/// (with the switch to change them) and what they said. The id is the Users row id (`platform|user id`).
pub fn user_card(engine: &Rc<Engine>, id: &str) -> Result<Value, String> {
    let (platform, user) = id.split_once('|').ok_or("that person is gone")?;
    let found = engine.community_user(platform, user).ok_or("that person is gone")?;
    let number = |key: &str| found.get(key).and_then(Value::as_u64).unwrap_or(0);
    let mut facts = Vec::new();
    let mut fact = |label: &'static str, value: String| {
        if !value.is_empty() {
            facts.push(json!({"label": tr(label), "value": value}));
        }
    };
    fact("lists.community.role", role_name(number("role").max(1)).to_owned());
    fact(
        "card.first_seen",
        crate::fmt::shown_iso(text(&found, "firstMessage"), true),
    );
    fact(
        "card.last_seen",
        crate::fmt::shown_iso(text(&found, "lastActive"), true),
    );
    if number("messages") > 0 {
        fact("card.messages", number("messages").to_string());
    }
    if number("activeSecs") > 0 {
        fact("card.chatting", duration(number("activeSecs")));
    }
    let groups: Vec<Value> = engine
        .groups_of(platform, user)
        .into_iter()
        .map(|(group, name, member)| json!({"id": group, "name": name, "member": member}))
        .collect();
    let (_, _, lines) = engine.chatter_sync(platform, user, u64::MAX, 0);
    let skip = lines.len().saturating_sub(CARD_MESSAGES);
    let messages: Vec<Value> = lines
        .iter()
        .skip(skip)
        .map(|line| json!({"time": line.time, "text": line.text}))
        .collect();
    let login = if text(&found, "name").is_empty() {
        text(&found, "display")
    } else {
        text(&found, "name")
    };
    Ok(json!({"domain": "user_card", "card": {
        "id": id, "title": text(&found, "display"), "platform": platform,
        "platformLabel": platform_label(platform), "color": text(&found, "color"),
        "role": role_name(number("role").max(1)), "facts": facts, "groups": groups, "messages": messages,
        "labels": {"groups": tr("card.groups"), "groupsTip": tr("card.groups_tip"), "noGroups": tr("card.no_groups"),
            "messages": tr("card.recent"), "noMessages": tr("lists.community.card_no_messages"),
            "profile": tr("btn.open_profile"), "close": tr("dialogs.close"),
            "says": trf("chat.card_platform_says", &[("platform", platform_label(platform))]),
            "looking": tr("chat.card_looking_up")},
        "asks": matches!(platform, "twitch" | "kick" | "youtube"),
        "profile": profile_url(platform, login, user)}}))
}

/// Puts a person in a group or takes them out; the id is `platform|user id|group id`.
fn card_group(engine: &Rc<Engine>, id: &str) -> Result<Value, String> {
    let mut parts = id.splitn(3, '|');
    let (Some(platform), Some(user), Some(group)) = (parts.next(), parts.next(), parts.next()) else {
        return Err("that person is gone".into());
    };
    let found = engine.community_user(platform, user).ok_or("that person is gone")?;
    let inside = engine
        .groups_of(platform, user)
        .iter()
        .any(|entry| entry.0 == group && entry.2);
    let mut who = crate::model::Obj::new();
    who.insert("userId".into(), user.into());
    who.insert("userName".into(), text(&found, "display").into());
    who.insert("user".into(), text(&found, "name").into());
    who.insert("userType".into(), platform.into());
    engine.group_users(group, &[who], !inside);
    Ok(json!({"accepted": true}))
}

// MCP agents

/// Where an agent's MCP server reaches this Streamerz (what `Streamerz --mcp` reads from the config).
fn mcp_url(config: &crate::Config) -> String {
    let websocket = &config.websockets;
    let host = match websocket.address.as_str() {
        "" | "0.0.0.0" | "*" => "127.0.0.1",
        host => host,
    };
    format!("ws://{host}:{}{}", websocket.port, websocket.endpoint)
}

fn mcp_server(engine: &Rc<Engine>) -> crate::agents::Server {
    crate::agents::Server::this(mcp_url(&engine.config()))
}

/// The logo file of an agent, when it has one.
fn agent_logo(id: &str) -> Option<&'static str> {
    Some(match id {
        "claude" | "claude-desktop" => "claude",
        "roo-code" => "roocode",
        "vibe" => "mistral",
        "codex" => "codex",
        "opencode" => "opencode",
        "gemini" => "gemini",
        "antigravity" => "antigravity",
        "cursor" => "cursor",
        "windsurf" => "windsurf",
        "vscode" => "vscode",
        "copilot" => "copilot",
        "qwen" => "qwen",
        "kiro" => "kiro",
        "crush" => "crush",
        "amp" => "amp",
        "lmstudio" => "lmstudio",
        "cline" => "cline",
        "zed" => "zed",
        "devin" => "devin",
        "kimi" => "kimi",
        "auggie" => "auggie",
        _ => return None,
    })
}

fn agent_state(state: crate::agents::State) -> (&'static str, &'static str, &'static str) {
    use crate::agents::State;
    match state {
        State::Added => ("up", tr("agents.state.added"), tr("agents.tip.added")),
        State::Ready => ("off", tr("agents.state.ready"), tr("agents.tip.ready")),
        State::Outdated => ("busy", tr("agents.state.outdated"), tr("agents.outdated")),
    }
}

fn mcp(engine: &Rc<Engine>) -> Vec<Value> {
    use crate::agents::State;
    let server = mcp_server(engine);
    let found = crate::agents::found(&crate::agents::Places::here(), &server);
    let rows = found
        .iter()
        .map(|agent| {
            let (health, state, explanation) = agent_state(agent.state);
            let mut detail = format!("{state}: {explanation}");
            if agent.cli.is_none() {
                detail += " ";
                detail += tr("agents.no_cli");
            }
            let name = agent.kind.name;
            json!({"id": agent.kind.id, "name": name, "enabled": true,
                "can": {"add_streamerz": agent.state != State::Added, "remove_streamerz": agent.state != State::Ready,
                    "open_chat": agent.cli.is_some(), "open_file": agent.file.is_file(),
                    "open_folder": agent.file.parent().is_some_and(|folder| folder.is_dir())},
                "detail": detail, "detailHealth": health, "cells": [
                    match agent_logo(agent.kind.id) { Some(logo) => json!({"brand": logo, "text": name}), None => note("bot", name) },
                    if agent.cli.is_some() { note("terminal", tr("agents.kind.cli")) } else { note("app-window", tr("agents.kind.app")) },
                    json!({"health": health, "text": state}),
                    agent.file.display().to_string()]})
        })
        .collect();
    let mut agents = table(
        "agent",
        "",
        tr("agents.empty"),
        vec![
            column("agent", "agents.header.agent", 230),
            column("kind", "agents.header.kind", 110),
            column("state", "agents.header.state", 130),
            column("config", "agents.header.config", 420),
        ],
        rows,
        vec![
            {
                let mut b = button("add_streamerz", "btn.add_streamerz", "plus", true);
                b["hideDisabled"] = true.into();
                b
            },
            {
                let mut b = button("remove_streamerz", "btn.remove_streamerz", "x", true);
                b["hideDisabled"] = true.into();
                b
            },
            button("open_chat", "btn.open_chat", "terminal", true),
            button("open_file", "btn.open_file", "external-link", true),
            button("open_folder", "btn.open_folder", "folder-open", false),
            {
                let mut b = button("copy_config", "btn.copy_config", "copy", false);
                b["top"] = true.into();
                b
            },
            json!({"id": "refresh", "label": tr("btn.refresh"), "icon": "refresh-cw", "top": true, "iconOnly": true}),
        ],
    );
    agents["guide"] = tr("agents.intro").into();
    agents["detail"] = tr("agents.selection_hint").into();
    agents["detailStatus"] = true.into();
    agents["selectFirst"] = true.into();
    agents["strip"] = true.into();
    // Open folder needs a selected agent: its row says whether the folder exists.
    agents["actions"][4]["needs"] = "row".into();
    let url = mcp_url(&engine.config());
    let connection = json!({"kind": "mcp_connection", "presentation": "settings", "domain": "mcp", "title": tr("agents.cfg.title"),
        "showTitle": true, "icon": "plug", "guide": "", "columns": [], "actions": [],
        "inline": super::sections::mcp_connection(&engine.config(), &trf("agents.cfg.about", &[("url", &url), ("command", &server.command)])),
        "rows": [{"id": "mcp_connection", "kind": "mcp_connection", "singleton": true, "name": tr("agents.cfg.title"), "health": "standby",
            "text": trf("agents.cfg.about", &[("url", &url), ("command", &server.command)])}]});
    vec![agents, connection]
}

pub const MCP_FIELDS: &[crate::automation::schema::F] = &[
    ("address", "Address", crate::automation::schema::W::Text),
    ("port", "Port", crate::automation::schema::W::Int),
    ("endpoint", "Route", crate::automation::schema::W::Text),
];

/// Normalise what was typed like the WebSocket server's own form does, then save it as that server's settings.
pub fn save_mcp_connection(engine: &Rc<Engine>, mut item: crate::model::Obj) -> Result<Value, String> {
    let address = text(&item, "address").trim().to_owned();
    item.insert(
        "address".into(),
        if address.is_empty() {
            "127.0.0.1".into()
        } else {
            address.into()
        },
    );
    let route = text(&item, "endpoint").trim().to_owned();
    item.insert(
        "endpoint".into(),
        match route.as_str() {
            "" => "/".to_owned(),
            route if route.starts_with('/') => route.to_owned(),
            route => format!("/{route}"),
        }
        .into(),
    );
    super::settings::save(engine, "server_websocket", item)
}

// Plugins

/// What the list shows for each plugin file: id, manifest (None when it doesn't load), on, load error.
fn plugin_rows(engine: &Rc<Engine>) -> Vec<(String, Option<crate::engine::plugins::Manifest>, bool, String)> {
    let on = engine.plugins_on();
    crate::engine::plugins::installed()
        .into_iter()
        .map(|(id, path)| match on.iter().find(|plugin| plugin.0 == id) {
            Some((_, manifest)) => (id, Some(manifest.clone()), true, String::new()),
            None => match crate::engine::plugins::load(&id, &path, std::rc::Weak::new()) {
                Ok(plugin) => {
                    let enabled = engine.config().plugins.get(&id).is_some_and(|plugin| plugin.enabled);
                    (id, Some(plugin.manifest.clone()), enabled, String::new())
                }
                Err(error) => (id, None, false, error),
            },
        })
        .collect()
}

fn plugins(engine: &Rc<Engine>) -> Value {
    let rows = plugin_rows(engine)
        .into_iter()
        .map(|(id, manifest, on, error)| {
            let (name, version) = manifest.as_ref().map_or((id.clone(), String::new()), |manifest| (manifest.name.clone(), manifest.version.clone()));
            let state = match (on, error.is_empty()) {
                (_, false) => tr("plugins.state.cant_load"),
                (true, _) => tr("plugins.state.on"),
                _ => tr("plugins.state.off"),
            };
            let detail = if !error.is_empty() {
                error.clone()
            } else if let Some(manifest) = &manifest {
                let mut text = manifest.about.clone();
                let subactions: Vec<&str> = manifest.subactions.iter().map(|sub| sub.name.as_str()).collect();
                let triggers: Vec<&str> = manifest.triggers.iter().map(|trigger| trigger.name.as_str()).collect();
                if !subactions.is_empty() {
                    text += &trf("plugins.subactions", &[("list", &subactions.join(", "))]);
                }
                if !triggers.is_empty() {
                    text += &trf("plugins.triggers", &[("list", &triggers.join(", "))]);
                }
                text
            } else {
                String::new()
            };
            let has_settings = manifest.as_ref().is_some_and(|manifest| !manifest.settings.is_empty());
            json!({"id": id, "name": name, "enabled": true, "can": {"toggle": manifest.is_some(), "settings": has_settings},
                "detail": detail, "detailOk": error.is_empty(), "cells": [name, version, state]})
        })
        .collect();
    let mut model = table(
        "plugin",
        "",
        tr("plugins.empty"),
        vec![
            column("name", "plugins.header.plugin", 300),
            column("version", "plugins.header.version", 120),
            column("state", "plugins.header.state", 160),
        ],
        rows,
        vec![
            button("toggle", "btn.on_off", "power", true),
            {
                let mut settings = button("settings", "btn.settings", "settings", true);
                settings["form"] = "plugin_settings:".into();
                settings
            },
            {
                let mut b = button("open_folder", "btn.open_folder", "folder-open", false);
                b["top"] = true.into();
                b["iconOnly"] = true.into();
                b
            },
            json!({"id": "refresh", "label": tr("btn.refresh"), "icon": "refresh-cw", "top": true, "iconOnly": true}),
        ],
    );
    model["detail"] = "".into();
    model["detailHeight"] = 100.into();
    model["strip"] = true.into();
    model["perRow"] = 4.into();
    model
}

// Entities whose values don't live in the config file itself

const PLUGIN_SETTINGS: &str = "plugin_settings:";

/// A plugin's settings as form fields, made once for each manifest.
pub fn plugin_settings_fields(
    id: &str,
    fields: &[crate::engine::plugins::Field],
) -> &'static [crate::automation::schema::F] {
    use crate::automation::schema::{F, W::*};
    use std::{collections::HashMap, sync::Mutex};
    static MADE: Mutex<Option<HashMap<String, &'static [F]>>> = Mutex::new(None);
    let signature = format!(
        "{id}/{}",
        fields
            .iter()
            .map(|field| format!("{}:{}:{}", field.key, field.kind, field.choices.join(",")))
            .collect::<Vec<_>>()
            .join(";")
    );
    let mut made = MADE.lock().unwrap_or_else(|error| error.into_inner());
    made.get_or_insert_with(HashMap::new)
        .entry(signature)
        .or_insert_with(|| {
            let leak = |text: &str| -> &'static str { Box::leak(text.to_owned().into_boxed_str()) };
            let out: Vec<F> = fields
                .iter()
                .map(|field| {
                    let widget = match field.kind.as_str() {
                        "lines" => Multi,
                        "number" => Num,
                        "bool" => Bool,
                        "choice" => Pick(Box::leak(
                            field
                                .choices
                                .iter()
                                .map(|choice| leak(choice))
                                .collect::<Vec<_>>()
                                .into_boxed_slice(),
                        )),
                        _ => Text,
                    };
                    let label = if field.label.is_empty() {
                        &field.key
                    } else {
                        &field.label
                    };
                    (leak(&field.key), leak(label), widget)
                })
                .collect();
            Box::leak(out.into_boxed_slice())
        })
}

fn plugin_manifest(engine: &Rc<Engine>, id: &str) -> Result<crate::engine::plugins::Manifest, String> {
    plugin_rows(engine)
        .into_iter()
        .find(|row| row.0 == id)
        .and_then(|row| row.1)
        .ok_or_else(|| format!("no plugin {id}"))
}

/// The fields of a kind whose item the core keeps outside the config: a global or a plugin's settings.
pub fn external_fields(engine: &Rc<Engine>, kind: &str) -> Option<&'static [crate::automation::schema::F]> {
    if let Some(fields) = super::polls::external_fields(kind) {
        return Some(fields);
    }
    if let Some(fields) = super::stream::external_fields(kind) {
        return Some(fields);
    }
    if let Some(fields) = super::accounts::external_fields(kind) {
        return Some(fields);
    }
    if kind == "global" {
        return Some(GLOBAL_FIELDS);
    }
    if kind == "user_group" {
        return Some(USER_GROUP_FIELDS);
    }
    if kind == "quote" {
        return Some(QUOTE_FIELDS);
    }
    let id = kind.strip_prefix(PLUGIN_SETTINGS)?;
    let manifest = plugin_manifest(engine, id).ok()?;
    Some(plugin_settings_fields(id, &manifest.settings))
}

/// The form for such a kind, with its current values; None for any other kind.
pub fn external_form(engine: &Rc<Engine>, kind: &str, item_id: &str) -> Option<Result<Value, String>> {
    if let Some(form) = super::polls::external_form(kind) {
        return Some(form);
    }
    if let Some(form) = super::stream::external_form(kind) {
        return Some(form);
    }
    if let Some(form) = super::accounts::external_form(engine, kind) {
        return Some(form);
    }
    if kind == "global" {
        return Some(
            global_item(engine, item_id).and_then(|item| crate::automation::entities::form_of("global", item)),
        );
    }
    if kind == "quote" {
        return Some(quote_item(engine, item_id).and_then(|item| crate::automation::entities::form_of("quote", item)));
    }
    if kind == "user_group" {
        let item = json!({"id": "", "name": tr("lists.groups.new_group")});
        return Some(crate::automation::entities::form_of("user_group", item));
    }
    let id = kind.strip_prefix(PLUGIN_SETTINGS)?;
    Some((|| {
        let manifest = plugin_manifest(engine, id)?;
        let fields = plugin_settings_fields(id, &manifest.settings);
        let mut settings = engine
            .config()
            .plugins
            .get(id)
            .map(|plugin| plugin.settings.clone())
            .unwrap_or_default();
        for field in &manifest.settings {
            if !settings.contains_key(&field.key) && !field.default.is_null() {
                settings.insert(field.key.clone(), field.default.clone());
            }
        }
        settings.insert("id".into(), id.into());
        let item = Value::Object(settings);
        let mut form = json!({"kind": kind, "title": trf("plugins.settings_title", &[("name", &manifest.name)]), "new": item.clone(),
            "fields": fields.iter().map(|(key, label, widget)| {
                let mut value = json!({"key": key, "label": crate::automation::schema::field_label(label), "widget": widget, "holds": widget.about()});
                if let crate::automation::schema::W::Pick(options) = widget { value["widget"]["options"] = json!(options.iter().map(|option| crate::automation::schema::field_label(option)).collect::<Vec<_>>()); }
                value
            }).collect::<Vec<_>>()});
        let object = item.as_object().ok_or("settings must be an object")?;
        for (index, (key, _, widget)) in fields.iter().enumerate() {
            form["fields"][index]["value"] = crate::automation::edits::shown(widget, object.get(*key));
        }
        form["item"] = item;
        Ok(form)
    })())
}

/// Save such a kind from a form's item.
pub fn save_external(
    engine: &Rc<Engine>,
    kind: &str,
    item_id: &str,
    item: crate::model::Obj,
) -> Option<Result<Value, String>> {
    if let Some(saved) = super::polls::save_external(engine, kind, item.clone()) {
        return Some(saved);
    }
    if let Some(saved) = super::stream::save_external(engine, kind, item.clone()) {
        return Some(saved);
    }
    if let Some(saved) = super::accounts::save_external(engine, kind, item.clone()) {
        return Some(saved);
    }
    if kind == "global" {
        return Some(save_global(engine, item_id, item));
    }
    if kind == "quote" {
        return Some(save_quote(engine, item_id, item));
    }
    if kind == "user_group" {
        return Some(add_group(engine, &item));
    }
    let id = kind.strip_prefix(PLUGIN_SETTINGS)?;
    let mut settings = item;
    settings.remove("id");
    engine.plugin_set(id, settings);
    Some(Ok(json!({"saved": id})))
}

// Operations

fn open_folder(path: &std::path::Path) -> Result<Value, String> {
    std::fs::create_dir_all(path).map_err(|error| trf("recovery.folder_failed", &[("error", &error.to_string())]))?;
    crate::oauth::open_browser(&path.display().to_string());
    Ok(json!({"accepted": true}))
}

fn values(title: &str, hint: &str, values: &crate::model::Obj) -> Value {
    json!({"values": {"title": title, "hint": hint, "rows": values.iter().map(|(name, value)| json!({"name": name, "value": crate::vars::text(value)})).collect::<Vec<_>>()}})
}

/// Run one table operation. Row ids come from the page models above.
pub fn act(engine: &Rc<Engine>, domain: &str, action: &str, id: &str) -> Result<Value, String> {
    let accepted = Ok(json!({"accepted": true}));
    if action == "refresh" {
        return accepted;
    }
    match (domain, action) {
        ("queues", "blocking") => {
            let config = engine.config();
            let now = if id == DEFAULT_QUEUE {
                config.actions.blocking
            } else {
                config
                    .actions
                    .queues
                    .iter()
                    .any(|queue| text(queue, "id") == id && flag(queue, "blocking"))
            };
            drop(config);
            engine.set_blocking(id, !now);
            accepted
        }
        ("queues", "pause") => {
            let paused = engine.paused().iter().any(|queue| queue == id);
            engine.pause(id, !paused, false);
            accepted
        }
        ("queues", "clear") => {
            engine.clear_queue(id);
            accepted
        }
        ("queues", "pause_all" | "resume_all") => {
            engine.pause("", action == "pause_all", false);
            accepted
        }
        ("history", "values_before" | "values_after" | "run_again") => {
            let record = engine
                .history()
                .iter()
                .find(|record| run_id(record) == id)
                .cloned()
                .ok_or("that run is no longer in the history")?;
            match action {
                "values_before" => Ok(values(&record.name, tr("automation.completed_hint"), &record.args)),
                "values_after" => Ok(values(
                    &record.name,
                    record
                        .outcome
                        .error
                        .as_deref()
                        .unwrap_or(tr("automation.completed_hint")),
                    &record.after,
                )),
                _ => {
                    let mut args = record.args.clone();
                    args.shift_remove("runningActionId");
                    args.shift_remove("actionQueuedAt");
                    engine.queue(&record.action_id, args);
                    accepted
                }
            }
        }
        ("history", "clear") => {
            engine.clear_history();
            accepted
        }
        ("backups", "backup_now") => {
            crate::save(&engine.config())
                .map_err(|error| trf("backups.save_failed", &[("error", &error.to_string())]))?;
            engine
                .flush_community()
                .map_err(|error| trf("backups.save_failed", &[("error", &error.to_string())]))?;
            let made = crate::backup::make(crate::backup::Kind::Manual, crate::brand::VERSION, "")
                .map_err(|error| trf("backups.create_failed", &[("error", &error)]))?;
            crate::say(trf("backups.created", &[("name", &made.name())]));
            accepted
        }
        ("backups", "restore" | "delete") => {
            let backup = crate::backup::list()
                .into_iter()
                .find(|backup| backup.name() == id)
                .ok_or("that backup is gone")?;
            if action == "delete" {
                crate::backup::delete(&backup).map_err(|error| trf("backups.delete_failed", &[("error", &error)]))?;
                return accepted;
            }
            crate::backup::restore(&backup).map_err(|error| trf("backups.restore_failed", &[("error", &error)]))?;
            crate::say(trf("backups.restored", &[("name", &backup.name())]));
            crate::update::restart()
        }
        ("log", "open_folder") => Ok(super::log::open_folder()),
        ("backups", "open_folder") => open_folder(&crate::backup::root()),
        ("community", "card") => user_values(engine, id),
        ("community", "card_group") => card_group(engine, id),
        ("community", "open_profile") => {
            let (platform, user) = id.split_once('|').ok_or("that person is gone")?;
            let found = engine.community_user(platform, user).ok_or("that person is gone")?;
            crate::oauth::open_browser(&profile_url(platform, text(&found, "name"), user));
            accepted
        }
        ("community", "quote_delete") => {
            let number: u64 = id.parse().map_err(|_| "that quote is gone")?;
            engine.change(|config| {
                config
                    .quotes
                    .retain(|quote| quote.get("id").and_then(Value::as_u64) != Some(number))
            });
            accepted
        }
        ("community", "credits_reset") => {
            engine.reset_credits();
            accepted
        }
        ("community", "credits_test") => {
            engine.test_credits();
            accepted
        }
        ("groups", "delete") => {
            engine.remove_group(id);
            accepted
        }
        ("groups", "remove_member") => {
            let (group, key) = id.split_once('|').ok_or("that member is gone")?;
            let (platform, user) = crate::usergroups::split(key).unwrap_or(("twitch", key));
            let mut who = crate::model::Obj::new();
            who.insert("userId".into(), user.into());
            who.insert("userType".into(), platform.into());
            engine.group_users(group, &[who], false);
            accepted
        }
        ("plugins", "open_folder") => open_folder(&crate::engine::plugins::dir()),
        ("mcp", "copy_config") => Ok(json!({"copy": mcp_server(engine).snippet(), "message": tr("agents.copied")})),
        ("mcp", "open_file" | "open_folder" | "add_streamerz" | "remove_streamerz" | "open_chat") => {
            let server = mcp_server(engine);
            let found = crate::agents::found(&crate::agents::Places::here(), &server);
            let agent = found
                .iter()
                .find(|agent| agent.kind.id == id)
                .ok_or("that agent is gone")?;
            match action {
                "open_file" | "open_folder" => {
                    let target = if action == "open_folder" {
                        agent.file.parent().unwrap_or(&agent.file)
                    } else {
                        &agent.file
                    };
                    crate::oauth::open_browser(&target.display().to_string());
                    accepted
                }
                "add_streamerz" | "remove_streamerz" => {
                    crate::agents::set(
                        agent.kind,
                        &crate::agents::Places::here(),
                        &server,
                        action == "add_streamerz",
                    )
                    .map_err(|error| trf("agents.cant_write", &[("name", agent.kind.name), ("error", &error)]))?;
                    accepted
                }
                _ => {
                    let cli = agent.cli.clone().ok_or("that agent has no command to chat with")?;
                    if agent.state != crate::agents::State::Added {
                        crate::agents::set(agent.kind, &crate::agents::Places::here(), &server, true).map_err(
                            |error| trf("agents.cant_write", &[("name", agent.kind.name), ("error", &error)]),
                        )?;
                    }
                    crate::agents::workspace(&server)
                        .and_then(|folder| crate::agents::chat(&cli, &folder))
                        .map_err(|error| trf("agents.cant_open", &[("name", agent.kind.name), ("error", &error)]))?;
                    crate::say(format!("Opened {} in a terminal", agent.kind.name));
                    accepted
                }
            }
        }
        ("plugins", "toggle") => {
            let row = plugin_rows(engine)
                .into_iter()
                .find(|row| row.0 == id)
                .ok_or("that plugin is gone")?;
            if row.1.is_none() {
                return Err(row.3);
            }
            engine.plugin_enable(id, !row.2);
            accepted
        }
        ("crashes", "open_folder") => open_folder(&crate::crash::folder()),
        ("crashes", "report" | "delete") => {
            let report = crate::crash::reports()
                .into_iter()
                .find(|report| report.path.display().to_string() == id)
                .ok_or("that report is gone")?;
            if action == "delete" {
                crate::crash::delete(&report).map_err(|error| trf("crashes.delete_failed", &[("error", &error)]))?;
                return accepted;
            }
            std::fs::read_to_string(&report.path)
                .map_err(|error| trf("crashes.read_failed", &[("error", &error.to_string())]))?;
            crate::crash::report_on_github(&report);
            accepted
        }
        _ => Err(format!("{domain} has no action {action}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_default_queue_cannot_be_edited_or_deleted_and_pause_all_follows_state() {
        let _lock = crate::TEST_LOCK.lock().unwrap_or_else(|error| error.into_inner());
        crate::engine::tests::run(async {
            let engine = crate::engine::tests::engine(json!({"actions": []}));
            engine.change(|config| {
                config.actions.queues.push(
                    json!({"id":"q","name":"Mine","blocking":true})
                        .as_object()
                        .unwrap()
                        .clone(),
                );
            });
            let model = read(&engine, "queues").unwrap();
            let rows = model["tables"][0]["rows"].as_array().unwrap();
            assert_eq!(rows[0]["id"], DEFAULT_QUEUE);
            assert_eq!(rows[0]["can"]["edit"], false);
            assert_eq!(rows[0]["can"]["delete"], false);
            assert_eq!(rows[1]["can"]["edit"], true);
            assert_eq!(rows[1]["cells"][1]["text"], tr("lists.queues.blocking"));
            let actions = model["tables"][0]["actions"].as_array().unwrap();
            let resume = actions.iter().find(|action| action["id"] == "resume_all").unwrap();
            assert_eq!(resume["enabled"], false);
            act(&engine, "queues", "pause", "q").unwrap();
            assert_eq!(engine.paused(), vec!["q".to_string()]);
            let model = read(&engine, "queues").unwrap();
            let actions = model["tables"][0]["actions"].as_array().unwrap();
            assert_eq!(
                actions.iter().find(|action| action["id"] == "resume_all").unwrap()["enabled"],
                true
            );
            act(&engine, "queues", "pause", "q").unwrap();
            assert!(engine.paused().is_empty());
            assert!(act(&engine, "queues", "nonsense", "").is_err());
        });
    }

    #[test]
    fn globals_are_saved_renamed_moved_and_deleted_by_row_id() {
        let _lock = crate::TEST_LOCK.lock().unwrap_or_else(|error| error.into_inner());
        crate::engine::tests::run(async {
            let engine = crate::engine::tests::engine(json!({"actions": []}));
            let form = |name: &str, value: &str, auto: bool, persisted: bool| {
                json!({"variableName": name, "value": value, "autoType": auto, "persisted": persisted})
                    .as_object()
                    .unwrap()
                    .clone()
            };
            assert!(save_global(&engine, "", form(" ", "x", false, true)).is_err());
            let saved = save_global(&engine, "", form("count", "3", true, true)).unwrap();
            assert_eq!(saved["saved"], "1|count");
            assert_eq!(engine.globals_list(true)["count"]["value"], 3);
            let item = global_item(&engine, "1|count").unwrap();
            assert_eq!(item["autoType"], true);
            // Renaming and making it temporary leaves no copy behind.
            save_global(&engine, "1|count", form("total", "3", true, false)).unwrap();
            assert!(engine.globals_list(true).get("count").is_none());
            assert_eq!(engine.globals_list(false)["total"]["value"], 3);
            let model = read(&engine, "globals").unwrap();
            assert_eq!(model["tables"][0]["rows"][0]["id"], "0|total");
            delete_global(&engine, "0|total").unwrap();
            assert!(engine.globals_list(false).is_empty());
            assert!(delete_global(&engine, "nonsense").is_err());
        });
    }

    #[test]
    fn history_rows_can_show_their_variables_and_the_list_clears() {
        let _lock = crate::TEST_LOCK.lock().unwrap_or_else(|error| error.into_inner());
        crate::engine::tests::run(async {
            let engine = crate::engine::tests::engine(
                json!({"actions": [{"id":"a","name":"Run me","enabled":true,"subActions":[]}]}),
            );
            engine.queue("a", json!({"who":"ana"}).as_object().unwrap().clone());
            tokio::task::yield_now().await;
            let model = read(&engine, "history").unwrap();
            let rows = model["tables"][0]["rows"].as_array().unwrap();
            if let Some(row) = rows.first() {
                let id = row["id"].as_str().unwrap();
                let shown = act(&engine, "history", "values_before", id).unwrap();
                assert!(shown["values"]["rows"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|row| row["name"] == "who" && row["value"] == "ana"));
            }
            act(&engine, "history", "clear", "").unwrap();
            assert!(read(&engine, "history").unwrap()["tables"][0]["rows"]
                .as_array()
                .unwrap()
                .is_empty());
            assert!(act(&engine, "history", "values_before", "missing").is_err());
        });
    }

    #[test]
    fn user_groups_are_added_listed_emptied_and_deleted() {
        let _lock = crate::TEST_LOCK.lock().unwrap_or_else(|error| error.into_inner());
        crate::engine::tests::run(async {
            let engine = crate::engine::tests::engine(json!({"actions": []}));
            let form = crate::automation::entities::form_of("user_group", json!({"id": "", "name": ""})).unwrap();
            assert_eq!(form["fields"][0]["key"], "name");
            let empty = add_group(&engine, json!({"name": "  "}).as_object().unwrap()).unwrap_err();
            assert_eq!(empty, tr("lists.name_required"), "a message, not the prompt label");
            assert_ne!(empty, tr("lists.ask_name"));
            let id = add_group(&engine, json!({"name": "Mods"}).as_object().unwrap()).unwrap()["saved"]
                .as_str()
                .unwrap()
                .to_owned();
            let mut who = crate::model::Obj::new();
            who.insert("userId".into(), "42".into());
            who.insert("userType".into(), "twitch".into());
            engine.group_users(&id, &[who], true);
            let model = read(&engine, "groups").unwrap();
            assert_eq!(model["tables"][0]["rows"][0]["cells"][0], "Mods");
            let members = group_members(&engine, &id).unwrap();
            let member = members["tables"][0]["rows"][0]["id"].as_str().unwrap().to_owned();
            assert!(member.starts_with(&format!("{id}|twitch")));
            act(&engine, "groups", "remove_member", &member).unwrap();
            assert!(group_members(&engine, &id).unwrap()["tables"][0]["rows"]
                .as_array()
                .unwrap()
                .is_empty());
            act(&engine, "groups", "delete", &id).unwrap();
            assert!(group_members(&engine, &id).is_err());
        });
    }

    #[test]
    fn community_quotes_are_added_edited_and_deleted_and_credits_reset() {
        let _lock = crate::TEST_LOCK.lock().unwrap_or_else(|error| error.into_inner());
        crate::engine::tests::run(async {
            let engine = crate::engine::tests::engine(json!({"actions": []}));
            let item = |quote: &str, game: &str| {
                json!({"quote": quote, "user": "Ana", "userId": "7", "gameName": game})
                    .as_object()
                    .unwrap()
                    .clone()
            };
            assert!(save_quote(&engine, "", item(" ", "")).is_err());
            save_quote(&engine, "", item("first", "Chess")).unwrap();
            let model = read_with(&engine, "community", &json!({"tab": "quote"})).unwrap();
            let quotes = &model["tables"][0];
            assert_eq!(quotes["kind"], "quote");
            assert_eq!(quotes["rows"][0]["cells"][1], "first");
            assert_eq!(quotes["rows"][0]["cells"][3], "Chess");
            let id = quotes["rows"][0]["id"].as_str().unwrap().to_owned();
            save_quote(&engine, &id, item("second", "Go")).unwrap();
            assert_eq!(quote_item(&engine, &id).unwrap()["quote"], "second");
            act(&engine, "community", "quote_delete", &id).unwrap();
            assert!(engine.config().quotes.is_empty());
            assert!(quote_item(&engine, &id).is_err());
            act(&engine, "community", "credits_test", "").unwrap();
            let credits = |engine: &Rc<Engine>| {
                read_with(engine, "community", &json!({"tab": "credit"})).unwrap()["tables"][0]["rows"]
                    .as_array()
                    .unwrap()
                    .len()
            };
            assert!(credits(&engine) > 1);
            act(&engine, "community", "credits_reset", "").unwrap();
            assert_eq!(credits(&engine), 0);
            assert!(act(&engine, "community", "card", "twitch|nobody").is_err());
        });
    }

    #[test]
    fn community_shows_one_tab_sorts_and_filters_every_person_and_opens_their_card() {
        let _lock = crate::TEST_LOCK.lock().unwrap_or_else(|error| error.into_inner());
        crate::engine::tests::run(async {
            let engine = crate::engine::tests::engine(json!({"actions": []}));
            engine.dev_mode();
            engine.change(|config| config.window.chat.save_history = true);
            for (platform, user, text) in [
                ("twitch", "bea", "hello"),
                ("kick", "ana", "hi there"),
                ("twitch", "cy", "yo"),
            ] {
                engine
                    .inject_chat(platform, user, "", "viewer", text, &Value::Null)
                    .unwrap();
            }
            let model = read(&engine, "community").unwrap();
            assert_eq!(model["tabs"]["selected"], "community_user");
            let ids: Vec<_> = model["tabs"]["options"]
                .as_array()
                .unwrap()
                .iter()
                .map(|tab| tab["id"].clone())
                .collect();
            assert_eq!(ids, [json!("community_user"), json!("quote"), json!("credit")]);
            assert_eq!(model["tables"].as_array().unwrap().len(), 1);
            let users = &model["tables"][0];
            assert_eq!(users["sort"], json!({"column": "last", "desc": true}));
            assert_eq!(users["columns"][1]["sortable"], true);
            let names = |model: &Value| -> Vec<String> {
                model["tables"][0]["rows"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|row| row["name"].as_str().unwrap().to_owned())
                    .collect()
            };
            let by_name = read_with(
                &engine,
                "community",
                &json!({"community_user": {"sort": {"column": "name", "desc": false}}}),
            )
            .unwrap();
            assert_eq!(names(&by_name), ["ana", "bea", "cy"]);
            let reversed = read_with(
                &engine,
                "community",
                &json!({"community_user": {"sort": {"column": "name", "desc": true}}}),
            )
            .unwrap();
            assert_eq!(names(&reversed), ["cy", "bea", "ana"]);
            let by_platform = read_with(
                &engine,
                "community",
                &json!({"community_user": {"sort": {"column": "platform", "desc": false}}}),
            )
            .unwrap();
            assert_eq!(by_platform["tables"][0]["rows"][0]["id"], "kick|ana");
            // A hidden platform leaves the list; hiding every one is ignored.
            let hidden = read_with(&engine, "community", &json!({"community_user": {"hidden": ["kick"]}})).unwrap();
            assert_eq!(names(&hidden).len(), 2);
            let options = hidden["tables"][0]["filter"]["options"].as_array().unwrap();
            assert_eq!(
                options
                    .iter()
                    .map(|option| option["id"].as_str().unwrap())
                    .collect::<Vec<_>>(),
                ["twitch", "kick"]
            );
            assert_eq!(options[1]["shown"], false);
            let all = read_with(
                &engine,
                "community",
                &json!({"community_user": {"hidden": ["kick", "twitch"]}}),
            )
            .unwrap();
            assert_eq!(names(&all).len(), 3);
            let only = read_with(&engine, "community", &json!({"tab": "bad"})).unwrap();
            assert_eq!(only["tables"][0]["kind"], "community_user");

            let card = user_card(&engine, "twitch|bea").unwrap();
            assert_eq!(card["card"]["title"], "bea");
            assert_eq!(card["card"]["messages"][0]["text"], "hello");
            assert!(card["card"]["facts"].as_array().unwrap().len() >= 3);
            assert!(card["card"]["profile"].as_str().unwrap().ends_with("twitch.tv/bea"));
            assert!(user_card(&engine, "twitch|nobody").is_err());
            let group = engine.add_group("Friends", false);
            act(&engine, "community", "card_group", &format!("twitch|bea|{group}")).unwrap();
            let card = user_card(&engine, "twitch|bea").unwrap();
            assert_eq!(card["card"]["groups"][0]["member"], true);
            act(&engine, "community", "card_group", &format!("twitch|bea|{group}")).unwrap();
            assert_eq!(
                user_card(&engine, "twitch|bea").unwrap()["card"]["groups"][0]["member"],
                false
            );
            // The same through the request MCP sends (view state and the card's row id).
            let quotes = crate::servers::answer(
                &engine,
                &json!({"request": "StreamerzCoreView", "domain": "community", "view": {"tab": "quote"}}),
                true,
            )
            .unwrap();
            assert_eq!(quotes["tables"][0]["kind"], "quote");
            let card = crate::servers::answer(
                &engine,
                &json!({"request": "StreamerzCoreView", "domain": "user_card", "itemId": "kick|ana"}),
                true,
            )
            .unwrap();
            assert_eq!(card["card"]["title"], "ana");
        });
    }

    #[test]
    fn every_page_model_marks_actions_and_names_its_domain() {
        let _lock = crate::TEST_LOCK.lock().unwrap_or_else(|error| error.into_inner());
        crate::engine::tests::run(async {
            let engine = crate::engine::tests::engine(json!({"actions": []}));
            for domain in DOMAINS {
                let model = read(&engine, domain).unwrap();
                assert_eq!(model["domain"], *domain);
                for table in model["tables"].as_array().unwrap() {
                    assert_eq!(table["domain"], *domain);
                    for action in table["actions"].as_array().unwrap() {
                        assert!(action.is_string() || action["id"].is_string(), "{domain}: {action}");
                    }
                }
            }
            assert!(read(&engine, "nothing").is_err());
        });
    }
}
