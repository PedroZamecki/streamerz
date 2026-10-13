//! Editing Streamer.bot's data, for the editor: sub-action trees addressed by paths (indexes down
//! the `subActions` lists), moves, new sub-actions and triggers, and import/export of whole sets.

use crate::engine::{uuid, DEFAULT_QUEUE};
use crate::model::{Action, Obj};
use crate::Config;
use serde_json::{json, Value};

/// Where a sub-action is: its index in the action's list, then in each container's `subActions`.
pub type Path = Vec<usize>;

pub fn kind(o: &Obj) -> u64 {
    o.get("type").and_then(Value::as_u64).unwrap_or(0)
}

/// If/Else and Switch hold only their branches; these hold any sub-action.
pub fn holds_any(kind: u64) -> bool {
    matches!(kind, 128 | 99900..=99904)
}

/// Branches are fixed parts of their If/Else or Switch: not moved or deleted on their own (Switch
/// cases excepted).
pub fn fixed(kind: u64) -> bool {
    matches!(kind, 99901 | 99902 | 99904)
}

fn children(o: &Obj) -> Vec<Obj> {
    o.get("subActions")
        .and_then(Value::as_array)
        .map(|a| a.iter().filter_map(|x| x.as_object().cloned()).collect())
        .unwrap_or_default()
}

fn set_children(o: &mut Obj, list: Vec<Obj>) {
    o.insert("subActions".into(), list.into_iter().map(Value::Object).collect());
}

pub fn get<'a>(list: &'a [Obj], path: &[usize]) -> Option<&'a Obj> {
    let (first, rest) = path.split_first()?;
    let o = list.get(*first)?;
    if rest.is_empty() {
        return Some(o);
    }
    let kids = o.get("subActions")?.as_array()?;
    let mut cur = kids.get(rest[0])?.as_object()?;
    for &i in &rest[1..] {
        cur = cur.get("subActions")?.as_array()?.get(i)?.as_object()?;
    }
    Some(cur)
}

/// Run `f` on the list at `parent` (the action's own list for an empty path).
fn with_list<T>(list: &mut Vec<Obj>, parent: &[usize], f: impl FnOnce(&mut Vec<Obj>) -> T) -> Option<T> {
    let Some((first, rest)) = parent.split_first() else {
        return Some(f(list));
    };
    let o = list.get_mut(*first)?;
    let mut kids = children(o);
    let out = with_list(&mut kids, rest, f);
    set_children(o, kids);
    out
}

pub fn update(list: &mut Vec<Obj>, path: &[usize], f: impl FnOnce(&mut Obj)) -> bool {
    let Some((last, parent)) = path.split_last() else {
        return false;
    };
    with_list(list, parent, |l| l.get_mut(*last).map(f)).flatten().is_some()
}

pub fn remove(list: &mut Vec<Obj>, path: &[usize]) -> Option<Obj> {
    let (last, parent) = path.split_last()?;
    let out = with_list(list, parent, |l| (*last < l.len()).then(|| l.remove(*last))).flatten();
    renumber(list);
    out
}

/// Insert at `parent`, before index `at` (clamped to the end).
pub fn insert(list: &mut Vec<Obj>, parent: &[usize], at: usize, o: Obj) -> bool {
    let ok = with_list(list, parent, |l| l.insert(at.min(l.len()), o)).is_some();
    renumber(list);
    ok
}

/// Set every sub-action's `index` (its place in its list) and `parentId` (its container's id, null
/// at the top), recursively.
pub fn renumber(list: &mut [Obj]) {
    fn go(list: &mut [Obj], parent: Option<&str>) {
        for (i, o) in list.iter_mut().enumerate() {
            o.insert("index".into(), i.into());
            o.insert("parentId".into(), parent.map_or(Value::Null, Value::from));
            let id = o.get("id").and_then(Value::as_str).unwrap_or("").to_string();
            if o.contains_key("subActions") {
                let mut kids = children(o);
                go(&mut kids, Some(&id));
                set_children(o, kids);
            }
        }
    }
    go(list, None);
}

/// Move the sub-action at `from` to `parent`, before index `at`. Branches and moves into the item's
/// own subtree are refused.
pub fn move_to(list: &mut Vec<Obj>, from: &[usize], parent: &[usize], mut at: usize) -> bool {
    let Some(o) = get(list, from) else { return false };
    if fixed(kind(o)) || parent.starts_with(from) {
        return false;
    }
    // Removing first shifts later siblings (and anything under them) up by one.
    let (last, from_parent) = from.split_last().expect("checked");
    let mut parent = parent.to_vec();
    if parent.len() > from_parent.len() && parent.starts_with(from_parent) && parent[from_parent.len()] > *last {
        parent[from_parent.len()] -= 1;
    }
    if parent == from_parent && at > *last {
        at -= 1;
    }
    let Some(o) = remove(list, from) else { return false };
    insert(list, &parent, at, o)
}

