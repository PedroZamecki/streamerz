//! Scenarios: actions, commands and settings in a JSON file, steps that fire triggers, run actions and
//! send chat, and what must come of it. They run on the real engine with no window, no login and nothing
//! sent anywhere (chat is kept, as in dev mode), in well under a second each:
//!
//! - `cargo test scenarios` runs every file in `tests/scenarios` (CI does too);
//! - `Streamerz --scenario FILE_OR_FOLDER...` runs them from the shipped app, and says what failed;
//! - the MCP server's `run_scenario` tool runs one given inline, so an assistant can test what it builds.
//!
//! ```json
//! {"name": "greets a follower",
//!  "actions": [{"name": "greet", "triggers": [{"type": 101}], "subActions": [{"type": 10, "text": "hi %user%"}]}],
//!  "config": {"commands": [...], "globals": {...}},
//!  "steps": [{"fire": 101, "args": {"user": "ana"}, "ran": 1},
//!            {"chat": {"platform": "kick", "user": "bo", "text": "!hi"}, "ran": 1},
//!            {"run": "greet", "args": {"user": "cy"}},
//!            {"wait": 200}],
//!  "expect": {"chat": ["hi ana", {"platform": "kick", "text": "hi bo"}], "log": ["hi"], "notLog": ["error"],
//!             "globals": {"count": 2}}}
//! ```
//!
//! Actions may leave out what an editor fills in (id, enabled, queue, trigger and sub-action ids).
//!
//! Platforms are fakes (`fakes`, see `fakes.rs`): a step `{"event": {"platform": "twitch", "type":
//! "channel.follow", "data": {...}}}` goes through that platform's real event handler, and
//! `expect.calls` checks the requests sub-actions made, in order, by partial match
//! (`"twitch POST moderation/bans"` or `{"platform": "twitch", "call": "...", "body": {...}}`).
//! `expect.runs` checks the actions that ran, in order: `{"action": "name", "args": {...}, "after":
//! {...}}`, where `after` is the arguments it ended with. `notCalls` lists requests that mustn't be made.
//! `{"request": {...}, "reply": {...}}` asks what a WebSocket client would; `{"app": "started"}`
//! (or test, addGroup, removeGroup, setGlobal, deleteGlobal, watchers; hotkey, mouse, midi, speech, clipboard, toastClicked and webhook for
//! what the system reports) does
//! what the window does.
//! `expect.config` checks the settings after (partial match: `{"timers": [{"enabled": true}]}`).
//! `ran` on a step counts the actions it started (for events and waits: those that finished);
//! `error` says the step must fail, with that text (a webhook refused, say).
//! `{tmp}` anywhere in a scenario is a folder of its own (removed after); `"debug": true` keeps
//! debug lines in the log for `expect.log`; `{http}` is a local web server's address (its requests
//! are `http` calls, answered from `fakes.http`); `"only": "linux"` or `"windows"` runs it there only.

mod fakes;

use crate::engine::Engine;
use serde_json::{json, Value};
use std::rc::Rc;
use std::sync::Arc;
use std::time::Duration;

/// The longest a step waits for its actions to finish, when it doesn't say (`wait`: exactly that).
const SETTLE_MS: u64 = 120;

/// One scenario's result: its name and what went wrong (nothing: it passed).
pub struct Outcome {
    pub name: String,
    pub failures: Vec<String>,
}

/// An action as an editor would save it: ids, enabled, queue, and ids on its triggers and sub-actions.
fn filled(mut a: Value, n: usize) -> Value {
    fn ids(list: Option<&mut Value>) {
        for x in list.and_then(Value::as_array_mut).into_iter().flatten() {
            if x.get("id").is_none() {
                x["id"] = crate::engine::uuid().into();
            }
            if x.get("enabled").is_none() {
                x["enabled"] = true.into();
            }
            ids(x.get_mut("subActions"));
        }
    }
    let o = a.as_object_mut().expect("an action is an object");
    o.entry("id").or_insert_with(|| format!("scenario-action-{n}").into());
    o.entry("name").or_insert_with(|| format!("action {n}").into());
    o.entry("enabled").or_insert(true.into());
    o.entry("queue")
        .or_insert("00000000-0000-0000-0000-000000000000".into());
    o.entry("triggers").or_insert(json!([]));
    o.entry("subActions").or_insert(json!([]));
    ids(o.get_mut("triggers"));
    ids(o.get_mut("subActions"));
    a
}

