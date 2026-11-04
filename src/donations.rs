//! Donations and Shops integration for Streamerz (Phase 13):
//! - Unified webhook and event dispatch for 12 donation/shop platforms:
//!   Ko-fi, StreamElements, Streamlabs, Patreon, Fourthwall, Throne,
//!   TipeeeStream, TreatStream, DonorDrive, Pally.gg, Shopify, Streamloots.
//! - Webhook routes served over the local HTTP server (`/api/<platform>` or `/<platform>`).
//! - Opt-in background clients for socket/poll-based platforms (StreamElements, Streamlabs, DonorDrive).
//! - 48 Triggers covering every donation and shop event.

use crate::engine::Engine;
use crate::model::Obj;
use futures_util::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::cell::Cell;
use std::rc::Rc;
use std::time::Duration;
use tokio_tungstenite::tungstenite::Message;

// 1. Ko-fi Triggers (7001..=7005)
pub const T_KOFI_DONATION: u32 = 7001;
pub const T_KOFI_SUBSCRIPTION: u32 = 7002;
pub const T_KOFI_RESUBSCRIPTION: u32 = 7003;
pub const T_KOFI_SHOP_ORDER: u32 = 7004;
pub const T_KOFI_COMMISSION: u32 = 7005;

// 2. StreamElements Triggers (1201..=1205)
pub const T_SE_TIP: u32 = 1201;
pub const T_SE_MERCH: u32 = 1202;
pub const T_SE_CONNECTED: u32 = 1203;
pub const T_SE_DISCONNECTED: u32 = 1204;
pub const T_SE_AUTHENTICATED: u32 = 1205;

// 3. Streamlabs Donations Triggers (201..=206)
pub const T_SL_DONATION: u32 = 201;
pub const T_SL_MERCHANDISE: u32 = 202;
pub const T_SL_CONNECTED: u32 = 203;
pub const T_SL_DISCONNECTED: u32 = 204;
pub const T_SL_CHARITY_DONATION: u32 = 205;
pub const T_SL_AUTHENTICATED: u32 = 206;

// 4. Patreon Triggers (8001..=8005)
pub const T_PATREON_FOLLOW_CREATED: u32 = 8001;
pub const T_PATREON_FOLLOW_DELETED: u32 = 8002;
pub const T_PATREON_PLEDGE_CREATED: u32 = 8003;
pub const T_PATREON_PLEDGE_UPDATED: u32 = 8004;
pub const T_PATREON_PLEDGE_DELETED: u32 = 8005;

// 5. Fourthwall Triggers (24001..=24013)
pub const T_FW_PRODUCT_CREATED: u32 = 24001;
pub const T_FW_PRODUCT_UPDATED: u32 = 24002;
pub const T_FW_GIFT_PURCHASE: u32 = 24003;
pub const T_FW_ORDER_PLACED: u32 = 24004;
pub const T_FW_ORDER_UPDATED: u32 = 24005;
pub const T_FW_DONATION: u32 = 24006;
pub const T_FW_SUBSCRIPTION_PURCHASED: u32 = 24007;
pub const T_FW_SUBSCRIPTION_EXPIRED: u32 = 24008;
pub const T_FW_SUBSCRIPTION_CHANGED: u32 = 24009;
pub const T_FW_THANK_YOU_SENT: u32 = 24010;
pub const T_FW_NEWSLETTER_SUBSCRIBED: u32 = 24011;
pub const T_FW_GIFT_DRAW_STARTED: u32 = 24012;
pub const T_FW_GIFT_DRAW_ENDED: u32 = 24013;

// 6. Throne Triggers (37001..=37003)
pub const T_THRONE_GIFT_PURCHASED: u32 = 37001;
pub const T_THRONE_CONTRIBUTION_PURCHASED: u32 = 37002;
pub const T_THRONE_GIFT_CROWDFUNDED: u32 = 37003;

// 7. TipeeeStream Trigger (10001)
pub const T_TIPEEESTREAM_DONATION: u32 = 10001;

// 8. TreatStream Triggers (11001..=11004)
pub const T_TREATSTREAM_TREAT: u32 = 11001;
pub const T_TREATSTREAM_AUTHENTICATED: u32 = 11002;
pub const T_TREATSTREAM_CONNECTED: u32 = 11003;
pub const T_TREATSTREAM_DISCONNECTED: u32 = 11004;

// 9. DonorDrive Triggers (3001..=3003)
pub const T_DONORDRIVE_DONATION: u32 = 3001;
pub const T_DONORDRIVE_PROFILE_UPDATED: u32 = 3002;
pub const T_DONORDRIVE_INCENTIVE: u32 = 3003;

// 10. Pally.gg Triggers (28001..=28003)
pub const T_PALLYGG_CONNECTED: u32 = 28001;
pub const T_PALLYGG_DISCONNECTED: u32 = 28002;
pub const T_PALLYGG_CAMPAIGN_TIP: u32 = 28003;

// 11. Shopify Triggers (13001..=13002)
pub const T_SHOPIFY_ORDER_CREATED: u32 = 13001;
pub const T_SHOPIFY_ORDER_PAID: u32 = 13002;

// 12. Streamloots Triggers (36001..=36004)
pub const T_STREAMLOOTS_PACK_PURCHASED: u32 = 36001;
pub const T_STREAMLOOTS_PACK_GIFTED: u32 = 36002;
pub const T_STREAMLOOTS_CARD_REDEEMED: u32 = 36003;
pub const T_STREAMLOOTS_REACTION: u32 = 36004;

#[derive(Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct DonationsConfig {
    pub kofi_token: String,
    pub streamelements_token: String,
    pub streamlabs_token: String,
    pub patreon_webhook_secret: String,
    pub donordrive_participant_id: String,
    pub donordrive_instance: String,
}

// ---------------------------------------------------------------------------
// Webhook & Event Dispatchers
// ---------------------------------------------------------------------------

pub fn handle_kofi(engine: &Rc<Engine>, data: &Value) {
    let raw_type = data["type"].as_str().unwrap_or("Donation");
    let from_name = data["from_name"].as_str().unwrap_or("Anonymous");
    let amount = data["amount"].as_str().unwrap_or("0");
    let currency = data["currency"].as_str().unwrap_or("USD");
    let message = data["message"].as_str().unwrap_or("");
    let is_first = data["is_first_subscription_payment"].as_bool().unwrap_or(false);

    let mut args = Obj::new();
    args.insert("user".into(), from_name.into());
    args.insert("fromName".into(), from_name.into());
    args.insert("amount".into(), amount.into());
    args.insert("currency".into(), currency.into());
    args.insert("message".into(), message.into());
    args.insert("type".into(), raw_type.into());

    match raw_type {
        "Subscription" => {
            if is_first {
                engine.fire(T_KOFI_SUBSCRIPTION, "KofiSubscription", "Kofi", "kofi", &args, |_| true);
                engine.emit("Kofi", "Subscription", || json!(args));
            } else {
                engine.fire(
                    T_KOFI_RESUBSCRIPTION,
                    "KofiResubscription",
                    "Kofi",
                    "kofi",
                    &args,
                    |_| true,
                );
                engine.emit("Kofi", "Resubscription", || json!(args));
            }
        }
        "Shop Order" => {
            engine.fire(T_KOFI_SHOP_ORDER, "KofiShopOrder", "Kofi", "kofi", &args, |_| true);
            engine.emit("Kofi", "ShopOrder", || json!(args));
        }
        "Commission" => {
            engine.fire(T_KOFI_COMMISSION, "KofiCommission", "Kofi", "kofi", &args, |_| true);
            engine.emit("Kofi", "Commission", || json!(args));
        }
        _ => {
            engine.fire(T_KOFI_DONATION, "KofiDonation", "Kofi", "kofi", &args, |_| true);
            engine.emit("Kofi", "Donation", || json!(args));
        }
    }
}

fn text(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        Value::Null => String::new(),
        v => v.to_string(),
    }
}

/// A number sent as a number or as text ("13.37").
fn number(v: &Value) -> f64 {
    v.as_f64()
        .or_else(|| v.as_str().and_then(|s| s.trim().parse().ok()))
        .unwrap_or(0.0)
}

/// A trigger's Range: none (any), only Max (exactly it), only Min (at least it), or both.
fn in_range(t: &Obj, v: f64) -> bool {
    let bound = |k: &str| t.get(k).and_then(Value::as_f64).filter(|x| *x > 0.0);
    match (bound("min"), bound("max")) {
        (None, None) => true,
        (None, Some(max)) => v == max,
        (Some(min), None) => v >= min,
        (Some(min), Some(max)) => (min..=max).contains(&v),
    }
}

