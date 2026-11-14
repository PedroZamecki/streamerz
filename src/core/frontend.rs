//! The frontend process owns GPU resources; its lifetime does not own the engine.

use super::protocol;
use crate::engine::Engine;
use futures_util::{stream::FuturesUnordered, StreamExt};
use serde_json::json;
use std::io;
use std::process::Stdio;
use std::rc::Rc;
use std::sync::{
    atomic::{AtomicU32, Ordering},
    Arc,
};
use tokio::io::BufReader;
use tokio::process::{Child, ChildStdin, ChildStdout, Command};
use tokio::sync::Notify;

/// One coalesced wake-up carries the event families needed by the active page.
#[derive(Default)]
pub struct Changes {
    topics: AtomicU32,
    ready: Notify,
}
impl Changes {
    pub fn mark(&self, event: crate::host::Event) {
        use crate::host::Event;
        let bit = match event {
            Event::Changed => 1,
            Event::Chat => 2,
            Event::Log => 4,
            Event::Image => 8,
            Event::Theme => 16,
            Event::Input => 32,
            Event::Dev => 64,
            Event::Toggle | Event::Show | Event::Restart | Event::Quit => return,
        };
        self.topics.fetch_or(bit, Ordering::Release);
        self.ready.notify_one();
    }
    async fn next(&self) -> u32 {
        self.ready.notified().await;
        self.topics.swap(0, Ordering::AcqRel)
    }
}

pub struct Frontend {
    child: Child,
    #[cfg(windows)]
    _job: super::renderer_job::RendererJob,
    input: ChildStdin,
    output: protocol::Reader<BufReader<ChildStdout>>,
}

impl Frontend {
    /// Launch the same installed executable with private inherited pipes. No extra release file.
    pub fn spawn() -> io::Result<Self> {
        let mut command = Command::new(std::env::current_exe()?);
        command
            .arg("--frontend")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .kill_on_drop(true);
        #[cfg(target_os = "linux")]
        {
            let parent = std::process::id() as libc::pid_t;
            // SAFETY: only async-signal-safe system calls run between fork and exec.
            // Check the parent after installing the signal to cover termination during fork.
            unsafe {
                command.pre_exec(move || {
                    if libc::prctl(libc::PR_SET_PDEATHSIG, libc::SIGTERM) != 0 {
                        return Err(io::Error::last_os_error());
                    }
                    if libc::getppid() != parent {
                        return Err(io::Error::other("backend exited before renderer startup"));
                    }
                    Ok(())
                });
            }
        }
        let mut child = command.spawn()?;
        #[cfg(windows)]
        let job = super::renderer_job::RendererJob::attach(&child)?;
        let input = child
            .stdin
            .take()
            .ok_or_else(|| io::Error::other("missing frontend input pipe"))?;
        let output = protocol::Reader::new(BufReader::new(
            child
                .stdout
                .take()
                .ok_or_else(|| io::Error::other("missing frontend output pipe"))?,
        ));
        Ok(Self {
            child,
            input,
            output,
            #[cfg(windows)]
            _job: job,
        })
    }

    /// Serve the same application requests as MCP. Closing the pipe ends only this frontend session.
    /// A notification coalesces invalidations without an idle poll or an unbounded event queue.
    pub async fn serve(
        mut self,
        engine: Rc<Engine>,
        changes: Arc<Changes>,
        requests_tx: tokio::sync::mpsc::Sender<super::requests::Call>,
    ) -> io::Result<()> {
        protocol::write(
            &mut self.input,
            &json!({
                "event": "core.ready",
                "protocol": 1,
                "navigation": crate::core::navigation::snapshot(),
                "title": crate::version(),
                "dark": crate::host::dark_of(&engine.config()),
                "animations": engine.config().animations,
                "translations": crate::i18n::snapshot(),
                "window": &engine.config().window,
            }),
        )
        .await?;
        let mut pending = std::collections::HashMap::<u64, tokio::sync::oneshot::Sender<String>>::new();
        let mut serial = 0u64;
        let mut requests = FuturesUnordered::new();
        loop {
            tokio::select! {
                frame = self.output.read() => {
                    let Some(request) = frame? else { break };
                    if !request.get("request").is_some_and(|v| v.is_string()) {
                        return Err(io::Error::new(io::ErrorKind::InvalidData, "frontend request has no name"));
                    }
                    if request["request"] == "FrontendDevResult" {
                        if let Some(reply) = request["commandId"].as_u64().and_then(|id| pending.remove(&id)) {
                            let _ = reply.send(request["result"].as_str().unwrap_or_default().to_owned());
                        }
                        continue;
                    }
                    // Possession of this inherited pipe establishes the local frontend identity.
                    let (reply, response) = tokio::sync::oneshot::channel();
                    requests_tx.send((request, reply)).await.map_err(|_| io::Error::new(io::ErrorKind::BrokenPipe, "backend request channel closed"))?;
                    requests.push(response);
                }
                Some(reply) = requests.next(), if !requests.is_empty() => {
                    let reply = reply.map_err(|_| io::Error::new(io::ErrorKind::BrokenPipe, "backend request was abandoned"))?;
                    protocol::write(&mut self.input, &reply).await?;
                }
                topics = changes.next() => {
                    if topics == 0 { continue; }
                    let commands = crate::host::DEV.lock().map(|mut commands| std::mem::take(&mut *commands)).unwrap_or_default();
                    for (command, reply) in commands {
                        serial = serial.wrapping_add(1);
                        pending.insert(serial, reply);
                        protocol::write(&mut self.input, &json!({"event": "frontend.dev", "commandId": serial, "command": command})).await?;
                    }
                    let names: Vec<_> = [(1, "config"), (2, "chat"), (4, "log"), (8, "images"), (16, "theme"), (32, "prompts")].into_iter().filter_map(|(bit, name)| (topics & bit != 0).then_some(name)).collect();
                    if !names.is_empty() {
                        let mut notification = json!({"event":"core.changed", "topics":names});
                        if topics & 17 != 0 { notification["appearance"] = json!({"dark":crate::host::dark_of(&engine.config()), "animations":engine.config().animations}); }
                        protocol::write(&mut self.input, &notification).await?;
                    }
                }
                status = self.child.wait() => {
                    return match status? {
                        status if status.success() => Ok(()),
                        status => Err(io::Error::other(format!("frontend exited: {status}"))),
                    };
                }
            }
        }
        self.child.kill().await.or_else(|error| {
            if error.kind() == io::ErrorKind::InvalidInput {
                Ok(())
            } else {
                Err(error)
            }
        })?;
        let _ = self.child.wait().await?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn chat_and_log_updates_do_not_overwrite_a_pending_config_change() {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        runtime.block_on(async {
            let changes = Changes::default();
            changes.mark(crate::host::Event::Changed);
            changes.mark(crate::host::Event::Chat);
            changes.mark(crate::host::Event::Log);
            assert_eq!(changes.next().await, 7);
            changes.mark(crate::host::Event::Image);
            assert_eq!(changes.next().await, 8);
        });
    }
}
