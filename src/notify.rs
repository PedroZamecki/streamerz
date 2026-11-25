//! Desktop notifications, native on each system: on Linux the freedesktop Notifications service over
//! D-Bus (every desktop environment has one; zbus is already in for the tray), on Windows a toast.
//! A failure only goes to the log: a missing notification daemon is no reason to stop anything.

/// Notification choices are shared with the engine without a background task.
#[derive(Clone, Copy, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct Settings {
    pub login: bool,
    pub updates: bool,
    pub streams: bool,
    pub automation: bool,
}
impl Default for Settings {
    fn default() -> Self {
        Self {
            login: true,
            updates: true,
            streams: true,
            automation: true,
        }
    }
}
#[derive(Clone, Copy)]
pub enum Kind {
    Login,
    Updates,
    Streams,
    Automation,
}
static ENABLED: std::sync::atomic::AtomicU8 = std::sync::atomic::AtomicU8::new(15);
static SILENCED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
/// Suppresses OS notifications while running headless scenarios.
pub fn silence(on: bool) {
    SILENCED.store(on, std::sync::atomic::Ordering::Relaxed);
}
pub fn configure(s: Settings) {
    let bits =
        u8::from(s.login) | (u8::from(s.updates) << 1) | (u8::from(s.streams) << 2) | (u8::from(s.automation) << 3);
    ENABLED.store(bits, std::sync::atomic::Ordering::Relaxed);
}
fn enabled(kind: Kind) -> bool {
    ENABLED.load(std::sync::atomic::Ordering::Relaxed) & (1 << kind as u8) != 0
}

/// Shows a notification, without waiting for it.
pub fn send(kind: Kind, title: &str, body: &str) {
    if !enabled(kind) {
        return;
    }
    critical(title, body);
}

/// Shows a critical notification regardless of category toggles (e.g., UI crash recovery).
pub fn critical(title: &str, body: &str) {
    if SILENCED.load(std::sync::atomic::Ordering::Relaxed) {
        return;
    }
    let (title, body) = (title.to_string(), body.to_string());
    tokio::task::spawn_local(async move {
        if let Err(e) = show(&title, &body).await {
            crate::warn(format!("Notification ({title}): {e}"));
        }
    });
}

/// A login ran out: a popup asks to log in again (even with the window hidden), and a notification says
/// so too. Asked once per login until it's done again, so a retry loop doesn't repeat it.
pub fn login_needed(platform: &'static str, bot: bool) {
    if crate::host::relogin(platform, bot) {
        let key = if bot { "relogin.ask_bot" } else { "relogin.ask" };
        send(Kind::Login, platform, &crate::i18n::trf(key, &[("platform", platform)]));
    }
}

/// A login lacks permissions Streamerz now asks for: asked like one that ran out, said as what it is.
pub fn permissions_needed(platform: &'static str, bot: bool) {
    if crate::host::relogin(platform, bot) {
        let key = if bot {
            "relogin.ask_permissions_bot"
        } else {
            "relogin.ask_permissions"
        };
        send(Kind::Login, platform, &crate::i18n::trf(key, &[("platform", platform)]));
    }
}

/// A toast from the Show Toast sub-action; clicking it fires Toast Activation.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Toast {
    pub id: String,
    pub title: String,
    pub text: String,
    pub attribution: String,
    pub icon_path: String,
}

impl Toast {
    /// Toast Activation's arguments, as Streamer.bot names them.
    pub fn args(&self) -> crate::model::Obj {
        let mut a = crate::model::Obj::new();
        let local = if std::path::Path::new(&self.icon_path).is_file() {
            self.icon_path.as_str()
        } else {
            ""
        };
        for (k, v) in [
            ("toast.id", self.id.as_str()),
            ("toast.title", &self.title),
            ("toast.message", &self.text),
            ("toast.attribution", &self.attribution),
            ("toast.iconPath", &self.icon_path),
            ("toast.localIconPath", local),
        ] {
            a.insert(k.into(), v.into());
        }
        a
    }
}

/// Shows `t`, without waiting; a click on it calls `clicked` (on this thread).
pub fn toast(t: Toast, clicked: impl FnOnce(&Toast) + 'static) {
    if SILENCED.load(std::sync::atomic::Ordering::Relaxed) || !enabled(Kind::Automation) {
        return;
    }
    tokio::task::spawn_local(async move {
        match show_toast(&t).await {
            Ok(true) => clicked(&t),
            Ok(false) => {}
            Err(e) => crate::warn(format!("Toast ({}): {e}", t.title)),
        }
    });
}