fn se_fire(engine: &Rc<Engine>, code: u32, name: &str, args: &Obj, amount: Option<f64>) {
    engine.fire(code, name, "StreamElements", "streamelements", args, |t| {
        amount.is_none_or(|a| in_range(t, a))
    });
    engine.emit("StreamElements", name, || json!(args));
}

/// A StreamElements event: its type ("tip", "merch", "connected", ...) and its `data`.
pub fn handle_streamelements(engine: &Rc<Engine>, event: &str, d: &Value) {
    let mut args = Obj::new();
    match event {
        "connected" => se_fire(engine, T_SE_CONNECTED, "Connected", &args, None),
        "disconnected" => se_fire(engine, T_SE_DISCONNECTED, "Disconnected", &args, None),
        "authenticated" => se_fire(engine, T_SE_AUTHENTICATED, "Authenticated", &args, None),
        "merch" | "merchandise" => {
            let items = d["items"].as_array().cloned().unwrap_or_default();
            let mut quantity = 0;
            for (i, it) in items.iter().enumerate() {
                args.insert(format!("merchItem{i}.name"), text(&it["name"]).into());
                args.insert(format!("merchItem{i}.price"), number(&it["price"]).into());
                let q = it["quantity"].as_u64().unwrap_or(1);
                args.insert(format!("merchItem{i}.quantity"), q.into());
                quantity += q;
            }
            let quantity = d["quantity"].as_u64().unwrap_or(quantity);
            for (k, v) in [
                ("merchUsername", text(&d["username"]).into()),
                ("merchAvatar", text(&d["avatar"]).into()),
                ("merchAmount", number(&d["amount"]).into()),
                ("merchCurrency", text(&d["currency"]).into()),
                ("merchMessage", text(&d["message"]).into()),
                ("merchQuantity", quantity.into()),
                ("merchItems", items.len().into()),
            ] {
                args.insert(k.into(), v);
            }
            se_fire(engine, T_SE_MERCH, "Merch", &args, None);
        }
        _ => {
            let amount = number(&d["amount"]);
            for (k, v) in [
                ("tipUsername", text(&d["username"]).into()),
                ("tipAvatar", text(&d["avatar"]).into()),
                ("tipAmount", amount.into()),
                ("tipCurrency", text(&d["currency"]).into()),
                ("tipMessage", text(&d["message"]).into()),
            ] {
                args.insert(k.into(), v);
            }
            se_fire(engine, T_SE_TIP, "Tip", &args, Some(amount));
        }
    }
}

fn sl_fire(engine: &Rc<Engine>, code: u32, name: &str, args: &Obj, amount: Option<f64>) {
    engine.fire(code, name, "Streamlabs", "streamlabs", args, |t| {
        amount.is_none_or(|a| in_range(t, a))
    });
    engine.emit("Streamlabs", name, || json!(args));
}

/// A Streamlabs event: its type ("donation", "merch", "streamlabscharitydonation", "connected", ...)
/// and one item of its `message` list.
pub fn handle_streamlabs(engine: &Rc<Engine>, event: &str, m: &Value) {
    let mut args = Obj::new();
    let from = text(if m["from"].is_null() { &m["name"] } else { &m["from"] });
    let formatted = text(if m["formatted_amount"].is_null() {
        &m["formattedAmount"]
    } else {
        &m["formatted_amount"]
    });
    match event {
        "connected" => sl_fire(engine, T_SL_CONNECTED, "Connected", &args, None),
        "disconnected" => sl_fire(engine, T_SL_DISCONNECTED, "Disconnected", &args, None),
        "authenticated" => sl_fire(engine, T_SL_AUTHENTICATED, "Authenticated", &args, None),
        "merch" | "merchandise" => {
            let image = text(if m["imageHref"].is_null() {
                &m["image_href"]
            } else {
                &m["imageHref"]
            });
            for (k, v) in [
                ("merchandiseFrom", from),
                ("merchandiseMessage", text(&m["message"])),
                ("merchandiseProduct", text(&m["product"])),
                ("merchandiseImageEscaped", crate::commands::url_encode(&image)),
                ("merchandiseImageUrl", image),
            ] {
                args.insert(k.into(), v.into());
            }
            sl_fire(engine, T_SL_MERCHANDISE, "Merchandise", &args, None);
        }
        "streamlabscharitydonation" | "charity" => {
            let amount = number(&m["amount"]);
            let at = if m["createdAt"].is_null() {
                &m["created_at"]
            } else {
                &m["createdAt"]
            };
            for (k, v) in [
                ("charityDonationFrom", from.into()),
                ("charityDonationAmount", amount.into()),
                ("charityDonationCurrency", text(&m["currency"]).into()),
                ("charityDonationFormattedAmount", formatted.into()),
                ("charityDonationMessage", text(&m["message"]).into()),
                ("charityDonationGif", text(&m["gif"]).into()),
                ("charityDonationIconClassName", text(&m["iconClassName"]).into()),
                ("charityDonationCreatedAt", text(at).into()),
            ] {
                args.insert(k.into(), v);
            }
            sl_fire(engine, T_SL_CHARITY_DONATION, "CharityDonation", &args, Some(amount));
        }
        _ => {
            let amount = number(&m["amount"]);
            for (k, v) in [
                ("donationFrom", from.into()),
                ("donationAmount", amount.into()),
                ("donationCurrency", text(&m["currency"]).into()),
                ("donationFormattedAmount", formatted.into()),
                ("donationMessage", text(&m["message"]).into()),
            ] {
                args.insert(k.into(), v);
            }
            sl_fire(engine, T_SL_DONATION, "Donation", &args, Some(amount));
        }
    }
}

pub fn handle_patreon(engine: &Rc<Engine>, event: &str, data: &Value) {
    let mut args = Obj::new();
    let user = data["fullName"]
        .as_str()
        .or_else(|| data["name"].as_str())
        .unwrap_or("Anonymous");
    let amount = data["amountCents"]
        .as_i64()
        .or_else(|| data["amount"].as_i64())
        .unwrap_or(0);
    let tier = data["tier"].as_str().unwrap_or("");

    args.insert("user".into(), user.into());
    args.insert("fullName".into(), user.into());
    args.insert("amountCents".into(), amount.into());
    args.insert("tier".into(), tier.into());

    match event {
        "pledges:create" | "members:pledge:create" | "PledgeCreated" => {
            engine.fire(
                T_PATREON_PLEDGE_CREATED,
                "PatreonPledgeCreated",
                "Patreon",
                "patreon",
                &args,
                |_| true,
            );
            engine.emit("Patreon", "PledgeCreated", || json!(args));
        }
        "pledges:update" | "members:pledge:update" | "PledgeUpdated" => {
            engine.fire(
                T_PATREON_PLEDGE_UPDATED,
                "PatreonPledgeUpdated",
                "Patreon",
                "patreon",
                &args,
                |_| true,
            );
            engine.emit("Patreon", "PledgeUpdated", || json!(args));
        }
        "pledges:delete" | "members:pledge:delete" | "PledgeDeleted" => {
            engine.fire(
                T_PATREON_PLEDGE_DELETED,
                "PatreonPledgeDeleted",
                "Patreon",
                "patreon",
                &args,
                |_| true,
            );
            engine.emit("Patreon", "PledgeDeleted", || json!(args));
        }
        "members:follow:delete" | "FollowDeleted" => {
            engine.fire(
                T_PATREON_FOLLOW_DELETED,
                "PatreonFollowDeleted",
                "Patreon",
                "patreon",
                &args,
                |_| true,
            );
            engine.emit("Patreon", "FollowDeleted", || json!(args));
        }
        _ => {
            engine.fire(
                T_PATREON_FOLLOW_CREATED,
                "PatreonFollowCreated",
                "Patreon",
                "patreon",
                &args,
                |_| true,
            );
            engine.emit("Patreon", "FollowCreated", || json!(args));
        }
    }
}

/// `v` flattened into `args` under `prefix` (`fw.offer.name`, `shopify.line_items[0].title`), as
/// Streamer.bot passes webhook payloads.
fn prefixed(args: &mut Obj, prefix: &str, v: &Value) {
    match v {
        Value::Object(o) => o.iter().for_each(|(k, x)| prefixed(args, &format!("{prefix}.{k}"), x)),
        Value::Array(a) => a
            .iter()
            .enumerate()
            .for_each(|(i, x)| prefixed(args, &format!("{prefix}[{i}]"), x)),
        x => drop(args.insert(prefix.to_string(), x.clone())),
    }
}

