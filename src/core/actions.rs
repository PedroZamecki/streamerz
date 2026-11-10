//! The Actions page: groups, actions, triggers and sub-actions as semantic trees, the pickers that add
//! to them, the forms that edit them and the operations on them. A frontend only draws these models.
//!
//! Row ids: `a:<action id>` and `g:<group name>` in the action tree; a trigger's or sub-action's own
//! id (`#<index>` or `#<i>.<j>` for the rare one that has none) in the editor.

use crate::automation::{catalog as picker, edits, entities, labels, schema};
use crate::edit::{self, Path};
use crate::engine::Engine;
use crate::i18n::{tr, trf};
use crate::model::{Action, Obj};
use crate::types::{name, SUB_ACTION_INFO, TRIGGERS};
use schema::{F, W};
use serde_json::{json, Value};
use std::rc::Rc;

pub fn supports(domain: &str) -> bool {
    matches!(domain, "actions" | "actions_picker")
}

/// Forms of this page have kinds `act:<form>:<context>...`.
pub fn handles(kind: &str) -> bool {
    kind.starts_with("act:")
}

fn text<'a>(value: &'a Value, key: &str) -> &'a str {
    value.get(key).and_then(Value::as_str).unwrap_or_default()
}
fn enc(text: &str) -> String {
    text.replace('%', "%25").replace(':', "%3A")
}
fn dec(text: &str) -> String {
    text.replace("%3A", ":").replace("%25", "%")
}
fn plain_id(node: &str) -> &str {
    node.strip_prefix("a:")
        .or_else(|| node.strip_prefix("g:"))
        .unwrap_or(node)
}

/// The file a group's picture is kept in, written once from the saved icon (named by its digest) so a
/// frontend can draw it. `None` when the icon is not a valid picture.
fn group_icon_file(icon: &Value) -> Option<String> {
    let bytes = crate::group_icons::decode(icon)?;
    let extension = if icon["format"] == "png" { "png" } else { "jpg" };
    let digest = icon["sha256"]
        .as_str()
        .filter(|digest| digest.len() == 64 && digest.bytes().all(|b| b.is_ascii_hexdigit()))?;
    let path = crate::config_path()
        .with_file_name("cache")
        .join("group-icons")
        .join(format!("{digest}.{extension}"));
    if !path.is_file() {
        std::fs::create_dir_all(path.parent()?).ok()?;
        std::fs::write(&path, bytes).ok()?;
    }
    Some(path.to_string_lossy().into_owned())
}

fn item_gone() -> String {
    tr("actions.item_gone").into()
}

// Rows

fn button(id: &str, label: &'static str, icon: &str, needs: &[&str]) -> Value {
    json!({"id": id, "label": tr(label), "icon": icon, "needs": needs})
}
fn with(mut value: Value, key: &str, set: impl Into<Value>) -> Value {
    value[key] = set.into();
    value
}
fn form(value: Value, kind: &str) -> Value {
    with(value, "form", kind)
}
fn op(value: Value, name: &str) -> Value {
    with(value, "op", name)
}
fn picker_for(value: Value, which: &str) -> Value {
    with(value, "picker", which)
}
fn confirm(value: Value, question: &str, label: &'static str) -> Value {
    with(
        with(with(value, "confirm", question), "confirmLabel", tr(label)),
        "danger",
        true,
    )
}
/// Only while its `needs` hold, else hidden.
fn only(value: Value) -> Value {
    with(value, "hidden", true)
}
/// Only in the context menu.
fn menu_only(value: Value) -> Value {
    with(value, "menuOnly", true)
}

fn node(id: String, kind: &str, label: String, icon: &str, muted: bool, children: Vec<Value>) -> Value {
    json!({"id": id, "kind": kind, "label": label, "icon": icon, "muted": muted, "children": children})
}

fn config_action<'a>(config: &'a crate::Config, id: &str) -> Option<&'a Action> {
    config.actions.actions.iter().find(|action| action.id == id)
}

fn group_names(config: &crate::Config) -> Vec<String> {
    crate::groups::names(config)
}

fn action_tree(config: &crate::Config) -> Value {
    let mut sorted: Vec<&Action> = config.actions.actions.iter().collect();
    sorted.sort_by_key(|action| action.name.to_lowercase());
    let row = |action: &&Action| {
        node(
            format!("a:{}", action.id),
            "action",
            action.name.clone(),
            "zap",
            !action.enabled,
            Vec::new(),
        )
    };
    let mut nodes = Vec::new();
    for group in group_names(config) {
        let children = sorted.iter().filter(|action| action.group == group).map(row).collect();
        let mut group_node = node(format!("g:{group}"), "group", group.clone(), "folder", false, children);
        if let Some(icon) = config.action_group_icons.get(&group) {
            group_node["hasIcon"] = true.into();
            if let Some(file) = group_icon_file(icon) {
                group_node["iconFile"] = file.into();
            }
        }
        nodes.push(group_node);
    }
    nodes.extend(sorted.iter().filter(|action| action.group.is_empty()).map(row));
    let plain = |value: Value| value;
    let buttons = vec![
        form(button("add", "btn.add", "plus", &[]), "act:action_add"),
        form(button("edit", "btn.edit", "pencil", &["action"]), "act:action_edit"),
        form(
            button("duplicate", "btn.duplicate", "copy", &["action"]),
            "act:action_duplicate",
        ),
        menu_only(op(
            button("toggle", "btn.on_off", "power", &["action"]),
            "action_toggle",
        )),
        confirm(
            op(button("delete", "btn.delete", "trash-2", &["action"]), "action_delete"),
            tr("actions.delete_confirm"),
            "btn.delete",
        ),
        op(button("test", "btn.test", "play", &["action"]), "action_run"),
        form(button("import", "btn.import", "download", &[]), "act:import"),
        form(button("export", "btn.export", "upload", &[]), "act:export"),
        with(
            op(
                picker_for(button("templates", "btn.templates", "layout-template", &[]), "template"),
                "template_add",
            ),
            "tooltip",
            tr("actions.templates_tooltip"),
        ),
        form(
            button("add_group", "btn.add_group", "folder-plus", &[]),
            "act:group_add",
        ),
        only(form(
            button("group_settings", "btn.group_variables", "settings", &["group"]),
            "act:group_settings",
        )),
        only(form(
            button("group_icon", "btn.set_group_icon", "image-plus", &["group"]),
            "act:group_icon",
        )),
        only(op(
            button("group_icon_clear", "btn.clear_group_icon", "image-off", &["group"]),
            "group_icon_clear",
        )),
        only(form(
            button("group_rename", "btn.rename_group", "pencil", &["group"]),
            "act:group_rename",
        )),
        only(confirm(
            op(
                button("group_delete", "btn.delete_group", "folder-x", &["group"]),
                "group_delete",
            ),
            tr("actions.delete_group_confirm"),
            "btn.delete_group",
        )),
    ];
    json!({"title": tr("actions.heading"), "searchHint": tr("actions.search_actions"), "empty": tr("actions.empty_actions"), "perRow": 5,
        "nodes": nodes, "buttons": buttons.into_iter().map(plain).collect::<Vec<_>>()})
}

fn field<'a>(object: &'a Obj, key: &str) -> &'a str {
    object.get(key).and_then(Value::as_str).unwrap_or_default()
}

fn trigger_id(trigger: &Obj, index: usize) -> String {
    let id = field(trigger, "id");
    if id.is_empty() {
        format!("#{index}")
    } else {
        id.into()
    }
}
fn sub_id(sub: &Obj, path: &[usize]) -> String {
    let id = field(sub, "id");
    if id.is_empty() {
        format!("#{}", path.iter().map(usize::to_string).collect::<Vec<_>>().join("."))
    } else {
        id.into()
    }
}

fn trigger_index(action: &Action, id: &str) -> Option<usize> {
    match id.strip_prefix('#') {
        Some(index) => index.parse().ok().filter(|index| *index < action.triggers.len()),
        None => action.triggers.iter().position(|trigger| field(trigger, "id") == id),
    }
}

fn find_sub(list: &[Obj], id: &str, path: &mut Path) -> bool {
    for (index, sub) in list.iter().enumerate() {
        path.push(index);
        if field(sub, "id") == id {
            return true;
        }
        let kids: Vec<Obj> = sub
            .get("subActions")
            .and_then(Value::as_array)
            .map(|kids| kids.iter().filter_map(|kid| kid.as_object().cloned()).collect())
            .unwrap_or_default();
        if find_sub(&kids, id, path) {
            return true;
        }
        path.pop();
    }
    false
}

/// Where a sub-action is.
fn sub_path(action: &Action, id: &str) -> Option<Path> {
    if let Some(path) = id.strip_prefix('#') {
        let path: Path = path.split('.').map(str::parse).collect::<Result<_, _>>().ok()?;
        return edit::get(&action.sub_actions, &path).map(|_| path);
    }
    let mut path = Path::new();
    find_sub(&action.sub_actions, id, &mut path).then_some(path)
}

fn sub_nodes(list: &[Obj], path: &mut Path, names: &labels::Names) -> Vec<Value> {
    let mut out = Vec::new();
    for (index, sub) in list.iter().enumerate() {
        path.push(index);
        let code = edit::kind(sub);
        let off = sub.get("enabled").and_then(Value::as_bool) == Some(false);
        let unsupported = !crate::engine::supported(code as u32);
        let kids: Vec<Obj> = sub
            .get("subActions")
            .and_then(Value::as_array)
            .map(|kids| kids.iter().filter_map(|kid| kid.as_object().cloned()).collect())
            .unwrap_or_default();
        let (title, category) = SUB_ACTION_INFO
            .binary_search_by_key(&(code as u32), |entry| entry.0)
            .map(|at| (SUB_ACTION_INFO[at].1, SUB_ACTION_INFO[at].2))
            .unwrap_or((tr("actions.subaction_word"), "Core"));
        let category = if category.is_empty() { "Core" } else { category };
        let mut row = node(
            sub_id(sub, path),
            "sub",
            labels::sub_label(sub, names),
            if off {
                "power-off"
            } else {
                labels::sub_icon(code as u32)
            },
            off || unsupported,
            sub_nodes(&kids, path, names),
        );
        row["italic"] = (99901..=99904).contains(&code).into();
        row["code"] = code.into();
        row["detail"] = json!({"title": trf("actions.subaction_details", &[("name", title), ("category", &category.replace('/', " > "))]),
            "text": picker::sub_action_description(code as u32, title, category)});
        out.push(row);
        path.pop();
    }
    out
}

