//! Warnings for the triggers and sub-actions that depend on something that isn't ready: Twitch not
//! connected, a channel that can't run ads, OBS down, a token missing.
//!
//! Nothing is disabled for it. The Actions page draws the warning on the row and in the form, and the
//! MCP `automation_warnings` tool lists every affected item, so a person or an assistant can see what is
//! not as it should be and why. `automation::requirements` says what each type needs; this file checks it
//! against the live state, captured once into a [`Readiness`] (so checking a thousand items costs no more
//! engine reads than checking one).

use crate::{
    automation::requirements::{self as rules, Feature, Kind, Level, Need, Service},
    core::preflight::PreflightFix,
    edit,
    engine::Engine,
    i18n::{tr, trf},
    model::Obj,
};
use serde::Serialize;
use serde_json::{json, Value};

/// One thing wrong or missing for one item.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Warning {
    /// "blocker": it won't work as things are. "warning": it may not work fully.
    pub severity: &'static str,
    /// Stable id for scripts, such as `twitch_disconnected` or `twitch_plan_ads`.
    pub code: String,
    /// `twitch`, `obs`, `spotify`... (see `Service::id`).
    pub service: &'static str,
    pub message: String,
    /// Where it is fixed.
    pub fix: PreflightFix,
}

/// A chat platform's state.
#[derive(Clone, Debug, Default)]
struct Platform {
    /// A login or a channel is set.
    set_up: bool,
    /// Connected (or waiting on a broadcast, for YouTube and TikTok).
    up: bool,
    /// The keyring is locked, so no login can be read.
    locked: bool,
    /// What stops its API and account events from working, if anything.
    login: Option<Warning>,
}

/// Another service's state.
#[derive(Clone, Debug, Default)]
struct Simple {
    set_up: bool,
    up: bool,
    login: bool,
}

/// A stream app's connection.
#[derive(Clone, Debug)]
struct Link {
    id: String,
    name: String,
    up: bool,
}

/// What is ready right now.
#[derive(Clone, Debug, Default)]
pub struct Readiness {
    platforms: Vec<(Service, Platform)>,
    /// Twitch's account type for the channel: "partner", "affiliate" or "" (neither). None until read.
    twitch_type: Option<String>,
    obs: Vec<Link>,
    slobs: Vec<Link>,
    others: Vec<(Service, Simple)>,
}

fn accounts_fix(action: String) -> PreflightFix {
    PreflightFix {
        label: tr("nav.page.accounts").to_owned(),
        page: "Accounts".to_owned(),
        action,
        ..Default::default()
    }
}

fn page_fix(label: &'static str, page: &str) -> PreflightFix {
    PreflightFix {
        label: tr(label).to_owned(),
        page: page.to_owned(),
        ..Default::default()
    }
}

fn warning(severity: &'static str, service: Service, what: &str, message: String, fix: PreflightFix) -> Warning {
    Warning {
        severity,
        code: format!("{}_{what}", service.id()),
        service: service.id(),
        message,
        fix,
    }
}

/// What the page that fixes a service is called, for its warnings' fix.
fn page_of(service: Service) -> (&'static str, &'static str) {
    match service {
        Service::Obs | Service::Slobs => ("nav.page.stream_apps", "Stream Apps"),
        Service::Spotify => ("nav.page.integrations", "Integrations"),
        Service::Twitch | Service::Kick | Service::YouTube | Service::TikTok => ("nav.page.accounts", "Accounts"),
        _ => ("nav.page.donations", "Donations"),
    }
}

