//! Logs and crash reports, next to the settings file (`logs` folder): the current log, the last few
//! ones gzipped, and a report for every panic. A report nobody has dealt with is offered at the next
//! start as a prefilled GitHub issue.

use std::collections::{BTreeMap, VecDeque};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

/// Old logs kept (gzipped) and crash reports kept.
const KEEP_LOGS: usize = 7;
const KEEP_CRASHES: usize = 20;
/// The current log stops growing here (a runaway loop shouldn't fill the disk).
const LOG_MAX: u64 = 4 * 1024 * 1024;

fn dir() -> PathBuf {
    crate::config_path().with_file_name("logs")
}

static LOG: Mutex<Option<(std::fs::File, u64)>> = Mutex::new(None);

/// How many of the last things done are kept.
const STEPS: usize = 40;

/// What the user was doing: the last things done in the window and the state around them. Kept in
/// `logs/context.txt` as it changes, so a run that dies without a panic (killed, the display ended it) is
/// understood too.
#[derive(Default)]
struct Trail {
    steps: VecDeque<(String, String)>,
    facts: BTreeMap<&'static str, String>,
}

static TRAIL: Mutex<Option<Trail>> = Mutex::new(None);
static STARTED: Mutex<Option<std::time::Instant>> = Mutex::new(None);
/// Describes the window's event in progress, for a panic inside a handler (set by the window).
static PROBE: Mutex<Option<fn() -> String>> = Mutex::new(None);

/// Notes something the user did ("opened page Chat"), newest last.
pub fn step(text: &str) {
    let body = {
        let Ok(mut guard) = TRAIL.lock() else { return };
        let t = guard.get_or_insert_with(Trail::default);
        if t.steps.len() >= STEPS {
            t.steps.pop_front();
        }
        t.steps
            .push_back((crate::fmt::date(&crate::fmt::now(), "HH:mm:ss"), text.to_string()));
        private(&trail_text(t))
    };
    // Only once the log runs (a test or a tool has no run to describe).
    if LOG.lock().is_ok_and(|l| l.is_some()) {
        let _ = std::fs::write(dir().join("context.txt"), body);
    }
}

/// Notes how things stand ("page", "window"...): replaces the last value of `key`.
pub fn fact(key: &'static str, value: &str) {
    if let Ok(mut t) = TRAIL.lock() {
        t.get_or_insert_with(Trail::default)
            .facts
            .insert(key, value.to_string());
    }
}

/// How to describe the event being handled, when a panic comes from inside one.
pub fn probe(f: fn() -> String) {
    if let Ok(mut p) = PROBE.lock() {
        *p = Some(f);
    }
}

/// The system Streamerz runs on, in a line: what a bug may depend on.
fn system() -> String {
    let mut parts = vec![format!("{} {}", std::env::consts::OS, std::env::consts::ARCH)];
    if let Some(name) = std::fs::read_to_string("/etc/os-release").ok().and_then(|t| {
        t.lines()
            .find_map(|l| l.strip_prefix("PRETTY_NAME=").map(str::to_string))
    }) {
        parts.push(name.trim_matches('"').to_string());
    }
    for (label, var) in [("desktop", "XDG_CURRENT_DESKTOP"), ("session", "XDG_SESSION_TYPE")] {
        if let Ok(v) = std::env::var(var) {
            parts.push(format!("{label} {v}"));
        }
    }
    let shows = match (
        std::env::var_os("WAYLAND_DISPLAY").is_some(),
        std::env::var_os("DISPLAY").is_some(),
    ) {
        (true, true) => "Wayland and X11 available",
        (true, false) => "Wayland",
        (false, true) => "X11",
        _ => "no display",
    };
    parts.push(shows.to_string());
    if std::env::var_os("APPIMAGE").is_some() {
        parts.push("AppImage".into());
    }
    parts.join(", ")
}

/// Resident memory in MB (Linux), for a report.
fn memory_mb() -> Option<u64> {
    let t = std::fs::read_to_string("/proc/self/statm").ok()?;
    let pages: u64 = t.split_whitespace().nth(1)?.parse().ok()?;
    Some(pages * 4096 / 1_000_000)
}

