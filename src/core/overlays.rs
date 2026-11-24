//! The Overlays page: every page (an `.html` file of an action group) with its address, and what is
//! done with one: copy its address, add it to OBS, open it, change its group's settings, open its
//! folder, share it as a `.streamerz` file, import one, start a new one.
//!
//! This is the page's model and its operations only. A frontend draws the table and the forms from
//! what is returned here, and MCP reaches the same through `core_view`, `core_action`, `entity_form`
//! and `edit_entity`. The work itself is in `overlays.rs` (serving, OBS), `package.rs` (the file) and
//! `groups.rs` (files and settings).
//!
//! A row's id is `<group>:<file>` (each part encoded). Forms have kinds `overlay:<form>:<row id>`:
//! `settings`, `page` and `export` for a row, `import` and `new` for none.

use super::actions::{dec, enc};
use super::pages::{button, confirm, danger};
use super::view::column;
use crate::engine::Engine;
use crate::groups::{self, Page};
use crate::i18n::{tr, trf};
use crate::{overlays, package};
use serde_json::{json, Value};
use std::rc::Rc;

pub const DOMAIN: &str = "overlays";
const FORMS: &str = "overlay:";

/// The list of bundled examples, shown in a dialog over the page.
pub const EXAMPLES: &str = "overlay_examples";

pub fn supports(domain: &str) -> bool {
    domain == DOMAIN || domain == EXAMPLES
}

/// Whether a form kind is one of this page's.
pub fn handles(kind: &str) -> bool {
    kind.starts_with(FORMS)
}

fn row_id(group: &str, file: &str) -> String {
    format!("{}:{}", enc(group), enc(file))
}

/// The group and file a row id names, when that page is still there.
fn row(engine: &Rc<Engine>, id: &str) -> Result<(String, String), String> {
    let (group, file) = id.split_once(':').ok_or_else(gone)?;
    let (group, file) = (dec(group), dec(file));
    if groups::file_path(&engine.config(), &group, &file).is_none() {
        return Err(gone());
    }
    Ok((group, file))
}

fn gone() -> String {
    tr("overlays.gone").into()
}

fn form_button(id: &str, label: &'static str, icon: &str, needs_row: bool) -> Value {
    let mut b = button(id, label, icon, needs_row);
    if needs_row {
        b["form"] = format!("{FORMS}{id}:").into();
    } else {
        b["entity"] = format!("{FORMS}{id}").into();
    }
    b
}

/// Files put in an overlay's folder by hand (or deleted there) since the list was last made: the
/// overlays' lists of files are brought in step with their folders.
pub fn sync(engine: &Rc<Engine>) {
    let stale: Vec<String> = {
        let config = engine.config();
        groups::all(&config)
            .into_iter()
            .filter(|g| groups::synced(&config, g).is_some())
            .collect()
    };
    if !stale.is_empty() {
        engine.change(|config| {
            for group in &stale {
                groups::sync_files(config, group);
            }
        });
    }
}

fn example_summary(about: &Value) -> String {
    let mut parts = Vec::new();
    let count_part = |n: u64, singular: &'static str, plural: &'static str, out: &mut Vec<String>| match n {
        0 => {}
        1 => out.push(tr(singular).to_owned()),
        n => out.push(trf(plural, &[("count", &n.to_string())])),
    };
    count_part(
        about["pages"].as_array().map_or(0, Vec::len) as u64,
        "overlays.examples.page",
        "overlays.examples.pages",
        &mut parts,
    );
    count_part(
        about["settings"].as_u64().unwrap_or(0),
        "overlays.examples.setting",
        "overlays.examples.settings",
        &mut parts,
    );
    count_part(
        about["actions"].as_u64().unwrap_or(0),
        "overlays.examples.action",
        "overlays.examples.actions",
        &mut parts,
    );
    count_part(
        about["commands"].as_u64().unwrap_or(0),
        "overlays.examples.command",
        "overlays.examples.commands",
        &mut parts,
    );
    count_part(
        about["timers"].as_u64().unwrap_or(0),
        "overlays.examples.timer",
        "overlays.examples.timers",
        &mut parts,
    );
    parts.join(" · ")
}

/// The bundled examples: what each one is, and whether it is already here.
fn read_examples(engine: &Rc<Engine>) -> Result<Value, String> {
    let config = engine.config();
    let mut rows = Vec::new();
    for example in crate::examples::ALL {
        let about = package::describe(&config, &crate::examples::package(example.slug)?);
        let state = match about["installed"]["state"].as_str() {
            Some("update") => tr("overlays.examples.update"),
            Some("same") => tr("overlays.examples.added"),
            _ => "",
        };
        let mut row = json!({
            "id": example.slug,
            "name": about["name"],
            "enabled": true,
            "can": {
                "add_example": about["installed"]["state"] != "same",
                "example_obs": about["pages"].as_array().is_some_and(|pages| pages.iter().any(|page| page["dock"] != true))
            },
            "detail": about["about"],
            "detailTitle": about["name"],
            "detailOk": true,
            "cells": [about["name"], about["about"], state]
        });
        if !state.is_empty() {
            row["detailBadge"] = state.into();
        }
        let summary = example_summary(&about);
        if !summary.is_empty() {
            row["detailNote"] = summary.into();
        }
        rows.push(row);
    }
    let mut about_col = column("about", "overlays.header.about", 280);
    about_col["hidden"] = true.into();
    let mut table = json!({"kind": "overlay_example", "title": tr("overlays.examples.title"), "what": "",
    "domain": EXAMPLES, "guide": tr("overlays.examples.guide"),
    "columns": [
        column("name", "overlays.header.example", 170),
        about_col,
        column("state", "overlays.header.state", 88),
    ],
    "rows": rows, "empty": "", "search": true, "showTitle": false,
    "actions": [button("add_example", "overlays.examples.add", "plus", true),
        {"id":"example_obs", "label":tr("overlays.btn.add_to_obs"), "icon":"monitor-play", "needs":"row", "form":"overlay:example_obs:"}]});
    table["perRow"] = 2.into();
    table["titleless"] = true.into();
    table["selectFirst"] = true.into();
    table["dialogWidth"] = 760.into();
    table["detailSide"] = true.into();
    table["detail"] = tr("overlays.examples.selection_hint").into();
    Ok(json!({"domain": EXAMPLES, "tables": [table]}))
}

/// Add a bundled example, or bring the one here up to the bundled version. Nothing is asked: it
/// comes with Streamerz, and the values already set stay.
pub fn add_example(engine: &Rc<Engine>, slug: &str) -> Result<String, String> {
    let package = crate::examples::package(slug)?;
    let about = package::describe(&engine.config(), &package);
    let mut result = Err(String::new());
    engine.change(|config| {
        // A group of the user's own with the example's name is left alone: the example gets a number.
        result = if about["installed"]["state"] == "replaces" {
            let free = (2..1000)
                .map(|n| format!("{} {n}", package.name))
                .find(|name| !groups::exists(config, name))
                .unwrap_or_default();
            package::install_as(config, &package, &free)
        } else {
            package::install(config, &package)
        };
    });
    let (group, _) = result?;
    crate::say(trf("overlays.import.done", &[("group", &group)]));
    Ok(group)
}

thread_local! {
    /// How many times Test was pressed for each overlay: each press sends the next of the made-up
    /// people and events, from the first.
    static ROUNDS: std::cell::RefCell<std::collections::HashMap<String, usize>> = Default::default();
}

/// A made-up picture: a face on a colour, or a record sleeve, drawn here so Test needs no network.
fn drawing(hue: usize, body: &str) -> String {
    let (from, to) = [
        ("7c3aed", "db2777"),
        ("0891b2", "4f46e5"),
        ("16a34a", "ca8a04"),
        ("ea580c", "be123c"),
        ("0d9488", "2563eb"),
    ][hue % 5];
    format!(
        "data:image/svg+xml,%3Csvg xmlns='http://www.w3.org/2000/svg' viewBox='0 0 96 96'%3E%3ClinearGradient \
         id='g' x1='0' y1='0' x2='1' y2='1'%3E%3Cstop stop-color='%23{from}'/%3E%3Cstop offset='1' \
         stop-color='%23{to}'/%3E%3C/linearGradient%3E%3Crect width='96' height='96' fill='url(%23g)'/%3E{body}%3C/svg%3E"
    )
}

const FACE: &str = "%3Ccircle cx='48' cy='38' r='17' fill='%23fff' fill-opacity='.9'/%3E%3Cpath d='M14 96c2-22 16-32 \
                    34-32s32 10 34 32z' fill='%23fff' fill-opacity='.9'/%3E";
const SLEEVE: &str = "%3Ccircle cx='48' cy='48' r='30' fill='%23000' fill-opacity='.35'/%3E%3Ccircle cx='48' cy='48' \
                      r='11' fill='%23fff' fill-opacity='.85'/%3E%3Ccircle cx='48' cy='48' r='3' fill='%23000' \
                      fill-opacity='.5'/%3E";

/// What Test sends to one overlay's open pages, made up: a chat message, an alert, a TikTok gift and a
/// song, different at each press (the next platform, the next kind of event), then what the overlay's
/// own `template.test` adds. They are built as the real ones are (the same lines the chat makes), and
/// only that overlay's pages get them: nothing reaches the chat, the actions or other overlays.
fn sample(engine: &Rc<Engine>, group: &str) -> Value {
    use crate::engine::chatlog::{alert_value, event_line, Line};
    let round = ROUNDS.with_borrow_mut(|rounds| {
        let round = rounds.entry(group.to_string()).or_default();
        *round = round.wrapping_add(1);
        *round - 1
    });
    let base = overlays::base(&engine.config());
    let now = crate::fmt::unix_now().as_millis() as u64;
    let people = [
        ("twitch", "ana_plays", "#ff7ab8"),
        ("youtube", "Leo Martins", ""),
        ("kick", "duda", "#53fc18"),
        ("tiktok", "bia.live", ""),
        ("twitch", "noobmaster69", "#5ad1ff"),
        ("youtube", "Rafa Gomes", ""),
    ];
    let (platform, user, color) = people[round % people.len()];
    let said = [
        tr("overlays.test.chat"),
        tr("overlays.test.chat2"),
        tr("overlays.test.chat3"),
    ][round % 3];
    let chat = Line {
        platform: platform.into(),
        msg_id: crate::engine::uuid(),
        time: crate::fmt::shown_time(&crate::fmt::now()),
        user: user.into(),
        user_id: format!("test-{}", round % people.len()),
        text: said.into(),
        color: color.into(),
        moderator: round % 4 == 1,
        subscriber: round.is_multiple_of(2),
        vip: round % 5 == 3,
        ..Default::default()
    };
    // The events, with the arguments their triggers carry.
    let rose = format!("{base}/overlays/-/icons/ff5c8a/gift.svg");
    let events: [(u32, Value); 10] = [
        (103, json!({"user": "ana_plays", "userId": "test-0"})),
        (
            4006,
            json!({"user": "Leo Martins", "userId": "test-1", "amount": "$10.00", "message": said}),
        ),
        (35014, json!({"user": "duda", "userId": "test-2"})),
        (
            41002,
            json!({"user": "bia.live", "userId": "test-3", "giftName": "Rose", "repeatCount": 5, "coins": 5,
            "giftPictureUrl": rose, "repeatEnd": true}),
        ),
        (107, json!({"user": "noobmaster69", "userId": "test-4", "viewers": 42})),
        (4018, json!({"user": "Rafa Gomes", "userId": "test-5"})),
        (102, json!({"user": "ana_plays", "userId": "test-0", "bits": 500})),
        (
            35016,
            json!({"user": "duda", "userId": "test-2", "recipientUser": "noobmaster69"}),
        ),
        (101, json!({"user": "noobmaster69", "userId": "test-4"})),
        (
            4008,
            json!({"user": "Leo Martins", "userId": "test-1", "levelName": "Fan"}),
        ),
    ];
    let (kind, args) = &events[round % events.len()];
    let args: crate::model::Obj = args.as_object().cloned().unwrap_or_default().into_iter().collect();
    let alert = event_line(*kind, &args);
    // A gift as TikTok tells it: a combo that grows with each press and ends on the fifth.
    let gifts = [
        ("Rose", 1),
        ("Heart Me", 1),
        ("Finger Heart", 5),
        ("Doughnut", 30),
        ("Galaxy", 1000),
    ];
    let (gift, diamonds) = gifts[(round / 5) % gifts.len()];
    let count = round % 5 + 1;
    let gifter = people[(round / 5) % people.len()].1;
    let gift = json!({"user": gifter, "nickname": gifter, "uniqueId": gifter, "userId": format!("test-gift-{}", round / 5),
        "profilePictureUrl": drawing(round / 5, FACE), "giftId": 5655 + (round / 5) % gifts.len(), "giftName": gift,
        "giftType": 1,
        "giftPictureUrl": rose, "diamondCount": diamonds, "repeatCount": count, "repeatEnd": count == 5,
        "coins": diamonds * count as u64, "userTotalCoins": diamonds * count as u64});
    let mut list = vec![json!([
        "Chat.Line",
        crate::core::chat::overlay_line(engine, &chat, &drawing(round, FACE))
    ])];
    if let Some(alert) = alert {
        list.push(json!([
            "Chat.Alert",
            alert_value(engine, *kind, &args, &alert, &drawing(round + 2, FACE))
        ]));
    }
    list.push(json!(["TikTok.Gift", gift]));
    let songs = [
        ("overlays.test.song", "Streamerz"),
        ("overlays.test.song2", "The Overlays"),
    ];
    let (title, artist) = songs[(round / 2) % 2];
    let mut topics = serde_json::Map::new();
    topics.insert(
        "song".into(),
        json!({"title": tr(title), "artist": artist, "album": tr("overlays.test.album"),
            "cover": drawing(round / 2 + 1, SLEEVE), "playing": round % 6 != 5, "durationMs": 215_000,
            "positionMs": 20_000 + (round as u64 % 6) * 30_000, "at": now}),
    );
    // What the overlay itself wants tried: its own topics and events.
    let own = groups::meta(&engine.config(), group).template.test;
    for (topic, value) in own["topics"].as_object().into_iter().flatten() {
        topics.insert(topic.clone(), value.clone());
    }
    for event in own["events"].as_array().into_iter().flatten() {
        if event[0].is_string() {
            list.push(event.clone());
        }
    }
    json!({"events": list, "topics": topics})
}