impl Readiness {
    /// Read the live state of every service an automation may depend on.
    pub fn capture(engine: &Engine) -> Self {
        let locked = crate::secrets::is_locked();
        let mut ready = Self {
            twitch_type: crate::twitch::channel_type(),
            ..Self::default()
        };
        for service in [Service::Twitch, Service::Kick, Service::YouTube, Service::TikTok] {
            let (set_up, up) = {
                let config = engine.config();
                match service {
                    Service::Twitch => (
                        config.twitch.broadcaster.is_some() || !config.twitch.channel.trim().is_empty(),
                        engine.twitch_connected(),
                    ),
                    Service::Kick => (
                        config.kick.broadcaster.is_some() || !config.kick.channel.trim().is_empty(),
                        engine.kick_connected(),
                    ),
                    Service::YouTube => (
                        config.youtube.broadcaster.is_some() || !config.youtube.channel_id.trim().is_empty(),
                        engine.youtube_connected() || engine.youtube_standby(),
                    ),
                    _ => (
                        crate::tiktok_bridge::set_up(&config) || !config.tiktok.channel.trim().is_empty(),
                        engine.tiktok_connected() || engine.tiktok_standby(),
                    ),
                }
            };
            // Whether it is broadcasting, or its video is private, doesn't matter to an automation.
            let login = super::preflight::evaluate_platform(engine, service.id())
                .filter(|(_, code, _, _)| !matches!(*code, "youtube_no_broadcast" | "youtube_private"))
                .map(|(severity, code, message, fix)| Warning {
                    severity,
                    code: code.to_owned(),
                    service: service.id(),
                    message,
                    fix,
                });
            ready.platforms.push((
                service,
                Platform {
                    set_up,
                    up,
                    locked,
                    login,
                },
            ));
        }
        let config = engine.config();
        let link = |item: &Obj, up: bool| Link {
            id: item.get("id").and_then(Value::as_str).unwrap_or_default().to_owned(),
            name: item.get("name").and_then(Value::as_str).unwrap_or_default().to_owned(),
            up,
        };
        for item in &config.obs.connections {
            let id = item.get("id").and_then(Value::as_str).unwrap_or_default();
            ready.obs.push(link(item, engine.obs_connected(Some(id))));
        }
        for item in &config.slobs.connections {
            let id = item.get("id").and_then(Value::as_str).unwrap_or_default();
            ready.slobs.push(link(item, engine.slobs_up(id)));
        }
        let spotify = &config.spotify;
        let spotify_login = spotify.enabled && !spotify.token.access_token.is_empty();
        let spotify_any = spotify_login || spotify.local_monitor;
        ready.others.push((
            Service::Spotify,
            Simple {
                set_up: spotify_any,
                up: spotify_any,
                login: spotify_login,
            },
        ));
        let donations = &config.donations;
        let live = engine.donations.borrow();
        let token = |text: &str| !text.trim().is_empty();
        for (service, set_up, up) in [
            (
                Service::StreamElements,
                token(&donations.streamelements_token),
                live.streamelements.get(),
            ),
            (
                Service::Streamlabs,
                token(&donations.streamlabs_token),
                live.streamlabs.get(),
            ),
            (
                Service::Kofi,
                token(&donations.kofi_token),
                token(&donations.kofi_token),
            ),
            (
                Service::Patreon,
                token(&donations.patreon_webhook_secret),
                token(&donations.patreon_webhook_secret),
            ),
            (
                Service::DonorDrive,
                token(&donations.donordrive_participant_id) && token(&donations.donordrive_instance),
                token(&donations.donordrive_participant_id) && token(&donations.donordrive_instance),
            ),
        ] {
            ready.others.push((service, Simple { set_up, up, login: up }));
        }
        ready
    }

    fn platform(&self, service: Service) -> Option<&Platform> {
        self.platforms
            .iter()
            .find(|(candidate, _)| *candidate == service)
            .map(|(_, state)| state)
    }

    fn check_platform(&self, service: Service, level: Level) -> Option<Warning> {
        let state = self.platform(service)?;
        let name = service.title();
        let not_set_up = || {
            warning(
                "blocker",
                service,
                "not_set_up",
                trf("requirements.platform_not_set_up", &[("platform", name)]),
                accounts_fix(format!("{}.login", service.id())),
            )
        };
        match level {
            Level::Configured => (!state.set_up).then(not_set_up),
            Level::Connected => {
                if !state.set_up {
                    return Some(not_set_up());
                }
                if state.up {
                    return None;
                }
                if state.locked {
                    return Some(warning(
                        "blocker",
                        service,
                        "keyring_locked",
                        tr("preflight.issue.keyring_locked").to_owned(),
                        accounts_fix("unlock".into()),
                    ));
                }
                Some(warning(
                    "blocker",
                    service,
                    "disconnected",
                    trf("stream.not_connected", &[("where", name)]),
                    accounts_fix(format!("{}.connect", service.id())),
                ))
            }
            Level::Login => state.login.clone(),
        }
    }

