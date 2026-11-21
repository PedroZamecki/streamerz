//! The frontend process owns GPU resources; its lifetime does not own the engine.

use super::protocol;
use crate::engine::Engine;
use futures_util::{stream::FuturesUnordered, StreamExt};
use serde_json::{json, Value};
use std::io;
use std::path::PathBuf;
use std::process::Stdio;
use std::rc::Rc;
use std::sync::{
    atomic::{AtomicBool, AtomicU32, Ordering},
    Arc,
};
use std::time::Duration;
use tokio::io::{AsyncBufRead, BufReader};
use tokio::process::{Child, ChildStdin, ChildStdout, Command};
use tokio::sync::Notify;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CrashInfo {
    pub summary: String,
    pub report_path: Option<PathBuf>,
    pub panic_recorded: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Exit {
    Clean,
    Crashed(CrashInfo),
    /// It ended before its window opened: the GPU (or its driver) couldn't show it.
    NoWindow(Option<CrashInfo>),
}

/// Set to draw the window on the processor instead of the GPU. Streamerz turns it on by itself when the
/// window can't open; a user whose driver opens a window that stays blank can set it by hand.
pub const SOFTWARE_RENDER: &str = "STREAMERZ_SOFTWARE_RENDER";

static SOFTWARE: AtomicBool = AtomicBool::new(false);

/// Whether the window is drawn in software, for the rest of this run.
pub fn software() -> bool {
    SOFTWARE.load(Ordering::Relaxed)
        || std::env::var_os(SOFTWARE_RENDER).is_some_and(|value| !value.is_empty() && value != "0")
}

/// Draw the window in software from now on (until Streamerz restarts). False when it already was.
pub fn use_software() -> bool {
    !software() && !SOFTWARE.swap(true, Ordering::Relaxed)
}

/// What the window process needs to draw without the GPU. Windows: the patched GPUI picks Direct3D's
/// WARP. Linux: only Mesa's software drivers (lavapipe for Vulkan when installed, llvmpipe for OpenGL).
fn software_env() -> Vec<(&'static str, String)> {
    #[cfg_attr(not(target_os = "linux"), allow(unused_mut))]
    let mut env = vec![(SOFTWARE_RENDER, "1".to_owned())];
    #[cfg(target_os = "linux")]
    {
        env.push(("LIBGL_ALWAYS_SOFTWARE", "1".to_owned()));
        let dirs = linux_data_dirs();
        // A vendor driver (NVIDIA's) would otherwise take EGL before Mesa.
        if let Some(mesa) = find_in(&dirs, "glvnd/egl_vendor.d", "mesa") {
            env.push(("__EGL_VENDOR_LIBRARY_FILENAMES", mesa));
        }
        // No hardware Vulkan driver: lavapipe, or none at all and OpenGL draws.
        let vulkan = find_in(&dirs, "vulkan/icd.d", "lvp").unwrap_or_else(|| "/dev/null".to_owned());
        env.push(("VK_DRIVER_FILES", vulkan.clone()));
        env.push(("VK_ICD_FILENAMES", vulkan));
    }
    env
}

#[cfg(target_os = "linux")]
fn linux_data_dirs() -> Vec<PathBuf> {
    let mut dirs: Vec<PathBuf> = std::env::var("XDG_DATA_DIRS")
        .ok()
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| "/usr/local/share:/usr/share".to_owned())
        .split(':')
        .filter(|dir| !dir.is_empty())
        .map(PathBuf::from)
        .collect();
    // System settings first, and the GL extension's folder inside Flatpak.
    dirs.insert(0, PathBuf::from("/etc"));
    dirs.push(PathBuf::from("/usr/lib/x86_64-linux-gnu/GL"));
    dirs.push(PathBuf::from("/usr/lib/x86_64-linux-gnu/GL/default/share"));
    dirs
}

/// The first `.json` in `<dir>/<sub>` whose name holds `name`.
#[cfg(target_os = "linux")]
fn find_in(dirs: &[PathBuf], sub: &str, name: &str) -> Option<String> {
    dirs.iter().find_map(|dir| {
        let mut found: Vec<PathBuf> = std::fs::read_dir(dir.join(sub))
            .ok()?
            .filter_map(|entry| entry.ok().map(|entry| entry.path()))
            .filter(|path| {
                path.extension().is_some_and(|ext| ext == "json")
                    && path
                        .file_name()
                        .is_some_and(|file| file.to_string_lossy().contains(name))
            })
            .collect();
        found.sort();
        // The machine's own architecture when there are several (lvp_icd.x86_64.json, lvp_icd.i686.json).
        let arch = std::env::consts::ARCH;
        let pick = found
            .iter()
            .position(|path| path.to_string_lossy().contains(arch))
            .unwrap_or(0);
        found.get(pick).map(|path| path.display().to_string())
    })
}

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
            // Another copy was started: the window comes to the front.
            Event::Show => 128,
            Event::Toggle | Event::Restart | Event::Quit => return,
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

