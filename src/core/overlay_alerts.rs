//! The Alerts tab of an alert box (an overlay whose rules are a `list` setting called `rules`): the
//! rules as rows, each edited by a form made from the rules' columns, moved, deleted, and played on the
//! preview with an event made up to match it. Changes are a draft, as on the Settings tab: the preview
//! shows them at once, Save sends them to OBS.
use super::overlay_test;
use crate::engine::Engine;
use crate::groups::{self, Def, Kind};
use crate::i18n::{tr, trf};
use serde_json::{json, Map, Value};
use std::rc::Rc;

/// The `rules` setting's declaration.
fn rules_def(config: &crate::Config, group: &str) -> Option<Def> {
    groups::meta(config, group)
        .variables
        .into_iter()
        .find(|v| v.name == "rules" && v.kind == Kind::List)
}

/// The rules as the preview shows them: the draft's when there is one.
fn rules(engine: &Rc<Engine>, group: &str) -> Vec<Value> {
    let draft = engine.overlay_draft(group);
    draft
        .get("rules")
        .cloned()
        .unwrap_or_else(|| {
            groups::values(&engine.config(), group)
                .get("rules")
                .cloned()
                .unwrap_or_default()
        })
        .as_array()
        .cloned()
        .unwrap_or_default()
}

/// Keeps new rules as the draft (and sends them to OBS at once with Show on OBS while I edit).
fn keep(engine: &Rc<Engine>, group: &str, rules: Vec<Value>) -> Result<Value, String> {
    let mut values = Map::new();
    values.insert("rules".into(), Value::Array(rules));
    engine.overlay_draft_set(group, &values)?;
    if super::overlays::live_edit_on(group) {
        engine.overlay_draft_save(group)?;
    }
    Ok(json!({"accepted": true, "draft": engine.overlay_draft(group).len()}))
}

fn text(rule: &Value, key: &str) -> String {
    match &rule[key] {
        Value::String(s) => s.clone(),
        Value::Null => String::new(),
        v => v.to_string(),
    }
}

/// The tab: a row a rule, with its picture, and Add a rule.
pub fn section(engine: &Rc<Engine>, group: &str, id: &str) -> Value {
    let config = engine.config();
    let rows: Vec<Value> = rules(engine, group)
        .iter()
        .enumerate()
        .map(|(at, rule)| {
            let mut when = format!("{}, {}", text(rule, "when"), text(rule, "from"));
            let (min, max) = (rule["min"].as_f64().unwrap_or(0.0), rule["max"].as_f64().unwrap_or(0.0));
            if min > 0.0 || max > 0.0 {
                let range = match (min > 0.0, max > 0.0) {
                    (true, true) => format!("{min} - {max}"),
                    (true, false) => trf("overlays.alerts.at_least", &[("n", &min.to_string())]),
                    _ => trf("overlays.alerts.at_most", &[("n", &max.to_string())]),
                };
                when = format!("{when}, {range}");
            }
            let mut detail = text(rule, "text");
            if !text(rule, "say").is_empty() {
                detail = format!("{detail} ({})", trf("overlays.alerts.says", &[("text", &text(rule, "say"))]));
            }
            let picture = Some(text(rule, "picture"))
                .filter(|p| groups::accepts_file("image", p))
                .and_then(|p| groups::file_path(&config, group, &p))
                .map(|p| p.display().to_string());
            let mut buttons = vec![
                json!({"id": format!("rule_play:{at}"), "label": tr("overlays.alerts.play"), "icon": "play", "iconOnly": true}),
                json!({"form": format!("overlay:rule:{id}|{at}"), "label": tr("btn.edit"), "icon": "pencil"}),
            ];
            if at > 0 {
                buttons.push(json!({"id": format!("rule_up:{at}"), "label": tr("btn.move_up"), "icon": "arrow-up", "iconOnly": true}));
            }
            buttons.push(json!({"id": format!("rule_delete:{at}"), "label": tr("btn.delete"), "icon": "trash-2",
                "confirm": trf("overlays.alerts.delete_confirm", &[("rule", &when)]), "danger": true, "iconOnly": true}));
            json!({"text": when, "detail": detail, "icon": "bell", "picture": picture, "buttons": buttons})
        })
        .collect();
    json!({"text": tr("overlays.alerts.intro"), "rows": rows, "empty": tr("overlays.alerts.none"),
        "buttons": [{"form": format!("overlay:rule:{id}|new"), "label": tr("overlays.alerts.add"), "icon": "plus"}]})
}

