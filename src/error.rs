//! Structured application errors with title, redacted description and actionable recommendations.

use crate::i18n::{tr, trf};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::fmt;
use std::sync::Mutex;

const MAX_ACTIVE_ERRORS: usize = 16;

/// Hide credentials a message or error body may carry (tokens, passwords, bearer headers).
pub fn redact(line: &str) -> String {
    const MARKERS: &[&str] = &[
        "access_token",
        "refresh_token",
        "accesstoken",
        "refreshtoken",
        "client_secret",
        "clientsecret",
        "id_token",
        "idtoken",
        "sessionid",
        "sessioncookie",
        "token=",
        "bearer ",
        "password",
    ];
    let lower = line.to_ascii_lowercase();
    let mut cuts = Vec::new();
    for marker in MARKERS {
        let mut from = 0;
        while let Some(i) = lower[from..].find(marker) {
            let end = from + i + marker.len();
            from = end;
            if let Some((start, end)) = credential_range(line, end, marker.ends_with(['=', ' '])) {
                cuts.push((start, end));
                from = end;
            }
        }
    }
    if cuts.is_empty() {
        return line.to_owned();
    }
    cuts.sort_unstable();
    let mut out = String::with_capacity(line.len());
    let mut at = 0;
    for (start, end) in cuts {
        if start >= at {
            out.push_str(&line[at..start]);
            out.push_str("***");
        }
        at = at.max(end);
    }
    out.push_str(&line[at..]);
    out
}

fn credential_range(line: &str, after_key: usize, has_separator: bool) -> Option<(usize, usize)> {
    let rest = &line[after_key..];
    let value = if has_separator {
        rest
    } else {
        rest.trim_start_matches(['"', '\''])
            .trim_start()
            .strip_prefix([':', '='])?
    }
    .trim_start();
    let first = *value.as_bytes().first()?;
    let (value, len) = if matches!(first, b'"' | b'\'') {
        let value = &value[1..];
        let mut escaped = false;
        let len = value
            .bytes()
            .position(|b| {
                let closes = b == first && !escaped;
                escaped = b == b'\\' && !escaped;
                closes
            })
            .unwrap_or(value.len());
        (value, len)
    } else {
        let len = value
            .bytes()
            .position(|b| b.is_ascii_whitespace() || b"&;,'\"}]".contains(&b))
            .unwrap_or(value.len());
        (value, len)
    };
    let start = line.len() - value.len();
    (len > 0).then_some((start, start + len))
}

/// Optional recovery action attached to an error banner.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ErrorAction {
    pub id: String,
    pub kind: String,
    pub target: String,
    pub label: String,
}

/// Standard structured error shared across backend, MCP, requests and frontend banners.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StreamerzError {
    pub code: String,
    pub title: String,
    pub description: String,
    pub recommendation: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub recommendations: Vec<String>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub unexpected: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub action: Option<ErrorAction>,
}

pub type Result<T> = std::result::Result<T, StreamerzError>;

/// An active session error surfaced in the frontend until cleared or dismissed.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ActiveError {
    pub key: String,
    pub timestamp: i64,
    #[serde(flatten)]
    pub error: StreamerzError,
}

static ACTIVE_ERRORS: Mutex<Vec<ActiveError>> = Mutex::new(Vec::new());

impl StreamerzError {
    pub fn new(
        code: impl Into<String>,
        title: impl Into<String>,
        description: impl Into<String>,
        recommendation: impl Into<String>,
    ) -> Self {
        let code = code.into();
        let title = clean_non_empty(title.into(), || tr("error.unexpected.title").to_owned());
        let description = clean_non_empty(redact(&description.into()), || {
            tr("error.unexpected.description").to_owned()
        });
        let recommendation = clean_non_empty(redact(&recommendation.into()), || {
            tr("error.unexpected.recommendation").to_owned()
        });
        Self {
            code,
            title,
            description,
            recommendation,
            recommendations: Vec::new(),
            unexpected: false,
            detail: None,
            action: None,
        }
    }

    pub fn with_recommendations(mut self, extra: impl IntoIterator<Item = impl Into<String>>) -> Self {
        for item in extra {
            let clean = redact(item.into().trim());
            if !clean.is_empty() && clean != self.recommendation && !self.recommendations.contains(&clean) {
                self.recommendations.push(clean);
            }
        }
        self
    }

    pub fn with_detail(mut self, detail: impl Into<String>) -> Self {
        let clean = redact(detail.into().trim());
        if !clean.is_empty() {
            self.detail = Some(clean);
        }
        self
    }

