//! The log page's model: recent lines at or above a level, matching a search.

use crate::{i18n::tr, Level};
use serde_json::{json, Value};

/// The level filter's choices: the lowest level each shows.
const LEVELS: [Level; 4] = [Level::Debug, Level::Info, Level::Warn, Level::Error];

pub fn read(level: usize, search: &str) -> Value {
    let level = level.min(LEVELS.len() - 1);
    let minimum = LEVELS[level];
    let words = search.to_lowercase();
    let mut lines = Vec::new();
    crate::status(|status| {
        lines = status
            .log
            .iter()
            .filter(|(line_level, _, text)| *line_level >= minimum && (words.is_empty() || text.to_lowercase().contains(&words)))
            .map(|(line_level, at, text)| {
                json!({"time": crate::fmt::shown_clock(*at), "level": line_level.tag().to_lowercase(), "tag": format!("{:<5}", line_level.tag()), "text": text})
            })
            .collect();
    });
    json!({"domain": "log", "log": {
        "levels": [tr("log.level.everything"), tr("log.level.info"), tr("log.level.warn"), tr("log.level.error")],
        "level": level, "search": search, "lines": lines,
        "show": tr("log.show"), "searchHint": tr("log.search"), "folder": tr("btn.open_folder"),
    }})
}

/// Open the folder the log files are written to.
pub fn open_folder() -> serde_json::Value {
    let folder = crate::config_path().with_file_name("logs");
    crate::oauth::open_browser(&folder.display().to_string());
    json!({"accepted": true})
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_log_filters_by_level_and_words() {
        let _lock = crate::TEST_LOCK.lock().unwrap_or_else(|error| error.into_inner());
        crate::say("log-test info line".into());
        crate::error("log-test error line".into());
        let all = read(0, "log-test");
        assert!(all["log"]["lines"].as_array().unwrap().len() >= 2);
        let errors = read(3, "log-test");
        assert!(errors["log"]["lines"]
            .as_array()
            .unwrap()
            .iter()
            .all(|line| line["level"] == "error"));
        assert!(!errors["log"]["lines"].as_array().unwrap().is_empty());
        assert!(read(1, "no such words anywhere 9f3").get("log").unwrap()["lines"]
            .as_array()
            .unwrap()
            .is_empty());
        assert_eq!(read(99, "")["log"]["level"], 3);
    }
}
