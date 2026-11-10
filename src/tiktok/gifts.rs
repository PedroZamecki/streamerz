//! Room-scoped gift metadata. Browser requests run on the existing reader/bridge tasks.
use crate::{engine::Engine, model::Obj};
use serde::Serialize;
use serde_json::{json, Value};
use std::{rc::Rc, sync::Mutex};

#[derive(Clone, Debug, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Gift {
    pub id: String,
    pub name: String,
    pub coins: u64,
    pub streak: bool,
    pub picture_url: String,
}

#[derive(Default, Serialize)]
#[serde(rename_all = "camelCase")]
struct Catalog {
    room: String,
    channel: String,
    state: &'static str,
    error: String,
    gifts: Vec<Gift>,
    #[serde(skip)]
    generation: u64,
    #[serde(skip)]
    pending: Vec<Gift>,
}

static CATALOG: Mutex<Catalog> = Mutex::new(Catalog {
    room: String::new(),
    channel: String::new(),
    state: "unavailable",
    error: String::new(),
    gifts: Vec::new(),
    generation: 0,
    pending: Vec::new(),
});

impl Catalog {
    fn complete(&mut self, generation: u64, result: Result<Vec<Gift>, String>) -> bool {
        if self.generation != generation {
            return false;
        }
        match result {
            Ok(gifts) => {
                self.gifts = gifts;
                for gift in std::mem::take(&mut self.pending) {
                    if !self.gifts.iter().any(|g| g.id == gift.id) {
                        self.gifts.push(gift);
                    }
                }
                self.state = "ready";
                self.error.clear();
            }
            Err(error) => {
                self.pending.clear();
                self.state = "error";
                self.error = error;
            }
        }
        true
    }
}

pub fn status() -> Value {
    CATALOG.lock().map(|c| json!(*c)).unwrap_or(Value::Null)
}

/// A form's status line does not need to allocate or copy the gift list.
pub fn summary() -> (&'static str, usize, String) {
    CATALOG
        .lock()
        .map(|c| (c.state, c.gifts.len(), c.error.clone()))
        .unwrap_or(("unavailable", 0, String::new()))
}

pub fn invalidate() {
    if let Ok(mut c) = CATALOG.lock() {
        let generation = c.generation.wrapping_add(1);
        *c = Catalog {
            generation,
            state: "unavailable",
            ..Default::default()
        };
    }
    crate::catalog::set("tiktok.gifts", Vec::new());
}

fn identity(value: &Value) -> String {
    value
        .as_str()
        .map(str::to_string)
        .or_else(|| value.as_u64().map(|n| n.to_string()))
        .unwrap_or_default()
}

pub fn parse(body: &Value) -> Result<Vec<Gift>, String> {
    if body["status_code"].as_i64().is_some_and(|n| n != 0) {
        return Err("TikTok refused the gift list".into());
    }
    let values = body["data"]["gifts"].as_array().ok_or("TikTok returned no gift list")?;
    let mut gifts = Vec::<Gift>::new();
    for value in values {
        let id = identity(&value["id"]);
        if id.is_empty() || id == "0" {
            continue;
        }
        let gift = Gift {
            id,
            name: value["name"].as_str().unwrap_or_default().into(),
            coins: value["diamond_count"].as_u64().unwrap_or(0),
            streak: value["type"].as_u64() == Some(1),
            picture_url: value["image"]["url_list"][0]
                .as_str()
                .filter(|s| s.starts_with("https://"))
                .unwrap_or_default()
                .into(),
        };
        if let Some(old) = gifts.iter_mut().find(|g| g.id == gift.id) {
            *old = gift;
        } else {
            gifts.push(gift);
        }
    }
    Ok(gifts)
}

fn publish(c: &Catalog) {
    crate::catalog::set(
        "tiktok.gifts",
        c.gifts.iter().map(|g| (g.id.clone(), g.name.clone())).collect(),
    );
    crate::host::changed();
}