/// Processes UI diagnostic frames (`FrontendLog`, `FrontendStep`, `FrontendCrash`) inside the backend.
/// Returns `true` if the frame was handled as a diagnostic message.
pub(crate) fn handle_diag_request(request: &Value, crash_info: &mut Option<CrashInfo>) -> bool {
    match request["request"].as_str() {
        Some("FrontendLog") => {
            let level = match request["level"].as_str().unwrap_or("info") {
                "debug" => crate::Level::Debug,
                "warn" => crate::Level::Warn,
                "error" => crate::Level::Error,
                "fatal" => crate::Level::Fatal,
                _ => crate::Level::Info,
            };
            let text = request["text"].as_str().unwrap_or_default().trim_end();
            if !text.is_empty() {
                crate::log_ui(level, text.to_owned());
            }
            true
        }
        Some("FrontendStep") => {
            match request["kind"].as_str().unwrap_or("step") {
                "fact" => {
                    let value = request["value"].as_str().unwrap_or_default();
                    match request["key"].as_str().unwrap_or_default() {
                        "UI page" => crate::crash::fact("UI page", value),
                        "UI action" => crate::crash::fact("UI action", value),
                        "UI dialog" => crate::crash::fact("UI dialog", value),
                        other if !other.is_empty() => crate::crash::step(&format!("[UI] {other}: {value}")),
                        _ => {}
                    }
                }
                _ => {
                    let text = request["text"].as_str().unwrap_or_default();
                    if !text.is_empty() {
                        crate::crash::step(&format!("[UI] {text}"));
                    }
                }
            }
            true
        }
        Some("FrontendCrash") => {
            let thread = request["thread"].as_str().unwrap_or("?");
            let location = request["location"].as_str().unwrap_or("");
            let message = request["message"].as_str().unwrap_or("(no message)");
            let context = request["context"].as_str().unwrap_or("");
            let backtrace = request["backtrace"].as_str().unwrap_or("");
            let existing = request["reportPath"].as_str().map(PathBuf::from);
            let path =
                crate::crash::record_ui_panic(thread, location, message, context, backtrace, existing.as_deref());
            *crash_info = Some(CrashInfo {
                summary: message.to_owned(),
                report_path: Some(path),
                panic_recorded: true,
            });
            true
        }
        _ => false,
    }
}

/// Drains any buffered diagnostic frames from the child's stdout after process exit.
async fn drain_diag_frames<R: AsyncBufRead + Unpin>(
    output: &mut protocol::Reader<R>,
    crash_info: &mut Option<CrashInfo>,
) {
    let _ = tokio::time::timeout(Duration::from_millis(200), async {
        while let Ok(Some(frame)) = output.read().await {
            handle_diag_request(&frame, crash_info);
        }
    })
    .await;
}

