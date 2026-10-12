//! Streamer.bot's data: its `actions.json` (v24) and import/export strings, read and written back
//! byte-for-byte the way Streamer.bot (Newtonsoft.Json) writes them. Field names and order are
//! Streamer.bot's; unknown fields are kept (at the end of their object) so nothing is lost.
#![allow(dead_code)] // ponytail: the engine (phase 3) and the editor (phase 5) use it.

use base64::Engine;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::io::{Read, Write};

/// A JSON object, in Streamer.bot's key order. Sub-actions and triggers are these: their fields
/// depend on their `type` (see types.rs).
pub type Obj = Map<String, Value>;

/// Streamer.bot's `data/actions.json`.
#[derive(Default, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct ActionsFile {
    pub version: u32,
    /// When it was saved.
    pub t: String,
    pub blocking: bool,
    pub collapsed_groups: Vec<Value>,
    pub groups: Vec<Value>,
    pub queues: Vec<Obj>,
    pub actions: Vec<Action>,
    #[serde(flatten)]
    pub rest: Obj,
}

#[derive(Clone, Default, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Action {
    pub id: String,
    /// Queue id; all zeros (or null in exports) is the default queue.
    pub queue: Option<String>,
    pub enabled: bool,
    pub exclude_from_history: bool,
    pub exclude_from_pending: bool,
    pub name: String,
    pub group: String,
    pub always_run: bool,
    pub random_action: bool,
    pub concurrent: bool,
    pub triggers: Vec<Obj>,
    pub sub_actions: Vec<Obj>,
    pub collapsed_groups: Vec<Value>,
    #[serde(flatten)]
    pub rest: Obj,
}

const BOM: &[u8] = b"\xef\xbb\xbf";

/// Export strings: base64 of this, then the gzipped JSON.
const MAGIC: &[u8] = b"SBAE";

/// JSON as Newtonsoft.Json writes it: indented (2 spaces, \r\n) for exports, compact for the data
/// files. ponytail: doubles past 1e16 or under 1e-5 come out as 1e20, Newtonsoft writes 1E+20;
/// Streamer.bot's numbers (weights, counts) never get there.
pub fn to_json(v: &impl Serialize, indent: bool) -> Vec<u8> {
    let s = if indent {
        serde_json::to_string_pretty(v)
    } else {
        serde_json::to_string(v)
    }
    .unwrap_or_default();
    // Newtonsoft also escapes these three, and breaks lines with \r\n. Raw newlines and these
    // characters can only be inside strings (escaped) or between values, so replacing is safe.
    let s = s
        .replace('\u{85}', "\\u0085")
        .replace('\u{2028}', "\\u2028")
        .replace('\u{2029}', "\\u2029");
    if indent {
        s.replace('\n', "\r\n").into_bytes()
    } else {
        s.into_bytes()
    }
}

/// Parse JSON that may start with a UTF-8 BOM (Streamer.bot writes one in its data files).
pub fn from_json<T: serde::de::DeserializeOwned>(b: &[u8]) -> Result<T, String> {
    serde_json::from_slice(b.strip_prefix(BOM).unwrap_or(b)).map_err(|e| e.to_string())
}

pub fn read_actions(b: &[u8]) -> Result<ActionsFile, String> {
    from_json(b)
}

/// actions.json as Streamer.bot writes it: BOM, then compact JSON.
pub fn write_actions(a: &ActionsFile) -> Vec<u8> {
    [BOM, &to_json(a, false)].concat()
}

/// An import string's JSON (as Streamer.bot wrote it).
pub fn unpack(s: &str) -> Result<Vec<u8>, String> {
    let raw = base64::engine::general_purpose::STANDARD
        .decode(s.trim())
        .map_err(|e| format!("not an import string ({e})"))?;
    let gz = raw.strip_prefix(MAGIC).ok_or("not a Streamer.bot import string")?;
    let mut json = Vec::new();
    flate2::read::GzDecoder::new(gz)
        .read_to_end(&mut json)
        .map_err(|e| format!("broken import string ({e})"))?;
    Ok(json)
}

