//! MIDI in and out, with what each system has: the kernel's raw MIDI devices on Linux
//! (`/dev/snd/midiC*D*`, read through tokio with no thread), winmm on Windows. Inputs are open only
//! while an action uses MIDI Message (`engine/midi_in.rs`); an output opens when a sub-action sends.
//! Names follow Streamer.bot, which uses NAudio's (`NoteOn`, `ControlChange`, `MainVolume`, `C4`...).

use crate::model::Obj;

/// One message from a device: status (command and channel) and its two data bytes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Msg {
    pub status: u8,
    pub data1: u8,
    pub data2: u8,
}

/// Turns the bytes a device sends into messages (running status kept, SysEx and real-time skipped).
#[derive(Default)]
pub struct Parser {
    running: u8,
    data: Vec<u8>,
    sysex: bool,
}

/// How many data bytes a status takes.
fn data_len(status: u8) -> usize {
    match status & 0xF0 {
        0xC0 | 0xD0 => 1,
        0x80..=0xE0 => 2,
        _ => match status {
            0xF1 | 0xF3 => 1,
            0xF2 => 2,
            _ => 0,
        },
    }
}

impl Parser {
    #[cfg_attr(windows, allow(dead_code))]
    pub fn feed(&mut self, b: u8) -> Option<Msg> {
        if b >= 0xF8 {
            return None; // real-time (clock...): ignored
        }
        if b == 0xF0 {
            self.sysex = true;
            return None;
        }
        if b == 0xF7 {
            self.sysex = false;
            return None;
        }
        if self.sysex {
            return None;
        }
        if b & 0x80 != 0 {
            self.running = b;
            self.data.clear();
            return (data_len(b) == 0).then_some(Msg {
                status: b,
                data1: 0,
                data2: 0,
            });
        }
        if self.running == 0 {
            return None;
        }
        self.data.push(b);
        if self.data.len() < data_len(self.running) {
            return None;
        }
        let m = Msg {
            status: self.running,
            data1: self.data[0],
            data2: self.data.get(1).copied().unwrap_or(0),
        };
        self.data.clear();
        Some(m)
    }
}

/// The commands a trigger can pick (its `commandCode` is the place; 0: any).
pub const COMMANDS: &[&str] = &[
    "",
    "NoteOn",
    "NoteOff",
    "ControlChange",
    "PatchChange",
    "PitchWheelChange",
    "KeyAfterTouch",
    "ChannelAfterTouch",
];

/// NAudio's MidiCommandCode names.
pub fn command_name(status: u8) -> &'static str {
    match status & 0xF0 {
        0x80 => "NoteOff",
        0x90 => "NoteOn",
        0xA0 => "KeyAfterTouch",
        0xB0 => "ControlChange",
        0xC0 => "PatchChange",
        0xD0 => "ChannelAfterTouch",
        0xE0 => "PitchWheelChange",
        _ => match status {
            0xF0 => "Sysex",
            0xF1 => "TimingClock",
            0xFA => "StartSequence",
            0xFB => "ContinueSequence",
            0xFC => "StopSequence",
            0xFE => "AutoSensing",
            _ => "Eox",
        },
    }
}

/// The status byte for a command name (`NoteOn`...) and channel 1-16, for Generic Event.
pub fn command_code(name: &str) -> Option<u8> {
    let n = name.trim();
    if let Ok(v) = n.parse::<u8>() {
        // A place in COMMANDS (what the trigger's form stores), or the status byte itself.
        return match COMMANDS.get(v as usize) {
            Some(c) if v < 0x80 => command_code(c),
            _ => Some(v),
        };
    }
    (0x80u8..=0xE0)
        .step_by(0x10)
        .find(|s| command_name(*s).eq_ignore_ascii_case(n))
        .or_else(|| n.eq_ignore_ascii_case("ProgramChange").then_some(0xC0))
}

