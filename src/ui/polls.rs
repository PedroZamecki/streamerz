//! The Polls page: polls on Twitch and YouTube, and Twitch's predictions, started, ended and followed
//! here. They run through the same sub-actions an action uses (Create Poll, Lock Prediction...), so the
//! page and the actions behave alike. Kick and TikTok have no API for either.

use super::dialogs;
use super::lists::{bar, browser, line, Cmds};
use super::rows::Rows;
use super::{tell, ROW};
use crate::i18n::{tr, trf};
use crate::model::Obj;
use fltk::button::{Button, CheckButton};
use fltk::frame::Frame;
use fltk::group::{Flex, Wizard};
use fltk::input::Input;
use fltk::menu::Choice;
use fltk::prelude::*;
use serde_json::{json, Value};
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::atomic::Ordering;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// How long polls and predictions may run: the choices' seconds.
const LASTS: [u64; 6] = [60, 120, 180, 300, 600, 1800];

/// What the platforms said last: Twitch's poll and prediction, YouTube's poll (their variables), and
/// how the last thing asked went.
#[derive(Default, Clone)]
struct Seen {
    twitch_poll: Obj,
    prediction: Obj,
    youtube_poll: Obj,
    note: String,
    ready: [bool; 2],
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
}

pub struct Polls {
    pub tab: Flex,
    cmds: Cmds,
    poll_results: Rows,
    pred_results: Rows,
    note: Frame,
    /// Filled by the engine's answers; shown on the next refresh.
    seen: Arc<Mutex<Seen>>,
    asked: Option<Instant>,
    targets: [CheckButton; 2],
    /// A target's tick while it can't be used: cleared on screen, put back once it can.
    held: [bool; 2],
    poll_buttons: [Button; 3],
    prediction_buttons: [Button; 4],
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

impl Polls {
    /// Shows what's known; asks the platforms again when something changed or it's been a while.
    pub fn refresh(&mut self) {
        let dirty = super::POLLS_DIRTY.swap(false, Ordering::Relaxed);
        if dirty || self.asked.is_none_or(|t| t.elapsed() > Duration::from_secs(10)) {
            self.asked = Some(Instant::now());
            ask(&self.cmds, &self.seen, None);
        }
        let seen = self.seen.lock().map(|s| s.clone()).unwrap_or_default();
        let sel = (self.poll_results.value(), self.pred_results.value());
        self.poll_results.clear();
        let tp = &seen.twitch_poll;
        if tp.get("active").and_then(Value::as_bool) == Some(true) {
            let n = tp.get("poll.choices.count").and_then(Value::as_u64).unwrap_or(0);
            for i in 0..n {
                self.poll_results.add(&line(&[
                    "Twitch".into(),
                    s(tp, "poll.Title").into(),
                    s(tp, &format!("poll.choice{i}.title")).into(),
                    num(tp, &format!("poll.choice{i}.totalVotes")),
                ]));
            }
        }
        let yp = &seen.youtube_poll;
        if !s(yp, "poll.question").is_empty() {
            let n = yp.get("poll.options.count").and_then(Value::as_u64).unwrap_or(0);
            for i in 0..n {
                self.poll_results.add(&line(&[
                    "YouTube".into(),
                    s(yp, "poll.question").into(),
                    s(yp, &format!("poll.option{i}.text")).into(),
                    format!(
                        "{} ({})",
                        num(yp, &format!("poll.option{i}.votes")),
                        s(yp, &format!("poll.option{i}.votePercentage"))
                    ),
                ]));
            }
        }
        self.pred_results.clear();
        let p = &seen.prediction;
        if !s(p, "prediction.Id").is_empty() {
            for i in 0..10 {
                let title = s(p, &format!("prediction.outcome{i}.title"));
                if title.is_empty() {
                    break;
                }
                self.pred_results.add(&line(&[
                    status(s(p, "prediction.Status")).into(),
                    s(p, "prediction.Title").into(),
                    title.into(),
                    num(p, &format!("prediction.outcome{i}.users")),
                    num(p, &format!("prediction.outcome{i}.points")),
                ]));
            }
        }
        if sel.0 > 0 {
            self.poll_results.select(sel.0);
        }
        if sel.1 > 0 {
            self.pred_results.select(sel.1);
        }
        let note = if seen.busy {
            tr("polls.working")
        } else if seen.received == 0 {
            tr("polls.checking")
        } else if seen.note.is_empty() && !seen.ready.iter().any(|ready| *ready) {
            tr("polls.accounts_needed")
        } else {
            &seen.note
        };
        self.note.set_label(&note.replace('@', "@@"));
        self.note.set_tooltip(&note.replace('@', "@@"));
        self.update_controls(&seen);
    }

