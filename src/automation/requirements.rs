//! What a trigger or sub-action type depends on, independent of any running service.
//!
//! A type that needs a service (Twitch, OBS, Spotify...) is never disabled for lacking it: the Actions
//! page and MCP show a warning instead. This file only says what each type *needs*; `core::requirements`
//! checks that against the live state.

use crate::types::{name, SUB_ACTION_INFO, TRIGGERS};
use serde_json::{json, Value};

/// A service an automation type can depend on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Service {
    Twitch,
    Kick,
    YouTube,
    TikTok,
    Obs,
    Slobs,
    Spotify,
    StreamElements,
    Streamlabs,
    Kofi,
    Patreon,
    DonorDrive,
}

impl Service {
    /// The id used in MCP output and warning codes.
    pub fn id(self) -> &'static str {
        match self {
            Self::Twitch => "twitch",
            Self::Kick => "kick",
            Self::YouTube => "youtube",
            Self::TikTok => "tiktok",
            Self::Obs => "obs",
            Self::Slobs => "slobs",
            Self::Spotify => "spotify",
            Self::StreamElements => "streamelements",
            Self::Streamlabs => "streamlabs",
            Self::Kofi => "kofi",
            Self::Patreon => "patreon",
            Self::DonorDrive => "donordrive",
        }
    }

    /// The name a person knows it by.
    pub fn title(self) -> &'static str {
        match self {
            Self::Twitch => "Twitch",
            Self::Kick => "Kick",
            Self::YouTube => "YouTube",
            Self::TikTok => "TikTok",
            Self::Obs => "OBS Studio",
            Self::Slobs => "Streamlabs Desktop",
            Self::Spotify => "Spotify",
            Self::StreamElements => "StreamElements",
            Self::Streamlabs => "Streamlabs",
            Self::Kofi => "Ko-fi",
            Self::Patreon => "Patreon",
            Self::DonorDrive => "DonorDrive",
        }
    }

    /// A chat platform (it has a broadcaster login, a channel and a chat).
    pub fn platform(self) -> bool {
        matches!(self, Self::Twitch | Self::Kick | Self::YouTube | Self::TikTok)
    }
}

/// How much of a service is needed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Level {
    /// Only that it is set up: connection events fire as it connects and disconnects.
    Configured,
    /// Up and receiving. A read-only connection is enough (a channel's chat with no login).
    Connected,
    /// Up and logged in: it calls the service's API or listens to events that need the account.
    Login,
}

impl Level {
    pub fn id(self) -> &'static str {
        match self {
            Self::Configured => "configured",
            Self::Connected => "connected",
            Self::Login => "login",
        }
    }
}

/// Something Twitch only gives an Affiliate or Partner channel.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Feature {
    Ads,
    Rewards,
    Subscriptions,
    Bits,
    Predictions,
    Charity,
}

impl Feature {
    pub fn id(self) -> &'static str {
        match self {
            Self::Ads => "ads",
            Self::Rewards => "rewards",
            Self::Subscriptions => "subscriptions",
            Self::Bits => "bits",
            Self::Predictions => "predictions",
            Self::Charity => "charity",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Need {
    /// A service, at a level.
    Service(Service, Level),
    /// Twitch only allows it on an Affiliate or Partner channel.
    TwitchPlan(Feature),
}

/// Whether the stored item is a trigger or a sub-action.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Trigger,
    SubAction,
}

impl Kind {
    pub fn parse(kind: &str) -> Option<Self> {
        match kind {
            "trigger" => Some(Self::Trigger),
            "subaction" => Some(Self::SubAction),
            _ => None,
        }
    }

    pub fn id(self) -> &'static str {
        match self {
            Self::Trigger => "trigger",
            Self::SubAction => "subaction",
        }
    }
}

/// Twitch triggers an anonymous chat reader (a channel set, no login) still gets: they come from chat lines.
const TWITCH_CHAT_TRIGGERS: &[u32] = &[120, 124, 133, 135, 157, 472];

