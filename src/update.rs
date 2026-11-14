//! Updates from the GitHub releases: checked at start and then every hour while open (not in dev
//! builds). Settings choose to ask first (the default), download in the background (it goes in place at
//! the next start), or not check. Files are downloaded as `.new`, checked, and renamed over the old ones;
//! no helper program is needed, because the running program is never the one written to.

use crate::i18n::{tr, trf};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::path::{Path, PathBuf};

fn latest() -> String {
    format!("https://api.github.com/repos/{}/releases/latest", crate::brand::REPO)
}

#[derive(Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Updates {
    #[default]
    Ask,
    Background,
    Ignore,
}

/// This program's file, as it was at start (also what autostart and the shortcuts' desktop file run): once updated, the running one is the renamed .old.
/// In an AppImage, the AppImage file (the program inside is read-only).
pub fn exe() -> Option<PathBuf> {
    static EXE: std::sync::OnceLock<Option<PathBuf>> = std::sync::OnceLock::new();
    EXE.get_or_init(|| appimage().or_else(|| std::env::current_exe().ok()))
        .clone()
}

/// The AppImage this runs from, if it does.
pub(crate) fn appimage() -> Option<PathBuf> {
    own_appimage(
        &std::env::current_exe().ok()?,
        Path::new(&std::env::var_os("APPDIR")?),
        Path::new(&std::env::var_os("APPIMAGE")?),
    )
}

/// AppImage environment variables can belong to a parent application launching this binary.
fn own_appimage(executable: &Path, directory: &Path, image: &Path) -> Option<PathBuf> {
    let executable = executable.canonicalize().ok()?;
    let directory = directory.canonicalize().ok()?;
    (directory.is_dir() && executable.starts_with(directory) && image.is_file()).then(|| image.to_path_buf())
}

/// "v1.2.3" as numbers; None for "dev" and the like.
fn semver(v: &str) -> Option<(u64, u64, u64)> {
    let mut n = v
        .trim()
        .trim_start_matches('v')
        .splitn(3, '.')
        .map(|p| p.parse::<u64>().ok());
    Some((n.next()??, n.next()??, n.next()??))
}

/// This build's version, if it's a release.
fn running() -> Option<&'static str> {
    option_env!("STREAMERZ_VERSION").filter(|v| semver(v).is_some())
}

/// A release newer than this build: its tag and the file this install is made from.
pub struct Release {
    pub tag: String,
    /// Where it lives now (and where the new one goes).
    path: PathBuf,
    url: String,
    /// The file's SHA-256 as GitHub lists it, when it does.
    sha256: Option<String>,
    /// Its size in bytes, as GitHub lists it (0 when it doesn't).
    size: u64,
}

/// How this copy was installed, which says who updates it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Install {
    /// The Windows exe: Streamerz replaces it.
    Exe,
    /// The AppImage: Streamerz replaces it.
    AppImage,
    /// The plain Linux program (`Streamerz-linux-x86_64`): Streamerz replaces it.
    Binary,
    /// Flatpak updates it (`flatpak update`, the software centers).
    Flatpak,
    /// A .deb, .rpm or Arch package under /usr: the system's updater does, from the Streamerz repository.
    Package,
}

/// How this copy was installed.
pub fn install() -> Install {
    if cfg!(windows) {
        Install::Exe
    } else if crate::sandbox::flatpak() {
        Install::Flatpak
    } else if appimage().is_some() {
        Install::AppImage
    } else if exe().is_some_and(|e| e.starts_with("/usr") && !e.starts_with("/usr/local")) {
        Install::Package
    } else {
        Install::Binary
    }
}

/// What Streamerz replaces itself: the Windows exe, the AppImage or the plain Linux program, and the
/// release file it's made from. Flatpak and the packages are updated by the system.
fn installed() -> Option<(PathBuf, &'static str)> {
    match install() {
        Install::Exe => Some((exe()?, "Streamerz-windows-x86_64.exe")),
        Install::AppImage => Some((appimage()?, "Streamerz-linux-x86_64.AppImage")),
        Install::Binary => Some((exe()?, "Streamerz-linux-x86_64")),
        Install::Flatpak | Install::Package => None,
    }
}

/// The line a desktop file runs to start this copy (autostart, the hotkeys portal's entry).
#[cfg(not(windows))]
pub fn launch_line() -> Option<String> {
    if let Some(id) = crate::sandbox::app_id() {
        return Some(format!("flatpak run {id}"));
    }
    Some(format!("\"{}\"", exe()?.display()))
}

/// The newest release found, for Settings to show.
static FOUND: std::sync::Mutex<Option<String>> = std::sync::Mutex::new(None);

pub fn found() -> Option<String> {
    FOUND.lock().ok()?.clone()
}

