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
    /// It failed in the GPU code (the renderer gave up, a lost device couldn't be remade).
    pub gpu: bool,
}

/// Whether a panic came from drawing on the GPU: its place is in the renderer's crates, or its message
/// names the GPU's parts.
pub(crate) fn gpu_failure(location: &str, message: &str) -> bool {
    let location = location.replace('\\', "/");
    [
        "gpui-pre-wgpu",
        "gpui-pre-windows",
        "/wgpu-",
        "/wgpu-hal-",
        "/ash-",
        "/glow-",
    ]
    .iter()
    .any(|place| location.contains(place))
        || ["GPU", "DirectX", "Direct3D", "Device lost", "swap chain", "Vulkan"]
            .iter()
            .any(|word| message.contains(word))
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Exit {
    Clean,
    Crashed(CrashInfo),
    /// The GPU failed: the window ended before it opened, or crashed while drawing.
    GpuFailed(Option<CrashInfo>),
}

/// Set to draw the window on the processor instead of the GPU, like the Settings switch.
pub const SOFTWARE_RENDER: &str = "STREAMERZ_SOFTWARE_RENDER";

/// The GPU couldn't show the window in this run: it is drawn in software until Streamerz restarts.
static FAILED: AtomicBool = AtomicBool::new(false);

/// What draws the open window, as it reported (a GPU's or a software renderer's name).
static RENDERER: std::sync::Mutex<Option<(String, bool)>> = std::sync::Mutex::new(None);

/// Why the window is drawn in software, decided before it opens; None: the GPU draws it.
pub fn software_reason(setting: bool) -> Option<&'static str> {
    let asked = std::env::var_os(SOFTWARE_RENDER).is_some_and(|value| !value.is_empty() && value != "0");
    reason_of(asked, setting, FAILED.load(Ordering::Relaxed), gpu_present)
}

fn reason_of(asked: bool, setting: bool, failed: bool, gpu_present: impl FnOnce() -> bool) -> Option<&'static str> {
    if asked {
        Some("STREAMERZ_SOFTWARE_RENDER is set")
    } else if setting {
        Some("turned on in Settings")
    } else if failed {
        Some("the GPU couldn't show it")
    } else if !gpu_present() {
        Some("no GPU was found")
    } else {
        None
    }
}

/// The window couldn't open on the GPU: true when it should try again in software (it wasn't already).
pub fn use_software(setting: bool) -> bool {
    software_reason(setting).is_none() && !FAILED.swap(true, Ordering::Relaxed)
}

/// What the window said it is drawn by: the device's name, and whether it draws on the processor.
pub fn renderer() -> Option<(String, bool)> {
    RENDERER.lock().ok()?.clone()
}

/// The handshake's `software`: why, and the environment the window sets before its graphics load.
fn software_plan(setting: bool) -> Value {
    match software_reason(setting) {
        Some(reason) => {
            let env: serde_json::Map<String, Value> = software_env()
                .into_iter()
                .map(|(key, value)| (key.to_owned(), value.into()))
                .collect();
            json!({"reason": reason, "env": env})
        }
        None => Value::Null,
    }
}

/// A GPU the window can draw with: a render node (or NVIDIA's device) on Linux. Windows always has
/// WARP behind the GPU, picked in the window itself when no GPU can draw.
fn gpu_present() -> bool {
    #[cfg(target_os = "linux")]
    {
        gpu_in(std::path::Path::new("/dev"))
    }
    #[cfg(not(target_os = "linux"))]
    {
        true
    }
}

