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

use crate::engine::Engine;
use serde_json::{json, Value};
use std::rc::Rc;
use std::sync::Arc;
use std::time::Duration;

/// How long a step's actions get to finish before the next step (a step's `wait` changes it).
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
        let e = Engine::new(Arc::new(tokio::sync::watch::channel(cfg).0), |_| {});
        e.dev_mode();
        let mut failures = Vec::new();
        let mut sent: Vec<Value> = Vec::new();
        for (i, step) in s["steps"].as_array().into_iter().flatten().enumerate() {
            let at = format!("step {}", i + 1);
            let ran = step_once(&e, step).map_err(|err| format!("{at}: {err}"));
            match (ran, step.get("ran").and_then(Value::as_u64)) {
                (Err(err), _) => failures.push(err),
                (Ok(Some(n)), Some(want)) if n as u64 != want => {
                    failures.push(format!("{at}: {n} actions ran, {want} expected"))
                }
                _ => {}
            }
            let wait = step.get("wait").and_then(Value::as_u64).unwrap_or(SETTLE_MS);
            tokio::time::sleep(Duration::from_millis(wait)).await;
            sent.extend(e.dev_take().unwrap_or_default());
        }
        let log = crate::capture(false);
        check(&e, &s["expect"], &sent, &log, &mut failures);
        failures
    });
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
        return Ok(Some(e.fire(*code, tname, "Scenario", "scenario", &args, |_| true)));
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
    if step.get("wait").is_some() {
        return Ok(None);
    }
    Err(format!("a step needs fire, chat, run or wait: {step}"))
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
