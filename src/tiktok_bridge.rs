//! TikTok through the user's browser. The extension loads one offscreen TikTok frame and makes
//! requests inside it, so TikTok's own scripts sign them
//! (msToken, X-Bogus, X-Gnarly, X-Dynosaur, ticket guard, csrf) exactly as when the user types in chat.
//! Streamerz opens nothing for it: the extension connects to the WebSocket server on [`PATH`] with the
//! token Streamerz wrote next to it, and its hidden context closes when Streamerz does.

use crate::engine::Engine;
use futures_util::{SinkExt, StreamExt};
use serde_json::{json, Value};
use std::cell::RefCell;
use std::collections::HashMap;
use std::path::PathBuf;
use std::rc::Rc;
use std::time::Duration;
use tokio::sync::{mpsc, oneshot};
use tokio_tungstenite::tungstenite::Message;

/// Where the extension connects, on the WebSocket server's port.
pub const PATH: &str = "/streamerz/tiktok";
/// Fixed by the public key in the extension's manifest.
pub const EXTENSION_ID: &str = "lhijoeeghjcohapolkfkacipklhbnecc";
/// The extension version this Streamerz is built with: what it writes to disk, and the signed Firefox
/// add-on it downloads. Store copies update themselves and may be newer; they work while their
/// protocol is in [`MIN_PROTOCOL`]..=[`PROTOCOL`].
pub const EXTENSION_VERSION: &str = "1.4.0";
/// The published Google Chrome Web Store extension ID.
pub const CHROME_STORE_ID: &str = "anckdakmhognoancpeefjlgicihplcjc";
/// The Chrome Web Store installation URL.
pub const CHROME_STORE_URL: &str = "https://chromewebstore.google.com/detail/anckdakmhognoancpeefjlgicihplcjc";
/// The published Microsoft Edge Add-ons product ID.
pub const EDGE_PRODUCT_ID: &str = "98d0235c-b425-4d57-82f0-26c1f0b7dc71";
/// The Microsoft Edge Add-ons installation URL.
pub const EDGE_STORE_URL: &str =
    "https://microsoftedge.microsoft.com/addons/detail/98d0235c-b425-4d57-82f0-26c1f0b7dc71";
/// The Mozilla Firefox Add-ons (AMO) extension ID.
pub const FIREFOX_GECKO_ID: &str = "tiktok-bridge@streamerz.app";
/// The messages this side speaks: the newest, and the oldest still understood. An extension outside
/// the range is refused with a reason saying which side to update.
const PROTOCOL: u64 = 2;
const MIN_PROTOCOL: u64 = 2;
/// How long one request in the tab may take (the first one opens and loads it).
const CALL: Duration = Duration::from_secs(45);
/// TikTok's chat box takes this many characters.
const MAX_CHARS: usize = 150;

/// Firefox's manifest: a persistent background page (an event page's socket dies when it idles).
const FIREFOX_MANIFEST: &[u8] = include_bytes!("../assets/tiktok-extension/manifest.firefox.json");

const FILES: [(&str, &[u8]); 6] = [
    (
        "manifest.json",
        include_bytes!("../assets/tiktok-extension/manifest.json"),
    ),
    ("bridge.js", include_bytes!("../assets/tiktok-extension/bridge.js")),
    ("icon.png", include_bytes!("../assets/tiktok-extension/icon.png")),
    (
        "offscreen.html",
        include_bytes!("../assets/tiktok-extension/offscreen.html"),
    ),
    (
        "offscreen.js",
        include_bytes!("../assets/tiktok-extension/offscreen.js"),
    ),
    ("frame.js", include_bytes!("../assets/tiktok-extension/frame.js")),
];

/// The query keys TikTok's page sends with every webcast request (browser, device, region...), as seen
/// on its own chat sends; the rest of a page request's query is particular to it.
const COMMON: [&str; 30] = [
    "WebIdLastTime",
    "aid",
    "app_language",
    "app_name",
    "browser_language",
    "browser_name",
    "browser_online",
    "browser_platform",
    "browser_version",
    "channel",
    "cookie_enabled",
    "data_collection_enabled",
    "device_id",
    "device_platform",
    "focus_state",
    "from_page",
    "history_len",
    "is_fullscreen",
    "is_page_visible",
    "os",
    "priority_region",
    "referer",
    "region",
    "root_referer",
    "screen_height",
    "screen_width",
    "tz_name",
    "user_is_login",
    "verifyFp",
    "webcast_language",
];

/// Whether a WebSocket handshake comes from a browser extension.
pub fn from_extension(origin: Option<&str>) -> bool {
    origin.is_some_and(|o| {
        let clean = o.trim_end_matches('/');
        clean.starts_with("chrome-extension://") || clean.starts_with("moz-extension://")
    })
}

/// The extension's folder (the browser loads it from there): see `extension_folder`.
pub fn folder() -> PathBuf {
    crate::extension_folder()
}

/// The same files for Firefox, with its own manifest, loaded as a temporary add-on in development.
pub fn firefox_folder() -> PathBuf {
    folder().with_file_name("tiktok-extension-firefox")
}

/// Whether version `a` ("1.2.0") comes before `b`.
pub fn older(a: &str, b: &str) -> bool {
    let parts = |v: &str| v.split('.').map(|n| n.parse::<u64>().unwrap_or(0)).collect::<Vec<_>>();
    parts(a) < parts(b)
}

/// The signed Firefox add-on of exactly [`EXTENSION_VERSION`]. Firefox installs only signed add-ons,
/// and AMO keeps an unlisted one's file private, so dev/publish-extensions.py puts each version on a
/// release of its own.
pub fn firefox_xpi_url() -> String {
    format!(
        "https://github.com/{}/releases/download/tiktok-extension-v{EXTENSION_VERSION}/streamerz-tiktok-{EXTENSION_VERSION}.xpi",
        crate::brand::REPO
    )
}

fn firefox_xpi_path() -> PathBuf {
    folder().with_file_name(format!("streamerz-tiktok-{EXTENSION_VERSION}.xpi"))
}

/// Whether `bytes` is the signed add-on of this version (Firefox checks the signature itself).
fn firefox_xpi_ok(bytes: &[u8]) -> bool {
    let Ok(files) = crate::zip::read(bytes, 16 << 20) else {
        return false;
    };
    let signed = files
        .iter()
        .any(|(name, _)| name.starts_with("META-INF/") && (name.ends_with(".rsa") || name.ends_with(".sig")));
    let manifest = files
        .iter()
        .find(|(name, _)| name == "manifest.json")
        .and_then(|(_, bytes)| serde_json::from_slice::<Value>(bytes).ok())
        .unwrap_or(Value::Null);
    signed
        && manifest["version"] == EXTENSION_VERSION
        && manifest["browser_specific_settings"]["gecko"]["id"] == FIREFOX_GECKO_ID
}

