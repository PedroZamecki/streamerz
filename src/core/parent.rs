//! Backend lifecycle. The renderer can come and go without stopping integrations or MCP.

use crate::engine::{Engine, UiCmd};
use crate::host::{Event, Message};
use std::collections::VecDeque;
use std::rc::Rc;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::mpsc;
use tokio::task::JoinHandle;

type Renderer = JoinHandle<std::io::Result<super::frontend::Exit>>;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum RecoveryAction {
    Respawn,
    Stop,
}

/// Bounds automatic UI restarts after crashes so a tight crash loop cannot spin forever.
#[derive(Default)]
pub(crate) struct CrashRecovery {
    crashes: VecDeque<Instant>,
}

impl CrashRecovery {
    const MAX_AUTO_RESTORES: usize = 2;
    const WINDOW: Duration = Duration::from_secs(60);

    pub(crate) fn record_crash(&mut self, now: Instant) -> RecoveryAction {
        while let Some(&oldest) = self.crashes.front() {
            if now.saturating_duration_since(oldest) > Self::WINDOW {
                self.crashes.pop_front();
            } else {
                break;
            }
        }
        self.crashes.push_back(now);
        if self.crashes.len() <= Self::MAX_AUTO_RESTORES {
            RecoveryAction::Respawn
        } else {
            RecoveryAction::Stop
        }
    }

    pub(crate) fn reset(&mut self) {
        self.crashes.clear();
    }
}

fn open(
    engine: Rc<Engine>,
    changes: Arc<super::frontend::Changes>,
    requests: mpsc::Sender<super::requests::Call>,
    recovered_crash: Option<serde_json::Value>,
) -> Option<Renderer> {
    match super::frontend::Frontend::spawn() {
        Ok(frontend) => {
            crate::host::set_attached(true);
            Some(tokio::task::spawn_local(frontend.serve(
                engine,
                changes,
                requests,
                recovered_crash,
            )))
        }
        Err(error) => {
            crate::error::record_active(
                "frontend.launch",
                crate::error::StreamerzError::frontend_launch(format!("Can't open Streamerz: {error}")),
            );
            None
        }
    }
}