/// The newest release, if it's newer than this build: its tag and what GitHub says of it.
async fn newer_release() -> Option<(String, Value)> {
    let now = semver(running()?)?;
    let headers = [
        ("User-Agent".to_string(), "Streamerz".to_string()),
        ("Accept".to_string(), "application/vnd.github+json".to_string()),
    ];
    let r = crate::http::request("GET", &latest(), &headers, b"").await.ok()?;
    let v: Value = serde_json::from_slice(&r.body).ok()?;
    let tag = v["tag_name"].as_str()?.to_string();
    (semver(&tag)? > now).then_some((tag, v))
}

/// The release newer than this build, if there is one and Streamerz replaces itself.
pub async fn newer() -> Option<Release> {
    let (path, name) = installed()?;
    let (tag, v) = newer_release().await?;
    let a = v["assets"].as_array()?.iter().find(|a| a["name"] == name)?;
    Some(Release {
        url: a["browser_download_url"].as_str()?.to_string(),
        sha256: a["digest"]
            .as_str()
            .and_then(|d| d.strip_prefix("sha256:"))
            .map(str::to_string),
        size: a["size"].as_u64().unwrap_or(0),
        tag,
        path,
    })
}

/// `path` with `ext` added to its whole name (Streamerz.AppImage.new).
fn beside(path: &Path, ext: &str) -> PathBuf {
    let mut n = path.as_os_str().to_owned();
    n.push(".");
    n.push(ext);
    PathBuf::from(n)
}

/// The largest release file taken: the plain Linux program is ~45 MB.
const MAX_SIZE: u64 = 256 << 20;

/// Why a downloaded file isn't the program it should be: an error page (not an ELF or an exe, or tiny),
/// a download cut short (fewer bytes than GitHub lists), or another file (not the SHA-256 GitHub lists).
fn unsound(got: &crate::http::Download, size: u64, sha256: Option<&str>) -> Option<String> {
    let program = got.magic.starts_with(b"\x7fELF") || got.magic.starts_with(b"MZ");
    if got.status != 200 {
        Some(format!("HTTP {}", got.status))
    } else if !got.complete || (size > 0 && got.len != size) {
        Some(trf(
            "update.cut_short",
            &[("got", &got.len.to_string()), ("size", &size.to_string())],
        ))
    } else if !program || got.len <= 100_000 || sha256.is_some_and(|want| !want.eq_ignore_ascii_case(&got.sha256)) {
        Some(tr("update.not_program").to_string())
    } else {
        None
    }
}

/// True when writing or renaming in `path`'s directory requires administrator / elevated permissions.
pub fn is_protected(path: &Path) -> bool {
    let parent = match path.parent() {
        Some(p) if !p.as_os_str().is_empty() => p,
        _ => return false,
    };
    let probe = parent.join(format!(".streamerz_write_probe_{}", std::process::id()));
    match std::fs::OpenOptions::new().write(true).create_new(true).open(&probe) {
        Ok(f) => {
            drop(f);
            let _ = std::fs::remove_file(&probe);
        }
        Err(_) => return true,
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        if let Ok(dir_meta) = parent.metadata() {
            if (dir_meta.mode() & 0o1000) != 0 {
                if let Ok(file_meta) = path.metadata() {
                    let uid = unsafe { libc::getuid() };
                    if uid != 0 && file_meta.uid() != uid {
                        return true;
                    }
                }
            }
        }
    }
    false
}

/// Protected updates belong to this user and installation, never the shared temporary folder.
fn staging_dir(path: &Path) -> PathBuf {
    use sha2::{Digest, Sha256};
    let digest = Sha256::digest(path.as_os_str().as_encoded_bytes());
    let id: String = digest.iter().map(|b| format!("{b:02x}")).collect();
    crate::default_config_path().with_file_name("updates").join(id)
}

fn private_dir(path: &Path) -> std::io::Result<()> {
    let builder = std::fs::DirBuilder::new();
    #[cfg(unix)]
    let builder = {
        use std::os::unix::fs::DirBuilderExt;
        let mut builder = builder;
        builder.mode(0o700);
        builder
    };
    match builder.create(path) {
        Ok(()) => {}
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {}
        Err(e) => return Err(e),
    }
    let metadata = std::fs::symlink_metadata(path)?;
    let valid = metadata.is_dir() && !metadata.file_type().is_symlink();
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        if metadata.uid() != unsafe { libc::getuid() } || metadata.mode() & 0o077 != 0 {
            return Err(std::io::Error::new(
                std::io::ErrorKind::PermissionDenied,
                "update directory is not private",
            ));
        }
    }
    if valid {
        Ok(())
    } else {
        Err(std::io::Error::new(
            std::io::ErrorKind::PermissionDenied,
            "update directory is not private",
        ))
    }
}

