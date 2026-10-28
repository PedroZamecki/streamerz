//! Plugins: WebAssembly modules in `plugins/` next to the settings file, run by an interpreter (wasmi)
//! on the engine's thread. Off until turned on: nothing is read or loaded for one that's off.
//!
//! The plugin's side (see PLUGINS.md): it exports `memory`, `alloc(len) -> ptr`, `manifest() -> packed`
//! and `on_event(ptr, len) -> packed` (packed: `ptr << 32 | len` of JSON in its memory, 0 for none). It
//! imports `streamerz.call(ptr, len) -> packed`: a JSON request to Streamerz, answered at once.
//! Everything that changes something (sending chat, running an action, a web request...) runs right
//! after the plugin's event returns; answers to web requests and timers come back as events.

use super::{Ctx, Engine, Flow};
use crate::model::Obj;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use std::rc::{Rc, Weak};
use wasmi::{Caller, Config, Instance, Linker, Memory, Module, Store, TypedFunc};

/// Its sub-action type ("Plugin", with `plugin` and `name`) and trigger type ("Plugin Event", with
/// `plugin` and `event`): outside Streamer.bot's codes, so its imports skip them.
pub const SUB_PLUGIN: u64 = 99800;
pub const T_PLUGIN: u32 = 99801;

/// Instructions a plugin may run per event before it's stopped.
const FUEL: u64 = 200_000_000;

/// A setting, or a sub-action's field.
#[derive(Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Field {
    pub key: String,
    pub label: String,
    /// "text", "lines", "number", "bool" or "choice".
    pub kind: String,
    pub choices: Vec<String>,
    pub default: Value,
    pub about: String,
}

#[derive(Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct SubDef {
    pub name: String,
    pub about: String,
    pub fields: Vec<Field>,
}

#[derive(Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct TriggerDef {
    pub name: String,
    pub about: String,
}

/// What a plugin says it is and adds.
#[derive(Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Manifest {
    pub name: String,
    pub version: String,
    pub about: String,
    pub settings: Vec<Field>,
    pub subactions: Vec<SubDef>,
    pub triggers: Vec<TriggerDef>,
    /// The triggers it wants to hear (Streamer.bot names, e.g. "TwitchChatMessage"; "*" for all).
    pub events: Vec<String>,
}

/// A plugin's settings in the config: on or off, and its settings' values.
#[derive(Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct PluginConfig {
    pub enabled: bool,
    pub settings: Obj,
}

/// What a plugin's `call` sees: who it is, and the changes it asked for.
struct Host {
    id: String,
    engine: Weak<Engine>,
    ops: Vec<Value>,
}

pub struct Loaded {
    pub id: String,
    pub manifest: Manifest,
    store: Store<Host>,
    memory: Memory,
    alloc: TypedFunc<i32, i32>,
    on_event: TypedFunc<(i32, i32), i64>,
    timers: Vec<(String, tokio::task::AbortHandle)>,
}

impl Drop for Loaded {
    fn drop(&mut self) {
        for (_, t) in &self.timers {
            t.abort();
        }
    }
}

/// Where plugins go.
pub fn dir() -> PathBuf {
    crate::config_path().with_file_name("plugins")
}

/// The plugins there: id (the file's name) and file.
pub fn installed() -> Vec<(String, PathBuf)> {
    let mut out: Vec<(String, PathBuf)> = std::fs::read_dir(dir())
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x == "wasm"))
        .filter_map(|p| Some((p.file_stem()?.to_string_lossy().into_owned(), p)))
        .collect();
    out.sort();
    out
}

/// JSON at `packed` (ptr << 32 | len) in `mem`.
fn read(mem: &[u8], packed: i64) -> Option<Value> {
    let (ptr, len) = ((packed as u64 >> 32) as usize, (packed as u64 & 0xffff_ffff) as usize);
    if packed == 0 {
        return None;
    }
    serde_json::from_slice(mem.get(ptr..ptr.checked_add(len)?)?).ok()
}

