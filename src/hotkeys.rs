//! The Hotkey Press and Mouse Click triggers. Streamer.bot catches them with SharpHook (libuiohook)
//! and its triggers keep SharpHook's key code (`keyCode`) and modifier mask (`modifierMask`), so a
//! trigger here is the same. Listening starts only while an enabled action uses one of them:
//! - Linux: the desktop's Global Shortcuts portal over D-Bus. No root and no `input` group: the
//!   desktop asks the user to allow each shortcut (KDE, GNOME), or binds it in its config (Hyprland).
//!   Mouse clicks aren't caught: only reading /dev/input could, which also sees every key typed.
//! - Windows: RegisterHotKey, and a low-level mouse hook while a Mouse Click trigger is used.

use crate::engine::Engine;
use crate::model::Obj;
use serde_json::{json, Value};
use std::rc::Rc;
use std::sync::OnceLock;

pub const T_KEY_PRESS: u32 = 16001;
pub const T_MOUSE_CLICK: u32 = 16002;

/// SharpHook's modifier bits (left ones; right ones are these << 4).
pub const SHIFT: i64 = 1;
pub const CTRL: i64 = 2;
pub const META: i64 = 4;
pub const ALT: i64 = 8;

/// A key: SharpHook's code, its name, its X keysym name (the portal's and Hyprland's), FLTK's key
/// value, and the Windows virtual key.
pub struct Key {
    pub code: i64,
    pub name: String,
    pub keysym: String,
    pub fltk: i32,
    pub vk: u16,
}

pub fn keys() -> &'static [Key] {
    static K: OnceLock<Vec<Key>> = OnceLock::new();
    K.get_or_init(|| {
        let k = |code: i64, name: &str, keysym: &str, fltk: i32, vk: u16| Key {
            code,
            name: name.into(),
            keysym: keysym.into(),
            fltk,
            vk,
        };
        let mut v = Vec::new();
        for i in 0..26u8 {
            let c = (b'a' + i) as char;
            v.push(k(
                65 + i64::from(i),
                &c.to_ascii_uppercase().to_string(),
                &c.to_string(),
                c as i32,
                0x41 + u16::from(i),
            ));
        }
        for i in 0..10u8 {
            let c = (b'0' + i) as char;
            v.push(k(
                48 + i64::from(i),
                &c.to_string(),
                &c.to_string(),
                c as i32,
                0x30 + u16::from(i),
            ));
            let n = format!("KP_{i}");
            v.push(k(
                96 + i64::from(i),
                &format!("Num {i}"),
                &n,
                0xffb0 + i32::from(i),
                0x60 + u16::from(i),
            ));
        }
        for i in 0..24u16 {
            let n = format!("F{}", i + 1);
            let code = if i < 12 {
                112 + i64::from(i)
            } else {
                -4096 + i64::from(i - 12)
            };
            v.push(k(code, &n, &n, 0xffbe + i32::from(i), 0x70 + i));
        }
        for (code, name, keysym, fltk, vk) in [
            (27, "Esc", "Escape", 0xff1b, 0x1b),
            (192, "`", "grave", '`' as i32, 0xc0),
            (45, "-", "minus", '-' as i32, 0xbd),
            (61, "=", "equal", '=' as i32, 0xbb),
            (8, "Backspace", "BackSpace", 0xff08, 0x08),
            (9, "Tab", "Tab", 0xff09, 0x09),
            (91, "[", "bracketleft", '[' as i32, 0xdb),
            (92, "]", "bracketright", ']' as i32, 0xdd),
            (93, "\\", "backslash", '\\' as i32, 0xdc),
            (59, ";", "semicolon", ';' as i32, 0xba),
            (222, "'", "apostrophe", '\'' as i32, 0xde),
            (10, "Enter", "Return", 0xff0d, 0x0d),
            (44, ",", "comma", ',' as i32, 0xbc),
            (46, ".", "period", '.' as i32, 0xbe),
            (47, "/", "slash", '/' as i32, 0xbf),
            (32, "Space", "space", ' ' as i32, 0x20),
            (154, "Print Screen", "Print", 0xff61, 0x2c),
            (145, "Scroll Lock", "Scroll_Lock", 0xff14, 0x91),
            (19, "Pause", "Pause", 0xff13, 0x13),
            (155, "Insert", "Insert", 0xff63, 0x2d),
            (127, "Delete", "Delete", 0xffff, 0x2e),
            (36, "Home", "Home", 0xff50, 0x24),
            (35, "End", "End", 0xff57, 0x23),
            (33, "Page Up", "Prior", 0xff55, 0x21),
            (34, "Page Down", "Next", 0xff56, 0x22),
            (38, "Up", "Up", 0xff52, 0x26),
            (37, "Left", "Left", 0xff51, 0x25),
            (39, "Right", "Right", 0xff53, 0x27),
            (40, "Down", "Down", 0xff54, 0x28),
            (111, "Num /", "KP_Divide", 0xffaf, 0x6f),
            (106, "Num *", "KP_Multiply", 0xffaa, 0x6a),
            (109, "Num -", "KP_Subtract", 0xffad, 0x6d),
            (107, "Num +", "KP_Add", 0xffab, 0x6b),
            (110, "Num .", "KP_Decimal", 0xffae, 0x6e),
            (125, "Num Enter", "KP_Enter", 0xff8d, 0x0d),
            (525, "Menu", "Menu", 0xff67, 0x5d),
            (-8158, "Play/Pause", "XF86AudioPlay", 0xef14, 0xb3),
            (-8156, "Stop", "XF86AudioStop", 0xef15, 0xb2),
            (-8176, "Previous", "XF86AudioPrev", 0xef16, 0xb1),
            (-8167, "Next", "XF86AudioNext", 0xef17, 0xb0),
            (-8160, "Mute", "XF86AudioMute", 0xef12, 0xad),
            (-8144, "Volume Down", "XF86AudioLowerVolume", 0xef11, 0xae),
            (-8146, "Volume Up", "XF86AudioRaiseVolume", 0xef13, 0xaf),
        ] {
            v.push(k(code, name, keysym, fltk, vk));
        }
        v
    })
}

