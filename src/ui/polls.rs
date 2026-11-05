//! The Polls page: polls on Twitch and YouTube, and Twitch's predictions, started, ended and followed
//! here. They run through the same sub-actions an action uses (Create Poll, Lock Prediction...), so the
//! page and the actions behave alike. Kick and TikTok have no API for either.

use super::dialogs;
use super::fit::Ellipsis;
use super::lists::{bar, browser, line, Cmds};
use super::rows::Rows;
use super::{tell, ROW};
use crate::i18n::{tr, trf};
use crate::model::Obj;
use fltk::button::Button;
use fltk::enums::{Align, Color, Font, Key, Shortcut};
use fltk::frame::Frame;
use fltk::group::{Flex, Wizard};
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
}

pub struct Polls {
    pub tab: Flex,
    cmds: Cmds,
    poll_results: Rows,
    pred_results: Rows,
    /// How the accounts are and how the last thing asked went: a state's icon and words.
    note: Frame,
    /// Filled by the engine's answers; shown on the next refresh.
    seen: Arc<Mutex<Seen>>,
    asked: Option<Instant>,
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
    /// The status line and the buttons as they are before anything is known.
    fn refresh_note(&mut self) {
        let seen = Seen::default();
        let (health, text) = note(&seen);
        health.show(&mut self.note, &text);
        self.update_controls(&seen);
    }

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
        let (health, note) = note(&seen);
        health.show(&mut self.note, &note);
        self.note.set_tooltip(&note.replace('@', "@@"));
        self.note.redraw();
        self.update_controls(&seen);
    }

    fn update_controls(&mut self, seen: &Seen) {
        let twitch_poll = seen.twitch_poll.get("active").and_then(Value::as_bool) == Some(true);
        let youtube_poll = !s(&seen.youtube_poll, "poll.question").is_empty();
        let prediction = s(&seen.prediction, "prediction.Status");
        let (mut poll, mut prediction_actions) =
            action_availability(seen.ready, twitch_poll, youtube_poll, prediction, seen.prediction_known);
        if seen.busy {
            poll = [false, false, false];
            prediction_actions = [false; 4];
        }
        for (button, available) in self.poll_buttons.iter_mut().zip(poll) {
            if available {
                button.activate();
            } else {
                button.deactivate();
            }
        }
        self.poll_buttons[0].set_tooltip(if seen.ready.iter().any(|r| *r) {
            ""
        } else {
            tr("polls.accounts_needed")
        });
        for (button, available) in self.prediction_buttons.iter_mut().zip(prediction_actions) {
            if available {
                button.activate();
            } else {
                button.deactivate();
            }
            button.set_tooltip(&if !seen.ready[0] {
                twitch_why(seen)
            } else if !seen.prediction_known {
                tr("polls.refresh_prediction").to_string()
            } else {
                String::new()
            });
        }
        self.prediction_buttons[0].set_tooltip(&if !seen.ready[0] {
            twitch_why(seen)
        } else if !seen.prediction_known {
            tr("polls.refresh_prediction").to_string()
        } else if !prediction_actions[0] {
            tr("polls.prediction_exists").to_string()
        } else {
            String::new()
        });
    }
}

/// Why Twitch can't run polls and predictions now.
fn twitch_why(seen: &Seen) -> String {
    match &seen.guest {
        Some((_, channel)) => trf("polls.guest_short", &[("channel", channel)]),
        None => tr("polls.twitch_unavailable").to_string(),
    }
}