fn own_file(path: &Path) -> bool {
    let Ok(metadata) = std::fs::symlink_metadata(path) else {
        return false;
    };
    if !metadata.is_file() {
        return false;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        if metadata.uid() != unsafe { libc::getuid() } {
            return false;
        }
    }
    true
}

/// Where an update for `path` is staged: (.new, .version, .part).
fn staged_paths(path: &Path) -> (PathBuf, PathBuf, PathBuf) {
    if is_protected(path) {
        let dir = staging_dir(path);
        (
            dir.join("update.new"),
            dir.join("update.version"),
            dir.join("update.part"),
        )
    } else {
        (beside(path, "new"), beside(path, "version"), beside(path, "part"))
    }
}

fn find_staged(path: &Path) -> Option<(PathBuf, PathBuf)> {
    find_staged_in(path, &staging_dir(path))
}

fn find_staged_in(path: &Path, dir: &Path) -> Option<(PathBuf, PathBuf)> {
    let beside_new = beside(path, "new");
    let beside_ver = beside(path, "version");
    if own_file(&beside_new) && own_file(&beside_ver) {
        return Some((beside_new, beside_ver));
    }
    // Lookup must not create directories or accept staging through a symlink.
    if !dir.exists() || private_dir(dir.parent()?).is_err() || private_dir(dir).is_err() {
        return None;
    }
    let new = dir.join("update.new");
    let tag = dir.join("update.version");
    (own_file(&new) && own_file(&tag)).then_some((new, tag))
}

/// Downloads the release as `.new` (beside the binary or in temp if protected), with the tag in `.version`;
/// nothing else changes until `apply`.
pub async fn stage(r: &Release) -> Result<(), String> {
    let (new, tag, part) = staged_paths(&r.path);
    if is_protected(&r.path) {
        let dir = staging_dir(&r.path);
        let root = dir.parent().ok_or("missing update directory")?;
        if let Some(parent) = root.parent() {
            std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        private_dir(root)
            .and_then(|_| private_dir(&dir))
            .map_err(|e| e.to_string())?;
    }
    if new.is_file() && std::fs::read_to_string(&tag).is_ok_and(|t| t == r.tag) {
        return Ok(());
    }
    let headers = [("User-Agent".to_string(), "Streamerz".to_string())];
    // Written to .part as it arrives, never held in memory, and checked before it becomes .new.
    let got = crate::http::download(&r.url, &headers, &part, MAX_SIZE).await;
    let bad = match &got {
        Ok(got) => unsound(got, r.size, r.sha256.as_deref()),
        Err(e) => Some(e.clone()),
    };
    if let Some(why) = bad {
        let _ = std::fs::remove_file(&part);
        return Err(why);
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&part, std::fs::Permissions::from_mode(0o755));
    }
    std::fs::rename(&part, &new).map_err(|e| format!("{}: {e}", new.display()))?;
    std::fs::write(&tag, &r.tag).map_err(|e| format!("{}: {e}", tag.display()))
}

fn apply_unprivileged(path: &Path, new: &Path) -> Result<(), String> {
    if cfg!(windows) {
        let old = free_old_path(path);
        std::fs::rename(path, &old).map_err(|e| format!("{}: {e}", path.display()))?;
        if let Err(e) = std::fs::rename(new, path) {
            let _ = std::fs::rename(&old, path);
            return Err(format!("{}: {e}", path.display()));
        }
    } else {
        if std::fs::rename(new, path).is_err() {
            let tmp = beside(path, "tmp");
            std::fs::copy(new, &tmp).map_err(|e| format!("{}: {e}", path.display()))?;
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                let _ = std::fs::set_permissions(&tmp, std::fs::Permissions::from_mode(0o755));
            }
            std::fs::rename(&tmp, path).map_err(|e| format!("{}: {e}", path.display()))?;
            let _ = std::fs::remove_file(new);
        }
    }
    Ok(())
}

fn free_old_path(path: &Path) -> PathBuf {
    let old = beside(path, "old");
    if !old.exists() {
        return old;
    }
    (2..)
        .map(|n| beside(path, &format!("old.{n}")))
        .find(|p| !p.exists())
        .unwrap()
}

/// PowerShell receives paths only as literals inside an encoded script, never through cmd.exe.
#[cfg(any(windows, test))]
fn windows_elevation_command(path: &Path, new: &Path) -> String {
    use base64::Engine;
    let literal = |p: &Path| p.to_string_lossy().replace('\'', "''");
    let script = format!(
        r#"$ErrorActionPreference='Stop'
$target='{}'
$source='{}'
$old=$target+'.old'
$n=2
while (Test-Path -LiteralPath $old) {{ $old=$target+'.old.'+$n; $n++ }}
Move-Item -LiteralPath $target -Destination $old -Force
try {{ Copy-Item -LiteralPath $source -Destination $target -Force }}
catch {{ Move-Item -LiteralPath $old -Destination $target -Force; throw }}
Remove-Item -LiteralPath $source -Force
"#,
        literal(path),
        literal(new)
    );
    let bytes: Vec<u8> = script.encode_utf16().flat_map(u16::to_le_bytes).collect();
    let encoded = base64::engine::general_purpose::STANDARD.encode(bytes);
    format!("$ErrorActionPreference='Stop'; try {{ $p=Start-Process -FilePath (Join-Path $PSHOME 'powershell.exe') -ArgumentList '-NoProfile','-NonInteractive','-EncodedCommand','{encoded}' -Verb RunAs -Wait -PassThru -WindowStyle Hidden; exit $p.ExitCode }} catch {{ $e=$_.Exception; while ($e) {{ if ($e.NativeErrorCode -eq 1223) {{ exit 122 }}; $e=$e.InnerException }}; exit 1 }}")
}

