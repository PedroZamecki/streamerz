//! TikTok's own pages in the OS webview, as a child process so the browser's memory goes away with it:
//! `streamerz-tiktok` on Linux (so Streamerz itself doesn't load WebKitGTK), `streamerz.exe --tiktok`
//! on Windows. TikTok signs its web chat requests, so chat is sent by typing into its live page.
//!
//! - `login <profile>`: TikTok's login page. Once logged in, prints {"username","cookies","avatar"} and exits 0;
//!   closing the window exits 1.
//! - `send <profile> <channel>`: the channel's live page in a hidden window, its video paused. Each stdin
//!   line is a JSON string to send; each gets one stdout line, {"ok":true} or {"error":"..."}. Exits when
//!   stdin closes, or after 2 minutes without a message (the page costs ~1 GB and some CPU).

use std::io::Write;
use std::path::PathBuf;
use std::time::{Duration, Instant};
use tao::dpi::LogicalSize;
use tao::event::{Event, StartCause, WindowEvent};
use tao::event_loop::{ControlFlow, EventLoopBuilder};
use tao::window::WindowBuilder;
use wry::{WebContext, WebViewBuilder};

/// The cookies that make a session.
const KEEP: [&str; 6] = [
    "sessionid",
    "sessionid_ss",
    "sid_tt",
    "sid_guard",
    "uid_tt",
    "tt-target-idc",
];

/// Logged-in user and profile picture, from the data TikTok's own page loads with.
const WHO: &str = r#"(() => { try {
  const d = JSON.parse(document.getElementById('__UNIVERSAL_DATA_FOR_REHYDRATION__').textContent);
  const u = d.__DEFAULT_SCOPE__['webapp.app-context'].user;
  return [u.uniqueId || '', (u.avatarUri || [])[0] || u.avatarThumb || ''];
} catch (e) { return ['', '']; } })()"#;

/// Sends one chat message on the live page, in order, then posts the outcome back. The box is TikTok's
/// own (`disabled` with the reason as placeholder when this account can't comment).
const SEND: &str = r#"
window.__szq = Promise.resolve();
setInterval(() => document.querySelectorAll('video').forEach(v => v.paused || v.pause()), 2000);
window.__szSend = text => { window.__szq = window.__szq.then(async () => {
  const sleep = ms => new Promise(r => setTimeout(r, ms));
  const done = r => window.ipc.postMessage(JSON.stringify(r));
  let box;
  for (let i = 0; i < 40 && !(box = document.querySelector('[data-e2e="room-chat-input-field"]')); i++) await sleep(500);
  if (!box) return done({ error: "the live page has no chat (not live, or not logged in)" });
  if (box.hasAttribute('disabled')) return done({ error: 'TikTok: ' + (box.getAttribute('placeholder') || "can't comment") });
  const clear = () => { box.focus(); document.execCommand('selectAll'); document.execCommand('delete'); };
  clear();
  document.execCommand('insertText', false, text.slice(0, 150));
  for (const t of ['keydown', 'keypress', 'keyup'])
    box.dispatchEvent(new KeyboardEvent(t, { key: 'Enter', code: 'Enter', keyCode: 13, which: 13, bubbles: true, cancelable: true }));
  await sleep(1500);
  if (box.textContent) { document.querySelector('[data-e2e="room-chat-send-btn"]')?.click(); await sleep(1500); }
  if (box.textContent) { clear(); return done({ error: "TikTok didn't take the message" }); }
  done({ ok: true });
}).catch(e => window.ipc.postMessage(JSON.stringify({ error: String(e) }))); };
"#;

const IDLE: Duration = Duration::from_secs(120);

enum Ev {
    Line(Option<String>),
    Result(String),
}

// Unused when built into streamerz.exe as a module.
#[allow(dead_code)]
fn main() {
    let a: Vec<_> = std::env::args_os().skip(1).collect();
    run(&a)
}

