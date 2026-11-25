//! MIDI Message (15001): every MIDI input is open while an enabled action uses the trigger, none
//! otherwise; and the MIDI Out sub-actions (9001-9003).

use super::{s, Engine, Obj};
use crate::midi::{self, Msg};
use serde_json::Value;
use std::rc::Rc;
use std::time::Duration;

pub const T_MIDI: u32 = 15001;

/// A trigger's field: empty (or missing) is any.
fn any_or(t: &Obj, k: &str, f: impl Fn(&str) -> bool) -> bool {
    let v = t.get(k).map(crate::vars::text).unwrap_or_default();
    let v = v.trim();
    v.is_empty() || v == "0" && (k == "channel" || k == "commandCode") || f(v)
}

/// The message a MIDI Out sub-action sends: Note On (9001), Control Change (9002) or the command
/// named (9003), on channel 1-16. Numbers may be written as text. None for an unknown command.
fn out_bytes(kind: u64, o: &Obj) -> Option<Vec<u8>> {
    let n = |k: &str| super::n(o, k).clamp(0.0, 127.0) as u8;
    let ch = n("channel").clamp(1, 16) - 1;
    Some(match kind {
        9001 => vec![0x90 | ch, n("note"), n("velocity")],
        9002 => vec![0xB0 | ch, n("controller"), n("value")],
        _ => vec![
            (midi::command_code(s(o, "command"))? & 0xF0) | ch,
            n("data1"),
            n("data2"),
        ],
    })
}

impl Engine {
    /// Open or close the MIDI inputs to match the actions (called when the settings change).
    pub fn midi_sync(self: &Rc<Self>) {
        // The devices there now, for the editors' device fields (a folder read; no device opened).
        let names = |l: Vec<midi::Device>| l.into_iter().map(|d| (d.name.clone(), d.name)).collect();
        crate::catalog::set("midi.inputs", names(midi::inputs()));
        crate::catalog::set("midi.outputs", names(midi::outputs()));
        let want = self.uses_trigger(T_MIDI);
        let mut st = self.st.borrow_mut();
        let open = !st.midi_tasks.is_empty();
        if want == open {
            return;
        }
        for t in std::mem::take(&mut st.midi_tasks) {
            t.abort();
        }
        if want {
            for d in midi::inputs() {
                crate::say(format!("MIDI: listening to {}", d.name));
                st.midi_tasks
                    .push(tokio::task::spawn_local(self.clone().midi_listen(d)).abort_handle());
            }
        }
    }

    async fn midi_listen(self: Rc<Self>, d: midi::Device) {
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
        let name = d.name.clone();
        let read = midi::read(d, tx);
        tokio::pin!(read);
        let start = std::time::Instant::now();
        loop {
            tokio::select! {
                r = &mut read => {
                    if let Err(e) = r {
                        crate::warn(format!("MIDI {name}: {e}"));
                    }
                    return;
                }
                m = rx.recv() => match m {
                    Some(m) => self.midi_fire(&name, &name, &m, start.elapsed().as_millis() as u64),
                    None => return,
                },
            }
        }
    }

    /// MIDI Message for `m` from a device. The trigger may name the device, the command (`NoteOn`),
    /// the channel (1-16; 0 any) and the note or controller (`data1`).
    pub(crate) fn midi_fire(self: &Rc<Self>, name: &str, device: &str, m: &Msg, ms: u64) {
        let args = midi::args(name, device, m, ms);
        let channel = (m.status & 0x0F) + 1;
        self.fire(T_MIDI, "Midi Message", "Core/MIDI", "midi", &args, |t| {
            any_or(t, "device", |v| {
                v.eq_ignore_ascii_case(device) || v.eq_ignore_ascii_case(name)
            }) && any_or(t, "commandCode", |v| midi::command_code(v) == Some(m.status & 0xF0))
                && any_or(t, "channel", |v| v.parse() == Ok(channel))
                && any_or(t, "data1", |v| v.parse() == Ok(m.data1))
        });
        self.emit("Midi", "Message", || Value::Object(args));
    }

    /// MIDI Out: Note On (9001, and Note Off after `duration` ms when asked), Control Change (9002),
    /// Generic Event (9003). Channel 1-16.
    pub(super) fn midi_out(self: &Rc<Self>, kind: u64, o: &Obj, device: &str) {
        let Some(bytes) = out_bytes(kind, o) else {
            return crate::warn(format!("MIDI Out: no command {}", s(o, "command")));
        };
        if let Err(e) = midi::send(device, &bytes) {
            return crate::warn(format!("MIDI Out: {e}"));
        }
        if kind == 9001 && o.get("sendNoteOff").and_then(Value::as_bool).unwrap_or(false) {
            let (device, wait) = (device.to_string(), super::n(o, "duration").max(0.0) as u64);
            let off = [0x80 | (bytes[0] & 0x0F), bytes[1], 0];
            tokio::task::spawn_local(async move {
                tokio::time::sleep(Duration::from_millis(wait)).await;
                if let Err(e) = midi::send(&device, &off) {
                    crate::warn(format!("MIDI Out: {e}"));
                }
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn obj(v: Value) -> Obj {
        v.as_object().cloned().unwrap_or_default()
    }

    #[test]
    fn midi_out_builds_its_message_from_numbers_or_text() {
        let note = obj(json!({"channel": 2, "note": 60, "velocity": 127}));
        assert_eq!(out_bytes(9001, &note), Some(vec![0x91, 60, 127]));
        // Filled in from variables, the numbers arrive as text.
        let typed = obj(json!({"channel": "10", "controller": "7", "value": "100"}));
        assert_eq!(out_bytes(9002, &typed), Some(vec![0xB9, 7, 100]));
        let wild = obj(json!({"channel": 99, "note": 300, "velocity": -4}));
        assert_eq!(out_bytes(9001, &wild), Some(vec![0x9F, 127, 0]));
        assert_eq!(out_bytes(9001, &obj(json!({}))), Some(vec![0x90, 0, 0]));
        assert_eq!(out_bytes(9003, &obj(json!({"command": "nothing"}))), None);
    }

    #[test]
    fn a_trigger_field_left_empty_or_zero_is_any() {
        let t = obj(json!({"channel": 0, "commandCode": "", "device": "Pad", "data1": 60}));
        assert!(any_or(&t, "channel", |_| false));
        assert!(any_or(&t, "commandCode", |_| false));
        assert!(!any_or(&t, "device", |v| v == "Other"));
        assert!(any_or(&t, "device", |v| v == "Pad"));
        // Zero is a real note: only the channel and the command read 0 as any.
        assert!(!any_or(&t, "data1", |v| v == "0"));
        assert!(any_or(&obj(json!({})), "data1", |_| false));
    }
}
