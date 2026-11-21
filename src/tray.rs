//! Tray icon: click (or "Show/Hide") toggles the window, "Software rendering" switches how it is drawn
//! (reachable when the window itself shows nothing), "Quit" exits. Without a tray (e.g. no
//! StatusNotifier host on Linux) the window stays up instead. The icon follows the desktop's colour
//! scheme (not the app's theme): the light-panel logo or the dark-panel one, changing as the desktop does.

use crate::brand::{tray as icon, SIZE};
use crate::host::{Message as Msg, Sender};
use crate::i18n::tr;
use crate::version;
use std::sync::atomic::{AtomicBool, Ordering};

/// The Software rendering setting, as the tray's check shows it.
static SOFTWARE: AtomicBool = AtomicBool::new(false);

/// Wakes the tray to show a new check (Linux: the tray task; Windows: the tray thread's message loop).
#[cfg(target_os = "linux")]
static REFRESH: std::sync::OnceLock<tokio::sync::Notify> = std::sync::OnceLock::new();
#[cfg(windows)]
static TRAY_THREAD: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
#[cfg(windows)]
const WM_SOFTWARE: u32 = windows_sys::Win32::UI::WindowsAndMessaging::WM_APP + 1;

/// The Software rendering setting changed (or is read at start): the tray's check follows it.
pub fn show_software(on: bool) {
    if SOFTWARE.swap(on, Ordering::Relaxed) == on {
        return;
    }
    #[cfg(target_os = "linux")]
    REFRESH.get_or_init(Default::default).notify_one();
    #[cfg(windows)]
    {
        let thread = TRAY_THREAD.load(Ordering::Relaxed);
        if thread != 0 {
            // SAFETY: no pointers; a thread message to the tray's own loop.
            unsafe { windows_sys::Win32::UI::WindowsAndMessaging::PostThreadMessageW(thread, WM_SOFTWARE, 0, 0) };
        }
    }
}

/// The tray: the window's sender, and whether the desktop is dark (the icon follows it).
#[cfg(target_os = "linux")]
struct Tray(Sender, bool);

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
        use ksni::menu::{CheckmarkItem, StandardItem};
        vec![
            StandardItem {
                label: tr("tray.show_hide").into(),
                activate: Box::new(|t: &mut Self| t.0.send(Msg::Toggle)),
                ..Default::default()
            }
            .into(),
            CheckmarkItem {
                label: tr("settings.software_render").into(),
                checked: SOFTWARE.load(Ordering::Relaxed),
                activate: Box::new(|t: &mut Self| t.0.send(Msg::SoftwareRender)),
                ..Default::default()
            }
            .into(),
            ksni::MenuItem::Separator,
            StandardItem {
                label: tr("tray.quit").into(),
                activate: Box::new(|t: &mut Self| t.0.send(Msg::Quit)),
                ..Default::default()
            }
            .into(),
        ]
    }
}

/// The desktop's accent colour: the portal's own `accent-color` (three numbers from 0 to 1) or, from
/// GNOME and the desktops that copy it, the name of one of its accents.
#[cfg(target_os = "linux")]
async fn desktop_accent(settings: &zbus::Proxy<'_>) -> Option<(u8, u8, u8)> {
    use zbus::zvariant::{OwnedValue, Value};
    async fn read(s: &zbus::Proxy<'_>, ns: &str, key: &str) -> Option<OwnedValue> {
        let m = match s.call_method("ReadOne", &(ns, key)).await {
            Ok(m) => m,
            Err(_) => s.call_method("Read", &(ns, key)).await.ok()?,
        };
        m.body().deserialize().ok()
    }
    fn unwrap<'a>(v: &'a Value<'a>) -> &'a Value<'a> {
        match v {
            Value::Value(inner) => unwrap(inner),
            _ => v,
        }
    }
    if let Some(v) = read(settings, "org.freedesktop.appearance", "accent-color").await {
        if let Value::Structure(s) = unwrap(&v) {
            let c: Vec<f64> = s.fields().iter().filter_map(|f| f64::try_from(f).ok()).collect();
            if let [r, g, b] = c[..] {
                if (0.0..=1.0).contains(&r) && (0.0..=1.0).contains(&g) && (0.0..=1.0).contains(&b) {
                    return Some(((r * 255.0) as u8, (g * 255.0) as u8, (b * 255.0) as u8));
                }
            }
        }
    }
    let v = read(settings, "org.gnome.desktop.interface", "accent-color").await?;
    let Value::Str(name) = unwrap(&v) else { return None };
    gnome_accent(name)
}

