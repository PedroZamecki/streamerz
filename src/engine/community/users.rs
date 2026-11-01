//! Compact resident user records. Only the record being changed becomes a JSON map.

use super::{s, SavedUser};
use crate::model::Obj;
use flate2::{read::DeflateDecoder, write::DeflateEncoder, Compression};
use serde::{Deserialize, Serialize};
use serde_json::value::RawValue;
use std::collections::HashMap;
use std::io::Read;

#[derive(Clone, Copy, Default)]
enum Mode {
    Map,
    #[default]
    Json,
    Deflate,
}

fn mode() -> Mode {
    static MODE: std::sync::OnceLock<Mode> = std::sync::OnceLock::new();
    *MODE.get_or_init(|| {
        if crate::brand::IS_DEV {
            match std::env::var("STREAMERZ_MEMORY_RECORDS").as_deref() {
                Ok("map") => return Mode::Map,
                Ok("deflate") => return Mode::Deflate,
                _ => {}
            }
        }
        Mode::Json
    })
}

/// Only the columns needed by the Community list. Full records stay packed in the engine.
#[derive(Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub(crate) struct UserRow {
    #[serde(rename = "type")]
    pub platform: String,
    pub id: String,
    pub channel: String,
    pub display: String,
    pub role: u64,
    pub first_message: String,
    pub last_active: String,
}

impl Default for UserRow {
    fn default() -> Self {
        Self {
            platform: String::new(),
            id: String::new(),
            channel: String::new(),
            display: String::new(),
            role: 1,
            first_message: String::new(),
            last_active: String::new(),
        }
    }
}

impl UserRow {
    fn from_obj(value: &Obj) -> Self {
        Self {
            platform: s(value, "type").into(),
            id: s(value, "id").into(),
            channel: s(value, "channel").into(),
            display: s(value, "display").into(),
            role: value.get("role").and_then(serde_json::Value::as_u64).unwrap_or(1),
            first_message: s(value, "firstMessage").into(),
            last_active: s(value, "lastActive").into(),
        }
    }
}

enum Record {
    Map(Obj),
    Json(Box<RawValue>),
    Deflate(Box<[u8]>),
}

impl Record {
    fn new(value: Obj) -> Self {
        Self::with_mode(value, mode())
    }

    fn with_mode(value: Obj, mode: Mode) -> Self {
        match mode {
            Mode::Map => Self::Map(value),
            Mode::Json => Self::Json(serde_json::value::to_raw_value(&SavedUser(&value)).expect("user JSON")),
            Mode::Deflate => {
                let mut encoder = DeflateEncoder::new(Vec::new(), Compression::fast());
                serde_json::to_writer(&mut encoder, &SavedUser(&value)).expect("user compression");
                Self::Deflate(encoder.finish().expect("compressed user").into_boxed_slice())
            }
        }
    }

    fn raw(&self) -> Vec<u8> {
        let Self::Deflate(bytes) = self else { unreachable!() };
        let mut data = Vec::new();
        DeflateDecoder::new(&bytes[..])
            .read_to_end(&mut data)
            .expect("resident user compression");
        data
    }

    fn row(&self) -> UserRow {
        let parsed = match self {
            Self::Map(value) => return UserRow::from_obj(value),
            Self::Json(value) => serde_json::from_str(value.get()),
            Self::Deflate(_) => serde_json::from_slice(&self.raw()),
        };
        parsed.unwrap_or_else(|_| UserRow::from_obj(&self.get()))
    }

    fn get(&self) -> Obj {
        match self {
            Self::Map(value) => value.clone(),
            Self::Json(value) => serde_json::from_str(value.get()).expect("resident user JSON"),
            Self::Deflate(_) => serde_json::from_slice(&self.raw()).expect("resident user JSON"),
        }
    }
}

impl Serialize for Record {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        match self {
            Self::Map(value) => SavedUser(value).serialize(serializer),
            Self::Json(value) => value.serialize(serializer),
            Self::Deflate(_) => {
                let data = self.raw();
                let raw = RawValue::from_string(String::from_utf8(data).expect("resident user UTF-8"))
                    .map_err(serde::ser::Error::custom)?;
                raw.serialize(serializer)
            }
        }
    }
}