/// Where a drop lands: into a container that holds anything (at its end), else before the target
/// in the target's list.
pub fn drop_target(list: &[Obj], target: &[usize]) -> Option<(Path, usize)> {
    let o = get(list, target)?;
    if holds_any(kind(o)) {
        return Some((target.to_vec(), children(o).len()));
    }
    let (last, parent) = target.split_last()?;
    Some((parent.to_vec(), *last))
}

/// A new sub-action of this type, with Streamer.bot's base fields (its own fields are left to the
/// form or the JSON editor) and the branches If/Else and Switch need.
pub fn new_sub(kind: u64) -> Obj {
    let mut o = Obj::new();
    let branch = |k: u64| {
        json!({"random": false, "subActions": [], "id": uuid(), "weight": 0.0, "type": k,
        "parentId": null, "enabled": true, "index": 0})
    };
    match kind {
        120 => {
            o.insert("subActions".into(), json!([branch(99901), branch(99902)]));
        }
        127 => {
            o.insert("subActions".into(), json!([branch(99904)]));
        }
        128 | 99900 => {
            o.insert("subActions".into(), json!([]));
        }
        _ => {}
    }
    for (k, v) in [
        ("id", Value::from(uuid())),
        ("weight", json!(0.0)),
        ("type", kind.into()),
        ("parentId", Value::Null),
        ("enabled", true.into()),
        ("index", 0.into()),
    ] {
        o.insert(k.into(), v);
    }
    let mut list = vec![o];
    renumber(&mut list);
    list.remove(0)
}

pub fn new_trigger(kind: u32) -> Obj {
    match json!({"id": uuid(), "type": kind, "enabled": true, "exclusions": []}) {
        Value::Object(o) => o,
        _ => Obj::new(),
    }
}

/// A new action, enabled, in the default queue.
pub fn new_action(name: &str, group: &str) -> Action {
    Action {
        id: uuid(),
        queue: Some(DEFAULT_QUEUE.into()),
        enabled: true,
        name: name.into(),
        group: group.into(),
        ..Default::default()
    }
}

/// Clones an action, giving it and its triggers and sub-actions fresh unique IDs.
pub fn duplicate_action(src: &Action, new_name: &str) -> Action {
    let mut dup = src.clone();
    dup.id = uuid();
    dup.name = new_name.to_string();

    for t in &mut dup.triggers {
        t.insert("id".into(), Value::from(uuid()));
    }

    fn regenerate_sub_ids(subs: &mut [Obj]) {
        for s in subs {
            s.insert("id".into(), Value::from(uuid()));
            if let Some(Value::Array(kids)) = s.get_mut("subActions") {
                let mut kid_objs: Vec<Obj> = kids.iter().filter_map(|v| v.as_object().cloned()).collect();
                regenerate_sub_ids(&mut kid_objs);
                *kids = kid_objs.into_iter().map(Value::Object).collect();
            }
        }
    }

    regenerate_sub_ids(&mut dup.sub_actions);
    dup
}

fn s<'a>(o: &'a Obj, k: &str) -> &'a str {
    o.get(k).and_then(Value::as_str).unwrap_or("")
}

/// Every id a set of actions refers to (commands, timers, queues, clients, servers in triggers;
/// queues of the actions).
fn referenced(actions: &[Action]) -> Vec<String> {
    let mut out = Vec::new();
    for a in actions {
        out.extend(a.queue.clone());
        for t in &a.triggers {
            for k in ["commandId", "timerId", "clientId", "serverId"] {
                if let Some(v) = t.get(k).and_then(Value::as_str) {
                    out.push(v.into());
                }
            }
        }
    }
    out
}

/// Export these actions (and the commands, timers, queues, WebSocket clients and servers their
/// triggers use) as Streamer.bot does: meta, then data, then versions.
pub fn export(c: &Config, ids: &[String], meta: [&str; 4]) -> Obj {
    let actions: Vec<Action> = c
        .actions
        .actions
        .iter()
        .filter(|a| ids.contains(&a.id))
        .cloned()
        .collect();
    let refs = referenced(&actions);
    let pick = |list: &[Obj]| -> Vec<Obj> {
        list.iter()
            .filter(|x| refs.iter().any(|r| r == s(x, "id")))
            .cloned()
            .collect()
    };
    let mut actions_json: Vec<Value> = Vec::new();
    for mut a in actions {
        // The default queue is null in exports.
        if a.queue.as_deref() == Some(DEFAULT_QUEUE) {
            a.queue = None;
        }
        actions_json.push(serde_json::to_value(&a).unwrap_or_default());
    }
    let v = json!({
        "meta": {"name": meta[0], "author": meta[1], "version": meta[2], "description": meta[3],
            "autoRunAction": null, "minimumVersion": null},
        "data": {"actions": actions_json, "queues": pick(&c.actions.queues), "commands": pick(&c.commands),
            "websocketServers": pick(&c.websockets.servers), "websocketClients": pick(&c.websockets.connections),
            "timers": pick(&c.timers)},
        "version": 23, "exportedFrom": crate::servers::API_VERSION, "minimumVersion": "1.0.0-alpha.1",
    });
    match v {
        Value::Object(o) => o,
        _ => Obj::new(),
    }
}

