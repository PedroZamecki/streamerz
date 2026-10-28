//! The connections set up (platforms, stream apps, donation services): the status bar's "3/5
//! connected", and on a click each one's state (connected, standby, connecting, or not and why, from its
//! last log line), with Retry and a way to its settings.

use super::fit::Ellipsis;
use super::{button, img, query, tell, ROW};
use crate::i18n::{tr, trf};
use crate::Config;
use fltk::app;
use fltk::enums::{Align, Color, Font};
use fltk::frame::Frame;
use fltk::group::Flex;
use fltk::prelude::*;
use fltk::window::Window;
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::{mpsc::UnboundedSender, watch};

type Cfg = Arc<watch::Sender<Config>>;
type Cmds = UnboundedSender<crate::engine::UiCmd>;

/// How long a Retry shows as connecting before the log says otherwise.
const TRYING: Duration = Duration::from_secs(20);

#[derive(Clone)]
enum Retry {
    Platform(usize),
    Obs(String),
    Slobs(String),
    Donations,
}

struct Conn {
    name: String,
    logo: &'static str,
    up: bool,
    standby: bool,
    /// The tab with its settings.
    tab: &'static str,
    retry: Retry,
    /// What its log lines start with.
    log: Vec<String>,
}

/// Every connection that's set up, with whether it's up or standing by.
fn list(cfg: &Cfg, cmds: &Cmds) -> Vec<Conn> {
    let (obs, slobs, se, sl, dd) = {
        let c = cfg.borrow();
        let ids = |l: &[crate::model::Obj]| -> Vec<(String, String)> {
            l.iter()
                .map(|o| {
                    let s = |k: &str| o.get(k).and_then(|v| v.as_str()).unwrap_or("").to_string();
                    (s("id"), s("name"))
                })
                .collect()
        };
        let d = &c.donations;
        (
            ids(&c.obs.connections),
            ids(&c.slobs.connections),
            !d.streamelements_token.trim().is_empty(),
            !d.streamlabs_token.trim().is_empty(),
            !d.donordrive_participant_id.trim().is_empty(),
        )
    };
    let set_up = {
        let c = cfg.borrow();
        [
            c.twitch.broadcaster.is_some(),
            c.youtube.broadcaster.is_some() || !c.youtube.channel_id.is_empty(),
            c.kick.broadcaster.is_some() || !c.kick.channel.is_empty(),
            c.tiktok.broadcaster.is_some() || !c.tiktok.channel.is_empty(),
        ]
    };
    let (o, s) = (obs.clone(), slobs.clone());
    let (plats, plats_standby, obs_up, slobs_up, se_up, sl_up) = query(cmds, move |e| {
        let d = e.donations.borrow();
        (
            [
                e.twitch_connected(),
                e.youtube_connected(),
                e.kick_connected(),
                e.tiktok_connected(),
            ],
            [false, e.youtube_standby(), false, e.tiktok_standby()],
            o.iter().map(|(id, _)| e.obs_connected(Some(id))).collect::<Vec<_>>(),
            s.iter().map(|(id, _)| e.slobs_up(id)).collect::<Vec<_>>(),
            d.streamelements.get(),
            d.streamlabs.get(),
        )
    })
    .unwrap_or_default();

    let mut out = Vec::new();
    for (i, name) in ["Twitch", "YouTube", "Kick", "TikTok"].into_iter().enumerate() {
        if set_up[i] {
            out.push(Conn {
                name: name.into(),
                logo: name,
                up: plats[i],
                standby: plats_standby[i],
                tab: "Accounts",
                retry: Retry::Platform(i),
                log: vec![format!("{name}:"), format!("{name} ("), format!("{name} EventSub")],
            });
        }
    }
    for (k, (id, name)) in obs.into_iter().enumerate() {
        out.push(Conn {
            log: vec![format!("OBS '{name}':")],
            name: format!("OBS: {name}"),
            logo: "OBS Studio",
            up: obs_up.get(k).copied().unwrap_or(false),
            standby: false,
            tab: "Stream Apps",
            retry: Retry::Obs(id),
        });
    }
    for (k, (id, name)) in slobs.into_iter().enumerate() {
        out.push(Conn {
            log: vec![format!("Streamlabs Desktop {name}:"), "Streamlabs Desktop:".into()],
            name: format!("Streamlabs Desktop: {name}"),
            logo: "Streamlabs Desktop",
            up: slobs_up.get(k).copied().unwrap_or(false),
            standby: false,
            tab: "Stream Apps",
            retry: Retry::Slobs(id),
        });
    }
    for (on, up, name) in [(se, se_up, "StreamElements"), (sl, sl_up, "Streamlabs")] {
        if on {
            out.push(Conn {
                log: vec![format!("{name}:")],
                name: name.into(),
                logo: name,
                up,
                standby: false,
                tab: "Donations",
                retry: Retry::Donations,
            });
        }
    }
    if dd {
        // Polled, not connected: up once it's watching.
        let up = last_line(&crate::log_tail(200), &["DonorDrive:".into()])
            .is_some_and(|l| l.starts_with("DonorDrive: watching"));
        out.push(Conn {
            name: "DonorDrive".into(),
            logo: "DonorDrive",
            up,
            standby: false,
            tab: "Donations",
            retry: Retry::Donations,
            log: vec!["DonorDrive:".into()],
        });
    }
    out
}