fn editor(config: &crate::Config, action: &Action) -> Value {
    let names = labels::names(config);
    let mut flags = Vec::new();
    for (on, what) in [
        (!action.enabled, tr("actions.flag_disabled")),
        (action.random_action, tr("actions.flag_random")),
        (action.concurrent, tr("actions.flag_concurrent")),
    ] {
        if on {
            flags.push(what);
        }
    }
    let title = if flags.is_empty() {
        action.name.clone()
    } else {
        format!("{} ({})", action.name, flags.join(", "))
    };
    let triggers: Vec<Value> = action
        .triggers
        .iter()
        .enumerate()
        .map(|(index, trigger)| {
            let code = trigger.get("type").and_then(Value::as_u64).unwrap_or(0) as u32;
            let raw = name(TRIGGERS, code).unwrap_or(tr("actions.unknown"));
            let category = picker::trigger_categorize(raw);
            let title = picker::trigger_item_title(code, raw);
            let mut row = node(
                trigger_id(trigger, index),
                "trigger",
                labels::trigger_label(trigger, &names),
                labels::trigger_icon(code),
                !labels::trigger_on(trigger),
                Vec::new(),
            );
            row["code"] = code.into();
            row["detail"] = json!({"title": trf("actions.trigger_details", &[("title", &title), ("category", &category.replace('/', " > "))]),
                "text": picker::trigger_description(code, raw, &title, &category)});
            row
        })
        .collect();
    let subs = sub_nodes(&action.sub_actions, &mut Vec::new(), &names);
    let trigger_buttons = vec![
        picker_for(
            form(button("add", "btn.add", "plus", &[]), "act:trigger_add"),
            "trigger",
        ),
        form(button("edit", "btn.edit", "pencil", &["trigger"]), "act:trigger_edit"),
        op(button("toggle", "btn.on_off", "power", &["trigger"]), "trigger_toggle"),
        form(button("test", "btn.test", "play", &["trigger"]), "act:trigger_test"),
        op(
            button("delete", "btn.delete", "trash-2", &["trigger"]),
            "trigger_delete",
        ),
        op(button("help", "btn.help", "circle-help", &["trigger"]), "trigger_help"),
    ];
    let sub_buttons = vec![
        picker_for(form(button("add", "btn.add", "plus", &[]), "act:sub_add"), "sub"),
        form(button("edit", "btn.edit", "pencil", &["sub"]), "act:sub_edit"),
        op(button("toggle", "btn.on_off", "power", &["sub"]), "sub_toggle"),
        op(button("delete", "btn.delete", "trash-2", &["sub"]), "sub_delete"),
        op(button("help", "btn.help", "circle-help", &["sub"]), "sub_help"),
        menu_only(op(button("up", "actions.move_up", "arrow-up", &["sub"]), "sub_up")),
        menu_only(op(
            button("down", "actions.move_down", "arrow-down", &["sub"]),
            "sub_down",
        )),
        menu_only(op(
            button("in", "actions.move_in", "indent-increase", &["sub"]),
            "sub_in",
        )),
        menu_only(op(
            button("out", "actions.move_out", "indent-decrease", &["sub"]),
            "sub_out",
        )),
    ];
    let group = if action.group.is_empty() {
        action.name.clone()
    } else {
        trf(
            "actions.action_details_title",
            &[("name", &action.name), ("group", &action.group)],
        )
    };
    json!({"actionId": action.id, "title": title,
        "searchHint": tr("actions.search_subactions"),
        "detail": {"title": group, "text": trf("actions.action_details_desc", &[("triggers", &action.triggers.len().to_string()), ("subactions", &action.sub_actions.len().to_string())])},
        "triggers": {"title": tr("actions.triggers_heading"), "empty": tr("actions.empty_triggers"), "perRow": 6, "nodes": triggers, "buttons": trigger_buttons},
        "subs": {"title": tr("actions.subactions_heading"), "searchHint": tr("actions.search_subactions"), "empty": tr("actions.empty_subactions"), "perRow": 7, "nodes": subs, "buttons": sub_buttons}})
}

/// `StreamerzCoreView` for `actions` (the tree and the open action's editor) and `actions_picker`.
pub fn read(engine: &Rc<Engine>, request: &Value) -> Result<Value, String> {
    let domain = text(request, "domain");
    let config = engine.config();
    if domain == "actions_picker" {
        return picker_model(engine, text(request, "kind"));
    }
    let open = plain_id(if text(request, "actionId").is_empty() {
        text(&request["view"], "actionId")
    } else {
        text(request, "actionId")
    });
    let open = config_action(&config, open);
    Ok(json!({"domain": "actions", "actions": {
        "tree": action_tree(&config),
        "empty": tr("actions.pick_to_edit"),
        "detail": {"title": tr("actions.details"), "text": tr("actions.details_empty")},
        "editor": open.map(|action| editor(&config, action)),
        // Nothing open: the same panes, empty, as 1.6.0 shows them.
        "idle": open.is_none().then(|| {
            let mut idle = editor(&config, &Action::default());
            idle["title"] = tr("actions.pick_to_edit").into();
            idle["idle"] = true.into();
            idle
        }),
    }}))
}

// Pickers

struct Entry {
    id: String,
    title: String,
    category: String,
    description: String,
    icon: &'static str,
}

fn tree_of(entries: Vec<Entry>) -> Vec<Value> {
    use std::collections::BTreeMap;
    #[derive(Default)]
    struct Level {
        folders: BTreeMap<String, Level>,
        items: Vec<Value>,
    }
    fn fold(level: Level, path: &str) -> Vec<Value> {
        let mut out: Vec<Value> = level
            .folders
            .into_iter()
            .map(|(name, inner)| {
                let id = format!("{path}/{name}");
                let icon = crate::automation::pick_icons::group(&name);
                node(format!("c:{id}"), "category", name, icon, false, fold(inner, &id))
            })
            .collect();
        let mut items = level.items;
        items.sort_by_key(|item| item["label"].as_str().unwrap_or_default().to_lowercase());
        out.extend(items);
        out
    }
    let mut root = Level::default();
    for entry in entries {
        let mut level = &mut root;
        for part in entry.category.split('/').filter(|part| !part.is_empty()) {
            level = level.folders.entry(part.trim().to_owned()).or_default();
        }
        let mut row = node(entry.id, "item", entry.title.clone(), entry.icon, false, Vec::new());
        row["detail"] = json!({"title": entry.title, "text": entry.description});
        level.items.push(row);
    }
    fold(root, "")
}

fn plugin_entries(engine: &Rc<Engine>, trigger: bool) -> Vec<Entry> {
    let mut out = Vec::new();
    for (id, manifest) in engine.plugins_on() {
        let list: Vec<(String, String)> = if trigger {
            manifest
                .triggers
                .iter()
                .map(|t| (t.name.clone(), t.about.clone()))
                .collect()
        } else {
            manifest
                .subactions
                .iter()
                .map(|t| (t.name.clone(), t.about.clone()))
                .collect()
        };
        for (name, about) in list {
            out.push(Entry {
                id: format!("p:{}:{}", enc(&id), enc(&name)),
                title: name,
                category: format!("Plugins/{}", manifest.name),
                description: about,
                icon: "puzzle",
            });
        }
    }
    out
}

fn picker_model(engine: &Rc<Engine>, which: &str) -> Result<Value, String> {
    let (title, entries) = match which {
        "trigger" => {
            let mut entries: Vec<Entry> = picker::trigger_pick_items()
                .into_iter()
                .map(|item| Entry {
                    icon: labels::trigger_icon(item.id as u32),
                    id: item.id.to_string(),
                    title: item.title,
                    category: item.category,
                    description: item.description,
                })
                .collect();
            entries.extend(plugin_entries(engine, true));
            (tr("actions.add_trigger"), entries)
        }
        "sub" => {
            let mut entries: Vec<Entry> = picker::sub_action_pick_items()
                .into_iter()
                .map(|item| Entry {
                    icon: labels::sub_icon(item.id as u32),
                    id: item.id.to_string(),
                    title: item.title,
                    category: item.category,
                    description: item.description,
                })
                .collect();
            entries.extend(plugin_entries(engine, false));
            (tr("actions.add_subaction"), entries)
        }
        "template" => (
            tr("actions.template_title"),
            crate::templates::TEMPLATES
                .iter()
                .map(|template| Entry {
                    id: template.name.into(),
                    title: if template.name == "Song requests (Spotify)" {
                        tr("spotify.template_name").into()
                    } else {
                        template.title().into()
                    },
                    category: String::new(),
                    description: if template.name == "Song requests (Spotify)" {
                        tr("spotify.template_about").into()
                    } else {
                        template.description().into()
                    },
                    icon: "layout-template",
                })
                .collect(),
        ),
        other => return Err(format!("no picker {other}")),
    };
    Ok(
        json!({"domain": "actions_picker", "kind": which, "title": title, "searchHint": tr("lists.search"),
        "nodes": tree_of(entries)}),
    )
}

// Forms

const ACTION_FIELDS: &[F] = &[
    ("name", "dialogs.name", W::Text),
    ("group", "dialogs.action.group", W::ActionGroup),
    ("queue", "dialogs.action.queue", W::Queue),
    ("enabled", "dialogs.action.enabled", W::Bool),
    ("randomAction", "dialogs.action.random", W::Bool),
    ("concurrent", "dialogs.action.concurrent", W::Bool),
    ("alwaysRun", "dialogs.action.always_run", W::Bool),
    ("excludeFromHistory", "dialogs.action.exclude_from_history", W::Bool),
    ("excludeFromPending", "dialogs.action.exclude_from_pending", W::Bool),
];
const NAME_FIELDS: &[F] = &[("name", "dialogs.name", W::Text)];
const IMPORT_FIELDS: &[F] = &[
    ("text", "dialogs.import.hint", W::Multi),
    ("file", "actions.import_file", W::Text),
];
const EXPORT_FIELDS: &[F] = &[
    (
        "scope",
        "actions.export_scope",
        W::Pick(&["actions.export_selected", "actions.export_all"]),
    ),
    ("name", "dialogs.name", W::Text),
    ("author", "dialogs.export.author", W::Text),
    ("version", "dialogs.export.version", W::Text),
    ("description", "dialogs.export.description", W::Text),
    ("file", "actions.export_file", W::Text),
];
const ICON_FIELDS: &[F] = &[("file", "actions.icon_file", W::Text)];
const TEST_FIELDS: &[F] = &[("args", "actions.test_args", W::Map)];

fn shown(label: &'static str) -> &'static str {
    if ["dialogs.", "actions.", "group."]
        .iter()
        .any(|prefix| label.starts_with(prefix))
    {
        tr(label)
    } else {
        schema::field_label(label)
    }
}

fn pairs<'a>(list: impl Iterator<Item = &'a Obj>) -> Vec<(String, String)> {
    list.map(|o| (field(o, "id").to_owned(), field(o, "name").to_owned()))
        .collect()
}