/// The settings a scenario starts from: the defaults, its `config` on top, its actions.
fn config(s: &Value) -> Result<crate::Config, String> {
    let mut base = serde_json::to_value(crate::Config::default()).map_err(|e| e.to_string())?;
    fn merge(into: &mut Value, from: &Value) {
        match (into, from) {
            (Value::Object(a), Value::Object(b)) => {
                for (k, v) in b {
                    merge(a.entry(k.clone()).or_insert(Value::Null), v);
                }
            }
            (a, b) => *a = b.clone(),
        }
    }
    if let Some(c) = s.get("config") {
        merge(&mut base, c);
    }
    let actions: Vec<Value> = s["actions"]
        .as_array()
        .into_iter()
        .flatten()
        .enumerate()
        .map(|(i, a)| filled(a.clone(), i + 1))
        .collect();
    base["actions"]["actions"] = actions.into();
    let mut commands = base["commands"].as_array().cloned().unwrap_or_default();
    for (i, c) in commands.iter_mut().enumerate() {
        let o = c.as_object_mut().ok_or("a command is an object")?;
        o.entry("id")
            .or_insert_with(|| format!("scenario-command-{}", i + 1).into());
        o.entry("enabled").or_insert(true.into());
        // Every platform's chat, unless the scenario says.
        o.entry("sources").or_insert(u32::MAX.into());
    }
    base["commands"] = commands.into();
    serde_json::from_value(base).map_err(|e| format!("config: {e}"))
}

/// Whether `want` (text, or {platform, text, bot}) is what was sent as `got`.
fn same_chat(want: &Value, got: &Value) -> bool {
    match want {
        Value::String(t) => got["text"] == t.as_str(),
        Value::Object(o) => o.iter().all(|(k, v)| got.get(k) == Some(v)),
        _ => false,
    }
}

/// A global's value as a scenario writes it: numbers compare as numbers ("2" and 2 are the same).
fn same_value(want: &Value, got: &Value) -> bool {
    let num = |v: &Value| v.as_f64().or_else(|| v.as_str()?.trim().parse().ok());
    match (num(want), num(got)) {
        (Some(a), Some(b)) => (a - b).abs() < 1e-9,
        _ => crate::vars::text(want) == crate::vars::text(got),
    }
}

/// Runs scenario `s` on a fresh engine. Call from a thread with no Tokio runtime.
pub fn run(s: &Value) -> Outcome {
    let name = s["name"].as_str().unwrap_or("(no name)").to_string();
    // `only`: "linux" or "windows"; elsewhere it passes untried.
    if s["only"].as_str().is_some_and(|os| os != std::env::consts::OS) {
        return Outcome {
            name: format!("{name} (not on this system)"),
            failures: Vec::new(),
        };
    }
    // `{tmp}`: a folder of its own, removed after.
    let tmp = s.to_string().contains("{tmp}").then(|| {
        let d = std::env::temp_dir().join(format!("streamerz-scenario-{}", crate::engine::uuid()));
        let _ = std::fs::create_dir_all(&d);
        d
    });
    // `{http}`: a local web server's address; what it's asked is kept as `http` calls.
    let http = s
        .to_string()
        .contains("{http}")
        .then(|| std::net::TcpListener::bind("127.0.0.1:0").ok())
        .flatten();
    let mut text = s.to_string();
    if let Some(d) = &tmp {
        let path = d.to_string_lossy().replace('\\', "/");
        text = text.replace("{tmp}", &path.replace('"', "\\\""));
    }
    if let Some(port) = http.as_ref().and_then(|l| l.local_addr().ok()).map(|a| a.port()) {
        text = text.replace("{http}", &format!("http://127.0.0.1:{port}"));
    }
    let owned: Value;
    let s = if tmp.is_some() || http.is_some() {
        owned = serde_json::from_str(&text).unwrap_or_else(|_| s.clone());
        &owned
    } else {
        s
    };
    let out = run_in(s, name, http);
    if let Some(d) = tmp {
        let _ = std::fs::remove_dir_all(d);
    }
    out
}