#[cfg(windows)]
fn apply_elevated_windows(path: &Path, new: &Path) -> Result<(), String> {
    use std::os::windows::ffi::OsStringExt;
    let mut system = [0u16; 260];
    let length = unsafe {
        windows_sys::Win32::System::SystemInformation::GetSystemDirectoryW(system.as_mut_ptr(), system.len() as u32)
    } as usize;
    if length == 0 || length >= system.len() {
        return Err("Windows system directory is unavailable".into());
    }
    let powershell =
        PathBuf::from(std::ffi::OsString::from_wide(&system[..length])).join("WindowsPowerShell/v1.0/powershell.exe");
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    let status = std::process::Command::new(powershell)
        .creation_flags(CREATE_NO_WINDOW)
        .args([
            "-NoProfile",
            "-NonInteractive",
            "-Command",
            &windows_elevation_command(path, new),
        ])
        .status()
        .map_err(|e| format!("powershell: {e}"))?;
    if status.code() == Some(122) {
        Err("cancelled".into())
    } else if status.success() && !new.exists() && path.is_file() {
        Ok(())
    } else {
        Err(format!("{}: elevation cancelled or update failed", path.display()))
    }
}

#[cfg(unix)]
const ELEVATION_SCRIPT: &str = r#"TARGET="$1"; NEW="$2"
TMP=$(mktemp "${TARGET}.update.XXXXXX") || exit 1
trap 'rm -f -- "$TMP"' EXIT HUP INT TERM
cp -- "$NEW" "$TMP" && chmod 755 "$TMP" && mv -fT -- "$TMP" "$TARGET" && rm -f -- "$NEW" "$TARGET.part" "$TARGET.old" "$TARGET.version"
"#;

/// Runs `shell` in the first of `terms` that starts, and waits for it. Only the first: trying the next
/// while the user types the password in it would open a second. gnome-terminal returns at once unless
/// told to wait.
#[cfg(unix)]
fn in_terminal(terms: &[String], shell: &str) {
    for t in terms {
        let mut c = std::process::Command::new(t);
        match t.rsplit('/').next().unwrap_or(t) {
            "xdg-terminal-exec" => {}
            "gnome-terminal" => drop(c.args(["--wait", "--"])),
            "kgx" => drop(c.arg("--")),
            _ => drop(c.arg("-e")),
        }
        if let Ok(mut child) = c.args(["sh", "-c", shell]).spawn() {
            let _ = child.wait();
            return;
        }
    }
}

#[cfg(unix)]
fn apply_elevated_unix(path: &Path, new: &Path) -> Result<(), String> {
    let script = ELEVATION_SCRIPT;

    let path_str = path.to_str().ok_or_else(|| "non-utf8 path".to_string())?;
    let new_str = new.to_str().ok_or_else(|| "non-utf8 path".to_string())?;

    match std::process::Command::new("pkexec")
        .args(["/bin/sh", "-c", script, "sh", path_str, new_str])
        .status()
    {
        Ok(st) if st.success() => return Ok(()),
        Ok(st) if st.code() == Some(126) => return Err("cancelled".to_string()),
        _ => {}
    }

    // Fallback: sudo in a terminal emulator
    let shell = format!(
        "echo '{}'; echo; sudo /bin/sh -c '{}' sh '{}' '{}' && echo && echo '{}'; printf '{}'; read _",
        crate::i18n::tr("update.terminal_why").replace('\'', "'\\''"),
        script.replace('\'', "'\\''"),
        path_str.replace('\'', "'\\''"),
        new_str.replace('\'', "'\\''"),
        crate::i18n::tr("input.terminal_done").replace('\'', "'\\''"),
        crate::i18n::tr("input.terminal_close").replace('\'', "'\\''"),
    );

    let mut terms: Vec<String> = std::env::var("TERMINAL").into_iter().collect();
    terms.extend(
        [
            "xdg-terminal-exec",
            "kgx",
            "gnome-terminal",
            "konsole",
            "xfce4-terminal",
            "alacritty",
            "kitty",
            "foot",
            "xterm",
        ]
        .map(String::from),
    );

    in_terminal(&terms, &shell);

    if !new.exists() {
        Ok(())
    } else {
        Err(format!("{}: permission denied to install update", path.display()))
    }
}