/// A host function's answer: `v` written into the plugin's memory (through its `alloc`), packed.
fn write(caller: &mut Caller<'_, Host>, v: &Value) -> i64 {
    let bytes = v.to_string().into_bytes();
    let Some(alloc) = caller
        .get_export("alloc")
        .and_then(|e| e.into_func())
        .and_then(|f| f.typed::<i32, i32>(&*caller).ok())
    else {
        return 0;
    };
    let Ok(ptr) = alloc.call(&mut *caller, bytes.len() as i32) else {
        return 0;
    };
    let Some(mem) = caller.get_export("memory").and_then(|e| e.into_memory()) else {
        return 0;
    };
    if mem.write(&mut *caller, ptr as usize, &bytes).is_err() {
        return 0;
    }
    ((ptr as u32 as i64) << 32) | bytes.len() as i64
}

/// Loads the plugin in `path` as `id`; `engine` answers its calls (none: only its manifest is wanted).
pub fn load(id: &str, path: &Path, engine: Weak<Engine>) -> Result<Loaded, String> {
    let bytes = std::fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let mut cfg = Config::default();
    cfg.consume_fuel(true);
    let wasm = wasmi::Engine::new(&cfg);
    let module = Module::new(&wasm, &bytes[..]).map_err(|e| format!("not a plugin: {e}"))?;
    let mut store = Store::new(
        &wasm,
        Host {
            id: id.into(),
            engine,
            ops: Vec::new(),
        },
    );
    let mut linker = <Linker<Host>>::new(&wasm);
    linker
        .func_wrap(
            "streamerz",
            "call",
            |mut caller: Caller<'_, Host>, ptr: i32, len: i32| -> i64 {
                let Some(mem) = caller.get_export("memory").and_then(|e| e.into_memory()) else {
                    return 0;
                };
                let req = read(mem.data(&caller), ((ptr as u32 as i64) << 32) | len as u32 as i64);
                let Some(req) = req else { return 0 };
                let answer = answer(caller.data_mut(), req);
                match answer {
                    Some(v) => write(&mut caller, &v),
                    None => 0,
                }
            },
        )
        .map_err(|e| e.to_string())?;
    store.set_fuel(FUEL).map_err(|e| e.to_string())?;
    let inst: Instance = linker
        .instantiate_and_start(&mut store, &module)
        .map_err(|e| format!("can't start: {e}"))?;
    let memory = inst.get_memory(&store, "memory").ok_or("no memory export")?;
    let alloc = inst
        .get_typed_func::<i32, i32>(&store, "alloc")
        .map_err(|e| format!("alloc: {e}"))?;
    let on_event = inst
        .get_typed_func::<(i32, i32), i64>(&store, "on_event")
        .map_err(|e| format!("on_event: {e}"))?;
    let manifest_f = inst
        .get_typed_func::<(), i64>(&store, "manifest")
        .map_err(|e| format!("manifest: {e}"))?;
    let packed = manifest_f.call(&mut store, ()).map_err(|e| format!("manifest: {e}"))?;
    let manifest: Manifest = read(memory.data(&store), packed)
        .and_then(|v| serde_json::from_value(v).ok())
        .ok_or("its manifest isn't valid JSON")?;
    Ok(Loaded {
        id: id.into(),
        manifest,
        store,
        memory,
        alloc,
        on_event,
        timers: Vec::new(),
    })
}

/// What a plugin's `call` asks that's answered at once; the rest is kept to run after its event.
fn answer(host: &mut Host, req: Value) -> Option<Value> {
    let op = req["op"].as_str().unwrap_or("");
    let e = host.engine.upgrade();
    match op {
        "global_get" => {
            let e = e?;
            let persisted = req["persisted"].as_bool().unwrap_or(true);
            let name = req["name"].as_str().unwrap_or("");
            Some(
                e.globals_list(persisted)
                    .get(name)
                    .map_or(Value::Null, |g| g["value"].clone()),
            )
        }
        "settings" => {
            let e = e?;
            let c = e.config();
            Some(Value::Object(
                c.plugins.get(&host.id).map(|p| p.settings.clone()).unwrap_or_default(),
            ))
        }
        "now" => Some(json!({"unix": crate::fmt::unix_now().as_secs_f64()})),
        _ => {
            host.ops.push(req);
            None
        }
    }
}