/// A received gift also teaches us about gifts missing from a list or added during this live.
pub fn observe(data: &Value) {
    let id = identity(&data["giftId"]);
    if id.is_empty() || id == "0" {
        return;
    }
    let Ok(mut c) = CATALOG.lock() else { return };
    let gift = Gift {
        id,
        name: data["giftName"].as_str().unwrap_or_default().into(),
        coins: data["diamondCount"].as_u64().unwrap_or(0),
        streak: data["giftType"].as_u64() == Some(1),
        picture_url: data["giftPictureUrl"].as_str().unwrap_or_default().into(),
    };
    if c.state == "loading" {
        if let Some(old) = c.pending.iter_mut().find(|g| g.id == gift.id) {
            *old = gift.clone();
        } else {
            c.pending.push(gift.clone());
        }
    }
    if let Some(old) = c.gifts.iter_mut().find(|g| g.id == gift.id) {
        // Sparse wire events must not erase richer catalog metadata.
        let mut updated = gift;
        if updated.name.is_empty() {
            updated.name.clone_from(&old.name);
        }
        if updated.picture_url.is_empty() {
            updated.picture_url.clone_from(&old.picture_url);
        }
        if data["diamondCount"].as_u64().is_none() {
            updated.coins = old.coins;
        }
        if data["giftType"].as_u64().is_none() {
            updated.streak = old.streak;
        }
        if *old == updated {
            return;
        }
        *old = updated;
    } else {
        c.gifts.push(gift);
    }
    publish(&c);
}

fn enrich_from(gift: &Gift, args: &mut Obj) {
    for (key, value) in [("giftName", &gift.name), ("giftPictureUrl", &gift.picture_url)] {
        if args.get(key).and_then(Value::as_str).is_none_or(str::is_empty) {
            args.insert(key.into(), value.clone().into());
        }
    }
    if args.get("diamondCount").and_then(Value::as_u64).is_none() {
        args.insert("diamondCount".into(), gift.coins.into());
    }
    if args.get("giftType").and_then(Value::as_u64).is_none() {
        args.insert("giftType".into(), u64::from(gift.streak).into());
    }
}

/// Use the current room's metadata for sparse messages, retaining explicit wire values.
pub fn enrich(args: &mut Obj) {
    if args
        .get("giftName")
        .and_then(Value::as_str)
        .is_some_and(|s| !s.is_empty())
        && args
            .get("giftPictureUrl")
            .and_then(Value::as_str)
            .is_some_and(|s| !s.is_empty())
        && args.get("diamondCount").and_then(Value::as_u64).is_some()
        && args.get("giftType").and_then(Value::as_u64).is_some()
    {
        return;
    }
    let id = args.get("giftId").map(identity).unwrap_or_default();
    if let Ok(catalog) = CATALOG.lock() {
        if let Some(gift) = catalog.gifts.iter().find(|g| g.id == id) {
            enrich_from(gift, args);
        }
    }
}

pub async fn refresh(engine: &Rc<Engine>) -> Result<Value, String> {
    if !engine.tiktok_connected() {
        return Err("Connect to a TikTok live first".into());
    }
    let room = engine
        .tiktok()
        .and_then(|t| t.room_id())
        .ok_or("Connect to a TikTok live first")?;
    let channel = engine.config().tiktok.channel.clone();
    if !engine.tiktok_bridge.ready() {
        return Err("Connect the TikTok browser account first".into());
    }
    let generation = {
        let mut c = CATALOG.lock().map_err(|_| "Gift catalog unavailable")?;
        if c.state == "loading" && c.room == room && c.channel == channel {
            return Ok(json!(*c));
        }
        let generation = c.generation.wrapping_add(1);
        if c.room != room || c.channel != channel {
            c.gifts.clear();
        }
        c.generation = generation;
        c.room = room.clone();
        c.channel = channel.clone();
        c.state = "loading";
        c.error.clear();
        c.pending.clear();
        publish(&c);
        generation
    };
    let result = crate::tiktok_bridge::gift_list(engine, &room)
        .await
        .and_then(|body| parse(&body));
    let mut c = CATALOG.lock().map_err(|_| "Gift catalog unavailable")?;
    if c.generation != generation
        || engine.config().tiktok.channel != channel
        || engine.tiktok().and_then(|t| t.room_id()).as_deref() != Some(&room)
    {
        return Err("The TikTok live or account changed; refresh its gifts again".into());
    }
    c.complete(generation, result);
    publish(&c);
    Ok(json!(*c))
}

