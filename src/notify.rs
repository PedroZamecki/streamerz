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
    if crate::ui::relogin(platform, bot) {
        let key = if bot { "relogin.ask_bot" } else { "relogin.ask" };
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
    if !enabled(Kind::Automation) {
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

/// Shows it through the Windows Runtime, in-process (so its Activated event reaches us), as from
/// PowerShell's app id like the other notifications; waits for a click.
#[cfg(windows)]
async fn show_toast(t: &Toast) -> Result<bool, String> {
    use windows::core::HSTRING;
    use windows::Data::Xml::Dom::XmlDocument;
    use windows::Foundation::TypedEventHandler;
    use windows::UI::Notifications::{ToastNotification, ToastNotificationManager};
    fn esc(s: &str) -> String {
        s.replace('&', "&amp;")
            .replace('<', "&lt;")
            .replace('>', "&gt;")
            .replace('"', "&quot;")
    }
    let image = if std::path::Path::new(&t.icon_path).is_file() {
        format!(r#"<image placement="appLogoOverride" src="{}"/>"#, esc(&t.icon_path))
    } else {
        String::new()
    };
    let xml = format!(
        r#"<toast><visual><binding template="ToastGeneric"><text>{}</text><text>{}</text><text placement="attribution">{}</text>{image}</binding></visual></toast>"#,
        esc(&t.title),
        esc(&t.text),
        esc(&t.attribution)
    );
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
        ToastNotificationManager::CreateToastNotifierWithId(&HSTRING::from(
            r"{1AC14E77-02E7-4E5D-B744-2EB1AE5198B7}\WindowsPowerShell\v1.0\powershell.exe",
        ))?
        .Show(&toast)?;
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

/// A toast through the system's own Windows Runtime, by PowerShell; the text travels in the environment,
/// never in the script. Shown as from PowerShell's app id, which every Windows has registered.
#[cfg(windows)]
async fn show(title: &str, body: &str) -> Result<(), String> {
    const SCRIPT: &str = r"
[void][Windows.UI.Notifications.ToastNotificationManager, Windows.UI.Notifications, ContentType = WindowsRuntime]
$x = [Windows.UI.Notifications.ToastNotificationManager]::GetTemplateContent([Windows.UI.Notifications.ToastTemplateType]::ToastText02)
$t = $x.GetElementsByTagName('text')
[void]$t.Item(0).AppendChild($x.CreateTextNode($env:STREAMERZ_TITLE))
[void]$t.Item(1).AppendChild($x.CreateTextNode($env:STREAMERZ_BODY))
[Windows.UI.Notifications.ToastNotificationManager]::CreateToastNotifier('{1AC14E77-02E7-4E5D-B744-2EB1AE5198B7}\WindowsPowerShell\v1.0\powershell.exe').Show([Windows.UI.Notifications.ToastNotification]::new($x))";
    tokio::process::Command::new("powershell")
        .args(["-NoProfile", "-NonInteractive", "-Command", SCRIPT])
        .env("STREAMERZ_TITLE", title)
        .env("STREAMERZ_BODY", body)
        .creation_flags(0x0800_0000) // no console window
        .status()
        .await
        .map(|_| ())
        .map_err(|e| e.to_string())
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