pub fn read(engine: &Rc<Engine>, domain: &str) -> Result<Value, String> {
    if domain == EXAMPLES {
        return read_examples(engine);
    }
    let config = engine.config();
    let rows: Vec<Value> = overlays::list(&config)
        .into_iter()
        .map(|(group, file)| {
            let page = overlays::page_of(&config, &group, &file);
            let meta = groups::meta(&config, &group);
            let connected = groups::listed(&config, &group);
            let kind = if page.dock {
                tr("overlays.kind.dock")
            } else {
                tr("overlays.kind.overlay")
            };
            let open = overlays::open_pages(&group);
            let open = if open == 0 {
                tr("overlays.open.none").to_string()
            } else {
                trf("overlays.open.count", &[("count", &open.to_string())])
            };
            let address = overlays::url(&config, &group, &file);
            // The group of actions of the overlay's name, with a chain: what its pages can run.
            let actions = if connected {
                json!({"text": holds(groups::counts(&config, &group)), "icon": "link"})
            } else {
                json!("")
            };
            let pages = meta.files.iter().filter(|f| groups::is_page(&f.name)).count();
            let delete = match (pages == 1, connected) {
                (true, true) => trf("overlays.delete_last_connected", &[("name", &group)]),
                (true, false) => trf("overlays.delete_last", &[("name", &group)]),
                _ => tr("overlays.delete_confirm").to_owned(),
            };
            json!({"id": row_id(&group, &file), "name": overlays::page_name(&page), "enabled": true,
                "can": {"connect": !connected, "disconnect": connected, "obs_setup": !page.dock, "add_to_obs": page.dock},
                "confirm": {"delete": delete, "disconnect": trf("overlays.disconnect_confirm", &[("name", &group)])},
                "detail": if meta.description.is_empty() { address.clone() } else { format!("{}\n{address}", meta.description) },
                "detailOk": true,
                "cells": [overlays::page_name(&page), group, kind, actions, open, address]})
        })
        .collect();
    let server_on = config.http.auto_start;
    // Above the list: what starts or brings in an overlay. Under it: what is done with the selected one.
    let top = |mut b: Value| {
        b["top"] = true.into();
        b
    };
    let icon = |mut b: Value| {
        b["iconOnly"] = true.into();
        b
    };
    let tip = |mut b: Value, text: &'static str| {
        b["tip"] = tr(text).into();
        b
    };
    let mut examples = top(button("examples", "overlays.btn.examples", "layout-template", false));
    examples["dialog"] = EXAMPLES.into();
    let mut new = top(form_button("new", "overlays.btn.new", "plus", false));
    new["primary"] = true.into();
    let guide = super::status::failure("http")
        .and_then(|failure| failure["text"].as_str().map(str::to_owned))
        .unwrap_or_else(|| {
            tr(if server_on {
                "overlays.guide"
            } else {
                "overlays.guide_server_off"
            })
            .to_owned()
        });
    let mut table = json!({"kind": "overlay", "title": "", "what": "", "domain": DOMAIN,
    "guide": guide,
    "columns": [
        column("name", "overlays.header.page", 220),
        column("group", "overlays.header.group", 170),
        column("kind", "overlays.header.kind", 90),
        column("actions", "overlays.header.actions", 170),
        column("open", "overlays.header.open", 100),
        column("address", "overlays.header.address", 420),
    ],
    "rows": rows, "empty": tr("overlays.empty"), "search": true, "showTitle": false,
    "actions": [
        examples,
        if !server_on { top(button("enable_server", "overlays.enable_server", "power", false)) }
        else { icon(top(button("server_state", "overlays.server_state", "server", false))) },
        top(form_button("import", "btn.import", "download", false)),
        new,
        icon(top(button("guide", "overlays.btn.guide", "book-open", false))),
        icon(top(button("refresh", "btn.refresh", "refresh-cw", false))),
        {"id":"obs_setup", "label":tr("overlays.btn.add_to_obs"), "icon":"monitor-play", "needs":"row", "form":"overlay:obs_setup:", "hideDisabled":true},
        {"id":"add_to_obs", "label":tr("overlays.btn.add_to_obs"), "icon":"monitor-play", "needs":"row", "hideDisabled":true},
        {"id": "open_page", "label": tr("overlays.btn.open_page"), "icon": "panel-right-open", "needs": "row", "view": "overlay"},
        button("copy_url", "overlays.btn.copy_url", "copy", true),
        button("open", "overlays.btn.open", "external-link", true),
        tip(button("test", "overlays.btn.test", "play", true), "overlays.test.tip"),
        tip(button("inspect", "overlays.btn.inspect", "activity", true), "overlays.inspect.tip"),
        form_button("rename", "btn.rename", "pencil", true),
        form_button("settings", "btn.settings", "settings", true),
        form_button("page", "overlays.btn.page", "ruler", true),
        tip(form_button("connect", "overlays.btn.connect", "link", true), "overlays.connect.tip"),
        confirm(
            button("disconnect", "overlays.btn.disconnect", "unlink", true),
            tr("overlays.disconnect_confirm").into(),
            "overlays.btn.disconnect",
        ),
        button("open_folder", "btn.open_folder", "folder-open", true),
        form_button("export", "btn.export", "upload", true),
        danger(confirm(button("delete", "btn.delete", "trash-2", true), tr("overlays.delete_confirm").into(), "btn.delete")),
    ]});
    table["strip"] = true.into();
    table["open"] = "open_page".into();
    table["detail"] = "".into();
    table["detailHeight"] = 120.into();
    Ok(json!({"domain": DOMAIN, "tables": [table]}))
}

/// Whether an overlay is edited with Show on OBS while I edit on (each change kept at once).
fn live_edit(group: &str) -> bool {
    LIVE_EDIT.with_borrow(|live| live.contains(group))
}

thread_local! {
    /// The overlays edited with Show on OBS while I edit on.
    static LIVE_EDIT: std::cell::RefCell<std::collections::HashSet<String>> = Default::default();
    /// The tab each overlay's page showed last, by group.
    static LAST_TAB: std::cell::RefCell<std::collections::HashMap<String, String>> = Default::default();
}

/// The tabs of an overlay's own page, in order; the Alerts tab only for an alert box.
fn page_tabs(meta: &groups::Meta) -> Vec<&'static str> {
    let mut tabs = vec!["settings", "test", "media", "live"];
    if is_alert_box(meta) {
        tabs.push("alerts");
    }
    tabs.push("code");
    tabs
}

/// An alert box keeps its rules in a list setting called `rules` (the example, and copies of it).
pub fn is_alert_box(meta: &groups::Meta) -> bool {
    meta.template.id.ends_with("alert-box")
        || meta
            .variables
            .iter()
            .any(|v| v.name == "rules" && v.kind == groups::Kind::List)
}

/// The row id of an overlay given by its group (its first page), for `core_view` `overlay:<group>`.
pub fn first_page(engine: &Rc<Engine>, group: &str) -> Result<String, String> {
    overlays::list(&engine.config())
        .into_iter()
        .find(|(g, _)| g == group)
        .map(|(g, f)| row_id(&g, &f))
        .ok_or_else(gone)
}

/// An overlay's own page: what the window draws in place of the list once a row is opened, and what
/// MCP reads as `overlay:<group>`. `tab` is the one shown (empty: Settings).
pub fn page(engine: &Rc<Engine>, id: &str, tab: &str) -> Result<Value, String> {
    let (group, file) = row(engine, id)?;
    let config = engine.config();
    let meta = groups::meta(&config, &group);
    let info = overlays::page_of(&config, &group, &file);
    let tabs = page_tabs(&meta);
    // The tab last shown for this overlay, kept by the core so Back and a new window find it again.
    let remembered = LAST_TAB
        .with_borrow(|last| last.get(&group).cloned())
        .unwrap_or_default();
    let tab = if tab.is_empty() { remembered.as_str() } else { tab };
    let tab = if tabs.contains(&tab) { tab } else { "settings" };
    LAST_TAB.with_borrow_mut(|last| last.insert(group.clone(), tab.to_owned()));
    let options: Vec<Value> = tabs
        .iter()
        .map(|t| {
            json!({"id": t, "label": tr(match *t {
                "settings" => "overlays.tab.settings",
                "test" => "overlays.tab.test",
                "media" => "overlays.tab.media",
                "live" => "overlays.tab.live",
                "alerts" => "overlays.tab.alerts",
                _ => "overlays.tab.code",
            })})
        })
        .collect();
    let mut toolbar = vec![
        if info.dock {
            button("add_to_obs", "overlays.btn.add_to_obs", "monitor-play", true)
        } else {
            form_button("obs_setup", "overlays.btn.add_to_obs", "monitor-play", true)
        },
        button("copy_url", "overlays.btn.copy_url", "copy", true),
        button("open", "overlays.btn.open", "external-link", true),
        button("open_folder", "btn.open_folder", "folder-open", true),
        form_button("export", "btn.export", "upload", true),
    ];
    for b in &mut toolbar {
        if b["form"].is_string() {
            b["form"] = format!("{}{id}", b["form"].as_str().unwrap_or_default()).into();
        }
    }
    let section = |title: &str, body: Value| {
        let mut section = body;
        if !title.is_empty() {
            section["title"] = title.into();
        }
        section
    };
    let sections = match tab {
        "settings" => {
            let request = json!({"kind": format!("{FORMS}settings:{id}")});
            let draft = engine.overlay_draft(&group);
            // The form shows the draft, as the preview does: it lives in the core, so it outlasts the window.
            let mut form = form_for(engine, &request)?;
            if !draft.is_empty() {
                let shown = super::actions::group_settings_form_with(&config, &group, &draft);
                form["fields"] = shown["fields"].clone();
            }
            form["empty"] = tr("overlays.settings.none").into();
            vec![json!({"form": form})]
        }
        "code" => {
            let request = json!({"kind": format!("{FORMS}page:{id}")});
            vec![json!({"form": form_for(engine, &request)?,
                "buttons": [button("inspect", "overlays.btn.inspect", "activity", true)]})]
        }
        "test" => {
            let mut sets = vec![button("test", "overlays.test.quick", "play", true)];
            sets.extend(
                super::overlay_test::SETS
                    .iter()
                    .map(|(id, label)| button(id, label, "play", true)),
            );
            let replay: Vec<Value> = super::overlay_test::recent()
                .into_iter()
                .map(|e| json!({"text": e["title"], "detail": format!("{} {}", e["at"].as_str().unwrap_or_default(), e["who"].as_str().unwrap_or_default()),
                    "button": {"id": e["id"], "label": tr("overlays.test.replay"), "icon": "rotate-ccw"}}))
                .collect();
            let seen = inspector(engine, &group);
            let happened: Vec<Value> = seen["rows"]
                .as_array()
                .into_iter()
                .flatten()
                .map(|r| json!({"text": r["name"], "detail": r["value"]}))
                .collect();
            vec![
                {
                    let mut first = json!({"form": super::overlay_test::form(format!("{FORMS}test:{id}"))});
                    if overlays::open_pages(&group) == 0 {
                        first["text"] = tr("overlays.test.no_pages").into();
                    }
                    first
                },
                section(tr("overlays.test.sets"), json!({"buttons": sets})),
                section(
                    tr("overlays.test.song_title"),
                    json!({"buttons": [
                        button("song_play", "overlays.test.song_play", "play", true),
                        button("song_pause", "overlays.test.song_pause", "pause", true),
                        form_button("topic", "overlays.test.topic", "radio", true),
                    ]}),
                ),
                section(
                    tr("overlays.test.replay_title"),
                    json!({"rows": replay, "empty": tr("overlays.test.replay_none")}),
                ),
                section(
                    tr("overlays.test.happened"),
                    json!({"rows": happened, "empty": seen["hint"]}),
                ),
            ]
        }
        "media" => vec![super::overlay_media::section(engine, &group, id)],
        "live" => live_sections(&config, &group, id),
        _ => vec![json!({"text": tr("overlays.alerts.soon")})],
    };
    // A form button of a section asks for its form for this overlay.
    let mut sections = sections;
    for section in &mut sections {
        for b in section["buttons"].as_array_mut().into_iter().flatten() {
            if let Some(prefix) = b["form"].as_str().filter(|f| f.ends_with(':')) {
                b["form"] = format!("{prefix}{id}").into();
            }
        }
    }
    Ok(json!({"domain": DOMAIN, "overlay": {
        "id": id,
        "group": group,
        "file": file,
        "title": overlays::page_name(&info),
        "subtitle": if overlays::page_name(&info) == group { String::new() } else { group.clone() },
        "back": tr("overlays.back"),
        "url": overlays::url(&config, &group, &file),
        "width": if info.width == 0 { 1920 } else { info.width },
        "height": if info.height == 0 { 1080 } else { info.height },
        "dock": info.dock,
        "toolbar": toolbar,
        "draft": engine.overlay_draft(&group).len(),
        "live": live_edit(&group),
        "tabs": {"options": options, "selected": tab},
        "tab": tab,
        "sections": sections,
    }}))
}

/// `StreamerzPreview`: the live preview of an overlay's page. `op` start (`itemId`, `background`),
/// stop, frame (`have`: the newest picture after that one) or input (`event`, at the page's own
/// coordinates).
pub fn preview(engine: &Rc<Engine>, request: &Value) -> Result<Value, String> {
    match request["op"].as_str().unwrap_or_default() {
        "start" => {
            let (group, file) = row(engine, request["itemId"].as_str().unwrap_or_default())?;
            server_on(engine)?;
            let config = engine.config();
            let page = overlays::page_of(&config, &group, &file);
            crate::overlay_preview::start(crate::overlay_preview::Ask {
                // A preview page shows the editor's draft; OBS keeps the saved settings.
                url: overlays::preview_url(&config, &group, &file),
                width: if page.width == 0 { 1920 } else { page.width },
                height: if page.height == 0 { 1080 } else { page.height },
                background: request["background"].as_str().unwrap_or_default().to_owned(),
            });
            Ok(json!({"accepted": true}))
        }
        "stop" => {
            crate::overlay_preview::stop();
            Ok(json!({"accepted": true}))
        }
        "input" => {
            crate::overlay_preview::input(request["event"].clone());
            Ok(json!({"accepted": true}))
        }
        // The settings form's changes as a draft: preview pages show them at once. With Show on OBS
        // while I edit, they are kept at once and every page shows them.
        "draft" => {
            let (group, _) = row(engine, request["itemId"].as_str().unwrap_or_default())?;
            let mut meta = groups::meta(&engine.config(), &group);
            let mut current = groups::values(&engine.config(), &group);
            current.extend(engine.overlay_draft(&group));
            let mut values = super::actions::settings_changes_from(&mut meta, request, true, &current)?;
            // The description is not a setting the pages see.
            let defs = groups::settings_defs(&meta);
            values.retain(|name, _| defs.iter().any(|v| &v.name == name));
            // A moment typed from now (`+15m`, `20:00`) goes back in its box as the day and time it
            // became, or the next change would count from then again.
            let mut shown = serde_json::Map::new();
            for change in request["changes"].as_array().into_iter().flatten() {
                let at = change["field"].as_u64().unwrap_or(0) as usize;
                let Some(def) = at.checked_sub(1).and_then(|i| defs.get(i)) else {
                    continue;
                };
                if def.kind != groups::Kind::DateTime {
                    continue;
                }
                if let Some(ms) = values.get(&def.name).and_then(Value::as_u64) {
                    let text = groups::kinds::show_moment(ms);
                    if change["value"].as_str().map(str::trim) != Some(text.as_str()) {
                        shown.insert(at.to_string(), text.into());
                    }
                }
            }
            values.retain(|name, value| current.get(name) != Some(value));
            if !values.is_empty() {
                engine.overlay_draft_set(&group, &values)?;
                if live_edit(&group) {
                    engine.overlay_draft_save(&group)?;
                }
            }
            Ok(json!({"draft": engine.overlay_draft(&group).len(), "shown": shown}))
        }
        "controls" => {
            let (group, _) = row(engine, request["itemId"].as_str().unwrap_or_default())?;
            live_changes(engine, &group, request)
        }
        "revert" => {
            let (group, _) = row(engine, request["itemId"].as_str().unwrap_or_default())?;
            engine.overlay_draft_revert(&group, None);
            Ok(json!({"accepted": true, "draft": 0}))
        }
        "save" => {
            let (group, _) = row(engine, request["itemId"].as_str().unwrap_or_default())?;
            let saved = engine.overlay_draft_save(&group)?;
            Ok(json!({"accepted": true, "saved": saved, "draft": 0}))
        }
        "live" => {
            let (group, _) = row(engine, request["itemId"].as_str().unwrap_or_default())?;
            let on = request["on"].as_bool().unwrap_or(false);
            LIVE_EDIT.with_borrow_mut(|live| {
                if on {
                    live.insert(group.clone());
                } else {
                    live.remove(&group);
                }
            });
            // Turned on with a draft: OBS shows it now.
            if on {
                engine.overlay_draft_save(&group)?;
            }
            Ok(json!({"accepted": true, "live": on}))
        }
        "frame" => {
            let mut out = crate::overlay_preview::frame(request["have"].as_u64().unwrap_or(0));
            if let (Some(out), Value::Object(scene)) = (
                out.as_object_mut(),
                crate::overlay_preview::scene(request["haveScene"].as_u64().unwrap_or(0)),
            ) {
                out.extend(scene);
            }
            Ok(out)
        }
        // Your scene: one picture of what OBS shows now, taken when asked (no timer).
        "scene" => {
            if engine.config().obs.connections.is_empty() {
                return Err(tr("overlays.obs.none").into());
            }
            let width = request["width"].as_u64().unwrap_or(1280).clamp(8, 3840);
            let engine = engine.clone();
            tokio::task::spawn_local(async move {
                let obs = engine.obs_start();
                let shot = async {
                    let now = obs.send_request(None, "GetCurrentProgramScene", json!({})).await?;
                    let scene = now["currentProgramSceneName"].as_str().unwrap_or_default().to_owned();
                    obs.send_request(
                        None,
                        "GetSourceScreenshot",
                        json!({"sourceName": scene, "imageFormat": "jpg", "imageWidth": width, "imageCompressionQuality": 80}),
                    )
                    .await
                };
                match shot.await {
                    Ok(v) => crate::overlay_preview::set_scene(v["imageData"].as_str().unwrap_or_default()),
                    Err(e) => crate::warn(format!("Overlay preview: no picture of the scene: {e}")),
                }
            });
            Ok(json!({"accepted": true}))
        }
        other => Err(format!("no preview operation {other}")),
    }
}