/// GNOME's named accents.
#[cfg(target_os = "linux")]
fn gnome_accent(name: &str) -> Option<(u8, u8, u8)> {
    Some(match name {
        "blue" => (0x35, 0x84, 0xe4),
        "teal" => (0x21, 0x90, 0xa4),
        "green" => (0x3a, 0x94, 0x4a),
        "yellow" => (0xc8, 0x88, 0x00),
        "orange" => (0xed, 0x5b, 0x00),
        "red" => (0xe6, 0x2d, 0x42),
        "pink" => (0xd5, 0x61, 0x99),
        "purple" => (0x91, 0x41, 0xac),
        "slate" => (0x6f, 0x83, 0x96),
        _ => return None,
    })
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
pub async fn run(ui: Sender) {
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
    crate::host::system_dark(dark);
    if let Some(s) = &settings {
        crate::host::system_accent(desktop_accent(s).await);
    }
    // In Flatpak, the sandbox may not own a well-known bus name: the tray goes by its unique one.
    match (Tray(ui.clone(), dark))
        .disable_dbus_name(crate::sandbox::flatpak())
        .spawn()
        .await
    {
        // The tray lives as long as its handle; its icon follows the desktop's colours, and its check the
        // Software rendering setting.
        Ok(handle) => {
            let mut changes = match &settings {
                Some(s) => s.receive_signal("SettingChanged").await.ok(),
                None => None,
            };
            let refresh = REFRESH.get_or_init(Default::default);
            loop {
                let next = async {
                    match changes.as_mut() {
                        Some(changes) => changes.next().await,
                        None => std::future::pending().await,
                    }
                };
                let m = tokio::select! {
                    m = next => m,
                    () = refresh.notified() => {
                        handle.update(|_| {}).await;
                        continue;
                    }
                };
                let (Some(m), Some(s)) = (m, &settings) else {
                    changes = None;
                    continue;
                };
                let Ok((space, key, _)) = m.body().deserialize::<(String, String, zbus::zvariant::OwnedValue)>() else {
                    continue;
                };
                if key == "accent-color" {
                    crate::host::system_accent(desktop_accent(s).await);
                    continue;
                }
                if space != "org.freedesktop.appearance" || key != "color-scheme" {
                    continue;
                }
                if let Some(now) = desktop_dark(s).await.filter(|&n| n != dark) {
                    dark = now;
                    crate::host::system_dark(now);
                    handle.update(|t| t.1 = now).await;
                }
            }
        }
        Err(e) => {
            crate::warn(format!("no tray icon ({e})"));
            ui.send(Msg::NoTray);
        }
    }
}

/// Whether Windows' taskbar (where the tray lives) is dark: the user's `SystemUsesLightTheme`, 0 when
/// dark. Unset (Windows before 10 1903) means dark. Read at start: a change shows on the next one.
#[cfg(windows)]
fn taskbar_dark() -> bool {
    light_off("SystemUsesLightTheme")
}

/// Whether Windows' apps are dark (`AppsUseLightTheme`).
#[cfg(windows)]
pub fn apps_dark() -> bool {
    light_off("AppsUseLightTheme")
}

#[cfg(windows)]
fn light_off(value: &str) -> bool {
    reg_dword(r"Software\Microsoft\Windows\CurrentVersion\Themes\Personalize", value).is_none_or(|v| v == 0)
}

/// The user's accent colour on Windows (Settings > Personalization > Colors).
#[cfg(windows)]
pub fn accent() -> Option<(u8, u8, u8)> {
    // 0xAABBGGRR.
    let v = reg_dword(r"Software\Microsoft\Windows\DWM", "AccentColor")?;
    Some((v as u8, (v >> 8) as u8, (v >> 16) as u8))
}

/// A DWORD from HKEY_CURRENT_USER.
#[cfg(windows)]
fn reg_dword(key: &str, name: &str) -> Option<u32> {
    use windows_sys::Win32::System::Registry::{RegGetValueW, HKEY_CURRENT_USER, RRF_RT_REG_DWORD};
    let w = |s: &str| s.encode_utf16().chain([0]).collect::<Vec<u16>>();
    let (key, name) = (w(key), w(name));
    let (mut value, mut size) = (0u32, 4u32);
    // SAFETY: NUL-terminated UTF-16 strings that outlive the call; `value` has room for the `size` (4)
    // bytes of a DWORD.
    let err = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            key.as_ptr(),
            name.as_ptr(),
            RRF_RT_REG_DWORD,
            std::ptr::null_mut(),
            std::ptr::addr_of_mut!(value).cast(),
            &mut size,
        )
    };
    (err == 0).then_some(value)
}