/// How long a toast is listened to for a click (it may wait in the notification list).
const TOAST_WAIT: std::time::Duration = std::time::Duration::from_secs(3600);

/// Shows it and waits: true when it was clicked, false when it closed or the hour passed.
#[cfg(target_os = "linux")]
async fn show_toast(t: &Toast) -> Result<bool, String> {
    use futures_util::StreamExt;
    use std::collections::HashMap;
    use zbus::zvariant::Value;
    const IFACE: &str = "org.freedesktop.Notifications";
    let c = zbus::Connection::session().await.map_err(|e| e.to_string())?;
    // Listen first, so a click right away isn't missed.
    let rule = zbus::MatchRule::builder()
        .msg_type(zbus::message::Type::Signal)
        .interface(IFACE)
        .map_err(|e| e.to_string())?
        .build();
    let mut signals = zbus::MessageStream::for_match_rule(rule, &c, None)
        .await
        .map_err(|e| e.to_string())?;
    let hints: HashMap<&str, Value> = HashMap::new();
    let icon = if t.icon_path.is_empty() {
        "streamerz"
    } else {
        t.icon_path.as_str()
    };
    let body = if t.attribution.is_empty() {
        t.text.clone()
    } else {
        format!("{}\n{}", t.text, t.attribution)
    };
    let reply = c
        .call_method(
            Some(IFACE),
            "/org/freedesktop/Notifications",
            Some(IFACE),
            "Notify",
            // The "default" action is a click on the notification itself.
            &(
                "Streamerz",
                0u32,
                icon,
                t.title.as_str(),
                body.as_str(),
                vec!["default", "Open"],
                hints,
                -1i32,
            ),
        )
        .await
        .map_err(|e| e.to_string())?;
    let id: u32 = reply.body().deserialize().map_err(|e| e.to_string())?;
    let wait = async {
        while let Some(Ok(m)) = signals.next().await {
            let member = m.header().member().map(|x| x.to_string()).unwrap_or_default();
            match member.as_str() {
                "ActionInvoked" if m.body().deserialize::<(u32, String)>().is_ok_and(|(n, _)| n == id) => {
                    return true;
                }
                "NotificationClosed" if m.body().deserialize::<(u32, u32)>().is_ok_and(|(n, _)| n == id) => {
                    return false;
                }
                _ => {}
            }
        }
        false
    };
    Ok(tokio::time::timeout(TOAST_WAIT, wait).await.unwrap_or(false))
}

/// Streamerz's own app id: its toasts say Streamerz, with its icon (they once came as from PowerShell).
#[cfg(windows)]
const APP_ID: &str = "PedroZamecki.Streamerz";

/// Registers [`APP_ID`] for this user, once a run: its name and icon under
/// HKCU\Software\Classes\AppUserModelId. An app outside the Store needs nothing more to send toasts
/// as itself: no Start menu shortcut, no helper program.
#[cfg(windows)]
fn app_id() -> windows::core::HSTRING {
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| {
        const PNG: &[u8] = include_bytes!("../assets/brand/streamerz.png");
        let icon = crate::default_config_path().with_file_name("streamerz.png");
        if std::fs::read(&icon).map_or(true, |old| old != PNG) {
            let _ = std::fs::write(&icon, PNG);
        }
        let key = format!(r"Software\Classes\AppUserModelId\{APP_ID}");
        let icon = icon.display().to_string();
        let icon = icon.strip_prefix(r"\\?\").unwrap_or(&icon);
        for (name, value) in [("DisplayName", "Streamerz"), ("IconUri", icon)] {
            if let Err(e) = set_user_value(&key, name, value) {
                crate::warn(format!("Notifications: can't register Streamerz ({e})"));
            }
        }
    });
    windows::core::HSTRING::from(APP_ID)
}