fn last_line<'a>(log: &'a [String], prefixes: &[String]) -> Option<&'a String> {
    log.iter()
        .rev()
        .find(|l| prefixes.iter().any(|p| l.starts_with(p.as_str())))
}

/// "3/5 connected" (plus standby count or keyring warning) for the status bar.
pub fn summary(cfg: &Cfg, cmds: &Cmds) -> (String, super::Health) {
    use super::Health as H;
    if crate::secrets::is_locked() {
        return (tr("conns.keyring_locked").into(), H::Locked);
    }
    let l = list(cfg, cmds);
    let up = l.iter().filter(|c| c.up).count();
    let sb = l.iter().filter(|c| !c.up && c.standby).count();
    if l.is_empty() {
        return (tr("conns.nothing_set_up").into(), H::Off);
    }
    let all = match (up, sb) {
        (0, 0) => H::Off,
        (n, s) if n + s == l.len() && n > 0 => H::Up,
        _ => H::Busy,
    };
    let text = if sb > 0 {
        trf(
            "conns.summary_standby",
            &[
                ("up", &up.to_string()),
                ("total", &l.len().to_string()),
                ("standby", &sb.to_string()),
            ],
        )
    } else {
        trf(
            "conns.summary",
            &[("up", &up.to_string()), ("total", &l.len().to_string())],
        )
    };
    (text, all)
}

enum State {
    Up,
    Standby(String),
    Connecting,
    Down(String),
}

/// Its state from the log (`log`, the last lines; `count`, how many were ever logged), and since a
/// Retry (when, and the count then): connecting until it says otherwise, for up to TRYING.
fn state(c: &Conn, log: &[String], count: u64, retried: Option<&(Instant, u64)>) -> State {
    if c.up {
        return State::Up;
    }
    if let Some((at, then)) = retried.filter(|r| r.0.elapsed() < TRYING) {
        let since = (count - then).min(log.len() as u64) as usize;
        let new = &log[log.len() - since..];
        match last_line(new, &c.log) {
            Some(l) if !l.to_lowercase().contains("connecting") && !l.to_lowercase().contains("checking") => {}
            _ if at.elapsed() < TRYING => return State::Connecting,
            _ => {}
        }
    }
    if c.standby {
        let detail = last_line(log, &c.log)
            .and_then(|l| l.split_once(": ").map(|x| x.1))
            .unwrap_or("standby, connects when the stream starts");
        return State::Standby(detail.to_string());
    }
    match last_line(log, &c.log) {
        Some(l) if l.to_lowercase().contains("connecting") => State::Connecting,
        Some(l) => {
            // Without the name it starts with.
            let why = l.split_once(": ").map_or(l.as_str(), |x| x.1);
            State::Down(why.to_string())
        }
        None => State::Down(String::new()),
    }
}