/// What a field that names something else can be set to.
fn choices(config: &crate::Config, widget: &W) -> Option<(Vec<(String, String)>, bool)> {
    let strict = |list: Vec<(String, String)>| Some((list, true));
    let typed = |list: Vec<(String, String)>| Some((list, false));
    match widget {
        W::Action => strict(
            config
                .actions
                .actions
                .iter()
                .map(|a| (a.id.clone(), a.name.clone()))
                .collect(),
        ),
        W::Queue => {
            let mut list = vec![(crate::engine::DEFAULT_QUEUE.to_owned(), "Default".to_owned())];
            list.extend(pairs(config.actions.queues.iter()));
            strict(list)
        }
        W::Timer => strict(pairs(config.timers.iter())),
        W::FileWatcher => strict(pairs(config.file_watchers.iter())),
        W::FileTail => strict(pairs(config.file_tails.iter())),
        W::Command => strict(pairs(config.commands.iter())),
        W::Client => strict(pairs(config.websockets.connections.iter())),
        W::Server => strict(pairs(config.websockets.servers.iter())),
        W::ObsConnection => strict(pairs(config.obs.connections.iter())),
        W::SlobsConnection => strict(pairs(config.slobs.connections.iter())),
        W::UserGroup => typed(
            pairs(config.groups.iter())
                .into_iter()
                .map(|(_, name)| (name.clone(), name))
                .collect(),
        ),
        W::ActionGroup => typed(
            group_names(config)
                .into_iter()
                .map(|name| (name.clone(), name))
                .collect(),
        ),
        W::CommandGroup => {
            let mut names: Vec<String> = config
                .commands
                .iter()
                .filter_map(|c| c.get("group").and_then(Value::as_str))
                .filter(|group| !group.is_empty())
                .map(str::to_owned)
                .collect();
            names.sort_by_key(|name| name.to_lowercase());
            names.dedup();
            typed(names.into_iter().map(|name| (name.clone(), name)).collect())
        }
        W::Suggest(key) | W::Entity(key) => typed(crate::catalog::choices(key)),
        W::Expression(values) => typed(values.iter().map(|v| ((*v).to_owned(), (*v).to_owned())).collect()),
        _ => None,
    }
}

fn form_fields(config: &crate::Config, fields: &[F], item: &Obj) -> Vec<Value> {
    fields
        .iter()
        .map(|(key, label, widget)| {
            let mut value = json!({"key": key, "label": shown(label), "widget": widget, "holds": widget.about(),
                "value": edits::shown(widget, item.get(*key))});
            if let W::Pick(options) = widget {
                value["widget"]["options"] = json!(options.iter().map(|option| shown(option)).collect::<Vec<_>>());
            }
            if key.is_empty() {
                value["hidden"] = true.into();
            }
            if let Some((list, strict)) = choices(config, widget) {
                value["choices"] = list
                    .iter()
                    .map(|(id, label)| json!({"id": id, "label": label}))
                    .collect();
                value["strict"] = strict.into();
            }
            value
        })
        .collect()
}

fn model(config: &crate::Config, kind: String, title: String, hint: &str, fields: &[F], item: Obj) -> Value {
    let item = Value::Object(item);
    json!({"kind": kind, "title": title, "hint": hint, "new": item, "item": item.clone(),
        "fields": form_fields(config, fields, item.as_object().unwrap_or(&Obj::new()))})
}

/// The parts of a form kind (`act`, the form, its context...), read from the request when the kind
/// is the bare one a button sends.
fn context(request: &Value) -> Vec<String> {
    let mut parts: Vec<String> = text(request, "kind").split(':').map(dec).collect();
    let ask = |at: usize, key: &str, parts: &mut Vec<String>| {
        if parts.get(at).is_none_or(String::is_empty) {
            if parts.len() <= at {
                parts.resize(at + 1, String::new());
            }
            parts[at] = text(request, key).to_owned();
        }
    };
    match parts.get(1).map(String::as_str) {
        Some(
            "action_add" | "action_edit" | "action_duplicate" | "group_rename" | "group_icon" | "group_settings"
            | "export",
        ) => ask(2, "itemId", &mut parts),
        Some("trigger_add") => {
            ask(2, "actionId", &mut parts);
            ask(3, "code", &mut parts)
        }
        Some("trigger_edit" | "trigger_test" | "sub_edit") => {
            ask(2, "actionId", &mut parts);
            ask(3, "itemId", &mut parts)
        }
        Some("sub_add") => {
            ask(2, "actionId", &mut parts);
            ask(3, "code", &mut parts);
            ask(4, "anchor", &mut parts)
        }
        _ => {}
    }
    // An action may be named by its row id (`a:<id>`) as well as by its id.
    if matches!(
        parts.get(1).map(String::as_str),
        Some("trigger_add" | "trigger_edit" | "trigger_test" | "sub_add" | "sub_edit")
    ) {
        if let Some(action) = parts.get_mut(2) {
            *action = plain_id(action).to_owned();
        }
    }
    parts
}

fn kind_of(parts: &[String]) -> String {
    parts.iter().map(|part| enc(part)).collect::<Vec<_>>().join(":")
}

fn action_item(action: &Action) -> Obj {
    let value = json!({"id": action.id, "name": action.name, "group": action.group,
        "queue": action.queue.clone().filter(|queue| !queue.is_empty()).unwrap_or_else(|| crate::engine::DEFAULT_QUEUE.into()),
        "enabled": action.enabled, "randomAction": action.random_action, "concurrent": action.concurrent,
        "alwaysRun": action.always_run, "excludeFromHistory": action.exclude_from_history,
        "excludeFromPending": action.exclude_from_pending});
    value.as_object().cloned().unwrap_or_default()
}

fn obj(value: Value) -> Obj {
    value.as_object().cloned().unwrap_or_default()
}

/// A plugin's pick: `p:<plugin>:<name>`.
fn plugin_pick(code: &str) -> Option<(String, String)> {
    let rest = code.strip_prefix("p:")?;
    let (plugin, name) = rest.split_once(':')?;
    Some((dec(plugin), dec(name)))
}

/// The fields a trigger or sub-action of this kind is edited with (None: JSON only).
fn automation_fields(engine: &Rc<Engine>, trigger: bool, item: &Obj) -> Option<&'static [F]> {
    let code = edit::kind(item);
    if !trigger && code == crate::engine::plugins::SUB_PLUGIN {
        let value = Value::Object(item.clone());
        let (plugin, name) = (text(&value, "plugin").to_owned(), text(&value, "name").to_owned());
        let on = engine.plugins_on();
        let sub = on
            .iter()
            .find(|entry| entry.0 == plugin)?
            .1
            .subactions
            .iter()
            .find(|sub| sub.name == name)?
            .clone();
        return Some(schema::plugin_fields(&format!("{plugin}/{name}"), &sub.fields));
    }
    if trigger {
        schema::trigger_fields(code)
    } else {
        schema::sub_fields(code)
    }
}

fn automation_form(
    engine: &Rc<Engine>,
    config: &crate::Config,
    parts: &[String],
    trigger: bool,
    mut item: Obj,
    title: String,
    hint: String,
) -> Value {
    if !trigger {
        item.shift_remove("subActions");
    }
    let fields = automation_fields(engine, trigger, &item);
    let mut out = model(
        config,
        kind_of(parts),
        title,
        &hint,
        fields.unwrap_or(&[]),
        item.clone(),
    );
    if fields.is_none() {
        out["jsonOnly"] = true.into();
        out["jsonText"] = serde_json::to_string_pretty(&Value::Object(item))
            .unwrap_or_default()
            .into();
    }
    out
}

