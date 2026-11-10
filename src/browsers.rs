//! The Chromium browsers on this computer (Chrome, Edge, Brave, Chromium, Vivaldi, Opera), on Linux and
//! Windows: where each is, where it keeps its profiles, and whether an extension is in one of them. Read
//! from files only: nothing is started, and nothing of theirs is changed.

use std::path::{Path, PathBuf};

/// A browser found.
#[derive(Clone, Debug, PartialEq)]
pub struct Browser {
    pub name: &'static str,
    /// Its program.
    pub exe: PathBuf,
    /// Where its profiles are (Default, Profile 1...).
    pub data: PathBuf,
    /// Its own address for the extensions page.
    pub extensions_page: &'static str,
}

/// A browser's name, its programs (Linux), its program under the install folders (Windows), its data
/// folder under the config folder (Linux) and under the local app data folder (Windows), and its
/// extensions page.
type Known = (
    &'static str,
    &'static [&'static str],
    &'static str,
    &'static str,
    &'static str,
    &'static str,
);

const KNOWN: [Known; 8] = [
    (
        "Google Chrome",
        &["google-chrome-stable", "google-chrome"],
        r"Google\Chrome\Application\chrome.exe",
        "google-chrome",
        r"Google\Chrome\User Data",
        "chrome://extensions",
    ),
    (
        "Mozilla Firefox",
        &["firefox", "firefox-bin"],
        r"Mozilla Firefox\firefox.exe",
        ".mozilla/firefox",
        r"Mozilla\Firefox\Profiles",
        "about:addons",
    ),
    (
        "Microsoft Edge",
        &["microsoft-edge-stable", "microsoft-edge"],
        r"Microsoft\Edge\Application\msedge.exe",
        "microsoft-edge",
        r"Microsoft\Edge\User Data",
        "edge://extensions",
    ),
    (
        "Brave",
        &["brave-browser", "brave"],
        r"BraveSoftware\Brave-Browser\Application\brave.exe",
        "BraveSoftware/Brave-Browser",
        r"BraveSoftware\Brave-Browser\User Data",
        "brave://extensions",
    ),
    (
        "Chromium",
        &["chromium", "chromium-browser"],
        r"Chromium\Application\chrome.exe",
        "chromium",
        r"Chromium\User Data",
        "chrome://extensions",
    ),
    (
        "Vivaldi",
        &["vivaldi-stable", "vivaldi"],
        r"Vivaldi\Application\vivaldi.exe",
        "vivaldi",
        r"Vivaldi\User Data",
        "vivaldi://extensions",
    ),
    (
        "Opera",
        &["opera"],
        r"Programs\Opera\opera.exe",
        "opera",
        r"Opera Software\Opera Stable",
        "opera://extensions",
    ),
    (
        "Opera GX",
        &["opera-gx"],
        r"Programs\Opera GX\opera.exe",
        "opera-gx",
        r"Opera Software\Opera GX Stable",
        "opera://extensions",
    ),
];

/// The browsers installed, in the order above (the most used first).
pub fn installed() -> Vec<Browser> {
    KNOWN.iter().filter_map(find).collect()
}

/// The first browser from `installed()` that is currently running, if any.
pub fn running_browser() -> Option<Browser> {
    installed().into_iter().find(|b| b.is_running())
}

#[cfg(not(windows))]
fn find(k: &Known) -> Option<Browser> {
    let (name, programs, _, data, _, page) = *k;
    let exe = programs.iter().find_map(|p| on_path(p))?;
    let data = if name.contains("Firefox") {
        std::env::var_os("HOME").map(|h| PathBuf::from(h).join(data))?
    } else {
        let roots: Vec<PathBuf> = ["CHROME_CONFIG_HOME", "XDG_CONFIG_HOME"]
            .iter()
            .filter_map(std::env::var_os)
            .filter(|p| !p.is_empty())
            .map(PathBuf::from)
            .chain(std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))
            .collect();
        let candidates: Vec<PathBuf> = roots.iter().map(|root| root.join(data)).collect();
        let explicit = std::env::var_os("CHROME_USER_DATA_DIR")
            .filter(|p| !p.is_empty())
            .map(PathBuf::from);
        data_dir(explicit, &candidates)?
    };
    Some(Browser {
        name,
        exe,
        data,
        extensions_page: page,
    })
}

