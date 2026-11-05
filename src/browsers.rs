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

const KNOWN: [Known; 6] = [
    (
        "Google Chrome",
        &["google-chrome-stable", "google-chrome"],
        r"Google\Chrome\Application\chrome.exe",
        "google-chrome",
        r"Google\Chrome\User Data",
        "chrome://extensions",
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
];

/// The browsers installed, in the order above (the most used first).
pub fn installed() -> Vec<Browser> {
    KNOWN.iter().filter_map(find).collect()
}

#[cfg(not(windows))]
fn find(k: &Known) -> Option<Browser> {
    let (name, programs, _, data, _, page) = *k;
    let exe = programs.iter().find_map(|p| on_path(p))?;
    let config = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))?;
    Some(Browser {
        name,
        exe,
        data: config.join(data),
        extensions_page: page,
    })
}

#[cfg(windows)]
fn find(k: &Known) -> Option<Browser> {
    let (name, _, program, _, data, page) = *k;
    let local = std::env::var_os("LOCALAPPDATA").map(PathBuf::from)?;
    let roots = ["ProgramFiles", "ProgramFiles(x86)"]
        .iter()
        .filter_map(std::env::var_os)
        .map(PathBuf::from)
        .chain([local.clone()]);
    let exe = roots.map(|r| r.join(program)).find(|p| p.is_file())?;
    // Opera keeps its profile in the roaming folder.
    let data = if name == "Opera" {
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
            .filter(|p| p.join("Preferences").is_file())
            .collect();
        if self.data.join("Preferences").is_file() {
            found.push(self.data.clone());
        }
        found.sort();
        found
    }

    /// Whether the extension `id` is in one of its profiles.
    pub fn has_extension(&self, id: &str) -> bool {
        self.profiles().iter().any(|p| profile_has(p, id))
    }

    /// Opens its extensions page (in a new window when it's running).
    pub fn open_extensions(&self) -> std::io::Result<()> {
        let mut c = std::process::Command::new(&self.exe);
        c.arg(self.extensions_page);
        c.stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null());
        c.spawn().map(drop)
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
    fn an_extension_is_found_in_a_profiles_settings() {
        let dir = std::env::temp_dir().join(format!("streamerz-browsers-{}", std::process::id()));
        let profile = dir.join("Default");
        std::fs::create_dir_all(&profile).unwrap();
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
        assert!(!b.has_extension("abc"));
        std::fs::write(
            profile.join("Secure Preferences"),
            r#"{"extensions":{"settings":{"abc":{"path":"/x"}}}}"#,
        )
        .unwrap();
        assert!(b.has_extension("abc"));
        let _ = std::fs::remove_dir_all(dir);
    }
}