/// A webhook's payload: its `data` object when it has one, else itself.
fn payload(data: &Value) -> &Value {
    data.get("data").filter(|d| d.is_object()).unwrap_or(data)
}

pub fn handle_fourthwall(engine: &Rc<Engine>, event: &str, data: &Value) {
    let mut args = Obj::new();
    for (k, v) in data.as_object().into_iter().flatten() {
        args.insert(k.clone(), v.clone());
    }
    // Streamer.bot's fw.* names: the payload's data, with the shop and time from around it.
    let body = payload(data);
    prefixed(&mut args, "fw", body);
    for k in ["shopId", "createdAt"] {
        if !args.contains_key(&format!("fw.{k}")) {
            if let Some(v) = data.get(k) {
                args.insert(format!("fw.{k}"), v.clone());
            }
        }
    }
    if event.eq_ignore_ascii_case("donation") {
        args.insert("fw.donationId".into(), body["id"].clone());
    }

    // Fourthwall sends DONATION, ORDER_PLACED...; older or hand-made ones order:placed, OrderPlaced.
    let key: String = event
        .chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .collect::<String>()
        .to_lowercase();
    let event = [
        "ProductCreated",
        "ProductUpdated",
        "GiftPurchase",
        "OrderPlaced",
        "OrderUpdated",
        "Donation",
        "SubscriptionPurchased",
        "SubscriptionExpired",
        "SubscriptionChanged",
        "ThankYouSent",
        "NewsletterSubscribed",
        "GiftDrawStarted",
        "GiftDrawEnded",
    ]
    .into_iter()
    .find(|c| c.to_lowercase() == key)
    .unwrap_or(event);
    match event {
        "product:created" | "ProductCreated" => {
            engine.fire(
                T_FW_PRODUCT_CREATED,
                "FourthwallProductCreated",
                "Fourthwall",
                "fourthwall",
                &args,
                |_| true,
            );
            engine.emit("Fourthwall", "ProductCreated", || json!(args));
        }
        "product:updated" | "ProductUpdated" => {
            engine.fire(
                T_FW_PRODUCT_UPDATED,
                "FourthwallProductUpdated",
                "Fourthwall",
                "fourthwall",
                &args,
                |_| true,
            );
            engine.emit("Fourthwall", "ProductUpdated", || json!(args));
        }
        "gift:purchase" | "GiftPurchase" => {
            engine.fire(
                T_FW_GIFT_PURCHASE,
                "FourthwallGiftPurchase",
                "Fourthwall",
                "fourthwall",
                &args,
                |_| true,
            );
            engine.emit("Fourthwall", "GiftPurchase", || json!(args));
        }
        "order:placed" | "OrderPlaced" => {
            engine.fire(
                T_FW_ORDER_PLACED,
                "FourthwallOrderPlaced",
                "Fourthwall",
                "fourthwall",
                &args,
                |_| true,
            );
            engine.emit("Fourthwall", "OrderPlaced", || json!(args));
        }
        "order:updated" | "OrderUpdated" => {
            engine.fire(
                T_FW_ORDER_UPDATED,
                "FourthwallOrderUpdated",
                "Fourthwall",
                "fourthwall",
                &args,
                |_| true,
            );
            engine.emit("Fourthwall", "OrderUpdated", || json!(args));
        }
        "donation" | "Donation" => {
            engine.fire(
                T_FW_DONATION,
                "FourthwallDonation",
                "Fourthwall",
                "fourthwall",
                &args,
                |_| true,
            );
            engine.emit("Fourthwall", "Donation", || json!(args));
        }
        "subscription:purchased" | "SubscriptionPurchased" => {
            engine.fire(
                T_FW_SUBSCRIPTION_PURCHASED,
                "FourthwallSubscriptionPurchased",
                "Fourthwall",
                "fourthwall",
                &args,
                |_| true,
            );
            engine.emit("Fourthwall", "SubscriptionPurchased", || json!(args));
        }
        "subscription:expired" | "SubscriptionExpired" => {
            engine.fire(
                T_FW_SUBSCRIPTION_EXPIRED,
                "FourthwallSubscriptionExpired",
                "Fourthwall",
                "fourthwall",
                &args,
                |_| true,
            );
            engine.emit("Fourthwall", "SubscriptionExpired", || json!(args));
        }
        "subscription:changed" | "SubscriptionChanged" => {
            engine.fire(
                T_FW_SUBSCRIPTION_CHANGED,
                "FourthwallSubscriptionChanged",
                "Fourthwall",
                "fourthwall",
                &args,
                |_| true,
            );
            engine.emit("Fourthwall", "SubscriptionChanged", || json!(args));
        }
        "thank_you:sent" | "ThankYouSent" => {
            engine.fire(
                T_FW_THANK_YOU_SENT,
                "FourthwallThankYouSent",
                "Fourthwall",
                "fourthwall",
                &args,
                |_| true,
            );
            engine.emit("Fourthwall", "ThankYouSent", || json!(args));
        }
        "newsletter:subscribed" | "NewsletterSubscribed" => {
            engine.fire(
                T_FW_NEWSLETTER_SUBSCRIBED,
                "FourthwallNewsletterSubscribed",
                "Fourthwall",
                "fourthwall",
                &args,
                |_| true,
            );
            engine.emit("Fourthwall", "NewsletterSubscribed", || json!(args));
        }
        "gift_draw:started" | "GiftDrawStarted" => {
            engine.fire(
                T_FW_GIFT_DRAW_STARTED,
                "FourthwallGiftDrawStarted",
                "Fourthwall",
                "fourthwall",
                &args,
                |_| true,
            );
            engine.emit("Fourthwall", "GiftDrawStarted", || json!(args));
        }
        "gift_draw:ended" | "GiftDrawEnded" => {
            engine.fire(
                T_FW_GIFT_DRAW_ENDED,
                "FourthwallGiftDrawEnded",
                "Fourthwall",
                "fourthwall",
                &args,
                |_| true,
            );
            engine.emit("Fourthwall", "GiftDrawEnded", || json!(args));
        }
        _ => {}
    }
}

pub fn handle_throne(engine: &Rc<Engine>, event: &str, data: &Value) {
    let mut args = Obj::new();
    for (k, v) in data.as_object().into_iter().flatten() {
        args.insert(k.clone(), v.clone());
    }
    prefixed(&mut args, "throne", payload(data));

    match event {
        "contribution:purchased" | "ContributionPurchased" => {
            engine.fire(
                T_THRONE_CONTRIBUTION_PURCHASED,
                "ThroneContributionPurchased",
                "Throne",
                "throne",
                &args,
                |_| true,
            );
            engine.emit("Throne", "ContributionPurchased", || json!(args));
        }
        "gift:crowdfunded" | "GiftCrowdfunded" => {
            engine.fire(
                T_THRONE_GIFT_CROWDFUNDED,
                "ThroneGiftCrowdfunded",
                "Throne",
                "throne",
                &args,
                |_| true,
            );
            engine.emit("Throne", "GiftCrowdfunded", || json!(args));
        }
        _ => {
            engine.fire(
                T_THRONE_GIFT_PURCHASED,
                "ThroneGiftPurchased",
                "Throne",
                "throne",
                &args,
                |_| true,
            );
            engine.emit("Throne", "GiftPurchased", || json!(args));
        }
    }
}

pub fn handle_tipeeestream(engine: &Rc<Engine>, data: &Value) {
    let mut args = Obj::new();
    let username = data["username"].as_str().unwrap_or("Anonymous");
    let amount = data["amount"].as_f64().unwrap_or(0.0);
    let currency = data["currency"].as_str().unwrap_or("EUR");
    let message = data["message"].as_str().unwrap_or("");

    args.insert("user".into(), username.into());
    args.insert("amount".into(), amount.into());
    args.insert("currency".into(), currency.into());
    args.insert("message".into(), message.into());

    engine.fire(
        T_TIPEEESTREAM_DONATION,
        "TipeeeStreamDonation",
        "TipeeeStream",
        "tipeeestream",
        &args,
        |_| true,
    );
    engine.emit("TipeeeStream", "Donation", || json!(args));
}

