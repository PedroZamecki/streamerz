//! Donations and Shops integration for streamerz (Phase 13):
//! - Unified webhook and event dispatch for 12 donation/shop platforms:
//!   Ko-fi, StreamElements, Streamlabs, Patreon, Fourthwall, Throne,
//!   TipeeeStream, TreatStream, DonorDrive, Pally.gg, Shopify, Streamloots.
//! - Webhook routes served over the local HTTP server (`/api/<platform>` or `/<platform>`).
//! - Opt-in background clients for socket/poll-based platforms (StreamElements, Streamlabs, DonorDrive).
//! - 48 Triggers covering every donation and shop event.

use crate::engine::Engine;
use crate::model::Obj;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::rc::Rc;

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

pub fn handle_streamelements(engine: &Rc<Engine>, event: &str, data: &Value) {
    let mut args = Obj::new();
    let user = data["username"].as_str().unwrap_or("Anonymous");
    let amount = data["amount"].as_f64().unwrap_or(0.0);
    let currency = data["currency"].as_str().unwrap_or("USD");
    let message = data["message"].as_str().unwrap_or("");

    args.insert("user".into(), user.into());
    args.insert("userName".into(), user.into());
    args.insert("amount".into(), amount.into());
    args.insert("currency".into(), currency.into());
    args.insert("message".into(), message.into());

    match event {
        "merch" | "merchandise" => {
            engine.fire(
                T_SE_MERCH,
                "StreamElementsMerch",
                "StreamElements",
                "streamelements",
                &args,
                |_| true,
            );
            engine.emit("StreamElements", "Merch", || json!(args));
        }
        "connected" => {
            engine.fire(
                T_SE_CONNECTED,
                "StreamElementsConnected",
                "StreamElements",
                "streamelements",
                &args,
                |_| true,
            );
            engine.emit("StreamElements", "Connected", || json!(args));
        }
        "disconnected" => {
            engine.fire(
                T_SE_DISCONNECTED,
                "StreamElementsDisconnected",
                "StreamElements",
                "streamelements",
                &args,
                |_| true,
            );
            engine.emit("StreamElements", "Disconnected", || json!(args));
        }
        _ => {
            engine.fire(
                T_SE_TIP,
                "StreamElementsTip",
                "StreamElements",
                "streamelements",
                &args,
                |_| true,
            );
            engine.emit("StreamElements", "Tip", || json!(args));
        }
    }
}

pub fn handle_streamlabs(engine: &Rc<Engine>, event: &str, data: &Value) {
    let mut args = Obj::new();
    let name = data["name"]
        .as_str()
        .or_else(|| data["from"].as_str())
        .unwrap_or("Anonymous");
    let amount = data["amount"].as_f64().unwrap_or(0.0);
    let currency = data["currency"].as_str().unwrap_or("USD");
    let message = data["message"].as_str().unwrap_or("");

    args.insert("user".into(), name.into());
    args.insert("from".into(), name.into());
    args.insert("amount".into(), amount.into());
    args.insert("currency".into(), currency.into());
    args.insert("message".into(), message.into());

    match event {
        "merchandise" => {
            engine.fire(
                T_SL_MERCHANDISE,
                "StreamlabsMerchandise",
                "Streamlabs",
                "streamlabs",
                &args,
                |_| true,
            );
            engine.emit("Streamlabs", "Merchandise", || json!(args));
        }
        "charity" => {
            engine.fire(
                T_SL_CHARITY_DONATION,
                "StreamlabsCharityDonation",
                "Streamlabs",
                "streamlabs",
                &args,
                |_| true,
            );
            engine.emit("Streamlabs", "CharityDonation", || json!(args));
        }
        "connected" => {
            engine.fire(
                T_SL_CONNECTED,
                "StreamlabsConnected",
                "Streamlabs",
                "streamlabs",
                &args,
                |_| true,
            );
            engine.emit("Streamlabs", "Connected", || json!(args));
        }
        "disconnected" => {
            engine.fire(
                T_SL_DISCONNECTED,
                "StreamlabsDisconnected",
                "Streamlabs",
                "streamlabs",
                &args,
                |_| true,
            );
            engine.emit("Streamlabs", "Disconnected", || json!(args));
        }
        _ => {
            engine.fire(
                T_SL_DONATION,
                "StreamlabsDonation",
                "Streamlabs",
                "streamlabs",
                &args,
                |_| true,
            );
            engine.emit("Streamlabs", "Donation", || json!(args));
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

pub fn handle_fourthwall(engine: &Rc<Engine>, event: &str, data: &Value) {
    let mut args = Obj::new();
    for (k, v) in data.as_object().into_iter().flatten() {
        args.insert(k.clone(), v.clone());
    }

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

pub fn handle_donordrive(engine: &Rc<Engine>, event: &str, data: &Value) {
    let mut args = Obj::new();
    for (k, v) in data.as_object().into_iter().flatten() {
        args.insert(k.clone(), v.clone());
    }

    match event {
        "incentive" | "Incentive" => {
            engine.fire(
                T_DONORDRIVE_INCENTIVE,
                "DonorDriveIncentive",
                "DonorDrive",
                "donordrive",
                &args,
                |_| true,
            );
            engine.emit("DonorDrive", "Incentive", || json!(args));
        }
        "profile" | "ProfileUpdated" => {
            engine.fire(
                T_DONORDRIVE_PROFILE_UPDATED,
                "DonorDriveProfileUpdated",
                "DonorDrive",
                "donordrive",
                &args,
                |_| true,
            );
            engine.emit("DonorDrive", "ProfileUpdated", || json!(args));
        }
        _ => {
            engine.fire(
                T_DONORDRIVE_DONATION,
                "DonorDriveDonation",
                "DonorDrive",
                "donordrive",
                &args,
                |_| true,
            );
            engine.emit("DonorDrive", "Donation", || json!(args));
        }
    }
}

pub fn handle_pallygg(engine: &Rc<Engine>, event: &str, data: &Value) {
    let mut args = Obj::new();
    for (k, v) in data.as_object().into_iter().flatten() {
        args.insert(k.clone(), v.clone());
    }

    match event {
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
pub fn handle_webhook(engine: &Rc<Engine>, platform: &str, body: &[u8]) -> Result<(), String> {
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

    match platform.to_lowercase().as_str() {
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

fn urlencoding_decode(s: &str) -> String {
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
}