    pub fn with_action(mut self, action: ErrorAction) -> Self {
        self.action = Some(action);
        self
    }

    /// Graceful fallback for any unexpected or unclassified runtime failure.
    pub fn unexpected(description: impl Into<String>) -> Self {
        let raw = redact(description.into().trim());
        let description = if raw.is_empty() {
            tr("error.unexpected.description").to_owned()
        } else {
            raw
        };
        let mut err = Self::new(
            "unexpected",
            tr("error.unexpected.title"),
            description,
            tr("error.unexpected.recommendation"),
        );
        err.unexpected = true;
        err
    }

    pub fn corrupted_config(path: &str, detail: &str, recovered_backup: bool) -> Self {
        let safe_detail = redact(detail);
        let desc = if recovered_backup {
            trf(
                "error.config_corrupted.recovered_desc",
                &[("path", path), ("detail", &safe_detail)],
            )
        } else {
            trf(
                "error.config_corrupted.defaults_desc",
                &[("path", path), ("detail", &safe_detail)],
            )
        };
        Self::new(
            "config.corrupted",
            tr("error.config_corrupted.title"),
            desc,
            tr("error.config_corrupted.recommendation"),
        )
        .with_detail(safe_detail)
        .with_action(ErrorAction {
            id: "open-backups".into(),
            kind: "navigate".into(),
            target: "Backups".into(),
            label: tr("error.action.open_backups").into(),
        })
    }

    pub fn config_save(path: &str, detail: &str) -> Self {
        let safe_detail = redact(detail);
        Self::new(
            "config.save",
            tr("error.config_save.title"),
            trf("error.config_save.desc", &[("path", path), ("detail", &safe_detail)]),
            tr("error.config_save.recommendation"),
        )
        .with_detail(safe_detail)
    }

    pub fn http_server_start(address: &str, port: u16, detail: &str) -> Self {
        let safe_detail = redact(detail);
        let port_str = port.to_string();
        Self::new(
            "server.http.bind",
            tr("error.server_http.title"),
            trf(
                "error.server_http.desc",
                &[("address", address), ("port", &port_str), ("detail", &safe_detail)],
            ),
            tr("error.server_http.recommendation"),
        )
        .with_detail(safe_detail)
        .with_recommendations([tr("error.action.open_servers")])
        .with_action(ErrorAction {
            id: "retry-http-server".into(),
            kind: "retry_server".into(),
            target: "http".into(),
            label: tr("error.action.retry_server").into(),
        })
    }

    pub fn websocket_server_start(address: &str, port: u16, detail: &str) -> Self {
        let safe_detail = redact(detail);
        let port_str = port.to_string();
        Self::new(
            "server.websocket.bind",
            tr("error.server_ws.title"),
            trf(
                "error.server_ws.desc",
                &[("address", address), ("port", &port_str), ("detail", &safe_detail)],
            ),
            tr("error.server_ws.recommendation"),
        )
        .with_detail(safe_detail)
        .with_action(ErrorAction {
            id: "retry-ws-server".into(),
            kind: "retry_server".into(),
            target: "websocket".into(),
            label: tr("error.action.retry_server").into(),
        })
    }

    pub fn udp_server_start(port: u16, detail: &str) -> Self {
        let safe_detail = redact(detail);
        let port_str = port.to_string();
        Self::new(
            "server.udp.bind",
            tr("error.server_udp.title"),
            trf(
                "error.server_udp.desc",
                &[("port", &port_str), ("detail", &safe_detail)],
            ),
            trf("error.server_udp.recommendation", &[("port", &port_str)]),
        )
        .with_detail(safe_detail)
        .with_action(ErrorAction {
            id: "retry-udp-server".into(),
            kind: "retry_server".into(),
            target: "udp".into(),
            label: tr("error.action.retry_server").into(),
        })
    }

    pub fn stream_deck_server_start(address: &str, port: u16, detail: &str) -> Self {
        let safe_detail = redact(detail);
        let port_str = port.to_string();
        Self::new(
            "server.stream_deck.bind",
            tr("error.server_stream_deck.title"),
            trf(
                "error.server_stream_deck.desc",
                &[("address", address), ("port", &port_str), ("detail", &safe_detail)],
            ),
            tr("error.server_stream_deck.recommendation"),
        )
        .with_detail(safe_detail)
        .with_action(ErrorAction {
            id: "retry-stream-deck-server".into(),
            kind: "retry_server".into(),
            target: "stream_deck".into(),
            label: tr("error.action.retry_server").into(),
        })
    }

