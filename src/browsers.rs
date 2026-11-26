//! The browsers on this computer (Chromium and Firefox families), on Linux and
//! Windows: where each is, where it keeps its profiles, and whether an extension is in one of them. Read
//! from files only: nothing is started, and nothing of theirs is changed.

use std::path::{Path, PathBuf};

/// The underlying browser engine / extension family.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BrowserEngine {
    Chromium,
    Edge,
    Firefox,
}

/// A browser found.
#[derive(Clone, Debug, PartialEq)]
pub struct Browser {
    pub name: &'static str,
    pub engine: BrowserEngine,
    /// Its program.
    pub exe: PathBuf,
    /// Where its profiles are (Default, Profile 1...).
    pub data: PathBuf,
    /// Its own address for the extensions page.
    pub extensions_page: &'static str,
}

/// Installation evidence from browser metadata, independent of an authenticated socket.
#[derive(Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExtensionInstall {
    pub state: &'static str,
    pub source: &'static str,
    pub version: Option<String>,
    pub profile: String,
    pub expected_folder: Option<bool>,
}

fn extension_setting<'a>(value: &'a serde_json::Value, id: &str) -> Option<&'a serde_json::Value> {
    value.get("extensions")?.get("settings")?.get(id)
}

fn disabled_extension(setting: &serde_json::Value) -> bool {
    setting["state"].as_i64() == Some(0)
        || setting["disable_reasons"]
            .as_array()
            .is_some_and(|items| !items.is_empty())
        || setting["disable_reasons"].as_i64().is_some_and(|reason| reason != 0)
}

/// Interns dynamically discovered browser folder names once so repeated `installed()` calls never leak.
fn intern_name(name: &str) -> &'static str {
    static CACHE: std::sync::Mutex<Vec<&'static str>> = std::sync::Mutex::new(Vec::new());
    if let Ok(mut cache) = CACHE.lock() {
        if let Some(&existing) = cache.iter().find(|&&s| s == name) {
            return existing;
        }
        if cache.len() < 64 {
            let leaked: &'static str = Box::leak(name.to_owned().into_boxed_str());
            cache.push(leaked);
            return leaked;
        }
    }
    "Chromium"
}

impl Browser {
    #[allow(dead_code)]
    pub fn is_firefox_based(&self) -> bool {
        self.engine == BrowserEngine::Firefox
    }

    /// The extension's store page for this browser engine. Firefox has none: Streamerz downloads the
    /// signed add-on itself (`tiktok_bridge::firefox_xpi`).
    pub fn store_url(&self) -> &'static str {
        match self.engine {
            BrowserEngine::Chromium => crate::tiktok_bridge::CHROME_STORE_URL,
            BrowserEngine::Edge => crate::tiktok_bridge::EDGE_STORE_URL,
            BrowserEngine::Firefox => "",
        }
    }

    /// Brand icon name for this browser (maps to brands/{name}.svg).
    #[allow(dead_code)]
    pub fn icon(&self) -> &'static str {
        match self.name {
            "Google Chrome" => "googlechrome",
            "Mozilla Firefox" => "firefox",
            "Microsoft Edge" => "microsoftedge",
            "Brave" => "brave",
            "Chromium" => "chromium",
            "Opera" => "opera",
            "Opera GX" => "operagx",
            "Vivaldi" => "vivaldi",
            "Zen Browser" => "zen",
            "Helium" => "helium",
            "Arc" => "arc",
            "Tor Browser" => "tor",
            "LibreWolf" => "librewolf",
            "Waterfox" => "waterfox",
            _ if self.is_firefox_based() => "firefox",
            _ => "browser",
        }
    }
}

/// A browser's name, engine, its programs (Linux), its program under the install folders (Windows),
/// its data folder under config/home (Linux) and under local/roaming app data (Windows), and its
/// extensions page.
type Known = (
    &'static str,
    BrowserEngine,
    &'static [&'static str],
    &'static str,
    &'static str,
    &'static str,
    &'static str,
);