fn apply_elevated(path: &Path, new: &Path) -> Result<(), String> {
    #[cfg(windows)]
    {
        apply_elevated_windows(path, new)
    }
    #[cfg(unix)]
    {
        apply_elevated_unix(path, new)
    }
    #[cfg(not(any(windows, unix)))]
    {
        let _ = (path, new);
        Err("Unsupported operating system for elevated updates".to_string())
    }
}

/// Puts the staged file in place. On Linux that's one atomic rename (the running program keeps its old
/// copy open, so nothing breaks); Windows can't replace a running program, only rename it, so the old
/// one waits as `.old` until the next start.
/// If the target folder is protected, prompts for elevation (UAC on Windows, polkit on Linux).
fn apply(path: &Path) -> Result<(), String> {
    let (new, tag) =
        find_staged(path).ok_or_else(|| format!("{}: no staged update found", beside(path, "new").display()))?;
    if is_protected(path) {
        apply_elevated(path, &new)?;
    } else {
        apply_unprivileged(path, &new)?;
    }
    let _ = std::fs::remove_file(tag);
    let _ = std::fs::remove_file(beside(path, "version"));
    Ok(())
}

/// At start: leftovers of the last update are cleaned, and a release the background mode staged is put
/// in place and started (this process quits).
pub fn tidy() {
    let Some((path, _)) = installed() else { return };
    for ext in ["old", "part"] {
        let _ = std::fs::remove_file(beside(&path, ext));
    }
    // Updates made while an older copy still ran left it as .old.2, .old.3...
    for n in 2..10 {
        let _ = std::fs::remove_file(beside(&path, &format!("old.{n}")));
    }
    let staged = find_staged(&path);
    let tag = staged
        .as_ref()
        .and_then(|(_, t)| std::fs::read_to_string(t).ok())
        .or_else(|| std::fs::read_to_string(beside(&path, "version")).ok());
    let Some(tag) = tag else {
        return;
    };
    let newer = matches!((semver(&tag), running().and_then(semver)), (Some(t), Some(r)) if t > r);
    if newer && !is_protected(&path) && apply(&path).is_ok() {
        restart();
    }
    // Stale: this build is that release already, or newer.
    if !newer {
        let _ = std::fs::remove_file(beside(&path, "new"));
        let _ = std::fs::remove_file(beside(&path, "version"));
        if let Some((new, tag)) = staged {
            let _ = std::fs::remove_file(new);
            let _ = std::fs::remove_file(tag);
        }
    }
}

/// The release tag last announced, so each is told about once.
static ANNOUNCED: std::sync::Mutex<String> = std::sync::Mutex::new(String::new());

