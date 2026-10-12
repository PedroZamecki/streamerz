//! Global and user global variables, persisted (in the config) or not, with when each was last
//! written. Triggers and events as the real Streamer.bot fires them: creating a global fires Created
//! only; changing it (even to the same value) fires Updated; unsetting it (Set Global, "remove")
//! fires Updated without a new value; only deleting it from the editor fires Deleted.

use super::{b, n, s, Ctx, Engine};
use crate::fmt;
use crate::model::Obj;
use crate::vars::{self, auto_type, text};
use serde_json::{json, Value};
use std::rc::Rc;

const T_GLOBAL_UPDATED: u32 = 709;
const T_USER_GLOBAL_UPDATED: u32 = 710;
const T_GLOBAL_CREATED: u32 = 711;
const T_GLOBAL_DELETED: u32 = 712;

/// The user a user global belongs to.
pub(super) struct User {
    /// "platform:id": the key in user_globals.
    key: String,
    id: String,
    platform: String,
    login: String,
    name: String,
}

/// Now, as Streamer.bot writes times in JSON (ISO 8601, local offset).
pub fn iso_now() -> String {
    fmt::date(&fmt::now(), "o")
}

/// A user's globals: {id, login, name, platform, values: {}, lastWrite: {}}.
fn user_entry(u: &User) -> Obj {
    match json!({"id": u.id, "login": u.login, "name": u.name, "platform": u.platform, "values": {}, "lastWrite": {}}) {
        Value::Object(o) => o,
        _ => Obj::new(),
    }
}

/// Set or remove `name` in a {values, lastWrite} pair.
fn put(values: &mut Obj, writes: &mut Obj, name: &str, v: &Option<Value>, now: &str) {
    match v {
        Some(v) => {
            values.insert(name.into(), v.clone());
            writes.insert(name.into(), now.into());
        }
        None => {
            values.shift_remove(name);
            writes.shift_remove(name);
        }
    }
}

fn obj_mut<'a>(o: &'a mut Obj, k: &str) -> &'a mut Obj {
    let v = o.entry(k).or_insert_with(|| Value::Object(Obj::new()));
    if !v.is_object() {
        *v = Value::Object(Obj::new());
    }
    v.as_object_mut().expect("just made an object")
}

impl Engine {
    /// The user a global belongs to: 1 the one who ran the action, 2 the target user.
    fn user(ctx: &Ctx, source: i64) -> Option<User> {
        let p = if source == 2 { "targetUser" } else { "user" };
        let get = |k: String| ctx.args.get(&k).map(text).unwrap_or_default();
        let id = get(format!("{p}Id"));
        if id.is_empty() {
            return None;
        }
        let platform = ctx.args.get("userType").map(text).unwrap_or_else(|| "twitch".into());
        let name = get(p.into());
        let login = Some(get(format!("{p}Name")))
            .filter(|l| !l.is_empty())
            .unwrap_or_else(|| name.to_lowercase());
        Some(User {
            key: format!("{platform}:{id}"),
            id,
            platform,
            login,
            name,
        })
    }

    pub(super) fn global_get(self: &Rc<Self>, ctx: &mut Ctx, o: &Obj) {
        let name = self.simple(ctx, s(o, "variableName"));
        let dest = self.simple(ctx, s(o, "destinationVariable"));
        let persisted = b(o, "persisted", true);
        let source = n(o, "source") as i64;
        let user = if source == 0 { None } else { Self::user(ctx, source) };
        if source != 0 && user.is_none() {
            ctx.args.insert(dest, Value::Null);
            return;
        }
        let v = match self.read_global(&name, persisted, user.as_ref()) {
            Some(v) => v,
            // A default creates the global.
            None => match o.get("defaultValue").filter(|d| !d.is_null()) {
                Some(d) => {
                    let d = match d {
                        Value::String(t) => auto_type(&self.expand(ctx, t)),
                        d => d.clone(),
                    };
                    self.write_global(ctx, &name, persisted, user.as_ref(), Some(d.clone()));
                    d
                }
                None => Value::Null,
            },
        };
        ctx.args.insert(dest, v);
    }

    pub(super) fn global_set(self: &Rc<Self>, ctx: &mut Ctx, o: &Obj) {
        let name = self.simple(ctx, s(o, "variableName"));
        let persisted = b(o, "persisted", true);
        let dest = n(o, "destination") as i64;
        let user = if dest == 0 { None } else { Self::user(ctx, dest) };
        if dest != 0 && user.is_none() {
            return;
        }
        let value = self.expand(ctx, s(o, "value"));
        // Source: 0 the value, 1 add it, 2 subtract it, 3 remove the global.
        let new = match n(o, "source") as i64 {
            0 if b(o, "autoType", false) => Some(auto_type(&value)),
            0 => Some(value.into()),
            op @ (1 | 2) => {
                let Some(by) = auto_type(&value).as_f64() else { return };
                let cur = self
                    .read_global(&name, persisted, user.as_ref())
                    .and_then(|v| v.as_f64());
                let by = if op == 1 { by } else { -by };
                Some(vars::number(cur.unwrap_or(0.0) + by))
            }
            _ => None,
        };
        self.write_global(ctx, &name, persisted, user.as_ref(), new);
    }

