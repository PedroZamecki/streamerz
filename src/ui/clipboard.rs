//! Clipboard Changed (42001): FLTK tells when the clipboard changes (X11 and Windows always; Wayland
//! only while the window has focus, as the protocol gives the selection to the focused app). It is
//! listened to only while an enabled action uses the trigger. The text is read by pasting it into a
//! hidden widget, then the engine fires the trigger with Streamer.bot's arguments.

use crate::engine::UiCmd;
use crate::model::Obj;
use crate::Config;
use fltk::app::{self, ClipboardContent};
use fltk::enums::Event;
use fltk::frame::Frame;
use fltk::prelude::*;
use serde_json::Value;
use std::cell::{Cell, RefCell};
use std::sync::Arc;
use tokio::sync::{mpsc::UnboundedSender, watch};

pub const T_CLIPBOARD: u32 = 42001;

thread_local! {
    static ON: Cell<bool> = const { Cell::new(false) };
    /// Takes the pasted text (made the first time the listener is on).
    static SINK: RefCell<Option<Frame>> = const { RefCell::new(None) };
}

fn used(c: &Config) -> bool {
    c.actions.actions.iter().filter(|a| a.enabled).any(|a| {
        a.triggers.iter().any(|t| {
            t.get("type").and_then(Value::as_u64) == Some(u64::from(T_CLIPBOARD))
                && t.get("enabled").and_then(Value::as_bool).unwrap_or(true)
        })
    })
}

/// Listen to the clipboard while an action uses Clipboard Changed (called when the settings change).
pub fn sync(cfg: &Arc<watch::Sender<Config>>, cmds: &UnboundedSender<UiCmd>) {
    let want = used(&cfg.borrow());
    if ON.with(|o| o.replace(want)) == want {
        return;
    }
    if !want {
        app::remove_clipboard_notify3();
        return;
    }
    SINK.with_borrow_mut(|s| {
        if s.is_none() {
            let mut f = Frame::new(0, 0, 0, 0, None);
            f.hide();
            let cmds = cmds.clone();
            f.handle(move |_, ev| {
                if ev != Event::Paste {
                    return false;
                }
                send(&cmds, true, false, app::event_text());
                true
            });
            *s = Some(f);
        }
    });
    let cmds = cmds.clone();
    // Source 1 is the clipboard (0 is X11's selection, which isn't it).
    app::add_clipboard_notify3(move |source| {
        if source != 1 {
            return;
        }
        let (text, image) = (
            app::clipboard_contains(ClipboardContent::Text),
            app::clipboard_contains(ClipboardContent::Image),
        );
        if text {
            // The text comes back as a paste event to the hidden frame.
            SINK.with_borrow(|s| {
                if let Some(f) = s {
                    app::paste_text(f);
                }
            });
        } else {
            send(&cmds, false, image, String::new());
        }
    });
}

/// What Streamer.bot passes: the formats, what they contain, and the text.
pub fn args(text: bool, image: bool, content: &str) -> Obj {
    let mut formats: Vec<&str> = Vec::new();
    if text {
        formats.extend(["Text", "UnicodeText", "System.String"]);
    }
    if image {
        formats.extend(["Bitmap", "System.Drawing.Bitmap"]);
    }
    let mut a = Obj::new();
    a.insert("formats.count".into(), formats.len().into());
    for (i, f) in formats.iter().enumerate() {
        a.insert(format!("formats.{i}"), (*f).into());
    }
    a.insert("containsText".into(), text.into());
    a.insert("containsImage".into(), image.into());
    a.insert("containsAudio".into(), false.into());
    if text {
        a.insert("clipboard.text".into(), content.into());
    }
    a.insert("error".into(), false.into());
    a
}

fn send(cmds: &UnboundedSender<UiCmd>, text: bool, image: bool, content: String) {
    let a = args(text, image, &content);
    super::tell(cmds, move |e| e.clipboard_changed(a));
}