/// The context of a report: system, how long it ran, the state, then the last steps.
fn trail_text(t: &Trail) -> String {
    let mut out = String::from("Context:\n");
    out += &format!("  System: {}\n", system());
    let up = STARTED.lock().ok().and_then(|s| s.map(|i| i.elapsed().as_secs()));
    if let Some(s) = up {
        out += &format!("  Running for: {}h {:02}m\n", s / 3600, s / 60 % 60);
    }
    if let Some(mb) = memory_mb() {
        out += &format!("  Memory: {mb} MB\n");
    }
    for (k, v) in &t.facts {
        out += &format!("  {k}: {v}\n");
    }
    out += "Last steps (newest last):\n";
    if t.steps.is_empty() {
        out += "  (none)\n";
    }
    for (at, what) in &t.steps {
        out += &format!("  {at} {what}\n");
    }
    out
}

/// The context as it stands now.
fn context() -> String {
    let t = TRAIL.try_lock().ok();
    let none = Trail::default();
    trail_text(t.as_deref().and_then(Option::as_ref).unwrap_or(&none))
}

/// Starts this run's log: the last one is gzipped away, the oldest ones deleted. Call once, early.
pub fn start() {
    if let Ok(mut s) = STARTED.lock() {
        *s = Some(std::time::Instant::now());
    }
    let dir = dir();
    if std::fs::create_dir_all(&dir).is_err() {
        return;
    }
    let latest = dir.join("latest.log");
    // The last run left its marker: it never got to quit. Whatever it said last is the report.
    let running = dir.join("running");
    if running.exists() {
        let tail = |name: &str, n: usize| {
            let t = std::fs::read_to_string(dir.join(name)).unwrap_or_default();
            let lines: Vec<&str> = t.lines().collect();
            lines[lines.len().saturating_sub(n)..].join("\n")
        };
        let context = std::fs::read_to_string(dir.join("context.txt")).unwrap_or_default();
        let body = private(&unclean_report(
            &tail("latest.log", 60),
            &tail("stderr.log", 20),
            &context,
        ));
        let stamp = crate::fmt::date(&crate::fmt::now(), "yyyyMMdd-HHmmss");
        let _ = std::fs::write(dir.join(format!("crash-{stamp}.txt")), body);
        keep(&dir, ".txt", KEEP_CRASHES);
    }
    let _ = std::fs::write(&running, std::process::id().to_string());
    if let Ok(old) = std::fs::read(&latest) {
        if !old.is_empty() {
            let stamp = std::fs::metadata(&latest)
                .and_then(|m| m.modified())
                .ok()
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .map_or(0, |d| d.as_secs());
            let mut z = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
            if z.write_all(&old).is_ok() {
                if let Ok(gz) = z.finish() {
                    let _ = std::fs::write(dir.join(format!("{stamp}.log.gz")), gz);
                }
            }
        }
    }
    keep(&dir, "log.gz", KEEP_LOGS);
    #[cfg(unix)]
    keep_stderr(&dir);
    if let Ok(f) = std::fs::File::create(&latest) {
        if let Ok(mut l) = LOG.lock() {
            *l = Some((f, 0));
        }
    }
    std::panic::set_hook({
        let previous = std::panic::take_hook();
        Box::new(move |info| {
            report(info);
            previous(info);
        })
    });
}

/// Started from a launcher (stderr isn't a terminal), what the libraries print to stderr (a Wayland protocol
/// error is printed there, and then the process exits) goes to `logs/stderr.log` instead of nowhere.
#[cfg(unix)]
fn keep_stderr(dir: &Path) {
    use std::os::fd::AsRawFd;
    extern "C" {
        fn isatty(fd: i32) -> i32;
        fn dup2(old: i32, new: i32) -> i32;
    }
    // SAFETY: plain libc calls on standard descriptors and a file we just opened (dup2 copies it).
    unsafe {
        if isatty(2) == 0 {
            if let Ok(f) = std::fs::File::create(dir.join("stderr.log")) {
                dup2(f.as_raw_fd(), 2);
            }
        }
    }
}

/// A run that ended without a panic: no message, so the end of the log and of stderr.
fn unclean_report(log: &str, stderr: &str, context: &str) -> String {
    format!(
        "Streamerz {} on {} {}\nIt stopped without a crash message (killed, or the display or a library ended it).\n\n{context}\nLast stderr lines:\n{stderr}\n\nLast log lines:\n{log}\n",
        crate::brand::VERSION,
        std::env::consts::OS,
        std::env::consts::ARCH,
    )
}