/// The form of one rule (`index` a number, or `new`): a field a column.
pub fn form(engine: &Rc<Engine>, group: &str, kind: String, index: &str) -> Result<Value, String> {
    let config = engine.config();
    let def = rules_def(&config, group).ok_or_else(|| tr("overlays.alerts.not_alert_box").to_owned())?;
    let all = rules(engine, group);
    let rule = match index.parse::<usize>() {
        Ok(at) => all
            .get(at)
            .cloned()
            .ok_or_else(|| tr("overlays.alerts.gone").to_owned())?,
        Err(_) => Value::Object(
            def.columns
                .iter()
                .map(|c| (c.name.clone(), c.default.clone()))
                .collect(),
        ),
    };
    let meta = groups::meta(&config, group);
    let files = |accept: &str| -> Vec<Value> {
        meta.files
            .iter()
            .filter(|f| !groups::is_page(&f.name) && groups::accepts_file(accept, &f.name))
            .map(|f| json!({"id": f.name, "label": f.name}))
            .collect()
    };
    let mut item = Map::new();
    let fields: Vec<Value> = def
        .columns
        .iter()
        .map(|c| {
            let v = rule.get(&c.name).cloned().unwrap_or_else(|| c.default.clone());
            let label = if c.label.is_empty() {
                c.name.clone()
            } else {
                c.label.clone()
            };
            let mut f = match c.kind {
                Kind::Choice => json!({"widget": {"kind": "pick", "options": c.options},
                    "value": c.options.iter().position(|o| v.as_str() == Some(o)).unwrap_or(0)}),
                Kind::Boolean => json!({"widget": {"kind": "bool"}, "value": v.as_bool().unwrap_or(false)}),
                Kind::Slider => {
                    let (min, max, step) = c.range();
                    json!({"widget": {"kind": "slider", "min": min, "max": max, "step": step},
                        "value": v.as_f64().unwrap_or(min).clamp(min, max)})
                }
                Kind::Color => json!({"widget": {"kind": "color"}, "value": v.as_str().unwrap_or("#ffffff")}),
                _ => json!({"widget": {"kind": "text"}, "value": c.show(&v)}),
            };
            f["key"] = c.name.clone().into();
            f["label"] = label.into();
            if !c.description.is_empty() {
                f["tip"] = c.description.clone().into();
            }
            // A picture, a sound or a voice is picked from what there is, or typed (Browse brings a file in).
            match c.name.as_str() {
                "picture" => {
                    let mut choices = files("image");
                    choices.extend(files("video"));
                    f["choices"] = choices.into();
                    f["other"] = true.into();
                    f["browse"] = "image".into();
                }
                "sound" => {
                    f["choices"] = files("sound").into();
                    f["other"] = true.into();
                    f["browse"] = "file".into();
                }
                "voice" => {
                    let voices = crate::voice::voices_with(&config.voices).unwrap_or_default();
                    let mut choices = vec![json!({"id": "Default", "label": tr("sounds.default")})];
                    choices.extend(voices.into_iter().map(|v| json!({"id": v.id, "label": v.name})));
                    f["choices"] = choices.into();
                    f["other"] = true.into();
                    if f["value"].as_str().is_none_or(str::is_empty) {
                        f["value"] = "Default".into();
                    }
                }
                _ => {}
            }
            item.insert(c.name.clone(), f["value"].clone());
            f
        })
        .collect();
    let title = if index == "new" {
        tr("overlays.alerts.add").to_owned()
    } else {
        trf(
            "overlays.alerts.edit_title",
            &[("rule", &format!("{}, {}", text(&rule, "when"), text(&rule, "from")))],
        )
    };
    item.insert("id".into(), index.into());
    Ok(
        json!({"kind": kind, "title": title, "okLabel": tr("btn.save"), "json": false, "item": item, "new": item.clone(),
        "fields": fields}),
    )
}