/// What an import holds, for the confirmation.
pub fn summary(x: &Obj) -> String {
    let n = |k: &str| {
        x.get("data")
            .and_then(|d| d.get(k))
            .and_then(Value::as_array)
            .map_or(0, Vec::len)
    };
    let name = x
        .get("meta")
        .and_then(|m| m.get("name"))
        .and_then(Value::as_str)
        .unwrap_or("");
    format!(
        "{name}\n{} actions, {} commands, {} queues, {} timers, {} WebSocket clients, {} WebSocket servers",
        n("actions"),
        n("commands"),
        n("queues"),
        n("timers"),
        n("websocketClients"),
        n("websocketServers")
    )
}

/// Bring an import into the config: items with the same name (or id) as existing ones replace
/// them. The names of the sub-action types that won't run here (kept as they are). Errors on older
/// export versions (sub-actions under `actions`), which need converting first.
pub fn import(c: &mut Config, x: &Obj) -> Result<Vec<&'static str>, String> {
    let data = x
        .get("data")
        .and_then(Value::as_object)
        .ok_or("no data in this import")?;
    let list = |k: &str| -> Vec<Obj> {
        data.get(k)
            .and_then(Value::as_array)
            .map(|a| a.iter().filter_map(|v| v.as_object().cloned()).collect())
            .unwrap_or_default()
    };
    let raw = list("actions");
    if raw
        .iter()
        .any(|a| a.contains_key("actions") && !a.contains_key("subActions"))
    {
        let v = x.get("version").map(crate::vars::text).unwrap_or_default();
        return Err(format!(
            "this import is in an older format (version {v}), which isn't supported yet"
        ));
    }
    let mut unsupported = Vec::new();
    let mut actions = Vec::new();
    for a in raw {
        let mut a: Action = serde_json::from_value(Value::Object(a)).map_err(|e| format!("bad action: {e}"))?;
        if a.queue.as_deref().is_none_or(str::is_empty) {
            a.queue = Some(DEFAULT_QUEUE.into());
        }
        walk(&a.sub_actions, &mut |o| {
            let k = kind(o) as u32;
            if !crate::engine::supported(k) {
                let name = crate::types::name(crate::types::SUB_ACTIONS, k).unwrap_or("unknown");
                if !unsupported.contains(&name) {
                    unsupported.push(name);
                }
            }
        });
        actions.push(a);
    }
    fn merge(into: &mut Vec<Obj>, new: Vec<Obj>) {
        for n in new {
            into.retain(|o| s(o, "id") != s(&n, "id") && s(o, "name") != s(&n, "name"));
            into.push(n);
        }
    }
    for a in actions {
        if !a.group.is_empty() && !c.actions.groups.iter().any(|g| g.as_str() == Some(&a.group)) {
            c.actions.groups.push(a.group.clone().into());
        }
        c.actions.actions.retain(|o| o.id != a.id && o.name != a.name);
        c.actions.actions.push(a);
    }
    merge(&mut c.actions.queues, list("queues"));
    merge(&mut c.commands, list("commands"));
    merge(&mut c.timers, list("timers"));
    merge(&mut c.websockets.connections, list("websocketClients"));
    merge(&mut c.websockets.servers, list("websocketServers"));
    Ok(unsupported)
}