fn run_in(s: &Value, name: String, http: Option<std::net::TcpListener>) -> Outcome {
    let debug = s["debug"] == true;
    if debug {
        crate::set_debug_log(true);
    }
    let out = run_engine(s, name, http);
    if debug {
        crate::set_debug_log(false);
    }
    out
}

fn run_engine(s: &Value, name: String, http: Option<std::net::TcpListener>) -> Outcome {
    let cfg = match config(s) {
        Ok(c) => c,
        Err(e) => {
            return Outcome {
                name,
                failures: vec![e],
            }
        }
    };
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("runtime");
    crate::capture(true);
    let failures = tokio::task::LocalSet::new().block_on(&rt, async {
        let e = Engine::with_music(Arc::new(tokio::sync::watch::channel(cfg).0), |_| {}, &fakes::MUSIC);
        e.dev_mode();
        let fk = fakes::install(&e, s.get("fakes").unwrap_or(&Value::Null), http);
        // As the app starts them (each only when set up or used).
        e.watchers();
        e.processes_sync();
        e.midi_sync();
        let mut failures = Vec::new();
        let mut sent: Vec<Value> = Vec::new();
        for (i, step) in s["steps"].as_array().into_iter().flatten().enumerate() {
            let at = format!("step {}", i + 1);
            let before = e.history().len();
            let ran = match (step_once(&e, step), step.get("error").and_then(Value::as_str)) {
                (Err(err), Some(want)) if err.contains(want) => Ok(None),
                (Ok(_), Some(want)) => Err(format!("{at}: it worked; it should fail with {want:?}")),
                (r, _) => r.map_err(|err| format!("{at}: {err}")),
            };
            match step.get("wait").and_then(Value::as_u64) {
                Some(ms) => tokio::time::sleep(Duration::from_millis(ms)).await,
                // Until nothing runs or waits (a little more for what was spawned), at most SETTLE_MS.
                None => {
                    let until = tokio::time::Instant::now() + Duration::from_millis(SETTLE_MS);
                    let mut quiet = 0;
                    while quiet < 3 && tokio::time::Instant::now() < until {
                        tokio::time::sleep(Duration::from_millis(5)).await;
                        quiet = if e.pending().is_empty() { quiet + 1 } else { 0 };
                    }
                }
            }
            sent.extend(e.dev_take().unwrap_or_default());
            // ponytail: history keeps 100 runs; a step that finishes more can't be counted this way.
            let finished = e.history().len().saturating_sub(before);
            match (ran, step.get("ran").and_then(Value::as_u64)) {
                (Err(err), _) => failures.push(err),
                (Ok(n), Some(want)) if n.unwrap_or(finished) as u64 != want => {
                    failures.push(format!("{at}: {} actions ran, {want} expected", n.unwrap_or(finished)))
                }
                _ => {}
            }
        }
        let log = crate::capture(false);
        check(&e, &s["expect"], &sent, &log, &mut failures);
        check_calls(&s["expect"], &fk.calls.borrow(), &mut failures);
        check_runs(&e, &s["expect"], &mut failures);
        if let Some(want) = s["expect"].get("config") {
            let got = serde_json::to_value(&*e.config()).unwrap_or_default();
            if !has(want, &got) {
                let keys: serde_json::Map<String, Value> = want
                    .as_object()
                    .into_iter()
                    .flatten()
                    .map(|(k, _)| (k.clone(), got[k].clone()))
                    .collect();
                failures.push(format!("the settings aren't {want}; they hold {}", Value::Object(keys)));
            }
        }
        failures
    });
    fakes::uninstall();
    Outcome { name, failures }
}