pub fn handle_treatstream(engine: &Rc<Engine>, data: &Value) {
    let mut args = Obj::new();
    let treat_name = data["treat"].as_str().unwrap_or("");
    let sender = data["sender"].as_str().unwrap_or("Anonymous");
    let message = data["message"].as_str().unwrap_or("");

    args.insert("treat".into(), treat_name.into());
    args.insert("user".into(), sender.into());
    args.insert("sender".into(), sender.into());
    args.insert("message".into(), message.into());

    engine.fire(
        T_TREATSTREAM_TREAT,
        "TreatStreamTreat",
        "TreatStream",
        "treatstream",
        &args,
        |_| true,
    );
    engine.emit("TreatStream", "Treat", || json!(args));
}

/// DonorDrive's profile variables, from a participant (or team) object and its recent donations.
fn dd_profile(p: &Value, donations: &Value) -> Obj {
    let mut a = Obj::new();
    let team = p["participantID"].is_null() && !p["teamID"].is_null();
    for (k, v) in [
        ("name", text(&p["displayName"]).into()),
        ("teamName", text(if team { &p["name"] } else { &p["teamName"] }).into()),
        ("eventName", text(&p["eventName"]).into()),
        ("goal", number(&p["fundraisingGoal"]).into()),
        ("donations", donations.clone()),
        ("raised", number(&p["sumDonations"]).into()),
        ("isTeam", team.into()),
        ("isParticipant", (!team).into()),
    ] {
        a.insert(k.into(), v);
    }
    a
}

/// A DonorDrive event: "ProfileUpdated" (`data` is the profile), "Donation" or "Incentive" (`data`
/// is the donation, with the profile's variables in `profile` and the incentive in `incentive`).
pub fn handle_donordrive(engine: &Rc<Engine>, event: &str, data: &Value) {
    let profile = dd_profile(&data["profile"], &data["profile"]["donations"]);
    let fire = |code: u32, name: &str, args: &Obj| {
        engine.fire(code, name, "DonorDrive", "donordrive", args, |_| true);
        engine.emit("DonorDrive", name, || json!(args));
    };
    if event.eq_ignore_ascii_case("profileupdated") || event == "profile" {
        return fire(
            T_DONORDRIVE_PROFILE_UPDATED,
            "ProfileUpdated",
            &dd_profile(data, &data["donations"]),
        );
    }
    let name = text(&data["displayName"]);
    let mut args = Obj::new();
    for (k, v) in [
        ("donorIsAnonymous", name.is_empty().into()),
        (
            "donorName",
            if name.is_empty() { "Anonymous".into() } else { name }.into(),
        ),
        ("donorAvatarUrl", text(&data["avatarImageURL"]).into()),
        ("recipientName", text(&data["recipientName"]).into()),
        ("donorMessage", text(&data["message"]).into()),
        ("donorAmount", number(&data["amount"]).into()),
        ("profileName", profile["name"].clone()),
        ("profileTeamName", profile["teamName"].clone()),
    ] {
        args.insert(k.into(), v);
    }
    for k in ["isTeam", "isParticipant", "eventName", "goal", "raised", "donations"] {
        args.insert(k.into(), profile[k].clone());
    }
    let inc = &data["incentive"];
    if !event.eq_ignore_ascii_case("incentive") || inc.is_null() {
        return fire(T_DONORDRIVE_DONATION, "Donation", &args);
    }
    args.remove("donations");
    args.insert("isIncentive".into(), true.into());
    for (k, v) in [
        ("id", text(&inc["incentiveID"]).into()),
        ("isActive", inc["isActive"].as_bool().unwrap_or(false).into()),
        ("amount", number(&inc["amount"]).into()),
        ("description", text(&inc["description"]).into()),
        ("image", text(&inc["incentiveImageURL"]).into()),
        ("quantity", inc["quantity"].as_i64().unwrap_or(-1).into()),
        ("quantityClaimed", inc["quantityClaimed"].as_i64().unwrap_or(0).into()),
        ("links.donate", text(&inc["links"]["donate"]).into()),
        ("startDate", text(&inc["startDateUTC"]).into()),
        ("endDate", text(&inc["endDateUTC"]).into()),
    ] {
        args.insert(format!("incentive.{k}"), v);
    }
    fire(T_DONORDRIVE_INCENTIVE, "Incentive", &args);
}

pub fn handle_pallygg(engine: &Rc<Engine>, event: &str, data: &Value) {
    let mut args = Obj::new();
    for (k, v) in data.as_object().into_iter().flatten() {
        args.insert(k.clone(), v.clone());
    }
    // Streamer.bot's names for a tip: the campaign tip and its page.
    let tip = data
        .pointer("/payload/campaignTip")
        .or(data.get("campaignTip"))
        .cloned()
        .unwrap_or_default();
    let page = data
        .pointer("/payload/page")
        .or(data.get("page"))
        .cloned()
        .unwrap_or_default();
    if tip.is_object() {
        let cap = |k: &str| format!("{}{}", k[..1].to_uppercase(), &k[1..]);
        for (k, v) in tip.as_object().into_iter().flatten() {
            args.insert(format!("tip{}", cap(k)), v.clone());
        }
        args.insert("createdAt".into(), tip["createdAt"].clone());
        for (k, f) in [
            ("campaignId", "id"),
            ("campaignTitle", "title"),
            ("campaignSlug", "slug"),
            ("campaignUrl", "url"),
        ] {
            args.insert(k.into(), page[f].clone());
        }
    }

    match event.to_ascii_lowercase().as_str() {
        "connected" => {
            engine.fire(
                T_PALLYGG_CONNECTED,
                "PallyggConnected",
                "Pallygg",
                "pallygg",
                &args,
                |_| true,
            );
            engine.emit("Pallygg", "Connected", || json!(args));
        }
        "disconnected" => {
            engine.fire(
                T_PALLYGG_DISCONNECTED,
                "PallyggDisconnected",
                "Pallygg",
                "pallygg",
                &args,
                |_| true,
            );
            engine.emit("Pallygg", "Disconnected", || json!(args));
        }
        _ => {
            engine.fire(
                T_PALLYGG_CAMPAIGN_TIP,
                "PallyggCampaignTip",
                "Pallygg",
                "pallygg",
                &args,
                |_| true,
            );
            engine.emit("Pallygg", "CampaignTip", || json!(args));
        }
    }
}

pub fn handle_shopify(engine: &Rc<Engine>, topic: &str, data: &Value) {
    let mut args = Obj::new();
    for (k, v) in data.as_object().into_iter().flatten() {
        args.insert(k.clone(), v.clone());
    }
    prefixed(&mut args, "shopify", data);
    args.insert("shopify.event".into(), topic.into());

    match topic {
        "orders/paid" | "OrderPaid" => {
            engine.fire(
                T_SHOPIFY_ORDER_PAID,
                "ShopifyOrderPaid",
                "Shopify",
                "shopify",
                &args,
                |_| true,
            );
            engine.emit("Shopify", "OrderPaid", || json!(args));
        }
        _ => {
            engine.fire(
                T_SHOPIFY_ORDER_CREATED,
                "ShopifyOrderCreated",
                "Shopify",
                "shopify",
                &args,
                |_| true,
            );
            engine.emit("Shopify", "OrderCreated", || json!(args));
        }
    }
}

pub fn handle_streamloots(engine: &Rc<Engine>, event: &str, data: &Value) {
    let mut args = Obj::new();
    for (k, v) in data.as_object().into_iter().flatten() {
        args.insert(k.clone(), v.clone());
    }

    match event {
        "pack:gifted" | "PackGifted" => {
            engine.fire(
                T_STREAMLOOTS_PACK_GIFTED,
                "StreamLootsPackGifted",
                "StreamLoots",
                "streamloots",
                &args,
                |_| true,
            );
            engine.emit("StreamLoots", "PackGifted", || json!(args));
        }
        "card:redeemed" | "CardRedeemed" => {
            engine.fire(
                T_STREAMLOOTS_CARD_REDEEMED,
                "StreamLootsCardRedeemed",
                "StreamLoots",
                "streamloots",
                &args,
                |_| true,
            );
            engine.emit("StreamLoots", "CardRedeemed", || json!(args));
        }
        "reaction" | "Reaction" => {
            engine.fire(
                T_STREAMLOOTS_REACTION,
                "StreamLootsReaction",
                "StreamLoots",
                "streamloots",
                &args,
                |_| true,
            );
            engine.emit("StreamLoots", "Reaction", || json!(args));
        }
        _ => {
            engine.fire(
                T_STREAMLOOTS_PACK_PURCHASED,
                "StreamLootsPackPurchased",
                "StreamLoots",
                "streamloots",
                &args,
                |_| true,
            );
            engine.emit("StreamLoots", "PackPurchased", || json!(args));
        }
    }
}