/// NAudio's MidiController names, else the number.
pub fn controller_name(n: u8) -> String {
    match n {
        0 => "BankSelect",
        1 => "Modulation",
        2 => "BreathController",
        4 => "FootController",
        7 => "MainVolume",
        10 => "Pan",
        11 => "Expression",
        32 => "BankSelectLsb",
        64 => "Sustain",
        65 => "Portamento",
        66 => "Sostenuto",
        67 => "SoftPedal",
        68 => "LegatoFootswitch",
        121 => "ResetAllControllers",
        123 => "AllNotesOff",
        _ => return n.to_string(),
    }
    .into()
}

/// NAudio's note name: octave is the number over 12 (60 is C5).
pub fn note_name(n: u8) -> String {
    const NAMES: [&str; 12] = ["C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B"];
    format!("{}{}", NAMES[(n % 12) as usize], n / 12)
}

/// MIDI Message's arguments (Streamer.bot's names), for a message from `device` at `ms` ms.
pub fn args(name: &str, device: &str, m: &Msg, ms: u64) -> Obj {
    let mut a = Obj::new();
    let put = |a: &mut Obj, k: &str, v: serde_json::Value| drop(a.insert(k.into(), v));
    put(&mut a, "name", name.into());
    put(&mut a, "deviceName", device.into());
    put(&mut a, "timestamp", ms.into());
    put(&mut a, "commandCode", command_name(m.status).into());
    put(&mut a, "commandCodeNum", (m.status & 0xF0).into());
    if m.status < 0xF0 {
        put(&mut a, "channel", ((m.status & 0x0F) as u64 + 1).into());
    }
    let raw = u32::from(m.status) | u32::from(m.data1) << 8 | u32::from(m.data2) << 16;
    put(&mut a, "rawMessage", format!("{raw:08X}").into());
    let norm = |v: u8| (f64::from(v) / 127.0).into();
    match m.status & 0xF0 {
        0x80 | 0x90 | 0xA0 => {
            put(&mut a, "noteName", note_name(m.data1).into());
            put(&mut a, "noteNumber", m.data1.into());
            put(&mut a, "velocity", m.data2.into());
            put(&mut a, "velocityNormal", norm(m.data2));
        }
        0xB0 => {
            put(&mut a, "controllerName", controller_name(m.data1).into());
            put(&mut a, "controllerNumber", m.data1.into());
            put(&mut a, "value", m.data2.into());
            put(&mut a, "valueNormal", norm(m.data2));
        }
        0xC0 | 0xD0 => {
            put(&mut a, "value", m.data1.into());
            put(&mut a, "valueNormal", norm(m.data1));
        }
        0xE0 => {
            let v = u16::from(m.data1) | u16::from(m.data2) << 7;
            put(&mut a, "value", v.into());
            put(&mut a, "valueNormal", (f64::from(v) / 16383.0).into());
        }
        _ => {}
    }
    a
}

/// A device: its name and how to open it.
#[derive(Clone, Debug, PartialEq)]
pub struct Device {
    pub name: String,
    pub id: String,
}

pub use imp::{inputs, outputs, read, send};

#[cfg(target_os = "linux")]
mod imp {
    use super::{Device, Msg, Parser};
    use std::os::fd::AsRawFd;
    use std::os::unix::fs::OpenOptionsExt;
    use tokio::io::unix::AsyncFd;
    use tokio::sync::mpsc::UnboundedSender;

    /// Where the raw MIDI devices are (a test points it elsewhere).
    fn dir() -> std::path::PathBuf {
        std::env::var_os("STREAMERZ_MIDI_DIR").map_or_else(|| "/dev/snd".into(), Into::into)
    }

