//! The engine, as in Streamer.bot: triggers queue actions, actions run their sub-actions in order
//! (or one at random), in action queues (blocking: one at a time; or not), with arguments, globals,
//! signals, history and pending runs. It all runs on the runtime thread (a LocalSet); actions and
//! persisted globals live in the config, so the window and the file see the same thing.

use crate::model::{Action, Obj};
use crate::types::{name, SUB_ACTIONS, TRIGGERS};
use crate::vars::{self, auto_type, text};
use crate::Config;
use serde_json::{json, Value};
use std::cell::RefCell;
use std::collections::{HashMap, HashSet, VecDeque};
use std::future::Future;
use std::pin::Pin;
use std::rc::Rc;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{oneshot, watch};

pub mod events;
pub mod globals;
mod rest;

pub const DEFAULT_QUEUE: &str = "00000000-0000-0000-0000-000000000000";
const HISTORY: usize = 100;

// Trigger types (types.rs TRIGGERS) the engine fires itself.
pub const T_TEST: u32 = 702;
pub const T_STARTED: u32 = 706;
const T_CUSTOM: u32 = 18001;

/// How a sub-action list ended (Streamer.bot's own names).
#[derive(Clone, Copy, PartialEq, Debug)]
enum Flow {
    Continue,
    /// Break: stops the whole action, through inline Run Actions too.
    BreakToRoot,
    LoopBreak,
    LoopContinue,
}

/// A queued or running action.
struct Run {
    id: String,
    action: String,
    name: String,
    args: Obj,
    queued_at: String,
}

/// One past run, for the history view (newest last).
#[derive(Clone)]
pub struct Record {
    pub action_id: String,
    pub name: String,
    /// The arguments it started with, and those it ended with.
    pub args: Obj,
    pub after: Obj,
    pub queued_at: String,
    pub ms: f64,
}

#[derive(Default)]
struct Queue {
    paused: bool,
    waiting: VecDeque<Run>,
    /// Running runs: id and action name.
    running: Vec<(String, String)>,
}

#[derive(Default)]
struct State {
    /// Non-persisted globals, and non-persisted user globals by user ("platform:id").
    temp: Obj,
    temp_writes: Obj,
    user_temp: Obj,
    queues: HashMap<String, Queue>,
    history: VecDeque<Record>,
    waiters: HashMap<String, Vec<oneshot::Sender<Obj>>>,
    signals: HashMap<String, VecDeque<Obj>>,
    /// Sub-action types already reported as unsupported.
    warned: HashSet<u64>,
    save_pending: bool,
    /// Command cooldowns: when each command (id) and each command per user ("id:user id") is
    /// usable again; the non-persisted counters, keyed like Config.counters.
    cooldowns: HashMap<String, std::time::Instant>,
    counters: Obj,
    /// Chat lines seen, for timers' Lines.
    chat_lines: u64,
    /// The running timers, stopped when timers change.
    timers: Vec<tokio::task::AbortHandle>,
}

pub struct Engine {
    cfg: Arc<watch::Sender<Config>>,
    /// Writes the config (a no-op in tests).
    save: fn(&Config),
    st: RefCell<State>,
    /// WebSocket connections listening to events.
    hub: RefCell<Vec<events::Listener>>,
    twitch: RefCell<Option<crate::twitch::TwitchHandle>>,
}

/// A running action's context.
struct Ctx {
    args: Obj,
}

type Fut<'a, T> = Pin<Box<dyn Future<Output = T> + 'a>>;

fn s<'a>(o: &'a Obj, k: &str) -> &'a str {
    o.get(k).and_then(Value::as_str).unwrap_or("")
}

fn b(o: &Obj, k: &str, default: bool) -> bool {
    o.get(k).and_then(Value::as_bool).unwrap_or(default)
}

fn n(o: &Obj, k: &str) -> f64 {
    match o.get(k) {
        Some(Value::String(t)) => t.trim().parse().unwrap_or(0.0),
        Some(v) => v.as_f64().unwrap_or(0.0),
        None => 0.0,
    }
}

fn subs(o: &Obj) -> &[Value] {
    o.get("subActions").and_then(Value::as_array).map_or(&[], Vec::as_slice)
}

/// What the window asks of the engine: run this on the engine's thread.
pub type UiCmd = Box<dyn FnOnce(&Rc<Engine>) + Send>;

/// Whether the engine runs this sub-action type (the rest are kept, logged once and skipped).
pub fn supported(kind: u32) -> bool {
    matches!(
        kind,
        2 | 4 | 6 | 8 | 9 | 10 | 23 | 120..=130 | 500 | 501 | 560 | 1001..=1005 | 1007 | 1009..=1011 | 1013..=1017 | 1020 | 1022 | 1023 | 1028
            | 1031..=1034 | 1038 | 10001 | 99900..=99904
    )
}

/// A random version 4 UUID.
pub fn uuid() -> String {
    let r = |bits: u32| crate::math::secure_u64() >> (64 - bits);
    let (a, b2, c, d, e) = (r(32), r(16), r(12), r(14), r(48));
    format!("{a:08x}-{b2:04x}-4{c:03x}-{:04x}-{e:012x}", 0x8000 | d)
}

/// Enabled / Disabled / Toggle (0, 1, 2) applied to a current state.
fn new_state(state: f64, on: bool) -> bool {
    match state as i64 {
        0 => true,
        1 => false,
        _ => !on,
    }
}

/// Compare for If/Else and While: numbers when both sides are (with Auto Type), else text.
fn compare(a: &str, b2: &str, auto: bool) -> std::cmp::Ordering {
    if auto {
        if let (Some(x), Some(y)) = (auto_type(a).as_f64(), auto_type(b2).as_f64()) {
            return x.partial_cmp(&y).unwrap_or(std::cmp::Ordering::Equal);
        }
    }
    a.cmp(b2)
}

