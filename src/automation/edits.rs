//! Parse form edits in the core. Frontends send changed fields, never persistence rules.

use super::schema::{field_label, parse_duration_field, F, W};
use crate::model::Obj;
use serde_json::{json, Value};

pub fn shown(widget: &W, value: Option<&Value>) -> Value {
    match widget {
        W::Bool => Value::Bool(value.and_then(Value::as_bool).unwrap_or(false)),
        W::Bit(bit) => Value::Bool(value.and_then(Value::as_u64).unwrap_or(0) & bit != 0),
        W::Pick(_) => value.cloned().unwrap_or(json!(0)),
        W::Duration(unit) => value
            .and_then(Value::as_u64)
            .map(|number| Value::String(crate::groups::format_duration(number.saturating_mul(*unit))))
            .unwrap_or_else(|| text(value).into()),
        W::Lines => value
            .and_then(Value::as_array)
            .map(|values| {
                values
                    .iter()
                    .map(|value| text(Some(value)))
                    .collect::<Vec<_>>()
                    .join("\n")
            })
            .unwrap_or_default()
            .into(),
        W::Map => value
            .and_then(Value::as_object)
            .map(|values| {
                values
                    .iter()
                    .map(|(key, value)| format!("{key}={}", text(Some(value))))
                    .collect::<Vec<_>>()
                    .join("\n")
            })
            .unwrap_or_default()
            .into(),
        _ => text(value).replace("\r\n", "\n").into(),
    }
}

fn text(value: Option<&Value>) -> String {
    match value {
        None | Some(Value::Null) => String::new(),
        Some(Value::String(value)) => value.clone(),
        Some(value) => value.to_string(),
    }
}

pub fn apply(fields: &[F], original: &Obj, changes: &[Value]) -> Result<Obj, String> {
    let mut draft = original.clone();
    let mut seen = std::collections::HashSet::with_capacity(changes.len());
    for change in changes {
        let index = change["field"]
            .as_u64()
            .and_then(|field| usize::try_from(field).ok())
            .ok_or_else(|| crate::i18n::tr("forms.invalid_field").to_owned())?;
        if !seen.insert(index) {
            return Err(crate::i18n::tr("forms.invalid_field").into());
        }
        let field = fields
            .get(index)
            .ok_or_else(|| crate::i18n::tr("forms.invalid_field").to_owned())?;
        let (key, label, widget) = field;
        if key.is_empty() {
            return Err(crate::i18n::tr("forms.invalid_field").into());
        }
        let value = &change["value"];
        // Unchanged controls must keep missing fields, numeric types and imported unknown data.
        if *value == shown(widget, original.get(*key)) {
            continue;
        }
        let invalid = || crate::i18n::trf("forms.invalid_value", &[("label", field_label(label))]);
        let input = || value.as_str().ok_or_else(invalid);
        let parsed = match widget {
            W::Bool => value.as_bool().ok_or_else(invalid)?.into(),
            W::Bit(bit) => {
                let enabled = value.as_bool().ok_or_else(invalid)?;
                let flags = draft.get(*key).and_then(Value::as_u64).unwrap_or(0) & !bit;
                (if enabled { flags | bit } else { flags }).into()
            }
            W::Int => {
                let input = input()?.trim();
                if input.is_empty() {
                    json!(0)
                } else {
                    input
                        .parse::<i64>()
                        .map(Value::from)
                        .map_err(|_| crate::i18n::trf("forms.whole_number", &[("label", field_label(label))]))?
                }
            }
            W::Num => {
                let input = input()?.trim();
                let number = if input.is_empty() {
                    0.0
                } else {
                    input
                        .parse::<f64>()
                        .map_err(|_| crate::i18n::trf("forms.number", &[("label", field_label(label))]))?
                };
                serde_json::Number::from_f64(number)
                    .map(Value::Number)
                    .ok_or_else(invalid)?
            }
            W::Pick(options) => {
                let index = value
                    .as_u64()
                    .filter(|index| *index < options.len() as u64)
                    .ok_or_else(invalid)?;
                index.into()
            }
            W::Duration(unit) => parse_duration_field(input()?, *unit)?,
            W::Multi => input()?.replace("\r\n", "\n").replace('\n', "\r\n").into(),
            W::Lines => input()?.lines().map(Value::from).collect(),
            W::Map => Value::Object(
                input()?
                    .lines()
                    .filter_map(|line| line.split_once('='))
                    .map(|(key, value)| (key.trim().to_owned(), value.into()))
                    .filter(|(key, _)| !key.is_empty())
                    .collect(),
            ),
            W::Expression(_) | W::Suggest(_) | W::Entity(_) | W::ActionGroup | W::CommandGroup => {
                input()?.trim().into()
            }
            W::Action
            | W::Queue
            | W::Timer
            | W::FileWatcher
            | W::FileTail
            | W::Command
            | W::UserGroup
            | W::Client
            | W::Server
            | W::ObsConnection
            | W::SlobsConnection
            | W::Sub
            | W::Trigger => {
                let input = input()?.trim();
                if input.is_empty() {
                    draft.shift_remove(*key);
                    continue;
                }
                input.into()
            }
            W::Text | W::Secret => input()?.into(),
            W::MouseButton => value.as_u64().filter(|button| *button <= 5).ok_or_else(invalid)?.into(),
            W::Keys => {
                let code = value["key"].as_i64().filter(|code| *code != 0).ok_or_else(invalid)?;
                let modifiers = value["modifiers"].as_u64().ok_or_else(invalid)?;
                draft.insert("modifierMask".into(), modifiers.into());
                code.into()
            }
            W::Access(_) => return Err(crate::i18n::tr("forms.invalid_field").into()),
        };
        match draft.get_mut(*key) {
            Some(value) => *value = parsed,
            None => {
                let at = draft
                    .keys()
                    .position(|key| ["subActions", "id"].contains(&key.as_str()))
                    .unwrap_or(draft.len());
                draft.shift_insert(at, (*key).into(), parsed);
            }
        }
    }
    Ok(draft)
}

