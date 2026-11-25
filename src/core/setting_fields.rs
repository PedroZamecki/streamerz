//! The settings form's fields for the kinds of `groups::kinds`: what each one draws, and how what the
//! form sends back is read into the setting's value. The older kinds are in `actions.rs`.
use crate::groups::{kinds, Def, Kind, Meta};
use crate::i18n::{tr, trf};
use serde_json::{json, Map, Value};

/// One control of a field made of several (`parts`): drawn on line `line` of the field.
fn part(key: &str, label: &str, widget: Value, value: Value, line: u32) -> Value {
    json!({"key": key, "label": label, "widget": widget, "value": value, "line": line})
}

/// A field of several controls; its value is theirs, by key.
fn parts(def: &Def, label: &str, list: Vec<Value>) -> Value {
    let value: Map<String, Value> = list
        .iter()
        .filter(|p| p["widget"]["kind"] != "button")
        .map(|p| (p["key"].as_str().unwrap_or_default().to_owned(), p["value"].clone()))
        .collect();
    json!({"key": def.name, "label": label, "widget": {"kind": "parts", "parts": list}, "value": value})
}

/// Values one click away under a box: `(label, what goes in the box)`.
fn presets(list: Vec<(String, String)>) -> Value {
    list.into_iter()
        .map(|(label, value)| json!({"label": label, "value": value}))
        .collect()
}

fn own_presets(def: &Def) -> Vec<(String, String)> {
    def.presets.iter().map(|p| (p.clone(), p.clone())).collect()
}

fn text_part(key: &str, label: &str, value: String, line: u32) -> Value {
    part(key, label, json!({"kind": "text"}), value.into(), line)
}

fn number_text(v: &Value) -> String {
    v.as_f64().map(|n| kinds::tidy(n).to_string()).unwrap_or_default()
}

/// The names shown for `kinds::ANCHORS`, `ANIMATIONS`, `EASINGS`, the nine weights and `EVENTS`, in
/// their order (a test checks each key ends with its name).
const ANCHOR_KEYS: [&str; 9] = [
    "forms.anchor.top-left",
    "forms.anchor.top",
    "forms.anchor.top-right",
    "forms.anchor.left",
    "forms.anchor.center",
    "forms.anchor.right",
    "forms.anchor.bottom-left",
    "forms.anchor.bottom",
    "forms.anchor.bottom-right",
];
const ANIMATION_KEYS: [&str; 11] = [
    "forms.animation.none",
    "forms.animation.fade",
    "forms.animation.slide-up",
    "forms.animation.slide-down",
    "forms.animation.slide-left",
    "forms.animation.slide-right",
    "forms.animation.zoom",
    "forms.animation.pop",
    "forms.animation.bounce",
    "forms.animation.flip",
    "forms.animation.shake",
];
const EASING_KEYS: [&str; 8] = [
    "forms.easing.linear",
    "forms.easing.ease",
    "forms.easing.ease_in",
    "forms.easing.ease_out",
    "forms.easing.ease_in_and_out",
    "forms.easing.back",
    "forms.easing.snappy",
    "forms.easing.steps",
];
const WEIGHT_KEYS: [&str; 9] = [
    "forms.weight.w100",
    "forms.weight.w200",
    "forms.weight.w300",
    "forms.weight.w400",
    "forms.weight.w500",
    "forms.weight.w600",
    "forms.weight.w700",
    "forms.weight.w800",
    "forms.weight.w900",
];
const EVENT_KEYS: [&str; 15] = [
    "forms.event.follow",
    "forms.event.sub",
    "forms.event.resub",
    "forms.event.giftsub",
    "forms.event.giftbomb",
    "forms.event.cheer",
    "forms.event.raid",
    "forms.event.superchat",
    "forms.event.supersticker",
    "forms.event.member",
    "forms.event.gift",
    "forms.event.share",
    "forms.event.reward",
    "forms.event.powerup",
    "forms.event.announcement",
];