    /// OBS or Streamlabs Desktop. `connection` is the id the item names (none: any).
    fn check_app(&self, service: Service, level: Level, connection: &str) -> Option<Warning> {
        let links = if service == Service::Obs {
            &self.obs
        } else {
            &self.slobs
        };
        let app = service.title();
        let fix = page_fix(page_of(service).0, page_of(service).1);
        if links.is_empty() {
            return Some(warning(
                "blocker",
                service,
                "not_set_up",
                trf("requirements.app_not_set_up", &[("app", app)]),
                fix,
            ));
        }
        if level == Level::Configured {
            return None;
        }
        if !connection.is_empty() {
            return match links.iter().find(|link| link.id == connection) {
                None => Some(warning(
                    "blocker",
                    service,
                    "connection_missing",
                    trf("requirements.app_connection_missing", &[("app", app)]),
                    fix,
                )),
                Some(link) if !link.up => Some(warning(
                    "blocker",
                    service,
                    "disconnected",
                    trf(
                        "requirements.app_connection_down",
                        &[("app", app), ("name", &link.name)],
                    ),
                    fix,
                )),
                Some(_) => None,
            };
        }
        (!links.iter().any(|link| link.up)).then(|| {
            warning(
                "blocker",
                service,
                "disconnected",
                trf("requirements.app_not_connected", &[("app", app)]),
                fix,
            )
        })
    }

    fn check_other(&self, service: Service, level: Level) -> Option<Warning> {
        let state = self
            .others
            .iter()
            .find(|(candidate, _)| *candidate == service)
            .map(|(_, state)| state)?;
        let fix = page_fix(page_of(service).0, page_of(service).1);
        let name = service.title();
        let not_set_up = || trf("requirements.service_not_set_up", &[("service", name)]);
        let (ok, message, what) = match (service, level) {
            (Service::Spotify, Level::Login) => {
                (state.login, tr("requirements.spotify_login").to_owned(), "not_set_up")
            }
            (Service::Spotify, _) => (state.set_up, tr("requirements.spotify_idle").to_owned(), "not_set_up"),
            (_, Level::Configured) => (state.set_up, not_set_up(), "not_set_up"),
            _ if !state.set_up => (false, not_set_up(), "not_set_up"),
            _ => (
                state.up,
                trf("requirements.service_not_connected", &[("service", name)]),
                "disconnected",
            ),
        };
        (!ok).then(|| warning("blocker", service, what, message, fix))
    }

    /// Twitch refuses some things to a channel that is neither Affiliate nor Partner. Said only when
    /// the account works, so the first problem is the one shown.
    fn check_plan(&self, feature: Feature) -> Option<Warning> {
        if self.check_platform(Service::Twitch, Level::Login).is_some() {
            return None;
        }
        if self.twitch_type.as_deref() != Some("") {
            return None;
        }
        let what = match feature {
            Feature::Ads => tr("requirements.what.ads"),
            Feature::Rewards => tr("requirements.what.rewards"),
            Feature::Subscriptions => tr("requirements.what.subscriptions"),
            Feature::Bits => tr("requirements.what.bits"),
            Feature::Predictions => tr("requirements.what.predictions"),
            Feature::Charity => tr("requirements.what.charity"),
        };
        Some(Warning {
            severity: "blocker",
            code: format!("twitch_plan_{}", feature.id()),
            service: "twitch",
            message: trf("requirements.twitch_plan", &[("what", what)]),
            fix: PreflightFix::default(),
        })
    }

    fn check(&self, need: &Need, trigger: bool, connection: &str) -> Option<Warning> {
        match *need {
            Need::TwitchPlan(feature) => self.check_plan(feature),
            Need::Service(service, level) if service.platform() => {
                // A Twitch event that needs the account isn't there when only a channel's chat is read.
                if service == Service::Twitch && trigger && level == Level::Login {
                    let state = self.platform(service)?;
                    if state.set_up
                        && state.up
                        && state.login.as_ref().is_some_and(|w| w.code == "twitch_login_missing")
                    {
                        return Some(warning(
                            "blocker",
                            service,
                            "login_needed",
                            tr("requirements.twitch_login_needed").to_owned(),
                            accounts_fix("twitch.login".into()),
                        ));
                    }
                }
                self.check_platform(service, level)
            }
            Need::Service(service, level) if matches!(service, Service::Obs | Service::Slobs) => {
                self.check_app(service, level, connection)
            }
            Need::Service(service, level) => self.check_other(service, level),
        }
    }

