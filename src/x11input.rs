//! X11 sessions: key combinations grabbed on the root window (XGrabKey) and mouse clicks from XInput2's
//! raw events, both given by the X server to any program: no permission and no setup. libX11 is already
//! loaded (FLTK draws with it); libXi is opened only when a Mouse Click trigger is used.

use std::ffi::{c_char, c_int, c_long, c_uint, c_ulong, c_void, CString};
use std::os::fd::{AsRawFd, RawFd};
use std::sync::atomic::{AtomicU8, Ordering};

type Display = c_void;
type Window = c_ulong;

#[repr(C)]
#[derive(Clone, Copy)]
struct XKeyEvent {
    type_: c_int,
    serial: c_ulong,
    send_event: c_int,
    display: *mut Display,
    window: Window,
    root: Window,
    subwindow: Window,
    time: c_ulong,
    x: c_int,
    y: c_int,
    x_root: c_int,
    y_root: c_int,
    state: c_uint,
    keycode: c_uint,
    same_screen: c_int,
}

#[repr(C)]
#[derive(Clone, Copy)]
struct XGenericEventCookie {
    type_: c_int,
    serial: c_ulong,
    send_event: c_int,
    display: *mut Display,
    extension: c_int,
    evtype: c_int,
    cookie: c_uint,
    data: *mut c_void,
}

/// The start of XIRawEvent (the rest isn't read).
#[repr(C)]
struct XIRawEvent {
    type_: c_int,
    serial: c_ulong,
    send_event: c_int,
    display: *mut Display,
    extension: c_int,
    evtype: c_int,
    time: c_ulong,
    deviceid: c_int,
    sourceid: c_int,
    detail: c_int,
}

#[repr(C)]
union XEvent {
    type_: c_int,
    key: XKeyEvent,
    cookie: XGenericEventCookie,
    pad: [c_long; 24],
}

#[repr(C)]
struct XErrorEvent {
    type_: c_int,
    display: *mut Display,
    resourceid: c_ulong,
    serial: c_ulong,
    error_code: u8,
}

#[repr(C)]
struct XIEventMask {
    deviceid: c_int,
    mask_len: c_int,
    mask: *mut u8,
}

type ErrorHandler = unsafe extern "C" fn(*mut Display, *mut XErrorEvent) -> c_int;

#[link(name = "X11")]
extern "C" {
    fn XOpenDisplay(name: *const c_char) -> *mut Display;
    fn XCloseDisplay(d: *mut Display) -> c_int;
    fn XDefaultRootWindow(d: *mut Display) -> Window;
    fn XConnectionNumber(d: *mut Display) -> c_int;
    fn XPending(d: *mut Display) -> c_int;
    fn XNextEvent(d: *mut Display, ev: *mut XEvent) -> c_int;
    fn XGetEventData(d: *mut Display, cookie: *mut XGenericEventCookie) -> c_int;
    fn XFreeEventData(d: *mut Display, cookie: *mut XGenericEventCookie);
    fn XQueryExtension(d: *mut Display, name: *const c_char, op: *mut c_int, ev: *mut c_int, err: *mut c_int) -> c_int;
    #[allow(clippy::too_many_arguments)]
    fn XQueryPointer(
        d: *mut Display,
        w: Window,
        root: *mut Window,
        child: *mut Window,
        rx: *mut c_int,
        ry: *mut c_int,
        wx: *mut c_int,
        wy: *mut c_int,
        mask: *mut c_uint,
    ) -> c_int;
    fn XGrabKey(d: *mut Display, code: c_int, mods: c_uint, w: Window, owner: c_int, pm: c_int, km: c_int) -> c_int;
    fn XStringToKeysym(s: *const c_char) -> c_ulong;
    fn XKeysymToKeycode(d: *mut Display, sym: c_ulong) -> u8;
    fn XSync(d: *mut Display, discard: c_int) -> c_int;
    fn XFlush(d: *mut Display) -> c_int;
    fn XSetErrorHandler(h: Option<ErrorHandler>) -> Option<ErrorHandler>;
}

const KEY_PRESS: c_int = 2;
const GENERIC_EVENT: c_int = 35;
const XI_RAW_BUTTON_PRESS: c_int = 15;
const XI_ALL_MASTER_DEVICES: c_int = 1;
const SHIFT_MASK: c_uint = 1;
const LOCK_MASK: c_uint = 2;
const CONTROL_MASK: c_uint = 4;
const MOD1_MASK: c_uint = 8;
const MOD2_MASK: c_uint = 16;
const MOD4_MASK: c_uint = 64;
const BAD_ACCESS: u8 = 10;