const KNOWN: [Known; 20] = [
    (
        "Google Chrome",
        BrowserEngine::Chromium,
        &["google-chrome-stable", "google-chrome"],
        r"Google\Chrome\Application\chrome.exe",
        "google-chrome",
        r"Google\Chrome\User Data",
        "chrome://extensions",
    ),
    (
        "Mozilla Firefox",
        BrowserEngine::Firefox,
        &["firefox", "firefox-bin"],
        r"Mozilla Firefox\firefox.exe",
        ".mozilla/firefox",
        r"Mozilla\Firefox\Profiles",
        "about:addons",
    ),
    (
        "Microsoft Edge",
        BrowserEngine::Edge,
        &["microsoft-edge-stable", "microsoft-edge"],
        r"Microsoft\Edge\Application\msedge.exe",
        "microsoft-edge",
        r"Microsoft\Edge\User Data",
        "edge://extensions",
    ),
    (
        "Brave",
        BrowserEngine::Chromium,
        &["brave-browser", "brave"],
        r"BraveSoftware\Brave-Browser\Application\brave.exe",
        "BraveSoftware/Brave-Browser",
        r"BraveSoftware\Brave-Browser\User Data",
        "brave://extensions",
    ),
    (
        "Zen Browser",
        BrowserEngine::Firefox,
        &["zen", "zen-bin", "zen-browser"],
        r"Zen Browser\zen.exe",
        ".zen",
        r"zen\Profiles",
        "about:addons",
    ),
    (
        "Helium",
        BrowserEngine::Chromium,
        &["helium", "helium-browser"],
        r"imput\Helium\Application\chrome.exe",
        "helium",
        r"imput\Helium\User Data",
        "chrome://extensions",
    ),
    (
        "Opera",
        BrowserEngine::Chromium,
        &["opera"],
        r"Programs\Opera\opera.exe",
        "opera",
        r"Opera Software\Opera Stable",
        "opera://extensions",
    ),
    (
        "Opera GX",
        BrowserEngine::Chromium,
        &["opera-gx"],
        r"Programs\Opera GX\opera.exe",
        "opera-gx",
        r"Opera Software\Opera GX Stable",
        "opera://extensions",
    ),
    (
        "Vivaldi",
        BrowserEngine::Chromium,
        &["vivaldi-stable", "vivaldi"],
        r"Vivaldi\Application\vivaldi.exe",
        "vivaldi",
        r"Vivaldi\User Data",
        "vivaldi://extensions",
    ),
    (
        "Chromium",
        BrowserEngine::Chromium,
        &["chromium", "chromium-browser"],
        r"Chromium\Application\chrome.exe",
        "chromium",
        r"Chromium\User Data",
        "chrome://extensions",
    ),
    (
        "Arc",
        BrowserEngine::Chromium,
        &["arc-browser", "arc"],
        r"Arc\Arc.exe",
        "arc",
        r"The Browser Company\Arc\User Data",
        "arc://extensions",
    ),
    (
        "Thorium",
        BrowserEngine::Chromium,
        &["thorium-browser", "thorium"],
        r"Thorium\Application\thorium.exe",
        "thorium",
        r"Thorium\User Data",
        "chrome://extensions",
    ),
    (
        "Floorp",
        BrowserEngine::Firefox,
        &["floorp"],
        r"Ablaze Floorp\floorp.exe",
        ".floorp",
        r"Floorp\Profiles",
        "about:addons",
    ),
    (
        "LibreWolf",
        BrowserEngine::Firefox,
        &["librewolf"],
        r"LibreWolf\librewolf.exe",
        ".librewolf",
        r"librewolf\Profiles",
        "about:addons",
    ),
    (
        "Waterfox",
        BrowserEngine::Firefox,
        &["waterfox", "waterfox-g", "waterfox-classic"],
        r"Waterfox\waterfox.exe",
        ".waterfox",
        r"Waterfox\Profiles",
        "about:addons",
    ),
    (
        "Mullvad Browser",
        BrowserEngine::Firefox,
        &["mullvad-browser"],
        r"Mullvad Browser\mullvadbrowser.exe",
        ".mullvad-browser",
        r"Mullvad\Browser\Profiles",
        "about:addons",
    ),
    (
        "Tor Browser",
        BrowserEngine::Firefox,
        &["torbrowser-launcher", "start-tor-browser"],
        r"Tor Browser\Browser\firefox.exe",
        ".local/share/torbrowser/tbb/x86_64/tor-browser/Browser/TorBrowser/Data/Browser/profile.default",
        r"Tor Browser\Browser\TorBrowser\Data\Browser\profile.default",
        "about:addons",
    ),
    (
        "Sidekick",
        BrowserEngine::Chromium,
        &["sidekick-browser", "sidekick"],
        r"Pushplaylabs\Sidekick\Application\sidekick.exe",
        "sidekick",
        r"Sidekick\User Data",
        "chrome://extensions",
    ),
    (
        "Wavebox",
        BrowserEngine::Chromium,
        &["wavebox"],
        r"Wavebox\Application\wavebox.exe",
        "wavebox",
        r"Wavebox\User Data",
        "wavebox://extensions",
    ),
    (
        "Yandex",
        BrowserEngine::Chromium,
        &["yandex-browser-stable", "yandex-browser"],
        r"Yandex\YandexBrowser\Application\browser.exe",
        "yandex-browser",
        r"Yandex\YandexBrowser\User Data",
        "chrome://extensions",
    ),
];

/// The browsers installed, in the order above (the most used first), plus dynamically discovered Chromium variants.
pub fn installed() -> Vec<Browser> {
    let mut browsers: Vec<Browser> = KNOWN.iter().filter_map(find).collect();
    for extra in scan_dynamic_chromium(&browsers) {
        if !browsers.iter().any(|b| b.data == extra.data || b.exe == extra.exe) {
            browsers.push(extra);
        }
    }
    browsers
}

/// The first browser from `installed()` that is currently running, if any.
pub fn running_browser() -> Option<Browser> {
    installed().into_iter().find(|b| b.is_running())
}

