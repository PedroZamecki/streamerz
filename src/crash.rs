//! Logs and crash reports, next to the settings file (`logs` folder): the current log, the last few
//! ones gzipped, and a report for every panic. A report nobody has dealt with is offered at the next
//! start as a prefilled GitHub issue.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

/// Old logs kept (gzipped) and crash reports kept.
const KEEP_LOGS: usize = 7;
const KEEP_CRASHES: usize = 20;
/// The current log stops growing here (a runaway loop shouldn't fill the disk).
const LOG_MAX: u64 = 4 * 1024 * 1024;
/// How much of a report goes into the issue's address (browsers and GitHub refuse long ones).
const ISSUE_BODY: usize = 3500;

fn dir() -> PathBuf {
    crate::config_path().with_file_name("logs")
}

static LOG: Mutex<Option<(std::fs::File, u64)>> = Mutex::new(None);

/// Starts this run's log: the last one is gzipped away, the oldest ones deleted. Call once, early.
pub fn start() {
    let dir = dir();
    if std::fs::create_dir_all(&dir).is_err() {
        return;
    }
    let latest = dir.join("latest.log");
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
pub fn line(text: &str) {
    let Ok(mut l) = LOG.lock() else { return };
    if let Some((f, size)) = l.as_mut() {
        if *size < LOG_MAX {
            let t = format!("{} {text}\n", crate::fmt::date(&crate::fmt::now(), "HH:mm:ss"));
            *size += t.len() as u64;
            let _ = f.write_all(t.as_bytes());
        }
    }
}

/// The report of a panic: what, where, the recent log and a backtrace, with the home folder hidden.
fn text(info: &std::panic::PanicHookInfo<'_>, log: &[String]) -> String {
    let what = info
        .payload()
        .downcast_ref::<&str>()
        .map(|s| s.to_string())
        .or_else(|| info.payload().downcast_ref::<String>().cloned())
        .unwrap_or_else(|| "(no message)".into());
    let at = info
        .location()
        .map_or(String::new(), |l| format!("{}:{}", l.file(), l.line()));
    let thread = std::thread::current().name().unwrap_or("?").to_string();
    let trace = std::backtrace::Backtrace::force_capture();
    let report = format!(
        "Streamerz {} on {} {}\nPanic in thread '{thread}' at {at}:\n{what}\n\nBacktrace:\n{trace}\n\nLast log lines:\n{}\n",
        crate::brand::VERSION,
        std::env::consts::OS,
        std::env::consts::ARCH,
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
    let log = crate::log_tail_try(60);
    let body = text(info, &log);
    let dir = dir();
    let _ = std::fs::create_dir_all(&dir);
    let stamp = crate::fmt::date(&crate::fmt::now(), "yyyyMMdd-HHmmss");
    let _ = std::fs::write(dir.join(format!("crash-{stamp}.txt")), body);
    keep(&dir, ".txt", KEEP_CRASHES);
    keep(&dir, ".reported", KEEP_CRASHES);
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

/// The issue's address, with the report cut to fit.
fn issue_url(report: &str) -> String {
    let cut: String = report.chars().take(ISSUE_BODY).collect();
    let body = format!(
        "**What were you doing when it closed?**\n\n\n\n<details><summary>Crash report</summary>\n\n```\n{cut}\n```\n</details>\n"
    );
    format!(
        "https://github.com/{}/issues/new?labels=crash&title={}&body={}",
        crate::brand::REPO,
        crate::commands::url_encode(&format!("Crash in {}", crate::brand::VERSION)),
        crate::commands::url_encode(&body)
    )
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
    fn the_issue_address_is_short_and_carries_the_report() {
        let long = "line of the report\n".repeat(2000);
        let url = issue_url(&long);
        assert!(url.len() < 8000, "{}", url.len());
        assert!(url.starts_with("https://github.com/") && url.contains("labels=crash"));
        assert!(url.contains("line+of+the+report"));
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