pub fn by_code(code: i64) -> Option<&'static Key> {
    keys().iter().find(|k| k.code == code)
}

pub fn by_fltk(key: i32) -> Option<&'static Key> {
    // Letters come lower case; the keypad's digits also as their own keys.
    keys().iter().find(|k| k.fltk == key)
}

/// The mask with right-hand modifiers counted as left ones.
pub fn norm(mask: i64) -> i64 {
    (mask | mask >> 4) & 15
}

const MODS: [(i64, &str, &str, &str); 4] = [
    (CTRL, "Ctrl", "CTRL", "CTRL"),
    (ALT, "Alt", "ALT", "ALT"),
    (SHIFT, "Shift", "SHIFT", "SHIFT"),
    (META, "Win", "LOGO", "SUPER"),
];

/// "Ctrl+Shift+F14", as the form shows it.
pub fn label(code: i64, mask: i64) -> String {
    let key = by_code(code).map_or_else(|| format!("key {code}"), |k| k.name.clone());
    let mut parts: Vec<String> = MODS
        .iter()
        .filter(|m| norm(mask) & m.0 != 0)
        .map(|m| m.1.to_string())
        .collect();
    parts.push(key);
    parts.join("+")
}

/// The portal's id for a combination.
fn id(code: i64, mask: i64) -> String {
    format!("k{code}m{}", norm(mask))
}

#[cfg_attr(windows, allow(dead_code))] // the portal (Linux) uses it
fn parse_id(id: &str) -> Option<(i64, i64)> {
    let (k, m) = id.strip_prefix('k')?.split_once('m')?;
    Some((k.parse().ok()?, m.parse().ok()?))
}

/// The shortcut as the portal's `preferred_trigger` ("CTRL+SHIFT+F14").
#[cfg_attr(windows, allow(dead_code))] // the portal (Linux) uses it
fn portal_trigger(code: i64, mask: i64) -> Option<String> {
    let k = by_code(code)?;
    let mut parts: Vec<&str> = MODS.iter().filter(|m| norm(mask) & m.0 != 0).map(|m| m.2).collect();
    parts.push(&k.keysym);
    Some(parts.join("+"))
}