/// Unified entry point for incoming webhook requests from any donation / shop service.
/// Whether a webhook really comes from its service, where the service signs it and a secret is set:
/// Ko-fi puts its verification token in the payload, Patreon an HMAC-MD5 of the body in
/// `X-Patreon-Signature`. Without the secret set, it's let through (and the log says to set it).
fn authentic(
    cfg: &DonationsConfig,
    platform: &str,
    body: &[u8],
    data: &Value,
    signature: Option<&str>,
) -> Result<(), String> {
    match platform {
        "kofi" | "ko-fi" => {
            let want = cfg.kofi_token.trim();
            if want.is_empty() {
                crate::warn("Ko-fi: webhook taken without a check: set its verification token in Donations".into());
                return Ok(());
            }
            if data["verification_token"].as_str().map(str::trim) == Some(want) {
                Ok(())
            } else {
                Err("Ko-fi: wrong verification token".into())
            }
        }
        "patreon" => {
            let secret = cfg.patreon_webhook_secret.trim();
            if secret.is_empty() {
                crate::warn("Patreon: webhook taken without a check: set its secret in Donations".into());
                return Ok(());
            }
            let want: String = hmac_md5(secret.as_bytes(), body)
                .iter()
                .map(|b| format!("{b:02x}"))
                .collect();
            if signature.is_some_and(|s| s.trim().eq_ignore_ascii_case(&want)) {
                Ok(())
            } else {
                Err("Patreon: wrong signature".into())
            }
        }
        _ => Ok(()),
    }
}

/// MD5 (RFC 1321): only for checking Patreon's HMAC-MD5 webhook signatures.
fn md5(data: &[u8]) -> [u8; 16] {
    const S: [u32; 64] = [
        7, 12, 17, 22, 7, 12, 17, 22, 7, 12, 17, 22, 7, 12, 17, 22, 5, 9, 14, 20, 5, 9, 14, 20, 5, 9, 14, 20, 5, 9, 14,
        20, 4, 11, 16, 23, 4, 11, 16, 23, 4, 11, 16, 23, 4, 11, 16, 23, 6, 10, 15, 21, 6, 10, 15, 21, 6, 10, 15, 21, 6,
        10, 15, 21,
    ];
    let k: Vec<u32> = (0..64)
        .map(|i| ((i as f64 + 1.0).sin().abs() * 4_294_967_296.0) as u32)
        .collect();
    let mut msg = data.to_vec();
    msg.push(0x80);
    while msg.len() % 64 != 56 {
        msg.push(0);
    }
    msg.extend_from_slice(&((data.len() as u64).wrapping_mul(8)).to_le_bytes());
    let mut h: [u32; 4] = [0x6745_2301, 0xefcd_ab89, 0x98ba_dcfe, 0x1032_5476];
    for chunk in msg.chunks(64) {
        let m: Vec<u32> = chunk
            .chunks(4)
            .map(|w| u32::from_le_bytes([w[0], w[1], w[2], w[3]]))
            .collect();
        let [mut a, mut b, mut c, mut d] = h;
        for i in 0..64 {
            let (f, g) = match i / 16 {
                0 => ((b & c) | (!b & d), i),
                1 => ((d & b) | (!d & c), (5 * i + 1) % 16),
                2 => (b ^ c ^ d, (3 * i + 5) % 16),
                _ => (c ^ (b | !d), (7 * i) % 16),
            };
            let f = f.wrapping_add(a).wrapping_add(k[i]).wrapping_add(m[g]);
            a = d;
            d = c;
            c = b;
            b = b.wrapping_add(f.rotate_left(S[i]));
        }
        for (x, y) in h.iter_mut().zip([a, b, c, d]) {
            *x = x.wrapping_add(y);
        }
    }
    let mut out = [0u8; 16];
    for (i, w) in h.iter().enumerate() {
        out[i * 4..i * 4 + 4].copy_from_slice(&w.to_le_bytes());
    }
    out
}

fn hmac_md5(key: &[u8], data: &[u8]) -> [u8; 16] {
    let mut k = [0u8; 64];
    if key.len() > 64 {
        k[..16].copy_from_slice(&md5(key));
    } else {
        k[..key.len()].copy_from_slice(key);
    }
    let mut inner: Vec<u8> = k.iter().map(|b| b ^ 0x36).collect();
    inner.extend_from_slice(data);
    let mut outer: Vec<u8> = k.iter().map(|b| b ^ 0x5c).collect();
    outer.extend_from_slice(&md5(&inner));
    md5(&outer)
}