/// The page's status line: its state and its words.
fn note(seen: &Seen) -> (super::Health, String) {
    use super::Health;
    let ready: Vec<&str> = ["Twitch", "YouTube"]
        .into_iter()
        .zip(seen.ready)
        .filter_map(|(name, ready)| ready.then_some(name))
        .collect();
    if seen.busy {
        (Health::Busy, tr("polls.working").into())
    } else if seen.received == 0 {
        (Health::Busy, tr("polls.checking").into())
    } else if !seen.note.is_empty() {
        (if seen.trouble { Health::Down } else { Health::Up }, seen.note.clone())
    } else if let Some((account, channel)) = &seen.guest {
        let guest = trf("polls.guest", &[("account", account), ("channel", channel)]);
        match ready.is_empty() {
            true => (Health::Off, guest),
            false => (
                Health::Up,
                format!("{} {guest}", trf("polls.ready_on", &[("where", &ready.join(", "))])),
            ),
        }
    } else if ready.is_empty() {
        (Health::Off, tr("polls.accounts_needed").into())
    } else {
        (Health::Up, trf("polls.ready_on", &[("where", &ready.join(", "))]))
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
) -> ([bool; 3], [bool; 4]) {
    let active = matches!(prediction, "ACTIVE" | "LOCKED");
    let prediction_ready = ready[0] && prediction_known;
    (
        [
            ready.iter().any(|ready| *ready),
            (ready[0] && twitch_poll) || (ready[1] && youtube_poll),
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
            for (kind, settings, what) in steps.unwrap_or_default() {
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
            if let Ok(mut s) = seen.lock() {
                if !s.finish(sequence) {
                    return;
                }
                s.twitch_poll = twitch_poll;
                s.prediction = prediction;
                s.youtube_poll = crate::youtube::current_poll();
                s.ready = ready;
                s.guest = guest;
                s.prediction_known = prediction_known;
                if !notes.is_empty() {
                    s.note = notes.join("  ");
                    s.trouble = trouble;
                }
            }
            crate::ui::changed();
        });
    });
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

/// A section's title with its icon.
fn section(col: &mut Flex, icon: &str, text: &str) {
    let mut f = Frame::default()
        .with_label(&format!("  {text}"))
        .with_align(Align::Left | Align::Inside | Align::ImageNextToText);
    f.set_label_font(Font::HelveticaBold);
    f.set_image(super::icons::icon(icon, Color::Foreground, ROW - 10));
    col.fixed(&f, ROW - 2);
}

/// A dialog's own buttons at the right of `bar`: Cancel, and the one that does it (Enter).
fn cancel_go(bar: &mut Flex, answer: &dialogs::Answer, go: &str) {
    Frame::default();
    let mut cancel = super::button(bar, tr("dialogs.cancel"), 90);
    let mut start = super::button(bar, go, 140);
    start.set_shortcut(Shortcut::from_key(Key::Enter));
    cancel.set_shortcut(Shortcut::from_key(Key::Escape));
    start.set_callback({
        let a = answer.clone();
        move |_| a.set(Some(true))
    });
    cancel.set_callback({
        let a = answer.clone();
        move |_| a.set(Some(false))
    });
}

/// What a new poll asks: its question, its choices, how long (Twitch), and where.
pub(super) struct NewPoll {
    question: String,
    choices: Vec<String>,
    secs: u64,
    on: [bool; 2],
}

/// What a question form asks.
struct Form {
    title: &'static str,
    go: &'static str,
    question: &'static str,
    /// The hint in answer `n`'s field.
    answer: fn(usize) -> String,
    most: usize,
    /// The duration's tooltip, and the one picked at first.
    lasts: (&'static str, usize),
    /// Where it can run (Twitch, YouTube): chips to pick from. None: Twitch only, no chips.
    ready: Option<[bool; 2]>,
}

/// A question, its answers (two fields at first, one more as the last is filled), how long it lasts,
/// and where it runs (each platform a chip with its logo). What was asked, when started; `check` may
/// refuse it (it says why).
fn ask_form(f: Form, check: impl Fn(&str, &[String], [bool; 2]) -> Option<&'static str>) -> Option<NewPoll> {
    const W: i32 = 460;
    const PAD: i32 = 16;
    let (mut win, answer) = dialogs::window(W, 200, f.title);
    let mut col = Flex::new(PAD, 4, W - PAD * 2, 200, None).column();
    col.set_pad(8);
    let mut question = super::fields::field(&mut col, "message-circle-question", f.question);
    let answers = super::fields::Growing::new(&mut col, 2, f.most, f.answer);
    let mut opts = Flex::default().row();
    opts.set_pad(8);
    let mut chips = Vec::new();
    if let Some(ready) = f.ready {
        for (i, (name, why)) in [
            ("Twitch", tr("polls.twitch_unavailable")),
            ("YouTube", tr("polls.youtube_unavailable")),
        ]
        .into_iter()
        .enumerate()
        {
            let mut c = super::fields::chip(&mut opts, name, super::img::logo(name, 16), ready[i]);
            if !ready[i] {
                c.deactivate();
                c.set_tooltip(why);
            }
            chips.push(c);
        }
    }
    Frame::default();
    let mut timer = super::fields::lead_icon(&mut opts, "timer");
    let mut lasts = lasts_choice(f.lasts.1);
    lasts.set_tooltip(f.lasts.0);
    timer.set_tooltip(f.lasts.0);
    opts.fixed(&lasts, 100);
    opts.end();
    col.fixed(&opts, ROW);
    let mut bar = Flex::default().row();
    bar.set_pad(8);
    cancel_go(&mut bar, &answer, f.go);
    bar.end();
    col.fixed(&bar, ROW + 4);
    col.end();
    win.end();
    // As tall as the fields shown.
    let fit = {
        let (resize, answers, col) = (win.resizer(), answers.clone(), col.clone());
        move || {
            let mut col = col.clone();
            let rows = 1 + answers.shown() as i32 + 1;
            let h = 4 + rows * ROW + (ROW + 4) + rows * 8 + 8;
            col.resize(PAD, 4, W - PAD * 2, h - 12);
            col.layout();
            resize(h);
        }
    };
    fit();
    answers.on_change(fit.clone());
    // YouTube takes four answers, and has no duration (it runs until it's ended).
    let on = {
        let chips = chips.clone();
        move || match chips.as_slice() {
            [t, y] => [t.value() && t.active(), y.value() && y.active()],
            _ => [true, false],
        }
    };
    let follow = {
        let (answers, on, most) = (answers.clone(), on.clone(), f.most);
        let (timer, lasts) = (timer.clone(), lasts.clone());
        move || {
            let (mut timer, mut lasts) = (timer.clone(), lasts.clone());
            let [twitch, youtube] = on();
            answers.set_max(if youtube { 4 } else { most });
            for w in [&mut timer as &mut dyn WidgetExt, &mut lasts as &mut dyn WidgetExt] {
                if twitch {
                    w.activate();
                } else {
                    w.deactivate();
                }
            }
        }
    };
    follow();
    for c in &chips {
        let follow = follow.clone();
        c.clone().set_callback(move |_| follow());
    }
    let ok = dialogs::run(&mut win, &answer, || {
        match check(&question.value(), &answers.values(), on()) {
            Some(why) => {
                dialogs::alert(why);
                false
            }
            None => true,
        }
    });
    let _ = question.take_focus();
    ok.then(|| NewPoll {
        question: question.value().trim().to_string(),
        choices: answers.values(),
        secs: LASTS[lasts.value().clamp(0, 5) as usize],
        on: on(),
    })
}

/// Asks for a new poll on the platforms that are `ready`.
pub(super) fn new_poll(ready: [bool; 2]) -> Option<NewPoll> {
    ask_form(
        Form {
            title: tr("btn.new_poll"),
            go: tr("btn.start_poll"),
            question: tr("polls.ask_hint"),
            answer: |n| trf("polls.choice_n", &[("n", &n.to_string())]),
            most: 5,
            lasts: (tr("polls.lasts"), 1),
            ready: Some(ready),
        },
        |q, choices, on| {
            if q.trim().is_empty() || choices.len() < 2 {
                Some(tr("polls.need_two"))
            } else if !on.iter().any(|on| *on) {
                Some(tr("polls.pick_where"))
            } else if on[1] && choices.len() > 4 {
                Some(tr("polls.youtube_limit"))
            } else {
                None
            }
        },
    )
}

/// Asks for a new prediction: its question, its outcomes and how long people can bet.
pub(super) fn new_prediction() -> Option<(String, Vec<String>, u64)> {
    ask_form(
        Form {
            title: tr("btn.new_prediction"),
            go: tr("btn.start_prediction"),
            question: tr("polls.predict_hint"),
            answer: |n| trf("polls.outcome_n", &[("n", &n.to_string())]),
            most: 10,
            lasts: (tr("polls.voting_time"), 0),
            ready: None,
        },
        |q, outcomes, _| (q.trim().is_empty() || outcomes.len() < 2).then_some(tr("polls.need_two")),
    )
    .map(|p| (p.question, p.choices, p.secs))
}

pub fn tab(tabs: &Wizard, cmds: &Cmds) -> Rc<RefCell<Polls>> {
    let (scroll, mut col) = super::scroll::page(tabs, "Polls");

    super::hint(&mut col, tr("polls.hint"), 36);
    // How the accounts are, and how the last thing asked went.
    let note = Frame::default()
        .with_align(Align::Left | Align::Inside | Align::Clip)
        .ellipsis();
    col.fixed(&note, ROW);

    // What's running: each list shows it, with what can be done to it below.
    section(&mut col, "vote", tr("polls.poll"));
    let poll_results = browser(
        &mut col,
        &[90, 220, 220, 120],
        &["polls.where", "polls.question", "polls.choice", "polls.votes"],
        "polls.no_poll",
    );
    let pb = bar(&mut col, &[tr("btn.new_poll"), tr("btn.end_poll"), tr("btn.refresh")]);
    poll_results.clone().set_menu(&pb);

    section(&mut col, "trophy", tr("polls.prediction"));
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
    let db = bar(
        &mut col,
        &[
            tr("btn.new_prediction"),
            tr("btn.lock"),
            tr("btn.resolve"),
            tr("btn.cancel_prediction"),
        ],
    );
    pred_results.clone().set_menu(&db);
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
        poll_buttons: [pb[0].clone(), pb[1].clone(), pb[2].clone()],
        prediction_buttons: [db[0].clone(), db[1].clone(), db[2].clone(), db[3].clone()],
    }));
    me.borrow_mut().refresh_note();
    let go = {
        let (cmds, seen) = (cmds.clone(), seen.clone());
        move |steps: Vec<(u64, Value, String)>| ask(&cmds, &seen, Some(steps))
    };
    let on = |b: &Button, f: Box<dyn Fn()>| b.clone().set_callback(move |_| f());
    let snapshot = {
        let seen = seen.clone();
        move || seen.lock().map(|s| s.clone()).unwrap_or_default()
    };

    on(&pb[0], {
        let (go, snapshot) = (go.clone(), snapshot.clone());
        Box::new(move || {
            let Some(p) = new_poll(snapshot().ready) else {
                return;
            };
            let mut steps = Vec::new();
            if p.on[0] {
                steps.push((
                    561,
                    json!({"title": p.question, "choices": p.choices, "duration": p.secs}),
                    trf("polls.poll_on", &[("where", "Twitch")]),
                ));
            }
            if p.on[1] {
                // YouTube takes up to 4 options, and runs until it's ended.
                steps.push((
                    4011,
                    json!({"question": p.question, "options": p.choices, "broadcast": ""}),
                    trf("polls.poll_on", &[("where", "YouTube")]),
                ));
            }
            go(steps);
        })
    });
    on(&pb[1], {
        let (go, snapshot) = (go.clone(), snapshot.clone());
        Box::new(move || {
            // Ends what is running, where it can.
            let seen = snapshot();
            let mut steps = Vec::new();
            if seen.ready[0] && seen.twitch_poll.get("active").and_then(Value::as_bool) == Some(true) {
                steps.push((562, json!({}), trf("polls.end_on", &[("where", "Twitch")])));
            }
            if seen.ready[1] && !s(&seen.youtube_poll, "poll.question").is_empty() {
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
        let go = go.clone();
        Box::new(move || {
            if let Some((q, o, secs)) = new_prediction() {
                go(vec![(
                    568,
                    json!({"title": q, "outcomes": o, "predictionWindow": secs}),
                    tr("polls.prediction_new").into(),
                )]);
            }
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
    fn poll_actions_require_an_available_account_and_known_prediction_state() {
        assert_eq!(
            action_availability([false; 2], true, true, "ACTIVE", true),
            ([false, false, true], [false; 4])
        );
        // New poll on any ready platform; End only where a poll runs.
        assert_eq!(
            action_availability([true, false], false, false, "", true).0,
            [true, false, true]
        );
        assert_eq!(
            action_availability([true, false], true, false, "", true).0,
            [true, true, true]
        );
        assert_eq!(
            action_availability([false, true], true, false, "", true).0,
            [true, false, true]
        );
        assert_eq!(
            action_availability([false, true], false, true, "", true).0,
            [true, true, true]
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
        assert!(matches!(note(&seen).0, crate::ui::Health::Busy));
        seen.received = 1;
        assert!(matches!(note(&seen).0, crate::ui::Health::Off));
        seen.ready = [true, true];
        assert_eq!(note(&seen).1, trf("polls.ready_on", &[("where", "Twitch, YouTube")]));
        seen.note = "x".into();
        seen.trouble = true;
        assert!(matches!(note(&seen).0, crate::ui::Health::Down));
    }

    #[test]
    fn reading_someone_elses_channel_says_why_twitch_is_off() {
        let mut seen = Seen {
            received: 1,
            guest: Some(("Me".into(), "friend".into())),
            ..Seen::default()
        };
        let (health, text) = note(&seen);
        assert!(matches!(health, crate::ui::Health::Off));
        assert!(text.contains("Me") && text.contains("friend"), "{text}");
        seen.ready = [false, true];
        let (health, text) = note(&seen);
        assert!(matches!(health, crate::ui::Health::Up));
        assert!(text.contains("YouTube") && text.contains("friend"), "{text}");
        assert!(twitch_why(&seen).contains("friend"));
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