/// The Hyprland config line that binds a combination to streamerz's shortcut (Hyprland's portal
/// doesn't bind them itself): Lua for a hyprland.lua config (Hyprland 0.55+), else hyprland.conf's.
pub fn hyprland_bind(code: i64, mask: i64, lua: bool) -> Option<String> {
    let k = by_code(code)?;
    let mods: Vec<&str> = MODS.iter().filter(|m| norm(mask) & m.0 != 0).map(|m| m.3).collect();
    let id = id(code, mask);
    Some(if lua {
        let mut keys = mods.clone();
        keys.push(&k.keysym);
        format!("hl.bind(\"{}\", hl.dsp.global(\"streamerz:{id}\"))", keys.join(" + "))
    } else {
        format!("bind = {}, {}, global, streamerz:{id}", mods.join(" "), k.keysym)
    })
}

/// Whether Hyprland's config is Lua (hyprland.lua) rather than hyprland.conf.
pub fn hyprland_lua() -> bool {
    std::env::var_os("XDG_CONFIG_HOME")
        .map(std::path::PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| std::path::PathBuf::from(h).join(".config")))
        .is_some_and(|c| c.join("hypr/hyprland.lua").exists())
}

/// Running listeners.
#[derive(Default)]
pub struct Live {
    combos: Vec<(i64, i64)>,
    mouse: bool,
    #[cfg(not(windows))]
    task: Option<crate::emote_events::Abort>,
    #[cfg(windows)]
    thread: Option<u32>,
}

impl Engine {
    /// The combinations the enabled Hotkey Press triggers want, and whether a Mouse Click one is used.
    fn hotkeys_wanted(&self) -> (Vec<(i64, i64)>, bool) {
        let c = self.config();
        let mut combos: Vec<(i64, i64)> = c
            .actions
            .actions
            .iter()
            .filter(|a| a.enabled)
            .flat_map(|a| a.triggers.iter())
            .filter(|t| t.get("type").and_then(Value::as_u64) == Some(u64::from(T_KEY_PRESS)))
            .filter(|t| t.get("enabled").and_then(Value::as_bool).unwrap_or(true))
            .filter_map(|t| {
                let n = |k: &str| t.get(k).and_then(Value::as_i64);
                Some((n("keyCode").filter(|c| *c != 0)?, norm(n("modifierMask").unwrap_or(0))))
            })
            .collect();
        combos.sort_unstable();
        combos.dedup();
        drop(c);
        (combos, cfg!(windows) && self.uses_trigger(T_MOUSE_CLICK))
    }

    /// Start, change or stop listening to match the triggers (cheap when nothing changed).
    pub fn hotkeys_sync(self: &Rc<Self>) {
        let (combos, mouse) = self.hotkeys_wanted();
        let mut live = self.hotkeys.borrow_mut();
        if live.combos == combos && live.mouse == mouse {
            return;
        }
        live.combos.clone_from(&combos);
        live.mouse = mouse;
        #[cfg(not(windows))]
        {
            live.task = None;
            if !combos.is_empty() {
                let h = tokio::task::spawn_local(portal(self.clone(), combos));
                live.task = Some(crate::emote_events::Abort(h));
            }
        }
        #[cfg(windows)]
        {
            if let Some(t) = live.thread.take() {
                win::stop(t);
            }
            if !combos.is_empty() || mouse {
                live.thread = win::start(self.clone(), combos, mouse);
            }
        }
    }

    /// A combination was pressed: the Hotkey Press triggers for it.
    pub fn fire_hotkey(self: &Rc<Self>, code: i64, mask: i64) {
        let n = norm(mask);
        let mut args = Obj::new();
        args.insert("key".into(), code.into());
        args.insert("modifier".into(), mask.into());
        args.insert("hasCtrl".into(), (n & CTRL != 0).into());
        args.insert("hasAlt".into(), (n & ALT != 0).into());
        args.insert("hasShift".into(), (n & SHIFT != 0).into());
        self.fire(T_KEY_PRESS, "Key Press", "Core/Inputs", "inputs", &args, |t| {
            let g = |k: &str| t.get(k).and_then(Value::as_i64).unwrap_or(0);
            g("keyCode") == code && norm(g("modifierMask")) == n
        });
        self.emit("Inputs", "KeyPress", || json!(args));
    }

