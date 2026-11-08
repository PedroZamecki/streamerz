//! Inputs, Hotkeys and MIDI (Phase 15).
//!
//! Sub-actions:
//! - 1012: Keyboard Press (key, modifiers)
//! - 9001: MIDI Note On (device, name, sendNoteOff, channel, note, velocity, duration)
//! - 9002: MIDI Control Change (device, name, channel, controller, value)
//! - 9003: MIDI Generic Event (device, name, command, channel, data1, data2)
//!
//! Triggers:
//! - 15001: MidiMessage

pub const T_MIDI_MESSAGE: u32 = 15001;

pub const S_KEYBOARD_PRESS: u32 = 1012;
pub const S_MIDI_NOTE_ON: u32 = 9001;
pub const S_MIDI_CONTROL_CHANGE: u32 = 9002;
pub const S_MIDI_GENERIC: u32 = 9003;

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

/// Find a named key in the same table used by the hotkey editor.
fn keyboard_vk(key: &str) -> Option<u16> {
    crate::hotkeys::keys()
        .iter()
        .find(|k| k.name.eq_ignore_ascii_case(key) || k.keysym.eq_ignore_ascii_case(key))
        .map(|k| k.vk)
}

/// Send a key and its modifiers on Windows.
pub fn simulate_keyboard_press(key: &str, modifiers: u32) -> Result<(), String> {
    let vk = keyboard_vk(key).ok_or_else(|| format!("unknown key: {key}"))?;
    #[cfg(target_os = "windows")]
    {
        use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
            SendInput, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYEVENTF_KEYUP, VK_CONTROL, VK_LWIN, VK_MENU,
            VK_SHIFT,
        };
        let mods = [(1, VK_MENU), (2, VK_CONTROL), (4, VK_SHIFT), (8, VK_LWIN)];
        let event = |key, up| INPUT {
            r#type: INPUT_KEYBOARD,
            Anonymous: INPUT_0 {
                ki: KEYBDINPUT {
                    wVk: key,
                    dwFlags: if up { KEYEVENTF_KEYUP } else { 0 },
                    ..Default::default()
                },
            },
        };
        let mut events = Vec::with_capacity(10);
        for (mask, key) in mods {
            if modifiers & mask != 0 {
                events.push(event(key, false));
            }
        }
        events.push(event(vk, false));
        events.push(event(vk, true));
        for (mask, key) in mods.into_iter().rev() {
            if modifiers & mask != 0 {
                events.push(event(key, true));
            }
        }
        let sent = unsafe {
            SendInput(
                events.len() as u32,
                events.as_ptr(),
                std::mem::size_of::<INPUT>() as i32,
            )
        };
        if sent != events.len() as u32 {
            return Err(format!("SendInput sent {sent} of {} events", events.len()));
        }
        Ok(())
    }
    #[cfg(not(target_os = "windows"))]
    {
        let _ = (vk, modifiers);
        Err("keyboard simulation is not supported on this system".into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn modifier_formatting() {
        assert_eq!(format_modifiers(0), "");
        assert_eq!(format_modifiers(2), "Control");
        assert_eq!(format_modifiers(3), "Control + Alt");
        assert_eq!(format_modifiers(7), "Control + Alt + Shift");
    }

    #[test]
    fn keyboard_press_simulation() {
        assert_eq!(keyboard_vk("F13"), Some(0x7c));
        assert_eq!(keyboard_vk("return"), Some(0x0d));
        assert_eq!(keyboard_vk(""), None);
    }
}