/// Identity index plus packed records. This does not discard historical users.
#[derive(Default)]
pub(super) struct Users {
    records: Vec<Record>,
    index: HashMap<String, HashMap<String, HashMap<String, usize>>>,
}

impl Users {
    pub fn push(&mut self, value: Obj) {
        self.index_value(&value, self.records.len());
        self.records.push(Record::new(value));
    }

    pub fn find(&self, platform: &str, id: &str, channel: &str) -> Option<usize> {
        let channels = self.index.get(platform)?;
        let exact = channels.get(channel).and_then(|ids| ids.get(id)).copied();
        let legacy = channels.get("").and_then(|ids| ids.get(id)).copied();
        exact.into_iter().chain(legacy).min()
    }

    fn index_value(&mut self, value: &Obj, index: usize) {
        self.index
            .entry(s(value, "type").into())
            .or_default()
            .entry(s(value, "channel").into())
            .or_default()
            .entry(s(value, "id").into())
            .or_insert(index);
    }

    pub fn get(&self, index: usize) -> Obj {
        self.records[index].get()
    }

    pub fn put(&mut self, index: usize, value: Obj) {
        let unchanged = self
            .index
            .get(s(&value, "type"))
            .and_then(|channels| channels.get(s(&value, "channel")))
            .and_then(|ids| ids.get(s(&value, "id")))
            == Some(&index);
        self.records[index] = Record::new(value);
        if !unchanged {
            self.reindex();
        }
    }

    pub fn rows(&self) -> impl Iterator<Item = UserRow> + '_ {
        self.records.iter().map(Record::row)
    }

    pub fn iter(&self) -> impl Iterator<Item = Obj> + '_ {
        self.records.iter().map(Record::get)
    }

    fn reindex(&mut self) {
        self.index.clear();
        for i in 0..self.records.len() {
            self.index_value(&self.records[i].get(), i);
        }
    }

    pub fn retain(&mut self, mut keep: impl FnMut(&Obj) -> bool) {
        self.records.retain(|record| keep(&record.get()));
        self.reindex();
        self.records.shrink_to_fit();
        self.index.shrink_to_fit();
    }

    pub fn edit_matching(&mut self, mut predicate: impl FnMut(&Obj) -> bool, mut edit: impl FnMut(&mut Obj)) {
        for record in &mut self.records {
            let mut value = record.get();
            if predicate(&value) {
                edit(&mut value);
                *record = Record::new(value);
            }
        }
        self.reindex();
    }
}

impl Serialize for Users {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        self.records.serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for Users {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct Visitor;
        impl<'de> serde::de::Visitor<'de> for Visitor {
            type Value = Users;
            fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
                f.write_str("user records")
            }
            fn visit_seq<A: serde::de::SeqAccess<'de>>(self, mut seq: A) -> Result<Users, A::Error> {
                let mut users = Users::default();
                while let Some(value) = seq.next_element::<Obj>()? {
                    users.push(value);
                }
                Ok(users)
            }
        }
        deserializer.deserialize_seq(Visitor)
    }
}