/// .NET regexes are mostly this syntax; (?'name') becomes (?<name>).
pub fn regex(p: &str) -> Option<regex_lite::Regex> {
    let p = p.replace("(?'", "(?<").replace("'>", ">");
    let p = fix_named(&p);
    match regex_lite::Regex::new(&p) {
        Ok(r) => Some(r),
        Err(e) => {
            crate::say(format!("regex {p}: {e}"));
            None
        }
    }
}

/// `(?'name'...)` is rewritten above to `(?<name>...)` but leaves the closing quote as `'`: fix it.
fn fix_named(p: &str) -> String {
    let mut out = String::new();
    let mut rest = p;
    while let Some(i) = rest.find("(?<") {
        out += &rest[..i + 3];
        rest = &rest[i + 3..];
        // Lookbehind (?<= (?<! isn't a name.
        if rest.starts_with('=') || rest.starts_with('!') {
            continue;
        }
        if let Some(j) = rest.find(['>', '\'']) {
            out += &rest[..j];
            out.push('>');
            rest = &rest[j + 1..];
        }
    }
    out + rest
}

/// Regex Match's arguments: match.count, match[i] numbered like .NET (unnamed groups first, then
/// named ones), and one argument per named group.
pub fn match_args(r: &regex_lite::Regex, text: &str, args: &mut Obj) -> bool {
    let Some(c) = r.captures(text) else { return false };
    let names: Vec<Option<&str>> = r.capture_names().collect();
    let order = (0..names.len())
        .filter(|&i| i == 0 || names[i].is_none())
        .chain((1..names.len()).filter(|&i| names[i].is_some()));
    for (k, i) in order.enumerate() {
        let v = c.get(i).map_or("", |m| m.as_str());
        args.insert(format!("match[{k}]"), v.into());
        if let Some(name) = names[i] {
            args.insert(name.into(), v.into());
        }
    }
    args.insert("match.count".into(), names.len().into());
    true
}

impl Engine {
    pub fn new(cfg: Arc<watch::Sender<Config>>, save: fn(&Config)) -> Rc<Self> {
        Rc::new(Engine {
            cfg,
            save,
            st: RefCell::default(),
            hub: RefCell::default(),
            twitch: RefCell::default(),
        })
    }

    pub fn set_twitch(&self, handle: crate::twitch::TwitchHandle) {
        *self.twitch.borrow_mut() = Some(handle);
    }

    pub fn twitch(&self) -> Option<crate::twitch::TwitchHandle> {
        self.twitch.borrow().clone()
    }

    /// The Twitch runner, started on first use: nothing runs for Twitch until it's turned on.
    pub fn twitch_start(self: &Rc<Self>) -> crate::twitch::TwitchHandle {
        if let Some(t) = self.twitch() {
            return t;
        }
        let t = crate::twitch::start(self.clone(), &self.cfg);
        self.set_twitch(t.clone());
        t
    }

    pub fn twitch_connected(&self) -> bool {
        self.twitch().is_some_and(|t| t.is_connected())
    }

    pub fn twitch_broadcaster(&self) -> Option<crate::twitch::TwitchAccount> {
        self.cfg.borrow().twitch.broadcaster.clone()
    }

    pub fn send_twitch_message(
        &self,
        text: &str,
        reply_id: Option<&str>,
        use_bot: bool,
        fallback: bool,
    ) -> Result<(), String> {
        let tw = self.twitch().ok_or("Twitch is not connected")?;
        let text = text.to_string();
        let reply_id = reply_id.map(String::from);
        tokio::task::spawn_local(async move {
            let _ = tw.send_message(text, reply_id, use_bot, fallback).await;
        });
        Ok(())
    }

    pub fn clear_first_words(&self) {
        if let Some(tw) = self.twitch() {
            tw.clear_first_words();
        }
    }

    /// Run `f` on the config, and save it soon (once for a burst of changes).
    pub fn change(self: &Rc<Self>, f: impl FnOnce(&mut Config)) {
        self.cfg.send_modify(f);
        crate::ui::changed();
        let mut st = self.st.borrow_mut();
        if st.save_pending {
            return;
        }
        st.save_pending = true;
        let me = self.clone();
        tokio::task::spawn_local(async move {
            tokio::time::sleep(Duration::from_secs(1)).await;
            me.st.borrow_mut().save_pending = false;
            (me.save)(&me.cfg.borrow());
        });
    }

