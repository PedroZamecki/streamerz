//! Pages made of sections of labelled rows: Settings, Notifications and Donations.
//!
//! A row has a label and an explanation, then optionally a status line and one control: a switch, a
//! choice (stored as its index), text, or buttons. Frontends draw them; `set` and `act` change things,
//! so MCP and any other frontend reach exactly the same behaviour.

use crate::{
    engine::Engine,
    host::Theme,
    i18n::{self, tr, trf},
};
use serde_json::{json, Value};
use std::rc::Rc;

pub const DOMAINS: &[&str] = &[
    "settings",
    "notifications",
    "donations",
    "integrations",
    "spotify",
    "chat_options",
];

pub fn supports(domain: &str) -> bool {
    DOMAINS.contains(&domain)
}

/// The date and time patterns offered ("" is the system's).
const DATES: [&str; 8] = [
    "",
    "dd/MM/yyyy",
    "MM/dd/yyyy",
    "yyyy-MM-dd",
    "dd.MM.yyyy",
    "dd/MM/yy",
    "MM/dd/yy",
    "d MMM yyyy",
];
const TIMES: [&str; 3] = ["", "HH:mm", "h:mm tt"];
const UPDATES: [crate::update::Updates; 3] = [
    crate::update::Updates::Ask,
    crate::update::Updates::Background,
    crate::update::Updates::Ignore,
];

fn section(title: &str, icon: &str, rows: Vec<Value>) -> Value {
    json!({"title": title, "icon": icon, "rows": rows})
}
fn row(key: &str, label: &str, about: &str, control: Value) -> Value {
    json!({"key": key, "label": label, "about": about, "control": control})
}
fn switch(value: bool) -> Value {
    json!({"type": "switch", "value": value})
}
fn choice(value: usize, options: Vec<String>) -> Value {
    json!({"type": "select", "value": value, "options": options})
}
fn button(id: &str, label: &str, icon: &str) -> Value {
    json!({"type": "buttons", "buttons": [{"id": id, "label": label, "icon": icon}]})
}

pub fn read(engine: &Rc<Engine>, domain: &str) -> Result<Value, String> {
    let sections = match domain {
        "settings" => settings(engine),
        "notifications" => notifications(engine),
        "chat_options" => super::chat::options(engine),
        "donations" => donations(engine),
        "integrations" => integrations(engine),
        "spotify" => spotify(engine),
        _ => return Err(format!("no sections page for {domain}")),
    };
    let config = engine.config();
    Ok(json!({"domain": domain, "sections": sections,
        "appearance": {"dark": crate::host::dark_of(&config), "animations": config.animations}}))
}

fn notifications(engine: &Rc<Engine>) -> Vec<Value> {
    let current = engine.config().notifications;
    let item = |key: &str, label: &'static str, on: bool| row(key, tr(label), "", switch(on));
    vec![section(
        tr("notifications.title"),
        "bell",
        vec![
            json!({"about": tr("notifications.about")}),
            item("login", "notifications.login", current.login),
            item("updates", "notifications.updates", current.updates),
            item("streams", "notifications.streams", current.streams),
            item("automation", "notifications.automation", current.automation),
        ],
    )]
}

