//! The Polls page: polls on Twitch and YouTube, and Twitch's predictions, started, ended and followed
//! from one model. They run through the same sub-actions an action uses (Create Poll, Lock Prediction...),
//! so the page and the actions behave alike. Kick and TikTok have no API for either.
//!
//! What the platforms said last lives here, read without waiting: a read answers from it and asks the
//! platforms again when something changed or it has been a while. The answer arrives as a change.

use super::pages::{button, button_if, confirm, danger};
use super::view::column;
use crate::{
    automation::schema::{F, W},
    engine::Engine,
    i18n::{tr, trf},
    model::Obj,
};
use serde_json::{json, Value};
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
    time::{Duration, Instant},
};

pub const DOMAIN: &str = "polls";

pub fn supports(domain: &str) -> bool {
    domain == DOMAIN
}

/// How long polls and predictions may run: the choices' seconds.
const LASTS: [u64; 6] = [60, 120, 180, 300, 600, 1800];
const LAST_NAMES: &[&str] = &["1 min", "2 min", "3 min", "5 min", "10 min", "30 min"];

const POLL_FIELDS: &[F] = &[
    ("question", "Question", W::Text),
    ("choices", "Choices", W::Lines),
    ("twitch", "Twitch", W::Bool),
    ("youtube", "YouTube", W::Bool),
    ("duration", "Lasts", W::Pick(LAST_NAMES)),
];
const PREDICTION_FIELDS: &[F] = &[
    ("question", "Question", W::Text),
    ("outcomes", "Outcomes", W::Lines),
    ("duration", "Voting time", W::Pick(LAST_NAMES)),
];

/// What the platforms said last: Twitch's poll and prediction, YouTube's poll (their variables), and
/// how the last thing asked went.
#[derive(Default, Clone)]
struct Seen {
    twitch_poll: Obj,
    prediction: Obj,
    youtube_poll: Obj,
    note: String,
    /// Something asked last didn't go through (the note says what).
    trouble: bool,
    ready: [bool; 2],
    /// Logged in to Twitch but reading someone else's channel: the account and the channel. Polls and
    /// predictions need the owner's login, so Twitch isn't ready.
    guest: Option<(String, String)>,
    prediction_known: bool,
    requested: u64,
    received: u64,
    busy: bool,
}

impl Seen {
    fn begin(&mut self, mutation: bool) -> Option<u64> {
        if self.busy {
            return None;
        }
        self.requested = self.requested.wrapping_add(1).max(1);
        self.busy = mutation;
        Some(self.requested)
    }
    fn finish(&mut self, sequence: u64) -> bool {
        if sequence != self.requested {
            return false;
        }
        self.received = sequence;
        self.busy = false;
        true
    }
    fn twitch_poll_active(&self) -> bool {
        self.twitch_poll.get("active").and_then(Value::as_bool) == Some(true)
    }
    fn youtube_poll_active(&self) -> bool {
        !s(&self.youtube_poll, "poll.question").is_empty()
    }
    /// The prediction's outcomes, in order.
    fn outcomes(&self) -> Vec<String> {
        (0..10)
            .map(|i| s(&self.prediction, &format!("prediction.outcome{i}.title")).to_string())
            .take_while(|title| !title.is_empty())
            .collect()
    }
}

thread_local! {
    static SEEN: RefCell<Seen> = RefCell::default();
    static ASKED: Cell<Option<Instant>> = const { Cell::new(None) };
}

fn seen() -> Seen {
    SEEN.with(|seen| seen.borrow().clone())
}

fn s<'a>(o: &'a Obj, k: &str) -> &'a str {
    o.get(k).and_then(Value::as_str).unwrap_or("")
}

fn num(o: &Obj, k: &str) -> String {
    o.get(k).map_or_else(String::new, |v| match v {
        Value::String(t) => t.clone(),
        v => v.to_string(),
    })
}

fn token_ready(access: &str, refresh: &str, expires_at: u64) -> bool {
    !access.is_empty() && (!crate::oauth::stale(expires_at) || !refresh.is_empty())
}

/// Why Twitch can't run polls and predictions now.
fn twitch_why(seen: &Seen) -> String {
    match &seen.guest {
        Some((_, channel)) => trf("polls.guest_short", &[("channel", channel)]),
        None => tr("polls.twitch_unavailable").to_string(),
    }
}

/// The page's status line: its state and its words.
fn status_line(seen: &Seen) -> (&'static str, String) {
    let ready: Vec<&str> = ["Twitch", "YouTube"]
        .into_iter()
        .zip(seen.ready)
        .filter_map(|(name, ready)| ready.then_some(name))
        .collect();
    if seen.busy {
        ("busy", tr("polls.working").into())
    } else if seen.received == 0 {
        ("busy", tr("polls.checking").into())
    } else if !seen.note.is_empty() {
        (if seen.trouble { "down" } else { "up" }, seen.note.clone())
    } else if let Some((account, channel)) = &seen.guest {
        let guest = trf("polls.guest", &[("account", account), ("channel", channel)]);
        if ready.is_empty() {
            ("off", guest)
        } else {
            (
                "up",
                format!("{} {guest}", trf("polls.ready_on", &[("where", &ready.join(", "))])),
            )
        }
    } else if ready.is_empty() {
        ("off", tr("polls.accounts_needed").into())
    } else {
        ("up", trf("polls.ready_on", &[("where", &ready.join(", "))]))
    }
}