    pub fn config(&self) -> watch::Ref<'_, Config> {
        self.cfg.borrow()
    }

    pub fn subscribe(&self) -> watch::Receiver<Config> {
        self.cfg.subscribe()
    }

    fn action<T>(&self, id: &str, f: impl FnOnce(&Action) -> T) -> Option<T> {
        self.cfg.borrow().actions.actions.iter().find(|a| a.id == id).map(f)
    }

    fn expand(&self, ctx: &Ctx, t: &str) -> String {
        vars::expand(t, &ctx.args, &self.cfg.borrow().globals)
    }

    fn simple(&self, ctx: &Ctx, t: &str) -> String {
        vars::simple(t, &ctx.args, &self.cfg.borrow().globals)
    }

    /// Fire a trigger: every enabled action with an enabled trigger of this type that `matches`
    /// gets queued with `args` plus the trigger's own (triggerId, triggerName, ...). How many.
    pub fn fire(
        self: &Rc<Self>,
        kind: u32,
        trigger_name: &str,
        category: &str,
        source: &str,
        args: &Obj,
        matches: impl Fn(&Obj) -> bool,
    ) -> usize {
        let hits: Vec<(String, String)> = {
            let c = self.cfg.borrow();
            c.actions
                .actions
                .iter()
                .filter(|a| a.enabled)
                .flat_map(|a| {
                    a.triggers
                        .iter()
                        .filter(|t| t.get("type").and_then(Value::as_u64) == Some(kind as u64))
                        .filter(|t| b(t, "enabled", true) && matches(t))
                        .map(|t| (a.id.clone(), s(t, "id").to_string()))
                })
                .collect()
        };
        for (action, trigger) in &hits {
            let mut a = args.clone();
            a.insert("triggerId".into(), trigger.as_str().into());
            a.insert("triggerName".into(), trigger_name.into());
            a.insert("triggerCategory".into(), category.into());
            a.insert("eventSource".into(), source.into());
            a.insert("__source".into(), name(TRIGGERS, kind).unwrap_or("").into());
            self.queue(action, a);
        }
        hits.len()
    }

    /// Streamer.bot Started.
    pub fn started(self: &Rc<Self>) {
        self.fire(
            T_STARTED,
            "Streamer.bot Started",
            "Core",
            "streamerbot",
            &Obj::new(),
            |_| true,
        );
        self.emit("Misc", "StreamerbotStarted", || json!({}));
    }

    /// Run an action as its Test trigger does (isTest true). Its arguments, if it ran.
    pub fn test(self: &Rc<Self>, action: &str) -> Option<Obj> {
        let mut args = Obj::new();
        args.insert("isTest".into(), true.into());
        args.insert("triggerName".into(), "Test".into());
        args.insert("triggerCategory".into(), "Core".into());
        args.insert("eventSource".into(), "streamerbot".into());
        args.insert("__source".into(), name(TRIGGERS, T_TEST).unwrap_or("Test").into());
        self.queue(action, args)
    }

    /// Queue an action (by id or name) with these arguments; it starts now unless its queue is
    /// busy (blocking) or paused. Its arguments as queued (with actionId, actionName, queueName,
    /// runningActionId and actionQueuedAt); None when there's no such enabled action.
    pub fn queue(self: &Rc<Self>, action: &str, mut args: Obj) -> Option<Obj> {
        let (id, aname, queue, concurrent) = {
            let c = self.cfg.borrow();
            let a = c
                .actions
                .actions
                .iter()
                .find(|a| a.id == action)
                .or_else(|| c.actions.actions.iter().find(|a| a.name == action))?;
            if !a.enabled {
                return None;
            }
            let q = a
                .queue
                .clone()
                .filter(|q| !q.is_empty())
                .unwrap_or(DEFAULT_QUEUE.into());
            (a.id.clone(), a.name.clone(), q, a.concurrent)
        };
        let queue_name = if queue == DEFAULT_QUEUE {
            "Default".to_string()
        } else {
            let c = self.cfg.borrow();
            c.actions
                .queues
                .iter()
                .find(|q| s(q, "id") == queue)
                .map(|q| s(q, "name").to_string())
                .unwrap_or_default()
        };
        let rid = uuid();
        let queued_at = globals::iso_now();
        // A run queued by another action keeps that one's (Streamer.bot does the same).
        for (k, v) in [
            ("actionId", &id),
            ("actionName", &aname),
            ("queueName", &queue_name),
            ("runningActionId", &rid),
            ("actionQueuedAt", &queued_at),
        ] {
            args.entry(k).or_insert_with(|| v.as_str().into());
        }
        let out = args.clone();
        let run = Run {
            id: rid,
            action: id,
            name: aname,
            args,
            queued_at,
        };
        if concurrent {
            // Outside any queue.
            let me = self.clone();
            tokio::task::spawn_local(async move { me.execute(run).await });
            return Some(out);
        }
        self.st
            .borrow_mut()
            .queues
            .entry(queue.clone())
            .or_default()
            .waiting
            .push_back(run);
        self.pump(&queue);
        crate::ui::changed();
        Some(out)
    }

    fn blocking(&self, queue: &str) -> bool {
        let c = self.cfg.borrow();
        if queue == DEFAULT_QUEUE {
            return c.actions.blocking;
        }
        c.actions
            .queues
            .iter()
            .find(|q| s(q, "id") == queue)
            .is_some_and(|q| b(q, "blocking", false))
    }

    /// Start what a queue can run now.
    fn pump(self: &Rc<Self>, queue: &str) {
        let blocking = self.blocking(queue);
        loop {
            let run = {
                let mut st = self.st.borrow_mut();
                let q = st.queues.entry(queue.into()).or_default();
                if q.paused || (blocking && !q.running.is_empty()) {
                    return;
                }
                let Some(run) = q.waiting.pop_front() else { return };
                q.running.push((run.id.clone(), run.name.clone()));
                run
            };
            let (me, queue) = (self.clone(), queue.to_string());
            tokio::task::spawn_local(async move {
                let id = run.id.clone();
                me.execute(run).await;
                if let Some(q) = me.st.borrow_mut().queues.get_mut(&queue) {
                    q.running.retain(|(r, _)| *r != id);
                }
                me.pump(&queue);
            });
        }
    }

    /// Run an action's sub-actions, with the run's arguments plus the action's own.
    async fn execute(self: &Rc<Self>, run: Run) {
        let Some((subs, random)) = self.action(&run.action, |a| (a.sub_actions.clone(), a.random_action)) else {
            return;
        };
        let mut ctx = Ctx { args: run.args };
        let start = ctx.args.clone();
        let started = (std::time::Instant::now(), globals::iso_now());
        self.emit(
            "Raw",
            "Action",
            || json!({"id": run.id, "actionId": run.action, "name": run.name, "arguments": start}),
        );
        let subs: Vec<Value> = subs.into_iter().map(Value::Object).collect();
        self.list(&mut ctx, &subs, random).await;
        self.emit("Raw", "ActionCompleted", || {
            json!({"duration": started.0.elapsed().as_secs_f64() * 1000.0, "queuedAt": run.queued_at,
                "startedAt": started.1, "completedAt": globals::iso_now(), "success": true,
                "excludeFromHistory": self.action(&run.action, |a| a.exclude_from_history).unwrap_or(false),
                "id": run.id, "actionId": run.action, "name": run.name, "arguments": ctx.args})
        });
        if self.action(&run.action, |a| a.exclude_from_history).unwrap_or(false) {
            return;
        }
        let mut st = self.st.borrow_mut();
        if st.history.len() == HISTORY {
            st.history.pop_front();
        }
        st.history.push_back(Record {
            action_id: run.action,
            name: run.name,
            args: start,
            after: ctx.args,
            queued_at: run.queued_at,
            ms: started.0.elapsed().as_secs_f64() * 1000.0,
        });
        drop(st);
        crate::ui::changed();
    }

    /// Run a sub-action list: in order, or one of the enabled ones at random (by weight).
    fn list<'a>(self: &'a Rc<Self>, ctx: &'a mut Ctx, subs: &'a [Value], random: bool) -> Fut<'a, Flow> {
        Box::pin(async move {
            let on: Vec<&Obj> = subs
                .iter()
                .filter_map(Value::as_object)
                .filter(|o| b(o, "enabled", true))
                .collect();
            if random {
                let total: f64 = on.iter().map(|o| n(o, "weight").max(0.0)).sum();
                let pick = if total > 0.0 {
                    let mut r = crate::math::random() * total;
                    on.iter().position(|o| {
                        r -= n(o, "weight").max(0.0);
                        r < 0.0
                    })
                } else if on.is_empty() {
                    None
                } else {
                    Some((crate::math::random() * on.len() as f64) as usize % on.len())
                };
                return match pick.and_then(|i| on.get(i)) {
                    Some(o) => self.one(ctx, o).await,
                    None => Flow::Continue,
                };
            }
            for o in on {
                let f = self.one(ctx, o).await;
                if f != Flow::Continue {
                    return f;
                }
            }
            Flow::Continue
        })
    }

    /// A container's children (If/Else's true/false branches, Switch cases, groups).
    fn branch<'a>(self: &'a Rc<Self>, ctx: &'a mut Ctx, o: &'a Obj) -> Fut<'a, Flow> {
        self.list(ctx, subs(o), b(o, "random", false))
    }

    fn one<'a>(self: &'a Rc<Self>, ctx: &'a mut Ctx, o: &'a Obj) -> Fut<'a, Flow> {
        Box::pin(async move {
            let kind = o.get("type").and_then(Value::as_u64).unwrap_or(0);
            // Containers (If/Else branches, Switch cases, groups) aren't sub-actions of their own.
            if !(99900..99999).contains(&kind) && self.listening("Raw", "SubAction") {
                let what = crate::types::SUB_ACTION_INFO
                    .binary_search_by_key(&(kind as u32), |x| x.0)
                    .map_or("", |i| crate::types::SUB_ACTION_INFO[i].1);
                let a = &ctx.args;
                let data = json!({"parentId": a.get("actionId"), "parentName": a.get("actionName"), "type": kind,
                    "id": a.get("runningActionId"), "actionId": o.get("id"), "name": what, "arguments": a});
                self.emit("Raw", "SubAction", || data);
            }
            match kind {
                // Run Action
                4 => {
                    let id = self.simple(ctx, s(o, "actionId"));
                    if b(o, "runImmedately", true) {
                        if let Some((subs, random)) = self.action(&id, |a| (a.sub_actions.clone(), a.random_action)) {
                            let subs: Vec<Value> = subs.into_iter().map(Value::Object).collect();
                            if self.list(ctx, &subs, random).await == Flow::BreakToRoot {
                                return Flow::BreakToRoot;
                            }
                        }
                    } else {
                        self.queue(&id, ctx.args.clone());
                    }
                }
                2 | 6 | 8 | 9 | 10 | 23 | 500 | 501 | 560 | 1007 | 1010 | 1013 | 1014 | 1015 | 10001 => {
                    self.rest(ctx, o, kind).await
                }
                // Set Argument
                123 => {
                    let k = self.simple(ctx, s(o, "variableName"));
                    let v = self.expand(ctx, s(o, "value"));
                    let v = if b(o, "autoType", false) {
                        auto_type(&v)
                    } else {
                        v.into()
                    };
                    ctx.args.insert(k, v);
                }
                // If/Else
                120 => {
                    let ok = self.condition(ctx, o);
                    let want = if ok { 99901 } else { 99902 };
                    let branch = subs(o)
                        .iter()
                        .filter_map(Value::as_object)
                        .find(|c| c.get("type").and_then(Value::as_u64) == Some(want));
                    if let Some(c) = branch {
                        return self.branch(ctx, c).await;
                    }
                }
                // Global (Get) / (Set)
                121 => self.global_get(ctx, o),
                122 => self.global_set(ctx, o),
                124 => return Flow::BreakToRoot,
                // Clear Non-Persisted Globals / User Globals
                125 => self.st.borrow_mut().temp.clear(),
                126 => self.st.borrow_mut().user_temp.clear(),
                // Switch
                127 => {
                    let input = self.expand(ctx, s(o, "input"));
                    let auto = b(o, "autoType", false);
                    let mut default = None;
                    for c in subs(o).iter().filter_map(Value::as_object) {
                        match c.get("type").and_then(Value::as_u64) {
                            Some(99903) => {
                                let cs = b(c, "caseSensitive", false);
                                let hit = c.get("values").and_then(Value::as_array).is_some_and(|vs| {
                                    vs.iter().any(|v| {
                                        let v = self.expand(ctx, &text(v));
                                        if auto && compare(&input, &v, true).is_eq() && auto_type(&v).is_number() {
                                            return true;
                                        }
                                        if cs {
                                            v == input
                                        } else {
                                            v.to_lowercase() == input.to_lowercase()
                                        }
                                    })
                                });
                                if hit {
                                    return self.branch(ctx, c).await;
                                }
                            }
                            Some(99904) => default = Some(c),
                            _ => {}
                        }
                    }
                    if let Some(c) = default {
                        return self.branch(ctx, c).await;
                    }
                }
                // While
                128 => loop {
                    if !self.while_condition(ctx, o) {
                        break;
                    }
                    match self.list(ctx, subs(o), false).await {
                        Flow::BreakToRoot => return Flow::BreakToRoot,
                        Flow::LoopBreak => break,
                        _ => {}
                    }
                    // Let the rest run, even in an endless loop.
                    tokio::task::yield_now().await;
                },
                129 => return Flow::LoopBreak,
                130 => return Flow::LoopContinue,
                // Set Action Queue Pause State: 0 pause, 1 resume; no queue: all of them.
                1001 => {
                    let q = self.simple(ctx, s(o, "actionQueueId"));
                    self.pause(&q, n(o, "state") as i64 == 0, b(o, "clear", false));
                }
                // Delay, in ms; Random: between value and maxValue.
                1002 => {
                    let (lo, hi) = (n(o, "value").max(0.0), n(o, "maxValue").max(0.0));
                    let ms = if b(o, "random", false) && hi > lo {
                        lo + crate::math::random() * (hi - lo)
                    } else {
                        lo
                    };
                    tokio::time::sleep(Duration::from_millis(ms as u64)).await;
                }
                // Get Random Number: 0 between min and max (inclusive), 1 a float.
                1003 => {
                    if n(o, "randomType") as i64 == 1 {
                        let f = crate::math::random();
                        ctx.args.insert("randomFloat".into(), vars::number(f));
                        ctx.args.insert("randomPercent".into(), ((f * 100.0) as i64).into());
                    } else {
                        let (lo, hi) = (n(o, "min") as i64, n(o, "max") as i64);
                        let (lo, hi) = (lo.min(hi), lo.max(hi));
                        let r = lo + (crate::math::random() * (hi - lo + 1) as f64) as i64;
                        ctx.args.insert("randomNumber".into(), r.min(hi).into());
                    }
                }
                // Set Action State
                1004 => {
                    let id = self.simple(ctx, s(o, "actionId"));
                    let st = n(o, "state");
                    self.change(|c| {
                        if let Some(a) = c.actions.actions.iter_mut().find(|a| a.id == id) {
                            a.enabled = new_state(st, a.enabled);
                        }
                    });
                }
                // Clear Action Queue
                1005 => {
                    self.clear_queue(&self.simple(ctx, s(o, "actionQueueId")));
                }
                // Comment
                1009 => {}
                // Set Action Group State
                1011 => {
                    let g = self.simple(ctx, s(o, "groupName"));
                    let st = n(o, "state");
                    self.change(|c| {
                        for a in c.actions.actions.iter_mut().filter(|a| a.group == g) {
                            a.enabled = new_state(st, a.enabled);
                        }
                    });
                }
                // Get Action State
                1016 => {
                    let id = self.simple(ctx, s(o, "actionId"));
                    let on = self.action(&id, |a| a.enabled).unwrap_or(false);
                    ctx.args.insert("actionState".into(), on.into());
                }
                // Get Action Group State: 0 all disabled, 1 all enabled, 2 mixed.
                1017 => {
                    let g = self.simple(ctx, s(o, "groupName"));
                    let (on, off): (Vec<_>, Vec<_>) = {
                        let c = self.cfg.borrow();
                        c.actions
                            .actions
                            .iter()
                            .filter(|a| a.group == g)
                            .map(|a| (a.id.clone(), a.enabled))
                            .partition(|(_, e)| *e)
                    };
                    let state = match (on.is_empty(), off.is_empty()) {
                        (false, true) => 1,
                        (true, _) => 0,
                        _ => 2,
                    };
                    let ids = |v: Vec<(String, bool)>| Value::from(v.into_iter().map(|(i, _)| i).collect::<Vec<_>>());
                    ctx.args.insert("actionGroupState".into(), state.into());
                    ctx.args.insert("actionsEnabled".into(), ids(on));
                    ctx.args.insert("actionsDisabled".into(), ids(off));
                }
                // Log
                1020 => crate::say(self.expand(ctx, s(o, "message"))),
                // Set Action Queue Blocking State: 0 blocking, 1 not.
                1022 => {
                    let q = self.simple(ctx, s(o, "actionQueueId"));
                    self.set_blocking(&q, n(o, "state") as i64 == 0);
                }
                // Get Input: a dialog in the window.
                1023 => {
                    let rx = crate::ui::input(
                        self.expand(ctx, s(o, "title")),
                        self.expand(ctx, s(o, "prompt")),
                        self.expand(ctx, s(o, "originalText")),
                    );
                    let mut var = self.simple(ctx, s(o, "outputVariable"));
                    if var.is_empty() {
                        var = "inputResult".into();
                    }
                    match rx.await.ok().flatten() {
                        Some(v) => {
                            let v = if b(o, "autoTypeOutput", false) {
                                auto_type(&v)
                            } else {
                                v.into()
                            };
                            ctx.args.insert(var, v);
                            ctx.args.insert("success".into(), true.into());
                        }
                        None => {
                            ctx.args.insert("success".into(), false.into());
                        }
                    }
                }
                // Set Sub-Action State / Weight
                1028 | 1038 => {
                    let id = self.simple(ctx, s(o, "actionId"));
                    let sub = self.simple(ctx, s(o, "subActionId"));
                    let (st, w) = (n(o, "state"), n(o, "weight"));
                    self.change(|c| {
                        if let Some(a) = c.actions.actions.iter_mut().find(|a| a.id == id) {
                            with_sub(&mut a.sub_actions, &sub, &mut |x| {
                                if kind == 1028 {
                                    let on = b(x, "enabled", true);
                                    x.insert("enabled".into(), new_state(st, on).into());
                                } else {
                                    x.insert("weight".into(), json!(w));
                                }
                            });
                        }
                    });
                }
                // Send Local Signal
                1031 => {
                    let name = self.simple(ctx, s(o, "signalName"));
                    let mut payload = if b(o, "useArge", false) {
                        ctx.args.clone()
                    } else {
                        Obj::new()
                    };
                    if let Some(custom) = o.get("customArgs").and_then(Value::as_object) {
                        for (k, v) in custom {
                            payload.insert(k.clone(), auto_type(&self.expand(ctx, &text(v))));
                        }
                    }
                    let mut st = self.st.borrow_mut();
                    let waiting = st.waiters.remove(&name).unwrap_or_default();
                    let mut delivered = false;
                    for w in waiting {
                        delivered |= w.send(payload.clone()).is_ok();
                    }
                    if !delivered && b(o, "queueSignal", false) {
                        st.signals.entry(name).or_default().push_back(payload);
                    }
                }
                // Wait For Local Signal: up to `timeout` ms (blank or over 30 s: 30 s).
                1032 => {
                    let name = self.simple(ctx, s(o, "signalName"));
                    let ms = match n(o, "timeout") as u64 {
                        0 => 30_000,
                        t => t.min(30_000),
                    };
                    let queued = self
                        .st
                        .borrow_mut()
                        .signals
                        .get_mut(&name)
                        .and_then(VecDeque::pop_front);
                    let got = match queued {
                        Some(p) => Some(p),
                        None => {
                            let (tx, rx) = oneshot::channel();
                            self.st.borrow_mut().waiters.entry(name.clone()).or_default().push(tx);
                            tokio::time::timeout(Duration::from_millis(ms), rx)
                                .await
                                .ok()
                                .and_then(Result::ok)
                        }
                    };
                    ctx.args.insert("signalName".into(), name.as_str().into());
                    match got {
                        Some(p) => {
                            let over = b(o, "overwrite", false);
                            for (k, v) in p {
                                if over || !ctx.args.contains_key(&k) {
                                    ctx.args.insert(k, v);
                                }
                            }
                            ctx.args.insert("signalTimeout".into(), false.into());
                            ctx.args.insert("hasResult".into(), true.into());
                        }
                        None => {
                            crate::say(format!("Wait For Local Signal: {name} timed out"));
                            ctx.args.insert("signalTimeout".into(), true.into());
                        }
                    }
                }
                // Set Trigger State
                1033 => {
                    let id = self.simple(ctx, s(o, "actionId"));
                    let trig = self.simple(ctx, s(o, "triggerId"));
                    let st = n(o, "state");
                    self.change(|c| {
                        let a = c.actions.actions.iter_mut().find(|a| a.id == id);
                        if let Some(t) = a.and_then(|a| a.triggers.iter_mut().find(|t| s(t, "id") == trig)) {
                            let on = b(t, "enabled", true);
                            t.insert("enabled".into(), new_state(st, on).into());
                        }
                    });
                }
                // Custom Event Trigger: fires the Custom trigger with this event name.
                1034 => {
                    let ev = self.expand(ctx, s(o, "eventName"));
                    let args = if b(o, "useArgs", false) {
                        ctx.args.clone()
                    } else {
                        Obj::new()
                    };
                    self.fire(T_CUSTOM, "Custom", "Custom", "custom", &args, |t| {
                        let want = s(t, "eventName");
                        want.is_empty() || want == ev
                    });
                }
                // Group
                99900 => return self.branch(ctx, o).await,
                _ => self.unsupported(kind),
            }
            Flow::Continue
        })
    }

    /// Log once per type that a sub-action can't run here (yet), and skip it.
    fn unsupported(&self, kind: u64) {
        if self.st.borrow_mut().warned.insert(kind) {
            let what = name(SUB_ACTIONS, kind as u32).unwrap_or("unknown");
            let why = if kind >= 99998 {
                "C# isn't supported"
            } else {
                "not supported yet"
            };
            crate::say(format!("sub-action {what} (type {kind}): {why}, skipped"));
        }
    }

    fn all_queues(&self) -> Vec<String> {
        let c = self.cfg.borrow();
        std::iter::once(DEFAULT_QUEUE.to_string())
            .chain(c.actions.queues.iter().map(|q| s(q, "id").to_string()))
            .collect()
    }

    /// If/Else: Equals, Not Equals, Contains (any case), Regex Match, Less Than, Greater Than, Does
    /// Not Exist, Equals / Not Equals (Ignore Case), Is Null or Empty.
    fn condition(&self, ctx: &mut Ctx, o: &Obj) -> bool {
        let raw = s(o, "input");
        let op = n(o, "operation") as i64;
        if op == 6 {
            return !ctx.args.contains_key(raw.trim_matches('%'));
        }
        let input = self.expand(ctx, raw);
        let value = self.expand(ctx, s(o, "value"));
        let auto = b(o, "autoType", false);
        match op {
            0 => compare(&input, &value, auto).is_eq(),
            1 => !compare(&input, &value, auto).is_eq(),
            2 => input.to_lowercase().contains(&value.to_lowercase()),
            3 => regex(&value).is_some_and(|r| match_args(&r, &input, &mut ctx.args)),
            4 => compare(&input, &value, auto).is_lt(),
            5 => compare(&input, &value, auto).is_gt(),
            7 => input.to_lowercase() == value.to_lowercase(),
            8 => input.to_lowercase() != value.to_lowercase(),
            9 => input.is_empty(),
            _ => false,
        }
    }

    /// While's own operators: Equals, Not Equals, Less Than, Greater Than.
    fn while_condition(&self, ctx: &Ctx, o: &Obj) -> bool {
        let mut input = self.expand(ctx, s(o, "input"));
        let mut value = self.expand(ctx, s(o, "value"));
        if b(o, "ignoreCase", false) {
            (input, value) = (input.to_lowercase(), value.to_lowercase());
        }
        let c = compare(&input, &value, b(o, "autoType", false));
        match n(o, "operation") as i64 {
            0 => c.is_eq(),
            1 => !c.is_eq(),
            2 => c.is_lt(),
            3 => c.is_gt(),
            _ => false,
        }
    }

    /// Past runs, oldest first.
    pub fn history(&self) -> std::cell::Ref<'_, VecDeque<Record>> {
        std::cell::Ref::map(self.st.borrow(), |s| &s.history)
    }

    pub fn clear_history(&self) {
        self.st.borrow_mut().history.clear();
    }

    /// What's running and waiting, per queue: (queue id, action name, running), leaving out
    /// actions set to Exclude from Pending.
    pub fn pending(&self) -> Vec<(String, String, bool)> {
        let c = self.cfg.borrow();
        let shown = |id: &str| !c.actions.actions.iter().any(|a| a.id == id && a.exclude_from_pending);
        let st = self.st.borrow();
        let mut out = Vec::new();
        for (q, x) in &st.queues {
            // ponytail: running runs only keep their name; Exclude from Pending hides waiting ones.
            out.extend(x.running.iter().map(|(_, nm)| (q.clone(), nm.clone(), true)));
            out.extend(
                x.waiting
                    .iter()
                    .filter(|r| shown(&r.action))
                    .map(|r| (q.clone(), r.name.clone(), false)),
            );
        }
        out
    }

    /// The paused queues' ids.
    pub fn paused(&self) -> Vec<String> {
        let st = self.st.borrow();
        st.queues
            .iter()
            .filter(|(_, q)| q.paused)
            .map(|(id, _)| id.clone())
            .collect()
    }

    /// Pause or resume a queue (all of them for ""), clearing its waiting runs if `clear`.
    pub fn pause(self: &Rc<Self>, queue: &str, pause: bool, clear: bool) {
        let ids = if queue.is_empty() {
            self.all_queues()
        } else {
            vec![queue.to_string()]
        };
        for q in ids {
            {
                let mut st = self.st.borrow_mut();
                let e = st.queues.entry(q.clone()).or_default();
                e.paused = pause;
                if clear {
                    e.waiting.clear();
                }
            }
            self.pump(&q);
        }
        crate::ui::changed();
    }

    /// Drop a queue's waiting runs.
    pub fn clear_queue(&self, queue: &str) {
        if let Some(q) = self.st.borrow_mut().queues.get_mut(queue) {
            q.waiting.clear();
        }
        crate::ui::changed();
    }

    /// Set a queue's Blocking (and start what it can run now).
    pub fn set_blocking(self: &Rc<Self>, queue: &str, blocking: bool) {
        self.change(|c| {
            if queue == DEFAULT_QUEUE {
                c.actions.blocking = blocking;
            } else if let Some(x) = c.actions.queues.iter_mut().find(|x| s(x, "id") == queue) {
                x.insert("blocking".into(), blocking.into());
            }
        });
        self.pump(queue);
    }
}