/// Visit every sub-action of a tree.
pub fn walk(list: &[Obj], f: &mut dyn FnMut(&Obj)) {
    for o in list {
        f(o);
        walk(&children(o), f);
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn duplicate_action_regenerates_ids() {
        let mut a = new_action("Original", "Group A");
        a.triggers.push(new_trigger(101));
        let mut sub = new_sub(1);
        sub.insert("subActions".into(), json!([new_sub(2)]));
        a.sub_actions.push(sub);

        let dup = duplicate_action(&a, "Original (Copy)");
        assert_eq!(dup.name, "Original (Copy)");
        assert_eq!(dup.group, "Group A");
        assert_ne!(dup.id, a.id);
        assert_eq!(dup.triggers.len(), 1);
        assert_ne!(dup.triggers[0]["id"], a.triggers[0]["id"]);
        assert_eq!(dup.sub_actions.len(), 1);
        assert_ne!(dup.sub_actions[0]["id"], a.sub_actions[0]["id"]);
        assert_ne!(
            dup.sub_actions[0]["subActions"][0]["id"],
            a.sub_actions[0]["subActions"][0]["id"]
        );
    }

    use super::*;

    fn tree() -> Vec<Obj> {
        // [log a, group [log b, log c], log d]
        let log = |m: &str| {
            let mut o = new_sub(1020);
            o.insert("message".into(), m.into());
            o
        };
        let mut g = new_sub(99900);
        set_children(&mut g, vec![log("b"), log("c")]);
        let mut l = vec![log("a"), g, log("d")];
        renumber(&mut l);
        l
    }

    fn msgs(l: &[Obj]) -> String {
        let mut out = String::new();
        walk(l, &mut |o| out += if kind(o) == 99900 { "G" } else { s(o, "message") });
        out
    }

    #[test]
    fn moves() {
        let mut l = tree();
        assert_eq!(msgs(&l), "aGbcd");
        // d to the front.
        assert!(move_to(&mut l, &[2], &[], 0));
        assert_eq!(msgs(&l), "daGbc");
        // a into the group's end (drop on the group).
        let (p, at) = drop_target(&l, &[2]).unwrap();
        assert!(move_to(&mut l, &[1], &p, at));
        assert_eq!(msgs(&l), "dGbca");
        // The group into itself: refused.
        assert!(!move_to(&mut l, &[1], &[1], 0));
        // c out, before d.
        assert!(move_to(&mut l, &[1, 1], &[], 0));
        assert_eq!(msgs(&l), "cdGba");
        let g = get(&l, &[2]).unwrap();
        let b = get(&l, &[2, 0]).unwrap();
        assert_eq!(b["parentId"], g["id"]);
        assert_eq!(
            (b["index"].clone(), get(&l, &[2, 1]).unwrap()["index"].clone()),
            (json!(0), json!(1))
        );
        assert!(remove(&mut l, &[2, 0]).is_some());
        assert_eq!(msgs(&l), "cdGa");
        // If/Else comes with its branches, which don't move.
        let i = new_sub(120);
        assert_eq!(children(&i).iter().map(kind).collect::<Vec<_>>(), [99901, 99902]);
        let mut l = vec![i];
        assert!(!move_to(&mut l, &[0, 0], &[], 0));
    }

    #[test]
    fn export_import() {
        let mut c = Config::default();
        let mut a = new_action("hi", "Greetings");
        let mut t = new_trigger(401);
        t.insert("commandId".into(), "c1".into());
        a.triggers.push(t);
        a.sub_actions.push(new_sub(1020));
        a.sub_actions.push(new_sub(99999));
        let id = a.id.clone();
        c.actions.actions.push(a);
        c.commands
            .push(json!({"id": "c1", "name": "hi"}).as_object().unwrap().clone());
        c.commands
            .push(json!({"id": "c2", "name": "other"}).as_object().unwrap().clone());
        let x = export(&c, std::slice::from_ref(&id), ["Hello", "me", "1.0.0", ""]);
        assert_eq!(
            x.keys().collect::<Vec<_>>(),
            ["meta", "data", "version", "exportedFrom", "minimumVersion"]
        );
        assert_eq!(x["data"]["actions"][0]["queue"], Value::Null);
        assert_eq!(x["data"]["commands"].as_array().unwrap().len(), 1);
        // Round trip through an export string, into a config that has an older "hi".
        let x: Obj = crate::model::read_export(&crate::model::write_export(&x)).unwrap();
        let mut d = Config::default();
        d.actions.actions.push(new_action("hi", ""));
        let unsupported = import(&mut d, &x).unwrap();
        assert_eq!(unsupported, ["ExecuteCode"]);
        assert_eq!(d.actions.actions.len(), 1);
        assert_eq!(
            (d.actions.actions[0].id.as_str(), d.actions.actions[0].queue.as_deref()),
            (id.as_str(), Some(DEFAULT_QUEUE))
        );
        assert_eq!(d.actions.groups, [json!("Greetings")]);
        assert_eq!(d.commands.len(), 1);
        assert!(summary(&x).contains("1 actions, 1 commands"));
        // Importing again keeps actions that aren't in the import.
        d.actions.actions.push(new_action("mine", "Greetings"));
        import(&mut d, &x).unwrap();
        assert_eq!(
            d.actions.actions.iter().map(|a| a.name.as_str()).collect::<Vec<_>>(),
            ["mine", "hi"]
        );
        let old = json!({"version": 10, "data": {"actions": [{"name": "x", "actions": []}]}});
        assert!(import(&mut d, old.as_object().unwrap())
            .unwrap_err()
            .contains("older format"));
    }
}
