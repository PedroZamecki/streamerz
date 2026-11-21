//! UI-process diagnostics: forwards frontend logs and breadcrumbs to the backend, and captures
//! UI panics both synchronously to `crash-ui-*.txt` and over the control pipe before aborting.

use serde_json::{json, Value};
use std::collections::{BTreeMap, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, Once};
use std::time::{Instant, SystemTime, UNIX_EPOCH};

const STEPS: usize = 40;
const PENDING_MAX: usize = 128;

#[derive(Default)]
struct DiagState {
    log_dir: Option<PathBuf>,
    version: String,
    started: Option<Instant>,
    steps: VecDeque<(String, String)>,
    facts: BTreeMap<String, String>,
    pending_logs: VecDeque<(&'static str, String)>,
    pending_steps: VecDeque<String>,
    pending_facts: VecDeque<(String, String)>,
}

static STATE: Mutex<DiagState> = Mutex::new(DiagState {
    log_dir: None,
    version: String::new(),
    started: None,
    steps: VecDeque::new(),
    facts: BTreeMap::new(),
    pending_logs: VecDeque::new(),
    pending_steps: VecDeque::new(),
    pending_facts: VecDeque::new(),
});

static READY: AtomicBool = AtomicBool::new(false);
static DEBUG: AtomicBool = AtomicBool::new(false);
static PANIC_HOOK: Once = Once::new();

pub fn init() {
    if let Ok(mut s) = STATE.lock() {
        s.started = Some(Instant::now());
        if s.log_dir.is_none() {
            if let Ok(dir) = std::env::var("STREAMERZ_LOG_DIR") {
                if !dir.is_empty() {
                    s.log_dir = Some(PathBuf::from(dir));
                }
            }
        }
        if s.version.is_empty() {
            if let Ok(ver) = std::env::var("STREAMERZ_VERSION") {
                s.version = ver;
            }
        }
    }
    PANIC_HOOK.call_once(|| {
        let previous = std::panic::take_hook();
        std::panic::set_hook(Box::new(move |info| {
            handle_panic(info);
            previous(info);
        }));
    });
}

pub fn mark_ready(log_dir: Option<&str>, version: Option<&str>, debug: bool) {
    DEBUG.store(debug, Ordering::Relaxed);
    let (logs, steps, facts) = {
        let Ok(mut s) = STATE.lock() else {
            READY.store(true, Ordering::Release);
            return;
        };
        if let Some(dir) = log_dir.filter(|d| !d.is_empty()) {
            s.log_dir = Some(PathBuf::from(dir));
        }
        if let Some(ver) = version.filter(|v| !v.is_empty()) {
            s.version = ver.to_owned();
        }
        READY.store(true, Ordering::Release);
        (
            std::mem::take(&mut s.pending_logs),
            std::mem::take(&mut s.pending_steps),
            std::mem::take(&mut s.pending_facts),
        )
    };
    for (k, v) in facts {
        crate::send(json!({"request": "FrontendStep", "kind": "fact", "key": k, "value": v}));
    }
    for text in steps {
        crate::send(json!({"request": "FrontendStep", "kind": "step", "text": text}));
    }
    for (level, text) in logs {
        crate::send(json!({"request": "FrontendLog", "level": level, "text": text}));
    }
}

/// The renderer's own warnings and errors, in Streamerz's log: what the GPU did (a failed frame, a lost
/// device, a switch to software rendering). Repeats are dropped and there is a cap, so a failing frame
/// can't flood it. GPUI's other notes (fonts, input devices) stay out.
struct Gpui;
static GPUI: Gpui = Gpui;
static FORWARDED: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
static LAST: Mutex<String> = Mutex::new(String::new());
const FORWARDED_MAX: u32 = 100;
/// Where the patched GPUI logs a switch to software rendering.
const SOFTWARE_TARGET: &str = "streamerz::software";
/// The renderers: Windows' Direct3D code and the wgpu one (Linux).
const RENDERER_TARGETS: [&str; 3] = [SOFTWARE_TARGET, "gpui_windows::directx", "gpui_wgpu"];
/// The window switched to software rendering by itself: what draws it is reported again.
pub static RENDERER_CHANGED: AtomicBool = AtomicBool::new(false);

impl log::Log for Gpui {
    fn enabled(&self, metadata: &log::Metadata) -> bool {
        metadata.level() <= log::Level::Warn
            && RENDERER_TARGETS
                .iter()
                .any(|target| metadata.target().starts_with(target))
    }
    fn log(&self, record: &log::Record) {
        if !self.enabled(record.metadata()) {
            return;
        }
        if record.target() == SOFTWARE_TARGET {
            RENDERER_CHANGED.store(true, Ordering::Relaxed);
        }
        let text = record.args().to_string();
        let Ok(mut last) = LAST.lock() else { return };
        if *last == text || FORWARDED.fetch_add(1, Ordering::Relaxed) >= FORWARDED_MAX {
            return;
        }
        last.clone_from(&text);
        drop(last);
        let text = if record.target() == SOFTWARE_TARGET {
            text
        } else {
            format!("{}: {text}", record.target())
        };
        match record.level() {
            log::Level::Error => log_error(text),
            _ => log_warn(text),
        }
    }
    fn flush(&self) {}
}

/// Sends the renderer's warnings and errors to Streamerz's log.
pub fn forward_gpui_log() {
    if log::set_logger(&GPUI).is_ok() {
        log::set_max_level(log::LevelFilter::Warn);
    }
}

pub fn set_debug(debug: bool) {
    DEBUG.store(debug, Ordering::Relaxed);
}

#[allow(dead_code)]
pub fn log_debug(text: impl Into<String>) {
    if DEBUG.load(Ordering::Relaxed) {
        emit_log("debug", text.into());
    }
}

#[allow(dead_code)]
pub fn log_info(text: impl Into<String>) {
    emit_log("info", text.into());
}

pub fn log_warn(text: impl Into<String>) {
    emit_log("warn", text.into());
}

pub fn log_error(text: impl Into<String>) {
    emit_log("error", text.into());
}

fn emit_log(level: &'static str, text: String) {
    if text.trim().is_empty() {
        return;
    }
    if READY.load(Ordering::Acquire) {
        crate::send(json!({"request": "FrontendLog", "level": level, "text": text}));
        return;
    }
    if let Ok(mut s) = STATE.lock() {
        if s.pending_logs.len() >= PENDING_MAX {
            s.pending_logs.pop_front();
        }
        s.pending_logs.push_back((level, text));
    }
}

pub fn step(text: impl Into<String>) {
    let text = text.into();
    let time = format_hms(SystemTime::now());
    {
        if let Ok(mut s) = STATE.lock() {
            if s.steps.len() >= STEPS {
                s.steps.pop_front();
            }
            s.steps.push_back((time, text.clone()));
            if !READY.load(Ordering::Acquire) {
                if s.pending_steps.len() >= STEPS {
                    s.pending_steps.pop_front();
                }
                s.pending_steps.push_back(text);
                return;
            }
        }
    }
    if READY.load(Ordering::Acquire) {
        crate::send(json!({"request": "FrontendStep", "kind": "step", "text": text}));
    }
}

pub fn fact(key: &str, value: impl Into<String>) {
    let value = value.into();
    {
        if let Ok(mut s) = STATE.lock() {
            s.facts.insert(key.to_owned(), value.clone());
            if !READY.load(Ordering::Acquire) {
                if s.pending_facts.len() >= PENDING_MAX {
                    s.pending_facts.pop_front();
                }
                s.pending_facts.push_back((key.to_owned(), value));
                return;
            }
        }
    }
    if READY.load(Ordering::Acquire) {
        crate::send(json!({"request": "FrontendStep", "kind": "fact", "key": key, "value": value}));
    }
}

fn handle_panic(info: &std::panic::PanicHookInfo<'_>) {
    let what = info
        .payload()
        .downcast_ref::<&str>()
        .map(|s| (*s).to_owned())
        .or_else(|| info.payload().downcast_ref::<String>().cloned())
        .unwrap_or_else(|| "(no message)".to_owned());
    let at = info
        .location()
        .map_or_else(String::new, |l| format!("{}:{}", l.file(), l.line()));
    let thread = std::thread::current().name().unwrap_or("?").to_owned();
    let trace = std::backtrace::Backtrace::force_capture().to_string();

    let (log_dir, version, context) = snapshot_context();
    let body = format_ui_report(&version, &thread, &at, &what, &context, &trace);
    let report_path = log_dir
        .as_deref()
        .and_then(|dir| write_crash_file(dir, &body, SystemTime::now()));

    let mut payload = json!({
        "request": "FrontendCrash",
        "thread": thread,
        "location": at,
        "message": redact_home(&what),
        "context": context,
        "backtrace": redact_home(&trace),
    });
    if let Some(path) = &report_path {
        payload["reportPath"] = Value::String(path.display().to_string());
    }
    crate::send(payload);
}

fn snapshot_context() -> (Option<PathBuf>, String, String) {
    let Ok(s) = STATE.try_lock() else {
        return (None, String::new(), String::from("Context:\n  (unavailable)\n"));
    };
    let ver = if s.version.is_empty() {
        std::env::var("STREAMERZ_VERSION").unwrap_or_else(|_| "unknown".to_owned())
    } else {
        s.version.clone()
    };
    let mut ctx = String::from("Context:\n");
    ctx += &format!(
        "  System: {} {} (UI process)\n",
        std::env::consts::OS,
        std::env::consts::ARCH
    );
    if let Some(secs) = s.started.map(|i| i.elapsed().as_secs()) {
        ctx += &format!(
            "  UI running for: {}h {:02}m {:02}s\n",
            secs / 3600,
            (secs / 60) % 60,
            secs % 60
        );
    }
    for (k, v) in &s.facts {
        ctx += &format!("  {k}: {v}\n");
    }
    ctx += "Last UI steps (newest last):\n";
    if s.steps.is_empty() {
        ctx += "  (none)\n";
    } else {
        for (at, what) in &s.steps {
            ctx += &format!("  {at} {what}\n");
        }
    }
    (s.log_dir.clone(), ver, redact_home(&ctx))
}

fn format_ui_report(version: &str, thread: &str, location: &str, message: &str, context: &str, trace: &str) -> String {
    let version = if version.trim().is_empty() {
        "unknown"
    } else {
        version.trim()
    };
    let text = format!(
        "Streamerz {version} on {} {}\nPanic in UI thread '{thread}' at {location}:\n{message}\n\n{context}\nBacktrace:\n{trace}\n",
        std::env::consts::OS,
        std::env::consts::ARCH,
    );
    redact_home(&text)
}

fn write_crash_file(dir: &Path, body: &str, now: SystemTime) -> Option<PathBuf> {
    if std::fs::create_dir_all(dir).is_err() {
        return None;
    }
    let stamp = format_utc_stamp(now);
    let pid = std::process::id();
    for seq in 0..100u32 {
        let name = if seq == 0 {
            format!("crash-ui-{stamp}.txt")
        } else {
            format!("crash-ui-{stamp}-{pid}-{seq}.txt")
        };
        let path = dir.join(name);
        if !path.exists() && std::fs::write(&path, body).is_ok() {
            return Some(path);
        }
    }
    None
}

fn redact_home(text: &str) -> String {
    let home = std::env::var("HOME")
        .or_else(|_| std::env::var("USERPROFILE"))
        .unwrap_or_default();
    if home.len() > 3 {
        text.replace(&home, "~")
    } else {
        text.to_owned()
    }
}

fn civil_from_unix(secs: i64) -> (i64, u32, u32, u32, u32, u32) {
    let days = secs.div_euclid(86_400);
    let rem = secs.rem_euclid(86_400) as u32;
    let h = rem / 3600;
    let m = (rem % 3600) / 60;
    let s = rem % 60;
    // Howard Hinnant's algorithm to convert days since 1970-01-01 to proleptic Gregorian (Y, M, D).
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = (z - era * 146_097) as u64;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = (yoe as i64) + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = y + i64::from(month <= 2);
    (year, month as u32, d as u32, h, m, s)
}

fn format_utc_stamp(now: SystemTime) -> String {
    let secs = now.duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs() as i64);
    let (y, mo, d, h, mi, s) = civil_from_unix(secs);
    format!("{y:04}{mo:02}{d:02}-{h:02}{mi:02}{s:02}")
}