/// One step; how many actions it started, when that's known.
fn step_once(e: &Rc<Engine>, step: &Value) -> Result<Option<usize>, String> {
    let args = step["args"].as_object().cloned().unwrap_or_default();
    if let Some(t) = step.get("fire") {
        let (code, tname) = crate::types::TRIGGERS
            .iter()
            .find(|(c, n)| t.as_u64() == Some(u64::from(*c)) || t.as_str().is_some_and(|s| s.eq_ignore_ascii_case(n)))
            .ok_or_else(|| format!("no trigger {t}"))?;
        return Ok(Some(e.fire(*code, tname, "Scenario", "scenario", &args, |t| {
            crate::engine::trigger_matches(*code, t, &args)
        })));
    }
    if let Some(c) = step.get("chat") {
        let g = |k: &str| c[k].as_str().unwrap_or("").to_string();
        let user = if g("user").is_empty() {
            "tester".into()
        } else {
            g("user")
        };
        let role = if g("role").is_empty() {
            "viewer".into()
        } else {
            g("role")
        };
        return e
            .inject_chat(&g("platform"), &user, &g("userId"), &role, &g("text"), c)
            .map(Some);
    }
    if let Some(a) = step.get("run").and_then(Value::as_str) {
        let id = {
            let c = e.config();
            c.actions
                .actions
                .iter()
                .find(|x| x.name == a || x.id == a)
                .map(|x| x.id.clone())
        }
        .ok_or_else(|| format!("no action {a}"))?;
        return e
            .queue(&id, args)
            .map(|_| Some(1))
            .ok_or_else(|| format!("{a} didn't run"));
    }
    if let Some(ev) = step.get("event") {
        return fakes::event(e, ev).map(|_| None);
    }
    // A WebSocket request, answered as the server does; `reply` is what the answer must hold.
    if let Some(req) = step.get("request") {
        let got = crate::servers::answer(e, req, true).unwrap_or(Value::Null);
        return match step.get("reply") {
            Some(want) if !has(want, &got) => Err(format!("{} answered {got}, expected {want}", req["request"])),
            _ => Ok(None),
        };
    }
    // What the window does: start, the Test button, user groups, globals.
    if let Some(op) = step.get("app").and_then(Value::as_str) {
        let g = |k: &str| step[k].as_str().unwrap_or("").to_string();
        match op {
            "started" => e.started(),
            "test" => {
                return e
                    .test(&g("action"))
                    .map(|_| Some(1))
                    .ok_or_else(|| format!("no action {}", g("action")))
            }
            "addGroup" => drop(e.add_group(&g("name"), false)),
            "removeGroup" => {
                let id = e
                    .config()
                    .groups
                    .iter()
                    .find(|x| x["name"] == g("name").as_str())
                    .map(|x| x["id"].clone());
                e.remove_group(id.as_ref().and_then(Value::as_str).unwrap_or(""));
            }
            "setGlobal" => e.put_global(&g("name"), step["value"].clone(), step["persisted"] != false),
            "deleteGlobal" => e.delete_global(&g("name"), step["persisted"] != false),
            // The clipboard changed (what the window reports).
            "clipboard" => {
                let text = step.get("text").and_then(Value::as_str);
                e.clipboard_changed(crate::ui::clipboard::args(
                    text.is_some(),
                    step["image"] == true,
                    text.unwrap_or(""),
                ))
            }
            // A POST to the HTTP server's /webhooks/<name>, as the server hands it over.
            "webhook" => {
                let body = step.get("body").map(|b| match b {
                    Value::String(t) => t.clone(),
                    v => v.to_string(),
                });
                e.custom_webhook(&g("name"), body.unwrap_or_default().as_bytes());
            }
            // A MIDI device sent a message (what the system reports).
            "midi" => {
                let b = |k: &str| step[k].as_u64().unwrap_or(0) as u8;
                let m = crate::midi::Msg {
                    status: b("status"),
                    data1: b("data1"),
                    data2: b("data2"),
                };
                e.midi_fire(&g("device"), &g("device"), &m, 0);
            }
            // A toast was clicked (what the system reports).
            "toastClicked" => e.toast_activated(&crate::notify::Toast {
                id: g("id"),
                title: g("title"),
                text: g("text"),
                attribution: g("attribution"),
                icon_path: g("iconPath"),
            }),
            // Restart the file watchers and tails (as saving their settings does).
            "watchers" => e.watchers(),
            // What the OS hooks report.
            "hotkey" => e.fire_hotkey(
                step["key"].as_i64().unwrap_or(0),
                step["modifiers"].as_i64().unwrap_or(0),
            ),
            "mouse" => e.fire_mouse(
                step["button"].as_i64().unwrap_or(1),
                step["clicks"].as_i64().unwrap_or(1),
                step["x"].as_i64().unwrap_or(0),
                step["y"].as_i64().unwrap_or(0),
            ),
            other => return Err(format!("no app step {other}")),
        }
        return Ok(None);
    }
    if step.get("wait").is_some() {
        return Ok(None);
    }
    Err(format!(
        "a step needs fire, chat, run, event, request, app or wait: {step}"
    ))
}