    /// A mouse button was clicked (SharpHook's numbers: 1 left, 2 right, 3 middle, 4 and 5 side).
    pub fn fire_mouse(self: &Rc<Self>, button: i64, clicks: i64, x: i64, y: i64) {
        let mut args = Obj::new();
        args.insert("mouse.button".into(), button.into());
        args.insert("mouse.clicks".into(), clicks.into());
        args.insert("mouse.x".into(), x.into());
        args.insert("mouse.y".into(), y.into());
        self.fire(T_MOUSE_CLICK, "Mouse Click", "Core/Inputs", "inputs", &args, |_| true);
        self.emit("Inputs", "MouseClick", || json!(args));
    }
}

#[cfg(not(windows))]
async fn portal(e: Rc<Engine>, combos: Vec<(i64, i64)>) {
    if let Err(err) = portal_run(&e, &combos).await {
        crate::say(format!("Hotkeys: {err}"));
    }
}

/// Bind the combinations through the Global Shortcuts portal, then fire them as they're pressed.
/// The session ends when this stops (the D-Bus connection closes).
#[cfg(not(windows))]
async fn portal_run(e: &Rc<Engine>, combos: &[(i64, i64)]) -> Result<(), String> {
    use futures_util::StreamExt;
    use std::collections::HashMap;
    use zbus::zvariant::{ObjectPath, OwnedObjectPath, OwnedValue, Value};
    use zbus::{Connection, Proxy};
    const DEST: &str = "org.freedesktop.portal.Desktop";
    const PATH: &str = "/org/freedesktop/portal/desktop";
    let err = |x: zbus::Error| x.to_string();

    let c = Connection::session().await.map_err(err)?;
    // The portal wants an app id; an app outside Flatpak names itself (portal 1.19+), and the name
    // must be a desktop file: a hidden one (not in the menus) is written when missing.
    let entry = desktop_file().map_err(|x| format!("can't write streamerz.desktop ({x})"))?;
    if let Ok(r) = Proxy::new(&c, DEST, PATH, "org.freedesktop.host.portal.Registry").await {
        if let Err(x) = r
            .call_method("Register", &("streamerz", HashMap::<&str, Value>::new()))
            .await
        {
            crate::say(format!(
                "Hotkeys: the portal didn't take the app id ({x}); {}",
                entry.display()
            ));
        }
    }
    let gs = Proxy::new(&c, DEST, PATH, "org.freedesktop.portal.GlobalShortcuts")
        .await
        .map_err(err)?;
    let sender = c
        .unique_name()
        .map(|n| n.trim_start_matches(':').replace('.', "_"))
        .unwrap_or_default();
    // A portal call answers on a Request object, listened to before the call.
    let request = |token: &'static str| {
        let (c, sender) = (c.clone(), sender.clone());
        async move {
            let path = format!("{PATH}/request/{sender}/{token}");
            let p = Proxy::new(&c, DEST, path, "org.freedesktop.portal.Request")
                .await
                .map_err(err)?;
            p.receive_signal("Response").await.map_err(err)
        }
    };
    let answer = |m: Option<zbus::Message>| -> Result<HashMap<String, OwnedValue>, String> {
        let m = m.ok_or("the portal went away")?;
        let (code, results): (u32, HashMap<String, OwnedValue>) = m.body().deserialize().map_err(err)?;
        match code {
            0 => Ok(results),
            1 => Err("the shortcuts weren't allowed".into()),
            _ => Err("the desktop refused the shortcuts".into()),
        }
    };

    let mut r = request("szsession").await?;
    let opts = HashMap::from([
        ("handle_token", Value::from("szsession")),
        ("session_handle_token", Value::from("streamerz")),
    ]);
    gs.call_method("CreateSession", &(opts,)).await.map_err(|x| {
        format!(
            "no Global Shortcuts portal ({x}): the desktop needs xdg-desktop-portal with KDE, GNOME 48+ or Hyprland"
        )
    })?;
    let got = answer(r.next().await)?;
    let session: String = got
        .get("session_handle")
        .and_then(|v| String::try_from(v.try_clone().ok()?).ok())
        .ok_or("the portal gave no session")?;
    let session = OwnedObjectPath::try_from(session).map_err(|x| x.to_string())?;

    let shortcuts: Vec<(String, HashMap<&str, Value>)> = combos
        .iter()
        .map(|&(k, m)| {
            let mut o = HashMap::from([("description", Value::from(format!("streamerz: {}", label(k, m))))]);
            if let Some(t) = portal_trigger(k, m) {
                o.insert("preferred_trigger", Value::from(t));
            }
            (id(k, m), o)
        })
        .collect();
    let mut r = request("szbind").await?;
    let mut activated = gs.receive_signal("Activated").await.map_err(err)?;
    let opts = HashMap::from([("handle_token", Value::from("szbind"))]);
    gs.call_method("BindShortcuts", &(ObjectPath::from(&session), shortcuts, "", opts))
        .await
        .map_err(err)?;
    let got = answer(r.next().await)?;
    let bound = got.get("shortcuts").map(|v| format!("{v:?}")).unwrap_or_default();
    crate::say(format!(
        "Hotkeys: {} shortcut(s) given to the desktop{}",
        combos.len(),
        if bound.contains("trigger_description") {
            ""
        } else {
            " (bind them in the desktop's settings)"
        }
    ));
    while let Some(m) = activated.next().await {
        let Ok((_, which, _, _)) = m
            .body()
            .deserialize::<(OwnedObjectPath, String, u64, HashMap<String, OwnedValue>)>()
        else {
            continue;
        };
        if let Some((k, mask)) = parse_id(&which) {
            e.fire_hotkey(k, mask);
        }
    }
    Ok(())
}