impl Loaded {
    /// Gives the plugin `ev`; its answer, and what it asked to do.
    fn deliver(&mut self, ev: &Value) -> Result<(Option<Value>, Vec<Value>), String> {
        let bytes = ev.to_string().into_bytes();
        let _ = self.store.set_fuel(FUEL);
        let ptr = self
            .alloc
            .call(&mut self.store, bytes.len() as i32)
            .map_err(|e| e.to_string())?;
        self.memory
            .write(&mut self.store, ptr as usize, &bytes)
            .map_err(|e| e.to_string())?;
        let r = self.on_event.call(&mut self.store, (ptr, bytes.len() as i32));
        let ops = std::mem::take(&mut self.store.data_mut().ops);
        let packed = r.map_err(|e| e.to_string())?;
        Ok((read(self.memory.data(&self.store), packed), ops))
    }
}

impl Engine {
    /// At start: the plugins that are on.
    pub fn plugins_start(self: &Rc<Self>) {
        let on: Vec<String> = self
            .config()
            .plugins
            .iter()
            .filter(|p| p.1.enabled)
            .map(|p| p.0.clone())
            .collect();
        for id in on {
            self.plugin_load(&id);
        }
    }

    fn plugin_load(self: &Rc<Self>, id: &str) {
        let Some((_, path)) = installed().into_iter().find(|p| p.0 == id) else {
            return crate::say(format!("Plugin {id}: not in {}", dir().display()));
        };
        match load(id, &path, Rc::downgrade(self)) {
            Ok(p) => {
                crate::say(format!("Plugin {} {} on", p.manifest.name, p.manifest.version));
                self.plugins.borrow_mut().push(p);
                let settings = self.plugin_settings(id);
                self.plugin_event(id, json!({"kind": "start", "settings": settings}));
            }
            Err(e) => crate::say(format!("Plugin {id}: {e}")),
        }
    }

    fn plugin_settings(&self, id: &str) -> Value {
        Value::Object(
            self.config()
                .plugins
                .get(id)
                .map(|p| p.settings.clone())
                .unwrap_or_default(),
        )
    }

    /// Turns plugin `id` on or off (and keeps it so).
    pub fn plugin_enable(self: &Rc<Self>, id: &str, on: bool) {
        self.change(|c| c.plugins.entry(id.into()).or_default().enabled = on);
        self.plugins.borrow_mut().retain(|p| p.id != id);
        if on {
            self.plugin_load(id);
        } else {
            crate::say(format!("Plugin {id} off"));
        }
    }

    /// New settings for plugin `id`: kept, and told to it.
    pub fn plugin_set(self: &Rc<Self>, id: &str, settings: Obj) {
        self.change(|c| c.plugins.entry(id.into()).or_default().settings = settings.clone());
        self.plugin_event(id, json!({"kind": "settings", "settings": settings}));
    }

    /// The plugins that are on: id and manifest.
    pub fn plugins_on(&self) -> Vec<(String, Manifest)> {
        self.plugins
            .borrow()
            .iter()
            .map(|p| (p.id.clone(), p.manifest.clone()))
            .collect()
    }

    /// Gives plugin `id` the event `ev`, then does what it asked; its answer.
    pub fn plugin_event(self: &Rc<Self>, id: &str, ev: Value) -> Option<Value> {
        let r = {
            let mut ps = self.plugins.borrow_mut();
            let p = ps.iter_mut().find(|p| p.id == id)?;
            p.deliver(&ev)
        };
        match r {
            Ok((reply, ops)) => {
                for op in ops {
                    self.plugin_op(id, op);
                }
                reply
            }
            Err(e) => {
                crate::say(format!("Plugin {id}: {e}"));
                None
            }
        }
    }

    /// A trigger fired: told to the plugins that listen to it (after, on their own: never inside
    /// the code that fired it).
    pub(super) fn plugins_hear(self: &Rc<Self>, name: &str, args: &Obj) {
        let want: Vec<String> = self
            .plugins
            .borrow()
            .iter()
            .filter(|p| p.manifest.events.iter().any(|e| e == "*" || e == name))
            .map(|p| p.id.clone())
            .collect();
        if want.is_empty() {
            return;
        }
        let (e, ev) = (self.clone(), json!({"kind": "trigger", "name": name, "args": args}));
        tokio::task::spawn_local(async move {
            for id in want {
                e.plugin_event(&id, ev.clone());
            }
        });
    }

