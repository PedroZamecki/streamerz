//! Toolkit-independent automation metadata used by every application adapter.

pub mod catalog;
pub mod edits;
pub mod entities;
pub mod labels;
pub mod pick_icons;
pub mod schema;

use serde_json::{json, Value};

/// Describe a stored automation type without starting a window or connecting to a service.
/// The same schema drives frontend forms and MCP, including preserved imported fields.
pub fn describe(kind: &str, code: u32) -> Result<Value, String> {
    if !matches!(kind, "subaction" | "trigger") {
        return Err(format!("unknown automation kind {kind}"));
    }
    let code = u64::from(code);
    let sub = kind == "subaction";
    let table = if sub {
        crate::types::SUB_ACTIONS
    } else {
        crate::types::TRIGGERS
    };
    let name = crate::types::name(table, code as u32).ok_or(format!("no {} {code}", kind))?;
    use self::{catalog as picker, schema as forms};
    let cat = if sub {
        String::new()
    } else {
        picker::trigger_categorize(name)
    };
    let (about, fields, new) = if sub {
        (
            picker::sub_action_description(code as u32, name, &cat),
            forms::sub_fields(code),
            Value::Object(crate::edit::new_sub(code)),
        )
    } else {
        let title = picker::trigger_clean_title(name);
        (
            picker::trigger_description(code as u32, name, &title, &cat),
            forms::trigger_fields(code),
            Value::Object(crate::edit::new_trigger(code as u32)),
        )
    };
    let form_state = match fields {
        Some([]) => "no_settings",
        Some(_) => "form",
        None => "json",
    };
    let fields: Vec<Value> = fields
        .unwrap_or(&[])
        .iter()
        .filter(|f| !f.0.is_empty())
        .map(|(k, l, w)| json!({"key": k, "label": forms::field_label(l), "holds": w.about()}))
        .collect();

    let docs = crate::docs::entry(kind, code as u32)
        .map(|e| e.text)
        .unwrap_or_default();
    Ok(
        json!({"code": code, "name": name, "about": about, "fields": fields, "new": new, "docs": docs,
    "formState": form_state, "available": crate::docs::available(kind, code as u32),
    "preservedFields": if sub { forms::preserved_fields(code) } else { &[][..] },
    "note": if form_state == "json" { "Advanced JSON editing; see the built-in reference for limitations." } else { "" }}),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_stored_type_has_a_headless_description() {
        for (kind, table) in [
            ("trigger", crate::types::TRIGGERS),
            ("subaction", crate::types::SUB_ACTIONS),
        ] {
            for &(code, name) in table {
                let value = describe(kind, code).unwrap();
                assert_eq!(value["name"], name);
                assert_eq!(value["new"]["type"], code);
                assert!(value["fields"].is_array());
                assert!(matches!(
                    value["formState"].as_str(),
                    Some("form" | "no_settings" | "json")
                ));
            }
        }
    }

    #[test]
    fn invalid_types_are_rejected_instead_of_describing_a_different_kind() {
        assert!(describe("invalid", 10).is_err());
        assert!(describe("subaction", u32::MAX).is_err());
        assert!(describe("trigger", u32::MAX).is_err());
    }
}
