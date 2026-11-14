//! Linux input devices (/dev/input/event*): mouse clicks for the Mouse Click trigger on Wayland, and key
//! presses for the Key Press trigger where the desktop gives no other way (no Global Shortcuts portal,
//! not X11). Wayland gives no program the clicks made in other programs' windows; only these devices
//! have them, and by default only root and the `input` group can read them.
//!
//! Streamerz asks once, through polkit (the system's own password dialog), for a udev rule that lets the
//! person at the computer read one kind of device, the way sound cards and webcams are shared: the mice
//! (and touchpads), or the keyboards. It never asks for both at once, and it reads only what a trigger
//! needs: mouse buttons, or the keys of the combinations in use. The rule is one small file in
//! /etc/udev/rules.d, removed by the same button.

use std::cell::RefCell;
use std::collections::HashMap;
use std::ffi::CString;
use std::os::fd::{AsRawFd, FromRawFd};
use std::os::unix::ffi::OsStrExt;
use std::os::unix::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use tokio::io::unix::AsyncFd;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Mice,
    Keyboards,
}

impl Kind {
    pub fn file(self) -> String {
        format!(
            "/etc/udev/rules.d/70-streamerz-{}.rules",
            match self {
                Kind::Mice => "mice",
                Kind::Keyboards => "keyboards",
            }
        )
    }

    /// Whether a device's udev properties (/run/udev/data) make it one of these.
    fn is(self, props: &str) -> bool {
        let has = |k: &str| {
            props
                .lines()
                .any(|l| l.strip_prefix("E:").and_then(|l| l.strip_prefix(k)) == Some("=1"))
        };
        match self {
            Kind::Mice => has("ID_INPUT_MOUSE") || has("ID_INPUT_TOUCHPAD"),
            Kind::Keyboards => has("ID_INPUT_KEYBOARD"),
        }
    }
}

/// The event devices of a kind, from sysfs and udev's database (no device is opened).
pub fn devices(kind: Kind) -> Vec<PathBuf> {
    let Ok(dir) = std::fs::read_dir("/sys/class/input") else {
        return Vec::new();
    };
    let mut v: Vec<PathBuf> = dir
        .flatten()
        .filter_map(|e| {
            let name = e.file_name().into_string().ok()?;
            if !name.starts_with("event") {
                return None;
            }
            let dev = std::fs::read_to_string(e.path().join("dev")).ok()?;
            let props = std::fs::read_to_string(format!("/run/udev/data/c{}", dev.trim())).ok()?;
            kind.is(&props).then(|| Path::new("/dev/input").join(name))
        })
        .collect();
    v.sort();
    v
}

fn readable(p: &Path) -> bool {
    CString::new(p.as_os_str().as_bytes())
        // SAFETY: a NUL-terminated path; access() only checks.
        .is_ok_and(|c| unsafe { libc::access(c.as_ptr(), libc::R_OK) } == 0)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Access {
    /// At least one is readable.
    Ready,
    /// There are some, none readable: the rule is needed.
    Blocked,
    /// None plugged in.
    NoDevice,
}

pub fn access(kind: Kind) -> Access {
    let d = devices(kind);
    if d.is_empty() {
        Access::NoDevice
    } else if d.iter().any(|p| readable(p)) {
        Access::Ready
    } else {
        Access::Blocked
    }
}

pub fn rule_installed(kind: Kind) -> bool {
    Path::new(&kind.file()).exists()
}

/// What a device said, already filtered to what a trigger wants.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Ev {
    /// A mouse button (SharpHook's numbers: 1 left, 2 right, 3 middle, 4 back, 5 forward) and how many
    /// clicks in a row.
    Click(i64, i64),
    /// A key went down (its evdev code) with these modifiers held (SharpHook's left bits).
    Key(u16, i64),
}

const EV_KEY: u16 = 1;
/// The longest gap between the clicks of a double click (Windows' default).
const DOUBLE_CLICK: f64 = 0.5;