#[cfg(target_os = "linux")]
fn gpu_in(dev: &std::path::Path) -> bool {
    dev.join("nvidia0").exists()
        || std::fs::read_dir(dev.join("dri")).is_ok_and(|mut nodes| {
            nodes.any(|node| node.is_ok_and(|node| node.file_name().to_string_lossy().starts_with("renderD")))
        })
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
            Event::Preview => 256,
            Event::Toggle | Event::Reopen | Event::Restart | Event::Quit => return,
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
        // What draws the window, once it is open.
        Some("FrontendRenderer") => {
            let device = request["device"].as_str().unwrap_or("an unknown device").to_owned();
            let software = request["software"].as_bool().unwrap_or(false);
            crate::say(format!(
                "The window is drawn by {device}{}",
                if software { " (in software)" } else { "" }
            ));
            if let Ok(mut renderer) = RENDERER.lock() {
                *renderer = Some((device, software));
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
                gpu: gpu_failure(location, message),
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
            "software": software_plan(engine.config().software_render),
        });
        if let Some(reason) = ready["software"]["reason"].as_str() {
            crate::say(format!("Drawing the window in software: {reason}"));
        }
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
                    let names: Vec<_> = [(1, "config"), (2, "chat"), (4, "log"), (8, "images"), (16, "theme"), (32, "prompts"), (128, "raise"), (256, "preview")].into_iter().filter_map(|(bit, name)| (topics & bit != 0).then_some(name)).collect();
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

        // The overlay preview is drawn only by the window: it goes with it.
        crate::overlay_preview::stop();
        let stopped = stopped_from_outside(&status);
        if stopped && crash_info.is_none() {
            crate::say(format!("The window was closed from outside Streamerz ({status})"));
        }
        Ok(exit_of(
            opened,
            status.success(),
            stopped,
            crash_info,
            status.to_string(),
        ))
    }
}

/// Whether something outside Streamerz asked the window to stop: a task manager, `kill`, the session
/// ending. That is a close, not a crash: no report, and the window isn't brought back.
fn stopped_from_outside(status: &std::process::ExitStatus) -> bool {
    #[cfg(target_os = "linux")]
    {
        use std::os::unix::process::ExitStatusExt;
        matches!(status.signal(), Some(libc::SIGTERM | libc::SIGHUP | libc::SIGINT))
    }
    #[cfg(not(target_os = "linux"))]
    {
        let _ = status;
        false
    }
}