/// The Live tab: a form of the overlay's controls, each set as it changes, and their OBS dock.
fn live_sections(config: &crate::Config, group: &str, id: &str) -> Vec<Value> {
    let controls = groups::controls(config, group);
    if controls.is_empty() {
        return vec![json!({"text": tr("overlays.live.none")})];
    }
    let fields: Vec<Value> = controls
        .iter()
        .map(|(c, v)| {
            let label = c.shown_label();
            match c.kind.as_str() {
                "button" => json!({"key": c.name, "label": label, "widget": {"kind": "button",
                    "icon": if c.icon.is_empty() { "zap" } else { c.icon.as_str() }},
                    "buttonLabel": label, "value": null,
                    "press": {"request": {"request": "StreamerzCoreAction", "domain": "overlays", "table": "overlay",
                        "action": format!("control:{}", c.name), "itemId": group, "id": "overlay:press"}}}),
                "toggle" => json!({"key": c.name, "label": label, "widget": {"kind": "bool"}, "value": v.as_bool().unwrap_or(false)}),
                "slider" => {
                    let (min, max, step) = c.range();
                    json!({"key": c.name, "label": label, "widget": {"kind": "slider", "min": min, "max": max, "step": step},
                        "value": v.as_f64().unwrap_or(min)})
                }
                "choice" => json!({"key": c.name, "label": label, "widget": {"kind": "pick", "options": c.options},
                    "value": c.options.iter().position(|o| v.as_str() == Some(o)).unwrap_or(0)}),
                "color" => json!({"key": c.name, "label": label, "widget": {"kind": "color"}, "value": v}),
                _ => json!({"key": c.name, "label": label, "widget": {"kind": "text"},
                    "value": v.as_str().map(str::to_owned).unwrap_or_else(|| v.to_string())}),
            }
        })
        .collect();
    let dock = overlays::controls_url(config, group);
    vec![
        json!({"form": {"kind": format!("{FORMS}live:{id}"), "title": tr("overlays.tab.live"), "live": true,
            "json": false, "hint": tr("overlays.live.hint"), "item": {"id": id}, "fields": fields}}),
        json!({"title": tr("overlays.live.dock"), "text": tr("overlays.live.dock_how"),
            "rows": [{"text": tr("overlays.live.dock_address"), "detail": dock, "icon": "panel-right-open",
                "button": {"id": "control_dock_copy", "label": tr("overlays.btn.copy_url"), "icon": "copy"}}]}),
    ]
}

/// The Live tab's changes: each control set as the form now holds it (`changes`: field and value).
fn live_changes(engine: &Rc<Engine>, group: &str, request: &Value) -> Result<Value, String> {
    let controls = groups::controls(&engine.config(), group);
    for change in request["changes"].as_array().into_iter().flatten() {
        let Some((control, _)) = change["field"].as_u64().and_then(|i| controls.get(i as usize)) else {
            continue;
        };
        let value = match control.kind.as_str() {
            "choice" => change["value"]
                .as_u64()
                .and_then(|i| control.options.get(i as usize))
                .map(|o| json!(o))
                .unwrap_or(Value::Null),
            "button" => continue,
            _ => change["value"].clone(),
        };
        engine.overlay_control_set(group, &control.name, &value, "window", false)?;
    }
    Ok(json!({"accepted": true}))
}

/// A button of a settings form: `press:<setting>` (a `button`), `play:<setting>` (an `animation`
/// again) or `sound:<setting>` (a `sound` as the form has it now).
pub fn is_press(action: &str) -> bool {
    ["press:", "play:", "sound:", "control:"]
        .iter()
        .any(|p| action.starts_with(p))
}

/// What a button of a settings form does. `itemId` is the overlay; a sound's `value` is the form's.
pub fn press(engine: &Rc<Engine>, request: &Value) -> Result<Value, String> {
    let group = request["itemId"].as_str().unwrap_or_default();
    let action = request["action"].as_str().unwrap_or_default();
    let (op, name) = action.split_once(':').ok_or("no setting named")?;
    let config = engine.config();
    if !groups::exists(&config, group) {
        return Err(tr("overlays.gone").to_owned());
    }
    // A button of the Live tab: pressed now.
    if op == "control" {
        drop(config);
        engine.overlay_control_set(group, name, &Value::Null, "window", false)?;
        return Ok(json!({"accepted": true}));
    }
    let def = groups::meta(&config, group)
        .variables
        .into_iter()
        .find(|d| d.name == name)
        .ok_or_else(|| format!("{group} has no setting {name}"))?;
    match op {
        "press" => {
            drop(config);
            let at = crate::fmt::unix_now().as_millis() as u64;
            engine.overlay_publish(&format!("{group}.{name}"), json!({"pressedAt": at}));
            Ok(json!({"accepted": true, "message": trf("overlays.pressed", &[("name", &def.label)])}))
        }
        "play" => {
            drop(config);
            engine.overlay_play(group, name);
            Ok(json!({"accepted": true}))
        }
        _ => {
            let value = &request["value"];
            let file = value["file"].as_str().unwrap_or_default().trim().trim_matches('"');
            let volume = value["volume"].as_f64().unwrap_or(100.0).clamp(0.0, 100.0) / 100.0;
            if file.is_empty() {
                return Err(tr("overlays.media.no_sound").to_owned());
            }
            let path = groups::file_path(&config, group, file)
                .or_else(|| Some(std::path::PathBuf::from(file)).filter(|p| p.is_absolute() && p.is_file()))
                .ok_or_else(|| tr("overlays.media.gone").to_owned())?;
            crate::sound::play(&path, &format!("media:{group}"), "", volume as f32)?;
            Ok(json!({"accepted": true}))
        }
    }
}

/// Run one of the page's operations on a row.
pub fn act(engine: &Rc<Engine>, action: &str, id: &str) -> Result<Value, String> {
    let accepted = Ok(json!({"accepted": true}));
    // The Media tab's files.
    if action.starts_with("media_") {
        let (group, _) = row(engine, id)?;
        return super::overlay_media::act(engine, &group, action);
    }
    // The Test tab's sets, song and replay.
    if action.starts_with("set:") || action.starts_with("replay:") || action.starts_with("song_") {
        let (group, _) = row(engine, id)?;
        return match action {
            "song_play" => Ok(super::overlay_test::song(engine, &group, true)),
            "song_pause" => Ok(super::overlay_test::song(engine, &group, false)),
            a if a.starts_with("set:") => super::overlay_test::run_set(engine, &group, a),
            a => super::overlay_test::replay(engine, &group, a),
        };
    }
    match action {
        "enable_server" => {
            let address = overlays::base(&engine.config());
            engine.change(|config| config.http.auto_start = true);
            Ok(json!({"accepted":true, "message": trf("overlays.server_starting", &[("address", &address)])}))
        }
        "server_state" => Ok(json!({"values":{"title":tr("overlays.server_state"), "rows":[
            {"name":tr("overlays.header.address"),"value":overlays::base(&engine.config())}
        ]}})),
        "refresh" => {
            sync(engine);
            accepted
        }
        "guide" => {
            // The guide this build came with, from its own server; with the server off, the one online.
            let config = engine.config();
            if config.http.auto_start {
                crate::oauth::open_browser(&format!("{}/overlays/-/guide", overlays::base(&config)));
            } else {
                crate::oauth::open_browser("https://github.com/PedroZamecki/streamerz/blob/main/docs/OVERLAYS.md");
            }
            accepted
        }
        // The Live tab: the address of the controls' dock.
        "control_dock_copy" => {
            let (group, _) = row(engine, id)?;
            let config = engine.config();
            let dock = overlays::controls_url(&config, &group);
            Ok(json!({"copy": dock, "message": tr("overlays.copied")}))
        }
        "copy_url" => {
            let (group, file) = row(engine, id)?;
            Ok(json!({"copy": overlays::url(&engine.config(), &group, &file), "message": tr("overlays.copied")}))
        }
        "open" => {
            let (group, file) = row(engine, id)?;
            server_on(engine)?;
            crate::oauth::open_browser(&overlays::url(&engine.config(), &group, &file));
            accepted
        }
        "open_folder" => {
            let (group, _) = row(engine, id)?;
            let folder = groups::folder(&engine.config(), &group).ok_or_else(gone)?;
            crate::oauth::open_browser(&folder.display().to_string());
            accepted
        }
        "add_example" => add_example(engine, id).map(|group| {
            let saved = overlays::list(&engine.config())
                .into_iter()
                .find(|(g, _)| g == &group)
                .map(|(g, f)| row_id(&g, &f))
                .unwrap_or_default();
            json!({"accepted": true, "saved": saved})
        }),
        "test" => {
            let (group, _) = row(engine, id)?;
            if overlays::open_pages(&group) == 0 {
                return Err(tr("overlays.test.closed").into());
            }
            engine.emit("Test", &group, || sample(engine, &group));
            Ok(json!({"accepted": true, "message": tr("overlays.test.sent")}))
        }
        "inspect" => {
            let (group, _) = row(engine, id)?;
            Ok(json!({"values": inspector(engine, &group)}))
        }
        "add_to_obs" => {
            let (group, file) = row(engine, id)?;
            let config = engine.config();
            let page = overlays::page_of(&config, &group, &file);
            // A dock is added in OBS itself (it has no request for it): the steps, with the address copied.
            if page.dock {
                return Ok(json!({"copy": overlays::url(&config, &group, &file),
                "values": {"title": tr("overlays.dock.title"), "hint": tr("overlays.dock.hint"), "rows": [
                    {"name": tr("overlays.dock.name"), "value": overlays::page_name(&page)},
                    {"name": tr("overlays.dock.url"), "value": overlays::url(&config, &group, &file)},
                ]},
                "message": tr("overlays.copied")}));
            }
            server_on(engine)?;
            if config.obs.connections.is_empty() {
                return Err(tr("overlays.obs.none").into());
            }
            let engine = engine.clone();
            tokio::task::spawn_local(async move {
                match overlays::add_to_obs(&engine, &group, &file).await {
                    Ok(("updated", name)) => crate::say(trf("overlays.obs.updated", &[("name", &name)])),
                    Ok((_, name)) => crate::say(trf("overlays.obs.added", &[("name", &name)])),
                    Err(error) => super::prompts::notice(
                        tr("overlays.btn.add_to_obs").into(),
                        trf("overlays.obs.failed", &[("error", &error)]),
                    ),
                }
            });
            accepted
        }
        "delete" => {
            let (group, file) = row(engine, id)?;
            let mut result = Ok(());
            engine.change(|config| {
                let mut meta = groups::meta(config, &group);
                meta.files.retain(|f| f.name != file);
                meta.template.pages.retain(|p| p.file != file);
                if meta.template.picture == file {
                    meta.template.picture.clear();
                }
                // An overlay with no page left is nothing: it goes whole, unless a group of actions
                // of its name still uses its settings.
                let last = !meta.files.iter().any(|f| groups::is_page(&f.name));
                if last && !groups::listed(config, &group) {
                    groups::remove(config, &group);
                    result = Ok(());
                } else {
                    result = groups::set_meta(config, &group, meta);
                }
            });
            result.map(|()| json!({"accepted": true}))
        }
        "disconnect" => {
            let (group, _) = row(engine, id)?;
            if !groups::listed(&engine.config(), &group) {
                return Err(tr("overlays.connect.not_connected").into());
            }
            engine.change(|config| {
                groups::ungroup(config, &group);
            });
            Ok(json!({"accepted": true, "message": trf("overlays.disconnect.done", &[("name", &group)])}))
        }
        _ => Err(format!("no overlay operation {action}")),
    }
}

/// The inspector as a dialog: the overlay's open pages and what each listens to, the last messages
/// sent to them, and what the pages logged, newest first.
fn inspector(engine: &Rc<Engine>, group: &str) -> Value {
    let seen = overlays::inspect(engine, group);
    let clock = |at: &Value| crate::fmt::shown_clock((at.as_u64().unwrap_or(0) / 1000) as i64);
    let list = |v: &Value| {
        v.as_array()
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
            .collect::<Vec<_>>()
            .join(", ")
    };
    let mut rows = Vec::new();
    for page in seen["pages"].as_array().into_iter().flatten() {
        let listens = [list(&page["topics"]), list(&page["events"])]
            .into_iter()
            .filter(|s| !s.is_empty())
            .collect::<Vec<_>>()
            .join(", ");
        rows.push(
            json!({"name": trf("overlays.inspect.page", &[("file", page["file"].as_str().unwrap_or_default())]),
            "value": trf("overlays.inspect.listens", &[("since", &clock(&page["openedMs"])),
                ("list", if listens.is_empty() { tr("overlays.inspect.nothing") } else { &listens })])}),
        );
    }
    for line in seen["log"].as_array().into_iter().flatten().rev().take(12) {
        let name = if line["error"] == true {
            tr("overlays.inspect.error")
        } else {
            tr("overlays.inspect.said")
        };
        rows.push(json!({"name": format!("{} {name}", clock(&line["atMs"])), "value": line["text"]}));
    }
    for message in seen["sent"].as_array().into_iter().flatten().rev().take(15) {
        let event = message["event"].as_str().unwrap_or_default();
        // A Test or a settings change is named for what it is, not for the overlay it went to.
        let event = event.split_once('.').map_or(event, |(source, kind)| match source {
            "Test" | "Settings" | "Reload" => source,
            "Overlay" => kind,
            _ => event,
        });
        rows.push(json!({"name": format!("{} {event}", clock(&message["atMs"])), "value": message["about"]}));
    }
    let hint = if seen["pages"].as_array().is_some_and(|p| !p.is_empty()) {
        tr("overlays.inspect.hint")
    } else {
        tr("overlays.inspect.closed")
    };
    json!({"title": trf("overlays.inspect.title", &[("name", group)]), "hint": hint, "rows": rows})
}

/// What a group of actions holds, in words: "2 actions, 1 command".
pub fn holds((actions, commands, timers): (usize, usize, usize)) -> String {
    let part = |count: usize, one: &'static str, many: &'static str| match count {
        0 => None,
        1 => Some(tr(one).to_owned()),
        n => Some(trf(many, &[("count", &n.to_string())])),
    };
    let parts: Vec<String> = [
        part(actions, "overlays.holds.action", "overlays.holds.actions"),
        part(commands, "overlays.holds.command", "overlays.holds.commands"),
        part(timers, "overlays.holds.timer", "overlays.holds.timers"),
    ]
    .into_iter()
    .flatten()
    .collect();
    if parts.is_empty() {
        tr("overlays.holds.nothing").to_owned()
    } else {
        parts.join(", ")
    }
}

/// The groups of actions an overlay can be connected to: those with no overlay of their own.
fn free_groups(config: &crate::Config) -> Vec<String> {
    groups::names(config)
        .into_iter()
        .filter(|g| !groups::has_overlay(config, g))
        .collect()
}