/// The form for a button of this page, with its current values.
pub fn form_for(engine: &Rc<Engine>, request: &Value) -> Result<Value, String> {
    let parts = context(request);
    let config = engine.config();
    let part = |at: usize| parts.get(at).map(String::as_str).unwrap_or_default();
    let simple = |parts: &[String], title: String, fields: &[F], item: Obj, hint: &str| {
        let mut out = model(&config, kind_of(parts), title, hint, fields, item);
        out["json"] = false.into();
        out
    };
    match part(1) {
        "action_add" => {
            let node = part(2);
            let group = if let Some(group) = node.strip_prefix("g:") {
                group.to_owned()
            } else if let Some(id) = node.strip_prefix("a:") {
                config_action(&config, id).map(|a| a.group.clone()).unwrap_or_default()
            } else {
                node.to_owned()
            };
            let mut parts = parts.clone();
            parts.truncate(2);
            parts.resize(3, String::new());
            parts[2] = group.clone();
            Ok(simple(
                &parts,
                tr("actions.new_action").into(),
                NAME_FIELDS,
                obj(json!({"id": "", "name": ""})),
                "",
            ))
        }
        "action_edit" => {
            let action = config_action(&config, plain_id(part(2))).ok_or_else(item_gone)?;
            let mut out = model(
                &config,
                kind_of(&["act".into(), "action_edit".into(), action.id.clone()]),
                tr("dialogs.action.title").into(),
                "",
                ACTION_FIELDS,
                action_item(action),
            );
            out["json"] = false.into();
            Ok(out)
        }
        "action_duplicate" => {
            let action = config_action(&config, plain_id(part(2))).ok_or_else(item_gone)?;
            let name = trf("actions.copy_suffix", &[("name", &action.name)]);
            Ok(simple(
                &["act".into(), "action_duplicate".into(), action.id.clone()],
                tr("actions.duplicate_action").into(),
                NAME_FIELDS,
                obj(json!({"id": "", "name": name})),
                "",
            ))
        }
        "group_add" => Ok(simple(
            &["act".into(), "group_add".into()],
            tr("btn.add_group").into(),
            NAME_FIELDS,
            obj(json!({"id": "", "name": ""})),
            "",
        )),
        "group_rename" => {
            let group = part(2).strip_prefix("g:").unwrap_or(part(2)).to_owned();
            if !crate::groups::exists(&config, &group) {
                return Err(item_gone());
            }
            Ok(simple(
                &["act".into(), "group_rename".into(), group.clone()],
                tr("btn.rename_group").into(),
                NAME_FIELDS,
                obj(json!({"id": "", "name": group})),
                "",
            ))
        }
        "group_icon" => {
            let group = part(2).strip_prefix("g:").unwrap_or(part(2)).to_owned();
            Ok(simple(
                &["act".into(), "group_icon".into(), group],
                tr("btn.set_group_icon").into(),
                ICON_FIELDS,
                obj(json!({"id": "", "file": ""})),
                "",
            ))
        }
        "import" => Ok(simple(
            &["act".into(), "import".into()],
            tr("dialogs.import.title").into(),
            IMPORT_FIELDS,
            obj(json!({"id": "", "text": "", "file": ""})),
            "",
        )),
        "export" => {
            let selected = config_action(&config, plain_id(part(2)));
            let group = part(2)
                .strip_prefix("g:")
                .filter(|group| crate::groups::exists(&config, group));
            let name = selected
                .map(|a| a.name.clone())
                .or_else(|| group.map(str::to_owned))
                .unwrap_or_default();
            let scope = usize::from(selected.is_none() && group.is_none());
            Ok(simple(
                &["act".into(), "export".into(), part(2).to_owned()],
                tr("dialogs.export.title").into(),
                EXPORT_FIELDS,
                obj(
                    json!({"id": "", "scope": scope, "name": name, "author": "", "version": "1.0.0", "description": "", "file": ""}),
                ),
                tr("dialogs.export.hint"),
            ))
        }
        "group_settings" => {
            let group = part(2).strip_prefix("g:").unwrap_or(part(2)).to_owned();
            if !crate::groups::exists(&config, &group) {
                return Err(item_gone());
            }
            Ok(group_settings_form(&config, &group))
        }
        "trigger_add" | "sub_add" => {
            let trigger = part(1) == "trigger_add";
            let action = config_action(&config, part(2)).ok_or_else(item_gone)?;
            let code = part(3);
            let (item, title, hint) = if trigger {
                let item = match plugin_pick(code) {
                    Some((plugin, event)) => {
                        let mut item = edit::new_trigger(crate::engine::plugins::T_PLUGIN);
                        item.insert("plugin".into(), plugin.into());
                        item.insert("event".into(), event.into());
                        item
                    }
                    None => edit::new_trigger(code.parse().map_err(|_| "pick a trigger")?),
                };
                let title = labels::trigger_title(edit::kind(&item));
                let hint = crate::docs::summary("trigger", edit::kind(&item) as u32)
                    .unwrap_or(tr("actions.trigger_hint"))
                    .to_owned();
                (item, title, hint)
            } else {
                let mut item = match plugin_pick(code) {
                    Some((plugin, name)) => {
                        let mut item = edit::new_sub(crate::engine::plugins::SUB_PLUGIN);
                        item.insert("plugin".into(), plugin.into());
                        item.insert("name".into(), name.into());
                        item
                    }
                    None => edit::new_sub(code.parse().map_err(|_| "pick a sub-action")?),
                };
                if edit::kind(&item) == 99903 {
                    item.insert("values".into(), json!([]));
                }
                let title = labels::sub_title(edit::kind(&item));
                (item, title, String::new())
            };
            // The new item keeps its id as the form is asked for again.
            let slot = if trigger { 4 } else { 5 };
            let mut parts = parts.clone();
            parts.resize(slot.max(parts.len()), String::new());
            let mut item = item;
            if parts.get(slot).is_none_or(String::is_empty) {
                parts.resize(slot + 1, String::new());
                parts[slot] = field(&item, "id").to_owned();
            } else {
                item.insert("id".into(), parts[slot].clone().into());
            }
            let _ = action;
            Ok(automation_form(engine, &config, &parts, trigger, item, title, hint))
        }
        "trigger_edit" => {
            let action = config_action(&config, part(2)).ok_or_else(item_gone)?;
            let at = trigger_index(action, part(3)).ok_or_else(item_gone)?;
            let item = action.triggers[at].clone();
            let code = edit::kind(&item) as u32;
            let hint = crate::docs::summary("trigger", code)
                .unwrap_or(tr("actions.trigger_hint"))
                .to_owned();
            let mut out = automation_form(
                engine,
                &config,
                &parts,
                true,
                item.clone(),
                labels::trigger_title(u64::from(code)),
                hint,
            );
            if automation_fields(engine, true, &item).is_none() && !crate::docs::available("trigger", code) {
                out["hint"] = tr("actions.trigger_hint").into();
            }
            Ok(out)
        }
        "trigger_test" => {
            let action = config_action(&config, part(2)).ok_or_else(item_gone)?;
            let at = trigger_index(action, part(3)).ok_or_else(item_gone)?;
            let _ = at;
            Ok(simple(
                &parts,
                tr("actions.test_trigger_title").into(),
                TEST_FIELDS,
                obj(json!({"id": "", "args": {}})),
                tr("actions.test_trigger_hint"),
            ))
        }
        "sub_edit" => {
            let action = config_action(&config, part(2)).ok_or_else(item_gone)?;
            let path = sub_path(action, part(3)).ok_or_else(item_gone)?;
            let item = edit::get(&action.sub_actions, &path).cloned().ok_or_else(item_gone)?;
            let code = edit::kind(&item);
            let title = labels::sub_title(code);
            let hint = match SUB_ACTION_INFO.binary_search_by_key(&(code as u32), |x| x.0) {
                Ok(i) => trf(
                    "actions.subaction_fields_hint",
                    &[("title", &title), ("fields", &SUB_ACTION_INFO[i].3.join(", "))],
                ),
                Err(_) => title.clone(),
            };
            Ok(automation_form(engine, &config, &parts, false, item, title, hint))
        }
        other => Err(format!("no form {other}")),
    }
}

/// The fields a form is edited with, for validating its changes.
fn fields_of(engine: &Rc<Engine>, parts: &[String], form: &Value) -> Result<&'static [F], String> {
    let item = form["item"].as_object().cloned().unwrap_or_default();
    Ok(match parts.get(1).map(String::as_str).unwrap_or_default() {
        "action_edit" => ACTION_FIELDS,
        "action_add" | "action_duplicate" | "group_add" | "group_rename" => NAME_FIELDS,
        "group_icon" => ICON_FIELDS,
        "import" => IMPORT_FIELDS,
        "export" => EXPORT_FIELDS,
        "trigger_test" => TEST_FIELDS,
        "trigger_add" | "trigger_edit" => automation_fields(engine, true, &item).unwrap_or(&[]),
        "sub_add" | "sub_edit" => automation_fields(engine, false, &item).unwrap_or(&[]),
        other => return Err(format!("no form {other}")),
    })
}

/// `StreamerzEditEntity` / `StreamerzEntityDraft` for these forms.
pub fn edit_form(engine: &Rc<Engine>, request: &Value, preview: bool) -> Result<Value, String> {
    let parts = context(request);
    if parts.get(1).map(String::as_str) == Some("group_settings") {
        return group_settings_edit(engine, parts.get(2).map_or("", String::as_str), request, preview);
    }
    let form = form_for(engine, request)?;
    let fields = fields_of(engine, &parts, &form)?;
    let item = entities::draft_with(form.clone(), fields, request)?;
    let part = |at: usize| parts.get(at).map(String::as_str).unwrap_or_default();
    if matches!(part(1), "trigger_add" | "trigger_edit" | "sub_add" | "sub_edit") {
        let original = form["item"].as_object().cloned().unwrap_or_default();
        if edit::kind(&item) != edit::kind(&original) {
            return Err(trf(
                "actions.json_editor_hint",
                &[("type", &edit::kind(&original).to_string())],
            ));
        }
    }
    if preview {
        let text = serde_json::to_string_pretty(&item).map_err(|error| error.to_string())?;
        return Ok(json!({"item": item, "json": text}));
    }
    save_form(engine, &parts, item)
}

fn picked(id: &str) -> Value {
    json!({"saved": id})
}

fn edit_action<T>(
    engine: &Rc<Engine>,
    id: &str,
    change: impl FnOnce(&mut Action) -> Result<T, String>,
) -> Result<T, String> {
    let mut result = Err(item_gone());
    engine.change(|config| {
        if let Some(action) = config.actions.actions.iter_mut().find(|action| action.id == id) {
            result = change(action);
        }
    });
    result
}