/// Twitch triggers only an Affiliate or Partner channel has.
fn twitch_trigger_feature(code: u32) -> Option<Feature> {
    Some(match code {
        102 | 143 | 480 => Feature::Bits,
        103..=106 | 477..=479 => Feature::Subscriptions,
        112..=115 | 171 => Feature::Rewards,
        128..=132 => Feature::Predictions,
        139 | 186 => Feature::Ads,
        141 | 142 | 146 | 147 => Feature::Charity,
        _ => return None,
    })
}

/// Twitch sub-action categories only an Affiliate or Partner channel has.
fn twitch_category_feature(category: &str) -> Option<Feature> {
    Some(match category {
        "Twitch/Ads" => Feature::Ads,
        "Twitch/Rewards" => Feature::Rewards,
        "Twitch/Subscribers" => Feature::Subscriptions,
        "Twitch/Predictions" => Feature::Predictions,
        "Twitch/Charity" => Feature::Charity,
        _ => return None,
    })
}

/// Events about a connection itself ("Connected", "Authenticated"): they need it set up, not up.
fn connection_event(name: &str) -> bool {
    ["Connected", "Disconnected", "Authenticated"]
        .iter()
        .any(|end| name.ends_with(end))
}

fn platform_service(top: &str) -> Option<Service> {
    Some(match top {
        "Twitch" => Service::Twitch,
        "Kick" => Service::Kick,
        "YouTube" => Service::YouTube,
        "TikTok" => Service::TikTok,
        _ => return None,
    })
}

/// What a sub-action type needs. Types the app can't run (not supported) need nothing: they say so already.
pub fn sub_action(code: u32) -> Vec<Need> {
    if !crate::engine::supported(code) {
        return Vec::new();
    }
    let Ok(at) = SUB_ACTION_INFO.binary_search_by_key(&code, |entry| entry.0) else {
        return Vec::new();
    };
    let category = SUB_ACTION_INFO[at].2;
    let top = category.split('/').next().unwrap_or_default();
    if let Some(service) = platform_service(top) {
        // Groups of users and the sub counter are kept here, not asked of the platform.
        let local = category.ends_with("/User Groups") || category == "Twitch/Sub Counter" || matches!(code, 22 | 550);
        if local {
            return Vec::new();
        }
        // The poll on the live is read from the chat, which needs no login.
        if code == 99665 {
            return vec![Need::Service(service, Level::Connected)];
        }
        let mut needs = vec![Need::Service(service, Level::Login)];
        if let Some(feature) = twitch_category_feature(category) {
            needs.push(Need::TwitchPlan(feature));
        }
        return needs;
    }
    match top {
        "OBS Studio" => vec![Need::Service(Service::Obs, Level::Connected)],
        "Streamlabs Desktop" => vec![Need::Service(Service::Slobs, Level::Connected)],
        _ if category.starts_with("Integrations/Spotify") => vec![Need::Service(Service::Spotify, Level::Login)],
        _ => Vec::new(),
    }
}