/// Shared by real delivery and the no-side-effect matching preview.
pub fn matches(trigger: &Obj, data: &Value) -> bool {
    data.as_object().is_some_and(|args| matches_args(trigger, args))
}

pub fn matches_args(trigger: &Obj, args: &Obj) -> bool {
    let missing = Value::Null;
    let data = |key: &str| args.get(key).unwrap_or(&missing);
    if validate(trigger).is_err() {
        return false;
    }
    let want = trigger.get("giftId").map(identity).unwrap_or_default();
    if !want.trim().is_empty() && want != identity(data("giftId")) {
        return false;
    }
    let completed = trigger.get("giftDelivery").and_then(Value::as_i64).unwrap_or(0) == 1;
    if completed && data("giftType").as_u64() == Some(1) && data("repeatEnd").as_bool() != Some(true) {
        return false;
    }
    let amount = match data("diamondCount").as_u64() {
        Some(unit) => unit.saturating_mul(data("repeatCount").as_u64().unwrap_or(1).max(1)),
        None => data("coins").as_u64().unwrap_or(0),
    };
    let min = trigger.get("minCoins").and_then(Value::as_u64).unwrap_or(0);
    let max = trigger.get("maxCoins").and_then(Value::as_u64).unwrap_or(0);
    amount >= min && (max == 0 || amount <= max)
}

pub fn validate(trigger: &Obj) -> Result<(), String> {
    if trigger
        .get("giftId")
        .is_some_and(|v| !v.is_string() && v.as_u64().is_none())
    {
        return Err(crate::i18n::tr("automation.gift_id_error").into());
    }
    if trigger
        .get("giftDelivery")
        .is_some_and(|v| !matches!(v.as_u64(), Some(0 | 1)))
    {
        return Err(crate::i18n::tr("automation.delivery_error").into());
    }
    let id = trigger.get("giftId").map(identity).unwrap_or_default();
    if !id.is_empty() && id.parse::<u64>().is_err() {
        return Err(crate::i18n::tr("automation.gift_id_error").into());
    }
    for key in ["minCoins", "maxCoins"] {
        if trigger.get(key).is_some_and(|v| v.as_u64().is_none()) {
            return Err(crate::i18n::tr("automation.coin_error").into());
        }
    }
    let min = trigger.get("minCoins").and_then(Value::as_u64).unwrap_or(0);
    let max = trigger.get("maxCoins").and_then(Value::as_u64).unwrap_or(0);
    if max != 0 && min > max {
        return Err(crate::i18n::tr("automation.coin_range_error").into());
    }
    Ok(())
}

