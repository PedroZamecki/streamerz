//! The Hotkey Press and Mouse Click triggers. Streamer.bot catches them with SharpHook (libuiohook)
//! and its triggers keep SharpHook's key code (`keyCode`) and modifier mask (`modifierMask`), so a
//! trigger here is the same. Listening starts only while an enabled action uses one of them, and asks
//! for the least access that works:
//! - Windows: RegisterHotKey, and a low-level mouse hook while a Mouse Click trigger is used.
//! - Linux on X11: the X server itself (key grabs, XInput2's raw clicks). Nothing to set up.
//! - Linux on Wayland, keys: the desktop's Global Shortcuts portal over D-Bus (KDE and GNOME ask the
//!   user to allow each shortcut; on Hyprland, Streamerz binds them through Hyprland's socket while it
//!   runs). Without the portal, the keyboards themselves, once the user allows it (`devinput`).
//! - Linux on Wayland, clicks: the mice themselves (`devinput`), once the user allows it: Wayland gives
//!   no program the clicks made in other windows.

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
/// value, the Windows virtual key, and the Linux evdev code (a key's place on the keyboard, read from
/// the device where nothing else can give the key).
pub struct Key {
    pub code: i64,
    pub name: String,
    pub keysym: String,
    pub fltk: i32,
    pub vk: u16,
    pub evdev: u16,
}