    fn read_global(&self, name: &str, persisted: bool, user: Option<&User>) -> Option<Value> {
        let c = self.cfg.borrow();
        let st = self.st.borrow();
        let values = |e: Option<&Value>| e?.get("values")?.get(name).cloned();
        match (user, persisted) {
            (None, true) => c.globals.get(name).cloned(),
            (None, false) => st.temp.get(name).cloned(),
            (Some(u), true) => values(c.user_globals.get(&u.key)),
            (Some(u), false) => values(st.user_temp.get(&u.key)),
        }
    }

    /// Set (Some) or remove (None) a global, then fire its triggers and events.
    fn write_global(self: &Rc<Self>, ctx: &Ctx, name: &str, persisted: bool, user: Option<&User>, v: Option<Value>) {
        let old = self.read_global(name, persisted, user);
        if old.is_none() && v.is_none() {
            return;
        }
        let now = iso_now();
        let in_user = |all: &mut Obj, u: &User| {
            let e = obj_mut(all, &u.key);
            if e.is_empty() {
                *e = user_entry(u);
            }
            let mut values = std::mem::take(obj_mut(e, "values"));
            let mut writes = std::mem::take(obj_mut(e, "lastWrite"));
            put(&mut values, &mut writes, name, &v, &now);
            e.insert("values".into(), values.into());
            e.insert("lastWrite".into(), writes.into());
        };
        if persisted {
            self.change(|c| match user {
                None => put(&mut c.globals, &mut c.global_writes, name, &v, &now),
                Some(u) => in_user(&mut c.user_globals, u),
            });
        } else {
            let mut st = self.st.borrow_mut();
            let st = &mut *st;
            match user {
                None => put(&mut st.temp, &mut st.temp_writes, name, &v, &now),
                Some(u) => in_user(&mut st.user_temp, u),
            }
            crate::ui::changed();
        }

        let mut args = Obj::new();
        args.insert("global.name".into(), name.into());
        args.insert("global.persisted".into(), persisted.into());
        args.insert("global.lastWrite".into(), now.as_str().into());
        let filter = |t: &Obj| {
            let want = s(t, "variableName");
            let p = n(t, "persisted") as i64;
            (want.is_empty() || want == name) && (p == 0 || (p == 1) == persisted)
        };
        let mut data = json!({"name": name, "persisted": persisted});
        if let Some(u) = user {
            args.insert("global.newValue".into(), v.clone().unwrap_or(Value::Null));
            args.insert("global.oldValue".into(), old.clone().unwrap_or(Value::Null));
            for k in ["user", "userName", "userId", "userType"] {
                if let Some(x) = ctx.args.get(k) {
                    args.insert(k.into(), x.clone());
                }
            }
            self.fire(
                T_USER_GLOBAL_UPDATED,
                "User Global Variable Updated",
                "Core",
                "streamerbot",
                &args,
                |t| {
                    filter(t)
                        && (s(t, "userLogon").is_empty() || s(t, "userLogon").eq_ignore_ascii_case(&u.login))
                        && (s(t, "platform").is_empty() || s(t, "platform").eq_ignore_ascii_case(&u.platform))
                },
            );
            data["userId"] = u.id.as_str().into();
            data["userLogin"] = u.login.as_str().into();
            data["userName"] = u.name.as_str().into();
            data["platform"] = u.platform.as_str().into();
            if let Some(o) = &old {
                data["oldValue"] = o.clone();
            }
            if let Some(n) = &v {
                data["newValue"] = n.clone();
            }
            data["lastWrite"] = now.as_str().into();
            self.emit("Misc", "UserGlobalVariableUpdated", || data);
            return;
        }
        match (&old, &v) {
            (None, Some(v)) => {
                args.insert("global.value".into(), v.clone());
                self.fire(
                    T_GLOBAL_CREATED,
                    "Global Variable Created",
                    "Core",
                    "streamerbot",
                    &args,
                    filter,
                );
                data["value"] = v.clone();
                data["lastWrite"] = now.as_str().into();
                data["timestamp"] = now.as_str().into();
                self.emit("Misc", "GlobalVariableCreated", || data);
            }
            (Some(o), v) => {
                args.insert("global.value".into(), v.clone().unwrap_or(Value::Null));
                args.insert("global.newValue".into(), v.clone().unwrap_or(Value::Null));
                args.insert("global.oldValue".into(), o.clone());
                self.fire(
                    T_GLOBAL_UPDATED,
                    "Global Variable Updated",
                    "Core",
                    "streamerbot",
                    &args,
                    filter,
                );
                data["oldValue"] = o.clone();
                if let Some(v) = v {
                    data["newValue"] = v.clone();
                }
                data["lastWrite"] = now.as_str().into();
                data["timestamp"] = now.as_str().into();
                self.emit("Misc", "GlobalVariableUpdated", || data);
            }
            (None, None) => {}
        }
    }