/// Quits for good: the marker that says "it stopped unexpectedly" is removed first.
pub fn quit(code: i32) -> ! {
    let _ = std::fs::remove_file(dir().join("running"));
    std::process::exit(code)
}

/// Deletes the oldest files of `dir` ending in `.ext`, keeping `n`.
fn keep(dir: &Path, ext: &str, n: usize) {
    let Ok(rd) = std::fs::read_dir(dir) else { return };
    let mut files: Vec<_> = rd
        .flatten()
        .filter(|e| e.file_name().to_string_lossy().ends_with(ext))
        .filter_map(|e| Some((e.metadata().ok()?.modified().ok()?, e.path())))
        .collect();
    files.sort();
    for (_, p) in files.iter().take(files.len().saturating_sub(n)) {
        let _ = std::fs::remove_file(p);
    }
}

/// One line of the log (already redacted).
pub fn line(level: crate::Level, text: &str) {
    let Ok(mut l) = LOG.lock() else { return };
    if let Some((f, size)) = l.as_mut() {
        if *size < LOG_MAX {
            let t = format!(
                "{} [{}] {text}\n",
                crate::fmt::date(&crate::fmt::now(), "HH:mm:ss"),
                level.tag()
            );
            *size += t.len() as u64;
            let _ = f.write_all(t.as_bytes());
        }
    }
}

/// A panic's message and where it happened.
fn what_at(info: &std::panic::PanicHookInfo<'_>) -> (String, String) {
    let what = info
        .payload()
        .downcast_ref::<&str>()
        .map(|s| s.to_string())
        .or_else(|| info.payload().downcast_ref::<String>().cloned())
        .unwrap_or_else(|| "(no message)".into());
    let at = info
        .location()
        .map_or(String::new(), |l| format!("{}:{}", l.file(), l.line()));
    (what, at)
}

/// The report of a panic: what, where, the recent log and a backtrace, with the home folder hidden.
fn text(info: &std::panic::PanicHookInfo<'_>, log: &[String]) -> String {
    let (what, at) = what_at(info);
    let thread = std::thread::current().name().unwrap_or("?").to_string();
    let trace = std::backtrace::Backtrace::force_capture();
    // What the window was handling (a panic inside a click or a key): only the window's own thread knows.
    let event = if thread == "main" {
        PROBE.try_lock().ok().and_then(|p| *p).map(|f| f())
    } else {
        None
    };
    let event = event.map_or(String::new(), |e| format!("Event being handled: {e}\n"));
    let report = format!(
        "Streamerz {} on {} {}\nPanic in thread '{thread}' at {at}:\n{what}\n\n{event}{}\nBacktrace:\n{trace}\n\nLast log lines:\n{}\n",
        crate::brand::VERSION,
        std::env::consts::OS,
        std::env::consts::ARCH,
        context(),
        log.join("\n"),
    );
    private(&report)
}

/// `text` without the user's home folder.
fn private(text: &str) -> String {
    let home = std::env::var("HOME")
        .or_else(|_| std::env::var("USERPROFILE"))
        .unwrap_or_default();
    if home.len() > 3 {
        text.replace(&home, "~")
    } else {
        text.to_string()
    }
}

fn report(info: &std::panic::PanicHookInfo<'_>) {
    // The log file says it too. Never waits: the panic may have come while writing a line.
    if let Ok(mut l) = LOG.try_lock() {
        if let Some((f, _)) = l.as_mut() {
            let (what, at) = what_at(info);
            let _ = writeln!(
                f,
                "{} [FATAL] {} ({at})",
                crate::fmt::date(&crate::fmt::now(), "HH:mm:ss"),
                private(&what)
            );
        }
    }
    let log = crate::log_tail_try(60);
    let body = text(info, &log);
    let dir = dir();
    let _ = std::fs::create_dir_all(&dir);
    let stamp = crate::fmt::date(&crate::fmt::now(), "yyyyMMdd-HHmmss");
    let _ = std::fs::write(dir.join(format!("crash-{stamp}.txt")), body);
    // This report says it; the next start needn't make another from the marker.
    let _ = std::fs::remove_file(dir.join("running"));
    keep(&dir, ".txt", KEEP_CRASHES);
    keep(&dir, ".reported", KEEP_CRASHES);
    // The report is written: now keep what would be lost.
    crate::engine::save_and_wait();
}