    /// What is wrong for an item of this type (`connection` is the OBS or Streamlabs Desktop connection
    /// it names, if any).
    pub fn for_code(&self, kind: Kind, code: u32, connection: &str) -> Vec<Warning> {
        let mut out: Vec<Warning> = Vec::new();
        for need in rules::of(kind, code) {
            if let Some(found) = self.check(&need, kind == Kind::Trigger, connection) {
                if !out.iter().any(|seen| seen.code == found.code) {
                    out.push(found);
                }
            }
        }
        out
    }

    /// What is wrong for a stored trigger or sub-action.
    pub fn warnings(&self, kind: Kind, item: &Obj) -> Vec<Warning> {
        let connection = item.get("connectionId").and_then(Value::as_str).unwrap_or_default();
        self.for_code(kind, edit::kind(item) as u32, connection)
    }

    /// Whether any trigger or sub-action of `action` has a warning, for its row.
    /// The Actions page asks this for every row it draws: nested sub-actions are read in place.
    pub fn action_has_warnings(&self, action: &crate::model::Action) -> bool {
        fn has(ready: &Readiness, sub: &Obj) -> bool {
            !ready.warnings(Kind::SubAction, sub).is_empty() || kids(sub).any(|kid| has(ready, kid))
        }
        action
            .triggers
            .iter()
            .any(|trigger| !self.warnings(Kind::Trigger, trigger).is_empty())
            || action.sub_actions.iter().any(|sub| has(self, sub))
    }

    /// Each service, and whether it is as it should be, for a report's header.
    fn services(&self) -> Vec<Value> {
        let mut out = Vec::new();
        for (service, state) in &self.platforms {
            let (status, issue) = if !state.set_up {
                ("not_set_up", self.check_platform(*service, Level::Configured))
            } else if let Some(issue) = state
                .login
                .clone()
                .or_else(|| self.check_platform(*service, Level::Connected))
            {
                ("attention", Some(issue))
            } else {
                ("ready", None)
            };
            out.push(json!({"service": service.id(), "name": service.title(), "status": status, "issue": issue}));
        }
        for service in [Service::Obs, Service::Slobs] {
            let links = if service == Service::Obs {
                &self.obs
            } else {
                &self.slobs
            };
            let (status, issue) = if links.is_empty() {
                ("not_set_up", self.check_app(service, Level::Configured, ""))
            } else if let Some(issue) = self.check_app(service, Level::Connected, "") {
                ("attention", Some(issue))
            } else {
                ("ready", None)
            };
            out.push(json!({"service": service.id(), "name": service.title(), "status": status, "issue": issue}));
        }
        for (service, state) in &self.others {
            let (status, issue) = if !state.set_up {
                ("not_set_up", self.check_other(*service, Level::Configured))
            } else if let Some(issue) = self.check_other(*service, Level::Connected) {
                ("attention", Some(issue))
            } else {
                ("ready", None)
            };
            out.push(json!({"service": service.id(), "name": service.title(), "status": status, "issue": issue}));
        }
        if let Some(kind) = &self.twitch_type {
            if let Some(twitch) = out.iter_mut().find(|entry| entry["service"] == "twitch") {
                twitch["plan"] = json!(match kind.as_str() {
                    "" => "none",
                    other => other,
                });
            }
        }
        out
    }
}

/// The texts of `warnings`, for a row's hover and a form's banners.
pub fn messages(warnings: &[Warning]) -> Vec<String> {
    warnings.iter().map(|warning| warning.message.clone()).collect()
}

fn warnings_json(warnings: &[Warning]) -> Value {
    serde_json::to_value(warnings).unwrap_or_default()
}