#[cfg(not(windows))]
fn find(k: &Known) -> Option<Browser> {
    let (name, engine, programs, _, data, _, page) = *k;
    let exe = programs.iter().find_map(|p| on_path(p))?;
    let data = if engine == BrowserEngine::Firefox {
        let home = std::env::var_os("HOME").map(PathBuf::from)?;
        let primary = home.join(data);
        let usual = home.join(".config").join(data.trim_start_matches('.'));
        let config = std::env::var_os("XDG_CONFIG_HOME")
            .filter(|value| !value.is_empty())
            .map(|root| PathBuf::from(root).join(data.trim_start_matches('.')))
            .filter(|candidate| candidate.join("profiles.ini").is_file())
            .unwrap_or(usual);
        if primary.join("profiles.ini").is_file() || (primary.is_dir() && !config.join("profiles.ini").is_file()) {
            primary
        } else {
            if config.is_dir() {
                config
            } else {
                primary
            }
        }
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
        engine,
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
    let (name, engine, _, program, _, data, page) = *k;
    let local = std::env::var_os("LOCALAPPDATA").map(PathBuf::from)?;
    let appdata = std::env::var_os("APPDATA").map(PathBuf::from);
    let roots: Vec<PathBuf> = ["ProgramFiles", "ProgramFiles(x86)", "ProgramW6432"]
        .iter()
        .filter_map(std::env::var_os)
        .map(PathBuf::from)
        .chain([local.clone()])
        .chain([local.join("Programs")])
        .collect();
    let exe = roots
        .iter()
        .map(|r| r.join(program))
        .find(|p| p.is_file())
        .or_else(|| registry_app_path(program))
        // The Microsoft Store's Firefox: its firefox.exe there is Firefox's alone (Tor Browser's has the
        // same name, so only the full install path above may match any other).
        .or_else(|| {
            (name == "Mozilla Firefox")
                .then(|| windows_apps_path(program))
                .flatten()
        })?;
    // Opera, Opera GX, and Firefox family keep their profiles in the roaming folder.
    let data = if engine == BrowserEngine::Firefox || name.starts_with("Opera") {
        appdata.map(|a| a.join(data)).unwrap_or_else(|| local.join(data))
    } else {
        local.join(data)
    };
    Some(Browser {
        name,
        engine,
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
            let len = (size as usize / 2).min(buf.len());
            let s = String::from_utf16_lossy(&buf[..len]);
            let path = PathBuf::from(s.trim_end_matches('\0').trim_matches('"'));
            if path.is_file() && windows_path_suffix(&path.to_string_lossy(), program) {
                return Some(path);
            }
        }
    }
    None
}

#[cfg(windows)]
fn windows_apps_path(program: &str) -> Option<PathBuf> {
    let local = std::env::var_os("LOCALAPPDATA")?;
    let file = Path::new(program).file_name()?.to_str()?;
    let path = PathBuf::from(local).join("Microsoft").join("WindowsApps").join(file);
    path.is_file().then_some(path)
}

/// Whether `path` is `expected` under some install folder (Windows paths, any case).
#[cfg(any(windows, test))]
fn windows_path_suffix(path: &str, expected: &str) -> bool {
    let path = path.replace('/', "\\").to_ascii_lowercase();
    let expected = expected.replace('/', "\\").to_ascii_lowercase();
    path == expected
        || path
            .strip_suffix(&expected)
            .is_some_and(|prefix| prefix.ends_with('\\'))
}

/// Whether a running process is this browser: its full image path when Windows gives it, else its name.
#[cfg(any(windows, test))]
fn process_matches(exe: &str, name: &str, image: Option<&str>) -> bool {
    match image {
        Some(path) => path.replace('/', "\\").eq_ignore_ascii_case(&exe.replace('/', "\\")),
        None => exe
            .rsplit(['/', '\\'])
            .next()
            .is_some_and(|file| file.eq_ignore_ascii_case(name)),
    }
}

/// `program` in the folders of PATH.
#[cfg(not(windows))]
fn on_path(program: &str) -> Option<PathBuf> {
    crate::sandbox::host_bin_dirs()
        .into_iter()
        .map(|d| d.join(program))
        .find(|p| crate::sandbox::host_has(p))
}

const NON_BROWSERS: &[&str] = &[
    "discord",
    "heroic",
    "slack",
    "spotify",
    "code",
    "vscode",
    "vscodium",
    "obsidian",
    "notion",
    "postman",
    "teams",
    "skype",
    "steam",
    "telegram",
    "signal",
    "webcord",
    "armcord",
    "vesktop",
    "vencord",
    "element",
    "thunderbird",
    "tidal",
    "whatsapp",
    "zoom",
    "1password",
    "bitwarden",
    "lutris",
    "battlenet",
    "cursor",
    "windsurf",
    "zed",
    "electron",
    "github-desktop",
    "figma",
];

fn is_valid_chromium_browser_dir(dir: &Path) -> bool {
    let Some(name) = dir.file_name().and_then(|n| n.to_str()) else {
        return false;
    };
    let lower = name.to_ascii_lowercase();
    if NON_BROWSERS.iter().any(|nb| lower.contains(nb)) {
        return false;
    }

    // Real Chromium browsers have a Default or Profile directory with browser user data
    let has_profile = dir.join("Default").is_dir()
        || dir.join("Profile 1").is_dir()
        || dir.join("Default/Preferences").is_file()
        || dir.join("Default/History").is_file();
    if !has_profile {
        return false;
    }

    // Reject Electron apps that put root Preferences with "electron"
    if let Ok(content) = std::fs::read_to_string(dir.join("Preferences")) {
        if content.contains(r#""electron""#) {
            return false;
        }
    }

    // Local State must contain browser/profile metadata
    if let Ok(state) = std::fs::read_to_string(dir.join("Local State")) {
        if !state.contains(r#""profile""#) && !state.contains(r#""browser""#) {
            return false;
        }
    } else {
        return false;
    }

    true
}

/// Dynamically discovers any installed Chromium browser profiles (e.g. custom or unlisted forks)
/// not already listed in [`KNOWN`].
fn scan_dynamic_chromium(known: &[Browser]) -> Vec<Browser> {
    let mut extra = Vec::new();
    #[cfg(not(windows))]
    {
        let roots: Vec<PathBuf> = ["CHROME_CONFIG_HOME", "XDG_CONFIG_HOME"]
            .iter()
            .filter_map(std::env::var_os)
            .filter(|p| !p.is_empty())
            .map(PathBuf::from)
            .chain(std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))
            .collect();
        for root in roots {
            let Ok(entries) = std::fs::read_dir(&root) else {
                continue;
            };
            for entry in entries.flatten() {
                let path = entry.path();
                if known.iter().any(|b| b.data == path) {
                    continue;
                }
                if path.is_dir() && path.join("Local State").is_file() && is_valid_chromium_browser_dir(&path) {
                    let Some(dir_name) = path.file_name().and_then(|n| n.to_str()) else {
                        continue;
                    };
                    if let Some(exe) = on_path(dir_name).or_else(|| on_path("chromium")) {
                        let exe_name = exe
                            .file_name()
                            .and_then(|n| n.to_str())
                            .unwrap_or_default()
                            .to_ascii_lowercase();
                        if NON_BROWSERS.iter().any(|nb| exe_name.contains(nb)) {
                            continue;
                        }
                        extra.push(Browser {
                            name: intern_name(dir_name),
                            engine: BrowserEngine::Chromium,
                            exe,
                            data: path,
                            extensions_page: "chrome://extensions",
                        });
                    }
                }
            }
        }
    }
    #[cfg(windows)]
    {
        if let Some(local) = std::env::var_os("LOCALAPPDATA").map(PathBuf::from) {
            if let Ok(entries) = std::fs::read_dir(&local) {
                for entry in entries.flatten() {
                    let path = entry.path();
                    let user_data = path.join("User Data");
                    if known.iter().any(|b| b.data == user_data) {
                        continue;
                    }
                    if user_data.is_dir()
                        && user_data.join("Local State").is_file()
                        && is_valid_chromium_browser_dir(&user_data)
                    {
                        let Some(dir_name) = path.file_name().and_then(|n| n.to_str()) else {
                            continue;
                        };
                        if NON_BROWSERS.iter().any(|nb| dir_name.to_ascii_lowercase().contains(nb)) {
                            continue;
                        }
                        let exe_candidate = path.join("Application").join("chrome.exe");
                        if exe_candidate.is_file() {
                            extra.push(Browser {
                                name: intern_name(dir_name),
                                engine: BrowserEngine::Chromium,
                                exe: exe_candidate,
                                data: user_data,
                                extensions_page: "chrome://extensions",
                            });
                        }
                    }
                }
            }
        }
    }
    extra
}

