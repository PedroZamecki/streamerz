//! The core supplies the active translation catalog once at renderer startup.

use std::collections::HashMap;
use std::sync::OnceLock;

static CATALOG: OnceLock<HashMap<String, String>> = OnceLock::new();

pub fn init(value: serde_json::Value) -> Result<(), String> {
    let catalog = serde_json::from_value(value).map_err(|error| format!("invalid translation catalog: {error}"))?;
    CATALOG
        .set(catalog)
        .map_err(|_| "translation catalog already installed".into())
}

pub fn tr(key: &'static str) -> &'static str {
    CATALOG
        .get()
        .and_then(|catalog| catalog.get(key))
        .map_or(key, String::as_str)
}