fn settings(engine: &Rc<Engine>) -> Vec<Value> {
    let config = engine.config();
    let chosen = config
        .theme
        .unwrap_or(if config.dark { Theme::Dark } else { Theme::Light });
    let system = if crate::host::system_is_dark() {
        tr("settings.theme.dark")
    } else {
        tr("settings.theme.light")
    };
    let languages: Vec<(&str, &str)> = i18n::all().collect();
    let detected = languages
        .iter()
        .find(|language| language.0 == i18n::detect())
        .map_or("", |language| language.1);
    let mut language_names = vec![trf("settings.language.auto", &[("language", detected)])];
    language_names.extend(languages.iter().map(|language| language.1.to_owned()));
    let now = crate::fmt::now();
    let patterns = crate::fmt::locale_patterns();
    let example =
        |pattern: &str, fallback: &str| crate::fmt::date(&now, if pattern.is_empty() { fallback } else { pattern });
    let dates: Vec<String> = DATES
        .iter()
        .map(|pattern| match *pattern {
            "" => trf("settings.system_format", &[("example", &example("", &patterns.0))]),
            pattern => example(&pattern.replace('/', "'/'"), ""),
        })
        .collect();
    let times: Vec<String> = TIMES
        .iter()
        .map(|pattern| match *pattern {
            "" => trf("settings.system_format", &[("example", &example("", &patterns.1))]),
            pattern => example(pattern, ""),
        })
        .collect();
    let path = crate::config_path();
    let update_about = match crate::update::found() {
        Some(tag) => trf("settings.new_versions.found", &[("version", &tag)]),
        None => trf("settings.new_versions.about", &[("version", &crate::version())]),
    };
    let mut memory = switch(config.memory_saver);
    if config.memory_saver != crate::memory_saver() {
        memory["button"] = json!({"id": "restart", "label": tr("update.restart"), "icon": "rotate-cw", "tooltip": tr("settings.restart_required")});
    }
    let mut about = row(
        "about",
        &trf("settings.about.version", &[("version", crate::brand::VERSION)]),
        tr("settings.about.tagline"),
        button("open_github", tr("btn.open_github"), "external-link"),
    );
    about["logo"] = true.into();
    vec![
        section(tr("settings.about"), "info", vec![about]),
        section(
            tr("settings.appearance"),
            "sun",
            vec![
                row(
                    "theme",
                    tr("settings.theme"),
                    tr("settings.theme.about"),
                    choice(
                        match chosen {
                            Theme::System => 0,
                            Theme::Dark => 1,
                            Theme::Light => 2,
                        },
                        vec![
                            trf("settings.theme.system", &[("theme", system)]),
                            tr("settings.theme.dark").into(),
                            tr("settings.theme.light").into(),
                        ],
                    ),
                ),
                row(
                    "language",
                    tr("settings.language"),
                    tr("settings.language.about"),
                    choice(
                        languages
                            .iter()
                            .position(|language| language.0 == config.locale)
                            .map_or(0, |index| index + 1),
                        language_names,
                    ),
                ),
                row(
                    "animations",
                    tr("settings.animations"),
                    tr("settings.animations.about"),
                    switch(config.animations),
                ),
            ],
        ),
        section(
            tr("settings.memory"),
            "power",
            vec![row(
                "memory_saver",
                tr("settings.memory.saver"),
                &[
                    tr("settings.memory.saver.about"),
                    tr("settings.memory.effects"),
                    tr("settings.memory.first_words"),
                    tr("settings.memory.files"),
                ]
                .join("\n"),
                memory,
            )],
        ),
        section(
            tr("settings.chat"),
            "message-square",
            vec![row(
                "chat_settings",
                tr("settings.chat.moved"),
                tr("settings.chat.moved.about"),
                json!({"type": "buttons", "buttons": [{"id": "chat_settings", "label": tr("btn.chat_settings"), "icon": "sliders-horizontal", "dialog": "chat_options"}]}),
            )],
        ),
        section(
            tr("notifications.title"),
            "bell",
            vec![row(
                "notifications",
                tr("notifications.title"),
                tr("notifications.about"),
                json!({"type": "buttons", "buttons": [{"id": "notifications", "label": tr("notifications.manage"), "dialog": "notifications"}]}),
            )],
        ),
        section(
            tr("settings.startup"),
            "power",
            vec![
                row(
                    "start_hidden",
                    tr("settings.start_hidden"),
                    tr("settings.start_hidden.about"),
                    switch(config.start_hidden),
                ),
                row(
                    "autostart",
                    tr("settings.autostart"),
                    if cfg!(windows) {
                        tr("settings.autostart.about_windows")
                    } else {
                        tr("settings.autostart.about_linux")
                    },
                    switch(config.autostart),
                ),
            ],
        ),
        section(
            tr("settings.datetime"),
            "calendar",
            vec![
                row(
                    "dates",
                    tr("settings.dates"),
                    tr("settings.dates.about"),
                    choice(
                        DATES
                            .iter()
                            .position(|pattern| *pattern == config.date_format)
                            .unwrap_or(0),
                        dates,
                    ),
                ),
                row(
                    "times",
                    tr("settings.times"),
                    tr("settings.times.about"),
                    choice(
                        TIMES
                            .iter()
                            .position(|pattern| *pattern == config.time_format)
                            .unwrap_or(0),
                        times,
                    ),
                ),
            ],
        ),
        section(
            tr("settings.updates"),
            "download",
            vec![row(
                "updates",
                tr("settings.new_versions"),
                &update_about,
                json!({"type": "select", "value": UPDATES.iter().position(|mode| *mode == config.updates).unwrap_or(0),
                    "options": [tr("settings.updates.ask"), tr("settings.updates.background"), tr("settings.updates.ignore")],
                    "button": {"id": "check_now", "label": tr("settings.check_now"), "icon": "refresh-cw"}}),
            )],
        ),
        section(
            tr("settings.log"),
            "scroll-text",
            vec![row(
                "debug_log",
                tr("settings.debug_log"),
                tr("settings.debug_log.about"),
                switch(config.debug_log),
            )],
        ),
        section(
            tr("settings.files"),
            "folder",
            vec![row(
                "files",
                tr("settings.files"),
                &path.display().to_string(),
                json!({"type": "buttons", "buttons": [
                    {"id": "open_file", "label": tr("btn.open_file"), "icon": "file-text"},
                    {"id": "copy_path", "label": tr("btn.copy_path"), "icon": "copy", "copy": path.display().to_string()},
                    {"id": "open_folder", "label": tr("btn.open_folder"), "icon": "folder-open"}]}),
            )],
        ),
        section(
            tr("settings.tools"),
            "plug",
            vec![row(
                "instance_name",
                tr("settings.instance_name"),
                tr("settings.instance_name.about"),
                json!({"type": "text", "value": config.instance_name}),
            )],
        ),
    ]
}