fn save_form(engine: &Rc<Engine>, parts: &[String], item: Obj) -> Result<Value, String> {
    let part = |at: usize| parts.get(at).map(String::as_str).unwrap_or_default();
    let value = Value::Object(item.clone());
    let name_of = || -> Result<String, String> {
        let name = text(&value, "name").trim();
        if name.is_empty() {
            Err(tr("dialogs.action.needs_name").into())
        } else {
            Ok(name.to_owned())
        }
    };
    match part(1) {
        "action_add" => {
            let action = edit::new_action(&name_of()?, part(2));
            let id = action.id.clone();
            engine.change(|config| config.actions.actions.push(action));
            Ok(picked(&format!("a:{id}")))
        }
        "action_edit" => {
            let name = name_of()?;
            let id = part(2).to_owned();
            edit_action(engine, &id, |action| {
                action.name = name;
                action.group = text(&value, "group").trim().to_owned();
                action.queue = Some(text(&value, "queue").to_owned()).filter(|queue| !queue.is_empty());
                for (key, field) in [
                    ("enabled", &mut action.enabled),
                    ("randomAction", &mut action.random_action),
                    ("concurrent", &mut action.concurrent),
                    ("alwaysRun", &mut action.always_run),
                    ("excludeFromHistory", &mut action.exclude_from_history),
                    ("excludeFromPending", &mut action.exclude_from_pending),
                ] {
                    *field = value.get(key).and_then(Value::as_bool).unwrap_or(*field);
                }
                Ok(())
            })?;
            Ok(picked(&format!("a:{id}")))
        }
        "action_duplicate" => {
            let name = name_of()?;
            let source = config_action(&engine.config(), part(2))
                .cloned()
                .ok_or_else(item_gone)?;
            let copy = edit::duplicate_action(&source, &name);
            let id = copy.id.clone();
            engine.change(|config| config.actions.actions.push(copy));
            Ok(picked(&format!("a:{id}")))
        }
        "group_add" => {
            let name = name_of()?;
            let mut created = false;
            engine.change(|config| created = crate::groups::create(config, &name));
            if created {
                Ok(picked(&format!("g:{name}")))
            } else {
                Err(tr("actions.group_exists").into())
            }
        }
        "group_rename" => {
            let name = name_of()?;
            let mut renamed = false;
            engine.change(|config| renamed = crate::groups::rename(config, part(2), &name));
            if renamed || name == part(2) {
                Ok(picked(&format!("g:{name}")))
            } else {
                Err(tr("actions.group_exists").into())
            }
        }
        "group_icon" => {
            let path = text(&value, "file").trim();
            let bytes = std::fs::read(path).map_err(|_| tr("actions.icon_read_error").to_owned())?;
            let icon = crate::group_icons::encode(&bytes).map_err(|error| {
                match error {
                    "Image is larger than 1 MB" => tr("actions.icon_too_large"),
                    "Choose a PNG or JPEG image" => tr("actions.icon_format"),
                    _ => tr("actions.icon_read_error"),
                }
                .to_owned()
            })?;
            let group = part(2).to_owned();
            engine.change(|config| {
                config.action_group_icons.insert(group.clone(), icon);
            });
            Ok(picked(&format!("g:{group}")))
        }
        "import" => import_text(engine, &value),
        "export" => export_text(engine, part(2), &value),
        "trigger_add" => {
            let mut trigger = item;
            let id = trigger_id(&trigger, 0);
            if trigger.get("id").is_none() {
                trigger.insert("id".into(), crate::engine::uuid().into());
            }
            edit_action(engine, part(2), |action| {
                action.triggers.push(trigger);
                Ok(())
            })?;
            Ok(picked(&id))
        }
        "trigger_edit" => {
            let id = part(3).to_owned();
            edit_action(engine, part(2), |action| {
                let at = trigger_index(action, &id).ok_or_else(item_gone)?;
                let mut trigger = item;
                if let Some(keep) = action.triggers[at].get("id") {
                    trigger.insert("id".into(), keep.clone());
                }
                action.triggers[at] = trigger;
                Ok(())
            })?;
            Ok(picked(&id))
        }
        "trigger_test" => {
            let action_id = part(2).to_owned();
            let config = engine.config();
            let action = config_action(&config, &action_id).ok_or_else(item_gone)?;
            let at = trigger_index(action, part(3)).ok_or_else(item_gone)?;
            let code = edit::kind(&action.triggers[at]) as u32;
            let mut args = value["args"].as_object().cloned().unwrap_or_default();
            args.insert("triggerId".into(), trigger_id(&action.triggers[at], at).into());
            args.insert("triggerName".into(), name(TRIGGERS, code).unwrap_or("").into());
            args.insert("isTest".into(), true.into());
            drop(config);
            engine
                .queue(&action_id, args)
                .map(|_| json!({"saved": part(3)}))
                .ok_or_else(|| tr("actions.test_disabled").into())
        }
        "sub_add" => {
            let mut sub = item;
            let fresh = edit::new_sub(edit::kind(&sub));
            if let Some(kids) = fresh.get("subActions") {
                sub.insert("subActions".into(), kids.clone());
            }
            let code = edit::kind(&sub);
            let id = sub_id(&sub, &[0]);
            let anchor = part(4).to_owned();
            edit_action(engine, part(2), |action| {
                let at = if anchor.is_empty() {
                    None
                } else {
                    sub_path(action, &anchor)
                };
                let list = &mut action.sub_actions;
                let (parent, position) = match at
                    .as_deref()
                    .and_then(|path| edit::get(list, path).map(|o| (path, edit::kind(o))))
                {
                    // Switch cases go into the Switch.
                    Some((path, kind)) if code == 99903 && kind == 127 => (path.to_vec(), usize::MAX),
                    Some((path, kind)) if edit::holds_any(kind) && code != 99903 => (path.to_vec(), usize::MAX),
                    Some((path, _)) => {
                        let (last, parent) = path.split_last().ok_or_else(item_gone)?;
                        (parent.to_vec(), last + 1)
                    }
                    None => (Vec::new(), usize::MAX),
                };
                edit::insert(list, &parent, position, sub);
                Ok(())
            })?;
            Ok(picked(&id))
        }
        "sub_edit" => {
            let id = part(3).to_owned();
            edit_action(engine, part(2), |action| {
                let path = sub_path(action, &id).ok_or_else(item_gone)?;
                let old = edit::get(&action.sub_actions, &path).cloned().ok_or_else(item_gone)?;
                let mut new = item;
                for field in ["id", "parentId", "index"] {
                    if let Some(keep) = old.get(field) {
                        new.insert(field.into(), keep.clone());
                    }
                }
                // Children go back where Streamer.bot keeps them: after its own fields, before the id.
                if let Some(kids) = old.get("subActions") {
                    let at = new.keys().position(|key| key == "id").unwrap_or(new.len());
                    new.shift_insert(at, "subActions".into(), kids.clone());
                }
                edit::update(&mut action.sub_actions, &path, |sub| *sub = new);
                Ok(())
            })?;
            Ok(picked(&id))
        }
        other => Err(format!("no form {other}")),
    }
}

// Group settings: the description and the values by form, the definitions, files and members as JSON.

fn group_settings_form(config: &crate::Config, group: &str) -> Value {
    let meta = crate::groups::meta(config, group);
    let values = crate::groups::values(config, group);
    let mut fields = vec![json!({"key": "description", "label": tr("dialogs.export.description"),
        "widget": {"kind": "multi"}, "holds": "text", "value": meta.description})];
    for def in &meta.variables {
        let label = if def.label.is_empty() {
            def.name.clone()
        } else {
            def.label.clone()
        };
        let value = values.get(&def.name).cloned().unwrap_or(Value::Null);
        let field = match def.kind {
            crate::groups::Kind::Boolean => json!({"key": def.name, "label": label, "widget": {"kind": "bool"},
                "holds": "true/false", "value": value.as_bool().unwrap_or(false)}),
            crate::groups::Kind::Choice => json!({"key": def.name, "label": label,
                "widget": {"kind": "pick", "options": def.options},
                "holds": "one of the options",
                "value": def.options.iter().position(|option| value.as_str() == Some(option)).unwrap_or(0)}),
            kind => json!({"key": def.name, "label": label, "widget": {"kind": "text"}, "holds": kind.name(),
                "value": kind.show(&value)}),
        };
        fields.push(field);
    }
    let item = json!({"id": "", "description": meta.description});
    json!({"kind": kind_of(&["act".into(), "group_settings".into(), group.into()]),
        "title": trf("group.settings_title", &[("name", group)]), "hint": tr("group.hint"),
        "new": item, "item": item, "fields": fields})
}

fn group_settings_edit(engine: &Rc<Engine>, group: &str, request: &Value, preview: bool) -> Result<Value, String> {
    let config = engine.config();
    if !crate::groups::exists(&config, group) {
        return Err(item_gone());
    }
    let mut meta = crate::groups::meta(&config, group);
    let mut values = Obj::new();
    if let Some(text) = request["json"].as_str() {
        meta = serde_json::from_str(text)
            .map_err(|error| trf("dialogs.not_valid_json", &[("error", &error.to_string())]))?;
    } else {
        for change in request["changes"].as_array().ok_or("changes: an array")? {
            let index = change["field"]
                .as_u64()
                .ok_or_else(|| tr("forms.invalid_field").to_owned())? as usize;
            let value = &change["value"];
            if index == 0 {
                meta.description = value
                    .as_str()
                    .ok_or_else(|| tr("forms.invalid_field").to_owned())?
                    .to_owned();
                continue;
            }
            let def = meta
                .variables
                .get(index - 1)
                .ok_or_else(|| tr("forms.invalid_field").to_owned())?;
            let parsed = match def.kind {
                crate::groups::Kind::Boolean => value.as_bool().map(Value::from),
                crate::groups::Kind::Choice => value
                    .as_u64()
                    .and_then(|at| def.options.get(at as usize))
                    .map(|option| Value::from(option.clone())),
                _ => value.as_str().and_then(|text| def.parse(text).ok()),
            };
            values.insert(
                def.name.clone(),
                parsed.ok_or_else(|| trf("forms.invalid_value", &[("label", &def.label)]))?,
            );
        }
    }
    drop(config);
    if preview {
        let item = serde_json::to_value(&meta).map_err(|error| error.to_string())?;
        let text = serde_json::to_string_pretty(&item).map_err(|error| error.to_string())?;
        return Ok(json!({"item": item, "json": text}));
    }
    let mut result = Ok(());
    engine.change(|config| {
        result = crate::groups::set_meta(config, group, meta).and_then(|()| {
            let mut stored = crate::groups::values(config, group);
            stored.extend(values);
            crate::groups::set_values(config, group, stored)
        });
    });
    result?;
    Ok(picked(&format!("g:{group}")))
}

// Import and export

fn import_text(engine: &Rc<Engine>, value: &Value) -> Result<Value, String> {
    let mut source = text(value, "text").trim().to_owned();
    let file = text(value, "file").trim();
    if source.is_empty() && !file.is_empty() {
        source = std::fs::read_to_string(file)
            .map_err(|error| {
                trf(
                    "dialogs.import.cant_read",
                    &[("file", file), ("error", &error.to_string())],
                )
            })?
            .trim()
            .to_owned();
    }
    let export = crate::model::read_export(&source)
        .map_err(|error| trf("dialogs.import.cant_import", &[("error", &error.to_string())]))?;
    ask_import(engine, export);
    Ok(json!({"saved": ""}))
}

/// What the import holds and what it runs, as the confirmation reads.
pub fn import_question(export: &Obj) -> String {
    let programs = edit::programs(export);
    let warn = if programs.is_empty() {
        String::new()
    } else {
        let mut list: Vec<String> = programs.iter().take(8).cloned().collect();
        if programs.len() > 8 {
            list.push(trf(
                "actions.import_more",
                &[("count", &(programs.len() - 8).to_string())],
            ));
        }
        trf("actions.import_warn", &[("programs", &list.join("\n"))])
    };
    trf(
        "actions.import_confirm",
        &[("summary", &edit::summary(export)), ("warn", &warn)],
    )
}

/// Ask first (a core prompt anyone can answer), then import.
fn ask_import(engine: &Rc<Engine>, export: Obj) {
    let answer = crate::host::confirm(import_question(&export), tr("dialogs.cancel"), tr("btn.import"));
    let engine = engine.clone();
    tokio::task::spawn_local(async move {
        if answer.await.unwrap_or(false) {
            finish_import(&engine, &export);
        }
    });
}

/// Import without asking; says what could not run.
pub fn finish_import(engine: &Rc<Engine>, export: &Obj) {
    let mut result = Ok(Vec::new());
    engine.change(|config| result = edit::import(config, export));
    match result {
        Ok(unsupported) if unsupported.is_empty() => crate::say(tr("actions.imported_toast").into()),
        Ok(unsupported) => {
            let message = trf("actions.imported_unsupported", &[("items", &unsupported.join(", "))]);
            crate::warn(message.clone());
            super::prompts::notice(tr("dialogs.import.title").into(), message);
        }
        Err(error) => super::prompts::notice(tr("dialogs.import.title").into(), error),
    }
}

fn export_text(engine: &Rc<Engine>, selected: &str, value: &Value) -> Result<Value, String> {
    let config = engine.config();
    let ids: Vec<String> = if value["scope"].as_u64() == Some(1) {
        config.actions.actions.iter().map(|action| action.id.clone()).collect()
    } else if let Some(group) = selected.strip_prefix("g:") {
        return export_group_text(&config, group, value);
    } else {
        vec![plain_id(selected).to_owned()]
    };
    if ids.iter().all(String::is_empty) {
        return Err(tr("dialogs.export.tick_at_least_one").into());
    }
    let groups: Vec<String> = if value["scope"].as_u64() == Some(1) {
        group_names(&config)
    } else {
        Vec::new()
    };
    let export = edit::export_with(
        &config,
        &ids,
        &groups,
        [
            text(value, "name"),
            text(value, "author"),
            text(value, "version"),
            text(value, "description"),
        ],
    );
    deliver(&export, value)
}