    /// The raw MIDI devices, named after their sound card ("Launchpad", "Launchpad 2" for a second port).
    fn devices() -> Vec<Device> {
        let mut out = Vec::new();
        for e in std::fs::read_dir(dir()).into_iter().flatten().flatten() {
            let f = e.file_name().to_string_lossy().into_owned();
            let Some(rest) = f.strip_prefix("midiC") else { continue };
            let Some((card, dev)) = rest.split_once('D') else {
                continue;
            };
            let id = std::fs::read_to_string(format!("/proc/asound/card{card}/id"))
                .map(|s| s.trim().to_string())
                .unwrap_or_else(|_| format!("MIDI {card}"));
            let name = if dev == "0" {
                id
            } else {
                format!("{id} {}", dev.parse::<u32>().unwrap_or(0) + 1)
            };
            out.push(Device {
                name,
                id: e.path().to_string_lossy().into_owned(),
            });
        }
        out.sort_by(|a, b| a.id.cmp(&b.id));
        out
    }

    pub fn inputs() -> Vec<Device> {
        devices()
    }

    pub fn outputs() -> Vec<Device> {
        devices()
    }

    /// Reads `d` until it fails or is dropped, sending each message.
    pub async fn read(d: Device, tx: UnboundedSender<Msg>) -> std::io::Result<()> {
        let f = std::fs::OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_NONBLOCK)
            .open(&d.id)?;
        // SAFETY: the owned File keeps this descriptor open and unchanged until AsyncFd drops it.
        let fd = unsafe { AsyncFd::register(f) }?;
        let mut p = Parser::default();
        let mut buf = [0u8; 256];
        loop {
            let mut g = fd.readable().await?;
            // SAFETY: reading into our buffer from our open file.
            let n = unsafe { libc::read(fd.get_ref().as_raw_fd(), buf.as_mut_ptr().cast(), buf.len()) };
            if n < 0 {
                let e = std::io::Error::last_os_error();
                if e.kind() == std::io::ErrorKind::WouldBlock {
                    g.clear_ready();
                    continue;
                }
                return Err(e);
            }
            if n == 0 {
                // A FIFO with no writer (tests): wait for the next one.
                g.clear_ready();
                tokio::task::yield_now().await;
                continue;
            }
            for b in &buf[..n as usize] {
                if let Some(m) = p.feed(*b) {
                    if tx.send(m).is_err() {
                        return Ok(());
                    }
                }
            }
        }
    }

    /// Sends `bytes` to the output named `name` (any case; empty: the first one).
    pub fn send(name: &str, bytes: &[u8]) -> Result<(), String> {
        use std::io::Write;
        let all = outputs();
        let d = all
            .iter()
            .find(|d| name.is_empty() || d.name.eq_ignore_ascii_case(name.trim()))
            .ok_or_else(|| format!("no MIDI output {name}"))?;
        std::fs::OpenOptions::new()
            .write(true)
            .open(&d.id)
            .and_then(|mut f| f.write_all(bytes))
            .map_err(|e| format!("{}: {e}", d.name))
    }
}

#[cfg(windows)]
mod imp {
    use super::{Device, Msg};
    use std::sync::Mutex;
    use tokio::sync::mpsc::UnboundedSender;
    use windows_sys::Win32::Media::Audio::{
        midiInClose, midiInGetDevCapsW, midiInGetNumDevs, midiInOpen, midiInStart, midiInStop, midiOutClose,
        midiOutGetDevCapsW, midiOutGetNumDevs, midiOutOpen, midiOutShortMsg, CALLBACK_FUNCTION, HMIDIIN, HMIDIOUT,
        MIDIINCAPSW, MIDIOUTCAPSW,
    };
    use windows_sys::Win32::Media::MM_MIM_DATA;

    fn wide(s: &[u16]) -> String {
        String::from_utf16_lossy(&s[..s.iter().position(|c| *c == 0).unwrap_or(s.len())])
    }