fn spotify_on(config: &crate::Config) -> bool {
    config.spotify.enabled && !config.spotify.token.access_token.is_empty()
}

fn integrations(engine: &Rc<Engine>) -> Vec<Value> {
    let on = spotify_on(&engine.config());
    let (health, text) = if on {
        ("up", tr("conns.connected"))
    } else {
        ("off", tr("conns.off"))
    };
    let mut block = section(
        tr("accounts.spotify.name"),
        "",
        vec![
            status(
                health,
                text,
                json!({"id": "configure", "label": tr("btn.configure"), "icon": "sliders-horizontal", "dialog": "spotify"}),
            ),
            json!({"about": tr("accounts.spotify.about")}),
        ],
    );
    block["brand"] = "spotify".into();
    block["inline"] = true.into();
    vec![
        section("", "", vec![json!({"about": tr("accounts.integrations.intro")})]),
        block,
    ]
}

fn spotify(engine: &Rc<Engine>) -> Vec<Value> {
    let config = engine.config();
    let on = spotify_on(&config);
    let (health, text) = if on {
        ("up", tr("conns.connected"))
    } else {
        ("off", tr("conns.off"))
    };
    let enabled = |button: Value, yes: bool| {
        let mut button = button;
        button["enabled"] = yes.into();
        button
    };
    vec![section(
        tr("accounts.spotify.setup"),
        "",
        vec![
            json!({"about": tr("accounts.spotify.help")}),
            row(
                "client_id",
                tr("accounts.spotify.client_id"),
                "",
                json!({"type": "text", "value": config.spotify.client_id}),
            ),
            json!({"control": {"type": "buttons", "buttons": [
                {"id": "open_dashboard", "label": tr("accounts.spotify.dashboard"), "icon": "external-link"},
                {"id": "copy_redirect", "label": tr("btn.copy_redirect_url"), "icon": "copy", "copy": crate::spotify::REDIRECT}]}}),
            json!({"status": {"health": health, "text": text}, "control": {"type": "buttons", "buttons": [
                enabled(json!({"id": "connect", "label": tr("btn.connect"), "icon": "plug"}), !on),
                enabled(json!({"id": "disconnect", "label": tr("btn.disconnect"), "icon": "unplug"}), on)]}}),
            row(
                "local_monitor",
                tr("spotify.local_monitor"),
                "",
                switch(config.spotify.local_monitor),
            ),
            json!({"about": tr("spotify.limited_hint")}),
            json!({"control": {"type": "buttons", "buttons": [{"id": "check_local", "label": tr("spotify.local_check"), "icon": "search"}]}}),
        ],
    )]
}