/// What `item` of an action is, for a report line.
fn report_item(action: &crate::model::Action, kind: Kind, id: String, item: &Obj, warnings: &[Warning]) -> Value {
    let code = edit::kind(item) as u32;
    let table = if kind == Kind::Trigger {
        crate::types::TRIGGERS
    } else {
        crate::types::SUB_ACTIONS
    };
    json!({"actionId": action.id, "action": action.name, "group": action.group, "enabled": action.enabled,
        "kind": kind.id(), "id": id, "code": code, "type": crate::types::name(table, code).unwrap_or_default(),
        "warnings": warnings_json(warnings)})
}

/// The sub-actions nested in `sub` (a group's, an If's), as they are stored.
fn kids(sub: &Obj) -> impl Iterator<Item = &Obj> {
    sub.get("subActions")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_object)
}

fn collect_subs<'a>(
    ready: &Readiness,
    action: &crate::model::Action,
    list: impl Iterator<Item = &'a Obj>,
    path: &mut edit::Path,
    out: &mut Vec<Value>,
) {
    for (index, sub) in list.enumerate() {
        path.push(index);
        let warnings = ready.warnings(Kind::SubAction, sub);
        if !warnings.is_empty() {
            out.push(report_item(
                action,
                Kind::SubAction,
                super::actions::sub_id(sub, path),
                sub,
                &warnings,
            ));
        }
        collect_subs(ready, action, kids(sub), path, out);
        path.pop();
    }
}

/// Every trigger and sub-action that has a warning, with the services' own state and the totals.
/// `action` (an id or a name) and `kind` narrow it.
pub fn report(engine: &Engine, action: &str, kind: Option<Kind>) -> Result<Value, String> {
    let ready = Readiness::capture(engine);
    let config = engine.config();
    let wanted = action.trim().to_lowercase();
    let chosen: Vec<&crate::model::Action> = config
        .actions
        .actions
        .iter()
        .filter(|candidate| {
            wanted.is_empty() || candidate.id.to_lowercase() == wanted || candidate.name.to_lowercase() == wanted
        })
        .collect();
    if !wanted.is_empty() && chosen.is_empty() {
        return Err(format!("no action {action}"));
    }
    let mut items = Vec::new();
    for action in &chosen {
        if kind != Some(Kind::SubAction) {
            for (index, trigger) in action.triggers.iter().enumerate() {
                let warnings = ready.warnings(Kind::Trigger, trigger);
                if !warnings.is_empty() {
                    items.push(report_item(
                        action,
                        Kind::Trigger,
                        super::actions::trigger_id(trigger, index),
                        trigger,
                        &warnings,
                    ));
                }
            }
        }
        if kind != Some(Kind::Trigger) {
            collect_subs(&ready, action, action.sub_actions.iter(), &mut Vec::new(), &mut items);
        }
    }
    let count = |severity: &str| {
        items
            .iter()
            .flat_map(|item| item["warnings"].as_array().into_iter().flatten())
            .filter(|warning| warning["severity"] == severity)
            .count()
    };
    let (blockers, warnings) = (count("blocker"), count("warning"));
    let mut services = ready.services();
    // How many items each service holds back.
    for entry in &mut services {
        let id = entry["service"].clone();
        let affected = items
            .iter()
            .filter(|item| {
                item["warnings"]
                    .as_array()
                    .is_some_and(|list| list.iter().any(|warning| warning["service"] == id))
            })
            .count();
        entry["affected"] = affected.into();
    }
    Ok(
        json!({"summary": {"actions": chosen.len(), "items": items.len(), "blockers": blockers, "warnings": warnings},
        "services": services, "items": items}),
    )
}