impl From<Vec<Obj>> for Users {
    fn from(values: Vec<Obj>) -> Self {
        let mut users = Self::default();
        for value in values {
            users.push(value);
        }
        users
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn every_encoding_preserves_compact_records_and_unknown_fields() {
        let value = json!({"type":"kick","id":"ü7","channel":"ana","role":1,
            "present":false,"messages":8,"lastActive":"2026-11-01T01:02:03.5-03:00",
            "custom":{"nested":[1,"hello",true]},"display":"Ana"});
        let mut expected = value.as_object().unwrap().clone();
        super::super::shrink(&mut expected);
        for mode in [Mode::Map, Mode::Json, Mode::Deflate] {
            let record = Record::with_mode(value.as_object().unwrap().clone(), mode);
            let output = serde_json::to_value(&record).unwrap();
            assert_eq!(output, serde_json::Value::Object(expected.clone()));
            assert_eq!(record.get()["custom"], value["custom"]);
        }
    }

    #[test]
    fn list_rows_read_columns_without_changing_stored_values() {
        for value in [
            serde_json::json!({"type":"twitch","id":"1","display":"Ana","custom":{"keep":true}}),
            serde_json::json!({"type":"kick","id":"2","display":null,"role":"future"}),
        ] {
            let record = Record::with_mode(value.as_object().unwrap().clone(), Mode::Json);
            let before = serde_json::to_value(&record).unwrap();
            let row = record.row();
            assert_eq!(row.id, value["id"].as_str().unwrap());
            assert_eq!(row.role, 1);
            assert_eq!(row.display, value["display"].as_str().unwrap_or(""));
            assert_eq!(serde_json::to_value(&record).unwrap(), before);
        }
    }

    #[test]
    fn legacy_and_exact_matches_keep_original_first_record_order() {
        let users: Users = serde_json::from_value(json!([
            {"type":"kick","id":"1","channel":"ana"},
            {"type":"kick","id":"1"},
            {"type":"kick","id":"2"},
            {"type":"kick","id":"2","channel":"ana"}
        ]))
        .unwrap();
        assert_eq!(users.find("kick", "1", "ana"), Some(0));
        assert_eq!(users.find("kick", "2", "ana"), Some(2));
        assert_eq!(users.find("kick", "1", "bia"), Some(1));
    }

    #[test]
    fn identity_legacy_unknown_fields_and_updates_round_trip() {
        let value =
            json!({"type":"twitch","id":"7","channel":"","display":"Ana","extra":{"nested":[1,2,3]},"messages":8});
        let mut users: Users = serde_json::from_value(json!([value])).unwrap();
        assert_eq!(users.find("twitch", "7", "ana"), Some(0));
        let mut user = users.get(0);
        super::super::expand(&mut user);
        assert_eq!(user["role"], 1);
        user.insert("messages".into(), json!(9));
        user.insert("channel".into(), json!("ana"));
        users.put(0, user);
        assert_eq!(users.find("twitch", "7", "ana"), Some(0));
        assert_eq!(users.find("twitch", "7", "other"), None);
        let round: Users = serde_json::from_slice(&serde_json::to_vec(&users).unwrap()).unwrap();
        assert_eq!(round.get(0)["extra"], value["extra"]);
        assert_eq!(round.get(0)["messages"], 9);
    }
}

/// Insertion order on the wire, with a small sorted index for duplicate checks.
/// The index shares the stored strings rather than allocating another name per person.
#[derive(Default, Clone)]
pub(super) struct Names {
    values: Vec<String>,
    sorted: Vec<usize>,
}

impl Names {
    pub fn add(&mut self, name: &str) {
        if name.is_empty() {
            return;
        }
        let at = match self.sorted.binary_search_by(|&i| self.values[i].as_str().cmp(name)) {
            Ok(_) => return,
            Err(at) => at,
        };
        self.sorted.insert(at, self.values.len());
        self.values.push(name.into());
    }

    pub fn as_slice(&self) -> &[String] {
        &self.values
    }
}

impl Serialize for Names {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        self.values.serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for Names {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let values = Vec::<String>::deserialize(deserializer)?;
        let mut sorted: Vec<usize> = (0..values.len()).collect();
        sorted.sort_unstable_by(|&a, &b| values[a].cmp(&values[b]));
        Ok(Self { values, sorted })
    }
}

#[cfg(test)]
mod name_tests {
    use super::*;
    #[test]
    fn credits_keep_insertion_order_and_case_sensitive_names_across_restart() {
        let mut names = Names::default();
        for name in ["Zed", "Ana", "Zed", "ana", ""] {
            names.add(name);
        }
        assert_eq!(names.as_slice(), ["Zed", "Ana", "ana"]);
        let mut loaded: Names = serde_json::from_slice(&serde_json::to_vec(&names).unwrap()).unwrap();
        loaded.add("Ana");
        loaded.add("Bo");
        assert_eq!(loaded.as_slice(), ["Zed", "Ana", "ana", "Bo"]);
    }
}