fn export_group_text(config: &crate::Config, group: &str, value: &Value) -> Result<Value, String> {
    let export = edit::export_group(
        config,
        group,
        [
            if text(value, "name").is_empty() {
                group
            } else {
                text(value, "name")
            },
            text(value, "author"),
            text(value, "version"),
            text(value, "description"),
        ],
    );
    deliver(&export, value)
}

fn deliver(export: &Obj, value: &Value) -> Result<Value, String> {
    let output = crate::model::write_export(export);
    let file = text(value, "file").trim();
    if !file.is_empty() {
        std::fs::write(file, &output).map_err(|error| {
            trf(
                "dialogs.export.cant_save",
                &[("file", file), ("error", &error.to_string())],
            )
        })?;
    }
    Ok(json!({"saved": "", "copy": output, "message": tr("actions.export_copied"), "text": output}))
}

// Operations

fn accepted() -> Result<Value, String> {
    Ok(json!({"accepted": true}))
}

/// Move a sub-action: `to` is the target's id (empty: the end of the list), `before` puts it above.
fn move_sub(action: &mut Action, id: &str, to: &str) -> Result<(), String> {
    let from = sub_path(action, id).ok_or_else(item_gone)?;
    let target = if to.is_empty() {
        Some((Vec::new(), usize::MAX))
    } else {
        let path = sub_path(action, to).ok_or_else(item_gone)?;
        edit::drop_target(&action.sub_actions, &path)
    };
    let (parent, at) = target.ok_or_else(item_gone)?;
    if edit::move_to(&mut action.sub_actions, &from, &parent, at) {
        Ok(())
    } else {
        Err(tr("actions.branch_delete_warning").into())
    }
}

fn step(action: &mut Action, id: &str, how: &str) -> Result<(), String> {
    let from = sub_path(action, id).ok_or_else(item_gone)?;
    let (last, parent) = from.split_last().ok_or_else(item_gone)?;
    let moved = match how {
        "up" if *last > 0 => edit::move_to(&mut action.sub_actions, &from, parent, last - 1),
        "down" => edit::move_to(&mut action.sub_actions, &from, parent, last + 2),
        "in" if *last > 0 => {
            let mut before = parent.to_vec();
            before.push(last - 1);
            let holds = edit::get(&action.sub_actions, &before).is_some_and(|o| edit::holds_any(edit::kind(o)));
            holds && edit::move_to(&mut action.sub_actions, &from, &before, usize::MAX)
        }
        "out" if !parent.is_empty() => {
            // Out of a branch means out of its If/Else or Switch too.
            let mut up = parent.to_vec();
            if edit::get(&action.sub_actions, &up).is_some_and(|o| edit::fixed(edit::kind(o))) {
                up.pop();
            }
            match up.split_last() {
                Some((index, outer)) => edit::move_to(&mut action.sub_actions, &from, outer, index + 1),
                None => false,
            }
        }
        _ => false,
    };
    // A move that cannot happen (already first, no container) changes nothing and is not an error.
    let _ = moved;
    Ok(())
}

fn help(kind: &str, code: u32, fallback: String) -> Result<Value, String> {
    let (title, body) = match crate::docs::entry(kind, code) {
        Some(entry) => (entry.title, entry.text),
        None if !fallback.is_empty() => (String::new(), fallback),
        None => return Err(tr("actions.no_help").into()),
    };
    super::prompts::notice(title, body);
    accepted()
}