#[cfg(not(windows))]
fn data_dir(explicit: Option<PathBuf>, candidates: &[PathBuf]) -> Option<PathBuf> {
    explicit.or_else(|| {
        candidates
            .iter()
            .find(|p| running_profile(p))
            .or_else(|| candidates.iter().find(|p| p.join("Local State").is_file()))
            .or_else(|| candidates.first())
            .cloned()
    })
}

#[cfg(windows)]
fn find(k: &Known) -> Option<Browser> {
    let (name, _, program, _, data, page) = *k;
    let local = std::env::var_os("LOCALAPPDATA").map(PathBuf::from)?;
    let roots: Vec<PathBuf> = ["ProgramFiles", "ProgramFiles(x86)", "ProgramW6432"]
        .iter()
        .filter_map(std::env::var_os)
        .map(PathBuf::from)
        .chain([local.clone()])
        .collect();
    let exe = roots
        .iter()
        .map(|r| r.join(program))
        .find(|p| p.is_file())
        .or_else(|| registry_app_path(program))?;
    // Opera, Opera GX, and Firefox keep their profiles in the roaming folder.
    let data = if name.starts_with("Opera") || name.contains("Firefox") {
        std::env::var_os("APPDATA").map(PathBuf::from)?.join(data)
    } else {
        local.join(data)
    };
    Some(Browser {
        name,
        exe,
        data,
        extensions_page: page,
    })
}

#[cfg(windows)]
fn registry_app_path(program: &str) -> Option<PathBuf> {
    use windows_sys::Win32::System::Registry::{RegGetValueW, HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE, RRF_RT_REG_SZ};
    let file = Path::new(program).file_name()?.to_str()?;
    let subkey = format!(r"SOFTWARE\Microsoft\Windows\CurrentVersion\App Paths\{file}");
    let subkey_w: Vec<u16> = subkey.encode_utf16().chain([0]).collect();
    for root in [HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE] {
        let mut buf = [0u16; 512];
        let mut size = (buf.len() * 2) as u32;
        let res = unsafe {
            RegGetValueW(
                root,
                subkey_w.as_ptr(),
                std::ptr::null(),
                RRF_RT_REG_SZ,
                std::ptr::null_mut(),
                buf.as_mut_ptr().cast(),
                &mut size,
            )
        };
        if res == 0 && size > 2 {
            let len = (size as usize / 2).saturating_sub(1);
            let s = String::from_utf16_lossy(&buf[..len]);
            let path = PathBuf::from(s.trim_matches('"'));
            if path.is_file() {
                return Some(path);
            }
        }
    }
    None
}

/// `program` in the folders of PATH.
#[cfg(not(windows))]
fn on_path(program: &str) -> Option<PathBuf> {
    std::env::var_os("PATH")?
        .to_str()?
        .split(':')
        .map(|d| Path::new(d).join(program))
        .find(|p| p.is_file())
}