/// Downloads the signed Firefox add-on (once per version) and checks it is that add-on.
pub async fn firefox_xpi() -> Result<PathBuf, String> {
    use crate::i18n::trf;
    let path = firefox_xpi_path();
    if std::fs::read(&path).is_ok_and(|bytes| firefox_xpi_ok(&bytes)) {
        return Ok(path);
    }
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|err| format!("{}: {err}", parent.display()))?;
    }
    let part = path.with_extension("part");
    let got = crate::http::download(&firefox_xpi_url(), &[], &part, 16 << 20).await;
    let bytes = std::fs::read(&part).unwrap_or_default();
    let _ = std::fs::remove_file(&part);
    let got = got?;
    if got.status == 404 {
        return Err(trf(
            "accounts.tiktok_sending.firefox_missing",
            &[("version", EXTENSION_VERSION)],
        ));
    }
    if got.status != 200 || !got.complete || !firefox_xpi_ok(&bytes) {
        return Err(trf(
            "accounts.tiktok_sending.firefox_bad",
            &[("version", EXTENSION_VERSION)],
        ));
    }
    std::fs::write(&path, &bytes).map_err(|err| format!("{}: {err}", path.display()))?;
    Ok(path)
}

/// A development restart reuses only the pairing explicitly created by Set up.
pub(crate) fn restore_pairing(c: &mut crate::Config) {
    if c.tiktok.bridge_token.is_empty() {
        if let Ok(bytes) = std::fs::read(folder().join("config.json")) {
            if let Ok(v) = serde_json::from_slice::<Value>(&bytes) {
                if let Some(token) = v["token"]
                    .as_str()
                    .filter(|t| t.len() == 64 && t.bytes().all(|b| b.is_ascii_hexdigit()))
                {
                    c.tiktok.bridge_token = token.into();
                }
            }
        }
    }
}

/// Whether sending through the browser is set up.
pub fn set_up(c: &crate::Config) -> bool {
    !c.tiktok.bridge_token.is_empty()
}

fn new_token() -> String {
    (0..4).map(|_| format!("{:016x}", crate::math::secure_u64())).collect()
}

/// The address the extension connects to: the WebSocket server's, on this machine.
fn server_url(c: &crate::Config) -> String {
    let w = &c.websockets;
    let host = match w.address.trim() {
        "" | "0.0.0.0" | "::" | "[::]" | "*" | "+" => "127.0.0.1",
        a => a,
    };
    format!("ws://{host}:{}{PATH}", w.port)
}

/// Sets sending up (a token, if there's none yet, and the local WebSocket server it connects to) and
/// writes the extension, with its settings, to [`folder`]. Returns the folder.
pub fn install(e: &Rc<Engine>) -> Result<PathBuf, String> {
    let need_change = {
        let c = e.config();
        !set_up(&c) || !c.websockets.auto_start
    };
    if need_change {
        let token = new_token();
        e.change(|c| {
            if c.tiktok.bridge_token.is_empty() {
                c.tiktok.bridge_token = token;
            }
            c.websockets.auto_start = true;
        });
    }
    write_files(&e.config())
}

/// What follows the setup page's address so the extension can pair with this Streamerz: where its
/// WebSocket server is and the token. A copy from a store reads it there; a copy written to disk also
/// has it in its config.json.
pub fn pairing(c: &crate::Config) -> String {
    format!(
        "#url={}&token={}",
        crate::commands::url_encode(&server_url(c)),
        c.tiktok.bridge_token
    )
}

/// The local setup guide opened as a tab in the chosen browser (and closed by the extension on pair).
pub fn setup_path() -> PathBuf {
    folder().with_file_name("tiktok-setup.html")
}

fn escape_html(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for ch in s.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            _ => out.push(ch),
        }
    }
    out
}

fn escape_js(s: &str) -> String {
    serde_json::to_string(s)
        .unwrap_or_default()
        .replace('&', r"\u0026")
        .replace('<', r"\u003c")
        .replace('>', r"\u003e")
}

