//! Clipboard trigger payloads, shared by native listeners and headless scenarios.

use crate::model::Obj;

pub const T_CLIPBOARD: u32 = 42001;

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