pub fn keys() -> &'static [Key] {
    static K: OnceLock<Vec<Key>> = OnceLock::new();
    K.get_or_init(|| {
        let k = |code: i64, name: &str, keysym: &str, fltk: i32, vk: u16, evdev: u16| Key {
            code,
            name: name.into(),
            keysym: keysym.into(),
            fltk,
            vk,
            evdev,
        };
        // evdev's codes for a to z, 0 to 9 and the keypad's 0 to 9.
        const LETTERS: [u16; 26] = [
            30, 48, 46, 32, 18, 33, 34, 35, 23, 36, 37, 38, 50, 49, 24, 25, 16, 19, 31, 20, 22, 47, 17, 45, 21, 44,
        ];
        const DIGITS: [u16; 10] = [11, 2, 3, 4, 5, 6, 7, 8, 9, 10];
        const KEYPAD: [u16; 10] = [82, 79, 80, 81, 75, 76, 77, 71, 72, 73];
        let mut v = Vec::new();
        for i in 0..26u8 {
            let c = (b'a' + i) as char;
            v.push(k(
                65 + i64::from(i),
                &c.to_ascii_uppercase().to_string(),
                &c.to_string(),
                c as i32,
                0x41 + u16::from(i),
                LETTERS[usize::from(i)],
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
                DIGITS[usize::from(i)],
            ));
            let n = format!("KP_{i}");
            v.push(k(
                96 + i64::from(i),
                &format!("Num {i}"),
                &n,
                0xffb0 + i32::from(i),
                0x60 + u16::from(i),
                KEYPAD[usize::from(i)],
            ));
        }
        for i in 0..24u16 {
            let n = format!("F{}", i + 1);
            let code = if i < 12 {
                112 + i64::from(i)
            } else {
                -4096 + i64::from(i - 12)
            };
            // F1 to F10 are 59 to 68, F11 and F12 87 and 88, F13 to F24 183 to 194.
            let evdev = match i {
                0..10 => 59 + i,
                10 | 11 => 77 + i,
                _ => 171 + i,
            };
            v.push(k(code, &n, &n, 0xffbe + i32::from(i), 0x70 + i, evdev));
        }
        for (code, name, keysym, fltk, vk, evdev) in [
            (27, "Esc", "Escape", 0xff1b, 0x1b, 1),
            (192, "`", "grave", '`' as i32, 0xc0, 41),
            (45, "-", "minus", '-' as i32, 0xbd, 12),
            (61, "=", "equal", '=' as i32, 0xbb, 13),
            (8, "Backspace", "BackSpace", 0xff08, 0x08, 14),
            (9, "Tab", "Tab", 0xff09, 0x09, 15),
            (91, "[", "bracketleft", '[' as i32, 0xdb, 26),
            (92, "]", "bracketright", ']' as i32, 0xdd, 27),
            (93, "\\", "backslash", '\\' as i32, 0xdc, 43),
            (59, ";", "semicolon", ';' as i32, 0xba, 39),
            (222, "'", "apostrophe", '\'' as i32, 0xde, 40),
            (10, "Enter", "Return", 0xff0d, 0x0d, 28),
            (44, ",", "comma", ',' as i32, 0xbc, 51),
            (46, ".", "period", '.' as i32, 0xbe, 52),
            (47, "/", "slash", '/' as i32, 0xbf, 53),
            (32, "Space", "space", ' ' as i32, 0x20, 57),
            (154, "Print Screen", "Print", 0xff61, 0x2c, 99),
            (145, "Scroll Lock", "Scroll_Lock", 0xff14, 0x91, 70),
            (19, "Pause", "Pause", 0xff13, 0x13, 119),
            (155, "Insert", "Insert", 0xff63, 0x2d, 110),
            (127, "Delete", "Delete", 0xffff, 0x2e, 111),
            (36, "Home", "Home", 0xff50, 0x24, 102),
            (35, "End", "End", 0xff57, 0x23, 107),
            (33, "Page Up", "Prior", 0xff55, 0x21, 104),
            (34, "Page Down", "Next", 0xff56, 0x22, 109),
            (38, "Up", "Up", 0xff52, 0x26, 103),
            (37, "Left", "Left", 0xff51, 0x25, 105),
            (39, "Right", "Right", 0xff53, 0x27, 106),
            (40, "Down", "Down", 0xff54, 0x28, 108),
            (111, "Num /", "KP_Divide", 0xffaf, 0x6f, 98),
            (106, "Num *", "KP_Multiply", 0xffaa, 0x6a, 55),
            (109, "Num -", "KP_Subtract", 0xffad, 0x6d, 74),
            (107, "Num +", "KP_Add", 0xffab, 0x6b, 78),
            (110, "Num .", "KP_Decimal", 0xffae, 0x6e, 83),
            (125, "Num Enter", "KP_Enter", 0xff8d, 0x0d, 96),
            (525, "Menu", "Menu", 0xff67, 0x5d, 127),
            (-8158, "Play/Pause", "XF86AudioPlay", 0xef14, 0xb3, 164),
            (-8156, "Stop", "XF86AudioStop", 0xef15, 0xb2, 166),
            (-8176, "Previous", "XF86AudioPrev", 0xef16, 0xb1, 165),
            (-8167, "Next", "XF86AudioNext", 0xef17, 0xb0, 163),
            (-8160, "Mute", "XF86AudioMute", 0xef12, 0xad, 113),
            (-8144, "Volume Down", "XF86AudioLowerVolume", 0xef11, 0xae, 114),
            (-8146, "Volume Up", "XF86AudioRaiseVolume", 0xef13, 0xaf, 115),
        ] {
            v.push(k(code, name, keysym, fltk, vk, evdev));
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

/// A mouse button's name, as the Mouse Click trigger's form shows it (0: any).
pub fn button_name(n: i64) -> &'static str {
    crate::i18n::tr(match n {
        1 => "input.button_left",
        2 => "input.button_right",
        3 => "input.button_middle",
        4 => "input.button_back",
        5 => "input.button_forward",
        _ => "input.button_any",
    })
}

/// FLTK's mouse button (1 left, 2 middle, 3 right, 4 back, 5 forward) as SharpHook's.
pub fn from_fltk_button(b: i32) -> Option<i64> {
    match b {
        1 => Some(1),
        2 => Some(3),
        3 => Some(2),
        4 => Some(4),
        5 => Some(5),
        _ => None,
    }
}

/// What the Press the Keys dialog says about how the shortcut is caught here.
pub fn how_keys() -> &'static str {
    use crate::i18n::tr;
    if cfg!(windows) {
        return tr("dialogs.keys.windows");
    }
    #[cfg(not(windows))]
    {
        if crate::x11input::session() {
            return tr("dialogs.keys.x11");
        }
        if PORTAL.load(Ordering::Relaxed) == 2 {
            return tr("dialogs.keys.no_portal");
        }
        if hyprland_socket().is_some() {
            return tr("dialogs.keys.hyprland");
        }
    }
    tr("dialogs.keys.desktop")
}

/// The button under a trigger's access note.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Fix {
    Allow,
    Remove,
}