/// What the scenario expects, against what happened.
fn check(e: &Rc<Engine>, want: &Value, sent: &[Value], log: &[String], failures: &mut Vec<String>) {
    // Each wanted line, in order, somewhere in the log.
    let mut from = 0;
    for w in want["log"].as_array().into_iter().flatten().filter_map(Value::as_str) {
        match log[from..].iter().position(|l| l.contains(w)) {
            Some(i) => from += i + 1,
            None => failures.push(format!("the log never says {w:?} (in order); it said: {:?}", tail(log))),
        }
    }
    for w in want["notLog"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
    {
        if let Some(l) = log.iter().find(|l| l.contains(w)) {
            failures.push(format!("the log says {w:?}: {l}"));
        }
    }
    if let Some(chat) = want["chat"].as_array() {
        let texts: Vec<&Value> = sent.iter().collect();
        if chat.len() != sent.len() || !chat.iter().zip(&texts).all(|(w, g)| same_chat(w, g)) {
            failures.push(format!("chat sent {texts:?}, expected {chat:?}"));
        }
    }
    for (k, v) in want["globals"].as_object().into_iter().flatten() {
        let got = e
            .globals_list(true)
            .get(k)
            .cloned()
            .or_else(|| e.globals_list(false).get(k).cloned())
            .map(|g| g.get("value").cloned().unwrap_or(g));
        match got {
            Some(g) if same_value(v, &g) => {}
            g => failures.push(format!("global {k} is {}, expected {v}", g.unwrap_or(Value::Null))),
        }
    }
}

/// Whether `got` has everything `want` has: objects by their keys, lists item by item, numbers as
/// numbers ("2" is 2).
fn has(want: &Value, got: &Value) -> bool {
    match (want, got) {
        (Value::Object(w), Value::Object(g)) => w.iter().all(|(k, v)| g.get(k).is_some_and(|x| has(v, x))),
        (Value::Object(_), _) => false,
        (Value::Array(w), Value::Array(g)) => w.len() == g.len() && w.iter().zip(g).all(|(a, b)| has(a, b)),
        (Value::Null, Value::Null) => true,
        (w, g) => same_value(w, g),
    }
}

/// A wanted call: `"platform call"` or an object.
fn call_matches(want: &Value, got: &Value) -> bool {
    match want {
        Value::String(t) => {
            let (p, c) = t.split_once(' ').unwrap_or((t, ""));
            got["platform"] == p && (c.is_empty() || got["call"] == c)
        }
        w => has(w, got),
    }
}

/// `expect.calls` (in order, others may come between) and `expect.notCalls`.
fn check_calls(want: &Value, calls: &[Value], failures: &mut Vec<String>) {
    let mut from = 0;
    for w in want["calls"].as_array().into_iter().flatten() {
        match calls[from..].iter().position(|c| call_matches(w, c)) {
            Some(i) => from += i + 1,
            None => failures.push(format!(
                "no call {w} (in order); the calls were: {}",
                Value::from(calls[from.min(calls.len())..].to_vec())
            )),
        }
    }
    for w in want["notCalls"].as_array().into_iter().flatten() {
        if let Some(c) = calls.iter().find(|c| call_matches(w, c)) {
            failures.push(format!("a call {w} was made: {c}"));
        }
    }
}

/// `expect.runs`: the actions that ran, in order (others may come between), with their arguments.
fn check_runs(e: &Rc<Engine>, want: &Value, failures: &mut Vec<String>) {
    let h = e.history();
    let runs: Vec<Value> = h
        .iter()
        .map(|r| json!({"action": r.name, "args": r.args, "after": r.after}))
        .collect();
    let mut from = 0;
    for w in want["runs"].as_array().into_iter().flatten() {
        match runs[from..].iter().position(|r| has(w, r)) {
            Some(i) => from += i + 1,
            None => {
                let near: Vec<&Value> = runs[from..].iter().filter(|r| r["action"] == w["action"]).collect();
                failures.push(format!("no run {w} (in order); runs of that action: {near:?}"))
            }
        }
    }
}

/// The log's last lines, for a failure.
fn tail(log: &[String]) -> &[String] {
    &log[log.len().saturating_sub(8)..]
}

/// The scenarios in `path`: one file (one scenario, or a list of them) or every `.json` in a folder.
pub fn load(path: &std::path::Path) -> Result<Vec<Value>, String> {
    let mut files = Vec::new();
    if path.is_dir() {
        for e in std::fs::read_dir(path)
            .map_err(|e| format!("{}: {e}", path.display()))?
            .flatten()
        {
            if e.path().extension().is_some_and(|x| x == "json") {
                files.push(e.path());
            }
        }
        files.sort();
    } else {
        files.push(path.to_path_buf());
    }
    let mut out = Vec::new();
    for f in files {
        let text = std::fs::read_to_string(&f).map_err(|e| format!("{}: {e}", f.display()))?;
        match serde_json::from_str::<Value>(&text).map_err(|e| format!("{}: {e}", f.display()))? {
            Value::Array(list) => out.extend(list),
            one => out.push(one),
        }
    }
    Ok(out)
}

/// `Streamerz --scenario PATH...`: runs them all, prints each result; the exit code is how many failed.
pub fn cli(paths: &[String]) -> i32 {
    crate::set_quiet(true);
    let mut failed = 0;
    let mut total = 0;
    for p in paths {
        let list = match load(std::path::Path::new(p)) {
            Ok(l) => l,
            Err(e) => {
                println!("FAIL {p}: {e}");
                total += 1;
                failed += 1;
                continue;
            }
        };
        for s in list {
            total += 1;
            let o = run(&s);
            if o.failures.is_empty() {
                println!("ok   {}", o.name);
            } else {
                failed += 1;
                println!("FAIL {}", o.name);
                for f in o.failures {
                    println!("     {f}");
                }
            }
        }
    }
    println!("{} of {total} scenarios passed", total - failed);
    failed.min(100)
}

/// Every scenario's trigger and sub-action codes, for the coverage list.
#[cfg(test)]
fn codes(s: &Value) -> (Vec<u64>, Vec<u64>) {
    let (mut triggers, mut subs) = (Vec::new(), Vec::new());
    fn walk(list: &Value, subs: &mut Vec<u64>) {
        for x in list.as_array().into_iter().flatten() {
            if let Some(t) = x["type"].as_u64() {
                subs.push(t);
            }
            walk(&x["subActions"], subs);
        }
    }
    for a in s["actions"].as_array().into_iter().flatten() {
        triggers.extend(
            a["triggers"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|t| t["type"].as_u64()),
        );
        walk(&a["subActions"], &mut subs);
    }
    for st in s["steps"].as_array().into_iter().flatten() {
        triggers.extend(st["fire"].as_u64());
    }
    (triggers, subs)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cli_reports_missing_path() {
        let _g = crate::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let path = std::env::temp_dir().join(format!("streamerz-missing-{}", crate::engine::uuid()));
        assert_eq!(cli(&[path.to_string_lossy().into_owned()]), 1);
        crate::set_quiet(false);
    }

    /// Every file in tests/scenarios passes.
    #[test]
    fn scenarios() {
        let _g = crate::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/scenarios");
        let all = load(&dir).unwrap();
        assert!(!all.is_empty(), "no scenarios in {}", dir.display());
        let mut bad = Vec::new();
        for s in &all {
            let o = run(s);
            if !o.failures.is_empty() {
                bad.push(format!("{}:\n  {}", o.name, o.failures.join("\n  ")));
            }
        }
        assert!(
            bad.is_empty(),
            "{} of {} scenarios failed:\n{}",
            bad.len(),
            all.len(),
            bad.join("\n")
        );
    }

    /// Writes `target/scenario-coverage.txt`: which triggers and sub-actions some scenario uses, and
    /// which none does yet. `cargo test --profile fast scenario_coverage -- --ignored`
    #[test]
    #[ignore]
    fn scenario_coverage() {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/scenarios");
        let (mut t, mut sub) = (std::collections::BTreeSet::new(), std::collections::BTreeSet::new());
        for s in load(&dir).unwrap() {
            let (a, b) = codes(&s);
            t.extend(a);
            sub.extend(b);
        }
        let mut out = String::new();
        for (kind, table, used) in [
            ("trigger", crate::types::TRIGGERS, &t),
            ("sub-action", crate::types::SUB_ACTIONS, &sub),
        ] {
            let n = table.iter().filter(|(c, _)| used.contains(&u64::from(*c))).count();
            out += &format!("{kind}s: {n} of {} in a scenario\n", table.len());
            for (c, name) in table.iter().filter(|(c, _)| used.contains(&u64::from(*c))) {
                out += &format!("  tested {c} {name}\n");
            }
        }
        std::fs::write("target/scenario-coverage.txt", out).unwrap();
    }

    /// Codes from "1-5, 7": a range or one code per item.
    fn codes_in(list: &str) -> std::collections::BTreeSet<u64> {
        let mut out = std::collections::BTreeSet::new();
        for part in list.split(',').map(str::trim).filter(|p| !p.is_empty()) {
            let (a, b) = part.split_once('-').unwrap_or((part, part));
            let (a, b): (u64, u64) = (a.trim().parse().unwrap(), b.trim().parse().unwrap());
            out.extend(a..=b);
        }
        out
    }

    /// The ratchet: every trigger and every sub-action the engine runs is in a scenario, or excluded
    /// with a reason (`tests/scenarios/excluded.json`), or still on its `todo` list. Sub-actions the
    /// engine doesn't run are tested by `unsupported_sub_actions_are_skipped`. A code covered by a
    /// scenario must leave `todo`.
    #[test]
    fn every_code_is_tested_or_excluded() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests");
        let (mut t, mut sub) = (std::collections::BTreeSet::new(), std::collections::BTreeSet::new());
        for s in load(&root.join("scenarios")).unwrap() {
            let (a, b) = codes(&s);
            t.extend(a);
            sub.extend(b);
        }
        let ex: Value = serde_json::from_str(&std::fs::read_to_string(root.join("excluded.json")).unwrap()).unwrap();
        let excluded: std::collections::BTreeSet<u64> = ex["triggers"]
            .as_array()
            .unwrap()
            .iter()
            .flat_map(|e| {
                assert!(e["why"].as_str().is_some_and(|w| !w.is_empty()), "no reason: {e}");
                // A unit test named for it must be there: `module::tests::name`.
                if let Some(t) = e["test"].as_str() {
                    let (path, name) = t.rsplit_once("::tests::").expect("module::tests::name");
                    let file = root.join("../src").join(path.replace("::", "/") + ".rs");
                    let code = std::fs::read_to_string(&file).unwrap_or_else(|_| panic!("no {}", file.display()));
                    assert!(code.contains(&format!("fn {name}()")), "no test {t}");
                }
                codes_in(e["codes"].as_str().unwrap())
            })
            .collect();
        let mut bad = Vec::new();
        for (kind, table, used, todo, skip) in [
            (
                "trigger",
                crate::types::TRIGGERS,
                &t,
                codes_in(ex["todo"]["triggers"].as_str().unwrap_or("")),
                &excluded,
            ),
            (
                "sub-action",
                crate::types::SUB_ACTIONS,
                &sub,
                codes_in(ex["todo"]["subActions"].as_str().unwrap_or("")),
                &std::collections::BTreeSet::new(),
            ),
        ] {
            for (code, name) in table {
                let c = u64::from(*code);
                let runs = kind == "trigger" || crate::engine::supported(*code);
                let covered = used.contains(&c);
                if runs && !covered && !skip.contains(&c) && !todo.contains(&c) {
                    bad.push(format!("{kind} {code} {name}: no scenario uses it"));
                }
                if covered && todo.contains(&c) {
                    bad.push(format!("{kind} {code} {name}: tested now, take it off todo"));
                }
            }
        }
        assert!(bad.is_empty(), "{}", bad.join("\n"));
    }

    /// A sub-action the engine doesn't run is kept, logged once and skipped: the action goes on.
    #[test]
    fn unsupported_sub_actions_are_skipped() {
        let _g = crate::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let skipped: Vec<u32> = crate::types::SUB_ACTIONS
            .iter()
            .map(|(c, _)| *c)
            .filter(|c| !crate::engine::supported(*c) && *c != 0)
            .collect();
        assert!(!skipped.is_empty());
        let mut subs: Vec<Value> = skipped.iter().map(|c| json!({"type": c})).collect();
        subs.push(json!({"type": 1020, "logLevel": 2, "message": "went on"}));
        let mut expect_log: Vec<String> = skipped.iter().map(|c| format!("(type {c})")).collect();
        expect_log.push("went on".into());
        let s = json!({"name": "unsupported", "actions": [{"name": "all", "subActions": subs}],
            "steps": [{"run": "all"}, {"run": "all"}], "expect": {"log": expect_log}});
        let o = run(&s);
        assert!(o.failures.is_empty(), "{}", o.failures.join("\n"));
    }

    #[test]
    fn a_failing_scenario_says_why() {
        let _g = crate::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let s = json!({"name": "wrong on purpose",
            "actions": [{"name": "a", "subActions": [{"type": 10, "text": "hello"}]}],
            "steps": [{"run": "a"}, {"fire": "nope"}],
            "expect": {"chat": ["bye"], "log": ["never said"], "globals": {"x": 1}}});
        let o = run(&s);
        let all = o.failures.join("\n");
        assert_eq!(o.failures.len(), 4, "{all}");
        assert!(
            all.contains("no trigger")
                && all.contains("hello")
                && all.contains("never said")
                && all.contains("global x")
        );
    }

    #[test]
    fn actions_are_filled_in_as_an_editor_would() {
        let a = filled(json!({"subActions": [{"type": 10, "subActions": [{"type": 1}]}]}), 3);
        assert_eq!(a["name"], "action 3");
        assert!(a["subActions"][0]["id"].is_string() && a["subActions"][0]["subActions"][0]["enabled"] == true);
        let c = config(&json!({"config": {"commands": [{"command": "!hi"}]}})).unwrap();
        assert_eq!(c.commands[0]["sources"], u32::MAX);
    }

    #[test]
    fn values_compare_loosely() {
        assert!(same_value(&json!(2), &json!("2")));
        assert!(same_value(&json!("ana"), &json!("ana")));
        assert!(!same_value(&json!(1), &json!(2)));
        assert!(same_chat(
            &json!({"platform": "kick"}),
            &json!({"platform": "kick", "text": "x"})
        ));
    }
}