/// The held modifiers and the last click, shared by every device.
#[derive(Default)]
pub struct State {
    mods: i64,
    last: (i64, f64, i64),
}

impl State {
    /// One input event: the click or key press it makes, if any.
    pub fn feed(&mut self, mouse: bool, keyboard: bool, ty: u16, code: u16, value: i32, time: f64) -> Option<Ev> {
        use crate::hotkeys::{ALT, CTRL, META, SHIFT};
        if ty != EV_KEY {
            return None;
        }
        if mouse && value == 1 {
            let button = match code {
                0x110 => 1,         // BTN_LEFT
                0x111 => 2,         // BTN_RIGHT
                0x112 => 3,         // BTN_MIDDLE
                0x113 | 0x116 => 4, // BTN_SIDE, BTN_BACK
                0x114 | 0x115 => 5, // BTN_EXTRA, BTN_FORWARD
                _ => 0,
            };
            if button != 0 {
                let n = if self.last.0 == button && time - self.last.1 <= DOUBLE_CLICK {
                    self.last.2 + 1
                } else {
                    1
                };
                self.last = (button, time, n);
                return Some(Ev::Click(button, n));
            }
        }
        if !keyboard {
            return None;
        }
        let bit = match code {
            29 | 97 => CTRL,
            42 | 54 => SHIFT,
            56 | 100 => ALT,
            125 | 126 => META,
            _ => 0,
        };
        if bit != 0 {
            if value == 0 {
                self.mods &= !bit;
            } else {
                self.mods |= bit;
            }
            return None;
        }
        // Keys only (mouse buttons are 0x110 and up); a held key's repeats (2) aren't presses.
        (value == 1 && code < 0x100).then_some(Ev::Key(code, self.mods))
    }
}

/// Read one device until it goes away, sending what it says.
async fn device(path: PathBuf, mouse: bool, keyboard: bool, state: Rc<RefCell<State>>, on: Rc<dyn Fn(Ev)>) {
    let Ok(file) = std::fs::OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NONBLOCK | libc::O_CLOEXEC)
        .open(&path)
    else {
        return;
    };
    // SAFETY: the owned File keeps this descriptor open and unchanged until AsyncFd drops it.
    let Ok(fd) = (unsafe { AsyncFd::register(file) }) else {
        return;
    };
    let size = std::mem::size_of::<libc::input_event>();
    let mut buf = vec![0u8; size * 64];
    loop {
        let Ok(mut guard) = fd.readable().await else { return };
        // SAFETY: reading into our buffer from the device.
        let n = unsafe { libc::read(fd.get_ref().as_raw_fd(), buf.as_mut_ptr().cast(), buf.len()) };
        if n < 0 {
            if std::io::Error::last_os_error().kind() == std::io::ErrorKind::WouldBlock {
                guard.clear_ready();
                continue;
            }
            return;
        }
        if n == 0 {
            return;
        }
        for chunk in buf[..n as usize].chunks_exact(size) {
            // SAFETY: the kernel writes whole input_events; read_unaligned copes with the byte buffer.
            let e: libc::input_event = unsafe { std::ptr::read_unaligned(chunk.as_ptr().cast()) };
            let time = e.time.tv_sec as f64 + e.time.tv_usec as f64 / 1e6;
            let got = state.borrow_mut().feed(mouse, keyboard, e.type_, e.code, e.value, time);
            if let Some(ev) = got {
                on(ev);
            }
        }
    }
}