fn retry(cmds: &Cmds, r: &Retry) {
    match r.clone() {
        Retry::Platform(i) => super::accounts::connect(cmds, i),
        Retry::Obs(id) => tell(cmds, move |e| e.obs_start().connect(Some(&id))),
        Retry::Slobs(id) => tell(cmds, move |e| e.slobs_connect(&id)),
        Retry::Donations => tell(cmds, |e| e.donations_retry()),
    }
}

/// The connections window: one row each, kept up to date (once a second, while it's open).
pub fn popup(cfg: &Cfg, cmds: &Cmds, go: impl Fn(&str) + 'static, status_bar: impl Fn() + 'static) {
    let conns = list(cfg, cmds);
    let extra_lock = i32::from(crate::secrets::is_locked()) * (ROW + 8);
    let rows_h = conns.len().max(1) as i32 * (ROW + 8) + extra_lock;
    let mut win = Window::default()
        .with_size(780, rows_h + 60)
        .with_label(tr("conns.title"));
    win.set_xclass("streamerz");
    let mut col = Flex::new(10, 10, 760, rows_h + 40, None).column();
    col.set_pad(8);

    if crate::secrets::is_locked() {
        let mut lrow = Flex::default().row();
        lrow.set_pad(8);
        let mut msg = Frame::default()
            .with_label(tr("conns.keyring_hint"))
            .with_align(Align::Left | Align::Inside | Align::Clip)
            .ellipsis();
        msg.set_label_color(Color::from_rgb(0xe0, 0xa8, 0x30));
        let mut ubtn = button(&mut lrow, tr("ui.unlock_now"), 110);
        lrow.end();
        col.fixed(&lrow, ROW);
        let cmds = cmds.clone();
        let mut win_c = win.clone();
        ubtn.set_callback(move |_| {
            super::accounts::unlock_logins_interactive(&cmds);
            if !crate::secrets::is_locked() {
                win_c.hide();
            }
        });
    }

    if conns.is_empty() {
        let f = Frame::default()
            .with_label(tr("conns.empty_hint"))
            .with_align(Align::Left | Align::Inside)
            .ellipsis();
        col.fixed(&f, ROW);
    }
    let retried: Rc<RefCell<HashMap<String, (Instant, u64)>>> = Rc::default();
    let go = Rc::new(go);
    let mut states = Vec::new();
    for c in &conns {
        let mut row = Flex::default().row();
        row.set_pad(8);
        let mut logo = Frame::default();
        logo.set_image(img::logo(c.logo, 18));
        row.fixed(&logo, 22);
        let mut name = Frame::default()
            .with_label(&c.name.replace('@', "@@"))
            .with_align(Align::Left | Align::Inside | Align::Clip)
            .ellipsis();
        name.set_label_font(Font::HelveticaBold);
        row.fixed(&name, 190);
        let st = Frame::default().with_align(Align::Left | Align::Inside | Align::Clip);
        let mut r = button(&mut row, if c.standby { tr("btn.check") } else { tr("btn.retry") }, 70);
        r.set_tooltip(tr("conns.retry_tooltip"));
        let mut s = button(&mut row, tr("btn.settings"), 80);
        row.end();
        col.fixed(&row, ROW);
        r.set_callback({
            let (cmds, retried, why, name) = (cmds.clone(), retried.clone(), c.retry.clone(), c.name.clone());
            let mut st = st.clone();
            move |_| {
                retried
                    .borrow_mut()
                    .insert(name.clone(), (Instant::now(), crate::log_count()));
                retry(&cmds, &why);
                // Shown at once; the next second says how it went.
                super::Health::Busy.show(&mut st, tr("conns.checking"));
                st.redraw();
            }
        });
        s.set_callback({
            let (go, tab) = (go.clone(), c.tab);
            move |b| {
                go(tab);
                if let Some(mut w) = b.window() {
                    w.hide();
                }
            }
        });
        states.push((st, r));
    }
    let mut bottom = Flex::default().row();
    bottom.set_pad(8);
    Frame::default();
    let mut close = button(&mut bottom, tr("dialogs.close"), 80);
    bottom.end();
    col.fixed(&bottom, ROW);
    col.end();
    win.end();
    close.set_callback({
        let mut win = win.clone();
        move |_| win.hide()
    });
    win.show();

    let (cfg, cmds) = (cfg.clone(), cmds.clone());
    let mut show = move |win: &Window| {
        let now = list(&cfg, &cmds);
        let (count, log) = (crate::log_count(), crate::log_tail(200));
        for (c, (st, r)) in now.iter().zip(states.iter_mut()) {
            use super::Health as H;
            let (h, text) = match state(c, &log, count, retried.borrow().get(&c.name)) {
                State::Up => (H::Up, tr("conns.connected").to_string()),
                State::Standby(why) => (H::Standby, why),
                State::Connecting => (H::Busy, tr("conns.connecting").to_string()),
                State::Down(why) if why.is_empty() => (H::Off, tr("conns.off").to_string()),
                // Retried and failed again: says so, so the click shows it did something.
                State::Down(why) if retried.borrow().contains_key(&c.name) => {
                    (H::Down, trf("conns.again", &[("reason", &why)]))
                }
                State::Down(why) => (H::Down, why),
            };
            if c.up {
                retried.borrow_mut().remove(&c.name);
                r.deactivate();
            } else {
                r.set_label(if c.standby { tr("btn.check") } else { tr("btn.retry") });
                super::with_icon(r);
                r.activate();
            }
            h.show(st, &text);
            st.set_tooltip(&text.replace('@', "@@"));
        }
        status_bar();
        win.clone().redraw();
    };
    show(&win);
    app::add_timeout3(1.0, move |h| {
        if win.shown() {
            show(&win);
            app::repeat_timeout3(1.0, h);
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn state_from_the_log_and_retry() {
        let c = Conn {
            name: "OBS: Main".into(),
            logo: "OBS Studio",
            up: false,
            standby: false,
            tab: "Stream Apps",
            retry: Retry::Donations,
            log: vec!["OBS 'Main':".into()],
        };
        let log: Vec<String> = ["OBS 'Main': failed: refused", "Twitch: x"].map(String::from).to_vec();
        assert!(matches!(state(&c, &log, 2, None), State::Down(w) if w == "failed: refused"));
        // Retried after those two lines: connecting until a new line about it.
        let r = (Instant::now(), 2);
        assert!(matches!(state(&c, &log, 2, Some(&r)), State::Connecting));
        let mut log = log;
        log.push("OBS 'Main': connecting to ws://x...".into());
        assert!(matches!(state(&c, &log, 3, Some(&r)), State::Connecting));
        log.push("OBS 'Main': failed: again".into());
        assert!(matches!(state(&c, &log, 4, Some(&r)), State::Down(w) if w == "failed: again"));
        assert!(matches!(
            state(&Conn { up: true, ..c.clone() }, &log, 4, None),
            State::Up
        ));
        assert!(matches!(
            state(&Conn { standby: true, ..c }, &log, 4, None),
            State::Standby(_)
        ));
    }

    impl Clone for Conn {
        fn clone(&self) -> Self {
            Conn {
                name: self.name.clone(),
                logo: self.logo,
                up: self.up,
                standby: self.standby,
                tab: self.tab,
                retry: self.retry.clone(),
                log: self.log.clone(),
            }
        }
    }
}
