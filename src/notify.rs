//! Desktop notifications, native on each system: on Linux the freedesktop Notifications service over
//! D-Bus (every desktop environment has one; zbus is already in for the tray), on Windows a toast.
//! A failure only goes to the log: a missing notification daemon is no reason to stop anything.

/// Shows a notification, without waiting for it.
pub fn send(title: &str, body: &str) {
    let (title, body) = (title.to_string(), body.to_string());
    tokio::task::spawn_local(async move {
        if let Err(e) = show(&title, &body).await {
            crate::warn(format!("Notification ({title}): {e}"));
        }
    });
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
    /// Needs a notification service on the session bus and shows one: `cargo test -- --ignored notify`.
    #[tokio::test(flavor = "current_thread")]
    #[ignore]
    async fn a_notification_is_accepted() {
        super::show("streamerz test", "This is the notification test.")
            .await
            .unwrap();
    }
}