/// The newest crash report nobody has dealt with, and its text.
fn pending() -> Option<(PathBuf, String)> {
    let mut found: Vec<PathBuf> = std::fs::read_dir(dir())
        .ok()?
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x == "txt"))
        .collect();
    found.sort();
    let path = found.pop()?;
    let body = std::fs::read_to_string(&path).ok()?;
    Some((path, body))
}

/// The first `n` chars of `text`.
fn head(text: &str, n: usize) -> String {
    text.chars().take(n).collect()
}

/// The last `n` chars of `text`.
fn tail(text: &str, n: usize) -> String {
    let skip = text.chars().count().saturating_sub(n);
    text.chars().skip(skip).collect()
}

/// The issue's address: what happened first (the message, the system, what the user was doing), then the
/// start of the backtrace and the end of the log, cut to fit (the full report stays in the logs folder).
fn issue_url(report: &str) -> String {
    let (top, rest) = report.split_once("\nBacktrace:\n").unwrap_or((report, ""));
    let (trace, log) = rest.split_once("\nLast log lines:\n").unwrap_or((rest, ""));
    if rest.is_empty() {
        // No panic message: the log tail is what there is.
        let (top, log) = report.split_once("\nLast log lines:\n").unwrap_or((report, ""));
        return issue_address(&head(top, 2000), "", &tail(log, 1200));
    }
    issue_address(&head(top, 2000), &head(trace, 900), &tail(log, 600))
}

fn issue_address(top: &str, trace: &str, log: &str) -> String {
    let section = |name: &str, text: &str| {
        if text.trim().is_empty() {
            String::new()
        } else {
            format!(
                "<details><summary>{name}</summary>\n\n```\n{}\n```\n</details>\n\n",
                text.trim_end()
            )
        }
    };
    let body = format!(
        "**What were you doing when it closed?**\n\n\n\n**What did you expect to happen?**\n\n\n\n{}{}{}The full report is in the logs folder (Crashes page, Open folder): attach it if it helps.\n",
        section("Crash report", top),
        section("Backtrace (start)", trace),
        section("Last log lines", log),
    );
    format!(
        "https://github.com/{}/issues/new?labels=crash&title={}&body={}",
        crate::brand::REPO,
        crate::commands::url_encode(&format!("Crash in {}", crate::brand::VERSION)),
        crate::commands::url_encode(&body)
    )
}

/// A crash report, for the Crashes page.
pub struct Report {
    pub path: PathBuf,
    /// When it was written (unix seconds).
    pub made: i64,
    /// The version that crashed, and what it said (or that it said nothing).
    pub version: String,
    pub what: String,
    /// Offered at a start already (and reported or dismissed).
    pub seen: bool,
}

impl Report {
    pub fn text(&self) -> String {
        std::fs::read_to_string(&self.path).unwrap_or_default()
    }
}

/// The version and the message of a report's text.
fn summary(text: &str) -> (String, String) {
    let mut lines = text.lines();
    let version = lines
        .next()
        .and_then(|l| l.strip_prefix("Streamerz "))
        .and_then(|l| l.split(" on ").next())
        .unwrap_or("")
        .to_string();
    let what = match lines.next() {
        Some(l) if l.starts_with("Panic in") => lines.next().unwrap_or(l),
        Some(l) => l,
        None => "",
    };
    (version, what.to_string())
}

/// The crash reports, newest first.
pub fn reports() -> Vec<Report> {
    let Ok(rd) = std::fs::read_dir(dir()) else {
        return Vec::new();
    };
    let mut out: Vec<Report> = rd
        .flatten()
        .map(|e| e.path())
        .filter(|p| {
            let name = p
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default();
            name.starts_with("crash-") && (name.ends_with(".txt") || name.ends_with(".reported"))
        })
        .map(|path| {
            let made = std::fs::metadata(&path)
                .and_then(|m| m.modified())
                .ok()
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .map_or(0, |d| d.as_secs() as i64);
            let head: String = std::fs::read_to_string(&path)
                .unwrap_or_default()
                .lines()
                .take(3)
                .collect::<Vec<_>>()
                .join("\n");
            let (version, what) = summary(&head);
            let seen = path.extension().is_some_and(|x| x == "reported");
            Report {
                path,
                made,
                version,
                what,
                seen,
            }
        })
        .collect();
    out.sort_by(|a, b| b.made.cmp(&a.made).then_with(|| b.path.cmp(&a.path)));
    out
}