/// YouTube can end only polls created by its runner. Public chat state cannot prove ownership, so
/// keep its authenticated End action available and report the request outcome without guessing.
fn action_availability(
    ready: [bool; 2],
    twitch_poll: bool,
    youtube_poll: bool,
    prediction: &str,
    prediction_known: bool,
) -> ([bool; 2], [bool; 4]) {
    let active = matches!(prediction, "ACTIVE" | "LOCKED");
    let prediction_ready = ready[0] && prediction_known;
    (
        [
            ready.iter().any(|ready| *ready),
            (ready[0] && twitch_poll) || (ready[1] && youtube_poll),
        ],
        [
            prediction_ready && !active,
            prediction_ready && prediction == "ACTIVE",
            prediction_ready && active,
            prediction_ready && active,
        ],
    )
}

/// What each button can do now: new poll, end poll; new, lock, resolve, cancel prediction.
fn availability(seen: &Seen) -> ([bool; 2], [bool; 4]) {
    let (mut poll, mut prediction) = action_availability(
        seen.ready,
        seen.twitch_poll_active(),
        seen.youtube_poll_active(),
        s(&seen.prediction, "prediction.Status"),
        seen.prediction_known,
    );
    if seen.busy {
        poll = [false; 2];
        prediction = [false; 4];
    }
    (poll, prediction)
}

/// Get Active Prediction excludes locked predictions, but this page must keep their outcomes
/// available for resolution. Use the same actionable statuses as the lock/resolve sub-actions.
fn actionable_prediction(response: &Value) -> Option<&Value> {
    response.get("data")?.as_array()?.iter().find(|prediction| {
        matches!(
            prediction.get("status").and_then(Value::as_str),
            Some("ACTIVE" | "LOCKED")
        )
    })
}

fn operation_result(kind: u64, response: &Obj) -> Option<bool> {
    let key = match kind {
        569 => "cancelled",
        570 => "locked",
        _ => "success",
    };
    let result = response.get(key).and_then(Value::as_bool);
    if matches!(kind, 569 | 570) {
        Some(result.unwrap_or(false))
    } else {
        result
    }
}

/// A prediction's status in words.
fn status_word(status: &str) -> &'static str {
    match status {
        "ACTIVE" => tr("polls.status_open"),
        "LOCKED" => tr("polls.status_locked"),
        "RESOLVED" => tr("polls.status_resolved"),
        "CANCELED" => tr("polls.status_canceled"),
        _ => "",
    }
}

type Step = (u64, Value, String);

/// Runs `steps` (sub-action code and settings, and what to say about it), then reads the current poll
/// and prediction again; the model shows it all when it comes. False when a request is already running.
fn ask(engine: &Rc<Engine>, steps: Vec<Step>) -> bool {
    let mutation = !steps.is_empty();
    let Some(sequence) = SEEN.with(|seen| seen.borrow_mut().begin(mutation)) else {
        return false;
    };
    if mutation {
        crate::host::changed();
    }
    let e = engine.clone();
    tokio::task::spawn_local(async move {
        let obj = |v: Value| v.as_object().cloned().unwrap_or_default();
        let mut notes = Vec::new();
        let mut trouble = false;
        let (ready, guest) = {
            let c = e.config();
            let guest = c
                .twitch
                .broadcaster
                .as_ref()
                .filter(|_| crate::twitch::other_channel(&c))
                .map(|a| (a.display_name.clone(), c.twitch.channel.trim().to_string()));
            let ready = [
                guest.is_none()
                    && e.twitch().is_some()
                    && c.twitch.broadcaster.as_ref().is_some_and(|a| {
                        token_ready(&a.token.access_token, &a.token.refresh_token, a.token.expires_at)
                    }),
                c.youtube.broadcaster.as_ref().is_some_and(|a| {
                    token_ready(&a.token.access_token, &a.token.refresh_token, a.token.expires_at)
                        && (!crate::oauth::stale(a.token.expires_at)
                            || (!c.youtube.client_id.is_empty() && !c.youtube.client_secret.is_empty()))
                }),
            ];
            (ready, guest)
        };
        for (kind, settings, what) in steps {
            // Twitch's sub-actions do nothing without Twitch (and don't say so).
            if (561..=572).contains(&kind) && !ready[0] {
                notes.push(match &guest {
                    Some((_, channel)) => format!("{what}: {}", trf("polls.guest_short", &[("channel", channel)])),
                    None => trf("polls.twitch_off", &[("what", &what)]),
                });
                trouble = true;
                continue;
            }
            if matches!(kind, 4011 | 4012) && !ready[1] {
                notes.push(tr("polls.youtube_unavailable").into());
                trouble = true;
                continue;
            }
            let r = e.run_one(kind, obj(settings)).await;
            let message = match operation_result(kind, &r) {
                Some(true) => "polls.done",
                Some(false) => {
                    trouble = true;
                    "polls.failed"
                }
                None => "polls.request_sent",
            };
            notes.push(trf(message, &[("what", &what)]));
        }
        let twitch_poll = if ready[0] {
            e.run_one(566, Obj::new()).await
        } else {
            Obj::new()
        };
        let mut prediction = Obj::new();
        let mut prediction_known = false;
        if let Some(twitch) = e.twitch().filter(|_| ready[0]) {
            match twitch
                .helix(
                    "GET",
                    "predictions",
                    vec![("broadcaster_id", "{broadcaster_id}")],
                    None,
                    false,
                    true,
                )
                .await
            {
                Ok(response) => {
                    prediction_known = true;
                    if let Some(current) = actionable_prediction(&response) {
                        crate::engine::prediction_args(current, &mut prediction);
                    }
                }
                Err(error) => {
                    trouble = true;
                    notes.push(trf(
                        "polls.lookup_failed",
                        &[
                            ("what", tr("polls.prediction")),
                            ("error", &crate::twitch::plain_error(&error)),
                        ],
                    ))
                }
            }
        }
        let applied = SEEN.with(|seen| {
            let mut seen = seen.borrow_mut();
            if !seen.finish(sequence) {
                return false;
            }
            seen.twitch_poll = twitch_poll;
            seen.prediction = prediction;
            seen.youtube_poll = crate::youtube::current_poll();
            seen.ready = ready;
            seen.guest = guest;
            seen.prediction_known = prediction_known;
            if !notes.is_empty() {
                seen.note = notes.join("  ");
                seen.trouble = trouble;
            }
            true
        });
        if applied {
            crate::host::changed();
        }
    });
    true
}

