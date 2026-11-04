//! Process Started and Process Stopped (Streamer.bot's Process Watcher): the process list is compared
//! every 2 s, only while an enabled action uses one of them (no system gives an unprivileged event for
//! it; Streamer.bot's WMI query polls too). New processes are read once; the rest is a list of ids.
//! The arguments are those Streamer.bot documents: name, executable, path, fullPath, processId and
//! parentProcessId.

use super::{s, Engine, Obj};
use serde_json::Value;
use std::collections::HashMap;
use std::rc::Rc;
use std::time::Duration;

pub const T_STARTED: u32 = 703;
pub const T_STOPPED: u32 = 704;
const EVERY: Duration = Duration::from_secs(2);

/// What is known of a running process.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Proc {
    /// Its name without `.exe`, as Windows lists it.
    pub name: String,
    /// The executable's file name.
    pub executable: String,
    pub full_path: String,
    pub parent: u32,
}

impl Proc {
    fn args(&self, pid: u32) -> Obj {
        let mut a = Obj::new();
        let dir = std::path::Path::new(&self.full_path)
            .parent()
            .map(|p| p.to_string_lossy().into_owned())
            .unwrap_or_default();
        a.insert("name".into(), self.name.as_str().into());
        a.insert("executable".into(), self.executable.as_str().into());
        a.insert("path".into(), dir.into());
        a.insert("fullPath".into(), self.full_path.as_str().into());
        a.insert("processId".into(), pid.to_string().into());
        a.insert("parentProcessId".into(), self.parent.to_string().into());
        a
    }

    /// The trigger's name (empty: any) is this process, with or without `.exe`, any case.
    fn is(&self, want: &str) -> bool {
        let w = want.trim();
        let w = without_exe(w);
        w.is_empty() || w.eq_ignore_ascii_case(&self.name) || {
            let e = &self.executable;
            without_exe(e).eq_ignore_ascii_case(w)
        }
    }
}

fn without_exe(name: &str) -> &str {
    match name.get(name.len().saturating_sub(4)..) {
        Some(ext) if ext.eq_ignore_ascii_case(".exe") => &name[..name.len() - 4],
        _ => name,
    }
}

impl Engine {
    /// Start or stop the process watch to match the actions (called when the settings change).
    pub fn processes_sync(self: &Rc<Self>) {
        let want = self.uses_trigger(T_STARTED) || self.uses_trigger(T_STOPPED);
        let mut st = self.st.borrow_mut();
        match (want, st.process_task.as_ref().is_some_and(|t| !t.is_finished())) {
            (true, false) => {
                st.process_task = Some(tokio::task::spawn_local(self.clone().process_watch()).abort_handle());
            }
            (false, true) => {
                if let Some(t) = st.process_task.take() {
                    t.abort();
                }
            }
            _ => {}
        }
    }

    async fn process_watch(self: Rc<Self>) {
        let mut known: HashMap<u32, Proc> = HashMap::new();
        list(&mut known);
        loop {
            tokio::time::sleep(EVERY).await;
            let before = known.clone();
            list(&mut known);
            for (pid, p) in &known {
                if !before.contains_key(pid) {
                    self.process_fire(T_STARTED, *pid, p);
                }
            }
            for (pid, p) in &before {
                if !known.contains_key(pid) {
                    self.process_fire(T_STOPPED, *pid, p);
                }
            }
        }
    }

    pub(crate) fn process_fire(self: &Rc<Self>, kind: u32, pid: u32, p: &Proc) {
        let args = p.args(pid);
        let name = if kind == T_STARTED {
            "Process Started"
        } else {
            "Process Stopped"
        };
        self.fire(kind, name, "Core/Processes", "processes", &args, |t| {
            p.is(s(t, "processName"))
        });
        let ev = if kind == T_STARTED {
            "ProcessStarted"
        } else {
            "ProcessStopped"
        };
        self.emit("Misc", ev, || Value::Object(args));
    }
}

/// Update `known` to the running processes: gone ones leave, new ones are read.
#[cfg(target_os = "linux")]
fn list(known: &mut HashMap<u32, Proc>) {
    let Ok(entries) = std::fs::read_dir("/proc") else {
        return;
    };
    let now: std::collections::HashSet<u32> = entries
        .flatten()
        .filter_map(|e| e.file_name().to_str()?.parse().ok())
        .collect();
    known.retain(|pid, _| now.contains(pid));
    for pid in now {
        if known.contains_key(&pid) {
            continue;
        }
        let base = format!("/proc/{pid}");
        let Ok(stat) = std::fs::read_to_string(format!("{base}/stat")) else {
            continue;
        };
        // "pid (comm) state ppid ...": the name may hold spaces and parentheses.
        let (Some(open), Some(close)) = (stat.find('('), stat.rfind(')')) else {
            continue;
        };
        let comm = stat[open + 1..close].to_string();
        let parent = stat[close + 1..]
            .split_whitespace()
            .nth(1)
            .and_then(|x| x.parse().ok())
            .unwrap_or(0);
        let full = std::fs::read_link(format!("{base}/exe"))
            .map(|p| p.to_string_lossy().trim_end_matches(" (deleted)").to_string())
            .unwrap_or_default();
        let executable = std::path::Path::new(&full)
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| comm.clone());
        let name = executable.strip_suffix(".exe").unwrap_or(&executable).to_string();
        known.insert(
            pid,
            Proc {
                name,
                executable,
                full_path: full,
                parent,
            },
        );
    }
}