    pub fn inputs() -> Vec<Device> {
        // SAFETY: plain queries into our own structs.
        (0..unsafe { midiInGetNumDevs() })
            .filter_map(|i| unsafe {
                let mut c: MIDIINCAPSW = std::mem::zeroed();
                (midiInGetDevCapsW(i as usize, &mut c, std::mem::size_of::<MIDIINCAPSW>() as u32) == 0).then(|| {
                    Device {
                        name: wide(&{ c.szPname }),
                        id: i.to_string(),
                    }
                })
            })
            .collect()
    }

    pub fn outputs() -> Vec<Device> {
        // SAFETY: plain queries into our own structs.
        (0..unsafe { midiOutGetNumDevs() })
            .filter_map(|i| unsafe {
                let mut c: MIDIOUTCAPSW = std::mem::zeroed();
                (midiOutGetDevCapsW(i as usize, &mut c, std::mem::size_of::<MIDIOUTCAPSW>() as u32) == 0).then(|| {
                    Device {
                        name: wide(&{ c.szPname }),
                        id: i.to_string(),
                    }
                })
            })
            .collect()
    }

    /// Where winmm's callback (on its own thread) sends each device's messages, by its index.
    static SINKS: Mutex<Vec<(usize, UnboundedSender<Msg>)>> = Mutex::new(Vec::new());

    unsafe extern "system" fn on_midi(_: HMIDIIN, msg: u32, instance: usize, p1: usize, _: usize) {
        if msg != MM_MIM_DATA {
            return;
        }
        let m = Msg {
            status: p1 as u8,
            data1: (p1 >> 8) as u8 & 0x7F,
            data2: (p1 >> 16) as u8 & 0x7F,
        };
        if let Ok(s) = SINKS.lock() {
            for (_, tx) in s.iter().filter(|(i, _)| *i == instance) {
                let _ = tx.send(m);
            }
        }
    }

    /// Closes the device when the read is dropped.
    struct Open(HMIDIIN, usize);

    impl Drop for Open {
        fn drop(&mut self) {
            // SAFETY: our open handle, closed once.
            unsafe {
                midiInStop(self.0);
                midiInClose(self.0);
            }
            if let Ok(mut s) = SINKS.lock() {
                s.retain(|(i, _)| *i != self.1);
            }
        }
    }

    pub async fn read(d: Device, tx: UnboundedSender<Msg>) -> std::io::Result<()> {
        let idx: usize = d.id.parse().unwrap_or(0);
        if let Ok(mut s) = SINKS.lock() {
            s.push((idx, tx));
        }
        let mut h: HMIDIIN = std::ptr::null_mut();
        // SAFETY: opening a device with our callback; closed by `Open`.
        let r = unsafe {
            midiInOpen(
                &mut h,
                idx as u32,
                on_midi as *const () as usize,
                idx,
                CALLBACK_FUNCTION,
            )
        };
        if r != 0 {
            if let Ok(mut s) = SINKS.lock() {
                s.retain(|(i, _)| *i != idx);
            }
            return Err(std::io::Error::other(format!("{}: can't open ({r})", d.name)));
        }
        let _open = Open(h, idx);
        // SAFETY: the handle we just opened.
        unsafe { midiInStart(h) };
        std::future::pending::<()>().await;
        Ok(())
    }

    pub fn send(name: &str, bytes: &[u8]) -> Result<(), String> {
        let all = outputs();
        let d = all
            .iter()
            .find(|d| name.is_empty() || d.name.eq_ignore_ascii_case(name.trim()))
            .ok_or_else(|| format!("no MIDI output {name}"))?;
        let mut h: HMIDIOUT = std::ptr::null_mut();
        // SAFETY: open, short messages, close: all on our handle.
        unsafe {
            if midiOutOpen(&mut h, d.id.parse().unwrap_or(0), 0, 0, 0) != 0 {
                return Err(format!("{}: can't open", d.name));
            }
            for m in bytes.chunks(3) {
                let packed = m
                    .iter()
                    .enumerate()
                    .fold(0u32, |a, (i, b)| a | u32::from(*b) << (8 * i));
                midiOutShortMsg(h, packed);
            }
            midiOutClose(h);
        }
        Ok(())
    }
}