    pub fn custom_server_start(name: &str, address: &str, port: u16, detail: &str) -> Self {
        let safe_detail = redact(detail);
        let port_str = port.to_string();
        Self::new(
            "server.custom.bind",
            tr("error.server_custom.title"),
            trf(
                "error.server_custom.desc",
                &[
                    ("name", name),
                    ("address", address),
                    ("port", &port_str),
                    ("detail", &safe_detail),
                ],
            ),
            tr("error.server_custom.recommendation"),
        )
        .with_detail(safe_detail)
        .with_action(ErrorAction {
            id: "open-servers".into(),
            kind: "navigate".into(),
            target: "Servers".into(),
            label: tr("error.action.open_servers").into(),
        })
    }

    pub fn auth_required(description: impl Into<String>) -> Self {
        Self::new(
            "auth.required",
            tr("error.auth.title"),
            description,
            tr("error.auth.recommendation"),
        )
    }

    pub fn validation(description: impl Into<String>) -> Self {
        Self::new(
            "validation",
            tr("error.validation.title"),
            description,
            tr("error.validation.recommendation"),
        )
    }

    pub fn not_found(description: impl Into<String>) -> Self {
        Self::new(
            "not_found",
            tr("error.not_found.title"),
            description,
            tr("error.not_found.recommendation"),
        )
    }

    pub fn network(description: impl Into<String>) -> Self {
        Self::new(
            "network",
            tr("error.network.title"),
            description,
            tr("error.network.recommendation"),
        )
    }

    pub fn storage(description: impl Into<String>) -> Self {
        Self::new(
            "storage",
            tr("error.storage.title"),
            description,
            tr("error.storage.recommendation"),
        )
    }

    pub fn frontend_launch(description: impl Into<String>) -> Self {
        Self::new(
            "frontend.launch",
            tr("error.frontend_launch.title"),
            description,
            tr("error.frontend_launch.recommendation"),
        )
    }

    pub fn frontend_crash(description: impl Into<String>) -> Self {
        Self::new(
            "frontend.crash",
            tr("error.frontend_crash.title"),
            description,
            tr("error.frontend_crash.recommendation"),
        )
    }

    pub fn too_many_requests() -> Self {
        Self::new(
            "rate_limit",
            tr("error.too_many_requests.title"),
            "Too many pending requests",
            tr("error.too_many_requests.recommendation"),
        )
    }

    /// Map any plain error message into a structured `StreamerzError`, preserving its description verbatim
    /// (after credential redaction) and falling back cleanly to `unexpected` for unrecognized messages.
    pub fn from_message(message: &str) -> Self {
        let trimmed = message.trim();
        if trimmed.is_empty() {
            return Self::unexpected("");
        }
        let clean = redact(trimmed);
        let lower = clean.to_ascii_lowercase();
        if lower.contains("authentication") || lower.contains("unauthorized") || lower.contains("forbidden") {
            return Self::auth_required(clean);
        }
        if lower.contains("too many pending requests") {
            return Self::too_many_requests();
        }
        if lower.contains("http server") && lower.contains("listen") {
            return Self::new(
                "server.http.bind",
                tr("error.server_http.title"),
                clean,
                tr("error.server_http.recommendation"),
            );
        }
        if lower.contains("websocket server") && lower.contains("listen") {
            return Self::new(
                "server.websocket.bind",
                tr("error.server_ws.title"),
                clean,
                tr("error.server_ws.recommendation"),
            );
        }
        if lower.contains("udp server") && lower.contains("listen") {
            return Self::new(
                "server.udp.bind",
                tr("error.server_udp.title"),
                clean,
                tr("error.server_ws.recommendation"),
            );
        }
        if lower.contains("stream deck server") && lower.contains("listen") {
            return Self::new(
                "server.stream_deck.bind",
                tr("error.server_stream_deck.title"),
                clean,
                tr("error.server_stream_deck.recommendation"),
            );
        }
        if lower.contains("can't save") || lower.contains("could not save") {
            return Self::new(
                "config.save",
                tr("error.config_save.title"),
                clean,
                tr("error.config_save.recommendation"),
            );
        }
        if lower.contains("timed out")
            || lower.contains("not connected")
            || lower.contains("isn't connected")
            || lower.contains("isn't running")
            || lower.contains("closed the connection")
            || lower.contains("connection")
            || lower.contains("dropped")
            || lower.contains("no answer")
        {
            return Self::network(clean);
        }
        if lower.contains("not found")
            || lower.starts_with("no ")
            || lower.contains("unknown ")
            || lower.contains("no tool ")
        {
            return Self::not_found(clean);
        }
        if lower.contains("not valid json")
            || lower.contains("invalid")
            || lower.contains("must be")
            || lower.contains("must contain")
            || lower.contains("missing argument")
            || lower.contains("required")
            || lower.contains("1 to 65535")
            || lower.contains("at least")
            || lower.contains("duration")
            || lower.contains("enter ")
            || lower.contains("pick ")
            || lower.contains("choose ")
            || lower.contains("unable to parse")
            || lower.contains("cannot be deleted")
        {
            return Self::validation(clean);
        }
        if lower.contains("permission") || lower.contains("disk") || lower.contains("write") || lower.contains("read") {
            return Self::storage(clean);
        }
        Self::unexpected(clean)
    }