impl Browser {
    /// Its profiles' folders (Default, Profile 1...): those with settings.
    pub fn profiles(&self) -> Vec<PathBuf> {
        let Ok(dir) = std::fs::read_dir(&self.data) else {
            // Opera keeps one profile in the data folder itself.
            return if self.data.join("Preferences").is_file() {
                vec![self.data.clone()]
            } else {
                Vec::new()
            };
        };
        let mut found: Vec<PathBuf> = dir
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.join("Preferences").is_file() || p.join("prefs.js").is_file())
            .collect();
        if self.data.join("Preferences").is_file() {
            found.push(self.data.clone());
        }
        found.sort();
        found
    }

    /// Old development installs may point to a deleted temporary folder, and disabled extensions do not connect.
    pub fn has_extension_from(&self, id: &str, expected: &Path) -> bool {
        self.profiles().iter().any(|profile| {
            ["Secure Preferences", "Preferences"].iter().any(|file| {
                std::fs::read(profile.join(file))
                    .ok()
                    .and_then(|b| serde_json::from_slice::<serde_json::Value>(&b).ok())
                    .and_then(|v| v.pointer(&format!("/extensions/settings/{id}")).cloned())
                    .is_some_and(|v| {
                        let disabled = v["state"].as_i64() == Some(0)
                            || v["disable_reasons"].as_i64().is_some_and(|r| r != 0)
                            || v["disable_reasons"].as_array().is_some_and(|r| !r.is_empty());
                        !disabled
                            && v["path"].as_str().is_some_and(|path| {
                                let path = Path::new(path);
                                path.is_dir()
                                    && expected.canonicalize().is_ok_and(|expected| {
                                        path.canonicalize().is_ok_and(|actual| actual == expected)
                                    })
                            })
                    })
            })
        })
    }

    /// Reuse the selected browser's data and last-used profile, including a running instance.
    pub(crate) fn command(&self, url: &str) -> std::process::Command {
        let mut c = std::process::Command::new(&self.exe);
        if !self.name.contains("Firefox") {
            c.arg(format!("--user-data-dir={}", self.data.display()));
            if let Some(profile) = self.preferred_profile() {
                c.arg(format!("--profile-directory={profile}"));
            }
        }
        c.arg(url).env_remove("GDK_BACKEND");
        #[cfg(not(windows))]
        if let Some(root) = self.data.ancestors().nth(
            KNOWN
                .iter()
                .find(|k| k.0 == self.name)
                .map_or(1, |k| Path::new(k.3).components().count()),
        ) {
            // A development instance's isolated XDG path must not select a fresh browser profile.
            c.env("XDG_CONFIG_HOME", root);
        }
        if std::env::var_os("APPIMAGE").is_some() {
            c.env_remove("LD_LIBRARY_PATH").env_remove("LD_PRELOAD");
        }
        c.stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null());
        c
    }

    fn preferred_profile(&self) -> Option<String> {
        // Opera keeps one profile in the data folder itself, where --profile-directory must not be passed.
        let profiles: Vec<PathBuf> = self
            .profiles()
            .into_iter()
            .filter(|p| p.parent() == Some(self.data.as_path()))
            .collect();
        let state = std::fs::read(self.data.join("Local State"))
            .ok()
            .and_then(|b| serde_json::from_slice::<serde_json::Value>(&b).ok());
        let last = state.as_ref().and_then(|v| v["profile"]["last_used"].as_str());
        last.filter(|p| profiles.iter().any(|dir| dir.file_name().is_some_and(|n| n == *p)))
            .map(str::to_string)
            .or_else(|| {
                profiles
                    .iter()
                    .find(|p| profile_has(p, crate::tiktok_bridge::EXTENSION_ID))
                    .or_else(|| profiles.iter().find(|p| p.file_name().is_some_and(|n| n == "Default")))
                    .or_else(|| profiles.first())
                    .and_then(|p| p.file_name())
                    .map(|p| p.to_string_lossy().into_owned())
            })
    }

    /// Opens `page` as a tab in that profile. Chromium rejects `chrome://` URLs from outside and opens a
    /// second browser window instead; a `file://` setup page opens as a tab in the existing window.
    pub fn open_setup(&self, page: &Path) -> std::io::Result<()> {
        self.command(&file_url(page)).spawn().map(drop)
    }

    /// Whether this browser is currently running (has an active lock or running process).
    pub fn is_running(&self) -> bool {
        #[cfg(not(windows))]
        {
            if running_profile(&self.data) {
                return true;
            }
            for profile in self.profiles() {
                if running_profile(&profile) {
                    return true;
                }
            }
            #[cfg(target_os = "linux")]
            if let Some(exe_name) = self.exe.file_name().and_then(|n| n.to_str()) {
                if let Ok(entries) = std::fs::read_dir("/proc") {
                    for entry in entries.flatten() {
                        let path = entry.path();
                        if path.is_dir() {
                            if let Ok(comm) = std::fs::read_to_string(path.join("comm")) {
                                let c = comm.trim();
                                if c == exe_name
                                    || exe_name.starts_with(c)
                                    || (exe_name.contains("firefox") && c.contains("firefox"))
                                {
                                    return true;
                                }
                            }
                        }
                    }
                }
            }
            false
        }
        #[cfg(windows)]
        {
            let lockfile = self.data.join("lockfile");
            if lockfile.is_file() {
                if std::fs::OpenOptions::new().write(true).open(&lockfile).is_err() {
                    return true;
                }
            }
            if let Some(exe_name) = self.exe.file_name().and_then(|n| n.to_str()) {
                let exe_lower = exe_name.to_ascii_lowercase();
                use windows_sys::Win32::Foundation::{CloseHandle, INVALID_HANDLE_VALUE};
                use windows_sys::Win32::System::Diagnostics::ToolHelp::{
                    CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W, TH32CS_SNAPPROCESS,
                };
                unsafe {
                    let snap = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0);
                    if snap != INVALID_HANDLE_VALUE {
                        let mut entry: PROCESSENTRY32W = std::mem::zeroed();
                        entry.dwSize = std::mem::size_of::<PROCESSENTRY32W>() as u32;
                        if Process32FirstW(snap, &mut entry) != 0 {
                            loop {
                                let len = entry
                                    .szExeFile
                                    .iter()
                                    .position(|&c| c == 0)
                                    .unwrap_or(entry.szExeFile.len());
                                let name = String::from_utf16_lossy(&entry.szExeFile[..len]).to_ascii_lowercase();
                                if name == exe_lower {
                                    CloseHandle(snap);
                                    return true;
                                }
                                if Process32NextW(snap, &mut entry) == 0 {
                                    break;
                                }
                            }
                        }
                        CloseHandle(snap);
                    }
                }
            }
            false
        }
    }
}