#[cfg(not(any(target_os = "linux", windows)))]
mod imp {
    use super::{Device, Msg};
    pub fn inputs() -> Vec<Device> {
        Vec::new()
    }
    pub fn outputs() -> Vec<Device> {
        Vec::new()
    }
    pub async fn read(_: Device, _: tokio::sync::mpsc::UnboundedSender<Msg>) -> std::io::Result<()> {
        Err(std::io::Error::new(std::io::ErrorKind::Unsupported, "MIDI"))
    }
    pub fn send(_: &str, _: &[u8]) -> Result<(), String> {
        Err("MIDI is not supported here".into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bytes_become_messages_with_running_status() {
        let mut p = Parser::default();
        let bytes = [0x90, 60, 100, 62, 90, 0xF8, 0xB0, 7, 64, 0xF0, 1, 2, 0xF7, 0xC1, 5];
        let got: Vec<Msg> = bytes.iter().filter_map(|b| p.feed(*b)).collect();
        let m = |s, a, b| Msg {
            status: s,
            data1: a,
            data2: b,
        };
        assert_eq!(got, [m(0x90, 60, 100), m(0x90, 62, 90), m(0xB0, 7, 64), m(0xC1, 5, 0)]);
    }

    /// Streamer.bot's own example: note 48 is C4, velocity 80 is 0.6299..., raw 00503090.
    #[test]
    fn arguments_have_naudio_names() {
        let a = args(
            "My MPK",
            "MPKmini2",
            &Msg {
                status: 0x90 | 10,
                data1: 48,
                data2: 80,
            },
            5,
        );
        assert_eq!(a["commandCode"], "NoteOn");
        assert_eq!(a["commandCodeNum"], 144);
        assert_eq!(a["channel"], 11);
        assert_eq!(a["noteName"], "C4");
        assert_eq!(a["rawMessage"], "0050309A");
        assert!((a["velocityNormal"].as_f64().unwrap() - 0.629_921_259_842_52).abs() < 1e-9);
        let c = args(
            "x",
            "y",
            &Msg {
                status: 0xB0,
                data1: 10,
                data2: 80,
            },
            0,
        );
        assert_eq!(c["controllerName"], "Pan");
        assert_eq!(command_code("ControlChange"), Some(0xB0));
        assert_eq!(command_code("noteon"), Some(0x90));
    }

    /// The Linux reader, on a FIFO standing in for /dev/snd/midiC9D0.
    #[cfg(target_os = "linux")]
    #[test]
    fn a_device_file_is_read() {
        let d = std::env::temp_dir().join(format!("streamerz-midi-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&d);
        let fifo = d.join("midiC9D0");
        let c = std::ffi::CString::new(fifo.to_string_lossy().as_bytes()).unwrap();
        // SAFETY: making a FIFO at our own path.
        assert_eq!(unsafe { libc::mkfifo(c.as_ptr(), 0o600) }, 0);
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        rt.block_on(async {
            let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
            let dev = Device {
                name: "test".into(),
                id: fifo.to_string_lossy().into_owned(),
            };
            let r = tokio::spawn(read(dev, tx));
            let path = fifo.clone();
            std::thread::spawn(move || {
                use std::io::Write;
                let mut f = std::fs::OpenOptions::new().write(true).open(path).unwrap();
                f.write_all(&[0xB0, 1, 127]).unwrap();
            });
            let got = tokio::time::timeout(std::time::Duration::from_secs(2), rx.recv()).await;
            r.abort();
            assert_eq!(
                got.unwrap(),
                Some(Msg {
                    status: 0xB0,
                    data1: 1,
                    data2: 127
                })
            );
        });
        let _ = std::fs::remove_dir_all(&d);
    }
}