fn status(health: &str, text: &str, button: Value) -> Value {
    json!({"status": {"health": health, "text": text}, "control": {"type": "buttons", "buttons": [button]}})
}

fn donations(engine: &Rc<Engine>) -> Vec<Value> {
    let config = engine.config();
    let donations = &config.donations;
    let runtime = {
        let live = engine.donations.borrow();
        (live.streamelements.get(), live.streamlabs.get())
    };
    let token_status = |token: &str, connected: bool| {
        if token.trim().is_empty() {
            ("off", tr("lists.donations.off_no_token"))
        } else if connected {
            ("up", tr("lists.donations.connected"))
        } else {
            ("down", tr("lists.donations.not_connected"))
        }
    };
    let edit = |kind: &str, name: &str| json!({"id": "edit", "label": tr("btn.edit"), "icon": "pencil", "form": kind, "name": name});
    let (se_health, se_text) = token_status(&donations.streamelements_token, runtime.0);
    let (sl_health, sl_text) = token_status(&donations.streamlabs_token, runtime.1);
    let (drive_health, drive_text) = match (
        donations.donordrive_instance.trim(),
        donations.donordrive_participant_id.trim(),
    ) {
        ("", _) | (_, "") => ("off", tr("lists.donations.status_off").to_owned()),
        (site, id) => (
            "standby",
            trf("lists.donations.donordrive_status", &[("id", id), ("site", site)]),
        ),
    };
    let block = |title: &str, brand: &str, guide: &str, status_row: Value| {
        let mut block = section(title, "", vec![json!({"about": guide}), status_row]);
        block["brand"] = brand.into();
        block
    };
    vec![
        block(
            "StreamElements",
            "streamelements",
            tr("lists.donations.streamelements_guide"),
            status(se_health, se_text, edit("donations_streamelements", "StreamElements")),
        ),
        block(
            "Streamlabs",
            "streamlabs",
            tr("lists.donations.streamlabs_guide"),
            status(sl_health, sl_text, edit("donations_streamlabs", "Streamlabs")),
        ),
        block(
            "DonorDrive",
            "donordrive",
            tr("lists.donations.donordrive_guide"),
            status(drive_health, &drive_text, edit("donations_donordrive", "DonorDrive")),
        ),
    ]
}

/// The MCP page's connection fields (address, port, route) as one row of inputs that save when left.
pub fn mcp_connection(config: &crate::Config, about: &str) -> Value {
    let field = |key: &str, label: &str, value: String| {
        row(key, label, "", json!({"type": "text", "value": value, "commit": true}))
    };
    let websocket = &config.websockets;
    json!({"domain": "mcp", "sections": [{"title": "", "icon": "", "flow": true, "rows": [
        field("address", tr("agents.cfg.address"), websocket.address.clone()),
        field("port", tr("agents.cfg.port"), websocket.port.to_string()),
        field("endpoint", tr("agents.cfg.route"), websocket.endpoint.clone()),
        json!({"about": about}),
    ]}]})
}