/// `args`: the mode and its arguments (see the module docs).
pub fn run(args: &[std::ffi::OsString]) -> ! {
    let arg = |i: usize| {
        args.get(i)
            .map(|a| a.to_string_lossy().into_owned())
            .unwrap_or_default()
    };
    let send = arg(0) == "send";
    let profile = PathBuf::from(arg(1));
    let url = if send {
        // STREAMERZ_TIKTOK_URL: a local copy of the page, for tests (see DEVELOPING).
        let site = std::env::var("STREAMERZ_TIKTOK_URL").unwrap_or_else(|_| "https://www.tiktok.com".into());
        format!("{site}/@{}/live", arg(2))
    } else {
        "https://www.tiktok.com/login".into()
    };
    let event_loop = EventLoopBuilder::<Ev>::with_user_event().build();
    let window = WindowBuilder::new()
        .with_title("Streamerz: TikTok")
        .with_inner_size(LogicalSize::new(520.0, 760.0))
        .with_visible(!send || std::env::var_os("STREAMERZ_TIKTOK_SHOW").is_some())
        .build(&event_loop)
        .unwrap_or_else(|e| fail(&e.to_string()));
    let proxy = event_loop.create_proxy();
    let mut ctx = WebContext::new(Some(profile));
    let mut builder = WebViewBuilder::new_with_web_context(&mut ctx).with_url(url);
    if send {
        let p = proxy.clone();
        builder = builder
            .with_initialization_script(SEND)
            .with_ipc_handler(move |r| drop(p.send_event(Ev::Result(r.body().clone()))));
    }
    #[cfg(not(target_os = "linux"))]
    let webview = builder.build(&window);
    #[cfg(target_os = "linux")]
    let webview = {
        use tao::platform::unix::WindowExtUnix;
        use wry::WebViewBuilderExtUnix;
        builder.build_gtk(window.default_vbox().unwrap())
    };
    let webview = webview.unwrap_or_else(|e| fail(&e.to_string()));

    if send {
        let p = proxy.clone();
        std::thread::spawn(move || {
            for line in std::io::stdin().lines() {
                let _ = p.send_event(Ev::Line(line.ok()));
            }
            let _ = p.send_event(Ev::Line(None));
        });
    }

    let mut cookies = String::new();
    let mut since = Instant::now();
    let mut last = Instant::now();
    event_loop.run(move |event, _, flow| {
        *flow = ControlFlow::WaitUntil(Instant::now() + if send { IDLE } else { Duration::from_secs(1) });
        match event {
            Event::WindowEvent {
                event: WindowEvent::CloseRequested,
                ..
            } => std::process::exit(1),
            Event::UserEvent(Ev::Line(None)) => std::process::exit(0),
            Event::UserEvent(Ev::Line(Some(line))) => {
                last = Instant::now();
                let text: String = serde_json::from_str(&line).unwrap_or(line);
                let text = serde_json::to_string(&text).unwrap_or_default();
                let _ = webview.evaluate_script(&format!("window.__szSend({text})"));
            }
            Event::UserEvent(Ev::Result(r)) if send => {
                last = Instant::now();
                let _ = writeln!(std::io::stdout(), "{r}");
            }
            // Login: the home page's answer to WHO.
            Event::UserEvent(Ev::Result(r)) => {
                let (user, avatar): (String, String) = serde_json::from_str(&r).unwrap_or_default();
                if !user.is_empty() {
                    done(&cookies, &user, &avatar);
                }
            }
            Event::NewEvents(StartCause::ResumeTimeReached { .. }) if send => {
                if last.elapsed() >= IDLE {
                    std::process::exit(0);
                }
            }
            Event::NewEvents(StartCause::ResumeTimeReached { .. }) if cookies.is_empty() => {
                let jar = webview.cookies_for_url("https://www.tiktok.com/").unwrap_or_default();
                if jar.iter().any(|c| c.name() == "sessionid" && !c.value().is_empty()) {
                    let kept = jar.iter().filter(|c| KEEP.contains(&c.name()));
                    cookies = kept
                        .map(|c| format!("{}={}", c.name(), c.value()))
                        .collect::<Vec<_>>()
                        .join("; ");
                    // The login page doesn't say who you are; the home page does.
                    let _ = webview.load_url("https://www.tiktok.com/");
                    since = Instant::now();
                }
            }
            Event::NewEvents(StartCause::ResumeTimeReached { .. }) => {
                let p = proxy.clone();
                let _ = webview.evaluate_script_with_callback(WHO, move |r| {
                    let _ = p.send_event(Ev::Result(r));
                });
                // ponytail: page layout can change; after 20s give up and let them type the username
                if since.elapsed() > Duration::from_secs(20) {
                    done(&cookies, "", "");
                }
            }
            _ => {}
        }
    })
}

fn done(cookies: &str, user: &str, avatar: &str) -> ! {
    let _ = writeln!(
        std::io::stdout(),
        "{}",
        serde_json::json!({ "username": user, "cookies": cookies, "avatar": avatar })
    );
    std::process::exit(0)
}

fn fail(e: &str) -> ! {
    let _ = writeln!(std::io::stderr(), "can't open the TikTok window: {e}");
    std::process::exit(2);
}