    /// The Plugin sub-action: the plugin runs it with the action's arguments; what it answers in
    /// `args` is added to them, and `"stop": true` stops the action.
    pub(super) fn plugin_sub(self: &Rc<Self>, ctx: &mut Ctx, o: &Obj) -> Flow {
        let (id, name) = (super::s(o, "plugin").to_string(), super::s(o, "name").to_string());
        let ev = json!({"kind": "subaction", "name": name, "fields": o, "args": ctx.args});
        let Some(reply) = self.plugin_event(&id, ev) else {
            if !self.plugins.borrow().iter().any(|p| p.id == id) {
                crate::say(format!("Plugin {id} is off: its {name} skipped"));
            }
            return Flow::Continue;
        };
        if let Some(a) = reply["args"].as_object() {
            ctx.args.extend(a.clone());
        }
        if reply["stop"].as_bool() == Some(true) {
            Flow::BreakToRoot
        } else {
            Flow::Continue
        }
    }

    /// Does what plugin `id` asked.
    fn plugin_op(self: &Rc<Self>, id: &str, op: Value) {
        let s = |k: &str| op[k].as_str().unwrap_or("").to_string();
        match s("op").as_str() {
            "log" => crate::say(format!("Plugin {id}: {}", s("text"))),
            "say" => {
                let p = match s("platform").as_str() {
                    "" => "twitch".to_string(),
                    p => p.to_string(),
                };
                self.send_chat(&p, &s("text"), op["bot"].as_bool().unwrap_or(true));
            }
            "run_action" => {
                let args = op["args"].as_object().cloned().unwrap_or_default();
                let found = {
                    let c = self.config();
                    let want = s("action");
                    c.actions
                        .actions
                        .iter()
                        .find(|a| a.id == want || a.name == want)
                        .map(|a| a.id.clone())
                };
                match found {
                    Some(a) => drop(self.queue(&a, args)),
                    None => crate::say(format!("Plugin {id}: no action {}", s("action"))),
                }
            }
            "global_set" => self.put_global(
                &s("name"),
                op["value"].clone(),
                op["persisted"].as_bool().unwrap_or(true),
            ),
            "trigger" => {
                let args = op["args"].as_object().cloned().unwrap_or_default();
                let (pid, ev) = (id.to_string(), s("event"));
                self.fire(T_PLUGIN, "Plugin Event", "Plugins", "plugin", &args, |t| {
                    super::s(t, "plugin") == pid && super::s(t, "event") == ev
                });
            }
            "http" => {
                let (e, id) = (self.clone(), id.to_string());
                tokio::task::spawn_local(async move {
                    let headers: Vec<(String, String)> = op["headers"]
                        .as_object()
                        .into_iter()
                        .flatten()
                        .map(|(k, v)| (k.clone(), v.as_str().unwrap_or("").to_string()))
                        .collect();
                    let method = match op["method"].as_str() {
                        Some(m) if !m.is_empty() => m.to_uppercase(),
                        _ => "GET".into(),
                    };
                    let body = op["body"].as_str().unwrap_or("").as_bytes().to_vec();
                    let url = op["url"].as_str().unwrap_or("");
                    let ev = match crate::http::request(&method, url, &headers, &body).await {
                        Ok(r) => json!({"kind": "http", "id": op["id"], "status": r.status, "body": r.text()}),
                        Err(err) => json!({"kind": "http", "id": op["id"], "status": 0, "error": err}),
                    };
                    e.plugin_event(&id, ev);
                });
            }
            "timer" => {
                let name = s("id");
                let ms = op["ms"].as_u64().unwrap_or(1000).max(50);
                let repeat = op["repeat"].as_bool().unwrap_or(false);
                let (e, pid, n) = (self.clone(), id.to_string(), name.clone());
                let t = tokio::task::spawn_local(async move {
                    loop {
                        tokio::time::sleep(std::time::Duration::from_millis(ms)).await;
                        e.plugin_event(&pid, json!({"kind": "timer", "id": n}));
                        if !repeat {
                            break;
                        }
                    }
                });
                let mut ps = self.plugins.borrow_mut();
                if let Some(p) = ps.iter_mut().find(|p| p.id == id) {
                    // One timer per name: a new one replaces it.
                    for (_, h) in p.timers.iter().filter(|(k, _)| *k == name) {
                        h.abort();
                    }
                    p.timers.retain(|(k, _)| *k != name);
                    p.timers.push((name, t.abort_handle()));
                }
            }
            "timer_cancel" => {
                let name = s("id");
                if let Some(p) = self.plugins.borrow_mut().iter_mut().find(|p| p.id == id) {
                    p.timers.retain(|(k, h)| {
                        let keep = *k != name;
                        if !keep {
                            h.abort();
                        }
                        keep
                    });
                }
            }
            other => crate::say(format!("Plugin {id}: no op {other}")),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A plugin in WebAssembly text: its manifest, and on any event it logs and asks to send chat.
    const ECHO: &str = r#"(module
      (import "streamerz" "call" (func $call (param i32 i32) (result i64)))
      (memory (export "memory") 1)
      (global $next (mut i32) (i32.const 1024))
      (data (i32.const 0) "{\"name\":\"Echo\",\"version\":\"1.0\",\"events\":[\"TwitchFollow\"],\"subactions\":[{\"name\":\"Hi\"}]}")
      (data (i32.const 200) "{\"op\":\"say\",\"text\":\"hi from echo\"}")
      (data (i32.const 300) "{\"args\":{\"echoed\":true}}")
      (func (export "alloc") (param $n i32) (result i32)
        (local $p i32)
        (local.set $p (global.get $next))
        (global.set $next (i32.add (global.get $next) (local.get $n)))
        (local.get $p))
      (func (export "manifest") (result i64)
        (i64.const 86))
      (func (export "on_event") (param i32 i32) (result i64)
        (drop (call $call (i32.const 200) (i32.const 34)))
        (i64.or (i64.shl (i64.const 300) (i64.const 32)) (i64.const 24))))"#;

    #[test]
    fn runs_a_plugin() {
        let dir = std::env::temp_dir().join(format!("sz-plugin-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("echo.wasm");
        std::fs::write(&path, ECHO).unwrap();
        let mut p = load("echo", &path, Weak::new()).unwrap();
        assert_eq!(p.manifest.name, "Echo");
        assert_eq!(p.manifest.subactions[0].name, "Hi");
        let (reply, ops) = p.deliver(&json!({"kind": "subaction"})).unwrap();
        assert_eq!(reply.unwrap()["args"]["echoed"], true);
        assert_eq!(ops[0]["op"], "say");
        let _ = std::fs::remove_dir_all(dir);
    }

    /// With STREAMERZ_PLUGIN_OUT=<dir>: writes the echo plugin there as a .wasm, to try by hand.
    #[test]
    fn writes_echo_for_trying() {
        if let Some(d) = std::env::var_os("STREAMERZ_PLUGIN_OUT") {
            std::fs::write(Path::new(&d).join("echo.wasm"), wat::parse_str(ECHO).unwrap()).unwrap();
        }
    }

    #[test]
    fn a_runaway_plugin_is_stopped() {
        let spin = r#"(module (memory (export "memory") 1)
          (data (i32.const 0) "{}")
          (func (export "alloc") (param i32) (result i32) (i32.const 100))
          (func (export "manifest") (result i64) (i64.const 2))
          (func (export "on_event") (param i32 i32) (result i64) (loop (br 0)) (i64.const 0)))"#;
        let dir = std::env::temp_dir().join(format!("sz-spin-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("spin.wasm");
        std::fs::write(&path, spin).unwrap();
        let mut p = load("spin", &path, Weak::new()).unwrap();
        assert!(p.deliver(&json!({})).is_err());
        let _ = std::fs::remove_dir_all(dir);
    }
}