pub fn preview(trigger: &Obj, data: &Value) -> Result<Value, String> {
    validate(trigger)?;
    let matched = matches(trigger, data);
    Ok(json!({"matches": matched, "runsActions": false, "sample": data,
        "note": if matched { "The gift matches all configured conditions." } else { "The gift ID, delivery phase or total coin bounds do not match." }}))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn catalog_details_fill_sparse_events_without_overriding_wire_values() {
        let gift = Gift {
            id: "55".into(),
            name: "New gift".into(),
            coins: 5,
            streak: true,
            picture_url: "https://example.test/gift.png".into(),
        };
        let mut args =
            json!({"giftId":55,"giftName":"","diamondCount":null,"giftType":null,"repeatCount":3,"repeatEnd":false})
                .as_object()
                .unwrap()
                .clone();
        enrich_from(&gift, &mut args);
        assert_eq!(args["giftName"], "New gift");
        assert_eq!(args["diamondCount"], 5);
        assert_eq!(args["giftType"], 1);
        let trigger = json!({"giftId":"55","giftDelivery":1,"minCoins":15})
            .as_object()
            .unwrap()
            .clone();
        assert!(!matches_args(&trigger, &args));
        args.insert("repeatEnd".into(), true.into());
        assert!(matches_args(&trigger, &args));
        args.insert("diamondCount".into(), 0.into());
        args.insert("giftType".into(), 0.into());
        enrich_from(&gift, &mut args);
        assert_eq!(args["diamondCount"], 0);
        assert_eq!(args["giftType"], 0);
    }
    #[test]
    fn late_replies_cannot_replace_a_new_context_and_failed_refresh_keeps_gifts() {
        let gift = |id: &str| Gift {
            id: id.into(),
            name: id.into(),
            coins: 1,
            streak: false,
            picture_url: String::new(),
        };
        let mut catalog = Catalog {
            generation: 2,
            gifts: vec![gift("old")],
            pending: vec![gift("observed")],
            ..Default::default()
        };
        assert!(!catalog.complete(1, Ok(vec![gift("stale")])));
        assert_eq!(catalog.gifts[0].id, "old");
        assert!(catalog.complete(2, Ok(vec![gift("new")])));
        assert_eq!(
            catalog.gifts.iter().map(|g| g.id.as_str()).collect::<Vec<_>>(),
            ["new", "observed"]
        );
        catalog.complete(2, Err("refused".into()));
        assert_eq!(catalog.state, "error");
        assert_eq!(catalog.gifts.len(), 2);
        catalog.complete(2, Ok(Vec::new()));
        assert_eq!(catalog.state, "ready");
        assert!(catalog.gifts.is_empty());
    }
    #[test]
    fn invalid_bounds_and_delivery_are_rejected_in_previews_and_delivery() {
        for trigger in [
            json!({"giftId":false}),
            json!({"minCoins":-1}),
            json!({"maxCoins":1,"minCoins":2}),
            json!({"giftDelivery":2}),
        ] {
            let trigger = trigger.as_object().unwrap();
            assert!(preview(trigger, &json!({})).is_err());
            assert!(!matches(trigger, &json!({})));
        }
        assert_eq!(
            preview(&Obj::new(), &json!({"giftId":3})).unwrap()["runsActions"],
            false
        );
    }
    #[test]
    fn exact_gifts_completed_combos_and_inclusive_totals() {
        let event = json!({"giftId": 5, "giftType": 1, "repeatCount": 3, "diamondCount": 2, "repeatEnd": false});
        let mut trigger = Obj::new();
        assert!(matches(&trigger, &event));
        trigger.insert("giftId".into(), "6".into());
        assert!(!matches(&trigger, &event));
        trigger.insert("giftId".into(), "5".into());
        trigger.insert("giftDelivery".into(), 1.into());
        assert!(!matches(&trigger, &event));
        let mut ended = event.clone();
        ended["repeatEnd"] = true.into();
        trigger.insert("minCoins".into(), 6.into());
        trigger.insert("maxCoins".into(), 6.into());
        assert!(matches(&trigger, &ended));
        trigger.insert("maxCoins".into(), 5.into());
        assert!(!matches(&trigger, &ended));
        trigger.remove("maxCoins");
        ended["giftType"] = 0.into();
        ended["repeatEnd"] = false.into();
        assert!(matches(&trigger, &ended));
    }
    #[test]
    fn catalog_changes_need_no_compiled_gift_names() {
        let parsed = parse(&json!({"status_code":0,"data":{"gifts":[
            {"id":123456,"name":"New gift","diamond_count":8,"type":1,"image":{"url_list":["https://example.test/new.png"]}}
        ]}})).unwrap();
        assert_eq!(parsed[0].id, "123456");
        assert!(parsed[0].streak);
        assert_eq!(parsed[0].coins, 8);
        assert!(parse(&json!({"data":{"gifts":[]}})).unwrap().is_empty());
        assert!(parse(&json!({"status_code":1,"data":{"gifts":[]}})).is_err());
        assert!(parse(&json!({})).is_err());
    }
}