/// Sets the text value `name` of the user's key `key`, creating the key.
#[cfg(windows)]
fn set_user_value(key: &str, name: &str, value: &str) -> std::io::Result<()> {
    use windows_sys::Win32::System::Registry::{
        RegCloseKey, RegCreateKeyExW, RegSetValueExW, HKEY, HKEY_CURRENT_USER, KEY_SET_VALUE, REG_OPTION_NON_VOLATILE,
        REG_SZ,
    };
    let w = |s: &str| s.encode_utf16().chain([0]).collect::<Vec<u16>>();
    let (key, name, value) = (w(key), w(name), w(value));
    let mut handle: HKEY = std::ptr::null_mut();
    // SAFETY: NUL-terminated UTF-16 strings that outlive the calls; the size is in bytes; the key opened
    // here is closed here.
    let err = unsafe {
        let err = RegCreateKeyExW(
            HKEY_CURRENT_USER,
            key.as_ptr(),
            0,
            std::ptr::null(),
            REG_OPTION_NON_VOLATILE,
            KEY_SET_VALUE,
            std::ptr::null(),
            &mut handle,
            std::ptr::null_mut(),
        );
        if err != 0 {
            err
        } else {
            let err = RegSetValueExW(
                handle,
                name.as_ptr(),
                0,
                REG_SZ,
                value.as_ptr().cast(),
                (value.len() * 2) as u32,
            );
            RegCloseKey(handle);
            err
        }
    };
    match err {
        0 => Ok(()),
        e => Err(std::io::Error::from_raw_os_error(e as i32)),
    }
}