/// Opens a prefilled GitHub issue for `r`, and marks it seen.
pub fn report_on_github(r: &Report) {
    crate::oauth::open_browser(&issue_url(&r.text()));
    if !r.seen {
        let _ = std::fs::rename(&r.path, r.path.with_extension("reported"));
    }
}

pub fn delete(r: &Report) -> Result<(), String> {
    std::fs::remove_file(&r.path).map_err(|e| format!("{}: {e}", r.path.display()))
}

/// The folder with the logs and the crash reports.
pub fn folder() -> PathBuf {
    dir()
}

/// At start: if the last run left a crash report, offers to report it. Either answer settles it.
pub async fn offer() {
    use crate::i18n::tr;
    let Some((path, body)) = pending() else { return };
    let yes = crate::ui::confirm(tr("crash.ask").to_string(), tr("crash.dismiss"), tr("crash.report"))
        .await
        .unwrap_or(false);
    let _ = std::fs::rename(&path, path.with_extension("reported"));
    if yes {
        crate::oauth::open_browser(&issue_url(&body));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_report_says_its_version_and_message() {
        let panic =
            "Streamerz 1.0.0 on linux x86_64\nPanic in thread 'main' at src/ui.rs:9:\nInvalid box\n\nBacktrace:";
        assert_eq!(summary(panic), ("1.0.0".into(), "Invalid box".into()));
        let quiet = unclean_report("", "", "");
        let (v, what) = summary(&quiet);
        assert!(!v.is_empty() && what.starts_with("It stopped without a crash message"));
    }

    #[test]
    fn the_issue_address_is_short_and_carries_the_report() {
        let long = "line of the report\n".repeat(2000);
        let url = issue_url(&long);
        assert!(url.len() < 8000, "{}", url.len());
        assert!(url.starts_with("https://github.com/") && url.contains("labels=crash"));
        assert!(url.contains("line+of+the+report"));
    }

    #[test]
    fn a_run_that_never_quit_is_reported_with_its_last_words() {
        let r = unclean_report(
            "12:00:01 connected",
            "[destroyed object]: error 0: Invalid box",
            "Context:\n  page: Chat\n",
        );
        assert!(
            r.contains("without a crash message")
                && r.contains("Invalid box")
                && r.contains("12:00:01")
                && r.contains("page: Chat")
        );
    }

    #[test]
    fn the_context_says_the_last_steps_and_the_state() {
        fact("Page", "Chat");
        for i in 0..STEPS + 5 {
            step(&format!("did {i}"));
        }
        let c = context();
        assert!(c.contains("System:") && c.contains("Page: Chat"));
        assert!(c.contains(&format!("did {}", STEPS + 4)) && !c.contains("did 0\n"));
    }

    #[test]
    fn the_issue_leads_with_the_context_and_cuts_the_rest() {
        let report = format!(
            "Streamerz 1.1.0 on linux x86_64\nPanic in thread 'main' at a.rs:1:\nboom\n\nContext:\n  Page: Chat\n\nBacktrace:\n{}\nLast log lines:\n{}",
            "frame\n".repeat(900),
            "log line\n".repeat(900)
        );
        let url = issue_url(&report);
        assert!(url.len() < 9000, "{}", url.len());
        assert!(url.contains("Page") && url.contains("expect") && url.contains("Context"));
        assert!(url.find("Context").unwrap() < url.find("frame").unwrap());
    }

    #[test]
    fn the_home_folder_is_hidden() {
        std::env::set_var("HOME", "/home/someone");
        assert_eq!(private("at /home/someone/x.rs"), "at ~/x.rs");
    }

    #[test]
    fn only_the_newest_files_are_kept() {
        let d = std::env::temp_dir().join(format!("streamerz-crash-{}", std::process::id()));
        std::fs::create_dir_all(&d).unwrap();
        for i in 0..5 {
            std::fs::write(d.join(format!("{i}.log.gz")), "x").unwrap();
            std::thread::sleep(std::time::Duration::from_millis(15));
        }
        keep(&d, "log.gz", 2);
        let mut left: Vec<_> = std::fs::read_dir(&d)
            .unwrap()
            .flatten()
            .map(|e| e.file_name())
            .collect();
        left.sort();
        assert_eq!(left, ["3.log.gz", "4.log.gz"]);
        std::fs::remove_dir_all(&d).unwrap();
    }
}
