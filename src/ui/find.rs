//! Finding a channel: a window with a search box and the channels that match, with their picture,
//! followers and whether they're live, as the platforms' own sites show them. A link or @name pasted in
//! the box works too (for what can't be searched).

use super::dialogs::{ok_cancel, window};
use super::rows::Rows;
use super::{img, nested_wait, tell};
use crate::engine::UiCmd;
use crate::i18n::{tr, trf};
use crate::search::{self, Found};
use fltk::button::Button;
use fltk::enums::{Align, CallbackTrigger};
use fltk::frame::Frame;
use fltk::group::Flex;
use fltk::input::Input;
use fltk::prelude::*;
use fltk::app;
use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::time::{Duration, Instant};
use tokio::sync::mpsc::UnboundedSender;

type Cmds = UnboundedSender<UiCmd>;

/// How long typing rests before the search is asked.
const REST: Duration = Duration::from_millis(350);
/// The pictures' height in the list.
const PIC: i32 = 22;

fn line(f: &Found) -> String {
    let clean = |s: &str| s.replace(['\t', '\n'], " ");
    format!(
        "{}\t{}\t{}\t{}",
        f.picture,
        clean(&f.name),
        clean(&f.detail),
        if f.live { tr("find.live") } else { "" }
    )
}

/// Asks for a channel on `platform` (a slug like "twitch"): its handle to put in the channel box, "" for
/// the account's own channel, None if cancelled.
pub(super) fn channel(cmds: &Cmds, platform: &str) -> Option<String> {
    let name = img::platform_name(platform);
    let (mut win, answer) = window(520, 440, &trf("find.title", &[("platform", name)]));
    let mut col = Flex::new(10, 10, 500, 420, None).column();
    col.set_pad(6);
    let mut input = Input::default();
    super::placeholder(&mut input, tr("find.placeholder"));
    input.set_trigger(CallbackTrigger::Changed);
    col.fixed(&input, super::ROW);
    let mut list = Rows::new(tr("find.empty"));
    list.set_column_widths(&[40, 250, 100, 60]);
    list.set_pic_column(0);
    let mut status = Frame::default().with_align(Align::Left | Align::Inside);
    status.set_label_color(super::muted());
    col.fixed(&status, 18);
    let mut bar = Flex::default().row();
    bar.set_pad(8);
    let mut own = Button::default().with_label(tr("find.own"));
    super::with_icon(&mut own);
    bar.fixed(&own, super::fit::grown(170, tr("find.own"), false));
    ok_cancel(&mut bar, &answer, true);
    bar.end();
    col.fixed(&bar, super::ROW + 4);
    col.end();
    win.end();
    win.resizable(&col);

    let searchable = search::can(platform);
    if !searchable {
        status.set_label(&trf("find.no_search", &[("platform", name)]).replace('@', "@@"));
    }
    let found: Rc<RefCell<Vec<Found>>> = Rc::default();
    let edited: Rc<Cell<Option<Instant>>> = Rc::default();
    input.set_callback({
        let edited = edited.clone();
        move |_| edited.set(Some(Instant::now()))
    });
    let use_own = Rc::new(Cell::new(false));
    own.set_callback({
        let (answer, use_own) = (answer.clone(), use_own.clone());
        move |_| {
            use_own.set(true);
            answer.set(Some(true));
        }
    });
    list.set_callback({
        let answer = answer.clone();
        move |_| {
            if app::event_clicks() {
                answer.set(Some(true));
            }
        }
    });

    let (tx, rx) = std::sync::mpsc::channel::<(u64, Vec<Found>)>();
    let asked = Cell::new(0u64);
    win.show();
    let _ = input.take_focus();
    while win.shown() && answer.get().is_none() {
        nested_wait(Some(0.15));
        if searchable && edited.get().is_some_and(|t| t.elapsed() >= REST) {
            edited.set(None);
            let q = input.value().trim().to_string();
            asked.set(asked.get() + 1);
            if q.is_empty() {
                found.borrow_mut().clear();
                list.clear();
                status.set_label("");
            } else {
                status.set_label(tr("find.searching"));
                let (n, p, tx) = (asked.get(), platform.to_string(), tx.clone());
                tell(cmds, move |_| {
                    tokio::task::spawn_local(async move {
                        let _ = tx.send((n, search::channels(&p, &q).await));
                        app::awake();
                    });
                });
            }
        }
        while let Ok((n, got)) = rx.try_recv() {
            if n != asked.get() {
                continue;
            }
            list.clear();
            for f in &got {
                list.add(&line(f));
            }
            if !got.is_empty() {
                list.select(1);
            }
            status.set_label(if got.is_empty() { tr("find.none") } else { "" });
            *found.borrow_mut() = got;
        }
        // Pictures come as they're fetched.
        for f in found.borrow().iter().filter(|f| !f.picture.is_empty()) {
            if !list.has_pic(&f.picture) {
                if let Some(p) = img::get(cmds, &f.picture, PIC, true) {
                    list.set_pic(&f.picture, p.first());
                }
            }
        }
    }
    let ok = answer.get() == Some(true);
    win.hide();
    if !ok {
        return None;
    }
    if use_own.get() {
        return Some(String::new());
    }
    let picked = usize::try_from(list.value() - 1)
        .ok()
        .and_then(|i| found.borrow().get(i).map(|f| f.handle.clone()));
    // Without a pick, what was typed (a link or @name).
    picked.or_else(|| Some(input.value().trim().to_string()).filter(|t| !t.is_empty()))
}
