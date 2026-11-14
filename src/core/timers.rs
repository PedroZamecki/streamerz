//! Timer authoring keeps action links and timer settings in one atomic core operation.
use crate::{automation::entities, engine::Engine, i18n::tr, model::Obj};
use serde_json::{json, Value};
use std::rc::Rc;

const LINKS: &str = "_linkedActions";

pub fn linked(config: &crate::Config, timer: &str) -> Vec<String> {
    config
        .actions
        .actions
        .iter()
        .filter(|a| {
            a.triggers.iter().any(|t| {
                t.get("type").and_then(Value::as_u64) == Some(701)
                    && t.get("timerId").and_then(Value::as_str) == Some(timer)
            })
        })
        .map(|a| a.id.clone())
        .collect()
}

pub fn decorate(config: &crate::Config, mut form: Value) -> Value {
    let id = form["item"]["id"].as_str().unwrap_or_default();
    let selected = linked(config, id);
    form["item"][LINKS] = json!(selected);
    form["fields"].as_array_mut().unwrap().push(json!({"key": LINKS,
        "label": tr("automation.timer_actions"), "holds": "action ids",
        "widget": {"kind": "checks", "options": config.actions.actions.iter().map(|a|
            json!({"id": a.id, "label": a.name})).collect::<Vec<_>>()}, "value": selected}));
    for field in form["fields"].as_array_mut().unwrap() {
        if field["key"] == "upperInterval" {
            field["showIf"] = json!({"field": 4, "equals": true});
        }
    }
    form["okLabel"] = tr("btn.save").into();
    form["notes"] = json!([tr("automation.timer_draft_hint")]);
    if form["item"]["id"].as_str().unwrap_or_default().is_empty() {
        entities::activation_buttons(&mut form);
    }
    form
}

pub fn draft(form: Value, request: &Value) -> Result<Obj, String> {
    if request["json"].is_string() {
        return entities::draft_with(form, entities::TIMERS_FIELDS, request);
    }
    let mut real = request.clone();
    let changes = request["changes"].as_array().ok_or("changes: an array")?;
    let index = entities::TIMERS_FIELDS.len() as u64;
    let selected: Vec<_> = changes.iter().filter(|c| c["field"].as_u64() == Some(index)).collect();
    if selected.len() > 1 {
        return Err(tr("forms.invalid_field").into());
    }
    let mut ids = form["item"][LINKS].clone();
    if let Some(change) = selected.first() {
        ids = change["value"].clone();
        if !ids.as_array().is_some_and(|ids| ids.iter().all(Value::is_string)) {
            return Err(tr("forms.invalid_field").into());
        }
    }
    real["changes"] = json!(changes
        .iter()
        .filter(|c| c["field"].as_u64() != Some(index))
        .collect::<Vec<_>>());
    let mut item = entities::draft_with(form, entities::TIMERS_FIELDS, &real)?;
    item.insert(LINKS.into(), ids);
    Ok(item)
}

pub fn save(engine: &Rc<Engine>, mut item: Obj) -> Result<Value, String> {
    let config = engine.config();
    let id = item
        .get("id")
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
        .map(str::to_owned)
        .unwrap_or_else(crate::engine::uuid);
    let selected = item.shift_remove(LINKS).unwrap_or_else(|| json!(linked(&config, &id)));
    let selected = selected
        .as_array()
        .ok_or_else(|| tr("forms.invalid_field").to_owned())?;
    if selected.iter().any(|id| {
        !config
            .actions
            .actions
            .iter()
            .any(|a| Some(a.id.as_str()) == id.as_str())
    }) {
        return Err(tr("actions.item_gone").into());
    }
    let interval = item.get("interval").and_then(Value::as_u64).unwrap_or(0);
    if interval == 0
        || (item.get("randomInterval") == Some(&json!(true))
            && item.get("upperInterval").and_then(Value::as_u64).unwrap_or(0) < interval)
    {
        return Err(tr("automation.timer_interval_invalid").into());
    }
    if item.get("enabled") == Some(&json!(true)) && selected.is_empty() {
        return Err(tr("automation.timer_action_required").into());
    }
    drop(config);
    item.insert("id".into(), id.clone().into());
    engine.change(|config| {
        for action in &mut config.actions.actions {
            let matching = |t: &Obj| {
                t.get("type").and_then(Value::as_u64) == Some(701)
                    && t.get("timerId").and_then(Value::as_str) == Some(id.as_str())
            };
            if selected.iter().any(|v| v.as_str() == Some(action.id.as_str())) {
                if !action.triggers.iter().any(matching) {
                    let mut trigger = crate::edit::new_trigger(701);
                    trigger.insert("timerId".into(), id.clone().into());
                    action.triggers.push(trigger);
                }
            } else {
                action.triggers.retain(|t| !matching(t));
            }
        }
        match config
            .timers
            .iter_mut()
            .find(|t| t.get("id").and_then(Value::as_str) == Some(id.as_str()))
        {
            Some(old) => *old = item,
            None => config.timers.push(item),
        }
    });
    engine.timers();
    Ok(json!({"saved": id}))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn timer_form_edits_parse_durations_and_reject_unlinked_activation() {
        crate::engine::tests::run(async {
            let engine = crate::engine::tests::engine(json!({"actions":[]}));
            let form = entities::form(&engine.config(), "timer", "").unwrap();
            assert_eq!(form["fields"][5]["showIf"], json!({"field":4,"equals":true}));
            let draft = entities::draft_of(
                form,
                "timer",
                &json!({"changes":[
                {"field":3,"value":"5 m"}, {"field":1,"value":true}]}),
            )
            .unwrap();
            assert_eq!(draft["interval"], 300);
            assert!(save(&engine, draft).is_err());
            assert!(engine.config().timers.is_empty());
        });
    }
    #[test]
    fn timer_links_are_atomic_and_preserve_other_triggers() {
        crate::engine::tests::run(async {
            let engine = crate::engine::tests::engine(json!({"actions": [
                {"id":"a","name":"A","triggers":[{"id":"keep","type":701,"timerId":"other","custom":7}]},
                {"id":"b","name":"B","triggers":[]}]}));
            let mut timer = entities::timer_default().as_object().unwrap().clone();
            timer.insert("id".into(), json!("timer"));
            timer.insert(LINKS.into(), json!(["a", "b"]));
            save(&engine, timer.clone()).unwrap();
            assert_eq!(linked(&engine.config(), "timer"), vec!["a", "b"]);
            timer.insert(LINKS.into(), json!(["missing"]));
            assert!(save(&engine, timer.clone()).is_err());
            assert_eq!(linked(&engine.config(), "timer"), vec!["a", "b"]);
            timer.insert(LINKS.into(), json!(["b"]));
            save(&engine, timer).unwrap();
            assert_eq!(linked(&engine.config(), "timer"), vec!["b"]);
            assert_eq!(engine.config().actions.actions[0].triggers[0]["custom"], 7);
            assert!(!engine.config().timers[0].contains_key(LINKS));
        });
    }
}