pub async fn run(
    engine: Rc<Engine>,
    commands: &mut mpsc::UnboundedReceiver<UiCmd>,
    messages: &mut mpsc::UnboundedReceiver<Message>,
    sender: mpsc::UnboundedSender<Message>,
) {
    let (requests_tx, mut requests_rx) = mpsc::channel(64);
    let mut requests = super::requests::Requests::default();
    let changes = Arc::new(super::frontend::Changes::default());
    let changed = changes.clone();
    crate::host::subscribe(move |event| {
        changed.mark(event);
        let message = match event {
            Event::Input => Some(Message::Input),
            Event::Toggle => Some(Message::Toggle),
            Event::Show => Some(Message::Show),
            Event::Reopen => Some(Message::Reopen),
            Event::Restart => Some(Message::Restart),
            Event::Quit => Some(Message::Quit),
            _ => None,
        };
        if let Some(message) = message {
            let _ = sender.send(message);
        }
    });
    let cli_hidden = std::env::args_os().any(|a| a == "--hidden");
    let show = (!engine.config().start_hidden && !cli_hidden) || !super::prompts::list().is_empty();
    let mut renderer = if show {
        to_front(&changes);
        open(engine.clone(), changes.clone(), requests_tx.clone(), None)
    } else {
        // Hidden from the start: once, after startup settles, let the kernel take back the idle code pages.
        tokio::spawn(async {
            tokio::time::sleep(std::time::Duration::from_secs(8)).await;
            crate::release_code_pages();
        });
        None
    };
    let mut recovery = CrashRecovery::default();
    loop {
        tokio::select! {
            Some(call) = requests_rx.recv() => requests.push(engine.clone(), call),
            Some((value, reply)) = requests.next(), if !requests.is_empty() => { let _ = reply.send(value); }

            command = commands.recv() => {
                let Some(command) = command else { break };
                command(&engine);
            }
            message = messages.recv() => {
                match message {
                    Some(Message::Toggle) => {
                        if let Some(task) = renderer.take() { task.abort(); let _ = task.await; crate::host::detach(); }
                        else { recovery.reset(); to_front(&changes); renderer = open(engine.clone(), changes.clone(), requests_tx.clone(), None); }
                    }
                    Some(Message::Input) => {
                        // Logins that ran out: each becomes a question in the window (a prompt).
                        let relogins = crate::host::RELOGINS.lock().map(|mut queue| std::mem::take(&mut *queue)).unwrap_or_default();
                        for (platform, bot) in relogins { super::accounts::ask_relogin(&engine, platform, bot); }
                        let pending = !super::prompts::list().is_empty() || crate::host::RELOGINS.lock().is_ok_and(|items| !items.is_empty());
                        if pending && renderer.is_none() { renderer = open(engine.clone(), changes.clone(), requests_tx.clone(), None); }
                    }
                    Some(Message::NoTray) => {
                        if renderer.is_none() { renderer = open(engine.clone(), changes.clone(), requests_tx.clone(), None); }
                    }
                    Some(Message::Show) => {
                        to_front(&changes);
                        if renderer.is_none() { recovery.reset(); renderer = open(engine.clone(), changes.clone(), requests_tx.clone(), None); }
                    }
                    Some(Message::Reopen) => {
                        if let Some(task) = renderer.take() {
                            task.abort();
                            let _ = task.await;
                            crate::host::detach();
                            recovery.reset();
                            to_front(&changes);
                            renderer = open(engine.clone(), changes.clone(), requests_tx.clone(), None);
                        }
                    }
                    Some(Message::Restart) => {
                        if let Some(task) = renderer.take() { task.abort(); let _ = task.await; crate::host::detach(); }
                        crate::update::restart();
                    }
                    Some(Message::Quit) | None => break,
                    _ => {}
                }
            }
            result = async {
                match renderer.as_mut() {
                    Some(task) => task.await,
                    None => std::future::pending().await,
                }
            } => {
                renderer = None;
                crate::host::detach();
                let crash = match result {
                    Ok(Ok(super::frontend::Exit::Clean)) => None,
                    Ok(Ok(super::frontend::Exit::Crashed(info))) => Some(info),
                    // Its window never opened: the GPU couldn't show it. Once, it is drawn in software.
                    Ok(Ok(super::frontend::Exit::NoWindow(info))) if super::frontend::use_software(engine.config().software_render) => {
                        let reason = info.map(|info| format!(" ({})", info.summary.trim())).unwrap_or_default();
                        crate::warn(format!("The GPU couldn't show the window{reason}; drawing it in software"));
                        renderer = open(engine.clone(), changes.clone(), requests_tx.clone(), None);
                        None
                    }
                    Ok(Ok(super::frontend::Exit::NoWindow(info))) => Some(super::frontend::report(info)),
                    Ok(Err(error)) => {
                        let msg = error.to_string();
                        crate::report_error(&crate::error::StreamerzError::frontend_crash(format!("Streamerz frontend: {msg}")));
                        let path = crate::crash::record_ui_unclean(&msg);
                        Some(super::frontend::CrashInfo {
                            summary: msg,
                            report_path: Some(path),
                            panic_recorded: false,
                        })
                    }
                    Err(error) => {
                        crate::report_error(&crate::error::StreamerzError::frontend_crash(format!("Streamerz frontend stopped: {error}")));
                        None
                    }
                };
                if let Some(info) = crash {
                    let summary = if info.summary.trim().is_empty() {
                        "unknown error".to_owned()
                    } else {
                        info.summary.clone()
                    };
                    match recovery.record_crash(Instant::now()) {
                        RecoveryAction::Respawn => {
                            crate::warn(format!("UI closed unexpectedly ({summary}); restoring window"));
                            crate::notify::critical(
                                crate::i18n::tr("crash.ui_title"),
                                &crate::i18n::trf("crash.ui_restored_notify", &[("summary", &summary)]),
                            );
                            let mut payload = serde_json::json!({"summary": summary});
                            if let Some(path) = &info.report_path {
                                payload["path"] = serde_json::json!(path.display().to_string());
                            }
                            to_front(&changes);
                            renderer = open(engine.clone(), changes.clone(), requests_tx.clone(), Some(payload));
                            if renderer.is_none() {
                                crate::notify::critical(
                                    crate::i18n::tr("crash.ui_title"),
                                    &crate::i18n::trf("crash.ui_stopped_notify", &[("summary", &summary)]),
                                );
                            }
                        }
                        RecoveryAction::Stop => {
                            crate::error::record_active(
                                "frontend.crash_loop",
                                crate::error::StreamerzError::frontend_crash(format!(
                                    "UI crashed repeatedly ({summary}); auto-restore paused while backend keeps running"
                                )),
                            );
                            crate::notify::critical(
                                crate::i18n::tr("crash.ui_title"),
                                &crate::i18n::trf("crash.ui_stopped_notify", &[("summary", &summary)]),
                            );
                        }
                    }
                }
                crate::trim_memory();
                crate::release_code_pages();
            }
        }
    }
    if let Some(task) = renderer {
        task.abort();
        let _ = task.await;
    }
    engine.save_shutdown();
    crate::crash::quit(0);
}

/// The user asked for the window (a start, the tray, another start): it comes to the front. A window
/// opened for a question (a Get Input) doesn't, so it never takes the focus from a game. Windows lets
/// only the process the user acted on take the front, so this one hands that on to the window's.
fn to_front(changes: &super::frontend::Changes) {
    #[cfg(windows)]
    // SAFETY: no pointers; ASFW_ANY lets the window's process take the front.
    unsafe {
        windows_sys::Win32::UI::WindowsAndMessaging::AllowSetForegroundWindow(
            windows_sys::Win32::UI::WindowsAndMessaging::ASFW_ANY,
        );
    }
    changes.mark(crate::host::Event::Show);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crash_recovery_respawns_twice_then_stops_until_window_expires_or_reset() {
        let mut rec = CrashRecovery::default();
        let t0 = Instant::now();
        assert_eq!(rec.record_crash(t0), RecoveryAction::Respawn);
        assert_eq!(rec.record_crash(t0 + Duration::from_secs(5)), RecoveryAction::Respawn);
        assert_eq!(rec.record_crash(t0 + Duration::from_secs(10)), RecoveryAction::Stop);

        // A crash after the 60s sliding window rolls the earlier crashes out.
        let t_later = t0 + Duration::from_secs(75);
        assert_eq!(rec.record_crash(t_later), RecoveryAction::Respawn);
        assert_eq!(
            rec.record_crash(t_later + Duration::from_secs(5)),
            RecoveryAction::Respawn
        );
        assert_eq!(
            rec.record_crash(t_later + Duration::from_secs(10)),
            RecoveryAction::Stop
        );

        // Manual reopen resets the budget immediately.
        rec.reset();
        assert_eq!(
            rec.record_crash(t_later + Duration::from_secs(11)),
            RecoveryAction::Respawn
        );
    }
}
