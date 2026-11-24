//! The Voices of the Sounds page: the text to speech engines and their voices.
use crate::engine::Engine;
use crate::i18n::tr;
use serde_json::{json, Value};
use std::rc::Rc;

/// The Voices table of the Sounds page.
pub fn table(_engine: &Rc<Engine>) -> Value {
    let mut t = super::pages::table(
        "sounds_voices",
        tr("sounds.show.voices"),
        tr("sounds.voices_empty"),
        vec![super::view::column("voice", "sounds.voice", 240)],
        Vec::new(),
        Vec::new(),
    );
    t["domain"] = json!(super::sounds::DOMAIN);
    t
}