/// An export string for this JSON. The gzip header matches .NET's (no time, fastest level, OS 0).
pub fn pack(json: &[u8]) -> String {
    let mut gz = flate2::GzBuilder::new()
        .operating_system(0)
        .write(MAGIC.to_vec(), flate2::Compression::fast());
    let _ = gz.write_all(json);
    base64::engine::general_purpose::STANDARD.encode(gz.finish().unwrap_or_default())
}

/// An import string's contents. Exports keep their own version's shape (sub-actions under
/// `actions` before v23, `subActions` after), so they stay plain JSON objects.
pub fn read_export(s: &str) -> Result<Obj, String> {
    from_json(&unpack(s)?)
}

pub fn write_export(v: &Obj) -> String {
    pack(&to_json(v, true))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{name, SUB_ACTIONS, TRIGGERS};

    /// Each fixture: unpacked, parsed and written back gives the very same JSON, and so does
    /// packing it again.
    fn round_trip_export(s: &str) {
        let json = unpack(s).unwrap();
        let v: Obj = from_json(&json).unwrap();
        let again = to_json(&v, true);
        assert_eq!(String::from_utf8_lossy(&again), String::from_utf8_lossy(&json));
        assert_eq!(unpack(&write_export(&v)).unwrap(), json);
    }

    #[test]
    fn exports_round_trip() {
        for f in ["export-v3.json", "export-v10.json", "export-v23.json"] {
            let json = std::fs::read(format!("tests/data/{f}")).unwrap();
            let s = pack(&json);
            assert!(s.starts_with("U0JBRR+LCAAAAAAABA"), "{f}: {s}"); // "SBAE", .NET's gzip header
            round_trip_export(&s);
        }
    }

    #[test]
    fn actions_round_trip() {
        let b = std::fs::read("tests/data/actions.json").unwrap();
        let a = read_actions(&b).unwrap();
        assert_eq!(a.actions[0].name, "Say hello");
        let sub = &a.actions[0].sub_actions[0];
        assert_eq!(name(SUB_ACTIONS, sub["type"].as_u64().unwrap() as u32), Some("Message"));
        let trig = &a.actions[0].triggers[0];
        assert_eq!(
            name(TRIGGERS, trig["type"].as_u64().unwrap() as u32),
            Some("TwitchFollow")
        );
        assert_eq!(String::from_utf8_lossy(&write_actions(&a)), String::from_utf8_lossy(&b));
    }

    #[test]
    fn unknown_fields_and_escapes_kept() {
        let json = "{\"version\":25,\"future\":{\"z\":1,\"a\":[0.5,1.0]},\"t\":\"\\u2028\\u0085\\u001f\u{e9}\"}";
        let a = read_actions(json.as_bytes()).unwrap();
        let out = String::from_utf8(to_json(&a, false)).unwrap();
        assert!(out.contains("\"t\":\"\\u2028\\u0085\\u001f\u{e9}\""), "{out}");
        assert!(out.ends_with("\"future\":{\"z\":1,\"a\":[0.5,1.0]}}"), "{out}");
    }

    #[test]
    fn rejects_other_strings() {
        assert!(unpack("hello").is_err());
        assert!(unpack(&base64::engine::general_purpose::STANDARD.encode("XXXXabc")).is_err());
    }

    /// The real thing, when available: every export string (one per line) and actions.json in
    /// $STREAMERZ_SAMPLES. Not committed: Streamer.bot's docs have no license, and actions.json
    /// is user data. See DEVELOPING.md.
    #[test]
    fn real_samples() {
        let Some(dir) = std::env::var_os("STREAMERZ_SAMPLES") else {
            return;
        };
        let dir = std::path::PathBuf::from(dir);
        let mut n = 0;
        if let Ok(s) = std::fs::read_to_string(dir.join("exports.txt")) {
            for line in s.lines().filter(|l| !l.trim().is_empty()) {
                round_trip_export(line);
                n += 1;
            }
        }
        if let Ok(b) = std::fs::read(dir.join("actions.json")) {
            let a = read_actions(&b).unwrap();
            assert_eq!(String::from_utf8_lossy(&write_actions(&a)), String::from_utf8_lossy(&b));
            n += 1;
        }
        assert!(n > 0, "nothing in {}", dir.display());
    }
}