/// What a trigger type needs. Types that never fire (unavailable) need nothing.
pub fn trigger(code: u32) -> Vec<Need> {
    if !crate::docs::available("trigger", code) {
        return Vec::new();
    }
    let Some(raw) = name(TRIGGERS, code) else {
        return Vec::new();
    };
    let at = |service: Service| {
        let level = if connection_event(raw) {
            Level::Configured
        } else {
            Level::Connected
        };
        vec![Need::Service(service, level)]
    };
    if raw.starts_with("Twitch") {
        let level = if connection_event(raw) {
            Level::Configured
        } else if TWITCH_CHAT_TRIGGERS.contains(&code) || raw.contains("SevenTV") || raw.contains("BetterTTV") {
            Level::Connected
        } else {
            Level::Login
        };
        let mut needs = vec![Need::Service(Service::Twitch, level)];
        if let Some(feature) = twitch_trigger_feature(code) {
            needs.push(Need::TwitchPlan(feature));
        }
        return needs;
    }
    if raw.starts_with("Kick") {
        return at(Service::Kick);
    }
    if raw.starts_with("YouTube") || raw.starts_with("Youtube") {
        return at(Service::YouTube);
    }
    if raw.starts_with("TikTok") {
        return at(Service::TikTok);
    }
    if raw.starts_with("StreamlabsDesktop") {
        return at(Service::Slobs);
    }
    if raw.starts_with("Obs") {
        return at(Service::Obs);
    }
    if raw.starts_with("Streamlabs") {
        return at(Service::Streamlabs);
    }
    if raw.starts_with("StreamElements") {
        return at(Service::StreamElements);
    }
    if raw.starts_with("Kofi") {
        return vec![Need::Service(Service::Kofi, Level::Configured)];
    }
    if raw.starts_with("Patreon") {
        return vec![Need::Service(Service::Patreon, Level::Configured)];
    }
    if raw.starts_with("DonorDrive") {
        return vec![Need::Service(Service::DonorDrive, Level::Configured)];
    }
    if raw.starts_with("Spotify") {
        return at(Service::Spotify);
    }
    Vec::new()
}

/// The needs of a stored type.
pub fn of(kind: Kind, code: u32) -> Vec<Need> {
    match kind {
        Kind::Trigger => trigger(code),
        Kind::SubAction => sub_action(code),
    }
}

