//! Toolkit-independent presentation notifications. No timers, threads or UI objects live here.
//! The frontend registers a sink and decides whether a notification requires a redraw.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::OnceLock;

/// How the window's colours are picked.
#[derive(Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Theme {
    System,
    Dark,
    Light,
}

pub fn dark_of(config: &crate::Config) -> bool {
    match config
        .theme
        .unwrap_or(if config.dark { Theme::Dark } else { Theme::Light })
    {
        Theme::Dark => true,
        Theme::Light => false,
        Theme::System => system_is_dark(),
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Event {
    Dev,
    Changed,
    Chat,
    Log,
    Image,
    Theme,
    Input,
    Toggle,
    /// Show the window if it's hidden (another copy was started).
    Show,
    Restart,
    Quit,
}

type Sink = Box<dyn Fn(Event) + Send + Sync>;
static SINK: OnceLock<Sink> = OnceLock::new();

/// Install the frontend's event sink once during startup. A headless core needs no subscriber.
pub fn subscribe(sink: impl Fn(Event) + Send + Sync + 'static) {
    let _ = SINK.set(Box::new(sink));
}

fn emit(event: Event) {
    if let Some(sink) = SINK.get() {
        sink(event);
    }
}

pub fn changed() {
    emit(Event::Changed);
}

pub fn chat() {
    emit(Event::Chat);
}

pub fn logged() {
    emit(Event::Log);
}

pub static POLLS_DIRTY: AtomicBool = AtomicBool::new(true);

pub fn polls_changed() {
    POLLS_DIRTY.store(true, Ordering::Relaxed);
    changed();
}

use std::sync::Mutex;
use tokio::sync::oneshot;

pub(crate) type Relogin = (&'static str, bool);
pub(crate) static RELOGINS: Mutex<Vec<Relogin>> = Mutex::new(Vec::new());
static RELOGIN_ASKED: Mutex<Vec<Relogin>> = Mutex::new(Vec::new());
/// Get Input is a core interaction. Any presentation adapter or MCP may supply its answer.
pub fn input(title: String, prompt: String, text: String) -> oneshot::Receiver<Option<String>> {
    let receiver = crate::core::prompts::input(title, prompt, text);
    emit(Event::Input);
    receiver
}
pub fn confirm(text: String, no: &'static str, yes: &'static str) -> oneshot::Receiver<bool> {
    let receiver = crate::core::prompts::confirm(text, no, yes);
    emit(Event::Input);
    receiver
}
pub fn prompts_changed() {
    emit(Event::Input);
}

pub fn relogin(platform: &'static str, bot: bool) -> bool {
    let Ok(mut asked) = RELOGIN_ASKED.lock() else {
        return false;
    };
    if asked.contains(&(platform, bot)) {
        return false;
    }
    asked.push((platform, bot));
    if let Ok(mut questions) = RELOGINS.lock() {
        questions.push((platform, bot));
    }
    emit(Event::Input);
    true
}

pub fn relogin_done(platform: &'static str, bot: bool) {
    if let Ok(mut asked) = RELOGIN_ASKED.lock() {
        asked.retain(|entry| *entry != (platform, bot));
    }
}

pub fn ask_youtube_stream(streams: Vec<(String, String)>) -> oneshot::Receiver<Option<String>> {
    let receiver = crate::core::prompts::youtube(streams);
    emit(Event::Input);
    receiver
}

pub fn quit() {
    emit(Event::Quit);
}

pub fn toggle() {
    emit(Event::Toggle);
}
/// Shows the window, leaving it as it is when it's up already.
pub fn show() {
    emit(Event::Show);
}
pub fn restart() {
    emit(Event::Restart);
}

static VIEWERS_VISIBLE: AtomicBool = AtomicBool::new(false);

/// Whether the window is showing the Chat page, which draws each platform's viewer count: the platforms
/// poll it only then (or for a trigger or an overlay).
pub fn viewers_visible() -> bool {
    VIEWERS_VISIBLE.load(Ordering::Relaxed)
}
pub fn set_viewers_visible(visible: bool) {
    VIEWERS_VISIBLE.store(visible, Ordering::Relaxed);
}

/// The pixel density emote and avatar pictures are asked for (the renderer scales them).
pub fn image_scale() -> u8 {
    1
}
pub fn images_changed() {
    emit(Event::Image);
}

#[derive(Clone, Copy)]
pub enum Message {
    /// Tray click: show or hide the window.
    Toggle,
    /// No tray icon (e.g. no StatusNotifier host on Linux): keep the window up, closing it quits.
    NoTray,
    /// New log lines (sent only while the window is up).
    Log,
    /// A Get Input sub-action wants an answer (see `input`).
    Input,
    /// The engine changed the config (an action's state, a global...).
    Changed,
    /// New chat lines (sent only while the window is up).
    Chat,
    /// A picture from the web is on disk now (see `img`).
    Image,
    /// An update is in place: restart into it.
    Restart,
    /// The desktop's colour scheme changed.
    Theme,
    /// A dev command for the window (see `dev`).
    Dev,
    /// The tray's Quit: save what the window keeps, then quit.
    Quit,
}

/// Whether the desktop is dark, as the tray or the system settings last said.
pub(crate) static SYSTEM_DARK: AtomicBool = AtomicBool::new(true);

/// The desktop's colour scheme is `dark` now: a window following it changes too.
pub fn system_dark(dark: bool) {
    if SYSTEM_DARK.swap(dark, Ordering::Relaxed) != dark {
        emit(Event::Theme);
    }
}

/// The desktop's accent colour (0x01RRGGBB; 0 when it has none).
pub(crate) static SYSTEM_ACCENT: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);

/// The desktop's accent colour changed (or it has none): a window following the system changes too.
pub fn system_accent(rgb: Option<(u8, u8, u8)>) {
    let v = rgb.map_or(0, |(r, g, b)| {
        0x0100_0000 | (u32::from(r) << 16) | (u32::from(g) << 8) | u32::from(b)
    });
    if SYSTEM_ACCENT.swap(v, Ordering::Relaxed) != v {
        emit(Event::Theme);
    }
}

pub fn system_is_dark() -> bool {
    SYSTEM_DARK.load(Ordering::Relaxed)
}

/// A native shell adapter can send messages without importing the frontend toolkit.
#[derive(Clone)]
pub struct Sender(std::sync::Arc<dyn Fn(Message) + Send + Sync>);
impl Sender {
    pub fn new(send: impl Fn(Message) + Send + Sync + 'static) -> Self {
        Self(std::sync::Arc::new(send))
    }
    pub fn send(&self, message: Message) {
        (self.0)(message);
    }
}

pub(crate) type DevCmd = (String, oneshot::Sender<String>);
pub(crate) static DEV: Mutex<Vec<DevCmd>> = Mutex::new(Vec::new());

/// Forward development instrumentation to the attached frontend adapter.
pub fn dev(command: String) -> Option<oneshot::Receiver<String>> {
    SINK.get()?;
    if !ATTACHED.load(Ordering::Relaxed) {
        return None;
    }
    let (tx, rx) = oneshot::channel();
    DEV.lock().ok()?.push((command, tx));
    emit(Event::Dev);
    Some(rx)
}

static ATTACHED: AtomicBool = AtomicBool::new(false);
pub fn set_attached(attached: bool) {
    ATTACHED.store(attached, Ordering::Relaxed);
}

/// The frontend is gone: dev commands still queued for it get no answer, so their callers
/// (a WebSocket connection answers one request at a time) stop waiting instead of hanging.
pub fn detach() {
    set_attached(false);
    set_viewers_visible(false);
    if let Ok(mut commands) = DEV.lock() {
        commands.clear();
    }
}

#[cfg(test)]
mod detach_tests {
    use super::*;
    #[test]
    fn a_dev_command_queued_while_the_frontend_ends_is_released() {
        let _lock = crate::TEST_LOCK.lock().unwrap_or_else(|error| error.into_inner());
        subscribe(|_| {});
        set_attached(true);
        let mut reply = dev("windows".into()).expect("attached");
        detach();
        assert!(matches!(reply.try_recv(), Err(oneshot::error::TryRecvError::Closed)));
        assert!(dev("windows".into()).is_none());
    }
}

#[cfg(test)]
mod theme_tests {
    use super::*;
    #[test]
    fn an_explicit_theme_overrides_the_legacy_dark_flag() {
        let mut config = crate::Config {
            dark: true,
            theme: Some(Theme::Light),
            ..Default::default()
        };
        assert!(!dark_of(&config));
        config.dark = false;
        config.theme = Some(Theme::Dark);
        assert!(dark_of(&config));
        config.theme = None;
        assert!(!dark_of(&config));
    }
}
