//! OBS Studio preflight readiness checks, multi-output platform mapping, and the companion Python
//! script for OBS (`Tools -> Scripts`).
//!
//! When OBS connects or starts streaming (either the native main output or Aitum Multistream /
//! Stream Suite / Vertical / Multi-RTMP), Streamerz checks whether every required output's platform
//! is connected, authenticated, and ready on the broadcaster's own channel. Credentials, stream
//! keys, and raw RTMP URLs never enter the preflight report or the companion script.

use crate::{
    core::outputs::{self, Inventory, ObsOutputPref, Output},
    engine::Engine,
    i18n::{tr, trf},
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{
    cell::RefCell,
    collections::HashMap,
    path::{Path, PathBuf},
    rc::Rc,
    time::{Duration, Instant},
};

pub const DOMAIN: &str = "obs_companion";
pub const SCRIPT_FILE_NAME: &str = "streamerz_obs_companion.py";

const NOTIFY_COOLDOWN: Duration = Duration::from_secs(15);

/// Select dropdown choices for an OBS output's platform mapping:
/// 0 = Auto, 1 = Twitch, 2 = Kick, 3 = YouTube, 4 = TikTok, 5 = Ignore
pub const OUTPUT_PLATFORM_VALUES: [&str; 6] = ["auto", "twitch", "kick", "youtube", "tiktok", "ignore"];

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PreflightFix {
    pub label: String,
    pub page: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub action: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub dialog: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub form: String,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PreflightIssue {
    /// "blocker" or "warning"
    pub severity: String,
    pub code: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub platform: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output_name: Option<String>,
    pub message: String,
    pub fix: PreflightFix,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OutputReadiness {
    pub id: String,
    pub connection_id: String,
    pub connection_name: String,
    pub source: String,
    pub key: String,
    pub name: String,
    pub platform: Option<String>,
    pub platform_source: Option<String>,
    pub required: bool,
    pub active: Option<bool>,
    pub reconnecting: Option<bool>,
    pub vertical: Option<bool>,
    pub controllable: bool,
    pub limitation: Option<String>,
    /// "ready", "warning", "blocked", or "ignored"
    pub status: String,
    pub detail: String,
    /// Index into OUTPUT_PLATFORM_VALUES (0..=5)
    pub choice_index: usize,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PreflightReport {
    pub ready: bool,
    /// "ready", "warning", or "blocked"
    pub status: String,
    pub summary: String,
    pub obs_configured: bool,
    pub obs_connected: bool,
    pub outputs: Vec<OutputReadiness>,
    pub issues: Vec<PreflightIssue>,
}

thread_local! {
    static LAST_REPORT: RefCell<Option<PreflightReport>> = const { RefCell::new(None) };
    static LAST_NOTIFIED: RefCell<HashMap<String, Instant>> = RefCell::default();
}

pub fn last_report() -> Option<PreflightReport> {
    LAST_REPORT.with_borrow(|r| r.clone())
}

pub fn store_report(report: PreflightReport) {
    LAST_REPORT.with_borrow_mut(|r| *r = Some(report));
}

#[cfg(test)]
pub fn clear_state() {
    LAST_REPORT.with_borrow_mut(|r| *r = None);
    LAST_NOTIFIED.with_borrow_mut(|m| m.clear());
}

fn platform_display(slug: &str) -> &'static str {
    match slug {
        "twitch" => "Twitch",
        "kick" => "Kick",
        "youtube" => "YouTube",
        "tiktok" => "TikTok",
        _ => "",
    }
}

fn choice_index_for(output: &Output, prefs: &[ObsOutputPref], conn_id: &str) -> usize {
    if let Some(pref) = prefs
        .iter()
        .find(|p| p.connection_id == conn_id && p.source == output.source && p.key == output.key)
    {
        if !pref.required {
            return 5;
        }
        if let Some(plat) = pref.platform.as_deref() {
            return match plat {
                "twitch" => 1,
                "kick" => 2,
                "youtube" => 3,
                "tiktok" => 4,
                "" => 5,
                _ => 0,
            };
        }
    }
    0
}

pub fn option_labels(auto_platform: Option<&str>) -> Vec<String> {
    let auto_label = match auto_platform
        .and_then(|p| (!p.is_empty()).then(|| platform_display(p)))
        .filter(|s| !s.is_empty())
    {
        Some(name) => trf("preflight.platform_auto_detected", &[("platform", name)]),
        None => tr("preflight.platform_auto_unknown").to_owned(),
    };
    vec![
        auto_label,
        "Twitch".to_owned(),
        "Kick".to_owned(),
        "YouTube".to_owned(),
        "TikTok".to_owned(),
        tr("preflight.platform_ignore").to_owned(),
    ]
}

fn same(a: &str, b: &str) -> bool {
    let norm = |s: &str| s.trim().trim_start_matches('@').replace(' ', "").to_lowercase();
    !a.trim().is_empty() && norm(a) == norm(b)
}

fn token_ready(access: &str, refresh: &str, expires_at: u64) -> bool {
    !access.is_empty() && (!crate::oauth::stale(expires_at) || !refresh.is_empty())
}

/// Evaluate readiness of a single platform ("twitch", "kick", "youtube", "tiktok") in `engine`.
/// Returns `None` if ready without issues, or `Some((severity, code, message, fix))`.
pub fn evaluate_platform(
    engine: &Engine,
    platform: &str,
) -> Option<(&'static str, &'static str, String, PreflightFix)> {
    let c = engine.config();
    let locked = crate::secrets::is_locked();
    let accounts_fix = |action: &str| PreflightFix {
        label: tr("nav.page.accounts").to_owned(),
        page: "Accounts".to_owned(),
        action: action.to_owned(),
        dialog: String::new(),
        form: String::new(),
    };
    match platform {
        "twitch" => {
            let b = c.twitch.broadcaster.as_ref();
            if b.is_none() {
                return Some((
                    "blocker",
                    "twitch_login_missing",
                    trf("preflight.issue.login_missing", &[("platform", "Twitch")]),
                    accounts_fix("twitch.login"),
                ));
            }
            let b = b.unwrap();
            if b.token.access_token.is_empty() {
                if locked {
                    return Some((
                        "blocker",
                        "keyring_locked",
                        tr("preflight.issue.keyring_locked").to_owned(),
                        accounts_fix("unlock"),
                    ));
                }
                return Some((
                    "blocker",
                    "twitch_token_expired",
                    trf("preflight.issue.token_expired", &[("platform", "Twitch")]),
                    accounts_fix("twitch.login"),
                ));
            }
            if !token_ready(&b.token.access_token, &b.token.refresh_token, b.token.expires_at) {
                return Some((
                    "blocker",
                    "twitch_token_expired",
                    trf("preflight.issue.token_expired", &[("platform", "Twitch")]),
                    accounts_fix("twitch.login"),
                ));
            }
            if crate::twitch::BROADCASTER_SCOPES
                .iter()
                .any(|w| !b.token.scope.iter().any(|g| g == w))
            {
                return Some((
                    "warning",
                    "twitch_permissions_missing",
                    trf("preflight.issue.permissions_missing", &[("platform", "Twitch")]),
                    accounts_fix("twitch.login"),
                ));
            }
            if crate::twitch::other_channel(&c) {
                return Some((
                    "warning",
                    "twitch_not_owner",
                    trf(
                        "stream.not_owner",
                        &[("account", &b.display_name), ("channel", c.twitch.channel.trim())],
                    ),
                    accounts_fix("twitch.channel"),
                ));
            }
            if engine.twitch().is_none() {
                return Some((
                    "blocker",
                    "twitch_disconnected",
                    trf("stream.not_connected", &[("where", "Twitch")]),
                    accounts_fix("twitch.connect"),
                ));
            }
            None
        }
        "kick" => {
            let b = c.kick.broadcaster.as_ref();
            if b.is_none() {
                return Some((
                    "blocker",
                    "kick_login_missing",
                    trf("preflight.issue.login_missing", &[("platform", "Kick")]),
                    accounts_fix("kick.login"),
                ));
            }
            let b = b.unwrap();
            if b.token.access_token.is_empty() && b.token.refresh_token.is_empty() {
                if locked {
                    return Some((
                        "blocker",
                        "keyring_locked",
                        tr("preflight.issue.keyring_locked").to_owned(),
                        accounts_fix("unlock"),
                    ));
                }
                return Some((
                    "blocker",
                    "kick_token_expired",
                    trf("preflight.issue.token_expired", &[("platform", "Kick")]),
                    accounts_fix("kick.login"),
                ));
            }
            if !token_ready(&b.token.access_token, &b.token.refresh_token, b.token.expires_at)
                || (crate::oauth::stale(b.token.expires_at)
                    && (c.kick.client_id.is_empty() || c.kick.client_secret.is_empty()))
            {
                return Some((
                    "blocker",
                    "kick_token_expired",
                    trf("preflight.issue.token_expired", &[("platform", "Kick")]),
                    accounts_fix("kick.login"),
                ));
            }
            if !c.kick.channel.trim().is_empty() && !same(&b.username, &c.kick.channel) {
                return Some((
                    "warning",
                    "kick_not_owner",
                    trf(
                        "stream.not_owner",
                        &[("account", &b.username), ("channel", c.kick.channel.trim())],
                    ),
                    accounts_fix("kick.channel"),
                ));
            }
            if engine.kick().is_none() {
                return Some((
                    "blocker",
                    "kick_disconnected",
                    trf("stream.not_connected", &[("where", "Kick")]),
                    accounts_fix("kick.connect"),
                ));
            }
            None
        }
        "youtube" => {
            let b = c.youtube.broadcaster.as_ref();
            if b.is_none() {
                return Some((
                    "blocker",
                    "youtube_login_missing",
                    trf("preflight.issue.login_missing", &[("platform", "YouTube")]),
                    accounts_fix("youtube.login"),
                ));
            }
            let b = b.unwrap();
            if b.token.access_token.is_empty() && b.token.refresh_token.is_empty() {
                if locked {
                    return Some((
                        "blocker",
                        "keyring_locked",
                        tr("preflight.issue.keyring_locked").to_owned(),
                        accounts_fix("unlock"),
                    ));
                }
                return Some((
                    "blocker",
                    "youtube_token_expired",
                    trf("preflight.issue.token_expired", &[("platform", "YouTube")]),
                    accounts_fix("youtube.login"),
                ));
            }
            if !token_ready(&b.token.access_token, &b.token.refresh_token, b.token.expires_at)
                || (crate::oauth::stale(b.token.expires_at)
                    && (c.youtube.client_id.is_empty() || c.youtube.client_secret.is_empty()))
            {
                return Some((
                    "blocker",
                    "youtube_token_expired",
                    trf("preflight.issue.token_expired", &[("platform", "YouTube")]),
                    accounts_fix("youtube.login"),
                ));
            }
            if !c.youtube.channel_id.trim().is_empty()
                && ![&b.channel_id, &b.name, &b.handle]
                    .iter()
                    .any(|mine| same(mine, &c.youtube.channel_id))
            {
                return Some((
                    "warning",
                    "youtube_not_owner",
                    trf(
                        "stream.not_owner",
                        &[("account", &b.name), ("channel", c.youtube.channel_id.trim())],
                    ),
                    accounts_fix("youtube.channel"),
                ));
            }
            if engine.youtube().is_none() {
                return Some((
                    "blocker",
                    "youtube_disconnected",
                    trf("stream.not_connected", &[("where", "YouTube")]),
                    accounts_fix("youtube.connect"),
                ));
            }
            let yt = super::stream::youtube_preflight_snapshot();
            if yt.nothing {
                return Some((
                    "warning",
                    "youtube_no_broadcast",
                    tr("stream.youtube_nothing").to_owned(),
                    PreflightFix {
                        label: tr("nav.page.stream").to_owned(),
                        page: "Stream".to_owned(),
                        action: "refresh".to_owned(),
                        dialog: String::new(),
                        form: String::new(),
                    },
                ));
            }
            if yt.privacy == "private" {
                return Some((
                    "warning",
                    "youtube_private",
                    tr("stream.youtube_private").to_owned(),
                    PreflightFix {
                        label: tr("btn.edit_stream").to_owned(),
                        page: "Stream".to_owned(),
                        action: String::new(),
                        dialog: String::new(),
                        form: "stream_info.youtube".to_owned(),
                    },
                ));
            }
            None
        }
        "tiktok" => {
            let bridge = &engine.tiktok_bridge;
            if c.tiktok.channel.trim().is_empty() && !bridge.is_connected() {
                return Some((
                    "blocker",
                    "tiktok_channel_missing",
                    trf("preflight.issue.login_missing", &[("platform", "TikTok")]),
                    accounts_fix("tiktok.channel"),
                ));
            }
            if !bridge.is_connected() || !bridge.ready() {
                return Some((
                    "warning",
                    "tiktok_bridge_not_ready",
                    tr("stream.tiktok_bridge").to_owned(),
                    PreflightFix {
                        label: tr("btn.set_up").to_owned(),
                        page: "Accounts".to_owned(),
                        action: "tiktok.sending".to_owned(),
                        dialog: "tiktok_sending".to_owned(),
                        form: String::new(),
                    },
                ));
            }
            if !bridge.user().is_empty()
                && !c.tiktok.channel.trim().is_empty()
                && !same(&bridge.user(), &c.tiktok.channel)
            {
                return Some((
                    "warning",
                    "tiktok_not_owner",
                    trf(
                        "stream.not_owner",
                        &[("account", &bridge.user()), ("channel", c.tiktok.channel.trim())],
                    ),
                    accounts_fix("tiktok.channel"),
                ));
            }
            None
        }
        _ => None,
    }
}

/// What stops a platform from being ready to go live: `evaluate_platform`, then whether its chat and
/// events are being read. A handle stays after Disconnect (its API still answers), so that alone
/// doesn't say it reads; YouTube waiting on its broadcast counts as reading.
fn stream_issue(engine: &Engine, platform: &str) -> Option<(&'static str, &'static str, String, PreflightFix)> {
    let found = evaluate_platform(engine, platform);
    if found.as_ref().is_some_and(|(severity, ..)| *severity == "blocker") {
        return found;
    }
    let (name, code, up) = match platform {
        "twitch" => ("Twitch", "twitch_disconnected", engine.twitch_connected()),
        "kick" => ("Kick", "kick_disconnected", engine.kick_connected()),
        "youtube" => (
            "YouTube",
            "youtube_disconnected",
            engine.youtube_connected() || engine.youtube_standby(),
        ),
        _ => return found,
    };
    if up {
        return found;
    }
    Some((
        "blocker",
        code,
        trf("stream.not_connected", &[("where", name)]),
        PreflightFix {
            label: tr("nav.page.accounts").to_owned(),
            page: "Accounts".to_owned(),
            action: format!("{platform}.connect"),
            ..Default::default()
        },
    ))
}

/// Build a complete `PreflightReport` from already-discovered `inventories` (and optional main
/// stream service/server hints passed by the OBS companion script).
pub fn evaluate_inventories(
    engine: &Engine,
    inventories: &[(String, Inventory)],
    main_service_hint: Option<&str>,
    main_server_hint: Option<&str>,
) -> PreflightReport {
    let config = engine.config();
    let obs_configured = !config.obs.connections.is_empty();
    let obs_connected = !inventories.is_empty()
        || engine.obs().is_some_and(|obs| {
            obs.is_connected(None)
                || config
                    .obs
                    .connections
                    .iter()
                    .any(|c| obs.is_connected(c["id"].as_str()))
        });

    let mut issues = Vec::new();
    let mut outputs = Vec::new();

    if !obs_configured && !obs_connected {
        issues.push(PreflightIssue {
            severity: "blocker".into(),
            code: "obs_not_configured".into(),
            platform: None,
            output_id: None,
            output_name: None,
            message: tr("preflight.issue.obs_not_configured").into(),
            fix: PreflightFix {
                label: tr("nav.page.stream_apps").into(),
                page: "Stream Apps".into(),
                action: String::new(),
                dialog: String::new(),
                form: String::new(),
            },
        });
    } else if !obs_connected {
        issues.push(PreflightIssue {
            severity: "blocker".into(),
            code: "obs_not_connected".into(),
            platform: None,
            output_id: None,
            output_name: None,
            message: tr("preflight.issue.obs_not_connected").into(),
            fix: PreflightFix {
                label: tr("nav.page.stream_apps").into(),
                page: "Stream Apps".into(),
                action: "connect".into(),
                dialog: String::new(),
                form: String::new(),
            },
        });
    }

    let mut youtube_outputs = 0usize;

    for (conn_name, inv) in inventories {
        for raw in &inv.outputs {
            let mut o = raw.clone();
            // If OBS main stream has no platform from GetStreamServiceSettings, fall back to companion hints.
            if o.source == "obs" && o.platform.is_none() {
                if let Some(p) = main_service_hint.and_then(outputs::platform) {
                    o.platform = Some(p);
                    o.platform_source = Some("service".into());
                } else if let Some(p) = main_server_hint.and_then(outputs::platform_from_server) {
                    o.platform = Some(p);
                    o.platform_source = Some("server".into());
                }
            }
            let choice_index = choice_index_for(&o, &config.obs_output_prefs, &inv.connection_id);
            if let Some(pref) = config
                .obs_output_prefs
                .iter()
                .find(|p| p.connection_id == inv.connection_id && p.source == o.source && p.key == o.key)
            {
                o.required = pref.required;
                if let Some(plat) = pref.platform.as_deref() {
                    if plat.is_empty() {
                        o.platform = None;
                        o.platform_source = Some("user".into());
                    } else {
                        o.platform = Some(plat.to_owned());
                        o.platform_source = Some("user".into());
                    }
                }
            }

            let display_name = if o.source == "obs" {
                tr("stream.obs_main_output").to_owned()
            } else {
                o.name.clone()
            };

            if !o.required {
                outputs.push(OutputReadiness {
                    id: o.id.clone(),
                    connection_id: inv.connection_id.clone(),
                    connection_name: conn_name.clone(),
                    source: o.source.clone(),
                    key: o.key.clone(),
                    name: display_name,
                    platform: o.platform.clone(),
                    platform_source: o.platform_source.clone(),
                    required: false,
                    active: o.active,
                    reconnecting: o.reconnecting,
                    vertical: o.vertical,
                    controllable: o.controllable,
                    limitation: o.limitation.clone(),
                    status: "ignored".into(),
                    detail: tr("preflight.state_ignored").into(),
                    choice_index,
                });
                continue;
            }

            let Some(plat) = o.platform.clone().filter(|p| !p.is_empty()) else {
                let msg = trf("preflight.issue.platform_unmapped", &[("output", &display_name)]);
                issues.push(PreflightIssue {
                    severity: "warning".into(),
                    code: "output_platform_unmapped".into(),
                    platform: None,
                    output_id: Some(o.id.clone()),
                    output_name: Some(display_name.clone()),
                    message: msg.clone(),
                    fix: PreflightFix {
                        label: tr("preflight.btn_companion").into(),
                        page: "Stream".into(),
                        action: String::new(),
                        dialog: DOMAIN.into(),
                        form: String::new(),
                    },
                });
                outputs.push(OutputReadiness {
                    id: o.id.clone(),
                    connection_id: inv.connection_id.clone(),
                    connection_name: conn_name.clone(),
                    source: o.source.clone(),
                    key: o.key.clone(),
                    name: display_name,
                    platform: None,
                    platform_source: o.platform_source.clone(),
                    required: true,
                    active: o.active,
                    reconnecting: o.reconnecting,
                    vertical: o.vertical,
                    controllable: o.controllable,
                    limitation: o.limitation.clone(),
                    status: "warning".into(),
                    detail: tr("preflight.state_unmapped").into(),
                    choice_index,
                });
                continue;
            };

            if plat == "youtube" {
                youtube_outputs += 1;
            }

            let (status, detail) = match stream_issue(engine, &plat) {
                Some((severity, code, message, fix)) => {
                    issues.push(PreflightIssue {
                        severity: severity.into(),
                        code: code.into(),
                        platform: Some(plat.clone()),
                        output_id: Some(o.id.clone()),
                        output_name: Some(display_name.clone()),
                        message: message.clone(),
                        fix,
                    });
                    (
                        if severity == "blocker" { "blocked" } else { "warning" }.to_owned(),
                        message,
                    )
                }
                None => (
                    "ready".to_owned(),
                    trf("preflight.output_ready_on", &[("platform", platform_display(&plat))]),
                ),
            };

            outputs.push(OutputReadiness {
                id: o.id.clone(),
                connection_id: inv.connection_id.clone(),
                connection_name: conn_name.clone(),
                source: o.source.clone(),
                key: o.key.clone(),
                name: display_name,
                platform: Some(plat),
                platform_source: o.platform_source.clone(),
                required: true,
                active: o.active,
                reconnecting: o.reconnecting,
                vertical: o.vertical,
                controllable: o.controllable,
                limitation: o.limitation.clone(),
                status,
                detail,
                choice_index,
            });
        }
    }

    if youtube_outputs > 1 {
        issues.push(PreflightIssue {
            severity: "warning".into(),
            code: "youtube_multi_output".into(),
            platform: Some("youtube".into()),
            output_id: None,
            output_name: None,
            message: trf(
                "preflight.issue.youtube_multi_output",
                &[("count", &youtube_outputs.to_string())],
            ),
            fix: PreflightFix {
                label: tr("nav.page.stream").into(),
                page: "Stream".into(),
                action: String::new(),
                dialog: String::new(),
                form: String::new(),
            },
        });
    }

    // If OBS has no discovered outputs yet (e.g., disconnected), still check any configured platforms
    // so the user gets a complete readiness picture right away.
    if outputs.is_empty() {
        for (slug, shown) in [
            (
                "twitch",
                config.twitch.broadcaster.is_some() || !config.twitch.channel.trim().is_empty(),
            ),
            (
                "kick",
                config.kick.broadcaster.is_some() || !config.kick.channel.trim().is_empty(),
            ),
            (
                "youtube",
                config.youtube.broadcaster.is_some() || !config.youtube.channel_id.trim().is_empty(),
            ),
            (
                "tiktok",
                !config.tiktok.channel.trim().is_empty() || engine.tiktok_bridge.is_connected(),
            ),
        ] {
            if shown {
                if let Some((severity, code, message, fix)) = stream_issue(engine, slug) {
                    issues.push(PreflightIssue {
                        severity: severity.into(),
                        code: code.into(),
                        platform: Some(slug.into()),
                        output_id: None,
                        output_name: None,
                        message,
                        fix,
                    });
                }
            }
        }
    }

    let blockers = issues.iter().filter(|i| i.severity == "blocker").count();
    let warnings = issues.iter().filter(|i| i.severity != "blocker").count();
    let required_count = outputs.iter().filter(|o| o.required).count();

    let (ready, status, summary) = if blockers > 0 {
        let first = issues
            .iter()
            .find(|i| i.severity == "blocker")
            .map(|i| i.message.as_str())
            .unwrap_or_default();
        (
            false,
            "blocked".to_owned(),
            trf(
                "preflight.summary_blocked",
                &[("count", &blockers.to_string()), ("first", first)],
            ),
        )
    } else if warnings > 0 {
        let first = issues.first().map(|i| i.message.as_str()).unwrap_or_default();
        (
            true,
            "warning".to_owned(),
            trf(
                "preflight.summary_warning",
                &[("count", &warnings.to_string()), ("first", first)],
            ),
        )
    } else {
        (
            true,
            "ready".to_owned(),
            trf(
                "preflight.summary_ready",
                &[("count", &required_count.max(1).to_string())],
            ),
        )
    };

    let report = PreflightReport {
        ready,
        status,
        summary,
        obs_configured,
        obs_connected,
        outputs,
        issues,
    };
    store_report(report.clone());
    report
}

/// Discover outputs across all connected OBS instances.
pub async fn discover_all(engine: &Engine) -> Vec<(String, Inventory)> {
    let Some(obs) = engine.obs() else {
        return Vec::new();
    };
    let conns: Vec<crate::obs::ObsConnInfo> = engine
        .config()
        .obs
        .connections
        .iter()
        .map(crate::obs::ObsConnInfo::from_obj)
        .collect();
    let targets: Vec<(String, String)> = if conns.is_empty() {
        if obs.is_connected(None) {
            vec![("default".to_owned(), "OBS".to_owned())]
        } else {
            Vec::new()
        }
    } else {
        conns
            .into_iter()
            .filter(|c| obs.is_connected(Some(&c.id)))
            .map(|c| (c.id, c.name))
            .collect()
    };
    let mut out = Vec::with_capacity(targets.len());
    let prefs = engine.config().obs_output_prefs.clone();
    for (id, name) in targets {
        if let Ok(mut inv) = outputs::discover(obs.as_ref(), &id).await {
            outputs::apply_prefs(&mut inv, &prefs);
            out.push((name, inv));
        }
    }
    out
}

/// Run an on-demand preflight check, optionally showing a desktop notification if issues are found.
pub async fn check(engine: &Rc<Engine>, req: &Value) -> Result<PreflightReport, String> {
    let inventories = discover_all(engine).await;
    let main_service = req["mainService"].as_str().filter(|s| !s.is_empty());
    let main_server = req["mainServer"].as_str().filter(|s| !s.is_empty());
    let report = evaluate_inventories(engine, &inventories, main_service, main_server);
    if req["notify"] == true && !report.issues.is_empty() {
        notify_report(engine, "companion", &report, true);
    }
    super::stream::obs_changed();
    Ok(report)
}

fn should_notify(key: &str) -> bool {
    LAST_NOTIFIED.with_borrow_mut(|map| {
        if map.get(key).is_some_and(|at| at.elapsed() < NOTIFY_COOLDOWN) {
            return false;
        }
        map.insert(key.to_owned(), Instant::now());
        true
    })
}

fn notify_report(engine: &Rc<Engine>, key: &str, report: &PreflightReport, include_warnings: bool) {
    let has_blocker = report.issues.iter().any(|i| i.severity == "blocker");
    if !has_blocker && (!include_warnings || report.issues.is_empty()) {
        return;
    }
    if !should_notify(key) {
        return;
    }
    let title = if has_blocker {
        tr("preflight.toast_blocked_title").to_owned()
    } else {
        tr("preflight.toast_warning_title").to_owned()
    };
    let text = report.summary.clone();
    let e = engine.clone();
    crate::notify::toast(
        crate::notify::Toast {
            id: format!("preflight:{key}"),
            title,
            text,
            attribution: "Streamerz".into(),
            icon_path: String::new(),
        },
        move |_| {
            if e.config().window.tab != "Stream" {
                e.change(|c| c.window.tab = "Stream".into());
            }
            super::stream::opened();
            crate::host::show();
            crate::host::changed();
        },
    );
}

/// Called when an OBS connection finishes `OP_IDENTIFIED`. Discovers outputs and warns if blockers exist.
pub fn on_obs_connected(engine: &Rc<Engine>, connection_id: &str) {
    let engine = engine.clone();
    let conn_id = connection_id.to_owned();
    tokio::task::spawn_local(async move {
        let inventories = discover_all(&engine).await;
        let report = evaluate_inventories(&engine, &inventories, None, None);
        // On initial OBS connection, only toast if there is a hard blocker on a required output.
        if report
            .issues
            .iter()
            .any(|i| i.severity == "blocker" && i.code != "obs_not_connected")
        {
            notify_report(&engine, &format!("connect:{conn_id}"), &report, false);
        }
        super::stream::obs_changed();
    });
}

/// Called when OBS begins streaming (`OBS_WEBSOCKET_OUTPUT_STARTING` or Aitum `output_started`).
pub fn on_obs_stream_starting(engine: &Rc<Engine>, connection_id: &str, vendor_output: Option<&str>) {
    let engine = engine.clone();
    let conn_id = connection_id.to_owned();
    let vendor_name = vendor_output.map(str::to_owned);
    tokio::task::spawn_local(async move {
        let inventories = discover_all(&engine).await;
        let report = evaluate_inventories(&engine, &inventories, None, None);
        let key = match vendor_name.as_deref() {
            Some(name) if !name.is_empty() => format!("start:{conn_id}:{name}"),
            _ => format!("start:{conn_id}"),
        };
        notify_report(&engine, &key, &report, true);
        super::stream::obs_changed();
    });
}

/// Update the persisted platform override / required flag for an OBS output.
/// `choice_index`: 0 = Auto, 1 = Twitch, 2 = Kick, 3 = YouTube, 4 = TikTok, 5 = Ignore.
pub fn set_output_choice(
    engine: &Rc<Engine>,
    connection_id: &str,
    source: &str,
    key: &str,
    choice_index: usize,
) -> Result<(), String> {
    if choice_index >= OUTPUT_PLATFORM_VALUES.len() {
        return Err("Invalid output platform choice".into());
    }
    if connection_id.is_empty() || source.is_empty() || key.is_empty() {
        return Err("Output identity is required".into());
    }
    let (platform, required) = match OUTPUT_PLATFORM_VALUES[choice_index] {
        "auto" => (None, true),
        "ignore" => (Some(String::new()), false),
        plat => (Some(plat.to_owned()), true),
    };
    engine.change(|config| {
        config
            .obs_output_prefs
            .retain(|p| !(p.connection_id == connection_id && p.source == source && p.key == key));
        if platform.is_some() || !required {
            config.obs_output_prefs.push(ObsOutputPref {
                connection_id: connection_id.to_owned(),
                source: source.to_owned(),
                key: key.to_owned(),
                platform,
                required,
            });
        }
    });
    let inventories = super::stream::cached_obs_inventories();
    if !inventories.is_empty() {
        let _ = evaluate_inventories(engine, &inventories, None, None);
    }
    super::stream::obs_changed();
    crate::host::changed();
    Ok(())
}

/// Set an output's platform override by output ID JSON (`["conn","source","key"]`) and choice index or slug.
pub fn set_output_by_id(engine: &Rc<Engine>, output_id: &str, value: &Value) -> Result<(), String> {
    let parsed: Vec<String> = serde_json::from_str(output_id).map_err(|_| "Invalid output ID".to_owned())?;
    if parsed.len() != 3 {
        return Err("Invalid output ID".into());
    }
    let choice_index = if let Some(idx) = value.as_u64() {
        idx as usize
    } else if let Some(slug) = value.as_str() {
        OUTPUT_PLATFORM_VALUES
            .iter()
            .position(|v| v.eq_ignore_ascii_case(slug))
            .ok_or_else(|| format!("Unknown platform option {slug}"))?
    } else {
        return Err("Platform choice must be an index or slug".into());
    };
    set_output_choice(engine, &parsed[0], &parsed[1], &parsed[2], choice_index)
}

/// Where the companion Python script for OBS is saved on disk.
pub fn companion_script_path() -> PathBuf {
    crate::config_path()
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .join(SCRIPT_FILE_NAME)
}

/// The command line the OBS companion script uses to start Streamerz in the background.
pub fn launch_command_args() -> Vec<String> {
    #[cfg(not(windows))]
    if let Some(id) = crate::sandbox::app_id() {
        return vec!["flatpak".into(), "run".into(), id, "--hidden".into()];
    }
    let exe = crate::update::exe()
        .map(|p| p.display().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "Streamerz".into());
    vec![exe, "--hidden".into()]
}

fn ws_url(config: &crate::Config) -> String {
    let host = match config.websockets.address.trim() {
        "" | "0.0.0.0" | "*" | "::" => "127.0.0.1",
        h => h,
    };
    let endpoint = if config.websockets.endpoint.starts_with('/') {
        config.websockets.endpoint.clone()
    } else {
        format!("/{}", config.websockets.endpoint)
    };
    format!("ws://{}:{}{}", host, config.websockets.port, endpoint)
}

/// Generate the self-contained, zero-dependency Python 3 OBS companion script.
pub fn companion_script(config: &crate::Config) -> String {
    let ws = ws_url(config);
    let cmd_json =
        serde_json::to_string(&launch_command_args()).unwrap_or_else(|_| "[\"Streamerz\", \"--hidden\"]".into());
    let ws_json = serde_json::to_string(&ws).unwrap_or_else(|_| "\"ws://127.0.0.1:8080/\"".into());

    format!(
        r#"# Streamerz Companion Script for OBS Studio (Tools -> Scripts)
# Generated by Streamerz. Zero external dependencies (Python 3 standard library only).
# 1. Ensures Streamerz is running whenever OBS Studio opens.
# 2. Runs a pre-flight readiness check across all streaming outputs when a stream starts.

import base64
import hashlib
import json
import os
import socket
import subprocess
import sys
import threading
import time
import urllib.parse

try:
    import obspython as obs
except ImportError:
    obs = None

DEFAULT_WS_URL = {ws_json}
DEFAULT_LAUNCH_CMD = {cmd_json}

STATE = {{
    "auto_launch": True,
    "launch_hidden": True,
    "check_on_start": True,
    "strict_mode": False,
    "ws_url": DEFAULT_WS_URL,
    "launch_cmd": DEFAULT_LAUNCH_CMD[0] if DEFAULT_LAUNCH_CMD else "Streamerz",
}}


def _log(msg):
    print(f"[Streamerz Companion] {{msg}}")


def _parse_ws_url(url):
    parsed = urllib.parse.urlparse(url if "://" in url else f"ws://{{url}}")
    host = parsed.hostname or "127.0.0.1"
    if host in ("0.0.0.0", "*", "::"):
        host = "127.0.0.1"
    port = parsed.port or 8080
    path = parsed.path or "/"
    return host, port, path


def _recv_exact(sock, n):
    buf = bytearray()
    while len(buf) < n:
        chunk = sock.recv(n - len(buf))
        if not chunk:
            raise ConnectionError("WebSocket closed early")
        buf.extend(chunk)
    return bytes(buf)


def _read_ws_frame(sock):
    head = _recv_exact(sock, 2)
    opcode = head[0] & 0x0F
    masked = (head[1] & 0x80) != 0
    length = head[1] & 0x7F
    if length == 126:
        length = int.from_bytes(_recv_exact(sock, 2), "big")
    elif length == 127:
        length = int.from_bytes(_recv_exact(sock, 8), "big")
    mask = _recv_exact(sock, 4) if masked else None
    payload = bytearray(_recv_exact(sock, length))
    if mask:
        for i in range(length):
            payload[i] ^= mask[i & 3]
    return opcode, bytes(payload)


def _send_ws_text(sock, text):
    data = text.encode("utf-8")
    mask = os.urandom(4)
    header = bytearray([0x81])
    n = len(data)
    if n < 126:
        header.append(0x80 | n)
    elif n < 65536:
        header.append(0x80 | 126)
        header.extend(n.to_bytes(2, "big"))
    else:
        header.append(0x80 | 127)
        header.extend(n.to_bytes(8, "big"))
    header.extend(mask)
    masked = bytearray(n)
    for i in range(n):
        masked[i] = data[i] ^ mask[i & 3]
    sock.sendall(header + masked)


def ws_request(url, payload, timeout=2.5):
    host, port, path = _parse_ws_url(url)
    key = base64.b64encode(os.urandom(16)).decode("ascii")
    with socket.create_connection((host, port), timeout=timeout) as sock:
        sock.settimeout(timeout)
        req = (
            f"GET {{path}} HTTP/1.1\r\n"
            f"Host: {{host}}:{{port}}\r\n"
            f"Upgrade: websocket\r\n"
            f"Connection: Upgrade\r\n"
            f"Sec-WebSocket-Key: {{key}}\r\n"
            f"Sec-WebSocket-Version: 13\r\n\r\n"
        )
        sock.sendall(req.encode("ascii"))
        resp = b""
        while b"\r\n\r\n" not in resp:
            chunk = sock.recv(4096)
            if not chunk:
                raise ConnectionError("Handshake closed")
            resp += chunk
        header_bytes, leftover = resp.split(b"\r\n\r\n", 1)
        status_line = header_bytes.split(b"\r\n", 1)[0].decode("latin1", errors="replace")
        if "101" not in status_line:
            raise ConnectionError(f"Unexpected HTTP status: {{status_line}}")

        buffered = bytearray(leftover)

        def recv_from_buffer(n):
            while len(buffered) < n:
                chunk = sock.recv(max(4096, n - len(buffered)))
                if not chunk:
                    raise ConnectionError("WebSocket closed")
                buffered.extend(chunk)
            out = bytes(buffered[:n])
            del buffered[:n]
            return out

        def read_frame():
            h = recv_from_buffer(2)
            op = h[0] & 0x0F
            msk = (h[1] & 0x80) != 0
            ln = h[1] & 0x7F
            if ln == 126:
                ln = int.from_bytes(recv_from_buffer(2), "big")
            elif ln == 127:
                ln = int.from_bytes(recv_from_buffer(8), "big")
            m = recv_from_buffer(4) if msk else None
            pl = bytearray(recv_from_buffer(ln))
            if m:
                for i in range(ln):
                    pl[i] ^= m[i & 3]
            return op, bytes(pl)

        req_id = payload.get("id") or "obs-companion-1"
        payload = dict(payload)
        payload["id"] = req_id
        _send_ws_text(sock, json.dumps(payload))

        deadline = time.monotonic() + timeout
        while time.monotonic() < deadline:
            opcode, body = read_frame()
            if opcode == 0x8:
                raise ConnectionError("WebSocket closed by server")
            if opcode == 0x9:
                continue
            if opcode == 0x1:
                msg = json.loads(body.decode("utf-8", errors="replace"))
                if msg.get("id") == req_id:
                    return msg
        raise TimeoutError("Timed out waiting for Streamerz reply")


def is_streamerz_running(url=None):
    try:
        reply = ws_request(url or STATE["ws_url"], {{"request": "GetInfo"}}, timeout=1.2)
        return reply.get("status") == "ok"
    except Exception:
        return False


def build_launch_args():
    custom = (STATE.get("launch_cmd") or "").strip()
    if custom and (not DEFAULT_LAUNCH_CMD or custom != DEFAULT_LAUNCH_CMD[0]):
        args = [custom]
        if STATE.get("launch_hidden", True) and "--hidden" not in args:
            args.append("--hidden")
        return args
    args = [arg for arg in DEFAULT_LAUNCH_CMD if arg != "--hidden"]
    if not args:
        args = ["Streamerz"]
    if STATE.get("launch_hidden", True):
        args.append("--hidden")
    return args


def launch_streamerz():
    args = build_launch_args()
    _log(f"Starting Streamerz: {{args}}")
    kwargs = {{
        "stdin": subprocess.DEVNULL,
        "stdout": subprocess.DEVNULL,
        "stderr": subprocess.DEVNULL,
        "close_fds": True,
    }}
    if sys.platform.startswith("win"):
        flags = getattr(subprocess, "DETACHED_PROCESS", 0x00000008) | getattr(
            subprocess, "CREATE_NEW_PROCESS_GROUP", 0x00000200
        ) | getattr(subprocess, "CREATE_NO_WINDOW", 0x08000000)
        kwargs["creationflags"] = flags
    else:
        kwargs["start_new_session"] = True
    subprocess.Popen(args, **kwargs)


def read_obs_main_service():
    if obs is None:
        return "", ""
    service_name = ""
    server_url = ""
    try:
        svc = obs.obs_frontend_get_streaming_service()
        if svc is not None:
            settings = obs.obs_service_get_settings(svc)
            if settings is not None:
                try:
                    service_name = obs.obs_data_get_string(settings, "service") or ""
                    server_url = obs.obs_data_get_string(settings, "server") or ""
                finally:
                    obs.obs_data_release(settings)
    except Exception as exc:
        _log(f"Could not inspect OBS streaming service: {{exc}}")
    return service_name, server_url


def ensure_running_and_preflight(trigger="startup", main_service="", main_server=""):
    url = STATE["ws_url"]
    if not is_streamerz_running(url):
        if STATE.get("auto_launch", True):
            try:
                launch_streamerz()
            except Exception as exc:
                _log(f"Failed to start Streamerz: {{exc}}")
                return None
            for _ in range(24):
                time.sleep(0.5)
                if is_streamerz_running(url):
                    break
            else:
                _log("Streamerz did not answer on its WebSocket after launch.")
                return None
        else:
            _log("Streamerz is not running and auto-launch is disabled.")
            return None

    try:
        reply = ws_request(
            url,
            {{
                "request": "StreamerzPreflight",
                "op": "check",
                "trigger": trigger,
                "notify": True,
                "mainService": main_service,
                "mainServer": main_server,
            }},
            timeout=4.0,
        )
        summary = reply.get("summary") or reply.get("status") or "ok"
        _log(f"Preflight ({{trigger}}): {{summary}}")
        if (
            trigger == "obs_starting"
            and STATE.get("strict_mode", False)
            and reply.get("status") == "blocked"
        ):
            _log("Strict mode is enabled and preflight reported blockers; stopping stream start.")
            try:
                ws_request(url, {{"request": "StreamerzShowWindow"}}, timeout=1.5)
            except Exception:
                pass
            if obs is not None:
                try:
                    obs.obs_frontend_streaming_stop()
                except Exception as exc:
                    _log(f"Could not stop stream: {{exc}}")
        return reply
    except Exception as exc:
        _log(f"Preflight check failed: {{exc}}")
        return None


def _on_frontend_event(event):
    if obs is None:
        return
    finished = getattr(obs, "OBS_FRONTEND_EVENT_FINISHED_LOADING", None)
    starting = getattr(obs, "OBS_FRONTEND_EVENT_STREAMING_STARTING", None)
    if event == finished:
        threading.Thread(
            target=ensure_running_and_preflight,
            kwargs={{"trigger": "obs_loaded"}},
            daemon=True,
        ).start()
    elif event == starting and STATE.get("check_on_start", True):
        svc, srv = read_obs_main_service()
        threading.Thread(
            target=ensure_running_and_preflight,
            kwargs={{
                "trigger": "obs_starting",
                "main_service": svc,
                "main_server": srv,
            }},
            daemon=True,
        ).start()


def script_description():
    return (
        "Guarantees Streamerz is running when OBS Studio opens and checks all "
        "streaming outputs (Main, Aitum Multistream, Vertical, Multi-RTMP) before going live."
    )


def script_defaults(settings):
    if obs is None:
        return
    obs.obs_data_set_default_bool(settings, "auto_launch", True)
    obs.obs_data_set_default_bool(settings, "launch_hidden", True)
    obs.obs_data_set_default_bool(settings, "check_on_start", True)
    obs.obs_data_set_default_bool(settings, "strict_mode", False)
    obs.obs_data_set_default_string(settings, "ws_url", DEFAULT_WS_URL)
    obs.obs_data_set_default_string(
        settings,
        "launch_cmd",
        DEFAULT_LAUNCH_CMD[0] if DEFAULT_LAUNCH_CMD else "Streamerz",
    )


def script_update(settings):
    if obs is None:
        return
    STATE["auto_launch"] = obs.obs_data_get_bool(settings, "auto_launch")
    STATE["launch_hidden"] = obs.obs_data_get_bool(settings, "launch_hidden")
    STATE["check_on_start"] = obs.obs_data_get_bool(settings, "check_on_start")
    STATE["strict_mode"] = obs.obs_data_get_bool(settings, "strict_mode")
    STATE["ws_url"] = obs.obs_data_get_string(settings, "ws_url") or DEFAULT_WS_URL
    STATE["launch_cmd"] = obs.obs_data_get_string(settings, "launch_cmd") or (
        DEFAULT_LAUNCH_CMD[0] if DEFAULT_LAUNCH_CMD else "Streamerz"
    )


def _btn_check_now(props, prop):
    svc, srv = read_obs_main_service()
    threading.Thread(
        target=ensure_running_and_preflight,
        kwargs={{"trigger": "manual", "main_service": svc, "main_server": srv}},
        daemon=True,
    ).start()
    return True


def _btn_open_streamerz(props, prop):
    def _open():
        if not is_streamerz_running(STATE["ws_url"]):
            try:
                launch_streamerz()
            except Exception as exc:
                _log(f"Could not start Streamerz: {{exc}}")
                return
            for _ in range(20):
                time.sleep(0.4)
                if is_streamerz_running(STATE["ws_url"]):
                    break
        try:
            ws_request(STATE["ws_url"], {{"request": "StreamerzShowWindow"}}, timeout=2.0)
        except Exception as exc:
            _log(f"Could not show Streamerz window: {{exc}}")

    threading.Thread(target=_open, daemon=True).start()
    return True


def script_properties():
    if obs is None:
        return None
    props = obs.obs_properties_create()
    obs.obs_properties_add_bool(
        props,
        "auto_launch",
        "Start Streamerz automatically when OBS opens",
    )
    obs.obs_properties_add_bool(
        props,
        "launch_hidden",
        "Start Streamerz hidden in the system tray",
    )
    obs.obs_properties_add_bool(
        props,
        "check_on_start",
        "Check accounts and outputs when starting a stream",
    )
    obs.obs_properties_add_bool(
        props,
        "strict_mode",
        "Stop stream start if a required account is blocked",
    )
    obs.obs_properties_add_text(
        props,
        "ws_url",
        "Streamerz WebSocket URL",
        obs.OBS_TEXT_DEFAULT,
    )
    obs.obs_properties_add_text(
        props,
        "launch_cmd",
        "Streamerz executable",
        obs.OBS_TEXT_DEFAULT,
    )
    obs.obs_properties_add_button(
        props,
        "check_now",
        "Check Streamerz Readiness Now",
        _btn_check_now,
    )
    obs.obs_properties_add_button(
        props,
        "open_streamerz",
        "Open Streamerz",
        _btn_open_streamerz,
    )
    return props


def script_load(settings):
    script_update(settings)
    if obs is not None:
        obs.obs_frontend_add_event_callback(_on_frontend_event)
    threading.Thread(
        target=ensure_running_and_preflight,
        kwargs={{"trigger": "script_load"}},
        daemon=True,
    ).start()
"#
    )
}

/// Write `streamerz_obs_companion.py` to `target` (or the default config folder path when `None`).
pub fn save_companion_script(config: &crate::Config, target: Option<&Path>) -> Result<PathBuf, String> {
    let path = target.map(Path::to_path_buf).unwrap_or_else(companion_script_path);
    if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let content = companion_script(config);
    std::fs::write(&path, content).map_err(|e| e.to_string())?;
    Ok(path)
}

/// Build the `"obs_companion"` sections dialog model.
pub fn companion_sections(engine: &Rc<Engine>) -> Vec<Value> {
    let config = engine.config();
    let path = companion_script_path();
    // Ensure the companion script file exists and is up to date when the dialog opens.
    if !path.is_file() {
        let _ = save_companion_script(&config, Some(&path));
    }
    let script_code = companion_script(&config);
    let path_str = path.display().to_string();

    let inventories = super::stream::cached_obs_inventories();
    let report = evaluate_inventories(engine, &inventories, None, None);

    let health = match report.status.as_str() {
        "blocked" => "down",
        "warning" => "attention",
        _ => "up",
    };

    let mut readiness_rows = vec![
        json!({"about": tr("preflight.dialog_intro")}),
        json!({
            "key": "preflight.summary",
            "status": {"health": health, "text": report.summary},
            "control": {"type": "buttons", "buttons": [
                {"id": "check_now", "label": tr("preflight.btn_check_now"), "icon": "refresh-cw"}
            ]}
        }),
    ];

    if !config.websockets.auto_start {
        readiness_rows.push(json!({
            "key": "preflight.ws_disabled",
            "status": {"health": "attention", "text": tr("preflight.ws_disabled_hint")},
            "control": {"type": "buttons", "buttons": [
                {"id": "go_servers", "label": tr("nav.page.servers"), "icon": "radio", "go": "Servers"}
            ]}
        }));
    }

    for (idx, issue) in report.issues.iter().enumerate() {
        let issue_health = if issue.severity == "blocker" {
            "down"
        } else {
            "attention"
        };
        let mut btn = json!({
            "id": format!("issue_fix:{idx}"),
            "label": issue.fix.label,
            "icon": if issue.fix.page == "Accounts" { "user-round" } else if issue.fix.page == "Stream Apps" { "monitor" } else { "sliders-horizontal" },
        });
        if !issue.fix.page.is_empty() {
            btn["go"] = issue.fix.page.clone().into();
        }
        let mut row = json!({
            "key": format!("issue.{idx}"),
            "status": {"health": issue_health, "text": issue.message},
            "control": {"type": "buttons", "buttons": [btn]}
        });
        if let Some(plat) = issue.platform.as_deref() {
            row["brand"] = plat.into();
        }
        readiness_rows.push(row);
    }

    let mut sections = vec![json!({
        "title": tr("preflight.dialog_title"),
        "icon": "shield",
        "rows": readiness_rows
    })];

    if !report.outputs.is_empty() {
        let mut output_rows = vec![json!({"about": tr("preflight.outputs_hint")})];
        for out in &report.outputs {
            let out_health = match out.status.as_str() {
                "blocked" => "down",
                "warning" => "attention",
                "ignored" => "off",
                _ => "up",
            };
            let active_label = match out.active {
                Some(true) => tr("stream.state_live"),
                Some(false) => tr("stream.state_not_live"),
                None => tr("preflight.state_standby"),
            };
            let raw_auto = inventories
                .iter()
                .find(|(_, inv)| inv.connection_id == out.connection_id)
                .and_then(|(_, inv)| inv.outputs.iter().find(|o| o.source == out.source && o.key == out.key))
                .and_then(|o| {
                    if o.platform_source.as_deref() == Some("user") {
                        outputs::platform(&o.name)
                    } else {
                        o.platform.clone()
                    }
                });
            let options = option_labels(raw_auto.as_deref());
            let mut row = json!({
                "key": format!("output.{}", out.id),
                "label": out.name,
                "status": {"health": out_health, "text": format!("{active_label} - {}", out.detail)},
                "control": {
                    "type": "select",
                    "value": out.choice_index,
                    "options": options
                }
            });
            if let Some(plat) = out.platform.as_deref() {
                row["brand"] = plat.into();
            }
            output_rows.push(row);
        }
        sections.push(json!({
            "title": tr("preflight.outputs_title"),
            "icon": "radio",
            "rows": output_rows
        }));
    }

    let script_rows = vec![
        json!({"about": tr("preflight.script_about")}),
        json!({
            "key": "script.path",
            "label": tr("preflight.script_file_label"),
            "value": path_str,
            "control": {"type": "buttons", "buttons": [
                {"id": "copy_path", "label": tr("btn.copy_path"), "icon": "copy", "copy": path_str},
                {"id": "open_folder", "label": tr("btn.open_folder"), "icon": "folder-open"}
            ]}
        }),
        json!({
            "key": "script.actions",
            "label": tr("preflight.script_actions_label"),
            "about": tr("preflight.script_actions_about"),
            "control": {"type": "buttons", "buttons": [
                {"id": "save_script", "label": tr("preflight.btn_save_script"), "icon": "download"},
                {"id": "copy_script", "label": tr("preflight.btn_copy_script"), "icon": "copy", "copy": script_code}
            ]}
        }),
        json!({"about": tr("preflight.script_steps")}),
    ];

    sections.push(json!({
        "title": tr("preflight.script_title"),
        "icon": "code",
        "rows": script_rows
    }));

    sections
}

pub fn set_setting(engine: &Rc<Engine>, key: &str, value: &Value) -> Result<Value, String> {
    if let Some(output_id) = key.strip_prefix("output.") {
        set_output_by_id(engine, output_id, value)?;
        return Ok(json!({"accepted": true}));
    }
    Err(format!("{DOMAIN} has no setting {key}"))
}

pub fn act(engine: &Rc<Engine>, action: &str) -> Result<Value, String> {
    let accepted = Ok(json!({"accepted": true}));
    match action {
        "check_now" => {
            let e = engine.clone();
            tokio::task::spawn_local(async move {
                let _ = check(&e, &json!({"notify": false})).await;
                crate::host::changed();
            });
            super::stream::act(engine, "refresh").ok();
            accepted
        }
        "save_script" => {
            let path = save_companion_script(&engine.config(), None)?;
            crate::say(trf(
                "preflight.script_saved_log",
                &[("path", &path.display().to_string())],
            ));
            crate::host::changed();
            accepted
        }
        "copy_path" => {
            crate::say(tr("settings.path_copied").into());
            accepted
        }
        "copy_script" => {
            crate::say(tr("preflight.script_copied_log").into());
            accepted
        }
        "open_folder" => {
            let path = companion_script_path();
            let _ = save_companion_script(&engine.config(), Some(&path));
            if let Some(folder) = path.parent() {
                crate::oauth::open_browser(&folder.display().to_string());
            }
            accepted
        }
        "go_servers" => accepted,
        _ if action.starts_with("issue_fix:") => accepted,
        _ => Err(format!("{DOMAIN} has no action {action}")),
    }
}

/// Handle `StreamerzPreflight` WebSocket / MCP requests.
pub async fn request(engine: &Rc<Engine>, req: &Value) -> Result<Value, String> {
    let op = req["op"].as_str().unwrap_or("check");
    match op {
        "check" => {
            let report = check(engine, req).await?;
            serde_json::to_value(report).map_err(|e| e.to_string())
        }
        "set_output" => {
            let output_id = req["outputId"].as_str().ok_or("outputId is required")?;
            let val = if !req["platform"].is_null() {
                &req["platform"]
            } else {
                &req["value"]
            };
            set_output_by_id(engine, output_id, val)?;
            let report = check(engine, &json!({})).await?;
            serde_json::to_value(report).map_err(|e| e.to_string())
        }
        "companion_script" => {
            let save = req["save"].as_bool().unwrap_or(false);
            let target = req["path"].as_str().filter(|s| !s.trim().is_empty()).map(PathBuf::from);
            let path = if save || target.is_some() {
                save_companion_script(&engine.config(), target.as_deref())?
            } else {
                companion_script_path()
            };
            Ok(json!({
                "path": path.display().to_string(),
                "saved": save || target.is_some(),
                "launchCommand": launch_command_args(),
                "script": companion_script(&engine.config()),
            }))
        }
        _ => Err(format!("Unknown preflight op '{op}'")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_output(conn: &str, source: &str, key: &str, name: &str, platform: Option<&str>) -> Output {
        Output {
            id: json!([conn, source, key]).to_string(),
            source: source.into(),
            name: name.into(),
            platform: platform.map(str::to_owned),
            platform_source: platform.map(|_| "name_hint".into()),
            active: Some(false),
            reconnecting: Some(false),
            vertical: Some(false),
            controllable: true,
            limitation: None,
            required: true,
            key: key.into(),
        }
    }

    #[test]
    fn multi_output_preflight_detects_missing_logins_private_youtube_and_tiktok_bridge() {
        let _lock = crate::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        crate::engine::tests::run(async {
            clear_state();
            let token = json!({
                "accessToken": "valid",
                "refreshToken": "refresh",
                "expiresAt": u64::MAX / 2,
                "scope": crate::twitch::BROADCASTER_SCOPES
            });
            let config: crate::Config = serde_json::from_value(json!({
                "obs": {"connections": [{"id": "main", "name": "Studio"}]},
                "twitch": {"broadcaster": {"userId": "1", "login": "me", "displayName": "Me", "token": token}},
                "youtube": {"broadcaster": {"channelId": "UC1", "name": "Me", "token": token}},
                "tiktok": {"channel": "my_tiktok"}
            }))
            .unwrap();
            let engine = Engine::new(std::sync::Arc::new(tokio::sync::watch::channel(config).0), |_| {});
            let _fakes = crate::scenario::fakes::install(&engine, &json!({"twitch": {}, "youtube": {}}), None);
            super::super::stream::set_youtube_preflight_for_test(false, "private");

            let inv = Inventory {
                connection_id: "main".into(),
                canvases: vec![],
                capabilities: vec![],
                outputs: vec![
                    make_output("main", "obs", "main", "Main stream", Some("twitch")),
                    make_output("main", "aitum-multistream", "Kick", "Kick", Some("kick")),
                    make_output(
                        "main",
                        "aitum-multistream",
                        "YouTube (horizontal)",
                        "YouTube (horizontal)",
                        Some("youtube"),
                    ),
                    make_output(
                        "main",
                        "aitum-multistream",
                        "YouTube (vertical)",
                        "YouTube (vertical)",
                        Some("youtube"),
                    ),
                    make_output("main", "aitum-multistream", "TikTok", "TikTok", Some("tiktok")),
                ],
            };

            let report = evaluate_inventories(&engine, &[("Studio".into(), inv)], None, None);
            assert!(!report.ready, "Kick login is missing so preflight is blocked");
            assert_eq!(report.status, "blocked");
            assert_eq!(report.outputs.len(), 5);
            assert_eq!(report.outputs[0].status, "ready", "Twitch is ready");
            assert_eq!(report.outputs[1].status, "blocked", "Kick lacks broadcaster login");
            assert_eq!(report.outputs[2].status, "warning", "YouTube broadcast is Private");
            assert_eq!(
                report.outputs[4].status, "warning",
                "TikTok browser extension is not ready"
            );
            assert!(report.issues.iter().any(|i| i.code == "kick_login_missing"));
            assert!(report.issues.iter().any(|i| i.code == "youtube_private"));
            assert!(report.issues.iter().any(|i| i.code == "youtube_multi_output"));
            assert!(report.issues.iter().any(|i| i.code == "tiktok_bridge_not_ready"));

            // Ignore the Kick output: preflight becomes ready (with warnings).
            set_output_choice(&engine, "main", "aitum-multistream", "Kick", 5).unwrap();
            let inv2 = Inventory {
                connection_id: "main".into(),
                canvases: vec![],
                capabilities: vec![],
                outputs: vec![
                    make_output("main", "obs", "main", "Main stream", Some("twitch")),
                    make_output("main", "aitum-multistream", "Kick", "Kick", Some("kick")),
                ],
            };
            let report2 = evaluate_inventories(&engine, &[("Studio".into(), inv2)], None, None);
            assert!(report2.ready);
            assert_eq!(report2.status, "ready");
            assert_eq!(report2.outputs[1].status, "ignored");
            crate::scenario::fakes::uninstall();
        });
    }

    #[test]
    fn companion_script_is_valid_python_and_contains_no_secrets() {
        let mut config = crate::Config::default();
        config.websockets.port = 8123;
        config.websockets.endpoint = "/ws".into();
        config.websockets.auth_password = "super_secret_ws_password".into();
        let script = companion_script(&config);
        assert!(script.contains("ws://127.0.0.1:8123/ws"));
        assert!(!script.contains("super_secret_ws_password"));
        assert!(script.contains("OBS_FRONTEND_EVENT_STREAMING_STARTING"));
        assert!(script.contains("StreamerzPreflight"));
    }

    #[test]
    fn a_platform_that_stopped_reading_blocks_going_live() {
        let _lock = crate::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        crate::engine::tests::run(async {
            clear_state();
            let config: crate::Config = serde_json::from_value(json!({
                "obs": {"connections": [{"id": "main", "name": "Studio"}]},
                "kick": {"clientId": "id", "clientSecret": "secret", "broadcaster": {"userId": 1, "username": "me",
                    "token": {"accessToken": "valid", "refreshToken": "refresh", "expiresAt": u64::MAX / 2}}}
            }))
            .unwrap();
            let engine = Engine::new(std::sync::Arc::new(tokio::sync::watch::channel(config).0), |_| {});
            let _fakes = crate::scenario::fakes::install(&engine, &json!({"kick": {}}), None);
            let kick = |report: &PreflightReport| {
                report
                    .issues
                    .iter()
                    .filter(|i| i.platform.as_deref() == Some("kick"))
                    .map(|i| i.code.clone())
                    .collect::<Vec<_>>()
            };
            assert!(kick(&evaluate_inventories(&engine, &[], None, None)).is_empty());
            // Disconnect keeps the handle: its API answers, but nothing is read.
            engine.kick().unwrap().set_connected(false);
            let report = evaluate_inventories(&engine, &[], None, None);
            assert_eq!(kick(&report), ["kick_disconnected"]);
            let issue = report.issues.iter().find(|i| i.code == "kick_disconnected").unwrap();
            assert_eq!(
                (issue.severity.as_str(), issue.fix.action.as_str()),
                ("blocker", "kick.connect")
            );
            // The account itself still works, so an automation's API call isn't blamed for it.
            assert!(evaluate_platform(&engine, "kick").is_none());
            // A required output on that platform is held back too.
            let inv = Inventory {
                connection_id: "main".into(),
                canvases: vec![],
                capabilities: vec![],
                outputs: vec![make_output("main", "aitum-multistream", "Kick", "Kick", Some("kick"))],
            };
            let report = evaluate_inventories(&engine, &[("Studio".into(), inv)], None, None);
            assert_eq!(report.outputs[0].status, "blocked");
            assert!(!report.ready);
            engine.kick().unwrap().set_connected(true);
            assert!(kick(&evaluate_inventories(&engine, &[], None, None)).is_empty());
            crate::scenario::fakes::uninstall();
        });
    }
}
