//! Tray icon: click (or "Show/Hide") toggles the window, "Quit" exits. Without a tray (e.g. no
//! StatusNotifier host on Linux) the window stays up instead.

use crate::ui::Msg;
use crate::version;
use fltk::app::Sender;

// ponytail: pre-shrunk raw pixels, no image decoder. After changing the icon run:
// magick icon_streamerz.png -resize 64x64 -depth 8 rgba:src/icon.rgba
pub const ICON: &[u8] = include_bytes!("icon.rgba");
pub const SIZE: i32 = 64;

#[cfg(target_os = "linux")]
pub async fn run(ui: Sender<Msg>) {
    use ksni::TrayMethods;

    struct Tray(Sender<Msg>);
    impl ksni::Tray for Tray {
        fn id(&self) -> String {
            "streamerz".into()
        }
        fn title(&self) -> String {
            "streamerz".into()
        }
        fn icon_pixmap(&self) -> Vec<ksni::Icon> {
            // RGBA to ARGB, as StatusNotifierItem wants.
            let data = ICON
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
                    label: "Show/Hide".into(),
                    activate: Box::new(|t: &mut Self| t.0.send(Msg::Toggle)),
                    ..Default::default()
                }
                .into(),
                StandardItem {
                    label: "Quit".into(),
                    activate: Box::new(|_| std::process::exit(0)),
                    ..Default::default()
                }
                .into(),
            ]
        }
    }

    match Tray(ui).spawn().await {
        // The tray lives as long as its handle.
        Ok(_handle) => std::future::pending().await,
        Err(e) => {
            crate::say(format!("no tray icon ({e})"));
            ui.send(Msg::NoTray);
        }
    }
}

#[cfg(windows)]
pub async fn run(ui: Sender<Msg>) {
    use tray_icon::menu::{Menu, MenuEvent, MenuItem};
    use tray_icon::{Icon, MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
    use windows_sys::Win32::UI::WindowsAndMessaging::{DispatchMessageW, GetMessageW, TranslateMessage, MSG};

    // Win32 delivers tray clicks to the thread that created the icon, so it gets its own
    // thread with a message loop.
    std::thread::spawn(move || {
        let open = MenuItem::new("Show/Hide", true, None);
        let close = MenuItem::new("Quit", true, None);
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

        let tray = Icon::from_rgba(ICON.to_vec(), SIZE as u32, SIZE as u32)
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