impl Frontend {
    /// Launch the same installed executable with private inherited pipes. No extra release file.
    pub fn spawn() -> io::Result<Self> {
        let mut command = Command::new(std::env::current_exe()?);
        command
            .arg("--frontend")
            .env("STREAMERZ_LOG_DIR", crate::crash::folder())
            .env("STREAMERZ_VERSION", crate::brand::VERSION)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .kill_on_drop(true);
        if software() {
            command.envs(software_env());
        }
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
        recovered_crash: Option<Value>,
    ) -> io::Result<Exit> {
        let mut ready = json!({
            "event": "core.ready",
            "protocol": 1,
            "navigation": crate::core::navigation::snapshot(),
            "title": crate::version(),
            "dark": crate::host::dark_of(&engine.config()),
            "animations": engine.config().animations,
            "translations": crate::i18n::snapshot(),
            "window": &engine.config().window,
            "logDir": crate::crash::folder(),
            "version": crate::brand::VERSION,
            "debugLog": crate::debug_log(),
            "activeErrors": crate::error::active_list(),
        });
        if let Some(crash) = recovered_crash {
            ready["recoveredCrash"] = crash;
        }
        protocol::write(&mut self.input, &ready).await?;
        let mut pending = std::collections::HashMap::<u64, tokio::sync::oneshot::Sender<String>>::new();
        let mut serial = 0u64;
        let mut requests = FuturesUnordered::new();
        let mut crash_info: Option<CrashInfo> = None;
        // The window asks for its first page only once it is open.
        let mut opened = false;
        let exit_status = loop {
            tokio::select! {
                frame = self.output.read() => {
                    let request = match frame {
                        Ok(Some(request)) => request,
                        Ok(None) => break None,
                        Err(error) => {
                            crate::warn(format!("Frontend pipe read failed: {error}"));
                            break None;
                        }
                    };
                    if !request.get("request").is_some_and(|v| v.is_string()) {
                        return Err(io::Error::new(io::ErrorKind::InvalidData, "frontend request has no name"));
                    }
                    if handle_diag_request(&request, &mut crash_info) {
                        continue;
                    }
                    if request["request"] == "FrontendDevResult" {
                        if let Some(reply) = request["commandId"].as_u64().and_then(|id| pending.remove(&id)) {
                            let _ = reply.send(request["result"].as_str().unwrap_or_default().to_owned());
                        }
                        continue;
                    }
                    opened = true;
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
                    let names: Vec<_> = [(1, "config"), (2, "chat"), (4, "log"), (8, "images"), (16, "theme"), (32, "prompts"), (128, "raise")].into_iter().filter_map(|(bit, name)| (topics & bit != 0).then_some(name)).collect();
                    if !names.is_empty() {
                        let mut notification = json!({"event":"core.changed", "topics":names});
                        if topics & 17 != 0 { notification["appearance"] = json!({"dark":crate::host::dark_of(&engine.config()), "animations":engine.config().animations}); }
                        if topics & 1 != 0 { notification["debugLog"] = json!(crate::debug_log()); }
                        protocol::write(&mut self.input, &notification).await?;
                    }
                }
                status = self.child.wait() => {
                    drain_diag_frames(&mut self.output, &mut crash_info).await;
                    break Some(status?);
                }
            }
        };

        let status = match exit_status {
            Some(status) => status,
            None => match tokio::time::timeout(Duration::from_secs(2), self.child.wait()).await {
                Ok(result) => result?,
                Err(_) => {
                    self.child.kill().await.or_else(|error| {
                        if error.kind() == io::ErrorKind::InvalidInput {
                            Ok(())
                        } else {
                            Err(error)
                        }
                    })?;
                    self.child.wait().await?
                }
            },
        };

        Ok(exit_of(opened, status.success(), crash_info, status.to_string()))
    }
}

/// How the window process ended. A failed exit with no report of its own gets one, except before the
/// window opened: that is reported only if drawing in software doesn't help either (`report`).
fn exit_of(opened: bool, success: bool, crash_info: Option<CrashInfo>, status: String) -> Exit {
    let unclean = |summary: String, report_path| CrashInfo {
        summary,
        report_path,
        panic_recorded: false,
    };
    match crash_info {
        _ if !opened => Exit::NoWindow(crash_info.or_else(|| (!success).then(|| unclean(status, None)))),
        Some(info) => Exit::Crashed(info),
        None if success => Exit::Clean,
        None => {
            let path = crate::crash::record_ui_unclean(&status);
            Exit::Crashed(unclean(status, Some(path)))
        }
    }
}

/// A window that couldn't open, as a crash, with its report written.
pub fn report(info: Option<CrashInfo>) -> CrashInfo {
    let mut info = info.unwrap_or_else(|| CrashInfo {
        summary: "the window couldn't open".to_owned(),
        report_path: None,
        panic_recorded: false,
    });
    if info.report_path.is_none() {
        info.report_path = Some(crate::crash::record_ui_unclean(&info.summary));
    }
    info
}

#[cfg(test)]
mod tests {
    use super::*;

    fn info(summary: &str) -> CrashInfo {
        CrashInfo {
            summary: summary.to_owned(),
            report_path: None,
            panic_recorded: false,
        }
    }

    #[test]
    fn a_window_that_never_opened_is_told_apart_from_a_crash() {
        let status = || "exit status: 1".to_owned();
        // The GPU couldn't make a surface: the process ends, cleanly or not, before its first request.
        assert_eq!(exit_of(false, true, None, status()), Exit::NoWindow(None));
        assert_eq!(
            exit_of(false, false, None, status()),
            Exit::NoWindow(Some(info("exit status: 1")))
        );
        assert_eq!(
            exit_of(false, false, Some(info("panic")), status()),
            Exit::NoWindow(Some(info("panic")))
        );
        // Once open, it is the usual clean close or crash.
        assert_eq!(exit_of(true, true, None, status()), Exit::Clean);
        assert_eq!(
            exit_of(true, true, Some(info("panic")), status()),
            Exit::Crashed(info("panic"))
        );
    }