/// The Firefox-family profiles that profiles.ini lists, relative to it or anywhere (IsRelative=0).
fn firefox_profiles(base: &Path) -> Vec<PathBuf> {
    let Ok(ini) = std::fs::read_to_string(base.join("profiles.ini")) else {
        return Vec::new();
    };
    let mut result = Vec::new();
    let mut in_profile = false;
    let mut path: Option<&str> = None;
    let mut relative = true;
    let mut finish = |in_profile: bool, path: &mut Option<&str>, relative: &mut bool| {
        if in_profile {
            if let Some(p) = path.take().filter(|p| !p.is_empty()) {
                let full = if *relative { base.join(p) } else { PathBuf::from(p) };
                if full.join("prefs.js").is_file() {
                    result.push(full);
                }
            }
        }
        *path = None;
        *relative = true;
    };
    for line in ini.lines() {
        let trimmed = line.trim();
        if let Some(header) = trimmed.strip_prefix('[').and_then(|s| s.strip_suffix(']')) {
            finish(in_profile, &mut path, &mut relative);
            in_profile = header.starts_with("Profile");
            continue;
        }
        if !in_profile {
            continue;
        }
        if let Some((key, value)) = trimmed.split_once('=') {
            match key.trim() {
                "Path" => path = Some(value.trim()),
                "IsRelative" => relative = value.trim() != "0",
                _ => {}
            }
        }
    }
    finish(in_profile, &mut path, &mut relative);
    result
}

impl Browser {
    /// Its profiles' folders (Default, Profile 1...): those with settings.
    pub fn profiles(&self) -> Vec<PathBuf> {
        let dir = std::fs::read_dir(&self.data);
        let mut found = Vec::new();
        if self.is_firefox_based() {
            for base in [Some(self.data.as_path()), self.data.parent()].into_iter().flatten() {
                found.extend(firefox_profiles(base));
            }
        }
        if let Ok(dir) = dir {
            found.extend(
                dir.flatten()
                    .map(|e| e.path())
                    .filter(|p| p.join("Preferences").is_file() || p.join("prefs.js").is_file()),
            );
        }
        // Opera keeps one profile in the data folder itself, and Tor Browser points straight at its profile.
        if self.data.join("Preferences").is_file() || self.data.join("prefs.js").is_file() {
            found.push(self.data.clone());
        }
        found.sort();
        found.dedup();
        found
    }

    /// The version of the extension's store copy in this browser, from the Chrome, Edge or Firefox store,
    /// when one is installed and on.
    pub fn store_copy_version(&self) -> Option<String> {
        if self.is_firefox_based() {
            return self.profiles().iter().find_map(|profile| {
                let text = std::fs::read_to_string(profile.join("extensions.json")).ok()?;
                let value: serde_json::Value = serde_json::from_str(&text).ok()?;
                value["addons"].as_array()?.iter().find_map(|addon| {
                    (addon["id"] == crate::tiktok_bridge::FIREFOX_GECKO_ID
                        && addon["active"] == true
                        && addon["appDisabled"] != true
                        && addon["userDisabled"] != true)
                        .then(|| addon["version"].as_str().map(str::to_owned))
                        .flatten()
                })
            });
        }
        self.profiles().iter().find_map(|profile| {
            ["Secure Preferences", "Preferences"].iter().find_map(|file| {
                let bytes = std::fs::read(profile.join(file)).ok()?;
                let value: serde_json::Value = serde_json::from_slice(&bytes).ok()?;
                [
                    crate::tiktok_bridge::CHROME_STORE_ID,
                    crate::tiktok_bridge::EDGE_PRODUCT_ID,
                ]
                .iter()
                .find_map(|id| {
                    let setting = extension_setting(&value, id)?;
                    if disabled_extension(setting) {
                        return None;
                    }
                    // "anckd.../1.2.0_0": the version is the folder's name.
                    setting["manifest"]["version"].as_str().map(str::to_owned).or_else(|| {
                        let path = setting["path"].as_str()?;
                        let folder = path.rsplit(['/', '\\']).next()?;
                        Some(folder.split('_').next()?.to_owned())
                    })
                })
            })
        })
    }