/// Looks for a newer release and acts as the settings say. `asked` is Settings' Check now: it asks
/// whatever the settings say and tells when there's nothing new.
///   Ask: a question first, then install (and offer a restart).
///   Background: only downloads it; it goes in place at the next start.
///   Ignore: nothing, unless asked.
pub async fn check(cfg: std::sync::Arc<tokio::sync::watch::Sender<crate::Config>>, asked: bool) {
    let mode = cfg.borrow().updates;
    if mode == Updates::Ignore && !asked {
        return;
    }
    // Flatpak and the packages are updated by the system: Check now only says what's out.
    let managed = match install() {
        Install::Flatpak => Some("update.by_flatpak"),
        Install::Package => Some("update.by_package"),
        _ => None,
    };
    if let Some(how) = managed {
        if asked {
            let what = match (newer_release().await, running()) {
                (Some((tag, _)), _) => trf(how, &[("version", &tag)]),
                (None, Some(v)) => trf("update.newest", &[("version", v)]),
                (None, None) => tr("update.dev").to_string(),
            };
            let _ = crate::host::confirm(what, tr("dialogs.ok"), "").await;
        }
        return;
    }
    let Some(release) = newer().await else {
        if asked {
            let what = match running() {
                Some(v) => trf("update.newest", &[("version", v)]),
                None => tr("update.dev").to_string(),
            };
            let _ = crate::host::confirm(what, tr("dialogs.ok"), "").await;
        }
        return;
    };
    let tag = release.tag.clone();
    *FOUND.lock().unwrap() = Some(tag.clone());
    // Told once per release, however many times it's looked for.
    let fresh = std::mem::replace(&mut *ANNOUNCED.lock().unwrap(), tag.clone()) != tag;
    if fresh {
        crate::say(format!("Streamerz {tag} is out"));
    }
    if background_download(mode, asked, is_protected(&release.path)) {
        crate::say(format!("Downloading {tag}..."));
        match stage(&release).await {
            Ok(()) if fresh => crate::notify::send(
                crate::notify::Kind::Updates,
                &trf("update.ready.title", &[("version", &tag)]),
                tr("update.ready.body"),
            ),
            Ok(()) => {}
            Err(e) => crate::error(format!("Download of {tag} failed: {e}")),
        }
        return;
    }
    if !fresh && !asked {
        return;
    }
    if fresh {
        crate::notify::send(
            crate::notify::Kind::Updates,
            &trf("update.out.title", &[("version", &tag)]),
            tr("update.out.body"),
        );
    }
    let now = running().unwrap_or("");
    let protected = is_protected(&release.path);
    let question = if protected {
        trf("update.ask_admin", &[("version", &tag), ("now", now)])
    } else {
        trf("update.ask", &[("version", &tag), ("now", now)])
    };
    if !crate::host::confirm(question, tr("update.later"), tr("update.now"))
        .await
        .unwrap_or(false)
    {
        return;
    }
    crate::say(format!("Updating to {tag}..."));
    // The password or UAC prompt can take as long as the user needs: it waits on a thread of its own,
    // never on the one that runs chat, the servers and the window.
    let path = release.path.clone();
    let applied = match stage(&release).await {
        Ok(()) => tokio::task::spawn_blocking(move || apply(&path))
            .await
            .unwrap_or_else(|e| Err(e.to_string())),
        Err(e) => Err(e),
    };
    if let Err(e) = applied {
        if e == "cancelled" {
            crate::say(tr("update.cancelled").to_string());
            return;
        }
        crate::error(format!("Update to {tag} failed: {e}"));
        let mut told = trf("update.failed", &[("error", &e)]);
        if install() == Install::AppImage {
            told = format!("{told}\n\n{}", tr("update.app_manager"));
        }
        let _ = crate::host::confirm(told, tr("dialogs.ok"), "").await;
        return;
    }
    let done = trf("update.done", &[("version", &tag)]);
    if crate::host::confirm(done, tr("update.later"), tr("update.restart"))
        .await
        .unwrap_or(false)
    {
        crate::host::restart();
    } else {
        crate::say(format!("Updated to {tag}: it runs from the next start."));
    }
}

fn background_download(mode: Updates, asked: bool, protected: bool) -> bool {
    mode == Updates::Background && !asked && !protected
}

/// Checks now and then once an hour while Streamerz is open.
pub async fn watch(cfg: std::sync::Arc<tokio::sync::watch::Sender<crate::Config>>) {
    // Flatpak and the packages are updated by the system: no timer.
    if installed().is_none() {
        return;
    }
    loop {
        check(cfg.clone(), false).await;
        tokio::time::sleep(std::time::Duration::from_secs(3600)).await;
    }
}

/// Starts the new build and quits this one.
pub fn restart() -> ! {
    if let Some(exe) = exe() {
        let mut cmd = std::process::Command::new(exe);
        cmd.args(std::env::args_os().skip(1).filter(|a| a != "--restart-handover"));
        cmd.arg("--restart-handover");
        // The AppImage this ran from mounted itself and told its children where; the new one makes its own.
        for v in ["APPDIR", "APPIMAGE", "ARGV0", "OWD"] {
            cmd.env_remove(v);
        }
        let _ = cmd.spawn();
    }
    crate::crash::quit(0)
}

#[cfg(test)]
mod tests {
    use super::{beside, semver, unsound};
    use std::path::Path;

    #[test]
    fn protected_background_updates_ask_instead_of_staging() {
        assert!(!super::background_download(super::Updates::Background, false, true));
        assert!(super::background_download(super::Updates::Background, false, false));
        assert!(!super::background_download(super::Updates::Background, true, false));
    }