pub fn handle_webhook(engine: &Rc<Engine>, platform: &str, body: &[u8], signature: Option<&str>) -> Result<(), String> {
    let data: Value = serde_json::from_slice(body).or_else(|_| {
        // Try parsing as urlencoded form data (e.g. data={"amount":"10.00",...})
        let s = std::str::from_utf8(body).map_err(|e| e.to_string())?;
        if let Some((_, json_str)) = s.split_once("data=") {
            let decoded = urlencoding_decode(json_str);
            serde_json::from_str(&decoded).map_err(|e| e.to_string())
        } else {
            Err("Unable to parse payload".into())
        }
    })?;
    let platform = platform.to_lowercase();
    authentic(&engine.config().donations, &platform, body, &data, signature)?;

    match platform.as_str() {
        "kofi" | "ko-fi" => handle_kofi(engine, &data),
        "streamelements" => {
            let evt = data["type"].as_str().unwrap_or("tip");
            handle_streamelements(engine, evt, &data);
        }
        "streamlabs" => {
            let evt = data["type"].as_str().unwrap_or("donation");
            handle_streamlabs(engine, evt, &data);
        }
        "patreon" => {
            let evt = data["event"]
                .as_str()
                .or_else(|| data["type"].as_str())
                .unwrap_or("PledgeCreated");
            handle_patreon(engine, evt, &data);
        }
        "fourthwall" => {
            let evt = data["type"]
                .as_str()
                .or_else(|| data["event"].as_str())
                .unwrap_or("Donation");
            handle_fourthwall(engine, evt, &data);
        }
        "throne" => {
            let evt = data["type"].as_str().unwrap_or("GiftPurchased");
            handle_throne(engine, evt, &data);
        }
        "tipeeestream" => handle_tipeeestream(engine, &data),
        "treatstream" => handle_treatstream(engine, &data),
        "donordrive" => {
            let evt = data["type"].as_str().unwrap_or("Donation");
            handle_donordrive(engine, evt, &data);
        }
        "pallygg" | "pally" => {
            let evt = data["type"].as_str().unwrap_or("CampaignTip");
            handle_pallygg(engine, evt, &data);
        }
        "shopify" => {
            let evt = data["topic"].as_str().unwrap_or("orders/create");
            handle_shopify(engine, evt, &data);
        }
        "streamloots" => {
            let evt = data["type"]
                .as_str()
                .or_else(|| data["event"].as_str())
                .unwrap_or("PackPurchased");
            handle_streamloots(engine, evt, &data);
        }
        _ => return Err(format!("Unknown donation platform '{platform}'")),
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// Live connections: StreamElements and Streamlabs over socket.io, DonorDrive polled
// ---------------------------------------------------------------------------

const SE_URL: &str = "wss://realtime.streamelements.com/socket.io/?EIO=3&transport=websocket";

/// The running connections: each only while its token (or participant) is set.
#[derive(Default)]
pub struct Live {
    cfg: Option<DonationsConfig>,
    tasks: Vec<crate::emote_events::Abort>,
    pub streamelements: Rc<Cell<bool>>,
    pub streamlabs: Rc<Cell<bool>>,
}

/// One socket.io session (Engine.IO 3, what both services speak): `hello` is sent once connected,
/// `on` gets each event's name and first argument and returns Some(why) to stop. Returns why it ended.
async fn socket_io(url: &str, hello: Option<String>, mut on: impl FnMut(&str, &Value) -> Option<String>) -> String {
    let mut ws = match crate::http::ws_connect(url).await {
        Ok((ws, _)) => ws,
        Err(e) => return e.to_string(),
    };
    // The server's ping interval; the client pings (Engine.IO 3).
    let mut ping = tokio::time::interval(Duration::from_secs(25));
    ping.reset();
    loop {
        let m = tokio::select! {
            _ = ping.tick() => {
                if ws.send(Message::Text("2".into())).await.is_err() {
                    return "connection lost".into();
                }
                continue;
            }
            m = ws.next() => m,
        };
        let t = match m {
            Some(Ok(Message::Text(t))) => t,
            Some(Ok(Message::Close(_))) | None | Some(Err(_)) => return "connection closed".into(),
            Some(Ok(_)) => continue,
        };
        if let Some(open) = t.strip_prefix('0') {
            let v: Value = serde_json::from_str(open).unwrap_or_default();
            let every = Duration::from_millis(v["pingInterval"].as_u64().unwrap_or(25000).max(1000));
            ping = tokio::time::interval_at(tokio::time::Instant::now() + every, every);
        } else if t.as_str() == "40" {
            if let Some(h) = &hello {
                let _ = ws.send(Message::Text(h.clone().into())).await;
            }
            if let Some(why) = on("connect", &Value::Null) {
                return why;
            }
        } else if let Some(err) = t.strip_prefix("44") {
            return format!("refused: {}", text(&serde_json::from_str(err).unwrap_or_default()));
        } else if let Some(ev) = t.strip_prefix("42") {
            let v: Value = serde_json::from_str(ev).unwrap_or_default();
            if let Some(why) = on(v[0].as_str().unwrap_or(""), &v[1]) {
                return why;
            }
        } else if t.as_str() == "41" || t.as_str() == "1" {
            return "disconnected by the server".into();
        }
    }
}

/// Keep reconnecting a socket.io service, a minute apart; the Connected and Disconnected triggers
/// follow the connection.
async fn keep(
    e: Rc<Engine>,
    who: &'static str,
    url: String,
    hello: Option<String>,
    up: Rc<Cell<bool>>,
    handle: fn(&Rc<Engine>, &str, &Value),
) {
    loop {
        let why = socket_io(&url, hello.clone(), |name, d| {
            match (who, name) {
                // Streamlabs checks the token before it lets the socket connect; StreamElements after.
                ("Streamlabs", "connect") | ("StreamElements", "authenticated") => {
                    up.set(true);
                    crate::say(format!("{who}: connected"));
                    handle(&e, "connected", &Value::Null);
                    handle(&e, "authenticated", &Value::Null);
                }
                ("StreamElements", "unauthorized") => return Some("the JWT token was refused".into()),
                ("StreamElements", "event") => handle(&e, d["type"].as_str().unwrap_or(""), &d["data"]),
                // The dashboard's Emulate buttons.
                ("StreamElements", "event:test") => {
                    let ev = &d["event"];
                    let data = json!({"username": ev["name"], "amount": ev["amount"], "message": ev["message"],
                        "avatar": ev["avatar"], "items": ev["items"], "quantity": ev["quantity"], "currency": ev["currency"]});
                    match d["listener"].as_str() {
                        Some("tip-latest") => handle(&e, "tip", &data),
                        Some("merch-latest") => handle(&e, "merch", &data),
                        _ => {}
                    }
                }
                ("Streamlabs", "event") => {
                    let kind = d["type"].as_str().unwrap_or("");
                    if matches!(kind, "donation" | "merch" | "streamlabscharitydonation") {
                        for m in d["message"].as_array().into_iter().flatten() {
                            handle(&e, kind, m);
                        }
                    }
                }
                _ => {}
            }
            None
        })
        .await;
        crate::warn(format!("{who}: {why}; trying again in a minute"));
        if up.replace(false) {
            handle(&e, "disconnected", &Value::Null);
        }
        tokio::time::sleep(Duration::from_secs(60)).await;
    }
}

/// A DonorDrive GET: the answer's status, ETag and JSON.
async fn dd_get(url: &str, etag: &str) -> (u16, String, Value) {
    let h = [("If-None-Match".to_string(), etag.to_string())];
    match crate::http::request("GET", url, if etag.is_empty() { &[] } else { &h }, b"").await {
        Ok(r) => {
            let tag = r
                .headers
                .iter()
                .find(|(k, _)| k.eq_ignore_ascii_case("etag"))
                .map(|(_, v)| v.clone())
                .unwrap_or_default();
            (r.status, tag, serde_json::from_slice(&r.body).unwrap_or_default())
        }
        Err(_) => (0, String::new(), Value::Null),
    }
}

/// Poll a DonorDrive participant every 15 s (what DonorDrive asks for), asking only whether it
/// changed; on a change, its new donations.
async fn donordrive(e: Rc<Engine>, base: String, id: String) {
    let api = format!("{}/api/participants/{id}", base.trim_end_matches('/'));
    let (mut etag, mut seen, mut first) = (String::new(), std::collections::HashSet::new(), true);
    loop {
        let (status, tag, mut profile) = dd_get(&api, &etag).await;
        match status {
            200 => {
                etag = tag;
                let (_, _, list) = dd_get(&format!("{api}/donations?limit=20"), "").await;
                let new: Vec<Value> = list
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter(|d| seen.insert(text(&d["donationID"])))
                    .cloned()
                    .collect();
                profile["donations"] = list;
                if !first {
                    handle_donordrive(&e, "ProfileUpdated", &profile);
                    let incentives = if new.iter().any(|d| !d["incentiveID"].is_null()) {
                        dd_get(&format!("{api}/incentives"), "").await.2
                    } else {
                        Value::Null
                    };
                    // Oldest first.
                    for mut d in new.into_iter().rev() {
                        d["profile"] = profile.clone();
                        let inc = incentives
                            .as_array()
                            .into_iter()
                            .flatten()
                            .find(|i| i["incentiveID"] == d["incentiveID"] && !i["incentiveID"].is_null());
                        handle_donordrive(&e, "Donation", &d);
                        if let Some(i) = inc {
                            d["incentive"] = i.clone();
                            handle_donordrive(&e, "Incentive", &d);
                        }
                    }
                } else {
                    crate::say(format!("DonorDrive: watching {}", text(&profile["displayName"])));
                }
                first = false;
            }
            304 => {}
            404 => crate::error(format!("DonorDrive: no participant {id} at {base}")),
            429 => tokio::time::sleep(Duration::from_secs(45)).await,
            _ => {}
        }
        tokio::time::sleep(Duration::from_secs(15)).await;
    }
}

impl Engine {
    /// Start, restart or stop the donation connections to match the settings.
    /// Connect the donation services again now (the window's Retry).
    pub fn donations_retry(self: &Rc<Self>) {
        self.donations.borrow_mut().cfg = None;
        self.donations_sync();
    }

    pub fn donations_sync(self: &Rc<Self>) {
        let cfg = self.config().donations.clone();
        let mut live = self.donations.borrow_mut();
        if live.cfg.as_ref() == Some(&cfg) {
            return;
        }
        live.tasks.clear();
        for (up, who) in [
            (&live.streamelements, "streamelements"),
            (&live.streamlabs, "streamlabs"),
        ] {
            if up.replace(false) {
                let handle = if who == "streamlabs" {
                    handle_streamlabs
                } else {
                    handle_streamelements
                };
                handle(self, "disconnected", &Value::Null);
            }
        }
        use crate::emote_events::Abort;
        use tokio::task::spawn_local;
        let se = cfg.streamelements_token.trim();
        if !se.is_empty() {
            let hello = json!(["authenticate", {"method": "jwt", "token": se}]);
            let (up, url) = (live.streamelements.clone(), SE_URL.to_string());
            let hello = Some(format!("42{hello}"));
            live.tasks.push(Abort(spawn_local(keep(
                self.clone(),
                "StreamElements",
                url,
                hello,
                up,
                handle_streamelements,
            ))));
        }
        let sl = cfg.streamlabs_token.trim();
        if !sl.is_empty() {
            let url = format!(
                "wss://sockets.streamlabs.com/socket.io/?token={}&EIO=3&transport=websocket",
                crate::commands::url_encode(sl)
            );
            let up = live.streamlabs.clone();
            live.tasks.push(Abort(spawn_local(keep(
                self.clone(),
                "Streamlabs",
                url,
                None,
                up,
                handle_streamlabs,
            ))));
        }
        let (base, id) = (cfg.donordrive_instance.trim(), cfg.donordrive_participant_id.trim());
        if !base.is_empty() && !id.is_empty() {
            let base = if base.contains("://") {
                base.to_string()
            } else {
                format!("https://{base}")
            };
            live.tasks
                .push(Abort(spawn_local(donordrive(self.clone(), base, id.into()))));
        }
        live.cfg = Some(cfg);
    }
}

pub(crate) fn urlencoding_decode(s: &str) -> String {
    let mut res = String::new();
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '%' {
            let h1 = chars.next().unwrap_or('0');
            let h2 = chars.next().unwrap_or('0');
            if let Ok(byte) = u8::from_str_radix(&format!("{h1}{h2}"), 16) {
                res.push(byte as char);
            }
        } else if c == '+' {
            res.push(' ');
        } else {
            res.push(c);
        }
    }
    res
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Config;
    use std::sync::Arc;

    /// A webhook with no signature header (none of these services sends one but Patreon).
    fn handle_webhook(e: &Rc<Engine>, p: &str, body: &[u8]) -> Result<(), String> {
        super::handle_webhook(e, p, body, None)
    }

    #[test]
    fn md5_and_hmac_md5_match_the_rfcs() {
        let hex = |b: &[u8]| b.iter().map(|x| format!("{x:02x}")).collect::<String>();
        assert_eq!(hex(&md5(b"")), "d41d8cd98f00b204e9800998ecf8427e");
        assert_eq!(
            hex(&md5(b"The quick brown fox jumps over the lazy dog")),
            "9e107d9d372bb6826bd81d3542a419d6"
        );
        // Longer than one block (RFC 1321's last example).
        assert_eq!(
            hex(&md5("1234567890".repeat(8).as_bytes())),
            "57edf4a22be3c955ac49da2e2107b67a"
        );
        // RFC 2104's test case 2.
        assert_eq!(
            hex(&hmac_md5(b"Jefe", b"what do ya want for nothing?")),
            "750c783e6ab0b503eaa86e310a5db738"
        );
    }

    #[test]
    fn webhooks_are_checked_once_a_secret_is_set() {
        let mut cfg = DonationsConfig::default();
        let kofi = json!({"verification_token": "abc"});
        assert!(
            authentic(&cfg, "kofi", b"", &kofi, None).is_ok(),
            "no token set: let through"
        );
        cfg.kofi_token = "abc".into();
        assert!(authentic(&cfg, "kofi", b"", &kofi, None).is_ok());
        assert!(authentic(&cfg, "kofi", b"", &json!({"verification_token": "nope"}), None).is_err());
        assert!(authentic(&cfg, "kofi", b"", &json!({}), None).is_err());
        cfg.patreon_webhook_secret = "Jefe".into();
        let body = b"what do ya want for nothing?";
        assert!(authentic(
            &cfg,
            "patreon",
            body,
            &json!({}),
            Some("750c783e6ab0b503eaa86e310a5db738")
        )
        .is_ok());
        assert!(authentic(&cfg, "patreon", body, &json!({}), Some("00")).is_err());
        assert!(authentic(&cfg, "patreon", body, &json!({}), None).is_err());
    }
    use tokio::sync::watch;

    #[test]
    fn parse_all_donation_webhooks() {
        let (tx, _rx) = watch::channel(Config::default());
        let engine = Engine::new(Arc::new(tx), |_| {});

        // Ko-fi
        assert!(handle_webhook(
            &engine,
            "kofi",
            br#"{"type":"Donation","from_name":"Alice","amount":"5.00","currency":"USD","message":"Great stream!"}"#
        )
        .is_ok());
        assert!(handle_webhook(&engine, "kofi", br#"{"type":"Subscription","from_name":"Bob","amount":"10.00","currency":"USD","is_first_subscription_payment":true}"#).is_ok());
        assert!(handle_webhook(&engine, "kofi", br#"{"type":"Subscription","from_name":"Bob","amount":"10.00","currency":"USD","is_first_subscription_payment":false}"#).is_ok());
        assert!(handle_webhook(
            &engine,
            "kofi",
            br#"{"type":"Shop Order","from_name":"Charlie","amount":"25.00"}"#
        )
        .is_ok());
        assert!(handle_webhook(
            &engine,
            "kofi",
            br#"{"type":"Commission","from_name":"Dave","amount":"50.00"}"#
        )
        .is_ok());

        // StreamElements
        assert!(handle_webhook(
            &engine,
            "streamelements",
            br#"{"type":"tip","username":"Eve","amount":15.0,"currency":"USD","message":"hi"}"#
        )
        .is_ok());
        assert!(handle_webhook(
            &engine,
            "streamelements",
            br#"{"type":"merch","username":"Eve","amount":35.0}"#
        )
        .is_ok());
        assert!(handle_webhook(&engine, "streamelements", br#"{"type":"connected"}"#).is_ok());
        assert!(handle_webhook(&engine, "streamelements", br#"{"type":"disconnected"}"#).is_ok());

        // Streamlabs
        assert!(handle_webhook(
            &engine,
            "streamlabs",
            br#"{"type":"donation","name":"Frank","amount":20.0,"currency":"USD"}"#
        )
        .is_ok());
        assert!(handle_webhook(
            &engine,
            "streamlabs",
            br#"{"type":"merchandise","name":"Frank","amount":40.0}"#
        )
        .is_ok());
        assert!(handle_webhook(
            &engine,
            "streamlabs",
            br#"{"type":"charity","name":"Frank","amount":100.0}"#
        )
        .is_ok());
        assert!(handle_webhook(&engine, "streamlabs", br#"{"type":"connected"}"#).is_ok());
        assert!(handle_webhook(&engine, "streamlabs", br#"{"type":"disconnected"}"#).is_ok());

        // Patreon
        assert!(handle_webhook(
            &engine,
            "patreon",
            br#"{"event":"PledgeCreated","fullName":"Grace","amountCents":500}"#
        )
        .is_ok());
        assert!(handle_webhook(
            &engine,
            "patreon",
            br#"{"event":"PledgeUpdated","fullName":"Grace","amountCents":1000}"#
        )
        .is_ok());
        assert!(handle_webhook(
            &engine,
            "patreon",
            br#"{"event":"PledgeDeleted","fullName":"Grace","amountCents":0}"#
        )
        .is_ok());
        assert!(handle_webhook(&engine, "patreon", br#"{"event":"FollowCreated","fullName":"Grace"}"#).is_ok());
        assert!(handle_webhook(&engine, "patreon", br#"{"event":"FollowDeleted","fullName":"Grace"}"#).is_ok());

        // Fourthwall
        assert!(handle_webhook(&engine, "fourthwall", br#"{"type":"ProductCreated","id":"p1"}"#).is_ok());
        assert!(handle_webhook(&engine, "fourthwall", br#"{"type":"ProductUpdated","id":"p1"}"#).is_ok());
        assert!(handle_webhook(&engine, "fourthwall", br#"{"type":"GiftPurchase","id":"g1"}"#).is_ok());
        assert!(handle_webhook(&engine, "fourthwall", br#"{"type":"OrderPlaced","id":"o1"}"#).is_ok());
        assert!(handle_webhook(&engine, "fourthwall", br#"{"type":"OrderUpdated","id":"o1"}"#).is_ok());
        assert!(handle_webhook(&engine, "fourthwall", br#"{"type":"Donation","amount":10}"#).is_ok());
        assert!(handle_webhook(&engine, "fourthwall", br#"{"type":"SubscriptionPurchased","id":"s1"}"#).is_ok());
        assert!(handle_webhook(&engine, "fourthwall", br#"{"type":"SubscriptionExpired","id":"s1"}"#).is_ok());
        assert!(handle_webhook(&engine, "fourthwall", br#"{"type":"SubscriptionChanged","id":"s1"}"#).is_ok());
        assert!(handle_webhook(&engine, "fourthwall", br#"{"type":"ThankYouSent","id":"t1"}"#).is_ok());
        assert!(handle_webhook(
            &engine,
            "fourthwall",
            br#"{"type":"NewsletterSubscribed","email":"a@b.com"}"#
        )
        .is_ok());
        assert!(handle_webhook(&engine, "fourthwall", br#"{"type":"GiftDrawStarted","id":"gd1"}"#).is_ok());
        assert!(handle_webhook(&engine, "fourthwall", br#"{"type":"GiftDrawEnded","id":"gd1"}"#).is_ok());

        // Throne
        assert!(handle_webhook(&engine, "throne", br#"{"type":"GiftPurchased","item":"Coffee"}"#).is_ok());
        assert!(handle_webhook(&engine, "throne", br#"{"type":"ContributionPurchased","amount":25}"#).is_ok());
        assert!(handle_webhook(&engine, "throne", br#"{"type":"GiftCrowdfunded","item":"Desk"}"#).is_ok());

        // TipeeeStream & TreatStream
        assert!(handle_webhook(
            &engine,
            "tipeeestream",
            br#"{"username":"Heidi","amount":10.0,"currency":"EUR"}"#
        )
        .is_ok());
        assert!(handle_webhook(&engine, "treatstream", br#"{"treat":"Pizza","sender":"Ivan"}"#).is_ok());

        // DonorDrive
        assert!(handle_webhook(&engine, "donordrive", br#"{"type":"Donation","amount":50.0}"#).is_ok());
        assert!(handle_webhook(&engine, "donordrive", br#"{"type":"Incentive","incentiveId":"inc1"}"#).is_ok());
        assert!(handle_webhook(
            &engine,
            "donordrive",
            br#"{"type":"ProfileUpdated","participantId":"p1"}"#
        )
        .is_ok());

        // Pally.gg
        assert!(handle_webhook(&engine, "pallygg", br#"{"type":"Connected"}"#).is_ok());
        assert!(handle_webhook(&engine, "pallygg", br#"{"type":"Disconnected"}"#).is_ok());
        assert!(handle_webhook(&engine, "pallygg", br#"{"type":"CampaignTip","amount":12.5}"#).is_ok());

        // Shopify
        assert!(handle_webhook(&engine, "shopify", br#"{"topic":"orders/create","id":1001}"#).is_ok());
        assert!(handle_webhook(&engine, "shopify", br#"{"topic":"orders/paid","id":1001}"#).is_ok());

        // Streamloots
        assert!(handle_webhook(&engine, "streamloots", br#"{"type":"PackPurchased","quantity":3}"#).is_ok());
        assert!(handle_webhook(&engine, "streamloots", br#"{"type":"PackGifted","recipient":"Judy"}"#).is_ok());
        assert!(handle_webhook(
            &engine,
            "streamloots",
            br#"{"type":"CardRedeemed","cardName":"Sound Effect"}"#
        )
        .is_ok());
        assert!(handle_webhook(&engine, "streamloots", br#"{"type":"Reaction","reaction":"clap"}"#).is_ok());
    }

    /// A fake socket.io server: the handshake, (`hello`) what the client says back, then `send`.
    async fn fake_socket_io(hello: bool, send: Vec<String>) -> (String, tokio::sync::oneshot::Receiver<String>) {
        let l = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("ws://{}/socket.io/?EIO=3&transport=websocket", l.local_addr().unwrap());
        let (tx, rx) = tokio::sync::oneshot::channel();
        tokio::spawn(async move {
            let (s, _) = l.accept().await.unwrap();
            let mut ws = tokio_tungstenite::accept_async(s).await.unwrap();
            for m in [r#"0{"sid":"x","pingInterval":50,"pingTimeout":20000}"#, "40"] {
                ws.send(Message::Text(m.into())).await.unwrap();
            }
            // The client's hello or its first ping.
            if hello {
                if let Some(Ok(Message::Text(t))) = ws.next().await {
                    let _ = tx.send(t.to_string());
                }
            }
            for m in send {
                ws.send(Message::Text(m.into())).await.unwrap();
            }
            while ws.next().await.is_some() {}
        });
        (url, rx)
    }

    #[test]
    fn socket_events_fire_with_their_variables() {
        use crate::engine::tests::{action, engine, log, run};
        let _g = crate::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let watch: &[(u32, &str, Value)] = &[
            (
                T_SE_TIP,
                "%tipUsername% %tipAmount% %tipCurrency% %tipMessage%",
                json!({"min": 5}),
            ),
            (
                T_SE_MERCH,
                "%merchUsername% %merchQuantity% %merchItems% %merchItem1.name%",
                json!({}),
            ),
            (T_SE_AUTHENTICATED, "se auth", json!({})),
            (T_SL_CONNECTED, "sl up", json!({})),
            (
                T_SL_DONATION,
                "%donationFrom% %donationFormattedAmount% %donationMessage%",
                json!({}),
            ),
            (
                T_SL_CHARITY_DONATION,
                "%charityDonationFrom% %charityDonationAmount%",
                json!({}),
            ),
            (T_SL_MERCHANDISE, "%merchandiseFrom% %merchandiseProduct%", json!({})),
            (
                T_DONORDRIVE_DONATION,
                "%donorName% %donorAmount% %raised% %donorIsAnonymous%",
                json!({}),
            ),
            (
                T_DONORDRIVE_INCENTIVE,
                "%donorName% %incentive.description% %incentive.quantity%",
                json!({}),
            ),
            (
                T_DONORDRIVE_PROFILE_UPDATED,
                "%name% %goal% %raised% %teamName%",
                json!({}),
            ),
        ];
        let actions: Vec<Value> = watch
            .iter()
            .map(|(code, vars, t)| {
                let mut a = action(&code.to_string(), "a", json!([log(&format!("T {code} {vars}"))]));
                let mut t = t.clone();
                t["id"] = "t".into();
                t["type"] = (*code).into();
                t["enabled"] = true.into();
                a["triggers"] = json!([t]);
                a
            })
            .collect();
        let e = engine(json!({ "actions": actions }));
        let out = run(async move {
            let ev = |name: &str, d: Value| format!("42{}", json!([name, d]));
            let (url, hello) = fake_socket_io(true, vec![
                ev("authenticated", json!({"channelId": "c"})),
                ev("event", json!({"type": "tip", "data": {"username": "small", "amount": 1, "currency": "USD"}})),
                ev("event", json!({"type": "tip", "data": {"username": "Ana", "amount": 10.5, "currency": "EUR", "message": "hi"}})),
                ev("event:test", json!({"listener": "merch-latest", "event": {"name": "Bo", "amount": 30,
                    "items": [{"name": "Mug", "price": 10, "quantity": 2}, {"name": "Cap", "price": 10, "quantity": 1}]}})),
            ])
            .await;
            let up = Rc::new(Cell::new(false));
            let se = crate::emote_events::Abort(tokio::task::spawn_local(keep(
                e.clone(),
                "StreamElements",
                url,
                Some("42[\"authenticate\",{\"method\":\"jwt\",\"token\":\"t\"}]".into()),
                up.clone(),
                handle_streamelements,
            )));
            let (url, _) = fake_socket_io(
                false,
                vec![
                    ev(
                        "event",
                        json!({"type": "donation", "message": [{"name": "Cy", "amount": "13.37",
                    "formatted_amount": "$13.37", "currency": "USD", "message": "gg"}]}),
                    ),
                    ev(
                        "event",
                        json!({"type": "streamlabscharitydonation", "for": "streamlabscharity",
                    "message": [{"from": "Di", "amount": 5, "currency": "USD"}]}),
                    ),
                    ev(
                        "event",
                        json!({"type": "merch", "message": [{"from": "Ed", "product": "Hoodie"}]}),
                    ),
                ],
            )
            .await;
            let sl = crate::emote_events::Abort(tokio::task::spawn_local(keep(
                e.clone(),
                "Streamlabs",
                url,
                None,
                Rc::new(Cell::new(false)),
                handle_streamlabs,
            )));
            tokio::time::sleep(Duration::from_millis(300)).await;
            assert!(up.get());
            assert!(hello.await.unwrap().contains("jwt"));
            drop((se, sl));
            let profile = json!({"displayName": "Jane", "teamName": "", "fundraisingGoal": 200, "sumDonations": 110,
                "participantID": 1});
            handle_donordrive(&e, "ProfileUpdated", &profile);
            let d = json!({"amount": 10, "message": null, "incentiveID": "i", "profile": profile,
                "incentive": {"incentiveID": "i", "description": "Shoutout", "amount": 10, "isActive": true}});
            handle_donordrive(&e, "Donation", &d);
            handle_donordrive(&e, "Incentive", &d);
            tokio::time::sleep(Duration::from_millis(100)).await;
        });
        let want = [
            "T 1205 se auth",
            "T 1201 Ana 10.5 EUR hi",
            "T 1202 Bo 3 2 Cap",
            "T 203 sl up",
            "T 201 Cy $13.37 gg",
            "T 205 Di 5",
            "T 202 Ed Hoodie",
            "T 3002 Jane 200 110 ",
            "T 3001 Anonymous 10 110 True",
            "T 3003 Anonymous Shoutout -1",
        ];
        for w in want {
            assert!(out.iter().any(|x| x == w), "{w} missing: {out:?}");
        }
        assert!(
            !out.iter().any(|x| x.contains("small")),
            "the range let a 1 tip through: {out:?}"
        );
    }
}