/// Asks the platforms again when something changed or it's been a while.
fn refresh_if_due(engine: &Rc<Engine>) {
    let dirty = crate::host::POLLS_DIRTY.swap(false, std::sync::atomic::Ordering::Relaxed);
    if dirty || ASKED.get().is_none_or(|at| at.elapsed() > Duration::from_secs(10)) {
        ASKED.set(Some(Instant::now()));
        ask(engine, Vec::new());
    }
}

fn custom(id: &str, label: &'static str, icon: &str, enabled: bool) -> Value {
    button_if(button(id, label, icon, false), enabled)
}

/// The two tables: what is running, and the buttons to start, end and follow it.
pub fn read(engine: &Rc<Engine>) -> Result<Value, String> {
    refresh_if_due(engine);
    Ok(json!({"domain": DOMAIN, "tables": model(&seen())}))
}

fn model(seen: &Seen) -> Vec<Value> {
    let mut poll_rows = Vec::new();
    let tp = &seen.twitch_poll;
    if seen.twitch_poll_active() {
        let n = tp.get("poll.choices.count").and_then(Value::as_u64).unwrap_or(0);
        for i in 0..n {
            poll_rows.push(
                json!({"id": format!("twitch|{i}"), "name": s(tp, "poll.Title"), "enabled": true,
                "cells": [{"brand": "twitch", "text": "Twitch"}, s(tp, "poll.Title"),
                    s(tp, &format!("poll.choice{i}.title")), num(tp, &format!("poll.choice{i}.totalVotes"))]}),
            );
        }
    }
    let yp = &seen.youtube_poll;
    if seen.youtube_poll_active() {
        let n = yp.get("poll.options.count").and_then(Value::as_u64).unwrap_or(0);
        for i in 0..n {
            poll_rows.push(json!({"id": format!("youtube|{i}"), "name": s(yp, "poll.question"), "enabled": true,
                "cells": [{"brand": "youtube", "text": "YouTube"}, s(yp, "poll.question"),
                    s(yp, &format!("poll.option{i}.text")),
                    format!("{} ({})", num(yp, &format!("poll.option{i}.votes")), s(yp, &format!("poll.option{i}.votePercentage")))]}));
        }
    }
    let p = &seen.prediction;
    let mut prediction_rows = Vec::new();
    if !s(p, "prediction.Id").is_empty() {
        for (i, title) in seen.outcomes().into_iter().enumerate() {
            prediction_rows.push(json!({"id": format!("prediction|{i}"), "name": s(p, "prediction.Title"), "enabled": true,
                "cells": [status_word(s(p, "prediction.Status")), s(p, "prediction.Title"), title,
                    num(p, &format!("prediction.outcome{i}.users")), num(p, &format!("prediction.outcome{i}.points"))]}));
        }
    }
    let (poll, prediction) = availability(seen);
    let (health, text) = status_line(seen);
    let table = |kind: &str,
                 title: &str,
                 icon: &str,
                 empty: &str,
                 columns: Vec<Value>,
                 rows: Vec<Value>,
                 actions: Vec<Value>| {
        json!({"kind": kind, "domain": DOMAIN, "title": title, "icon": icon, "what": "", "guide": "", "columns": columns,
            "rows": rows, "empty": empty, "search": false, "showTitle": true, "actions": actions, "perRow": 4})
    };
    // What stops a button, in words, for its hover.
    let tip = |mut button: Value, why: String| {
        if !why.is_empty() {
            button["tip"] = why.into();
        }
        button
    };
    let twitch_down = if !seen.ready[0] {
        twitch_why(seen)
    } else if !seen.prediction_known {
        tr("polls.refresh_prediction").to_string()
    } else {
        String::new()
    };
    let mut poll_table = table(
        "poll",
        tr("polls.poll"),
        "vote",
        tr("polls.no_poll"),
        vec![
            column("where", "polls.where", 110),
            column("question", "polls.question", 260),
            column("choice", "polls.choice", 220),
            column("votes", "polls.votes", 120),
        ],
        poll_rows,
        vec![
            tip(
                json!({"entity": "poll", "id": "new", "label": tr("btn.new_poll"), "icon": "plus", "needs": "none", "enabled": poll[0], "top": true, "primary": true}),
                if seen.ready.iter().any(|ready| *ready) {
                    String::new()
                } else {
                    tr("polls.accounts_needed").to_string()
                },
            ),
            {
                let mut b = custom("end", "btn.end_poll", "square", poll[1]);
                b["top"] = true.into();
                b["hideDisabled"] = true.into();
                b
            },
            json!({"id": "refresh", "label": tr("btn.refresh"), "icon": "refresh-cw", "top": true, "iconOnly": true}),
        ],
    );
    poll_table["intro"] = tr("polls.hint").into();
    poll_table["status"] = json!({"health": health, "text": text});
    let prediction_table = table(
        "prediction",
        tr("polls.prediction"),
        "trophy",
        tr("polls.no_prediction"),
        vec![
            column("status", "polls.status", 110),
            column("question", "polls.question", 220),
            column("outcome", "polls.outcome", 200),
            column("users", "polls.users", 90),
            column("points", "polls.points", 100),
        ],
        prediction_rows,
        vec![
            tip(
                json!({"entity": "prediction", "id": "new_prediction", "label": tr("btn.new_prediction"), "icon": "plus", "needs": "none", "enabled": prediction[0], "top": true, "primary": true}),
                if !twitch_down.is_empty() {
                    twitch_down.clone()
                } else if !prediction[0] {
                    tr("polls.prediction_exists").to_string()
                } else {
                    String::new()
                },
            ),
            tip(
                {
                    let mut b = custom("lock", "btn.lock", "lock", prediction[1]);
                    b["top"] = true.into();
                    b["hideDisabled"] = true.into();
                    b
                },
                twitch_down.clone(),
            ),
            tip(
                {
                    let mut b = custom("resolve", "btn.resolve", "trophy", prediction[2]);
                    b["top"] = true.into();
                    b["hideDisabled"] = true.into();
                    b
                },
                twitch_down.clone(),
            ),
            danger(confirm(
                tip(
                    {
                        let mut b = custom("cancel", "btn.cancel_prediction", "x", prediction[3]);
                        b["top"] = true.into();
                        b["hideDisabled"] = true.into();
                        b
                    },
                    twitch_down.clone(),
                ),
                tr("polls.cancel_ask").into(),
                "btn.cancel_prediction",
            )),
        ],
    );
    vec![poll_table, prediction_table]
}