/// One operation of the page. Row ids come from the models above.
pub fn act(engine: &Rc<Engine>, request: &Value) -> Result<Value, String> {
    let operation = text(request, "action");
    let item = text(request, "itemId");
    let action_id = plain_id(text(request, "actionId")).to_owned();
    match operation {
        "refresh" => accepted(),
        "action_toggle" => {
            edit_action(engine, plain_id(item), |action| {
                action.enabled = !action.enabled;
                Ok(())
            })?;
            accepted()
        }
        "action_delete" => {
            let id = plain_id(item).to_owned();
            if config_action(&engine.config(), &id).is_none() {
                return Err(item_gone());
            }
            engine.change(|config| config.actions.actions.retain(|action| action.id != id));
            accepted()
        }
        "action_run" => {
            let config = engine.config();
            let action = config_action(&config, plain_id(item)).ok_or_else(item_gone)?;
            let id = action.id.clone();
            drop(config);
            engine
                .test(&id)
                .map(|_| json!({"accepted": true}))
                .ok_or_else(|| tr("actions.test_disabled").into())
        }
        "action_move" => {
            let group = text(request, "to").trim().to_owned();
            edit_action(engine, plain_id(item), |action| {
                action.group = group;
                Ok(())
            })?;
            accepted()
        }
        "group_delete" => {
            let group = plain_id(item).to_owned();
            if !crate::groups::exists(&engine.config(), &group) {
                return Err(item_gone());
            }
            engine.change(|config| crate::groups::remove(config, &group));
            accepted()
        }
        "group_icon_clear" => {
            let group = plain_id(item).to_owned();
            engine.change(|config| {
                config.action_group_icons.remove(&group);
            });
            accepted()
        }
        "template_add" => {
            let code = text(request, "code");
            let template = crate::templates::TEMPLATES
                .iter()
                .find(|template| template.name == code || template.title() == code)
                .ok_or("template not found")?;
            let made = (template.make)();
            let names: Vec<String> = made
                .actions
                .iter()
                .map(|action| action.name.clone())
                .chain(
                    made.commands
                        .iter()
                        .map(|command| format!("command {}", command["command"].as_str().unwrap_or(""))),
                )
                .chain(
                    made.timers
                        .iter()
                        .map(|timer| format!("timer {}", timer["name"].as_str().unwrap_or(""))),
                )
                .collect();
            let timers = !made.timers.is_empty();
            let first = made.actions.first().map(|action| action.id.clone());
            let mut added = Ok(());
            engine.change(|config| added = made.add_to(config));
            added?;
            if timers {
                engine.timers();
            }
            crate::say(trf(
                "actions.added_from_template",
                &[("name", template.title()), ("items", &names.join(", "))],
            ));
            Ok(json!({"accepted": true, "saved": first.map(|id| format!("a:{id}")).unwrap_or_default()}))
        }
        "trigger_toggle" => {
            edit_action(engine, &action_id, |action| {
                let at = trigger_index(action, item).ok_or_else(item_gone)?;
                let on = labels::trigger_on(&action.triggers[at]);
                action.triggers[at].insert("enabled".into(), (!on).into());
                Ok(())
            })?;
            accepted()
        }
        "trigger_delete" => {
            edit_action(engine, &action_id, |action| {
                let at = trigger_index(action, item).ok_or_else(item_gone)?;
                action.triggers.remove(at);
                Ok(())
            })?;
            accepted()
        }
        "trigger_help" => {
            let config = engine.config();
            let action = config_action(&config, &action_id).ok_or_else(item_gone)?;
            let at = trigger_index(action, item).ok_or_else(item_gone)?;
            help("trigger", edit::kind(&action.triggers[at]) as u32, String::new())
        }
        "sub_help" => {
            let config = engine.config();
            let action = config_action(&config, &action_id).ok_or_else(item_gone)?;
            let path = sub_path(action, item).ok_or_else(item_gone)?;
            let code = edit::get(&action.sub_actions, &path)
                .map(edit::kind)
                .ok_or_else(item_gone)?;
            help("subaction", code as u32, String::new())
        }
        "sub_toggle" => {
            edit_action(engine, &action_id, |action| {
                let path = sub_path(action, item).ok_or_else(item_gone)?;
                edit::update(&mut action.sub_actions, &path, |sub| {
                    let on = sub.get("enabled").and_then(Value::as_bool).unwrap_or(true);
                    sub.insert("enabled".into(), (!on).into());
                });
                Ok(())
            })?;
            accepted()
        }
        "sub_delete" => {
            edit_action(engine, &action_id, |action| {
                let path = sub_path(action, item).ok_or_else(item_gone)?;
                let fixed = edit::get(&action.sub_actions, &path).is_none_or(|sub| edit::fixed(edit::kind(sub)));
                if fixed {
                    return Err(tr("actions.branch_delete_warning").into());
                }
                edit::remove(&mut action.sub_actions, &path);
                Ok(())
            })?;
            accepted()
        }
        "sub_up" | "sub_down" | "sub_in" | "sub_out" => {
            let how = operation.trim_start_matches("sub_").to_owned();
            edit_action(engine, &action_id, |action| step(action, item, &how))?;
            accepted()
        }
        "sub_move" => {
            let to = text(request, "to").to_owned();
            edit_action(engine, &action_id, |action| move_sub(action, item, &to))?;
            accepted()
        }
        other => Err(format!("actions has no operation {other}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::servers::answer;

    fn ask(engine: &Rc<Engine>, request: Value) -> Value {
        answer(engine, &request, true).unwrap()
    }
    fn field(form: &Value, key: &str) -> usize {
        form["fields"]
            .as_array()
            .unwrap()
            .iter()
            .position(|field| field["key"] == key)
            .unwrap()
    }
    fn view(engine: &Rc<Engine>, action: &str) -> Value {
        ask(
            engine,
            json!({"request": "StreamerzCoreView", "domain": "actions", "actionId": action}),
        )
    }
    fn add_action(engine: &Rc<Engine>, name: &str, group: &str) -> String {
        let form = ask(
            engine,
            json!({"request": "StreamerzEntityForm", "kind": "act:action_add", "itemId": format!("g:{group}")}),
        );
        let saved = ask(
            engine,
            json!({"request": "StreamerzEditEntity", "kind": form["kind"], "itemId": "", "changes": [{"field": 0, "value": name}]}),
        );
        assert_eq!(saved["status"], "ok", "{saved}");
        saved["saved"].as_str().unwrap().to_owned()
    }

    #[test]
    fn a_groups_picture_is_written_once_and_named_in_its_tree_node() {
        let _lock = crate::TEST_LOCK.lock().unwrap_or_else(|error| error.into_inner());
        crate::engine::tests::run(async {
            let engine = crate::engine::tests::engine(json!({"actions": []}));
            add_action(&engine, "Alert", "Alerts");
            add_action(&engine, "Other", "Plain");
            let icon = crate::group_icons::encode(b"\x89PNG\r\n\x1a\nbytes").unwrap();
            engine.change(|config| {
                config.action_group_icons.insert("Alerts".into(), icon.clone());
                // A saved icon that does not match its digest is not drawn.
                let mut broken = icon.clone();
                broken["sha256"] = "0".repeat(64).into();
                config.action_group_icons.insert("Plain".into(), broken);
            });
            let page = view(&engine, "");
            let nodes = page["actions"]["tree"]["nodes"].as_array().unwrap();
            let node = |name: &str| nodes.iter().find(|node| node["label"] == name).unwrap();
            assert_eq!(node("Alerts")["hasIcon"], true);
            let file = node("Alerts")["iconFile"].as_str().unwrap();
            assert!(
                file.ends_with(&format!("{}.png", icon["sha256"].as_str().unwrap())),
                "{file}"
            );
            assert_eq!(std::fs::read(file).unwrap(), b"\x89PNG\r\n\x1a\nbytes");
            assert!(node("Plain")["iconFile"].is_null());
            let _ = std::fs::remove_file(file);
        });
    }

    #[test]
    fn actions_are_added_listed_by_group_edited_duplicated_toggled_and_deleted() {
        let _lock = crate::TEST_LOCK.lock().unwrap_or_else(|error| error.into_inner());
        crate::engine::tests::run(async {
            let engine = crate::engine::tests::engine(json!({"actions": []}));
            let empty = view(&engine, "");
            assert_eq!(empty["actions"]["tree"]["nodes"], json!([]));
            assert!(empty["actions"]["editor"].is_null());
            let created = ask(
                &engine,
                json!({"request": "StreamerzEditEntity", "kind": "act:group_add", "itemId": "", "changes": [{"field": 0, "value": "Fun"}]}),
            );
            assert_eq!(created["saved"], "g:Fun");
            let id = add_action(&engine, "Hello", "Fun");
            let tree = view(&engine, "")["actions"]["tree"]["nodes"].clone();
            assert_eq!(tree[0]["id"], "g:Fun");
            assert_eq!(tree[0]["children"][0]["id"], id);
            assert_eq!(tree[0]["children"][0]["label"], "Hello");
            // Edit through its form: the name, the group and a flag.
            let form = ask(
                &engine,
                json!({"request": "StreamerzEntityForm", "kind": "act:action_edit", "itemId": id}),
            );
            let changes = json!([{"field": field(&form, "name"), "value": "Hi"}, {"field": field(&form, "enabled"), "value": false}]);
            assert_eq!(
                ask(
                    &engine,
                    json!({"request": "StreamerzEditEntity", "kind": form["kind"], "itemId": "", "changes": changes})
                )["status"],
                "ok"
            );
            let shown = view(&engine, &id);
            assert_eq!(shown["actions"]["tree"]["nodes"][0]["children"][0]["label"], "Hi");
            assert_eq!(shown["actions"]["tree"]["nodes"][0]["children"][0]["muted"], true);
            assert!(shown["actions"]["editor"]["title"]
                .as_str()
                .unwrap()
                .contains("disabled"));
            // A name is required.
            let bad = ask(
                &engine,
                json!({"request": "StreamerzEditEntity", "kind": form["kind"], "itemId": "", "changes": [{"field": field(&form, "name"), "value": " "}]}),
            );
            assert_eq!(bad["status"], "error");
            // Toggle, duplicate, delete.
            let act = |action: &str, item: &str| {
                ask(
                    &engine,
                    json!({"request": "StreamerzCoreAction", "domain": "actions", "action": action, "itemId": item}),
                )
            };
            assert_eq!(act("action_toggle", &id)["status"], "ok");
            assert!(engine.config().actions.actions[0].enabled);
            let copy = ask(
                &engine,
                json!({"request": "StreamerzEntityForm", "kind": "act:action_duplicate", "itemId": id}),
            );
            assert_eq!(copy["item"]["name"], "Hi (Copy)");
            let copied = ask(
                &engine,
                json!({"request": "StreamerzEditEntity", "kind": copy["kind"], "itemId": "", "changes": []}),
            );
            assert_eq!(copied["status"], "ok", "{copied}");
            assert_eq!(engine.config().actions.actions.len(), 2);
            assert_ne!(engine.config().actions.actions[1].id, id);
            assert_eq!(act("action_delete", &id)["status"], "ok");
            assert_eq!(act("action_delete", &id)["status"], "error");
            assert_eq!(engine.config().actions.actions.len(), 1);
            // Renaming and deleting a group.
            let rename = ask(
                &engine,
                json!({"request": "StreamerzEntityForm", "kind": "act:group_rename", "itemId": "g:Fun"}),
            );
            assert_eq!(rename["item"]["name"], "Fun");
            let renamed = ask(
                &engine,
                json!({"request": "StreamerzEditEntity", "kind": rename["kind"], "itemId": "", "changes": [{"field": 0, "value": "Games"}]}),
            );
            assert_eq!(renamed["saved"], "g:Games");
            assert_eq!(engine.config().actions.actions[0].group, "Games");
            assert_eq!(act("group_delete", "g:Games")["status"], "ok");
            assert_eq!(engine.config().actions.actions[0].group, "");
            assert_eq!(act("group_delete", "g:Games")["status"], "error");
        });
    }

    #[test]
    fn triggers_and_sub_actions_are_added_edited_reordered_nested_and_deleted() {
        let _lock = crate::TEST_LOCK.lock().unwrap_or_else(|error| error.into_inner());
        crate::engine::tests::run(async {
            let engine = crate::engine::tests::engine(json!({"actions": []}));
            let id = add_action(&engine, "Work", "");
            let action_id = id.strip_prefix("a:").unwrap().to_owned();
            let act = |action: &str, item: &str| {
                ask(
                    &engine,
                    json!({"request": "StreamerzCoreAction", "domain": "actions", "action": action, "itemId": item, "actionId": action_id}),
                )
            };
            // A trigger through its form.
            let form = ask(
                &engine,
                json!({"request": "StreamerzEntityForm", "kind": "act:trigger_add", "actionId": id, "code": "101"}),
            );
            assert_eq!(form["status"], "ok", "{form}");
            let saved = ask(
                &engine,
                json!({"request": "StreamerzEditEntity", "kind": form["kind"], "itemId": "", "changes": []}),
            );
            assert_eq!(saved["status"], "ok", "{saved}");
            let trigger = saved["saved"].as_str().unwrap().to_owned();
            let editor = view(&engine, &action_id)["actions"]["editor"].clone();
            assert_eq!(editor["triggers"]["nodes"][0]["id"], trigger);
            assert!(editor["triggers"]["nodes"][0]["label"]
                .as_str()
                .unwrap()
                .starts_with("Twitch"));
            assert!(editor["triggers"]["nodes"][0]["detail"]["text"]
                .as_str()
                .is_some_and(|text| !text.is_empty()));
            assert_eq!(act("trigger_toggle", &trigger)["status"], "ok");
            assert_eq!(engine.config().actions.actions[0].triggers[0]["enabled"], false);
            // Changing a trigger's type through JSON is refused.
            let edit_form = ask(
                &engine,
                json!({"request": "StreamerzEntityForm", "kind": "act:trigger_edit", "actionId": action_id, "itemId": trigger}),
            );
            let wrong = ask(
                &engine,
                json!({"request": "StreamerzEditEntity", "kind": edit_form["kind"], "itemId": edit_form["item"]["id"], "json": json!({"id": trigger, "type": 102}).to_string()}),
            );
            assert_eq!(wrong["status"], "error");
            assert_eq!(act("trigger_delete", &trigger)["status"], "ok");
            assert!(engine.config().actions.actions[0].triggers.is_empty());
            // Sub-actions: a group, then one inside it by selecting the group, then one after it.
            let add = |code: &str, anchor: &str| -> String {
                let form = ask(
                    &engine,
                    json!({"request": "StreamerzEntityForm", "kind": "act:sub_add", "actionId": action_id, "code": code, "anchor": anchor}),
                );
                assert_eq!(form["status"], "ok", "{form}");
                let saved = ask(
                    &engine,
                    json!({"request": "StreamerzEditEntity", "kind": form["kind"], "itemId": "", "changes": []}),
                );
                assert_eq!(saved["status"], "ok", "{saved}");
                saved["saved"].as_str().unwrap().to_owned()
            };
            let group = add("99900", "");
            let inner = add("1009", &group);
            let after = add("1009", "");
            let list = engine.config().actions.actions[0].sub_actions.clone();
            assert_eq!(list.len(), 2);
            assert_eq!(list[0]["subActions"].as_array().unwrap().len(), 1);
            assert_eq!(list[0]["subActions"][0]["id"], inner);
            // Move: the last one up, then into the group, then out again.
            assert_eq!(act("sub_up", &after)["status"], "ok");
            assert_eq!(engine.config().actions.actions[0].sub_actions[0]["id"], after);
            assert_eq!(act("sub_down", &after)["status"], "ok");
            assert_eq!(engine.config().actions.actions[0].sub_actions[1]["id"], after);
            assert_eq!(act("sub_in", &after)["status"], "ok");
            assert_eq!(
                engine.config().actions.actions[0].sub_actions[0]["subActions"]
                    .as_array()
                    .unwrap()
                    .len(),
                2
            );
            assert_eq!(act("sub_out", &after)["status"], "ok");
            assert_eq!(engine.config().actions.actions[0].sub_actions.len(), 2);
            assert_eq!(act("sub_toggle", &inner)["status"], "ok");
            let shown = view(&engine, &action_id)["actions"]["editor"]["subs"]["nodes"].clone();
            assert_eq!(shown[0]["children"][0]["icon"], "power-off");
            // Edit an existing sub-action; its place and children survive.
            let edit_form = ask(
                &engine,
                json!({"request": "StreamerzEntityForm", "kind": "act:sub_edit", "actionId": action_id, "itemId": group}),
            );
            assert_eq!(edit_form["status"], "ok", "{edit_form}");
            let name_field = field(&edit_form, "name");
            let renamed = ask(
                &engine,
                json!({"request": "StreamerzEditEntity", "kind": edit_form["kind"], "itemId": "", "changes": [{"field": name_field, "value": "Greeting"}]}),
            );
            assert_eq!(renamed["status"], "ok", "{renamed}");
            let list = engine.config().actions.actions[0].sub_actions.clone();
            assert_eq!(list[0]["name"], "Greeting");
            assert_eq!(list[0]["subActions"].as_array().unwrap().len(), 1);
            // If/Else branches are fixed; the If/Else itself goes.
            let branch = add("120", "");
            let branch_true = engine.config().actions.actions[0]
                .sub_actions
                .iter()
                .find(|s| s["id"] == branch)
                .unwrap()["subActions"][0]["id"]
                .as_str()
                .unwrap()
                .to_owned();
            assert_eq!(act("sub_delete", &branch_true)["status"], "error");
            assert_eq!(act("sub_delete", &branch)["status"], "ok");
            assert_eq!(act("sub_delete", &group)["status"], "ok");
            assert_eq!(act("sub_delete", &group)["status"], "error");
        });
    }

    #[test]
    fn pickers_list_every_trigger_and_sub_action_by_category_and_the_templates() {
        let _lock = crate::TEST_LOCK.lock().unwrap_or_else(|error| error.into_inner());
        crate::engine::tests::run(async {
            let engine = crate::engine::tests::engine(json!({"actions": []}));
            fn leaves(nodes: &Value, out: &mut Vec<Value>) {
                for node in nodes.as_array().into_iter().flatten() {
                    if node["kind"] == "item" {
                        out.push(node.clone());
                    }
                    leaves(&node["children"], out);
                }
            }
            for (kind, minimum) in [("trigger", 50), ("sub", 100), ("template", 3)] {
                let model = ask(
                    &engine,
                    json!({"request": "StreamerzCoreView", "domain": "actions_picker", "kind": kind}),
                );
                assert_eq!(model["status"], "ok", "{model}");
                let mut all = Vec::new();
                leaves(&model["nodes"], &mut all);
                assert!(all.len() >= minimum, "{kind}: {}", all.len());
                assert!(all
                    .iter()
                    .all(|item| item["detail"]["text"].is_string() && item["id"].is_string()));
            }
            let sub = ask(
                &engine,
                json!({"request": "StreamerzCoreView", "domain": "actions_picker", "kind": "sub"}),
            );
            assert!(sub["nodes"]
                .as_array()
                .unwrap()
                .iter()
                .all(|node| node["kind"] == "category"));
            assert_eq!(
                ask(
                    &engine,
                    json!({"request": "StreamerzCoreView", "domain": "actions_picker", "kind": "other"})
                )["status"],
                "error"
            );
            // A template adds its actions.
            let name = crate::templates::TEMPLATES[0].name;
            let added = ask(
                &engine,
                json!({"request": "StreamerzCoreAction", "domain": "actions", "action": "template_add", "code": name}),
            );
            assert_eq!(added["status"], "ok", "{added}");
            assert!(!engine.config().actions.actions.is_empty());
        });
    }

    #[test]
    fn export_and_import_round_trip_and_the_import_asks_before_it_changes_anything() {
        let _lock = crate::TEST_LOCK.lock().unwrap_or_else(|error| error.into_inner());
        crate::engine::tests::run(async {
            let engine = crate::engine::tests::engine(json!({"actions": []}));
            let id = add_action(&engine, "Shared", "");
            let form = ask(
                &engine,
                json!({"request": "StreamerzEntityForm", "kind": "act:export", "itemId": id}),
            );
            assert_eq!(form["item"]["name"], "Shared");
            let done = ask(
                &engine,
                json!({"request": "StreamerzEditEntity", "kind": form["kind"], "itemId": "", "changes": []}),
            );
            assert_eq!(done["status"], "ok", "{done}");
            let text = done["copy"].as_str().unwrap().to_owned();
            assert!(!text.is_empty());
            // Nothing selected and nothing ticked is refused.
            let none = ask(
                &engine,
                json!({"request": "StreamerzEntityForm", "kind": "act:export", "itemId": ""}),
            );
            assert_eq!(
                ask(
                    &engine,
                    json!({"request": "StreamerzEditEntity", "kind": none["kind"], "itemId": "", "changes": [{"field": 0, "value": 0}]})
                )["status"],
                "error"
            );
            // Importing a broken string says so; a good one asks first.
            let import = ask(&engine, json!({"request": "StreamerzEntityForm", "kind": "act:import"}));
            let bad = ask(
                &engine,
                json!({"request": "StreamerzEditEntity", "kind": import["kind"], "itemId": "", "changes": [{"field": 0, "value": "not an export"}]}),
            );
            assert_eq!(bad["status"], "error");
            engine.change(|config| config.actions.actions.clear());
            let good = ask(
                &engine,
                json!({"request": "StreamerzEditEntity", "kind": import["kind"], "itemId": "", "changes": [{"field": 0, "value": text}]}),
            );
            assert_eq!(good["status"], "ok", "{good}");
            assert!(engine.config().actions.actions.is_empty());
            let pending = crate::core::prompts::list();
            let question = pending.iter().rev().find_map(|prompt| match &prompt.content {
                crate::core::prompts::Content::Confirm { text, .. } => Some((prompt.id, text.clone())),
                _ => None,
            });
            let (prompt, question) = question.expect("an import confirmation");
            assert!(question.contains("Shared") || !question.is_empty());
            crate::core::prompts::answer(prompt, json!(true)).unwrap();
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
            assert_eq!(engine.config().actions.actions.len(), 1);
            assert_eq!(engine.config().actions.actions[0].name, "Shared");
        });
    }

    #[test]
    fn group_settings_edit_the_description_and_values_by_form_and_the_rest_as_json() {
        let _lock = crate::TEST_LOCK.lock().unwrap_or_else(|error| error.into_inner());
        crate::engine::tests::run(async {
            let engine = crate::engine::tests::engine(json!({"actions": []}));
            add_action(&engine, "Grouped", "Tools");
            let meta = json!({"description": "A tool", "variables": [
                {"name": "limit", "label": "Limit", "kind": "integer", "default": 3},
                {"name": "loud", "label": "Loud", "kind": "boolean", "default": false},
                {"name": "mode", "label": "Mode", "kind": "choice", "default": "a", "options": ["a", "b"]}]});
            let set = ask(
                &engine,
                json!({"request": "StreamerzSetGroupMeta", "group": "Tools", "meta": meta}),
            );
            assert_eq!(set["status"], "ok", "{set}");
            let form = ask(
                &engine,
                json!({"request": "StreamerzEntityForm", "kind": "act:group_settings", "itemId": "g:Tools"}),
            );
            assert_eq!(form["status"], "ok", "{form}");
            assert_eq!(form["fields"].as_array().unwrap().len(), 4);
            assert_eq!(form["fields"][1]["value"], "3");
            assert_eq!(form["fields"][3]["widget"]["kind"], "pick");
            let changes = json!([{"field": 0, "value": "Better"}, {"field": 1, "value": "9"}, {"field": 2, "value": true}, {"field": 3, "value": 1}]);
            let saved = ask(
                &engine,
                json!({"request": "StreamerzEditEntity", "kind": form["kind"], "itemId": "", "changes": changes}),
            );
            assert_eq!(saved["status"], "ok", "{saved}");
            let config = engine.config();
            assert_eq!(crate::groups::meta(&config, "Tools").description, "Better");
            let values = crate::groups::values(&config, "Tools");
            assert_eq!(
                (values["limit"].clone(), values["loud"].clone(), values["mode"].clone()),
                (json!(9), json!(true), json!("b"))
            );
            drop(config);
            // A value of the wrong kind is refused and changes nothing.
            let bad = ask(
                &engine,
                json!({"request": "StreamerzEditEntity", "kind": form["kind"], "itemId": "", "changes": [{"field": 1, "value": "many"}]}),
            );
            assert_eq!(bad["status"], "error");
            // JSON shows the definitions too, and can change them.
            let draft = ask(
                &engine,
                json!({"request": "StreamerzEntityDraft", "kind": form["kind"], "itemId": "", "changes": []}),
            );
            assert!(draft["json"].as_str().unwrap().contains("\"limit\""));
            let renamed = draft["json"]
                .as_str()
                .unwrap()
                .replace("A tool", "x")
                .replace("Better", "Json");
            let by_json = ask(
                &engine,
                json!({"request": "StreamerzEditEntity", "kind": form["kind"], "itemId": "", "json": renamed}),
            );
            assert_eq!(by_json["status"], "ok", "{by_json}");
            assert_eq!(crate::groups::meta(&engine.config(), "Tools").description, "Json");
            let broken = ask(
                &engine,
                json!({"request": "StreamerzEditEntity", "kind": form["kind"], "itemId": "", "json": "{"}),
            );
            assert_eq!(broken["status"], "error");
        });
    }

    #[test]
    fn forms_offer_what_a_field_can_name_and_json_only_types_open_as_json() {
        let _lock = crate::TEST_LOCK.lock().unwrap_or_else(|error| error.into_inner());
        crate::engine::tests::run(async {
            let engine = crate::engine::tests::engine(json!({"actions": []}));
            let id = add_action(&engine, "Other", "");
            let action_id = id.strip_prefix("a:").unwrap().to_owned();
            let form = ask(
                &engine,
                json!({"request": "StreamerzEntityForm", "kind": "act:action_edit", "itemId": id}),
            );
            let queue = &form["fields"][field(&form, "queue")];
            assert_eq!(queue["strict"], true);
            assert_eq!(queue["choices"][0]["label"], "Default");
            // Run Action names the other actions.
            let sub = ask(
                &engine,
                json!({"request": "StreamerzEntityForm", "kind": "act:sub_add", "actionId": action_id, "code": "4"}),
            );
            assert_eq!(sub["status"], "ok", "{sub}");
            let target = sub["fields"]
                .as_array()
                .unwrap()
                .iter()
                .find(|field| field["widget"]["kind"] == "action");
            assert!(target.is_some_and(|field| field["choices"][0]["label"] == "Other"));
            // The canonical kind carries the whole context, so a form can be asked for again.
            let again = ask(
                &engine,
                json!({"request": "StreamerzEntityForm", "kind": sub["kind"], "itemId": ""}),
            );
            assert_eq!(again["kind"], sub["kind"]);
            // Testing a trigger on a disabled action says so.
            let trigger = ask(
                &engine,
                json!({"request": "StreamerzEntityForm", "kind": "act:trigger_add", "actionId": action_id, "code": "101"}),
            );
            let saved = ask(
                &engine,
                json!({"request": "StreamerzEditEntity", "kind": trigger["kind"], "itemId": "", "changes": []}),
            );
            let trigger_id = saved["saved"].as_str().unwrap().to_owned();
            engine.change(|config| config.actions.actions[0].enabled = false);
            let test = ask(
                &engine,
                json!({"request": "StreamerzEntityForm", "kind": "act:trigger_test", "actionId": action_id, "itemId": trigger_id}),
            );
            let result = ask(
                &engine,
                json!({"request": "StreamerzEditEntity", "kind": test["kind"], "itemId": "", "changes": [{"field": 0, "value": "user=Ann"}]}),
            );
            assert_eq!(result["status"], "error");
            for kind in ["unknown", "act:nothing"] {
                assert_eq!(
                    ask(
                        &engine,
                        json!({"request": "StreamerzEntityForm", "kind": kind, "itemId": ""})
                    )["status"],
                    "error"
                );
            }
        });
    }
}