/// The last X error on any display (Xlib has one handler for the process; the default one exits).
static LAST_ERROR: AtomicU8 = AtomicU8::new(0);

unsafe extern "C" fn on_error(_: *mut Display, e: *mut XErrorEvent) -> c_int {
    LAST_ERROR.store((*e).error_code, Ordering::Relaxed);
    0
}

/// Whether this is an X11 session (not Wayland, where X11 programs see only each other).
pub fn session() -> bool {
    match std::env::var("XDG_SESSION_TYPE").as_deref() {
        Ok("x11") => true,
        Ok("wayland") => false,
        _ => std::env::var_os("WAYLAND_DISPLAY").is_none() && std::env::var_os("DISPLAY").is_some(),
    }
}

/// SharpHook's modifier bits as X's, and back (NumLock and CapsLock ignored).
fn x_mods(mask: i64) -> c_uint {
    use crate::hotkeys::{norm, ALT, CTRL, META, SHIFT};
    let m = norm(mask);
    [
        (SHIFT, SHIFT_MASK),
        (CTRL, CONTROL_MASK),
        (ALT, MOD1_MASK),
        (META, MOD4_MASK),
    ]
    .iter()
    .filter(|(b, _)| m & b != 0)
    .fold(0, |acc, (_, x)| acc | x)
}

/// X's mouse buttons as SharpHook's (1 left, 2 right, 3 middle, 4 back, 5 forward); 4 to 7 are the wheel.
fn button(detail: c_int) -> i64 {
    match detail {
        1 => 1,
        2 => 3,
        3 => 2,
        8 => 4,
        9 => 5,
        _ => 0,
    }
}

struct Conn(*mut Display);

impl Drop for Conn {
    fn drop(&mut self) {
        // SAFETY: our own display; closing it also drops its grabs.
        unsafe { XCloseDisplay(self.0) };
    }
}

impl AsRawFd for Conn {
    fn as_raw_fd(&self) -> RawFd {
        // SAFETY: an open display.
        unsafe { XConnectionNumber(self.0) }
    }
}

/// XISelectEvents from libXi, opened at run time (it isn't linked).
fn xi_select(d: *mut Display, root: Window) -> Result<(), String> {
    type Select = unsafe extern "C" fn(*mut Display, Window, *mut XIEventMask, c_int) -> c_int;
    // SAFETY: dlopen/dlsym with NUL-terminated names; the symbol has XISelectEvents' C signature.
    unsafe {
        let lib = libc::dlopen(c"libXi.so.6".as_ptr(), libc::RTLD_NOW | libc::RTLD_LOCAL);
        if lib.is_null() {
            return Err("libXi isn't installed".into());
        }
        let f = libc::dlsym(lib, c"XISelectEvents".as_ptr());
        if f.is_null() {
            return Err("libXi has no XISelectEvents".into());
        }
        let select: Select = std::mem::transmute::<*mut c_void, Select>(f);
        let mut bits = [0u8; 2];
        bits[(XI_RAW_BUTTON_PRESS / 8) as usize] |= 1 << (XI_RAW_BUTTON_PRESS % 8);
        let mut mask = XIEventMask {
            deviceid: XI_ALL_MASTER_DEVICES,
            mask_len: bits.len() as c_int,
            mask: bits.as_mut_ptr(),
        };
        select(d, root, &mut mask, 1);
    }
    Ok(())
}

