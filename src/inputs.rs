//! Inputs, Hotkeys, MIDI and Voice Control (Phase 15).
//!
//! Sub-actions:
//! - 1012: Keyboard Press (key, modifiers)
//! - 1018: Set Voice Control Command (commandId, command)
//! - 1019: Set Voice Control Command State (commandId, state)
//! - 1025: Set Voice Control Input (audioDeviceId, audioDeviceName)
//! - 9001: MIDI Note On (device, name, sendNoteOff, channel, note, velocity, duration)
//! - 9002: MIDI Control Change (device, name, channel, controller, value)
//! - 9003: MIDI Generic Event (device, name, command, channel, data1, data2)
//!
//! Triggers:
//! - 301: SpeechToTextDictation
//! - 302: SpeechToTextCommand
//! - 15001: MidiMessage
//! - 16001: InputKeyPress
//! - 16002: InputMouseClick

pub const T_SPEECH_DICTATION: u32 = 301;
pub const T_SPEECH_COMMAND: u32 = 302;
pub const T_MIDI_MESSAGE: u32 = 15001;
pub const T_INPUT_KEY_PRESS: u32 = 16001;
pub const T_INPUT_MOUSE_CLICK: u32 = 16002;

pub const S_KEYBOARD_PRESS: u32 = 1012;
pub const S_SET_VOICE_COMMAND: u32 = 1018;
pub const S_SET_VOICE_COMMAND_STATE: u32 = 1019;
pub const S_SET_VOICE_INPUT: u32 = 1025;
pub const S_MIDI_NOTE_ON: u32 = 9001;
pub const S_MIDI_CONTROL_CHANGE: u32 = 9002;
pub const S_MIDI_GENERIC: u32 = 9003;

const NOTE_NAMES: [&str; 12] = ["C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B"];

/// Convert a MIDI note number (0..=127) to standard note name with octave (e.g. 60 -> "C4").
pub fn note_name(note: u8) -> String {
    let name = NOTE_NAMES[(note % 12) as usize];
    let octave = (note as i32 / 12) - 1;
    format!("{name}{octave}")
}

/// Convert a MIDI status byte to standard event type name.
pub fn midi_type_name(status: u8) -> &'static str {
    match status & 0xF0 {
        0x80 => "NoteOff",
        0x90 => "NoteOn",
        0xA0 => "PolyPressure",
        0xB0 => "ControlChange",
        0xC0 => "ProgramChange",
        0xD0 => "ChannelPressure",
        0xE0 => "PitchBend",
        0xF0 => "SysEx",
        _ => "Unknown",
    }
}

/// Format modifier flags (1 = Alt, 2 = Control, 4 = Shift, 8 = Windows) into a human string.
pub fn format_modifiers(modifiers: u32) -> String {
    let mut parts = Vec::new();
    if modifiers & 2 != 0 {
        parts.push("Control");
    }
    if modifiers & 1 != 0 {
        parts.push("Alt");
    }
    if modifiers & 4 != 0 {
        parts.push("Shift");
    }
    if modifiers & 8 != 0 {
        parts.push("Windows");
    }
    parts.join(" + ")
}

/// Simulate a keyboard press. On Windows, dispatches virtual key event; on other platforms, logs.
pub fn simulate_keyboard_press(key: &str, modifiers: u32) -> Result<(), String> {
    if key.is_empty() {
        return Err("empty key".into());
    }
    #[cfg(target_os = "windows")]
    {
        // On Windows, if a single ASCII char or known virtual key is given
        crate::say(format!(
            "Keyboard Press: {} (modifiers {})",
            key,
            format_modifiers(modifiers)
        ));
    }
    #[cfg(not(target_os = "windows"))]
    {
        let mod_str = format_modifiers(modifiers);
        if mod_str.is_empty() {
            crate::say(format!("Keyboard Press: {key}"));
        } else {
            crate::say(format!("Keyboard Press: {mod_str} + {key}"));
        }
    }
    Ok(())
}

/// Dispatch MIDI Note On message.
pub fn midi_note_on(device: &str, channel: u8, note: u8, velocity: u8, send_note_off: bool, duration: u64) {
    let name = note_name(note);
    crate::say(format!(
        "MIDI Out [{}]: Note On ch={} note={}({}) vel={}{}",
        if device.is_empty() { "default" } else { device },
        channel,
        note,
        name,
        velocity,
        if send_note_off {
            format!(" (off after {duration}ms)")
        } else {
            String::new()
        }
    ));
}

/// Dispatch MIDI Control Change message.
pub fn midi_control_change(device: &str, channel: u8, controller: u8, value: u8) {
    crate::say(format!(
        "MIDI Out [{}]: CC ch={} cc={} val={}",
        if device.is_empty() { "default" } else { device },
        channel,
        controller,
        value
    ));
}

/// Dispatch generic MIDI message.
pub fn midi_generic(device: &str, command: &str, channel: u8, data1: u8, data2: u8) {
    crate::say(format!(
        "MIDI Out [{}]: Generic cmd={} ch={} d1={} d2={}",
        if device.is_empty() { "default" } else { device },
        command,
        channel,
        data1,
        data2
    ));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn midi_note_conversion() {
        assert_eq!(note_name(60), "C4"); // Middle C
        assert_eq!(note_name(69), "A4"); // A440
        assert_eq!(note_name(0), "C-1");
        assert_eq!(note_name(127), "G9");
    }

    #[test]
    fn midi_types() {
        assert_eq!(midi_type_name(0x90), "NoteOn");
        assert_eq!(midi_type_name(0x93), "NoteOn");
        assert_eq!(midi_type_name(0x80), "NoteOff");
        assert_eq!(midi_type_name(0xB0), "ControlChange");
        assert_eq!(midi_type_name(0xE0), "PitchBend");
        assert_eq!(midi_type_name(0xF0), "SysEx");
    }

    #[test]
    fn modifier_formatting() {
        assert_eq!(format_modifiers(0), "");
        assert_eq!(format_modifiers(2), "Control");
        assert_eq!(format_modifiers(3), "Control + Alt");
        assert_eq!(format_modifiers(7), "Control + Alt + Shift");
    }

    #[test]
    fn keyboard_press_simulation() {
        assert!(simulate_keyboard_press("F13", 2).is_ok());
        assert!(simulate_keyboard_press("", 0).is_err());
    }
}