fn format_hms(now: SystemTime) -> String {
    let secs = now.duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs() as i64);
    let (_, _, _, h, mi, s) = civil_from_unix(secs);
    format!("{h:02}:{mi:02}:{s:02}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn utc_stamp_formats_known_epoch_dates() {
        // 2026-03-15 14:05:09 UTC = 1773583509
        let t = UNIX_EPOCH + Duration::from_secs(1_773_583_509);
        assert_eq!(format_utc_stamp(t), "20260315-140509");
        assert_eq!(format_hms(t), "14:05:09");
        assert_eq!(format_utc_stamp(UNIX_EPOCH), "19700101-000000");
    }

    #[test]
    fn ui_report_matches_crash_summary_header_and_redacts_home() {
        let dir = std::env::temp_dir().join(format!("streamerz-diag-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let report = format_ui_report(
            "1.9.0",
            "main",
            "frontend/src/controls/form.rs:281",
            "range check failed",
            "Context:\n  UI page: Overlays\nLast UI steps (newest last):\n  12:00:00 opened page Overlays\n",
            "0: backtrace_frame",
        );
        assert!(report.starts_with("Streamerz 1.9.0 on "));
        assert!(
            report.contains("\nPanic in UI thread 'main' at frontend/src/controls/form.rs:281:\nrange check failed\n")
        );
        let p1 = write_crash_file(&dir, &report, UNIX_EPOCH + Duration::from_secs(100)).unwrap();
        let p2 = write_crash_file(&dir, &report, UNIX_EPOCH + Duration::from_secs(100)).unwrap();
        assert_ne!(p1, p2, "same-second UI crashes must not overwrite each other");
        assert_eq!(std::fs::read_to_string(&p1).unwrap(), report);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn step_and_fact_record_bounded_context() {
        for i in 0..(STEPS + 10) {
            step(format!("action {i}"));
        }
        fact("UI page", "Overlays");
        let (_, _, ctx) = snapshot_context();
        assert!(ctx.contains("UI page: Overlays"), "{ctx}");
        assert!(ctx.contains(&format!("action {}", STEPS + 9)), "{ctx}");
        assert!(!ctx.contains("action 0\n"), "old steps must roll off: {ctx}");
    }
}
