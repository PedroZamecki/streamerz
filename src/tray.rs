//! Tray icon: click (or "Show/Hide") toggles the window, "Quit" exits. Without a tray (e.g. no
//! StatusNotifier host on Linux) the window stays up instead. The icon follows the desktop's colour
//! scheme (not the app's theme): the light-panel logo or the dark-panel one, changing as the desktop does.

use crate::brand::{tray as icon, SIZE};
use crate::i18n::tr;
use crate::ui::Msg;
use crate::version;
use fltk::app::Sender;

/// The tray: the window's sender, and whether the desktop is dark (the icon follows it).
#[cfg(target_os = "linux")]
struct Tray(Sender<Msg>, bool);

#[cfg(target_os = "linux")]
impl ksni::Tray for Tray {
    fn id(&self) -> String {
        "streamerz".into()
    }
    fn title(&self) -> String {
        "Streamerz".into()
    }
    fn icon_pixmap(&self) -> Vec<ksni::Icon> {
        // RGBA to ARGB, as StatusNotifierItem wants.
        let data = icon(self.1)
            .as_chunks::<4>()
            .0
            .iter()
            .flat_map(|&[r, g, b, a]| [a, r, g, b])
            .collect();
        vec![ksni::Icon {
            width: SIZE,
            height: SIZE,
            data,
        }]
    }
    fn tool_tip(&self) -> ksni::ToolTip {
        ksni::ToolTip {
            title: version(),
            ..Default::default()
        }
    }
    fn activate(&mut self, _x: i32, _y: i32) {
        self.0.send(Msg::Toggle);
    }
    fn menu(&self) -> Vec<ksni::MenuItem<Self>> {
        use ksni::menu::StandardItem;
        vec![
            StandardItem {
                label: tr("tray.show_hide").into(),
                activate: Box::new(|t: &mut Self| t.0.send(Msg::Toggle)),
                ..Default::default()
            }
            .into(),
            StandardItem {
                label: tr("tray.quit").into(),
                activate: Box::new(|_| std::process::exit(0)),
                ..Default::default()
            }
            .into(),
        ]
    }
}

/// Whether the desktop is dark, from the settings portal's `color-scheme` (1 dark, 2 light, 0 no
/// preference: dark, as most panels are). `None` when the portal doesn't answer.
#[cfg(target_os = "linux")]
async fn desktop_dark(settings: &zbus::Proxy<'_>) -> Option<bool> {
    use zbus::zvariant::{OwnedValue, Value};
    // The variant, or (the older `Read`) a variant in a variant.
    fn scheme(v: &Value) -> Option<u32> {
        match v {
            Value::U32(n) => Some(*n),
            Value::Value(inner) => scheme(inner),
            _ => None,
        }
    }
    let args = ("org.freedesktop.appearance", "color-scheme");
    let m = match settings.call_method("ReadOne", &args).await {
        Ok(m) => m,
        Err(_) => settings.call_method("Read", &args).await.ok()?,
    };
    let v: OwnedValue = m.body().deserialize().ok()?;
    scheme(&v).map(|n| n != 2)
}

#[cfg(target_os = "linux")]
pub async fn run(ui: Sender<Msg>) {
    use futures_util::StreamExt;
    use ksni::TrayMethods;
    use zbus::{Connection, Proxy};

    let connection = Connection::session().await.ok();
    let settings = match &connection {
        Some(c) => Proxy::new(
            c,
            "org.freedesktop.portal.Desktop",
            "/org/freedesktop/portal/desktop",
            "org.freedesktop.portal.Settings",
        )
        .await
        .ok(),
        None => None,
    };
    let mut dark = match &settings {
        Some(s) => desktop_dark(s).await.unwrap_or(true),
        None => true,
    };
    match (Tray(ui, dark)).spawn().await {
        // The tray lives as long as its handle; its icon follows the desktop's colours.
        Ok(handle) => {
            if let Some(s) = &settings {
                if let Ok(mut changes) = s.receive_signal("SettingChanged").await {
                    while let Some(m) = changes.next().await {
                        let Ok((space, key, _)) =
                            m.body().deserialize::<(String, String, zbus::zvariant::OwnedValue)>()
                        else {
                            continue;
                        };
                        if space != "org.freedesktop.appearance" || key != "color-scheme" {
                            continue;
                        }
                        if let Some(now) = desktop_dark(s).await.filter(|&n| n != dark) {
                            dark = now;
                            handle.update(|t| t.1 = now).await;
                        }
                    }
                }
            }
            std::future::pending().await
        }
        Err(e) => {
            crate::say(format!("no tray icon ({e})"));
            ui.send(Msg::NoTray);
        }
    }
}