    #[test]
    fn update_uses_an_unused_old_file_name() {
        let root = std::env::temp_dir().join(format!("streamerz-old-{}", crate::engine::uuid()));
        std::fs::create_dir_all(&root).unwrap();
        let exe = root.join("Streamerz.exe");
        std::fs::write(super::beside(&exe, "old"), b"in use").unwrap();
        assert_eq!(super::free_old_path(&exe), super::beside(&exe, "old.2"));
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn an_inherited_appimage_cannot_replace_the_running_program() {
        let root = std::env::temp_dir().join(format!("streamerz-appimage-{}", crate::engine::uuid()));
        let mount = root.join("mount");
        std::fs::create_dir_all(mount.join("usr/bin")).unwrap();
        let own = mount.join("usr/bin/Streamerz");
        let child = root.join("Streamerz");
        let image = root.join("parent.AppImage");
        for path in [&own, &child, &image] {
            std::fs::write(path, b"test").unwrap();
        }
        assert_eq!(super::own_appimage(&own, &mount, &image), Some(image.clone()));
        assert_eq!(super::own_appimage(&child, &mount, &image), None);
        assert_eq!(super::own_appimage(&own, &root.join("missing"), &image), None);
        assert_eq!(super::own_appimage(&own, &mount, &root.join("missing")), None);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn versions_compare() {
        assert!(semver("v1.10.0") > semver("v1.9.3"));
        assert_eq!(semver("v2.0.1"), Some((2, 0, 1)));
        assert_eq!(semver("dev"), None);
    }

    #[test]
    fn downloads_are_checked() {
        let got = |status, len, complete, sha256: &str, magic: &[u8]| crate::http::Download {
            status,
            len,
            complete,
            sha256: sha256.into(),
            magic: magic.to_vec(),
        };
        assert!(unsound(&got(200, 200_000, true, "ab", b"\x7fELF"), 200_000, Some("AB")).is_none());
        assert!(unsound(&got(200, 200_000, true, "ab", b"MZ\x90\0"), 0, None).is_none());
        // The 1.7.0 failure: a 25 MB exe cut at 16 MB.
        assert!(unsound(&got(200, 16 << 20, true, "ab", b"MZ\x90\0"), 25_311_232, Some("ab")).is_some());
        assert!(unsound(&got(200, 200_000, false, "ab", b"MZ\x90\0"), 0, None).is_some());
        assert!(unsound(&got(200, 200_000, true, "ab", b"MZ\x90\0"), 200_000, Some("00")).is_some());
        assert!(unsound(&got(200, 22, true, "ab", b"<htm"), 0, None).is_some());
        assert!(unsound(&got(404, 200_000, true, "ab", b"MZ\x90\0"), 0, None).is_some());
    }

    #[test]
    fn staged_names_keep_the_whole_name() {
        assert_eq!(
            beside(Path::new("/a/x.AppImage"), "new"),
            Path::new("/a/x.AppImage.new")
        );
    }

    #[test]
    fn protected_folder_detection() {
        let temp = std::env::temp_dir().join(format!("streamerz-test-prot-{}", crate::engine::uuid()));
        std::fs::create_dir_all(&temp).unwrap();
        let exe = temp.join("MyCustomName.exe");
        assert!(!super::is_protected(&exe));

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let ro_dir = temp.join("readonly_dir");
            std::fs::create_dir_all(&ro_dir).unwrap();
            std::fs::set_permissions(&ro_dir, std::fs::Permissions::from_mode(0o555)).unwrap();
            let ro_exe = ro_dir.join("MyCustomName.exe");
            assert!(super::is_protected(&ro_exe));
            // Restore permissions to clean up
            std::fs::set_permissions(&ro_dir, std::fs::Permissions::from_mode(0o755)).unwrap();
        }

        assert!(super::is_protected(Path::new("/usr/bin/streamerz")));
        std::fs::remove_dir_all(temp).unwrap();
    }

    #[test]
    fn staged_paths_and_find_staged_custom_name() {
        let temp = std::env::temp_dir().join(format!("streamerz-test-staged-{}", crate::engine::uuid()));
        std::fs::create_dir_all(&temp).unwrap();
        let custom_exe = temp.join("CustomStreamerzApp.exe");

        // When in writable folder:
        let (new_path, ver_path, part_path) = super::staged_paths(&custom_exe);
        assert_eq!(new_path, temp.join("CustomStreamerzApp.exe.new"));
        assert_eq!(ver_path, temp.join("CustomStreamerzApp.exe.version"));
        assert_eq!(part_path, temp.join("CustomStreamerzApp.exe.part"));

        // When beside .new exists:
        std::fs::write(&new_path, b"new-binary").unwrap();
        std::fs::write(&ver_path, b"v2.0.0").unwrap();
        assert_eq!(
            super::find_staged(&custom_exe),
            Some((new_path.clone(), ver_path.clone()))
        );
        std::fs::remove_file(&new_path).unwrap();
        std::fs::remove_file(&ver_path).unwrap();

        // Installations with the same basename cannot share staged updates.
        assert_ne!(
            super::staging_dir(Path::new("/one/App")),
            super::staging_dir(Path::new("/two/App"))
        );
        let root = temp.join("private");
        let dir = root.join("install");
        super::private_dir(&root).unwrap();
        super::private_dir(&dir).unwrap();
        let p_new = dir.join("update.new");
        let p_ver = dir.join("update.version");
        std::fs::write(&p_new, b"new-binary").unwrap();
        std::fs::write(&p_ver, b"v2.0.0").unwrap();
        assert_eq!(super::find_staged_in(&custom_exe, &dir), Some((p_new, p_ver)));

        std::fs::remove_dir_all(temp).unwrap();
    }

    #[test]
    fn apply_unprivileged_replaces_custom_name() {
        let temp = std::env::temp_dir().join(format!("streamerz-test-apply-{}", crate::engine::uuid()));
        std::fs::create_dir_all(&temp).unwrap();
        let custom_exe = temp.join("Streamerz Renamed Executable");
        std::fs::write(&custom_exe, b"old-version-content").unwrap();

        let new_file = temp.join("Streamerz Renamed Executable.new");
        let ver_file = temp.join("Streamerz Renamed Executable.version");
        std::fs::write(&new_file, b"new-version-content").unwrap();
        std::fs::write(&ver_file, b"v9.9.9").unwrap();

        super::apply(&custom_exe).unwrap();

        assert_eq!(std::fs::read(&custom_exe).unwrap(), b"new-version-content");
        assert!(!new_file.exists());
        assert!(!ver_file.exists());

        std::fs::remove_dir_all(temp).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn unix_elevation_script_handles_custom_names_and_spaces() {
        let temp = std::env::temp_dir().join(format!("streamerz-test-unix-elev-{}", crate::engine::uuid()));
        let dir_with_spaces = temp.join("folder with spaces");
        std::fs::create_dir_all(&dir_with_spaces).unwrap();

        let target = dir_with_spaces.join("My App's $(false) %TEMP%.AppImage");
        let new_file = temp.join("new_staged_binary");
        std::fs::write(&target, b"original-data").unwrap();
        std::fs::write(&new_file, b"updated-data").unwrap();

        let script = super::ELEVATION_SCRIPT;

        let status = std::process::Command::new("/bin/sh")
            .args(["-c", script, "sh", target.to_str().unwrap(), new_file.to_str().unwrap()])
            .status()
            .unwrap();

        assert!(status.success());
        assert_eq!(std::fs::read(&target).unwrap(), b"updated-data");
        assert!(!new_file.exists());

        std::fs::remove_dir_all(temp).unwrap();
    }

    #[test]
    fn windows_elevation_paths_are_encoded_literals() {
        use base64::Engine;
        let target = Path::new(r"C:\Program Files\App's %TEMP% & $(exit 9).exe");
        let source = Path::new(r"C:\private\new.exe");
        let command = super::windows_elevation_command(target, source);
        assert!(!command.contains("cmd.exe"));
        assert!(!command.contains("%TEMP%"));
        // A declined UAC prompt (ERROR_CANCELLED) may sit inside another exception.
        assert!(command.contains("NativeErrorCode -eq 1223"));
        assert!(command.contains("InnerException"));
        assert!(command.contains("exit 122"));
        let encoded = command
            .split("'-EncodedCommand','")
            .nth(1)
            .unwrap()
            .split('\'')
            .next()
            .unwrap();
        let bytes = base64::engine::general_purpose::STANDARD.decode(encoded).unwrap();
        let utf16: Vec<u16> = bytes
            .as_chunks::<2>()
            .0
            .iter()
            .map(|b| u16::from_le_bytes([b[0], b[1]]))
            .collect();
        let script = String::from_utf16(&utf16).unwrap();
        assert!(script.contains("$target='C:\\Program Files\\App''s %TEMP% & $(exit 9).exe'"));
        assert!(script.contains("-LiteralPath $source"));
        assert!(script.contains("while (Test-Path -LiteralPath $old)"));
    }

    #[cfg(unix)]
    #[test]
    fn staging_rejects_symlinks_and_public_directories() {
        use std::os::unix::fs::{symlink, PermissionsExt};
        let root = std::env::temp_dir().join(format!("streamerz-private-{}", crate::engine::uuid()));
        super::private_dir(&root).unwrap();
        assert_eq!(root.metadata().unwrap().permissions().mode() & 0o777, 0o700);
        let link = root.join("link");
        symlink(&root, &link).unwrap();
        assert!(super::private_dir(&link).is_err());
        assert!(!super::own_file(&link));
        std::fs::set_permissions(&root, std::fs::Permissions::from_mode(0o755)).unwrap();
        assert!(super::private_dir(&root).is_err());
        std::fs::remove_file(link).unwrap();
        std::fs::remove_dir(root).unwrap();
    }

    /// Only one terminal opens for the password, even when it returns at once.
    #[cfg(unix)]
    #[test]
    fn the_password_terminal_opens_once() {
        use std::os::unix::fs::PermissionsExt;
        let dir = std::env::temp_dir().join(format!("streamerz-terms-{}", crate::engine::uuid()));
        std::fs::create_dir_all(&dir).unwrap();
        let log = dir.join("log");
        let mut terms = vec![dir.join("missing").display().to_string()];
        for name in ["first", "second"] {
            let path = dir.join(name);
            std::fs::write(&path, format!("#!/bin/sh\necho {name} >> '{}'\n", log.display())).unwrap();
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
            terms.push(path.display().to_string());
        }
        super::in_terminal(&terms, "true");
        assert_eq!(std::fs::read_to_string(&log).unwrap(), "first\n");
        std::fs::remove_dir_all(dir).unwrap();
    }
}