/// What a trigger or sub-action type needs and what is wrong with it now, without an item.
pub fn type_report(engine: &Engine, kind: Kind, code: u32, connection: &str) -> Result<Value, String> {
    let table = if kind == Kind::Trigger {
        crate::types::TRIGGERS
    } else {
        crate::types::SUB_ACTIONS
    };
    let name = crate::types::name(table, code).ok_or_else(|| format!("no {} {code}", kind.id()))?;
    let ready = Readiness::capture(engine);
    let warnings = ready.for_code(kind, code, connection);
    Ok(json!({"kind": kind.id(), "code": code, "type": name,
        "needs": rules::needs_json(&rules::of(kind, code)), "ok": warnings.is_empty(),
        "warnings": warnings_json(&warnings)}))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ready(twitch: Platform, twitch_type: Option<&str>) -> Readiness {
        Readiness {
            platforms: vec![
                (Service::Twitch, twitch),
                (Service::Kick, Platform::default()),
                (Service::YouTube, Platform::default()),
                (Service::TikTok, Platform::default()),
            ],
            twitch_type: twitch_type.map(str::to_owned),
            ..Readiness::default()
        }
    }

    fn working() -> Platform {
        Platform {
            set_up: true,
            up: true,
            locked: false,
            login: None,
        }
    }

    fn codes(warnings: &[Warning]) -> Vec<&str> {
        warnings.iter().map(|warning| warning.code.as_str()).collect()
    }

    #[test]
    fn nothing_is_said_when_everything_is_ready_and_nothing_is_needed() {
        let ready = ready(working(), Some("partner"));
        assert!(ready.for_code(Kind::SubAction, 19, "").is_empty());
        assert!(ready.for_code(Kind::SubAction, 15, "").is_empty());
        assert!(ready.for_code(Kind::Trigger, 101, "").is_empty());
        // Local sub-actions never need a service.
        assert!(Readiness::default().for_code(Kind::SubAction, 1020, "").is_empty());
        assert!(Readiness::default().for_code(Kind::SubAction, 120, "").is_empty());
    }

    #[test]
    fn a_trigger_of_a_platform_with_no_login_and_no_channel_says_it_is_not_set_up() {
        let ready = ready(Platform::default(), None);
        let found = ready.for_code(Kind::Trigger, 133, "");
        assert_eq!(codes(&found), ["twitch_not_set_up"]);
        assert_eq!(found[0].severity, "blocker");
        assert_eq!(found[0].fix.page, "Accounts");
        assert_eq!(found[0].fix.action, "twitch.login");
    }

    #[test]
    fn a_channel_that_is_neither_affiliate_nor_partner_cannot_run_ads() {
        let ready = ready(working(), Some(""));
        let found = ready.for_code(Kind::SubAction, 19, "");
        assert_eq!(codes(&found), ["twitch_plan_ads"]);
        assert!(found[0].message.contains("Affiliate or Partner"));
        // Chatting, changing the title and raiding stay allowed.
        assert!(ready.for_code(Kind::SubAction, 15, "").is_empty());
        assert!(ready.for_code(Kind::Trigger, 101, "").is_empty());
        // The triggers and other sub-actions only a monetized channel has say so too.
        assert_eq!(codes(&ready.for_code(Kind::Trigger, 112, "")), ["twitch_plan_rewards"]);
        assert_eq!(
            codes(&ready.for_code(Kind::Trigger, 103, "")),
            ["twitch_plan_subscriptions"]
        );
        assert_eq!(
            codes(&ready.for_code(Kind::SubAction, 200, "")),
            ["twitch_plan_rewards"]
        );
    }

    #[test]
    fn an_affiliate_or_a_channel_not_read_yet_is_not_warned() {
        for kind in [Some("affiliate"), Some("partner"), None] {
            assert!(ready(working(), kind).for_code(Kind::SubAction, 19, "").is_empty());
        }
    }

    #[test]
    fn the_plan_is_not_blamed_while_the_account_is_not_working() {
        let broken = Platform {
            set_up: true,
            up: false,
            locked: false,
            login: Some(warning(
                "blocker",
                Service::Twitch,
                "disconnected",
                "Twitch isn't connected.".into(),
                PreflightFix::default(),
            )),
        };
        let found = ready(broken, Some("")).for_code(Kind::SubAction, 19, "");
        assert_eq!(codes(&found), ["twitch_disconnected"]);
    }

    #[test]
    fn a_disconnected_platform_is_a_warning_for_its_triggers() {
        let idle = Platform {
            set_up: true,
            up: false,
            locked: false,
            login: None,
        };
        let found = ready(idle.clone(), None).for_code(Kind::Trigger, 133, "");
        assert_eq!(codes(&found), ["twitch_disconnected"]);
        assert_eq!(found[0].fix.action, "twitch.connect");
        let locked = Platform { locked: true, ..idle };
        assert_eq!(
            codes(&ready(locked, None).for_code(Kind::Trigger, 133, "")),
            ["twitch_keyring_locked"]
        );
    }

    #[test]
    fn a_twitch_event_that_needs_the_account_is_not_there_for_a_channel_read_without_a_login() {
        let anonymous = Platform {
            set_up: true,
            up: true,
            locked: false,
            login: Some(warning(
                "blocker",
                Service::Twitch,
                "login_missing",
                "Twitch: broadcaster account is not logged in.".into(),
                PreflightFix::default(),
            )),
        };
        let ready = ready(anonymous, None);
        // Chat lines come through; follows need the login.
        assert!(ready.for_code(Kind::Trigger, 133, "").is_empty());
        assert_eq!(codes(&ready.for_code(Kind::Trigger, 101, "")), ["twitch_login_needed"]);
        // An action asks the API, which has no login to ask with.
        assert_eq!(
            codes(&ready.for_code(Kind::SubAction, 15, "")),
            ["twitch_login_missing"]
        );
    }

    #[test]
    fn connection_events_only_need_the_service_set_up() {
        let mut ready = Readiness::default();
        assert_eq!(codes(&ready.for_code(Kind::Trigger, 14001, "")), ["obs_not_set_up"]);
        ready.obs.push(Link {
            id: "main".into(),
            name: "Main".into(),
            up: false,
        });
        assert!(ready.for_code(Kind::Trigger, 14001, "").is_empty());
        assert_eq!(codes(&ready.for_code(Kind::Trigger, 14004, "")), ["obs_disconnected"]);
    }

    #[test]
    fn obs_sub_actions_check_the_connection_they_name() {
        let mut ready = Readiness::default();
        assert_eq!(codes(&ready.for_code(Kind::SubAction, 30, "")), ["obs_not_set_up"]);
        ready.obs.push(Link {
            id: "a".into(),
            name: "Stream PC".into(),
            up: true,
        });
        ready.obs.push(Link {
            id: "b".into(),
            name: "Backup".into(),
            up: false,
        });
        assert!(ready.for_code(Kind::SubAction, 30, "").is_empty());
        assert!(ready.for_code(Kind::SubAction, 30, "a").is_empty());
        let down = ready.for_code(Kind::SubAction, 30, "b");
        assert_eq!(codes(&down), ["obs_disconnected"]);
        assert!(down[0].message.contains("Backup"));
        assert_eq!(
            codes(&ready.for_code(Kind::SubAction, 30, "gone")),
            ["obs_connection_missing"]
        );
        assert_eq!(down[0].fix.page, "Stream Apps");
    }

    #[test]
    fn spotify_and_donation_services_report_their_own_state() {
        let mut ready = Readiness::default();
        ready.others.push((Service::Spotify, Simple::default()));
        ready.others.push((
            Service::StreamElements,
            Simple {
                set_up: true,
                up: false,
                login: false,
            },
        ));
        ready.others.push((
            Service::Kofi,
            Simple {
                set_up: true,
                up: true,
                login: true,
            },
        ));
        assert_eq!(
            codes(&ready.for_code(Kind::SubAction, 99840, "")),
            ["spotify_not_set_up"]
        );
        assert_eq!(codes(&ready.for_code(Kind::Trigger, 99809, "")), ["spotify_not_set_up"]);
        assert_eq!(
            codes(&ready.for_code(Kind::Trigger, 1201, "")),
            ["streamelements_disconnected"]
        );
        assert!(ready.for_code(Kind::Trigger, 1203, "").is_empty());
        assert!(ready.for_code(Kind::Trigger, 7001, "").is_empty());
        assert!(codes(&ready.for_code(Kind::Trigger, 8001, "")).is_empty());
    }

    #[test]
    fn a_warning_has_a_stable_shape_for_the_mcp() {
        let found = ready(Platform::default(), None).for_code(Kind::Trigger, 133, "");
        let json = warnings_json(&found);
        assert_eq!(json[0]["severity"], "blocker");
        assert_eq!(json[0]["code"], "twitch_not_set_up");
        assert_eq!(json[0]["service"], "twitch");
        assert!(json[0]["message"].as_str().is_some_and(|text| !text.is_empty()));
        assert_eq!(json[0]["fix"]["page"], "Accounts");
        assert_eq!(messages(&found).len(), 1);
    }

    fn engine(actions: serde_json::Value) -> std::rc::Rc<Engine> {
        crate::engine::tests::engine(actions)
    }

    #[test]
    fn a_fresh_install_warns_about_what_an_action_uses_and_the_report_lists_it() {
        let _lock = crate::TEST_LOCK.lock().unwrap_or_else(|error| error.into_inner());
        crate::engine::tests::run(async {
            let engine = engine(
                json!({"actions": [{"id": "ads", "name": "Ads", "enabled": true, "queue": crate::engine::DEFAULT_QUEUE,
                "triggers": [{"id": "t1", "type": 101, "enabled": true}, {"type": 401}],
                "subActions": [{"id": "s1", "type": 19, "enabled": true},
                    {"id": "s2", "type": 1020, "logLevel": 1, "message": "hi"},
                    {"id": "s3", "type": 128, "subActions": [{"id": "s4", "type": 30, "enabled": true}]}]}]}),
            );
            let report = report(&engine, "", None).unwrap();
            assert_eq!(report["summary"]["actions"], 1);
            let items = report["items"].as_array().unwrap();
            let ids: Vec<&str> = items.iter().map(|item| item["id"].as_str().unwrap()).collect();
            assert_eq!(ids, ["t1", "s1", "s4"]);
            assert_eq!(items[0]["warnings"][0]["code"], "twitch_login_missing");
            assert_eq!(items[1]["warnings"][0]["code"], "twitch_login_missing");
            assert_eq!(items[2]["warnings"][0]["code"], "obs_not_set_up");
            assert_eq!(items[2]["type"], "ObsSourceVisbility");
            assert_eq!(report["summary"]["items"], 3);
            let services = report["services"].as_array().unwrap();
            let twitch = services.iter().find(|entry| entry["service"] == "twitch").unwrap();
            assert_eq!(twitch["status"], "not_set_up");
            assert_eq!(twitch["affected"], 2);
            // Narrowing by name, by kind, and an unknown action.
            assert_eq!(
                super::report(&engine, "ads", Some(Kind::Trigger)).unwrap()["items"]
                    .as_array()
                    .unwrap()
                    .len(),
                1
            );
            assert_eq!(
                super::report(&engine, "ADS", Some(Kind::SubAction)).unwrap()["items"]
                    .as_array()
                    .unwrap()
                    .len(),
                2
            );
            assert!(super::report(&engine, "nope", None).is_err());
        });
    }

    #[test]
    fn a_type_can_be_checked_without_an_item() {
        let _lock = crate::TEST_LOCK.lock().unwrap_or_else(|error| error.into_inner());
        crate::engine::tests::run(async {
            let engine = engine(json!({"actions": []}));
            let found = type_report(&engine, Kind::SubAction, 19, "").unwrap();
            assert_eq!(found["type"], "RunCommercial");
            assert_eq!(found["ok"], false);
            assert_eq!(found["needs"][0]["service"], "twitch");
            let local = type_report(&engine, Kind::SubAction, 1020, "").unwrap();
            assert_eq!(local["ok"], true);
            assert!(type_report(&engine, Kind::Trigger, u32::MAX, "").is_err());
        });
    }

    #[test]
    fn a_row_has_warnings_when_any_nested_item_does() {
        let action = |subs: serde_json::Value, triggers: serde_json::Value| -> crate::model::Action {
            serde_json::from_value(json!({"id": "a", "name": "A", "subActions": subs, "triggers": triggers})).unwrap()
        };
        let ready = ready(working(), Some("partner"));
        assert!(!ready.action_has_warnings(&action(json!([]), json!([]))));
        // An OBS sub-action two groups deep, with no OBS set up.
        let deep = json!([{"type": 1020}, {"type": 128, "subActions": [
            {"type": 1020}, {"type": 128, "subActions": [{"type": 30}]}]}]);
        assert!(ready.action_has_warnings(&action(deep, json!([]))));
        let local = json!([{"type": 128, "subActions": [{"type": 1020}, "not an object"]}]);
        assert!(!ready.action_has_warnings(&action(local, json!([]))));
        // A trigger alone is enough.
        let fresh = Readiness::default();
        assert!(fresh.action_has_warnings(&action(json!([]), json!([{"type": 14004}]))));
    }
}