/// Change one setting by its row key. Choices arrive as their index.
pub fn set(engine: &Rc<Engine>, domain: &str, key: &str, value: &Value) -> Result<Value, String> {
    let on = value.as_bool();
    let index = value.as_u64().map(|index| index as usize);
    let need_bool = || on.ok_or_else(|| format!("{key}: true or false"));
    let need_index = |limit: usize| {
        index
            .filter(|index| *index < limit)
            .ok_or_else(|| format!("{key}: a choice from 0 to {}", limit - 1))
    };
    match (domain, key) {
        ("mcp", "address" | "port" | "endpoint") => {
            let text = value.as_str().ok_or_else(|| format!("{key}: text"))?.trim().to_owned();
            let mut item = super::settings::read(&engine.config(), "mcp_connection")
                .and_then(|value| value.as_object().cloned())
                .ok_or("mcp: no connection settings")?;
            if key == "port" {
                let port = text.parse::<u16>().ok().filter(|port| *port > 0);
                item.insert(
                    "port".into(),
                    port.ok_or(tr("lists.servers.port_range_warning"))?.into(),
                );
            } else {
                item.insert(key.into(), text.into());
            }
            super::pages::save_mcp_connection(engine, item)?;
        }
        ("settings", "theme") => {
            let picked = [Theme::System, Theme::Dark, Theme::Light][need_index(3)?];
            engine.change(|config| config.theme = Some(picked));
            crate::host::system_dark(crate::host::system_is_dark());
        }
        ("settings", "language") => {
            let index = index.ok_or("language: a choice")?;
            // The first is "automatic": no language saved, the system's followed.
            let code = if index == 0 {
                ""
            } else {
                i18n::all()
                    .nth(index - 1)
                    .map(|language| language.0)
                    .ok_or("language: a listed choice")?
            };
            engine.change(|config| config.locale = code.into());
            i18n::set(code);
        }
        ("settings", "animations") => {
            let value = need_bool()?;
            engine.change(|config| config.animations = value);
        }
        ("settings", "memory_saver") => {
            let value = need_bool()?;
            engine.change(|config| config.memory_saver = value);
        }
        ("settings", "start_hidden") => {
            let value = need_bool()?;
            engine.change(|config| config.start_hidden = value);
        }
        ("accounts", key) if key.ends_with(".autostart") => {
            super::accounts::set_autostart(engine, key.trim_end_matches(".autostart"), need_bool()?)?;
        }
        ("settings", "autostart") => {
            let value = need_bool()?;
            engine.change(|config| config.autostart = value);
            if let Err(error) = crate::autostart(value) {
                crate::error(format!("Start automatically: {error}"));
            }
        }
        ("settings", "dates") => {
            let pattern = DATES[need_index(DATES.len())?];
            engine.change(|config| config.date_format = pattern.into());
            crate::fmt::set_shown_as(pattern, &engine.config().time_format);
        }
        ("settings", "times") => {
            let pattern = TIMES[need_index(TIMES.len())?];
            engine.change(|config| config.time_format = pattern.into());
            crate::fmt::set_shown_as(&engine.config().date_format, pattern);
        }
        ("settings", "updates") => {
            let mode = UPDATES[need_index(UPDATES.len())?];
            engine.change(|config| config.updates = mode);
        }
        ("settings", "debug_log") => {
            let value = need_bool()?;
            engine.change(|config| config.debug_log = value);
            crate::set_debug_log(value);
        }
        ("settings", "instance_name") => {
            let name = value.as_str().ok_or("instance_name: text")?.trim().to_owned();
            engine.change(|config| config.instance_name = name);
        }
        ("spotify", "client_id") => {
            let id = value.as_str().ok_or("client_id: text")?.trim().to_owned();
            let (changed, was_on) = {
                let config = engine.config();
                (config.spotify.client_id != id, config.spotify.enabled)
            };
            // A new client ID forgets the old login (and says so to the triggers).
            if changed {
                engine.change(|config| {
                    config.spotify.client_id = id;
                    config.spotify.enabled = false;
                    config.spotify.token = Default::default();
                });
                if was_on {
                    crate::spotify::event(engine, 99805, "");
                }
            }
        }
        ("spotify", "local_monitor") => {
            let on = need_bool()?;
            engine.change(|config| config.spotify.local_monitor = on);
            if on {
                engine.spotify_local_start();
            } else {
                engine.spotify_local_stop();
            }
        }
        ("chat_options", _) => return super::chat::set_option(engine, key, value),
        ("notifications", "login" | "updates" | "streams" | "automation") => {
            let value = need_bool()?;
            engine.change(|config| {
                match key {
                    "login" => config.notifications.login = value,
                    "updates" => config.notifications.updates = value,
                    "streams" => config.notifications.streams = value,
                    _ => config.notifications.automation = value,
                }
                crate::notify::configure(config.notifications);
            });
        }
        _ => return Err(format!("{domain} has no setting {key}")),
    }
    Ok(json!({"accepted": true}))
}