    fn update_controls(&mut self, seen: &Seen) {
        let selected = self.targets.each_ref().map(|target| target.is_checked());
        let active_poll = seen.twitch_poll.get("active").and_then(Value::as_bool) == Some(true);
        let prediction = s(&seen.prediction, "prediction.Status");
        let (mut poll, mut prediction_actions) =
            action_availability(seen.ready, selected, active_poll, prediction, seen.prediction_known);
        if seen.busy {
            poll = [false; 3];
            prediction_actions = [false; 4];
        }
        for ((target, ready), held) in self.targets.iter_mut().zip(seen.ready).zip(&mut self.held) {
            if ready && *held {
                target.set_checked(true);
                *held = false;
            } else if !ready && target.is_checked() {
                target.set_checked(false);
                *held = true;
            }
            if ready && !seen.busy {
                target.activate();
            } else {
                target.deactivate();
            }
        }
        for (button, available) in self.poll_buttons.iter_mut().zip(poll) {
            if available {
                button.activate();
            } else {
                button.deactivate();
            }
        }
        for (button, available) in self.prediction_buttons.iter_mut().zip(prediction_actions) {
            if available {
                button.activate();
            } else {
                button.deactivate();
            }
            button.set_tooltip(if !seen.ready[0] {
                tr("polls.twitch_unavailable")
            } else if !seen.prediction_known {
                tr("polls.refresh_prediction")
            } else {
                ""
            });
        }
        for (i, target) in self.targets.iter_mut().enumerate() {
            target.set_tooltip(if seen.ready[i] {
                ""
            } else if i == 0 {
                tr("polls.twitch_unavailable")
            } else {
                tr("polls.youtube_unavailable")
            });
        }
        self.prediction_buttons[0].set_tooltip(if !seen.ready[0] {
            tr("polls.twitch_unavailable")
        } else if !seen.prediction_known {
            tr("polls.refresh_prediction")
        } else if !prediction_actions[0] {
            tr("polls.prediction_exists")
        } else {
            ""
        });
    }
}

/// YouTube can end only polls created by its runner. Public chat state cannot prove ownership, so
/// keep its authenticated End action available and report the request outcome without guessing.
fn action_availability(
    ready: [bool; 2],
    selected: [bool; 2],
    twitch_poll: bool,
    prediction: &str,
    prediction_known: bool,
) -> ([bool; 3], [bool; 4]) {
    let targets = [ready[0] && selected[0], ready[1] && selected[1]];
    let active = matches!(prediction, "ACTIVE" | "LOCKED");
    let prediction_ready = ready[0] && prediction_known;
    (
        [
            targets.iter().any(|target| *target),
            (targets[0] && twitch_poll) || targets[1],
            true,
        ],
        [
            prediction_ready && !active,
            prediction_ready && prediction == "ACTIVE",
            prediction_ready && active,
            prediction_ready && active,
        ],
    )
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
fn status(st: &str) -> &'static str {
    match st {
        "ACTIVE" => tr("polls.status_open"),
        "LOCKED" => tr("polls.status_locked"),
        "RESOLVED" => tr("polls.status_resolved"),
        "CANCELED" => tr("polls.status_canceled"),
        _ => "",
    }
}

/// Asks the engine to run `steps` (sub-action code and settings, and what to say when it worked or
/// didn't), then reads the current poll and prediction again; the page shows it all when it comes.
fn ask(cmds: &Cmds, seen: &Arc<Mutex<Seen>>, steps: Option<Vec<(u64, Value, String)>>) {
    let mutation = steps.as_ref().is_some_and(|steps| !steps.is_empty());
    let Some(sequence) = seen.lock().ok().and_then(|mut seen| seen.begin(mutation)) else {
        return;
    };
    if mutation {
        crate::ui::changed();
    }
    let seen = seen.clone();
    tell(cmds, move |e| {
        let e = e.clone();
        tokio::task::spawn_local(async move {
            let obj = |v: Value| v.as_object().cloned().unwrap_or_default();
            let mut notes = Vec::new();
            let ready = {
                let c = e.config();
                [
                    e.twitch().is_some()
                        && c.twitch.broadcaster.as_ref().is_some_and(|a| {
                            token_ready(&a.token.access_token, &a.token.refresh_token, a.token.expires_at)
                        }),
                    c.youtube.broadcaster.as_ref().is_some_and(|a| {
                        token_ready(&a.token.access_token, &a.token.refresh_token, a.token.expires_at)
                            && (!crate::oauth::stale(a.token.expires_at)
                                || (!c.youtube.client_id.is_empty() && !c.youtube.client_secret.is_empty()))
                    }),
                ]
            };
            for (kind, settings, what) in steps.unwrap_or_default() {
                // Twitch's sub-actions do nothing without Twitch (and don't say so).
                if (561..=572).contains(&kind) && !ready[0] {
                    notes.push(trf("polls.twitch_off", &[("what", &what)]));
                    continue;
                }
                if matches!(kind, 4011 | 4012) && !ready[1] {
                    notes.push(tr("polls.youtube_unavailable").into());
                    continue;
                }
                let r = e.run_one(kind, obj(settings)).await;
                let message = match operation_result(kind, &r) {
                    Some(true) => "polls.done",
                    Some(false) => "polls.failed",
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
                    Err(error) => notes.push(trf(
                        "polls.lookup_failed",
                        &[("what", tr("polls.prediction")), ("error", &error)],
                    )),
                }
            }
            if let Ok(mut s) = seen.lock() {
                if !s.finish(sequence) {
                    return;
                }
                s.twitch_poll = twitch_poll;
                s.prediction = prediction;
                s.youtube_poll = crate::youtube::current_poll();
                s.ready = ready;
                s.prediction_known = prediction_known;
                if !notes.is_empty() {
                    s.note = notes.join("  ");
                }
            }
            crate::ui::changed();
        });
    });
}

/// The non-empty texts of `inputs`.
fn texts(inputs: &[Input]) -> Vec<String> {
    inputs
        .iter()
        .map(|i| i.value().trim().to_string())
        .filter(|t| !t.is_empty())
        .collect()
}

fn lasts_choice(default: usize) -> Choice {
    let mut c = super::fit::choice();
    let names: Vec<String> = LASTS
        .iter()
        .map(|s| {
            if *s < 60 {
                format!("{s} s")
            } else {
                format!("{} min", s / 60)
            }
        })
        .collect();
    c.add_choice(&names.join("|"));
    c.set_value(default as i32);
    c
}

pub fn tab(tabs: &Wizard, cmds: &Cmds) -> Rc<RefCell<Polls>> {
    let (scroll, mut col) = super::tab_scrolled(tabs, "Polls", 1150);

    super::hint(&mut col, tr("polls.hint"), 36);
    // How the last thing asked went.
    let mut note =
        Frame::default().with_align(fltk::enums::Align::Left | fltk::enums::Align::Inside | fltk::enums::Align::Wrap);
    note.set_label_font(fltk::enums::Font::HelveticaBold);
    col.fixed(&note, 60);
    // A poll, on Twitch and YouTube.
    let mut t = Frame::default()
        .with_label(tr("polls.poll"))
        .with_align(fltk::enums::Align::Left | fltk::enums::Align::Inside);
    t.set_label_font(fltk::enums::Font::HelveticaBold);
    col.fixed(&t, ROW);
    let r = dialogs::row(tr("polls.question"));
    let question = Input::default();
    r.end();
    col.fixed(&r, ROW);
    let mut choices = Vec::new();
    for i in 1..=5 {
        let r = dialogs::row(&trf("polls.choice_n", &[("n", &i.to_string())]));
        choices.push(Input::default());
        r.end();
        col.fixed(&r, ROW);
    }
    let mut r = dialogs::row(tr("polls.lasts"));
    let lasts = lasts_choice(1);
    r.fixed(&lasts, 120);
    Frame::default();
    r.end();
    col.fixed(&r, ROW);
    let mut r = Flex::default().row();
    r.set_pad(8);
    let on_twitch = CheckButton::default().with_label(tr("polls.platform_twitch"));
    on_twitch.set_checked(true);
    r.fixed(&on_twitch, 90);
    let on_youtube = CheckButton::default().with_label(tr("polls.platform_youtube"));
    r.fixed(&on_youtube, 100);
    Frame::default();
    r.end();
    col.fixed(&r, ROW);
    let pb = bar(&mut col, &[tr("btn.start_poll"), tr("btn.end_poll"), tr("btn.refresh")]);
    let poll_results = browser(
        &mut col,
        &[90, 220, 220, 120],
        &["polls.where", "polls.question", "polls.choice", "polls.votes"],
        "polls.no_poll",
    );
    col.fixed(&*poll_results, 150);
    poll_results.clone().set_menu(&pb);

    // A prediction, on Twitch.
    let mut t = Frame::default()
        .with_label(tr("polls.prediction"))
        .with_align(fltk::enums::Align::Left | fltk::enums::Align::Inside);
    t.set_label_font(fltk::enums::Font::HelveticaBold);
    col.fixed(&t, ROW);
    let r = dialogs::row(tr("polls.question"));
    let pquestion = Input::default();
    r.end();
    col.fixed(&r, ROW);
    let mut outcomes = Vec::new();
    for i in 1..=4 {
        let r = dialogs::row(&trf("polls.outcome_n", &[("n", &i.to_string())]));
        outcomes.push(Input::default());
        r.end();
        col.fixed(&r, ROW);
    }
    let mut r = dialogs::row(tr("polls.voting_time"));
    let window = lasts_choice(0);
    r.fixed(&window, 120);
    Frame::default();
    r.end();
    col.fixed(&r, ROW);
    let db = bar(
        &mut col,
        &[
            tr("btn.start_prediction"),
            tr("btn.lock"),
            tr("btn.resolve"),
            tr("btn.cancel_prediction"),
        ],
    );
    let pred_results = browser(
        &mut col,
        &[90, 220, 220, 90, 100],
        &[
            "polls.status",
            "polls.question",
            "polls.outcome",
            "polls.users",
            "polls.points",
        ],
        "polls.no_prediction",
    );
    col.fixed(&*pred_results, 150);
    pred_results.clone().set_menu(&db);
    Frame::default();
    col.end();
    scroll.end();

    let seen: Arc<Mutex<Seen>> = Arc::default();
    let me = Rc::new(RefCell::new(Polls {
        tab: col,
        cmds: cmds.clone(),
        poll_results,
        pred_results,
        note,
        seen: seen.clone(),
        asked: None,
        targets: [on_twitch.clone(), on_youtube.clone()],
        held: [false; 2],
        poll_buttons: [pb[0].clone(), pb[1].clone(), pb[2].clone()],
        prediction_buttons: [db[0].clone(), db[1].clone(), db[2].clone(), db[3].clone()],
    }));
    me.borrow_mut().update_controls(&Seen::default());
    for target in [&on_twitch, &on_youtube] {
        let (me, seen) = (me.clone(), seen.clone());
        target.clone().set_callback(move |_| {
            let snapshot = seen.lock().map(|s| s.clone()).unwrap_or_default();
            if let Ok(mut me) = me.try_borrow_mut() {
                me.update_controls(&snapshot);
            }
        });
    }
    let go = {
        let (cmds, seen) = (cmds.clone(), seen.clone());
        move |steps: Vec<(u64, Value, String)>| ask(&cmds, &seen, Some(steps))
    };
    let on = |b: &Button, f: Box<dyn Fn()>| b.clone().set_callback(move |_| f());

    on(&pb[0], {
        let (go, question, choices, lasts, on_twitch, on_youtube) = (
            go.clone(),
            question.clone(),
            choices.clone(),
            lasts.clone(),
            on_twitch.clone(),
            on_youtube.clone(),
        );
        Box::new(move || {
            let (q, c) = (question.value().trim().to_string(), texts(&choices));
            if q.is_empty() || c.len() < 2 {
                dialogs::message(tr("polls.need_two"));
                return;
            }
            if on_youtube.active() && on_youtube.is_checked() && c.len() > 4 {
                dialogs::message(tr("polls.youtube_limit"));
                return;
            }
            let secs = LASTS[lasts.value().clamp(0, 5) as usize];
            let mut steps = Vec::new();
            if on_twitch.active() && on_twitch.is_checked() {
                steps.push((
                    561,
                    json!({"title": q, "choices": c, "duration": secs}),
                    trf("polls.poll_on", &[("where", "Twitch")]),
                ));
            }
            if on_youtube.active() && on_youtube.is_checked() {
                // YouTube takes up to 4 options, and runs until it's ended.
                steps.push((
                    4011,
                    json!({"question": q, "options": c, "broadcast": ""}),
                    trf("polls.poll_on", &[("where", "YouTube")]),
                ));
            }
            go(steps);
        })
    });
    on(&pb[1], {
        let (go, on_twitch, on_youtube) = (go.clone(), on_twitch.clone(), on_youtube.clone());
        Box::new(move || {
            let mut steps = Vec::new();
            if on_twitch.active() && on_twitch.is_checked() {
                steps.push((562, json!({}), trf("polls.end_on", &[("where", "Twitch")])));
            }
            if on_youtube.active() && on_youtube.is_checked() {
                steps.push((
                    4012,
                    json!({"broadcast": ""}),
                    trf("polls.end_on", &[("where", "YouTube")]),
                ));
            }
            go(steps);
        })
    });
    on(&pb[2], {
        let go = go.clone();
        Box::new(move || go(Vec::new()))
    });
    on(&db[0], {
        let (go, pquestion, outcomes, window) = (go.clone(), pquestion.clone(), outcomes.clone(), window.clone());
        Box::new(move || {
            let (q, o) = (pquestion.value().trim().to_string(), texts(&outcomes));
            if q.is_empty() || o.len() < 2 {
                dialogs::message(tr("polls.need_two"));
                return;
            }
            let secs = LASTS[window.value().clamp(0, 5) as usize];
            go(vec![(
                568,
                json!({"title": q, "outcomes": o, "predictionWindow": secs}),
                tr("polls.prediction_new").into(),
            )]);
        })
    });
    on(&db[1], {
        let go = go.clone();
        Box::new(move || go(vec![(570, json!({}), tr("polls.prediction_lock").into())]))
    });
    on(&db[2], {
        let (go, seen) = (go.clone(), seen.clone());
        Box::new(move || {
            // Which outcome won: the prediction's, picked from a menu.
            let p = seen.lock().map(|s| s.prediction.clone()).unwrap_or_default();
            let names: Vec<String> = (0..10)
                .map(|i| s(&p, &format!("prediction.outcome{i}.title")).to_string())
                .take_while(|t| !t.is_empty())
                .collect();
            if names.is_empty() {
                dialogs::message(tr("polls.no_prediction"));
                return;
            }
            let labels: Vec<&str> = names.iter().map(String::as_str).collect();
            if let Some(i) = super::pick(&labels) {
                go(vec![(
                    571,
                    json!({"winningIndex": i.to_string()}),
                    trf("polls.resolve_as", &[("outcome", &names[i])]),
                )]);
            }
        })
    });
    on(&db[3], {
        let go = go.clone();
        Box::new(move || {
            if dialogs::choice(
                tr("polls.cancel_ask"),
                tr("dialogs.cancel"),
                tr("btn.cancel_prediction"),
            ) {
                go(vec![(569, json!({}), tr("polls.prediction_cancel").into())]);
            }
        })
    });
    me
}

#[cfg(test)]
mod tests {
    use super::*;

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
    fn poll_actions_require_an_available_selected_account_and_known_prediction_state() {
        assert_eq!(
            action_availability([false; 2], [true; 2], true, "ACTIVE", true),
            ([false, false, true], [false; 4])
        );
        assert_eq!(
            action_availability([true, false], [false; 2], false, "", true).0,
            [false, false, true]
        );
        assert_eq!(
            action_availability([true, false], [true, false], false, "LOCKED", true).1,
            [false, false, true, true]
        );
        assert_eq!(
            action_availability([true, false], [true, false], false, "ACTIVE", true).1,
            [false, true, true, true]
        );
        assert_eq!(
            action_availability([true, false], [true, false], false, "", true).1,
            [true, false, false, false]
        );
        assert_eq!(
            action_availability([true, false], [true, false], false, "", false).1,
            [false; 4]
        );
    }

    #[test]
    fn operations_without_success_flags_are_not_reported_as_successful() {
        assert_eq!(operation_result(562, &Obj::new()), None);
        assert_eq!(operation_result(4012, &Obj::new()), None);
        assert_eq!(operation_result(570, &Obj::new()), Some(false));
        assert_eq!(operation_result(569, &Obj::new()), Some(false));
        assert_eq!(
            operation_result(570, json!({"locked": true}).as_object().unwrap()),
            Some(true)
        );
        assert_eq!(
            operation_result(569, json!({"cancelled": false}).as_object().unwrap()),
            Some(false)
        );
    }

    #[test]
    fn a_predictions_status_in_words() {
        assert_eq!(super::status("LOCKED"), crate::i18n::tr("polls.status_locked"));
        assert_eq!(super::status(""), "");
    }
}