/// A toast's XML: its title, text and, when given, a quieter line and a picture.
#[cfg(any(windows, test))]
fn toast_xml(title: &str, text: &str, attribution: &str, icon: &str) -> String {
    fn esc(s: &str) -> String {
        s.replace('&', "&amp;")
            .replace('<', "&lt;")
            .replace('>', "&gt;")
            .replace('"', "&quot;")
    }
    let attribution = if attribution.is_empty() {
        String::new()
    } else {
        format!(r#"<text placement="attribution">{}</text>"#, esc(attribution))
    };
    let image = if !icon.is_empty() && std::path::Path::new(icon).is_file() {
        format!(r#"<image placement="appLogoOverride" src="{}"/>"#, esc(icon))
    } else {
        String::new()
    };
    format!(
        r#"<toast><visual><binding template="ToastGeneric"><text>{}</text><text>{}</text>{attribution}{image}</binding></visual></toast>"#,
        esc(title),
        esc(text)
    )
}

/// Shows it through the Windows Runtime, in-process (so its Activated event reaches us), as
/// Streamerz; waits for a click.
#[cfg(windows)]
async fn show_toast(t: &Toast) -> Result<bool, String> {
    use windows::core::HSTRING;
    use windows::Data::Xml::Dom::XmlDocument;
    use windows::Foundation::TypedEventHandler;
    use windows::UI::Notifications::{ToastNotification, ToastNotificationManager};
    let xml = toast_xml(&t.title, &t.text, &t.attribution, &t.icon_path);
    let (tx, rx) = tokio::sync::oneshot::channel::<()>();
    let tx = std::sync::Mutex::new(Some(tx));
    let toast = (|| -> windows::core::Result<ToastNotification> {
        let doc = XmlDocument::new()?;
        doc.LoadXml(&HSTRING::from(xml))?;
        let toast = ToastNotification::CreateToastNotification(&doc)?;
        toast.Activated(&TypedEventHandler::new(move |_, _| {
            if let Some(tx) = tx.lock().ok().and_then(|mut t| t.take()) {
                let _ = tx.send(());
            }
            Ok(())
        }))?;
        ToastNotificationManager::CreateToastNotifierWithId(&app_id())?.Show(&toast)?;
        Ok(toast)
    })()
    .map_err(|e| e.to_string())?;
    let clicked = tokio::time::timeout(TOAST_WAIT, rx).await.is_ok_and(|r| r.is_ok());
    drop(toast);
    Ok(clicked)
}

#[cfg(not(any(target_os = "linux", windows)))]
async fn show_toast(_: &Toast) -> Result<bool, String> {
    Err("toasts are not supported here".into())
}

#[cfg(target_os = "linux")]
async fn show(title: &str, body: &str) -> Result<(), String> {
    use std::collections::HashMap;
    use zbus::zvariant::Value;
    let c = zbus::Connection::session().await.map_err(|e| e.to_string())?;
    let hints: HashMap<&str, Value> = HashMap::new();
    c.call_method(
        Some("org.freedesktop.Notifications"),
        "/org/freedesktop/Notifications",
        Some("org.freedesktop.Notifications"),
        "Notify",
        // App, replaces nothing, icon (by name, from the icon theme), summary, body, no actions, hints, ms.
        &(
            "Streamerz",
            0u32,
            "streamerz",
            title,
            body,
            Vec::<&str>::new(),
            hints,
            8000i32,
        ),
    )
    .await
    .map(|_| ())
    .map_err(|e| e.to_string())
}

/// A toast through the Windows Runtime, as Streamerz (no helper program, nothing to wait for).
#[cfg(windows)]
async fn show(title: &str, body: &str) -> Result<(), String> {
    use windows::core::HSTRING;
    use windows::Data::Xml::Dom::XmlDocument;
    use windows::UI::Notifications::{ToastNotification, ToastNotificationManager};
    (|| -> windows::core::Result<()> {
        let doc = XmlDocument::new()?;
        doc.LoadXml(&HSTRING::from(toast_xml(title, body, "", "")))?;
        let toast = ToastNotification::CreateToastNotification(&doc)?;
        ToastNotificationManager::CreateToastNotifierWithId(&app_id())?.Show(&toast)
    })()
    .map_err(|e| e.to_string())
}

#[cfg(test)]
mod xml_tests {
    #[test]
    fn toast_text_is_escaped_and_empty_parts_left_out() {
        let xml = super::toast_xml("A & B", "<b>hi</b>", "", "");
        assert!(xml.contains("<text>A &amp; B</text><text>&lt;b&gt;hi&lt;/b&gt;</text>"));
        assert!(!xml.contains("attribution") && !xml.contains("<image"));
        assert!(
            super::toast_xml("t", "x", "by \"me\"", "").contains(r#"placement="attribution">by &quot;me&quot;</text>"#)
        );
    }
}

#[cfg(all(test, target_os = "linux"))]
mod tests {
    #[test]
    fn notification_preferences_default_on_and_preserve_disabled_values() {
        let defaults: super::Settings = serde_json::from_str("{}").unwrap();
        assert!(defaults.login && defaults.updates && defaults.streams && defaults.automation);
        let saved: super::Settings = serde_json::from_str(r#"{"updates":false}"#).unwrap();
        assert!(saved.login && !saved.updates && saved.streams && saved.automation);
        let restored: super::Settings = serde_json::from_str(&serde_json::to_string(&saved).unwrap()).unwrap();
        assert!(restored.login && !restored.updates && restored.streams && restored.automation);
    }

    /// A fake notification service on a private bus clicks every toast: the click comes back. Needs its
    /// own bus: `dbus-run-session -- cargo test -- --ignored clicked_toast`.
    #[tokio::test(flavor = "current_thread")]
    #[ignore]
    async fn a_clicked_toast_is_reported() {
        use futures_util::StreamExt;
        const IFACE: &str = "org.freedesktop.Notifications";
        let server = zbus::Connection::session().await.unwrap();
        server.request_name(IFACE).await.unwrap();
        let rule = zbus::MatchRule::builder()
            .msg_type(zbus::message::Type::MethodCall)
            .interface(IFACE)
            .unwrap()
            .build();
        let mut calls = zbus::MessageStream::for_match_rule(rule, &server, None).await.unwrap();
        let s2 = server.clone();
        tokio::spawn(async move {
            while let Some(Ok(m)) = calls.next().await {
                if m.header().member().is_some_and(|x| x.as_str() == "Notify") {
                    s2.reply(&m.header(), &(7u32,)).await.unwrap();
                    let path = "/org/freedesktop/Notifications";
                    s2.emit_signal(None::<&str>, path, IFACE, "ActionInvoked", &(7u32, "default"))
                        .await
                        .unwrap();
                }
            }
        });
        let t = super::Toast {
            title: "Title".into(),
            text: "Text".into(),
            ..Default::default()
        };
        let clicked = tokio::time::timeout(std::time::Duration::from_secs(5), super::show_toast(&t)).await;
        assert_eq!(clicked, Ok(Ok(true)));
    }

    /// Needs a notification service on the session bus and shows one: `cargo test -- --ignored notify`.
    #[tokio::test(flavor = "current_thread")]
    #[ignore]
    async fn a_notification_is_accepted() {
        super::show("streamerz test", "This is the notification test.")
            .await
            .unwrap();
    }
}