#[cfg(windows)]
fn list(known: &mut HashMap<u32, Proc>) {
    use windows_sys::Win32::Foundation::{CloseHandle, INVALID_HANDLE_VALUE};
    use windows_sys::Win32::System::Diagnostics::ToolHelp::{
        CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W, TH32CS_SNAPPROCESS,
    };
    use windows_sys::Win32::System::Threading::{
        OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION,
    };
    // SAFETY: a snapshot handle, read with the documented first/next calls, closed once.
    unsafe {
        let snap = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0);
        if snap == INVALID_HANDLE_VALUE {
            return;
        }
        let mut e: PROCESSENTRY32W = std::mem::zeroed();
        e.dwSize = std::mem::size_of::<PROCESSENTRY32W>() as u32;
        let mut now = HashMap::new();
        let mut ok = Process32FirstW(snap, &mut e) != 0;
        while ok {
            let len = e.szExeFile.iter().position(|c| *c == 0).unwrap_or(e.szExeFile.len());
            now.insert(
                e.th32ProcessID,
                (String::from_utf16_lossy(&e.szExeFile[..len]), e.th32ParentProcessID),
            );
            ok = Process32NextW(snap, &mut e) != 0;
        }
        CloseHandle(snap);
        known.retain(|pid, _| now.contains_key(pid));
        for (pid, (executable, parent)) in now {
            if known.contains_key(&pid) {
                continue;
            }
            let mut full = String::new();
            let h = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
            if !h.is_null() {
                let mut buf = [0u16; 1024];
                let mut n = buf.len() as u32;
                if QueryFullProcessImageNameW(h, PROCESS_NAME_WIN32, buf.as_mut_ptr(), &mut n) != 0 {
                    full = String::from_utf16_lossy(&buf[..n as usize]);
                }
                CloseHandle(h);
            }
            let name = executable
                .strip_suffix(".exe")
                .or_else(|| executable.strip_suffix(".EXE"))
                .unwrap_or(&executable)
                .to_string();
            known.insert(
                pid,
                Proc {
                    name,
                    executable,
                    full_path: full,
                    parent,
                },
            );
        }
    }
}

#[cfg(not(any(target_os = "linux", windows)))]
fn list(_: &mut HashMap<u32, Proc>) {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn process_names_match_exe_extension_in_any_case() {
        let p = Proc {
            name: "OBS64".into(),
            executable: "OBS64.ExE".into(),
            ..Proc::default()
        };
        assert!(p.is("obs64.eXe"));
        assert!(p.is("obs64"));
        assert!(!p.is("other.ExE"));
    }

    /// A program started and ended is seen as new, then gone, with its name and paths.
    #[cfg(target_os = "linux")]
    #[test]
    fn a_new_process_is_listed_then_gone() {
        let mut known = HashMap::new();
        list(&mut known);
        assert!(known.contains_key(&std::process::id()));
        let mut child = std::process::Command::new("sleep").arg("5").spawn().unwrap();
        list(&mut known);
        let p = known.get(&child.id()).cloned().expect("the new process");
        assert_eq!(p.name, "sleep");
        assert!(p.full_path.ends_with("/sleep"), "{p:?}");
        assert_eq!(p.parent, std::process::id());
        assert!(p.is("sleep") && p.is("SLEEP.exe") && p.is("") && !p.is("obs64.exe"));
        child.kill().unwrap();
        child.wait().unwrap();
        list(&mut known);
        assert!(!known.contains_key(&child.id()));
    }
    /// The same on Windows, with a program every Windows has.
    #[cfg(windows)]
    #[test]
    fn a_new_process_is_listed_then_gone() {
        let mut known = HashMap::new();
        list(&mut known);
        assert!(known.contains_key(&std::process::id()));
        let mut child = std::process::Command::new("ping")
            .args(["-n", "10", "127.0.0.1"])
            .stdout(std::process::Stdio::null())
            .spawn()
            .unwrap();
        list(&mut known);
        let p = known.get(&child.id()).cloned().expect("the new process");
        assert_eq!(p.name.to_lowercase(), "ping", "{p:?}");
        assert!(p.full_path.to_lowercase().ends_with("\\ping.exe"), "{p:?}");
        assert_eq!(p.parent, std::process::id());
        assert!(p.is("ping") && p.is("PING.exe") && !p.is("obs64.exe"));
        child.kill().unwrap();
        child.wait().unwrap();
        list(&mut known);
        assert!(!known.contains_key(&child.id()));
    }
}