/// How the window process ended. A failed exit with no report of its own gets one, except before the
/// window opened: that is reported only if drawing in software doesn't help either (`report`). One
/// stopped from outside (`stopped`) closed, like the window's own close button.
fn exit_of(opened: bool, success: bool, stopped: bool, crash_info: Option<CrashInfo>, status: String) -> Exit {
    let unclean = |summary: String, report_path| CrashInfo {
        summary,
        report_path,
        panic_recorded: false,
        gpu: false,
    };
    match crash_info {
        Some(info) if info.gpu => Exit::GpuFailed(Some(info)),
        None if stopped => Exit::Clean,
        _ if !opened => Exit::GpuFailed(crash_info.or_else(|| (!success).then(|| unclean(status, None)))),
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
        gpu: true,
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
            gpu: false,
        }
    }

    #[test]
    fn a_window_that_never_opened_is_told_apart_from_a_crash() {
        let status = || "exit status: 1".to_owned();
        // The GPU couldn't make a surface: the process ends, cleanly or not, before its first request.
        assert_eq!(exit_of(false, true, false, None, status()), Exit::GpuFailed(None));
        assert_eq!(
            exit_of(false, false, false, None, status()),
            Exit::GpuFailed(Some(info("exit status: 1")))
        );
        assert_eq!(
            exit_of(false, false, false, Some(info("panic")), status()),
            Exit::GpuFailed(Some(info("panic")))
        );
        // Once open, it is the usual clean close or crash.
        assert_eq!(exit_of(true, true, false, None, status()), Exit::Clean);
        assert_eq!(
            exit_of(true, true, false, Some(info("panic")), status()),
            Exit::Crashed(info("panic"))
        );
    }

    #[test]
    fn a_window_stopped_from_outside_closed_and_did_not_crash() {
        // A task manager or `kill` sends SIGTERM: no crash report, no window brought back.
        let status = || "signal: 15 (SIGTERM)".to_owned();
        assert_eq!(exit_of(true, false, true, None, status()), Exit::Clean);
        assert_eq!(exit_of(false, false, true, None, status()), Exit::Clean);
        // A panic it reported before the signal is still a crash.
        assert_eq!(
            exit_of(true, false, true, Some(info("panic")), status()),
            Exit::Crashed(info("panic"))
        );
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn stop_signals_are_told_apart_from_crash_signals() {
        use std::os::unix::process::ExitStatusExt;
        let signal = |number| std::process::ExitStatus::from_raw(number);
        assert!(stopped_from_outside(&signal(libc::SIGTERM)));
        assert!(stopped_from_outside(&signal(libc::SIGHUP)));
        assert!(stopped_from_outside(&signal(libc::SIGINT)));
        assert!(!stopped_from_outside(&signal(libc::SIGSEGV)));
        assert!(!stopped_from_outside(&signal(libc::SIGKILL)));
        assert!(!stopped_from_outside(&signal(libc::SIGABRT)));
        assert!(!stopped_from_outside(&std::process::ExitStatus::from_raw(1 << 8)));
    }

    #[test]
    fn a_crash_in_the_gpu_code_is_the_gpu_failing() {
        // Linux's renderer gives up after ten GPU errors in a row; Windows can't remake a lost device.
        let wgpu = "/home/u/.cargo/registry/src/index.crates.io-1/gpui-pre-wgpu-0.3.11/src/wgpu_renderer.rs:1180:17";
        assert!(gpu_failure(
            wgpu,
            "Too many consecutive GPU errors. Last error: Validation"
        ));
        assert!(gpu_failure(
            r"C:\Users\u\.cargo\registry\src\gpui-pre-windows-0.3.11\src\events.rs:1336:13",
            "x"
        ));
        assert!(gpu_failure(
            "frontend/src/app.rs:10:5",
            "Device lost: Recreating DirectX devices"
        ));
        assert!(!gpu_failure(
            "frontend/src/controls/form.rs:280:9",
            "index out of bounds"
        ));
        // A window that opened and crashed in the GPU code goes the same way as one that never opened.
        let mut gpu = info("Too many consecutive GPU errors");
        gpu.gpu = true;
        assert_eq!(
            exit_of(true, false, false, Some(gpu.clone()), "exit".into()),
            Exit::GpuFailed(Some(gpu))
        );
    }

    #[test]
    fn software_rendering_is_chosen_before_the_window_opens() {
        let gpu = || true;
        assert_eq!(reason_of(false, false, false, gpu), None);
        assert_eq!(reason_of(false, false, false, || false), Some("no GPU was found"));
        assert_eq!(reason_of(false, false, true, gpu), Some("the GPU couldn't show it"));
        assert_eq!(reason_of(false, true, false, gpu), Some("turned on in Settings"));
        assert_eq!(
            reason_of(true, false, false, gpu),
            Some("STREAMERZ_SOFTWARE_RENDER is set")
        );
        // The GPU isn't looked for when something else already decided.
        assert!(reason_of(false, true, false, || unreachable!()).is_some());
    }

    #[test]
    fn the_window_is_told_how_to_draw_in_software() {
        let _lock = crate::TEST_LOCK.lock().unwrap_or_else(|error| error.into_inner());
        assert_eq!(software_plan(true)["reason"], "turned on in Settings");
        let env = &software_plan(true)["env"];
        assert_eq!(env[SOFTWARE_RENDER], "1");
        #[cfg(target_os = "linux")]
        {
            assert_eq!(env["LIBGL_ALWAYS_SOFTWARE"], "1");
            let vulkan = env["VK_DRIVER_FILES"].as_str().unwrap_or_default();
            assert!(vulkan == "/dev/null" || vulkan.contains("lvp"), "{vulkan}");
        }
        // A failure in software isn't retried.
        assert!(!use_software(true));
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn a_gpu_is_a_render_node_or_nvidias_device() {
        let root = std::env::temp_dir().join(format!("streamerz-dev-{}", std::process::id()));
        std::fs::create_dir_all(root.join("dri")).unwrap();
        std::fs::write(root.join("dri/card0"), "").unwrap();
        assert!(!gpu_in(&root), "a display without a render node can't draw");
        std::fs::write(root.join("dri/renderD128"), "").unwrap();
        assert!(gpu_in(&root));
        std::fs::remove_dir_all(root.join("dri")).unwrap();
        std::fs::write(root.join("nvidia0"), "").unwrap();
        assert!(gpu_in(&root));
        let _ = std::fs::remove_dir_all(root);
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