/// `~/.local/share/applications/streamerz.desktop`, the portal's app id: written (hidden from menus)
/// when missing or pointing at another streamerz.
#[cfg(not(windows))]
fn desktop_file() -> std::io::Result<std::path::PathBuf> {
    let data = std::env::var_os("XDG_DATA_HOME")
        .map(std::path::PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| std::path::PathBuf::from(h).join(".local/share")))
        .ok_or(std::io::ErrorKind::NotFound)?;
    let file = data.join("applications/streamerz.desktop");
    let body = format!(
        "[Desktop Entry]\nType=Application\nName=streamerz\nExec=\"{}\"\nNoDisplay=true\n",
        std::env::current_exe()?.display()
    );
    if std::fs::read_to_string(&file).ok().as_deref() != Some(body.as_str()) {
        std::fs::create_dir_all(data.join("applications"))?;
        std::fs::write(&file, body)?;
    }
    Ok(file)
}

#[cfg(windows)]
mod win {
    use super::*;
    use std::sync::Mutex;
    use tokio::sync::mpsc::{unbounded_channel, UnboundedSender};
    use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
    use windows_sys::Win32::System::Threading::GetCurrentThreadId;
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
        GetDoubleClickTime, RegisterHotKey, MOD_ALT, MOD_CONTROL, MOD_NOREPEAT, MOD_SHIFT, MOD_WIN,
    };
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        CallNextHookEx, GetMessageW, PostThreadMessageW, SetWindowsHookExW, UnhookWindowsHookEx, MSG, MSLLHOOKSTRUCT,
        WH_MOUSE_LL, WM_HOTKEY, WM_LBUTTONDOWN, WM_MBUTTONDOWN, WM_QUIT, WM_RBUTTONDOWN, WM_XBUTTONDOWN,
    };

    enum Ev {
        Key(usize),
        Mouse(i64, i64, i64),
    }

    static MOUSE: Mutex<Option<UnboundedSender<Ev>>> = Mutex::new(None);
    /// The last click (button, time) and its count, for double clicks.
    static LAST: Mutex<(i64, u32, i64)> = Mutex::new((0, 0, 0));

    unsafe extern "system" fn hook(code: i32, w: usize, l: isize) -> isize {
        if code >= 0 {
            let m = &*(l as *const MSLLHOOKSTRUCT);
            let button = match w as u32 {
                WM_LBUTTONDOWN => 1,
                WM_RBUTTONDOWN => 2,
                WM_MBUTTONDOWN => 3,
                WM_XBUTTONDOWN => 3 + i64::from((m.mouseData >> 16) as u16),
                _ => 0,
            };
            if button != 0 {
                let clicks = LAST.lock().map_or(1, |mut last| {
                    let n = if last.0 == button && m.time.wrapping_sub(last.1) <= GetDoubleClickTime() {
                        last.2 + 1
                    } else {
                        1
                    };
                    *last = (button, m.time, n);
                    n
                });
                if let Ok(s) = MOUSE.lock() {
                    if let Some(s) = s.as_ref() {
                        let _ = s.send(Ev::Mouse(
                            button,
                            clicks,
                            (i64::from(m.pt.x) << 32) | i64::from(m.pt.y as u32),
                        ));
                    }
                }
            }
        }
        CallNextHookEx(std::ptr::null_mut(), code, w, l)
    }

    /// Listen on a thread of its own (hotkeys and hooks need a message loop); its id, to stop it.
    pub fn start(e: Rc<Engine>, combos: Vec<(i64, i64)>, mouse: bool) -> Option<u32> {
        let (tx, mut rx) = unbounded_channel::<Ev>();
        let (id_tx, id_rx) = std::sync::mpsc::channel();
        let list = combos.clone();
        std::thread::spawn(move || unsafe {
            let _ = id_tx.send(GetCurrentThreadId());
            for (i, &(k, m)) in list.iter().enumerate() {
                let Some(key) = by_code(k) else { continue };
                let mut mods = MOD_NOREPEAT;
                for (bit, flag) in [(CTRL, MOD_CONTROL), (ALT, MOD_ALT), (SHIFT, MOD_SHIFT), (META, MOD_WIN)] {
                    if norm(m) & bit != 0 {
                        mods |= flag;
                    }
                }
                if RegisterHotKey(std::ptr::null_mut(), i as i32 + 1, mods, u32::from(key.vk)) == 0 {
                    crate::say(format!("Hotkeys: {} is taken by another program", label(k, m)));
                }
            }
            let hooked = mouse.then(|| {
                if let Ok(mut s) = MOUSE.lock() {
                    *s = Some(tx.clone());
                }
                SetWindowsHookExW(WH_MOUSE_LL, Some(hook), GetModuleHandleW(std::ptr::null()), 0)
            });
            let mut msg: MSG = std::mem::zeroed();
            while GetMessageW(&mut msg, std::ptr::null_mut(), 0, 0) > 0 {
                if msg.message == WM_HOTKEY {
                    let _ = tx.send(Ev::Key(msg.wParam.saturating_sub(1)));
                }
            }
            if let Some(h) = hooked {
                UnhookWindowsHookEx(h);
                if let Ok(mut s) = MOUSE.lock() {
                    *s = None;
                }
            }
            // Hotkeys go with the thread.
        });
        tokio::task::spawn_local(async move {
            while let Some(ev) = rx.recv().await {
                match ev {
                    Ev::Key(i) => {
                        if let Some(&(k, m)) = combos.get(i) {
                            e.fire_hotkey(k, m);
                        }
                    }
                    Ev::Mouse(b, n, xy) => e.fire_mouse(b, n, xy >> 32, i64::from(xy as i32)),
                }
            }
        });
        id_rx.recv().ok()
    }

    pub fn stop(thread: u32) {
        unsafe {
            PostThreadMessageW(thread, WM_QUIT, 0, 0);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keys_names_and_ids() {
        // Streamer.bot's docs: key 92 with modifier 11 is Ctrl+Alt+Shift+].
        assert_eq!(label(92, 11), "Ctrl+Alt+Shift+]");
        assert_eq!(label(-4095, CTRL | SHIFT << 4), "Ctrl+Shift+F14");
        assert_eq!(portal_trigger(65, CTRL | META).as_deref(), Some("CTRL+LOGO+a"));
        assert_eq!(
            hyprland_bind(-4095, CTRL | SHIFT, false).as_deref(),
            Some("bind = CTRL SHIFT, F14, global, streamerz:k-4095m3")
        );
        assert_eq!(
            hyprland_bind(-4095, CTRL | SHIFT, true).as_deref(),
            Some("hl.bind(\"CTRL + SHIFT + F14\", hl.dsp.global(\"streamerz:k-4095m3\"))")
        );
        assert_eq!(parse_id(&id(-4095, 3)), Some((-4095, 3)));
        assert_eq!(by_fltk(0xffbe).map(|k| k.code), Some(112));
        assert_eq!(by_fltk('q' as i32).map(|k| k.vk), Some(0x51));
        // Codes are unique.
        let mut codes: Vec<i64> = keys().iter().map(|k| k.code).collect();
        codes.sort_unstable();
        codes.dedup();
        assert_eq!(codes.len(), keys().len());
    }
}