/// Run one button of a sections page.
pub fn act(engine: &Rc<Engine>, domain: &str, action: &str) -> Result<Value, String> {
    let accepted = Ok(json!({"accepted": true}));
    match (domain, action) {
        ("spotify", "open_dashboard") => {
            crate::oauth::open_browser("https://developer.spotify.com/dashboard");
            accepted
        }
        ("spotify", "copy_redirect") => accepted,
        ("spotify", "disconnect") => {
            engine.change(|config| {
                config.spotify.enabled = false;
                config.spotify.token = Default::default();
            });
            crate::spotify::event(engine, 99805, "");
            accepted
        }
        ("spotify", "connect") => {
            let id = engine.config().spotify.client_id.trim().to_owned();
            if id.is_empty() {
                return Err(tr("accounts.spotify.need_client_id").into());
            }
            let events = engine.clone();
            let request = id.clone();
            super::login::spawn(
                engine,
                "Spotify",
                &tr("accounts.who_broadcaster").to_lowercase(),
                async move {
                    let result = crate::spotify::login(&request).await;
                    if let Err(error) = &result {
                        crate::spotify::event(&events, 99806, error);
                    }
                    result
                },
                move |engine, token| {
                    let current = engine.config().spotify.client_id == id;
                    if current {
                        engine.change(|config| {
                            config.spotify.token = token;
                            config.spotify.enabled = true;
                        });
                        crate::spotify::event(engine, 99804, "");
                    }
                },
            );
            accepted
        }
        ("spotify", "check_local") => {
            tokio::task::spawn_local(async move {
                let message =
                    match tokio::time::timeout(std::time::Duration::from_secs(2), crate::spotify::local::current())
                        .await
                    {
                        Ok(Ok(Some(track))) => trf(
                            "spotify.local_found",
                            &[("artist", &track.artist), ("title", &track.title)],
                        ),
                        Ok(Ok(None)) => tr("spotify.local_missing").to_owned(),
                        Ok(Err(error)) => trf("spotify.local_error", &[("error", &error)]),
                        Err(_) => tr("spotify.local_timeout").to_owned(),
                    };
                super::prompts::notice(tr("accounts.spotify.setup").into(), message);
            });
            accepted
        }
        ("settings", "open_github") => {
            crate::oauth::open_browser(&format!("https://github.com/{}", crate::brand::REPO));
            accepted
        }
        ("settings", "restart") => {
            crate::host::restart();
            accepted
        }
        ("settings", "check_now") => {
            let cfg = engine.config_sender();
            tokio::task::spawn_local(crate::update::check(cfg, true));
            accepted
        }
        ("settings", "open_file") => {
            crate::oauth::open_browser(&crate::config_path().display().to_string());
            accepted
        }
        ("settings", "open_folder") => {
            if let Some(folder) = crate::config_path().parent() {
                crate::oauth::open_browser(&folder.display().to_string());
            }
            accepted
        }
        ("settings", "copy_path") => {
            crate::say(tr("settings.path_copied").into());
            accepted
        }
        _ => Err(format!("{domain} has no action {action}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mcp_connection_fields_save_one_by_one_and_reject_a_bad_port() {
        let _lock = crate::TEST_LOCK.lock().unwrap_or_else(|error| error.into_inner());
        crate::engine::tests::run(async {
            let engine = crate::engine::tests::engine(json!({"actions": []}));
            let model = mcp_connection(&engine.config(), "about");
            let keys: Vec<_> = model["sections"][0]["rows"]
                .as_array()
                .unwrap()
                .iter()
                .map(|row| row["key"].clone())
                .collect();
            assert_eq!(keys[..3], [json!("address"), json!("port"), json!("endpoint")]);
            assert_eq!(model["sections"][0]["rows"][1]["control"]["commit"], true);
            assert!(set(&engine, "mcp", "port", &json!("abc")).is_err());
            assert!(set(&engine, "mcp", "port", &json!("0")).is_err());
            assert!(set(&engine, "mcp", "port", &json!(8110)).is_err());
            set(&engine, "mcp", "port", &json!(" 8110 ")).unwrap();
            assert_eq!(engine.config().websockets.port, 8110);
            set(&engine, "mcp", "endpoint", &json!("agents")).unwrap();
            assert_eq!(engine.config().websockets.endpoint, "/agents");
            set(&engine, "mcp", "address", &json!("")).unwrap();
            assert_eq!(engine.config().websockets.address, "127.0.0.1");
            assert!(set(&engine, "mcp", "other", &json!("x")).is_err());
        });
    }

    #[test]
    fn settings_apply_by_key_validate_their_values_and_every_domain_reads() {
        let _lock = crate::TEST_LOCK.lock().unwrap_or_else(|error| error.into_inner());
        crate::engine::tests::run(async {
            let engine = crate::engine::tests::engine(json!({"actions": []}));
            for domain in DOMAINS {
                let model = read(&engine, domain).unwrap();
                assert!(!model["sections"].as_array().unwrap().is_empty(), "{domain}");
            }
            assert!(read(&engine, "nothing").is_err());
            set(&engine, "settings", "theme", &json!(2)).unwrap();
            assert!(!crate::host::dark_of(&engine.config()));
            assert!(set(&engine, "settings", "theme", &json!(3)).is_err());
            assert!(set(&engine, "settings", "animations", &json!("yes")).is_err());
            set(&engine, "settings", "animations", &json!(false)).unwrap();
            assert!(!engine.config().animations);
            set(&engine, "settings", "instance_name", &json!("  Studio ")).unwrap();
            assert_eq!(engine.config().instance_name, "Studio");
            set(&engine, "notifications", "streams", &json!(false)).unwrap();
            assert!(!engine.config().notifications.streams);
            assert!(set(&engine, "settings", "nonsense", &json!(true)).is_err());
            assert!(act(&engine, "settings", "nonsense").is_err());
            let model = read(&engine, "settings").unwrap();
            let theme = model["sections"][1]["rows"][0].clone();
            assert_eq!(theme["control"]["value"], 2);
            // Spotify: connecting needs a client ID, and a new ID forgets any saved login.
            assert!(act(&engine, "spotify", "connect").is_err());
            engine.change(|config| {
                config.spotify.client_id = "old".into();
                config.spotify.enabled = true;
                config.spotify.token.access_token = "token".into();
            });
            assert_eq!(
                read(&engine, "integrations").unwrap()["sections"][1]["rows"][0]["status"]["health"],
                "up"
            );
            set(&engine, "spotify", "client_id", &json!("new")).unwrap();
            assert!(!engine.config().spotify.enabled && engine.config().spotify.token.access_token.is_empty());
            assert_eq!(
                read(&engine, "spotify").unwrap()["sections"][0]["rows"][3]["control"]["buttons"][0]["enabled"],
                true
            );
        });
    }
}
