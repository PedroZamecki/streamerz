//! Running inside Flatpak. The sandbox gets its own folders and only sees its runtime's programs, so:
//! settings and caches stay where every other copy of Streamerz keeps them (the host's XDG folders),
//! host programs (browsers, terminals, Run a Program) start through `flatpak-spawn --host`, and the
//! host's programs are looked for under `/run/host` (the manifest gives `host-os` read-only).
//! Outside Flatpak, each of these is a no-op.

use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::Command;

/// Whether this runs inside Flatpak (it puts `/.flatpak-info` in every sandbox).
pub fn flatpak() -> bool {
    static IN: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *IN.get_or_init(|| Path::new("/.flatpak-info").is_file())
}

/// The Flatpak app id, when running in one.
#[cfg(not(windows))]
pub fn app_id() -> Option<String> {
    flatpak()
        .then(|| std::env::var("FLATPAK_ID").ok())
        .flatten()
        .filter(|id| !id.is_empty())
}

/// In Flatpak, point the XDG folders back at the host's (Flatpak sets `HOST_XDG_*` when the host had
/// its own; else the defaults under the home). Called first thing in `main`, before any thread.
pub fn use_host_folders() {
    if !flatpak() {
        return;
    }
    let Some(home) = std::env::var_os("HOME").map(PathBuf::from) else {
        return;
    };
    for (var, default) in [
        ("XDG_CONFIG_HOME", ".config"),
        ("XDG_DATA_HOME", ".local/share"),
        ("XDG_CACHE_HOME", ".cache"),
        ("XDG_STATE_HOME", ".local/state"),
    ] {
        let host = std::env::var_os(format!("HOST_{var}"))
            .filter(|v| !v.is_empty())
            .unwrap_or_else(|| home.join(default).into_os_string());
        std::env::set_var(var, host);
    }
}

/// `cmd` started on the host when in Flatpak: `flatpak-spawn --host`, with its folder and environment
/// passed along (a variable removed is unset there too). Outside Flatpak, `cmd` as it is. Stdio is set
/// on the command this returns.
pub fn on_host(cmd: Command) -> Command {
    if flatpak() {
        through_spawn(&cmd)
    } else {
        cmd
    }
}

fn through_spawn(cmd: &Command) -> Command {
    let mut host = Command::new("flatpak-spawn");
    host.arg("--host");
    if let Some(dir) = cmd.get_current_dir() {
        let mut arg = OsString::from("--directory=");
        arg.push(dir);
        host.arg(arg);
    }
    for (key, value) in cmd.get_envs() {
        let mut arg = OsString::from(if value.is_some() { "--env=" } else { "--unset-env=" });
        arg.push(key);
        if let Some(value) = value {
            arg.push("=");
            arg.push(value);
        }
        host.arg(arg);
    }
    host.arg(cmd.get_program()).args(cmd.get_args());
    host
}

/// Where a host path can be read from here: under `/run/host` for the system folders Flatpak gives
/// there, else the path itself (the home and `/opt` are shared as they are).
pub fn host_path(path: &Path) -> PathBuf {
    if flatpak()
        && ["/usr", "/bin", "/sbin", "/lib", "/lib64", "/etc"]
            .iter()
            .any(|d| path.starts_with(d))
    {
        return Path::new("/run/host").join(path.strip_prefix("/").unwrap_or(path));
    }
    path.to_path_buf()
}

/// Whether the host has a program at `path` (a link counts: it may point where the sandbox can't see).
pub fn host_has(path: &Path) -> bool {
    let seen = host_path(path);
    if flatpak() {
        seen.symlink_metadata().is_ok_and(|m| !m.is_dir())
    } else {
        seen.is_file()
    }
}

/// The folders the host looks for programs in. In Flatpak, PATH is the sandbox's, so the usual ones.
pub fn host_bin_dirs() -> Vec<PathBuf> {
    if !flatpak() {
        return std::env::var_os("PATH")
            .map(|p| std::env::split_paths(&p).collect())
            .unwrap_or_default();
    }
    let mut dirs: Vec<PathBuf> = std::env::var_os("HOME")
        .map(|h| vec![PathBuf::from(h).join(".local/bin")])
        .unwrap_or_default();
    dirs.extend(["/usr/local/bin", "/usr/bin", "/bin", "/snap/bin"].map(PathBuf::from));
    dirs
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn outside_flatpak_nothing_changes() {
        if flatpak() {
            return;
        }
        let mut cmd = Command::new("true");
        cmd.arg("x").env("A", "1");
        let same = on_host(cmd);
        assert_eq!(same.get_program(), "true");
        assert_eq!(host_path(Path::new("/usr/bin/x")), Path::new("/usr/bin/x"));
    }

    #[test]
    fn a_host_command_keeps_its_folder_environment_and_arguments() {
        let mut cmd = Command::new("/usr/bin/chromium");
        cmd.arg("--a")
            .arg("b c")
            .current_dir("/tmp")
            .env("K", "v")
            .env_remove("LD_PRELOAD");
        let host = through_spawn(&cmd);
        assert_eq!(host.get_program(), "flatpak-spawn");
        let args: Vec<_> = host.get_args().map(|a| a.to_string_lossy().into_owned()).collect();
        assert_eq!(args[0], "--host");
        assert_eq!(args[1], "--directory=/tmp");
        assert!(args.contains(&"--env=K=v".to_owned()), "{args:?}");
        assert!(args.contains(&"--unset-env=LD_PRELOAD".to_owned()), "{args:?}");
        assert_eq!(args[args.len() - 3..], ["/usr/bin/chromium", "--a", "b c"]);
    }
}