#[cfg(windows)]
pub async fn run(ui: Sender) {
    use tray_icon::menu::{CheckMenuItem, Menu, MenuEvent, MenuItem, PredefinedMenuItem};
    use tray_icon::{Icon, MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
    use windows_sys::Win32::UI::WindowsAndMessaging::{DispatchMessageW, GetMessageW, TranslateMessage, MSG};

    // The system's colour scheme and accent, as Windows has them at start.
    crate::host::system_dark(apps_dark());
    crate::host::system_accent(accent());
    // Win32 delivers tray clicks to the thread that created the icon, so it gets its own
    // thread with a message loop.
    std::thread::spawn(move || {
        let open = MenuItem::new(tr("tray.show_hide"), true, None);
        let software = CheckMenuItem::new(
            tr("settings.software_render"),
            true,
            SOFTWARE.load(Ordering::Relaxed),
            None,
        );
        let close = MenuItem::new(tr("tray.quit"), true, None);
        let menu = Menu::new();
        let fail = |e: String| {
            crate::warn(format!("no tray icon ({e})"));
            ui.send(Msg::NoTray);
        };
        if let Err(e) = menu.append_items(&[&open, &software, &PredefinedMenuItem::separator(), &close]) {
            return fail(e.to_string());
        }
        let (open_id, software_id, close_id) = (open.id().clone(), software.id().clone(), close.id().clone());
        let menu_ui = ui.clone();
        MenuEvent::set_event_handler(Some(move |e: MenuEvent| {
            if e.id == open_id {
                menu_ui.send(Msg::Toggle);
            } else if e.id == software_id {
                menu_ui.send(Msg::SoftwareRender);
            } else if e.id == close_id {
                menu_ui.send(Msg::Quit);
            }
        }));
        // Left click toggles the window; right click opens the menu.
        let click_ui = ui.clone();
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
        watch_session_end();
        // Kept alive by the message loop below.
        let _tray = match tray {
            Ok(t) => t,
            Err(e) => return fail(e),
        };

        // SAFETY: no pointers.
        TRAY_THREAD.store(
            unsafe { windows_sys::Win32::System::Threading::GetCurrentThreadId() },
            Ordering::Relaxed,
        );
        // SAFETY: standard Win32 message loop; MSG is plain data, zeroed is a valid initial value.
        unsafe {
            let mut msg: MSG = std::mem::zeroed();
            while GetMessageW(&mut msg, std::ptr::null_mut(), 0, 0) > 0 {
                if msg.message == WM_SOFTWARE {
                    software.set_checked(SOFTWARE.load(Ordering::Relaxed));
                    continue;
                }
                TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
        }
    });
}

/// Windows ends the session (logout, shutdown, restart) by telling each top-level window, then ends
/// the process. The renderer is another process and its window may not exist yet (started hidden): a hidden
/// window of this thread listens instead, saves and quits like Quit does, so the next start sees no crash.
#[cfg(windows)]
fn watch_session_end() {
    use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
    use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        CreateWindowExW, DefWindowProcW, RegisterClassW, WM_ENDSESSION, WNDCLASSW,
    };
    unsafe extern "system" fn listen(h: HWND, m: u32, w: WPARAM, l: LPARAM) -> LRESULT {
        if m == WM_ENDSESSION && w != 0 {
            crate::say("Windows is ending the session: saving and quitting".into());
            crate::engine::shutdown_and_wait();
            crate::crash::quit(0);
        }
        // SAFETY: the arguments Windows gave this procedure, passed on unchanged.
        unsafe { DefWindowProcW(h, m, w, l) }
    }
    let name: Vec<u16> = "StreamerzSession".encode_utf16().chain([0]).collect();
    // SAFETY: a window class and a never-shown window of this thread, whose message loop runs below;
    // `name` outlives both calls (the class copies it).
    unsafe {
        let module = GetModuleHandleW(std::ptr::null());
        let mut class: WNDCLASSW = std::mem::zeroed();
        class.lpfnWndProc = Some(listen);
        class.hInstance = module;
        class.lpszClassName = name.as_ptr();
        RegisterClassW(&class);
        CreateWindowExW(
            0,
            name.as_ptr(),
            name.as_ptr(),
            0,
            0,
            0,
            0,
            0,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            module,
            std::ptr::null(),
        );
    }
}

#[cfg(not(any(target_os = "linux", windows)))]
pub async fn run(ui: Sender) {
    ui.send(Msg::NoTray);
}

#[cfg(all(test, target_os = "linux"))]
mod tests {
    #[test]
    fn gnome_accents_have_colours() {
        assert_eq!(super::gnome_accent("blue"), Some((0x35, 0x84, 0xe4)));
        assert_eq!(super::gnome_accent("nope"), None);
    }
}