/// Whether Windows' taskbar (where the tray lives) is dark: the user's `SystemUsesLightTheme`, 0 when
/// dark. Unset (Windows before 10 1903) means dark. Read at start: a change shows on the next one.
#[cfg(windows)]
fn taskbar_dark() -> bool {
    use windows_sys::Win32::System::Registry::{RegGetValueW, HKEY_CURRENT_USER, RRF_RT_REG_DWORD};
    let w = |s: &str| s.encode_utf16().chain([0]).collect::<Vec<u16>>();
    let (key, name) = (
        w(r"Software\Microsoft\Windows\CurrentVersion\Themes\Personalize"),
        w("SystemUsesLightTheme"),
    );
    let (mut light, mut size) = (1u32, 4u32);
    // SAFETY: NUL-terminated UTF-16 strings that outlive the call; `light` has room for the `size` (4)
    // bytes of a DWORD.
    let err = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            key.as_ptr(),
            name.as_ptr(),
            RRF_RT_REG_DWORD,
            std::ptr::null_mut(),
            std::ptr::addr_of_mut!(light).cast(),
            &mut size,
        )
    };
    err != 0 || light == 0
}

#[cfg(windows)]
pub async fn run(ui: Sender<Msg>) {
    use tray_icon::menu::{Menu, MenuEvent, MenuItem};
    use tray_icon::{Icon, MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
    use windows_sys::Win32::UI::WindowsAndMessaging::{DispatchMessageW, GetMessageW, TranslateMessage, MSG};

    // Win32 delivers tray clicks to the thread that created the icon, so it gets its own
    // thread with a message loop.
    std::thread::spawn(move || {
        let open = MenuItem::new(tr("tray.show_hide"), true, None);
        let close = MenuItem::new(tr("tray.quit"), true, None);
        let menu = Menu::new();
        let fail = |e: String| {
            crate::say(format!("no tray icon ({e})"));
            ui.send(Msg::NoTray);
        };
        if let Err(e) = menu.append_items(&[&open, &close]) {
            return fail(e.to_string());
        }
        let (open_id, close_id) = (open.id().clone(), close.id().clone());
        let menu_ui = ui;
        MenuEvent::set_event_handler(Some(move |e: MenuEvent| {
            if e.id == open_id {
                menu_ui.send(Msg::Toggle);
            } else if e.id == close_id {
                std::process::exit(0);
            }
        }));
        // Left click toggles the window; right click opens the menu.
        let click_ui = ui;
        TrayIconEvent::set_event_handler(Some(move |e| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = e
            {
                click_ui.send(Msg::Toggle);
            }
        }));

        let tray = Icon::from_rgba(icon(taskbar_dark()).to_vec(), SIZE as u32, SIZE as u32)
            .map_err(|e| e.to_string())
            .and_then(|icon| {
                TrayIconBuilder::new()
                    .with_icon(icon)
                    .with_tooltip(version())
                    .with_menu(Box::new(menu))
                    .with_menu_on_left_click(false)
                    .build()
                    .map_err(|e| e.to_string())
            });
        // Kept alive by the message loop below.
        let _tray = match tray {
            Ok(t) => t,
            Err(e) => return fail(e),
        };

        // SAFETY: standard Win32 message loop; MSG is plain data, zeroed is a valid initial value.
        unsafe {
            let mut msg: MSG = std::mem::zeroed();
            while GetMessageW(&mut msg, std::ptr::null_mut(), 0, 0) > 0 {
                TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
        }
    });
}

#[cfg(not(any(target_os = "linux", windows)))]
pub async fn run(ui: Sender<Msg>) {
    ui.send(Msg::NoTray);
}