    /// Read evidence for every matching copy, including disabled and other-profile copies.
    /// An absent metadata file is unverifiable, not proof that an extension is missing.
    pub fn extension_installs(&self, id: &str, expected: &Path) -> Vec<ExtensionInstall> {
        let mut installs = Vec::new();
        let mut readable = false;
        for profile in self.profiles() {
            let name = profile.file_name().unwrap_or_default().to_string_lossy().into_owned();
            let files: &[&str] = if self.is_firefox_based() {
                &["extensions.json"]
            } else {
                &["Secure Preferences", "Preferences"]
            };
            let mut seen = std::collections::HashSet::new();
            for file in files {
                let Some(value) = std::fs::read(profile.join(file))
                    .ok()
                    .and_then(|bytes| serde_json::from_slice::<serde_json::Value>(&bytes).ok())
                else {
                    continue;
                };
                readable = true;
                if self.is_firefox_based() {
                    for addon in value["addons"].as_array().into_iter().flatten() {
                        if addon["id"] != id && addon["id"] != crate::tiktok_bridge::FIREFOX_GECKO_ID {
                            continue;
                        }
                        let active =
                            addon["active"] == true && addon["appDisabled"] != true && addon["userDisabled"] != true;
                        installs.push(ExtensionInstall {
                            state: if active { "enabled" } else { "disabled" },
                            source: "firefox",
                            version: addon["version"].as_str().map(str::to_owned),
                            profile: name.clone(),
                            expected_folder: None,
                        });
                    }
                    continue;
                }
                for ext_id in [
                    id,
                    crate::tiktok_bridge::CHROME_STORE_ID,
                    crate::tiktok_bridge::EDGE_PRODUCT_ID,
                    crate::tiktok_bridge::EXTENSION_ID,
                ] {
                    let Some(setting) = extension_setting(&value, ext_id) else {
                        continue;
                    };
                    if !seen.insert(ext_id.to_owned()) {
                        continue;
                    }
                    let store = ext_id == crate::tiktok_bridge::CHROME_STORE_ID
                        || ext_id == crate::tiktok_bridge::EDGE_PRODUCT_ID;
                    let path = setting["path"].as_str().map(Path::new);
                    let matches = (!store).then(|| {
                        path.is_some_and(|path| {
                            path.canonicalize()
                                .ok()
                                .zip(expected.canonicalize().ok())
                                .is_some_and(|(a, b)| a == b)
                        })
                    });
                    let manifest = path
                        .filter(|_| !store)
                        .and_then(|path| std::fs::read(path.join("manifest.json")).ok())
                        .and_then(|bytes| serde_json::from_slice::<serde_json::Value>(&bytes).ok());
                    let version = setting["manifest"]["version"]
                        .as_str()
                        .or_else(|| manifest.as_ref().and_then(|value| value["version"].as_str()))
                        .or_else(|| setting["service_worker_registration_info"]["version"].as_str())
                        .map(str::to_owned);
                    let state = if disabled_extension(setting) {
                        "disabled"
                    } else if !store && !path.is_some_and(Path::is_dir) {
                        "missing_folder"
                    } else {
                        "enabled"
                    };
                    installs.push(ExtensionInstall {
                        state,
                        source: if store { "store" } else { "unpacked" },
                        version,
                        profile: name.clone(),
                        expected_folder: matches,
                    });
                }
            }
        }
        if installs.is_empty() {
            installs.push(ExtensionInstall {
                state: if readable { "not_detected" } else { "unverifiable" },
                source: "unknown",
                version: None,
                profile: String::new(),
                expected_folder: None,
            });
        }
        installs
    }

    /// Whether this browser profile has the extension installed:
    /// - In Firefox / Gecko browsers: inspects `extensions.json` for `tiktok-bridge@streamerz.app`.
    /// - In Chromium / Edge browsers: inspects `Preferences` and `Secure Preferences` for published
    ///   store IDs (`CHROME_STORE_ID`, `EDGE_PRODUCT_ID`) and the local unpacked development ID.
    #[cfg(test)]
    pub fn has_extension_from(&self, id: &str, expected: &Path) -> bool {
        self.extension_installs(id, expected)
            .iter()
            .any(|copy| copy.state == "enabled")
    }