/// Listen for the combinations and, when `mouse`, the clicks, until dropped.
pub async fn run(
    combos: &[(i64, i64)],
    mouse: bool,
    on_key: impl Fn(i64, i64),
    on_click: impl Fn(i64, i64, i64, i64),
) -> Result<(), String> {
    // SAFETY: opens a connection of our own to $DISPLAY.
    let d = unsafe { XOpenDisplay(std::ptr::null()) };
    if d.is_null() {
        return Err("can't open the X display".into());
    }
    let conn = Conn(d);
    // SAFETY: every call below is on our open display.
    let root = unsafe { XDefaultRootWindow(d) };
    unsafe { XSetErrorHandler(Some(on_error)) };
    // (X keycode, X modifiers) for each combination.
    let mut grabs: Vec<(u8, c_uint, i64, i64)> = Vec::new();
    for &(code, mask) in combos {
        let Some(key) = crate::hotkeys::by_code(code) else {
            continue;
        };
        let Ok(sym) = CString::new(key.keysym.as_str()) else {
            continue;
        };
        let kc = unsafe { XKeysymToKeycode(d, XStringToKeysym(sym.as_ptr())) };
        if kc == 0 {
            crate::warn(format!("Hotkeys: this keyboard has no {}", key.name));
            continue;
        }
        let mods = x_mods(mask);
        LAST_ERROR.store(0, Ordering::Relaxed);
        // The combination with CapsLock and NumLock on or off.
        for extra in [0, LOCK_MASK, MOD2_MASK, LOCK_MASK | MOD2_MASK] {
            unsafe { XGrabKey(d, c_int::from(kc), mods | extra, root, 0, 1, 1) };
        }
        unsafe { XSync(d, 0) };
        if LAST_ERROR.load(Ordering::Relaxed) == BAD_ACCESS {
            crate::warn(format!(
                "Hotkeys: {} is taken by another program",
                crate::hotkeys::label(code, mask)
            ));
            continue;
        }
        grabs.push((kc, mods, code, mask));
    }
    let mut xi = -1;
    if mouse {
        let (mut ev, mut err) = (0, 0);
        if unsafe { XQueryExtension(d, c"XInputExtension".as_ptr(), &mut xi, &mut ev, &mut err) } == 0 {
            return Err("the X server has no XInput extension".into());
        }
        xi_select(d, root)?;
        unsafe { XSync(d, 0) };
    }
    if !grabs.is_empty() {
        crate::say(format!("Hotkeys: {} shortcut(s) taken from X11", grabs.len()));
    }
    if mouse {
        crate::say("Mouse Click: listening to X11's clicks".into());
    }
    let fd = tokio::io::unix::AsyncFd::new(conn).map_err(|e| e.to_string())?;
    let mut last = (0i64, 0u64, 0i64);
    loop {
        // SAFETY: as above; an event is read only after XPending says one is there.
        while unsafe { XPending(d) } > 0 {
            let mut ev: XEvent = unsafe { std::mem::zeroed() };
            unsafe { XNextEvent(d, &mut ev) };
            match unsafe { ev.type_ } {
                KEY_PRESS => {
                    let k = unsafe { ev.key };
                    let mods = k.state & (SHIFT_MASK | CONTROL_MASK | MOD1_MASK | MOD4_MASK);
                    if let Some(g) = grabs.iter().find(|g| c_uint::from(g.0) == k.keycode && g.1 == mods) {
                        on_key(g.2, g.3);
                    }
                }
                GENERIC_EVENT if unsafe { ev.cookie.extension } == xi => {
                    let mut c = unsafe { ev.cookie };
                    if unsafe { XGetEventData(d, &mut c) } != 0 {
                        if c.evtype == XI_RAW_BUTTON_PRESS && !c.data.is_null() {
                            let raw = unsafe { &*(c.data as *const XIRawEvent) };
                            let b = button(raw.detail);
                            let time = u64::from(raw.time as u32);
                            if b != 0 {
                                let n = if last.0 == b && time.wrapping_sub(last.1) <= 500 {
                                    last.2 + 1
                                } else {
                                    1
                                };
                                last = (b, time, n);
                                let (mut r, mut ch, mut x, mut y, mut wx, mut wy, mut m) = (0, 0, 0, 0, 0, 0, 0);
                                unsafe {
                                    XQueryPointer(d, root, &mut r, &mut ch, &mut x, &mut y, &mut wx, &mut wy, &mut m)
                                };
                                on_click(b, n, i64::from(x), i64::from(y));
                            }
                        }
                        unsafe { XFreeEventData(d, &mut c) };
                    }
                }
                _ => {}
            }
        }
        unsafe { XFlush(d) };
        let mut guard = fd.readable().await.map_err(|e| e.to_string())?;
        guard.clear_ready();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hotkeys::{ALT, CTRL, META, SHIFT};

    #[test]
    fn modifiers_and_buttons_map_to_x11() {
        assert_eq!(x_mods(CTRL | SHIFT), CONTROL_MASK | SHIFT_MASK);
        assert_eq!(x_mods(ALT | META), MOD1_MASK | MOD4_MASK);
        // Right-hand modifiers count as left ones.
        assert_eq!(x_mods(CTRL << 4), CONTROL_MASK);
        assert_eq!((button(1), button(2), button(3), button(8), button(9)), (1, 3, 2, 4, 5));
        // The wheel isn't a click.
        assert!((4..=7).all(|b| button(b) == 0));
    }
}