/// Saves a rule's form into the draft.
pub fn save(engine: &Rc<Engine>, group: &str, index: &str, request: &Value) -> Result<Value, String> {
    let def = rules_def(&engine.config(), group).ok_or_else(|| tr("overlays.alerts.not_alert_box").to_owned())?;
    let mut all = rules(engine, group);
    let at = index.parse::<usize>().ok();
    let mut rule: Map<String, Value> = match at {
        Some(at) => all
            .get(at)
            .and_then(Value::as_object)
            .cloned()
            .ok_or_else(|| tr("overlays.alerts.gone").to_owned())?,
        None => def
            .columns
            .iter()
            .map(|c| (c.name.clone(), c.default.clone()))
            .collect(),
    };
    for change in request["changes"].as_array().into_iter().flatten() {
        let Some(c) = change["field"].as_u64().and_then(|i| def.columns.get(i as usize)) else {
            continue;
        };
        let v = &change["value"];
        let parsed = match c.kind {
            Kind::Choice => v.as_u64().and_then(|i| c.options.get(i as usize)).map(|o| json!(o)),
            Kind::Boolean => v.as_bool().map(Value::from),
            Kind::Slider => v
                .as_f64()
                .map(|n| crate::groups::kinds::tidy(n.clamp(c.range().0, c.range().1))),
            _ => {
                let typed = v.as_str().unwrap_or_default().trim().trim_matches('"').to_string();
                let typed = if c.name == "voice" && typed.eq_ignore_ascii_case("Default") {
                    String::new()
                } else {
                    typed
                };
                // A file of this computer picked for a picture or a sound comes into the overlay.
                let brought =
                    if matches!(c.name.as_str(), "picture" | "sound") && std::path::Path::new(&typed).is_absolute() {
                        let accept = if c.name == "picture" { "image" } else { "sound" };
                        let mut meta = groups::meta(&engine.config(), group);
                        let file_def = Def {
                            name: c.name.clone(),
                            label: c.label.clone(),
                            kind: Kind::File,
                            accept: accept.into(),
                            ..Def::default()
                        };
                        let (file, name) = super::actions::bring_in(&meta, &file_def, &typed)?;
                        meta.files.push(file);
                        let mut result = Ok(());
                        engine.change(|cfg| result = groups::set_meta(cfg, group, meta));
                        result?;
                        name
                    } else {
                        typed
                    };
                Some(c.parse(&brought).map_err(|e| format!("{}: {e}", c.label))?)
            }
        };
        rule.insert(
            c.name.clone(),
            parsed.ok_or_else(|| trf("forms.invalid_value", &[("label", &c.label)]))?,
        );
    }
    match at {
        Some(at) => all[at] = Value::Object(rule),
        None => all.push(Value::Object(rule)),
    }
    keep(engine, group, all)
}

/// A rule's buttons: `rule_play:<n>`, `rule_up:<n>`, `rule_delete:<n>`.
pub fn act(engine: &Rc<Engine>, group: &str, action: &str) -> Result<Value, String> {
    let (op, at) = action.split_once(':').ok_or("no rule named")?;
    let at: usize = at.parse().map_err(|_| "no rule named")?;
    let mut all = rules(engine, group);
    if at >= all.len() {
        return Err(tr("overlays.alerts.gone").into());
    }
    match op {
        "rule_play" => {
            let (code, args) = made_up(&all[at]);
            overlay_test::play(engine, group, code, &args)?;
            Ok(json!({"accepted": true}))
        }
        "rule_up" => {
            if at > 0 {
                all.swap(at, at - 1);
            }
            keep(engine, group, all)
        }
        "rule_delete" => {
            all.remove(at);
            keep(engine, group, all)
        }
        other => Err(format!("no rule operation {other}")),
    }
}

