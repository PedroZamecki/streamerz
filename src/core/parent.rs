//! Backend lifecycle. The renderer can come and go without stopping integrations or MCP.

use crate::engine::{Engine, UiCmd};
use crate::host::{Event, Message};
use std::rc::Rc;
use std::sync::Arc;
use tokio::sync::mpsc;
use tokio::task::JoinHandle;

type Renderer = JoinHandle<std::io::Result<()>>;

fn open(
    engine: Rc<Engine>,
    changes: Arc<super::frontend::Changes>,
    requests: mpsc::Sender<super::requests::Call>,
) -> Option<Renderer> {
    match super::frontend::Frontend::spawn() {
        Ok(frontend) => {
            crate::host::set_attached(true);
            Some(tokio::task::spawn_local(frontend.serve(engine, changes, requests)))
        }
        Err(error) => {
            crate::error(format!("Can't open Streamerz: {error}"));
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
            Event::Restart => Some(Message::Restart),
            Event::Quit => Some(Message::Quit),
            _ => None,
        };
        if let Some(message) = message {
            let _ = sender.send(message);
        }
    });
    let show = !engine.config().start_hidden || !super::prompts::list().is_empty();
    let mut renderer = if show {
        open(engine.clone(), changes.clone(), requests_tx.clone())
    } else {
        // Hidden from the start: once, after startup settles, let the kernel take back the idle code pages.
        tokio::spawn(async {
            tokio::time::sleep(std::time::Duration::from_secs(8)).await;
            crate::release_code_pages();
        });
        None
    };
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
                        else { renderer = open(engine.clone(), changes.clone(), requests_tx.clone()); }
                    }
                    Some(Message::Input) => {
                        // Logins that ran out: each becomes a question in the window (a prompt).
                        let relogins = crate::host::RELOGINS.lock().map(|mut queue| std::mem::take(&mut *queue)).unwrap_or_default();
                        for (platform, bot) in relogins { super::accounts::ask_relogin(&engine, platform, bot); }
                        let pending = !super::prompts::list().is_empty() || crate::host::RELOGINS.lock().is_ok_and(|items| !items.is_empty());
                        if pending && renderer.is_none() { renderer = open(engine.clone(), changes.clone(), requests_tx.clone()); }
                    }
                    Some(Message::NoTray) => {
                        if renderer.is_none() { renderer = open(engine.clone(), changes.clone(), requests_tx.clone()); }
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
                match result {
                    Ok(Ok(())) => {},
                    Ok(Err(error)) => crate::error(format!("Streamerz frontend: {error}")),
                    Err(error) => crate::error(format!("Streamerz frontend stopped: {error}")),
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