/// Run one button. Starting a poll or prediction is a form (`entity_form` kinds `poll` and `prediction`).
pub fn act(engine: &Rc<Engine>, action: &str) -> Result<Value, String> {
    let accepted = Ok(json!({"accepted": true}));
    let now = seen();
    let (poll, prediction) = availability(&now);
    let go = |steps: Vec<Step>| {
        if ask(engine, steps) {
            accepted.clone()
        } else {
            Err(tr("polls.working").into())
        }
    };
    let unavailable = |why: String| -> Result<Value, String> { Err(why) };
    let prediction_why = || {
        if now.busy {
            tr("polls.working").to_string()
        } else if !now.ready[0] {
            twitch_why(&now)
        } else if !now.prediction_known {
            tr("polls.refresh_prediction").to_string()
        } else {
            tr("polls.no_prediction").to_string()
        }
    };
    match action {
        "refresh" => {
            ASKED.set(Some(Instant::now()));
            ask(engine, Vec::new());
            accepted
        }
        "new" | "new_prediction" => Err(trf(
            "polls.use_form",
            &[("kind", if action == "new" { "poll" } else { "prediction" })],
        )),
        "end" => {
            if !poll[1] {
                return unavailable(if now.busy {
                    tr("polls.working").into()
                } else {
                    tr("polls.no_poll").into()
                });
            }
            // Ends what is running, where it can.
            let mut steps = Vec::new();
            if now.ready[0] && now.twitch_poll_active() {
                steps.push((562, json!({}), trf("polls.end_on", &[("where", "Twitch")])));
            }
            if now.ready[1] && now.youtube_poll_active() {
                steps.push((
                    4012,
                    json!({"broadcast": ""}),
                    trf("polls.end_on", &[("where", "YouTube")]),
                ));
            }
            go(steps)
        }
        "lock" => {
            if !prediction[1] {
                return unavailable(prediction_why());
            }
            go(vec![(570, json!({}), tr("polls.prediction_lock").into())])
        }
        "cancel" => {
            if !prediction[3] {
                return unavailable(prediction_why());
            }
            go(vec![(569, json!({}), tr("polls.prediction_cancel").into())])
        }
        "resolve" => {
            if !prediction[2] {
                return unavailable(prediction_why());
            }
            // Which outcome won: the prediction's, picked in a prompt.
            let names = now.outcomes();
            if names.is_empty() {
                return unavailable(tr("polls.no_prediction").into());
            }
            let options = names
                .iter()
                .enumerate()
                .map(|(i, name)| super::prompts::Choice {
                    id: i.to_string(),
                    label: name.clone(),
                })
                .collect();
            let answer = super::prompts::choice(tr("polls.resolve_title").into(), options);
            let engine = engine.clone();
            tokio::task::spawn_local(async move {
                if let Ok(Some(index)) = answer.await {
                    if let Some(name) = index.parse::<usize>().ok().and_then(|i| names.get(i)) {
                        ask(
                            &engine,
                            vec![(
                                571,
                                json!({"winningIndex": index}),
                                trf("polls.resolve_as", &[("outcome", name)]),
                            )],
                        );
                    }
                }
            });
            accepted
        }
        _ => Err(format!("polls has no action {action}")),
    }
}