#[cfg(test)]
mod tests {
    use super::*;
    use W::*;

    #[test]
    fn edits_preserve_unknown_fields_missing_defaults_and_other_flags() {
        let fields: &[F] = &[
            ("name", "Name", Text),
            ("enabled", "Enabled", Bool),
            ("sources", "Twitch", Bit(1)),
            ("sources", "Kick", Bit(2)),
            ("count", "Count", Int),
        ];
        let original = json!({"id": "original", "name": "name", "sources": 8, "count": 7, "unknown": {"keep": true}})
            .as_object()
            .unwrap()
            .clone();
        let changed = apply(
            fields,
            &original,
            &[
                json!({"field":0,"value":"edited"}),
                json!({"field":1,"value":false}),
                json!({"field":2,"value":true}),
                json!({"field":3,"value":true}),
                json!({"field":4,"value":"7"}),
            ],
        )
        .unwrap();
        assert_eq!(changed["sources"], 11);
        assert!(!changed.contains_key("enabled"));
        assert_eq!(changed["count"], original["count"]);
        assert_eq!(changed["unknown"], original["unknown"]);
        assert_eq!(original["name"], "name");
    }

    #[test]
    fn validation_failure_is_atomic_and_does_not_accept_non_finite_numbers() {
        let fields: &[F] = &[
            ("name", "Name", Text),
            ("count", "Count", Int),
            ("amount", "Amount", Num),
        ];
        let original = json!({"name":"before","count":1}).as_object().unwrap().clone();
        assert!(apply(
            fields,
            &original,
            &[json!({"field":0,"value":"after"}), json!({"field":1,"value":"bad"})]
        )
        .is_err());
        assert_eq!(original["name"], "before");
        assert!(apply(fields, &original, &[json!({"field":2,"value":"NaN"})]).is_err());
        assert!(apply(fields, &original, &[json!({"field":99,"value":"bad"})]).is_err());
        assert!(apply(
            fields,
            &original,
            &[json!({"field":0,"value":"after"}), json!({"field":0,"value":"before"})]
        )
        .is_err());
    }
}