/// Formats an absolute local file path as a `file://` URL that Chromium opens as a tab.
pub fn file_url(path: &Path) -> String {
    let raw = path.to_string_lossy();
    let raw = raw.strip_prefix(r"\\?\").unwrap_or(&raw);
    let normalized = raw.replace('\\', "/");
    let mut out = String::from("file://");
    if !normalized.starts_with('/') {
        out.push('/');
    }
    for b in normalized.bytes() {
        if b.is_ascii_alphanumeric() || matches!(b, b'/' | b'-' | b'_' | b'.' | b'~' | b':') {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

#[cfg(not(windows))]
fn running_profile(dir: &Path) -> bool {
    let Some(pid) = std::fs::read_link(dir.join("SingletonLock"))
        .ok()
        .and_then(|p| p.to_str()?.rsplit_once('-')?.1.parse::<u32>().ok())
    else {
        return false;
    };
    #[cfg(target_os = "linux")]
    return std::path::Path::new("/proc").join(pid.to_string()).is_dir();
    #[cfg(not(target_os = "linux"))]
    {
        let _ = pid;
        dir.join("SingletonSocket").exists()
    }
}

/// Whether the profile at `dir` lists the extension `id` (its settings say which ones it has).
fn profile_has(dir: &Path, id: &str) -> bool {
    ["Secure Preferences", "Preferences"].iter().any(|f| {
        std::fs::read_to_string(dir.join(f)).is_ok_and(|t| {
            serde_json::from_str::<serde_json::Value>(&t)
                .ok()
                .and_then(|v| v.pointer(&format!("/extensions/settings/{id}")).cloned())
                .is_some()
        })
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn browser_launch_reuses_the_last_profile_with_explicit_data_directory() {
        let dir = std::env::temp_dir().join(format!("streamerz-browser-launch-{}", std::process::id()));
        for name in ["Default", "Profile 1"] {
            std::fs::create_dir_all(dir.join(name)).unwrap();
            std::fs::write(dir.join(name).join("Preferences"), "{}").unwrap();
        }
        std::fs::write(dir.join("Local State"), r#"{"profile":{"last_used":"Profile 1"}}"#).unwrap();
        let setup = dir.join("tiktok-setup.html");
        let b = Browser {
            name: "Test",
            exe: "/browser".into(),
            data: dir.clone(),
            extensions_page: "chrome://extensions",
        };
        let command = b.command(&file_url(&setup));
        let args: Vec<_> = command.get_args().map(|s| s.to_string_lossy().into_owned()).collect();
        assert_eq!(
            args,
            [
                format!("--user-data-dir={}", dir.display()),
                "--profile-directory=Profile 1".into(),
                file_url(&setup)
            ]
        );
        std::fs::write(dir.join("Local State"), r#"{"profile":{"last_used":"../outside"}}"#).unwrap();
        assert_eq!(b.preferred_profile().as_deref(), Some("Default"));
        let _ = std::fs::remove_dir_all(&dir);

        // Opera's single profile sits directly in its data directory.
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("Preferences"), "{}").unwrap();
        assert_eq!(b.preferred_profile(), None);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn file_urls_encode_paths_for_browser_tabs() {
        assert_eq!(
            file_url(Path::new("/tmp/my folder/tiktok-setup#1.html")),
            "file:///tmp/my%20folder/tiktok-setup%231.html"
        );
        assert_eq!(
            file_url(Path::new(r"\\?\C:\Users\My Name\streamerz\tiktok-setup.html")),
            "file:///C:/Users/My%20Name/streamerz/tiktok-setup.html"
        );
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn running_browser_beats_an_empty_development_config_but_not_an_explicit_profile() {
        let dir = std::env::temp_dir().join(format!("streamerz-browser-selection-{}", std::process::id()));
        let empty = dir.join("isolated/chromium");
        let running = dir.join("desktop/chromium");
        std::fs::create_dir_all(&running).unwrap();
        std::os::unix::fs::symlink(format!("host-{}", std::process::id()), running.join("SingletonLock")).unwrap();
        assert_eq!(data_dir(None, &[empty.clone(), running.clone()]), Some(running.clone()));
        assert_eq!(data_dir(Some(empty.clone()), &[running]), Some(empty));
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn an_extension_is_found_in_a_profiles_settings() {
        let dir = std::env::temp_dir().join(format!("streamerz-browsers-{}", std::process::id()));
        let profile = dir.join("Default");
        let ext = dir.join("tiktok-extension");
        std::fs::create_dir_all(&profile).unwrap();
        std::fs::create_dir_all(&ext).unwrap();
        std::fs::write(
            profile.join("Preferences"),
            r#"{"extensions":{"settings":{"other":{}}}}"#,
        )
        .unwrap();
        let b = Browser {
            name: "Test",
            exe: PathBuf::new(),
            data: dir.clone(),
            extensions_page: "chrome://extensions",
        };
        assert_eq!(b.profiles(), vec![profile.clone()]);
        assert!(!profile_has(&profile, "abc"));
        std::fs::write(
            profile.join("Secure Preferences"),
            serde_json::json!({"extensions":{"settings":{"abc":{"path": ext.display().to_string(), "state": 1}}}})
                .to_string(),
        )
        .unwrap();
        assert!(profile_has(&profile, "abc"));
        assert!(b.has_extension_from("abc", &ext));
        std::fs::write(
            profile.join("Secure Preferences"),
            serde_json::json!({"extensions":{"settings":{"abc":{"path": ext.display().to_string(), "disable_reasons": [1]}}}}).to_string(),
        )
        .unwrap();
        assert!(!b.has_extension_from("abc", &ext));
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn browser_is_running_reflects_profile_state() {
        let dir = std::env::temp_dir().join(format!("streamerz-running-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let b = Browser {
            name: "Test",
            exe: "/nonexistent-browser-exe".into(),
            data: dir.clone(),
            extensions_page: "chrome://extensions",
        };
        // Not running initially
        assert!(!b.is_running());

        #[cfg(target_os = "linux")]
        {
            // Simulate running via SingletonLock symlink to our own PID
            let lock = dir.join("SingletonLock");
            let _ = std::os::unix::fs::symlink(format!("host-{}", std::process::id()), &lock);
            assert!(b.is_running());
            let _ = std::fs::remove_file(lock);
        }

        let _ = std::fs::remove_dir_all(dir);
    }
}