// The forms that start a poll or a prediction

pub fn external_fields(kind: &str) -> Option<&'static [F]> {
    match kind {
        "poll" => Some(POLL_FIELDS),
        "prediction" => Some(PREDICTION_FIELDS),
        _ => None,
    }
}

fn form_json(kind: &str, title: &str, ok: &str, fields: &[(&F, &str, bool)], item: Value, notes: Vec<String>) -> Value {
    let object = item.as_object().cloned().unwrap_or_default();
    let fields: Vec<Value> = fields
        .iter()
        .map(|(&(key, _, ref widget), label, off)| {
            let mut field = json!({"key": key, "label": label, "widget": widget, "holds": widget.about(),
                "value": crate::automation::edits::shown(widget, object.get(key))});
            if matches!(widget, W::Pick(_)) {
                field["widget"]["options"] = json!(LAST_NAMES);
            }
            // A platform's check box shows its logo.
            if matches!(widget, W::Bool) && matches!(key, "twitch" | "youtube") {
                field["brand"] = key.into();
            }
            if *off {
                field["disabled"] = true.into();
            }
            field
        })
        .collect();
    json!({"kind": kind, "title": title, "okLabel": ok, "json": false, "notes": notes, "new": item.clone(), "fields": fields, "item": item})
}

/// The form that starts a poll or a prediction, with the platforms that can run it.
pub fn external_form(kind: &str) -> Option<Result<Value, String>> {
    let now = seen();
    Some(Ok(match kind {
        "poll" => {
            let mut notes = Vec::new();
            if !now.ready[0] {
                notes.push(twitch_why(&now));
            }
            if !now.ready[1] {
                notes.push(tr("polls.youtube_unavailable").into());
            }
            let item = json!({"id": "", "question": "", "choices": [], "twitch": now.ready[0], "youtube": now.ready[1], "duration": 1});
            let mut form = form_json(
                kind,
                tr("btn.new_poll"),
                tr("btn.start_poll"),
                &[
                    (&POLL_FIELDS[0], tr("polls.question"), false),
                    (&POLL_FIELDS[1], tr("polls.choices"), false),
                    (&POLL_FIELDS[2], "Twitch", !now.ready[0]),
                    (&POLL_FIELDS[3], "YouTube", !now.ready[1]),
                    (&POLL_FIELDS[4], tr("polls.lasts"), false),
                ],
                item,
                notes.clone(),
            );
            // A platform that can't run it says why on hover.
            if !now.ready[0] {
                form["fields"][2]["tip"] = twitch_why(&now).into();
            }
            if !now.ready[1] {
                form["fields"][3]["tip"] = tr("polls.youtube_unavailable").into();
            }
            form
        }
        "prediction" => {
            let notes = if now.ready[0] {
                Vec::new()
            } else {
                vec![twitch_why(&now)]
            };
            let item = json!({"id": "", "question": "", "outcomes": [], "duration": 0});
            form_json(
                kind,
                tr("btn.new_prediction"),
                tr("btn.start_prediction"),
                &[
                    (&PREDICTION_FIELDS[0], tr("polls.question"), false),
                    (&PREDICTION_FIELDS[1], tr("polls.outcomes"), false),
                    (&PREDICTION_FIELDS[2], tr("polls.voting_time"), false),
                ],
                item,
                notes,
            )
        }
        _ => return None,
    }))
}

fn texts(item: &Obj, key: &str) -> Vec<String> {
    item.get(key)
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .map(|text| text.trim().to_string())
        .filter(|text| !text.is_empty())
        .collect()
}