/// A page is served by the HTTP server: say so instead of opening an address that answers nothing.
fn server_on(engine: &Rc<Engine>) -> Result<(), String> {
    if let Some(failure) = super::status::failure("http") {
        return Err(failure["text"].as_str().unwrap_or_default().to_owned());
    }
    if engine.config().http.auto_start {
        Ok(())
    } else {
        Err(tr("overlays.server_off").into())
    }
}

fn field(key: &str, label: &str, kind: &str, value: Value) -> Value {
    let holds = if kind == "bool" { "true/false" } else { "text" };
    json!({"key": key, "label": label, "widget": {"kind": kind}, "holds": holds, "value": value})
}

fn form(kind: String, title: String, hint: &str, fields: Vec<Value>) -> Value {
    let item: serde_json::Map<String, Value> = fields
        .iter()
        .map(|f| (f["key"].as_str().unwrap_or_default().to_owned(), f["value"].clone()))
        .collect();
    json!({"kind": kind, "title": title, "hint": hint, "new": item, "item": item, "fields": fields})
}

/// The form's values with what was changed in it: `changes` is `[{field: index, value}]`.
fn filled(form: &Value, request: &Value) -> Result<serde_json::Map<String, Value>, String> {
    let mut item = form["item"].as_object().cloned().unwrap_or_default();
    if let Some(text) = request["json"].as_str() {
        let typed: serde_json::Map<String, Value> = serde_json::from_str(text)
            .map_err(|error| trf("dialogs.not_valid_json", &[("error", &error.to_string())]))?;
        item.extend(typed.into_iter().filter(|(key, _)| form["item"].get(key).is_some()));
        return Ok(item);
    }
    for change in request["changes"].as_array().into_iter().flatten() {
        let key = change["field"]
            .as_u64()
            .and_then(|at| form["fields"][at as usize]["key"].as_str())
            .ok_or_else(|| tr("forms.invalid_field").to_owned())?;
        let (old, new) = (&form["item"][key], &change["value"]);
        if old.is_boolean() != new.is_boolean() || old.is_string() != new.is_string() {
            return Err(tr("forms.invalid_field").into());
        }
        item.insert(key.to_owned(), new.clone());
    }
    Ok(item)
}

/// The parts of a form's kind: the form's name, then the row it is for.
fn parts(request: &Value) -> (String, String) {
    let kind = request["kind"].as_str().unwrap_or_default();
    let rest = kind.strip_prefix(FORMS).unwrap_or_default();
    let (name, id) = rest.split_once(':').unwrap_or((rest, ""));
    (name.to_owned(), id.to_owned())
}

fn home() -> std::path::PathBuf {
    std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(std::path::PathBuf::from)
        .unwrap_or_default()
}

/// A stable destination value includes the scene disclosed before committing, rather than a list
/// position that could change when OBS changes scenes or another connection arrives.
fn obs_destinations(engine: &Rc<Engine>) -> Vec<(String, String)> {
    engine
        .config()
        .obs
        .connections
        .iter()
        .filter_map(|item| {
            let id = item.get("id")?.as_str()?;
            if !engine.obs_connected(Some(id)) {
                return None;
            }
            let scene = crate::obs::current_scene(id)?;
            let name = item.get("name").and_then(Value::as_str).unwrap_or(id);
            Some((format!("{}:{}", enc(id), enc(&scene)), format!("{name}: {scene}")))
        })
        .collect()
}

fn obs_setup_form(engine: &Rc<Engine>, name: &str, raw: &str) -> Result<Value, String> {
    let (id, snapshot) = raw.split_once('|').unwrap_or((raw, ""));
    if name == "example_obs" {
        crate::examples::package(id)?;
    } else {
        row(engine, id)?;
    }
    let destinations = obs_destinations(engine);
    if destinations.is_empty() {
        return Err(tr("overlays.obs.none").into());
    }
    let selected = if snapshot.is_empty() {
        destinations[0].0.clone()
    } else {
        dec(snapshot)
    };
    let kind = format!("overlay:{name}:{id}|{}", enc(&selected));
    let mut destination = field("destination", tr("overlays.obs.destination"), "text", selected.into());
    destination["choices"] = destinations
        .iter()
        .map(|(id, label)| json!({"id":id,"label":label}))
        .collect::<Vec<_>>()
        .into();
    destination["strict"] = true.into();
    let address = overlays::base(&engine.config());
    let hint = trf("overlays.obs.setup_effects", &[("address", &address)]);
    let mut out = form(kind, tr("overlays.btn.add_to_obs").into(), &hint, vec![destination]);
    out["okLabel"] = tr("overlays.btn.add_to_obs").into();
    out["json"] = false.into();
    Ok(out)
}

/// `StreamerzEntityForm` for this page's forms.
pub fn form_for(engine: &Rc<Engine>, request: &Value) -> Result<Value, String> {
    let (name, id) = parts(request);
    let kind = request["kind"].as_str().unwrap_or_default().to_owned();
    let config = engine.config();
    match name.as_str() {
        "example_obs" | "obs_setup" => obs_setup_form(engine, &name, &id),
        "rename" => {
            let (group, _) = row(engine, &id)?;
            let hint = if groups::listed(&config, &group) {
                tr("overlays.rename.hint_connected")
            } else {
                tr("overlays.rename.hint")
            };
            let mut out = form(
                kind,
                trf("overlays.rename.title", &[("name", &group)]),
                hint,
                vec![field("name", tr("dialogs.name"), "text", group.into())],
            );
            out["focusField"] = 0.into();
            out["okLabel"] = tr("btn.rename").into();
            out["json"] = false.into();
            Ok(out)
        }
        "settings" => {
            let (group, _) = row(engine, &id)?;
            let mut form = super::actions::group_settings_form(&config, &group);
            form["kind"] = kind.into();
            // Said of the overlay, and of its actions only when some are connected.
            form["title"] = trf("overlays.settings.title", &[("name", &group)]).into();
            form["hint"] = if groups::listed(&config, &group) {
                tr("overlays.settings.hint_connected")
            } else {
                tr("overlays.settings.hint")
            }
            .into();
            Ok(form)
        }
        "page" => {
            let (group, file) = row(engine, &id)?;
            let page = overlays::page_of(&config, &group, &file);
            let granted = groups::meta(&config, &group).permissions;
            let mut fields = vec![
                field("title", tr("overlays.page.title"), "text", page.title.clone().into()),
                field(
                    "width",
                    tr("overlays.page.width"),
                    "text",
                    page.width.to_string().into(),
                ),
                field(
                    "height",
                    tr("overlays.page.height"),
                    "text",
                    page.height.to_string().into(),
                ),
                field("dock", tr("overlays.page.dock"), "bool", page.dock.into()),
            ];
            // What the group's pages may ask Streamerz to do, each one a tick.
            for (permission, _) in overlays::PERMISSIONS {
                fields.push(field(
                    permission,
                    tr(permission_label(permission)),
                    "bool",
                    granted.iter().any(|g| g == permission).into(),
                ));
            }
            Ok(form(
                kind,
                trf("overlays.page.form_title", &[("name", &overlays::page_name(&page))]),
                tr("overlays.page.hint"),
                fields,
            ))
        }
        "export" => {
            let (group, _) = row(engine, &id)?;
            let meta = groups::meta(&config, &group);
            let t = meta.template;
            let file = home().join(format!("{group}.{}", package::EXTENSION));
            Ok(form(
                kind,
                trf("overlays.export.title", &[("name", &group)]),
                tr("overlays.export.hint"),
                vec![
                    field(
                        "file",
                        tr("overlays.export.file"),
                        "text",
                        file.display().to_string().into(),
                    ),
                    field(
                        "version",
                        tr("dialogs.export.version"),
                        "text",
                        if t.version.is_empty() {
                            "1.0.0".into()
                        } else {
                            t.version.into()
                        },
                    ),
                    field("author", tr("dialogs.export.author"), "text", t.author.into()),
                    field("link", tr("overlays.export.link"), "text", t.link.into()),
                    field("values", tr("overlays.export.values"), "bool", false.into()),
                ],
            ))
        }
        "connect" => {
            let (group, _) = row(engine, &id)?;
            let mut options = vec![trf("overlays.connect.new", &[("name", &group)])];
            options.extend(free_groups(&config));
            let mut pick = field("group", tr("overlays.connect.group"), "pick", 0.into());
            pick["widget"]["options"] = options.into();
            pick["holds"] = "a position in the list".into();
            Ok(form(
                kind,
                trf("overlays.connect.title", &[("name", &group)]),
                tr("overlays.connect.hint"),
                vec![pick],
            ))
        }
        "test" => Ok(super::overlay_test::form(kind)),
        "media_add" | "media_rename" | "media_replace" => {
            let (row_id, file) = id.split_once('|').unwrap_or((id.as_str(), ""));
            row(engine, row_id)?;
            let (title, label, value, browse) = match name.as_str() {
                "media_add" => (
                    tr("overlays.media.add").to_owned(),
                    tr("overlays.media.file"),
                    String::new(),
                    true,
                ),
                "media_replace" => (
                    trf("overlays.media.replace_title", &[("name", file)]),
                    tr("overlays.media.file"),
                    String::new(),
                    true,
                ),
                _ => (
                    trf("overlays.media.rename_title", &[("name", file)]),
                    tr("dialogs.name"),
                    file.to_owned(),
                    false,
                ),
            };
            let mut input = field("value", label, "text", value.into());
            if browse {
                input["browse"] = "file".into();
            }
            let mut out = form(
                kind,
                title,
                if browse {
                    tr("overlays.media.add_hint")
                } else {
                    tr("overlays.media.rename_hint")
                },
                vec![input],
            );
            out["json"] = false.into();
            Ok(out)
        }
        "topic" => Ok(form(
            kind,
            tr("overlays.test.topic").into(),
            tr("overlays.test.topic_hint"),
            vec![
                field("topic", tr("overlays.test.topic_name"), "text", "".into()),
                field("value", tr("overlays.test.topic_value"), "multi", "".into()),
            ],
        )),
        "import" => Ok(form(
            kind,
            tr("overlays.import.title").into(),
            tr("overlays.import.hint"),
            vec![field("file", tr("overlays.import.file"), "text", "".into())],
        )),
        "new" => Ok(form(
            kind,
            tr("overlays.new.title").into(),
            tr("overlays.new.hint"),
            vec![
                field("name", tr("dialogs.name"), "text", tr("overlays.new.name").into()),
                field("dock", tr("overlays.page.dock"), "bool", false.into()),
            ],
        )),
        other => Err(format!("no form {other}")),
    }
}

fn permission_label(permission: &str) -> &'static str {
    match permission {
        "chat.read" => "overlays.permission.chat_read",
        "alerts.read" => "overlays.permission.alerts_read",
        "chat.send" => "overlays.permission.chat_send",
        "actions.run" => "overlays.permission.actions_run",
        _ => "overlays.permission.values_write",
    }
}

/// A size typed in a form: whole pixels, empty or 0 for OBS's own.
fn pixels(item: &serde_json::Map<String, Value>, key: &str, label: &'static str) -> Result<u32, String> {
    let text = item[key].as_str().unwrap_or_default().trim();
    if text.is_empty() {
        return Ok(0);
    }
    text.parse::<u32>()
        .ok()
        .filter(|n| *n <= 16_384)
        .ok_or_else(|| trf("forms.invalid_value", &[("label", tr(label))]))
}

/// `StreamerzEditEntity` and `StreamerzEntityDraft` for this page's forms.
pub fn edit_form(engine: &Rc<Engine>, request: &Value, preview: bool) -> Result<Value, String> {
    let (name, id) = parts(request);
    if name == "settings" {
        let (group, _) = row(engine, &id)?;
        return super::actions::group_settings_edit(engine, &group, request, preview).map(|mut saved| {
            if saved["saved"].is_string() {
                saved["saved"] = id.clone().into();
                // What was saved is the form as it was: no draft is left.
                engine.overlay_draft_revert(&group, None);
            }
            saved
        });
    }
    if name == "test" {
        let (group, _) = row(engine, &id)?;
        return super::overlay_test::send(engine, &group, request);
    }
    if matches!(name.as_str(), "media_add" | "media_rename" | "media_replace") {
        let (row_id, file) = id.split_once('|').unwrap_or((id.as_str(), ""));
        let (group, _) = row(engine, row_id)?;
        let value = request["changes"][0]["value"].as_str().unwrap_or_default();
        if preview {
            return Ok(json!({"item": {"value": value}, "json": "{}"}));
        }
        return super::overlay_media::edit(engine, &group, &name, file, value);
    }
    let form = form_for(engine, request)?;
    let item = filled(&form, request)?;
    if name == "topic" && !preview {
        let topic = item["topic"].as_str().unwrap_or_default().trim().to_owned();
        if topic.is_empty() {
            return Err(trf("forms.invalid_value", &[("label", tr("overlays.test.topic_name"))]));
        }
        let text = item["value"].as_str().unwrap_or_default();
        let value = serde_json::from_str::<Value>(text).unwrap_or_else(|_| Value::String(text.to_owned()));
        engine.overlay_publish(&topic, value);
        return Ok(json!({"accepted": true, "message": tr("overlays.test.done")}));
    }
    if preview {
        let text = serde_json::to_string_pretty(&item).map_err(|error| error.to_string())?;
        return Ok(json!({"item": item, "json": text}));
    }
    let text = |key: &str| item[key].as_str().unwrap_or_default().trim().to_owned();
    match name.as_str() {
        "rename" => {
            let (group, file) = row(engine, &id)?;
            let wanted = text("name");
            if wanted.is_empty() {
                return Err(tr("lists.name_required").into());
            }
            if wanted == group {
                return Ok(json!({"saved": row_id(&group, &file)}));
            }
            let mut renamed = false;
            engine.change(|config| renamed = groups::rename(config, &group, &wanted));
            if renamed {
                Ok(json!({"saved": row_id(&wanted, &file)}))
            } else {
                Err(tr("overlays.rename_exists").into())
            }
        }
        "example_obs" | "obs_setup" => {
            let raw = id.split_once('|').map_or(id.as_str(), |(id, _)| id);
            let destination = text("destination");
            let (connection, scene) = destination
                .split_once(':')
                .ok_or_else(|| tr("forms.invalid_field").to_owned())?;
            let (connection, scene) = (dec(connection), dec(scene));
            if !engine.obs_connected(Some(&connection)) || scene.is_empty() {
                return Err(tr("overlays.obs.none").into());
            }
            let (group, file) = if name == "example_obs" {
                let group = add_example(engine, raw)?;
                let file = overlays::list(&engine.config())
                    .into_iter()
                    .find(|(g, f)| g == &group && !overlays::page_of(&engine.config(), g, f).dock)
                    .map(|(_, f)| f)
                    .ok_or_else(gone)?;
                (group, file)
            } else {
                row(engine, raw)?
            };
            let saved = row_id(&group, &file);
            let listening = crate::servers::wait_for_http(&engine.config().http);
            if !engine.config().http.auto_start {
                super::status::clear_failure("http");
            }
            engine.change(|config| config.http.auto_start = true);
            let engine = engine.clone();
            tokio::task::spawn_local(async move {
                // Binding the existing server acknowledges readiness. Source creation preserves
                // the selected scene even if the scene on air changes while OBS answers.
                let result = listening.await.unwrap_or_else(|_| Err(tr("conns.failure.bind").into()));
                let result = match result {
                    Ok(()) => overlays::add_to_obs_at(&engine, &group, &file, &connection, &scene).await,
                    Err(error) => Err(error),
                };
                match result {
                    Ok((what, name)) => crate::say(trf(
                        if what == "updated" {
                            "overlays.obs.updated"
                        } else {
                            "overlays.obs.added"
                        },
                        &[("name", &name)],
                    )),
                    Err(error) => super::prompts::notice(
                        tr("overlays.btn.add_to_obs").into(),
                        trf("overlays.obs.partial", &[("error", &error)]),
                    ),
                }
                crate::host::changed();
            });
            Ok(json!({"saved": saved}))
        }
        "connect" => {
            let (group, _) = row(engine, &id)?;
            // The first choice is a new group of the overlay's name; the rest are the groups there.
            let to = match item["group"].as_u64().unwrap_or(0) as usize {
                0 => String::new(),
                at => free_groups(&engine.config())
                    .get(at - 1)
                    .cloned()
                    .ok_or_else(|| tr("forms.invalid_field").to_owned())?,
            };
            let mut result = Ok(());
            engine.change(|config| result = groups::connect(config, &group, &to));
            result?;
            crate::say(trf("overlays.connect.done", &[("name", &group)]));
            Ok(json!({"saved": id}))
        }
        "page" => {
            let (group, file) = row(engine, &id)?;
            let page = Page {
                file: file.clone(),
                title: text("title"),
                width: pixels(&item, "width", "overlays.page.width")?,
                height: pixels(&item, "height", "overlays.page.height")?,
                dock: item["dock"] == true,
            };
            let permissions: Vec<String> = overlays::PERMISSIONS
                .iter()
                .filter(|(p, _)| item[*p] == true)
                .map(|(p, _)| p.to_string())
                .collect();
            let mut result = Ok(());
            engine.change(|config| {
                let mut meta = groups::meta(config, &group);
                meta.template.pages.retain(|p| p.file != file);
                meta.template.pages.push(page);
                meta.permissions = permissions;
                result = groups::set_meta(config, &group, meta);
            });
            result.map(|()| json!({"saved": id}))
        }
        "export" => {
            let (group, _) = row(engine, &id)?;
            if text("file").is_empty() {
                return Err(tr("overlays.export.no_file").into());
            }
            let mut result = Ok(());
            engine.change(|config| {
                let mut meta = groups::meta(config, &group);
                meta.template.version = text("version");
                meta.template.author = text("author");
                meta.template.link = text("link");
                // Every page gets listed, so whoever imports it gets its Add to OBS button.
                for f in &meta.files {
                    let listed = meta.template.pages.iter().any(|p| p.file == f.name);
                    if f.name.to_ascii_lowercase().ends_with(".html") && !listed {
                        meta.template.pages.push(Page {
                            file: f.name.clone(),
                            ..Default::default()
                        });
                    }
                }
                result = groups::set_meta(config, &group, meta).and_then(|()| package::ensure_id(config, &group));
            });
            result?;
            let config = engine.config();
            let file = package::pack(&config, &group, item["values"] == true)?;
            let mut path = std::path::PathBuf::from(text("file"));
            if path.extension().is_none_or(|x| x != package::EXTENSION) {
                path.as_mut_os_string().push(format!(".{}", package::EXTENSION));
            }
            std::fs::write(&path, &file).map_err(|error| {
                trf(
                    "dialogs.export.cant_save",
                    &[("file", &path.display().to_string()), ("error", &error.to_string())],
                )
            })?;
            let saved = trf("overlays.export.saved", &[("file", &path.display().to_string())]);
            let checks = package::checks(&config, &group);
            if checks.is_empty() {
                crate::say(saved);
            } else {
                super::prompts::notice(
                    trf("overlays.export.title", &[("name", &group)]),
                    format!("{saved}\n\n{}\n{}", tr("overlays.export.look_at"), checks.join("\n")),
                );
            }
            Ok(json!({"saved": id}))
        }
        "import" => {
            let source = text("file");
            if source.is_empty() {
                return Err(tr("overlays.import.no_file").into());
            }
            let engine = engine.clone();
            tokio::task::spawn_local(async move {
                if let Err(error) = import(&engine, &source).await {
                    super::prompts::notice(tr("overlays.import.title").into(), error);
                }
            });
            Ok(json!({"saved": ""}))
        }
        "new" => {
            let wanted = text("name");
            if wanted.is_empty() {
                return Err(tr("lists.name_required").into());
            }
            let dock = item["dock"] == true;
            let mut made = Err(String::new());
            engine.change(|config| made = new_overlay(config, &wanted, dock));
            let group = made?;
            Ok(json!({"saved": row_id(&group, "index.html")}))
        }
        other => Err(format!("no form {other}")),
    }
}

