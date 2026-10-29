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
        self.note.set_label(&seen.note.replace('@', "@@"));
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
    let seen = seen.clone();
    tell(cmds, move |e| {
        let e = e.clone();
        tokio::task::spawn_local(async move {
            let obj = |v: Value| v.as_object().cloned().unwrap_or_default();
            let mut notes = Vec::new();
            for (kind, settings, what) in steps.unwrap_or_default() {
                // Twitch's sub-actions do nothing without Twitch (and don't say so).
                if (561..=572).contains(&kind) && e.twitch().is_none() {
                    notes.push(trf("polls.twitch_off", &[("what", &what)]));
                    continue;
                }
                let r = e.run_one(kind, obj(settings)).await;
                // Ending and locking don't say: their failures are in the Log.
                let ok = r.get("success").and_then(Value::as_bool).unwrap_or(true);
                notes.push(trf(if ok { "polls.done" } else { "polls.failed" }, &[("what", &what)]));
            }
            let twitch_poll = if e.twitch().is_some() {
                e.run_one(566, Obj::new()).await
            } else {
                Obj::new()
            };
            let prediction = if e.twitch().is_some() {
                e.run_one(567, Obj::new()).await
            } else {
                Obj::new()
            };
            if let Ok(mut s) = seen.lock() {
                s.twitch_poll = twitch_poll;
                s.prediction = prediction;
                s.youtube_poll = crate::youtube::current_poll();
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
    let (scroll, mut col) = super::tab_scrolled(tabs, "Polls", 900);

    super::hint(&mut col, tr("polls.hint"), 36);
    // How the last thing asked went.
    let mut note =
        Frame::default().with_align(fltk::enums::Align::Left | fltk::enums::Align::Inside | fltk::enums::Align::Wrap);
    note.set_label_font(fltk::enums::Font::HelveticaBold);
    col.fixed(&note, 22);
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
    let on_twitch = CheckButton::default().with_label("Twitch");
    on_twitch.set_checked(true);
    r.fixed(&on_twitch, 90);
    let on_youtube = CheckButton::default().with_label("YouTube");
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
    }));
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
            let secs = LASTS[lasts.value().clamp(0, 5) as usize];
            let mut steps = Vec::new();
            if on_twitch.is_checked() {
                steps.push((
                    561,
                    json!({"title": q, "choices": c, "duration": secs}),
                    trf("polls.poll_on", &[("where", "Twitch")]),
                ));
            }
            if on_youtube.is_checked() {
                // YouTube takes up to 4 options, and runs until it's ended.
                let c4: Vec<&String> = c.iter().take(4).collect();
                steps.push((
                    4011,
                    json!({"question": q, "options": c4, "broadcast": ""}),
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
            if on_twitch.is_checked() {
                steps.push((562, json!({}), trf("polls.end_on", &[("where", "Twitch")])));
            }
            if on_youtube.is_checked() {
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
    #[test]
    fn a_predictions_status_in_words() {
        assert_eq!(super::status("LOCKED"), crate::i18n::tr("polls.status_locked"));
        assert_eq!(super::status(""), "");
    }
}