/// An event that matches a rule: its kind on its platform, worth its least amount (or its most).
pub fn made_up(rule: &Value) -> (u32, crate::model::Obj) {
    let from = rule["from"].as_str().unwrap_or("Any");
    let on = |twitch: u32, youtube: u32, kick: u32, tiktok: u32| -> u32 {
        let pick = match from {
            "YouTube" => youtube,
            "Kick" => kick,
            "TikTok" => tiktok,
            _ => twitch,
        };
        if pick == 0 {
            [twitch, kick, youtube, tiktok]
                .into_iter()
                .find(|c| *c != 0)
                .unwrap_or(101)
        } else {
            pick
        }
    };
    let code = match rule["when"].as_str().unwrap_or("Any") {
        "Sub" => on(103, 4018, 35014, 41006),
        "Resub" => on(104, 4009, 35015, 0),
        "Gift sub" => on(105, 0, 35016, 0),
        "Gift bomb" => on(106, 4014, 35017, 0),
        "Cheer" => on(102, 0, 35025, 0),
        "Raid" => 107,
        "Super Chat" => 4006,
        "Super Sticker" => 4007,
        "Member" => 4008,
        "TikTok gift" => 41002,
        "Share" => 41005,
        "Reward" => on(112, 0, 35024, 0),
        "Power-up" => 196,
        _ => on(101, 0, 35011, 41004),
    };
    let mut args = overlay_test::made_up(code, 0);
    let (min, max) = (rule["min"].as_f64().unwrap_or(0.0), rule["max"].as_f64().unwrap_or(0.0));
    let worth = if min > 0.0 { min } else { max };
    if worth > 0.0 {
        let n = json!(worth.round() as u64);
        for key in [
            "bits",
            "kicks",
            "viewers",
            "viewerCount",
            "gifts",
            "count",
            "totalGifts",
            "cumulative",
            "months",
            "coins",
        ] {
            if args.contains_key(key) || matches!(key, "count" | "coins") {
                args.insert(key.into(), n.clone());
            }
        }
        if matches!(code, 4006 | 4007) {
            args.insert("amount".into(), json!(worth));
            args.insert("amountMicros".into(), json!((worth * 1e6) as u64));
        }
    }
    (code, args)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_alerts_tab_edits_moves_deletes_and_plays_rules_as_a_draft() {
        let _g = crate::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        crate::engine::tests::run(async {
            let e = crate::engine::tests::engine(json!({"actions": []}));
            let example = crate::examples::package("alert-box").unwrap();
            let mut group = String::new();
            e.change(|c| group = crate::package::install(c, &example).unwrap().0);
            let id = format!("{group}:index.html");
            let saved = rules(&e, &group).len();
            let tab = section(&e, &group, &id);
            assert_eq!(tab["rows"].as_array().unwrap().len(), saved);
            assert!(
                tab["rows"][0]["picture"].as_str().unwrap().ends_with("star.svg"),
                "{}",
                tab["rows"][0]
            );
            // The form of the first rule: a field a column, with its sounds and voices to pick.
            let form = form(&e, &group, "overlay:rule:x|0".into(), "0").unwrap();
            let at = |key: &str| {
                form["fields"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .position(|f| f["key"] == key)
                    .unwrap()
            };
            assert_eq!(form["fields"][at("sound")]["choices"][0]["id"], "sounds/chime.wav");
            assert_eq!(form["fields"][at("voice")]["value"], "Default");
            let change = json!({"changes": [{"field": at("text"), "value": "%user% is here!"}, {"field": at("say"), "value": "Welcome %user%"}]});
            save(&e, &group, "0", &change).unwrap();
            assert_eq!(rules(&e, &group)[0]["say"], "Welcome %user%");
            assert_eq!(e.overlay_draft(&group).len(), 1, "a draft, until Save");
            assert_ne!(
                crate::groups::values(&e.config(), &group)["rules"][0]["say"],
                "Welcome %user%"
            );
            // A new rule, moved up, then deleted; Play this rule sends its event.
            save(
                &e,
                &group,
                "new",
                &json!({"changes": [{"field": at("text"), "value": "new one"}]}),
            )
            .unwrap();
            let last = rules(&e, &group).len() - 1;
            act(&e, &group, &format!("rule_up:{last}")).unwrap();
            assert_eq!(rules(&e, &group)[last - 1]["text"], "new one");
            act(&e, &group, &format!("rule_delete:{}", last - 1)).unwrap();
            assert_eq!(rules(&e, &group).len(), saved);
            act(&e, &group, "rule_play:0").unwrap();
            assert!(act(&e, &group, "rule_play:99").is_err());
            let _ = crate::groups::set_meta(&mut crate::Config::default(), &group, crate::groups::Meta::default());
        });
    }

    #[test]
    fn a_rule_is_played_with_an_event_that_matches_it() {
        let (code, args) = made_up(&json!({"when": "Cheer", "from": "Kick", "min": 500, "max": 0}));
        assert_eq!(code, 35025);
        assert_eq!(args["kicks"], 500);
        let (code, args) = made_up(&json!({"when": "Raid", "from": "Any", "min": 0, "max": 50}));
        assert_eq!((code, args["viewers"].clone()), (107, json!(50)));
        let (code, _) = made_up(&json!({"when": "Gift sub", "from": "TikTok"}));
        assert_eq!(code, 105, "no TikTok gift sub: the first platform that has it");
        let (code, args) = made_up(&json!({"when": "Super Chat", "from": "YouTube", "min": 20}));
        assert_eq!((code, args["amountMicros"].clone()), (4006, json!(20_000_000)));
        assert_eq!(made_up(&json!({"when": "Any", "from": "TikTok"})).0, 41004);
    }
}