    /// Reconstruct a `StreamerzError` from a wire response JSON object.
    pub fn from_response(value: &Value) -> Option<Self> {
        if let Some(info) = value.get("errorInfo") {
            if let Ok(mut parsed) = serde_json::from_value::<Self>(info.clone()) {
                parsed.description = redact(&parsed.description);
                parsed.recommendation = redact(&parsed.recommendation);
                parsed.recommendations = parsed.recommendations.into_iter().map(|item| redact(&item)).collect();
                return Some(parsed);
            }
        }
        if value["status"] == "error" {
            let msg = value["error"].as_str().unwrap_or("");
            return Some(Self::from_message(msg));
        }
        None
    }

    pub fn wire_message(&self) -> &str {
        &self.description
    }

    pub fn to_json_response(&self, id: &Value) -> Value {
        json!({
            "id": id,
            "status": "error",
            "error": self.description,
            "errorInfo": self,
        })
    }

    pub fn to_http_response(&self, extra: Option<Value>) -> Value {
        let mut out = json!({
            "status": "error",
            "error": self.description,
            "errorInfo": self,
        });
        if let (Some(obj), Some(fields)) = (out.as_object_mut(), extra.and_then(|v| v.as_object().cloned())) {
            for (k, v) in fields {
                if !matches!(k.as_str(), "status" | "error" | "errorInfo") {
                    obj.insert(k, v);
                }
            }
        }
        out
    }

    pub fn format_log(&self) -> String {
        format!(
            "[{}] {}: {} ({})",
            self.code, self.title, self.description, self.recommendation
        )
    }

    pub fn format_mcp(&self) -> String {
        let mut lines = vec![format!("{}: {}", self.title, self.description)];
        if !self.recommendation.is_empty() {
            lines.push(format!("{}{}", tr("error.recommended_prefix"), self.recommendation));
        }
        for rec in &self.recommendations {
            lines.push(format!("- {rec}"));
        }
        lines.join("\n")
    }
}

fn clean_non_empty(value: String, fallback: impl FnOnce() -> String) -> String {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        fallback()
    } else {
        trimmed.to_owned()
    }
}

impl fmt::Display for StreamerzError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.description.is_empty() {
            write!(f, "{}: {}", self.title, self.recommendation)
        } else {
            write!(f, "{}: {} ({})", self.title, self.description, self.recommendation)
        }
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

impl From<std::io::Error> for StreamerzError {
    fn from(value: std::io::Error) -> Self {
        use std::io::ErrorKind;
        let msg = value.to_string();
        match value.kind() {
            ErrorKind::NotFound => Self::not_found(msg),
            ErrorKind::PermissionDenied => Self::storage(msg),
            ErrorKind::TimedOut
            | ErrorKind::ConnectionRefused
            | ErrorKind::ConnectionReset
            | ErrorKind::ConnectionAborted
            | ErrorKind::NotConnected
            | ErrorKind::BrokenPipe => Self::network(msg),
            ErrorKind::InvalidData | ErrorKind::InvalidInput => Self::validation(msg),
            _ => Self::unexpected(msg),
        }
    }
}

impl From<serde_json::Error> for StreamerzError {
    fn from(value: serde_json::Error) -> Self {
        Self::validation(format!("Not valid JSON: {value}"))
    }
}