/// Read every readable device of the kinds wanted, and those plugged in (or allowed) later, until dropped.
pub async fn run(mouse: bool, keyboard: bool, on: Rc<dyn Fn(Ev)>) -> std::io::Result<()> {
    // SAFETY: inotify_init1 returns a new fd or -1.
    let raw = unsafe { libc::inotify_init1(libc::IN_NONBLOCK | libc::IN_CLOEXEC) };
    if raw < 0 {
        return Err(std::io::Error::last_os_error());
    }
    // SAFETY: raw is a fresh fd we own.
    let owned = unsafe { std::os::fd::OwnedFd::from_raw_fd(raw) };
    // SAFETY: OwnedFd keeps the same descriptor open throughout AsyncFd's lifetime.
    let ino = unsafe { AsyncFd::register(owned) }?;
    // New devices, and permission given to existing ones (the ACL comes as an attribute change).
    // SAFETY: a valid inotify fd and a NUL-terminated path.
    unsafe { libc::inotify_add_watch(raw, c"/dev/input".as_ptr(), libc::IN_CREATE | libc::IN_ATTRIB) };
    let state = Rc::new(RefCell::new(State::default()));
    let mut open: HashMap<PathBuf, crate::emote_events::Abort> = HashMap::new();
    let mut buf = [0u8; 4096];
    loop {
        open.retain(|_, t| !t.0.is_finished());
        let mut wanted: Vec<(PathBuf, bool, bool)> = Vec::new();
        for (on_kind, kind) in [(mouse, Kind::Mice), (keyboard, Kind::Keyboards)] {
            if !on_kind {
                continue;
            }
            for p in devices(kind) {
                let (m, k) = (kind == Kind::Mice, kind == Kind::Keyboards);
                match wanted.iter_mut().find(|w| w.0 == p) {
                    Some(w) => (w.1, w.2) = (w.1 || m, w.2 || k),
                    None => wanted.push((p, m, k)),
                }
            }
        }
        for (p, m, k) in wanted {
            if !open.contains_key(&p) && readable(&p) {
                let task = tokio::task::spawn_local(device(p.clone(), m, k, state.clone(), on.clone()));
                open.insert(p, crate::emote_events::Abort(task));
            }
        }
        let mut guard = ino.readable().await?;
        // SAFETY: reading into our buffer from the inotify fd; what it says doesn't matter, only that
        // something changed.
        let n = unsafe { libc::read(raw, buf.as_mut_ptr().cast(), buf.len()) };
        if n < 0 {
            guard.clear_ready();
            continue;
        }
        // udev fills its database right after the node appears.
        tokio::time::sleep(std::time::Duration::from_millis(300)).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hotkeys::{CTRL, SHIFT};

    #[test]
    fn devices_are_told_apart_by_udev_properties() {
        let mouse = "I:123\nE:ID_INPUT=1\nE:ID_INPUT_MOUSE=1\nG:seat\n";
        let combo = "E:ID_INPUT=1\nE:ID_INPUT_MOUSE=1\nE:ID_INPUT_KEY=1\nE:ID_INPUT_KEYBOARD=1\n";
        let touchpad = "E:ID_INPUT_TOUCHPAD=1\n";
        let power = "E:ID_INPUT=1\nE:ID_INPUT_KEY=1\n";
        assert!(Kind::Mice.is(mouse) && Kind::Mice.is(combo) && Kind::Mice.is(touchpad));
        assert!(!Kind::Mice.is(power) && !Kind::Keyboards.is(power) && !Kind::Keyboards.is(mouse));
        assert!(Kind::Keyboards.is(combo));
        // Not fooled by a longer name.
        assert!(!Kind::Mice.is("E:ID_INPUT_MOUSE_EXTRA=1\n"));
    }

    #[test]
    fn clicks_and_double_clicks_are_counted() {
        let mut s = State::default();
        assert_eq!(s.feed(true, false, 1, 0x110, 1, 10.0), Some(Ev::Click(1, 1)));
        assert_eq!(s.feed(true, false, 1, 0x110, 0, 10.05), None);
        assert_eq!(s.feed(true, false, 1, 0x110, 1, 10.2), Some(Ev::Click(1, 2)));
        assert_eq!(s.feed(true, false, 1, 0x110, 1, 11.0), Some(Ev::Click(1, 1)));
        assert_eq!(s.feed(true, false, 1, 0x111, 1, 11.1), Some(Ev::Click(2, 1)));
        assert_eq!(s.feed(true, false, 1, 0x112, 1, 12.0), Some(Ev::Click(3, 1)));
        assert_eq!(s.feed(true, false, 1, 0x113, 1, 13.0), Some(Ev::Click(4, 1)));
        assert_eq!(s.feed(true, false, 1, 0x115, 1, 14.0), Some(Ev::Click(5, 1)));
        // Movement and wheel aren't clicks.
        assert_eq!(s.feed(true, false, 2, 0, 5, 15.0), None);
        // A mouse that also says it's a keyboard: its keys aren't read for clicks.
        assert_eq!(s.feed(true, false, 1, 30, 1, 16.0), None);
    }

    #[test]
    fn keys_come_with_the_modifiers_held() {
        let mut s = State::default();
        assert_eq!(s.feed(false, true, 1, 29, 1, 0.0), None);
        assert_eq!(s.feed(false, true, 1, 54, 1, 0.0), None);
        assert_eq!(s.feed(false, true, 1, 16, 1, 0.0), Some(Ev::Key(16, CTRL | SHIFT)));
        // Held: repeats aren't presses.
        assert_eq!(s.feed(false, true, 1, 16, 2, 0.0), None);
        assert_eq!(s.feed(false, true, 1, 16, 0, 0.0), None);
        assert_eq!(s.feed(false, true, 1, 54, 0, 0.0), None);
        assert_eq!(s.feed(false, true, 1, 16, 1, 0.0), Some(Ev::Key(16, CTRL)));
        // Reading only mice: no keys.
        assert_eq!(s.feed(true, false, 1, 16, 1, 0.0), None);
        // Mouse buttons aren't keys.
        assert_eq!(s.feed(false, true, 1, 0x110, 1, 0.0), None);
    }

    /// The reader on a stand-in for a device: a FIFO fed input_events.
    #[test]
    fn reads_events_from_a_device() {
        let dir = std::env::temp_dir().join(format!("szdev-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let fifo = dir.join("event0");
        let _ = std::fs::remove_file(&fifo);
        let c = CString::new(fifo.as_os_str().as_bytes()).unwrap();
        // SAFETY: a NUL-terminated path.
        assert_eq!(unsafe { libc::mkfifo(c.as_ptr(), 0o600) }, 0);
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        let got = Rc::new(RefCell::new(Vec::new()));
        tokio::task::LocalSet::new().block_on(&rt, async {
            let sink = got.clone();
            let on: Rc<dyn Fn(Ev)> = Rc::new(move |e| sink.borrow_mut().push(e));
            let reader = tokio::task::spawn_local(device(fifo.clone(), true, false, Rc::default(), on));
            // Read and write: opening only the writing end would wait for the reader, which hasn't run yet.
            let mut w = std::fs::OpenOptions::new().read(true).write(true).open(&fifo).unwrap();
            let ev = |sec: i64, code: u16, value: i32| {
                let e = libc::input_event {
                    time: libc::timeval {
                        tv_sec: sec,
                        tv_usec: 0,
                    },
                    type_: EV_KEY,
                    code,
                    value,
                };
                // SAFETY: plain old data, viewed as its bytes.
                unsafe { std::slice::from_raw_parts((&e as *const libc::input_event).cast::<u8>(), size_of_val(&e)) }
                    .to_vec()
            };
            use std::io::Write;
            w.write_all(&[ev(1, 0x111, 1), ev(1, 0x111, 0), ev(5, 0x110, 1)].concat())
                .unwrap();
            // Kept open until the reader has opened its end (the data goes with the last opener).
            tokio::time::sleep(std::time::Duration::from_millis(300)).await;
            drop(w);
            let _ = tokio::time::timeout(std::time::Duration::from_secs(5), reader).await;
        });
        assert_eq!(*got.borrow(), [Ev::Click(2, 1), Ev::Click(1, 1)]);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