/// What the setup tab offers, by how the extension gets into the browser.
pub enum Setup<'a> {
    /// Chromium: its store, or the copy Streamerz keeps, loaded unpacked.
    Chromium { store: &'a str },
    /// Firefox: the signed add-on Streamerz downloaded; Firefox asks to add it.
    Firefox { addon: &'a std::path::Path },
    /// Firefox in development: the copy Streamerz keeps, as a temporary add-on.
    FirefoxTemporary,
}

/// One step of the setup tab: its text, then a value to copy or a link to follow.
struct Step {
    text: String,
    copy: Option<(String, String)>,
    link: Option<(String, String)>,
}

fn setup_html(browser: &str, extensions_page: &str, ext_folder: &std::path::Path, setup: &Setup) -> String {
    use crate::i18n::{tr, trf};
    let b = [("browser", browser)];
    let folder = ext_folder.display().to_string();
    let copy_addr = tr("accounts.tiktok_sending.page_copy_address").to_owned();
    let copy_dir = tr("accounts.tiktok_sending.page_copy_folder").to_owned();
    let step = |text: String| Step {
        text,
        copy: None,
        link: None,
    };
    let manual = |open: String, load: String, page: &str| {
        [
            Step {
                copy: Some((page.to_owned(), copy_addr.clone())),
                ..step(open)
            },
            Step {
                copy: Some((folder.clone(), copy_dir.clone())),
                ..step(load)
            },
        ]
    };
    let (lead, steps): (String, Vec<Step>) = match setup {
        Setup::Chromium { store } => {
            let mut steps = Vec::new();
            if !store.is_empty() {
                steps.push(Step {
                    link: Some((
                        (*store).to_owned(),
                        trf("accounts.tiktok_sending.page_store_button", &b),
                    )),
                    ..step(trf("accounts.tiktok_sending.page_store", &b))
                });
            }
            steps.extend(manual(
                trf("accounts.tiktok_sending.page_step_open", &b),
                tr("accounts.tiktok_sending.page_step_load").to_owned(),
                extensions_page,
            ));
            (tr("accounts.tiktok_sending.page_lead").to_owned(), steps)
        }
        Setup::Firefox { addon } => (
            trf("accounts.tiktok_sending.page_firefox_lead", &b),
            vec![Step {
                link: Some((
                    crate::browsers::file_url(addon),
                    trf("accounts.tiktok_sending.page_firefox_button", &b),
                )),
                ..step(trf("accounts.tiktok_sending.page_firefox_again", &b))
            }],
        ),
        Setup::FirefoxTemporary => (
            trf("accounts.tiktok_sending.page_firefox_temp_lead", &b),
            manual(
                tr("accounts.tiktok_sending.page_firefox_temp_open").to_owned(),
                tr("accounts.tiktok_sending.page_firefox_temp_load").to_owned(),
                FIREFOX_DEBUGGING,
            )
            .into(),
        ),
    };
    let title = escape_html(&trf("accounts.tiktok_sending.page_title", &b));
    let lead = escape_html(&lead);
    let copied_js = escape_js(tr("accounts.tiktok_sending.page_copied"));
    let mut sections = String::new();
    let mut binds = String::new();
    for (index, step) in steps.iter().enumerate() {
        sections += &format!("  <section>\n    <p class=\"step\">{}</p>\n", escape_html(&step.text));
        if let Some((value, label)) = &step.copy {
            let value_html = escape_html(value);
            sections += &format!(
                "    <div class=\"bar\"><code title=\"{value_html}\">{value_html}</code><button type=\"button\" id=\"copy-{index}\">{}</button></div>\n",
                escape_html(label)
            );
            binds += &format!("  bind(\"copy-{index}\", {});\n", escape_js(value));
        }
        if let Some((href, label)) = &step.link {
            sections += &format!(
                "    <div class=\"bar\"><a class=\"go\" href=\"{}\" target=\"_blank\" rel=\"noopener noreferrer\">{}</a></div>\n",
                escape_html(href),
                escape_html(label)
            );
        }
        sections += "  </section>\n";
    }
    format!(
        r#"<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width,initial-scale=1">
<title>{title}</title>
<style>
  :root {{ color-scheme: dark light; --bg:#09090b; --card:#18181b; --line:#27272a; --ink:#fafafa; --muted:#a1a1aa; --btn:#27272a; --btn-hover:#3f3f46; }}
  @media (prefers-color-scheme: light) {{
    :root {{ --bg:#fafafa; --card:#ffffff; --line:#e4e4e7; --ink:#09090b; --muted:#52525b; --btn:#f4f4f5; --btn-hover:#e4e4e7; }}
  }}
  * {{ box-sizing:border-box; }}
  body {{ margin:0; min-height:100vh; display:flex; align-items:center; justify-content:center; padding:24px; background:var(--bg); color:var(--ink); font:14px/1.5 system-ui,-apple-system,Segoe UI,Roboto,sans-serif; }}
  main {{ width:100%; max-width:620px; background:var(--card); border:1px solid var(--line); border-radius:12px; padding:24px; }}
  h1 {{ margin:0 0 6px; font-size:18px; font-weight:600; }}
  p.lead {{ margin:0 0 20px; color:var(--muted); }}
  section {{ border-top:1px solid var(--line); padding-top:16px; margin-top:16px; }}
  p.step {{ margin:0 0 10px; }}
  .bar {{ display:flex; gap:8px; align-items:center; }}
  code {{ flex:1; min-width:0; overflow:hidden; text-overflow:ellipsis; white-space:nowrap; padding:8px 10px; background:var(--bg); border:1px solid var(--line); border-radius:6px; font:13px/1.3 ui-monospace,SFMono-Regular,Consolas,monospace; }}
  button, a.go {{ flex:none; padding:8px 14px; background:var(--btn); color:var(--ink); border:1px solid var(--line); border-radius:6px; font:inherit; font-weight:500; cursor:pointer; text-decoration:none; }}
  button:hover, a.go:hover {{ background:var(--btn-hover); }}
</style>
</head>
<body>
<main>
  <h1>{title}</h1>
  <p class="lead">{lead}</p>
{sections}</main>
<script>
  const copied = {copied_js};
  function bind(id, text) {{
    const btn = document.getElementById(id);
    const label = btn.textContent;
    btn.addEventListener("click", async () => {{
      try {{
        await navigator.clipboard.writeText(text);
      }} catch {{
        const t = document.createElement("textarea");
        t.value = text;
        document.body.appendChild(t);
        t.select();
        document.execCommand("copy");
        t.remove();
      }}
      btn.textContent = copied;
      setTimeout(() => {{ btn.textContent = label; }}, 1400);
    }});
  }}
{binds}</script>
</body>
</html>
"#
    )
}

/// Firefox's page for temporary add-ons.
const FIREFOX_DEBUGGING: &str = "about:debugging#/runtime/this-firefox";

pub fn write_setup_page(
    browser: &str,
    extensions_page: &str,
    ext_folder: &std::path::Path,
    setup: &Setup,
) -> Result<PathBuf, String> {
    let path = setup_path();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|err| format!("can't create {}: {err}", parent.display()))?;
    }
    let html = setup_html(browser, extensions_page, ext_folder, setup);
    write_private(&path, html.as_bytes()).map_err(|err| format!("can't write {}: {err}", path.display()))?;
    Ok(path)
}

fn write_files(c: &crate::Config) -> Result<PathBuf, String> {
    write_dir(&folder(), c, None)
}

/// Writes the Firefox copy (its own manifest) for a temporary add-on. Returns its folder.
pub fn write_firefox_files(c: &crate::Config) -> Result<PathBuf, String> {
    write_dir(&firefox_folder(), c, Some(FIREFOX_MANIFEST))
}

fn write_dir(dir: &std::path::Path, c: &crate::Config, manifest: Option<&[u8]>) -> Result<PathBuf, String> {
    std::fs::create_dir_all(dir).map_err(|err| format!("can't create {}: {err}", dir.display()))?;
    let settings = json!({"url": server_url(c), "token": c.tiktok.bridge_token, "build": build()});
    let config = format!("self.STREAMERZ = {settings};\n");
    let config_json = settings.to_string();
    let files = FILES
        .iter()
        .map(|(n, b)| {
            (
                *n,
                if *n == "manifest.json" {
                    manifest.unwrap_or(b)
                } else {
                    *b
                },
            )
        })
        .chain([
            ("config.js", config.as_bytes()),
            ("config.json", config_json.as_bytes()),
        ]);
    for (name, bytes) in files {
        let path = dir.join(name);
        // Unchanged files stay as they are: the browser notices nothing.
        if std::fs::read(&path).is_ok_and(|old| old == bytes) {
            continue;
        }
        write_private(&path, bytes).map_err(|err| format!("can't write {}: {err}", path.display()))?;
    }
    Ok(dir.to_path_buf())
}

/// A fingerprint of the extension's files: the extension restarts itself when it changes (a new
/// Streamerz wrote new ones), so the browser never runs old code against this version.
fn build() -> String {
    use sha2::Digest;
    let mut hash = sha2::Sha256::new();
    for (name, bytes) in FILES {
        hash.update(name.as_bytes());
        hash.update(bytes);
    }
    hash.update(FIREFOX_MANIFEST);
    hash.finalize()[..6].iter().map(|b| format!("{b:02x}")).collect()
}

/// Only the owner can read them: config.js holds the token.
fn write_private(path: &std::path::Path, bytes: &[u8]) -> std::io::Result<()> {
    use std::io::Write;
    let mut o = std::fs::OpenOptions::new();
    o.write(true).create(true).truncate(true);
    #[cfg(unix)]
    std::os::unix::fs::OpenOptionsExt::mode(&mut o, 0o600);
    o.open(path)?.write_all(bytes)
}

/// At start, and when the WebSocket server's address changes: the extension's files follow this
/// version and the server's address (the browser reads them when it starts the extension).
pub fn refresh(c: &crate::Config) {
    if set_up(c) && folder().is_dir() {
        if let Err(err) = write_files(c) {
            crate::warn(format!("TikTok extension: {err}"));
        }
    }
    if set_up(c) && firefox_folder().is_dir() {
        if let Err(err) = write_firefox_files(c) {
            crate::warn(format!("TikTok extension: {err}"));
        }
    }
}

/// Turns sending off: the token goes (the extension is refused from then on) and so does its folder.
pub fn remove(e: &Rc<Engine>) {
    e.change(|c| c.tiktok.bridge_token.clear());
    e.tiktok_bridge.disconnect();
    let _ = std::fs::remove_dir_all(folder());
    let _ = std::fs::remove_dir_all(firefox_folder());
    let _ = std::fs::remove_file(firefox_xpi_path());
    let _ = std::fs::remove_file(setup_path());
}

/// For the MCP tool and the WebSocket request.
pub fn status(e: &Engine) -> Value {
    let b = &e.tiktok_bridge;
    json!({
        "setUp": set_up(&e.config()),
        "connected": b.is_connected(),
        "user": b.user(),
        "avatar": b.avatar(),
        "ready": b.ready(),
        "loginRequired": b.login_required(),
        "error": b.error(),
        "version": b.version(),
        "latestVersion": EXTENSION_VERSION,
        "outdated": b.is_outdated(),
        "refused": b.refused(),
        "folder": folder().display().to_string(),
        "extensionId": EXTENSION_ID
    })
}

struct Link {
    tx: mpsc::UnboundedSender<String>,
    pending: HashMap<u64, oneshot::Sender<Value>>,
    next: u64,
    /// Which connection this is (a newer one replaces it).
    serial: u64,
}

struct PendingCall<'a> {
    bridge: &'a Bridge,
    serial: u64,
    id: u64,
}
impl Drop for PendingCall<'_> {
    fn drop(&mut self) {
        if let Some(link) = self
            .bridge
            .link
            .borrow_mut()
            .as_mut()
            .filter(|link| link.serial == self.serial)
        {
            link.pending.remove(&self.id);
        }
    }
}

/// The connected extension, if any, and what was learnt from its tab.
#[derive(Default)]
pub struct Bridge {
    link: RefCell<Option<Link>>,
    serial: std::cell::Cell<u64>,
    /// The page's common query, once read.
    params: RefCell<Option<Vec<(String, String)>>>,
    /// The browser's TikTok account (its @handle), once known.
    user: RefCell<String>,
    uid: RefCell<String>,
    avatar: RefCell<String>,
    version: RefCell<String>,
    ready: std::cell::Cell<bool>,
    login_required: std::cell::Cell<bool>,
    error: RefCell<String>,
    /// Why the last extension that tried was turned away (another protocol), until one connects.
    refused: RefCell<String>,
    warming: std::cell::Cell<bool>,
    refresh_pending: std::cell::Cell<bool>,
}

impl Bridge {
    pub fn is_connected(&self) -> bool {
        self.link.borrow().is_some()
    }

    /// The TikTok account logged in to the browser ("" until known).
    pub fn user(&self) -> String {
        self.user.borrow().clone()
    }

    pub fn avatar(&self) -> String {
        self.avatar.borrow().clone()
    }

    pub fn version(&self) -> String {
        self.version.borrow().clone()
    }

    /// Older than the version this Streamerz is built with (a newer store copy is fine).
    pub fn is_outdated(&self) -> bool {
        let v = self.version.borrow();
        !v.is_empty() && older(&v, EXTENSION_VERSION)
    }

    pub fn refused(&self) -> String {
        self.refused.borrow().clone()
    }

    pub fn ready(&self) -> bool {
        self.ready.get()
    }
    pub fn login_required(&self) -> bool {
        self.login_required.get()
    }
    pub fn error(&self) -> String {
        self.error.borrow().clone()
    }
    pub fn uid(&self) -> String {
        self.uid.borrow().clone()
    }
    #[cfg(test)]
    pub(crate) fn test_account(&self, user: &str, uid: &str) {
        *self.user.borrow_mut() = user.into();
        *self.uid.borrow_mut() = uid.into();
    }

    /// Browser cookies stay in memory and are requested only when the reader connects.
    pub async fn session(&self) -> String {
        if !self.is_connected() {
            return String::new();
        }
        self.call(json!({"op": "session"}))
            .await
            .ok()
            .and_then(|v| v["cookie"].as_str().map(str::to_string))
            .unwrap_or_default()
    }

    pub async fn login(&self) -> Result<(), String> {
        self.call(json!({"op": "login"})).await.map(drop)
    }

    fn disconnect(&self) {
        crate::tiktok::gifts::invalidate();
        // Dropping the sender ends the connection's task, which closes the socket.
        self.link.borrow_mut().take();
        self.params.borrow_mut().take();
        self.user.borrow_mut().clear();
        self.uid.borrow_mut().clear();
        self.avatar.borrow_mut().clear();
        self.version.borrow_mut().clear();
        crate::roles::clear("tiktok");
        self.ready.set(false);
        self.login_required.set(false);
        self.warming.set(false);
        self.refresh_pending.set(false);
        self.error.borrow_mut().clear();
        crate::host::changed();
    }

    /// One request to the extension; its answer, or its error.
    async fn call(&self, mut msg: Value) -> Result<Value, String> {
        let (rx, id, serial) = {
            let mut link = self.link.borrow_mut();
            let l = link.as_mut().ok_or(NOT_CONNECTED)?;
            l.next += 1;
            let id = l.next;
            msg["id"] = id.into();
            let (tx, rx) = oneshot::channel();
            l.pending.insert(id, tx);
            if l.tx.send(msg.to_string()).is_err() {
                l.pending.remove(&id);
                return Err(NOT_CONNECTED.into());
            }
            (rx, id, l.serial)
        };
        let _pending = PendingCall {
            bridge: self,
            serial,
            id,
        };
        let v = match tokio::time::timeout(CALL, rx).await {
            Ok(Ok(v)) => v,
            Ok(Err(_)) => return Err(NOT_CONNECTED.into()),
            Err(_) => {
                if let Some(l) = self.link.borrow_mut().as_mut() {
                    l.pending.retain(|_, tx| !tx.is_closed());
                }
                self.ready.set(false);
                crate::host::changed();
                return Err("TikTok didn't answer. Try again, or check Accounts > TikTok.".into());
            }
        };
        match v.get("error").and_then(Value::as_str) {
            Some(err) => {
                self.ready.set(false);
                self.login_required
                    .set(v["code"] == "login_required" || err.starts_with("Log in to TikTok"));
                *self.error.borrow_mut() = err.to_string();
                crate::host::changed();
                Err(err.to_string())
            }
            None => {
                if v.get("params").is_some() || v.get("status").is_some() {
                    self.ready.set(true);
                    self.error.borrow_mut().clear();
                    crate::host::changed();
                }
                Ok(v)
            }
        }
    }

    /// The page's common query, kept until the session changes or a send fails.
    async fn params(&self) -> Result<Vec<(String, String)>, String> {
        if let Some(p) = self.params.borrow().clone() {
            return Ok(p);
        }
        let v = self.call(json!({"op": "page"})).await?;
        let p = common_params(&v["params"]);
        if p.iter().all(|(k, _)| k != "device_id") {
            return Err("TikTok's page didn't say which device it is".into());
        }
        *self.params.borrow_mut() = Some(p.clone());
        Ok(p)
    }

    /// A request made inside the hidden frame: (HTTP status, body as JSON).
    async fn fetch(&self, url: &str, init: Value) -> Result<(u64, Value), String> {
        let v = self.call(json!({"op": "fetch", "url": url, "init": init})).await?;
        let status = v["status"].as_u64().unwrap_or(0);
        let body = v["body"].as_str().unwrap_or("");
        let json = serde_json::from_str(body).unwrap_or(Value::Null);
        Ok((status, json))
    }
}

const NOT_CONNECTED: &str = "the Streamerz for TikTok extension isn't connected (Accounts > TikTok)";

pub(crate) async fn gift_list(e: &Rc<Engine>, room: &str) -> Result<Value, String> {
    let b = &e.tiktok_bridge;
    let mut params = b.params().await?;
    params.retain(|(key, _)| key != "room_id");
    params.push(("room_id".into(), room.into()));
    let query = params
        .iter()
        .map(|(k, v)| format!("{}={}", encode(k), encode(v)))
        .collect::<Vec<_>>()
        .join("&");
    let url = format!("https://webcast.tiktok.com/webcast/gift/list/?{query}");
    let (status, body) = b.fetch(&url, json!({})).await?;
    if status != 200 {
        return Err(format!("TikTok gift list answered HTTP {status}"));
    }
    Ok(body)
}

/// The common keys of a page request's query, in the page's order.
fn common_params(v: &Value) -> Vec<(String, String)> {
    let Some(o) = v.as_object() else { return Vec::new() };
    o.iter()
        .filter(|(k, _)| COMMON.contains(&k.as_str()))
        .map(|(k, v)| (k.clone(), v.as_str().unwrap_or_default().to_string()))
        .collect()
}

fn encode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => out.push(b as char),
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

/// TikTok's chat send, as its live page makes it: (URL, JSON body). The page's common query, with the
/// live's page as referer, then the message (TikTok's page puts it in both).
pub fn chat_request(
    params: &[(String, String)],
    channel: &str,
    room: &str,
    text: &str,
    now_ms: u64,
) -> (String, String) {
    let page = format!("https://www.tiktok.com/@{channel}/live");
    let mut query: Vec<(&str, String)> = params
        .iter()
        .map(|(k, v)| {
            let v = match k.as_str() {
                "referer" | "root_referer" => page.clone(),
                // A message is only ever typed into a page that's in front.
                "focus_state" | "is_page_visible" => "true".into(),
                _ => v.clone(),
            };
            (k.as_str(), v)
        })
        .collect();
    let now = now_ms.to_string();
    query.extend([
        ("room_id", room.to_string()),
        ("content", text.to_string()),
        ("emotes_with_index", String::new()),
        ("input_type", "0".into()),
        ("client_start_timestamp_millisecond", now),
    ]);
    query.sort_by(|a, b| a.0.cmp(b.0));
    let q: Vec<String> = query.iter().map(|(k, v)| format!("{k}={}", encode(v))).collect();
    let url = format!("https://webcast.tiktok.com/webcast/room/chat/?{}", q.join("&"));
    let body = json!({"room_id": room, "content": text, "emotes_with_index": "", "input_type": 0,
        "client_start_timestamp_millisecond": now_ms});
    (url, body.to_string())
}

/// TikTok's answer to a request: Ok, or why it refused.
fn answer(status: u64, body: &Value) -> Result<(), String> {
    match body["status_code"].as_i64() {
        Some(0) => Ok(()),
        Some(code) => {
            let d = &body["data"];
            let why = [&d["prompts"], &d["message"], &body["message"]]
                .into_iter()
                .filter_map(Value::as_str)
                .find(|s| !s.is_empty())
                .map_or_else(|| format!("error {code}"), str::to_string);
            Err(format!("TikTok: {why}"))
        }
        None if status != 200 => Err(format!("TikTok answered HTTP {status}")),
        None => Err("TikTok's answer was empty".into()),
    }
}

/// Native sticker encoding is not implemented. Never post an internal picture ID as chat text.
pub(crate) fn validate_message(text: &str) -> Result<(), String> {
    if text.trim().is_empty() {
        Err(crate::i18n::tr("chat.tiktok_empty").to_string())
    } else if text.chars().count() > MAX_CHARS {
        Err(crate::i18n::tr("chat.tiktok_too_long").to_string())
    } else if text.split("[tiktok:").skip(1).any(|tail| tail.contains(']')) {
        Err(crate::i18n::tr("chat.tiktok_stickers_unavailable").to_string())
    } else {
        Ok(())
    }
}

/// Sends `text` to the live being read, as the browser's TikTok account.
pub async fn send_message(e: &Rc<Engine>, text: &str) -> Result<(), String> {
    validate_message(text)?;
    if !e.tiktok_connected() {
        return Err("TikTok isn't connected to a live".into());
    }
    let room = e
        .tiktok()
        .and_then(|t| t.room_id())
        .ok_or("TikTok isn't connected to a live")?;
    let channel = crate::tiktok::clean_tiktok_channel(&e.config().tiktok.channel);
    let b = &e.tiktok_bridge;
    let params = b.params().await?;
    let (url, body) = chat_request(&params, &channel, &room, text, crate::templates::clock::now_ms());
    let init = json!({"method": "POST", "headers": {"content-type": "application/json; charset=UTF-8"}, "body": body});
    let (status, json) = b.fetch(&url, init).await?;
    let r = answer(status, &json);
    if r.is_err() {
        // Read the page's query again next time, in case it was what TikTok didn't like.
        b.params.borrow_mut().take();
    }
    r
}

/// Sets stream title on TikTok LIVE for the broadcaster.
pub async fn set_stream_title(e: &Rc<Engine>, title: &str) -> Result<(), String> {
    if !e.tiktok_connected() {
        return Err("TikTok isn't connected to a live".into());
    }
    let room = e
        .tiktok()
        .and_then(|t| t.room_id())
        .ok_or("TikTok isn't connected to a live")?;
    let b = &e.tiktok_bridge;
    let params = b.params().await?;
    let mut query: Vec<(&str, String)> = params.iter().map(|(k, v)| (k.as_str(), v.clone())).collect();
    query.push(("room_id", room.clone()));
    query.sort_by(|a, b| a.0.cmp(b.0));
    let q: Vec<String> = query.iter().map(|(k, v)| format!("{k}={}", encode(v))).collect();
    let url = format!("https://webcast.tiktok.com/webcast/room/update_room/?{}", q.join("&"));
    let body = json!({"room_id": room, "title": title});
    let init = json!({"method": "POST", "headers": {"content-type": "application/json; charset=UTF-8"}, "body": body.to_string()});
    let (status, json) = b.fetch(&url, init).await?;
    answer(status, &json)
}

/// Pins a chat comment in TikTok LIVE chat.
pub async fn pin_comment(e: &Rc<Engine>, comment_id: &str) -> Result<(), String> {
    if !e.tiktok_connected() {
        return Err("TikTok isn't connected to a live".into());
    }
    let room = e
        .tiktok()
        .and_then(|t| t.room_id())
        .ok_or("TikTok isn't connected to a live")?;
    let b = &e.tiktok_bridge;
    let params = b.params().await?;
    let mut query: Vec<(&str, String)> = params.iter().map(|(k, v)| (k.as_str(), v.clone())).collect();
    query.push(("room_id", room.clone()));
    query.sort_by(|a, b| a.0.cmp(b.0));
    let q: Vec<String> = query.iter().map(|(k, v)| format!("{k}={}", encode(v))).collect();
    let url = format!("https://webcast.tiktok.com/webcast/room/pin_comment/?{}", q.join("&"));
    let body = json!({"room_id": room, "comment_id": comment_id, "action_type": 1});
    let init = json!({"method": "POST", "headers": {"content-type": "application/json; charset=UTF-8"}, "body": body.to_string()});
    let (status, json) = b.fetch(&url, init).await?;
    answer(status, &json)
}

/// Unpins the current pinned comment in TikTok LIVE chat.
pub async fn unpin_comment(e: &Rc<Engine>) -> Result<(), String> {
    if !e.tiktok_connected() {
        return Err("TikTok isn't connected to a live".into());
    }
    let room = e
        .tiktok()
        .and_then(|t| t.room_id())
        .ok_or("TikTok isn't connected to a live")?;
    let b = &e.tiktok_bridge;
    let params = b.params().await?;
    let mut query: Vec<(&str, String)> = params.iter().map(|(k, v)| (k.as_str(), v.clone())).collect();
    query.push(("room_id", room.clone()));
    query.sort_by(|a, b| a.0.cmp(b.0));
    let q: Vec<String> = query.iter().map(|(k, v)| format!("{k}={}", encode(v))).collect();
    let url = format!("https://webcast.tiktok.com/webcast/room/pin_comment/?{}", q.join("&"));
    let body = json!({"room_id": room, "action_type": 2});
    let init = json!({"method": "POST", "headers": {"content-type": "application/json; charset=UTF-8"}, "body": body.to_string()});
    let (status, json) = b.fetch(&url, init).await?;
    answer(status, &json)
}

/// The public reader's room endpoint, signed by the browser when public lookup fails.
pub(crate) async fn room_id(e: &Rc<Engine>, channel: &str) -> Result<u64, crate::webcast::RoomLookupError> {
    use crate::webcast::RoomLookupError;
    let b = &e.tiktok_bridge;
    let url = crate::webcast::user_room_url(channel).replace("user_is_login=false", "user_is_login=true");
    let (status, body) = b.fetch(&url, json!({})).await.map_err(RoomLookupError::Transient)?;
    if status != 200 {
        return Err(RoomLookupError::Transient(format!("TikTok answered HTTP {status}")));
    }
    crate::webcast::room_from_response(channel, &body, b.user().eq_ignore_ascii_case(channel)).await
}

/// Search uses TikTok's signed web request, in the same hidden context as sending.
pub async fn search(e: &Rc<Engine>, query: &str) -> Result<Vec<crate::search::Found>, String> {
    let v = e
        .tiktok_bridge
        .call(json!({"op": "search", "query": query.trim()}))
        .await?;
    let body: Value = serde_json::from_str(v["body"].as_str().unwrap_or_default())
        .map_err(|_| "TikTok's search didn't answer. Try again.".to_string())?;
    answer(v["status"].as_u64().unwrap_or(0), &body)?;
    Ok(crate::search::tiktok_found(&body))
}

/// Explicit recovery reloads identity and signing state, never a chat send.
pub(crate) fn retry(e: &Rc<Engine>) -> Result<(), String> {
    if !e.tiktok_bridge.is_connected() {
        return Err(NOT_CONNECTED.into());
    }
    warm(e, e.tiktok_bridge.serial.get());
    Ok(())
}

fn warm(e: &Rc<Engine>, serial: u64) {
    let b = &e.tiktok_bridge;
    if b.warming.replace(true) {
        b.refresh_pending.set(true);
        return;
    }
    crate::tiktok::gifts::invalidate();
    b.ready.set(false);
    b.login_required.set(false);
    let previous = b.user();
    b.user.borrow_mut().clear();
    b.uid.borrow_mut().clear();
    b.avatar.borrow_mut().clear();
    crate::roles::clear("tiktok");
    b.params.borrow_mut().take();
    b.error.borrow_mut().clear();
    crate::host::changed();
    let e = e.clone();
    tokio::task::spawn_local(async move {
        let b = &e.tiktok_bridge;
        let r = async {
            b.params().await?;
            let (status, v) = b
                .fetch("https://www.tiktok.com/passport/web/account/info/?aid=1988", json!({}))
                .await?;
            if status != 200 {
                return Err(format!("TikTok answered HTTP {status}"));
            }
            Ok(v)
        }
        .await;
        if b.serial.get() != serial || !b.is_connected() {
            return;
        }
        match r {
            Ok(v) => {
                *b.user.borrow_mut() = v["data"]["username"].as_str().unwrap_or_default().to_string();
                *b.uid.borrow_mut() = v["data"]["user_id_str"].as_str().unwrap_or_default().to_string();
                *b.avatar.borrow_mut() = v["data"]["avatar_url"]
                    .as_str()
                    .filter(|u| u.starts_with("https://"))
                    .unwrap_or_default()
                    .to_string();
                let user = b.user();
                if user.is_empty() {
                    b.uid.borrow_mut().clear();
                    b.login_required.set(true);
                }
                // Passport sometimes omits the avatar. Reuse signed search once, not an idle poll.
                if b.avatar().is_empty() && !user.is_empty() {
                    let picture = search(&e, &user)
                        .await
                        .ok()
                        .and_then(|found| found.into_iter().find(|p| p.handle.eq_ignore_ascii_case(&user)))
                        .map(|p| p.picture)
                        .filter(|url| url.starts_with("https://"))
                        .unwrap_or_default();
                    if b.serial.get() != serial || !b.is_connected() {
                        return;
                    }
                    *b.avatar.borrow_mut() = picture;
                }
                b.error.borrow_mut().clear();
                b.ready.set(true);
                let _ = crate::tiktok::gifts::refresh(&e).await;
                e.settle_role("tiktok");
                let user = b.user();
                if !user.is_empty() && e.config().tiktok.channel.trim().is_empty() {
                    e.change(|c| c.tiktok.channel = user);
                    if !e.dev() && !e.config().tiktok.manual_connect {
                        e.tiktok_start().send(crate::tiktok::TikTokCmd::Connect);
                    }
                }
                if previous != b.user() && (e.tiktok_connected() || e.tiktok_standby()) {
                    if let Some(t) = e.tiktok() {
                        t.send(crate::tiktok::TikTokCmd::Connect);
                    }
                }
            }
            Err(err) => {
                b.ready.set(false);
                crate::warn(format!("TikTok extension: {err}"));
                *b.error.borrow_mut() = err;
                if !previous.is_empty() && (e.tiktok_connected() || e.tiktok_standby()) {
                    if let Some(t) = e.tiktok() {
                        t.send(crate::tiktok::TikTokCmd::Connect);
                    }
                }
            }
        }
        crate::host::changed();
        b.warming.set(false);
        if b.refresh_pending.replace(false) {
            warm(&e, serial);
        }
    });
}

fn same(a: &str, b: &str) -> bool {
    a.len() == b.len() && a.bytes().zip(b.bytes()).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

/// Why an extension's hello can't be served: a protocol this Streamerz doesn't speak, and which side
/// to update.
fn refusal(hello: &Value) -> Option<String> {
    use crate::i18n::trf;
    let protocol = hello["protocol"].as_u64().unwrap_or(0);
    let version = hello["version"].as_str().unwrap_or("?");
    if protocol > PROTOCOL {
        Some(trf(
            "accounts.tiktok_sending.streamerz_too_old",
            &[("version", version)],
        ))
    } else if protocol < MIN_PROTOCOL {
        Some(trf(
            "accounts.tiktok_sending.extension_too_old",
            &[("version", version), ("latest", EXTENSION_VERSION)],
        ))
    } else {
        None
    }
}

/// One connection from the extension, until it closes. The first message must be its hello, with
/// this install's token.
pub async fn serve<S>(e: Rc<Engine>, ws: tokio_tungstenite::WebSocketStream<S>)
where
    S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin,
{
    let (mut sink, mut stream) = ws.split();
    let hello = match tokio::time::timeout(Duration::from_secs(10), stream.next()).await {
        Ok(Some(Ok(Message::Text(t)))) => serde_json::from_str::<Value>(&t).unwrap_or(Value::Null),
        _ => return,
    };
    let token = e.config().tiktok.bridge_token.clone();
    if token.is_empty() || hello["op"] != "hello" || !same(hello["token"].as_str().unwrap_or(""), &token) {
        crate::warn(
            "TikTok extension: refused a connection with a wrong token (set it up again in Accounts > TikTok)".into(),
        );
        let _ = sink.close().await;
        return;
    }
    if let Some(why) = refusal(&hello) {
        crate::warn(format!("TikTok extension: {why}"));
        *e.tiktok_bridge.refused.borrow_mut() = why;
        crate::host::changed();
        let _ = sink.close().await;
        return;
    }
    let b = &e.tiktok_bridge;
    b.refused.borrow_mut().clear();
    let serial = b.serial.get() + 1;
    b.serial.set(serial);
    let (tx, mut rx) = mpsc::unbounded_channel::<String>();
    b.disconnect();
    let ver = hello["version"].as_str().unwrap_or("").to_string();
    *b.version.borrow_mut() = ver;
    *b.link.borrow_mut() = Some(Link {
        tx,
        pending: HashMap::new(),
        next: 0,
        serial,
    });
    crate::say("TikTok: the browser extension connected".into());
    crate::host::changed();
    warm(&e, serial);
    loop {
        tokio::select! {
            out = rx.recv() => match out {
                Some(text) => if sink.send(Message::Text(text.into())).await.is_err() { break },
                None => break,
            },
            m = stream.next() => match m {
                Some(Ok(Message::Text(t))) => {
                    let Ok(v) = serde_json::from_str::<Value>(&t) else { continue };
                    if v["op"] == "session_changed" { warm(&e, serial); continue; }
                    let Some(id) = v["id"].as_u64() else { continue };
                    let waiting = b.link.borrow_mut().as_mut().and_then(|l| l.pending.remove(&id));
                    if let Some(w) = waiting {
                        let _ = w.send(v);
                    }
                }
                Some(Ok(Message::Close(_))) | None | Some(Err(_)) => break,
                _ => {}
            },
        }
    }
    let _ = sink.close().await;
    if b.link.borrow().as_ref().is_some_and(|l| l.serial == serial) {
        b.disconnect();
        crate::say("TikTok: the browser extension disconnected".into());
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn the_setup_page_link_carries_where_streamerz_is_and_its_token() {
        let mut c = crate::Config::default();
        c.websockets.address = "0.0.0.0".into();
        c.websockets.port = 8080;
        c.tiktok.bridge_token = "ab".repeat(32);
        let link = super::pairing(&c);
        assert!(link.starts_with("#url="), "{link}");
        assert!(link.contains("127.0.0.1%3A8080%2Fstreamerz%2Ftiktok"), "{link}");
        assert!(link.ends_with(&format!("&token={}", "ab".repeat(32))));
    }

    use super::*;

    #[test]
    fn cancelling_a_browser_read_releases_pending_state_and_keeps_replacement_connections() {
        tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap().block_on(async {
            let bridge = Bridge::default();
            let (send, mut wire) = mpsc::unbounded_channel();
            *bridge.link.borrow_mut() = Some(Link { tx: send.clone(), pending: HashMap::new(), next: 0, serial: 1 });
            let mut request = Box::pin(bridge.call(json!({"op":"fetch"})));
            tokio::select! { _ = &mut request => panic!("request should wait"), _ = wire.recv() => {} }
            assert_eq!(bridge.link.borrow().as_ref().unwrap().pending.len(), 1);
            drop(request);
            assert!(bridge.link.borrow().as_ref().unwrap().pending.is_empty());
            let mut request = Box::pin(bridge.call(json!({"op":"fetch"})));
            let message = tokio::select! { _ = &mut request => panic!("request should wait"), message = wire.recv() => message.unwrap() };
            let id = serde_json::from_str::<Value>(&message).unwrap()["id"].as_u64().unwrap();
            let (reply, _received) = oneshot::channel();
            *bridge.link.borrow_mut() = Some(Link { tx: send, pending: HashMap::from([(id, reply)]), next: id, serial: 2 });
            drop(request);
            assert!(bridge.link.borrow().as_ref().unwrap().pending.contains_key(&id));
        });
    }

    fn params() -> Vec<(String, String)> {
        let page = json!({"aid": "1988", "device_id": "42", "channel_id": "86", "max_time": "0",
            "referer": "https://www.tiktok.com/live/history/center", "focus_state": "false",
            "is_page_visible": "false", "browser_version": "5.0 (X11; Linux x86_64)",
            "msToken": "m", "X-Bogus": "b", "X-Gnarly": "g", "X-Dynosaur": "d"});
        common_params(&page)
    }

    #[test]
    fn native_sticker_tokens_cannot_leak_into_outgoing_text() {
        assert!(validate_message("hello 🙂 :)").is_ok());
        assert!(validate_message("[tiktok:123]").is_err());
        assert!(validate_message("hello[tiktok:123]!").is_err());
    }

    #[test]
    fn messages_are_validated_without_truncating_unicode() {
        assert!(validate_message("").is_err());
        assert!(validate_message(" \n ").is_err());
        assert!(validate_message(&"🙂".repeat(150)).is_ok());
        assert!(validate_message(&"🙂".repeat(151)).is_err());
        assert!(validate_message(&format!("@creator {}", "a".repeat(150))).is_err());
    }

    #[test]
    fn keeps_only_the_common_query() {
        let keys: Vec<_> = params().into_iter().map(|(k, _)| k).collect();
        assert_eq!(
            keys,
            [
                "aid",
                "device_id",
                "referer",
                "focus_state",
                "is_page_visible",
                "browser_version"
            ]
        );
    }

    #[test]
    fn chat_request_is_the_pages() {
        let (url, body) = chat_request(&params(), "some.one", "7704", "hi & ção", 1_700_000_000_123);
        let u = url
            .strip_prefix("https://webcast.tiktok.com/webcast/room/chat/?")
            .unwrap();
        let q: HashMap<&str, &str> = u.split('&').filter_map(|kv| kv.split_once('=')).collect();
        assert_eq!(q["room_id"], "7704");
        assert_eq!(q["content"], "hi%20%26%20%C3%A7%C3%A3o");
        assert_eq!(q["referer"], "https%3A%2F%2Fwww.tiktok.com%2F%40some.one%2Flive");
        assert_eq!(q["focus_state"], "true");
        assert_eq!(q["is_page_visible"], "true");
        assert_eq!(q["browser_version"], "5.0%20%28X11%3B%20Linux%20x86_64%29");
        assert_eq!(q["client_start_timestamp_millisecond"], "1700000000123");
        assert!(!q.contains_key("channel_id") && !q.contains_key("X-Bogus"));
        let b: Value = serde_json::from_str(&body).unwrap();
        assert_eq!(
            b,
            json!({"room_id": "7704", "content": "hi & ção", "emotes_with_index": "",
            "input_type": 0, "client_start_timestamp_millisecond": 1_700_000_000_123u64})
        );
    }

    #[test]
    fn answers() {
        assert!(answer(200, &json!({"status_code": 0, "data": {"content": "hi"}})).is_ok());
        assert_eq!(
            answer(
                200,
                &json!({"status_code": 4003110, "data": {"prompts": "You're commenting too fast"}})
            ),
            Err("TikTok: You're commenting too fast".into())
        );
        assert_eq!(answer(200, &json!({"status_code": 7})), Err("TikTok: error 7".into()));
        assert_eq!(answer(403, &Value::Null), Err("TikTok answered HTTP 403".into()));
    }

    #[test]
    fn only_the_extension_and_its_token() {
        assert!(from_extension(Some(
            "chrome-extension://lhijoeeghjcohapolkfkacipklhbnecc"
        )));
        assert!(from_extension(Some(
            "chrome-extension://storeextensionid1234567890abcdef"
        )));
        assert!(from_extension(Some(
            "moz-extension://a3b2c1d0-1234-5678-9abc-def012345678"
        )));
        assert!(!from_extension(Some("https://www.tiktok.com")));
        assert!(!from_extension(Some("http://localhost:8080")));
        assert!(!from_extension(None));
        assert!(same("abc", "abc") && !same("abc", "abd") && !same("abc", "ab"));
    }

    #[test]
    fn the_extension_id_is_its_keys() {
        use base64::Engine as _;
        use sha2::{Digest, Sha256};
        let m: Value = serde_json::from_slice(FILES[0].1).unwrap();
        let der = base64::engine::general_purpose::STANDARD
            .decode(m["key"].as_str().unwrap())
            .unwrap();
        let id: String = Sha256::digest(&der)[..16]
            .iter()
            .flat_map(|b| [b >> 4, b & 15])
            .map(|n| (b'a' + n) as char)
            .collect();
        assert_eq!(id, EXTENSION_ID);
    }

    #[test]
    fn server_url_is_local() {
        let mut c = crate::Config::default();
        c.websockets.address = "0.0.0.0".into();
        c.websockets.port = 8090;
        assert_eq!(server_url(&c), "ws://127.0.0.1:8090/streamerz/tiktok");
    }

    #[test]
    fn setup_page_escapes_paths_and_includes_both_copy_targets() {
        let folder = std::path::Path::new(r"/tmp/a&b<c>\tiktok-extension");
        let html = setup_html(
            "Chromium",
            "chrome://extensions",
            folder,
            &Setup::Chromium {
                store: CHROME_STORE_URL,
            },
        );
        assert!(html.contains("chrome://extensions"));
        assert!(html.contains("/tmp/a&amp;b&lt;c&gt;\\tiktok-extension"));
        assert!(!html.contains("/tmp/a&b<c>"));
        assert!(html.contains(r#"bind("copy-2", "/tmp/a\u0026b\u003cc\u003e\\tiktok-extension");"#));
        assert!(html.contains(CHROME_STORE_URL));
        // Everything on it is the user's language (it once had English of its own).
        assert!(!html.contains("Option 1"));
        let firefox = setup_html(
            "Zen Browser",
            "about:addons",
            folder,
            &Setup::Firefox {
                addon: std::path::Path::new("/tmp/streamerz-tiktok-1.4.0.xpi"),
            },
        );
        assert!(firefox.contains("file:///tmp/streamerz-tiktok-1.4.0.xpi"));
        assert!(!firefox.contains("chrome://"));
        let temporary = setup_html("Mozilla Firefox", "about:addons", folder, &Setup::FirefoxTemporary);
        assert!(temporary.contains(FIREFOX_DEBUGGING));
    }

    #[test]
    fn the_firefox_addon_must_be_signed_and_this_version() {
        let xpi = |version: &str, signed: bool| {
            let mut files = vec![(
                "manifest.json".to_owned(),
                json!({"version": version, "browser_specific_settings": {"gecko": {"id": FIREFOX_GECKO_ID}}})
                    .to_string()
                    .into_bytes(),
            )];
            if signed {
                files.push(("META-INF/mozilla.rsa".to_owned(), b"signature".to_vec()));
            }
            crate::zip::write(&files).unwrap()
        };
        assert!(firefox_xpi_ok(&xpi(EXTENSION_VERSION, true)));
        assert!(!firefox_xpi_ok(&xpi(EXTENSION_VERSION, false)));
        assert!(!firefox_xpi_ok(&xpi("0.0.1", true)));
        assert!(!firefox_xpi_ok(b"<html>Not Found</html>"));
        assert!(firefox_xpi_url().ends_with(&format!(
            "/releases/download/tiktok-extension-v{EXTENSION_VERSION}/streamerz-tiktok-{EXTENSION_VERSION}.xpi"
        )));
    }

    #[test]
    fn extensions_outside_the_protocol_range_are_told_which_side_to_update() {
        let hello = |protocol: u64| json!({"op": "hello", "protocol": protocol, "version": "9.9.9"});
        assert!(refusal(&hello(PROTOCOL)).is_none());
        assert!(refusal(&hello(MIN_PROTOCOL)).is_none());
        let newer = refusal(&hello(PROTOCOL + 1)).unwrap();
        let older_one = refusal(&hello(MIN_PROTOCOL - 1)).unwrap();
        assert_ne!(newer, older_one);
        assert!(newer.contains("9.9.9"));
        assert!(older(EXTENSION_VERSION, "99.0.0") && !older("2.0.0", "1.10.0") && older("1.9.0", "1.10.0"));
    }

    #[test]
    fn tiktok_stream_title_and_pin_metadata() {
        // Verify trigger
        assert_eq!(crate::types::name(crate::types::TRIGGERS, 41013), Some("TikTokRoomPin"));

        // Verify subactions
        assert_eq!(
            crate::types::name(crate::types::SUB_ACTIONS, 43010),
            Some("TikTokSetStreamTitle")
        );
        assert_eq!(
            crate::types::name(crate::types::SUB_ACTIONS, 43011),
            Some("TikTokPinComment")
        );
        assert_eq!(
            crate::types::name(crate::types::SUB_ACTIONS, 43012),
            Some("TikTokUnpinComment")
        );

        let title_of = |code| {
            crate::types::SUB_ACTION_INFO
                .binary_search_by_key(&code, |x| x.0)
                .ok()
                .map(|i| crate::types::SUB_ACTION_INFO[i].1)
        };
        assert_eq!(title_of(43010), Some("Set Stream Title"));
        assert_eq!(title_of(43011), Some("Pin Comment"));
        assert_eq!(title_of(43012), Some("Unpin Comment"));

        // Verify update_room payload
        let title_body = json!({"room_id": "123456789", "title": "New Stream Title"});
        assert_eq!(title_body["title"], "New Stream Title");

        // Verify pin payload (action_type 1) and unpin payload (action_type 2)
        let pin_body = json!({"room_id": "123456789", "comment_id": "c1", "action_type": 1});
        assert_eq!(pin_body["action_type"], 1);
        let unpin_body = json!({"room_id": "123456789", "action_type": 2});
        assert_eq!(unpin_body["action_type"], 2);
    }
}