/// Find a sub-action by id anywhere in a tree.
fn with_sub(list: &mut [Obj], id: &str, f: &mut dyn FnMut(&mut Obj)) {
    for x in list {
        if s(x, "id") == id {
            f(x);
            return;
        }
        if let Some(children) = x.get_mut("subActions").and_then(Value::as_array_mut) {
            let mut objs: Vec<Obj> = children.iter().filter_map(|c| c.as_object().cloned()).collect();
            with_sub(&mut objs, id, f);
            *children = objs.into_iter().map(Value::Object).collect();
        }
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::model::ActionsFile;

    pub(crate) fn engine(actions: Value) -> Rc<Engine> {
        let c = Config {
            actions: serde_json::from_value::<ActionsFile>(actions).unwrap(),
            ..Default::default()
        };
        Engine::new(Arc::new(watch::channel(c).0), |_| {})
    }

    pub(crate) fn action(id: &str, name: &str, subs: Value) -> Value {
        json!({"id": id, "name": name, "enabled": true, "queue": DEFAULT_QUEUE, "subActions": subs, "triggers": []})
    }

    pub(crate) fn log(msg: &str) -> Value {
        json!({"type": 1020, "logLevel": 1, "message": msg})
    }

    /// Run a test body on a LocalSet, then return the log lines that start with "T ".
    pub(crate) fn run(f: impl Future<Output = ()>) -> Vec<String> {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        tokio::task::LocalSet::new().block_on(&rt, f);
        let mut out = Vec::new();
        crate::status(|s| {
            out = s.log.iter().filter(|x| x.starts_with("T ")).cloned().collect();
            s.log.clear();
        });
        out
    }

    async fn settle() {
        tokio::time::sleep(Duration::from_millis(50)).await;
    }

    #[test]
    fn logic_and_flow() {
        let _g = crate::TEST_LOCK.lock().unwrap();
        let ifelse = |op: i64, input: &str, value: &str, yes: &str, no: &str| {
            json!({"type": 120, "input": input, "operation": op, "value": value, "autoType": true, "subActions": [
                {"type": 99901, "random": false, "subActions": [log(yes)]},
                {"type": 99902, "random": false, "subActions": [log(no)]}]})
        };
        let main = action(
            "a1",
            "main",
            json!([
                {"type": 123, "variableName": "n", "value": "15", "autoType": true},
                {"type": 123, "variableName": "s", "value": "Hello World", "autoType": false},
                ifelse(4, "%n%", "2", "T lt yes", "T lt no"),
                ifelse(2, "%s%", "world", "T contains yes", "T contains no"),
                {"type": 120, "input": "%s%", "operation": 3, "value": "(?<first>\\w+) (\\w+)", "autoType": true, "subActions": [
                    {"type": 99901, "random": false, "subActions": [log("T re %match.count%|%match[0]%|%match[1]%|%match[2]%|%first%")]},
                    {"type": 99902, "random": false, "subActions": []}]},
                ifelse(6, "nothere", "", "T dne yes", "T dne no"),
                {"type": 123, "variableName": "i", "value": "0", "autoType": true},
                {"type": 128, "input": "%i%", "operation": 2, "value": "5", "autoType": true, "subActions": [
                    {"type": 123, "variableName": "i", "value": "$math(%i% + 1)$", "autoType": true},
                    {"type": 120, "input": "%i%", "operation": 0, "value": "2", "autoType": true, "subActions": [
                        {"type": 99901, "subActions": [{"type": 130}]}, {"type": 99902, "subActions": []}]},
                    {"type": 120, "input": "%i%", "operation": 0, "value": "4", "autoType": true, "subActions": [
                        {"type": 99901, "subActions": [{"type": 129}]}, {"type": 99902, "subActions": []}]},
                    log("T loop %i%")]},
                {"type": 127, "input": "%i%", "autoType": true, "subActions": [
                    {"type": 99903, "values": ["1", "4"], "caseSensitive": false, "subActions": [log("T case 1or4")]},
                    {"type": 99904, "subActions": [log("T default")]}]},
                {"type": 4, "actionId": "a2", "runImmedately": true},
                log("T parent sees %c% (not reached: the child breaks)")
            ]),
        );
        let child = action(
            "a2",
            "child",
            json!([
                log("T child sees %n%"),
                {"type": 123, "variableName": "c", "value": "fromchild"},
                {"type": 124},
                log("T after break")
            ]),
        );
        let e = engine(json!({"actions": [main, child]}));
        let out = run(async move {
            e.queue("main", Obj::new()).unwrap();
            settle().await;
            assert_eq!(e.history().len(), 1);
        });
        assert_eq!(
            out,
            [
                "T lt no",
                "T contains yes",
                "T re 3|Hello World|World|Hello|Hello",
                "T dne yes",
                "T loop 1",
                "T loop 3",
                "T case 1or4",
                "T child sees 15"
            ]
        );
    }

    #[test]
    fn pause_clear_pending_history() {
        let _g = crate::TEST_LOCK.lock().unwrap();
        let mut hidden = action("h", "hidden", json!([log("T hidden")]));
        hidden["excludeFromHistory"] = json!(true);
        let quick = action("k", "quick", json!([log("T quick")]));
        let e = engine(json!({"blocking": true, "actions": [hidden, quick]}));
        let out = run(async move {
            let waiting = |name: &str| vec![(DEFAULT_QUEUE.to_string(), name.to_string(), false)];
            e.pause(DEFAULT_QUEUE, true, false);
            e.queue("k", Obj::new());
            assert_eq!(e.pending(), waiting("quick"));
            assert_eq!(e.paused(), [DEFAULT_QUEUE]);
            e.clear_queue(DEFAULT_QUEUE);
            assert!(e.pending().is_empty());
            e.queue("h", Obj::new());
            e.queue("k", Obj::new());
            e.pause("", false, false);
            tokio::time::sleep(Duration::from_millis(50)).await;
            assert!(e.pending().is_empty() && e.paused().is_empty());
            let h = e.history();
            assert_eq!(h.iter().map(|r| r.name.as_str()).collect::<Vec<_>>(), ["quick"]);
            assert_eq!(h[0].after["actionName"], "quick");
        });
        assert_eq!(out, ["T hidden", "T quick"]);
    }

    #[test]
    fn queues_globals_signals_triggers() {
        let _g = crate::TEST_LOCK.lock().unwrap();
        let q = "11111111-0000-0000-0000-000000000000";
        let slow = json!({"id": "q1", "name": "slow", "enabled": true, "queue": q, "triggers": [], "subActions": [
            log("T start %n%"), {"type": 1002, "value": 30}, log("T end %n%")]});
        let driver = action(
            "d",
            "driver",
            json!([
                {"type": 123, "variableName": "n", "value": "a"},
                {"type": 4, "actionId": "q1", "runImmedately": false},
                {"type": 123, "variableName": "n", "value": "b"},
                {"type": 4, "actionId": "q1", "runImmedately": false},
                {"type": 122, "destination": 0, "variableName": "g", "persisted": true, "source": 0, "value": "5", "autoType": true},
                {"type": 122, "destination": 0, "variableName": "g", "persisted": true, "source": 1, "value": "3", "autoType": true},
                {"type": 122, "destination": 0, "variableName": "g", "persisted": true, "source": 2, "value": "x", "autoType": true},
                {"type": 121, "source": 0, "variableName": "g", "persisted": true, "destinationVariable": "gv"},
                {"type": 121, "source": 0, "variableName": "missing", "persisted": false, "destinationVariable": "m", "defaultValue": "dflt"},
                log("T globals %gv% ~g~ %m%"),
                {"type": 1031, "signalName": "sig", "useArge": false, "customArgs": {"k": "v"}, "queueSignal": true},
                {"type": 1034, "eventName": "ev", "useArgs": true},
                {"type": 1004, "actionId": "q1", "state": 2},
                {"type": 1016, "actionId": "q1"},
                log("T state %actionState%")
            ]),
        );
        let waiter = action(
            "w",
            "waiter",
            json!([
                {"type": 1032, "signalName": "sig", "overwrite": false, "timeout": 200},
                log("T got %k% %signalTimeout%"),
                {"type": 1032, "signalName": "none", "timeout": 20},
                log("T timeout %signalTimeout%")
            ]),
        );
        let mut custom = action("c", "custom", json!([log("T custom %n% %triggerName% %__source%")]));
        custom["triggers"] = json!([{"eventName": "ev", "id": "t1", "type": 18001, "enabled": true, "exclusions": []}]);
        let mut created = action("gc", "created", json!([log("T created %global.name%=%global.value%")]));
        created["triggers"] = json!([{"variableName": "", "persisted": 0, "id": "t2", "type": 711, "enabled": true}]);
        let e = engine(
            json!({"blocking": false, "queues": [{"id": q, "name": "Q", "blocking": true}],
            "actions": [slow, driver, waiter, custom, created]}),
        );
        let e2 = e.clone();
        let out = run(async move {
            e.queue("d", Obj::new()).unwrap();
            settle().await;
            e.queue("w", Obj::new()).unwrap();
            tokio::time::sleep(Duration::from_millis(200)).await;
            assert_eq!(e.cfg.borrow().globals["g"], json!(8));
        });
        let _ = e2;
        let pos = |s: &str| {
            out.iter()
                .position(|x| x == s)
                .unwrap_or_else(|| panic!("{s} missing: {out:?}"))
        };
        // Blocking queue: one after the other.
        assert!(pos("T start a") < pos("T end a") && pos("T end a") < pos("T start b"));
        assert!(out.contains(&"T globals 8 8 dflt".to_string()));
        assert!(out.contains(&"T created g=5".to_string()) && out.contains(&"T created missing=dflt".to_string()));
        assert!(out.contains(&"T custom b Custom CustomEvent".to_string()));
        assert!(out.contains(&"T state False".to_string()));
        assert!(out.contains(&"T got v False".to_string()));
        assert!(out.contains(&"T timeout True".to_string()));
    }

    #[test]
    fn uuids() {
        let u = uuid();
        assert_eq!(u.len(), 36);
        assert_eq!(&u[14..15], "4");
        assert_ne!(uuid(), u);
    }
}
