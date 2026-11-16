//! Structured frontend error type matching the backend's `StreamerzError` contract.

use crate::i18n::tr;
use serde_json::Value;
use std::fmt;

fn tr_or(key: &'static str, fallback: &'static str) -> String {
    let translated = tr(key);
    if translated == key {
        fallback.to_owned()
    } else {
        translated.to_owned()
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ErrorAction {
    pub id: String,
    pub kind: String,
    pub target: String,
    pub label: String,
}

impl ErrorAction {
    pub fn from_value(value: &Value) -> Option<Self> {
        let obj = value.as_object()?;
        let id = obj.get("id").and_then(Value::as_str).unwrap_or_default().trim();
        let kind = obj.get("kind").and_then(Value::as_str).unwrap_or_default().trim();
        let target = obj.get("target").and_then(Value::as_str).unwrap_or_default().trim();
        let label = obj.get("label").and_then(Value::as_str).unwrap_or_default().trim();
        if kind.is_empty() || label.is_empty() {
            return None;
        }
        Some(Self {
            id: id.to_owned(),
            kind: kind.to_owned(),
            target: target.to_owned(),
            label: label.to_owned(),
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StreamerzError {
    pub code: String,
    pub title: String,
    pub description: String,
    pub recommendation: String,
    pub recommendations: Vec<String>,
    pub unexpected: bool,
    pub detail: Option<String>,
    pub action: Option<ErrorAction>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ActiveError {
    pub key: String,
    pub timestamp: i64,
    pub error: StreamerzError,
}

impl ActiveError {
    pub fn from_value(value: &Value) -> Option<Self> {
        let key = value.get("key").and_then(Value::as_str)?.trim().to_owned();
        if key.is_empty() {
            return None;
        }
        let timestamp = value.get("timestamp").and_then(Value::as_i64).unwrap_or(0);
        let error = StreamerzError::from_value(value)?;
        Some(Self { key, timestamp, error })
    }

    pub fn list_from_value(value: &Value) -> Vec<Self> {
        value
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(Self::from_value)
            .collect()
    }
}

impl StreamerzError {
    pub fn new(
        code: impl Into<String>,
        title: impl Into<String>,
        description: impl Into<String>,
        recommendation: impl Into<String>,
    ) -> Self {
        let raw_title = title.into();
        let raw_desc = description.into();
        let raw_rec = recommendation.into();
        let title = if raw_title.trim().is_empty() {
            tr_or("error.unexpected.title", "Unexpected error")
        } else {
            raw_title.trim().to_owned()
        };
        let description = if raw_desc.trim().is_empty() {
            tr_or("error.unexpected.description", "An unexpected error occurred.")
        } else {
            raw_desc.trim().to_owned()
        };
        let recommendation = if raw_rec.trim().is_empty() {
            tr_or(
                "error.unexpected.recommendation",
                "Try again, and check the Log page for details.",
            )
        } else {
            raw_rec.trim().to_owned()
        };
        Self {
            code: code.into(),
            title,
            description,
            recommendation,
            recommendations: Vec::new(),
            unexpected: false,
            detail: None,
            action: None,
        }
    }

    pub fn unexpected(description: impl Into<String>) -> Self {
        let mut err = Self::new(
            "unexpected",
            tr_or("error.unexpected.title", "Unexpected error"),
            description,
            tr_or(
                "error.unexpected.recommendation",
                "Try again, and check the Log page for details.",
            ),
        );
        err.unexpected = true;
        err
    }

    pub fn validation(description: impl Into<String>) -> Self {
        Self::new(
            "validation",
            tr_or("error.validation.title", "Check the entered details"),
            description,
            tr_or(
                "error.validation.recommendation",
                "Review the highlighted details and fix any invalid values before trying again.",
            ),
        )
    }

    pub fn not_found(description: impl Into<String>) -> Self {
        Self::new(
            "not_found",
            tr_or("error.not_found.title", "Item not found"),
            description,
            tr_or(
                "error.not_found.recommendation",
                "Refresh the page or verify that the requested item still exists.",
            ),
        )
    }

    pub fn network(description: impl Into<String>) -> Self {
        Self::new(
            "network",
            tr_or("error.network.title", "Connection issue"),
            description,
            tr_or(
                "error.network.recommendation",
                "Check that the service is running and reachable, then try again.",
            ),
        )
    }

    pub fn storage(description: impl Into<String>) -> Self {
        Self::new(
            "storage",
            tr_or("error.storage.title", "File access error"),
            description,
            tr_or(
                "error.storage.recommendation",
                "Verify file permissions and free disk space, then try again.",
            ),
        )
    }

    pub fn auth_required(description: impl Into<String>) -> Self {
        Self::new(
            "auth.required",
            tr_or("error.auth.title", "Authentication required"),
            description,
            tr_or(
                "error.auth.recommendation",
                "Sign in or enter valid authentication credentials and try again.",
            ),
        )
    }

    pub fn from_message(message: &str) -> Self {
        let trimmed = message.trim();
        if trimmed.is_empty() {
            return Self::unexpected("");
        }
        let lower = trimmed.to_ascii_lowercase();
        if lower.contains("authentication") || lower.contains("unauthorized") || lower.contains("forbidden") {
            return Self::auth_required(trimmed);
        }
        if lower.contains("timed out")
            || lower.contains("not connected")
            || lower.contains("isn't connected")
            || lower.contains("connection")
            || lower.contains("dropped")
        {
            return Self::network(trimmed);
        }
        if lower.contains("not found") || lower.starts_with("no ") || lower.contains("unknown ") {
            return Self::not_found(trimmed);
        }
        if lower.contains("not valid json")
            || lower.contains("invalid")
            || lower.contains("must be")
            || lower.contains("must contain")
            || lower.contains("required")
            || lower.contains("enter ")
            || lower.contains("pick ")
            || lower.contains("choose ")
            || lower.contains("cannot be deleted")
        {
            return Self::validation(trimmed);
        }
        if lower.contains("permission") || lower.contains("disk") || lower.contains("write") || lower.contains("read") {
            return Self::storage(trimmed);
        }
        Self::unexpected(trimmed)
    }

    pub fn from_value(value: &Value) -> Option<Self> {
        let obj = value.as_object()?;
        let description = obj
            .get("description")
            .or_else(|| obj.get("error"))
            .and_then(Value::as_str)
            .unwrap_or_default()
            .trim();
        let title = obj.get("title").and_then(Value::as_str).unwrap_or_default().trim();
        if description.is_empty() && title.is_empty() {
            return None;
        }
        let code = obj
            .get("code")
            .and_then(Value::as_str)
            .filter(|c| !c.trim().is_empty())
            .unwrap_or("unexpected")
            .to_owned();
        let recommendation = obj.get("recommendation").and_then(Value::as_str).unwrap_or_default();
        let mut err = Self::new(code, title, description, recommendation);
        err.unexpected = obj.get("unexpected").and_then(Value::as_bool).unwrap_or(false);
        err.detail = obj
            .get("detail")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_owned);
        err.recommendations = obj
            .get("recommendations")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_owned)
            .collect();
        err.action = obj.get("action").and_then(ErrorAction::from_value);
        Some(err)
    }

    /// Reconstruct a `StreamerzError` from any core IPC response object.
    pub fn from_response(value: &Value) -> Option<Self> {
        if let Some(info) = value.get("errorInfo") {
            if let Some(parsed) = Self::from_value(info) {
                return Some(parsed);
            }
        }
        if value["status"] == "error" {
            let msg = value["error"].as_str().unwrap_or("");
            return Some(Self::from_message(msg));
        }
        None
    }

    pub fn all_recommendations(&self) -> Vec<&str> {
        let mut out = Vec::with_capacity(1 + self.recommendations.len());
        if !self.recommendation.is_empty() {
            out.push(self.recommendation.as_str());
        }
        for item in &self.recommendations {
            if item.as_str() != self.recommendation {
                out.push(item.as_str());
            }
        }
        out
    }

    pub fn summary_line(&self) -> String {
        if self.description.is_empty() {
            self.title.clone()
        } else {
            format!("{}: {}", self.title, self.description)
        }
    }

    pub fn full_label(&self) -> String {
        let mut parts = vec![self.summary_line()];
        let recs = self.all_recommendations();
        if let Some((first, rest)) = recs.split_first() {
            parts.push(format!(
                "{}{}",
                tr_or("error.recommended_prefix", "Recommended: "),
                first
            ));
            for item in rest {
                parts.push(format!("- {item}"));
            }
        }
        parts.join("\n")
    }
}

impl fmt::Display for StreamerzError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} ({})", self.summary_line(), self.recommendation)
    }
}

impl std::error::Error for StreamerzError {}

impl From<String> for StreamerzError {
    fn from(value: String) -> Self {
        Self::from_message(&value)
    }
}

impl From<&str> for StreamerzError {
    fn from(value: &str) -> Self {
        Self::from_message(value)
    }
}

#[cfg(test)]
mod tests {
    use super::{ActiveError, StreamerzError};
    use serde_json::json;

    #[test]
    fn parses_structured_error_info_from_response() {
        let reply = json!({
            "id": "page:1",
            "status": "error",
            "error": "HTTP server cannot listen on 127.0.0.1:7474",
            "errorInfo": {
                "code": "server.http.bind",
                "title": "HTTP server start error",
                "description": "HTTP server cannot listen on 127.0.0.1:7474",
                "recommendation": "Check whether port 7474 is already in use.",
                "recommendations": ["Choose a different HTTP port in Servers."],
                "action": {
                    "id": "retry-http-server",
                    "kind": "retry_server",
                    "target": "http",
                    "label": "Restart server"
                }
            }
        });
        let err = StreamerzError::from_response(&reply).expect("structured error");
        assert_eq!(err.code, "server.http.bind");
        assert_eq!(err.title, "HTTP server start error");
        assert!(err.description.contains("127.0.0.1:7474"));
        assert_eq!(err.recommendations.len(), 1);
        assert_eq!(err.action.as_ref().unwrap().target, "http");
        assert!(err.full_label().contains("Recommended: "));
    }

    #[test]
    fn upgrades_legacy_error_string_and_unexpected_fallback_without_dropping_context() {
        let reply = json!({"status": "error", "error": "Enter a channel name"});
        let err = StreamerzError::from_response(&reply).expect("upgraded error");
        assert_eq!(err.code, "validation");
        assert_eq!(err.description, "Enter a channel name");
        assert!(!err.recommendation.is_empty());

        let unknown = StreamerzError::from_response(&json!({"status": "error", "error": "weird xyz failure"}))
            .expect("unexpected fallback");
        assert!(unknown.unexpected);
        assert_eq!(unknown.code, "unexpected");
        assert_eq!(unknown.description, "weird xyz failure");
        assert!(!unknown.recommendation.is_empty());
    }

    #[test]
    fn parses_active_errors_list_from_ready_or_status() {
        let list = ActiveError::list_from_value(&json!([
            {
                "key": "config.corrupted",
                "timestamp": 1700000000,
                "code": "config.corrupted",
                "title": "Corrupted configuration",
                "description": "streamerz.json is damaged",
                "recommendation": "Restore a backup on the Backups page."
            }
        ]));
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].key, "config.corrupted");
        assert_eq!(list[0].error.title, "Corrupted configuration");
    }
}
