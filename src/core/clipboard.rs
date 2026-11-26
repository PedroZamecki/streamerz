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

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn text_lists_its_formats_and_carries_the_text() {
        let a = args(true, false, "hello");
        assert_eq!(a["formats.count"], 3);
        assert_eq!(a["formats.0"], "Text");
        assert_eq!(a["formats.1"], "UnicodeText");
        assert_eq!(a["formats.2"], "System.String");
        assert_eq!(a["containsText"], true);
        assert_eq!(a["containsImage"], false);
        assert_eq!(a["containsAudio"], false);
        assert_eq!(a["clipboard.text"], "hello");
        assert_eq!(a["error"], false);
    }

    #[test]
    fn an_image_has_no_text_and_its_formats_follow_the_text_ones() {
        let image = args(false, true, "ignored");
        assert_eq!(image["formats.count"], 2);
        assert_eq!(image["formats.0"], "Bitmap");
        assert!(!image.contains_key("clipboard.text"));
        let both = args(true, true, "");
        assert_eq!(both["formats.count"], 5);
        assert_eq!(both["formats.3"], "Bitmap");
        assert_eq!(both["clipboard.text"], "");
        let none = args(false, false, "");
        assert_eq!(none["formats.count"], 0);
        assert!(!none.contains_key("formats.0"));
        assert_eq!(none["containsText"], json!(false));
    }
}