fn names(keys: &[&'static str]) -> Vec<String> {
    keys.iter().map(|k| tr(k).to_owned()).collect()
}

fn anchor_names() -> Vec<String> {
    names(&ANCHOR_KEYS)
}

/// The easings' names, with the current one at the end when it is CSS of its own.
fn easing_options(current: &str) -> (Vec<String>, usize) {
    let mut names = names(&EASING_KEYS);
    let at = kinds::EASINGS
        .iter()
        .position(|(_, css)| *css == current)
        .unwrap_or_else(|| {
            names.push(current.to_owned());
            names.len() - 1
        });
    (names, at)
}

fn easing_of(index: Option<u64>, old: &str) -> String {
    match index.and_then(|i| kinds::EASINGS.get(i as usize)) {
        Some((_, css)) => (*css).to_owned(),
        None => old.to_owned(),
    }
}

/// What a press of a button in the form asks the core to do.
fn press(group: &str, action: String, with_value: bool) -> Value {
    json!({"request": {"request": "StreamerzCoreAction", "domain": "overlays", "table": "overlay",
        "action": action, "itemId": group, "id": "overlay:press"}, "withValue": with_value})
}

fn button_part(key: &str, label: &str, icon: &str, press: Value, line: u32) -> Value {
    let mut p = part(key, label, json!({"kind": "button", "icon": icon}), Value::Null, line);
    p["press"] = press;
    p
}

/// The field for a setting of one of the newer kinds (`None` for the others).
pub(super) fn field(
    config: &crate::Config,
    group: &str,
    meta: &Meta,
    def: &Def,
    label: &str,
    value: &Value,
) -> Option<Value> {
    let v = if value.is_null() { &def.default } else { value };
    let text = v.as_str().unwrap_or_default().to_owned();
    let base = |widget: Value, holds: &str, shown: Value| json!({"key": def.name, "label": label, "widget": widget, "holds": holds, "value": shown});
    Some(match def.kind {
        Kind::Date => {
            let mut f = base(json!({"kind": "text"}), "a day as YYYY-MM-DD", text.into());
            let mut list = vec![
                (tr("forms.preset.today").to_owned(), kinds::day_from_today(0)),
                (tr("forms.preset.tomorrow").to_owned(), kinds::day_from_today(1)),
                (tr("forms.preset.next_week").to_owned(), kinds::day_from_today(7)),
            ];
            list.extend(own_presets(def));
            f["placeholder"] = "YYYY-MM-DD".into();
            f["presets"] = presets(list);
            f
        }
        Kind::Time => {
            let mut f = base(json!({"kind": "text"}), "a time of day as HH:MM", text.into());
            f["placeholder"] = "HH:MM".into();
            f["presets"] = presets(own_presets(def));
            f
        }
        Kind::DateTime => {
            let shown = kinds::show_moment(v.as_u64().unwrap_or(0));
            let mut f = base(
                json!({"kind": "text"}),
                "a day and time as YYYY-MM-DD HH:MM",
                shown.into(),
            );
            let list = if def.presets.is_empty() {
                vec![
                    (tr("forms.preset.in_15m").to_owned(), "+15m".to_owned()),
                    (tr("forms.preset.in_1h").to_owned(), "+1h".to_owned()),
                    (trf("forms.preset.tonight", &[("time", "20:00")]), "20:00".to_owned()),
                ]
            } else {
                own_presets(def)
            };
            f["placeholder"] = "YYYY-MM-DD HH:MM".into();
            f["presets"] = presets(list);
            f
        }
        Kind::Duration => {
            let mut f = base(json!({"kind": "text"}), "a time as h:mm:ss", def.kind.show(v).into());
            f["presets"] = presets(own_presets(def));
            f
        }
        Kind::Range => {
            let (min, max, step) = def.range();
            let pair = v.as_array().filter(|p| p.len() == 2);
            let at = |i: usize, or: f64| {
                pair.and_then(|p| p[i].as_f64())
                    .filter(|n| n.is_finite())
                    .unwrap_or(or)
                    .clamp(min, max)
            };
            let (a, b) = (at(0, min), at(1, max));
            base(
                json!({"kind": "slider", "min": min, "max": max, "step": step, "range": true}),
                &format!("two numbers from {min} to {max}"),
                json!([a.min(b), a.max(b)]),
            )
        }
        Kind::Percent => {
            let step = def.step.filter(|s| s.is_finite() && *s > 0.0).unwrap_or(1.0);
            let n = v.as_f64().unwrap_or(0.0).clamp(0.0, 100.0);
            base(
                json!({"kind": "slider", "min": 0.0, "max": 100.0, "step": step, "suffix": "%"}),
                "a number from 0 to 100",
                json!(n),
            )
        }
        Kind::Position => {
            let o = kinds::fill(Kind::Position, v);
            let anchor = kinds::ANCHORS
                .iter()
                .position(|a| o["anchor"].as_str() == Some(a))
                .unwrap_or(4);
            parts(
                def,
                label,
                vec![
                    part(
                        "anchor",
                        tr("forms.position.anchor"),
                        json!({"kind": "anchor", "options": anchor_names()}),
                        json!(anchor),
                        0,
                    ),
                    text_part("x", tr("forms.position.x"), number_text(&o["x"]), 0),
                    text_part("y", tr("forms.position.y"), number_text(&o["y"]), 0),
                ],
            )
        }
        Kind::Size => {
            let o = kinds::fill(Kind::Size, v);
            let mut f = parts(
                def,
                label,
                vec![
                    text_part("width", tr("forms.size.width"), number_text(&o["width"]), 0),
                    text_part("height", tr("forms.size.height"), number_text(&o["height"]), 0),
                    part("lock", tr("forms.size.lock"), json!({"kind": "bool"}), json!(false), 0),
                ],
            );
            // While Keep ratio is ticked, changing one side changes the other.
            f["widget"]["ratio"] = json!([0, 1, 2]);
            f
        }
        Kind::Animation => {
            let o = kinds::fill(Kind::Animation, v);
            let names = names(&ANIMATION_KEYS);
            let at = kinds::ANIMATIONS
                .iter()
                .position(|a| o["name"].as_str() == Some(a))
                .unwrap_or(1);
            let (easings, easing) = easing_options(o["easing"].as_str().unwrap_or("ease-out"));
            parts(
                def,
                label,
                vec![
                    part(
                        "name",
                        tr("forms.animation.name"),
                        json!({"kind": "pick", "options": names}),
                        json!(at),
                        0,
                    ),
                    part(
                        "easing",
                        tr("forms.animation.easing"),
                        json!({"kind": "pick", "options": easings}),
                        json!(easing),
                        0,
                    ),
                    text_part(
                        "durationMs",
                        tr("forms.animation.length"),
                        number_text(&o["durationMs"]),
                        1,
                    ),
                    text_part("delayMs", tr("forms.animation.delay"), number_text(&o["delayMs"]), 1),
                    button_part(
                        "play",
                        tr("btn.play"),
                        "play",
                        press(group, format!("play:{}", def.name), false),
                        1,
                    ),
                ],
            )
        }
        Kind::Easing => {
            let (names, at) = easing_options(&text);
            base(json!({"kind": "pick", "options": names}), "a CSS easing", json!(at))
        }
        Kind::Gradient => match kinds::gradient_parts(&text).or_else(|| text.is_empty().then(|| (90.0, vec![]))) {
            Some((angle, colors)) => {
                let color = |i: usize, or: &str| colors.get(i).cloned().unwrap_or_else(|| or.to_owned());
                let mut f = parts(
                    def,
                    label,
                    vec![
                        part(
                            "from",
                            tr("forms.gradient.from"),
                            json!({"kind": "color"}),
                            color(0, "#7c3aed").into(),
                            0,
                        ),
                        part(
                            "to",
                            tr("forms.gradient.to"),
                            json!({"kind": "color"}),
                            color(1, "#db2777").into(),
                            0,
                        ),
                        part(
                            "angle",
                            tr("forms.gradient.angle"),
                            json!({"kind": "slider", "min": 0.0, "max": 360.0, "step": 1.0, "suffix": "°"}),
                            json!(angle),
                            1,
                        ),
                    ],
                );
                // A bar drawn with the two colors, as the page will show them.
                f["widget"]["gradient"] = json!([0, 1, 2]);
                f
            }
            // CSS of its own (more colors, a radial one): as text.
            None => base(json!({"kind": "code"}), "a CSS gradient", text.into()),
        },
        Kind::TextStyle => {
            let o = kinds::fill(Kind::TextStyle, v);
            let weights = names(&WEIGHT_KEYS);
            let weight = (o["weight"].as_u64().unwrap_or(700) / 100).clamp(1, 9) - 1;
            parts(
                def,
                label,
                vec![
                    part(
                        "font",
                        tr("forms.text.font"),
                        json!({"kind": "font"}),
                        o["font"].clone(),
                        0,
                    ),
                    text_part("size", tr("forms.text.size"), number_text(&o["size"]), 0),
                    part(
                        "weight",
                        tr("forms.text.weight"),
                        json!({"kind": "pick", "options": weights}),
                        json!(weight),
                        0,
                    ),
                    part(
                        "color",
                        tr("forms.text.color"),
                        json!({"kind": "color"}),
                        o["color"].clone(),
                        1,
                    ),
                    text_part("outline", tr("forms.text.outline"), number_text(&o["outline"]), 1),
                    part(
                        "outlineColor",
                        tr("forms.text.outline_color"),
                        json!({"kind": "color"}),
                        o["outlineColor"].clone(),
                        1,
                    ),
                    part(
                        "shadow",
                        tr("forms.text.shadow"),
                        json!({"kind": "bool"}),
                        json!(o["shadow"].as_bool().unwrap_or(false)),
                        1,
                    ),
                ],
            )
        }
        Kind::Sound => {
            let o = kinds::fill(Kind::Sound, v);
            let files: Vec<Value> = meta
                .files
                .iter()
                .filter(|f| crate::groups::accepts_file("sound", &f.name))
                .map(|f| json!({"id": f.name, "label": f.name}))
                .collect();
            let mut file = part(
                "file",
                tr("forms.sound.file"),
                json!({"kind": "text"}),
                o["file"].clone(),
                0,
            );
            file["choices"] = files.into();
            file["other"] = true.into();
            file["browse"] = "file".into();
            file["placeholder"] = tr("forms.file.placeholder").into();
            parts(
                def,
                label,
                vec![
                    file,
                    part(
                        "volume",
                        tr("forms.sound.volume"),
                        json!({"kind": "slider", "min": 0.0, "max": 100.0, "step": 1.0, "suffix": "%"}),
                        json!(o["volume"].as_f64().unwrap_or(100.0)),
                        1,
                    ),
                    button_part(
                        "play",
                        tr("btn.play"),
                        "play",
                        press(group, format!("sound:{}", def.name), true),
                        1,
                    ),
                ],
            )
        }
        Kind::Event => {
            let options: Vec<Value> = kinds::EVENTS
                .iter()
                .zip(EVENT_KEYS)
                .map(|((kind, icon, _), key)| json!({"id": kind, "label": tr(key), "icon": icon}))
                .collect();
            base(
                json!({"kind": "checks", "options": options, "columns": 3}),
                "a list of kinds of events",
                if v.is_array() { v.clone() } else { json!([]) },
            )
        }
        Kind::Reward => {
            let current = match (v["platform"].as_str(), v["id"].as_str()) {
                (Some(p), Some(id)) if !id.is_empty() => crate::rewards::key(p, id),
                _ => String::new(),
            };
            let mut choices: Vec<Value> = Vec::new();
            for (platform, list) in [
                ("twitch", crate::twitch::rewards_known()),
                ("kick", crate::kick::rewards_known()),
            ] {
                for r in list.iter() {
                    let Some(id) = r["id"].as_str() else { continue };
                    let title = r["title"].as_str().unwrap_or(id);
                    choices.push(json!({"id": crate::rewards::key(platform, id),
                        "label": format!("{title} ({})", if platform == "twitch" { "Twitch" } else { "Kick" })}));
                }
            }
            let mut f = base(json!({"kind": "text"}), "one of the channel's rewards", current.into());
            f["choices"] = choices.into();
            f["strict"] = true.into();
            f
        }
        Kind::Gift => {
            let id = v["id"].as_str().unwrap_or_default().to_owned();
            let mut f = base(json!({"kind": "text"}), "one of TikTok's gifts", id.clone().into());
            f["choices"] = crate::tiktok::gifts::gallery().into();
            f["gallery"] = json!({"any": tr("forms.gift.none"), "anyTip": tr("forms.gift.none_tip"),
                "search": tr("automation.search_gifts"), "empty": tr("automation.no_gifts_match"),
                "icon": "gift", "status": crate::tiktok::gifts::gallery_status(),
                "current": (!id.is_empty()).then(|| v["name"].as_str().filter(|n| !n.is_empty())
                    .map_or_else(|| trf("automation.gift_unnamed", &[("id", &id)]), str::to_owned))});
            // The list loads as the form opens when it isn't here, as in a gift trigger's form.
            f["refresh"] = json!({"request": {"request": "StreamerzTikTokGifts", "refresh": true},
                "label": tr("btn.refresh_gifts"), "tip": tr("automation.refresh_gifts_tip"),
                "auto": crate::tiktok::gifts::wants_loading()});
            f
        }
        Kind::UserGroup => {
            let choices: Vec<Value> = config
                .groups
                .iter()
                .filter_map(|g| g.get("name").and_then(Value::as_str))
                .map(|name| json!({"id": name, "label": name}))
                .collect();
            let mut f = base(json!({"kind": "text"}), "the name of a user group", text.into());
            f["choices"] = choices.into();
            f["strict"] = true.into();
            f
        }
        Kind::Scene | Kind::Source => {
            let key = if def.kind == Kind::Scene {
                "obs.scenes"
            } else {
                "obs.sources"
            };
            let choices: Vec<Value> = crate::catalog::choices(key)
                .into_iter()
                .map(|(id, label)| json!({"id": id, "label": label}))
                .collect();
            let mut f = base(json!({"kind": "text"}), "a name in OBS", text.into());
            f["choices"] = choices.into();
            f["other"] = true.into();
            f["placeholder"] = tr(if def.kind == Kind::Scene {
                "forms.obs.scene_placeholder"
            } else {
                "forms.obs.source_placeholder"
            })
            .into();
            f
        }
        Kind::Code => {
            let mut f = base(json!({"kind": "code", "language": def.accept}), "code", text.into());
            f["placeholder"] = match def.accept.as_str() {
                "css" => ".title { color: gold; }",
                "html" => "<div class=\"box\"></div>",
                "js" => "streamerz.log('hi')",
                _ => "",
            }
            .into();
            f
        }
        Kind::Json => base(
            json!({"kind": "code", "language": "json"}),
            "any value as JSON",
            serde_json::to_string_pretty(v).unwrap_or_default().into(),
        ),
        Kind::Button => {
            let mut f = base(
                json!({"kind": "button", "icon": "zap"}),
                "nothing: a button",
                Value::Null,
            );
            f["press"] = press(group, format!("press:{}", def.name), false);
            f["buttonLabel"] = tr("forms.press").into();
            f
        }
        Kind::Hidden => {
            let mut f = base(json!({"kind": "text"}), "a value kept but not shown", "".into());
            f["hidden"] = true.into();
            f
        }
        _ => return None,
    })
}

fn number(o: &Value, key: &str, label: &str) -> Result<Value, String> {
    let text = o[key].as_str().unwrap_or_default();
    if text.trim().is_empty() {
        return Ok(json!(0));
    }
    crate::fmt::parse_number(text.trim())
        .filter(|n| n.is_finite())
        .map(kinds::tidy)
        .ok_or_else(|| trf("forms.not_a_number", &[("label", label)]))
}

fn whole(o: &Value, key: &str, label: &str) -> Result<Value, String> {
    match number(o, key, label)?.as_f64() {
        Some(n) if n >= 0.0 => Ok(json!(n.round() as u64)),
        _ => Err(trf("forms.not_a_number", &[("label", label)])),
    }
}

fn snap(n: f64, (min, max, step): (f64, f64, f64)) -> Value {
    kinds::tidy((((n - min) / step).round() * step + min).clamp(min, max))
}

/// The value of a setting of one of the newer kinds from what its field sent; `None` for the
/// others. `old` is the value it had. A file of this computer is brought in through `bring_in`.
pub(super) fn read(
    def: &Def,
    sent: &Value,
    old: &Value,
    bring_in: &mut dyn FnMut(&str) -> Result<String, String>,
) -> Option<Result<Value, String>> {
    let label = def.label.as_str();
    let bad = || trf("forms.invalid_value", &[("label", label)]);
    let text = || sent.as_str().ok_or_else(bad);
    let invalid =
        |e: &dyn std::fmt::Display| trf("forms.setting_invalid", &[("label", label), ("error", &e.to_string())]);
    let typed = |t: &str| def.parse(t).map_err(|e| invalid(&e));
    Some(match def.kind {
        Kind::Date | Kind::Time | Kind::DateTime | Kind::Duration | Kind::UserGroup | Kind::Scene | Kind::Source => {
            text().and_then(typed)
        }
        Kind::Code => text().map(Value::from),
        Kind::Json => text().and_then(|t| serde_json::from_str(t).map_err(|e| invalid(&e))),
        Kind::Hidden | Kind::Button => Ok(old.clone()),
        Kind::Range => sent
            .as_array()
            .filter(|p| p.len() == 2)
            .and_then(|p| Some((p[0].as_f64()?, p[1].as_f64()?)))
            .map(|(a, b)| json!([snap(a.min(b), def.range()), snap(a.max(b), def.range())]))
            .ok_or_else(bad),
        Kind::Percent => {
            let step = def.step.filter(|s| s.is_finite() && *s > 0.0).unwrap_or(1.0);
            sent.as_f64().map(|n| snap(n, (0.0, 100.0, step))).ok_or_else(bad)
        }
        Kind::Easing => Ok(easing_of(sent.as_u64(), old.as_str().unwrap_or("ease")).into()),
        Kind::Event => Some(sent.clone()).filter(|v| def.accepts(v)).ok_or_else(bad),
        Kind::Position => (|| {
            let anchor = sent["anchor"]
                .as_u64()
                .and_then(|i| kinds::ANCHORS.get(i as usize))
                .ok_or_else(bad)?;
            Ok(
                json!({"anchor": anchor, "x": number(sent, "x", tr("forms.position.x"))?,
                "y": number(sent, "y", tr("forms.position.y"))?}),
            )
        })(),
        Kind::Size => (|| {
            Ok(json!({"width": whole(sent, "width", tr("forms.size.width"))?,
                "height": whole(sent, "height", tr("forms.size.height"))?}))
        })(),
        Kind::Animation => (|| {
            let name = sent["name"]
                .as_u64()
                .and_then(|i| kinds::ANIMATIONS.get(i as usize))
                .ok_or_else(bad)?;
            let old_easing = old["easing"].as_str().unwrap_or("ease-out");
            Ok(
                json!({"name": name, "durationMs": whole(sent, "durationMs", tr("forms.animation.length"))?,
                "delayMs": whole(sent, "delayMs", tr("forms.animation.delay"))?,
                "easing": easing_of(sent["easing"].as_u64(), old_easing)}),
            )
        })(),
        Kind::Gradient => match sent {
            Value::String(css) => Ok(css.trim().into()),
            Value::Object(_) => {
                let color = |k: &str| {
                    sent[k]
                        .as_str()
                        .filter(|c| crate::groups::is_color(c))
                        .map(str::to_owned)
                };
                match (color("from"), color("to"), sent["angle"].as_f64()) {
                    (Some(from), Some(to), Some(angle)) => Ok(kinds::gradient_css(angle, &[from, to]).into()),
                    _ => Err(bad()),
                }
            }
            _ => Err(bad()),
        },
        Kind::TextStyle => (|| {
            let color = |k: &str| {
                sent[k]
                    .as_str()
                    .filter(|c| crate::groups::is_color(c))
                    .map(Value::from)
                    .ok_or_else(bad)
            };
            Ok(json!({"font": sent["font"].as_str().unwrap_or_default(),
                "size": number(sent, "size", tr("forms.text.size"))?,
                "weight": (sent["weight"].as_u64().unwrap_or(6).min(8) + 1) * 100,
                "color": color("color")?, "outline": number(sent, "outline", tr("forms.text.outline"))?,
                "outlineColor": color("outlineColor")?, "shadow": sent["shadow"].as_bool().unwrap_or(false)}))
        })(),
        Kind::Sound => (|| {
            let typed = sent["file"].as_str().unwrap_or_default().trim().trim_matches('"');
            let file = bring_in(typed)?;
            let volume = sent["volume"]
                .as_f64()
                .map(|n| snap(n, (0.0, 100.0, 1.0)))
                .ok_or_else(bad)?;
            Ok(json!({"file": file, "volume": volume}))
        })(),
        Kind::Reward => (|| {
            let key = text()?;
            if key.is_empty() {
                return Ok(json!({}));
            }
            let (platform, id) = key
                .split_once(':')
                .filter(|(p, _)| matches!(*p, "twitch" | "kick"))
                .ok_or_else(bad)?;
            let known = if platform == "twitch" {
                crate::twitch::rewards_known()
            } else {
                crate::kick::rewards_known()
            };
            let title = known
                .iter()
                .find(|r| r["id"].as_str() == Some(id))
                .and_then(|r| r["title"].as_str())
                .map(str::to_owned)
                .or_else(|| {
                    (old["id"].as_str() == Some(id)).then(|| old["title"].as_str().unwrap_or_default().to_owned())
                })
                .unwrap_or_default();
            Ok(json!({"platform": platform, "id": id, "title": title}))
        })(),
        Kind::Gift => (|| {
            let id = text()?.trim();
            if id.is_empty() {
                return Ok(json!({}));
            }
            Ok(crate::tiktok::gifts::find(id)
                .or_else(|| (old["id"].as_str() == Some(id)).then(|| old.clone()))
                .unwrap_or_else(|| json!({"id": id, "name": "", "coins": 0, "picture": ""})))
        })(),
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn def(kind: Kind) -> Def {
        Def {
            name: "s".into(),
            label: "S".into(),
            kind,
            default: kinds::blank(kind),
            ..Def::default()
        }
    }

    /// What the field shows, sent back unchanged, is the value again.
    fn round_trip(kind: Kind, value: Value) -> Value {
        let config = crate::Config::default();
        let d = def(kind);
        let f = field(&config, "g", &Meta::default(), &d, "S", &value).unwrap();
        let mut keep = |t: &str| Ok(t.to_owned());
        read(&d, &f["value"], &value, &mut keep).unwrap().unwrap()
    }

    #[test]
    fn each_kind_comes_back_as_it_was_shown() {
        for (kind, value) in [
            (Kind::Position, json!({"anchor": "bottom-right", "x": 24, "y": -8.5})),
            (Kind::Size, json!({"width": 1280, "height": 720})),
            (
                Kind::Animation,
                json!({"name": "pop", "durationMs": 350, "delayMs": 100, "easing": "ease-in"}),
            ),
            (
                Kind::Animation,
                json!({"name": "fade", "durationMs": 1, "delayMs": 0, "easing": "cubic-bezier(1, 0, 0, 1)"}),
            ),
            (Kind::TextStyle, kinds::blank(Kind::TextStyle)),
            (Kind::Sound, json!({"file": "ding.mp3", "volume": 40})),
            (Kind::Gradient, json!("linear-gradient(45deg, #ff0000, #00ff00)")),
            (Kind::Gradient, json!("radial-gradient(red, blue)")),
            (Kind::Range, json!([10, 20])),
            (Kind::Percent, json!(35)),
            (Kind::Event, json!(["follow", "raid"])),
            (Kind::Time, json!("20:30")),
            (Kind::Date, json!("2030-01-02")),
            (Kind::DateTime, json!(kinds::moment("2030-01-02 20:30").unwrap())),
            (Kind::Code, json!(".a { color: red }")),
            (Kind::Json, json!({"a": [1, 2]})),
            (Kind::Easing, json!("ease-in-out")),
            (Kind::Reward, json!({})),
            (Kind::Gift, json!({})),
        ] {
            assert_eq!(round_trip(kind, value.clone()), value, "{}", kind.name());
        }
    }

    #[test]
    fn the_shown_names_follow_the_lists() {
        let ends = |keys: &[&str], names: Vec<String>| {
            keys.len() == names.len() && keys.iter().zip(names).all(|(k, n)| k.ends_with(&n))
        };
        assert!(ends(
            &ANCHOR_KEYS,
            kinds::ANCHORS.iter().map(|a| format!(".{a}")).collect()
        ));
        assert!(ends(
            &ANIMATION_KEYS,
            kinds::ANIMATIONS.iter().map(|a| format!(".{a}")).collect()
        ));
        assert!(ends(
            &EVENT_KEYS,
            kinds::EVENTS.iter().map(|e| format!(".{}", e.0)).collect()
        ));
        let easings = kinds::EASINGS
            .iter()
            .map(|(n, _)| format!(".{}", n.to_ascii_lowercase().replace(' ', "_")));
        assert!(ends(&EASING_KEYS, easings.collect()));
        assert!(ends(&WEIGHT_KEYS, (1..=9).map(|w| format!(".w{w}00")).collect()));
    }

    #[test]
    fn parts_are_checked_and_buttons_say_what_they_do() {
        let d = def(Kind::Size);
        let mut keep = |t: &str| Ok(t.to_owned());
        let wrong = read(&d, &json!({"width": "wide", "height": "2"}), &Value::Null, &mut keep).unwrap();
        assert!(wrong.is_err());
        let config = crate::Config::default();
        let b = field(
            &config,
            "Clock",
            &Meta::default(),
            &def(Kind::Button),
            "Go",
            &Value::Null,
        )
        .unwrap();
        assert_eq!(b["press"]["request"]["action"], "press:s");
        assert_eq!(b["press"]["request"]["itemId"], "Clock");
        let h = field(
            &config,
            "Clock",
            &Meta::default(),
            &def(Kind::Hidden),
            "H",
            &json!({"x": 1}),
        )
        .unwrap();
        assert_eq!(h["hidden"], true);
        assert_eq!(
            read(&def(Kind::Hidden), &json!(""), &json!({"x": 1}), &mut keep)
                .unwrap()
                .unwrap(),
            json!({"x": 1})
        );
        // A date offers today, tomorrow and next week.
        let date = field(&config, "Clock", &Meta::default(), &def(Kind::Date), "D", &json!("")).unwrap();
        assert_eq!(date["presets"][0]["value"], kinds::day_from_today(0));
    }

    fn sent(d: &Def, value: Value, old: Value) -> Result<Value, String> {
        let mut keep = |t: &str| Ok(t.to_owned());
        read(d, &value, &old, &mut keep).unwrap()
    }

    #[test]
    fn sliders_snap_to_their_step_and_stay_in_range() {
        let mut d = def(Kind::Range);
        d.min = Some(0.0);
        d.max = Some(10.0);
        d.step = Some(0.5);
        // Sent backwards, off the step and past the end: ordered, snapped and clamped.
        assert_eq!(sent(&d, json!([12, 2.3]), Value::Null), Ok(json!([2.5, 10])));
        assert!(sent(&d, json!([1]), Value::Null).is_err());
        assert!(sent(&d, json!(["a", 2]), Value::Null).is_err());
        // The field shows a stored pair clamped too, the default ends when there is none.
        let config = crate::Config::default();
        let f = field(&config, "g", &Meta::default(), &d, "S", &json!([-5, 4])).unwrap();
        assert_eq!(f["value"], json!([0.0, 4.0]));
        let f = field(&config, "g", &Meta::default(), &d, "S", &json!("x")).unwrap();
        assert_eq!(f["value"], json!([0.0, 10.0]));
        let mut p = def(Kind::Percent);
        p.step = Some(5.0);
        assert_eq!(sent(&p, json!(42), Value::Null), Ok(json!(40)));
        assert_eq!(sent(&p, json!(130), Value::Null), Ok(json!(100)));
        assert!(sent(&p, json!("half"), Value::Null).is_err());
        // A step of zero or below falls back to 1, never a division by zero.
        p.step = Some(0.0);
        assert_eq!(sent(&p, json!(33.4), Value::Null), Ok(json!(33)));
    }

    #[test]
    fn parts_are_read_back_into_their_values() {
        let d = def(Kind::Gradient);
        assert_eq!(
            sent(
                &d,
                json!({"from": "#ff0000", "to": "#00ff00", "angle": 45}),
                Value::Null
            ),
            Ok(json!(kinds::gradient_css(45.0, &["#ff0000".into(), "#00ff00".into()])))
        );
        assert!(sent(&d, json!({"from": "red;}", "to": "#00ff00", "angle": 45}), Value::Null).is_err());
        assert!(sent(&d, json!(5), Value::Null).is_err());
        assert_eq!(
            sent(&d, json!("  radial-gradient(red, blue) "), Value::Null),
            Ok(json!("radial-gradient(red, blue)"))
        );
        let t = def(Kind::TextStyle);
        let style = json!({"font": "Inter", "size": "24", "weight": 99, "color": "#ffffff",
            "outline": "", "outlineColor": "#000000", "shadow": true});
        let styled = sent(&t, style, Value::Null).unwrap();
        assert_eq!(styled["weight"], 900, "a weight past the list is the heaviest");
        assert_eq!(styled["size"], 24);
        assert_eq!(styled["outline"], 0, "an empty box is 0");
        assert!(sent(&t, json!({"color": "nope", "outlineColor": "#000000"}), Value::Null).is_err());
        let a = def(Kind::Animation);
        assert!(sent(&a, json!({"name": 99, "durationMs": "1", "delayMs": "0"}), Value::Null).is_err());
        assert!(
            sent(&a, json!({"name": 0, "durationMs": "-5", "delayMs": "0"}), Value::Null).is_err(),
            "no negative lengths"
        );
        // An easing past the list keeps the old one.
        let kept = sent(
            &a,
            json!({"name": 0, "durationMs": "1", "delayMs": "0", "easing": 99}),
            json!({"easing": "steps(3)"}),
        );
        assert_eq!(kept.unwrap()["easing"], "steps(3)");
        let s = def(Kind::Sound);
        let mut brought = Vec::new();
        let mut bring = |t: &str| {
            brought.push(t.to_owned());
            Ok(format!("sounds/{t}"))
        };
        let got = read(
            &s,
            &json!({"file": " \"ding.mp3\" ", "volume": 40.4}),
            &Value::Null,
            &mut bring,
        )
        .unwrap()
        .unwrap();
        assert_eq!(got, json!({"file": "sounds/ding.mp3", "volume": 40}));
        assert_eq!(brought, ["ding.mp3"], "the quotes a pasted path has are dropped");
    }

    #[test]
    fn a_reward_or_gift_is_read_from_its_id_and_bad_text_is_refused() {
        let r = def(Kind::Reward);
        assert_eq!(sent(&r, json!(""), Value::Null), Ok(json!({})));
        assert!(sent(&r, json!("orkut:1"), Value::Null).is_err());
        assert!(sent(&r, json!("no-colon"), Value::Null).is_err());
        assert!(sent(&r, json!(7), Value::Null).is_err());
        // An id no longer listed keeps the title it had.
        let old = json!({"platform": "twitch", "id": "gone-id", "title": "Hydrate"});
        assert_eq!(
            sent(&r, json!("twitch:gone-id"), old),
            Ok(json!({"platform": "twitch", "id": "gone-id", "title": "Hydrate"}))
        );
        let g = def(Kind::Gift);
        assert_eq!(sent(&g, json!("  "), Value::Null), Ok(json!({})));
        let old = json!({"id": "zz-test-gift", "name": "Rose", "coins": 1, "picture": ""});
        assert_eq!(sent(&g, json!("zz-test-gift"), old.clone()), Ok(old));
        let j = def(Kind::Json);
        assert!(sent(&j, json!("{nope"), Value::Null).is_err());
        assert_eq!(sent(&j, json!("[1, 2]"), Value::Null), Ok(json!([1, 2])));
        let e = def(Kind::Event);
        assert!(sent(&e, json!(["follow", "not-an-event"]), Value::Null).is_err());
    }
}