/// `needs` as `describe` and MCP show it: what service, how much of it, in plain words.
pub fn needs_json(needs: &[Need]) -> Vec<Value> {
    needs
        .iter()
        .map(|need| match need {
            Need::Service(service, level) => {
                json!({"service": service.id(), "name": service.title(), "level": level.id()})
            }
            Need::TwitchPlan(feature) => {
                json!({"service": "twitch", "name": "Twitch", "level": "affiliate_or_partner", "feature": feature.id()})
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn service(needs: &[Need], wanted: Service) -> Option<Level> {
        needs.iter().find_map(|need| match need {
            Need::Service(service, level) if *service == wanted => Some(*level),
            _ => None,
        })
    }

    #[test]
    fn running_ads_needs_a_working_twitch_login_and_an_ad_capable_channel() {
        let needs = sub_action(19);
        assert_eq!(service(&needs, Service::Twitch), Some(Level::Login));
        assert!(needs.contains(&Need::TwitchPlan(Feature::Ads)));
    }

    #[test]
    fn a_plain_twitch_api_call_needs_a_login_but_no_plan() {
        let needs = sub_action(15);
        assert_eq!(service(&needs, Service::Twitch), Some(Level::Login));
        assert!(!needs.iter().any(|need| matches!(need, Need::TwitchPlan(_))));
    }

    #[test]
    fn what_twitch_gives_only_monetized_channels_is_marked() {
        for (code, feature) in [
            (200, Feature::Rewards),
            (508, Feature::Subscriptions),
            (568, Feature::Predictions),
            (25, Feature::Charity),
            (555, Feature::Ads),
        ] {
            assert!(
                sub_action(code).contains(&Need::TwitchPlan(feature)),
                "sub-action {code}"
            );
        }
        for (code, feature) in [
            (102, Feature::Bits),
            (103, Feature::Subscriptions),
            (112, Feature::Rewards),
            (128, Feature::Predictions),
            (139, Feature::Ads),
            (141, Feature::Charity),
        ] {
            assert!(trigger(code).contains(&Need::TwitchPlan(feature)), "trigger {code}");
        }
        // Following, raiding and chatting are for every channel.
        for code in [101, 107, 133] {
            assert!(
                !trigger(code).iter().any(|need| matches!(need, Need::TwitchPlan(_))),
                "trigger {code}"
            );
        }
    }

    #[test]
    fn chat_triggers_work_with_a_read_only_connection_and_events_need_the_login() {
        assert_eq!(service(&trigger(133), Service::Twitch), Some(Level::Connected));
        assert_eq!(service(&trigger(101), Service::Twitch), Some(Level::Login));
        assert_eq!(service(&trigger(35010), Service::Kick), Some(Level::Connected));
        assert_eq!(service(&trigger(4003), Service::YouTube), Some(Level::Connected));
        assert_eq!(service(&trigger(41001), Service::TikTok), Some(Level::Connected));
    }

    #[test]
    fn connection_events_only_need_the_service_set_up() {
        assert_eq!(service(&trigger(14001), Service::Obs), Some(Level::Configured));
        assert_eq!(service(&trigger(14004), Service::Obs), Some(Level::Connected));
        assert_eq!(service(&trigger(22002), Service::Slobs), Some(Level::Configured));
        assert_eq!(service(&trigger(41008), Service::TikTok), Some(Level::Configured));
        assert_eq!(
            service(&trigger(1203), Service::StreamElements),
            Some(Level::Configured)
        );
    }

    #[test]
    fn stream_apps_donations_and_spotify_are_mapped() {
        assert_eq!(service(&sub_action(30), Service::Obs), Some(Level::Connected));
        assert_eq!(service(&sub_action(401), Service::Slobs), Some(Level::Connected));
        assert_eq!(service(&sub_action(99840), Service::Spotify), Some(Level::Login));
        assert_eq!(service(&trigger(201), Service::Streamlabs), Some(Level::Connected));
        assert_eq!(service(&trigger(1201), Service::StreamElements), Some(Level::Connected));
        assert_eq!(service(&trigger(7001), Service::Kofi), Some(Level::Configured));
        assert_eq!(service(&trigger(8003), Service::Patreon), Some(Level::Configured));
        assert_eq!(service(&trigger(3001), Service::DonorDrive), Some(Level::Configured));
        assert_eq!(service(&trigger(99809), Service::Spotify), Some(Level::Connected));
    }

    #[test]
    fn nothing_that_runs_here_alone_needs_a_service() {
        // Logic, files, globals, groups of users and the sub counter are local.
        for code in [10001, 120, 121, 1001, 1009, 99901, 553, 551, 22, 550] {
            assert!(sub_action(code).is_empty(), "sub-action {code}");
        }
        for code in [401, 701, 709] {
            assert!(trigger(code).is_empty(), "trigger {code}");
        }
    }

    #[test]
    fn types_that_never_run_need_nothing() {
        // Speaker.bot's sub-actions aren't supported, so a missing service isn't why they don't work.
        assert!(!crate::engine::supported(601));
        assert!(sub_action(601).is_empty());
        assert!(sub_action(u32::MAX).is_empty());
        assert!(trigger(u32::MAX).is_empty());
    }

    #[test]
    fn every_supported_service_sub_action_needs_its_service() {
        for &(code, _, category, _) in SUB_ACTION_INFO {
            if !crate::engine::supported(code) {
                continue;
            }
            let top = category.split('/').next().unwrap_or_default();
            let local =
                category.ends_with("/User Groups") || category == "Twitch/Sub Counter" || matches!(code, 22 | 550);
            let wanted = matches!(
                top,
                "Twitch" | "Kick" | "YouTube" | "TikTok" | "OBS Studio" | "Streamlabs Desktop"
            ) || category.starts_with("Integrations/Spotify");
            assert_eq!(!sub_action(code).is_empty(), wanted && !local, "{code} {category}");
        }
    }

    #[test]
    fn needs_read_as_json() {
        let json = needs_json(&sub_action(19));
        assert_eq!(json[0]["service"], "twitch");
        assert_eq!(json[0]["level"], "login");
        assert_eq!(json[1]["level"], "affiliate_or_partner");
        assert_eq!(json[1]["feature"], "ads");
    }
}