    #[test]
    fn software_rendering_is_turned_on_once_and_passed_to_the_window() {
        let _lock = crate::TEST_LOCK.lock().unwrap_or_else(|error| error.into_inner());
        std::env::remove_var(SOFTWARE_RENDER);
        SOFTWARE.store(false, Ordering::Relaxed);
        assert!(!software());
        assert!(use_software());
        assert!(software());
        assert!(!use_software(), "a second failure in software isn't retried");
        let env = software_env();
        assert!(env.contains(&(SOFTWARE_RENDER, "1".to_owned())));
        #[cfg(target_os = "linux")]
        {
            assert!(env.contains(&("LIBGL_ALWAYS_SOFTWARE", "1".to_owned())));
            let vulkan = env
                .iter()
                .find(|(key, _)| *key == "VK_DRIVER_FILES")
                .map(|(_, value)| value);
            assert!(
                vulkan.is_some_and(|path| path == "/dev/null" || path.contains("lvp")),
                "{vulkan:?}"
            );
        }
        SOFTWARE.store(false, Ordering::Relaxed);
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn software_drivers_are_found_by_name_and_architecture() {
        let root = std::env::temp_dir().join(format!("streamerz-icd-{}", std::process::id()));
        let icd = root.join("share/vulkan/icd.d");
        std::fs::create_dir_all(&icd).unwrap();
        for name in [
            "intel_icd.x86_64.json",
            "lvp_icd.i686.json",
            &format!("lvp_icd.{}.json", std::env::consts::ARCH),
            "lvp_notes.txt",
        ] {
            std::fs::write(icd.join(name), "{}").unwrap();
        }
        let dirs = [root.join("missing"), root.join("share")];
        let found = find_in(&dirs, "vulkan/icd.d", "lvp").unwrap();
        assert!(
            found.ends_with(&format!("lvp_icd.{}.json", std::env::consts::ARCH)),
            "{found}"
        );
        assert_eq!(find_in(&dirs, "vulkan/icd.d", "nouveau"), None);
        assert_eq!(find_in(&dirs, "glvnd/egl_vendor.d", "mesa"), None);
        let _ = std::fs::remove_dir_all(root);
    }
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

    #[test]
    fn frontend_log_step_and_crash_frames_are_captured_into_unified_logs_even_during_drain() {
        let _lock = crate::TEST_LOCK.lock().unwrap_or_else(|error| error.into_inner());
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        runtime.block_on(async {
            let (mut writer, reader) = tokio::io::duplex(4096);
            protocol::write(
                &mut writer,
                &json!({"request": "FrontendLog", "level": "warn", "text": "slider min clamped"}),
            )
            .await
            .unwrap();
            protocol::write(
                &mut writer,
                &json!({"request": "FrontendStep", "kind": "fact", "key": "UI page", "value": "Overlays"}),
            )
            .await
            .unwrap();
            protocol::write(
                &mut writer,
                &json!({"request": "FrontendStep", "kind": "step", "text": "opened overlays settings"}),
            )
            .await
            .unwrap();
            protocol::write(
                &mut writer,
                &json!({
                    "request": "FrontendCrash",
                    "thread": "main",
                    "location": "frontend/src/controls/form.rs:280",
                    "message": "simulated UI panic",
                    "context": "Context:\n  UI page: Overlays\n",
                    "backtrace": "frame 0"
                }),
            )
            .await
            .unwrap();
            drop(writer);

            let mut proto_reader = protocol::Reader::new(BufReader::new(reader));
            let mut crash_info = None;
            drain_diag_frames(&mut proto_reader, &mut crash_info).await;

            let info = crash_info.expect("FrontendCrash must produce CrashInfo");
            assert_eq!(info.summary, "simulated UI panic");
            assert!(info.panic_recorded);
            let report_path = info.report_path.expect("crash report path");
            assert!(report_path.is_file(), "report missing at {}", report_path.display());
            let tail = crate::log_tail_tagged(20).join("\n");
            assert!(tail.contains("[WARN] [UI] slider min clamped"), "{tail}");
            assert!(
                tail.contains(
                    "[FATAL] [UI] Panic in thread 'main' at frontend/src/controls/form.rs:280: simulated UI panic"
                ),
                "{tail}"
            );
            let _ = std::fs::remove_file(report_path);
        });
    }
}

#[cfg(test)]
mod raise_tests {
    use super::Changes;
    use crate::host::Event;
    use std::sync::atomic::Ordering;

    /// A start, the tray or another copy bring the window forward; a question (Get Input) only opens
    /// it, so it never takes the focus from a game.
    #[test]
    fn only_the_users_asks_raise_the_window() {
        let changes = Changes::default();
        changes.mark(Event::Input);
        assert_eq!(changes.topics.load(Ordering::Acquire) & 128, 0);
        changes.mark(Event::Show);
        assert_eq!(changes.topics.load(Ordering::Acquire) & 128, 128);
    }
}