/// Start the poll or prediction a form describes; what is wrong with it comes back as the form's error.
pub fn save_external(engine: &Rc<Engine>, kind: &str, item: Obj) -> Option<Result<Value, String>> {
    let question = item
        .get("question")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .trim()
        .to_string();
    let secs = LASTS[item.get("duration").and_then(Value::as_u64).unwrap_or(0).min(5) as usize];
    let now = seen();
    let flag = |key: &str| item.get(key).and_then(Value::as_bool) == Some(true);
    let steps: Result<Vec<Step>, &str> = match kind {
        "poll" => {
            let choices = texts(&item, "choices");
            // A platform that can't run it is off, whatever the check box says.
            let on = [flag("twitch") && now.ready[0], flag("youtube") && now.ready[1]];
            if question.is_empty() || choices.len() < 2 {
                Err("polls.need_two")
            } else if !on.iter().any(|on| *on) {
                Err("polls.pick_where")
            } else if on[1] && choices.len() > 4 {
                Err("polls.youtube_limit")
            } else if choices.len() > 5 {
                Err("polls.poll_limit")
            } else {
                let mut steps = Vec::new();
                if on[0] {
                    steps.push((
                        561,
                        json!({"title": question, "choices": choices, "duration": secs}),
                        trf("polls.poll_on", &[("where", "Twitch")]),
                    ));
                }
                if on[1] {
                    // YouTube takes up to 4 options, and runs until it's ended.
                    steps.push((
                        4011,
                        json!({"question": question, "options": choices, "broadcast": ""}),
                        trf("polls.poll_on", &[("where", "YouTube")]),
                    ));
                }
                Ok(steps)
            }
        }
        "prediction" => {
            let outcomes = texts(&item, "outcomes");
            if question.is_empty() || outcomes.len() < 2 {
                Err("polls.need_two")
            } else if outcomes.len() > 10 {
                Err("polls.outcome_limit")
            } else if !now.ready[0] {
                return Some(Err(twitch_why(&now)));
            } else {
                Ok(vec![(
                    568,
                    json!({"title": question, "outcomes": outcomes, "predictionWindow": secs}),
                    tr("polls.prediction_new").into(),
                )])
            }
        }
        _ => return None,
    };
    Some(match steps {
        Err(why) => Err(tr(why).into()),
        Ok(steps) => {
            if ask(engine, steps) {
                Ok(json!({"accepted": true}))
            } else {
                Err(tr("polls.working").into())
            }
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn reset(seen: Seen) {
        SEEN.with(|state| *state.borrow_mut() = seen);
        ASKED.set(Some(Instant::now()));
    }

    fn item(value: Value) -> Obj {
        value.as_object().unwrap().clone()
    }

    #[test]
    fn an_older_poll_read_cannot_replace_a_completed_lock() {
        let mut seen = Seen::default();
        let before = seen.begin(false).unwrap();
        let lock = seen.begin(true).unwrap();
        assert_eq!(seen.begin(false), None);
        assert_eq!(seen.begin(true), None);
        assert!(seen.finish(lock));
        assert!(!seen.finish(before));
        assert_eq!(seen.received, lock);
        assert!(!seen.busy);
        let after = seen.begin(false).unwrap();
        assert!(seen.finish(after));
    }

    #[test]
    fn expired_accounts_need_refresh_credentials_before_they_are_available() {
        assert!(!token_ready("", "", 0));
        assert!(!token_ready("expired", "", 1));
        assert!(token_ready("expired", "refresh", 1));
        assert!(token_ready("valid", "", u64::MAX));
    }

    #[test]
    fn locked_predictions_remain_available_and_terminal_predictions_do_not() {
        let response = json!({"data": [
            {"id": "old", "status": "RESOLVED"},
            {"id": "current", "status": "LOCKED", "outcomes": [{"id": "one", "title": "Yes"}]}
        ]});
        let prediction = actionable_prediction(&response).unwrap();
        let mut args = Obj::new();
        crate::engine::prediction_args(prediction, &mut args);
        assert_eq!(args["prediction.Id"], "current");
        assert_eq!(args["prediction.outcome0.title"], "Yes");
        assert_eq!(args["prediction.Status"], "LOCKED");
        assert!(!args["active"].as_bool().unwrap());
        for status in ["RESOLVED", "CANCELED"] {
            assert!(actionable_prediction(&json!({"data": [{"status": status}]})).is_none());
        }
        assert!(actionable_prediction(&json!({"data": [{"status": "ACTIVE"}]})).is_some());
        assert!(actionable_prediction(&json!({"data": []})).is_none());
    }

    #[test]
    fn poll_actions_require_an_available_account_and_known_prediction_state() {
        assert_eq!(
            action_availability([false; 2], true, true, "ACTIVE", true),
            ([false, false], [false; 4])
        );
        // New poll on any ready platform; End only where a poll runs.
        assert_eq!(
            action_availability([true, false], false, false, "", true).0,
            [true, false]
        );
        assert_eq!(
            action_availability([true, false], true, false, "", true).0,
            [true, true]
        );
        assert_eq!(
            action_availability([false, true], true, false, "", true).0,
            [true, false]
        );
        assert_eq!(
            action_availability([false, true], false, true, "", true).0,
            [true, true]
        );
        assert_eq!(
            action_availability([true, false], false, false, "LOCKED", true).1,
            [false, false, true, true]
        );
        assert_eq!(
            action_availability([true, false], false, false, "ACTIVE", true).1,
            [false, true, true, true]
        );
        assert_eq!(
            action_availability([true, false], false, false, "", true).1,
            [true, false, false, false]
        );
        assert_eq!(
            action_availability([true, false], false, false, "", false).1,
            [false; 4]
        );
    }

    #[test]
    fn the_status_line_says_what_is_ready_and_what_went_wrong() {
        let mut seen = Seen::default();
        assert_eq!(status_line(&seen).0, "busy");
        seen.received = 1;
        assert_eq!(status_line(&seen).0, "off");
        seen.ready = [true, true];
        assert_eq!(
            status_line(&seen).1,
            trf("polls.ready_on", &[("where", "Twitch, YouTube")])
        );
        seen.note = "x".into();
        seen.trouble = true;
        assert_eq!(status_line(&seen).0, "down");
        seen.busy = true;
        assert_eq!(status_line(&seen).0, "busy");
    }

    #[test]
    fn reading_someone_elses_channel_says_why_twitch_is_off() {
        let mut seen = Seen {
            received: 1,
            guest: Some(("Me".into(), "friend".into())),
            ..Seen::default()
        };
        let (health, text) = status_line(&seen);
        assert_eq!(health, "off");
        assert!(text.contains("Me") && text.contains("friend"), "{text}");
        seen.ready = [false, true];
        let (health, text) = status_line(&seen);
        assert_eq!(health, "up");
        assert!(text.contains("YouTube") && text.contains("friend"), "{text}");
        assert!(twitch_why(&seen).contains("friend"));
    }

    #[test]
    fn operations_without_success_flags_are_not_reported_as_successful() {
        assert_eq!(operation_result(562, &Obj::new()), None);
        assert_eq!(operation_result(4012, &Obj::new()), None);
        assert_eq!(operation_result(570, &Obj::new()), Some(false));
        assert_eq!(operation_result(569, &Obj::new()), Some(false));
        assert_eq!(operation_result(570, &item(json!({"locked": true}))), Some(true));
        assert_eq!(operation_result(569, &item(json!({"cancelled": false}))), Some(false));
    }

    #[test]
    fn a_predictions_status_in_words() {
        assert_eq!(status_word("LOCKED"), tr("polls.status_locked"));
        assert_eq!(status_word(""), "");
    }

    #[test]
    fn the_model_lists_what_runs_and_enables_only_what_can_be_done() {
        let mut args = Obj::new();
        args.insert("active".into(), true.into());
        args.insert("poll.Title".into(), "Best colour".into());
        args.insert("poll.choices.count".into(), 2.into());
        args.insert("poll.choice0.title".into(), "Red".into());
        args.insert("poll.choice0.totalVotes".into(), 3.into());
        args.insert("poll.choice1.title".into(), "Blue".into());
        args.insert("poll.choice1.totalVotes".into(), 5.into());
        let mut prediction = Obj::new();
        crate::engine::prediction_args(
            &json!({"id": "p", "title": "Win?", "status": "ACTIVE", "outcomes": [
                {"title": "Yes", "users": 2, "channel_points": 100}, {"title": "No", "users": 1, "channel_points": 40}]}),
            &mut prediction,
        );
        let seen = Seen {
            twitch_poll: args,
            prediction,
            ready: [true, false],
            prediction_known: true,
            received: 1,
            ..Seen::default()
        };
        let tables = model(&seen);
        let poll = &tables[0];
        assert_eq!(poll["kind"], "poll");
        assert_eq!(poll["rows"].as_array().unwrap().len(), 2);
        assert_eq!(poll["rows"][1]["cells"][0]["brand"], "twitch");
        assert_eq!(poll["rows"][1]["cells"][2], "Blue");
        assert_eq!(poll["rows"][1]["cells"][3], "5");
        assert_eq!(poll["actions"][0]["enabled"], true);
        assert_eq!(poll["actions"][1]["enabled"], true);
        assert_eq!(poll["status"]["health"], "up");
        let prediction = &tables[1];
        assert_eq!(prediction["rows"][0]["cells"][0], tr("polls.status_open"));
        assert_eq!(prediction["rows"][1]["cells"][4], "40");
        let enabled: Vec<_> = prediction["actions"]
            .as_array()
            .unwrap()
            .iter()
            .map(|a| a["enabled"].clone())
            .collect();
        assert_eq!(enabled, [json!(false), json!(true), json!(true), json!(true)]);
        assert!(prediction["actions"][3]["confirm"].is_string());
        // Busy: nothing can be started while a request is running.
        let busy = Seen { busy: true, ..seen };
        assert!(model(&busy)[1]["actions"]
            .as_array()
            .unwrap()
            .iter()
            .all(|a| a["enabled"] == false));
        assert_eq!(model(&busy)[0]["status"]["health"], "busy");
    }

    #[test]
    fn the_forms_refuse_what_cannot_run_and_say_why() {
        let _lock = crate::TEST_LOCK.lock().unwrap_or_else(|error| error.into_inner());
        crate::engine::tests::run(async {
            let engine = crate::engine::tests::engine(json!({"actions": []}));
            reset(Seen {
                received: 1,
                ..Seen::default()
            });
            let form = external_form("poll").unwrap().unwrap();
            assert_eq!(form["okLabel"], tr("btn.start_poll"));
            assert_eq!(form["fields"][2]["disabled"], true);
            assert_eq!(form["fields"][3]["value"], false);
            assert_eq!(form["notes"].as_array().unwrap().len(), 2);
            // Each platform's box carries its logo, and a box that is off says why on hover.
            assert_eq!(
                (form["fields"][2]["brand"].as_str(), form["fields"][3]["brand"].as_str()),
                (Some("twitch"), Some("youtube"))
            );
            assert_eq!(form["fields"][2]["tip"], twitch_why(&seen()));
            assert_eq!(form["fields"][3]["tip"], tr("polls.youtube_unavailable"));
            assert!(form["fields"][0]["brand"].is_null() && form["fields"][4]["brand"].is_null());
            // The page's buttons say why they are off.
            let tables = model(&seen());
            assert_eq!(tables[0]["actions"][0]["tip"], tr("polls.accounts_needed"));
            assert_eq!(tables[1]["actions"][0]["tip"], twitch_why(&seen()));
            assert!(tables[1]["actions"][1]["tip"].is_string());
            assert!(external_form("nonsense").is_none());
            let save = |kind: &str, value: Value| save_external(&engine, kind, item(value)).unwrap();
            assert_eq!(
                save("poll", json!({"question": " ", "choices": ["a", "b"], "twitch": true})),
                Err(tr("polls.need_two").into())
            );
            assert_eq!(
                save(
                    "poll",
                    json!({"question": "q", "choices": ["a", " ", "b"], "twitch": true})
                ),
                Err(tr("polls.pick_where").into()),
                "Twitch is off here, whatever the box says"
            );
            reset(Seen {
                received: 1,
                ready: [true, true],
                ..Seen::default()
            });
            assert_eq!(
                save(
                    "poll",
                    json!({"question": "q", "choices": ["a", "b", "c", "d", "e"], "youtube": true})
                ),
                Err(tr("polls.youtube_limit").into())
            );
            assert_eq!(
                save(
                    "poll",
                    json!({"question": "q", "choices": ["1", "2", "3", "4", "5", "6"], "twitch": true})
                ),
                Err(tr("polls.poll_limit").into())
            );
            assert_eq!(
                save("prediction", json!({"question": "q", "outcomes": ["a"]})),
                Err(tr("polls.need_two").into())
            );
            reset(Seen {
                received: 1,
                ..Seen::default()
            });
            assert!(save("prediction", json!({"question": "q", "outcomes": ["a", "b"]})).is_err());
            assert!(save_external(&engine, "nonsense", Obj::new()).is_none());
        });
    }

    #[test]
    fn buttons_without_their_conditions_say_why_and_a_started_prediction_step_reports_back() {
        let _lock = crate::TEST_LOCK.lock().unwrap_or_else(|error| error.into_inner());
        crate::engine::tests::run(async {
            let engine = crate::engine::tests::engine(json!({"actions": []}));
            reset(Seen {
                received: 1,
                ..Seen::default()
            });
            for action in ["end", "lock", "resolve", "cancel"] {
                assert!(act(&engine, action).is_err(), "{action}");
            }
            assert!(act(&engine, "new").is_err());
            assert!(act(&engine, "nonsense").is_err());
            // A prediction that is open: Resolve asks which outcome won, and the answer is acted on.
            let mut prediction = Obj::new();
            crate::engine::prediction_args(
                &json!({"id": "p", "title": "Win?", "status": "ACTIVE", "outcomes": [{"title": "Yes"}, {"title": "No"}]}),
                &mut prediction,
            );
            let open = prediction.clone();
            reset(Seen {
                received: 1,
                ready: [true, false],
                prediction_known: true,
                prediction,
                ..Seen::default()
            });
            let before = super::super::prompts::list().len();
            act(&engine, "resolve").unwrap();
            let prompts = super::super::prompts::list();
            assert_eq!(prompts.len(), before + 1);
            let prompt = prompts.last().unwrap();
            let super::super::prompts::Content::Choice { options, .. } = &prompt.content else {
                panic!("a choice of outcomes");
            };
            assert_eq!(
                options.iter().map(|o| o.label.as_str()).collect::<Vec<_>>(),
                ["Yes", "No"]
            );
            super::super::prompts::answer(prompt.id, json!("1")).unwrap();
            for _ in 0..4 {
                tokio::task::yield_now().await;
            }
            // This test has no Twitch connection, so the step reports that instead of reaching out.
            let after = seen();
            assert!(after.trouble, "{:?}", after.note);
            assert!(after.note.contains("No"), "{}", after.note);
            // Cancelling the choice does nothing.
            reset(Seen {
                received: 1,
                ready: [true, false],
                prediction_known: true,
                prediction: open.clone(),
                ..Seen::default()
            });
            act(&engine, "resolve").unwrap();
            let prompt = super::super::prompts::list().pop().unwrap();
            super::super::prompts::answer(prompt.id, Value::Null).unwrap();
            tokio::task::yield_now().await;
            assert!(!seen().busy);
        });
    }

    #[test]
    fn the_page_is_readable_and_refreshes_without_a_frontend() {
        let _lock = crate::TEST_LOCK.lock().unwrap_or_else(|error| error.into_inner());
        crate::engine::tests::run(async {
            let engine = crate::engine::tests::engine(json!({"actions": []}));
            reset(Seen::default());
            ASKED.set(None);
            let first = crate::servers::answer(
                &engine,
                &json!({"request": "StreamerzCoreView", "domain": "polls"}),
                true,
            )
            .unwrap();
            assert_eq!(first["tables"][0]["status"]["health"], "busy", "still checking");
            for _ in 0..4 {
                tokio::task::yield_now().await;
            }
            let second = crate::servers::answer(
                &engine,
                &json!({"request": "StreamerzCoreView", "domain": "polls"}),
                true,
            )
            .unwrap();
            assert_eq!(second["tables"][0]["status"]["text"], tr("polls.accounts_needed"));
            assert_eq!(second["tables"][1]["kind"], "prediction");
            // The same through the actions request.
            let act = json!({"request": "StreamerzCoreAction", "domain": "polls", "action": "refresh"});
            assert_eq!(crate::servers::answer(&engine, &act, true).unwrap()["status"], "ok");
        });
    }
}