/// Record a persistent active session error keyed by `key`, log it, and notify the frontend.
pub fn record_active(key: impl Into<String>, error: StreamerzError) {
    let key = key.into();
    if key.is_empty() || key.len() > 256 {
        return;
    }
    crate::report_error(&error);
    let timestamp = crate::fmt::now_secs() as i64;
    if let Ok(mut list) = ACTIVE_ERRORS.lock() {
        list.retain(|entry| entry.key != key);
        if list.len() >= MAX_ACTIVE_ERRORS {
            list.remove(0);
        }
        list.push(ActiveError { key, timestamp, error });
    }
    crate::host::changed();
}

/// Clear an active error once resolved (or dismissed by the user).
pub fn clear_active(key: &str) -> bool {
    let Ok(mut list) = ACTIVE_ERRORS.lock() else {
        return false;
    };
    let before = list.len();
    list.retain(|entry| entry.key != key);
    let removed = list.len() != before;
    drop(list);
    if removed {
        crate::host::changed();
    }
    removed
}

pub fn active_list() -> Vec<ActiveError> {
    ACTIVE_ERRORS.lock().map(|list| list.clone()).unwrap_or_default()
}

#[cfg(test)]
pub fn clear_all_active_for_test() {
    if let Ok(mut list) = ACTIVE_ERRORS.lock() {
        list.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn redacts_credentials_in_structured_errors() {
        let err =
            StreamerzError::unexpected(r#"token request failed: {"access_token":"secret_tok","refresh_token": "r1"}"#);
        assert!(err.unexpected);
        assert_eq!(err.title, tr("error.unexpected.title"));
        assert!(!err.recommendation.is_empty());
        assert!(!err.description.contains("secret_tok"));
        assert!(!err.description.contains("r1"));
        assert!(err.description.contains("***"));
    }

    #[test]
    fn redacts_whole_quoted_credentials_and_keeps_surrounding_text() {
        let input = r#"prefix {"password": "quoted \"part\" with spaces", "clientSecret": "other value", "ok":true}"#;
        assert_eq!(
            redact(input),
            r#"prefix {"password": "***", "clientSecret": "***", "ok":true}"#
        );
        assert_eq!(redact("password='with a space'"), "password='***'");
        assert_eq!(redact("password=áß!"), "password=***");
        assert_eq!(redact("password field is required"), "password field is required");
        assert_eq!(redact("token=t87%2B&state=one"), "token=***&state=one");
    }

    #[test]
    fn corrupted_config_and_server_errors_have_title_description_and_recommendation() {
        let cfg = StreamerzError::corrupted_config("/tmp/streamerz.json", "expected value at line 1", true);
        assert_eq!(cfg.code, "config.corrupted");
        assert_eq!(cfg.title, "Corrupted configuration");
        assert!(cfg.description.contains("/tmp/streamerz.json"));
        assert!(cfg.description.contains(".bak"));
        assert!(!cfg.recommendation.is_empty());
        assert_eq!(cfg.action.as_ref().unwrap().target, "Backups");

        let http = StreamerzError::http_server_start("127.0.0.1", 7474, "address already in use");
        assert_eq!(http.code, "server.http.bind");
        assert_eq!(http.title, "HTTP server start error");
        assert!(http.description.contains("127.0.0.1:7474"));
        assert!(http.recommendation.contains("Restart the HTTP server"));
        assert_eq!(http.action.as_ref().unwrap().target, "http");
    }

    #[test]
    fn unexpected_errors_never_drop_context_and_roundtrip_wire_json() {
        let unknown = StreamerzError::from_message("something completely bizarre broke inside module X");
        assert!(unknown.unexpected);
        assert_eq!(unknown.code, "unexpected");
        assert_eq!(unknown.title, "Unexpected error");
        assert_eq!(
            unknown.description,
            "something completely bizarre broke inside module X"
        );
        assert!(!unknown.recommendation.is_empty());

        let wire = unknown.to_json_response(&json!("req-1"));
        assert_eq!(wire["id"], "req-1");
        assert_eq!(wire["status"], "error");
        assert_eq!(wire["error"], unknown.description);
        let parsed = StreamerzError::from_response(&wire).expect("parsed");
        assert_eq!(parsed, unknown);
    }

    #[test]
    fn active_registry_records_replaces_clears_and_bounds() {
        let _lock = crate::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        clear_all_active_for_test();
        for n in 0..20 {
            record_active(format!("k:{n}"), StreamerzError::unexpected(format!("err {n}")));
        }
        let items = active_list();
        assert_eq!(items.len(), MAX_ACTIVE_ERRORS);
        assert_eq!(items[0].key, "k:4");
        assert!(clear_active("k:19"));
        assert!(!clear_active("missing"));
        clear_all_active_for_test();
    }
}