    /// Set a (non-user) global, as the editor's global viewer does: fires the same triggers as
    /// Global (Set).
    pub fn put_global(self: &Rc<Self>, name: &str, value: Value, persisted: bool) {
        let ctx = Ctx { args: Obj::new() };
        self.write_global(&ctx, name, persisted, None, Some(value));
    }

    /// Delete a (non-user) global, as the editor's global viewer does: fires Deleted.
    pub fn delete_global(self: &Rc<Self>, name: &str, persisted: bool) {
        let Some(old) = self.read_global(name, persisted, None) else {
            return;
        };
        let now = iso_now();
        if persisted {
            self.change(|c| put(&mut c.globals, &mut c.global_writes, name, &None, &now));
        } else {
            let mut st = self.st.borrow_mut();
            let st = &mut *st;
            put(&mut st.temp, &mut st.temp_writes, name, &None, &now);
        }
        let mut args = Obj::new();
        args.insert("global.name".into(), name.into());
        args.insert("global.value".into(), old.clone());
        args.insert("global.persisted".into(), persisted.into());
        args.insert("global.lastWrite".into(), now.as_str().into());
        self.fire(
            T_GLOBAL_DELETED,
            "Global Variable Deleted",
            "Core",
            "streamerbot",
            &args,
            |t| {
                let want = s(t, "variableName");
                let p = n(t, "persisted") as i64;
                (want.is_empty() || want == name) && (p == 0 || (p == 1) == persisted)
            },
        );
        self.emit(
            "Misc",
            "GlobalVariableDeleted",
            || json!({"name": name, "persisted": persisted, "value": old, "lastWrite": now, "timestamp": now}),
        );
    }

    /// Globals as GetGlobals lists them: name -> {name, value, lastWrite}.
    pub fn globals_list(&self, persisted: bool) -> Obj {
        let c = self.cfg.borrow();
        let st = self.st.borrow();
        let (values, writes) = if persisted {
            (&c.globals, &c.global_writes)
        } else {
            (&st.temp, &st.temp_writes)
        };
        values
            .iter()
            .map(|(k, v)| {
                let w = writes.get(k).cloned().unwrap_or(Value::Null);
                (k.clone(), json!({"name": k, "value": v, "lastWrite": w}))
            })
            .collect()
    }

    /// User globals as TwitchGetUserGlobals/TwitchGetUserGlobal list them: every user's `name`
    /// (or every variable of user `user_id`), on one platform.
    pub fn user_globals_list(
        &self,
        platform: &str,
        name: Option<&str>,
        user_id: Option<&str>,
        persisted: bool,
    ) -> Vec<Value> {
        let c = self.cfg.borrow();
        let st = self.st.borrow();
        let all: Vec<&Value> = if persisted {
            c.user_globals.values().collect()
        } else {
            st.user_temp.values().collect()
        };
        let mut out = Vec::new();
        for e in all {
            let Some(e) = e.as_object() else { continue };
            if s(e, "platform") != platform || user_id.is_some_and(|id| s(e, "id") != id) {
                continue;
            }
            let Some(values) = e.get("values").and_then(Value::as_object) else {
                continue;
            };
            for (k, v) in values {
                if name.is_some_and(|n| n != k) {
                    continue;
                }
                let w = e
                    .get("lastWrite")
                    .and_then(|w| w.get(k))
                    .cloned()
                    .unwrap_or(Value::Null);
                out.push(if user_id.is_some() {
                    json!({"name": k, "value": v, "lastWrite": w})
                } else {
                    json!({"name": k, "userId": s(e, "id"), "userLogin": s(e, "login"), "userName": s(e, "name"),
                        "platform": platform, "value": v, "lastWrite": w})
                });
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::super::tests::{action, engine, log, run};
    use super::*;

    #[test]
    fn triggers_like_streamerbot() {
        let _g = crate::TEST_LOCK.lock().unwrap();
        let set = |src: i64, v: &str| json!({"type": 122, "destination": 0, "variableName": "g", "persisted": false, "source": src, "value": v});
        let main = action("a1", "main", json!([set(0, "1"), set(0, "1"), set(3, ""), set(3, "")]));
        let watch = |id: &str, kind: u32, what: &str| {
            let mut a = action(
                id,
                id,
                json!([log(&format!("T {what} %global.oldValue%>%global.newValue%"))]),
            );
            a["triggers"] = json!([{"variableName": "g", "persisted": 0, "id": id, "type": kind, "enabled": true}]);
            a
        };
        let (c, u, d) = (
            watch("c", 711, "created"),
            watch("u", 709, "updated"),
            watch("d", 712, "deleted"),
        );
        let e = engine(json!({"actions": [main, c, u, d]}));
        let mut out = run(async move {
            e.queue("a1", Obj::new());
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
            assert!(e.globals_list(false).is_empty());
        });
        out.sort();
        assert_eq!(
            out,
            [
                "T created %global.oldValue%>%global.newValue%",
                "T updated 1>",
                "T updated 1>1"
            ]
        );
    }
}