/// What a Key Press (`mouse` false) or Mouse Click trigger's form says about catching it here, and the
/// button that changes it, if any.
pub fn access_note(mouse: bool) -> (String, Option<Fix>) {
    use crate::i18n::tr;
    #[cfg(windows)]
    {
        (
            tr(if mouse {
                "input.windows_mouse"
            } else {
                "dialogs.keys.windows"
            })
            .into(),
            None,
        )
    }
    #[cfg(not(windows))]
    {
        use crate::devinput::{access, rule_installed, Access, Kind};
        if crate::x11input::session() {
            return (
                tr(if mouse { "input.x11_mouse" } else { "dialogs.keys.x11" }).into(),
                None,
            );
        }
        let kind = if mouse { Kind::Mice } else { Kind::Keyboards };
        let ours = rule_installed(kind);
        // Keys need the keyboards only without the portal (or to take back an earlier allow).
        if !mouse && PORTAL.load(Ordering::Relaxed) != 2 && !ours {
            return (how_keys().into(), None);
        }
        let place = if mouse && hyprland_socket().is_none() {
            format!(" {}", tr("input.no_position"))
        } else {
            String::new()
        };
        match (access(kind), ours) {
            (Access::Ready, true) => (
                format!(
                    "{}{place}",
                    tr(if mouse {
                        "input.mice_allowed"
                    } else {
                        "input.keys_allowed"
                    })
                ),
                Some(Fix::Remove),
            ),
            (Access::Ready, false) => (
                format!(
                    "{}{place}",
                    tr(if mouse { "input.mice_group" } else { "input.keys_group" })
                ),
                None,
            ),
            (Access::NoDevice, _) => (
                tr(if mouse { "input.no_mouse" } else { "input.no_keyboard" }).into(),
                ours.then_some(Fix::Remove),
            ),
            // Allowed but not yet in effect (the session isn't a local one, or udev hasn't caught up).
            (Access::Blocked, true) => (tr("input.pending").into(), Some(Fix::Remove)),
            (Access::Blocked, false) => (
                tr(if mouse { "input.mice_why" } else { "input.keys_why" }).into(),
                Some(Fix::Allow),
            ),
        }
    }
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
        (combos, self.uses_trigger(T_MOUSE_CLICK))
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
            if !combos.is_empty() || mouse {
                let h = tokio::task::spawn_local(linux(self.clone(), combos, mouse));
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

    /// A mouse button was clicked (SharpHook's numbers: 1 left, 2 right, 3 middle, 4 back, 5 forward).
    /// A trigger's `button` (Streamerz's own; 0 or missing: any) picks one.
    pub fn fire_mouse(self: &Rc<Self>, button: i64, clicks: i64, x: i64, y: i64) {
        let mut args = Obj::new();
        args.insert("mouse.button".into(), button.into());
        args.insert("mouse.clicks".into(), clicks.into());
        args.insert("mouse.x".into(), x.into());
        args.insert("mouse.y".into(), y.into());
        self.fire(T_MOUSE_CLICK, "Mouse Click", "Core/Inputs", "inputs", &args, |t| {
            matches!(t.get("button").and_then(Value::as_i64), None | Some(0))
                || t.get("button").and_then(Value::as_i64) == Some(button)
        });
        self.emit("Inputs", "MouseClick", || json!(args));
    }
}

/// How Linux listens: see the module's notes.
#[cfg(not(windows))]
async fn linux(e: Rc<Engine>, combos: Vec<(i64, i64)>, mouse: bool) {
    if crate::x11input::session() {
        let (k, c) = (e.clone(), e.clone());
        let on_key = move |code, mask| k.fire_hotkey(code, mask);
        let on_click = move |b, n, x, y| c.fire_mouse(b, n, x, y);
        if let Err(err) = crate::x11input::run(&combos, mouse, on_key, on_click).await {
            crate::error(format!("Hotkeys: {err}"));
        }
        return;
    }
    let _mice =
        mouse.then(|| crate::emote_events::Abort(tokio::task::spawn_local(devices(e.clone(), true, Vec::new()))));
    if !combos.is_empty() {
        match portal_run(&e, &combos).await {
            Ok(()) => {}
            Err(Portal::Missing(why)) => {
                PORTAL.store(2, Ordering::Relaxed);
                use crate::devinput::{access, Access, Kind};
                if access(Kind::Keyboards) == Access::Ready {
                    crate::say(
                        "Hotkeys: no Global Shortcuts portal; reading the keyboards (allowed in the trigger)".into(),
                    );
                } else {
                    crate::error(format!(
                        "Hotkeys: this desktop has no Global Shortcuts portal ({why}). Open the Key Press trigger \
                         to let Streamerz read the keyboards instead."
                    ));
                }
                // Starts reading as soon as it's allowed.
                devices(e.clone(), false, combos).await;
            }
            Err(Portal::Failed(err)) => crate::error(format!("Hotkeys: {err}")),
        }
    }
    std::future::pending::<()>().await;
}

/// Mouse clicks and, with `combos`, key presses read from the devices (Wayland).
#[cfg(not(windows))]
async fn devices(e: Rc<Engine>, mouse: bool, combos: Vec<(i64, i64)>) {
    use crate::devinput::{access, Access, Ev, Kind};
    let keyboard = !combos.is_empty();
    if mouse {
        match access(Kind::Mice) {
            Access::Ready => crate::say("Mouse Click: reading the mice".into()),
            Access::NoDevice => crate::warn("Mouse Click: no mouse found; listening for one".into()),
            Access::Blocked => crate::warn(
                "Mouse Click: Streamerz isn't allowed to read the mice yet. Open the Mouse Click trigger to allow it."
                    .into(),
            ),
        }
    }
    // Clicks wait for the pointer's place (asked of Hyprland); one at a time, in order.
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<(i64, i64)>();
    let on: Rc<dyn Fn(Ev)> = Rc::new({
        let e = e.clone();
        move |ev| match ev {
            Ev::Click(b, n) => drop(tx.send((b, n))),
            Ev::Key(code, mods) => {
                if let Some(k) = keys().iter().find(|k| k.evdev == code) {
                    if let Some(&(c, m)) = combos.iter().find(|(c, m)| *c == k.code && norm(*m) == mods) {
                        e.fire_hotkey(c, m);
                    }
                }
            }
        }
    });
    let clicks = async {
        while let Some((b, n)) = rx.recv().await {
            let (x, y) = hyprland_cursor().await.unwrap_or((0, 0));
            e.fire_mouse(b, n, x, y);
        }
    };
    let read = crate::devinput::run(mouse, keyboard, on);
    tokio::select! {
        r = read => if let Err(err) = r { crate::error(format!("Input devices: {err}")) },
        () = clicks => {}
    }
}

/// The portal's state, for the trigger's form: 0 not tried, 1 there, 2 missing.
#[cfg(not(windows))]
pub static PORTAL: std::sync::atomic::AtomicU8 = std::sync::atomic::AtomicU8::new(0);
#[cfg(not(windows))]
use std::sync::atomic::Ordering;

#[cfg(not(windows))]
enum Portal {
    Missing(String),
    Failed(String),
}

#[cfg(not(windows))]
impl From<String> for Portal {
    fn from(s: String) -> Self {
        Portal::Failed(s)
    }
}

#[cfg(not(windows))]
impl From<&str> for Portal {
    fn from(s: &str) -> Self {
        Portal::Failed(s.into())
    }
}

/// Hyprland's control socket, when this is Hyprland.
#[cfg(not(windows))]
fn hyprland_socket() -> Option<std::path::PathBuf> {
    let sig = std::env::var_os("HYPRLAND_INSTANCE_SIGNATURE")?;
    let run = std::env::var_os("XDG_RUNTIME_DIR")?;
    let p = std::path::PathBuf::from(run)
        .join("hypr")
        .join(sig)
        .join(".socket.sock");
    p.exists().then_some(p)
}

/// One request to Hyprland's socket, blocking (it answers at once); its reply.
#[cfg(not(windows))]
fn hyprland(cmd: &str) -> Option<String> {
    use std::io::{Read, Write};
    let mut s = std::os::unix::net::UnixStream::connect(hyprland_socket()?).ok()?;
    s.set_read_timeout(Some(std::time::Duration::from_secs(2))).ok()?;
    s.write_all(cmd.as_bytes()).ok()?;
    let mut out = String::new();
    s.read_to_string(&mut out).ok()?;
    Some(out)
}

/// Where the pointer is, from Hyprland (other Wayland desktops don't tell).
#[cfg(not(windows))]
async fn hyprland_cursor() -> Option<(i64, i64)> {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let mut s = tokio::net::UnixStream::connect(hyprland_socket()?).await.ok()?;
    s.write_all(b"j/cursorpos").await.ok()?;
    let mut out = String::new();
    s.read_to_string(&mut out).await.ok()?;
    let v: Value = serde_json::from_str(&out).ok()?;
    Some((v.get("x")?.as_i64()?, v.get("y")?.as_i64()?))
}

/// A combination's modifiers as Hyprland's binds list shows them (SHIFT 1, CTRL 4, ALT 8, SUPER 64).
#[cfg_attr(windows, allow(dead_code))]
fn hyprland_modmask(mask: i64) -> i64 {
    let m = norm(mask);
    [(SHIFT, 1), (CTRL, 4), (ALT, 8), (META, 64)]
        .iter()
        .filter(|(b, _)| m & b != 0)
        .fold(0, |a, (_, h)| a | h)
}

/// The requests that add and remove Streamerz's bind for a combination on Hyprland's socket: Lua's
/// `eval` for a hyprland.lua config, else `keyword`. The bind doesn't take the key from other programs
/// (non-consuming), as Streamer.bot's hook doesn't.
#[cfg_attr(windows, allow(dead_code))]
fn hyprland_requests(code: i64, mask: i64, lua: bool) -> Option<(String, String)> {
    let k = by_code(code)?;
    let mods: Vec<&str> = MODS.iter().filter(|m| norm(mask) & m.0 != 0).map(|m| m.3).collect();
    let id = id(code, mask);
    let what = format!("Streamerz: {}", label(code, mask)).replace(['"', ','], "");
    Some(if lua {
        let mut keys = mods.clone();
        keys.push(&k.keysym);
        let keys = keys.join(" + ");
        (
            format!(
                "eval hl.bind(\"{keys}\", hl.dsp.global(\"streamerz:{id}\"), {{description = \"{what}\", non_consuming = true}})"
            ),
            format!("eval hl.unbind(\"{keys}\")"),
        )
    } else {
        let mods = mods.join(" ");
        (
            format!("keyword bindnd {mods}, {}, {what}, global, streamerz:{id}", k.keysym),
            format!("keyword unbind {mods}, {}", k.keysym),
        )
    })
}

/// Streamerz's binds on Hyprland (the requests that remove them), removed when listening stops or
/// Streamerz quits (`release`).
#[cfg(not(windows))]
struct HyprlandBinds(Vec<String>);

#[cfg(not(windows))]
static HYPRLAND_UNDO: std::sync::Mutex<Vec<String>> = std::sync::Mutex::new(Vec::new());

/// On quit: take Streamerz's binds back from the desktop (the tasks that hold them aren't dropped).
pub fn release() {
    #[cfg(not(windows))]
    for r in HYPRLAND_UNDO
        .lock()
        .map(|mut u| std::mem::take(&mut *u))
        .unwrap_or_default()
    {
        let _ = hyprland(&r);
    }
}

#[cfg(not(windows))]
impl HyprlandBinds {
    /// Bind each combination Hyprland doesn't already bind to something else.
    fn add(combos: &[(i64, i64)]) -> Self {
        let lua = hyprland_lua();
        let list: Vec<Value> = hyprland("j/binds")
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default();
        let mut undo = Vec::new();
        for &(code, mask) in combos {
            let (Some(k), Some((add, remove))) = (by_code(code), hyprland_requests(code, mask, lua)) else {
                continue;
            };
            let same: Vec<&Value> = list
                .iter()
                .filter(|b| {
                    b.get("modmask").and_then(Value::as_i64) == Some(hyprland_modmask(mask))
                        && b.get("key")
                            .and_then(Value::as_str)
                            .is_some_and(|x| x.eq_ignore_ascii_case(&k.keysym))
                        && b.get("submap").and_then(Value::as_str).unwrap_or("").is_empty()
                })
                .collect();
            let ours = |b: &&Value| {
                b.get("description")
                    .and_then(Value::as_str)
                    .is_some_and(|d| d.starts_with("Streamerz:"))
            };
            if same.iter().any(|b| !ours(b)) {
                crate::warn(format!(
                    "Hotkeys: Hyprland already binds {} to something else; change one of them",
                    label(code, mask)
                ));
                continue;
            }
            // One left by an earlier run that ended without removing it.
            if !same.is_empty() {
                let _ = hyprland(&remove);
            }
            match hyprland(&add) {
                Some(r) if r.trim() == "ok" => {
                    if let Ok(mut u) = HYPRLAND_UNDO.lock() {
                        u.push(remove.clone());
                    }
                    undo.push(remove);
                }
                r => crate::warn(format!(
                    "Hotkeys: Hyprland didn't take {} ({})",
                    label(code, mask),
                    r.unwrap_or_default().trim()
                )),
            }
        }
        Self(undo)
    }
}

#[cfg(not(windows))]
impl Drop for HyprlandBinds {
    fn drop(&mut self) {
        if let Ok(mut u) = HYPRLAND_UNDO.lock() {
            u.retain(|r| !self.0.contains(r));
        }
        for r in &self.0 {
            let _ = hyprland(r);
        }
    }
}

/// Bind the combinations through the Global Shortcuts portal, then fire them as they're pressed.
/// The session ends when this stops (the D-Bus connection closes).
#[cfg(not(windows))]
async fn portal_run(e: &Rc<Engine>, combos: &[(i64, i64)]) -> Result<(), Portal> {
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
            crate::warn(format!(
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
    gs.call_method("CreateSession", &(opts,))
        .await
        .map_err(|x| Portal::Missing(x.to_string()))?;
    PORTAL.store(1, Ordering::Relaxed);
    let got = answer(r.next().await)?;
    let session: String = got
        .get("session_handle")
        .and_then(|v| String::try_from(v.try_clone().ok()?).ok())
        .ok_or("the portal gave no session")?;
    let session = OwnedObjectPath::try_from(session).map_err(|x| x.to_string())?;

    let shortcuts: Vec<(String, HashMap<&str, Value>)> = combos
        .iter()
        .map(|&(k, m)| {
            let mut o = HashMap::from([("description", Value::from(format!("Streamerz: {}", label(k, m))))]);
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
    // Hyprland's portal takes the shortcuts but doesn't bind keys to them: Streamerz does, while it runs.
    let _binds = hyprland_socket().map(|_| HyprlandBinds::add(combos));
    crate::say(format!(
        "Hotkeys: {} shortcut(s) given to the desktop{}",
        combos.len(),
        if _binds.is_some() {
            " (bound in Hyprland)"
        } else if bound.contains("trigger_description") {
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
        "[Desktop Entry]\nType=Application\nName=Streamerz\nExec=\"{}\"\nNoDisplay=true\n",
        crate::update::exe().ok_or(std::io::ErrorKind::NotFound)?.display()
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
        CallNextHookEx, CreateWindowExW, DestroyWindow, GetMessageW, PostThreadMessageW, SetWindowsHookExW,
        UnhookWindowsHookEx, HWND_MESSAGE, MSG, MSLLHOOKSTRUCT, WH_MOUSE_LL, WM_HOTKEY, WM_LBUTTONDOWN, WM_MBUTTONDOWN,
        WM_QUIT, WM_RBUTTONDOWN, WM_XBUTTONDOWN,
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
            // The hotkeys belong to a message-only window of this thread (not to the thread itself, which
            // Wine's server crashes on when input is injected); its WM_HOTKEY still comes through GetMessageW.
            let class: Vec<u16> = "STATIC".encode_utf16().chain([0]).collect();
            let window = CreateWindowExW(
                0,
                class.as_ptr(),
                std::ptr::null(),
                0,
                0,
                0,
                0,
                0,
                HWND_MESSAGE,
                std::ptr::null_mut(),
                GetModuleHandleW(std::ptr::null()),
                std::ptr::null(),
            );
            for (i, &(k, m)) in list.iter().enumerate() {
                let Some(key) = by_code(k) else { continue };
                let mut mods = MOD_NOREPEAT;
                for (bit, flag) in [(CTRL, MOD_CONTROL), (ALT, MOD_ALT), (SHIFT, MOD_SHIFT), (META, MOD_WIN)] {
                    if norm(m) & bit != 0 {
                        mods |= flag;
                    }
                }
                if RegisterHotKey(window, i as i32 + 1, mods, u32::from(key.vk)) == 0 {
                    crate::warn(format!("Hotkeys: {} is taken by another program", label(k, m)));
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
            if !window.is_null() {
                DestroyWindow(window);
            }
            // Hotkeys go with the window.
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
        let (add, remove) = hyprland_requests(-4095, CTRL | SHIFT, false).unwrap();
        assert_eq!(
            add,
            "keyword bindnd CTRL SHIFT, F14, Streamerz: Ctrl+Shift+F14, global, streamerz:k-4095m3"
        );
        assert_eq!(remove, "keyword unbind CTRL SHIFT, F14");
        let (add, remove) = hyprland_requests(-4095, CTRL | SHIFT, true).unwrap();
        assert_eq!(
            add,
            "eval hl.bind(\"CTRL + SHIFT + F14\", hl.dsp.global(\"streamerz:k-4095m3\"), \
             {description = \"Streamerz: Ctrl+Shift+F14\", non_consuming = true})"
        );
        assert_eq!(remove, "eval hl.unbind(\"CTRL + SHIFT + F14\")");
        // A comma in the label would split Hyprland's legacy line.
        assert_eq!(hyprland_requests(44, CTRL, false).unwrap().0.split(", ").count(), 5);
        assert_eq!(hyprland_modmask(CTRL | SHIFT << 4), 5);
        assert_eq!(hyprland_modmask(ALT | META), 72);
        // The Mouse Click form catches FLTK's buttons as SharpHook's.
        assert_eq!(
            (1..=5).map(from_fltk_button).collect::<Vec<_>>(),
            [1, 3, 2, 4, 5].map(Some)
        );
        assert_eq!(from_fltk_button(0), None);
        assert_eq!(parse_id(&id(-4095, 3)), Some((-4095, 3)));
        assert_eq!(by_fltk(0xffbe).map(|k| k.code), Some(112));
        assert_eq!(by_fltk('q' as i32).map(|k| k.vk), Some(0x51));
        // Codes are unique.
        let mut codes: Vec<i64> = keys().iter().map(|k| k.code).collect();
        codes.sort_unstable();
        codes.dedup();
        assert_eq!(codes.len(), keys().len());
        // So are the evdev codes, and the ones read from a keyboard match the names.
        let mut ev: Vec<u16> = keys().iter().map(|k| k.evdev).collect();
        ev.sort_unstable();
        ev.dedup();
        assert_eq!(ev.len(), keys().len());
        let evdev = |name: &str| keys().iter().find(|k| k.name == name).map(|k| k.evdev);
        assert_eq!(
            (evdev("Q"), evdev("A"), evdev("Z"), evdev("M")),
            (Some(16), Some(30), Some(44), Some(50))
        );
        assert_eq!(
            (
                evdev("1"),
                evdev("0"),
                evdev("F10"),
                evdev("F11"),
                evdev("F13"),
                evdev("F24")
            ),
            (Some(2), Some(11), Some(68), Some(87), Some(183), Some(194))
        );
    }
}