    /// Reuse the selected browser's data and last-used profile, including a running instance.
    pub(crate) fn command(&self, url: &str) -> std::process::Command {
        let mut c = std::process::Command::new(&self.exe);
        if !self.is_firefox_based() {
            c.arg(format!("--user-data-dir={}", self.data.display()));
            if let Some(profile) = self.preferred_profile() {
                c.arg(format!("--profile-directory={profile}"));
            }
        }
        c.arg(url).env_remove("GDK_BACKEND");
        #[cfg(not(windows))]
        {
            let rel = KNOWN.iter().find(|k| k.0 == self.name).map(|k| k.4);
            let xdg_depth = match (self.is_firefox_based(), rel) {
                (false, Some(r)) => Some(Path::new(r).components().count()),
                (false, None) => Some(1),
                (true, Some(r)) => {
                    let stripped = r.trim_start_matches('.');
                    // Only set XDG_CONFIG_HOME when data lives under an XDG config root, not ~/.mozilla or ~/.zen.
                    (stripped != r && self.data.ends_with(stripped)).then(|| Path::new(stripped).components().count())
                }
                (true, None) => None,
            };
            if let Some(root) = xdg_depth.and_then(|d| self.data.ancestors().nth(d)) {
                // A development instance's isolated XDG path must not select a fresh browser profile.
                c.env("XDG_CONFIG_HOME", root);
            }
        }
        if std::env::var_os("APPIMAGE").is_some() {
            c.env_remove("LD_LIBRARY_PATH").env_remove("LD_PRELOAD");
        }
        let mut c = crate::sandbox::on_host(c);
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
        let mut enabled: Vec<String> = self
            .extension_installs(crate::tiktok_bridge::EXTENSION_ID, &crate::tiktok_bridge::folder())
            .iter()
            .filter(|copy| copy.state == "enabled")
            .map(|copy| copy.profile.clone())
            .collect();
        enabled.sort();
        enabled.dedup();
        if enabled.len() == 1
            && profiles
                .iter()
                .any(|profile| profile.file_name().is_some_and(|name| name == enabled[0].as_str()))
        {
            return enabled.pop();
        }
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
    /// `pairing` follows the address (`#url=...&token=...`): a copy of the extension from a store, which
    /// has no settings file of its own, reads it there.
    pub fn open_setup(&self, page: &Path, pairing: &str) -> std::io::Result<()> {
        self.command(&format!("{}{pairing}", file_url(page))).spawn().map(drop)
    }

    /// The setup page and, in a second tab, the signed add-on: Firefox asks to add it.
    pub fn open_setup_with_addon(&self, page: &Path, pairing: &str, addon: &Path) -> std::io::Result<()> {
        self.command(&format!("{}{pairing}", file_url(page)))
            .arg(file_url(addon))
            .spawn()
            .map(drop)
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
                let canon_exe = self.exe.canonicalize().ok();
                if let Ok(entries) = std::fs::read_dir("/proc") {
                    for entry in entries.flatten() {
                        let path = entry.path();
                        let Some(file_name) = path.file_name().and_then(|f| f.to_str()) else {
                            continue;
                        };
                        let Ok(pid) = file_name.parse::<u32>() else { continue };
                        if !is_process_alive(pid) {
                            continue;
                        }
                        if let (Some(want), Ok(actual)) = (&canon_exe, std::fs::read_link(path.join("exe"))) {
                            if actual == *want {
                                return true;
                            }
                        }
                        let Ok(comm) = std::fs::read_to_string(path.join("comm")) else {
                            continue;
                        };
                        if linux_comm_matches(&comm, exe_name) {
                            return true;
                        }
                    }
                }
            }
            false
        }
        #[cfg(windows)]
        {
            let lockfile = self.data.join("lockfile");
            if lockfile.is_file() && std::fs::OpenOptions::new().write(true).open(&lockfile).is_err() {
                return true;
            }
            if self.is_firefox_based() {
                for profile in self.profiles() {
                    let parent_lock = profile.join("parent.lock");
                    if parent_lock.is_file() && std::fs::OpenOptions::new().write(true).open(&parent_lock).is_err() {
                        return true;
                    }
                }
            }
            if let Some(exe_name) = self.exe.file_name().and_then(|n| n.to_str()) {
                let exe_path = self.exe.to_string_lossy();
                let mut buf = vec![0u16; 4096];
                use windows_sys::Win32::Foundation::{CloseHandle, INVALID_HANDLE_VALUE};
                use windows_sys::Win32::System::Diagnostics::ToolHelp::{
                    CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W, TH32CS_SNAPPROCESS,
                };
                use windows_sys::Win32::System::Threading::{
                    OpenProcess, QueryFullProcessImageNameW, PROCESS_QUERY_LIMITED_INFORMATION,
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
                                let name = String::from_utf16_lossy(&entry.szExeFile[..len]);
                                let mut image = None;
                                if name.eq_ignore_ascii_case(exe_name) {
                                    let process =
                                        OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, entry.th32ProcessID);
                                    if !process.is_null() {
                                        let mut size = buf.len() as u32;
                                        if QueryFullProcessImageNameW(process, 0, buf.as_mut_ptr(), &mut size) != 0 {
                                            let n = (size as usize).min(buf.len());
                                            image = Some(String::from_utf16_lossy(&buf[..n]));
                                        }
                                        CloseHandle(process);
                                    }
                                }
                                if process_matches(&exe_path, &name, image.as_deref()) {
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
    crate::commands::percent_into(&mut out, &normalized, |b| {
        b.is_ascii_alphanumeric() || matches!(b, b'/' | b'-' | b'_' | b'.' | b'~' | b':')
    });
    out
}

#[cfg(target_os = "linux")]
fn is_process_alive(pid: u32) -> bool {
    let proc_path = std::path::Path::new("/proc").join(pid.to_string());
    let Ok(status) = std::fs::read_to_string(proc_path.join("status")) else {
        return false;
    };
    for line in status.lines() {
        if let Some(state) = line.strip_prefix("State:") {
            let s = state.trim();
            if s.starts_with('Z') || s.starts_with('X') {
                return false;
            }
        }
    }
    true
}

#[cfg(any(target_os = "linux", test))]
fn linux_comm_matches(comm: &str, exe_name: &str) -> bool {
    let c = comm.trim();
    if c.is_empty() || exe_name.is_empty() {
        return false;
    }
    // Linux /proc/<pid>/comm is truncated to 15 bytes (TASK_COMM_LEN - 1).
    let truncated = exe_name.get(..15).unwrap_or(exe_name);
    if c.eq_ignore_ascii_case(exe_name) || c.eq_ignore_ascii_case(truncated) {
        return true;
    }
    for suffix in ["-bin", "-browser", "-stable"] {
        if let Some(base) = exe_name.strip_suffix(suffix) {
            if !base.is_empty() && c.eq_ignore_ascii_case(base) {
                return true;
            }
            if !base.is_empty() && c.eq_ignore_ascii_case(&format!("{base}-bin")) {
                return true;
            }
        }
    }
    c.eq_ignore_ascii_case(&format!("{exe_name}-bin"))
}

#[cfg(not(windows))]
fn running_profile(dir: &Path) -> bool {
    if let Some(pid) = std::fs::read_link(dir.join("SingletonLock"))
        .ok()
        .and_then(|p| p.to_str()?.rsplit_once('-')?.1.parse::<u32>().ok())
    {
        // A Flatpak sandbox doesn't see the host's processes: the browser's socket says it runs.
        #[cfg(target_os = "linux")]
        if crate::sandbox::flatpak() {
            return dir.join("SingletonSocket").exists();
        }
        #[cfg(target_os = "linux")]
        if is_process_alive(pid) {
            return true;
        }
        #[cfg(not(target_os = "linux"))]
        {
            let _ = pid;
            if dir.join("SingletonSocket").exists() {
                return true;
            }
        }
    }
    // Firefox-family profiles hold a `lock` symlink whose target ends with `:+<pid>`.
    if let Some(pid) = std::fs::read_link(dir.join("lock"))
        .ok()
        .and_then(|p| p.to_str()?.rsplit_once(":+")?.1.parse::<u32>().ok())
    {
        #[cfg(target_os = "linux")]
        if crate::sandbox::flatpak() {
            return true;
        }
        #[cfg(target_os = "linux")]
        if is_process_alive(pid) {
            return true;
        }
        #[cfg(not(target_os = "linux"))]
        {
            let _ = pid;
            return true;
        }
    }
    false
}

/// Whether the profile at `dir` lists the extension `id` (its settings say which ones it has).
fn profile_has(dir: &Path, id: &str) -> bool {
    ["Secure Preferences", "Preferences"].iter().any(|f| {
        std::fs::read_to_string(dir.join(f)).is_ok_and(|t| {
            serde_json::from_str::<serde_json::Value>(&t)
                .ok()
                .is_some_and(|v| extension_setting(&v, id).is_some())
        })
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn windows_browser_paths_distinguish_shared_names() {
        assert!(super::windows_path_suffix(
            r"C:\Program Files\Google\Chrome\Application\chrome.exe",
            r"Google\Chrome\Application\chrome.exe"
        ));
        assert!(!super::windows_path_suffix(
            r"C:\Program Files\Google\Chrome\Application\chrome.exe",
            r"Chromium\Application\chrome.exe"
        ));
        assert!(!super::process_matches(
            r"C:\Helium\chrome.exe",
            "chrome.exe",
            Some(r"C:\Chrome\chrome.exe")
        ));
        assert!(super::process_matches(r"C:\Helium\chrome.exe", "chrome.exe", None));
    }
    #[test]
    fn firefox_ini_profiles_include_external_paths() {
        let root = std::env::temp_dir().join(format!("sz-profiles-{}", std::process::id()));
        let external = root.join("external");
        std::fs::create_dir_all(root.join("relative")).unwrap();
        std::fs::create_dir_all(&external).unwrap();
        std::fs::write(root.join("relative/prefs.js"), "").unwrap();
        std::fs::write(external.join("prefs.js"), "").unwrap();
        std::fs::write(
            root.join("profiles.ini"),
            format!(
                "[Profile0]\nIsRelative=1\nPath=relative\n[Profile1]\nIsRelative=0\nPath={}\n",
                external.display()
            ),
        )
        .unwrap();
        assert_eq!(super::firefox_profiles(&root), vec![root.join("relative"), external]);
        std::fs::remove_dir_all(root).unwrap();
    }

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
            engine: BrowserEngine::Chromium,
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
            engine: BrowserEngine::Chromium,
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
    fn browser_finds_chrome_store_and_firefox_extensions() {
        let dir = std::env::temp_dir().join(format!("streamerz-store-ext-{}", std::process::id()));
        let chrome_prof = dir.join("chrome/Default");
        std::fs::create_dir_all(&chrome_prof).unwrap();
        std::fs::write(
            chrome_prof.join("Preferences"),
            serde_json::json!({
                "extensions": {
                    "settings": {
                        crate::tiktok_bridge::CHROME_STORE_ID: {
                            "state": 1
                        }
                    }
                }
            })
            .to_string(),
        )
        .unwrap();

        let chrome_browser = Browser {
            name: "Google Chrome",
            engine: BrowserEngine::Chromium,
            exe: PathBuf::new(),
            data: dir.join("chrome"),
            extensions_page: "chrome://extensions",
        };
        assert!(chrome_browser.has_extension_from("ignored", Path::new("/dummy")));

        // Firefox / Zen extensions.json
        let ff_prof = dir.join("zen/Profiles/default.release");
        std::fs::create_dir_all(&ff_prof).unwrap();
        std::fs::write(ff_prof.join("prefs.js"), "// mozilla prefs\n").unwrap();
        std::fs::write(
            ff_prof.join("extensions.json"),
            serde_json::json!({
                "addons": [
                    {
                        "id": crate::tiktok_bridge::FIREFOX_GECKO_ID,
                        "active": true,
                        "userDisabled": false
                    }
                ]
            })
            .to_string(),
        )
        .unwrap();

        let zen_browser = Browser {
            name: "Zen Browser",
            engine: BrowserEngine::Firefox,
            exe: "/zen".into(),
            data: dir.join("zen/Profiles"),
            extensions_page: "about:addons",
        };
        assert!(zen_browser.has_extension_from("ignored", Path::new("/dummy")));

        // Verify command does not inject Chromium flags for Firefox engine
        let cmd = zen_browser.command("https://example.com");
        let args: Vec<_> = cmd.get_args().map(|s| s.to_string_lossy().into_owned()).collect();
        assert_eq!(args, ["https://example.com"]);

        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn mv3_installation_evidence_keeps_disabled_copies_and_other_folders_distinct() {
        let dir = std::env::temp_dir().join(format!("streamerz-install-evidence-{}", std::process::id()));
        let profile = dir.join("Default");
        let unpacked = dir.join("older-local-copy");
        std::fs::create_dir_all(&profile).unwrap();
        std::fs::create_dir_all(&unpacked).unwrap();
        std::fs::write(unpacked.join("manifest.json"), r#"{"version":"1.3.0"}"#).unwrap();
        std::fs::write(profile.join("Preferences"), serde_json::json!({"extensions":{"settings":{
            crate::tiktok_bridge::CHROME_STORE_ID: {"disable_reasons":[1],"active_bit":false,"manifest":{"version":"1.2.0"}},
            crate::tiktok_bridge::EXTENSION_ID: {"path":unpacked,"location":4,"has_started_service_worker":true}
        }}}).to_string()).unwrap();
        let browser = Browser {
            name: "Test",
            engine: BrowserEngine::Chromium,
            exe: PathBuf::new(),
            data: dir.clone(),
            extensions_page: "chrome://extensions",
        };
        let copies = browser.extension_installs(crate::tiktok_bridge::EXTENSION_ID, &dir.join("not-created"));
        let local = copies.iter().find(|copy| copy.source == "unpacked").unwrap();
        assert_eq!(local.state, "enabled");
        assert_eq!(local.version.as_deref(), Some("1.3.0"));
        assert_eq!(local.expected_folder, Some(false));
        assert_eq!(
            copies.iter().find(|copy| copy.source == "store").unwrap().state,
            "disabled"
        );
        assert!(browser.store_copy_version().is_none());
        assert!(browser.has_extension_from(crate::tiktok_bridge::EXTENSION_ID, &dir.join("not-created")));
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn firefox_app_disabled_and_unreadable_profiles_are_not_enabled() {
        let dir = std::env::temp_dir().join(format!("streamerz-firefox-evidence-{}", std::process::id()));
        let profile = dir.join("Default");
        std::fs::create_dir_all(&profile).unwrap();
        std::fs::write(profile.join("prefs.js"), "").unwrap();
        let browser = Browser {
            name: "Test",
            engine: BrowserEngine::Firefox,
            exe: PathBuf::new(),
            data: dir.clone(),
            extensions_page: "about:addons",
        };
        assert_eq!(browser.extension_installs("ignored", &dir)[0].state, "unverifiable");
        std::fs::write(profile.join("extensions.json"), serde_json::json!({"addons":[{
            "id":crate::tiktok_bridge::FIREFOX_GECKO_ID,"active":false,"appDisabled":true,"userDisabled":false,"version":"1.4.0"
        }]}).to_string()).unwrap();
        assert_eq!(browser.extension_installs("ignored", &dir)[0].state, "disabled");
        assert!(!browser.has_extension_from("ignored", &dir));
        assert!(browser.store_copy_version().is_none());
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn browser_is_running_reflects_profile_state() {
        let dir = std::env::temp_dir().join(format!("streamerz-running-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let b = Browser {
            name: "Test",
            engine: BrowserEngine::Chromium,
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

    #[test]
    fn electron_and_non_browsers_are_excluded() {
        let dir = std::env::temp_dir().join(format!("streamerz-non-browser-{}", std::process::id()));
        let discord_dir = dir.join("discord");
        std::fs::create_dir_all(&discord_dir).unwrap();
        std::fs::write(discord_dir.join("Local State"), r#"{"uninstall_metrics":{}}"#).unwrap();
        std::fs::write(
            discord_dir.join("Preferences"),
            r#"{"electron":{"media":{}},"browser":{}}"#,
        )
        .unwrap();

        assert!(!is_valid_chromium_browser_dir(&discord_dir));

        let heroic_dir = dir.join("heroic");
        std::fs::create_dir_all(&heroic_dir).unwrap();
        std::fs::write(heroic_dir.join("Local State"), r#"{"user":{}}"#).unwrap();
        assert!(!is_valid_chromium_browser_dir(&heroic_dir));

        // Valid custom Chromium fork
        let custom_dir = dir.join("custom-browser");
        let default_prof = custom_dir.join("Default");
        std::fs::create_dir_all(&default_prof).unwrap();
        std::fs::write(default_prof.join("Preferences"), r#"{"browser":{}}"#).unwrap();
        std::fs::write(
            custom_dir.join("Local State"),
            r#"{"browser":{"enabled":true},"profile":{"info_cache":{}}}"#,
        )
        .unwrap();
        assert!(is_valid_chromium_browser_dir(&custom_dir));

        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn firefox_ini_handles_brackets_in_values_and_single_dir_profiles() {
        let root = std::env::temp_dir().join(format!("sz-ff-brackets-{}", std::process::id()));
        let prof = root.join("abc.default");
        std::fs::create_dir_all(&prof).unwrap();
        std::fs::write(prof.join("prefs.js"), "").unwrap();
        std::fs::write(
            root.join("profiles.ini"),
            "[Profile0]\nName=Work [Dev]\nIsRelative=1\nPath=abc.default\n[Profile1]\nIsRelative=1\nPath=\n",
        )
        .unwrap();
        assert_eq!(firefox_profiles(&root), vec![prof.clone()]);

        // Tor Browser points directly at its profile directory containing prefs.js.
        let tor = Browser {
            name: "Tor Browser",
            engine: BrowserEngine::Firefox,
            exe: "/tor".into(),
            data: prof.clone(),
            extensions_page: "about:addons",
        };
        assert_eq!(tor.profiles(), vec![prof]);
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn linux_process_matching_avoids_cross_browser_and_substring_false_positives() {
        assert!(linux_comm_matches("firefox", "firefox"));
        assert!(linux_comm_matches("firefox-bin", "firefox"));
        assert!(linux_comm_matches("firefox", "firefox-bin"));
        assert!(linux_comm_matches("zen-bin", "zen"));
        assert!(linux_comm_matches("google-chrome-s", "google-chrome-stable"));
        assert!(!linux_comm_matches("zen-bin", "firefox"));
        assert!(!linux_comm_matches("firefox", "zen"));
        assert!(!linux_comm_matches("zenity", "zen"));
        assert!(!linux_comm_matches("ryzenadj", "zen"));
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn firefox_lock_symlink_and_dotdir_xdg_env() {
        let root = std::env::temp_dir().join(format!("sz-ff-lock-{}", std::process::id()));
        let dot_ff = root.join(".mozilla/firefox");
        let prof = dot_ff.join("p1.default");
        std::fs::create_dir_all(&prof).unwrap();
        std::fs::write(prof.join("prefs.js"), "").unwrap();
        let ff = Browser {
            name: "Mozilla Firefox",
            engine: BrowserEngine::Firefox,
            exe: "/nonexistent-firefox".into(),
            data: dot_ff.clone(),
            extensions_page: "about:addons",
        };
        assert!(!ff.is_running());
        let lock = prof.join("lock");
        std::os::unix::fs::symlink(format!("127.0.1.1:+{}", std::process::id()), &lock).unwrap();
        assert!(ff.is_running());
        let _ = std::fs::remove_file(&lock);

        // Dot-folder under HOME must not override XDG_CONFIG_HOME to HOME.
        let cmd = ff.command("https://example.com");
        assert!(cmd.get_envs().all(|(k, _)| k != "XDG_CONFIG_HOME"));

        // XDG-based Firefox folder sets XDG_CONFIG_HOME to its config root.
        let xdg_ff = Browser {
            data: root.join(".config/mozilla/firefox"),
            ..ff
        };
        let cmd2 = xdg_ff.command("https://example.com");
        let xdg = cmd2
            .get_envs()
            .find(|(k, _)| *k == "XDG_CONFIG_HOME")
            .and_then(|(_, v)| v)
            .map(PathBuf::from);
        assert_eq!(xdg, Some(root.join(".config")));
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn intern_name_reuses_allocation_and_extension_setting_handles_special_ids() {
        let a = intern_name("CustomBrowser");
        let b = intern_name("CustomBrowser");
        assert!(std::ptr::eq(a, b));
        let prefs = serde_json::json!({"extensions": {"settings": {"a/b~1": {"state": 1}}}});
        assert!(extension_setting(&prefs, "a/b~1").is_some());
    }
}