/// A new overlay: a group of its own with the starter page, one setting to show how they work, and
/// the page listed. The name gets a number when it is taken.
pub fn new_overlay(config: &mut crate::Config, wanted: &str, dock: bool) -> Result<String, String> {
    let group = (1..1000)
        .map(|n| {
            if n == 1 {
                wanted.to_owned()
            } else {
                format!("{wanted} {n}")
            }
        })
        .find(|name| !groups::exists(config, name))
        .ok_or("no name left for it")?;
    let meta = groups::Meta {
        variables: vec![groups::Def {
            name: "text".into(),
            label: tr("overlays.new.setting").into(),
            kind: groups::Kind::Text,
            default: "Hello".into(),
            ..Default::default()
        }],
        files: vec![
            groups::text_file("index.html", overlays::STARTER),
            groups::text_file(overlays::TYPES_FILE, overlays::TYPES),
        ],
        template: groups::Template {
            pages: vec![Page {
                file: "index.html".into(),
                title: group.clone(),
                dock,
                ..Default::default()
            }],
            ..Default::default()
        },
        ..Default::default()
    };
    groups::set_meta(config, &group, meta)?;
    let values = groups::values(config, &group);
    groups::set_values(config, &group, values)?;
    Ok(group)
}

/// What a template file holds, as the question asked before it is imported.
pub fn import_question(about: &Value) -> String {
    let text = |key: &str| about[key].as_str().unwrap_or_default();
    let count = |key: &str| about[key].as_u64().unwrap_or(0).to_string();
    let mut lines = vec![match (text("version").is_empty(), text("author").is_empty()) {
        (false, false) => trf(
            "overlays.import.named_by",
            &[
                ("name", text("name")),
                ("version", text("version")),
                ("author", text("author")),
            ],
        ),
        (false, true) => trf(
            "overlays.import.named_version",
            &[("name", text("name")), ("version", text("version"))],
        ),
        _ => text("name").to_owned(),
    }];
    if !text("about").is_empty() {
        lines.push(text("about").to_owned());
    }
    let files = about["files"].as_array().map_or(0, Vec::len).to_string();
    lines.push(String::new());
    lines.push(trf(
        "overlays.import.holds",
        &[
            ("actions", &count("actions")),
            ("commands", &count("commands")),
            ("timers", &count("timers")),
            ("files", &files),
            ("size", &super::pages::size(about["size"].as_u64().unwrap_or(0))),
        ],
    ));
    let group = about["installed"]["group"].as_str().unwrap_or_default();
    lines.push(match about["installed"]["state"].as_str().unwrap_or_default() {
        "update" => trf(
            "overlays.import.update",
            &[
                ("group", group),
                ("version", about["installed"]["version"].as_str().unwrap_or_default()),
            ],
        ),
        "same" => trf("overlays.import.same", &[("group", group)]),
        "replaces" => trf("overlays.import.replaces", &[("group", group)]),
        _ => tr("overlays.import.new").to_owned(),
    });
    let permissions: Vec<String> = about["permissions"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|p| {
            let label = tr(permission_label(p["name"].as_str().unwrap_or_default()));
            if p["new"] == true && about["installed"]["state"] == "update" {
                trf("overlays.import.permission_new", &[("permission", label)])
            } else {
                label.to_owned()
            }
        })
        .collect();
    if !permissions.is_empty() {
        lines.push(String::new());
        lines.push(tr("overlays.import.may").to_owned());
        lines.extend(permissions);
    }
    if about["files"].as_array().is_some_and(|f| !f.is_empty()) {
        lines.push(String::new());
        lines.push(tr("overlays.import.internet").to_owned());
    }
    let requires: Vec<&str> = about["requires"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .collect();
    if !requires.is_empty() {
        lines.push(trf("overlays.import.requires", &[("list", &requires.join(", "))]));
    }
    let programs: Vec<&str> = about["programs"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .collect();
    if !programs.is_empty() {
        lines.push(
            trf("actions.import_warn", &[("programs", &programs.join("\n"))])
                .trim_start()
                .to_owned(),
        );
    }
    lines.join("\n")
}

/// Read a template from a file here or an `https` address, ask, and install it on a yes.
async fn import(engine: &Rc<Engine>, source: &str) -> Result<(), String> {
    let bytes = if source.starts_with("https://") || source.starts_with("http://") {
        package::download(source).await?
    } else {
        std::fs::read(source).map_err(|error| {
            trf(
                "dialogs.import.cant_read",
                &[("file", source), ("error", &error.to_string())],
            )
        })?
    };
    let package = package::open(&bytes)?;
    let about = package::describe(&engine.config(), &package);
    let yes = crate::host::confirm(import_question(&about), tr("dialogs.cancel"), tr("btn.import"));
    if !yes.await.unwrap_or(false) {
        return Ok(());
    }
    let mut result = Err(String::new());
    engine.change(|config| result = package::install(config, &package));
    let (group, unsupported) = result?;
    if unsupported.is_empty() {
        crate::say(trf("overlays.import.done", &[("group", &group)]));
    } else {
        super::prompts::notice(
            tr("overlays.import.title").into(),
            trf("actions.imported_unsupported", &[("items", &unsupported.join(", "))]),
        );
    }
    Ok(())
}

/// An overlay as a request tells it: its pages with their addresses, what it may do, and the group of
/// actions connected to it.
fn described(config: &crate::Config, group: &str) -> Value {
    let meta = groups::meta(config, group);
    let pages: Vec<Value> = meta
        .files
        .iter()
        .filter(|f| groups::is_page(&f.name))
        .map(|f| {
            let page = overlays::page_of(config, group, &f.name);
            json!({"file": f.name, "title": overlays::page_name(&page), "url": overlays::url(config, group, &f.name),
                "dock": page.dock, "width": page.width, "height": page.height})
        })
        .collect();
    let (actions, commands, timers) = groups::counts(config, group);
    json!({"name": group, "about": meta.description, "pages": pages,
        "files": meta.files.iter().map(|f| json!({"name": f.name, "size": f.size})).collect::<Vec<_>>(),
        "permissions": meta.permissions, "settings": meta.variables.len(),
        "open": overlays::open_pages(group),
        "connected": groups::listed(config, group),
        "holds": {"actions": actions, "commands": commands, "timers": timers}})
}

/// The page a request means: the file it names, else the overlay's first page.
fn page_asked(config: &crate::Config, group: &str, file: &str) -> Result<String, String> {
    let meta = groups::meta(config, group);
    let pages = || meta.files.iter().filter(|f| groups::is_page(&f.name));
    let found = if file.is_empty() {
        pages().find(|f| f.name == "index.html").or_else(|| pages().next())
    } else {
        pages().find(|f| f.name == file)
    };
    found
        .map(|f| f.name.clone())
        .ok_or_else(|| format!("{group} has no page {file}").trim_end().to_owned())
}

/// Create an overlay or change one, in one request: only what is given changes.
fn save(config: &mut crate::Config, req: &Value) -> Result<Value, String> {
    use base64::Engine as _;
    let b64 = base64::engine::general_purpose::STANDARD;
    let name = req["overlay"].as_str().unwrap_or_default().trim().to_owned();
    if name.is_empty() {
        return Err("overlay: its name".into());
    }
    let created = !groups::exists(config, &name);
    let mut meta = groups::meta(config, &name);
    let mut fresh: Vec<String> = Vec::new();
    for (key, encoded) in [("files", false), ("filesBase64", true)] {
        let Some(files) = req.get(key).filter(|v| !v.is_null()) else {
            continue;
        };
        let files = files
            .as_object()
            .ok_or_else(|| format!("{key}: an object of file name to content"))?;
        for (file, content) in files {
            let content = content.as_str().ok_or_else(|| format!("{file}: its content as text"))?;
            let content = if encoded {
                b64.decode(content).map_err(|_| format!("{file}: not base64"))?;
                content.to_owned()
            } else {
                b64.encode(content)
            };
            // One that is there keeps its place in the list; a new one goes at the end.
            match meta.files.iter_mut().find(|f| f.name == *file) {
                Some(have) => have.content = content,
                None => {
                    fresh.push(file.clone());
                    meta.files.push(groups::File {
                        name: file.clone(),
                        content,
                        size: 0,
                    });
                }
            }
        }
    }
    for file in req["remove"].as_array().into_iter().flatten().filter_map(Value::as_str) {
        meta.files.retain(|f| f.name != file);
    }
    // Something new with no page of its own starts from the commented starter page.
    if created && !meta.files.iter().any(|f| groups::is_page(&f.name)) {
        meta.files.push(groups::text_file("index.html", overlays::STARTER));
        fresh.push("index.html".into());
    }
    // The types of `window.streamerz` come with anything new, for whoever edits it in an editor.
    if created && !meta.files.iter().any(|f| f.name == overlays::TYPES_FILE) {
        meta.files
            .push(groups::text_file(overlays::TYPES_FILE, overlays::TYPES));
    }
    let parse = |key: &str| -> Result<Option<Value>, String> { Ok(req.get(key).filter(|v| !v.is_null()).cloned()) };
    if let Some(settings) = parse("settings")? {
        meta.variables = serde_json::from_value(settings).map_err(|e| format!("settings: {e}"))?;
    }
    if let Some(permissions) = parse("permissions")? {
        meta.permissions = serde_json::from_value(permissions).map_err(|e| format!("permissions: {e}"))?;
    }
    if let Some(about) = req["description"].as_str() {
        meta.description = about.to_owned();
    }
    if let Some(requires) = parse("requires")? {
        meta.template.requires = serde_json::from_value(requires).map_err(|e| format!("requires: {e}"))?;
    }
    if let Some(test) = req.get("test") {
        meta.template.test = test.clone();
    }
    if let Some(pages) = parse("pages")? {
        meta.template.pages = serde_json::from_value(pages).map_err(|e| format!("pages: {e}"))?;
    } else {
        // Every new page is listed, so it has a name and an Add to OBS of its own.
        for file in fresh.iter().filter(|f| groups::is_page(f)) {
            if !meta.template.pages.iter().any(|p| p.file == *file) {
                let stem = file.rsplit('/').next().unwrap_or(file).trim_end_matches(".html");
                meta.template.pages.push(Page {
                    file: file.clone(),
                    title: if stem == "index" {
                        name.clone()
                    } else {
                        format!("{name}: {stem}")
                    },
                    dock: req["dock"] == true,
                    ..Default::default()
                });
            }
        }
    }
    let have = |file: &str| meta.files.iter().any(|f| f.name == file);
    let gone: Vec<String> = meta
        .template
        .pages
        .iter()
        .filter(|p| !have(&p.file))
        .map(|p| p.file.clone())
        .collect();
    meta.template.pages.retain(|p| !gone.contains(&p.file));
    if !meta.template.picture.is_empty() && !have(&meta.template.picture) {
        meta.template.picture.clear();
    }
    groups::set_meta(config, &name, meta)?;
    // New settings start at their defaults, the ones set stay, and the given ones are set.
    let mut values = groups::values(config, &name);
    for (key, value) in req["values"].as_object().into_iter().flatten() {
        values.insert(key.clone(), value.clone());
    }
    groups::set_values(config, &name, values)?;
    match &req["connect"] {
        Value::Bool(true) => groups::connect(config, &name, "")?,
        Value::String(group) => groups::connect(config, &name, group)?,
        _ => {}
    }
    let mut out = described(config, &name);
    out["created"] = created.into();
    out["checks"] = package::checks(config, &name).into();
    Ok(out)
}

/// `StreamerzOverlay`: everything the Overlays page does and what goes into making an overlay, with
/// no window: an assistant (MCP) or any program on the WebSocket builds, tries and installs one.
///
/// | `op` | With | Gives |
/// | --- | --- | --- |
/// | `list` | | every overlay: its pages and addresses, permissions, open pages, connected actions |
/// | `read` | `overlay`, `file?` | its settings (declared and set), files and pages; with `file`, that file's `text` (or `content`, base64) |
/// | `save` | `overlay`, `files?` `{name: text}`, `filesBase64?`, `remove?` `[name]`, `settings?`, `values?`, `permissions?`, `pages?`, `description?`, `requires?`, `test?`, `dock?`, `connect?` | makes it or changes what is given; `checks` is what to look at |
/// | `rename` | `overlay`, `name` | renames the overlay and its connected group of actions |
/// | `delete` | `overlay`, `file?` | the file, or the whole overlay (its connected actions stay in Actions) |
/// | `test` | `overlay`, `rounds?`, or `events?` `[[name, data]]` and `topics?` `{name: value}` | sends the made-up chat, events, gift and song (or exactly what is given) to its open pages |
/// | `inspect` | `overlay` | its open pages and what each listens to, the last messages sent, what the pages logged, their errors |
/// | `obs` | `overlay`, `file?` | adds the page to OBS's scene on air (a dock: the steps) |
/// | `connect` | `overlay`, `group?` | connects it to a group of actions (none: a new one of its name) |
/// | `disconnect` | `overlay` | its group leaves Actions; the overlay stays |
/// | `examples` | | the bundled examples and whether each is here |
/// | `add_example` | `slug` | adds or updates it |
/// | `shot` | `overlay`, `file?`, `path?`, `width?`, `height?`, `waitMs?`, `test?`, `background?` | a PNG of the page as a browser draws it, with what its console said |
pub async fn request(engine: &Rc<Engine>, req: &Value) -> Result<Value, String> {
    let text = |key: &str| req[key].as_str().unwrap_or_default().trim().to_owned();
    let overlay = text("overlay");
    let known = || {
        if !overlay.is_empty() && groups::has_overlay(&engine.config(), &overlay) {
            Ok(())
        } else {
            Err(format!("no overlay named {overlay}"))
        }
    };
    // What was put in a folder by hand counts before anything is read or changed.
    sync(engine);
    match text("op").as_str() {
        // The editor's unsaved settings: `action` get (the default), set (with `values`), save or
        // revert (one `setting`, or all). Only preview pages show them; save sends them to every page.
        "draft" => {
            known()?;
            match text("action").as_str() {
                "" | "get" => {}
                "set" => {
                    let values = req["values"].as_object().ok_or("values must be an object")?;
                    engine.overlay_draft_set(&overlay, values)?;
                }
                "save" => {
                    engine.overlay_draft_save(&overlay)?;
                }
                "revert" => {
                    let setting = text("setting");
                    engine.overlay_draft_revert(&overlay, (!setting.is_empty()).then_some(setting.as_str()));
                }
                other => return Err(format!("unknown draft action {other}: get, set, save or revert")),
            }
            let config = engine.config();
            let first = groups::meta(&config, &overlay)
                .files
                .iter()
                .find(|f| groups::is_page(&f.name))
                .map(|f| f.name.clone())
                .unwrap_or_default();
            Ok(json!({"draft": engine.overlay_draft(&overlay), "settings": engine.overlay_preview_settings(&overlay),
                "preview": overlays::preview_url(&config, &overlay, &first)}))
        }
        // The live controls: their values now, or one set (`control` and `value`; a button is pressed).
        "control" => {
            known()?;
            let control = text("control");
            if !control.is_empty() {
                let value = req.get("value").cloned().unwrap_or(Value::Null);
                engine.overlay_control_set(&overlay, &control, &value, "mcp", false)?;
            }
            let config = engine.config();
            let controls: Vec<Value> = groups::controls(&config, &overlay)
                .into_iter()
                .map(|(c, v)| {
                    let mut def = serde_json::to_value(&c).unwrap_or_default();
                    def["value"] = v;
                    def
                })
                .collect();
            Ok(json!({"controls": controls,
                "dock": overlays::controls_url(&config, &overlay)}))
        }
        "list" => {
            let config = engine.config();
            let all: Vec<Value> = groups::all(&config)
                .iter()
                .filter(|g| groups::has_overlay(&config, g))
                .map(|g| described(&config, g))
                .collect();
            let topics: serde_json::Map<String, Value> = engine
                .overlay_topics()
                .into_iter()
                .filter_map(|t| Some((t.clone(), engine.overlay_value(&t)?)))
                .collect();
            Ok(json!({"overlays": all, "topics": topics, "base": overlays::base(&config),
                "serverOn": config.http.auto_start}))
        }
        "read" => {
            known()?;
            let config = engine.config();
            let meta = groups::meta(&config, &overlay);
            let mut out = described(&config, &overlay);
            out["settings"] = serde_json::to_value(&meta.variables).map_err(|e| e.to_string())?;
            out["values"] = Value::Object(groups::values(&config, &overlay).into_iter().collect());
            out["pageList"] = serde_json::to_value(&meta.template.pages).map_err(|e| e.to_string())?;
            out["test"] = meta.template.test;
            out["folder"] = groups::folder(&config, &overlay)
                .map(|f| f.display().to_string())
                .into();
            let file = text("file");
            if !file.is_empty() {
                let bytes =
                    groups::read_file(&config, &overlay, &file).ok_or_else(|| format!("{overlay} has no file {file}"))?;
                match String::from_utf8(bytes) {
                    Ok(text) => out["text"] = text.into(),
                    Err(not) => {
                        use base64::Engine as _;
                        out["content"] = base64::engine::general_purpose::STANDARD
                            .encode(not.into_bytes())
                            .into();
                    }
                }
            }
            Ok(out)
        }
        "save" => {
            let mut result = Err(String::new());
            engine.change(|config| result = save(config, req));
            result
        }
        "rename" => {
            known()?;
            let wanted = text("name");
            if wanted.is_empty() {
                return Err(tr("lists.name_required").into());
            }
            if wanted != overlay {
                let mut renamed = false;
                engine.change(|config| renamed = groups::rename(config, &overlay, &wanted));
                if !renamed {
                    return Err(tr("overlays.rename_exists").into());
                }
            }
            Ok(described(&engine.config(), &wanted))
        }
        "delete" => {
            known()?;
            let file = text("file");
            let mut result = Ok(());
            let mut kept = false;
            engine.change(|config| {
                let mut meta = groups::meta(config, &overlay);
                if file.is_empty() {
                    meta.files.clear();
                } else if meta.files.iter().any(|f| f.name == file) {
                    meta.files.retain(|f| f.name != file);
                } else {
                    result = Err(format!("{overlay} has no file {file}"));
                    return;
                }
                let have = |name: &str| meta.files.iter().any(|f| f.name == name);
                let pages: Vec<Page> = meta.template.pages.iter().filter(|p| have(&p.file)).cloned().collect();
                meta.template.pages = pages;
                if !have(&meta.template.picture) {
                    meta.template.picture.clear();
                }
                // With no page left it is no overlay: it goes whole, unless actions still use its settings.
                let last = !meta.files.iter().any(|f| groups::is_page(&f.name));
                if last && !groups::listed(config, &overlay) {
                    groups::remove(config, &overlay);
                } else {
                    kept = last;
                    result = groups::set_meta(config, &overlay, meta);
                }
            });
            result.map(|()| json!({"groupKept": kept}))
        }
        "test" => {
            known()?;
            let pages = overlays::open_pages(&overlay);
            if pages == 0 {
                return Err(tr("overlays.test.closed").into());
            }
            let own = req.get("events").is_some() || req.get("topics").is_some();
            let mut all = Vec::new();
            if own {
                let made = json!({"events": req["events"].as_array().cloned().unwrap_or_default(),
                    "topics": req["topics"].as_object().cloned().unwrap_or_default()});
                engine.emit("Test", &overlay, || made.clone());
                all.push(made);
            } else {
                for _ in 0..req["rounds"].as_u64().unwrap_or(1).clamp(1, 20) {
                    let made = sample(engine, &overlay);
                    engine.emit("Test", &overlay, || made.clone());
                    all.push(made);
                }
            }
            Ok(json!({"pages": pages, "sent": all}))
        }
        "inspect" => {
            known()?;
            let config = engine.config();
            let mut out = overlays::inspect(engine, &overlay);
            out["overlay"] = described(&config, &overlay);
            out["values"] = Value::Object(groups::values(&config, &overlay).into_iter().collect());
            Ok(out)
        }
        "obs" => {
            known()?;
            let config = engine.config();
            let file = page_asked(&config, &overlay, &text("file"))?;
            let page = overlays::page_of(&config, &overlay, &file);
            let url = overlays::url(&config, &overlay, &file);
            if page.dock {
                return Ok(json!({"dock": true, "name": overlays::page_name(&page), "url": url,
                    "steps": tr("overlays.dock.hint")}));
            }
            server_on(engine)?;
            if config.obs.connections.is_empty() {
                return Err(tr("overlays.obs.none").into());
            }
            drop(config);
            let (what, name) = overlays::add_to_obs(engine, &overlay, &file).await?;
            Ok(json!({"source": name, "state": what, "url": url}))
        }
        "connect" => {
            known()?;
            let group = text("group");
            let mut result = Ok(());
            engine.change(|config| result = groups::connect(config, &overlay, &group));
            result.map(|()| described(&engine.config(), &overlay))
        }
        "disconnect" => {
            known()?;
            if !groups::listed(&engine.config(), &overlay) {
                return Err(tr("overlays.connect.not_connected").into());
            }
            engine.change(|config| {
                groups::ungroup(config, &overlay);
            });
            Ok(described(&engine.config(), &overlay))
        }
        "examples" => {
            let config = engine.config();
            let mut rows = Vec::new();
            for example in crate::examples::ALL {
                let about = package::describe(&config, &crate::examples::package(example.slug)?);
                rows.push(json!({"slug": example.slug, "name": about["name"], "about": about["about"],
                    "kind": about["kind"], "permissions": about["permissions"], "pages": about["pages"],
                    "installed": about["installed"]}));
            }
            Ok(json!({"examples": rows}))
        }
        "add_example" => {
            let group = add_example(engine, &text("slug"))?;
            Ok(described(&engine.config(), &group))
        }
        "shot" => {
            known()?;
            server_on(engine)?;
            let config = engine.config();
            let file = page_asked(&config, &overlay, &text("file"))?;
            let page = overlays::page_of(&config, &overlay, &file);
            let size = |key: &str, own: u32, usual: u32| {
                req[key]
                    .as_u64()
                    .map(|n| n.clamp(16, 8192) as u32)
                    .unwrap_or(if own == 0 { usual } else { own })
            };
            let ask = crate::overlay_shot::Ask {
                url: overlays::url(&config, &overlay, &file),
                width: size("width", page.width, 1280),
                height: size("height", page.height, 720),
                wait_ms: req["waitMs"].as_u64().unwrap_or(1500).min(60_000),
                background: text("background"),
                path: text("path"),
                script_lines: overlays::script_lines(),
            };
            drop(config);
            let rounds = req["test"].as_u64().unwrap_or(0).min(20);
            let (engine2, group) = (engine.clone(), overlay.clone());
            crate::overlay_shot::take(ask, move || {
                // The page is open and listening by now: the made-up events go to it before the picture.
                for _ in 0..rounds {
                    let made = sample(&engine2, &group);
                    engine2.emit("Test", &group, || made.clone());
                }
            })
            .await
        }
        other => Err(format!("no overlay op {other}: list, read, save, rename, delete, test, inspect, obs, connect, disconnect, examples, add_example, shot")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::tests::{engine, run};

    #[test]
    fn overlay_setup_is_explicit_and_destinations_do_not_follow_later_scene_changes() {
        let _g = crate::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        run(async {
            let e = engine(json!({"actions":[]}));
            e.change(|config| {
                config.http.auto_start = false;
                config.http.address = "127.0.0.1".into();
                config.http.port = 18747;
            });
            let before = e.config().http.clone();
            let model = read(&e, DOMAIN).unwrap();
            assert!(model["tables"][0]["actions"]
                .as_array()
                .unwrap()
                .iter()
                .any(|a| a["id"] == "enable_server"));
            assert!(!e.config().http.auto_start);
            assert!(form_for(&e, &json!({"kind":"overlay:example_obs:clock"})).is_err());
            assert!(
                groups::all(&e.config()).is_empty(),
                "missing OBS must not install an example"
            );
            act(&e, "enable_server", "").unwrap();
            assert!(e.config().http.auto_start);
            assert_eq!(e.config().http.address, before.address);
            assert_eq!(e.config().http.port, before.port);
            let fake = Rc::new(crate::ports::fakes::FakeObs::default());
            fake.connected.set(true);
            e.set_obs(fake.clone());
            let group = add_example(&e, "clock").unwrap();
            let file = overlays::list(&e.config())
                .into_iter()
                .find(|(g, _)| g == &group)
                .unwrap()
                .1;
            fake.answer("GetInputList", json!({"inputs":[]}));
            fake.answer("CreateInput", json!({}));
            overlays::add_to_obs_at(&e, &group, &file, "chosen", "Disclosed scene")
                .await
                .unwrap();
            assert_eq!(fake.requests("CreateInput")[0]["sceneName"], "Disclosed scene");
            assert!(fake.requests("GetCurrentProgramScene").is_empty());
            fake.connected.set(false);
            assert!(overlays::add_to_obs_at(&e, &group, &file, "chosen", "Disclosed scene")
                .await
                .is_err());
            assert_eq!(
                fake.requests("CreateInput").len(),
                1,
                "never fall back after a disconnect"
            );
        });
    }

    #[test]
    fn a_moment_typed_from_now_comes_back_as_its_day_and_buttons_press() {
        let _g = crate::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        run(async {
            let e = engine(json!({"actions": []}));
            let example = crate::examples::package("countdown-to").unwrap();
            let mut group = String::new();
            e.change(|c| group = crate::package::install(c, &example).unwrap().0);
            let id = format!("{group}:index.html");
            // Field 1 is Counts down to, a datetime.
            let drafted = preview(
                &e,
                &json!({"op": "draft", "itemId": id, "changes": [{"field": 1, "value": "+15m"}]}),
            )
            .unwrap();
            assert_eq!(drafted["draft"], 1);
            let text = drafted["shown"]["1"].as_str().unwrap().to_owned();
            assert_eq!(text.len(), "2030-01-02 20:15:09".len(), "{text}");
            // The day and time sent back reads as the same moment: no change, nothing to send back.
            let again = preview(
                &e,
                &json!({"op": "draft", "itemId": id, "changes": [{"field": 1, "value": text}]}),
            )
            .unwrap();
            assert_eq!(again["shown"], json!({}));
            assert_eq!(e.overlay_draft(&group)["target"], groups::kinds::moment(&text).unwrap());
            // A half typed day is left out of the draft rather than refused.
            let half = preview(
                &e,
                &json!({"op": "draft", "itemId": id, "changes": [{"field": 1, "value": "2030-0"}]}),
            )
            .unwrap();
            assert_eq!(half["draft"], 1);

            // The button setting publishes its topic; Play of a sound needs a file.
            let pressed = press(&e, &json!({"itemId": group, "action": "press:celebrate"})).unwrap();
            assert!(pressed["message"].as_str().unwrap().contains("Try the end"));
            assert!(e.overlay_topics().contains(&format!("{group}.celebrate")));
            assert!(press(
                &e,
                &json!({"itemId": group, "action": "sound:sound", "value": {"file": "", "volume": 50}})
            )
            .is_err());
            assert!(press(&e, &json!({"itemId": group, "action": "press:nothing"})).is_err());
            assert!(is_press("play:enter") && !is_press("media_play:x"));
        });
    }

    #[test]
    fn live_controls_are_set_from_the_tab_a_dock_a_page_and_mcp() {
        let _g = crate::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        run(async {
            let e = engine(json!({"actions": []}));
            let example = crate::examples::package("countdown-to").unwrap();
            let mut group = String::new();
            e.change(|c| group = crate::package::install(c, &example).unwrap().0);
            let id = format!("{group}:index.html");
            // The Live tab: a form of the controls, set as it changes, and the dock's address.
            let live = super::super::view::read_request(
                &e,
                &json!({"domain": DOMAIN, "view": {"overlay": id, "overlayTab": "live"}}),
            )
            .unwrap();
            let sections = &live["overlay"]["sections"];
            let fields = sections[0]["form"]["fields"].as_array().unwrap();
            assert_eq!(fields.len(), 3);
            assert_eq!(fields[0]["widget"]["kind"], "bool");
            assert_eq!(fields[1]["widget"]["kind"], "slider");
            assert_eq!(fields[2]["press"]["request"]["action"], "control:addMinute");
            assert!(sections[1]["rows"][0]["detail"]
                .as_str()
                .unwrap()
                .ends_with("/Countdown%20to/-/controls"));
            preview(&e, &json!({"op": "controls", "itemId": id, "changes": [{"field": 0, "value": false}, {"field": 1, "value": 120.0}]})).unwrap();
            let c = e.config();
            assert_eq!(groups::control_values(&c, &group), json!({"show": false, "size": 120}));
            // Kept apart from the settings: sharing never carries them.
            assert!(!groups::values(&c, &group).contains_key("show"));
            drop(c);
            // A press, the WebSocket op (as MCP), and a page's call.
            press(&e, &json!({"itemId": group, "action": "control:addMinute"})).unwrap();
            let read = request(
                &e,
                &json!({"op": "control", "overlay": group, "control": "size", "value": "77"}),
            )
            .await
            .unwrap();
            assert_eq!(read["controls"][1]["value"], 77);
            assert!(request(
                &e,
                &json!({"op": "control", "overlay": group, "control": "size", "value": "huge"})
            )
            .await
            .is_err());
            let token = overlays::token(&e.config(), &group);
            let body =
                json!({"op": "control", "group": group, "token": token, "name": "show", "value": true, "from": "dock"});
            let (status, answer) = overlays::call(&e, body.to_string().as_bytes());
            assert_eq!((status, &answer["value"]), (200, &json!(true)), "{answer}");
            // A page gets them as they are; the dock is a page of its own, made from them.
            let c = e.config();
            let served = match overlays::file_with(&c, &format!("/overlays/{group}/index.html"), &Default::default()) {
                overlays::Served::Bytes(_, _, b) => String::from_utf8(b).unwrap(),
                _ => panic!("not served"),
            };
            assert!(
                served.contains(r#""controls":{"show":true,"size":77}"#),
                "the page has its controls"
            );
            let dock = overlays::controls_dock(&c, &group);
            assert!(
                dock.contains("\"name\":\"addMinute\"") && dock.contains("streamerz.control.set"),
                "{}",
                &dock[..200]
            );
        });
    }

    #[test]
    fn an_overlay_opens_its_own_page_with_tabs_and_its_forms() {
        let _g = crate::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        run(async {
            let e = engine(json!({"actions": []}));
            let made = edit_form(&e, &json!({"kind": "overlay:new", "changes": [{"field": 0, "value": "Clock"}, {"field": 1, "value": false}]}), false).unwrap();
            let id = made["saved"].as_str().unwrap().to_owned();
            let table = &read(&e, DOMAIN).unwrap()["tables"][0];
            assert_eq!(table["open"], "open_page", "a double click opens the overlay's page");
            assert!(table["actions"]
                .as_array()
                .unwrap()
                .iter()
                .any(|a| a["id"] == "open_page" && a["view"] == "overlay"));

            let request = json!({"domain": DOMAIN, "view": {"overlay": id}});
            let model = super::super::view::read_request(&e, &request).unwrap();
            let page = &model["overlay"];
            assert_eq!(model["domain"], DOMAIN);
            assert_eq!(page["title"], "Clock");
            assert_eq!(page["tab"], "settings");
            let tabs: Vec<_> = page["tabs"]["options"]
                .as_array()
                .unwrap()
                .iter()
                .map(|t| t["id"].clone())
                .collect();
            assert_eq!(
                tabs,
                ["settings", "test", "media", "live", "code"],
                "no Alerts tab but for an alert box"
            );
            assert!(page["sections"][0]["form"]["kind"]
                .as_str()
                .unwrap()
                .starts_with("overlay:settings:"));
            assert!(page["toolbar"]
                .as_array()
                .unwrap()
                .iter()
                .any(|b| b["id"] == "copy_url"));
            assert_eq!(page["toolbar"][0]["form"], format!("overlay:obs_setup:{id}"));

            let code = super::super::view::read_request(
                &e,
                &json!({"domain": DOMAIN, "view": {"overlay": id, "overlayTab": "code"}}),
            )
            .unwrap();
            assert_eq!(
                code["overlay"]["sections"][0]["form"]["kind"],
                format!("overlay:page:{id}")
            );
            let again =
                super::super::view::read_request(&e, &json!({"domain": DOMAIN, "view": {"overlay": id}})).unwrap();
            assert_eq!(again["overlay"]["tab"], "code", "the tab shown last comes back");
            // A change in the form is a draft: the page says so and the form shows it, until Revert or Save.
            let settings = super::super::view::read_request(
                &e,
                &json!({"domain": DOMAIN, "view": {"overlay": id, "overlayTab": "settings"}}),
            )
            .unwrap();
            let field = &settings["overlay"]["sections"][0]["form"]["fields"][1];
            let (key, kind) = (
                field["key"].as_str().unwrap().to_owned(),
                field["widget"]["kind"].as_str().unwrap().to_owned(),
            );
            let changed = match kind.as_str() {
                "bool" => json!(!field["value"].as_bool().unwrap()),
                "color" => json!("#123456"),
                _ => json!("Draft value"),
            };
            let drafted = preview(
                &e,
                &json!({"op": "draft", "itemId": id, "changes": [{"field": 1, "value": changed}]}),
            )
            .unwrap();
            assert_eq!(drafted["draft"], 1, "{key} ({kind}) is a draft");
            let shown = super::super::view::read_request(
                &e,
                &json!({"domain": DOMAIN, "view": {"overlay": id, "overlayTab": "settings"}}),
            )
            .unwrap();
            assert_eq!(shown["overlay"]["draft"], 1);
            assert_eq!(
                shown["overlay"]["sections"][0]["form"]["fields"][1]["value"], changed,
                "the form shows the draft"
            );
            preview(&e, &json!({"op": "revert", "itemId": id})).unwrap();
            assert!(e.overlay_draft("Clock").is_empty());
            preview(
                &e,
                &json!({"op": "draft", "itemId": id, "changes": [{"field": 1, "value": changed}]}),
            )
            .unwrap();
            let saved = preview(&e, &json!({"op": "save", "itemId": id})).unwrap();
            assert_eq!(saved["saved"], 1);
            assert!(e.overlay_draft("Clock").is_empty());
            assert_ne!(groups::values(&e.config(), "Clock")[&key], Value::Null);

            // The Test tab: its form sends one event; a form it can't show says so.
            let test = super::super::view::read_request(
                &e,
                &json!({"domain": DOMAIN, "view": {"overlay": id, "overlayTab": "test"}}),
            )
            .unwrap();
            let sections = test["overlay"]["sections"].as_array().unwrap();
            assert_eq!(sections[0]["form"]["kind"], format!("overlay:test:{id}"));
            assert!(sections[1]["buttons"]
                .as_array()
                .unwrap()
                .iter()
                .any(|b| b["id"] == "set:raid"));
            assert_eq!(sections[2]["buttons"][2]["form"], format!("overlay:topic:{id}"));
            let sent = edit_form(
                &e,
                &json!({"kind": format!("overlay:test:{id}"), "changes": [{"field": 3, "value": "1"}]}),
                false,
            )
            .unwrap();
            assert_eq!(sent["accepted"], true);
            let any = json!({"kind": format!("overlay:test:{id}"), "changes": [{"field": 0, "value": super::super::overlay_test::EVENTS.len()}, {"field": 1, "value": "706"}]});
            assert!(
                edit_form(&e, &any, false).is_err(),
                "Streamer.bot Started is no alert a page shows"
            );
            let topic = json!({"kind": format!("overlay:topic:{id}"), "changes": [{"field": 0, "value": "score"}, {"field": 1, "value": "{\"a\": 1}"}]});
            assert_eq!(edit_form(&e, &topic, false).unwrap()["accepted"], true);

            // The Media tab: a file brought in, renamed (a setting that named it follows) and deleted.
            let picture = std::env::temp_dir().join(format!("streamerz-media-{}.png", std::process::id()));
            std::fs::write(&picture, b"not really a png").unwrap();
            let add = json!({"kind": format!("overlay:media_add:{id}"), "changes": [{"field": 0, "value": picture.display().to_string()}]});
            edit_form(&e, &add, false).unwrap();
            let name = picture.file_name().unwrap().to_str().unwrap().to_owned();
            let media = super::super::view::read_request(
                &e,
                &json!({"domain": DOMAIN, "view": {"overlay": id, "overlayTab": "media"}}),
            )
            .unwrap();
            let rows = media["overlay"]["sections"][0]["rows"].as_array().unwrap().clone();
            assert!(
                rows.iter()
                    .any(|r| r["text"] == name.as_str() && r["picture"].is_string()),
                "{rows:?}"
            );
            let rename = json!({"kind": format!("overlay:media_rename:{id}|{name}"), "changes": [{"field": 0, "value": "img/logo.png"}]});
            edit_form(&e, &rename, false).unwrap();
            let files: Vec<String> = groups::meta(&e.config(), "Clock")
                .files
                .iter()
                .map(|f| f.name.clone())
                .collect();
            assert!(
                files.contains(&"img/logo.png".to_owned()) && !files.contains(&name),
                "{files:?}"
            );
            act(&e, "media_delete:img/logo.png", &id).unwrap();
            assert!(!groups::meta(&e.config(), "Clock")
                .files
                .iter()
                .any(|f| f.name == "img/logo.png"));
            let _ = std::fs::remove_file(&picture);

            // MCP asks by the group's name.
            let by_group =
                super::super::view::read_request(&e, &json!({"domain": "overlay:Clock", "tab": "media"})).unwrap();
            assert_eq!(by_group["overlay"]["id"], id);
            assert!(by_group["overlay"]["sections"][0]["rows"].as_array().is_some());
            assert!(super::super::view::read_request(&e, &json!({"domain": "overlay:Nope"})).is_err());
        });
    }

    #[test]
    fn the_page_lists_every_overlay_and_works_without_a_frontend() {
        let _g = crate::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        run(async {
            let e = engine(json!({"actions": []}));
            assert_eq!(read(&e, DOMAIN).unwrap()["tables"][0]["rows"], json!([]));
            // New: a group with the starter page, listed, with a setting.
            let form = form_for(&e, &json!({"kind": "overlay:new"})).unwrap();
            assert_eq!(form["fields"][0]["key"], "name");
            let made = edit_form(&e, &json!({"kind": "overlay:new", "changes": [{"field": 0, "value": "Clock"}, {"field": 1, "value": false}]}), false).unwrap();
            let id = made["saved"].as_str().unwrap().to_owned();
            assert_eq!(id, "Clock:index.html");
            let again = edit_form(
                &e,
                &json!({"kind": "overlay:new", "changes": [{"field": 0, "value": "Clock"}]}),
                false,
            )
            .unwrap();
            assert_eq!(again["saved"], "Clock 2:index.html", "a taken name gets a number");
            assert!(edit_form(
                &e,
                &json!({"kind": "overlay:new", "changes": [{"field": 0, "value": " "}]}),
                false
            )
            .is_err());
            let model = read(&e, DOMAIN).unwrap();
            let rows = model["tables"][0]["rows"].as_array().unwrap();
            assert_eq!(rows.len(), 2);
            assert_eq!(rows[0]["cells"][0], "Clock");
            assert!(rows[0]["cells"][5]
                .as_str()
                .unwrap()
                .ends_with("/overlays/Clock/index.html"));
            // An overlay makes no group of actions, and says it has none connected.
            assert!(groups::names(&e.config()).is_empty());
            assert_eq!(rows[0]["cells"][3], "");
            assert_eq!(
                rows[0]["can"],
                json!({"connect": true, "disconnect": false, "obs_setup": true, "add_to_obs":false})
            );
            let page = String::from_utf8(groups::read_file(&e.config(), "Clock", "index.html").unwrap()).unwrap();
            assert!(page.contains("streamerz.onSettings"));
            // Copy URL and the dock's steps give the address; a row that is gone says so.
            let copied = act(&e, "copy_url", &id).unwrap();
            assert_eq!(copied["copy"], rows[0]["cells"][5]);
            // Test needs an open page to send to.
            assert!(act(&e, "test", &id).is_err());
            assert!(act(&e, "copy_url", "Nope:x.html").is_err());
            assert!(act(&e, "fly", &id).is_err());
            // The page's form: its title, size, whether it is a dock, and what the pages may do.
            let kind = format!("overlay:page:{id}");
            let form = form_for(&e, &json!({"kind": kind})).unwrap();
            let keys: Vec<&str> = form["fields"]
                .as_array()
                .unwrap()
                .iter()
                .filter_map(|f| f["key"].as_str())
                .collect();
            assert_eq!(
                keys,
                [
                    "title",
                    "width",
                    "height",
                    "dock",
                    "chat.read",
                    "alerts.read",
                    "chat.send",
                    "actions.run",
                    "values.write"
                ]
            );
            let changes = json!([{"field": 0, "value": "Big clock"}, {"field": 1, "value": "800"}, {"field": 2, "value": "200"}, {"field": 6, "value": true}]);
            edit_form(&e, &json!({"kind": kind, "changes": changes}), false).unwrap();
            let meta = groups::meta(&e.config(), "Clock");
            assert_eq!(
                meta.template.pages,
                [Page {
                    file: "index.html".into(),
                    title: "Big clock".into(),
                    width: 800,
                    height: 200,
                    dock: false
                }]
            );
            assert_eq!(meta.permissions, ["chat.send"]);
            assert!(edit_form(
                &e,
                &json!({"kind": kind, "changes": [{"field": 1, "value": "wide"}]}),
                false
            )
            .is_err());
            assert!(edit_form(
                &e,
                &json!({"kind": kind, "changes": [{"field": 3, "value": "yes"}]}),
                false
            )
            .is_err());
            assert_eq!(
                overlays::obs_source(&e.config(), "Clock", "index.html").0,
                "Clock: Big clock"
            );
            // The group's settings, through the same form the Actions page uses.
            let kind = format!("overlay:settings:{id}");
            let form = form_for(&e, &json!({"kind": kind})).unwrap();
            assert_eq!(
                (form["kind"].as_str(), form["fields"][1]["key"].as_str()),
                (Some(kind.as_str()), Some("text"))
            );
            let saved = edit_form(
                &e,
                &json!({"kind": kind, "changes": [{"field": 0, "value": "A clock"}, {"field": 1, "value": "Hi there"}]}),
                false,
            )
            .unwrap();
            assert_eq!(saved["saved"], id.as_str());
            assert_eq!(groups::values(&e.config(), "Clock")["text"], "Hi there");
            // Export writes the file and records what was typed; import of it asks first.
            let file = std::env::temp_dir().join(format!("streamerz-overlays-{}", std::process::id()));
            let kind = format!("overlay:export:{id}");
            let form = form_for(&e, &json!({"kind": kind})).unwrap();
            assert!(form["fields"][0]["value"]
                .as_str()
                .unwrap()
                .ends_with("Clock.streamerz"));
            let changes =
                json!([{"field": 0, "value": file}, {"field": 1, "value": "1.2.0"}, {"field": 2, "value": "ana"}]);
            edit_form(&e, &json!({"kind": kind, "changes": changes}), false).unwrap();
            let file = file.with_extension("streamerz");
            // The one thing worth a look (a permission no page uses) is said in a notice.
            let notices = super::super::prompts::list();
            let said = serde_json::to_string(&notices).unwrap();
            assert!(said.contains("chat.send"), "{said}");
            for notice in notices {
                let _ = super::super::prompts::answer(notice.id, json!(true));
            }
            let package = package::open(&std::fs::read(&file).unwrap()).unwrap();
            let about = package::describe(&e.config(), &package);
            assert_eq!(
                (about["version"].as_str(), about["author"].as_str()),
                (Some("1.2.0"), Some("ana"))
            );
            assert_eq!(about["installed"]["state"], "same");
            assert!(edit_form(
                &e,
                &json!({"kind": kind, "changes": [{"field": 0, "value": ""}]}),
                false
            )
            .is_err());
            let question = import_question(&about);
            for part in ["Clock", "1.2.0", "ana", "files: 2", "Send chat messages"] {
                assert!(question.contains(part), "{part} in:\n{question}");
            }
            assert!(edit_form(&e, &json!({"kind": "overlay:import", "changes": []}), false).is_err());
            // Connect: the first choice is a new group of the overlay's name, then the groups there.
            e.change(|c| {
                groups::create(c, "Votes");
            });
            let kind = format!("overlay:connect:{id}");
            let form = form_for(&e, &json!({"kind": kind})).unwrap();
            assert_eq!(
                form["fields"][0]["widget"]["options"],
                json!(["A new group named Clock", "Votes"])
            );
            assert!(act(&e, "disconnect", &id).is_err(), "nothing is connected yet");
            edit_form(&e, &json!({"kind": kind, "changes": [{"field": 0, "value": 1}]}), false).unwrap();
            assert!(groups::connected(&e.config(), "Clock") && !groups::exists(&e.config(), "Votes"));
            let rows = read(&e, DOMAIN).unwrap()["tables"][0]["rows"].clone();
            assert_eq!(rows[0]["cells"][3], json!({"text": "An empty group", "icon": "link"}));
            assert_eq!(
                rows[0]["can"],
                json!({"connect": false, "disconnect": true, "obs_setup":true, "add_to_obs":false})
            );
            assert!(rows[0]["confirm"]["delete"]
                .as_str()
                .unwrap()
                .contains("actions connected"));
            // The Actions page shows the group with a chain, and asks its own question to delete it.
            let tree = super::super::actions::read(&e, &json!({"domain": "actions"})).unwrap();
            let node = tree["actions"]["tree"]["nodes"]
                .as_array()
                .unwrap()
                .iter()
                .find(|n| n["id"] == "g:Clock")
                .cloned()
                .unwrap();
            assert_eq!(node["note"]["icon"], "link");
            assert!(node["confirm"]["group_delete"]
                .as_str()
                .unwrap()
                .starts_with("There is an overlay connected to this group"));
            assert!(tree["actions"]["tree"]["nodes"]
                .as_array()
                .unwrap()
                .iter()
                .all(|n| n["id"] != "g:Clock 2"));
            // Deleting the group there leaves the overlay here.
            super::super::actions::act(&e, &json!({"action": "group_delete", "itemId": "g:Clock"})).unwrap();
            assert!(!groups::listed(&e.config(), "Clock") && groups::has_overlay(&e.config(), "Clock"));
            // Disconnect does the same from this page.
            edit_form(&e, &json!({"kind": kind, "changes": []}), false).unwrap();
            act(&e, "disconnect", &id).unwrap();
            assert!(!groups::listed(&e.config(), "Clock") && groups::has_overlay(&e.config(), "Clock"));
            // Delete takes the page off the disk and off the list; the last page takes the overlay along.
            act(&e, "delete", &id).unwrap();
            assert!(!groups::exists(&e.config(), "Clock"));
            assert!(e.config().action_group_variables.get("Clock").is_none());
            assert_eq!(
                read(&e, DOMAIN).unwrap()["tables"][0]["rows"].as_array().unwrap().len(),
                1
            );
            e.change(|c| groups::remove(c, "Clock 2"));
            let _ = std::fs::remove_file(file);
        });
    }

    #[test]
    fn an_overlay_is_made_changed_tried_and_deleted_by_requests_alone() {
        let _g = crate::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        run(async {
            let e = engine(json!({"actions": []}));
            let ask = |req: Value| {
                let e = e.clone();
                async move { request(&e, &req).await }
            };
            // A name is enough: the starter page, listed, with no group of actions.
            let made = ask(json!({"op": "save", "overlay": "Score"})).await.unwrap();
            assert_eq!((&made["created"], &made["connected"]), (&json!(true), &json!(false)));
            assert_eq!(made["pages"][0]["file"], "index.html");
            assert!(made["pages"][0]["url"]
                .as_str()
                .unwrap()
                .ends_with("/overlays/Score/index.html"));
            assert!(groups::names(&e.config()).is_empty());
            // Files, settings, values, permissions and what Test adds, in one request.
            let page = "<!doctype html><title>Score</title><script>streamerz.chat(l => document.title = l.text); \
                        streamerz.on('score', v => document.body.textContent = v)</script>";
            let settings = json!([{"name": "color", "label": "Color", "kind": "color", "default": "#ff0000"},
                {"name": "size", "label": "Size", "kind": "slider", "default": 40, "min": 10, "max": 90}]);
            let saved = ask(
                json!({"op": "save", "overlay": "Score", "files": {"index.html": page, "dock.html": "<p>dock</p>"},
                "filesBase64": {"img/dot.png": "iVBORw0KGgo="}, "settings": settings, "values": {"size": 55},
                "permissions": ["chat.read"], "description": "Shows the score", "test": {"topics": {"score": 3}},
                "dock": true}),
            )
            .await
            .unwrap();
            assert_eq!(saved["created"], false);
            let files: Vec<&str> = saved["files"]
                .as_array()
                .unwrap()
                .iter()
                .filter_map(|f| f["name"].as_str())
                .collect();
            assert_eq!(files, ["index.html", "streamerz.d.ts", "dock.html", "img/dot.png"]);
            assert_eq!(saved["pages"][1]["title"], "Score: dock");
            assert_eq!(saved["pages"][1]["dock"], true);
            let read = ask(json!({"op": "read", "overlay": "Score", "file": "index.html"}))
                .await
                .unwrap();
            assert_eq!(read["text"], page);
            assert_eq!(
                (&read["values"]["size"], &read["values"]["color"]),
                (&json!(55), &json!("#ff0000"))
            );
            assert_eq!(read["settings"][1]["max"], 90.0);
            assert_eq!(read["test"]["topics"]["score"], 3);
            let picture = ask(json!({"op": "read", "overlay": "Score", "file": "img/dot.png"}))
                .await
                .unwrap();
            assert_eq!(picture["content"], "iVBORw0KGgo=");
            // What is wrong is refused whole, and said.
            for bad in [
                json!({"op": "save", "overlay": "Score", "values": {"size": 500}}),
                json!({"op": "save", "overlay": "Score", "permissions": ["everything"]}),
                json!({"op": "save", "overlay": "Score", "files": {"../x.html": "x"}}),
                json!({"op": "save", "overlay": " "}),
                json!({"op": "read", "overlay": "Nope"}),
                json!({"op": "read", "overlay": "Score", "file": "nope.html"}),
                json!({"op": "fly", "overlay": "Score"}),
            ] {
                assert!(ask(bad.clone()).await.is_err(), "{bad}");
            }
            assert_eq!(groups::values(&e.config(), "Score")["size"], 55);
            // The list has it, with the topics; Test and the inspector need an open page.
            let list = ask(json!({"op": "list"})).await.unwrap();
            assert_eq!(list["overlays"][0]["name"], "Score");
            assert_eq!(list["overlays"][0]["open"], 0);
            assert!(ask(json!({"op": "test", "overlay": "Score"})).await.is_err());
            let seen = ask(json!({"op": "inspect", "overlay": "Score"})).await.unwrap();
            assert_eq!(
                (&seen["pages"], &seen["sent"], &seen["log"]),
                (&json!([]), &json!([]), &json!([]))
            );
            assert_eq!(seen["values"]["size"], 55);
            assert_eq!(
                act(&e, "inspect", "Score:index.html").unwrap()["values"]["rows"],
                json!([])
            );
            // Connected to a new group of its name, then to none again.
            let linked = ask(json!({"op": "connect", "overlay": "Score"})).await.unwrap();
            assert_eq!(linked["connected"], true);
            assert_eq!(groups::names(&e.config()), ["Score"]);
            let free = ask(json!({"op": "disconnect", "overlay": "Score"})).await.unwrap();
            assert_eq!(free["connected"], false);
            assert!(ask(json!({"op": "disconnect", "overlay": "Score"})).await.is_err());
            // A dock gives its steps instead of going to OBS; a page needs the server and OBS.
            let dock = ask(json!({"op": "obs", "overlay": "Score", "file": "dock.html"}))
                .await
                .unwrap();
            assert_eq!((&dock["dock"], &dock["name"]), (&json!(true), &json!("Score: dock")));
            assert!(ask(json!({"op": "obs", "overlay": "Score"})).await.is_err());
            // One file goes, then the rest: with a group connected its settings stay for the actions.
            ask(json!({"op": "delete", "overlay": "Score", "file": "dock.html"}))
                .await
                .unwrap();
            assert_eq!(groups::meta(&e.config(), "Score").template.pages.len(), 1);
            ask(json!({"op": "connect", "overlay": "Score"})).await.unwrap();
            let gone = ask(json!({"op": "delete", "overlay": "Score"})).await.unwrap();
            assert_eq!(gone["groupKept"], true);
            assert!(!groups::has_overlay(&e.config(), "Score") && groups::listed(&e.config(), "Score"));
            assert_eq!(groups::values(&e.config(), "Score")["size"], 55);
            e.change(|c| groups::remove(c, "Score"));
            // The examples: listed, and one added by its slug.
            let examples = ask(json!({"op": "examples"})).await.unwrap();
            let slugs: Vec<&str> = examples["examples"]
                .as_array()
                .unwrap()
                .iter()
                .filter_map(|x| x["slug"].as_str())
                .collect();
            assert!(slugs.contains(&"clock") && slugs.contains(&"poll"), "{slugs:?}");
            let poll = ask(json!({"op": "add_example", "slug": "poll"})).await.unwrap();
            assert_eq!(poll["connected"], true, "an example with actions comes connected");
            let clock = ask(json!({"op": "add_example", "slug": "clock"})).await.unwrap();
            assert_eq!(clock["connected"], false, "an example with pages only makes no group");
            assert!(ask(json!({"op": "add_example", "slug": "nope"})).await.is_err());
            let names = groups::names(&e.config());
            for name in [poll["name"].as_str().unwrap(), clock["name"].as_str().unwrap()] {
                e.change(|c| groups::remove(c, name));
            }
            assert_eq!(names, [poll["name"].as_str().unwrap()]);
        });
    }

    #[test]
    fn overlays_can_be_renamed_through_form_and_request() {
        let _g = crate::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        run(async {
            let e = engine(json!({"actions": []}));
            let made = edit_form(
                &e,
                &json!({"kind": "overlay:new", "changes": [{"field": 0, "value": "Clock"}]}),
                false,
            )
            .unwrap();
            let id = made["saved"].as_str().unwrap().to_owned();
            assert_eq!(id, "Clock:index.html");

            // Rename action is exposed on the table with a pencil icon.
            let table = read(&e, DOMAIN).unwrap();
            let rename_btn = table["tables"][0]["actions"]
                .as_array()
                .unwrap()
                .iter()
                .find(|a| a["id"] == "rename")
                .unwrap();
            assert_eq!(rename_btn["icon"], "pencil");
            assert_eq!(rename_btn["form"], "overlay:rename:");
            assert_eq!(rename_btn["needs"], "row");

            // Form opens prefilled, focused, and non-JSON.
            let form = form_for(&e, &json!({"kind": format!("overlay:rename:{id}")})).unwrap();
            assert_eq!(form["focusField"], 0);
            assert_eq!(form["json"], false);
            assert_eq!(form["fields"][0]["value"], "Clock");
            assert_eq!(form["hint"], tr("overlays.rename.hint"));

            // Connect to a group and verify connected hint and full rename preservation.
            request(&e, &json!({"op": "connect", "overlay": "Clock"}))
                .await
                .unwrap();
            e.change(|c| {
                c.actions.actions.push(crate::model::Action {
                    id: "a1".into(),
                    name: "Tick".into(),
                    group: "Clock".into(),
                    enabled: true,
                    ..Default::default()
                });
            });
            let connected_form = form_for(&e, &json!({"kind": format!("overlay:rename:{id}")})).unwrap();
            assert_eq!(connected_form["hint"], tr("overlays.rename.hint_connected"));

            // Empty name fails, unchanged succeeds cleanly, new name renames overlay, page title, and connected group.
            assert!(edit_form(
                &e,
                &json!({"kind": format!("overlay:rename:{id}"), "changes": [{"field": 0, "value": "   "}]}),
                false,
            )
            .is_err());
            let same = edit_form(
                &e,
                &json!({"kind": format!("overlay:rename:{id}"), "changes": [{"field": 0, "value": "Clock"}]}),
                false,
            )
            .unwrap();
            assert_eq!(same["saved"], "Clock:index.html");

            let saved = edit_form(
                &e,
                &json!({"kind": format!("overlay:rename:{id}"), "changes": [{"field": 0, "value": "Stream Clock"}]}),
                false,
            )
            .unwrap();
            assert_eq!(saved["saved"], "Stream Clock:index.html");
            let config = e.config();
            assert!(!groups::exists(&config, "Clock"));
            assert!(groups::has_overlay(&config, "Stream Clock"));
            assert!(groups::listed(&config, "Stream Clock"));
            assert_eq!(config.actions.actions[0].group, "Stream Clock");
            assert_eq!(
                overlays::page_of(&config, "Stream Clock", "index.html").title,
                "Stream Clock"
            );
            drop(config);

            // Duplicate name is rejected on both form and request.
            e.change(|c| {
                groups::create(c, "Taken");
            });
            assert!(edit_form(
                &e,
                &json!({"kind": "overlay:rename:Stream Clock:index.html", "changes": [{"field": 0, "value": "Taken"}]}),
                false,
            )
            .is_err());
            assert!(
                request(&e, &json!({"op": "rename", "overlay": "Stream Clock", "name": "Taken"}))
                    .await
                    .is_err()
            );
            assert!(
                request(&e, &json!({"op": "rename", "overlay": "Missing", "name": "Other"}))
                    .await
                    .is_err()
            );

            let renamed = request(
                &e,
                &json!({"op": "rename", "overlay": "Stream Clock", "name": "Desk Clock"}),
            )
            .await
            .unwrap();
            assert_eq!(renamed["name"], "Desk Clock");
            assert!(groups::has_overlay(&e.config(), "Desk Clock"));

            e.change(|c| {
                groups::remove(c, "Desk Clock");
                groups::remove(c, "Taken");
            });
        });
    }

    #[test]
    fn examples_model_uses_side_detail_search_and_keeps_cells_contract() {
        let _lock = crate::TEST_LOCK.lock().unwrap_or_else(|error| error.into_inner());
        crate::engine::tests::run(async {
            let e = crate::engine::tests::engine(json!({"actions": []}));
            let model = read(&e, EXAMPLES).unwrap();
            let table = &model["tables"][0];
            assert_eq!(table["search"], true);
            assert_eq!(table["selectFirst"], true);
            assert_eq!(table["detailSide"], true);
            assert_eq!(table["dialogWidth"], 760);
            assert_eq!(table["perRow"], 2);
            assert_eq!(table["columns"][1]["key"], "about");
            assert_eq!(table["columns"][1]["hidden"], true);
            let rows = table["rows"].as_array().unwrap();
            assert!(!rows.is_empty());
            for row in rows {
                assert_eq!(row["detailTitle"], row["name"]);
                assert!(row["detail"].as_str().is_some_and(|s| !s.is_empty()));
                assert!(row["detailNote"].as_str().is_some_and(|s| !s.is_empty()));
                assert_eq!(row["cells"].as_array().unwrap().len(), 3);
            }
        });
    }
}
